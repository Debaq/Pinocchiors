//! Zona de forma libre del escaneo: la región suave alrededor de un clic, con
//! su borde cerrado y puntos de adentro, para ajustarle una superficie.

use cad_model::ScanGrid;
use cad_model::geom::{P3, add, cross, dot, norm, normalize, scale, sub};
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
    /// Grilla de puntos de la piel, si la superficie por grilla se pega más al
    /// escaneo que el relleno (ver `fit_surface`)
    pub grid: Option<ScanGrid>,
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
        Some(FreeZone { boundary, interior, faces, grid: None })
    }
}

impl FreeZone {
    /// Elige la superficie que más se pega al escaneo: el relleno de OpenCASCADE
    /// por puntos de adentro o la B-spline por una grilla de la piel
    /// (`skin_grid`) recortada por el borde. Para el relleno, los puntos de adentro que mejor lo ajustan:
    /// con demasiados o muy pocos puntos la cara sale inválida o se aparta del
    /// escaneo, así que prueba varias cantidades (puntos más
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
        // B-spline por una grilla de la piel, recortada por el borde: que pase
        // por los puntos o, más densa, que los aproxime (sin copiar el ruido)
        self.grid = None;
        let mut best_grid: Option<(f64, ScanGrid)> = None;
        let diag = mesh.diagonal();
        for (n, tol) in [(13usize, 0.0), (21, 0.0), (31, diag * 2e-4)] {
            let Some(mut grid) = self.skin_grid(mesh, n) else { break };
            grid.tol = tol;
            let Ok(face) = Shape::grid_face(&grid.points, grid.rows, grid.cols, grid.tol, &self.boundary) else { continue };
            if !face.is_valid() {
                continue;
            }
            let Some(rms) = deviation(&face) else { continue };
            if best_grid.as_ref().is_none_or(|(b, _)| rms < *b) {
                best_grid = Some((rms, grid));
            }
        }
        let fill = best.map(|(rms, count)| {
            self.interior = order[..count].to_vec();
            rms
        });
        match (fill, best_grid) {
            (Some(f), Some((g, grid))) if g < f => {
                self.grid = Some(grid);
                Some(g)
            }
            (None, Some((g, grid))) => {
                self.interior = order[..order.len().min(36)].to_vec();
                self.grid = Some(grid);
                Some(g)
            }
            (f, _) => f,
        }
    }

    /// Grilla de `size × size` puntos de la piel sobre el rectángulo que cubre el
    /// borde, en el plano medio de la zona (`tol` 0). Cada punto es donde la recta perpendicular al plano toca la zona;
    /// los que caen fuera (esquinas del rectángulo) siguen suave a los vecinos.
    /// `None` si la zona se dobla sobre sí (no es una altura sobre el plano).
    pub fn skin_grid(&self, mesh: &ScanMesh, size: usize) -> Option<ScanGrid> {
        if size < 2 {
            return None;
        }
        let tri = |f: u32| mesh.triangles[f as usize].map(|v| mesh.vertices[v as usize]);
        // Normal media (por área) y que ninguna cara mire de costado
        let mut n = [0.0; 3];
        for &f in &self.faces {
            n = add(n, scale(mesh.face_normals[f as usize], mesh.face_areas[f as usize]));
        }
        if norm(n) < 1e-12 {
            return None;
        }
        let n = normalize(n);
        if self.faces.iter().any(|&f| mesh.face_areas[f as usize] > 1e-12 && dot(mesh.face_normals[f as usize], n) < 0.2) {
            return None;
        }
        // Ejes del rectángulo: los del borde proyectado (el más largo primero)
        let c = scale(self.boundary.iter().fold([0.0; 3], |a, p| add(a, *p)), 1.0 / self.boundary.len() as f64);
        let u0 = normalize(if n[0].abs() < 0.9 { cross(n, [1.0, 0.0, 0.0]) } else { cross(n, [0.0, 1.0, 0.0]) });
        let v0 = cross(n, u0);
        let (mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0);
        for p in &self.boundary {
            let d = sub(*p, c);
            let (x, y) = (dot(d, u0), dot(d, v0));
            (sxx, syy, sxy) = (sxx + x * x, syy + y * y, sxy + x * y);
        }
        let a = 0.5 * (2.0 * sxy).atan2(sxx - syy);
        let u = add(scale(u0, a.cos()), scale(v0, a.sin()));
        let v = cross(n, u);
        let uv = |p: P3| {
            let d = sub(p, c);
            [dot(d, u), dot(d, v)]
        };
        let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
        for p in &self.boundary {
            let q = uv(*p);
            for k in 0..2 {
                lo[k] = lo[k].min(q[k]);
                hi[k] = hi[k].max(q[k]);
            }
        }
        for k in 0..2 {
            let m = (hi[k] - lo[k]) * 0.02;
            lo[k] -= m;
            hi[k] += m;
        }
        let side = (hi[0] - lo[0]).max(hi[1] - lo[1]);
        if side <= 0.0 {
            return None;
        }
        // Triángulos de la zona con su caja en el plano
        let tris: Vec<([P3; 3], [f64; 4])> = self
            .faces
            .iter()
            .map(|&f| {
                let t = tri(f);
                let q = t.map(uv);
                let b = [q[0][0].min(q[1][0]).min(q[2][0]), q[0][0].max(q[1][0]).max(q[2][0]), q[0][1].min(q[1][1]).min(q[2][1]), q[0][1].max(q[1][1]).max(q[2][1])];
                (t, b)
            })
            .collect();
        let (rows, cols) = (size, size);
        let mut h: Vec<Option<f64>> = vec![None; rows * cols];
        for r in 0..rows {
            for k in 0..cols {
                let q = [lo[0] + (hi[0] - lo[0]) * k as f64 / (cols - 1) as f64, lo[1] + (hi[1] - lo[1]) * r as f64 / (rows - 1) as f64];
                let o = add(c, add(scale(u, q[0]), scale(v, q[1])));
                let mut hits: Vec<f64> = Vec::new();
                for (t, b) in &tris {
                    if q[0] < b[0] || q[0] > b[1] || q[1] < b[2] || q[1] > b[3] {
                        continue;
                    }
                    if let Some(s) = ray_triangle(o, n, t) {
                        hits.push(s);
                    }
                }
                if let (Some(mn), Some(mx)) = (hits.iter().copied().reduce(f64::min), hits.iter().copied().reduce(f64::max)) {
                    // Dos capas de la zona en la misma recta: se dobla
                    if mx - mn > side * 0.02 {
                        return None;
                    }
                    h[r * cols + k] = Some(0.5 * (mn + mx));
                }
            }
        }
        if h.iter().filter(|x| x.is_some()).count() < rows * cols / 3 {
            return None;
        }
        // Los de afuera (esquinas del rectángulo, fuera del borde): sobre el
        // plano tangente del triángulo de la zona más cercano, así siguen la
        // pendiente de la piel en el borde (promediar con los vecinos la
        // aplanaba justo donde la zona suele ser más empinada)
        let centers: Vec<([f64; 2], P3, P3)> = self
            .faces
            .iter()
            .filter(|&&f| mesh.face_areas[f as usize] > 1e-12)
            .map(|&f| {
                let c = mesh.face_centroids[f as usize];
                (uv(c), c, mesh.face_normals[f as usize])
            })
            .collect();
        let mut z = vec![0.0; rows * cols];
        for r in 0..rows {
            for k in 0..cols {
                let i = r * cols + k;
                if let Some(v) = h[i] {
                    z[i] = v;
                    continue;
                }
                let q = [lo[0] + (hi[0] - lo[0]) * k as f64 / (cols - 1) as f64, lo[1] + (hi[1] - lo[1]) * r as f64 / (rows - 1) as f64];
                let o = add(c, add(scale(u, q[0]), scale(v, q[1])));
                let (_, p, m) = centers.iter().min_by(|a, b| {
                    let da = (a.0[0] - q[0]).powi(2) + (a.0[1] - q[1]).powi(2);
                    let db = (b.0[0] - q[0]).powi(2) + (b.0[1] - q[1]).powi(2);
                    da.total_cmp(&db)
                })?;
                z[i] = dot(sub(*p, o), *m) / dot(n, *m);
            }
        }
        let points = (0..rows * cols)
            .map(|i| {
                let (r, k) = (i / cols, i % cols);
                let q = [lo[0] + (hi[0] - lo[0]) * k as f64 / (cols - 1) as f64, lo[1] + (hi[1] - lo[1]) * r as f64 / (rows - 1) as f64];
                add(c, add(add(scale(u, q[0]), scale(v, q[1])), scale(n, z[i])))
            })
            .collect();
        Some(ScanGrid { rows, cols, tol: 0.0, points })
    }
}

/// Distancia con signo a lo largo de `d` (unitaria) desde `o` hasta el
/// triángulo, si la recta lo atraviesa (Möller–Trumbore, en los dos sentidos).
fn ray_triangle(o: P3, d: P3, t: &[P3; 3]) -> Option<f64> {
    let e1 = sub(t[1], t[0]);
    let e2 = sub(t[2], t[0]);
    let p = cross(d, e2);
    let det = dot(e1, p);
    if det.abs() < 1e-14 {
        return None;
    }
    let inv = 1.0 / det;
    let s = sub(o, t[0]);
    let a = dot(s, p) * inv;
    if !(-1e-9..=1.0 + 1e-9).contains(&a) {
        return None;
    }
    let q = cross(s, e1);
    let b = dot(d, q) * inv;
    if b < -1e-9 || a + b > 1.0 + 1e-9 {
        return None;
    }
    Some(dot(e2, q) * inv)
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
