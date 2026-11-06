//! Compensaciones de medida sobre una capa antes de pasarla a píxeles (SLA10).
//!
//! Una capa se describe por **líneas** horizontales de muestreo (una por fila, o `n` por fila
//! con antialiasing) y en cada una los tramos `[x0, x1)` que caen dentro de la pieza, en
//! píxeles y sin redondear. Sobre eso se agranda o achica la pieza con un disco de radio `d`
//! mm (exacto en x, muestreado en y), y se encuentran los agujeros (aire encerrado) para
//! agrandarlos aparte.

/// Tramos de cada línea, desde la línea `first`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Lines {
    pub first: u32,
    pub rows: Vec<Vec<(f32, f32)>>,
}

impl Lines {
    /// Junta tramos `(línea, x0, x1)` en orden de línea y de x.
    pub fn from_runs(runs: impl IntoIterator<Item = (u32, f32, f32)>) -> Self {
        let mut out = Lines::default();
        for (line, x0, x1) in runs {
            if x1 <= x0 {
                continue;
            }
            if out.rows.is_empty() {
                out.first = line;
            }
            let i = (line - out.first) as usize;
            if out.rows.len() <= i {
                out.rows.resize(i + 1, Vec::new());
            }
            push_merge(&mut out.rows[i], (x0, x1));
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.rows.iter().all(Vec::is_empty)
    }

    fn get(&self, line: i64) -> &[(f32, f32)] {
        let i = line - self.first as i64;
        if i < 0 { &[] } else { self.rows.get(i as usize).map_or(&[], Vec::as_slice) }
    }

    /// Recorre los tramos: `(línea, x0, x1)`.
    pub fn runs(&self) -> impl Iterator<Item = (u32, f32, f32)> + '_ {
        self.rows.iter().enumerate().flat_map(move |(i, r)| r.iter().map(move |&(a, b)| (self.first + i as u32, a, b)))
    }
}

/// Agrega un tramo al final de una lista ordenada, juntándolo con el anterior si se tocan.
fn push_merge(row: &mut Vec<(f32, f32)>, (a, b): (f32, f32)) {
    if let Some(last) = row.last_mut()
        && a <= last.1
    {
        last.1 = last.1.max(b);
        return;
    }
    row.push((a, b));
}

/// Medio ancho en x (píxeles) del disco de radio `d` mm a `k` líneas del centro.
fn half_width(d: f32, k: i64, line_mm: f32, pw: f32) -> f32 {
    let dy = k as f32 * line_mm;
    (d * d - dy * dy).max(0.0).sqrt() / pw
}

/// Agranda (`d > 0`) o achica (`d < 0`) los tramos con un disco de radio `|d|` mm. `pw` es el
/// ancho de un píxel y `line_mm` la distancia entre líneas; no se pasa de la línea `lines`.
pub fn offset(l: &Lines, d: f32, pw: f32, line_mm: f32, lines: u32) -> Lines {
    if d == 0.0 || l.rows.is_empty() {
        return l.clone();
    }
    let r = d.abs();
    let k_max = (r / line_mm + 1e-4).floor() as i64;
    let widths: Vec<f32> = (0..=k_max).map(|k| half_width(r, k, line_mm, pw)).collect();
    if d > 0.0 {
        // Unión de los tramos vecinos estirados.
        let first = (l.first as i64 - k_max).max(0);
        let last = (l.first as i64 + l.rows.len() as i64 + k_max).min(lines as i64);
        let mut rows: Vec<Vec<(f32, f32)>> = vec![Vec::new(); (last - first).max(0) as usize];
        for (i, row) in l.rows.iter().enumerate() {
            let src = l.first as i64 + i as i64;
            for k in -k_max..=k_max {
                let y = src + k;
                if y < first || y >= last {
                    continue;
                }
                let w = widths[k.unsigned_abs() as usize];
                rows[(y - first) as usize].extend(row.iter().map(|&(a, b)| (a - w, b + w)));
            }
        }
        for row in &mut rows {
            row.sort_unstable_by(|p, q| p.0.total_cmp(&q.0));
            let mut merged = Vec::with_capacity(row.len());
            for &t in row.iter() {
                push_merge(&mut merged, t);
            }
            *row = merged;
        }
        Lines { first: first as u32, rows }
    } else {
        // Intersección de los tramos vecinos encogidos: una línea vecina vacía borra.
        let rows = (0..l.rows.len())
            .map(|i| {
                let y = l.first as i64 + i as i64;
                let mut acc = shrink(l.get(y), widths[0]);
                for k in 1..=k_max {
                    if acc.is_empty() {
                        break;
                    }
                    let w = widths[k as usize];
                    acc = intersect(&acc, &shrink(l.get(y - k), w));
                    acc = intersect(&acc, &shrink(l.get(y + k), w));
                }
                acc
            })
            .collect();
        Lines { first: l.first, rows }
    }
}

fn shrink(row: &[(f32, f32)], w: f32) -> Vec<(f32, f32)> {
    row.iter().map(|&(a, b)| (a + w, b - w)).filter(|(a, b)| b > a).collect()
}

fn intersect(p: &[(f32, f32)], q: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    while i < p.len() && j < q.len() {
        let (a, b) = (p[i].0.max(q[j].0), p[i].1.min(q[j].1));
        if b > a {
            out.push((a, b));
        }
        if p[i].1 < q[j].1 { i += 1 } else { j += 1 }
    }
    out
}

/// `l` sin lo que cubre `m`.
pub fn subtract(l: &Lines, m: &Lines) -> Lines {
    let rows = l
        .rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let cut = m.get(l.first as i64 + i as i64);
            let mut out = Vec::new();
            let mut j = 0;
            for &(mut a, b) in row {
                while j < cut.len() && cut[j].1 <= a {
                    j += 1;
                }
                let mut k = j;
                while k < cut.len() && cut[k].0 < b {
                    if cut[k].0 > a {
                        out.push((a, cut[k].0));
                    }
                    a = a.max(cut[k].1);
                    k += 1;
                }
                if b > a {
                    out.push((a, b));
                }
            }
            out
        })
        .collect();
    Lines { first: l.first, rows }
}

/// Los agujeros de la capa: el aire encerrado (que no llega al borde), solo los que caben en
/// un cuadrado de `max_mm` de lado.
pub fn holes(l: &Lines, pw: f32, line_mm: f32, max_mm: f32) -> Lines {
    // Huecos entre tramos de cada línea; los de las puntas son aire de afuera.
    struct Gap {
        line: usize,
        a: f32,
        b: f32,
    }
    let mut gaps: Vec<Gap> = Vec::new();
    let mut starts = Vec::with_capacity(l.rows.len() + 1);
    let mut outside = Vec::new();
    for (i, row) in l.rows.iter().enumerate() {
        starts.push(gaps.len());
        let mut prev = f32::NEG_INFINITY;
        for &(a, b) in row {
            gaps.push(Gap { line: i, a: prev, b: a });
            prev = b;
        }
        gaps.push(Gap { line: i, a: prev, b: f32::INFINITY });
    }
    starts.push(gaps.len());
    let mut parent: Vec<usize> = (0..gaps.len()).collect();
    fn find(p: &mut [usize], mut x: usize) -> usize {
        while p[x] != x {
            p[x] = p[p[x]];
            x = p[x];
        }
        x
    }
    for (g, gap) in gaps.iter().enumerate() {
        if gap.a.is_infinite() || gap.b.is_infinite() || gap.line == 0 || gap.line + 1 == l.rows.len() {
            outside.push(g);
        }
    }
    // Unir con los huecos de la línea de abajo que se pisan.
    for i in 1..l.rows.len() {
        let (lo, hi) = (starts[i - 1], starts[i]);
        let mut j = lo;
        for g in starts[i]..starts[i + 1] {
            while j < hi && gaps[j].b <= gaps[g].a {
                j += 1;
            }
            let mut k = j;
            while k < hi && gaps[k].a < gaps[g].b {
                let (ra, rb) = (find(&mut parent, g), find(&mut parent, k));
                parent[ra] = rb;
                k += 1;
            }
        }
    }
    let n = gaps.len();
    let mut out_root = vec![false; n];
    for g in outside {
        let r = find(&mut parent, g);
        out_root[r] = true;
    }
    // Tamaño de cada agujero.
    let mut bbox = vec![(f32::MAX, f32::MIN, usize::MAX, 0usize); n];
    for g in 0..n {
        let r = find(&mut parent, g);
        let b = &mut bbox[r];
        b.0 = b.0.min(gaps[g].a);
        b.1 = b.1.max(gaps[g].b);
        b.2 = b.2.min(gaps[g].line);
        b.3 = b.3.max(gaps[g].line + 1);
    }
    let fits = |b: &(f32, f32, usize, usize)| (b.1 - b.0) * pw <= max_mm && (b.3 - b.2) as f32 * line_mm <= max_mm;
    let mut rows = vec![Vec::new(); l.rows.len()];
    for g in 0..n {
        let r = find(&mut parent, g);
        if !out_root[r] && fits(&bbox[r]) {
            rows[gaps[g].line].push((gaps[g].a, gaps[g].b));
        }
    }
    Lines { first: l.first, rows }
}

/// Las compensaciones de una capa, en mm.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Compensation {
    /// Agrandar (+) o achicar (−) la pieza en XY.
    pub xy: f32,
    /// Agrandar los agujeros (≥ 0) de hasta `hole_max` mm.
    pub hole: f32,
    pub hole_max: f32,
}

impl Compensation {
    pub fn is_none(&self) -> bool {
        self.xy == 0.0 && self.hole <= 0.0
    }

    /// Aplica la compensación a las líneas de una capa.
    pub fn apply(&self, l: Lines, pw: f32, line_mm: f32, lines: u32) -> Lines {
        let mut l = if self.xy != 0.0 { offset(&l, self.xy, pw, line_mm, lines) } else { l };
        if self.hole > 0.0 && self.hole_max > 0.0 {
            let h = holes(&l, pw, line_mm, self.hole_max);
            if !h.is_empty() {
                l = subtract(&l, &offset(&h, self.hole, pw, line_mm, lines));
            }
        }
        l
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un cuadrado de `n` líneas de `x0` a `x1`, con un agujero opcional.
    fn square(first: u32, n: u32, x0: f32, x1: f32, hole: Option<(u32, u32, f32, f32)>) -> Lines {
        Lines::from_runs((first..first + n).flat_map(|y| match hole {
            Some((h0, h1, a, b)) if y >= h0 && y < h1 => vec![(y, x0, a), (y, b, x1)],
            _ => vec![(y, x0, x1)],
        }))
    }

    #[test]
    fn agrandar_y_achicar() {
        // Píxeles de 0,05 mm, una línea por píxel. Cuadrado de 100 × 100.
        let l = square(100, 100, 50.0, 150.0, None);
        let big = offset(&l, 0.5, 0.05, 0.05, 1000);
        assert_eq!(big.first, 90);
        assert_eq!(big.rows.len(), 120);
        assert_eq!(big.rows[50], vec![(40.0, 160.0)]);
        // Las esquinas quedan redondas: en la primera línea nueva, nada más que el lado.
        assert_eq!(big.rows[0], vec![(50.0, 150.0)]);
        let small = offset(&l, -0.5, 0.05, 0.05, 1000);
        let rows: Vec<_> = small.runs().collect();
        assert_eq!(rows.len(), 80);
        assert_eq!(rows[0], (110, 60.0, 140.0));
        // Achicar y agrandar devuelve lo mismo en los lados.
        let back = offset(&small, 0.5, 0.05, 0.05, 1000);
        assert_eq!(back.get(150), &[(50.0, 150.0)]);
    }

    #[test]
    fn agujero_encerrado_se_agranda() {
        let l = square(0, 100, 0.0, 100.0, Some((40, 60, 40.0, 60.0)));
        let h = holes(&l, 0.05, 0.05, 5.0);
        let runs: Vec<_> = h.runs().collect();
        assert_eq!(runs.len(), 20);
        assert!(runs.iter().all(|&(_, a, b)| a == 40.0 && b == 60.0));
        let c = Compensation { xy: 0.0, hole: 0.1, hole_max: 5.0 };
        let out = c.apply(l.clone(), 0.05, 0.05, 1000);
        assert_eq!(out.get(50), &[(0.0, 38.0), (62.0, 100.0)]);
        // El borde del agujero estaba entre las líneas 39 y 40: se corre 2 líneas.
        assert_eq!(out.get(38), &[(0.0, 40.0), (60.0, 100.0)]);
        assert_eq!(out.get(37), &[(0.0, 100.0)]);
        // Más grande que hole_max: no se toca.
        assert!(holes(&l, 0.05, 0.05, 0.5).is_empty());
    }

    #[test]
    fn la_entrada_de_un_borde_no_es_agujero() {
        // Una U: el aire de adentro llega a la línea de arriba.
        let l = Lines::from_runs((0..50).flat_map(|y| if y < 10 { vec![(y, 0.0, 30.0)] } else { vec![(y, 0.0, 10.0), (y, 20.0, 30.0)] }));
        assert!(holes(&l, 0.05, 0.05, 50.0).is_empty());
    }
}
