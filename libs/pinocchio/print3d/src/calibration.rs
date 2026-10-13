//! Modelos de calibración imprimibles y cuentas para sacar las holguras de la impresora
//!
//! Dos piezas, sin soportes y con todos sus huecos verticales:
//!
//! - **Prueba de holgura**: una placa con agujeros de un mismo diámetro nominal pero con holguras
//!   crecientes (cada uno numerado en relieve) y clavijas del diámetro nominal. Se prueba la clavija
//!   en cada agujero y se anota en cuál entra justa.
//! - **Bloque de calibración**: un prisma de medidas conocidas (más largo en X, luego Y, más bajo en Z)
//!   con agujeros pasantes de diámetros conocidos, de menor a mayor en X. Al medirlo salen la
//!   contracción por eje y cuánto más chicos salen los agujeros.

use pinocchio_math::{Real, Vector3};
use pinocchio_mesh::Mesh;
use serde::{Deserialize, Serialize};

use crate::capping::cap_triangles;
use crate::error::{Print3dError, Result};
use crate::planar_joints::weld_build;

type P2 = [Real; 2];

// ═══════════════════════════════════════════════════════════════════════════
// GEOMETRÍA BÁSICA
// ═══════════════════════════════════════════════════════════════════════════

fn circle(center: P2, diameter: Real, segments: usize) -> Vec<P2> {
    (0..segments)
        .map(|i| {
            let a = std::f64::consts::TAU * i as Real / segments as Real;
            [center[0] + diameter * 0.5 * a.cos(), center[1] + diameter * 0.5 * a.sin()]
        })
        .collect()
}

fn rect(x0: Real, y0: Real, x1: Real, y1: Real) -> Vec<P2> {
    vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
}

/// Prisma cerrado entre `z0` y `z1` con la región `outer` (antihoraria) menos los `holes`
fn extrude(outer: &[P2], holes: &[Vec<P2>], z0: Real, z1: Real) -> Result<Mesh> {
    let at = |p: &P2, z: Real| Vector3::new(p[0], p[1], z);
    let tol = 1e-9;
    let mut pos: Vec<Vector3> = Vec::new();
    let mut tris: Vec<[usize; 3]> = Vec::new();
    let face = |z: Real, normal: Real, pos: &mut Vec<Vector3>, tris: &mut Vec<[usize; 3]>| {
        let mut segs: Vec<(Vector3, Vector3)> = Vec::new();
        for ring in std::iter::once(outer).chain(holes.iter().map(|h| h.as_slice())) {
            for i in 0..ring.len() {
                segs.push((at(&ring[i], z), at(&ring[(i + 1) % ring.len()], z)));
            }
        }
        for t in cap_triangles(&segs, Vector3::new(0.0, 0.0, normal), at(&outer[0], z), tol) {
            let base = pos.len();
            pos.extend(t);
            tris.push([base, base + 1, base + 2]);
        }
    };
    face(z1, 1.0, &mut pos, &mut tris);
    face(z0, -1.0, &mut pos, &mut tris);
    // Paredes: normal (q − p) × dir; el exterior sube, los agujeros bajan desde la cara de arriba
    let mut walls = |ring: &[P2], from: Real, to: Real| {
        let dir = Vector3::new(0.0, 0.0, to - from);
        let base = pos.len();
        let m = ring.len();
        pos.extend(ring.iter().map(|p| at(p, from)));
        pos.extend(ring.iter().map(|p| at(p, from) + dir));
        for i in 0..m {
            let j = (i + 1) % m;
            tris.push([base + i, base + j, base + m + j]);
            tris.push([base + i, base + m + j, base + m + i]);
        }
    };
    walls(outer, z0, z1);
    for h in holes {
        // Los agujeros se dan antihorarios: la pared mira al eje bajando desde arriba
        walls(h, z1, z0);
    }
    weld_build(&pos, &tris)
}

/// Digito de siete segmentos en relieve: (caja x0, y0, x1, y1) por segmento encendido
fn digit_boxes(d: u8, x: Real, y: Real, w: Real, h: Real, t: Real) -> Vec<[Real; 4]> {
    // a b c d e f g
    const SEGMENTS: [[bool; 7]; 10] = [
        [true, true, true, true, true, true, false],
        [false, true, true, false, false, false, false],
        [true, true, false, true, true, false, true],
        [true, true, true, true, false, false, true],
        [false, true, true, false, false, true, true],
        [true, false, true, true, false, true, true],
        [true, false, true, true, true, true, true],
        [true, true, true, false, false, false, false],
        [true, true, true, true, true, true, true],
        [true, true, true, true, false, true, true],
    ];
    let on = SEGMENTS[(d % 10) as usize];
    let mid = y + h * 0.5;
    let boxes = [
        [x, y + h - t, x + w, y + h],        // a: arriba
        [x + w - t, mid - t * 0.5, x + w, y + h], // b: arriba derecha
        [x + w - t, y, x + w, mid + t * 0.5], // c: abajo derecha
        [x, y, x + w, y + t],                // d: abajo
        [x, y, x + t, mid + t * 0.5],        // e: abajo izquierda
        [x, mid - t * 0.5, x + t, y + h],    // f: arriba izquierda
        [x, mid - t * 0.5, x + w, mid + t * 0.5], // g: medio
    ];
    (0..7).filter(|&i| on[i]).map(|i| boxes[i]).collect()
}

// ═══════════════════════════════════════════════════════════════════════════
// PRUEBA DE HOLGURA
// ═══════════════════════════════════════════════════════════════════════════

/// Parámetros de la prueba de holgura
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClearanceTest {
    /// Diámetro nominal de la clavija y de los agujeros (mm)
    pub diameter: Real,
    /// Holgura en el diámetro de cada agujero (mm): el agujero mide `diameter + holgura`
    pub clearances: Vec<Real>,
}

impl Default for ClearanceTest {
    fn default() -> Self {
        Self { diameter: 8.0, clearances: vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6] }
    }
}

impl ClearanceTest {
    /// Holguras desde `start` con paso `step`, `count` agujeros
    pub fn range(diameter: Real, start: Real, step: Real, count: usize) -> Self {
        Self { diameter, clearances: (0..count).map(|i| ((start + step * i as Real) * 1000.0).round() / 1000.0).collect() }
    }
}

/// Las piezas de la prueba: la placa numerada con los agujeros y dos clavijas, ya separadas
pub fn clearance_test(cfg: &ClearanceTest) -> Result<Vec<(String, Mesh)>> {
    let n = cfg.clearances.len();
    if !(1..=9).contains(&n) {
        return Err(Print3dError::InvalidConfig("la prueba lleva entre 1 y 9 agujeros".into()));
    }
    if !(cfg.diameter >= 2.0 && cfg.clearances.iter().all(|c| (0.0..=2.0).contains(c))) {
        return Err(Print3dError::InvalidConfig("diámetro desde 2 mm y holguras entre 0 y 2 mm".into()));
    }
    let d = cfg.diameter;
    let pitch = d + 8.0;
    let width = 28.0;
    let thick = 4.0;
    let plate_outline = rect(0.0, 0.0, pitch * n as Real, width);
    let y_hole = 9.0;
    let holes: Vec<Vec<P2>> = cfg.clearances.iter().enumerate().map(|(i, c)| circle([pitch * (i as Real + 0.5), y_hole], d + c, 64)).collect();
    let plate = extrude(&plate_outline, &holes, 0.0, thick)?;
    let mut parts = vec![("placa".to_string(), plate)];
    // Números en relieve sobre cada agujero (cajas que se apoyan en la placa)
    let (dw, dh, dt, relief) = (4.0, 7.0, 1.0, 0.8);
    for i in 0..n {
        let digit = ((i + 1) % 10) as u8;
        let x = pitch * (i as Real + 0.5) - dw * 0.5;
        for (k, b) in digit_boxes(digit, x, 18.0, dw, dh, dt).into_iter().enumerate() {
            parts.push((format!("numero_{}_{k}", i + 1), extrude(&rect(b[0], b[1], b[2], b[3]), &[], thick - 0.2, thick + relief)?));
        }
    }
    // Clavijas: base ancha que apoya, y el cilindro del diámetro nominal encima
    for k in 0..2 {
        let cx = (d + 14.0) * (k as Real + 0.5);
        let cy = -(d + 12.0);
        let base = extrude(&circle([cx, cy], d + 8.0, 64), &[], 0.0, 2.0)?;
        let peg = extrude(&circle([cx, cy], d, 64), &[], 1.0, 14.0)?;
        parts.push((format!("base_{}", k + 1), base));
        parts.push((format!("clavija_{}", k + 1), peg));
    }
    Ok(parts)
}

// ═══════════════════════════════════════════════════════════════════════════
// BLOQUE DE CALIBRACIÓN
// ═══════════════════════════════════════════════════════════════════════════

/// Parámetros del bloque
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationBlock {
    /// Alto (Z) en mm; el largo (X) y el fondo (Y) salen de los agujeros
    pub height: Real,
    /// Diámetros nominales de los agujeros pasantes, de menor a mayor (a lo largo de X)
    pub holes: Vec<Real>,
    /// Fondo (Y) en mm
    pub depth: Real,
}

impl Default for CalibrationBlock {
    fn default() -> Self {
        Self { height: 20.0, holes: vec![4.0, 6.0, 8.0, 10.0], depth: 30.0 }
    }
}

impl CalibrationBlock {
    /// Posiciones X de los centros y el largo total del bloque
    pub fn layout(&self) -> (Vec<Real>, Real) {
        let margin = 5.0;
        let mut x = margin;
        let mut centers = Vec::new();
        for (i, d) in self.holes.iter().enumerate() {
            if i > 0 {
                x += margin;
            }
            centers.push(x + d * 0.5);
            x += d;
        }
        (centers, x + margin)
    }

    /// Medidas nominales del bloque (X, Y, Z)
    pub fn size(&self) -> [Real; 3] {
        [self.layout().1, self.depth, self.height]
    }
}

/// El bloque: largo en X, luego fondo en Y, y más bajo en Z (así los tres ejes no se confunden)
pub fn calibration_block(cfg: &CalibrationBlock) -> Result<Mesh> {
    if cfg.holes.is_empty() || cfg.holes.len() > 6 || cfg.holes.windows(2).any(|w| w[1] < w[0]) {
        return Err(Print3dError::InvalidConfig("entre 1 y 6 agujeros, de menor a mayor".into()));
    }
    if !(cfg.holes.iter().all(|d| (1.5..=cfg.depth - 8.0).contains(d)) && cfg.height >= 10.0 && cfg.depth >= 20.0) {
        return Err(Print3dError::InvalidConfig("agujeros desde 1,5 mm con 4 mm de pared, alto desde 10 y fondo desde 20".into()));
    }
    let [sx, sy, sz] = cfg.size();
    if !(sx > sy && sy > sz) {
        return Err(Print3dError::InvalidConfig(format!("el bloque tiene que ser más largo en X que en Y y más alto en Y que en Z ({sx:.0} × {sy:.0} × {sz:.0})")));
    }
    let (centers, _) = cfg.layout();
    let holes: Vec<Vec<P2>> = centers.iter().zip(&cfg.holes).map(|(x, d)| circle([*x, sy * 0.5], *d, 64)).collect();
    extrude(&rect(0.0, 0.0, sx, sy), &holes, 0.0, sz)
}

// ═══════════════════════════════════════════════════════════════════════════
// CUENTAS
// ═══════════════════════════════════════════════════════════════════════════

/// Contracción por eje a partir de lo medido en el bloque impreso sin compensar
///
/// Si el bloque mide `medido` donde debía medir `nominal`, el material encogió `1 − medido/nominal`.
pub fn solve_shrink(nominal: [Real; 3], measured: [Real; 3]) -> Result<[Real; 3]> {
    let mut out = [0.0; 3];
    for i in 0..3 {
        if !(nominal[i] > 0.0 && measured[i] > 0.0) {
            return Err(Print3dError::InvalidConfig("las medidas tienen que ser positivas".into()));
        }
        let s = 1.0 - measured[i] / nominal[i];
        if !(-0.02..=0.15).contains(&s) {
            return Err(Print3dError::InvalidConfig(format!("la medida {:.2} mm se aleja demasiado de los {:.2} mm del modelo: revisa la medición", measured[i], nominal[i])));
        }
        out[i] = s;
    }
    Ok(out)
}

/// Cuánto más chico sale un agujero, por lado (mm): promedio de `(nominal − medido) / 2`
pub fn solve_hole_comp(holes: &[(Real, Real)]) -> Result<Real> {
    if holes.is_empty() {
        return Err(Print3dError::InvalidConfig("falta medir al menos un agujero".into()));
    }
    for (nominal, measured) in holes {
        if !(*nominal > 0.0 && *measured > 0.0 && (nominal - measured).abs() < nominal * 0.5) {
            return Err(Print3dError::InvalidConfig(format!("el agujero de {nominal:.1} mm medido en {measured:.2} mm no cuadra: revisa la medición")));
        }
    }
    Ok(holes.iter().map(|(n, m)| (n - m) / 2.0).sum::<Real>() / holes.len() as Real)
}

/// Lo que se midió en las dos piezas (cualquiera puede faltar)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CalibrationMeasures {
    /// Holgura (en el diámetro) del agujero donde la clavija entra justa
    #[serde(default)]
    pub fit_clearance: Option<Real>,
    /// Bloque medido (X, Y, Z) en mm
    #[serde(default)]
    pub block_size: Option<[Real; 3]>,
    /// Diámetros medidos de los agujeros del bloque, en el mismo orden
    #[serde(default)]
    pub block_holes: Vec<Real>,
}

/// Resultado de la calibración
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CalibrationResult {
    /// Holgura de un ajuste normal (mm, en el diámetro)
    pub fit_clearance: Option<Real>,
    pub shrink: Option<[Real; 3]>,
    pub hole_comp: Option<Real>,
}

/// Saca las holguras de lo medido, contra el bloque `block` que se imprimió
pub fn solve_calibration(block: &CalibrationBlock, m: &CalibrationMeasures) -> Result<CalibrationResult> {
    let mut out = CalibrationResult { fit_clearance: m.fit_clearance, ..Default::default() };
    if let Some(c) = m.fit_clearance
        && !(0.0..=2.0).contains(&c)
    {
        return Err(Print3dError::InvalidConfig("la holgura va entre 0 y 2 mm".into()));
    }
    if let Some(size) = m.block_size {
        out.shrink = Some(solve_shrink(block.size(), size)?);
    }
    if !m.block_holes.is_empty() {
        if m.block_holes.len() > block.holes.len() {
            return Err(Print3dError::InvalidConfig("se midieron más agujeros de los que tiene el bloque".into()));
        }
        let pairs: Vec<(Real, Real)> = block.holes.iter().copied().zip(m.block_holes.iter().copied()).collect();
        out.hole_comp = Some(solve_hole_comp(&pairs)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{compute_bounding_box, compute_volume};

    #[test]
    fn la_placa_tiene_los_agujeros_y_numeros() {
        let parts = clearance_test(&ClearanceTest::default()).unwrap();
        let plate = &parts[0].1;
        assert!(plate.is_closed());
        let poly = |d: Real| 0.5 * 64.0 * (d / 2.0).powi(2) * (std::f64::consts::TAU / 64.0).sin();
        let n = 6.0;
        let full = 16.0 * n * 28.0 * 4.0;
        let holes: Real = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6].iter().map(|c| poly(8.0 + c) * 4.0).sum();
        assert!((compute_volume(plate) - (full - holes)).abs() < 1e-6, "{} vs {}", compute_volume(plate), full - holes);
        // Seis números, cada uno de varias cajas cerradas; clavijas con su base
        assert!(parts.iter().filter(|(n, _)| n.starts_with("numero_")).count() >= 6 * 2);
        assert!(parts.iter().all(|(_, m)| m.is_closed()));
        let peg = parts.iter().find(|(n, _)| n == "clavija_1").unwrap();
        // Del diámetro nominal exacto en los vértices
        let d = compute_bounding_box(&peg.1).dimensions();
        assert!((d[0] - 8.0).abs() < 1e-6 && (d[1] - 8.0).abs() < 1e-6 && (d[2] - 13.0).abs() < 1e-9);
    }

    #[test]
    fn rangos_y_limites_de_la_prueba() {
        let t = ClearanceTest::range(10.0, 0.05, 0.05, 4);
        assert_eq!(t.clearances, vec![0.05, 0.1, 0.15, 0.2]);
        assert!(clearance_test(&ClearanceTest { diameter: 8.0, clearances: vec![] }).is_err());
        assert!(clearance_test(&ClearanceTest { diameter: 1.0, clearances: vec![0.2] }).is_err());
        assert!(clearance_test(&ClearanceTest { diameter: 8.0, clearances: vec![0.2; 10] }).is_err());
    }

    #[test]
    fn el_bloque_mide_lo_que_dice() {
        let cfg = CalibrationBlock::default();
        let [sx, sy, sz] = cfg.size();
        assert!(sx > sy && sy > sz, "{sx} {sy} {sz}");
        assert!((sx - 53.0).abs() < 1e-9, "{sx}");
        let block = calibration_block(&cfg).unwrap();
        assert!(block.is_closed());
        let d = compute_bounding_box(&block).dimensions();
        assert!((d[0] - sx).abs() < 1e-9 && (d[1] - sy).abs() < 1e-9 && (d[2] - sz).abs() < 1e-9);
        let poly = |d: Real| 0.5 * 64.0 * (d / 2.0).powi(2) * (std::f64::consts::TAU / 64.0).sin();
        let expected = sx * sy * sz - [4.0, 6.0, 8.0, 10.0].iter().map(|d| poly(*d) * sz).sum::<Real>();
        assert!((compute_volume(&block) - expected).abs() < 1e-6);
    }

    #[test]
    fn bloques_mal_pensados_se_rechazan() {
        assert!(calibration_block(&CalibrationBlock { holes: vec![8.0, 4.0], ..Default::default() }).is_err());
        assert!(calibration_block(&CalibrationBlock { holes: vec![], ..Default::default() }).is_err());
        // Si el alto iguala al fondo los ejes se confunden
        assert!(calibration_block(&CalibrationBlock { height: 30.0, ..Default::default() }).is_err());
        assert!(calibration_block(&CalibrationBlock { holes: vec![25.0], ..Default::default() }).is_err());
    }

    #[test]
    fn la_contraccion_sale_de_lo_medido() {
        let block = CalibrationBlock::default();
        let [sx, sy, sz] = block.size();
        // Un material que encoge 0,5 % en X, 0,4 % en Y y 0,8 % en Z
        let m = CalibrationMeasures { block_size: Some([sx * 0.995, sy * 0.996, sz * 0.992]), ..Default::default() };
        let r = solve_calibration(&block, &m).unwrap();
        let s = r.shrink.unwrap();
        assert!((s[0] - 0.005).abs() < 1e-12 && (s[1] - 0.004).abs() < 1e-12 && (s[2] - 0.008).abs() < 1e-12);
        // Una medida absurda se rechaza
        assert!(solve_calibration(&block, &CalibrationMeasures { block_size: Some([sx * 0.5, sy, sz]), ..Default::default() }).is_err());
    }

    #[test]
    fn los_agujeros_salen_mas_chicos() {
        let block = CalibrationBlock::default();
        // Cada agujero salió 0,2 mm más chico (0,1 por lado), con algo de ruido
        let m = CalibrationMeasures { block_holes: vec![3.79, 5.81, 7.8, 9.8], ..Default::default() };
        let r = solve_calibration(&block, &m).unwrap();
        assert!((r.hole_comp.unwrap() - 0.1).abs() < 0.01, "{:?}", r.hole_comp);
        assert!(solve_calibration(&block, &CalibrationMeasures { block_holes: vec![4.0; 5], ..Default::default() }).is_err());
        assert!(solve_hole_comp(&[]).is_err());
        assert!(solve_hole_comp(&[(8.0, 3.0)]).is_err());
    }

    #[test]
    fn la_holgura_elegida_pasa_tal_cual() {
        let r = solve_calibration(&CalibrationBlock::default(), &CalibrationMeasures { fit_clearance: Some(0.3), ..Default::default() }).unwrap();
        assert_eq!(r, CalibrationResult { fit_clearance: Some(0.3), shrink: None, hole_comp: None });
        assert!(solve_calibration(&CalibrationBlock::default(), &CalibrationMeasures { fit_clearance: Some(5.0), ..Default::default() }).is_err());
    }

    #[test]
    fn la_placa_cabe_en_una_cama_chica() {
        let parts = clearance_test(&ClearanceTest::default()).unwrap();
        let (mut lo, mut hi) = ([Real::INFINITY; 2], [Real::NEG_INFINITY; 2]);
        for (_, m) in &parts {
            let bb = compute_bounding_box(m);
            for k in 0..2 {
                lo[k] = lo[k].min(bb.min[k]);
                hi[k] = hi[k].max(bb.max[k]);
            }
        }
        // Toda la prueba junta en menos de 120 × 70 mm
        assert!(hi[0] - lo[0] < 120.0 && hi[1] - lo[1] < 70.0, "{:?} {:?}", lo, hi);
    }
}
