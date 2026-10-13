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
    /// Cuartos de vuelta sobre Z (0..3); si es 0 vale `rotated`
    #[serde(default)]
    pub quarter_turns: u8,
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
        placements.push(Placement { id: it.id, bed: bed_idx, position: [x, y], rotated: rot, quarter_turns: 0 });
    }
    placements.sort_by_key(|p| p.id);
    let utilization = beds.iter().map(|b| (b.used / (bw * bh)).min(1.0)).collect();
    NestResult { placements, bed_count: beds.len(), utilization, unplaced }
}

/// Máscara de ocupación de una pieza en planta, en celdas de lado `cell`
struct Mask {
    w: usize,
    h: usize,
    /// Celdas del cuerpo (fila a fila)
    core: Vec<bool>,
    /// Cuerpo engrosado por la separación; celdas del borde `pad` incluidas
    grown: Vec<bool>,
    /// Margen de `grown` alrededor del cuerpo, en celdas
    pad: usize,
}

/// Marca las celdas que toca la proyección en XY de la malla girada `turns` cuartos de vuelta
fn rasterize(mesh: &Mesh, turns: u8, cell: Real, spacing: Real) -> Option<Mask> {
    let pts: Vec<(Real, Real)> = mesh
        .vertices
        .iter()
        .map(|v| {
            let (mut x, mut y) = (v.position.x(), v.position.y());
            for _ in 0..turns {
                (x, y) = (-y, x);
            }
            (x, y)
        })
        .collect();
    if pts.is_empty() {
        return None;
    }
    let (min_x, min_y) = pts.iter().fold((Real::INFINITY, Real::INFINITY), |m, p| (m.0.min(p.0), m.1.min(p.1)));
    let (max_x, max_y) = pts.iter().fold((Real::NEG_INFINITY, Real::NEG_INFINITY), |m, p| (m.0.max(p.0), m.1.max(p.1)));
    // Un margen de una celda por el muestreo y la separación (mitad por pieza no: cada una engrosa la suya entera)
    let pad = (spacing / cell).ceil() as usize + 1;
    let (cw, ch) = (((max_x - min_x) / cell).ceil() as usize + 1, ((max_y - min_y) / cell).ceil() as usize + 1);
    let (w, h) = (cw + 2 * pad, ch + 2 * pad);
    let mut core = vec![false; w * h];
    let mut mark = |x: Real, y: Real| {
        let cx = ((x - min_x) / cell).floor().max(0.0) as usize + pad;
        let cy = ((y - min_y) / cell).floor().max(0.0) as usize + pad;
        if cx < w && cy < h {
            core[cy * w + cx] = true;
        }
    };
    for f in 0..mesh.num_faces() {
        let [a, b, c] = mesh.get_face_vertices(f).map(|i| pts[i]);
        let (lo_x, hi_x) = (a.0.min(b.0).min(c.0), a.0.max(b.0).max(c.0));
        let (lo_y, hi_y) = (a.1.min(b.1).min(c.1), a.1.max(b.1).max(c.1));
        let sign = |p: (Real, Real), q: (Real, Real), r: (Real, Real)| (p.0 - r.0) * (q.1 - r.1) - (q.0 - r.0) * (p.1 - r.1);
        let mut y = lo_y;
        while y <= hi_y + cell {
            let mut x = lo_x;
            while x <= hi_x + cell {
                let p = (x.min(hi_x), y.min(hi_y));
                let (d1, d2, d3) = (sign(p, a, b), sign(p, b, c), sign(p, c, a));
                if !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0)) {
                    mark(p.0, p.1);
                }
                x += cell * 0.5;
            }
            y += cell * 0.5;
        }
        for (p, q) in [(a, b), (b, c), (c, a)] {
            let steps = (((q.0 - p.0).hypot(q.1 - p.1)) / (cell * 0.5)).ceil().max(1.0) as usize;
            for k in 0..=steps {
                let t = k as Real / steps as Real;
                mark(p.0 + (q.0 - p.0) * t, p.1 + (q.1 - p.1) * t);
            }
        }
    }
    // Engrosar el cuerpo `pad` celdas (cuadrado: algo conservador)
    let mut grown = core.clone();
    for y in 0..h {
        for x in 0..w {
            if !core[y * w + x] {
                continue;
            }
            for dy in y.saturating_sub(pad)..=(y + pad).min(h - 1) {
                for dx in x.saturating_sub(pad)..=(x + pad).min(w - 1) {
                    grown[dy * w + dx] = true;
                }
            }
        }
    }
    Some(Mask { w, h, core, grown, pad })
}

/// Cama con celdas ocupadas
struct Grid {
    w: usize,
    h: usize,
    occupied: Vec<bool>,
    used: usize,
}

impl Grid {
    /// ¿Cabe la máscara con su esquina en (px, py)? El cuerpo tiene que quedar dentro de la cama
    fn fits(&self, m: &Mask, px: isize, py: isize) -> bool {
        for y in 0..m.h {
            for x in 0..m.w {
                let (gx, gy) = (px + x as isize, py + y as isize);
                let inside = gx >= 0 && gy >= 0 && (gx as usize) < self.w && (gy as usize) < self.h;
                if m.core[y * m.w + x] && !inside {
                    return false;
                }
                if m.grown[y * m.w + x] && inside && self.occupied[gy as usize * self.w + gx as usize] {
                    return false;
                }
            }
        }
        true
    }

    fn put(&mut self, m: &Mask, px: isize, py: isize) {
        for y in 0..m.h {
            for x in 0..m.w {
                if m.core[y * m.w + x] {
                    let (gx, gy) = ((px + x as isize) as usize, (py + y as isize) as usize);
                    if !self.occupied[gy * self.w + gx] {
                        self.occupied[gy * self.w + gx] = true;
                        self.used += 1;
                    }
                }
            }
        }
    }
}

/// Distribuye las mallas por su contorno en planta (no por su caja), con giros de 90° opcionales
///
/// Cada pieza se prueba en las posiciones de la cama de abajo hacia arriba y de izquierda
/// a derecha; gana la que queda más abajo. Va por celdas (de `cell` mm), así que la
/// separación real es de al menos `spacing` y a lo sumo una celda más.
pub fn nest_contour(meshes: &[&Mesh], bed: [Real; 2], spacing: Real, allow_rotation: bool, cell: Real) -> NestResult {
    let cell = cell.max(0.1);
    let spacing = spacing.max(0.0);
    let (gw, gh) = ((bed[0] / cell).floor() as usize, (bed[1] / cell).floor() as usize);
    let turns_list: &[u8] = if allow_rotation { &[0, 1, 2, 3] } else { &[0] };

    // Las más grandes primero (por área de la caja)
    let mut order: Vec<usize> = (0..meshes.len()).collect();
    let area = |i: usize| {
        let f = footprint(meshes[i]);
        f[0] * f[1]
    };
    order.sort_by(|&a, &b| area(b).total_cmp(&area(a)));

    let mut beds: Vec<Grid> = Vec::new();
    let mut placements = Vec::new();
    let mut unplaced = Vec::new();
    for id in order {
        let masks: Vec<(u8, Mask)> = turns_list.iter().filter_map(|&t| rasterize(meshes[id], t, cell, spacing).map(|m| (t, m))).collect();
        let try_bed = |grid: &Grid| -> Option<(isize, isize, u8, usize)> {
            let mut best: Option<(isize, isize, u8, usize)> = None;
            for (k, (turns, m)) in masks.iter().enumerate() {
                let core_w = m.w - 2 * m.pad;
                let core_h = m.h - 2 * m.pad;
                if core_w > grid.w || core_h > grid.h {
                    continue;
                }
                let pad = m.pad as isize;
                'scan: for py in -pad..=(grid.h as isize - core_h as isize - pad) {
                    if best.is_some_and(|b| py > b.1) {
                        break;
                    }
                    for px in -pad..=(grid.w as isize - core_w as isize - pad) {
                        if best.is_some_and(|b| py == b.1 && px >= b.0) {
                            break 'scan;
                        }
                        if grid.fits(m, px, py) {
                            best = Some((px, py, *turns, k));
                            break 'scan;
                        }
                    }
                }
            }
            best
        };
        let mut placed = None;
        for (bi, grid) in beds.iter().enumerate() {
            if let Some(found) = try_bed(grid) {
                placed = Some((bi, found));
                break;
            }
        }
        if placed.is_none() {
            let empty = Grid { w: gw, h: gh, occupied: vec![false; gw * gh], used: 0 };
            match try_bed(&empty) {
                Some(found) => {
                    beds.push(empty);
                    placed = Some((beds.len() - 1, found));
                }
                None => {
                    unplaced.push(id);
                    continue;
                }
            }
        }
        let (bi, (px, py, turns, k)) = placed.expect("colocada");
        let mask = &masks[k].1;
        beds[bi].put(mask, px, py);
        // El cuerpo empieza `pad` celdas dentro de la máscara
        placements.push(Placement {
            id,
            bed: bi,
            position: [(px + mask.pad as isize) as Real * cell, (py + mask.pad as isize) as Real * cell],
            rotated: turns % 2 == 1,
            quarter_turns: turns,
        });
    }
    placements.sort_by_key(|p| p.id);
    let utilization = beds.iter().map(|g| (g.used as Real / (g.w * g.h).max(1) as Real).min(1.0)).collect();
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
        let turns = if p.quarter_turns > 0 { p.quarter_turns % 4 } else { u8::from(p.rotated) };
        for _ in 0..turns {
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

    /// Triángulo rectángulo recto (prisma) de catetos `a` y `b`, alto `h`
    fn wedge(a: Real, b: Real, h: Real) -> Mesh {
        let v = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(a, 0.0, 0.0),
            Vector3::new(0.0, b, 0.0),
            Vector3::new(0.0, 0.0, h),
            Vector3::new(a, 0.0, h),
            Vector3::new(0.0, b, h),
        ];
        let f = vec![[0, 2, 1], [3, 4, 5], [0, 1, 4], [0, 4, 3], [1, 2, 5], [1, 5, 4], [2, 0, 3], [2, 3, 5]];
        Mesh::from_triangles(&v, &f)
    }

    #[test]
    fn contorno_aprovecha_mejor_que_la_caja() {
        // Seis cuñas de 50x50: por caja caben 4 en 110x110; por contorno, enfrentadas, caben más
        let meshes: Vec<Mesh> = (0..6).map(|_| wedge(50.0, 50.0, 10.0)).collect();
        let refs: Vec<&Mesh> = meshes.iter().collect();
        let items: Vec<NestItem> = meshes.iter().enumerate().map(|(id, m)| NestItem { id, size: footprint(m) }).collect();
        let by_box = nest(&items, [110.0, 110.0], 1.0, true);
        let by_shape = nest_contour(&refs, [110.0, 110.0], 1.0, true, 1.0);
        assert!(by_shape.unplaced.is_empty(), "{by_shape:?}");
        assert!(by_shape.bed_count < by_box.bed_count, "contorno {} camas, caja {}", by_shape.bed_count, by_box.bed_count);
    }

    #[test]
    fn contorno_no_solapa_y_cabe_en_la_cama() {
        let mut meshes: Vec<Mesh> = (0..5).map(|i| wedge(40.0 + i as Real * 3.0, 30.0, 8.0)).collect();
        let refs: Vec<&Mesh> = meshes.iter().collect();
        let r = nest_contour(&refs, [120.0, 100.0], 2.0, true, 0.5);
        assert!(r.unplaced.is_empty());
        apply_nesting(&mut meshes, &r);
        for (i, m) in meshes.iter().enumerate() {
            let bb = crate::analysis::compute_bounding_box(m);
            assert!(bb.min[0] >= -1e-6 && bb.min[1] >= -1e-6 && bb.max[0] <= 120.0 + 1e-6 && bb.max[1] <= 100.0 + 1e-6, "{i}: {bb:?}");
        }
        // Dos piezas de la misma cama no se pisan: distancia entre sus vértices (cualquier punto de una no dentro de la otra)
        for i in 0..meshes.len() {
            for j in i + 1..meshes.len() {
                if r.placements[i].bed != r.placements[j].bed {
                    continue;
                }
                let inside = |p: Vector3, m: &Mesh| {
                    // prisma triangular: dentro si x,y,x+y/..: usar caja + semiplano de la hipotenusa por triángulos de la base
                    (0..m.num_faces()).any(|f| {
                        let [a, b, c] = m.get_face_vertices(f).map(|k| m.vertices[k].position);
                        if (a.z() - b.z()).abs() > 1e-9 || (b.z() - c.z()).abs() > 1e-9 || a.z() > 1e-6 {
                            return false;
                        }
                        let sg = |p: Vector3, q: Vector3, r: Vector3| (p.x() - r.x()) * (q.y() - r.y()) - (q.x() - r.x()) * (p.y() - r.y());
                        let (d1, d2, d3) = (sg(p, a, b), sg(p, b, c), sg(p, c, a));
                        !((d1 < -1e-9 || d2 < -1e-9 || d3 < -1e-9) && (d1 > 1e-9 || d2 > 1e-9 || d3 > 1e-9))
                    })
                };
                for v in &meshes[i].vertices {
                    assert!(!inside(v.position, &meshes[j]), "pieza {i} dentro de {j}");
                }
                for v in &meshes[j].vertices {
                    assert!(!inside(v.position, &meshes[i]), "pieza {j} dentro de {i}");
                }
            }
        }
    }

    #[test]
    fn contorno_pieza_demasiado_grande() {
        let meshes = [make_box(300.0, 300.0, 5.0)];
        let refs: Vec<&Mesh> = meshes.iter().collect();
        let r = nest_contour(&refs, [100.0, 100.0], 2.0, true, 1.0);
        assert_eq!(r.unplaced, vec![0]);
        assert_eq!(r.bed_count, 0);
    }
}
