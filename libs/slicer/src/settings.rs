//! Configuración final para el motor: máquina + proceso + filamento + cambios del usuario.

use crate::error::SlicerError;
use crate::presets::{preset, PresetKind};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// Qué laminar con qué perfiles. `overrides` usa las claves de Orca
/// (`layer_height`, `wall_loops`, `sparse_infill_density`…) con valores en texto
/// o listas de texto, igual que en sus perfiles.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SliceSettings {
    pub printer: String,
    pub process: String,
    pub filament: String,
    #[serde(default)]
    pub overrides: BTreeMap<String, Value>,
}

/// Claves de temperatura de cama de cada tipo de placa: la cama de la K1 no
/// declara tipo, así que «temperatura de cama» las cambia todas.
const BED_KEYS: &[&str] = &["hot_plate_temp", "textured_plate_temp", "cool_plate_temp", "eng_plate_temp", "supertack_plate_temp"];

/// Claves que no son del motor y no deben llegar al perfil.
const META_KEYS: &[&str] = &["name", "type", "from", "compatible_printers", "compatible_prints", "compatible_printers_condition", "compatible_prints_condition"];

/// Une los perfiles y los cambios en el JSON que lee el motor.
pub fn build_config(s: &SliceSettings) -> Result<Map<String, Value>, SlicerError> {
    let get = |kind, name: &str| preset(kind, name).ok_or_else(|| SlicerError::UnknownPreset(name.to_string()));
    let machine = get(PresetKind::Machine, &s.printer)?;
    let process = get(PresetKind::Process, &s.process)?;
    let filament = get(PresetKind::Filament, &s.filament)?;

    let mut out = Map::new();
    for part in [machine, process, filament] {
        for (k, v) in part {
            if !META_KEYS.contains(&k.as_str()) {
                out.insert(k.clone(), v.clone());
            }
        }
    }
    for (k, v) in &s.overrides {
        match k.as_str() {
            "bed_temperature" => {
                for key in BED_KEYS {
                    out.insert((*key).to_string(), as_list(v));
                }
            }
            "bed_temperature_initial_layer" => {
                for key in BED_KEYS {
                    out.insert(format!("{key}_initial_layer"), as_list(v));
                }
            }
            _ => {
                // Si el perfil lo tiene como lista (por extrusor o filamento), se respeta la forma.
                let shaped = match out.get(k) {
                    Some(Value::Array(_)) => as_list(v),
                    _ => v.clone(),
                };
                out.insert(k.clone(), shaped);
            }
        }
    }
    out.insert("name".into(), Value::String("pinocchio".into()));
    out.insert("from".into(), Value::String("User".into()));
    out.insert("printer_settings_id".into(), Value::String(s.printer.clone()));
    out.insert("print_settings_id".into(), Value::String(s.process.clone()));
    out.insert("filament_settings_id".into(), Value::Array(vec![Value::String(s.filament.clone())]));
    Ok(out)
}

fn as_list(v: &Value) -> Value {
    match v {
        Value::Array(_) => v.clone(),
        Value::String(_) => Value::Array(vec![v.clone()]),
        other => Value::Array(vec![Value::String(text(other))]),
    }
}

fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Bool(b) => if *b { "1" } else { "0" }.to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::index;

    fn k1() -> SliceSettings {
        let p = index().printers.iter().find(|p| p.name.starts_with("Creality K1 (")).unwrap();
        SliceSettings { printer: p.name.clone(), process: p.default_process.clone(), filament: p.default_filament.clone(), overrides: BTreeMap::new() }
    }

    #[test]
    fn une_maquina_proceso_y_filamento() {
        let c = build_config(&k1()).unwrap();
        assert_eq!(c["gcode_flavor"], "klipper");
        assert_eq!(c["layer_height"], "0.2");
        assert!(c["nozzle_temperature"].is_array());
        assert!(!c.contains_key("inherits"));
        assert!(!c.contains_key("compatible_printers"));
    }

    #[test]
    fn los_cambios_respetan_la_forma_del_perfil() {
        let mut s = k1();
        s.overrides.insert("nozzle_temperature".into(), Value::String("215".into()));
        s.overrides.insert("layer_height".into(), Value::String("0.16".into()));
        s.overrides.insert("bed_temperature".into(), Value::String("55".into()));
        let c = build_config(&s).unwrap();
        assert_eq!(c["nozzle_temperature"], serde_json::json!(["215"]));
        assert_eq!(c["layer_height"], "0.16");
        assert_eq!(c["textured_plate_temp"], serde_json::json!(["55"]));
        assert_eq!(c["hot_plate_temp"], serde_json::json!(["55"]));
    }

    #[test]
    fn perfil_desconocido_es_error() {
        let mut s = k1();
        s.process = "no existe".into();
        assert!(matches!(build_config(&s), Err(SlicerError::UnknownPreset(_))));
    }
}
