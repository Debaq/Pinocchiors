//! Calibraciones guardadas, por impresora física (la de red, por su id), boquilla y filamento.
//!
//! Qué depende de qué: el input shaping, el VFA, las esquinas y la escuadra de ejes (skew) son de la impresora, con
//! cualquier boquilla; la altura de la primera capa, la holgura y la compensación de agujeros dependen de la boquilla
//! puesta; temperatura, flujo, pressure advance, retracción, velocidad volumétrica y contracción dependen del filamento
//! y de la boquilla. La boquilla sale del perfil de máquina del laminador (`Creality K1 (0.6 nozzle)` → 0,6 mm). Lo
//! guardado antes de que existiera la boquilla se toma como de 0,4 mm.
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

/// Boquilla que se supone cuando no se sabe (y para lo guardado antes de calibrar por boquilla)
pub const DEFAULT_NOZZLE: f64 = 0.4;

fn default_nozzle() -> f64 {
    DEFAULT_NOZZLE
}

/// Dos diámetros de boquilla son la misma (0,4 y 0,40)
fn same_nozzle(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.005
}

/// Un filamento en una impresora con una boquilla
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilamentCal {
    /// Perfil de filamento del laminador
    pub filament: String,
    /// Diámetro de la boquilla (mm); lo viejo, sin boquilla, es de 0,4
    #[serde(default = "default_nozzle")]
    pub nozzle: f64,
    #[serde(default)]
    pub values: BTreeMap<String, CalValue>,
}

impl Default for FilamentCal {
    fn default() -> Self {
        Self { filament: String::new(), nozzle: DEFAULT_NOZZLE, values: BTreeMap::new() }
    }
}

/// Lo de la impresora que depende de la boquilla puesta (primera capa, holgura, agujeros)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NozzleCal {
    pub nozzle: f64,
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
    /// Lo que es de la impresora sin importar el filamento ni la boquilla (input shaping, VFA, esquinas, escuadra)
    #[serde(default)]
    pub values: BTreeMap<String, CalValue>,
    /// Lo que es de la impresora pero cambia con la boquilla (Z offset, holgura, agujeros), por diámetro
    #[serde(default)]
    pub nozzles: Vec<NozzleCal>,
    /// Filamentos, cada uno con su boquilla
    #[serde(default)]
    pub filaments: Vec<FilamentCal>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CalibrationStore {
    #[serde(default)]
    pub printers: Vec<PrinterCal>,
}

/// Qué se calibra: la clave, si es de la impresora (sin filamento), si cambia con la boquilla y la prueba que lo da
pub struct CalKey {
    pub key: &'static str,
    /// No depende del filamento
    pub printer_level: bool,
    /// Cambia con el diámetro de la boquilla (los del filamento siempre)
    pub nozzle_level: bool,
    pub test: &'static str,
}

const fn key(key: &'static str, printer_level: bool, nozzle_level: bool, test: &'static str) -> CalKey {
    CalKey { key, printer_level, nozzle_level, test }
}

pub const KEYS: &[CalKey] = &[
    key("z_offset", true, true, "first_layer"),
    key("shaper_freq_x", true, false, "shaper_frequency"),
    key("shaper_freq_y", true, false, "shaper_frequency"),
    key("shaper_damping", true, false, "shaper_damping"),
    key("vfa_speed", true, false, "vfa"),
    key("corner_speed", true, false, "cornering"),
    // Factor de escuadra XY (Klipper `SET_SKEW`): tan del ángulo que falta o sobra en la esquina D
    key("skew_xy", true, false, "skew"),
    key("fit_clearance", true, true, "clearance"),
    key("hole_comp", true, true, "block"),
    key("temperature", false, true, "temperature"),
    key("flow_ratio", false, true, "flow_fine"),
    key("pressure_advance", false, true, "pressure_advance"),
    key("retraction_length", false, true, "retraction"),
    key("max_volumetric_speed", false, true, "volumetric_speed"),
    key("shrink_x", false, true, "block"),
    key("shrink_y", false, true, "block"),
    key("shrink_z", false, true, "block"),
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
        Ok(bytes) => {
            let mut store: CalibrationStore = serde_json::from_slice(&bytes).map_err(|e| format!("calibraciones.json dañado: {e}"))?;
            migrate(&mut store);
            Ok(store)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(CalibrationStore::default()),
        Err(e) => Err(format!("No se pudo leer {}: {e}", path.display())),
    }
}

/// Lo guardado antes de calibrar por boquilla: el Z offset, la holgura y la compensación de agujeros eran de la
/// impresora sin más, y los filamentos no decían su boquilla (entran como de 0,4 mm, ver `FilamentCal::nozzle`)
pub fn migrate(store: &mut CalibrationStore) {
    for p in &mut store.printers {
        let moving: Vec<String> = p.values.keys().filter(|k| key_info(k).is_some_and(|i| i.nozzle_level)).cloned().collect();
        if moving.is_empty() {
            continue;
        }
        let i = match p.nozzles.iter().position(|n| same_nozzle(n.nozzle, DEFAULT_NOZZLE)) {
            Some(i) => i,
            None => {
                p.nozzles.push(NozzleCal { nozzle: DEFAULT_NOZZLE, values: BTreeMap::new() });
                p.nozzles.len() - 1
            }
        };
        for k in moving {
            if let Some(v) = p.values.remove(&k) {
                // Si ya hay uno de 0,4 guardado con el modelo nuevo, ese manda
                p.nozzles[i].values.entry(k).or_insert(v);
            }
        }
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
    /// Diámetro de la boquilla (mm). Casi siempre se deja vacío y sale del perfil de máquina
    #[serde(default)]
    pub nozzle: Option<f64>,
}

impl CalTarget {
    /// La boquilla de esta calibración: la pedida, la del perfil de máquina del laminador o 0,4
    pub fn nozzle(&self) -> f64 {
        self.nozzle.filter(|n| n.is_finite() && *n > 0.0).unwrap_or_else(|| nozzle_of_machine(&self.machine))
    }
}

/// Diámetro de la boquilla de un perfil de máquina del laminador (`Creality K1 (0.6 nozzle)` → 0,6)
pub fn nozzle_of_machine(machine: &str) -> f64 {
    if let Some(p) = pinocchio_slicer::index().printers.iter().find(|p| p.name == machine)
        && p.nozzle > 0.0
    {
        return p.nozzle;
    }
    // Un perfil que ya no está en el índice: se lee del nombre («0.6 nozzle», «(0.6 nozzle)»)
    machine
        .split_whitespace()
        .zip(machine.split_whitespace().skip(1))
        .find(|(_, w)| w.trim_end_matches(')').eq_ignore_ascii_case("nozzle"))
        .and_then(|(n, _)| n.trim_start_matches('(').parse::<f64>().ok())
        .filter(|n| *n > 0.0 && *n < 5.0)
        .unwrap_or(DEFAULT_NOZZLE)
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

/// Dónde vive `info` en la impresora: sus valores sueltos, los de la boquilla o los del filamento con esa boquilla
fn values_mut<'a>(p: &'a mut PrinterCal, info: &CalKey, t: &CalTarget) -> &'a mut BTreeMap<String, CalValue> {
    let nozzle = t.nozzle();
    if info.printer_level && !info.nozzle_level {
        return &mut p.values;
    }
    if info.printer_level {
        let i = match p.nozzles.iter().position(|n| same_nozzle(n.nozzle, nozzle)) {
            Some(i) => i,
            None => {
                p.nozzles.push(NozzleCal { nozzle, values: BTreeMap::new() });
                p.nozzles.len() - 1
            }
        };
        return &mut p.nozzles[i].values;
    }
    let i = match p.filaments.iter().position(|f| f.filament == t.filament && same_nozzle(f.nozzle, nozzle)) {
        Some(i) => i,
        None => {
            p.filaments.push(FilamentCal { filament: t.filament.clone(), nozzle, ..Default::default() });
            p.filaments.len() - 1
        }
    };
    &mut p.filaments[i].values
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
        let values = values_mut(p, info, t);
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
        let nozzle = t.nozzle();
        match key {
            Some(k) => {
                if let Some(info) = key_info(k) {
                    values_mut(p, info, t).remove(k);
                }
            }
            None => p.filaments.retain(|f| !(f.filament == t.filament && same_nozzle(f.nozzle, nozzle))),
        }
        Ok(())
    })?;
    view(t)
}

/// Lo calibrado de una impresora con una boquilla y un filamento
#[derive(Debug, Clone, Default, Serialize)]
pub struct CalibrationView {
    pub printer_id: String,
    pub printer_name: String,
    pub machine: String,
    pub filament: String,
    /// Boquilla de esta vista (mm)
    pub nozzle: f64,
    /// De la impresora, de la impresora con esta boquilla y del filamento con esta boquilla, juntos
    pub values: BTreeMap<String, CalValue>,
    /// Filamentos calibrados en esta impresora con esta boquilla
    pub filaments: Vec<String>,
    /// Boquillas con algo calibrado en esta impresora (mm, de menor a mayor)
    pub nozzles: Vec<f64>,
    /// Este filamento tiene al menos un valor propio calibrado con esta boquilla (los de otra boquilla no cuentan)
    pub calibrated: bool,
}

pub fn view(t: &CalTarget) -> Result<CalibrationView, String> {
    let store = load()?;
    let nozzle = t.nozzle();
    let mut out = CalibrationView { printer_id: t.printer_id.clone(), printer_name: t.printer_name.clone(), machine: t.machine.clone(), filament: t.filament.clone(), nozzle, ..Default::default() };
    if let Some(p) = store.printers.iter().find(|p| p.id == t.printer_id) {
        out.printer_name = p.name.clone();
        if out.machine.is_empty() {
            out.machine = p.machine.clone();
        }
        out.values = p.values.clone();
        if let Some(n) = p.nozzles.iter().find(|n| same_nozzle(n.nozzle, nozzle)) {
            out.values.extend(n.values.clone());
        }
        out.filaments = p.filaments.iter().filter(|f| same_nozzle(f.nozzle, nozzle) && !f.values.is_empty()).map(|f| f.filament.clone()).collect();
        if let Some(f) = p.filaments.iter().find(|f| f.filament == t.filament && same_nozzle(f.nozzle, nozzle)) {
            out.calibrated = !f.values.is_empty();
            out.values.extend(f.values.clone());
        }
        let mut nozzles: Vec<f64> = p.nozzles.iter().filter(|n| !n.values.is_empty()).map(|n| n.nozzle).chain(p.filaments.iter().filter(|f| !f.values.is_empty()).map(|f| f.nozzle)).collect();
        nozzles.sort_by(f64::total_cmp);
        nozzles.dedup_by(|a, b| same_nozzle(*a, *b));
        out.nozzles = nozzles;
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

/// Medidas (AC, BD, AD, mm) de un cuadrado de 100 mm con el factor de escuadra `factor`, para `SET_SKEW XY=AC,BD,AD`:
/// Klipper solo recibe las tres medidas, así que de un factor guardado se arma un cuadrado que da el mismo factor
pub fn skew_measures(factor: f64) -> (f64, f64, f64) {
    let (ad, side) = (100.0_f64, 100.0_f64);
    // Ángulo en la esquina D: 90° + atan(factor) (ver `slicer::calib::skew_factor`)
    let cos_d = (std::f64::consts::FRAC_PI_2 + factor.atan()).cos();
    let ac = (ad * ad + side * side - 2.0 * ad * side * cos_d).sqrt();
    let bd = (ad * ad + side * side + 2.0 * ad * side * cos_d).sqrt();
    // Redondeados a 1/1000 mm, que es lo que importa
    ((ac * 1000.0).round() / 1000.0, (bd * 1000.0).round() / 1000.0, ad)
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
        // Escuadra de ejes: necesita `[skew_correction]` en el printer.cfg. En las que no se pueden tocar (prueba «printer»:
        // K1 de fábrica, AD5X) el valor solo se anota
        if let Some(f) = values.get("skew_xy").filter(|v| v.test != "printer").map(|v| v.value) {
            let (ac, bd, ad) = skew_measures(f);
            lines.push(format!("SET_SKEW XY={},{},{} ; Pinocchio: escuadra de ejes calibrada (necesita [skew_correction] en printer.cfg)", fmt(ac), fmt(bd), fmt(ad)));
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
        CalTarget { printer_id: "k1-436a".into(), printer_name: "K1-436A".into(), machine: "Creality K1 (0.4 nozzle)".into(), filament: "Creality Generic PLA @K1-all".into(), nozzle: None }
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

    fn k1_nozzle(n: &str) -> CalTarget {
        CalTarget { machine: format!("Creality K1 ({n} nozzle)"), ..k1() }
    }

    #[test]
    fn la_boquilla_sale_del_perfil_de_maquina() {
        assert_eq!(k1().nozzle(), 0.4);
        assert_eq!(k1_nozzle("0.8").nozzle(), 0.8);
        assert_eq!(nozzle_of_machine("Flashforge AD5X 1.0 nozzle"), 1.0);
        // Un perfil que no está en el índice: se lee del nombre; sin nombre claro, 0,4
        assert_eq!(nozzle_of_machine("Mi impresora (0.25 nozzle)"), 0.25);
        assert_eq!(nozzle_of_machine("Mi impresora"), 0.4);
        assert_eq!(CalTarget { nozzle: Some(0.6), ..k1() }.nozzle(), 0.6);
    }

    #[test]
    fn lo_guardado_sin_boquilla_pasa_a_0_4() {
        with_data_dir(|| {
            // El formato de antes: Z offset y holgura sueltos en la impresora, filamentos sin boquilla
            let old = r#"{"printers":[{"id":"k1-436a","name":"K1-436A","machine":"Creality K1 (0.6 nozzle)",
                "values":{"z_offset":{"value":-0.04,"date":"2026-11-06T10:00Z"},"shaper_freq_x":{"value":55.0,"date":"2026-11-06T10:05Z"},
                          "fit_clearance":{"value":0.3,"date":"2026-11-06T10:10Z"}},
                "filaments":[{"filament":"Creality Generic PLA @K1-all","values":{"temperature":{"value":212.0,"date":"2026-11-06T11:00Z"}}}]}]}"#;
            std::fs::write(data_dir().unwrap().join("calibraciones.json"), old).unwrap();
            let store = load().unwrap();
            let p = &store.printers[0];
            assert!(p.values.contains_key("shaper_freq_x") && !p.values.contains_key("z_offset") && !p.values.contains_key("fit_clearance"));
            assert_eq!(p.nozzles.len(), 1);
            assert_eq!(p.nozzles[0].nozzle, 0.4);
            assert!(p.nozzles[0].values.contains_key("z_offset") && p.nozzles[0].values.contains_key("fit_clearance"));
            assert_eq!(p.filaments[0].nozzle, 0.4);
            // Con la de 0,4: todo; con la de 0,6 (la que decía el perfil guardado): solo lo de la impresora
            let v04 = view(&k1()).unwrap();
            assert_eq!((v04.values["z_offset"].value, v04.values["temperature"].value, v04.nozzle, v04.calibrated), (-0.04, 212.0, 0.4, true));
            let v06 = view(&k1_nozzle("0.6")).unwrap();
            assert!(v06.values.contains_key("shaper_freq_x") && !v06.values.contains_key("z_offset") && !v06.values.contains_key("temperature") && !v06.calibrated);
            assert_eq!(v06.nozzles, vec![0.4]);
            // Migrar dos veces no cambia nada
            let mut again = store.clone();
            migrate(&mut again);
            assert_eq!(again, store);
        });
    }

    #[test]
    fn una_calibracion_de_0_4_no_se_aplica_con_0_8() {
        with_data_dir(|| {
            let hf = |t: CalTarget| CalTarget { filament: "Creality HF Generic PLA".into(), ..t };
            let (n04, n08) = (hf(k1()), hf(k1_nozzle("0.8")));
            set_value(&n04, "temperature", 212.0, "temperature", "").unwrap();
            set_value(&n04, "flow_ratio", 0.97, "flow_fine", "").unwrap();
            set_value(&n04, "z_offset", -0.04, "first_layer", "").unwrap();
            set_value(&n04, "shaper_freq_x", 55.0, "shaper_frequency", "").unwrap();
            // Con 0,8: ni el filamento ni el Z offset; el input shaping, que es de la impresora, sí
            let v = view(&n08).unwrap();
            assert_eq!(v.nozzle, 0.8);
            assert!(!v.calibrated && v.filaments.is_empty());
            assert!(!v.values.contains_key("temperature") && !v.values.contains_key("z_offset") && v.values.contains_key("shaper_freq_x"));
            // Y de verdad no llega al laminador: con el perfil de 0,4 sí hay cambios, con el de 0,8 no
            let settings = |name: &str| {
                let m = pinocchio_slicer::index().printers.iter().find(|p| p.name == name).unwrap();
                pinocchio_slicer::SliceSettings { printer: m.name.clone(), process: m.default_process.clone(), filament: m.default_filament.clone(), overrides: Default::default() }
            };
            let t04 = crate::slicer::with_calibration(settings("Creality K1 (0.4 nozzle)"), Some(&n04)).unwrap();
            assert_eq!(t04.overrides["filament_flow_ratio"], "0.97");
            // El destino dice 0,4 pero el perfil que se lamina es el de 0,8: manda el perfil
            let t08 = crate::slicer::with_calibration(settings("Creality K1 (0.8 nozzle)"), Some(&n04)).unwrap();
            assert!(!t08.overrides.contains_key("filament_flow_ratio") && !t08.overrides.contains_key("nozzle_temperature"), "{:?}", t08.overrides);
            // Calibrar con 0,8 guarda aparte y no pisa lo de 0,4
            set_value(&n08, "temperature", 225.0, "temperature", "").unwrap();
            assert_eq!(view(&n08).unwrap().values["temperature"].value, 225.0);
            assert_eq!(view(&n04).unwrap().values["temperature"].value, 212.0);
            assert_eq!(view(&n04).unwrap().nozzles, vec![0.4, 0.8]);
            // Borrar un valor de una boquilla no toca el de la otra
            clear(&n08, Some("temperature")).unwrap();
            assert!(!view(&n08).unwrap().values.contains_key("temperature"));
            assert!(view(&n04).unwrap().values.contains_key("temperature"));
        });
    }

    #[test]
    fn la_escuadra_se_aplica_solo_donde_se_puede() {
        let v = |x: f64, test: &str| CalValue { value: x, date: String::new(), test: test.into(), note: String::new(), history: Vec::new() };
        // El cuadrado armado da el mismo factor que se guardó
        for f in [0.0, 0.003, -0.0042, 0.02] {
            let (ac, bd, ad) = skew_measures(f);
            let back = pinocchio_slicer::calib::skew_factor(ac, bd, ad).unwrap();
            assert!((back - f).abs() < 1e-5, "{f} -> {back}");
        }
        let mut base = Map::new();
        base.insert("gcode_flavor".into(), serde_json::json!("klipper"));
        let mut values = BTreeMap::new();
        values.insert("skew_xy".to_string(), v(0.004, "skew"));
        let gcode = overrides(&values, &base)["machine_start_gcode"].as_str().unwrap().to_string();
        assert!(gcode.starts_with("SET_SKEW XY=") && gcode.contains("[skew_correction]"), "{gcode}");
        // Impresora que guarda lo suyo (K1 de fábrica, AD5X): se anota y no va al gcode
        values.insert("skew_xy".to_string(), v(0.004, "printer"));
        assert!(!overrides(&values, &base).contains_key("machine_start_gcode"));
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
