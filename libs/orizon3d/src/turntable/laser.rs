//! Línea láser: extracción subpíxel en la imagen y triangulación contra el
//! plano calibrado del láser.

use crate::camera::RgbFrame;
use crate::pointcloud::{Point, PointCloud};

use super::rig::{Calibration, View};

/// Cómo encontrar la línea en cada fila de la imagen
#[derive(Debug, Clone, Copy)]
pub struct LineSettings {
    /// Señal mínima del pico (niveles de 0 a 255)
    pub threshold: f32,
    /// Ancho máximo a media altura (px): más ancho es un reflejo o luz
    /// ambiente, no la línea
    pub max_width: usize,
    /// Fracción del pico bajo la cual los píxeles no cuentan para el centroide
    pub floor: f32,
    /// Una fila con otro pico de más de esta fracción del principal es
    /// ambigua (reflejo, o la línea partida por un borde) y se descarta
    pub ambiguity: f32,
    /// Filas más anchas que esta razón de la mediana del cuadro se descartan:
    /// en un borde o una arista la línea se ensancha y el centroide se corre
    pub width_ratio: f32,
    /// Filas que se quitan en cada punta de un tramo continuo de la línea: el
    /// desenfoque estira la línea más allá del borde del objeto y esas filas
    /// triangulan puntos en el aire
    pub trim: usize,
}

impl Default for LineSettings {
    fn default() -> Self {
        LineSettings { threshold: 25.0, max_width: 40, floor: 0.25, ambiguity: 0.5, width_ratio: 1.8, trim: 2 }
    }
}

/// Un punto de la línea: columna subpíxel `u` en la fila `v`
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinePoint {
    pub u: f64,
    pub v: f64,
    pub strength: f32,
    /// Ancho a media altura (px)
    pub width: f32,
}

/// Señal del láser rojo en el píxel `i`: con cuadro apagado, lo que subió el
/// rojo; sin él, cuánto domina el rojo sobre el verde y el azul
fn signal(on: &[u8], off: Option<&[u8]>, i: usize) -> f32 {
    let (r, g, b) = (on[i * 3] as f32, on[i * 3 + 1] as f32, on[i * 3 + 2] as f32);
    match off {
        Some(off) => r - off[i * 3] as f32,
        None => r - g.max(b),
    }
}

/// Busca la línea de un láser vertical: en cada fila, el centroide del pico
/// más fuerte. `off` es el mismo cuadro con el láser apagado (resta la luz
/// ambiente y la textura roja del objeto)
pub fn extract_vertical_line(on: &RgbFrame, off: Option<&RgbFrame>, s: &LineSettings) -> Vec<LinePoint> {
    let (w, h) = (on.width as usize, on.height as usize);
    let off = off.filter(|f| f.width == on.width && f.height == on.height).map(|f| f.rgb.as_slice());
    let mut out = Vec::new();
    let mut row = vec![0f32; w];
    for v in 0..h {
        for (u, x) in row.iter_mut().enumerate() {
            *x = signal(&on.rgb, off, v * w + u);
        }
        let (peak_u, &peak) = row.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap();
        if peak < s.threshold {
            continue;
        }
        // Extensión a media altura: una mancha ancha no es la línea
        let half = peak * 0.5;
        let mut l = peak_u;
        while l > 0 && row[l - 1] >= half {
            l -= 1;
        }
        let mut r = peak_u;
        while r + 1 < w && row[r + 1] >= half {
            r += 1;
        }
        let width = (r - l + 1) as f32;
        if width > s.max_width as f32 {
            continue;
        }
        // Centroide sobre el piso: toma las colas del perfil para la parte
        // subpíxel y descarta el ruido de fondo
        let floor = peak * s.floor;
        while l > 0 && row[l - 1] > floor {
            l -= 1;
        }
        while r + 1 < w && row[r + 1] > floor {
            r += 1;
        }
        let rival = row[..l].iter().chain(&row[r + 1..]).fold(0f32, |a, &b| a.max(b));
        if rival > peak * s.ambiguity {
            continue;
        }
        let (mut sw, mut su) = (0.0f64, 0.0f64);
        for (u, &x) in row.iter().enumerate().take(r + 1).skip(l) {
            let wgt = (x - floor).max(0.0) as f64;
            sw += wgt;
            su += wgt * u as f64;
        }
        if sw > 0.0 {
            out.push(LinePoint { u: su / sw, v: v as f64, strength: peak, width });
        }
    }
    if !out.is_empty() {
        let mut widths: Vec<f32> = out.iter().map(|p| p.width).collect();
        let mid = widths.len() / 2;
        let median = *widths.select_nth_unstable_by(mid, f32::total_cmp).1;
        out.retain(|p| p.width <= median * s.width_ratio + 0.5);
    }
    trim_runs(out, s.trim)
}

/// Quita `trim` filas en cada punta de los tramos continuos (filas seguidas
/// sin salto de columna) y los tramos que no las tienen
fn trim_runs(points: Vec<LinePoint>, trim: usize) -> Vec<LinePoint> {
    if trim == 0 {
        return points;
    }
    let mut out = Vec::with_capacity(points.len());
    let mut start = 0;
    for i in 1..=points.len() {
        let breaks = i == points.len() || points[i].v - points[i - 1].v > 1.5 || (points[i].u - points[i - 1].u).abs() > 2.0;
        if breaks {
            if i - start > 2 * trim {
                out.extend_from_slice(&points[start + trim..i - trim]);
            }
            start = i;
        }
    }
    out
}

/// Qué puntos triangulados se conservan
#[derive(Debug, Clone, Copy)]
pub struct TriangulateSettings {
    /// Altura mínima sobre el plato (mm): descarta la línea sobre el plato
    pub min_height: f64,
    /// Radio máximo desde el eje (mm): descarta lo que está fuera del plato
    pub max_radius: f64,
    /// Ángulo mínimo entre el rayo de la cámara y el plano del láser (grados):
    /// por debajo, un error de un píxel mueve mucho el punto
    pub min_angle: f64,
}

impl Default for TriangulateSettings {
    fn default() -> Self {
        TriangulateSettings { min_height: 0.8, max_radius: 60.0, min_angle: 8.0 }
    }
}

/// Lleva los puntos de la línea de `laser` al marco del objeto: rayo de la
/// cámara ∩ plano del láser, deshaciendo el giro del plato (`plate_deg`).
/// `color` es un cuadro con luz del que se toma el color de cada punto
pub fn triangulate(
    line: &[LinePoint],
    calib: &Calibration,
    view: &View,
    laser: usize,
    plate_deg: f64,
    color: Option<&RgbFrame>,
    s: &TriangulateSettings,
) -> PointCloud {
    let Some(plane) = view.lasers.get(laser) else {
        return PointCloud::default();
    };
    let world_from_cam = view.cam_from_world.inverse();
    let object_from_world = calib.plate.world_from_object(plate_deg).inverse();
    let center = world_from_cam.translation.vector;
    let sin_min = s.min_angle.to_radians().sin();
    let color = color.filter(|c| c.width == calib.camera.width && c.height == calib.camera.height);
    let mut cloud = PointCloud { points: Vec::with_capacity(line.len()), has_color: color.is_some() };
    for lp in line {
        let dir = world_from_cam.rotation * calib.camera.ray(lp.u, lp.v);
        if plane.normal.dot(&dir).abs() < sin_min {
            continue;
        }
        let Some(t) = plane.intersect(&center, &dir).filter(|&t| t > 0.0) else {
            continue;
        };
        let p = center + dir * t;
        if calib.plate.height(&p) < s.min_height || calib.plate.radius(&p) > s.max_radius {
            continue;
        }
        let q = object_from_world * nalgebra::Point3::from(p);
        let view_dir = object_from_world.rotation * -dir;
        let rgb = color.map_or([200, 200, 200], |c| sample(c, lp.u, lp.v));
        cloud.points.push(Point {
            x: q.x as f32,
            y: q.y as f32,
            z: q.z as f32,
            rgb,
            view: [view_dir.x as f32, view_dir.y as f32, view_dir.z as f32],
        });
    }
    cloud
}

/// Color bilineal en un píxel subpíxel
fn sample(img: &RgbFrame, u: f64, v: f64) -> [u8; 3] {
    let (w, h) = (img.width as usize, img.height as usize);
    let u = u.clamp(0.0, (w - 1) as f64);
    let v = v.clamp(0.0, (h - 1) as f64);
    let (u0, v0) = (u.floor() as usize, v.floor() as usize);
    let (u1, v1) = ((u0 + 1).min(w - 1), (v0 + 1).min(h - 1));
    let (fu, fv) = (u - u0 as f64, v - v0 as f64);
    let px = |x: usize, y: usize, c: usize| img.rgb[(y * w + x) * 3 + c] as f64;
    let mut out = [0u8; 3];
    for (c, o) in out.iter_mut().enumerate() {
        let top = px(u0, v0, c) * (1.0 - fu) + px(u1, v0, c) * fu;
        let bottom = px(u0, v1, c) * (1.0 - fu) + px(u1, v1, c) * fu;
        *o = (top * (1.0 - fv) + bottom * fv).round() as u8;
    }
    out
}
