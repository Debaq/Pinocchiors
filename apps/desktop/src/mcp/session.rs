//! Estado de la sesión del conector: quién ejecuta, el archivo del proyecto y
//! el historial para deshacer.

use std::path::PathBuf;
use std::sync::Mutex;

use crate::mcp::backend::Backend;

/// Pasos que se pueden deshacer
const HISTORY: usize = 20;

pub struct Session {
    pub backend: Box<dyn Backend>,
    pub project: Mutex<ProjectFile>,
    history: Mutex<History>,
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
        }
    }

    /// Estado de antes de un cambio (se guarda con [`commit`](Self::commit))
    pub fn before_change(&self) -> Result<Vec<u8>, String> {
        self.backend.snapshot()
    }

    /// El cambio salió bien: `before` queda para deshacer
    pub fn commit(&self, label: &str, before: Vec<u8>) {
        let mut project = self.project.lock().unwrap();
        let mut h = self.history.lock().unwrap();
        h.undo.push(Step { label: label.into(), state: before, file: project.clone() });
        if h.undo.len() > HISTORY {
            h.undo.remove(0);
        }
        h.redo.clear();
        project.dirty = true;
    }

    /// El cambio falló a mitad: vuelve a como estaba
    pub fn rollback(&self, before: &[u8]) -> Result<(), String> {
        self.backend.restore(before)
    }

    /// Deshace el último cambio; devuelve qué se deshizo
    pub fn undo(&self) -> Result<Option<String>, String> {
        self.travel(true)
    }

    pub fn redo(&self) -> Result<Option<String>, String> {
        self.travel(false)
    }

    fn travel(&self, back: bool) -> Result<Option<String>, String> {
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
    pub fn replaced(&self, label: &str, before: Vec<u8>, path: Option<PathBuf>, ui: String) {
        let mut project = self.project.lock().unwrap();
        let mut h = self.history.lock().unwrap();
        h.undo.push(Step { label: label.into(), state: before, file: project.clone() });
        if h.undo.len() > HISTORY {
            h.undo.remove(0);
        }
        h.redo.clear();
        *project = ProjectFile { path, ui, dirty: false };
    }
}
