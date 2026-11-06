//! Vaciado para resina (SLA14 de `PLAN_SLA.md`): menos resina y menos fuerza al despegar.
//!
//! Se trabaja sobre las capas, no sobre la malla (sin booleanas):
//! 1. **Interior**: las capas de la pieza se pasan a una grilla de vóxeles (~0,2 mm; lleno =
//!    todos sus píxeles encendidos) y se calcula la distancia euclídea exacta de cada vóxel
//!    a lo que no está lleno (Felzenszwalb, separable en x, y, z). Lo que queda a más de la
//!    pared más un margen por el tamaño del vóxel es interior: nunca queda una pared más
//!    fina que `wall`.
//! 2. **Relleno**: dentro del interior se dejan encendidas columnas (`Pillars`) o una
//!    rejilla de paredes (`Grid`) cada `infill_spacing` mm. La rejilla tiene ranuras
//!    horizontales (un cuarto de cada `infill_spacing` de altura) donde solo siguen las
//!    columnas de los nodos: el aire queda conectado (un solo hueco, también en los rincones
//!    contra la pared) y la pared de encima cuelga de las columnas, no es una isla.
//! 3. El hueco queda cerrado: la revisión lo ve como resina atrapada y `prepare` le abre el
//!    venteo de siempre por abajo. Además se pueden poner **drenajes a mano**
//!    ([`drain_holes`]: un agujero hacia adentro según la normal de la cara) y su **tapón**
//!    imprimible ([`plugs`]: pin con tope, apoyado en la placa al lado de la balsa).

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::sla::{GraySpan, Layer, ResinPrinter, Span};
use crate::sla_supports::{tube, Tri, ZGrid};

/// Qué se deja adentro del vaciado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Infill {
    None,
    /// Columnas verticales: sostienen el techo del hueco.
    Pillars,
    /// Paredes en cuadrícula con ranuras horizontales (columnas continuas en los nodos).
    Grid,
}

/// Cómo vaciar (mm).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HollowSettings {
    /// Grosor mínimo de la pared.
    pub wall: f32,
    pub infill: Infill,
    /// Distancia entre columnas o paredes del relleno.
    pub infill_spacing: f32,
    /// Grosor de cada columna o pared del relleno.
    pub infill_width: f32,
}

impl Default for HollowSettings {
    fn default() -> Self {
        Self { wall: 2.0, infill: Infill::Pillars, infill_spacing: 8.0, infill_width: 1.2 }
    }
}

/// Vóxeles de más que esto agrandan el vóxel (memoria: dos `f32` por vóxel).
const MAX_VOXELS: f64 = 20e6;

/// El interior de una pieza laminada, listo para apagarlo en cualquier laminado con la
/// misma pantalla y alto de capa.
pub struct Hollow {
    /// Origen de la grilla en píxeles y capas (con un vóxel de margen alrededor).
    ox: i64,
    oy: i64,
    oz: i64,
    /// Píxeles y capas por vóxel.
    bx: i64,
    by: i64,
    bz: i64,
    nx: usize,
    ny: usize,
    nz: usize,
    inside: Vec<bool>,
    pixel: (f32, f32),
    layer_height: f32,
    s: HollowSettings,
}

impl Hollow {
    /// `None` si no hay interior (pieza más delgada que dos paredes).
    pub fn new(layers: &[Layer], printer: &ResinPrinter, layer_height: f32, s: &HollowSettings) -> Option<Hollow> {
        let (pw, ph) = printer.pixel_size();
        let used: Vec<usize> = (0..layers.len()).filter(|&k| !layers[k].spans.is_empty()).collect();
        let (k0, k1) = (*used.first()?, *used.last()? + 1);
        let spans = || layers[k0..k1].iter().flat_map(|l| l.spans.iter());
        let (x0, x1) = (spans().map(|s| s.x0).min()?, spans().map(|s| s.x1).max()?);
        let (y0, y1) = (spans().map(|s| s.y).min()?, spans().map(|s| s.y).max()? + 1);
        let size = [(x1 - x0) as f32 * pw, (y1 - y0) as f32 * ph, (k1 - k0) as f32 * layer_height];
        let wall = s.wall.max(0.1);
        let cell = (wall / 10.0).max(pw.min(ph)).max(((size[0] * size[1] * size[2]) as f64 / MAX_VOXELS).cbrt() as f32);
        let bx = ((cell / pw).round() as i64).max(1);
        let by = ((cell / ph).round() as i64).max(1);
        let bz = (((cell / layer_height).round() as i64).max(1)).min(65535 / (bx * by)).max(1);
        let nx = ((x1 - x0) as usize).div_ceil(bx as usize) + 2;
        let ny = ((y1 - y0) as usize).div_ceil(by as usize) + 2;
        let nz = (k1 - k0).div_ceil(bz as usize) + 2;
        let (ox, oy, oz) = (x0 as i64 - bx, y0 as i64 - by, k0 as i64 - bz);
        let plane = nx * ny;

        // Píxeles encendidos por vóxel; lleno = todos.
        let full = (bx * by * bz) as u16;
        let mut dist: Vec<f32> = vec![0.0; plane * nz];
        dist.par_chunks_mut(plane).enumerate().for_each(|(iz, slab)| {
            let mut count = vec![0u16; plane];
            for k in 0..bz {
                let layer = oz + iz as i64 * bz + k;
                if layer < k0 as i64 || layer >= k1 as i64 {
                    continue;
                }
                for sp in &layers[layer as usize].spans {
                    let iy = ((sp.y as i64 - oy) / by) as usize;
                    let (a, b) = (sp.x0 as i64 - ox, sp.x1 as i64 - ox);
                    let mut x = a;
                    while x < b {
                        let ix = x / bx;
                        let end = ((ix + 1) * bx).min(b);
                        count[iy * nx + ix as usize] += (end - x) as u16;
                        x = end;
                    }
                }
            }
            for (d, c) in slab.iter_mut().zip(&count) {
                *d = if *c == full { f32::INFINITY } else { 0.0 };
            }
        });

        // Distancia al cuadrado (mm²) al vóxel no lleno más cercano, eje por eje.
        let (vx, vy, vz) = (bx as f32 * pw, by as f32 * ph, bz as f32 * layer_height);
        dist.par_chunks_mut(nx).for_each(|line| edt_line(line, vx));
        dist.par_chunks_mut(plane).for_each(|slab| {
            let mut buf = vec![0.0; ny];
            for ix in 0..nx {
                for iy in 0..ny {
                    buf[iy] = slab[iy * nx + ix];
                }
                edt_line(&mut buf, vy);
                for iy in 0..ny {
                    slab[iy * nx + ix] = buf[iy];
                }
            }
        });
        let cols: Vec<Vec<f32>> = (0..plane)
            .into_par_iter()
            .map(|i| {
                let mut col: Vec<f32> = (0..nz).map(|iz| dist[iz * plane + i]).collect();
                edt_line(&mut col, vz);
                col
            })
            .collect();
        let margin = wall + (vx * vx + vy * vy + vz * vz).sqrt() + pw.max(ph);
        let mut inside = vec![false; plane * nz];
        for (i, col) in cols.iter().enumerate() {
            for (iz, d) in col.iter().enumerate() {
                inside[iz * plane + i] = *d >= margin * margin;
            }
        }
        inside.iter().any(|v| *v).then_some(Hollow { ox, oy, oz, bx, by, bz, nx, ny, nz, inside, pixel: (pw, ph), layer_height, s: s.clone() })
    }

    /// Apaga el interior (menos el relleno) en `layers`. Devuelve los píxeles apagados.
    pub fn apply(&self, layers: &mut [Layer]) -> u64 {
        layers.par_iter_mut().enumerate().map(|(k, l)| self.apply_layer(k, l)).sum()
    }

    fn apply_layer(&self, k: usize, layer: &mut Layer) -> u64 {
        let iz = (k as i64 - self.oz).div_euclid(self.bz);
        if iz < 0 || iz as usize >= self.nz {
            return 0;
        }
        let slab = &self.inside[iz as usize * self.nx * self.ny..][..self.nx * self.ny];
        // Tramos de píxeles que se apagan, por fila.
        let mut cuts: Vec<(u32, u32, u32)> = Vec::new();
        for sp in &layer.spans {
            let iy = (sp.y as i64 - self.oy).div_euclid(self.by);
            if iy < 0 || iy as usize >= self.ny {
                continue;
            }
            let row = &slab[iy as usize * self.nx..][..self.nx];
            let mut x = sp.x0 as i64;
            while x < sp.x1 as i64 {
                let ix = (x - self.ox).div_euclid(self.bx);
                let end = (self.ox + (ix + 1) * self.bx).min(sp.x1 as i64);
                if ix >= 0 && (ix as usize) < self.nx && row[ix as usize] {
                    match cuts.last_mut() {
                        Some(c) if c.0 == sp.y && c.2 as i64 == x => c.2 = end as u32,
                        _ => cuts.push((sp.y, x as u32, end as u32)),
                    }
                }
                x = end;
            }
        }
        if cuts.is_empty() {
            return 0;
        }
        // Lo que se deja: el relleno.
        let mut remove: Vec<(u32, u32, u32)> = Vec::new();
        for &(y, a, b) in &cuts {
            let keep = self.keep(k, y, a, b);
            let mut x = a;
            for (ka, kb) in keep {
                if ka > x {
                    remove.push((y, x, ka));
                }
                x = x.max(kb);
            }
            if x < b {
                remove.push((y, x, b));
            }
        }
        let removed: u64 = remove.iter().map(|r| (r.2 - r.1) as u64).sum();
        layer.spans = subtract(&layer.spans, &remove, |s| (s.y, s.x0, s.x1), |s, x0, x1| Span { x0, x1, ..s });
        if !layer.gray.is_empty() {
            layer.gray = subtract(&layer.gray, &remove, |s| (s.y, s.x0, s.x1), |s: GraySpan, x0, x1| GraySpan { x0, x1, ..s });
        }
        removed
    }

    /// Píxeles del relleno en la fila `y` de la capa `k` entre `a` y `b`, ordenados.
    fn keep(&self, k: usize, y: u32, a: u32, b: u32) -> Vec<(u32, u32)> {
        let (pw, ph) = self.pixel;
        let sp = self.s.infill_spacing.max(0.5);
        let w = self.s.infill_width.clamp(0.05, sp * 0.5);
        let ym = (y as f32 + 0.5) * ph;
        let zm = (k as f32 + 0.5) * self.layer_height;
        let in_band = |v: f32, width: f32| v.rem_euclid(sp) < width;
        let pillars = || if in_band(ym, w) { periodic(a, b, pw, sp, 0.0, w) } else { Vec::new() };
        match self.s.infill {
            Infill::None => Vec::new(),
            Infill::Pillars => pillars(),
            // Ranura: solo las columnas de los nodos (el aire pasa de una celda a otra).
            Infill::Grid if in_band(zm, sp / 4.0) => pillars(),
            Infill::Grid if in_band(ym, w) => vec![(a, b)],
            Infill::Grid => periodic(a, b, pw, sp, 0.0, w),
        }
    }
}

/// Píxeles de `a..b` cuyo centro cae en `[off + n·period, off + n·period + width)` mm.
fn periodic(a: u32, b: u32, pw: f32, period: f32, off: f32, width: f32) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    let x_mm = (a as f32 + 0.5) * pw;
    let mut n = ((x_mm - off) / period).floor() as i64;
    loop {
        let start = off + n as f32 * period;
        // Primer píxel con centro >= start, primero con centro >= start + width.
        let pa = ((start / pw - 0.5).ceil().max(0.0) as u32).max(a);
        let pb = (((start + width) / pw - 0.5).ceil().max(0.0) as u32).min(b);
        if pa >= b {
            break;
        }
        if pb > pa {
            out.push((pa, pb));
        }
        n += 1;
    }
    out
}

/// Resta de tramos ordenados por (fila, x) los tramos `remove` (también ordenados).
fn subtract<T: Copy>(spans: &[T], remove: &[(u32, u32, u32)], key: impl Fn(&T) -> (u32, u32, u32), make: impl Fn(T, u32, u32) -> T) -> Vec<T> {
    let mut out = Vec::with_capacity(spans.len());
    let mut j = 0;
    for s in spans {
        let (y, mut x0, x1) = key(s);
        while j < remove.len() && (remove[j].0 < y || (remove[j].0 == y && remove[j].2 <= x0)) {
            j += 1;
        }
        let mut i = j;
        while i < remove.len() && remove[i].0 == y && remove[i].1 < x1 {
            let (_, r0, r1) = remove[i];
            if r0 > x0 {
                out.push(make(*s, x0, r0));
            }
            x0 = x0.max(r1);
            i += 1;
        }
        if x0 < x1 {
            out.push(make(*s, x0, x1));
        }
    }
    out
}

/// Transformada de distancia 1D al cuadrado (Felzenszwalb y Huttenlocher) con muestras
/// cada `step` mm; `INFINITY` = lejos de todo.
fn edt_line(f: &mut [f32], step: f32) {
    let n = f.len();
    let src: Vec<(f32, f32)> = (0..n).filter(|&q| f[q].is_finite()).map(|q| (q as f32 * step, f[q])).collect();
    if src.is_empty() {
        return;
    }
    // Envolvente inferior de las parábolas.
    let mut v: Vec<(f32, f32)> = Vec::with_capacity(src.len());
    let mut z: Vec<f32> = Vec::with_capacity(src.len() + 1);
    z.push(f32::NEG_INFINITY);
    for &(q, fq) in &src {
        while let Some(&(p, fp)) = v.last() {
            let s = ((fq + q * q) - (fp + p * p)) / (2.0 * (q - p));
            if s <= *z.last().unwrap() {
                v.pop();
                z.pop();
            } else {
                z.push(s);
                break;
            }
        }
        if v.is_empty() {
            z.clear();
            z.push(f32::NEG_INFINITY);
        }
        v.push((q, fq));
    }
    z.push(f32::INFINITY);
    let mut k = 0;
    for (i, out) in f.iter_mut().enumerate() {
        let x = i as f32 * step;
        while z[k + 1] < x {
            k += 1;
        }
        let (p, fp) = v[k];
        *out = (x - p) * (x - p) + fp;
    }
}

/// Agujeros de drenaje hacia adentro de la pieza, según la normal de la cara más cercana a
/// cada punto: cáscaras dadas vuelta (restan al laminar).
pub fn drain_holes(model: &[Tri], drains: &[[f32; 3]], diameter: f32, depth: f32) -> Vec<Tri> {
    let grid = ZGrid::new(model, 2.0);
    let mut out = Vec::new();
    for p in drains {
        let n = grid.nearest_normal(p).unwrap_or([0.0, 0.0, -1.0]);
        // Apenas afuera de la cara: más afuera se comería los soportes que pasan cerca.
        let a = [p[0] + n[0] * 0.3, p[1] + n[1] * 0.3, p[2] + n[2] * 0.3];
        let b = [p[0] - n[0] * depth, p[1] - n[1] * depth, p[2] - n[2] * depth];
        let mut t = Vec::new();
        tube(&mut t, a, diameter / 2.0, b, diameter / 2.0, 20);
        out.extend(t.into_iter().map(|t| [t[0], t[2], t[1]]));
    }
    out
}

/// Tapones para `count` drenajes: pin de `diameter − 2·clearance` y `depth − 0,3` mm sobre un
/// tope de 0,8 mm más ancho, parados en la placa delante (o detrás) de `occupied` (caja XY
/// de lo que ya hay). Devuelve los triángulos y cuántos entraron.
pub fn plugs(count: usize, diameter: f32, depth: f32, clearance: f32, occupied: ([f32; 2], [f32; 2]), printer: &ResinPrinter) -> (Vec<Tri>, usize) {
    let (cap_r, cap_h) = (diameter / 2.0 + 0.8, 0.8);
    let pin_r = (diameter / 2.0 - clearance).max(0.2);
    let (lo, hi) = occupied;
    let gap = 2.0;
    let step = 2.0 * cap_r + gap;
    let rows = [lo[1] - gap - cap_r, hi[1] + gap + cap_r];
    let mut out = Vec::new();
    let mut placed = 0;
    'rows: for y in rows {
        if y - cap_r < 0.5 || y + cap_r > printer.bed_y - 0.5 {
            continue;
        }
        let mut x = lo[0].max(cap_r + 0.5);
        while placed < count {
            if x + cap_r > printer.bed_x - 0.5 {
                continue 'rows;
            }
            tube(&mut out, [x, y, 0.0], cap_r, [x, y, cap_h], cap_r, 24);
            tube(&mut out, [x, y, cap_h - 0.05], pin_r, [x, y, cap_h + (depth - 0.3).max(0.5)], pin_r, 20);
            placed += 1;
            x += step;
        }
        break;
    }
    (out, placed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sla::tests::cube;
    use crate::sla::{slice, LD_002H};

    #[test]
    fn distancia_exacta_en_una_linea() {
        let mut f = vec![0.0, f32::INFINITY, f32::INFINITY, f32::INFINITY, 0.0];
        edt_line(&mut f, 0.5);
        assert_eq!(f, vec![0.0, 0.25, 1.0, 0.25, 0.0]);
        let mut g = vec![f32::INFINITY; 3];
        edt_line(&mut g, 1.0);
        assert!(g.iter().all(|v| v.is_infinite()));
    }

    fn lit(l: &Layer, (px, py): (u32, u32)) -> bool {
        l.spans.iter().any(|s| s.y == py && s.x0 <= px && px < s.x1)
    }

    #[test]
    fn cubo_vaciado_deja_la_pared() {
        let h = 0.05;
        let mut layers = slice(&cube([20.0, 20.0, 0.0], [20.0, 20.0, 20.0]), &LD_002H, h).unwrap();
        let before: u64 = layers.iter().map(|l| l.area()).sum();
        let s = HollowSettings { wall: 2.0, infill: Infill::None, ..Default::default() };
        let hw = Hollow::new(&layers, &LD_002H, h, &s).unwrap();
        let removed = hw.apply(&mut layers);
        let after: u64 = layers.iter().map(|l| l.area()).sum();
        assert_eq!(before - removed, after);
        // Hueco de unos 15 mm de lado (pared de 2 a ~2,8 mm).
        let (pw, ph) = LD_002H.pixel_size();
        let mm3 = removed as f32 * pw * ph * h;
        assert!(mm3 > 14.0f32.powi(3) && mm3 < 16.0f32.powi(3), "{mm3}");
        let mid = &layers[200];
        let px = |x: f32, y: f32| LD_002H.mm_to_pixel(x, y).unwrap();
        assert!(!lit(mid, px(30.0, 30.0)));
        assert!(lit(mid, px(21.9, 30.0)) && lit(mid, px(30.0, 38.1)));
        // Piso y techo de al menos 2 mm.
        assert!(lit(&layers[39], px(30.0, 30.0)) && lit(&layers[360], px(30.0, 30.0)));
    }

    #[test]
    fn relleno_de_columnas_y_rejilla() {
        let h = 0.05;
        let base = slice(&cube([20.0, 20.0, 0.0], [30.0, 30.0, 20.0]), &LD_002H, h).unwrap();
        let removed = |infill| {
            let mut l = base.clone();
            let s = HollowSettings { wall: 2.0, infill, infill_spacing: 6.0, infill_width: 1.0 };
            Hollow::new(&l, &LD_002H, h, &s).unwrap().apply(&mut l)
        };
        let (none, pillars, grid) = (removed(Infill::None), removed(Infill::Pillars), removed(Infill::Grid));
        assert!(pillars < none && grid < pillars, "{none} {pillars} {grid}");
        // Columnas: ~(1/6)² del interior; rejilla con ventanas: bastante más.
        let frac = |r: u64| 1.0 - r as f32 / none as f32;
        assert!((frac(pillars) - 1.0 / 36.0).abs() < 0.02, "{}", frac(pillars));
        assert!(frac(grid) > 0.15 && frac(grid) < 0.35, "{}", frac(grid));
    }

    #[test]
    fn pieza_delgada_no_se_vacia() {
        let layers = slice(&cube([20.0, 20.0, 0.0], [3.0, 30.0, 10.0]), &LD_002H, 0.05).unwrap();
        assert!(Hollow::new(&layers, &LD_002H, 0.05, &HollowSettings::default()).is_none());
    }

    #[test]
    fn tapones_en_la_placa() {
        let (t, n) = plugs(3, 3.0, 2.5, 0.1, ([20.0, 20.0], [40.0, 40.0]), &LD_002H);
        assert_eq!(n, 3);
        assert!(!t.is_empty());
        let zmax = t.iter().flatten().map(|v| v[2]).fold(0f32, f32::max);
        assert!((zmax - 3.0).abs() < 1e-4, "{zmax}");
        assert!(t.iter().flatten().all(|v| v[1] < 20.0));
    }
}
