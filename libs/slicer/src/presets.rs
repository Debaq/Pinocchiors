//! Perfiles de Orca ya aplanados (sin `inherits`), embebidos en el binario.
//!
//! Salen de `tools/flatten_presets.py` sobre `resources/profiles` de Orca.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/presets.rs"));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PresetKind {
    Machine,
    Process,
    Filament,
}

impl PresetKind {
    fn from_dir(dir: &str) -> Option<Self> {
        match dir {
            "machine" => Some(Self::Machine),
            "process" => Some(Self::Process),
            "filament" => Some(Self::Filament),
            _ => None,
        }
    }
}

/// Una impresora con los procesos y filamentos que le sirven.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrinterEntry {
    pub name: String,
    pub model: String,
    pub nozzle: f64,
    pub default_process: String,
    pub default_filament: String,
    pub processes: Vec<String>,
    pub filaments: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetIndex {
    pub printers: Vec<PrinterEntry>,
}

/// Impresoras disponibles con sus procesos y filamentos.
pub fn index() -> &'static PresetIndex {
    static INDEX: OnceLock<PresetIndex> = OnceLock::new();
    INDEX.get_or_init(|| serde_json::from_str(PRESET_INDEX).expect("presets/index.json inválido"))
}

fn table() -> &'static HashMap<(PresetKind, String), Map<String, Value>> {
    static TABLE: OnceLock<HashMap<(PresetKind, String), Map<String, Value>>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t = HashMap::new();
        for (dir, _stem, text) in PRESET_FILES {
            let Some(kind) = PresetKind::from_dir(dir) else { continue };
            let Ok(Value::Object(map)) = serde_json::from_str::<Value>(text) else { continue };
            if let Some(name) = map.get("name").and_then(Value::as_str) {
                t.insert((kind, name.to_string()), map);
            }
        }
        t
    })
}

/// Valores de un perfil ya resuelto.
pub fn preset(kind: PresetKind, name: &str) -> Option<&'static Map<String, Value>> {
    table().get(&(kind, name.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_k1_tiene_perfiles_completos() {
        let k1 = index().printers.iter().find(|p| p.name == "Creality K1 (0.4 nozzle)").expect("falta la K1");
        assert!(k1.processes.contains(&k1.default_process));
        assert!(k1.filaments.contains(&k1.default_filament));
        let m = preset(PresetKind::Machine, &k1.name).unwrap();
        assert_eq!(m.get("gcode_flavor").and_then(Value::as_str), Some("klipper"));
        for p in &k1.processes {
            assert!(preset(PresetKind::Process, p).is_some(), "{p}");
        }
        for f in &k1.filaments {
            assert!(preset(PresetKind::Filament, f).is_some(), "{f}");
        }
    }
}
