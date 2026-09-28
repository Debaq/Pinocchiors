//! Empaquetado de cartas en el cuadrado UV [0, 1]².
//!
//! Cada carta se escala a su área 3D (densidad de texel uniforme), se gira a
//! su caja mínima y las cajas se acomodan con un skyline de abajo a la
//! izquierda, probando cada caja también girada 90°. El margen entre cartas
//! se da en texels del tamaño de textura destino.

/// Transformación de una carta al atlas: `uv' = (R (s·uv) − min, girada 90°
/// si corresponde, + offset) / side`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Placement {
    scale: f64,
    cos: f64,
    sin: f64,
    min: [f64; 2],
    /// Alto de la caja antes del giro de 90°.
    height: f64,
    rotated: bool,
    offset: [f64; 2],
    side: f64,
}

impl Placement {
    pub fn apply(&self, p: [f64; 2]) -> [f64; 2] {
        let q = [p[0] * self.scale, p[1] * self.scale];
        let r = [self.cos * q[0] - self.sin * q[1] - self.min[0], self.sin * q[0] + self.cos * q[1] - self.min[1]];
        // Giro de +90° (no reflexión: conserva la orientación)
        let r = if self.rotated { [self.height - r[1], r[0]] } else { r };
        [(r[0] + self.offset[0]) / self.side, (r[1] + self.offset[1]) / self.side]
    }
}

/// Carta a empaquetar: sus puntos UV y sus áreas.
pub(crate) struct ChartShape<'a> {
    pub points: &'a [[f64; 2]],
    pub area_3d: f64,
    pub area_uv: f64,
}

/// Cápsula convexa (cadena monótona de Andrew).
fn convex_hull(points: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let mut pts = points.to_vec();
    pts.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    pts.dedup();
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
    let mut hull: Vec<[f64; 2]> = Vec::with_capacity(2 * pts.len());
    for pass in 0..2 {
        let start = hull.len();
        let iter: Box<dyn Iterator<Item = &[f64; 2]>> =
            if pass == 0 { Box::new(pts.iter()) } else { Box::new(pts.iter().rev()) };
        for &p in iter {
            while hull.len() >= start + 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0 {
                hull.pop();
            }
            hull.push(p);
        }
        hull.pop();
    }
    hull
}

/// Giro (cos, sin) que deja la caja de menor área, con su mínimo y tamaño.
fn min_box(points: &[[f64; 2]]) -> (f64, f64, [f64; 2], [f64; 2]) {
    let hull = convex_hull(points);
    let mut candidates = vec![(1.0, 0.0)];
    for k in 0..hull.len() {
        let (a, b) = (hull[k], hull[(k + 1) % hull.len()]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = (dx * dx + dy * dy).sqrt();
        if len > 0.0 {
            // Giro que alinea la arista con el eje x
            candidates.push((dx / len, -dy / len));
        }
    }
    let bounds = |cos: f64, sin: f64| {
        let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        for p in &hull {
            let r = [cos * p[0] - sin * p[1], sin * p[0] + cos * p[1]];
            for c in 0..2 {
                lo[c] = lo[c].min(r[c]);
                hi[c] = hi[c].max(r[c]);
            }
        }
        (lo, [hi[0] - lo[0], hi[1] - lo[1]])
    };
    candidates
        .into_iter()
        .map(|(cos, sin)| {
            let (lo, size) = bounds(cos, sin);
            (cos, sin, lo, size)
        })
        .min_by(|a, b| (a.3[0] * a.3[1]).total_cmp(&(b.3[0] * b.3[1])))
        .expect("al menos el giro nulo")
}

/// Posición (x, y) y giro de 90° de cada caja.
type Layout = Vec<(f64, f64, bool)>;

/// Skyline de abajo a la izquierda en una franja de ancho `width`. Devuelve
/// (x, y, girada) por caja y la altura usada.
fn skyline(sizes: &[[f64; 2]], width: f64) -> (Layout, f64) {
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by(|&a, &b| {
        let key = |i: usize| sizes[i][0].max(sizes[i][1]);
        key(b).total_cmp(&key(a)).then(a.cmp(&b))
    });
    // Segmentos (x, y, ancho) de izquierda a derecha
    let mut segments: Vec<(f64, f64, f64)> = vec![(0.0, 0.0, width)];
    let mut placed = vec![(0.0, 0.0, false); sizes.len()];
    let mut height: f64 = 0.0;
    let eps = width * 1e-12;

    for i in order {
        let mut best: Option<(f64, f64, f64, bool, f64, f64)> = None; // (top, x, y, rot, w, h)
        for rotated in [false, true] {
            let [w, h] = if rotated { [sizes[i][1], sizes[i][0]] } else { sizes[i] };
            for s in 0..segments.len() {
                let x = segments[s].0;
                if x + w > width + eps {
                    break;
                }
                let mut y: f64 = 0.0;
                for seg in &segments[s..] {
                    if seg.0 >= x + w - eps {
                        break;
                    }
                    y = y.max(seg.1);
                }
                let better = best.is_none_or(|b| (y + h, x) < (b.0, b.1));
                if better {
                    best = Some((y + h, x, y, rotated, w, h));
                }
            }
        }
        // La franja siempre cabe al menos la dimensión menor (ver `pack`)
        let (top, x, y, rotated, w, _) = best.expect("franja más angosta que una carta");
        placed[i] = (x, y, rotated);
        height = height.max(top);

        // Reemplazar el tramo [x, x + w) por un segmento a altura `top`
        let end = x + w;
        let mut next = Vec::with_capacity(segments.len() + 2);
        for &(sx, sy, sw) in &segments {
            let se = sx + sw;
            if se <= x + eps || sx >= end - eps {
                next.push((sx, sy, sw));
                continue;
            }
            if sx < x - eps {
                next.push((sx, sy, x - sx));
            }
            if se > end + eps {
                next.push((end, sy, se - end));
            }
        }
        next.push((x, top, w));
        next.sort_by(|a, b| a.0.total_cmp(&b.0));
        // Fusionar vecinos a la misma altura
        segments.clear();
        for seg in next {
            match segments.last_mut() {
                Some(last) if (last.1 - seg.1).abs() <= eps && (last.0 + last.2 - seg.0).abs() <= eps => last.2 += seg.2,
                _ => segments.push(seg),
            }
        }
    }
    (placed, height)
}

/// Acomoda las cartas en [0, 1]². Devuelve la transformación de cada una y
/// la fracción del cuadrado que cubren.
pub(crate) fn pack(charts: &[ChartShape], texture_size: u32, padding: u32) -> (Vec<Placement>, f64) {
    struct Prepared {
        scale: f64,
        cos: f64,
        sin: f64,
        min: [f64; 2],
        size: [f64; 2],
    }
    let prepared: Vec<Prepared> = charts
        .iter()
        .map(|c| {
            let scale = if c.area_uv > 0.0 { (c.area_3d / c.area_uv).sqrt() } else { 1.0 };
            let scaled: Vec<[f64; 2]> = c.points.iter().map(|p| [p[0] * scale, p[1] * scale]).collect();
            let (cos, sin, min, size) = min_box(&scaled);
            Prepared { scale, cos, sin, min, size }
        })
        .collect();
    if prepared.is_empty() {
        return (Vec::new(), 0.0);
    }

    let total_3d: f64 = charts.iter().map(|c| c.area_3d).sum();
    let box_area: f64 = prepared.iter().map(|p| p.size[0] * p.size[1]).sum();
    let min_width = prepared.iter().map(|p| p.size[0].min(p.size[1])).fold(0.0, f64::max);

    // El margen en unidades 3D depende del lado final: iterar
    let mut side = (box_area / 0.6).sqrt().max(min_width).max(1e-12);
    let mut best: Option<(f64, f64, Layout)> = None;
    for _ in 0..5 {
        let pad = 1.1 * padding as f64 * side / texture_size.max(1) as f64;
        let sizes: Vec<[f64; 2]> = prepared.iter().map(|p| [p.size[0] + pad, p.size[1] + pad]).collect();
        let padded_area: f64 = sizes.iter().map(|s| s[0] * s[1]).sum();
        let narrowest = sizes.iter().map(|s| s[0].min(s[1])).fold(0.0, f64::max);
        let mut round: Option<(f64, Layout)> = None;
        for factor in [0.85, 0.95, 1.0, 1.05, 1.15, 1.3, 1.5] {
            let width = (padded_area.sqrt() * factor).max(narrowest);
            let (placed, height) = skyline(&sizes, width);
            let s = width.max(height);
            if round.as_ref().is_none_or(|r| s < r.0) {
                round = Some((s, placed));
            }
        }
        let (s, placed) = round.expect("al menos un ancho");
        side = s;
        best = Some((s, pad, placed));
    }
    let (side, pad, placed) = best.expect("al menos una ronda");

    let placements = prepared
        .iter()
        .zip(placed)
        .map(|(p, (x, y, rotated))| Placement {
            scale: p.scale,
            cos: p.cos,
            sin: p.sin,
            min: p.min,
            height: p.size[1],
            rotated,
            offset: [x + pad / 2.0, y + pad / 2.0],
            side,
        })
        .collect();
    (placements, total_3d / (side * side))
}
