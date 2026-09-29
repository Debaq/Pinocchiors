//! Escáner sintético: renderiza el mapa de profundidad de un objeto asimétrico
//! girando en una tornamesa frente a la cámara, con ruido de sensor. Lo usan la
//! prueba de escaneo y el ejemplo `synthetic_scan`.

#![allow(dead_code)]

use orizon3d_core::camera::{default_depth_intrinsics, Extrinsics};
use orizon3d_core::pointcloud::CloudParams;
use orizon3d_core::scan::Transform;

/// Distancia de la cámara al eje de la tornamesa (mm)
pub const CENTER_Z: f32 = 300.0;

/// Distancia con signo al objeto en su propio marco (mm): caja 70×50×40 con
/// una esfera saliendo de una esquina de arriba y un cilindro al costado. Las
/// paredes verticales no fijan la altura: solo la esfera lo hace
pub fn sdf(p: [f32; 3]) -> f32 {
    let q = [p[0].abs() - 35.0, p[1].abs() - 25.0, p[2].abs() - 20.0];
    let outside = (q[0].max(0.0).powi(2) + q[1].max(0.0).powi(2) + q[2].max(0.0).powi(2)).sqrt();
    let bx = outside + q[0].max(q[1]).max(q[2]).min(0.0);
    let s = [p[0] - 20.0, p[1] + 25.0, p[2] - 5.0];
    let sphere = (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt() - 18.0;
    let c = [p[0] + 35.0, p[2] + 10.0];
    let cyl = ((c[0] * c[0] + c[1] * c[1]).sqrt() - 10.0).max(p[1].abs() - 20.0);
    bx.min(sphere).min(cyl)
}

/// Punto de cámara → marco del objeto con la tornamesa girada `a` rad (eje Y)
pub fn to_object(p: [f32; 3], a: f32) -> [f32; 3] {
    let (s, c) = a.sin_cos();
    let (x, z) = (p[0], p[2] - CENTER_Z);
    [c * x - s * z, p[1], s * x + c * z]
}

/// Pose verdadera del cuadro con la tornamesa en `deg` grados (cámara → sistema
/// del primer cuadro)
pub fn truth_pose(deg: f32) -> Transform {
    let (s, c) = deg.to_radians().sin_cos();
    Transform { r: [c, 0.0, -s, 0.0, 1.0, 0.0, s, 0.0, c], t: [CENTER_Z * s, 0.0, CENTER_Z - CENTER_Z * c] }
}

/// Error (mm) de un punto del sistema global respecto de la superficie real
pub fn surface_error(p: [f32; 3]) -> f32 {
    sdf(to_object(p, 0.0)).abs()
}

/// Parámetros de nube que usa el escáner para un cuadro de `w`×`h`
pub fn cloud_params(w: u32, h: u32) -> CloudParams {
    CloudParams {
        depth_intr: default_depth_intrinsics(w, h),
        rgb_intr: None,
        extrinsics: Extrinsics::default(),
        depth_scale: 0.1,
        clip_min_mm: 0.0,
        clip_max_mm: 0.0,
        roi: None,
        edge_filter: true,
    }
}

/// Focal en píxeles para un FOV horizontal
pub fn focal(w: u32, hfov_deg: f32) -> f32 {
    (w as f32 / 2.0) / (hfov_deg.to_radians() / 2.0).tan()
}

/// Mapa de profundidad (unidades de 0,1 mm) con la tornamesa girada `a` rad.
/// `noise` es la desviación aproximada del ruido de profundidad en mm
pub fn render(a: f32, w: u32, h: u32, f: f32, noise: f32, seed: &mut u32) -> Vec<u16> {
    let mut rnd = || {
        *seed ^= *seed << 13;
        *seed ^= *seed >> 17;
        *seed ^= *seed << 5;
        (*seed as f32 / u32::MAX as f32) - 0.5
    };
    let mut out = vec![0u16; (w * h) as usize];
    for v in 0..h {
        for u in 0..w {
            let d = [(u as f32 - w as f32 / 2.0) / f, (v as f32 - h as f32 / 2.0) / f, 1.0];
            let mut t = CENTER_Z - 80.0;
            for _ in 0..96 {
                let dist = sdf(to_object([d[0] * t, d[1] * t, d[2] * t], a));
                if dist < 0.02 {
                    // Suma de uniformes ≈ normal
                    let n = (rnd() + rnd() + rnd() + rnd()) * noise;
                    out[(v * w + u) as usize] = ((t + n) * 10.0).round() as u16;
                    break;
                }
                t += dist;
                if t > CENTER_Z + 80.0 {
                    break;
                }
            }
        }
    }
    out
}
