//! Elegir una zona del escaneo con un clic y ajustarle una primitiva.

use std::collections::{HashMap, HashSet, VecDeque};

use cad_model::geom::{P2, P3, Plane, add, cross, dot, norm, normalize, scale, sub};
use cad_solver::fitting::{self, PrimitiveParams};
use serde::{Deserialize, Serialize};

use crate::mesh::ScanMesh;
use crate::outline::simplify_closed;
use crate::regions_area;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PickOptions {
    /// Ángulo máximo (grados) entre normales para crecer la región.
    pub angle_threshold: f64,
    /// Tope de caras de la región.
    pub max_faces: usize,
    /// Banda de tolerancia del ajuste (mm); `None` = 0,2 % de la diagonal.
    pub tolerance: Option<f64>,
}

impl Default for PickOptions {
    fn default() -> Self {
        Self { angle_threshold: 10.0, max_faces: 500_000, tolerance: None }
    }
}

impl PickOptions {
    pub(crate) fn tol(&self, mesh: &ScanMesh) -> f64 {
        self.tolerance.unwrap_or(mesh.diagonal() * 0.002).max(1e-9)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanePick {
    /// Plano ajustado; la normal sale de la superficie escaneada.
    pub plane: Plane,
    pub rms: f64,
    pub faces: Vec<u32>,
    pub area: f64,
    /// Contornos de la zona en coordenadas del plano (el más grande primero).
    pub boundary: Vec<Vec<P2>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CylinderPick {
    /// Punto del eje donde empieza la zona y dirección del eje.
    pub origin: P3,
    pub direction: P3,
    pub radius: f64,
    /// Largo de la zona a lo largo del eje.
    pub length: f64,
    pub rms: f64,
    pub faces: Vec<u32>,
    /// Normales hacia el eje: es un agujero (si no, un tetón/eje).
    pub hole: bool,
    /// Fracción de la vuelta que cubre la zona (1 = cilindro completo).
    pub coverage: f64,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PickError {
    #[error("el triángulo {0} no existe")]
    NoFace(u32),
    #[error("la zona es demasiado chica para ajustar")]
    TooSmall,
    #[error("la zona no parece {0}")]
    NoFit(&'static str),
}

/// Cilindro por normales: el eje es la dirección a la que todas las normales
/// son perpendiculares (autovector menor de Σ n·nᵀ); el corte perpendicular es
/// un círculo. No depende del orden de los puntos.
pub fn fit_cylinder_normals(pts: &[P3], normals: &[P3]) -> fitting::FitResult {
    use nalgebra::{Matrix3, SymmetricEigen, Vector3};
    let fail = fitting::FitResult {
        primitive_type: fitting::PrimitiveType::Unknown,
        parameters: PrimitiveParams::None,
        rms_error: f64::INFINITY,
        inlier_ratio: 0.0,
    };
    if pts.len() < 6 {
        return fail;
    }
    let mut m = Matrix3::zeros();
    for n in normals {
        let v = Vector3::from(*n);
        m += v * v.transpose();
    }
    let eig = SymmetricEigen::new(m);
    let k = eig.eigenvalues.imin();
    let axis = normalize([eig.eigenvectors[(0, k)], eig.eigenvectors[(1, k)], eig.eigenvectors[(2, k)]]);
    let u = normalize(if axis[0].abs() < 0.9 { cross(axis, [1.0, 0.0, 0.0]) } else { cross(axis, [0.0, 1.0, 0.0]) });
    let v = cross(axis, u);
    let flat: Vec<P2> = pts.iter().map(|p| [dot(*p, u), dot(*p, v)]).collect();
    let Some((c, r, rms)) = crate::outline::fit_circle(&flat) else { return fail };
    // Punto del eje a la altura media de la nube
    let h = pts.iter().map(|p| dot(*p, axis)).sum::<f64>() / pts.len() as f64;
    let point = add(add(scale(u, c[0]), scale(v, c[1])), scale(axis, h));
    fitting::FitResult {
        primitive_type: fitting::PrimitiveType::Cylinder,
        parameters: PrimitiveParams::Cylinder { point, axis, radius: r },
        rms_error: rms,
        inlier_ratio: 1.0,
    }
}

/// Cara con normal poco confiable: su altura es comparable al ruido, así que
/// el ruido la puede inclinar cualquier cosa (típico de teselados de CAD).
pub(crate) fn unreliable_normal(mesh: &ScanMesh, f: usize, noise: f64) -> bool {
    let [a, b, c] = mesh.triangles[f].map(|i| mesh.vertices[i as usize]);
    let longest = norm(sub(a, b)).max(norm(sub(b, c))).max(norm(sub(c, a)));
    longest > 0.0 && 2.0 * mesh.face_areas[f] / longest < noise * 2.0
}

/// Crecimiento por vecinos desde `seed` mientras `accept(cara, vecina)` lo permita.
fn grow(mesh: &ScanMesh, seed: u32, max: usize, accept: impl Fn(u32, u32) -> bool) -> Vec<u32> {
    let mut seen = HashSet::from([seed]);
    let mut queue = VecDeque::from([seed]);
    let mut out = Vec::new();
    while let Some(f) = queue.pop_front() {
        out.push(f);
        if out.len() >= max {
            break;
        }
        for &g in &mesh.adjacency[f as usize] {
            if !seen.contains(&g) && accept(f, g) {
                seen.insert(g);
                queue.push_back(g);
            }
        }
    }
    out
}

pub fn pick_plane(mesh: &ScanMesh, seed: u32, opts: &PickOptions) -> Result<PlanePick, PickError> {
    if seed as usize >= mesh.face_count() {
        return Err(PickError::NoFace(seed));
    }
    let tol = opts.tol(mesh);
    let cos = opts.angle_threshold.to_radians().cos();
    let n0 = mesh.face_normals[seed as usize];
    let first = grow(mesh, seed, opts.max_faces, |_, g| dot(mesh.face_normals[g as usize], n0) >= cos);
    let (pts, nrm) = mesh.sample_faces(&first, 20_000);
    if pts.len() < 3 {
        return Err(PickError::TooSmall);
    }
    let fit = fitting::fit_plane_ransac(&pts, tol, 100);
    let PrimitiveParams::Plane { point, normal } = fit.parameters else {
        return Err(PickError::NoFit("un plano"));
    };
    // Orientar la normal como las del escaneo
    let avg = nrm.iter().fold([0.0; 3], |a, n| add(a, *n));
    let mut n = normalize(normal);
    if dot(n, avg) < 0.0 {
        n = scale(n, -1.0);
    }

    // Segunda pasada contra el plano ajustado: más precisa que la normal semilla
    // Un triángulo largo y fino con ruido tiene una normal cualquiera: se
    // acepta si sus tres vértices están en la banda, aunque la normal no cuadre.
    let in_band = |p: P3| dot(sub(p, point), n).abs() <= tol * 3.0;
    let faces = grow(mesh, seed, opts.max_faces, |_, g| {
        let gi = g as usize;
        let verts_in = mesh.triangles[gi].iter().all(|&v| in_band(mesh.vertices[v as usize]));
        verts_in && (dot(mesh.face_normals[gi], n) >= cos || unreliable_normal(mesh, gi, tol))
    });
    let centroid = faces.iter().fold([0.0; 3], |a, &f| add(a, mesh.face_centroids[f as usize]));
    let centroid = scale(centroid, 1.0 / faces.len() as f64);
    let origin = sub(centroid, scale(n, dot(sub(centroid, point), n)));
    let plane = Plane::from_normal(origin, n);

    let rms = (faces.iter().map(|&f| dot(sub(mesh.face_centroids[f as usize], origin), n).powi(2)).sum::<f64>()
        / faces.len() as f64)
        .sqrt();
    let area = faces.iter().map(|&f| mesh.face_areas[f as usize]).sum();
    let mut boundary: Vec<Vec<P2>> = boundary_loops(mesh, &faces)
        .into_iter()
        .map(|l| simplify_closed(&l.iter().map(|&p| plane.to_local(p)).collect::<Vec<_>>(), tol))
        .filter(|l| l.len() >= 3)
        .collect();
    boundary.sort_by(|a, b| regions_area(b).total_cmp(&regions_area(a)));
    Ok(PlanePick { plane, rms, faces, area, boundary })
}

pub fn pick_cylinder(mesh: &ScanMesh, seed: u32, opts: &PickOptions) -> Result<CylinderPick, PickError> {
    if seed as usize >= mesh.face_count() {
        return Err(PickError::NoFace(seed));
    }
    let tol = opts.tol(mesh);
    // Superficie suave: el ángulo se mide entre vecinas, no contra la semilla
    let cos = opts.angle_threshold.max(15.0).to_radians().cos();
    let first = grow(mesh, seed, opts.max_faces, |f, g| {
        dot(mesh.face_normals[f as usize], mesh.face_normals[g as usize]) >= cos
    });
    let (pts, nrm) = mesh.sample_faces(&first, 20_000);
    if pts.len() < 8 {
        return Err(PickError::TooSmall);
    }
    // RANSAC primero; el heredado depende del orden de los puntos y a veces no
    // devuelve nada: entonces el ajuste por normales (eje = espacio nulo).
    let fits = [fit_cylinder_normals(&pts, &nrm), fitting::fit_cylinder_ransac(&pts, &nrm, tol, 100)];
    let Some((point, axis, radius)) = fits
        .iter()
        .filter_map(|f| match f.parameters {
            PrimitiveParams::Cylinder { point, axis, radius } if f.rms_error <= tol * 3.0 => {
                Some((point, axis, radius, f.rms_error))
            }
            _ => None,
        })
        .min_by(|a, b| a.3.total_cmp(&b.3))
        .map(|(p, a, r, _)| (p, a, r))
    else {
        return Err(PickError::NoFit("un cilindro"));
    };
    let axis = normalize(axis);
    let radial = |p: P3| {
        let d = sub(p, point);
        sub(d, scale(axis, dot(d, axis)))
    };
    // Refinar: caras sobre la superficie con normal radial
    let max_axial = (10f64).to_radians().sin().max(1.0 - cos);
    let on_surface = |p: P3| (norm(radial(p)) - radius).abs() <= tol * 3.0;
    let faces = grow(mesh, seed, opts.max_faces, |_, g| {
        let gi = g as usize;
        let verts_in = mesh.triangles[gi].iter().all(|&v| on_surface(mesh.vertices[v as usize]));
        verts_in && (dot(mesh.face_normals[gi], axis).abs() <= max_axial.max(0.2) || unreliable_normal(mesh, gi, tol))
    });
    if faces.len() < 3 {
        return Err(PickError::NoFit("un cilindro"));
    }
    let mut inward = 0.0;
    let (mut t0, mut t1) = (f64::MAX, f64::MIN);
    let mut sq = 0.0;
    let mut sectors = [false; 36];
    let u = normalize(if axis[0].abs() < 0.9 { cross(axis, [1.0, 0.0, 0.0]) } else { cross(axis, [0.0, 1.0, 0.0]) });
    let v = cross(axis, u);
    for &f in &faces {
        let fi = f as usize;
        let c = mesh.face_centroids[fi];
        let r = radial(c);
        inward += dot(mesh.face_normals[fi], normalize(r)) * mesh.face_areas[fi];
        sq += (norm(r) - radius).powi(2);
        for &vi in &mesh.triangles[fi] {
            let t = dot(sub(mesh.vertices[vi as usize], point), axis);
            t0 = t0.min(t);
            t1 = t1.max(t);
        }
        let ang = dot(r, v).atan2(dot(r, u));
        let s = (((ang + std::f64::consts::PI) / (2.0 * std::f64::consts::PI)) * 36.0) as usize;
        sectors[s.min(35)] = true;
    }
    Ok(CylinderPick {
        origin: add(point, scale(axis, t0)),
        direction: axis,
        radius,
        length: t1 - t0,
        rms: (sq / faces.len() as f64).sqrt(),
        hole: inward < 0.0,
        coverage: sectors.iter().filter(|&&s| s).count() as f64 / 36.0,
        faces,
    })
}

/// Lazos de borde de un conjunto de caras (aristas que no comparten con otra
/// cara del conjunto), como polilíneas 3D cerradas.
pub fn boundary_loops(mesh: &ScanMesh, faces: &[u32]) -> Vec<Vec<P3>> {
    let mut count: HashMap<(u32, u32), (u32, u32, usize)> = HashMap::new();
    for &f in faces {
        let t = mesh.triangles[f as usize];
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            let e = count.entry((a.min(b), a.max(b))).or_insert((a, b, 0));
            e.2 += 1;
        }
    }
    // Aristas de borde con su sentido original (a → b)
    let mut next: HashMap<u32, Vec<u32>> = HashMap::new();
    for &(a, b, n) in count.values() {
        if n == 1 {
            next.entry(a).or_default().push(b);
        }
    }
    let mut loops = Vec::new();
    while let Some((&start, _)) = next.iter().find(|(_, v)| !v.is_empty()) {
        let mut l = vec![mesh.vertices[start as usize]];
        let mut cur = start;
        while let Some(n) = next.get_mut(&cur).and_then(|v| v.pop()) {
            if n == start {
                break;
            }
            l.push(mesh.vertices[n as usize]);
            cur = n;
            if l.len() > faces.len() * 3 {
                break;
            }
        }
        if l.len() >= 3 {
            loops.push(l);
        }
    }
    loops
}
