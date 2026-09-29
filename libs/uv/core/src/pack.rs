//! Empaquetado de cartas en el cuadrado UV [0, 1]².
//!
//! Cada carta se escala a su área 3D (densidad de texel uniforme) y se gira a
//! su caja mínima. Dos métodos, gana el que deja el cuadrado más chico:
//!
//! - **Rasterizado** (como xatlas): cada carta se rasteriza de forma
//!   conservadora en una grilla y se busca, en cuatro giros de 90°, la
//!   posición más baja donde su máscara dilatada por el margen no pisa nada.
//!   Las cartas encajan en los huecos de las otras.
//! - **Skyline** de cajas, de abajo a la izquierda, probando el giro de 90°.
//!
//! El margen entre cartas se da en texels del tamaño de textura destino.

use rayon::prelude::*;

/// Transformación de una carta al atlas: `uv' = (giro k·90°(R (s·uv) − min)
/// + offset) / side`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Placement {
    scale: f64,
    cos: f64,
    sin: f64,
    min: [f64; 2],
    /// Tamaño de la caja antes del giro de k·90°.
    size: [f64; 2],
    /// Giros de +90° (0..4).
    turns: u8,
    offset: [f64; 2],
    side: f64,
}

/// Gira `r` (dentro de la caja `[0, w] × [0, h]`) `turns` veces +90° y lo
/// deja otra vez en el cuadrante positivo. Nunca refleja: conserva la
/// orientación.
fn turn(r: [f64; 2], size: [f64; 2], turns: u8) -> [f64; 2] {
    let [w, h] = size;
    match turns % 4 {
        0 => r,
        1 => [h - r[1], r[0]],
        2 => [w - r[0], h - r[1]],
        _ => [r[1], w - r[0]],
    }
}

/// Tamaño de la caja tras `turns` giros de 90°.
fn turned_size(size: [f64; 2], turns: u8) -> [f64; 2] {
    if turns % 2 == 1 { [size[1], size[0]] } else { size }
}

impl Placement {
    /// Punto de la carta en su caja mínima, antes de girar y desplazar.
    fn local(&self, p: [f64; 2]) -> [f64; 2] {
        let q = [p[0] * self.scale, p[1] * self.scale];
        [self.cos * q[0] - self.sin * q[1] - self.min[0], self.sin * q[0] + self.cos * q[1] - self.min[1]]
    }

    pub fn apply(&self, p: [f64; 2]) -> [f64; 2] {
        let r = turn(self.local(p), self.size, self.turns);
        [(r[0] + self.offset[0]) / self.side, (r[1] + self.offset[1]) / self.side]
    }
}

/// Carta a empaquetar: sus puntos UV, sus triángulos (índices a `points`) y
/// sus áreas.
pub(crate) struct ChartShape<'a> {
    pub points: &'a [[f64; 2]],
    pub triangles: &'a [[usize; 3]],
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

/// Carta escalada y girada a su caja mínima.
struct Prepared {
    scale: f64,
    cos: f64,
    sin: f64,
    min: [f64; 2],
    size: [f64; 2],
    /// Puntos en la caja `[0, size]`.
    local: Vec<[f64; 2]>,
}

impl Prepared {
    fn placement(&self, turns: u8, offset: [f64; 2], side: f64) -> Placement {
        Placement { scale: self.scale, cos: self.cos, sin: self.sin, min: self.min, size: self.size, turns, offset, side }
    }
}

/// Máscara de bits: `rows × width` celdas, cada fila en palabras de 64.
#[derive(Clone)]
struct Mask {
    width: usize,
    words: usize,
    rows: Vec<Vec<u64>>,
}

impl Mask {
    fn new(width: usize, height: usize) -> Self {
        let words = width.div_ceil(64) + 1;
        Self { width, words, rows: vec![vec![0; words]; height] }
    }

    fn height(&self) -> usize {
        self.rows.len()
    }

    fn set(&mut self, x: usize, y: usize) {
        self.rows[y][x / 64] |= 1 << (x % 64);
    }

    fn get(&self, x: usize, y: usize) -> bool {
        self.rows[y][x / 64] & (1 << (x % 64)) != 0
    }

    /// Dilatación cuadrada de `radius` celdas; la máscara crece `radius` por
    /// lado.
    fn dilate(&self, radius: usize) -> Mask {
        let (w, h) = (self.width + 2 * radius, self.height() + 2 * radius);
        let mut out = Mask::new(w, h);
        // Horizontal: por fila, cada celda marcada cubre [x, x + 2r]
        let mut horizontal = Mask::new(w, self.height());
        for y in 0..self.height() {
            for x in 0..self.width {
                if self.get(x, y) {
                    for dx in 0..=2 * radius {
                        horizontal.set(x + dx, y);
                    }
                }
            }
        }
        // Vertical: OR de las filas [y − 2r, y]
        for y in 0..h {
            let lo = y.saturating_sub(2 * radius);
            let hi = y.min(self.height().saturating_sub(1));
            if lo > hi || lo >= self.height() {
                continue;
            }
            for src in &horizontal.rows[lo..=hi] {
                for (dst, s) in out.rows[y].iter_mut().zip(src) {
                    *dst |= s;
                }
            }
        }
        out
    }
}

/// ¿El triángulo toca la celda `[x0, x0 + 1] × [y0, y0 + 1]`? Ejes
/// separadores: las cajas ya se solapan (recorremos la caja del triángulo),
/// quedan las normales de sus aristas.
fn triangle_touches_cell(t: &[[f64; 2]; 3], x0: f64, y0: f64) -> bool {
    let area = (t[1][0] - t[0][0]) * (t[2][1] - t[0][1]) - (t[1][1] - t[0][1]) * (t[2][0] - t[0][0]);
    if area.abs() < 1e-12 {
        return true;
    }
    let corners = [[x0, y0], [x0 + 1.0, y0], [x0, y0 + 1.0], [x0 + 1.0, y0 + 1.0]];
    (0..3).all(|k| {
        let (a, b) = (t[k], t[(k + 1) % 3]);
        corners.iter().any(|p| ((b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])) * area.signum() >= -1e-9)
    })
}

/// Rasterizado conservador de la carta girada `turns` veces, con celdas de
/// lado `cell`: marca toda celda que toque algún triángulo o punto.
fn rasterize(chart: &Prepared, triangles: &[[usize; 3]], turns: u8, cell: f64) -> Mask {
    let size = turned_size(chart.size, turns);
    let (w, h) = ((size[0] / cell).floor() as usize + 1, (size[1] / cell).floor() as usize + 1);
    let mut mask = Mask::new(w, h);
    let pts: Vec<[f64; 2]> = chart.local.iter().map(|&p| turn(p, chart.size, turns).map(|c| c / cell)).collect();
    let clamp_x = |v: f64| (v.max(0.0) as usize).min(w - 1);
    let clamp_y = |v: f64| (v.max(0.0) as usize).min(h - 1);
    for p in &pts {
        mask.set(clamp_x(p[0]), clamp_y(p[1]));
    }
    for tri in triangles {
        let t = tri.map(|i| pts[i]);
        let (x0, x1) = (t.iter().map(|p| p[0]).fold(f64::MAX, f64::min), t.iter().map(|p| p[0]).fold(f64::MIN, f64::max));
        let (y0, y1) = (t.iter().map(|p| p[1]).fold(f64::MAX, f64::min), t.iter().map(|p| p[1]).fold(f64::MIN, f64::max));
        for y in clamp_y(y0)..=clamp_y(y1) {
            for x in clamp_x(x0)..=clamp_x(x1) {
                if !mask.get(x, y) && triangle_touches_cell(&t, x as f64, y as f64) {
                    mask.set(x, y);
                }
            }
        }
    }
    mask
}

/// Atlas de ancho fijo que crece hacia arriba.
struct Atlas {
    mask: Mask,
}

impl Atlas {
    /// ¿Cabe `m` con su esquina en (x, y)?
    fn fits(&self, m: &Mask, x: usize, y: usize) -> bool {
        let (word, shift) = (x / 64, x % 64);
        for (r, row) in m.rows.iter().enumerate() {
            let Some(dst) = self.mask.rows.get(y + r) else {
                return true;
            };
            for (k, &bits) in row.iter().enumerate() {
                if bits == 0 {
                    continue;
                }
                if dst[word + k] & (bits << shift) != 0 {
                    return false;
                }
                if shift != 0 && word + k + 1 < dst.len() && dst[word + k + 1] & (bits >> (64 - shift)) != 0 {
                    return false;
                }
            }
        }
        true
    }

    /// Primera x en `0..=max_x` donde cabe la máscara dilatada con su esquina
    /// en la fila `y`. Las celdas de muestra descartan en bloque (64 x por
    /// palabra) antes de probar la máscara entera.
    fn first_fit(&self, o: &Oriented, y: usize, max_x: usize) -> Option<usize> {
        let words = self.mask.words;
        // Candidatas: bits x ≤ max_x
        let mut free = vec![0u64; words];
        for (k, word) in free.iter_mut().enumerate() {
            let lo = k * 64;
            if lo > max_x {
                break;
            }
            let n = (max_x - lo + 1).min(64);
            *word = if n == 64 { u64::MAX } else { (1u64 << n) - 1 };
        }
        for &(dx, dy) in &o.samples {
            let Some(row) = self.mask.rows.get(y + dy) else {
                continue;
            };
            // x libre si la celda (x + dx) de la fila está vacía
            let (ws, bs) = (dx / 64, dx % 64);
            for (k, word) in free.iter_mut().enumerate() {
                let lo = row.get(k + ws).copied().unwrap_or(0);
                let hi = row.get(k + ws + 1).copied().unwrap_or(0);
                let shifted = if bs == 0 { lo } else { (lo >> bs) | (hi << (64 - bs)) };
                *word &= !shifted;
            }
        }
        for (k, &word) in free.iter().enumerate() {
            let mut bits = word;
            while bits != 0 {
                let x = k * 64 + bits.trailing_zeros() as usize;
                if self.fits(&o.dilated, x, y) {
                    return Some(x);
                }
                bits &= bits - 1;
            }
        }
        None
    }

    fn stamp(&mut self, m: &Mask, x: usize, y: usize) {
        let needed = y + m.height();
        if self.mask.rows.len() < needed {
            self.mask.rows.resize(needed, vec![0; self.mask.words]);
        }
        let (word, shift) = (x / 64, x % 64);
        for (r, row) in m.rows.iter().enumerate() {
            let dst = &mut self.mask.rows[y + r];
            for (k, &bits) in row.iter().enumerate() {
                if bits == 0 {
                    continue;
                }
                dst[word + k] |= bits << shift;
                if shift != 0 && word + k + 1 < dst.len() {
                    dst[word + k + 1] |= bits >> (64 - shift);
                }
            }
        }
    }
}

/// Máscaras de una carta en un giro: la propia (se estampa) y la dilatada
/// por el margen (se prueba contra lo estampado).
struct Oriented {
    raw: Mask,
    dilated: Mask,
    /// Celdas (x, y) de la dilatada para descartar posiciones rápido: los
    /// extremos de algunas filas.
    samples: Vec<(usize, usize)>,
}

impl Oriented {
    fn new(raw: Mask, radius: usize) -> Self {
        let dilated = raw.dilate(radius);
        let h = dilated.height();
        let mut samples = Vec::new();
        for y in [0, h / 4, h / 2, 3 * h / 4, h - 1] {
            let set: Vec<usize> = (0..dilated.width).filter(|&x| dilated.get(x, y)).collect();
            for x in [set.first(), set.get(set.len() / 2), set.last()].into_iter().flatten() {
                if !samples.contains(&(*x, y)) {
                    samples.push((*x, y));
                }
            }
        }
        Self { raw, dilated, samples }
    }
}

/// Esquina (celdas) de la máscara dilatada y giro de cada carta.
type RasterLayout = Vec<(usize, usize, u8)>;

/// Acomoda las máscaras en una franja de `width` celdas: cada carta, de
/// mayor a menor, en la posición de tope más bajo (luego más a la
/// izquierda). Devuelve la esquina y el giro de cada una y el lado ocupado.
fn raster_strip(masks: &[Vec<Oriented>], order: &[usize], width: usize, radius: usize) -> (RasterLayout, usize) {
    let mut atlas = Atlas { mask: Mask::new(width, 0) };
    let mut placed = vec![(0, 0, 0); masks.len()];
    let mut used = [0usize; 2];
    for &i in order {
        let mut best: Option<(usize, usize, usize, u8)> = None; // (tope, x, y, giro)
        for (turns, o) in masks[i].iter().enumerate() {
            let (w, h) = (o.dilated.width, o.dilated.height());
            if w > width {
                continue;
            }
            for y in 0..=atlas.mask.height() {
                if best.is_some_and(|b| y + h >= b.0) {
                    break;
                }
                // Probar la máscara propia en (x + r, y + r) contra la dilatada
                // del resto equivale a probar la dilatada contra las propias
                if let Some(x) = atlas.first_fit(o, y, width - w) {
                    best = Some((y + h, x, y, turns as u8));
                    break;
                }
            }
        }
        let (_, x, y, turns) = best.expect("la franja cabe la dimensión menor de cada carta");
        let o = &masks[i][turns as usize];
        atlas.stamp(&o.raw, x + radius, y + radius);
        placed[i] = (x, y, turns);
        used[0] = used[0].max(x + o.dilated.width);
        used[1] = used[1].max(y + o.dilated.height());
    }
    (placed, used[0].max(used[1]))
}

/// Celdas por lado del atlas en el empaquetado por rasterizado.
const RASTER_CELLS: f64 = 1024.0;

/// Empaquetado por rasterizado. Devuelve las transformaciones y el lado en
/// unidades 3D, o `None` si no logró respetar el margen.
fn raster_pack(
    prepared: &[Prepared],
    triangles: &[&[[usize; 3]]],
    area_3d: f64,
    texture_size: u32,
    padding: u32,
) -> Option<(Vec<Placement>, f64)> {
    let texture = texture_size.max(1) as f64;
    let cells = RASTER_CELLS.min(texture);
    let box_area: f64 = prepared.iter().map(|p| p.size[0] * p.size[1]).sum();
    let mut cell = (area_3d.max(box_area * 0.25) / 0.6).sqrt().max(1e-12) / cells;
    let mut order: Vec<usize> = (0..prepared.len()).collect();
    order.sort_by(|&a, &b| {
        let key = |i: usize| prepared[i].size[0] * prepared[i].size[1];
        key(b).total_cmp(&key(a)).then(a.cmp(&b))
    });

    let mut best: Option<(Vec<Placement>, f64)> = None;
    for _ in 0..3 {
        // Margen en celdas si el atlas termina con `cells` de lado; la mitad
        // de las celdas propias ya separa por el rasterizado conservador
        let radius = (padding as f64 * cells / texture).ceil() as usize;
        let masks: Vec<Vec<Oriented>> = prepared
            .par_iter()
            .zip(triangles)
            .map(|(p, tris)| (0..4u8).map(|turns| Oriented::new(rasterize(p, tris, turns, cell), radius)).collect())
            .collect();
        let filled: usize = masks.iter().map(|m| m[0].dilated.rows.iter().flatten().map(|w| w.count_ones() as usize).sum::<usize>()).sum();
        let narrowest = masks.iter().map(|m| m[0].dilated.width.min(m[0].dilated.height())).max().unwrap_or(1);
        let widths: Vec<usize> = [0.8, 0.85, 0.9, 0.95, 1.0, 1.05, 1.1, 1.2, 1.3]
            .iter()
            .map(|f| (((filled as f64 / 0.8).sqrt() * f) as usize).max(narrowest))
            .collect();
        let (placed, side_cells) =
            widths.par_iter().map(|&w| raster_strip(&masks, &order, w, radius)).min_by_key(|(_, side)| *side).expect("al menos un ancho");

        // El margen real depende del lado final
        if radius as f64 * texture >= padding as f64 * side_cells as f64 {
            let side = side_cells as f64 * cell;
            if best.as_ref().is_none_or(|b| side < b.1) {
                let placements = prepared
                    .iter()
                    .zip(&placed)
                    .map(|(p, &(x, y, turns))| p.placement(turns, [(x + radius) as f64 * cell, (y + radius) as f64 * cell], side))
                    .collect();
                best = Some((placements, side));
            }
        }
        // Ya cerca del lado buscado: afinar no cambia casi nada
        if best.is_some() && side_cells as f64 >= 0.95 * cells {
            break;
        }
        // Siguiente ronda: celdas tales que el lado quede algo por debajo de
        // `cells` (pasarse invalida el margen)
        cell *= side_cells as f64 / (0.98 * cells);
    }
    best
}

/// Empaquetado por skyline de cajas. Devuelve las transformaciones y el lado.
fn skyline_pack(prepared: &[Prepared], texture_size: u32, padding: u32) -> (Vec<Placement>, f64) {
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
        .map(|(p, (x, y, rotated))| p.placement(u8::from(rotated), [x + pad / 2.0, y + pad / 2.0], side))
        .collect();
    (placements, side)
}

/// Acomoda las cartas en [0, 1]². Devuelve la transformación de cada una y
/// la fracción del cuadrado que cubren.
pub(crate) fn pack(charts: &[ChartShape], texture_size: u32, padding: u32) -> (Vec<Placement>, f64) {
    let prepared: Vec<Prepared> = charts
        .iter()
        .map(|c| {
            let scale = if c.area_uv > 0.0 { (c.area_3d / c.area_uv).sqrt() } else { 1.0 };
            let scaled: Vec<[f64; 2]> = c.points.iter().map(|p| [p[0] * scale, p[1] * scale]).collect();
            let (cos, sin, min, size) = min_box(&scaled);
            let local =
                scaled.iter().map(|q| [cos * q[0] - sin * q[1] - min[0], sin * q[0] + cos * q[1] - min[1]].map(|v| v.max(0.0))).collect();
            Prepared { scale, cos, sin, min, size, local }
        })
        .collect();
    if prepared.is_empty() {
        return (Vec::new(), 0.0);
    }

    let total_3d: f64 = charts.iter().map(|c| c.area_3d).sum();
    let triangles: Vec<&[[usize; 3]]> = charts.iter().map(|c| c.triangles).collect();
    let (mut placements, mut side) = skyline_pack(&prepared, texture_size, padding);
    if let Some((raster, raster_side)) = raster_pack(&prepared, &triangles, total_3d, texture_size, padding)
        && raster_side < side
    {
        (placements, side) = (raster, raster_side);
    }
    (placements, total_3d / (side * side))
}
