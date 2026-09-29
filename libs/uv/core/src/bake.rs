//! Horneado: pasar texturas de una superficie de referencia a un mapa UV
//! nuevo.
//!
//! Se rasteriza la malla destino en su espacio UV y, por cada texel, se busca
//! el punto de la superficie de referencia: el primero sobre la normal del
//! texel (hacia afuera o hacia adentro, a lo sumo una arista media de la cara)
//! en una cara orientada como la destino y, si no hay, el más cercano. El
//! texel recibe el grupo, las UV originales y los marcos tangentes de ambos
//! lados. Como cada texel lee de un solo triángulo original, no hay
//! extrapolación entre islas ni costuras visibles. Cada canal es una función
//! del contexto del texel; al final los bordes de las islas se dilatan.

use crate::{CornerFrames, UvSurface};
use pinocchio_math::Vector3;
use rayon::prelude::*;

/// Lo que ve un texel del mapa nuevo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TexelContext {
    /// Grupo del triángulo de referencia.
    pub group: usize,
    /// UV originales en el punto de referencia.
    pub uv: [f32; 2],
    /// `[T, B, N]` de la superficie de referencia en ese punto (convención
    /// glTF, ver [`UvSurface::frame_at`]).
    pub source_frame: [[f32; 3]; 3],
    /// `[T, B, N]` de la malla destino en el texel, interpolado de sus
    /// [`CornerFrames`] como lo hace el visor.
    pub target_frame: [[f32; 3]; 3],
}

/// Imágenes horneadas, fila por fila (fila 0 = v cercana a 0, como glTF).
pub struct Baked {
    pub width: u32,
    pub height: u32,
    /// Una imagen RGBA por canal pedido.
    pub images: Vec<Vec<[u8; 4]>>,
    /// Fracción de texels dentro de alguna isla.
    pub coverage: f64,
}

/// Canal a hornear: color del texel según su contexto.
pub type BakeChannel<'a> = &'a (dyn Fn(&TexelContext) -> [u8; 4] + Sync);

fn to_f32(v: Vector3) -> [f32; 3] {
    [v.x() as f32, v.y() as f32, v.z() as f32]
}

fn from_f32(v: [f32; 3]) -> Vector3 {
    Vector3::new(v[0] as f64, v[1] as f64, v[2] as f64)
}

/// Hornea `channels` en imágenes `width × height` sobre las UV `corners` de
/// la malla destino (caras de `N` vértices, triangulación en abanico). Los
/// bordes de las islas se extienden `padding` texels.
#[allow(clippy::too_many_arguments)]
pub fn bake<const N: usize>(
    surface: &UvSurface,
    positions: &[[f64; 3]],
    faces: &[[usize; N]],
    corners: &[[[f32; 2]; N]],
    frames: &CornerFrames<N>,
    width: u32,
    height: u32,
    padding: u32,
    channels: &[BakeChannel],
) -> Baked {
    let (w, h) = (width as usize, height as usize);
    let per_face = N.saturating_sub(2).max(1);

    // Triángulo destino que cubre cada texel
    let mut owner = vec![u32::MAX; w * h];
    for (f, uvs) in corners.iter().enumerate() {
        for k in 1..N.saturating_sub(1) {
            let t = [0, k, k + 1].map(|i| [uvs[i][0] as f64 * w as f64, uvs[i][1] as f64 * h as f64]);
            let area = (t[1][0] - t[0][0]) * (t[2][1] - t[0][1]) - (t[1][1] - t[0][1]) * (t[2][0] - t[0][0]);
            if area.abs() < 1e-12 {
                continue;
            }
            let x0 = t.iter().map(|q| q[0]).fold(f64::MAX, f64::min).floor().max(0.0) as usize;
            let x1 = (t.iter().map(|q| q[0]).fold(f64::MIN, f64::max).ceil().max(0.0) as usize).min(w);
            let y0 = t.iter().map(|q| q[1]).fold(f64::MAX, f64::min).floor().max(0.0) as usize;
            let y1 = (t.iter().map(|q| q[1]).fold(f64::MIN, f64::max).ceil().max(0.0) as usize).min(h);
            for y in y0..y1 {
                for x in x0..x1 {
                    if barycentric_2d(&t, area, x, y).iter().all(|&l| l >= -1e-6) {
                        owner[y * w + x] = (f * per_face + k - 1) as u32;
                    }
                }
            }
        }
    }

    // Alcance del rayo por la normal: la arista media de cada cara destino
    let reach: Vec<f64> = faces
        .iter()
        .map(|face| {
            (0..N).map(|i| from_f64(positions[face[i]]).distance(&from_f64(positions[face[(i + 1) % N]]))).sum::<f64>()
                / N as f64
        })
        .collect();

    let context = |x: usize, y: usize| -> Option<TexelContext> {
        let id = owner[y * w + x];
        if id == u32::MAX {
            return None;
        }
        let (f, k) = (id as usize / per_face, id as usize % per_face + 1);
        let ids = [0, k, k + 1];
        let uvs = &corners[f];
        let t = ids.map(|i| [uvs[i][0] as f64 * w as f64, uvs[i][1] as f64 * h as f64]);
        let area = (t[1][0] - t[0][0]) * (t[2][1] - t[0][1]) - (t[1][1] - t[0][1]) * (t[2][0] - t[0][0]);
        let l = barycentric_2d(&t, area, x, y);

        let p = ids
            .iter()
            .zip(l)
            .fold(Vector3::zero(), |acc, (&i, li)| acc + from_f64(positions[faces[f][i]]) * li);

        // Marco destino: interpolado y reortogonalizado, como en el sombreador
        let normal = ids.iter().zip(l).fold(Vector3::zero(), |acc, (&i, li)| acc + from_f32(frames.normals[f][i]) * li);
        let tangent = ids.iter().zip(l).fold(Vector3::zero(), |acc, (&i, li)| {
            let t = frames.tangents[f][i];
            acc + from_f32([t[0], t[1], t[2]]) * li
        });
        let sign = ids.iter().zip(l).map(|(&i, li)| frames.tangents[f][i][3] as f64 * li).sum::<f64>();
        let n = normal.try_normalize();

        // Punto del original: por la normal y, si no hay, el más cercano
        let hit = n
            .and_then(|n| surface.project_along(&p, &n, reach[f]))
            .unwrap_or_else(|| surface.closest(&p));
        let uv = surface.uv_at(hit.triangle, &hit.point);
        let source = surface.frame_at(hit.triangle, &hit.point);

        let n = n.unwrap_or(Vector3::unit_z());
        let t_axis = (tangent - n * n.dot(&tangent)).try_normalize().unwrap_or(source[0]);
        let b = n.cross(&t_axis) * if sign >= 0.0 { 1.0 } else { -1.0 };

        Some(TexelContext {
            group: surface.group(hit.triangle),
            uv: uv.map(|c| c as f32),
            source_frame: source.map(to_f32),
            target_frame: [to_f32(t_axis), to_f32(b), to_f32(n)],
        })
    };

    // Una fila por tarea: máscara de cobertura y color por canal
    let rows: Vec<Row> = (0..h)
        .into_par_iter()
        .map(|y| {
            let mut mask = vec![false; w];
            let mut colors = vec![vec![[0u8; 4]; w]; channels.len()];
            for x in 0..w {
                if let Some(ctx) = context(x, y) {
                    mask[x] = true;
                    for (c, channel) in channels.iter().enumerate() {
                        colors[c][x] = channel(&ctx);
                    }
                }
            }
            (mask, colors)
        })
        .collect();

    let mask: Vec<bool> = rows.iter().flat_map(|(m, _)| m.iter().copied()).collect();
    let coverage = mask.iter().filter(|&&m| m).count() as f64 / mask.len().max(1) as f64;
    let images = (0..channels.len())
        .map(|c| {
            let image: Vec<[u8; 4]> = rows.iter().flat_map(|(_, colors)| colors[c].iter().copied()).collect();
            dilate(image, &mask, w, h, padding)
        })
        .collect();
    Baked { width, height, images, coverage }
}

/// Fila horneada: máscara de cobertura y color por canal.
type Row = (Vec<bool>, Vec<Vec<[u8; 4]>>);

fn from_f64(p: [f64; 3]) -> Vector3 {
    Vector3::new(p[0], p[1], p[2])
}

/// Baricéntricas del centro del texel `(x, y)` en el triángulo `t` (en px).
fn barycentric_2d(t: &[[f64; 2]; 3], area: f64, x: usize, y: usize) -> [f64; 3] {
    let c = [x as f64 + 0.5, y as f64 + 0.5];
    let edge = |a: [f64; 2], b: [f64; 2]| ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])) / area;
    [edge(t[1], t[2]), edge(t[2], t[0]), edge(t[0], t[1])]
}

/// Extiende los bordes de las islas `padding` texels (promedio de vecinos ya
/// llenos) y llena el resto con el color medio, para que el filtrado y los
/// mipmaps no mezclen el fondo en los bordes.
fn dilate(mut image: Vec<[u8; 4]>, mask: &[bool], w: usize, h: usize, padding: u32) -> Vec<[u8; 4]> {
    let mut filled = mask.to_vec();
    for _ in 0..padding {
        let grown: Vec<Option<[u8; 4]>> = (0..image.len())
            .into_par_iter()
            .map(|i| {
                if filled[i] {
                    return None;
                }
                let (x, y) = ((i % w) as i64, (i / w) as i64);
                let mut sum = [0u32; 4];
                let mut count = 0;
                for (dx, dy) in [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                        continue;
                    }
                    let j = ny as usize * w + nx as usize;
                    if filled[j] {
                        for k in 0..4 {
                            sum[k] += image[j][k] as u32;
                        }
                        count += 1;
                    }
                }
                (count > 0).then(|| sum.map(|s| ((s + count / 2) / count) as u8))
            })
            .collect();
        let mut changed = false;
        for (i, c) in grown.into_iter().enumerate() {
            if let Some(c) = c {
                image[i] = c;
                filled[i] = true;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let count = filled.iter().filter(|&&f| f).count().max(1) as u64;
    let mut sum = [0u64; 4];
    for (c, _) in image.iter().zip(&filled).filter(|(_, f)| **f) {
        for k in 0..4 {
            sum[k] += c[k] as u64;
        }
    }
    let mean = sum.map(|s| (s / count) as u8);
    for (c, f) in image.iter_mut().zip(&filled) {
        if !f {
            *c = mean;
        }
    }
    image
}
