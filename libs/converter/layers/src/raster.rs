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
