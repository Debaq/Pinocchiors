//! Referencias del diseño tomadas del escaneo ([`ScanFit`]): dónde se hizo
//! clic queda como triángulo + baricéntricas, así la referencia sigue al
//! escaneo si se mueve; si la malla cambió (reparada, remallada) se busca el
//! punto más cercano al último lugar conocido.

use cad_model::geom::{P3, Plane, add, cross, dot, norm, normalize, scale, sub};
use cad_model::{ScanDab, ScanFit, ScanSeed};

use crate::{PickError, PickOptions, ScanMesh, pick_cylinder, pick_plane};

/// Geometría ajustada de una referencia del escaneo
#[derive(Debug, Clone, PartialEq)]
pub enum ScanGeom {
    Plane(Plane),
    Axis { origin: P3, direction: P3 },
    Point(P3),
}

impl ScanMesh {
    /// Huella de los triángulos (cambia si la malla se rehace, no si se mueve)
    pub fn topology(&self) -> u32 {
        // FNV-1a sobre la cantidad de vértices y los índices
        let mut h: u32 = 0x811c_9dc5;
        let mut eat = |v: u32| {
            for b in v.to_le_bytes() {
                h ^= b as u32;
                h = h.wrapping_mul(0x0100_0193);
            }
        };
        eat(self.vertices.len() as u32);
        for t in &self.triangles {
            t.iter().for_each(|&i| eat(i));
        }
        h
    }

    fn corners(&self, triangle: u32) -> [P3; 3] {
        self.triangles[triangle as usize].map(|i| self.vertices[i as usize])
    }

    /// El punto de la superficie más cercano a `p`: triángulo y baricéntricas
    pub fn closest(&self, p: P3) -> Option<(u32, [f64; 3])> {
        let mut best: Option<(f64, u32, [f64; 3])> = None;
        for t in 0..self.triangles.len() as u32 {
            let [a, b, c] = self.corners(t);
            let bary = closest_on_triangle(p, a, b, c);
            let q = add(add(scale(a, bary[0]), scale(b, bary[1])), scale(c, bary[2]));
            let d = dot(sub(q, p), sub(q, p));
            if best.is_none_or(|(bd, _, _)| d < bd) {
                best = Some((d, t, bary));
            }
        }
        best.map(|(_, t, bary)| (t, bary))
    }

    /// Semilla para un clic sobre el triángulo `triangle` en `at` (mm)
    pub fn seed(&self, triangle: u32, at: P3) -> Option<ScanSeed> {
        if triangle as usize >= self.triangles.len() {
            return None;
        }
        let [a, b, c] = self.corners(triangle);
        Some(ScanSeed { triangle, bary: closest_on_triangle(at, a, b, c), at, topology: self.topology() })
    }

    /// Dónde está hoy la semilla: el mismo triángulo si la malla es la misma
    /// (aunque se haya movido), si no el punto más cercano a donde estaba
    pub fn locate(&self, seed: &ScanSeed) -> Option<(u32, P3)> {
        let (t, bary) = if seed.topology == self.topology() && (seed.triangle as usize) < self.triangles.len() {
            (seed.triangle, seed.bary)
        } else {
            self.closest(seed.at)?
        };
        let [a, b, c] = self.corners(t);
        Some((t, add(add(scale(a, bary[0]), scale(b, bary[1])), scale(c, bary[2]))))
    }

    /// Toque del pincel sobre el triángulo `triangle` en `at` (mm)
    pub fn dab(&self, triangle: u32, at: P3) -> Option<ScanDab> {
        let seed = self.seed(triangle, at)?;
        self.fit_dab(ScanDab { seed, point: at, normal: [0.0; 3] })
    }

    /// El toque puesto de nuevo sobre el escaneo de hoy: punto de la piel y
    /// su normal hacia afuera
    pub fn fit_dab(&self, dab: ScanDab) -> Option<ScanDab> {
        let (t, point) = self.locate(&dab.seed)?;
        Some(ScanDab { seed: dab.seed, point, normal: self.face_normals[t as usize] })
    }
}

/// Ajusta la referencia contra el escaneo de hoy
pub fn fit_reference(mesh: &ScanMesh, fit: &ScanFit, opts: &PickOptions) -> Result<ScanGeom, PickError> {
    let lost = || PickError::Other("no se encontró en el escaneo el lugar de la referencia".into());
    Ok(match fit {
        ScanFit::Zone { seed } => {
            let (t, _) = mesh.locate(seed).ok_or_else(lost)?;
            ScanGeom::Plane(pick_plane(mesh, t, opts)?.plane)
        }
        ScanFit::Points { seeds } => {
            let mut p = [[0.0; 3]; 3];
            for (k, s) in seeds.iter().enumerate() {
                p[k] = mesh.locate(s).ok_or_else(lost)?.1;
            }
            let n = cross(sub(p[1], p[0]), sub(p[2], p[0]));
            if norm(n) < 1e-9 * norm(sub(p[1], p[0])).max(1.0).powi(2) {
                return Err(PickError::Other("los tres puntos están alineados".into()));
            }
            ScanGeom::Plane(Plane { origin: p[0], normal: normalize(n), x_dir: normalize(sub(p[1], p[0])) })
        }
        ScanFit::Cylinder { seed } => {
            let (t, _) = mesh.locate(seed).ok_or_else(lost)?;
            let c = pick_cylinder(mesh, t, opts)?;
            ScanGeom::Axis { origin: c.origin, direction: c.direction }
        }
        ScanFit::Point { seed } => ScanGeom::Point(mesh.locate(seed).ok_or_else(lost)?.1),
    })
}

/// Baricéntricas del punto del triángulo más cercano a `p` (Ericson,
/// «Real-Time Collision Detection» 5.1.5)
pub(crate) fn closest_on_triangle(p: P3, a: P3, b: P3, c: P3) -> [f64; 3] {
    let (ab, ac, ap) = (sub(b, a), sub(c, a), sub(p, a));
    let (d1, d2) = (dot(ab, ap), dot(ac, ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return [1.0, 0.0, 0.0];
    }
    let bp = sub(p, b);
    let (d3, d4) = (dot(ab, bp), dot(ac, bp));
    if d3 >= 0.0 && d4 <= d3 {
        return [0.0, 1.0, 0.0];
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return [1.0 - v, v, 0.0];
    }
    let cp = sub(p, c);
    let (d5, d6) = (dot(ab, cp), dot(ac, cp));
    if d6 >= 0.0 && d5 <= d6 {
        return [0.0, 0.0, 1.0];
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return [1.0 - w, 0.0, w];
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return [0.0, 1.0 - w, w];
    }
    let denom = 1.0 / (va + vb + vc);
    let (v, w) = (vb * denom, vc * denom);
    [1.0 - v - w, v, w]
}
