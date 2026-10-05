//! Ajuste de primitivas geométricas sobre nubes de puntos.
//!
//! Versión robusta:
//! - Plane: SVD directo + variante RANSAC
//! - Sphere: lineal algebraico + RANSAC
//! - Cylinder: eje desde nullspace de normales + Gauss-Newton + RANSAC
//! - `fit_best`: honesto, devuelve `Unknown` si ningún modelo cumple el umbral

use nalgebra::{DMatrix, Matrix3, Vector3};

/// Resultado del ajuste de una primitiva.
#[derive(Debug, Clone)]
pub struct FitResult {
    pub primitive_type: PrimitiveType,
    pub parameters: PrimitiveParams,
    pub rms_error: f64,
    pub inlier_ratio: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PrimitiveType {
    Plane,
    Cylinder,
    Sphere,
    Unknown,
}

#[derive(Debug, Clone)]
pub enum PrimitiveParams {
    Plane { point: [f64; 3], normal: [f64; 3] },
    Cylinder { point: [f64; 3], axis: [f64; 3], radius: f64 },
    Sphere { center: [f64; 3], radius: f64 },
    None,
}

fn unknown_result() -> FitResult {
    FitResult {
        primitive_type: PrimitiveType::Unknown,
        parameters: PrimitiveParams::None,
        rms_error: f64::INFINITY,
        inlier_ratio: 0.0,
    }
}

// ---------------------------------------------------------------------------
//  PLANE
// ---------------------------------------------------------------------------

/// Ajustar plano por SVD (centroide + normal = vector singular menor).
pub fn fit_plane(vertices: &[[f64; 3]]) -> FitResult {
    let n = vertices.len();
    if n < 3 {
        return unknown_result();
    }
    let (cx, cy, cz) = centroid(vertices);
    let mat = DMatrix::from_fn(n, 3, |i, j| match j {
        0 => vertices[i][0] - cx,
        1 => vertices[i][1] - cy,
        _ => vertices[i][2] - cz,
    });
    let svd = mat.svd(false, true);
    let vt = match svd.v_t {
        Some(v) => v,
        None => return unknown_result(),
    };
    let normal = Vector3::new(vt[(2, 0)], vt[(2, 1)], vt[(2, 2)]).normalize();

    let mut sum_sq = 0.0;
    for v in vertices {
        let d = (v[0] - cx) * normal.x + (v[1] - cy) * normal.y + (v[2] - cz) * normal.z;
        sum_sq += d * d;
    }
    let rms = (sum_sq / n as f64).sqrt();

    FitResult {
        primitive_type: PrimitiveType::Plane,
        parameters: PrimitiveParams::Plane {
            point: [cx, cy, cz],
            normal: [normal.x, normal.y, normal.z],
        },
        rms_error: rms,
        inlier_ratio: 1.0,
    }
}

/// Plane RANSAC: sortea triples, cuenta inliers, refit con los inliers.
pub fn fit_plane_ransac(vertices: &[[f64; 3]], inlier_dist: f64, iterations: usize) -> FitResult {
    let n = vertices.len();
    if n < 3 {
        return unknown_result();
    }
    let mut rng = LcgRng::seed_from_data(vertices);
    let mut best_inliers: Vec<usize> = Vec::new();
    let mut best_count = 0usize;

    for _ in 0..iterations {
        let (a, b, c) = (rng.range(n), rng.range(n), rng.range(n));
        if a == b || b == c || a == c { continue; }
        let p0 = Vector3::from_row_slice(&vertices[a]);
        let p1 = Vector3::from_row_slice(&vertices[b]);
        let p2 = Vector3::from_row_slice(&vertices[c]);
        let n_vec = (p1 - p0).cross(&(p2 - p0));
        let nn = n_vec.norm();
        if nn < 1e-10 { continue; }
        let normal = n_vec / nn;
        let d_off = -normal.dot(&p0);

        let mut count = 0usize;
        let mut local: Vec<usize> = Vec::new();
        for (i, v) in vertices.iter().enumerate() {
            let p = Vector3::from_row_slice(v);
            let dist = (normal.dot(&p) + d_off).abs();
            if dist <= inlier_dist {
                local.push(i);
                count += 1;
            }
        }
        if count > best_count {
            best_count = count;
            best_inliers = local;
        }
    }

    if best_count < 3 { return fit_plane(vertices); }
    let inlier_verts: Vec<[f64; 3]> = best_inliers.iter().map(|&i| vertices[i]).collect();
    let mut refit = fit_plane(&inlier_verts);
    refit.inlier_ratio = best_count as f64 / n as f64;
    refit
}

// ---------------------------------------------------------------------------
//  SPHERE
// ---------------------------------------------------------------------------

pub fn fit_sphere(vertices: &[[f64; 3]]) -> FitResult {
    let n = vertices.len();
    if n < 4 {
        return unknown_result();
    }
    let a_mat = DMatrix::from_fn(n, 4, |i, j| match j {
        0 => vertices[i][0],
        1 => vertices[i][1],
        2 => vertices[i][2],
        _ => 1.0,
    });
    let b_vec = DMatrix::from_fn(n, 1, |i, _| {
        vertices[i][0].powi(2) + vertices[i][1].powi(2) + vertices[i][2].powi(2)
    });
    let ata = a_mat.transpose() * &a_mat;
    let atb = a_mat.transpose() * &b_vec;
    let x = match ata.lu().solve(&atb) {
        Some(x) => x,
        None => return unknown_result(),
    };
    let cx = x[(0, 0)] / 2.0;
    let cy = x[(1, 0)] / 2.0;
    let cz = x[(2, 0)] / 2.0;
    let radius_sq = x[(3, 0)] + cx * cx + cy * cy + cz * cz;
    if radius_sq <= 0.0 { return unknown_result(); }
    let radius = radius_sq.sqrt();

    let mut sum_sq = 0.0;
    for v in vertices {
        let dx = v[0] - cx; let dy = v[1] - cy; let dz = v[2] - cz;
        let err = (dx*dx + dy*dy + dz*dz).sqrt() - radius;
        sum_sq += err * err;
    }
    let rms = (sum_sq / n as f64).sqrt();

    FitResult {
        primitive_type: PrimitiveType::Sphere,
        parameters: PrimitiveParams::Sphere { center: [cx, cy, cz], radius },
        rms_error: rms,
        inlier_ratio: 1.0,
    }
}

pub fn fit_sphere_ransac(vertices: &[[f64; 3]], inlier_dist: f64, iterations: usize) -> FitResult {
    let n = vertices.len();
    if n < 4 { return unknown_result(); }
    let mut rng = LcgRng::seed_from_data(vertices);
    let mut best_inliers: Vec<usize> = Vec::new();
    let mut best_count = 0usize;

    for _ in 0..iterations {
        let mut idxs = [0usize; 4];
        for k in 0..4 { idxs[k] = rng.range(n); }
        if !all_unique(&idxs) { continue; }
        let sample: Vec<[f64; 3]> = idxs.iter().map(|&i| vertices[i]).collect();
        let s = fit_sphere(&sample);
        let (sc, sr) = match &s.parameters {
            PrimitiveParams::Sphere { center, radius } => (*center, *radius),
            _ => continue,
        };
        if !sr.is_finite() || sr <= 0.0 { continue; }
        let center = Vector3::from_row_slice(&sc);
        let mut count = 0usize;
        let mut local: Vec<usize> = Vec::new();
        for (i, v) in vertices.iter().enumerate() {
            let p = Vector3::from_row_slice(v);
            let d = ((p - center).norm() - sr).abs();
            if d <= inlier_dist {
                local.push(i);
                count += 1;
            }
        }
        if count > best_count { best_count = count; best_inliers = local; }
    }

    if best_count < 4 { return fit_sphere(vertices); }
    let inlier_verts: Vec<[f64; 3]> = best_inliers.iter().map(|&i| vertices[i]).collect();
    let mut refit = fit_sphere(&inlier_verts);
    refit.inlier_ratio = best_count as f64 / n as f64;
    refit
}

// ---------------------------------------------------------------------------
//  CYLINDER
// ---------------------------------------------------------------------------

/// Cilindro robusto: axis desde nullspace de normales (n_i · axis = 0) +
/// proyección a círculo + refinamiento Gauss-Newton.
pub fn fit_cylinder_robust(
    vertices: &[[f64; 3]],
    normals: &[[f64; 3]],
) -> FitResult {
    let n = vertices.len();
    if n < 5 || normals.len() != n { return fit_cylinder(vertices); }

    // Eje = vector singular menor de la matriz de normales (n_i · axis ≈ 0).
    let nmat = DMatrix::from_fn(n, 3, |i, j| normals[i][j]);
    let svd = nmat.svd(false, true);
    let vt = match svd.v_t { Some(v) => v, None => return fit_cylinder(vertices) };
    let axis = Vector3::new(vt[(2, 0)], vt[(2, 1)], vt[(2, 2)]).normalize();

    // Proyectar al plano perpendicular y ajustar círculo.
    let (cx, cy, cz) = centroid(vertices);
    let u = if axis.x.abs() < 0.9 {
        axis.cross(&Vector3::new(1.0, 0.0, 0.0)).normalize()
    } else {
        axis.cross(&Vector3::new(0.0, 1.0, 0.0)).normalize()
    };
    let v_basis = axis.cross(&u);

    let mut projected: Vec<[f64; 2]> = Vec::with_capacity(n);
    for vert in vertices {
        let p = Vector3::new(vert[0] - cx, vert[1] - cy, vert[2] - cz);
        projected.push([p.dot(&u), p.dot(&v_basis)]);
    }
    let (cu, cv, radius0) = fit_circle_2d(&projected);
    let center0 = Vector3::new(cx, cy, cz) + u * cu + v_basis * cv;

    // Gauss-Newton: refinar (center, axis, radius) sobre f_i = ||(p_i - c) - ((p_i-c)·a)a|| - r
    let (center, axis_r, radius) = gauss_newton_cylinder(vertices, center0, axis, radius0, 12);

    let rms = cylinder_rms(vertices, &center, &axis_r, radius);

    FitResult {
        primitive_type: PrimitiveType::Cylinder,
        parameters: PrimitiveParams::Cylinder {
            point: [center.x, center.y, center.z],
            axis: [axis_r.x, axis_r.y, axis_r.z],
            radius,
        },
        rms_error: rms,
        inlier_ratio: 1.0,
    }
}

/// Versión legacy sin normales (PCA + círculo). Mantenida como fallback.
pub fn fit_cylinder(vertices: &[[f64; 3]]) -> FitResult {
    let n = vertices.len();
    if n < 5 { return unknown_result(); }
    let (cx, cy, cz) = centroid(vertices);
    let mat = DMatrix::from_fn(n, 3, |i, j| match j {
        0 => vertices[i][0] - cx,
        1 => vertices[i][1] - cy,
        _ => vertices[i][2] - cz,
    });
    let svd = mat.svd(false, true);
    let vt = match svd.v_t { Some(v) => v, None => return unknown_result() };
    let axis = Vector3::new(vt[(0, 0)], vt[(0, 1)], vt[(0, 2)]).normalize();

    let u = if axis.x.abs() < 0.9 {
        axis.cross(&Vector3::new(1.0, 0.0, 0.0)).normalize()
    } else {
        axis.cross(&Vector3::new(0.0, 1.0, 0.0)).normalize()
    };
    let v_basis = axis.cross(&u);
    let mut projected: Vec<[f64; 2]> = Vec::with_capacity(n);
    for vert in vertices {
        let p = Vector3::new(vert[0] - cx, vert[1] - cy, vert[2] - cz);
        projected.push([p.dot(&u), p.dot(&v_basis)]);
    }
    let (cu, cv, radius0) = fit_circle_2d(&projected);
    let center0 = Vector3::new(cx, cy, cz) + u * cu + v_basis * cv;

    // GN también en versión sin normales
    let (center, axis_r, radius) = gauss_newton_cylinder(vertices, center0, axis, radius0, 12);
    let rms = cylinder_rms(vertices, &center, &axis_r, radius);

    FitResult {
        primitive_type: PrimitiveType::Cylinder,
        parameters: PrimitiveParams::Cylinder {
            point: [center.x, center.y, center.z],
            axis: [axis_r.x, axis_r.y, axis_r.z],
            radius,
        },
        rms_error: rms,
        inlier_ratio: 1.0,
    }
}

/// RANSAC cilindro: arranca desde fit_cylinder_robust (nullspace + GN sobre todos los puntos),
/// luego refina iterativamente sobre inliers. Estrategia "trimmed least squares" más que
/// RANSAC clásico, pero más estable porque el init ya es bueno.
pub fn fit_cylinder_ransac(
    vertices: &[[f64; 3]],
    normals: &[[f64; 3]],
    inlier_dist: f64,
    _iterations: usize,
) -> FitResult {
    let n = vertices.len();
    if n < 5 || normals.len() != n {
        return fit_cylinder(vertices);
    }

    // Init: fit nullspace + GN sobre todos los puntos (sesgado por outliers pero útil)
    let mut current = fit_cylinder_robust(vertices, normals);
    let (mut center, mut axis, mut radius) = match &current.parameters {
        PrimitiveParams::Cylinder { point, axis, radius } => (
            Vector3::from_row_slice(point),
            Vector3::from_row_slice(axis),
            *radius,
        ),
        _ => return current,
    };

    // Refinar con tightening progresivo: comenzar con threshold amplio (sesgo por outliers
     // puede hacer que el radio inicial difiera mucho del real), reducir cada pasada.
    let mut best_count = 0usize;
    let max_passes = 8;
    for pass in 0..max_passes {
        // threshold: empieza en 8x → llega a inlier_dist al final
        let t = 8.0_f64.powf(1.0 - pass as f64 / (max_passes as f64 - 1.0).max(1.0));
        let pass_thresh = inlier_dist * t;
        let mut local: Vec<usize> = Vec::new();
        for (k, v) in vertices.iter().enumerate() {
            let p = Vector3::from_row_slice(v);
            let to_p = p - center;
            let along = to_p.dot(&axis);
            let perp = to_p - axis * along;
            let d = (perp.norm() - radius).abs();
            if d <= pass_thresh { local.push(k); }
        }
        // Solo en la pasada final cuenta para inlier_ratio
        if pass == max_passes - 1 && local.len() > best_count { best_count = local.len(); }
        if local.len() < 5 { break; }

        let inlier_verts: Vec<[f64; 3]> = local.iter().map(|&i| vertices[i]).collect();
        let inlier_normals: Vec<[f64; 3]> = local.iter().map(|&i| normals[i]).collect();
        let refit = fit_cylinder_robust(&inlier_verts, &inlier_normals);
        if let PrimitiveParams::Cylinder { point, axis: a, radius: r } = &refit.parameters {
            center = Vector3::from_row_slice(point);
            axis = Vector3::from_row_slice(a);
            radius = *r;
        } else { break; }
        current = refit;
    }

    // Recontar inliers final con el fit ya refinado
    let mut final_inliers = 0usize;
    for v in vertices {
        let p = Vector3::from_row_slice(v);
        let to_p = p - center;
        let along = to_p.dot(&axis);
        let perp = to_p - axis * along;
        let d = (perp.norm() - radius).abs();
        if d <= inlier_dist { final_inliers += 1; }
    }
    current.inlier_ratio = final_inliers.max(best_count) as f64 / n as f64;
    current
}

/// Refinamiento Gauss-Newton del cilindro.
/// Parámetros: center(3), axis(3, normalizado), radius(1) → 7 params, axis con restricción ||a||=1.
/// Linealización numérica (Jacobiano por diferencias finitas pequeñas).
fn gauss_newton_cylinder(
    vertices: &[[f64; 3]],
    mut center: Vector3<f64>,
    mut axis: Vector3<f64>,
    mut radius: f64,
    max_iter: usize,
) -> (Vector3<f64>, Vector3<f64>, f64) {
    let n = vertices.len();
    if n < 5 { return (center, axis, radius); }

    for _ in 0..max_iter {
        axis = axis.normalize();
        // Residuos f_i = ||perp_i|| - r
        let mut f = DMatrix::<f64>::zeros(n, 1);
        let mut j_mat = DMatrix::<f64>::zeros(n, 7);
        for i in 0..n {
            let p = Vector3::from_row_slice(&vertices[i]);
            let to_p = p - center;
            let along = to_p.dot(&axis);
            let perp = to_p - axis * along;
            let perp_norm = perp.norm().max(1e-12);
            f[(i, 0)] = perp_norm - radius;

            // d(perp)/d(center) = -I + a aᵀ  (porque perp = (p-c) - ((p-c)·a)a, ∂perp/∂c = -I + a aᵀ)
            // d(||perp||)/d(c) = (perp / ||perp||) · (-I + a aᵀ)
            let u = perp / perp_norm;
            let dc = -u + axis * (axis.dot(&u));
            j_mat[(i, 0)] = dc.x;
            j_mat[(i, 1)] = dc.y;
            j_mat[(i, 2)] = dc.z;

            // d(perp)/d(a) = -along * I - (p-c) ⊗ a + 2*along*a ⊗ a  (parcial)
            // Simpler: d||perp||/da = (perp/||perp||) · d(perp)/da con d(perp)/da = -along*I - (a⊗(p-c)) traza... usa numérico para esto.
            let eps = 1e-6;
            for k in 0..3 {
                let mut a_p = axis;
                a_p[k] += eps;
                let a_p = a_p.normalize();
                let along_p = to_p.dot(&a_p);
                let perp_p = to_p - a_p * along_p;
                j_mat[(i, 3 + k)] = (perp_p.norm() - perp_norm) / eps;
            }
            j_mat[(i, 6)] = -1.0; // d(f)/dr
        }

        let jt = j_mat.transpose();
        let jtj = &jt * &j_mat;
        let jtf = &jt * &f;
        let delta = match jtj.lu().solve(&jtf) {
            Some(d) => d,
            None => break,
        };
        // Limit step
        let step_norm = delta.norm();
        let scale = if step_norm > 1.0 { 1.0 / step_norm } else { 1.0 };
        center.x -= delta[(0, 0)] * scale;
        center.y -= delta[(1, 0)] * scale;
        center.z -= delta[(2, 0)] * scale;
        axis.x   -= delta[(3, 0)] * scale;
        axis.y   -= delta[(4, 0)] * scale;
        axis.z   -= delta[(5, 0)] * scale;
        radius   -= delta[(6, 0)] * scale;
        axis = axis.normalize();
        if step_norm * scale < 1e-9 { break; }
    }
    (center, axis, radius.max(1e-9))
}

fn cylinder_rms(vertices: &[[f64; 3]], center: &Vector3<f64>, axis: &Vector3<f64>, radius: f64) -> f64 {
    let mut sum_sq = 0.0;
    for v in vertices {
        let p = Vector3::from_row_slice(v);
        let to_p = p - center;
        let along = to_p.dot(axis);
        let perp = to_p - axis * along;
        let d = perp.norm() - radius;
        sum_sq += d * d;
    }
    (sum_sq / vertices.len() as f64).sqrt()
}

// ---------------------------------------------------------------------------
//  BEST FIT
// ---------------------------------------------------------------------------

/// Selección honesta: prueba plano/esfera/cilindro y devuelve la primitiva de
/// menor error que cumpla el umbral. Si ninguna cumple, devuelve `Unknown`.
pub fn fit_best(vertices: &[[f64; 3]], error_threshold: f64) -> FitResult {
    fit_best_with_normals(vertices, None, error_threshold)
}

pub fn fit_best_with_normals(
    vertices: &[[f64; 3]],
    normals: Option<&[[f64; 3]]>,
    error_threshold: f64,
) -> FitResult {
    let plane = fit_plane_ransac(vertices, error_threshold, 80);
    let mut sphere = fit_sphere_ransac(vertices, error_threshold, 80);
    let mut cylinder = match normals {
        Some(ns) => fit_cylinder_ransac(vertices, ns, error_threshold, 80),
        None => fit_cylinder(vertices),
    };

    // Sanity checks: rechazar primitivas cuya geometría no esté bien soportada
    // por la nube de puntos (patches curvos locales que algebraicamente "fitean"
    // pero no representan una primitiva real).
    if let PrimitiveParams::Sphere { center, radius } = &sphere.parameters
        && sphere_angular_coverage(vertices, *center, *radius) < 0.15 {
            sphere = unknown_result();
        }
    if let PrimitiveParams::Cylinder { point, axis, radius } = &cylinder.parameters {
        let (axial_span, angular_cov) = cylinder_coverage(vertices, *point, *axis, *radius);
        if axial_span < 0.5 * (*radius) || angular_cov < 0.20 {
            cylinder = unknown_result();
        }
    }

    // Penalización por complejidad (Occam): a igual error, preferimos primitivas
    // con menos grados de libertad.
    //   Plano:    3 dof (point + normal con escala) — bonus
    //   Cilindro: 5 dof (eje + radio)               — neutro
    //   Esfera:   4 dof (centro + radio)            — penaliza moderado
    let mut plane_p = plane.clone();
    let mut cylinder_p = cylinder.clone();
    let mut sphere_p = sphere.clone();
    let penalty_unit = 0.30 * error_threshold; // 30% del threshold por nivel
    plane_p.rms_error = plane.rms_error;
    sphere_p.rms_error = sphere.rms_error + penalty_unit;
    cylinder_p.rms_error = cylinder.rms_error + 0.15 * error_threshold;

    // Score: requerir cobertura mínima
    let min_inlier_ratio = 0.5;
    let candidates = [plane_p, cylinder_p, sphere_p];
    let originals = [plane, cylinder, sphere];
    let mut best_idx: Option<usize> = None;
    let mut best_score = f64::NEG_INFINITY;
    for (i, c) in candidates.iter().enumerate() {
        if !c.rms_error.is_finite() { continue; }
        if c.rms_error > error_threshold { continue; }
        if c.inlier_ratio < min_inlier_ratio { continue; }
        let score = c.inlier_ratio - c.rms_error / error_threshold.max(1e-12) * 0.5;
        if score > best_score {
            best_score = score;
            best_idx = Some(i);
        }
    }
    match best_idx {
        Some(i) => originals[i].clone(),
        None => unknown_result(),
    }
}

/// Cobertura angular sobre la esfera: 0 = un punto, 1 = esfera completa.
/// Mide cuán "abierto" es el cono que contiene a todos los puntos vistos desde el centro.
fn sphere_angular_coverage(vertices: &[[f64; 3]], center: [f64; 3], radius: f64) -> f64 {
    if vertices.is_empty() || radius <= 0.0 { return 0.0; }
    let c = Vector3::from_row_slice(&center);
    // Dirección promedio desde el centro hacia los puntos
    let mut cdir = Vector3::zeros();
    for v in vertices {
        let d = Vector3::from_row_slice(v) - c;
        if d.norm() > 1e-12 { cdir += d.normalize(); }
    }
    if cdir.norm() < 1e-12 { return 1.0; } // puntos distribuidos por toda la esfera
    let cdir = cdir.normalize();
    // Menor dot product = punto más alejado de la dirección central
    let mut min_dot = 1.0;
    for v in vertices {
        let d = Vector3::from_row_slice(v) - c;
        if d.norm() < 1e-12 { continue; }
        let dot = cdir.dot(&d.normalize());
        if dot < min_dot { min_dot = dot; }
    }
    // min_dot=1 → todo apuntando igual (cono 0°); min_dot=-1 → cubre esfera completa
    (1.0 - min_dot) / 2.0
}

/// Cobertura del cilindro: (extensión axial, cobertura angular alrededor del eje).
fn cylinder_coverage(vertices: &[[f64; 3]], point: [f64; 3], axis: [f64; 3], _radius: f64) -> (f64, f64) {
    if vertices.is_empty() { return (0.0, 0.0); }
    let p0 = Vector3::from_row_slice(&point);
    let a = Vector3::from_row_slice(&axis).normalize();
    // Base ortonormal en el plano perpendicular
    let u = if a.x.abs() < 0.9 { a.cross(&Vector3::new(1.0, 0.0, 0.0)).normalize() }
            else { a.cross(&Vector3::new(0.0, 1.0, 0.0)).normalize() };
    let v = a.cross(&u);

    let mut min_along = f64::INFINITY;
    let mut max_along = f64::NEG_INFINITY;
    let mut avg_dir_uv = nalgebra::Vector2::<f64>::zeros();
    for vert in vertices {
        let p = Vector3::from_row_slice(vert) - p0;
        let along = p.dot(&a);
        if along < min_along { min_along = along; }
        if along > max_along { max_along = along; }
        let uu = p.dot(&u);
        let vv = p.dot(&v);
        let n = (uu*uu + vv*vv).sqrt();
        if n > 1e-12 { avg_dir_uv += nalgebra::Vector2::new(uu/n, vv/n); }
    }
    let axial_span = max_along - min_along;
    let avg_n = avg_dir_uv.norm();
    if avg_n < 1e-12 { return (axial_span, 1.0); }
    let avg_unit = avg_dir_uv / avg_n;
    // Cobertura angular: min dot con la dirección promedio
    let mut min_dot = 1.0;
    for vert in vertices {
        let p = Vector3::from_row_slice(vert) - p0;
        let uu = p.dot(&u);
        let vv = p.dot(&v);
        let n = (uu*uu + vv*vv).sqrt();
        if n < 1e-12 { continue; }
        let dot = (uu*avg_unit.x + vv*avg_unit.y) / n;
        if dot < min_dot { min_dot = dot; }
    }
    let angular_cov = (1.0 - min_dot) / 2.0;
    (axial_span, angular_cov)
}

// ---------------------------------------------------------------------------
//  HELPERS
// ---------------------------------------------------------------------------

fn centroid(vertices: &[[f64; 3]]) -> (f64, f64, f64) {
    let n = vertices.len() as f64;
    let mut cx = 0.0; let mut cy = 0.0; let mut cz = 0.0;
    for v in vertices { cx += v[0]; cy += v[1]; cz += v[2]; }
    (cx / n, cy / n, cz / n)
}

fn all_unique(a: &[usize]) -> bool {
    for i in 0..a.len() {
        for j in (i+1)..a.len() {
            if a[i] == a[j] { return false; }
        }
    }
    true
}

/// Círculo 2D por mínimos cuadrados algebraico.
fn fit_circle_2d(points: &[[f64; 2]]) -> (f64, f64, f64) {
    let n = points.len();
    if n < 3 { return (0.0, 0.0, 0.0); }
    let a_mat = DMatrix::from_fn(n, 3, |i, j| match j {
        0 => points[i][0],
        1 => points[i][1],
        _ => 1.0,
    });
    let b_vec = DMatrix::from_fn(n, 1, |i, _| points[i][0].powi(2) + points[i][1].powi(2));
    let ata = a_mat.transpose() * &a_mat;
    let atb = a_mat.transpose() * &b_vec;
    match ata.lu().solve(&atb) {
        Some(x) => {
            let cx = x[(0, 0)] / 2.0;
            let cy = x[(1, 0)] / 2.0;
            let r = (x[(2, 0)] + cx*cx + cy*cy).max(0.0).sqrt();
            (cx, cy, r)
        }
        None => (0.0, 0.0, 0.0),
    }
}

/// LCG simple, determinista, sin deps externas. Suficiente para RANSAC.
struct LcgRng { state: u64 }
impl LcgRng {
    fn seed_from_data(verts: &[[f64; 3]]) -> Self {
        // Hash burdo de los primeros puntos + longitud
        let mut s: u64 = 0xcafebabe_u64.wrapping_mul(verts.len() as u64 + 1);
        for v in verts.iter().take(8) {
            for c in v {
                s ^= c.to_bits();
                s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            }
        }
        Self { state: s | 1 }
    }
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.state
    }
    fn range(&mut self, hi: usize) -> usize {
        (self.next_u64() as usize) % hi.max(1)
    }
}

#[allow(dead_code)]
fn outer3(a: &Vector3<f64>, b: &Vector3<f64>) -> Matrix3<f64> {
    Matrix3::new(
        a.x*b.x, a.x*b.y, a.x*b.z,
        a.y*b.x, a.y*b.y, a.y*b.z,
        a.z*b.x, a.z*b.y, a.z*b.z,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    #[test]
    fn plane_xy_exact() {
        let verts: Vec<[f64; 3]> = vec![
            [0.0, 0.0, 2.0], [1.0, 0.0, 2.0], [0.0, 1.0, 2.0],
            [1.0, 1.0, 2.0], [0.5, 0.5, 2.0],
        ];
        let r = fit_plane(&verts);
        assert_eq!(r.primitive_type, PrimitiveType::Plane);
        assert!(r.rms_error < 1e-10);
    }

    #[test]
    fn plane_ransac_with_outliers() {
        // Plane z=0 + 2 outliers
        let mut verts: Vec<[f64; 3]> = Vec::new();
        for i in 0..10 { for j in 0..10 {
            verts.push([i as f64 * 0.1, j as f64 * 0.1, 0.0]);
        }}
        verts.push([0.5, 0.5, 5.0]);
        verts.push([0.3, 0.7, -4.0]);
        let r = fit_plane_ransac(&verts, 0.01, 200);
        assert_eq!(r.primitive_type, PrimitiveType::Plane);
        assert!(r.rms_error < 0.05, "rms={}", r.rms_error);
        assert!(r.inlier_ratio > 0.9, "inliers={}", r.inlier_ratio);
    }

    #[test]
    fn sphere_clean() {
        let (cx, cy, cz, r) = (1.0, 2.0, 3.0, 3.0);
        let mut verts = Vec::new();
        for i in 0..20 { for j in 1..10 {
            let theta = 2.0 * PI * i as f64 / 20.0;
            let phi = PI * j as f64 / 10.0;
            verts.push([
                cx + r * phi.sin() * theta.cos(),
                cy + r * phi.sin() * theta.sin(),
                cz + r * phi.cos(),
            ]);
        }}
        let res = fit_sphere(&verts);
        assert_eq!(res.primitive_type, PrimitiveType::Sphere);
        assert!(res.rms_error < 1e-6);
    }

    fn make_cylinder_with_normals(r: f64, h: f64, n_theta: usize, n_z: usize, axis_dir: Vector3<f64>) -> (Vec<[f64; 3]>, Vec<[f64; 3]>) {
        // Construir base ortonormal alrededor del eje
        let a = axis_dir.normalize();
        let helper = if a.x.abs() < 0.9 { Vector3::new(1.0, 0.0, 0.0) } else { Vector3::new(0.0, 1.0, 0.0) };
        let u = a.cross(&helper).normalize();
        let v = a.cross(&u);
        let mut verts = Vec::new();
        let mut normals = Vec::new();
        for i in 0..n_theta {
            for j in 0..n_z {
                let theta = 2.0 * PI * i as f64 / n_theta as f64;
                let z = j as f64 * h / (n_z as f64 - 1.0).max(1.0);
                let p = u * (r * theta.cos()) + v * (r * theta.sin()) + a * z;
                let n = u * theta.cos() + v * theta.sin();
                verts.push([p.x, p.y, p.z]);
                normals.push([n.x, n.y, n.z]);
            }
        }
        (verts, normals)
    }

    #[test]
    fn cylinder_robust_clean() {
        let (verts, normals) = make_cylinder_with_normals(2.0, 5.0, 30, 6, Vector3::new(0.0, 0.0, 1.0));
        let r = fit_cylinder_robust(&verts, &normals);
        assert_eq!(r.primitive_type, PrimitiveType::Cylinder);
        assert!(r.rms_error < 1e-3, "rms={}", r.rms_error);
        if let PrimitiveParams::Cylinder { radius, axis, .. } = r.parameters {
            assert!((radius - 2.0).abs() < 0.05, "r={radius}");
            assert!(axis[2].abs() > 0.95, "axis={:?}", axis);
        }
    }

    #[test]
    fn cylinder_short_height() {
        // Cilindro plano (h pequeño) — caso donde PCA legacy falla.
        let (verts, normals) = make_cylinder_with_normals(2.0, 0.3, 40, 3, Vector3::new(0.0, 0.0, 1.0));
        let r = fit_cylinder_robust(&verts, &normals);
        assert_eq!(r.primitive_type, PrimitiveType::Cylinder);
        if let PrimitiveParams::Cylinder { radius, axis, .. } = r.parameters {
            assert!((radius - 2.0).abs() < 0.1, "r={radius}");
            assert!(axis[2].abs() > 0.9, "axis={:?}", axis);
        }
    }

    #[test]
    fn cylinder_oblique_axis() {
        let axis = Vector3::new(1.0, 1.0, 2.0);
        let (verts, normals) = make_cylinder_with_normals(1.5, 4.0, 30, 5, axis);
        let r = fit_cylinder_robust(&verts, &normals);
        assert_eq!(r.primitive_type, PrimitiveType::Cylinder);
        if let PrimitiveParams::Cylinder { radius, axis: a, .. } = r.parameters {
            assert!((radius - 1.5).abs() < 0.1, "r={radius}");
            let expected = axis.normalize();
            let dot = (a[0]*expected.x + a[1]*expected.y + a[2]*expected.z).abs();
            assert!(dot > 0.95, "axis dot={dot}");
        }
    }

    #[test]
    fn cylinder_ransac_with_outliers() {
        let (mut verts, mut normals) = make_cylinder_with_normals(1.0, 3.0, 25, 5, Vector3::new(0.0, 0.0, 1.0));
        // Outliers
        verts.push([5.0, 0.0, 1.0]); normals.push([1.0, 0.0, 0.0]);
        verts.push([-5.0, 0.0, 1.5]); normals.push([0.0, 1.0, 0.0]);
        let r = fit_cylinder_ransac(&verts, &normals, 0.05, 100);
        assert_eq!(r.primitive_type, PrimitiveType::Cylinder);
        assert!(r.inlier_ratio > 0.9, "inliers={}", r.inlier_ratio);
        if let PrimitiveParams::Cylinder { radius, .. } = r.parameters {
            assert!((radius - 1.0).abs() < 0.1, "r={radius}");
        }
    }

    #[test]
    fn best_fit_returns_unknown_on_noise() {
        // Nube totalmente aleatoria
        let mut verts = Vec::new();
        let mut s: u64 = 12345;
        for _ in 0..50 {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let a = (s as f64) / u64::MAX as f64;
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let b = (s as f64) / u64::MAX as f64;
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let c = (s as f64) / u64::MAX as f64;
            verts.push([a, b, c]);
        }
        let r = fit_best(&verts, 0.001); // umbral muy estricto
        assert_eq!(r.primitive_type, PrimitiveType::Unknown);
    }

    #[test]
    fn best_fit_plane_with_normals() {
        // Plane verts + normals coherentes
        let mut verts = Vec::new();
        let mut normals = Vec::new();
        for i in 0..10 { for j in 0..10 {
            verts.push([i as f64 * 0.1, j as f64 * 0.1, 0.0]);
            normals.push([0.0, 0.0, 1.0]);
        }}
        let r = fit_best_with_normals(&verts, Some(&normals), 0.01);
        assert_eq!(r.primitive_type, PrimitiveType::Plane);
    }
}
