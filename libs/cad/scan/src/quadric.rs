//! Conos y toros: superficies de revolución ajustadas a una zona del escaneo.
//!
//! La normal de una superficie de revolución corta siempre a su eje, así que el
//! eje es la recta que cruza a todas las rectas normales (coordenadas de
//! Plücker: una condición lineal). Con el eje, cada punto pasa a (altura,
//! radio) y el perfil es una recta (cono) o un círculo (toro). Los puntos que
//! no calzan (una zona que se escapó a un plano vecino) se descartan y se
//! vuelve a ajustar.

use cad_model::geom::{P2, P3, add, cross, dot, norm, normalize, scale, sub};
use nalgebra::{SMatrix, SymmetricEigen};

use crate::mesh::ScanMesh;
use crate::outline::fit_circle;
use crate::pick::{PickError, PickOptions, unreliable_normal};

#[derive(Debug, Clone, PartialEq)]
pub struct ConeFit {
    pub apex: P3,
    /// Hacia donde se abre el cono
    pub direction: P3,
    /// Semiángulo (radianes)
    pub half_angle: f64,
    pub rms: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TorusFit {
    pub center: P3,
    pub axis: P3,
    pub major: f64,
    pub minor: f64,
    pub rms: f64,
}

/// Eje de revolución: un punto y la dirección, o `None` si la zona no
/// determina uno (plano, esfera, muy poca curvatura)
fn fit_axis(pts: &[P3], nrm: &[P3]) -> Option<(P3, P3)> {
    if pts.len() < 12 {
        return None;
    }
    let mut m = SMatrix::<f64, 6, 6>::zeros();
    for (p, n) in pts.iter().zip(nrm) {
        let mm = cross(*p, *n);
        let q = SMatrix::<f64, 6, 1>::from_column_slice(&[mm[0], mm[1], mm[2], n[0], n[1], n[2]]);
        m += q * q.transpose();
    }
    let eig = SymmetricEigen::new(m);
    let k = eig.eigenvalues.imin();
    // Si hay dos casi nulos el eje no está definido (esfera, cilindro corto…)
    let mut sorted: Vec<f64> = eig.eigenvalues.iter().copied().collect();
    sorted.sort_by(f64::total_cmp);
    if sorted[1] < sorted[5] * 1e-4 {
        return None;
    }
    let x = eig.eigenvectors.column(k);
    let a = [x[0], x[1], x[2]];
    if norm(a) < 1e-9 {
        return None;
    }
    let axis = normalize(a);
    let b = scale([x[3], x[4], x[5]], 1.0 / norm(a));
    let b = sub(b, scale(axis, dot(axis, b)));
    // Punto del eje más cercano al origen
    Some((cross(axis, b), axis))
}

/// (altura, radio) de cada punto respecto del eje
fn profile(pts: &[P3], origin: P3, axis: P3) -> Vec<P2> {
    pts.iter()
        .map(|p| {
            let d = sub(*p, origin);
            let h = dot(d, axis);
            [h, norm(sub(d, scale(axis, h)))]
        })
        .collect()
}

/// Recta ρ = k·h + b por mínimos cuadrados
fn fit_line(p: &[P2]) -> Option<(f64, f64)> {
    let n = p.len() as f64;
    let (sh, sr) = p.iter().fold((0.0, 0.0), |a, q| (a.0 + q[0], a.1 + q[1]));
    let (shh, shr) = p.iter().fold((0.0, 0.0), |a, q| (a.0 + q[0] * q[0], a.1 + q[0] * q[1]));
    let den = n * shh - sh * sh;
    if den.abs() < 1e-12 {
        return None;
    }
    let k = (n * shr - sh * sr) / den;
    Some((k, (sr - k * sh) / n))
}

fn rms_of(values: impl Iterator<Item = f64>) -> f64 {
    let (s, n) = values.fold((0.0, 0usize), |a, v| (a.0 + v * v, a.1 + 1));
    if n == 0 { f64::INFINITY } else { (s / n as f64).sqrt() }
}

/// Ajusta un modelo con descarte de los puntos que no calzan: `fit` devuelve
/// el modelo y la distancia de un punto a él
fn trimmed<M>(pts: &[P3], nrm: &[P3], tol: f64, fit: impl Fn(&[P3], &[P3]) -> Option<(M, Box<dyn Fn(P3) -> f64>)>) -> Option<(M, f64)> {
    let mut keep: Vec<usize> = (0..pts.len()).collect();
    let mut best = None;
    for _ in 0..5 {
        let (p, n): (Vec<P3>, Vec<P3>) = keep.iter().map(|&i| (pts[i], nrm[i])).unzip();
        let Some((model, dist)) = fit(&p, &n) else { break };
        let rms_kept = rms_of(keep.iter().map(|&i| dist(pts[i])));
        let limit = (rms_kept * 2.5).max(tol * 1.5);
        let next: Vec<usize> = (0..pts.len()).filter(|&i| dist(pts[i]).abs() <= limit).collect();
        best = Some((model, rms_kept));
        if next.len() == keep.len() || next.len() < pts.len() / 3 || next.len() < 12 {
            break;
        }
        keep = next;
    }
    best
}

pub fn fit_cone(pts: &[P3], nrm: &[P3], tol: f64) -> Option<ConeFit> {
    let (fit, rms) = trimmed(pts, nrm, tol, |p, n| {
        let (origin, axis) = fit_axis(p, n)?;
        let (k, b) = fit_line(&profile(p, origin, axis))?;
        // Entre 1° y 87° de semiángulo: fuera de eso es un cilindro o un plano
        if !(0.017..=19.0).contains(&k.abs()) {
            return None;
        }
        let cos = 1.0 / (1.0 + k * k).sqrt();
        let dist: Box<dyn Fn(P3) -> f64> = Box::new(move |q| {
            let d = sub(q, origin);
            let h = dot(d, axis);
            (norm(sub(d, scale(axis, h))) - (k * h + b)) * cos
        });
        let h_apex = -b / k;
        let direction = if k > 0.0 { axis } else { scale(axis, -1.0) };
        Some((ConeFit { apex: add(origin, scale(axis, h_apex)), direction, half_angle: k.abs().atan(), rms: 0.0 }, dist))
    })?;
    (rms <= tol).then_some(ConeFit { rms, ..fit })
}

pub fn fit_torus(pts: &[P3], nrm: &[P3], tol: f64) -> Option<TorusFit> {
    let (fit, rms) = trimmed(pts, nrm, tol, |p, n| {
        let (origin, axis) = fit_axis(p, n)?;
        let (c, r, _) = fit_circle(&profile(p, origin, axis))?;
        // El tubo no puede cruzar el eje (sería una esfera o un toro "de huso")
        if !(c[1] > r * 1.05 && r > 1e-6) {
            return None;
        }
        let dist: Box<dyn Fn(P3) -> f64> = Box::new(move |q| {
            let d = sub(q, origin);
            let h = dot(d, axis);
            let rho = norm(sub(d, scale(axis, h)));
            ((h - c[0]).powi(2) + (rho - c[1]).powi(2)).sqrt() - r
        });
        Some((TorusFit { center: add(origin, scale(axis, c[0])), axis, major: c[1], minor: r, rms: 0.0 }, dist))
    })?;
    (rms <= tol).then_some(TorusFit { rms, ..fit })
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ConePick {
    pub apex: P3,
    /// Hacia donde se abre el cono
    pub direction: P3,
    pub half_angle: f64,
    /// Distancia del vértice a donde empieza y termina la zona, a lo largo del eje
    pub start: f64,
    pub end: f64,
    pub rms: f64,
    pub faces: Vec<u32>,
    /// Normales hacia el eje: es un hueco (si no, un tronco macizo)
    pub hole: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TorusPick {
    pub center: P3,
    pub axis: P3,
    pub major: f64,
    pub minor: f64,
    pub rms: f64,
    pub faces: Vec<u32>,
}

/// Zona suave alrededor de `seed` (para ajustar) y sus muestras
fn smooth_zone(mesh: &ScanMesh, seed: u32, opts: &PickOptions, grow: impl Fn(&ScanMesh, u32, usize, &dyn Fn(u32, u32) -> bool) -> Vec<u32>) -> Result<(Vec<P3>, Vec<P3>), PickError> {
    if seed as usize >= mesh.face_count() {
        return Err(PickError::NoFace(seed));
    }
    let cos = opts.angle_threshold.max(15.0).to_radians().cos();
    let first = grow(mesh, seed, opts.max_faces, &|f, g| dot(mesh.face_normals[f as usize], mesh.face_normals[g as usize]) >= cos);
    let (pts, nrm) = mesh.sample_centroids(&first, 20_000);
    if pts.len() < 12 {
        return Err(PickError::TooSmall);
    }
    Ok((pts, nrm))
}

pub fn pick_cone(mesh: &ScanMesh, seed: u32, opts: &PickOptions) -> Result<ConePick, PickError> {
    let tol = opts.tol(mesh);
    let (pts, nrm) = smooth_zone(mesh, seed, opts, crate::pick::grow_dyn)?;
    let fit = fit_cone(&pts, &nrm, tol).ok_or(PickError::NoFit("un cono"))?;
    let (apex, dir, tan) = (fit.apex, fit.direction, fit.half_angle.tan());
    let dist = |p: P3| {
        let d = sub(p, apex);
        let h = dot(d, dir);
        (norm(sub(d, scale(dir, h))) - h * tan).abs() / (1.0 + tan * tan).sqrt()
    };
    let faces = crate::pick::grow_dyn(mesh, seed, opts.max_faces, &|_, g| {
        let gi = g as usize;
        mesh.triangles[gi].iter().all(|&v| dist(mesh.vertices[v as usize]) <= tol * 3.0) || (unreliable_normal(mesh, gi, tol) && dist(mesh.face_centroids[gi]) <= tol * 3.0)
    });
    if faces.len() < 3 {
        return Err(PickError::NoFit("un cono"));
    }
    let (mut t0, mut t1) = (f64::MAX, f64::MIN);
    let mut inward = 0.0;
    let mut sq = 0.0;
    for &f in &faces {
        let fi = f as usize;
        let c = mesh.face_centroids[fi];
        let d = sub(c, apex);
        let h = dot(d, dir);
        let radial = sub(d, scale(dir, h));
        if norm(radial) > 1e-9 {
            inward += dot(mesh.face_normals[fi], normalize(radial)) * mesh.face_areas[fi];
        }
        sq += dist(c).powi(2);
        for &v in &mesh.triangles[fi] {
            let t = dot(sub(mesh.vertices[v as usize], apex), dir);
            t0 = t0.min(t);
            t1 = t1.max(t);
        }
    }
    Ok(ConePick { apex, direction: dir, half_angle: fit.half_angle, start: t0.max(0.0), end: t1, rms: (sq / faces.len() as f64).sqrt(), faces, hole: inward < 0.0 })
}

pub fn pick_torus(mesh: &ScanMesh, seed: u32, opts: &PickOptions) -> Result<TorusPick, PickError> {
    let tol = opts.tol(mesh);
    let (pts, nrm) = smooth_zone(mesh, seed, opts, crate::pick::grow_dyn)?;
    let fit = fit_torus(&pts, &nrm, tol).ok_or(PickError::NoFit("un toro"))?;
    let dist = |p: P3| {
        let d = sub(p, fit.center);
        let h = dot(d, fit.axis);
        let rho = norm(sub(d, scale(fit.axis, h)));
        (((rho - fit.major).powi(2) + h * h).sqrt() - fit.minor).abs()
    };
    let faces = crate::pick::grow_dyn(mesh, seed, opts.max_faces, &|_, g| {
        let gi = g as usize;
        mesh.triangles[gi].iter().all(|&v| dist(mesh.vertices[v as usize]) <= tol * 3.0) || (unreliable_normal(mesh, gi, tol) && dist(mesh.face_centroids[gi]) <= tol * 3.0)
    });
    if faces.len() < 3 {
        return Err(PickError::NoFit("un toro"));
    }
    let rms = (faces.iter().map(|&f| dist(mesh.face_centroids[f as usize]).powi(2)).sum::<f64>() / faces.len() as f64).sqrt();
    Ok(TorusPick { center: fit.center, axis: fit.axis, major: fit.major, minor: fit.minor, rms, faces })
}
