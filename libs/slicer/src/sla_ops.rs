//! Operaciones sobre las capas ya laminadas (SLA15 de `PLAN_SLA.md`, al estilo de UVtools).
//! Todas apagadas de fábrica:
//!
//! - **Quitar islas chicas** y **cerrar huecos chicos**: en `sla_check` (`remove_islands`,
//!   `fill_holes`), que es donde están las componentes de cada capa.
//! - **Alivio de la balsa** ([`raft_relief`]): en las capas de la balsa quedan lo que está
//!   bajo los pies de los soportes y cruces en una cuadrícula (brazos que no se tocan); el
//!   resto se apaga. Nada encierra aire (si no, cada celda sería una ventosa contra el film):
//!   menos resina, menos fuerza al despegar y la balsa sale más fácil de la placa.
//! - **Atenuar el interior** ([`dim_interior`]): los píxeles a más de `dim_wall` mm del borde
//!   llevan `dim_percent` % de luz (en gris). Las áreas grandes se encogen y tiran menos; el
//!   borde, que define la medida, sigue con toda la luz. No toca las capas de base.
//! - **Doble exposición** ([`double_exposure`], solo `.ctb` v3): cada capa normal se expone
//!   dos veces sin subir la placa entre medio: primero achicada `double_erode` píxeles con
//!   `double_first` de la exposición y después entera con el resto.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::sla::{GraySpan, Layer, ResinPrinter, Span};
use crate::sla_hollow::edt_line;
use crate::sla_layers::LayerParams;

/// Qué operaciones hacer y cómo (0 o `false` = apagada).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayerOps {
    /// Quitar islas de hasta tantos píxeles (ruido del laminado).
    pub remove_islands_px: u32,
    /// Cerrar huecos encerrados de hasta tantos píxeles en cada capa.
    pub fill_holes_px: u32,
    /// Alivio de la balsa: cruces cada `relief_spacing` mm con brazos de `relief_width` mm.
    pub raft_relief: bool,
    pub relief_spacing: f32,
    pub relief_width: f32,
    /// Atenuar el interior: a más de `dim_wall` mm del borde, `dim_percent` % de luz.
    pub dim: bool,
    pub dim_wall: f32,
    pub dim_percent: u8,
    /// Doble exposición (`.ctb` v3): primero la capa achicada `double_erode` píxeles con la
    /// fracción `double_first` de la exposición, después entera con el resto.
    pub double_exposure: bool,
    pub double_erode: u32,
    pub double_first: f32,
}

impl Default for LayerOps {
    fn default() -> Self {
        Self {
            remove_islands_px: 0,
            fill_holes_px: 0,
            raft_relief: false,
            relief_spacing: 4.0,
            relief_width: 1.0,
            dim: false,
            dim_wall: 1.0,
            dim_percent: 85,
            double_exposure: false,
            double_erode: 2,
            double_first: 0.5,
        }
    }
}

/// Una máscara de píxeles en la caja `x0..x0+w`, `y0..y0+h`.
struct Mask {
    x0: u32,
    y0: u32,
    w: usize,
    h: usize,
    on: Vec<bool>,
}

impl Mask {
    fn empty(x0: u32, y0: u32, x1: u32, y1: u32) -> Self {
        let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
        Mask { x0, y0, w, h, on: vec![false; w * h] }
    }

    /// Caja de los tramos (agrandada `pad` píxeles, dentro de la pantalla).
    fn bounds(spans: &[Span], pad: u32, p: &ResinPrinter) -> Option<(u32, u32, u32, u32)> {
        let x0 = spans.iter().map(|s| s.x0).min()?.saturating_sub(pad);
        let x1 = (spans.iter().map(|s| s.x1).max()? + pad).min(p.res_x);
        let y0 = spans.first()?.y.saturating_sub(pad);
        let y1 = (spans.last()?.y + 1 + pad).min(p.res_y);
        Some((x0, y0, x1, y1))
    }

    fn paint(&mut self, spans: &[Span]) {
        for s in spans {
            if s.y < self.y0 || s.y >= self.y0 + self.h as u32 {
                continue;
            }
            let row = (s.y - self.y0) as usize * self.w;
            let (a, b) = (s.x0.max(self.x0), s.x1.min(self.x0 + self.w as u32));
            for x in a..b {
                self.on[row + (x - self.x0) as usize] = true;
            }
        }
    }

    fn spans(&self) -> Vec<Span> {
        let mut out = Vec::new();
        for r in 0..self.h {
            let row = &self.on[r * self.w..(r + 1) * self.w];
            let mut x = 0;
            while x < self.w {
                if row[x] {
                    let a = x;
                    while x < self.w && row[x] {
                        x += 1;
                    }
                    out.push(Span { y: self.y0 + r as u32, x0: self.x0 + a as u32, x1: self.x0 + x as u32 });
                } else {
                    x += 1;
                }
            }
        }
        out
    }

    /// Distancia al cuadrado (mm²) de cada píxel encendido al apagado más cercano. Con
    /// `edge_off`, lo de afuera de la caja cuenta como apagado.
    fn distance(&self, pw: f32, ph: f32, edge_off: bool) -> Vec<f32> {
        let pad = if edge_off { 0.0 } else { f32::INFINITY };
        let mut d: Vec<f32> = self.on.iter().map(|&v| if v { f32::INFINITY } else { 0.0 }).collect();
        // Con un píxel más de cada lado, para que el borde de la caja cuente (o no).
        let mut line = Vec::new();
        for r in 0..self.h {
            line.clear();
            line.push(pad);
            line.extend_from_slice(&d[r * self.w..(r + 1) * self.w]);
            line.push(pad);
            edt_line(&mut line, pw);
            d[r * self.w..(r + 1) * self.w].copy_from_slice(&line[1..self.w + 1]);
        }
        for c in 0..self.w {
            line.clear();
            line.push(pad);
            line.extend((0..self.h).map(|r| d[r * self.w + c]));
            line.push(pad);
            edt_line(&mut line, ph);
            for r in 0..self.h {
                d[r * self.w + c] = line[r + 1];
            }
        }
        d
    }
}

/// Alivio de la balsa en las primeras `raft_layers` capas. Devuelve los píxeles apagados.
pub fn raft_relief(layers: &mut [Layer], printer: &ResinPrinter, raft_layers: usize, ops: &LayerOps) -> u64 {
    let n = raft_layers.min(layers.len());
    if n == 0 || n >= layers.len() {
        return 0;
    }
    let (pw, ph) = printer.pixel_size();
    // Lo que se apoya en la balsa: la primera capa de arriba, un poco agrandada.
    let above = layers[n].spans.clone();
    let grow = 0.3f32;
    layers[..n]
        .par_iter_mut()
        .map(|l| {
            let Some((x0, y0, x1, y1)) = Mask::bounds(&l.spans, 1, printer) else { return 0 };
            let mut raft = Mask::empty(x0, y0, x1, y1);
            raft.paint(&l.spans);
            let mut feet = Mask::empty(x0, y0, x1, y1);
            feet.paint(&above);
            // Distancia a los pies: invertir la máscara.
            let mut not_feet = Mask::empty(x0, y0, x1, y1);
            not_feet.on = feet.on.iter().map(|v| !v).collect();
            // Afuera de la caja no hay pies.
            let to_feet = not_feet.distance(pw, ph, false);
            let sp = ops.relief_spacing.max(1.0);
            let w = ops.relief_width.clamp(0.2, sp / 2.0);
            // Brazo de cada cruz: un cuarto de la cuadrícula (queda medio paso libre entre cruces).
            let arm = sp / 4.0;
            let to_node = |v: f32| {
                let m = v.rem_euclid(sp);
                m.min(sp - m)
            };
            let mut removed = 0u64;
            for r in 0..raft.h {
                let dy = to_node((y0 as usize + r) as f32 * ph);
                for c in 0..raft.w {
                    let i = r * raft.w + c;
                    if !raft.on[i] {
                        continue;
                    }
                    let dx = to_node((x0 as usize + c) as f32 * pw);
                    let cross = (dy <= w / 2.0 && dx <= arm) || (dx <= w / 2.0 && dy <= arm);
                    if !(cross || to_feet[i] <= grow * grow) {
                        raft.on[i] = false;
                        removed += 1;
                    }
                }
            }
            if removed > 0 {
                l.spans = raft.spans();
                l.gray.clear();
            }
            removed
        })
        .sum()
}

/// Atenúa el interior de las capas normales (desde `bottom_layers`). Devuelve cuántas capas tocó.
pub fn dim_interior(layers: &mut [Layer], printer: &ResinPrinter, bottom_layers: usize, ops: &LayerOps) -> usize {
    let (pw, ph) = printer.pixel_size();
    let wall2 = ops.dim_wall.max(0.0).powi(2);
    let v = (255.0 * ops.dim_percent.min(100) as f32 / 100.0).round() as u8;
    if v == 255 {
        return 0;
    }
    layers
        .par_iter_mut()
        .skip(bottom_layers)
        .map(|l| {
            let Some((x0, y0, x1, y1)) = Mask::bounds(&l.spans, 1, printer) else { return 0 };
            let mut m = Mask::empty(x0, y0, x1, y1);
            m.paint(&l.spans);
            let d = m.distance(pw, ph, true);
            // Tramos del interior, por fila.
            let mut inner: Vec<(u32, u32, u32)> = Vec::new();
            for r in 0..m.h {
                let mut c = 0;
                while c < m.w {
                    if d[r * m.w + c] > wall2 {
                        let a = c;
                        while c < m.w && d[r * m.w + c] > wall2 {
                            c += 1;
                        }
                        inner.push((y0 + r as u32, x0 + a as u32, x0 + c as u32));
                    } else {
                        c += 1;
                    }
                }
            }
            if inner.is_empty() {
                return 0;
            }
            // La imagen: lo de siempre menos el interior, más el interior en gris.
            let base: Vec<GraySpan> = l.image_spans().collect();
            let mut out = crate::sla_hollow::subtract(&base, &inner, |s| (s.y, s.x0, s.x1), |s: GraySpan, a, b| GraySpan { x0: a, x1: b, ..s });
            out.extend(inner.iter().map(|&(y, a, b)| GraySpan { y, x0: a, x1: b, v }));
            out.sort_unstable_by_key(|s| (s.y, s.x0));
            l.gray = out;
            1
        })
        .sum()
}

/// Doble exposición: cada capa normal (desde `bottom_layers`) pasa a dos, a la misma
/// altura: la primera achicada `erode` píxeles (sin subir después) y la segunda entera.
pub fn double_exposure(layers: &[Layer], params: &[LayerParams], printer: &ResinPrinter, bottom_layers: usize, ops: &LayerOps) -> (Vec<Layer>, Vec<LayerParams>) {
    let (pw, ph) = printer.pixel_size();
    let r = ops.double_erode as f32 * pw.min(ph);
    let first = ops.double_first.clamp(0.05, 0.95);
    let shrunk: Vec<Option<Layer>> = layers
        .par_iter()
        .enumerate()
        .map(|(i, l)| {
            if i < bottom_layers {
                return None;
            }
            let Some((x0, y0, x1, y1)) = Mask::bounds(&l.spans, 1, printer) else { return Some(l.clone()) };
            let mut m = Mask::empty(x0, y0, x1, y1);
            m.paint(&l.spans);
            let d = m.distance(pw, ph, true);
            for (on, d) in m.on.iter_mut().zip(&d) {
                *on = *on && *d > r * r;
            }
            Some(Layer { z: l.z, spans: m.spans(), gray: Vec::new() })
        })
        .collect();
    let mut out_l = Vec::with_capacity(layers.len() * 2);
    let mut out_p = Vec::with_capacity(layers.len() * 2);
    for ((l, p), s) in layers.iter().zip(params).zip(shrunk) {
        if let Some(s) = s {
            out_l.push(s);
            out_p.push(LayerParams { exposure: p.exposure * first, lift: 0.0, ..*p });
            out_l.push(l.clone());
            out_p.push(LayerParams { exposure: p.exposure * (1.0 - first), light_off: 0.0, ..*p });
        } else {
            out_l.push(l.clone());
            out_p.push(*p);
        }
    }
    (out_l, out_p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sla::tests::cube;
    use crate::sla::{slice, LD_002H};

    fn px(x: f32, y: f32) -> (u32, u32) {
        LD_002H.mm_to_pixel(x, y).unwrap()
    }

    fn value(l: &Layer, (x, y): (u32, u32)) -> u8 {
        l.image_spans().find(|s| s.y == y && s.x0 <= x && x < s.x1).map_or(0, |s| s.v)
    }

    #[test]
    fn atenuar_deja_el_borde_con_toda_la_luz() {
        let mut layers = slice(&cube([20.0, 20.0, 0.0], [10.0, 10.0, 1.0]), &LD_002H, 0.05).unwrap();
        let ops = LayerOps { dim: true, dim_wall: 1.0, dim_percent: 80, ..Default::default() };
        assert_eq!(dim_interior(&mut layers, &LD_002H, 8, &ops), 12);
        assert!(layers[0].gray.is_empty());
        let l = &layers[10];
        assert_eq!(value(l, px(25.0, 25.0)), 204);
        assert_eq!(value(l, px(20.5, 25.0)), 255);
        assert_eq!(value(l, px(19.0, 25.0)), 0);
        // Mismos píxeles encendidos que antes.
        assert_eq!(l.image_spans().map(|s| (s.x1 - s.x0) as u64).sum::<u64>(), l.area());
    }

    #[test]
    fn alivio_de_la_balsa() {
        // «Balsa» de 30 × 30 × 1 mm con un «pie» de 1 mm encima.
        let mut t = cube([10.0, 10.0, 0.0], [30.0, 30.0, 1.0]);
        t.extend(cube([22.0, 22.0, 1.0], [1.0, 1.0, 1.0]));
        let mut layers = slice(&t, &LD_002H, 0.05).unwrap();
        let before = layers[5].area();
        let ops = LayerOps { raft_relief: true, relief_spacing: 5.0, relief_width: 1.0, ..Default::default() };
        assert!(raft_relief(&mut layers, &LD_002H, 20, &ops) > 0);
        let l = &layers[5];
        assert!(l.area() < before / 3, "{} {before}", l.area());
        let on = |x: f32, y: f32| value(l, px(x, y)) > 0;
        assert!(on(15.0, 15.0) && on(16.0, 15.0) && on(15.0, 13.9), "cruz");
        assert!(on(22.5, 22.5), "pie");
        assert!(!on(17.5, 15.0) && !on(17.5, 17.5), "entre cruces");
        // Nada de aire encerrado: ningún hueco en la revisión.
        let r = crate::sla_check::check(&layers, &LD_002H, 0.05);
        assert!(r.cavities.is_empty(), "{:?}", r.cavities);
        assert_eq!(layers[25].area(), slice(&t, &LD_002H, 0.05).unwrap()[25].area());
    }

    #[test]
    fn doble_exposicion_duplica_las_capas_normales() {
        let layers = slice(&cube([20.0, 20.0, 0.0], [10.0, 10.0, 1.0]), &LD_002H, 0.05).unwrap();
        let p = LayerParams { exposure: 4.0, light_off: 1.0, lift: 6.0, lift_speed: 60.0, retract_speed: 150.0, pwm: 255 };
        let params = vec![p; layers.len()];
        let ops = LayerOps { double_exposure: true, double_erode: 3, double_first: 0.25, ..Default::default() };
        let (l, q) = double_exposure(&layers, &params, &LD_002H, 8, &ops);
        assert_eq!(l.len(), 8 + 2 * 12);
        assert_eq!((q[8].exposure, q[8].lift, q[9].exposure, q[9].lift, q[9].light_off), (1.0, 0.0, 3.0, 6.0, 0.0));
        assert_eq!(l[8].z, l[9].z);
        assert!(l[8].area() < l[9].area() && l[9] == layers[8]);
    }
}
