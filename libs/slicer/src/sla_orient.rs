//! Orientación automática para resina (SLA13 de `PLAN_SLA.md`): busca el giro de la pieza
//! que menos tira al despegar, menos succiona, menos soportes pide y menos alto queda.
//!
//! Cómo busca, de lo barato a lo caro:
//! 1. **Esfera**: ~260 direcciones repartidas parejo (espiral de Fibonacci); para cada una
//!    se cuenta, sin laminar, el área en voladizo que pide soportes (proyectada: lo plano
//!    pesa más) y la altura. Se quedan las mejores, separadas entre sí al menos 15°.
//! 2. **Laminado grueso** (pantalla de 0,3 mm) de esas: área de la capa más grande,
//!    succión de las ventosas e islas, con la revisión de siempre (`sla_check`).
//! 3. **Afinar**: alrededor de las tres mejores, giros de ±6° y después ±3°.
//!
//! Un giro es [`Turn`]: primero alrededor de X, después de Y y al final de Z (este último
//! solo para que entre en la pantalla), siempre por el centro de la caja de la pieza.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::sla::{slice, ResinPrinter};
use crate::sla_check::check;
use crate::sla_supports::Tri;

/// Giro de la pieza en grados: X, después Y, después Z, por el centro de su caja.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Turn {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Turn {
    pub fn is_zero(&self) -> bool {
        self.x == 0.0 && self.y == 0.0 && self.z == 0.0
    }

    /// El giro (sin Z) que deja hacia arriba la dirección `up` de la pieza.
    pub fn facing_up(up: [f32; 3]) -> Turn {
        let r = (up[1] * up[1] + up[2] * up[2]).sqrt();
        Turn { x: up[1].atan2(up[2]).to_degrees(), y: (-up[0]).atan2(r).to_degrees(), z: 0.0 }
    }

    /// La dirección de la pieza que queda hacia arriba con este giro.
    pub fn up(&self) -> [f32; 3] {
        // Inversa de X·Y aplicada a +Z: primero Y al revés y después X al revés.
        let (sy, cy) = self.y.to_radians().sin_cos();
        let (sx, cx) = self.x.to_radians().sin_cos();
        let v = [-sy, 0.0, cy];
        [v[0], v[1] * cx + v[2] * sx, -v[1] * sx + v[2] * cx]
    }
}

/// Gira los triángulos por el centro de su caja.
pub fn turn(tris: &[Tri], t: Turn) -> Vec<Tri> {
    let (min, max) = bounds(tris);
    let ctr: [f32; 3] = std::array::from_fn(|c| (min[c] + max[c]) / 2.0);
    let rot = |deg: f32, (a, b): (usize, usize)| {
        let (s, co) = deg.to_radians().sin_cos();
        move |v: [f32; 3]| {
            let mut w = v;
            let (p, q) = (v[a] - ctr[a], v[b] - ctr[b]);
            w[a] = ctr[a] + p * co - q * s;
            w[b] = ctr[b] + p * s + q * co;
            w
        }
    };
    let (rx, ry, rz) = (rot(t.x, (1, 2)), rot(t.y, (2, 0)), rot(t.z, (0, 1)));
    tris.iter().map(|tri| tri.map(|v| rz(ry(rx(v))))).collect()
}

/// Cómo queda la pieza con un giro.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Orientation {
    pub turn: Turn,
    pub fits: bool,
    pub height_mm: f32,
    /// Área de la capa más grande (fuerza al despegar), mm².
    pub max_area_mm2: f32,
    /// Suma de la succión más fuerte de cada ventosa que no toca la placa, mm².
    pub suction_mm2: f32,
    pub islands: usize,
    /// Área en voladizo que pide soportes, proyectada en la placa (mm²): marcas y cantidad de soportes.
    pub overhang_mm2: f32,
    /// Menor es mejor (ver [`auto_orient`]).
    pub score: f32,
}

/// Busca los mejores giros de `model` para `printer`. Sale ordenado de mejor a peor, con
/// hasta `keep` giros separados al menos 20° y siempre el giro 0 (para comparar).
///
/// Puntaje (todo relativo a la pieza sin girar, salvo la altura): área máxima, más 2 ×
/// succión, más voladizo, más 0,25 × altura / diagonal, más 0,01 por isla (tope 0,3); +1000
/// si no entra.
///
/// `overhang_angle` es el «Ángulo límite» de los soportes (45° de fábrica).
pub fn auto_orient(model: &[Tri], printer: &ResinPrinter, layer_height: f32, overhang_angle: f32, keep: usize) -> Vec<Orientation> {
    let cos = overhang_angle.clamp(1.0, 89.0).to_radians().cos();
    let normals: Vec<[f32; 3]> = model.iter().map(area_normal).collect();
    let diag = diagonal(model).max(1.0);

    // 1. Esfera, sin laminar.
    let face = footprint(&normals).max(1.0);
    let mut rough: Vec<([f32; 3], f32)> = fibonacci(260)
        .into_par_iter()
        .map(|u| (u, overhang(&normals, u, cos) / face + 0.25 * extent(model, u) / diag))
        .collect();
    rough.sort_by(|a, b| a.1.total_cmp(&b.1));
    let mut picked: Vec<[f32; 3]> = Vec::new();
    for (u, _) in &rough {
        if picked.len() >= 20 {
            break;
        }
        if picked.iter().all(|p| angle(p, u) >= 15.0) {
            picked.push(*u);
        }
    }

    // 2. Laminado grueso.
    let coarse = printer.coarse(0.3);
    let h = layer_height.max(0.1);
    let eval = |t: Turn| measure(model, &normals, printer, &coarse, h, cos, t);
    let mut cases: Vec<Turn> = vec![Turn::default()];
    cases.extend(picked.iter().map(|&u| Turn::facing_up(u)));
    let mut all: Vec<Orientation> = cases.par_iter().map(|&t| eval(t)).collect();
    let base = all[0].clone();
    let score = |o: &mut Orientation| o.score = score_of(o, &base, diag);
    all.iter_mut().for_each(score);

    // 3. Afinar alrededor de las tres mejores.
    for step in [6.0f32, 3.0] {
        all.sort_by(|a, b| a.score.total_cmp(&b.score));
        let near: Vec<Turn> = all
            .iter()
            .take(3)
            .flat_map(|o| {
                let t = o.turn;
                [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0), (-1.0, -1.0), (1.0, 1.0), (-1.0, 1.0), (1.0, -1.0)]
                    .map(|(dx, dy)| Turn { x: t.x + dx * step, y: (t.y + dy * step).clamp(-90.0, 90.0), z: t.z })
            })
            .filter(|t| all.iter().all(|o| o.turn != *t))
            .collect();
        let mut more: Vec<Orientation> = near.par_iter().map(|&t| eval(t)).collect();
        more.iter_mut().for_each(score);
        all.extend(more);
    }

    // Si no entra, probar girando en Z.
    for o in all.iter_mut().filter(|o| !o.fits) {
        for z in [90.0, 45.0, 135.0] {
            let t = Turn { z, ..o.turn };
            if fits(&place(&turn(model, t), printer, 5.0), printer) {
                o.turn = t;
                o.fits = true;
                o.score -= 1000.0;
                break;
            }
        }
    }

    all.sort_by(|a, b| a.score.total_cmp(&b.score));
    let mut out: Vec<Orientation> = Vec::new();
    for o in all.iter() {
        if out.len() >= keep.max(1) {
            break;
        }
        if out.iter().all(|p| angle(&p.turn.up(), &o.turn.up()) >= 20.0) {
            out.push(o.clone());
        }
    }
    if !out.iter().any(|o| o.turn.x == 0.0 && o.turn.y == 0.0)
        && let Some(o) = all.iter().find(|o| o.turn.x == 0.0 && o.turn.y == 0.0)
    {
        out.push(o.clone());
    }
    out
}

fn score_of(o: &Orientation, base: &Orientation, diag: f32) -> f32 {
    let ref_area = base.max_area_mm2.max(1.0);
    let mut s = o.max_area_mm2 / ref_area
        + 2.0 * o.suction_mm2 / ref_area
        + o.overhang_mm2 / ref_area
        + 0.25 * o.height_mm / diag
        + (0.01 * o.islands as f32).min(0.3);
    if !o.fits {
        s += 1000.0;
    }
    s
}

/// Lamina en grueso la pieza girada y mide todo.
fn measure(model: &[Tri], normals: &[[f32; 3]], printer: &ResinPrinter, coarse: &ResinPrinter, h: f32, cos: f32, t: Turn) -> Orientation {
    let tris = place(&turn(model, t), printer, 5.0);
    let height = tris.iter().flatten().map(|v| v[2]).fold(0f32, f32::max) - 5.0;
    let (max_area, suction, islands) = match slice(&tris, coarse, h) {
        Ok(layers) => {
            let r = check(&layers, coarse, h);
            let suction = r.cavities.iter().filter(|c| !c.on_plate).map(|c| c.max_suction_mm2).sum();
            (r.max_area_mm2, suction, r.islands.len())
        }
        Err(_) => (f32::MAX, f32::MAX, 0),
    };
    Orientation {
        turn: t,
        fits: fits(&tris, printer),
        height_mm: height,
        max_area_mm2: max_area,
        suction_mm2: suction,
        islands,
        overhang_mm2: overhang(normals, t.up(), cos),
        score: 0.0,
    }
}

/// Normal por el área (la mitad del producto cruz).
fn area_normal(t: &Tri) -> [f32; 3] {
    let a = [t[1][0] - t[0][0], t[1][1] - t[0][1], t[1][2] - t[0][2]];
    let b = [t[2][0] - t[0][0], t[2][1] - t[0][1], t[2][2] - t[0][2]];
    [(a[1] * b[2] - a[2] * b[1]) / 2.0, (a[2] * b[0] - a[0] * b[2]) / 2.0, (a[0] * b[1] - a[1] * b[0]) / 2.0]
}

/// Área en voladizo proyectada en la placa con `up` hacia arriba.
fn overhang(normals: &[[f32; 3]], up: [f32; 3], cos: f32) -> f32 {
    normals
        .iter()
        .map(|n| {
            let down = -(n[0] * up[0] + n[1] * up[1] + n[2] * up[2]);
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if len > 0.0 && down >= cos * len { down } else { 0.0 }
        })
        .sum()
}

/// Escala de áreas que no depende del giro: un sexto de la superficie (una cara de un cubo).
fn footprint(normals: &[[f32; 3]]) -> f32 {
    normals.iter().map(|n| (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt()).sum::<f32>() / 6.0
}

/// Largo de la pieza a lo largo de `u`.
fn extent(model: &[Tri], u: [f32; 3]) -> f32 {
    let (lo, hi) = model.iter().flatten().fold((f32::MAX, f32::MIN), |(lo, hi), v| {
        let d = v[0] * u[0] + v[1] * u[1] + v[2] * u[2];
        (lo.min(d), hi.max(d))
    });
    (hi - lo).max(0.0)
}

/// `n` direcciones repartidas parejo en la esfera.
fn fibonacci(n: usize) -> Vec<[f32; 3]> {
    let golden = std::f32::consts::PI * (3.0 - 5f32.sqrt());
    (0..n)
        .map(|i| {
            let z = 1.0 - 2.0 * (i as f32 + 0.5) / n as f32;
            let r = (1.0 - z * z).sqrt();
            let a = golden * i as f32;
            [r * a.cos(), r * a.sin(), z]
        })
        .collect()
}

fn angle(a: &[f32; 3], b: &[f32; 3]) -> f32 {
    let d = (a[0] * b[0] + a[1] * b[1] + a[2] * b[2]).clamp(-1.0, 1.0);
    d.acos().to_degrees()
}

fn bounds(tris: &[Tri]) -> ([f32; 3], [f32; 3]) {
    let (mut min, mut max) = ([f32::MAX; 3], [f32::MIN; 3]);
    for v in tris.iter().flatten() {
        for c in 0..3 {
            min[c] = min[c].min(v[c]);
            max[c] = max[c].max(v[c]);
        }
    }
    (min, max)
}

fn diagonal(tris: &[Tri]) -> f32 {
    let (min, max) = bounds(tris);
    (0..3).map(|c| (max[c] - min[c]).powi(2)).sum::<f32>().sqrt()
}

/// Centra en la cama y apoya a `z` mm de la placa.
fn place(tris: &[Tri], printer: &ResinPrinter, z: f32) -> Vec<Tri> {
    let (min, max) = bounds(tris);
    let d = [printer.bed_x / 2.0 - (min[0] + max[0]) / 2.0, printer.bed_y / 2.0 - (min[1] + max[1]) / 2.0, z - min[2]];
    tris.iter().map(|t| t.map(|v| [v[0] + d[0], v[1] + d[1], v[2] + d[2]])).collect()
}

fn fits(tris: &[Tri], p: &ResinPrinter) -> bool {
    tris.iter().flatten().all(|v| v[0] >= 0.0 && v[1] >= 0.0 && v[0] <= p.bed_x && v[1] <= p.bed_y && v[2] <= p.bed_z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sla::tests::cube;
    use crate::sla::LD_002H;

    #[test]
    fn el_giro_deja_arriba_la_direccion_pedida() {
        for u in fibonacci(40) {
            let t = Turn::facing_up(u);
            let back = t.up();
            assert!(angle(&back, &u) < 0.1, "{u:?} → {t:?} → {back:?}");
            // Girar un triángulo con la normal `u` la deja apuntando a +Z.
            let p = if u[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
            let a = [u[1] * p[2] - u[2] * p[1], u[2] * p[0] - u[0] * p[2], u[0] * p[1] - u[1] * p[0]];
            let b = [u[1] * a[2] - u[2] * a[1], u[2] * a[0] - u[0] * a[2], u[0] * a[1] - u[1] * a[0]];
            let tri: Tri = [[0.0; 3], a, [-b[0], -b[1], -b[2]]];
            let n = area_normal(&tri);
            let r = area_normal(&turn(&[tri], t)[0]);
            let len = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
            let flip = if n[0] * u[0] + n[1] * u[1] + n[2] * u[2] > 0.0 { 1.0 } else { -1.0 };
            assert!((flip * r[2] / len - 1.0).abs() < 1e-3, "{u:?} {r:?}");
        }
    }

    #[test]
    fn una_placa_plana_se_para() {
        let plate = cube([10.0, 10.0, 0.0], [30.0, 30.0, 2.0]);
        let opts = auto_orient(&plate, &LD_002H, 0.05, 45.0, 5);
        let flat = opts.iter().find(|o| o.turn.is_zero()).unwrap();
        assert!(!opts[0].turn.is_zero(), "{opts:#?}");
        assert!(opts[0].max_area_mm2 < flat.max_area_mm2 / 2.0);
        assert!(opts[0].overhang_mm2 < flat.overhang_mm2 / 2.0);
    }

    #[test]
    fn un_cubo_queda_de_punta() {
        // De punta (las tres caras de abajo a 54,7°) no pide soportes en las caras.
        let c = cube([10.0, 10.0, 0.0], [15.0, 15.0, 15.0]);
        let opts = auto_orient(&c, &LD_002H, 0.05, 45.0, 3);
        let up = opts[0].turn.up();
        assert!(up.iter().all(|x| x.abs() > 0.4), "{opts:#?}");
        assert!(opts[0].overhang_mm2 < 1.0);
    }
}
