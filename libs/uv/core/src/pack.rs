//! Empaquetado de cartas en el cuadrado UV [0, 1]².
//!
//! Por rasterizado, como xatlas: cada carta se escala a su área 3D (densidad
//! de texel uniforme), se gira a su caja mínima y se dibuja en una grilla de
//! celdas. Las cartas, de mayor a menor, van al primer hueco libre (de abajo
//! a la izquierda) probando los cuatro giros de 90°; solo si no caben se
//! agranda el cuadrado. Las cartas chicas llenan los huecos de las grandes,
//! cosa que las cajas de un skyline no pueden. El margen entre cartas se da
//! en texels del tamaño de textura destino.

use rayon::prelude::*;

/// Transformación afín de una carta al atlas: `uv' = M·uv + t`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Placement {
    m: [[f64; 2]; 2],
    t: [f64; 2],
}

impl Placement {
    pub fn apply(&self, p: [f64; 2]) -> [f64; 2] {
        [
            self.m[0][0] * p[0] + self.m[0][1] * p[1] + self.t[0],
            self.m[1][0] * p[0] + self.m[1][1] * p[1] + self.t[1],
        ]
    }
}

/// Carta a empaquetar: sus triángulos en UV (deben cubrir la carta; pueden
/// solaparse entre sí) y sus áreas.
pub(crate) struct ChartShape {
    pub triangles: Vec<[[f64; 2]; 3]>,
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

/// Giro (cos, sin) que deja la caja de menor área.
fn min_box(points: &[[f64; 2]]) -> (f64, f64) {
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
        [hi[0] - lo[0], hi[1] - lo[1]]
    };
    candidates
        .into_iter()
        .min_by(|a, b| {
            let area = |&(cos, sin): &(f64, f64)| {
                let size = bounds(cos, sin);
                size[0] * size[1]
            };
            area(a).total_cmp(&area(b))
        })
        .expect("al menos el giro nulo")
}

/// Máscara de celdas: una fila de bits por cada fila de celdas.
#[derive(Debug, Clone)]
struct Mask {
    w: usize,
    h: usize,
    words: usize,
    bits: Vec<u64>,
}

impl Mask {
    fn new(w: usize, h: usize) -> Self {
        let words = w.div_ceil(64).max(1);
        Self { w, h, words, bits: vec![0; words * h] }
    }

    fn set(&mut self, x: usize, y: usize) {
        self.bits[y * self.words + x / 64] |= 1 << (x % 64);
    }

    fn get(&self, x: usize, y: usize) -> bool {
        self.bits[y * self.words + x / 64] & (1 << (x % 64)) != 0
    }

    fn row(&self, y: usize) -> &[u64] {
        &self.bits[y * self.words..(y + 1) * self.words]
    }

    /// La máscara engordada `p` celdas hacia cada lado (cuadrado de lado
    /// 2p + 1); su celda (p, p) corresponde a la (0, 0) de la original.
    fn dilate(&self, p: usize) -> Mask {
        let mut wide = Mask::new(self.w + 2 * p, self.h);
        for y in 0..self.h {
            for x in (0..self.w).filter(|&x| self.get(x, y)) {
                for dx in 0..=2 * p {
                    wide.set(x + dx, y);
                }
            }
        }
        let mut out = Mask::new(self.w + 2 * p, self.h + 2 * p);
        for y in 0..wide.h {
            for x in (0..wide.w).filter(|&x| wide.get(x, y)) {
                for dy in 0..=2 * p {
                    out.set(x, y + dy);
                }
            }
        }
        out
    }
}

/// Celdas de `w × h` que toca algún triángulo. Conservador: la celda entra si
/// ningún eje separador (x, y o la normal de una arista) la aparta del
/// triángulo.
fn rasterize(triangles: &[[[f64; 2]; 3]], w: usize, h: usize) -> Mask {
    let mut mask = Mask::new(w, h);
    for tri in triangles {
        let lo = [0, 1].map(|c| tri.iter().map(|p| p[c]).fold(f64::INFINITY, f64::min));
        let hi = [0, 1].map(|c| tri.iter().map(|p| p[c]).fold(f64::NEG_INFINITY, f64::max));
        let x0 = (lo[0].floor().max(0.0) as usize).min(w - 1);
        let y0 = (lo[1].floor().max(0.0) as usize).min(h - 1);
        let x1 = (hi[0].floor().max(0.0) as usize + 1).min(w);
        let y1 = (hi[1].floor().max(0.0) as usize + 1).min(h);

        let [a, b, c] = *tri;
        let area2 = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
        // Arista p → q: e(x) = s·(q − p) × (x − p) ≥ 0 del lado interior
        let sign = area2.signum();
        let degenerate = area2.abs() <= 1e-12 * (1.0 + (hi[0] - lo[0]) * (hi[1] - lo[1]));
        let edges = [(a, b), (b, c), (c, a)].map(|(p, q)| (p, [-sign * (q[1] - p[1]), sign * (q[0] - p[0])]));
        for y in y0..y1 {
            for x in x0..x1 {
                let inside = degenerate
                    || edges.iter().all(|&(p, g)| {
                        // Esquina de la celda que maximiza la función de la arista
                        let cx = x as f64 + if g[0] > 0.0 { 1.0 } else { 0.0 };
                        let cy = y as f64 + if g[1] > 0.0 { 1.0 } else { 0.0 };
                        g[0] * (cx - p[0]) + g[1] * (cy - p[1]) >= 0.0
                    });
                if inside {
                    mask.set(x, y);
                }
            }
        }
    }
    mask
}

/// Una carta en uno de sus cuatro giros, en celdas: `celda = m·uv + shift`.
struct Oriented {
    m: [[f64; 2]; 2],
    shift: [f64; 2],
    mask: Mask,
    /// `mask` engordada el margen entre cartas; es lo que ocupa en el atlas.
    halo: Mask,
}

impl Oriented {
    fn new(chart: &ChartShape, m: [[f64; 2]; 2], pad: usize) -> Self {
        let apply = |p: [f64; 2]| [m[0][0] * p[0] + m[0][1] * p[1], m[1][0] * p[0] + m[1][1] * p[1]];
        let triangles: Vec<[[f64; 2]; 3]> = chart.triangles.iter().map(|t| t.map(apply)).collect();
        let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        for p in triangles.iter().flatten() {
            for c in 0..2 {
                lo[c] = lo[c].min(p[c]);
                hi[c] = hi[c].max(p[c]);
            }
        }
        if !lo[0].is_finite() {
            (lo, hi) = ([0.0; 2], [0.0; 2]);
        }
        let shift = [-lo[0], -lo[1]];
        let size = [0, 1].map(|c| ((hi[c] - lo[c]).ceil() as usize).max(1));
        let local: Vec<[[f64; 2]; 3]> = triangles.iter().map(|t| t.map(|p| [p[0] + shift[0], p[1] + shift[1]])).collect();
        let mask = rasterize(&local, size[0], size[1]);
        let halo = mask.dilate(pad);
        Self { m, shift, mask, halo }
    }
}

/// Grilla de ocupación cuadrada.
struct Atlas {
    size: usize,
    words: usize,
    bits: Vec<u64>,
}

impl Atlas {
    fn new(size: usize) -> Self {
        // Una palabra de más para leer bits corridos sin salirse de la fila
        let words = size / 64 + 2;
        Self { size, words, bits: vec![0; words * size] }
    }

    /// ¿Cabe `mask` con su esquina en (x, y) sin tocar celdas ocupadas?
    fn fits(&self, mask: &Mask, x: usize, y: usize) -> bool {
        (0..mask.h).all(|r| {
            let row = &self.bits[(y + r) * self.words..(y + r + 1) * self.words];
            mask.row(r).iter().enumerate().all(|(j, &bits)| {
                if bits == 0 {
                    return true;
                }
                let (k, s) = ((x + 64 * j) / 64, (x + 64 * j) % 64);
                let occupied = if s == 0 { row[k] } else { (row[k] >> s) | (row[k + 1] << (64 - s)) };
                occupied & bits == 0
            })
        })
    }

    /// Marca `halo` con su celda (pad, pad) en (x, y).
    fn insert(&mut self, halo: &Mask, x: usize, y: usize, pad: usize) {
        for r in 0..halo.h {
            let Some(ay) = (y + r).checked_sub(pad).filter(|&v| v < self.size) else { continue };
            for c in (0..halo.w).filter(|&c| halo.get(c, r)) {
                if let Some(ax) = (x + c).checked_sub(pad).filter(|&v| v < self.size) {
                    self.bits[ay * self.words + ax / 64] |= 1 << (ax % 64);
                }
            }
        }
    }

    /// Celdas libres de la fila `y` en las columnas `[from, to)`.
    fn free_in_row(&self, y: usize, from: usize, to: usize) -> usize {
        let row = &self.bits[y * self.words..(y + 1) * self.words];
        let used: usize = (from / 64..to.div_ceil(64))
            .map(|k| {
                let lo = (k * 64).max(from) - k * 64;
                let hi = ((k + 1) * 64).min(to) - k * 64;
                let range = if hi - lo == 64 { u64::MAX } else { ((1u64 << (hi - lo)) - 1) << lo };
                (row[k] & range).count_ones() as usize
            })
            .sum();
        (to - from) - used
    }

    /// Primera posición libre (y, x) de `mask` desde `margin` cuya extensión
    /// `max(x + w, y + h)` sea exactamente `t`, o a lo sumo `t` si `full`.
    fn find(&self, mask: &Mask, margin: usize, t: usize, full: bool) -> Option<(usize, usize)> {
        let (x_max, y_max) = (t.checked_sub(mask.w)?, t.checked_sub(mask.h)?);
        if x_max < margin || y_max < margin {
            return None;
        }
        if !full {
            // Solo la columna x_max y la fila y_max
            return (margin..y_max)
                .find(|&y| self.fits(mask, x_max, y))
                .map(|y| (y, x_max))
                .or_else(|| (margin..=x_max).find(|&x| self.fits(mask, x, y_max)).map(|x| (y_max, x)));
        }
        // Una fila del atlas sirve solo si tiene tantas celdas libres como la
        // fila de la carta que caería en ella
        let need: Vec<usize> = (0..mask.h).map(|r| mask.row(r).iter().map(|w| w.count_ones() as usize).sum()).collect();
        let free: Vec<usize> = (0..t).map(|y| if y < margin { 0 } else { self.free_in_row(y, margin, t) }).collect();
        if free.iter().sum::<usize>() < need.iter().sum::<usize>() {
            return None;
        }
        (margin..=y_max)
            .into_par_iter()
            .filter(|&y| need.iter().enumerate().all(|(r, &n)| free[y + r] >= n))
            .find_map_first(|y| (margin..=x_max).find(|&x| self.fits(mask, x, y)).map(|x| (y, x)))
    }
}

/// Giro de `o` cuartos de vuelta.
fn quarter_turn(o: usize) -> [[f64; 2]; 2] {
    match o % 4 {
        0 => [[1.0, 0.0], [0.0, 1.0]],
        1 => [[0.0, -1.0], [1.0, 0.0]],
        2 => [[-1.0, 0.0], [0.0, -1.0]],
        _ => [[0.0, 1.0], [-1.0, 0.0]],
    }
}

fn mul(a: [[f64; 2]; 2], b: [[f64; 2]; 2]) -> [[f64; 2]; 2] {
    [0, 1].map(|i| [0, 1].map(|j| a[i][0] * b[0][j] + a[i][1] * b[1][j]))
}

/// Acomoda las cartas con `k` celdas por unidad 3D en una grilla de lado
/// `capacity`. Devuelve el lado usado (en celdas, con margen) y la
/// transformación de cada carta, o nada si no caben.
fn place_all(charts: &[ChartShape], bases: &[[[f64; 2]; 2]], k: f64, pad: usize, capacity: usize) -> Option<(usize, Vec<Placement>)> {
    let margin = pad.div_ceil(2);
    let oriented: Vec<[Oriented; 4]> = charts
        .par_iter()
        .zip(bases)
        .map(|(chart, base)| {
            let scaled = base.map(|row| row.map(|v| v * k));
            [0, 1, 2, 3].map(|o| Oriented::new(chart, mul(quarter_turn(o), scaled), pad))
        })
        .collect();

    // De mayor a menor
    let cells = |i: usize| oriented[i][0].mask.bits.iter().map(|w| w.count_ones() as usize).sum::<usize>();
    let sizes: Vec<usize> = (0..charts.len()).map(cells).collect();
    let mut order: Vec<usize> = (0..charts.len()).collect();
    order.sort_by(|&a, &b| sizes[b].cmp(&sizes[a]).then(a.cmp(&b)));

    let mut atlas = Atlas::new(capacity);
    let mut extent = 0;
    let mut spots = vec![(0, 0, 0); charts.len()];
    for i in order {
        let need = |o: usize| margin + oriented[i][o].mask.w.max(oriented[i][o].mask.h);
        let start = |o: usize| extent.max(need(o));
        let mut t = (0..4).map(start).min().expect("cuatro giros");
        let (o, y, x) = loop {
            if t + margin > capacity {
                return None;
            }
            // Primera posición libre de cada giro; gana el borde superior más bajo
            let found = (0..4usize)
                .into_par_iter()
                .filter(|&o| need(o) <= t)
                .filter_map(|o| atlas.find(&oriented[i][o].mask, margin, t, t == start(o)).map(|(y, x)| (y + oriented[i][o].mask.h, y, x, o)))
                .min();
            if let Some((_, y, x, o)) = found {
                break (o, y, x);
            }
            t += 1;
        };
        let placed = &oriented[i][o];
        atlas.insert(&placed.halo, x, y, pad);
        extent = extent.max(x + placed.mask.w).max(y + placed.mask.h);
        spots[i] = (o, x, y);
    }

    let side = extent + margin;
    let placements = spots
        .iter()
        .zip(&oriented)
        .map(|(&(o, x, y), variants)| {
            let v = &variants[o];
            let s = side as f64;
            Placement {
                m: v.m.map(|row| row.map(|c| c / s)),
                t: [(v.shift[0] + x as f64) / s, (v.shift[1] + y as f64) / s],
            }
        })
        .collect();
    Some((side, placements))
}

/// Acomoda las cartas en [0, 1]². Devuelve la transformación de cada una y
/// la fracción del cuadrado que cubren.
pub(crate) fn pack(charts: &[ChartShape], texture_size: u32, padding: u32) -> (Vec<Placement>, f64) {
    if charts.is_empty() {
        return (Vec::new(), 0.0);
    }
    let texture_size = texture_size.max(1) as usize;
    // 512 celdas por lado (4 texels con 2048 px). Con 1024 la cobertura sube
    // 1–2 puntos pero el empaquetado tarda el triple.
    let resolution = texture_size.min(512);
    let pad = (padding as usize * resolution).div_ceil(texture_size);

    // Escala a densidad de texel uniforme y giro a la caja mínima
    let bases: Vec<[[f64; 2]; 2]> = charts
        .iter()
        .map(|c| {
            let scale = if c.area_uv > 0.0 { (c.area_3d / c.area_uv).sqrt() } else { 1.0 };
            let points: Vec<[f64; 2]> = c.triangles.iter().flatten().map(|p| [p[0] * scale, p[1] * scale]).collect();
            let (cos, sin) = min_box(&points);
            [[scale * cos, -scale * sin], [scale * sin, scale * cos]]
        })
        .collect();
    let total_3d: f64 = charts.iter().map(|c| c.area_3d).sum();

    // Espacio para que quepan aunque todas sean de una celda
    let capacity = (2 * resolution).max(2 * (charts.len() as f64).sqrt().ceil() as usize * (pad + 2));
    let mut k = if total_3d > 0.0 { (0.6 * (resolution * resolution) as f64 / total_3d).sqrt() } else { 1.0 };
    // El margen está medido para un lado de `resolution` celdas: sirve si el
    // lado final no lo supera. Se busca la escala más grande que cumple.
    let mut best: Option<(f64, Vec<Placement>)> = None;
    let mut fallback: Option<(usize, f64, Vec<Placement>)> = None;
    for _ in 0..12 {
        match place_all(charts, &bases, k, pad, capacity) {
            Some((side, placements)) if side <= resolution => {
                let coverage = total_3d * k * k / (side * side) as f64;
                if best.as_ref().is_none_or(|b| coverage > b.0) {
                    best = Some((coverage, placements));
                }
                if side as f64 >= 0.98 * resolution as f64 {
                    break;
                }
                k *= resolution as f64 / side as f64;
            }
            Some((side, placements)) => {
                if best.is_some() {
                    break;
                }
                if fallback.as_ref().is_none_or(|f| side < f.0) {
                    fallback = Some((side, total_3d * k * k / (side * side) as f64, placements));
                }
                k *= 0.99 * resolution as f64 / side as f64;
            }
            None => k *= 0.7,
        }
    }
    match (best, fallback) {
        (Some((coverage, placements)), _) => (placements, coverage),
        (None, Some((_, coverage, placements))) => (placements, coverage),
        (None, None) => panic!("no se pudieron empaquetar {} cartas", charts.len()),
    }
}
