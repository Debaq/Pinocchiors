//! Pruebas de calibración FDM: cada una arma sus piezas, los ajustes del perfil y el modo de calibración del motor,
//! como la GUI de OrcaSlicer 2.4.2 (`Plater::calib_*`, `adjust_settings_for_flowrate_calib`), y dice cómo se lee el
//! resultado (qué valor tiene cada bloque, pieza o altura).
//!
//! Los modelos son los de Orca (`resources/calib`), empaquetados por `examples/pack_calib.rs` en `calib/*.bin.gz`.
//! Coordenadas en mm, Z arriba; las piezas salen centradas en la cama.

use std::collections::BTreeMap;
use std::io::Read;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::engine::Calib;
use crate::error::SlicerError;

type Tri = [[f32; 3]; 3];

/// Las pruebas
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalibTest {
    /// Parches de una capa para ajustar la altura de la primera capa (Z offset) mientras imprime
    FirstLayer,
    /// Torre de temperatura: un bloque cada 5 °C
    Temperature,
    /// Flujo, pasada gruesa (−5 % … +5 %)
    FlowCoarse,
    /// Flujo, pasada fina (−4 % … +3,5 % alrededor del de la gruesa)
    FlowFine,
    /// Torre de pressure advance: cambia cada milímetro
    PressureAdvance,
    /// Torre de retracción: cambia cada milímetro
    Retraction,
    /// Velocidad volumétrica máxima: espiral cada vez más rápida
    VolumetricSpeed,
    /// VFA (marcas finas en las paredes): cada 5 mm, más rápido
    Vfa,
    /// Input shaping, frecuencia (Klipper)
    ShaperFrequency,
    /// Input shaping, amortiguación (Klipper)
    ShaperDamping,
    /// Esquinas (jerk / velocidad en las esquinas)
    Cornering,
}

impl CalibTest {
    pub const ALL: [CalibTest; 11] = [
        CalibTest::FirstLayer,
        CalibTest::Temperature,
        CalibTest::FlowCoarse,
        CalibTest::FlowFine,
        CalibTest::PressureAdvance,
        CalibTest::Retraction,
        CalibTest::VolumetricSpeed,
        CalibTest::Vfa,
        CalibTest::ShaperFrequency,
        CalibTest::ShaperDamping,
        CalibTest::Cornering,
    ];
}

/// Rango de la prueba; lo que falte sale de los valores por defecto del tipo de filamento
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct CalibRange {
    #[serde(default)]
    pub start: Option<f64>,
    #[serde(default)]
    pub end: Option<f64>,
    #[serde(default)]
    pub step: Option<f64>,
    /// Input shaping, amortiguación: la frecuencia ya elegida (Hz)
    #[serde(default)]
    pub frequency: Option<f64>,
}

/// Cómo se lee el resultado
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reading {
    /// Bloques apilados (de abajo hacia arriba): se elige el mejor
    Blocks { blocks: Vec<Block> },
    /// Piezas sueltas en la cama: se elige la mejor
    Pieces { pieces: Vec<Piece> },
    /// El valor cambia de a poco con la altura: se mide a qué altura se ve mejor (mm desde la cama)
    Height { height_mm: f64, start: f64, end: f64 },
    /// Se ajusta en vivo mientras imprime (primera capa)
    Live,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub value: f64,
    /// Desde y hasta qué altura (mm)
    pub z0: f64,
    pub z1: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Piece {
    pub value: f64,
    /// Lo que dice la pieza impresa (el número en relieve: «+2», «-1.5»)
    pub mark: String,
    /// Centro en la cama (mm)
    pub x: f64,
    pub y: f64,
}

/// Una prueba lista para laminar
#[derive(Debug, Clone)]
pub struct PreparedTest {
    pub test: CalibTest,
    /// Configuración final (la del perfil con los cambios de la prueba)
    pub config: Map<String, Value>,
    /// Piezas en coordenadas de la cama
    pub parts: Vec<Vec<Tri>>,
    pub calib: Calib,
    pub reading: Reading,
    /// La clave de Orca que se calibra y su unidad, para mostrar
    pub unit: &'static str,
}

// ═══════════════════════════════════════════════════════════════════════════
// MODELOS
// ═══════════════════════════════════════════════════════════════════════════

fn packed(name: &str) -> &'static [u8] {
    match name {
        "temperature_tower" => include_bytes!("../calib/temperature_tower.bin.gz"),
        "pa_tower" => include_bytes!("../calib/pa_tower.bin.gz"),
        "retraction_tower" => include_bytes!("../calib/retraction_tower.bin.gz"),
        "volumetric_speed" => include_bytes!("../calib/volumetric_speed.bin.gz"),
        "vfa" => include_bytes!("../calib/vfa.bin.gz"),
        "ringing_tower" => include_bytes!("../calib/ringing_tower.bin.gz"),
        "cornering" => include_bytes!("../calib/cornering.bin.gz"),
        "flow_coarse" => include_bytes!("../calib/flow_coarse.bin.gz"),
        "flow_fine" => include_bytes!("../calib/flow_fine.bin.gz"),
        _ => &[],
    }
}

/// Piezas de un modelo empaquetado: (nombre, triángulos)
fn model(name: &str) -> Result<Vec<(String, Vec<Tri>)>, SlicerError> {
    let bad = || SlicerError::Engine(format!("modelo de calibración «{name}» dañado"));
    let mut raw = Vec::new();
    flate2::read::GzDecoder::new(packed(name)).read_to_end(&mut raw).map_err(|_| bad())?;
    /// Lector de bytes en orden
    struct Cursor<'a> {
        raw: &'a [u8],
        at: usize,
    }
    impl Cursor<'_> {
        fn take(&mut self, n: usize) -> Option<&[u8]> {
            let b = self.raw.get(self.at..self.at + n)?;
            self.at += n;
            Some(b)
        }
        fn u32(&mut self) -> Option<u32> {
            self.take(4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        }
    }
    let mut c = Cursor { raw: &raw, at: 0 };
    let count = c.u32().ok_or_else(bad)?;
    let mut out = Vec::new();
    for _ in 0..count {
        let len = c.u32().ok_or_else(bad)? as usize;
        let name = String::from_utf8_lossy(c.take(len).ok_or_else(bad)?).into_owned();
        let tris = c.u32().ok_or_else(bad)? as usize;
        let bytes = c.take(tris * 36).ok_or_else(bad)?;
        let f: Vec<f32> = bytes.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect();
        let tris: Vec<Tri> = f.as_chunks::<9>().0.iter().map(|t| [[t[0], t[1], t[2]], [t[3], t[4], t[5]], [t[6], t[7], t[8]]]).collect();
        out.push((name, tris));
    }
    Ok(out)
}

fn bbox(parts: &[Vec<Tri>]) -> ([f32; 3], [f32; 3]) {
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in parts.iter().flatten().flatten() {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    (lo, hi)
}

fn scale(parts: &mut [Vec<Tri>], s: [f32; 3]) {
    for p in parts.iter_mut().flatten().flatten() {
        for k in 0..3 {
            p[k] *= s[k];
        }
    }
}

/// Centra las piezas (juntas) en la cama y las apoya en Z = 0
fn center_on_bed(parts: &mut [Vec<Tri>], bed: ([f64; 2], [f64; 2])) {
    let (lo, hi) = bbox(parts);
    let dx = ((bed.0[0] + bed.1[0]) * 0.5) as f32 - (lo[0] + hi[0]) * 0.5;
    let dy = ((bed.0[1] + bed.1[1]) * 0.5) as f32 - (lo[1] + hi[1]) * 0.5;
    for p in parts.iter_mut().flatten().flatten() {
        p[0] += dx;
        p[1] += dy;
        p[2] -= lo[2];
    }
}

/// Quita los triángulos que quedan enteros fuera de [z0, z1] (para la miniatura; el corte exacto lo hace el motor)
fn trim(parts: &mut [Vec<Tri>], z0: f32, z1: f32) {
    for p in parts.iter_mut() {
        p.retain(|t| {
            let lo = t.iter().map(|v| v[2]).fold(f32::MAX, f32::min);
            let hi = t.iter().map(|v| v[2]).fold(f32::MIN, f32::max);
            hi > z0 && lo < z1
        });
    }
}

/// Un prisma (12 triángulos)
fn cuboid(x0: f32, y0: f32, x1: f32, y1: f32, h: f32) -> Vec<Tri> {
    let v = [[x0, y0, 0.0], [x1, y0, 0.0], [x1, y1, 0.0], [x0, y1, 0.0], [x0, y0, h], [x1, y0, h], [x1, y1, h], [x0, y1, h]];
    let f: [[usize; 3]; 12] = [[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]];
    f.iter().map(|t| [v[t[0]], v[t[1]], v[t[2]]]).collect()
}

// ═══════════════════════════════════════════════════════════════════════════
// CONFIGURACIÓN
// ═══════════════════════════════════════════════════════════════════════════

/// Primer número de una clave (texto o lista de textos), con «%» relativo a `percent_of`
fn num(config: &Map<String, Value>, key: &str) -> Option<f64> {
    num_rel(config, key, 1.0)
}

fn num_rel(config: &Map<String, Value>, key: &str, percent_of: f64) -> Option<f64> {
    let v = match config.get(key)? {
        Value::Array(list) => list.first()?.clone(),
        other => other.clone(),
    };
    let text = match v {
        Value::String(s) => s,
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => (b as u8).to_string(),
        _ => return None,
    };
    let text = text.trim();
    if text == "nil" || text.is_empty() {
        return None;
    }
    match text.strip_suffix('%') {
        Some(p) => p.trim().parse::<f64>().ok().map(|p| p / 100.0 * percent_of),
        None => text.parse().ok(),
    }
}

fn text(config: &Map<String, Value>, key: &str) -> Option<String> {
    match config.get(key)? {
        Value::Array(list) => list.first()?.as_str().map(str::to_string),
        Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

fn fmt(v: f64) -> String {
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" { "0".into() } else { s.to_string() }
}

/// Cambios de configuración de una prueba
struct Edit<'a>(&'a mut Map<String, Value>);

impl Edit<'_> {
    /// Opción simple
    fn set(&mut self, key: &str, v: impl ToString) -> &mut Self {
        self.0.insert(key.to_string(), Value::String(v.to_string()));
        self
    }
    /// Opción por extrusor o filamento (lista): todos los elementos con ese valor
    fn list(&mut self, key: &str, v: impl ToString) -> &mut Self {
        let n = match self.0.get(key) {
            Some(Value::Array(l)) if !l.is_empty() => l.len(),
            _ => 1,
        };
        self.0.insert(key.to_string(), Value::Array(vec![Value::String(v.to_string()); n]));
        self
    }
}

/// Sección de una línea de extrusión (mm²/mm de largo), como `Flow::mm3_per_mm` de Orca
fn mm3_per_mm(width: f64, height: f64) -> f64 {
    height * (width - height * (1.0 - std::f64::consts::FRAC_PI_4))
}

/// La cama: esquinas mínima y máxima de `printable_area`
fn bed(config: &Map<String, Value>) -> ([f64; 2], [f64; 2]) {
    let pts: Vec<[f64; 2]> = match config.get("printable_area") {
        Some(Value::Array(list)) => list
            .iter()
            .filter_map(|p| p.as_str())
            .filter_map(|p| p.split_once('x'))
            .filter_map(|(x, y)| Some([x.trim().parse().ok()?, y.trim().parse().ok()?]))
            .collect(),
        _ => Vec::new(),
    };
    if pts.is_empty() {
        return ([0.0, 0.0], [220.0, 220.0]);
    }
    let lo = [pts.iter().map(|p| p[0]).fold(f64::MAX, f64::min), pts.iter().map(|p| p[1]).fold(f64::MAX, f64::min)];
    let hi = [pts.iter().map(|p| p[0]).fold(f64::MIN, f64::max), pts.iter().map(|p| p[1]).fold(f64::MIN, f64::max)];
    (lo, hi)
}

/// Tipo de filamento normalizado (PLA, PETG, ABS, ASA, TPU, PA, PC…)
pub fn filament_type(config: &Map<String, Value>) -> String {
    text(config, "filament_type").unwrap_or_else(|| "PLA".into()).to_uppercase()
}

/// Rango por defecto de cada prueba según el filamento (inicio, fin, paso)
pub fn default_range(test: CalibTest, config: &Map<String, Value>) -> (f64, f64, f64) {
    let kind = filament_type(config);
    let has = |k: &str| kind.contains(k);
    match test {
        CalibTest::Temperature => {
            let (hi, lo) = if has("PETG") || has("PCTG") {
                (260.0, 220.0)
            } else if has("ABS") || has("ASA") {
                (270.0, 230.0)
            } else if has("TPU") || has("TPE") {
                (240.0, 200.0)
            } else if has("PA") || has("PC") || has("NYLON") {
                (300.0, 260.0)
            } else if has("PLA") {
                (230.0, 190.0)
            } else {
                let t = num(config, "nozzle_temperature").unwrap_or(220.0);
                ((t / 5.0).round() * 5.0 + 20.0, (t / 5.0).round() * 5.0 - 20.0)
            };
            (hi, lo, 5.0)
        }
        CalibTest::PressureAdvance => {
            if has("TPU") || has("TPE") {
                (0.0, 0.2, 0.004)
            } else {
                (0.0, 0.1, 0.002)
            }
        }
        CalibTest::Retraction => {
            if has("TPU") || has("TPE") {
                (0.0, 1.0, 0.1)
            } else {
                (0.0, 2.0, 0.1)
            }
        }
        CalibTest::VolumetricSpeed => {
            if has("TPU") || has("TPE") {
                (2.0, 12.0, 0.5)
            } else {
                (5.0, 30.0, 0.5)
            }
        }
        CalibTest::Vfa => (40.0, 200.0, 10.0),
        CalibTest::ShaperFrequency => (15.0, 110.0, 0.0),
        CalibTest::ShaperDamping => (0.0, 0.2, 0.0),
        CalibTest::Cornering => (1.0, 15.0, 0.0),
        CalibTest::FlowCoarse | CalibTest::FlowFine | CalibTest::FirstLayer => (0.0, 0.0, 0.0),
    }
}

fn need(ok: bool, msg: impl Into<String>) -> Result<(), SlicerError> {
    if ok { Ok(()) } else { Err(SlicerError::Engine(msg.into())) }
}

// ═══════════════════════════════════════════════════════════════════════════
// PRUEBAS
// ═══════════════════════════════════════════════════════════════════════════

/// Arma la prueba `test` sobre la configuración ya unida `base` (máquina + proceso + filamento)
pub fn prepare(test: CalibTest, range: &CalibRange, base: &Map<String, Value>) -> Result<PreparedTest, SlicerError> {
    let (d_start, d_end, d_step) = default_range(test, base);
    let start = range.start.unwrap_or(d_start);
    let end = range.end.unwrap_or(d_end);
    let step = range.step.unwrap_or(d_step);
    let nozzle = num(base, "nozzle_diameter").unwrap_or(0.4);
    let bed = bed(base);
    let mut config = base.clone();
    let mut object: BTreeMap<String, String> = BTreeMap::new();
    let klipper = text(base, "gcode_flavor").as_deref() == Some("klipper");
    let mut calib = Calib { mode: "none".into(), print_numbers: true, ..Default::default() };
    // Lo que queda después del corte del motor, solo para la miniatura (el motor recibe el modelo entero: el corte con
    // tapa de Orca se cae con mallas abiertas)
    let mut thumb: Option<(f32, f32)> = None;
    {
        let mut e = Edit(&mut config);
        e.set("resonance_avoidance", "0").set("enable_wrapping_detection", "0").set("precise_z_height", "0");
    }

    let (parts, reading, unit): (Vec<Vec<Tri>>, Reading, &'static str) = match test {
        // ── Primera capa: cinco parches de una capa (centro y esquinas) ──
        CalibTest::FirstLayer => {
            let h = num(base, "initial_layer_print_height").unwrap_or(0.2) as f32;
            let (lo, hi) = bed;
            let (w, d) = ((hi[0] - lo[0]) as f32, (hi[1] - lo[1]) as f32);
            let side = (w.min(d) * 0.16).clamp(20.0, 40.0);
            let inset = (w.min(d) * 0.12).max(side * 0.5 + 10.0);
            let (cx, cy) = (((lo[0] + hi[0]) * 0.5) as f32, ((lo[1] + hi[1]) * 0.5) as f32);
            let centers = [(cx, cy), (lo[0] as f32 + inset, lo[1] as f32 + inset), (hi[0] as f32 - inset, lo[1] as f32 + inset), (hi[0] as f32 - inset, hi[1] as f32 - inset), (lo[0] as f32 + inset, hi[1] as f32 - inset)];
            let parts = centers.iter().map(|&(x, y)| cuboid(x - side * 0.5, y - side * 0.5, x + side * 0.5, y + side * 0.5, h)).collect();
            Edit(&mut config).set("brim_type", "no_brim").set("skirt_loops", "1").set("bottom_shell_layers", "1").set("top_shell_layers", "1").set("top_surface_pattern", "monotonicline").set("bottom_surface_pattern", "monotonicline");
            (parts, Reading::Live, "mm")
        }

        // ── Torre de temperatura: bloques de 10 mm, 5 °C cada uno (de 500 °C abajo, cortada al rango) ──
        CalibTest::Temperature => {
            let (hi_t, lo_t) = (start.max(end), start.min(end));
            need(hi_t - lo_t >= 5.0 && lo_t >= 150.0 && hi_t <= 350.0, "el rango de temperatura va entre 150 y 350 °C, con al menos 5 °C de diferencia")?;
            let s = (nozzle / 0.4) as f32;
            let mut parts: Vec<Vec<Tri>> = model("temperature_tower")?.into_iter().map(|p| p.1).collect();
            scale(&mut parts, [s, s, s]);
            // Arriba: el bloque de la más fría; abajo: el de la más caliente
            let top = (((500.0 - lo_t) / 5.0).round() + 1.0) * 10.0 * s as f64 - 1e-3;
            let bottom = ((500.0 - hi_t) / 5.0).round() * 10.0 * s as f64 + 1e-3;
            object.insert("@keep_below".into(), fmt(top));
            object.insert("@keep_above".into(), fmt(bottom));
            thumb = Some((bottom as f32, top as f32));
            for (k, v) in [("layer_height", fmt(nozzle / 2.0)), ("brim_type", "outer_only".into()), ("brim_width", "5".into()), ("brim_object_gap", "0".into()), ("alternate_extra_wall", "0".into()), ("seam_slope_type", "none".into()), ("overhang_reverse", "0".into())] {
                object.insert(k.into(), v);
            }
            Edit(&mut config).list("nozzle_temperature_initial_layer", hi_t).list("nozzle_temperature", hi_t).set("initial_layer_print_height", fmt(nozzle / 2.0));
            calib = Calib { mode: "temp_tower".into(), start: hi_t, end: lo_t, step: 5.0, ..calib };
            let block = 10.0 * s as f64;
            let n = ((hi_t - lo_t) / 5.0).round() as usize + 1;
            let blocks = (0..n).map(|i| Block { value: hi_t - 5.0 * i as f64, z0: block * i as f64, z1: block * (i + 1) as f64 }).collect();
            (parts, Reading::Blocks { blocks }, "°C")
        }

        // ── Flujo (YOLO lineal de Orca): piezas con su flujo, se elige la de la superficie más lisa ──
        CalibTest::FlowCoarse | CalibTest::FlowFine => {
            let fine = test == CalibTest::FlowFine;
            let cur = num(base, "filament_flow_ratio").unwrap_or(1.0);
            let layer = nozzle / 2.0;
            let first = num(base, "initial_layer_print_height").unwrap_or(layer).max(layer);
            let pieces = model(if fine { "flow_fine" } else { "flow_coarse" })?;
            let mods: Vec<f64> = pieces
                .iter()
                .map(|(name, _)| {
                    let m = name.strip_prefix("flowrate_").unwrap_or("0");
                    let m = m.strip_prefix('m').map_or_else(|| m.to_string(), |r| format!("-{r}"));
                    m.parse::<f64>().unwrap_or(0.0)
                })
                .collect();
            let mut parts: Vec<Vec<Tri>> = pieces.into_iter().map(|p| p.1).collect();
            let xy = (nozzle / 0.6) as f32;
            let z = ((first + 9.0 * layer) / 2.0) as f32;
            scale(&mut parts, [if xy > 1.2 { xy } else { 1.0 }, if xy > 1.2 { xy } else { 1.0 }, z]);
            center_on_bed(&mut parts, bed);
            let max_vol = num(base, "filament_max_volumetric_speed").unwrap_or(15.0);
            let infill = mm3_per_mm(nozzle * 1.2, layer);
            let max_speed = max_vol / (infill * (cur + if fine { 0.035 } else { 0.05 }) / cur);
            let solid = num(base, "internal_solid_infill_speed").unwrap_or(max_speed).min(max_speed).floor();
            let top = num(base, "top_surface_speed").unwrap_or(max_speed).min(max_speed).floor();
            let mut readings = Vec::new();
            calib.objects = mods
                .iter()
                .zip(&parts)
                .map(|(m, p)| {
                    let (lo, hi) = bbox(std::slice::from_ref(p));
                    readings.push(Piece { value: cur + m, mark: format!("{}{}", if *m > 0.0 { "+" } else { "" }, fmt(m * 100.0)), x: ((lo[0] + hi[0]) * 0.5) as f64, y: ((lo[1] + hi[1]) * 0.5) as f64 });
                    let w = fmt(nozzle * 1.2);
                    [
                        ("wall_loops", "1".to_string()),
                        ("only_one_wall_top", "1".into()),
                        ("thick_internal_bridges", "0".into()),
                        ("enable_extra_bridge_layer", "disabled".into()),
                        ("internal_bridge_density", "100%".into()),
                        ("sparse_infill_density", "35%".into()),
                        ("min_width_top_surface", "100%".into()),
                        ("bottom_shell_layers", "2".into()),
                        ("top_shell_layers", "5".into()),
                        ("top_shell_thickness", "0".into()),
                        ("bottom_shell_thickness", "0".into()),
                        ("detect_thin_wall", "1".into()),
                        ("filter_out_gap_fill", "0".into()),
                        ("sparse_infill_pattern", "rectilinear".into()),
                        ("top_surface_line_width", w.clone()),
                        ("internal_solid_infill_line_width", w),
                        ("top_surface_pattern", "monotonic".into()),
                        ("top_solid_infill_flow_ratio", "1".into()),
                        ("infill_direction", "45".into()),
                        ("solid_infill_direction", "135".into()),
                        ("align_infill_direction_to_model", "1".into()),
                        ("ironing_type", "no ironing".into()),
                        ("internal_solid_infill_speed", fmt(solid)),
                        ("top_surface_speed", fmt(top)),
                        ("seam_slope_type", "none".into()),
                        ("gap_fill_target", "nowhere".into()),
                        ("calib_flowrate_topinfill_special_order", "1".into()),
                        ("print_flow_ratio", fmt((cur + m) / cur)),
                    ]
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect()
                })
                .collect();
            Edit(&mut config)
                .set("layer_height", fmt(layer))
                .set("alternate_extra_wall", "0")
                .set("initial_layer_print_height", fmt(first))
                .set("reduce_crossing_wall", "1")
                .set("max_volumetric_extrusion_rate_slope", "0");
            return Ok(PreparedTest { test, config, parts, calib, reading: Reading::Pieces { pieces: readings }, unit: "" });
        }

        // ── Torre de PA: cambia cada milímetro ──
        CalibTest::PressureAdvance => {
            need(step > 0.0 && end > start && start >= 0.0, "el pressure advance va de menor a mayor, con un paso positivo")?;
            let parts: Vec<Vec<Tri>> = model("pa_tower")?.into_iter().map(|p| p.1).collect();
            let height = ((end - start) / step).ceil() + 1.0;
            thumb = Some((-1.0, height as f32 + 1.0));
            object.insert("@keep_below".into(), fmt(height));
            let line = num_rel(base, "line_width", nozzle).filter(|w| *w > 0.0).unwrap_or(nozzle * 1.125);
            let layer = num(base, "layer_height").unwrap_or(0.2);
            let max_vol = num(base, "filament_max_volumetric_speed").unwrap_or(15.0);
            let speed = num(base, "outer_wall_speed").unwrap_or(100.0).max(100.0).min(max_vol / mm3_per_mm(line, layer)).floor();
            for (k, v) in [
                ("alternate_extra_wall", "0".to_string()),
                ("outer_wall_speed", fmt(speed)),
                ("inner_wall_speed", fmt(speed)),
                ("seam_position", "back".into()),
                ("wall_loops", "2".into()),
                ("top_shell_layers", "0".into()),
                ("bottom_shell_layers", "0".into()),
                ("sparse_infill_density", "0%".into()),
                ("brim_type", "brim_ears".into()),
                ("brim_object_gap", "0".into()),
                ("brim_ears_max_angle", "135".into()),
                ("brim_width", "6".into()),
                ("seam_slope_type", "none".into()),
            ] {
                object.insert(k.into(), v);
            }
            Edit(&mut config).list("slow_down_layer_time", "1").set("max_volumetric_extrusion_rate_slope", "0").list("enable_pressure_advance", "1");
            calib = Calib { mode: "pa_tower".into(), start, end, step, ..calib };
            let n = height as usize;
            let blocks = (0..n).map(|i| Block { value: start + step * i as f64, z0: i as f64, z1: (i + 1) as f64 }).collect();
            (parts, Reading::Blocks { blocks }, "")
        }

        // ── Torre de retracción: cambia cada milímetro desde 0,4 mm ──
        CalibTest::Retraction => {
            need(step > 0.0 && end > start && start >= 0.0, "la retracción va de menor a mayor, con un paso positivo")?;
            let parts: Vec<Vec<Tri>> = model("retraction_tower")?.into_iter().map(|p| p.1).collect();
            let layer = if nozzle <= 0.1 { 0.05 } else if nozzle <= 0.2 { 0.1 } else { 0.2 };
            let height = 1.0 + 0.4 + (end - start) / step - 1e-3;
            thumb = Some((-1.0, height as f32 + 1.0));
            object.insert("@keep_below".into(), fmt(height));
            for (k, v) in [
                ("wall_loops", "2"),
                ("top_shell_layers", "0"),
                ("bottom_shell_layers", "3"),
                ("sparse_infill_density", "0%"),
                ("alternate_extra_wall", "0"),
                ("seam_position", "aligned"),
                ("wall_sequence", "inner wall/outer wall"),
                ("overhang_reverse", "0"),
            ] {
                object.insert(k.into(), v.into());
            }
            object.insert("layer_height".into(), fmt(layer));
            let mut e = Edit(&mut config);
            e.set("use_firmware_retraction", "0").set("initial_layer_print_height", fmt(layer));
            if num(base, "max_layer_height").is_some_and(|m| m < layer) {
                e.list("max_layer_height", fmt(layer));
            }
            calib = Calib { mode: "retraction_tower".into(), start, end, step, ..calib };
            let n = ((end - start) / step).round() as usize + 1;
            let blocks = (0..n).map(|i| Block { value: start + step * i as f64, z0: 0.4 + i as f64, z1: 1.4 + i as f64 }).collect();
            (parts, Reading::Blocks { blocks }, "mm")
        }

        // ── Velocidad volumétrica: espiral de una pared cada vez más rápida ──
        CalibTest::VolumetricSpeed => {
            need(step > 0.0 && end > start && start > 0.0, "la velocidad va de menor a mayor, con un paso positivo")?;
            let mut parts: Vec<Vec<Tri>> = model("volumetric_speed")?.into_iter().map(|p| p.1).collect();
            let (lo, hi) = bed;
            let (blo, bhi) = bbox(&parts);
            let fit = ((hi[0] - lo[0] - 10.0) / (bhi[0] - blo[0]) as f64).min(1.0) as f32;
            scale(&mut parts, [fit, 1.0, 1.0]);
            let line = nozzle * 1.75;
            let layer = nozzle * 0.8;
            let height = (end - start + 1.0) / step;
            thumb = Some((-1.0, height as f32 + 1.0));
            object.insert("@keep_below".into(), fmt(height));
            for (k, v) in [
                ("enable_overhang_speed", "0".to_string()),
                ("wall_loops", "1".into()),
                ("alternate_extra_wall", "0".into()),
                ("top_shell_layers", "0".into()),
                ("bottom_shell_layers", "0".into()),
                ("sparse_infill_density", "0%".into()),
                ("outer_wall_line_width", fmt(line)),
                ("layer_height", fmt(layer)),
                ("brim_type", "outer_and_inner".into()),
                ("brim_width", "5".into()),
                ("brim_object_gap", "0".into()),
            ] {
                object.insert(k.into(), v);
            }
            let mut e = Edit(&mut config);
            if num(base, "max_layer_height").is_some_and(|m| m < layer) {
                e.list("max_layer_height", fmt(layer));
            }
            e.list("filament_max_volumetric_speed", "200").list("slow_down_layer_time", "0").set("timelapse_type", "0").set("spiral_mode", "1").set("max_volumetric_extrusion_rate_slope", "0");
            let flow = num(base, "filament_flow_ratio").unwrap_or(1.0);
            let per_mm = mm3_per_mm(line, layer) * flow;
            calib = Calib { mode: "vol_speed_tower".into(), start: start / per_mm, end: end / per_mm, step: step / per_mm, ..calib };
            (parts, Reading::Height { height_mm: height, start, end: start + height * step }, "mm³/s")
        }

        // ── VFA: cada 5 mm, más rápido ──
        CalibTest::Vfa => {
            need(step > 0.0 && end > start && start > 0.0, "la velocidad va de menor a mayor, con un paso positivo")?;
            let parts: Vec<Vec<Tri>> = model("vfa")?.into_iter().map(|p| p.1).collect();
            let height = 5.0 * ((end - start) / step + 1.0);
            thumb = Some((-1.0, height as f32 + 1.0));
            object.insert("@keep_below".into(), fmt(height));
            for (k, v) in [("brim_type", "outer_only"), ("brim_width", "3"), ("brim_object_gap", "0")] {
                object.insert(k.into(), v.into());
            }
            Edit(&mut config)
                .list("slow_down_layer_time", "0")
                .set("enable_overhang_speed", "0")
                .set("timelapse_type", "0")
                .set("wall_loops", "1")
                .set("alternate_extra_wall", "0")
                .set("top_shell_layers", "0")
                .set("bottom_shell_layers", "1")
                .set("sparse_infill_density", "0%")
                .set("detect_thin_wall", "0")
                .set("spiral_mode", "1");
            calib = Calib { mode: "vfa_tower".into(), start, end, step, ..calib };
            let n = ((end - start) / step).round() as usize + 1;
            let blocks = (0..n).map(|i| Block { value: start + step * i as f64, z0: 5.0 * i as f64, z1: 5.0 * (i + 1) as f64 }).collect();
            (parts, Reading::Blocks { blocks }, "mm/s")
        }

        // ── Input shaping y esquinas: torre de una pared muy rápida ──
        CalibTest::ShaperFrequency | CalibTest::ShaperDamping | CalibTest::Cornering => {
            need(klipper || test == CalibTest::Cornering, "el input shaping se calibra solo en impresoras con Klipper")?;
            need(end > start, "el rango va de menor a mayor")?;
            let name = if test == CalibTest::Cornering { "cornering" } else { "ringing_tower" };
            let parts: Vec<Vec<Tri>> = model(name)?.into_iter().map(|p| p.1).collect();
            let (_, hi) = bbox(&parts);
            for (k, v) in [("brim_type", "outer_only"), ("brim_width", "3"), ("brim_object_gap", "0")] {
                object.insert(k.into(), v.into());
            }
            let accel = if test == CalibTest::Cornering { "2000" } else { "20000" };
            let mut e = Edit(&mut config);
            if test == CalibTest::Cornering {
                e.list("machine_max_jerk_x", fmt(end)).list("machine_max_jerk_y", fmt(end)).set("default_jerk", "0").list("filament_max_volumetric_speed", "200").set("input_shaping_emit", "1").set("input_shaping_type", "Disable");
            } else {
                let jerk = if klipper { 5.0 } else { 10.0 };
                let jx = num(base, "machine_max_jerk_x").unwrap_or(jerk).max(jerk);
                let jy = num(base, "machine_max_jerk_y").unwrap_or(jerk).max(jerk);
                e.list("machine_max_jerk_x", fmt(jx)).list("machine_max_jerk_y", fmt(jy)).set("default_jerk", "0").set("input_shaping_emit", "0").set("layer_height", "0.2");
            }
            if num(base, "enable_pressure_advance").unwrap_or(0.0) == 0.0 {
                e.list("enable_pressure_advance", "1").list("pressure_advance", "0").list("adaptive_pressure_advance", "0");
            }
            e.list("slow_down_layer_time", "0")
                .list("slow_down_min_speed", "0")
                .list("slow_down_for_layer_cooling", "0")
                .set("enable_overhang_speed", "0")
                .set("timelapse_type", "0")
                .set("wall_loops", "1")
                .set("top_shell_layers", "0")
                .set("bottom_shell_layers", "1")
                .set("sparse_infill_density", "0%")
                .set("detect_thin_wall", "0")
                .set("spiral_mode", "1")
                .set("spiral_mode_smooth", "0")
                .set("bottom_surface_pattern", "rectilinear")
                .set("outer_wall_speed", "200")
                .set("default_acceleration", accel)
                .set("outer_wall_acceleration", accel);
            // Orca: en la de frecuencia, `start` es la amortiguación y las frecuencias van aparte; en la de
            // amortiguación, `start`/`end` son las amortiguaciones y la frecuencia ya elegida va en `freq_start_*`
            calib = match test {
                CalibTest::ShaperFrequency => Calib { mode: "input_shaping_freq".into(), start: 0.1, end: 0.0, freq_start_x: start, freq_end_x: end, freq_start_y: start, freq_end_y: end, shaper_type: "mzv".into(), ..calib },
                CalibTest::ShaperDamping => {
                    let f = range.frequency.unwrap_or(40.0);
                    Calib { mode: "input_shaping_damp".into(), start, end, freq_start_x: f, freq_start_y: f, shaper_type: "mzv".into(), ..calib }
                }
                _ => Calib { mode: "cornering".into(), start, end, ..calib },
            };
            let unit = match test {
                CalibTest::ShaperFrequency => "Hz",
                CalibTest::ShaperDamping => "",
                _ => "mm/s",
            };
            (parts, Reading::Height { height_mm: hi[2] as f64, start, end }, unit)
        }
    };
    let mut parts = parts;
    if test != CalibTest::FirstLayer {
        center_on_bed(&mut parts, bed);
    }
    if let Some((z0, z1)) = thumb {
        // Ya apoyada: lo que queda entre z0 y z1, bajado a la cama
        let mut t = parts.clone();
        trim(&mut t, z0, z1);
        for p in t.iter_mut().flatten().flatten() {
            p[2] = (p[2] - z0.max(0.0)).max(0.0);
        }
        calib.thumbnail_parts = Some(t);
    }
    if !object.is_empty() {
        calib.objects = vec![object; parts.len()];
    }
    // Las cajas de la primera capa no llevan modo; las demás sí
    if test == CalibTest::FirstLayer && calib.objects.is_empty() {
        calib.mode = "none".into();
    }
    Ok(PreparedTest { test, config, parts, calib, reading, unit })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::index;
    use crate::settings::{build_config, SliceSettings};

    fn k1() -> Map<String, Value> {
        let k1 = index().printers.iter().find(|p| p.name == "Creality K1 (0.4 nozzle)").unwrap();
        build_config(&SliceSettings { printer: k1.name.clone(), process: k1.default_process.clone(), filament: k1.default_filament.clone(), overrides: Default::default() }).unwrap()
    }

    #[test]
    fn los_modelos_se_leen() {
        for name in ["temperature_tower", "pa_tower", "retraction_tower", "volumetric_speed", "vfa", "ringing_tower", "cornering", "flow_coarse", "flow_fine"] {
            let m = model(name).unwrap();
            assert!(!m.is_empty() && m.iter().all(|p| !p.1.is_empty()), "{name}");
        }
        assert_eq!(model("flow_coarse").unwrap().len(), 11);
        assert_eq!(model("flow_fine").unwrap().len(), 16);
    }

    #[test]
    fn torre_de_temperatura_cortada_al_rango() {
        let t = prepare(CalibTest::Temperature, &CalibRange { start: Some(230.0), end: Some(200.0), ..Default::default() }, &k1()).unwrap();
        let Reading::Blocks { blocks } = &t.reading else { panic!() };
        assert_eq!(blocks.len(), 7);
        assert_eq!((blocks[0].value, blocks[6].value), (230.0, 200.0));
        assert_eq!(t.calib.mode, "temp_tower");
        // Corte: de (500−230)/5·10 = 540 a ((500−200)/5+1)·10 = 610 mm del modelo
        let o = &t.calib.objects[0];
        assert!((o["@keep_above"].parse::<f64>().unwrap() - 540.0).abs() < 0.01 && (o["@keep_below"].parse::<f64>().unwrap() - 610.0).abs() < 0.01, "{o:?}");
        assert_eq!(t.config["nozzle_temperature"], Value::Array(vec![Value::String("230".into())]));
        // Centrada en la cama de 220 × 220
        let (lo, hi) = bbox(&t.parts);
        assert!(((lo[0] + hi[0]) * 0.5 - 110.0).abs() < 0.5 && lo[2].abs() < 1e-3);
    }

    #[test]
    fn flujo_una_pieza_por_valor() {
        let base = k1();
        let cur = num(&base, "filament_flow_ratio").unwrap();
        let t = prepare(CalibTest::FlowCoarse, &CalibRange::default(), &base).unwrap();
        let Reading::Pieces { pieces } = &t.reading else { panic!() };
        assert_eq!(pieces.len(), 11);
        assert_eq!(t.calib.objects.len(), 11);
        let plus = pieces.iter().position(|p| p.mark == "+5").unwrap();
        assert!((pieces[plus].value - (cur + 0.05)).abs() < 1e-9);
        assert!((t.calib.objects[plus]["print_flow_ratio"].parse::<f64>().unwrap() - (cur + 0.05) / cur).abs() < 1e-5);
        let fine = prepare(CalibTest::FlowFine, &CalibRange::default(), &base).unwrap();
        let Reading::Pieces { pieces } = &fine.reading else { panic!() };
        assert!(pieces.iter().any(|p| p.mark == "-0.5"));
    }

    #[test]
    fn las_demas_pruebas_se_arman() {
        let base = k1();
        for test in CalibTest::ALL {
            let t = prepare(test, &CalibRange::default(), &base).unwrap_or_else(|e| panic!("{test:?}: {e}"));
            assert!(!t.parts.is_empty() && t.parts.iter().all(|p| !p.is_empty()), "{test:?}");
            let (lo, hi) = bbox(&t.parts);
            assert!(lo[0] >= 0.0 && lo[1] >= 0.0 && hi[0] <= 220.0 && hi[1] <= 220.0, "{test:?} fuera de la cama: {lo:?} {hi:?}");
        }
        let pa = prepare(CalibTest::PressureAdvance, &CalibRange::default(), &base).unwrap();
        let Reading::Blocks { blocks } = &pa.reading else { panic!() };
        assert_eq!(blocks.len(), 51);
        assert!(prepare(CalibTest::Temperature, &CalibRange { start: Some(200.0), end: Some(198.0), ..Default::default() }, &base).is_err());
    }
}
