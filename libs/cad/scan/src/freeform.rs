//! Zona de forma libre del escaneo: la región suave alrededor de un clic, con
//! su borde cerrado y puntos de adentro, para ajustarle una superficie.

use cad_model::geom::{P3, add, norm, scale, sub};
use cad_model::occt::{Curve, Shape};

use crate::ScanMesh;
use crate::refs::closest_on_triangle;
use crate::pick::{boundary_loops, grow_dyn};

#[derive(Debug, Clone, PartialEq)]
pub struct FreeZone {
    /// Borde exterior cerrado (el último punto no repite el primero), repartido parejo
    pub boundary: Vec<P3>,
    /// Puntos de la zona, de a lo sumo `max_interior`, para que la superficie los siga
    pub interior: Vec<P3>,
    pub faces: Vec<u32>,
}

impl ScanMesh {
    /// La zona suave (normales vecinas a menos de `angle` grados) que contiene
    /// el triángulo `seed`. `None` si no tiene un borde cerrado (un objeto
    /// cerrado y suave entero) o es muy chica.
    pub fn free_zone(&self, seed: u32, angle: f64, max_interior: usize) -> Option<FreeZone> {
        if seed as usize >= self.face_count() {
            return None;
        }
        let cos = angle.to_radians().cos();
        let faces = grow_dyn(self, seed, usize::MAX, &|f, g| cad_model::geom::dot(self.face_normals[f as usize], self.face_normals[g as usize]) >= cos);
        if faces.len() < 8 {
            return None;
        }
        // El lazo de borde más largo es el contorno exterior
        let loops = boundary_loops(self, &faces);
        let length = |l: &[P3]| (0..l.len()).map(|k| norm(sub(l[(k + 1) % l.len()], l[k]))).sum::<f64>();
        let outer = loops.into_iter().max_by(|a, b| length(a).total_cmp(&length(b)))?;
        let total = length(&outer);
        if total <= 0.0 {
            return None;
        }
        let n = ((total / (self.diagonal() * 0.01).max(1e-9)).ceil() as usize).clamp(12, 40);
        let boundary = resample_closed(&outer, n);
        // Interior: centroides parejos, sin los pegados al borde (ya los fija el borde)
        let step = (faces.len() as f64 / max_interior as f64).max(1.0);
        let margin = total / n as f64 * 1.5;
        let interior: Vec<P3> = (0..faces.len().min(max_interior))
            .map(|i| self.face_centroids[faces[(i as f64 * step) as usize] as usize])
            .filter(|p| boundary.iter().all(|b| norm(sub(*p, *b)) > margin))
            .collect();
        Some(FreeZone { boundary, interior, faces })
    }
}

impl FreeZone {
    /// Elige los puntos de adentro que mejor ajustan la superficie de relleno
    /// de OpenCASCADE: con demasiados o muy pocos puntos la cara sale inválida o
    /// se aparta del escaneo, así que prueba varias cantidades (puntos más
    /// lejanos entre sí) y se queda con la válida de menor desviación. Devuelve
    /// la desviación media cuadrática (mm). `None` si ninguna sirve.
    pub fn fit_surface(&mut self, mesh: &ScanMesh) -> Option<f64> {
        let all = std::mem::take(&mut self.interior);
        if all.is_empty() {
            return None;
        }
        // Puntos más lejanos entre sí: cada prefijo es un buen reparto
        let mut order = vec![all[0]];
        let mut far: Vec<f64> = all.iter().map(|p| norm(sub(*p, all[0]))).collect();
        while order.len() < all.len().min(100) {
            let (k, _) = far.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap();
            order.push(all[k]);
            for (i, p) in all.iter().enumerate() {
                far[i] = far[i].min(norm(sub(*p, all[k])));
            }
        }
        let n = self.boundary.len();
        let lines: Vec<Curve> = (0..n).map(|k| Curve::Line(self.boundary[k], self.boundary[(k + 1) % n])).collect();
        let wire = Shape::wire(&lines).ok()?;
        // Triángulos de la zona (de a lo sumo 3000) contra los que se mide
        let step = (self.faces.len() as f64 / 8000.0).max(1.0);
        let tris: Vec<[P3; 3]> = (0..self.faces.len().min(8000)).map(|i| mesh.triangles[self.faces[(i as f64 * step) as usize] as usize].map(|v| mesh.vertices[v as usize])).collect();
        let deviation = |face: &Shape| -> Option<f64> {
            let t = face.tessellate(mesh.diagonal() * 0.005, 0.3).ok()?;
            let k = (t.positions.len() as f64 / 120.0).max(1.0);
            let mut sq = 0.0;
            let mut count = 0.0;
            for i in 0..t.positions.len().min(120) {
                let p = t.positions[(i as f64 * k) as usize];
                let d = tris
                    .iter()
                    .map(|[a, b, c]| {
                        let w = closest_on_triangle(p, *a, *b, *c);
                        norm(sub(p, add(add(scale(*a, w[0]), scale(*b, w[1])), scale(*c, w[2]))))
                    })
                    .fold(f64::MAX, f64::min);
                sq += d * d;
                count += 1.0;
            }
            (count > 0.0).then(|| (sq / count).sqrt())
        };
        let mut best: Option<(f64, usize)> = None;
        for count in [12usize, 24, 36, 48, 60, 72, 84, 100] {
            let count = count.min(order.len());
            let Ok(face) = Shape::fill(&[(wire.clone(), None)], &order[..count]) else { continue };
            if !face.is_valid() {
                continue;
            }
            let Some(rms) = deviation(&face) else { continue };
            if best.is_none_or(|(b, _)| rms < b) {
                best = Some((rms, count));
            }
        }
        let (rms, count) = best?;
        self.interior = order[..count].to_vec();
        Some(rms)
    }
}

/// `n` puntos a distancias iguales a lo largo del lazo cerrado
fn resample_closed(l: &[P3], n: usize) -> Vec<P3> {
    let m = l.len();
    let mut acc = vec![0.0];
    for k in 0..m {
        acc.push(acc[k] + norm(sub(l[(k + 1) % m], l[k])));
    }
    let total = acc[m];
    let mut out = Vec::with_capacity(n);
    let mut seg = 0;
    for i in 0..n {
        let s = total * i as f64 / n as f64;
        while seg + 1 < m && acc[seg + 1] < s {
            seg += 1;
        }
        let span = (acc[seg + 1] - acc[seg]).max(1e-12);
        let t = ((s - acc[seg]) / span).clamp(0.0, 1.0);
        out.push(add(l[seg], scale(sub(l[(seg + 1) % m], l[seg]), t)));
    }
    out
}
