//! Miniaturas de la placa para la pantalla de la impresora.
//!
//! Rasterizador por software: vista en perspectiva aérea (desde el frente-derecha,
//! como la cámara de Orca), sombreado difuso y fondo transparente, con 3×3 muestras
//! por píxel. Sale RGBA con la fila de abajo primero (lo que espera el motor).

/// Una vista de las piezas en `width × height`.
pub fn render_thumbnail(parts: &[Vec<[[f32; 3]; 3]>], width: u32, height: u32, color: [u8; 3]) -> Vec<u8> {
    const SS: usize = 3;
    let (w, h) = (width as usize * SS, height as usize * SS);
    let mut depth = vec![f32::INFINITY; w * h];
    let mut shade = vec![0f32; w * h];

    let tris: Vec<&[[f32; 3]; 3]> = parts.iter().flatten().collect();
    if tris.is_empty() || width == 0 || height == 0 {
        return vec![0; width as usize * height as usize * 4];
    }

    // Cámara: girada 30° alrededor de Z y mirando 35° hacia abajo.
    let (yaw, pitch) = (30f32.to_radians(), 35f32.to_radians());
    let view = |p: &[f32; 3]| -> [f32; 3] {
        let (sy, cy) = yaw.sin_cos();
        let x = p[0] * cy - p[1] * sy;
        let y = p[0] * sy + p[1] * cy;
        let (sp, cp) = pitch.sin_cos();
        // La cámara mira hacia +Y y hacia abajo: dirección (0, cp, -sp), arriba (0, sp, cp).
        // Pantalla: x a la derecha, v hacia arriba, d profundidad (más lejos = mayor).
        [x, y * sp + p[2] * cp, y * cp - p[2] * sp]
    };
    let light = normalize([-0.4, 0.5, -0.75]);

    let mut min = [f32::MAX; 2];
    let mut max = [f32::MIN; 2];
    let projected: Vec<[[f32; 3]; 3]> = tris
        .iter()
        .map(|t| {
            let v = [view(&t[0]), view(&t[1]), view(&t[2])];
            for q in &v {
                min[0] = min[0].min(q[0]);
                min[1] = min[1].min(q[1]);
                max[0] = max[0].max(q[0]);
                max[1] = max[1].max(q[1]);
            }
            v
        })
        .collect();
    // Encuadre con 8 % de margen, misma escala en los dos ejes.
    let span = [(max[0] - min[0]).max(1e-6), (max[1] - min[1]).max(1e-6)];
    let scale = 0.84 * (w as f32 / span[0]).min(h as f32 / span[1]);
    let center = [(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0];
    let to_px = |q: &[f32; 3]| [(q[0] - center[0]) * scale + w as f32 / 2.0, (q[1] - center[1]) * scale + h as f32 / 2.0, q[2]];

    for tri in &projected {
        let a = to_px(&tri[0]);
        let b = to_px(&tri[1]);
        let c = to_px(&tri[2]);
        let n = normalize(cross(sub(tri[1], tri[0]), sub(tri[2], tri[0])));
        // Las caras se ven por cualquier lado (mallas con normales invertidas)
        let lit = 0.35 + 0.65 * dot(n, light).abs();
        let area = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
        if area.abs() < 1e-9 {
            continue;
        }
        let x0 = a[0].min(b[0]).min(c[0]).floor().max(0.0) as usize;
        let x1 = (a[0].max(b[0]).max(c[0]).ceil() as usize).min(w - 1);
        let y0 = a[1].min(b[1]).min(c[1]).floor().max(0.0) as usize;
        let y1 = (a[1].max(b[1]).max(c[1]).ceil() as usize).min(h - 1);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let p = [x as f32 + 0.5, y as f32 + 0.5];
                let w0 = ((b[0] - p[0]) * (c[1] - p[1]) - (b[1] - p[1]) * (c[0] - p[0])) / area;
                let w1 = ((c[0] - p[0]) * (a[1] - p[1]) - (c[1] - p[1]) * (a[0] - p[0])) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let d = w0 * a[2] + w1 * b[2] + w2 * c[2];
                let i = y * w + x;
                if d < depth[i] {
                    depth[i] = d;
                    shade[i] = lit;
                }
            }
        }
    }

    // Promedio de las muestras; fila 0 = abajo (el rasterizado ya tiene v hacia arriba).
    let mut out = vec![0u8; width as usize * height as usize * 4];
    for y in 0..height as usize {
        for x in 0..width as usize {
            let (mut cover, mut lum) = (0f32, 0f32);
            for sy in 0..SS {
                for sx in 0..SS {
                    let i = (y * SS + sy) * w + x * SS + sx;
                    if depth[i].is_finite() {
                        cover += 1.0;
                        lum += shade[i];
                    }
                }
            }
            if cover == 0.0 {
                continue;
            }
            let l = lum / cover;
            let o = (y * width as usize + x) * 4;
            for k in 0..3 {
                out[o + k] = (color[k] as f32 * l).round().clamp(0.0, 255.0) as u8;
            }
            out[o + 3] = (255.0 * cover / (SS * SS) as f32).round() as u8;
        }
    }
    out
}

/// Color `#RRGGBB` (o `#RRGGBBAA`) del filamento; naranja si no se entiende.
pub fn parse_color(text: &str) -> [u8; 3] {
    let hex = text.trim().trim_start_matches('#');
    let byte = |i: usize| hex.get(i..i + 2).and_then(|s| u8::from_str_radix(s, 16).ok());
    match (byte(0), byte(2), byte(4)) {
        (Some(r), Some(g), Some(b)) => [r, g, b],
        _ => [0xF2, 0x8C, 0x28],
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn normalize(a: [f32; 3]) -> [f32; 3] {
    let l = dot(a, a).sqrt();
    if l > 0.0 { [a[0] / l, a[1] / l, a[2] / l] } else { a }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube(s: f32) -> Vec<[[f32; 3]; 3]> {
        let p = |a: f32, b: f32, c: f32| [a * s, b * s, c * s];
        let v = [p(0., 0., 0.), p(1., 0., 0.), p(1., 1., 0.), p(0., 1., 0.), p(0., 0., 1.), p(1., 0., 1.), p(1., 1., 1.), p(0., 1., 1.)];
        let f = [[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]];
        f.iter().map(|t| [v[t[0]], v[t[1]], v[t[2]]]).collect()
    }

    #[test]
    fn el_cubo_llena_el_centro_y_deja_las_esquinas_transparentes() {
        let img = render_thumbnail(&[cube(20.0)], 96, 96, [200, 100, 50]);
        assert_eq!(img.len(), 96 * 96 * 4);
        let alpha = |x: usize, y: usize| img[(y * 96 + x) * 4 + 3];
        assert_eq!(alpha(48, 48), 255);
        assert_eq!(alpha(0, 0), 0);
        assert_eq!(alpha(95, 95), 0);
        // la tapa (arriba en la imagen = filas altas) se ve más clara o distinta a los lados: hay al menos dos tonos
        let mut tones: Vec<u8> = (0..96 * 96).filter(|i| img[i * 4 + 3] == 255).map(|i| img[i * 4]).collect();
        tones.sort();
        tones.dedup();
        assert!(tones.len() >= 2);
    }

    #[test]
    fn colores() {
        assert_eq!(parse_color("#FF8000"), [255, 128, 0]);
        assert_eq!(parse_color("#00ff00ff"), [0, 255, 0]);
        assert_eq!(parse_color("rojo"), [0xF2, 0x8C, 0x28]);
    }
}
