//! Distribución de piezas en la cama (nesting) con rectángulos máximos
//!
//! Cada pieza se trata como su rectángulo en planta (apoyada en la cama).
//! Se reparte en tantas camas como haga falta, con separación entre piezas y
//! giro opcional de 90° sobre Z.

use pinocchio_math::{Real, Vector3};
use pinocchio_mesh::Mesh;
use serde::{Deserialize, Serialize};

use crate::analysis::compute_bounding_box;

/// Pieza a distribuir: su huella (ancho, fondo) en mm
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NestItem {
    pub id: usize,
    pub size: [Real; 2],
}

/// Dónde queda una pieza
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    pub id: usize,
    /// Cama (0, 1, ...)
    pub bed: usize,
    /// Esquina mínima de la huella en la cama (mm)
    pub position: [Real; 2],
    /// Girada 90° sobre Z
    pub rotated: bool,
}

/// Resultado de la distribución
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NestResult {
    pub placements: Vec<Placement>,
    pub bed_count: usize,
    /// Fracción de cada cama ocupada por las huellas
    pub utilization: Vec<Real>,
    /// Piezas que no caben en una cama vacía
    pub unplaced: Vec<usize>,
}

#[derive(Clone, Copy)]
struct Rect {
    x: Real,
    y: Real,
    w: Real,
    h: Real,
}

struct Bed {
    free: Vec<Rect>,
    used: Real,
}

impl Bed {
    fn new(w: Real, h: Real) -> Self {
        Self { free: vec![Rect { x: 0.0, y: 0.0, w, h }], used: 0.0 }
    }

    /// Mejor hueco por ajuste del lado corto: (rect libre, sobrante corto, rotada)
    fn best_fit(&self, w: Real, h: Real, rotation: bool) -> Option<(Real, Real, Real, bool, Real)> {
        let mut best: Option<(Real, Real, Real, bool, Real)> = None; // (short, long, x, rot, y)
        for r in &self.free {
            for (rot, iw, ih) in [(false, w, h), (true, h, w)] {
                if rot && !rotation {
                    continue;
                }
                if iw <= r.w + 1e-9 && ih <= r.h + 1e-9 {
                    let short = (r.w - iw).min(r.h - ih);
                    let long = (r.w - iw).max(r.h - ih);
                    if best.is_none_or(|b| (short, long) < (b.0, b.1)) {
                        best = Some((short, long, r.x, rot, r.y));
                    }
                }
            }
        }
        best
    }

    fn place(&mut self, x: Real, y: Real, w: Real, h: Real) {
        let placed = Rect { x, y, w, h };
        let mut next = Vec::new();
        for r in &self.free {
            if x >= r.x + r.w - 1e-9 || x + w <= r.x + 1e-9 || y >= r.y + r.h - 1e-9 || y + h <= r.y + 1e-9 {
                next.push(*r);
                continue;
            }
            // Dividir el libre alrededor de lo colocado
            if x > r.x {
                next.push(Rect { x: r.x, y: r.y, w: x - r.x, h: r.h });
            }
            if x + w < r.x + r.w {
                next.push(Rect { x: x + w, y: r.y, w: r.x + r.w - (x + w), h: r.h });
            }
            if y > r.y {
                next.push(Rect { x: r.x, y: r.y, w: r.w, h: y - r.y });
            }
            if y + h < r.y + r.h {
                next.push(Rect { x: r.x, y: y + h, w: r.w, h: r.y + r.h - (y + h) });
            }
        }
        // Quitar los contenidos en otro
        let contained = |a: &Rect, b: &Rect| a.x >= b.x - 1e-9 && a.y >= b.y - 1e-9 && a.x + a.w <= b.x + b.w + 1e-9 && a.y + a.h <= b.y + b.h + 1e-9;
        let mut keep = vec![true; next.len()];
        for i in 0..next.len() {
            for j in 0..next.len() {
                if i != j && keep[i] && keep[j] && contained(&next[i], &next[j]) {
                    keep[i] = false;
                }
            }
        }
        self.free = next.into_iter().zip(keep).filter_map(|(r, k)| k.then_some(r)).collect();
        self.used += placed.w * placed.h;
    }
}

/// Reparte `items` en camas de `bed` (ancho, fondo) con `spacing` mm entre piezas
pub fn nest(items: &[NestItem], bed: [Real; 2], spacing: Real, allow_rotation: bool) -> NestResult {
    let spacing = spacing.max(0.0);
    // Cada pieza ocupa su huella más la separación; la cama crece lo mismo para que
    // la última pieza no necesite separación contra el borde
    let (bw, bh) = (bed[0] + spacing, bed[1] + spacing);
    let mut order: Vec<&NestItem> = items.iter().collect();
    order.sort_by(|a, b| (b.size[0] * b.size[1]).total_cmp(&(a.size[0] * a.size[1])));

    let mut beds: Vec<Bed> = Vec::new();
    let mut placements = Vec::new();
    let mut unplaced = Vec::new();
    for it in order {
        let (w, h) = (it.size[0] + spacing, it.size[1] + spacing);
        let fits_empty = (w <= bw + 1e-9 && h <= bh + 1e-9) || (allow_rotation && h <= bw + 1e-9 && w <= bh + 1e-9);
        if !fits_empty {
            unplaced.push(it.id);
            continue;
        }
        let slot = beds.iter().enumerate().find_map(|(i, b)| b.best_fit(w, h, allow_rotation).map(|f| (i, f)));
        let (bed_idx, (_, _, x, rot, y)) = match slot {
            Some(s) => s,
            None => {
                beds.push(Bed::new(bw, bh));
                let i = beds.len() - 1;
                (i, beds[i].best_fit(w, h, allow_rotation).expect("cabe en una cama vacía"))
            }
        };
        let (pw, ph) = if rot { (h, w) } else { (w, h) };
        beds[bed_idx].place(x, y, pw, ph);
        placements.push(Placement { id: it.id, bed: bed_idx, position: [x, y], rotated: rot });
    }
    placements.sort_by_key(|p| p.id);
    let utilization = beds.iter().map(|b| (b.used / (bw * bh)).min(1.0)).collect();
    NestResult { placements, bed_count: beds.len(), utilization, unplaced }
}

/// Huella (ancho, fondo) de una malla
pub fn footprint(mesh: &Mesh) -> [Real; 2] {
    let d = compute_bounding_box(mesh).dimensions();
    [d[0], d[1]]
}

/// Coloca las mallas según `result`: giradas si hace falta y trasladadas a su
/// sitio en la cama (esquina de la cama en el origen, base en Z = 0). Las mallas
/// sin colocar quedan igual
pub fn apply_nesting(meshes: &mut [Mesh], result: &NestResult) {
    for p in &result.placements {
        let Some(mesh) = meshes.get_mut(p.id) else { continue };
        if mesh.vertices.is_empty() {
            continue;
        }
        if p.rotated {
            for v in &mut mesh.vertices {
                let r = |q: Vector3| Vector3::new(-q.y(), q.x(), q.z());
                v.position = r(v.position);
                v.normal = r(v.normal);
            }
        }
        let bb = compute_bounding_box(mesh);
        let shift = Vector3::new(p.position[0] - bb.min[0], p.position[1] - bb.min[1], -bb.min[2]);
        for v in &mut mesh.vertices {
            v.position += shift;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::printability::tests::make_box;

    fn overlap(a: ([Real; 2], [Real; 2]), b: ([Real; 2], [Real; 2])) -> bool {
        a.0[0] < b.0[0] + b.1[0] - 1e-9 && b.0[0] < a.0[0] + a.1[0] - 1e-9 && a.0[1] < b.0[1] + b.1[1] - 1e-9 && b.0[1] < a.0[1] + a.1[1] - 1e-9
    }

    #[test]
    fn cuatro_cuadrados_en_una_cama() {
        let items: Vec<NestItem> = (0..4).map(|id| NestItem { id, size: [100.0, 100.0] }).collect();
        let r = nest(&items, [220.0, 220.0], 5.0, true);
        assert_eq!(r.bed_count, 1, "{r:?}");
        assert!(r.unplaced.is_empty());
    }

    #[test]
    fn desborda_a_otra_cama_sin_solaparse() {
        let items: Vec<NestItem> = (0..9).map(|id| NestItem { id, size: [100.0, 80.0 + id as Real] }).collect();
        let r = nest(&items, [220.0, 220.0], 2.0, true);
        assert!(r.bed_count >= 2);
        assert_eq!(r.placements.len(), 9);
        for i in 0..9 {
            for j in i + 1..9 {
                let (a, b) = (&r.placements[i], &r.placements[j]);
                if a.bed != b.bed {
                    continue;
                }
                let dim = |p: &Placement| {
                    let s = items[p.id].size;
                    if p.rotated { [s[1], s[0]] } else { s }
                };
                assert!(!overlap((a.position, dim(a)), (b.position, dim(b))), "{a:?} {b:?}");
            }
        }
        for p in &r.placements {
            let s = items[p.id].size;
            let (w, h) = if p.rotated { (s[1], s[0]) } else { (s[0], s[1]) };
            assert!(p.position[0] + w <= 220.0 + 1e-6 && p.position[1] + h <= 220.0 + 1e-6, "{p:?}");
        }
    }

    #[test]
    fn pieza_demasiado_grande() {
        let items = [NestItem { id: 0, size: [300.0, 300.0] }, NestItem { id: 1, size: [10.0, 10.0] }];
        let r = nest(&items, [220.0, 220.0], 2.0, true);
        assert_eq!(r.unplaced, vec![0]);
        assert_eq!(r.placements.len(), 1);
    }

    #[test]
    fn giro_permite_que_quepa() {
        let items = [NestItem { id: 0, size: [200.0, 100.0] }];
        let r = nest(&items, [120.0, 220.0], 0.0, true);
        assert!(r.unplaced.is_empty() && r.placements[0].rotated);
        let r = nest(&items, [120.0, 220.0], 0.0, false);
        assert_eq!(r.unplaced, vec![0]);
    }

    #[test]
    fn aplica_a_las_mallas() {
        let mut meshes = vec![make_box(200.0, 100.0, 10.0), make_box(50.0, 50.0, 5.0)];
        let items: Vec<NestItem> = meshes.iter().enumerate().map(|(id, m)| NestItem { id, size: footprint(m) }).collect();
        let r = nest(&items, [120.0, 220.0], 2.0, true);
        apply_nesting(&mut meshes, &r);
        for m in &meshes {
            let bb = compute_bounding_box(m);
            assert!(bb.min[0] >= -1e-9 && bb.min[1] >= -1e-9 && bb.min[2].abs() < 1e-9);
            assert!(bb.max[0] <= 120.0 + 1e-6 && bb.max[1] <= 220.0 + 1e-6, "{bb:?}");
        }
    }
}
