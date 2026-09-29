//! Mallado de una esfera escaneada: la malla debe quedar sobre la superficie,
//! sin una segunda pared detrás y con las caras hacia afuera, tanto con una
//! vista como con varias alrededor del objeto.

use orizon3d_core::{mesh, pointcloud::{Point, PointCloud}};

const R: f32 = 40.0;
const C: [f32; 3] = [0.0, 0.0, 300.0];

/// Puntos de la esfera visibles desde una cámara en `cam`, muestreados en ángulo
fn view(cam: [f32; 3], step: f32) -> Vec<([f32; 3], [f32; 3])> {
    let mut out = Vec::new();
    let n = (std::f32::consts::PI * R / step) as i32;
    for a in 0..n {
        let th = std::f32::consts::PI * (a as f32 + 0.5) / n as f32;
        let m = ((2.0 * n as f32 * th.sin()) as i32).max(1);
        for b in 0..m {
            let ph = 2.0 * std::f32::consts::PI * b as f32 / m as f32;
            let nrm = [th.sin() * ph.cos(), th.cos(), th.sin() * ph.sin()];
            let p = [C[0] + R * nrm[0], C[1] + R * nrm[1], C[2] + R * nrm[2]];
            let to_cam = [cam[0] - p[0], cam[1] - p[1], cam[2] - p[2]];
            if nrm[0] * to_cam[0] + nrm[1] * to_cam[1] + nrm[2] * to_cam[2] > 0.0 {
                let l = (to_cam[0] * to_cam[0] + to_cam[1] * to_cam[1] + to_cam[2] * to_cam[2]).sqrt();
                out.push((p, [to_cam[0] / l, to_cam[1] / l, to_cam[2] / l]));
            }
        }
    }
    out
}

fn cloud(pts: Vec<([f32; 3], [f32; 3])>) -> PointCloud {
    PointCloud { points: pts.into_iter().map(|(p, v)| Point { x: p[0], y: p[1], z: p[2], rgb: [200; 3], view: v }).collect(), has_color: false }
}

/// (error medio en mm, fracción de vértices a más de 1,5 mm, fracción de caras hacia adentro)
fn measure(m: &mesh::Mesh) -> (f32, f32, f32) {
    let errs: Vec<f32> = m.vertices.iter().map(|v| {
        let d = [v[0] - C[0], v[1] - C[1], v[2] - C[2]];
        ((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() - R).abs()
    }).collect();
    let bad = errs.iter().filter(|&&e| e > 1.5).count();
    let mean = errs.iter().sum::<f32>() / errs.len().max(1) as f32;
    // Caras con normal que apunta hacia adentro de la esfera
    let mut inward = 0;
    for t in &m.tris {
        let [a, b, c] = t.map(|i| m.vertices[i as usize]);
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let w = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let n = [u[1] * w[2] - u[2] * w[1], u[2] * w[0] - u[0] * w[2], u[0] * w[1] - u[1] * w[0]];
        let g = [(a[0] + b[0] + c[0]) / 3.0 - C[0], (a[1] + b[1] + c[1]) / 3.0 - C[1], (a[2] + b[2] + c[2]) / 3.0 - C[2]];
        if n[0] * g[0] + n[1] * g[1] + n[2] * g[2] < 0.0 { inward += 1; }
    }
    let n = errs.len().max(1) as f32;
    (mean, bad as f32 / n, inward as f32 / m.tris.len().max(1) as f32)
}

fn check(m: &mesh::Mesh) {
    assert!(!m.is_empty());
    let (mean, off, inward) = measure(m);
    assert!(mean < 0.5, "error medio {mean} mm");
    assert!(off < 0.02, "{:.1}% de los vértices fuera de la superficie", off * 100.0);
    assert!(inward < 0.02, "{:.1}% de las caras hacia adentro", inward * 100.0);
}

#[test]
fn single_view_has_one_wall_facing_the_camera() {
    check(&mesh::reconstruct(&cloud(view([0.0, 0.0, 0.0], 1.0)), 2.0, 1, 2));
}

#[test]
fn views_around_the_object_close_the_surface_facing_out() {
    let mut all = Vec::new();
    for k in 0..8 {
        let a = k as f32 * std::f32::consts::TAU / 8.0;
        all.extend(view([C[0] + 300.0 * a.sin(), 0.0, C[2] - 300.0 * a.cos()], 1.0));
    }
    check(&mesh::reconstruct(&cloud(all), 2.0, 1, 2));
}
