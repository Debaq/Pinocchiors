//! Calibraciones guardadas, por impresora física (la de red, por su id) y filamento.
//!
//! Viven en `calibraciones.json`, en la carpeta de datos de la app (no en el navegador): así las leen el laminador, el
//! conector MCP sin ventana y la IA. Cada valor lleva la fecha, la prueba de la que salió y su historial. Al laminar
//! para una impresora con un filamento calibrado, sus valores van como base del perfil (los cambios a mano encima).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Un valor calibrado
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalValue {
    pub value: f64,
    /// Fecha en UTC (AAAA-MM-DDTHH:MMZ); la interfaz la muestra en hora local
    pub date: String,
    /// Prueba de la que salió (`temperature`, `flow_fine`…) o `manual`
    #[serde(default)]
    pub test: String,
    #[serde(default)]
    pub note: String,
    /// Valores anteriores (el más nuevo al final)
    #[serde(default)]
    pub history: Vec<(f64, String)>,
}

/// Un filamento en una impresora
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FilamentCal {
    /// Perfil de filamento del laminador
    pub filament: String,
    #[serde(default)]
    pub values: BTreeMap<String, CalValue>,
}

/// Una impresora física
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PrinterCal {
    /// Id de la impresora de red en la app
    pub id: String,
    pub name: String,
    /// Perfil de máquina del laminador (`Creality K1 (0.4 nozzle)`)
    #[serde(default)]
    pub machine: String,
    /// Lo que es de la impresora sin importar el filamento (Z offset, input shaping, holguras…)
    #[serde(default)]
    pub values: BTreeMap<String, CalValue>,
    #[serde(default)]
    pub filaments: Vec<FilamentCal>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CalibrationStore {
    #[serde(default)]
    pub printers: Vec<PrinterCal>,
}

/// Qué se calibra: la clave, si es de la impresora (sin filamento) y la prueba que lo da
pub struct CalKey {
    pub key: &'static str,
    pub printer_level: bool,
    pub test: &'static str,
}

pub const KEYS: &[CalKey] = &[
    CalKey { key: "z_offset", printer_level: true, test: "first_layer" },
    CalKey { key: "shaper_freq_x", printer_level: true, test: "shaper_frequency" },
    CalKey { key: "shaper_freq_y", printer_level: true, test: "shaper_frequency" },
    CalKey { key: "shaper_damping", printer_level: true, test: "shaper_damping" },
    CalKey { key: "vfa_speed", printer_level: true, test: "vfa" },
    CalKey { key: "corner_speed", printer_level: true, test: "cornering" },
    CalKey { key: "fit_clearance", printer_level: true, test: "clearance" },
    CalKey { key: "hole_comp", printer_level: true, test: "block" },
    CalKey { key: "temperature", printer_level: false, test: "temperature" },
    CalKey { key: "flow_ratio", printer_level: false, test: "flow_fine" },
    CalKey { key: "pressure_advance", printer_level: false, test: "pressure_advance" },
    CalKey { key: "retraction_length", printer_level: false, test: "retraction" },
    CalKey { key: "max_volumetric_speed", printer_level: false, test: "volumetric_speed" },
    CalKey { key: "shrink_x", printer_level: false, test: "block" },
    CalKey { key: "shrink_y", printer_level: false, test: "block" },
    CalKey { key: "shrink_z", printer_level: false, test: "block" },
];

pub fn key_info(key: &str) -> Option<&'static CalKey> {
    KEYS.iter().find(|k| k.key == key)
}

static LOCK: Mutex<()> = Mutex::new(());

/// La carpeta de datos de la app (la misma que `app_data_dir` de Tauri); `PINOCCHIO_DATA_DIR` la reemplaza
pub fn data_dir() -> Option<PathBuf> {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(dir) = env("PINOCCHIO_DATA_DIR") {
        return Some(dir);
    }
    let base = if cfg!(windows) {
        env("APPDATA")
    } else if cfg!(target_os = "macos") {
        env("HOME").map(|h| h.join("Library").join("Application Support"))
    } else {
        env("XDG_DATA_HOME").or_else(|| env("HOME").map(|h| h.join(".local").join("share")))
    };
    base.map(|b| b.join("com.pinocchio.desktop"))
}

fn store_path() -> Result<PathBuf, String> {
    Ok(data_dir().ok_or("No se encontró la carpeta de datos de la app")?.join("calibraciones.json"))
}

/// Lee las calibraciones (vacías si todavía no hay archivo)
pub fn load() -> Result<CalibrationStore, String> {
    let path = store_path()?;
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| format!("calibraciones.json dañado: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(CalibrationStore::default()),
        Err(e) => Err(format!("No se pudo leer {}: {e}", path.display())),
    }
}

fn save(store: &CalibrationStore) -> Result<(), String> {
    let path = store_path()?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("No se pudo crear {}: {e}", dir.display()))?;
    }
    // Se escribe aparte y se reemplaza: un corte a la mitad no deja el archivo roto
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(store).map_err(|e| e.to_string())?).map_err(|e| format!("No se pudo guardar: {e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("No se pudo guardar: {e}"))
}

/// Cambia las calibraciones guardadas con `edit` (de a una a la vez)
fn update<T>(edit: impl FnOnce(&mut CalibrationStore) -> Result<T, String>) -> Result<T, String> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut store = load()?;
    let out = edit(&mut store)?;
    save(&store)?;
    Ok(out)
}

/// Ahora, en UTC (AAAA-MM-DDTHH:MMZ)
fn now() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Días desde 1970 → fecha civil (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}Z", rest / 3600, (rest % 3600) / 60)
}

/// Quién: la impresora física (id, nombre, perfil de máquina) y el filamento
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CalTarget {
    pub printer_id: String,
    #[serde(default)]
    pub printer_name: String,
    #[serde(default)]
    pub machine: String,
    #[serde(default)]
    pub filament: String,
}

fn printer_mut<'a>(store: &'a mut CalibrationStore, t: &CalTarget) -> &'a mut PrinterCal {
    let i = match store.printers.iter().position(|p| p.id == t.printer_id) {
        Some(i) => i,
        None => {
            store.printers.push(PrinterCal { id: t.printer_id.clone(), ..Default::default() });
            store.printers.len() - 1
        }
    };
    let p = &mut store.printers[i];
    if !t.printer_name.is_empty() {
        p.name = t.printer_name.clone();
    }
    if !t.machine.is_empty() {
        p.machine = t.machine.clone();
    }
    p
}

/// Guarda `key` = `value` (de la impresora o de la impresora con el filamento, según la clave)
pub fn set_value(t: &CalTarget, key: &str, value: f64, test: &str, note: &str) -> Result<CalibrationView, String> {
    let info = key_info(key).ok_or_else(|| format!("No se calibra «{key}»"))?;
    if !value.is_finite() {
        return Err("El valor no es un número".into());
    }
    if t.printer_id.trim().is_empty() {
        return Err("Falta la impresora".into());
    }
    if !info.printer_level && t.filament.trim().is_empty() {
        return Err("Falta el filamento".into());
    }
    update(|store| {
        let p = printer_mut(store, t);
        let values = if info.printer_level {
            &mut p.values
        } else {
            let i = match p.filaments.iter().position(|f| f.filament == t.filament) {
                Some(i) => i,
                None => {
                    p.filaments.push(FilamentCal { filament: t.filament.clone(), ..Default::default() });
                    p.filaments.len() - 1
                }
            };
            &mut p.filaments[i].values
        };
        let mut history = values.get(key).map(|v| {
            let mut h = v.history.clone();
            h.push((v.value, v.date.clone()));
            h
        });
        if let Some(h) = history.as_mut() {
            // Las últimas 20 bastan
            let extra = h.len().saturating_sub(20);
            h.drain(..extra);
        }
        values.insert(key.to_string(), CalValue { value, date: now(), test: test.to_string(), note: note.to_string(), history: history.unwrap_or_default() });
        Ok(())
    })?;
    view(t)
}

/// Borra `key` (o todo lo de la impresora con ese filamento, sin clave)
pub fn clear(t: &CalTarget, key: Option<&str>) -> Result<CalibrationView, String> {
    update(|store| {
        let Some(p) = store.printers.iter_mut().find(|p| p.id == t.printer_id) else { return Ok(()) };
        match key {
            Some(k) if key_info(k).is_some_and(|i| i.printer_level) => {
                p.values.remove(k);
            }
            Some(k) => {
                if let Some(f) = p.filaments.iter_mut().find(|f| f.filament == t.filament) {
                    f.values.remove(k);
                }
            }
            None => p.filaments.retain(|f| f.filament != t.filament),
        }
        Ok(())
    })?;
    view(t)
}

/// Lo calibrado de una impresora con un filamento
#[derive(Debug, Clone, Default, Serialize)]
pub struct CalibrationView {
    pub printer_id: String,
    pub printer_name: String,
    pub machine: String,
    pub filament: String,
    /// De la impresora y del filamento en ella, juntos
    pub values: BTreeMap<String, CalValue>,
    /// Filamentos calibrados en esta impresora
    pub filaments: Vec<String>,
}

pub fn view(t: &CalTarget) -> Result<CalibrationView, String> {
    let store = load()?;
    let mut out = CalibrationView { printer_id: t.printer_id.clone(), printer_name: t.printer_name.clone(), machine: t.machine.clone(), filament: t.filament.clone(), ..Default::default() };
    if let Some(p) = store.printers.iter().find(|p| p.id == t.printer_id) {
        out.printer_name = p.name.clone();
        out.machine = p.machine.clone();
        out.values = p.values.clone();
        out.filaments = p.filaments.iter().map(|f| f.filament.clone()).collect();
        if let Some(f) = p.filaments.iter().find(|f| f.filament == t.filament) {
            out.values.extend(f.values.clone());
        }
    }
    Ok(out)
}

fn first(config: &Map<String, Value>, key: &str) -> Option<f64> {
    let v = match config.get(key)? {
        Value::Array(l) => l.first()?.clone(),
        other => other.clone(),
    };
    v.as_f64().or_else(|| v.as_str()?.trim().trim_end_matches('%').parse().ok())
}

fn fmt(v: f64) -> String {
    let s = format!("{v:.5}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Los cambios al perfil del laminador que salen de lo calibrado (claves de Orca). `base` es la configuración del perfil
/// (de ahí sale cuánto más caliente va la primera capa y si la impresora es Klipper)
pub fn overrides(values: &BTreeMap<String, CalValue>, base: &Map<String, Value>) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    let get = |k: &str| values.get(k).map(|v| v.value);
    let s = |v: f64| Value::String(fmt(v));
    if let Some(t) = get("temperature") {
        let delta = match (first(base, "nozzle_temperature_initial_layer"), first(base, "nozzle_temperature")) {
            (Some(a), Some(b)) => a - b,
            _ => 0.0,
        };
        out.insert("nozzle_temperature".into(), s(t.round()));
        out.insert("nozzle_temperature_initial_layer".into(), s((t + delta).round()));
    }
    if let Some(f) = get("flow_ratio") {
        out.insert("filament_flow_ratio".into(), s(f));
    }
    if let Some(pa) = get("pressure_advance") {
        out.insert("enable_pressure_advance".into(), Value::String("1".into()));
        out.insert("pressure_advance".into(), s(pa));
    }
    if let Some(r) = get("retraction_length") {
        out.insert("filament_retraction_length".into(), s(r));
    }
    if let Some(v) = get("max_volumetric_speed") {
        out.insert("filament_max_volumetric_speed".into(), s(v));
    }
    // Las paredes de afuera no pasan de la velocidad sin marcas
    if let (Some(v), Some(cur)) = (get("vfa_speed"), first(base, "outer_wall_speed"))
        && v < cur
    {
        out.insert("outer_wall_speed".into(), s(v));
    }
    // Klipper: Z offset e input shaping al final del gcode de inicio (solo para esta impresión, no toca printer.cfg)
    if base.get("gcode_flavor").and_then(Value::as_str) == Some("klipper") {
        let mut lines = Vec::new();
        // Si la impresora guarda su propio Z (Creality de fábrica, prueba «printer»), no se repite en el gcode
        if let Some(z) = values.get("z_offset").filter(|v| v.test != "printer").map(|v| v.value) {
            lines.push(format!("SET_GCODE_OFFSET Z={} MOVE=0 ; Pinocchio: primera capa calibrada", fmt(z)));
        }
        if let (Some(x), Some(y)) = (get("shaper_freq_x"), get("shaper_freq_y")) {
            let damping = get("shaper_damping").map(|d| format!(" DAMPING_RATIO_X={0} DAMPING_RATIO_Y={0}", fmt(d))).unwrap_or_default();
            lines.push(format!("SET_INPUT_SHAPER SHAPER_FREQ_X={} SHAPER_FREQ_Y={}{damping} ; Pinocchio: input shaping calibrado", fmt(x), fmt(y)));
        }
        if let Some(c) = get("corner_speed") {
            lines.push(format!("SET_VELOCITY_LIMIT SQUARE_CORNER_VELOCITY={} ; Pinocchio: esquinas calibradas", fmt(c)));
        }
        if !lines.is_empty() {
            let start = base.get("machine_start_gcode").and_then(Value::as_str).unwrap_or("").trim_end().to_string();
            let joined = if start.is_empty() { lines.join("\n") } else { format!("{start}\n{}", lines.join("\n")) };
            out.insert("machine_start_gcode".into(), Value::String(joined));
        }
    }
    out
}

// ═══════════════════════════════════════════════════════════════════════════
// COMANDOS
// ═══════════════════════════════════════════════════════════════════════════

#[tauri::command]
pub fn calibration_get(target: CalTarget) -> Result<CalibrationView, String> {
    view(&target)
}

#[tauri::command]
pub fn calibration_save(target: CalTarget, key: String, value: f64, test: Option<String>, note: Option<String>) -> Result<CalibrationView, String> {
    set_value(&target, &key, value, test.as_deref().unwrap_or("manual"), note.as_deref().unwrap_or(""))
}

#[tauri::command]
pub fn calibration_clear(target: CalTarget, key: Option<String>) -> Result<CalibrationView, String> {
    clear(&target, key.as_deref())
}

/// Todas las impresoras con algo calibrado
#[tauri::command]
pub fn calibration_list() -> Result<CalibrationStore, String> {
    load()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Carpeta de datos propia para la prueba (las pruebas de este módulo corren de a una)
    pub(crate) fn with_data_dir<T>(run: impl FnOnce() -> T) -> T {
        static SERIAL: Mutex<()> = Mutex::new(());
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("pinocchio-cal-{}-{:?}", std::process::id(), std::thread::current().id()));
        std::fs::create_dir_all(&dir).unwrap();
        // SAFETY: las pruebas que tocan esta variable se serializan con SERIAL
        unsafe { std::env::set_var("PINOCCHIO_DATA_DIR", &dir) };
        let out = run();
        std::fs::remove_dir_all(&dir).ok();
        out
    }

    fn k1() -> CalTarget {
        CalTarget { printer_id: "k1-436a".into(), printer_name: "K1-436A".into(), machine: "Creality K1 (0.4 nozzle)".into(), filament: "Creality Generic PLA @K1-all".into() }
    }

    #[test]
    fn la_fecha_es_valida() {
        let d = now();
        assert_eq!(d.len(), 17, "{d}");
        assert!(d.starts_with("20") && d.ends_with('Z') && &d[4..5] == "-" && &d[10..11] == "T", "{d}");
    }

    #[test]
    fn guardar_leer_e_historial() {
        with_data_dir(|| {
            let t = k1();
            assert!(view(&t).unwrap().values.is_empty());
            set_value(&t, "temperature", 215.0, "temperature", "").unwrap();
            set_value(&t, "z_offset", -0.04, "first_layer", "").unwrap();
            let v = set_value(&t, "temperature", 210.0, "temperature", "menos hilos").unwrap();
            assert_eq!(v.values["temperature"].value, 210.0);
            assert_eq!(v.values["temperature"].history.len(), 1);
            assert_eq!(v.values["temperature"].history[0].0, 215.0);
            assert_eq!(v.values["z_offset"].value, -0.04);
            // Otro filamento: la temperatura no, el Z offset (de la impresora) sí
            let petg = CalTarget { filament: "PETG".into(), ..k1() };
            let v = view(&petg).unwrap();
            assert!(!v.values.contains_key("temperature") && v.values.contains_key("z_offset"));
            assert_eq!(v.filaments, vec!["Creality Generic PLA @K1-all".to_string()]);
            // Errores claros
            assert!(set_value(&t, "nada", 1.0, "", "").is_err());
            assert!(set_value(&CalTarget { filament: String::new(), ..k1() }, "temperature", 200.0, "", "").is_err());
            // Borrar
            let v = clear(&t, Some("temperature")).unwrap();
            assert!(!v.values.contains_key("temperature") && v.values.contains_key("z_offset"));
        });
    }

    #[test]
    fn de_lo_calibrado_salen_los_cambios_del_perfil() {
        let mut values = BTreeMap::new();
        let v = |x: f64| CalValue { value: x, date: String::new(), test: String::new(), note: String::new(), history: Vec::new() };
        values.insert("temperature".to_string(), v(212.0));
        values.insert("flow_ratio".to_string(), v(0.975));
        values.insert("pressure_advance".to_string(), v(0.036));
        values.insert("z_offset".to_string(), v(-0.02));
        values.insert("vfa_speed".to_string(), v(120.0));
        let mut base = Map::new();
        base.insert("nozzle_temperature".into(), serde_json::json!(["220"]));
        base.insert("nozzle_temperature_initial_layer".into(), serde_json::json!(["225"]));
        base.insert("outer_wall_speed".into(), serde_json::json!("200"));
        base.insert("gcode_flavor".into(), serde_json::json!("klipper"));
        base.insert("machine_start_gcode".into(), serde_json::json!("START_PRINT"));
        let o = overrides(&values, &base);
        assert_eq!(o["nozzle_temperature"], "212");
        assert_eq!(o["nozzle_temperature_initial_layer"], "217");
        assert_eq!(o["filament_flow_ratio"], "0.975");
        assert_eq!(o["enable_pressure_advance"], "1");
        assert_eq!(o["outer_wall_speed"], "120");
        assert!(o["machine_start_gcode"].as_str().unwrap().starts_with("START_PRINT\nSET_GCODE_OFFSET Z=-0.02"));
        // Sin Klipper no se toca el gcode de inicio
        base.insert("gcode_flavor".into(), serde_json::json!("marlin"));
        assert!(!overrides(&values, &base).contains_key("machine_start_gcode"));
    }
}
