//! Estado de la sesión del conector: quién ejecuta, el archivo del proyecto y
//! el historial para deshacer.

use std::path::PathBuf;
use std::sync::Mutex;

use crate::mcp::backend::{Backend, Mode};

/// Pasos que se pueden deshacer
const HISTORY: usize = 20;

pub struct Session {
    pub backend: Box<dyn Backend>,
    pub project: Mutex<ProjectFile>,
    history: Mutex<History>,
    /// Pestaña en la que la IA está trabajando: las herramientas sin `tab` van
    /// a esta y no a la activa de la ventana (en vivo la puede cambiar el usuario)
    pub focus: Mutex<Option<u32>>,
}

/// El archivo del proyecto abierto
#[derive(Default, Clone)]
pub struct ProjectFile {
    pub path: Option<PathBuf>,
    /// Estado de la interfaz guardado en el proyecto: se devuelve igual al
    /// guardar, así la app lo abre como estaba
    pub ui: String,
    /// Hay cambios sin guardar
    pub dirty: bool,
}

#[derive(Default)]
struct History {
    undo: Vec<Step>,
    redo: Vec<Step>,
}

/// Lo de antes de un cambio: el estado del motor y el archivo (con su interfaz)
pub struct Before {
    state: Vec<u8>,
    file: ProjectFile,
}

/// Un paso: qué herramienta fue y el estado de antes (o de después, en `redo`)
struct Step {
    label: String,
    state: Vec<u8>,
    file: ProjectFile,
}

impl Session {
    pub fn new(backend: Box<dyn Backend>) -> Self {
        Self {
            backend,
            project: Mutex::new(ProjectFile { ui: "{}".into(), ..Default::default() }),
            history: Mutex::new(History::default()),
            focus: Mutex::new(None),
        }
    }

    /// Estado de antes de un cambio (se guarda con [`commit`](Self::commit))
    pub fn before_change(&self) -> Result<Before, String> {
        Ok(Before { state: self.backend.snapshot()?, file: self.project.lock().unwrap().clone() })
    }

    /// El cambio salió bien: `before` queda para deshacer (en vivo, el
    /// deshacer es el de la app)
    pub fn commit(&self, label: &str, before: Before) {
        if self.backend.mode() == Mode::Live {
            return;
        }
        let mut project = self.project.lock().unwrap();
        let mut h = self.history.lock().unwrap();
        h.undo.push(Step { label: label.into(), state: before.state, file: before.file });
        if h.undo.len() > HISTORY {
            h.undo.remove(0);
        }
        h.redo.clear();
        project.dirty = true;
    }

    /// El cambio falló a mitad: vuelve a como estaba
    pub fn rollback(&self, before: Before) -> Result<(), String> {
        self.backend.restore(&before.state)?;
        if self.backend.mode() != Mode::Live {
            *self.project.lock().unwrap() = before.file;
        }
        Ok(())
    }

    /// Cambia el estado de la interfaz guardado en el proyecto (lista de
    /// objetos, nombre del modelo…): lo que el conector agrega para que la app
    /// lo muestre al abrir el archivo
    pub fn edit_ui(&self, edit: impl FnOnce(&mut serde_json::Value)) {
        let mut project = self.project.lock().unwrap();
        let mut ui: serde_json::Value = serde_json::from_str(&project.ui).unwrap_or_else(|_| serde_json::json!({}));
        if !ui.is_object() {
            ui = serde_json::json!({});
        }
        edit(&mut ui);
        project.ui = ui.to_string();
    }

    /// El estado de la interfaz guardado en el proyecto
    pub fn ui(&self) -> serde_json::Value {
        serde_json::from_str(&self.project.lock().unwrap().ui).unwrap_or_else(|_| serde_json::json!({}))
    }

    /// Deshace el último cambio; devuelve qué se deshizo
    pub fn undo(&self) -> Result<Option<String>, String> {
        self.travel(true)
    }

    pub fn redo(&self) -> Result<Option<String>, String> {
        self.travel(false)
    }

    fn travel(&self, back: bool) -> Result<Option<String>, String> {
        // En vivo: el deshacer de la app (solo pasos de la IA)
        if let Some(r) = self.backend.ask(if back { "undo" } else { "redo" }) {
            return r.map(|v| v.as_str().map(str::to_string));
        }
        // Mismo orden de candados que `commit`: primero el proyecto
        let mut project = self.project.lock().unwrap();
        let mut h = self.history.lock().unwrap();
        let Some(step) = (if back { h.undo.pop() } else { h.redo.pop() }) else { return Ok(None) };
        let now = Step { label: step.label.clone(), state: self.backend.snapshot()?, file: project.clone() };
        if let Err(e) = self.backend.restore(&step.state) {
            // No se pudo: el paso vuelve a su lugar
            if back { h.undo.push(step) } else { h.redo.push(step) }
            return Err(e);
        }
        *project = step.file;
        let label = step.label;
        if back { h.redo.push(now) } else { h.undo.push(now) }
        Ok(Some(label))
    }

    /// Nombres de lo que se puede deshacer y rehacer (el último primero)
    pub fn history(&self) -> (Vec<String>, Vec<String>) {
        let h = self.history.lock().unwrap();
        (h.undo.iter().rev().map(|s| s.label.clone()).collect(), h.redo.iter().rev().map(|s| s.label.clone()).collect())
    }

    /// Proyecto nuevo o abierto: se puede volver al anterior con `undo`
    pub fn replaced(&self, label: &str, before: Before, path: Option<PathBuf>, ui: String) {
        let mut project = self.project.lock().unwrap();
        let mut h = self.history.lock().unwrap();
        h.undo.push(Step { label: label.into(), state: before.state, file: before.file });
        if h.undo.len() > HISTORY {
            h.undo.remove(0);
        }
        h.redo.clear();
        *project = ProjectFile { path, ui, dirty: false };
    }
}
