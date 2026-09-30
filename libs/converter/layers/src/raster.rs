//! Dibujo de guías sobre una capa: aristas UV (líneas con antialias) e islas
//! (triángulos rellenos). Coordenadas en píxeles del lienzo, con el centro
//! del píxel `(x, y)` en `(x + 0.5, y + 0.5)`.

use crate::Layer;

/// Segmentos de un color con antialias. Donde se cruzan queda la mayor
/// cobertura (no se acumula: todas las líneas son del mismo color).
pub fn draw_lines(layer: &mut Layer, segments: impl IntoIterator<Item = [[f32; 2]; 2]>, color: [u8; 3], width: f32) {
    let half = width.max(0.5) / 2.0;
    let reach = half.ceil() as i64 + 1;
    let (w, h) = (layer.width as i64, layer.height as i64);
    for [a, b] in segments {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len2 = dx * dx + dy * dy;
        let len = len2.sqrt();
        // Afuera del lienzo no se dibuja (ni se recorre)
        let min_x = a[0].min(b[0]) - half;
        let max_x = a[0].max(b[0]) + half;
        let min_y = a[1].min(b[1]) - half;
        let max_y = a[1].max(b[1]) + half;
        if max_x < 0.0 || max_y < 0.0 || min_x > w as f32 || min_y > h as f32 || !len.is_finite() {
            continue;
        }
        // Se avanza de a medio píxel y se cubre la vecindad de cada paso
        let steps = (len * 2.0).ceil().max(1.0) as usize;
        let mut last = (i64::MIN, i64::MIN);
        for s in 0..=steps {
            let t = s as f32 / steps as f32;
            let cx = (a[0] + dx * t).floor() as i64;
            let cy = (a[1] + dy * t).floor() as i64;
            if (cx, cy) == last {
                continue;
            }
            last = (cx, cy);
            for y in (cy - reach).max(0)..=(cy + reach).min(h - 1) {
                for x in (cx - reach).max(0)..=(cx + reach).min(w - 1) {
                    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                    // Distancia del centro del píxel al segmento
                    let u = if len2 > 0.0 { (((px - a[0]) * dx + (py - a[1]) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
                    let (qx, qy) = (a[0] + dx * u - px, a[1] + dy * u - py);
                    let coverage = (half + 0.5 - (qx * qx + qy * qy).sqrt()).clamp(0.0, 1.0);
                    if coverage <= 0.0 {
                        continue;
                    }
                    let alpha = (coverage * 255.0).round() as u8;
                    let p = &mut layer.pixels[((y * w + x) * 4) as usize..][..4];
                    if alpha > p[3] {
                        p[..3].copy_from_slice(&color);
                        p[3] = alpha;
                    }
                }
            }
        }
    }
}

/// Triángulos rellenos de un color (sin antialias: sirven para seleccionar
/// islas con la varita mágica). Cuenta el píxel cuyo centro cae adentro.
pub fn fill_triangles(layer: &mut Layer, triangles: impl IntoIterator<Item = [[f32; 2]; 3]>, color: [u8; 4]) {
    let (w, h) = (layer.width as i64, layer.height as i64);
    for [a, b, c] in triangles {
        let area = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
        if area == 0.0 || !area.is_finite() {
            continue;
        }
        let x0 = (a[0].min(b[0]).min(c[0]).floor() as i64).max(0);
        let x1 = (a[0].max(b[0]).max(c[0]).ceil() as i64).min(w - 1);
        let y0 = (a[1].min(b[1]).min(c[1]).floor() as i64).max(0);
        let y1 = (a[1].max(b[1]).max(c[1]).ceil() as i64).min(h - 1);
        let edge = |p: [f32; 2], q: [f32; 2], x: f32, y: f32| ((q[0] - p[0]) * (y - p[1]) - (q[1] - p[1]) * (x - p[0])) * area.signum();
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                if edge(a, b, px, py) >= 0.0 && edge(b, c, px, py) >= 0.0 && edge(c, a, px, py) >= 0.0 {
                    layer.pixels[((y * w + x) * 4) as usize..][..4].copy_from_slice(&color);
                }
            }
        }
    }
}

/// Filas de 5 bits (el bit 4 es la columna izquierda) de un carácter de la
/// fuente de 5 × 7. Minúsculas y vocales con tilde se dibujan en mayúscula
/// sin tilde; lo que no está, como espacio.
fn glyph(c: char) -> [u8; 7] {
    let c = match c.to_uppercase().next().unwrap_or(' ') {
        'Á' | 'À' | 'Â' | 'Ä' => 'A',
        'É' | 'È' | 'Ê' | 'Ë' => 'E',
        'Í' | 'Ì' | 'Î' | 'Ï' => 'I',
        'Ó' | 'Ò' | 'Ô' | 'Ö' => 'O',
        'Ú' | 'Ù' | 'Û' | 'Ü' => 'U',
        other => other,
    };
    match c {
        'A' => [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'B' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110],
        'C' => [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110],
        'D' => [0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110],
        'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
        'F' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000],
        'G' => [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111],
        'H' => [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'I' => [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        'J' => [0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100],
        'K' => [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001],
        'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        'M' => [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001],
        'N' => [0b10001, 0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001],
        'O' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'P' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
        'Q' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101],
        'R' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001],
        'S' => [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110],
        'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        'U' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'V' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100],
        'W' => [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010],
        'X' => [0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001],
        'Y' => [0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100],
        'Z' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111],
        '0' => [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
        '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        '2' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
        '3' => [0b11111, 0b00010, 0b00100, 0b00010, 0b00001, 0b10001, 0b01110],
        '4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
        '5' => [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110],
        '6' => [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
        '7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
        '8' => [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
        '9' => [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100],
        '-' => [0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000],
        '.' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b01100, 0b01100],
        'Ñ' => [0b01101, 0b10010, 0b00000, 0b10001, 0b11001, 0b10101, 0b10011],
        _ => [0; 7],
    }
}

/// Texto en mayúsculas con una fuente de 5 × 7 celdas de `scale` píxeles,
/// centrado en `center` y con un borde de `outline` alrededor de cada trazo
/// para que se lea sobre cualquier fondo. Un texto largo se parte en dos
/// líneas por el espacio más cercano al medio.
pub fn draw_text(layer: &mut Layer, text: &str, center: [f32; 2], scale: u32, color: [u8; 3], outline: [u8; 3]) {
    let scale = scale.max(1) as i64;
    let chars: Vec<char> = text.trim().chars().collect();
    let lines: Vec<&[char]> = if chars.len() > 14 {
        let middle = chars.len() / 2;
        match (0..chars.len()).filter(|&i| chars[i] == ' ').min_by_key(|&i| i.abs_diff(middle)) {
            Some(i) => vec![&chars[..i], &chars[i + 1..]],
            None => vec![&chars[..]],
        }
    } else {
        vec![&chars[..]]
    };
    // Celdas: 6 por carácter (5 + 1 de separación), 9 por línea (7 + 2)
    let height = 9 * lines.len() as i64 - 2;
    let border = (scale / 2).max(1);
    let (w, h) = (layer.width as i64, layer.height as i64);
    let mut paint = |x0: i64, y0: i64, size: i64, rgb: [u8; 3]| {
        for y in y0.max(0)..(y0 + size).min(h) {
            for x in x0.max(0)..(x0 + size).min(w) {
                let p = &mut layer.pixels[((y * w + x) * 4) as usize..][..4];
                p[..3].copy_from_slice(&rgb);
                p[3] = 255;
            }
        }
    };
    // Primero todo el borde, después las letras: el borde no tapa trazos vecinos
    for pass in 0..2 {
        for (row, line) in lines.iter().enumerate() {
            let width = 6 * line.len() as i64 - 1;
            let left = center[0].round() as i64 - width * scale / 2;
            let top = center[1].round() as i64 - height * scale / 2 + 9 * row as i64 * scale;
            for (k, &c) in line.iter().enumerate() {
                for (gy, bits) in glyph(c).iter().enumerate() {
                    for gx in 0..5 {
                        if bits & (0b10000 >> gx) == 0 {
                            continue;
                        }
                        let x = left + (6 * k as i64 + gx) * scale;
                        let y = top + gy as i64 * scale;
                        if pass == 0 {
                            paint(x - border, y - border, scale + 2 * border, outline);
                        } else {
                            paint(x, y, scale, color);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha(layer: &Layer, x: u32, y: u32) -> u8 {
        layer.pixels[((y * layer.width + x) * 4 + 3) as usize]
    }

    #[test]
    fn horizontal_line_covers_its_row() {
        let mut layer = Layer::new("malla", 10, 5);
        draw_lines(&mut layer, [[[1.0, 2.5], [8.0, 2.5]]], [0, 255, 0], 1.0);
        assert_eq!(alpha(&layer, 4, 2), 255);
        assert_eq!(alpha(&layer, 4, 0), 0);
        assert_eq!(alpha(&layer, 4, 4), 0);
        assert_eq!(&layer.pixels[(2 * 10 + 4) * 4..][..3], &[0, 255, 0]);
        // Fuera del segmento
        assert_eq!(alpha(&layer, 9, 2), 0);
    }

    #[test]
    fn lines_outside_the_canvas_are_ignored() {
        let mut layer = Layer::new("malla", 4, 4);
        draw_lines(&mut layer, [[[-10.0, -10.0], [-5.0, -2.0]], [[f32::NAN, 0.0], [1.0, 1.0]]], [255; 3], 2.0);
        assert!(layer.pixels.iter().all(|&p| p == 0));
    }

    #[test]
    fn text_is_centered_with_an_outline() {
        let mut layer = Layer::new("nombres", 40, 20);
        draw_text(&mut layer, "Pata", [20.0, 10.0], 1, [255, 255, 255], [0, 0, 0]);
        let color = |x: u32, y: u32| &layer.pixels[((y * 40 + x) * 4) as usize..][..4];
        // 4 letras: 23 celdas de ancho centradas en x = 20 → desde x = 9;
        // 7 de alto centradas en y = 10 → desde y = 7
        assert_eq!(color(8, 7), &[0, 0, 0, 255], "borde a la izquierda de la P");
        assert_eq!(color(9, 7), &[255, 255, 255, 255], "trazo superior de la P");
        assert_eq!(color(13, 7), &[0, 0, 0, 255], "la P termina en su cuarta columna");
        assert_eq!(color(0, 0)[3], 0, "fuera del texto no se pinta");
        // Minúsculas y tildes: misma figura que la mayúscula sin tilde
        let (mut a, mut b) = (Layer::new("a", 10, 10), Layer::new("b", 10, 10));
        draw_text(&mut a, "á", [5.0, 5.0], 1, [255; 3], [0; 3]);
        draw_text(&mut b, "A", [5.0, 5.0], 1, [255; 3], [0; 3]);
        assert_eq!(a.pixels, b.pixels);
    }

    #[test]
    fn long_text_wraps_in_two_lines() {
        let mut layer = Layer::new("nombres", 120, 40);
        draw_text(&mut layer, "Pata delantera izquierda", [60.0, 20.0], 1, [255; 3], [0; 3]);
        // Dos líneas de 7 celdas con 2 de separación: filas pintadas arriba y abajo del centro
        let painted_row = |y: u32| (0..120).any(|x| layer.pixels[((y * 120 + x) * 4 + 3) as usize] > 0);
        assert!(painted_row(13) && painted_row(25));
        // Cada línea cabe en el ancho (la frase entera no cabría: 24 × 6 celdas)
        assert!(!(0..40).any(|y| layer.pixels[((y * 120) * 4 + 3) as usize] > 0));
    }

    #[test]
    fn triangle_fill_counts_centers_inside() {
        let mut layer = Layer::new("islas", 4, 4);
        // Mitad inferior izquierda del cuadrado (en cualquier orientación)
        fill_triangles(&mut layer, [[[0.0, 0.0], [0.0, 4.0], [4.0, 4.0]]], [1, 2, 3, 100]);
        assert_eq!(alpha(&layer, 0, 3), 100);
        assert_eq!(alpha(&layer, 3, 0), 0);
        let filled = layer.pixels.chunks(4).filter(|p| p[3] > 0).count();
        assert_eq!(filled, 10, "4 + 3 + 2 + 1 con la diagonal incluida");
    }
}
