//! Imagen PNG de una malla sin ventana, para que la IA vea lo que hizo:
//! proyección ortográfica, sombreado, aristas visibles, caras o aristas
//! resaltadas, el número de cada cara (o arista) encima y un triedro de ejes.

use std::collections::HashSet;

/// Malla para dibujar (Z arriba, mm)
#[derive(Default)]
pub struct Mesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
    /// Cara de cada triángulo (para colores, resaltado y números)
    pub triangle_face: Vec<u32>,
    /// Aristas como polilíneas, en el orden de sus números
    pub edges: Vec<Vec<[f32; 3]>>,
}

/// Lee la malla de `cad_mesh` (Y arriba, unidades de la escena) y la deja en Z arriba y mm
pub fn decode_cad_mesh(bytes: &[u8], mm_per_unit: f32) -> Result<Mesh, String> {
    let word = |i: usize| -> Result<[u8; 4], String> { bytes.get(i * 4..i * 4 + 4).and_then(|b| b.try_into().ok()).ok_or_else(|| "malla incompleta".to_string()) };
    let u = |i: usize| word(i).map(u32::from_le_bytes);
    let f = |i: usize| word(i).map(f32::from_le_bytes);
    let (nv, nt, ne, np) = (u(0)? as usize, u(1)? as usize, u(2)? as usize, u(3)? as usize);
    let mut at = 4;
    let vec3 = |at: &mut usize, n: usize, scale: f32| -> Result<Vec<[f32; 3]>, String> {
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            let (x, y, z) = (f(*at)?, f(*at + 1)?, f(*at + 2)?);
            // Y arriba → Z arriba
            out.push([x * scale, -z * scale, y * scale]);
            *at += 3;
        }
        Ok(out)
    };
    let positions = vec3(&mut at, nv, mm_per_unit)?;
    let normals = vec3(&mut at, nv, 1.0)?;
    let mut triangles = Vec::with_capacity(nt);
    for _ in 0..nt {
        triangles.push([u(at)?, u(at + 1)?, u(at + 2)?]);
        at += 3;
    }
    let mut triangle_face = Vec::with_capacity(nt);
    for _ in 0..nt {
        triangle_face.push(u(at)?);
        at += 1;
    }
    let mut ends = Vec::with_capacity(ne);
    for _ in 0..ne {
        ends.push(u(at)? as usize);
        at += 1;
    }
    let points = vec3(&mut at, np, mm_per_unit)?;
    let mut edges = Vec::with_capacity(ne);
    let mut start = 0;
    for end in ends {
        edges.push(points.get(start..end).map(<[_]>::to_vec).unwrap_or_default());
        start = end;
    }
    Ok(Mesh { positions, normals, triangles, triangle_face, edges })
}

/// Desde dónde se mira
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    /// Del centro hacia la cámara
    pub toward_eye: [f32; 3],
    /// Arriba de la imagen
    pub up: [f32; 3],
}

impl Camera {
    /// Vistas con nombre: iso, top, bottom, front, back, left, right
    pub fn named(name: &str) -> Option<Camera> {
        let c = |e: [f32; 3], u: [f32; 3]| Some(Camera { toward_eye: e, up: u });
        match name {
            "iso" => c([1.0, -1.0, 0.8], [0.0, 0.0, 1.0]),
            "iso_back" => c([-1.0, 1.0, 0.8], [0.0, 0.0, 1.0]),
            "iso_below" => c([1.0, -1.0, -0.8], [0.0, 0.0, 1.0]),
            "top" => c([0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
            "bottom" => c([0.0, 0.0, -1.0], [0.0, -1.0, 0.0]),
            "front" => c([0.0, -1.0, 0.0], [0.0, 0.0, 1.0]),
            "back" => c([0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
            "right" => c([1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            "left" => c([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            _ => None,
        }
    }

    /// Ángulos: azimut desde +X hacia +Y, elevación sobre el plano XY (grados)
    pub fn orbit(azimuth: f32, elevation: f32) -> Camera {
        let (a, e) = (azimuth.to_radians(), elevation.clamp(-89.0, 89.0).to_radians());
        Camera { toward_eye: [e.cos() * a.cos(), e.cos() * a.sin(), e.sin()], up: [0.0, 0.0, 1.0] }
    }
}

/// Qué numerar encima
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Labels {
    None,
    Faces,
    Edges,
}

pub struct Options {
    pub width: u32,
    pub height: u32,
    pub camera: Camera,
    pub labels: Labels,
    pub highlight_faces: HashSet<u32>,
    pub highlight_edges: HashSet<usize>,
    /// Color de cada cara (por pieza); sin color, gris azulado
    pub face_color: Box<dyn Fn(u32) -> [u8; 3]>,
}

/// Lo dibujado: la imagen y qué números quedaron a la vista
pub struct Picture {
    pub png: Vec<u8>,
    pub labeled: Vec<u32>,
}

type V3 = [f32; 3];

fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn norm(a: V3) -> V3 {
    let l = dot(a, a).sqrt();
    if l < 1e-12 { [0.0, 0.0, 1.0] } else { [a[0] / l, a[1] / l, a[2] / l] }
}

/// Supermuestreo: se dibuja al doble y se reduce (bordes suaves)
const SS: u32 = 2;

pub fn render(mesh: &Mesh, o: &Options) -> Result<Picture, String> {
    let (w, h) = (o.width * SS, o.height * SS);
    // Base de la cámara: derecha, arriba y hacia adentro de la imagen
    let back = norm(o.camera.toward_eye);
    let fwd = [-back[0], -back[1], -back[2]];
    let mut up = o.camera.up;
    if dot(norm(up), back).abs() > 0.99 {
        up = [0.0, 1.0, 0.0];
    }
    let right = norm(cross(fwd, up));
    let up = cross(right, fwd);

    // Encuadre: la caja proyectada de todo, con margen
    let pts = mesh.positions.iter().chain(mesh.edges.iter().flatten());
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    let (mut dlo, mut dhi) = (f32::MAX, f32::MIN);
    for p in pts {
        let (x, y, d) = (dot(*p, right), dot(*p, up), dot(*p, fwd));
        lo = [lo[0].min(x), lo[1].min(y)];
        hi = [hi[0].max(x), hi[1].max(y)];
        dlo = dlo.min(d);
        dhi = dhi.max(d);
    }
    if lo[0] > hi[0] {
        lo = [-1.0, -1.0];
        hi = [1.0, 1.0];
    }
    let span = [(hi[0] - lo[0]).max(1e-6), (hi[1] - lo[1]).max(1e-6)];
    let scale = (w as f32 * 0.86 / span[0]).min(h as f32 * 0.86 / span[1]);
    let mid = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
    let project = |p: V3| -> (f32, f32, f32) {
        (w as f32 / 2.0 + (dot(p, right) - mid[0]) * scale, h as f32 / 2.0 - (dot(p, up) - mid[1]) * scale, dot(p, fwd))
    };
    let depth_eps = (dhi - dlo).max(1e-3) * 0.004;

    let n = (w * h) as usize;
    let mut color = vec![[0u8; 3]; n];
    for y in 0..h {
        // Fondo claro con un degradé suave
        let t = y as f32 / h as f32;
        let c = (247.0 - 18.0 * t) as u8;
        for x in 0..w {
            color[(y * w + x) as usize] = [c, c, (c as f32 + 4.0).min(255.0) as u8];
        }
    }
    let mut zbuf = vec![f32::MAX; n];
    let mut ids = vec![u32::MAX; n];
    let light = norm([back[0] * 0.75 + up[0] * 0.55 + right[0] * 0.3, back[1] * 0.75 + up[1] * 0.55 + right[1] * 0.3, back[2] * 0.75 + up[2] * 0.55 + right[2] * 0.3]);

    let screen: Vec<(f32, f32, f32)> = mesh.positions.iter().map(|p| project(*p)).collect();
    for (t, tri) in mesh.triangles.iter().enumerate() {
        let [a, b, c] = tri.map(|i| i as usize);
        let (Some(&pa), Some(&pb), Some(&pc)) = (screen.get(a), screen.get(b), screen.get(c)) else { continue };
        let face = mesh.triangle_face.get(t).copied().unwrap_or(0);
        let area = (pb.0 - pa.0) * (pc.1 - pa.1) - (pb.1 - pa.1) * (pc.0 - pa.0);
        if area.abs() < 1e-9 {
            continue;
        }
        let base = if o.highlight_faces.contains(&face) { [255, 140, 30] } else { (o.face_color)(face) };
        let (x0, x1) = (pa.0.min(pb.0).min(pc.0).floor().max(0.0) as u32, pa.0.max(pb.0).max(pc.0).ceil().min(w as f32 - 1.0) as u32);
        let (y0, y1) = (pa.1.min(pb.1).min(pc.1).floor().max(0.0) as u32, pa.1.max(pb.1).max(pc.1).ceil().min(h as f32 - 1.0) as u32);
        let na = mesh.normals.get(a).copied().unwrap_or([0.0, 0.0, 1.0]);
        let nb = mesh.normals.get(b).copied().unwrap_or(na);
        let nc = mesh.normals.get(c).copied().unwrap_or(na);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let w0 = ((pb.0 - px) * (pc.1 - py) - (pb.1 - py) * (pc.0 - px)) / area;
                let w1 = ((pc.0 - px) * (pa.1 - py) - (pc.1 - py) * (pa.0 - px)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < -1e-4 || w1 < -1e-4 || w2 < -1e-4 {
                    continue;
                }
                let d = w0 * pa.2 + w1 * pb.2 + w2 * pc.2;
                let i = (y * w + x) as usize;
                if d >= zbuf[i] {
                    continue;
                }
                zbuf[i] = d;
                ids[i] = face;
                let mut nrm = norm([0, 1, 2].map(|k| w0 * na[k] + w1 * nb[k] + w2 * nc[k]));
                if dot(nrm, back) < 0.0 {
                    nrm = nrm.map(|v| -v);
                }
                let lit = 0.38 + 0.62 * dot(nrm, light).max(0.0);
                color[i] = base.map(|c| (c as f32 * lit).min(255.0) as u8);
            }
        }
    }

    // Aristas: solo lo que se ve (prueba de profundidad), las resaltadas más gruesas
    let line = |color: &mut [[u8; 3]], a: V3, b: V3, rgb: [u8; 3], thick: i32, test: bool| {
        let (pa, pb) = (project(a), project(b));
        let steps = ((pb.0 - pa.0).abs().max((pb.1 - pa.1).abs()).ceil() as i32).max(1);
        for s in 0..=steps {
            let t = s as f32 / steps as f32;
            let (x, y, d) = (pa.0 + (pb.0 - pa.0) * t, pa.1 + (pb.1 - pa.1) * t, pa.2 + (pb.2 - pa.2) * t);
            for dy in -thick / 2..=thick / 2 {
                for dx in -thick / 2..=thick / 2 {
                    let (xi, yi) = (x as i32 + dx, y as i32 + dy);
                    if xi < 0 || yi < 0 || xi >= w as i32 || yi >= h as i32 {
                        continue;
                    }
                    let i = (yi as u32 * w + xi as u32) as usize;
                    if !test || d <= zbuf[i] + depth_eps {
                        color[i] = rgb;
                    }
                }
            }
        }
    };
    for (k, e) in mesh.edges.iter().enumerate() {
        let hl = o.highlight_edges.contains(&k);
        let (rgb, thick) = if hl { ([235, 90, 0], 6) } else { ([35, 38, 45], 2) };
        for seg in e.windows(2) {
            line(&mut color, seg[0], seg[1], rgb, thick, true);
        }
    }

    // Reducir a la mitad (promedio 2×2)
    let (ow, oh) = (o.width, o.height);
    let mut out = image::RgbImage::new(ow, oh);
    for y in 0..oh {
        for x in 0..ow {
            let mut acc = [0u32; 3];
            for sy in 0..SS {
                for sx in 0..SS {
                    let c = color[((y * SS + sy) * w + x * SS + sx) as usize];
                    for k in 0..3 {
                        acc[k] += c[k] as u32;
                    }
                }
            }
            out.put_pixel(x, y, image::Rgb(acc.map(|v| (v / (SS * SS)) as u8)));
        }
    }

    // Números encima: en el punto visible de cada cara más cercano a su centro visible
    let mut labeled = Vec::new();
    match o.labels {
        Labels::Faces => {
            // Cada número va en el punto más adentro de su cara (lejos de sus
            // bordes): distancia de cada píxel al más cercano de otra cara
            let dist = inner_distance(&ids, w as usize, h as usize);
            // Por cara: la mayor distancia y el centro de sus píxeles
            let mut stats: std::collections::BTreeMap<u32, (u32, f64, f64, f64)> = std::collections::BTreeMap::new();
            for (i, &f) in ids.iter().enumerate() {
                if f != u32::MAX {
                    let e = stats.entry(f).or_insert((0, 0.0, 0.0, 0.0));
                    e.0 = e.0.max(dist[i]);
                    e.1 += (i % w as usize) as f64;
                    e.2 += (i / w as usize) as f64;
                    e.3 += 1.0;
                }
            }
            // Entre los puntos casi tan adentro como el mejor, el más cercano al
            // centro (en una cara larga no queda en una punta)
            let mut best: std::collections::BTreeMap<u32, (u32, usize, f64)> = std::collections::BTreeMap::new();
            for (i, &f) in ids.iter().enumerate() {
                let Some(st) = (f != u32::MAX).then(|| stats[&f]) else { continue };
                if (dist[i] as f64) < st.0 as f64 * 0.8 {
                    continue;
                }
                let (cx, cy) = (st.1 / st.3, st.2 / st.3);
                let d = ((i % w as usize) as f64 - cx).powi(2) + ((i / w as usize) as f64 - cy).powi(2);
                let e = best.entry(f).or_insert((st.0, i, f64::MAX));
                if d < e.2 {
                    *e = (st.0, i, d);
                }
            }
            let mut taken: Vec<(i32, i32, i32, i32)> = Vec::new();
            // Las caras más anchas primero: si dos números chocan, queda el de la grande
            let mut faces: Vec<(u32, (u32, usize, f64))> = best.into_iter().filter(|(_, b)| b.0 >= 6 * SS).collect();
            faces.sort_by_key(|f| std::cmp::Reverse(f.1 .0));
            for (f, (_, i, _)) in faces {
                let (x, y) = (i as u32 % w, i as u32 / w);
                if label(&mut out, &mut taken, x / SS, y / SS, &f.to_string(), [20, 20, 20], [255, 255, 255]) {
                    labeled.push(f);
                }
            }
            labeled.sort_unstable();
        }
        Labels::Edges => {
            let mut taken = Vec::new();
            for (k, e) in mesh.edges.iter().enumerate() {
                if e.len() < 2 {
                    continue;
                }
                let m = e[e.len() / 2];
                let m = if e.len() == 2 { [0, 1, 2].map(|j| (e[0][j] + e[1][j]) / 2.0) } else { m };
                let (x, y, d) = project(m);
                let (xi, yi) = (x as i32, y as i32);
                if xi < 0 || yi < 0 || xi >= w as i32 || yi >= h as i32 {
                    continue;
                }
                // Visible: nada más cerca en ese punto (con margen)
                let i = (yi as u32 * w + xi as u32) as usize;
                if d > zbuf[i] + depth_eps * 3.0 {
                    continue;
                }
                if label(&mut out, &mut taken, xi as u32 / SS, yi as u32 / SS, &k.to_string(), [10, 60, 170], [235, 243, 255]) {
                    labeled.push(k as u32);
                }
            }
        }
        Labels::None => {}
    }
    triad(&mut out, right, up);

    let mut png = Vec::new();
    image::DynamicImage::ImageRgb8(out).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(Picture { png, labeled })
}

/// Distancia (en píxeles, aprox. con pasos 3/4) de cada píxel al borde de su
/// cara: dos pasadas de chaflán sobre los números de cara
fn inner_distance(ids: &[u32], w: usize, h: usize) -> Vec<u32> {
    const BIG: u32 = u32::MAX / 4;
    let mut d: Vec<u32> = vec![BIG; w * h];
    let differs = |i: usize, j: usize| ids[i] != ids[j];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if x == 0 || y == 0 || x == w - 1 || y == h - 1 || differs(i, i - 1) || differs(i, i + 1) || differs(i, i - w) || differs(i, i + w) {
                d[i] = 0;
            }
        }
    }
    for y in 1..h {
        for x in 1..w - 1 {
            let i = y * w + x;
            let m = (d[i - 1] + 3).min(d[i - w] + 3).min(d[i - w - 1] + 4).min(d[i - w + 1] + 4);
            d[i] = d[i].min(m);
        }
    }
    for y in (0..h - 1).rev() {
        for x in (1..w - 1).rev() {
            let i = y * w + x;
            let m = (d[i + 1] + 3).min(d[i + w] + 3).min(d[i + w + 1] + 4).min(d[i + w - 1] + 4);
            d[i] = d[i].min(m);
        }
    }
    d.iter().map(|v| v / 3).collect()
}

// ─── Texto: números y ejes con una letra de 5 × 7 ───────────────────────────

fn glyph(c: char) -> Option<[u8; 7]> {
    Some(match c {
        '0' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
        '1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
        '2' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F],
        '3' => [0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E],
        '4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        '5' => [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
        '6' => [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
        '7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        '9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
        'X' => [0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11],
        'Y' => [0x11, 0x11, 0x0A, 0x04, 0x04, 0x04, 0x04],
        'Z' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F],
        _ => return None,
    })
}

const FONT_SCALE: u32 = 2;

fn text(img: &mut image::RgbImage, x: i32, y: i32, s: &str, rgb: [u8; 3]) {
    for (k, c) in s.chars().enumerate() {
        let Some(rows) = glyph(c) else { continue };
        for (ry, bits) in rows.iter().enumerate() {
            for rx in 0..5 {
                if bits & (0x10 >> rx) == 0 {
                    continue;
                }
                for sy in 0..FONT_SCALE {
                    for sx in 0..FONT_SCALE {
                        let px = x + (k as u32 * 6 * FONT_SCALE + rx * FONT_SCALE + sx) as i32;
                        let py = y + (ry as u32 * FONT_SCALE + sy) as i32;
                        if px >= 0 && py >= 0 && (px as u32) < img.width() && (py as u32) < img.height() {
                            img.put_pixel(px as u32, py as u32, image::Rgb(rgb));
                        }
                    }
                }
            }
        }
    }
}

/// Un número en una cajita centrada en (x, y); no se pone si pisa otra
fn label(img: &mut image::RgbImage, taken: &mut Vec<(i32, i32, i32, i32)>, x: u32, y: u32, s: &str, ink: [u8; 3], paper: [u8; 3]) -> bool {
    let tw = (s.len() as u32 * 6 * FONT_SCALE) as i32 - FONT_SCALE as i32;
    let th = (7 * FONT_SCALE) as i32;
    let (bx, by) = (x as i32 - tw / 2 - 3, y as i32 - th / 2 - 3);
    let r = (bx, by, bx + tw + 6, by + th + 6);
    if taken.iter().any(|t| r.0 < t.2 && t.0 < r.2 && r.1 < t.3 && t.1 < r.3) {
        return false;
    }
    taken.push(r);
    for py in r.1.max(0)..r.3.min(img.height() as i32) {
        for px in r.0.max(0)..r.2.min(img.width() as i32) {
            let edge = py == r.1 || py == r.3 - 1 || px == r.0 || px == r.2 - 1;
            img.put_pixel(px as u32, py as u32, image::Rgb(if edge { ink } else { paper }));
        }
    }
    text(img, bx + 3, by + 3, s, ink);
    true
}

/// Ejes X (rojo), Y (verde) y Z (azul) abajo a la izquierda
fn triad(img: &mut image::RgbImage, right: V3, up: V3) {
    let (ox, oy) = (44.0f32, img.height() as f32 - 44.0);
    let axes = [([1.0, 0.0, 0.0], [210, 40, 40], "X"), ([0.0, 1.0, 0.0], [30, 150, 50], "Y"), ([0.0, 0.0, 1.0], [40, 80, 220], "Z")];
    for (axis, rgb, name) in axes {
        let (dx, dy) = (dot(axis, right) * 30.0, -dot(axis, up) * 30.0);
        let steps = 40;
        for s in 0..=steps {
            let t = s as f32 / steps as f32;
            for k in -1..=1 {
                let (x, y) = ((ox + dx * t) as i32 + k, (oy + dy * t) as i32);
                let (x2, y2) = ((ox + dx * t) as i32, (oy + dy * t) as i32 + k);
                for (px, py) in [(x, y), (x2, y2)] {
                    if px >= 0 && py >= 0 && (px as u32) < img.width() && (py as u32) < img.height() {
                        img.put_pixel(px as u32, py as u32, image::Rgb(rgb));
                    }
                }
            }
        }
        if dx.abs() + dy.abs() > 6.0 {
            text(img, (ox + dx * 1.3) as i32 - 5, (oy + dy * 1.3) as i32 - 7, name, rgb);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cubo de 10 mm: 6 caras de 2 triángulos y sus 12 aristas
    fn cube() -> Mesh {
        let mut m = Mesh::default();
        let faces: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
            ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            ([0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]),
            ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
            ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
            ([0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
            ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        ];
        for (f, (n, a, b)) in faces.iter().enumerate() {
            let c = n.map(|v| v * 5.0);
            let base = m.positions.len() as u32;
            for (s, t) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                m.positions.push([0, 1, 2].map(|k| c[k] + (a[k] * s + b[k] * t) * 5.0));
                m.normals.push(*n);
            }
            m.triangles.push([base, base + 1, base + 2]);
            m.triangles.push([base, base + 2, base + 3]);
            m.triangle_face.extend([f as u32, f as u32]);
        }
        // Arriba al frente (se ve en iso) y abajo atrás (oculta)
        m.edges.push(vec![[-5.0, -5.0, 5.0], [5.0, -5.0, 5.0]]);
        m.edges.push(vec![[-5.0, 5.0, -5.0], [5.0, 5.0, -5.0]]);
        m
    }

    fn opts(view: &str, labels: Labels) -> Options {
        Options {
            width: 320,
            height: 240,
            camera: Camera::named(view).unwrap(),
            labels,
            highlight_faces: HashSet::new(),
            highlight_edges: HashSet::new(),
            face_color: Box::new(|_| [150, 170, 200]),
        }
    }

    #[test]
    fn iso_view_labels_the_three_visible_faces() {
        let pic = render(&cube(), &opts("iso", Labels::Faces)).unwrap();
        assert_eq!(&pic.png[1..4], b"PNG");
        let mut seen = pic.labeled.clone();
        seen.sort();
        // Desde (+X, −Y, +Z): arriba (0), derecha +X (2) y frente −Y (5)
        assert_eq!(seen, vec![0, 2, 5]);
        let top = render(&cube(), &opts("top", Labels::Faces)).unwrap();
        assert_eq!(top.labeled, vec![0]);
    }

    #[test]
    fn edges_on_the_far_side_are_not_labeled() {
        let pic = render(&cube(), &opts("iso", Labels::Edges)).unwrap();
        assert_eq!(pic.labeled, vec![0], "solo la arista de adelante arriba se ve");
    }
}
