//! Las herramientas que ve la IA: una tabla con nombre, descripción, esquema y
//! función. La lista de `tools/list`, la guía y las pruebas salen de acá.

use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{json, Value};

use crate::session::Session;

mod basics;
mod cad;
mod features;
mod project;
mod sketch;
mod view;

/// Una herramienta
pub struct Tool {
    pub name: &'static str,
    pub title: &'static str,
    /// Qué hace, cuándo usarla, unidades, qué devuelve y un ejemplo
    pub description: &'static str,
    pub input: fn() -> Value,
    pub effect: Effect,
    pub run: fn(&Ctx, &Value) -> Result<Output, Fail>,
}

/// Qué le hace al trabajo
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Solo mira
    Read,
    /// Cambia el trabajo y se puede deshacer con `undo`
    Change,
    /// Reemplaza el trabajo entero (abrir, nuevo); también se deshace. La
    /// herramienta guarda ella misma el paso (`Session::replaced`)
    Replace,
    /// Escribe archivos fuera del proyecto (exportar, guardar)
    Write,
    /// Se mueve por el historial (`undo`, `redo`)
    History,
}

impl Tool {
    /// Entrada de `tools/list`
    pub fn listing(&self) -> Value {
        json!({
            "name": self.name,
            "title": self.title,
            "description": self.description,
            "inputSchema": (self.input)(),
            "annotations": {
                "title": self.title,
                "readOnlyHint": self.effect == Effect::Read,
                "destructiveHint": self.effect == Effect::Replace,
                "idempotentHint": matches!(self.effect, Effect::Read | Effect::Write),
                "openWorldHint": false,
            },
        })
    }
}

/// Todas las herramientas, en el orden en que se listan
pub fn all() -> Vec<&'static Tool> {
    let mut list: Vec<&'static Tool> = Vec::new();
    list.extend(basics::TOOLS);
    list.extend(project::TOOLS);
    list.extend(view::TOOLS);
    list.extend(cad::TOOLS[..5].iter());
    list.extend(sketch::TOOLS);
    list.extend(features::TOOLS);
    list.extend(cad::TOOLS[5..].iter());
    list
}

pub fn find(name: &str) -> Option<&'static Tool> {
    all().into_iter().find(|t| t.name == name)
}

/// Resultado de una herramienta: un resumen corto para leer y los datos
pub struct Output {
    pub text: String,
    pub data: Value,
    /// Imágenes PNG (vistas del trabajo)
    pub images: Vec<Vec<u8>>,
}

impl Output {
    pub fn new(text: impl Into<String>, data: Value) -> Self {
        Self { text: text.into(), data, images: Vec::new() }
    }
}

/// Error de una herramienta, con una pista de cómo seguir
#[derive(Debug)]
pub struct Fail {
    pub message: String,
    pub hint: Option<String>,
}

impl Fail {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into(), hint: None }
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

impl From<String> for Fail {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

/// Lo que una herramienta tiene a mano mientras corre
pub struct Ctx<'a> {
    pub session: &'a Session,
    pub progress: &'a (dyn Fn(f64, &str) + Sync),
    pub cancel: &'a AtomicBool,
}

impl Ctx<'_> {
    /// Un comando de la app; devuelve su JSON
    pub fn call(&self, cmd: &str, args: Value) -> Result<Value, Fail> {
        self.check_cancel()?;
        Ok(self.session.backend.call(cmd, args)?.json())
    }

    /// Un comando de la app que devuelve bytes (mallas, imágenes)
    pub fn call_bytes(&self, cmd: &str, args: Value) -> Result<Vec<u8>, Fail> {
        self.check_cancel()?;
        match self.session.backend.call(cmd, args)? {
            crate::backend::Reply::Bytes(b) => Ok(b),
            crate::backend::Reply::Json(v) => Err(Fail::new(format!("{cmd} returned {v} instead of data"))),
        }
    }

    /// Avance de 0 a 1 con lo que se está haciendo
    pub fn progress(&self, fraction: f64, message: &str) {
        (self.progress)(fraction, message);
    }

    /// Corta si el cliente canceló el pedido
    pub fn check_cancel(&self) -> Result<(), Fail> {
        if self.cancel.load(Ordering::SeqCst) { Err(Fail::new("cancelled by the client")) } else { Ok(()) }
    }
}

// Lectura de argumentos ya verificados contra el esquema

pub fn arg_str<'a>(args: &'a Value, name: &str) -> Option<&'a str> {
    args.get(name).and_then(Value::as_str)
}

pub fn arg_bool(args: &Value, name: &str) -> Option<bool> {
    args.get(name).and_then(Value::as_bool)
}
