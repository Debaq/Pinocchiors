//! Revisión de las capas de resina antes de imprimir (SLA3 de `PLAN_SLA.md`).
//!
//! - **Islas**: partes de una capa que no se apoyan en nada de la capa anterior. En una
//!   impresora de resina la pieza cuelga de la placa y crece hacia la pantalla: una isla
//!   se forma sobre el film y se pierde al despegar. Necesita un soporte.
//! - **Ventosas**: huecos de una capa encerrados por la pieza (no tocan el aire de afuera
//!   en esa capa) que, mirando hacia la placa, tampoco llegan al aire de afuera. Al
//!   despegar hacen succión contra el film. Se arreglan con un agujero de venteo en el
//!   piso del hueco (o inclinando la pieza).
//! - **Huecos cerrados**: ventosas que después se tapan: la resina queda atrapada adentro.
//!
//! Cada capa se trabaja en tramos (no en píxeles): componentes conexas de los tramos
//! encendidos y de los de aire dentro de la caja de la capa, en paralelo; después se
//! unen las de capas seguidas, en orden.

use rayon::prelude::*;
use serde::Serialize;

use crate::sla::{Layer, ResinPrinter};

/// Una isla: parte de la capa `layer` que no se apoya en la anterior.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Island {
    pub layer: usize,
    /// Un punto adentro, en la cara de abajo de la capa (mm).
    pub pos: [f32; 3],
    pub area_mm2: f32,
}

/// Un hueco encerrado (ventosa mientras se imprime, o hueco cerrado al final).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Cavity {
    /// Primera y última capa del hueco.
    pub first_layer: usize,
    pub last_layer: usize,
    /// Punto en el piso del hueco (la cara de arriba de la capa anterior), mm. Ahí va el venteo.
    pub floor: [f32; 3],
    /// El piso de cada bolsillo que succionó por su cuenta antes de juntarse con el resto
    /// (un hueco puede empezar en varias partes a la vez): uno de venteo para cada uno.
    pub pockets: Vec<[f32; 3]>,
    pub volume_ml: f32,
    /// Capas en que hizo succión (encerrado y sin salida hacia la placa).
    pub suction_layers: u32,
    /// Área más grande de la ventosa en una capa (mm²): la fuerza de la succión.
    pub max_suction_mm2: f32,
    /// Se tapa sin haber tenido salida: la resina queda atrapada.
    pub trapped: bool,
    /// El piso es la placa (empieza en la capa 0): no se puede ventear por abajo.
    pub on_plate: bool,
}

impl Cavity {
    /// Si hace falta arreglarlo: resina atrapada (más de 1 mm³) o ventosa de al menos
    /// [`MIN_CAVITY_MM2`] que succiona más de dos capas.
    pub fn is_problem(&self) -> bool {
        (self.trapped && self.volume_ml >= 0.001) || (self.max_suction_mm2 >= MIN_CAVITY_MM2 && self.suction_layers > 2)
    }
}

/// Lo que encontró la revisión.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ResinReport {
    pub layers: usize,
    /// Islas de al menos [`MIN_ISLAND_MM2`].
    pub islands: Vec<Island>,
    /// Islas más chicas (un par de píxeles: no imprimen nada que importe).
    pub tiny_islands: usize,
    /// Solo los huecos que son un problema ([`Cavity::is_problem`]).
    pub cavities: Vec<Cavity>,
    /// Capa con más área (mm²) y cuál: la que más tira al despegar.
    pub max_area_mm2: f32,
    pub max_area_layer: usize,
}

/// Islas más chicas que esto (mm², unos 4 píxeles) se cuentan aparte.
pub const MIN_ISLAND_MM2: f32 = 0.01;

/// Ventosas más chicas que esto (mm²) no se informan.
pub const MIN_CAVITY_MM2: f32 = 1.0;

/// Revisa las capas laminadas para `printer`.
pub fn check(layers: &[Layer], printer: &ResinPrinter, layer_height: f32) -> ResinReport {
    let (pw, ph) = printer.pixel_size();
    let px_mm2 = pw * ph;
    let infos: Vec<Option<LayerInfo>> = layers.par_iter().map(|l| LayerInfo::new(l, printer.res_x, printer.res_y)).collect();

    let mut report = ResinReport { layers: layers.len(), ..Default::default() };
    for (k, l) in layers.iter().enumerate() {
        let a = l.area() as f32 * px_mm2;
        if a > report.max_area_mm2 {
            report.max_area_mm2 = a;
            report.max_area_layer = k;
        }
    }

    let mut sets = Sets::default();
    // Nodo global de cada componente de aire encerrada de la capa anterior.
    let mut prev_nodes: Vec<u32> = Vec::new();
    let mut buf_a = Vec::new();
    let mut buf_b = Vec::new();
    for k in 0..layers.len() {
        let cur = infos[k].as_ref();
        let bottom = layers[k].z - layer_height;
        let to_mm = |(px, py): (u32, u32), z: f32| {
            let (x, y) = printer.pixel_to_mm(px, py);
            [x, y, z]
        };

        // Nodos nuevos para el aire encerrado de esta capa.
        let mut nodes = Vec::new();
        if let Some(c) = cur {
            for comp in 0..c.air.ncomp {
                if c.air_outer[comp] {
                    nodes.push(u32::MAX);
                    continue;
                }
                let area = c.air.area[comp];
                nodes.push(sets.add(Set {
                    first: k,
                    last: k,
                    px: area,
                    floor: to_mm(c.air.sample[comp], bottom),
                    floor_area: area,
                    on_plate: k == 0,
                    ..Default::default()
                }));
            }
        }

        if k > 0 {
            let prev = infos[k - 1].as_ref();
            let mut supported = vec![false; cur.map_or(0, |c| c.solid.ncomp)];
            for y in row_range(prev, cur) {
                // Islas: tramos encendidos que se pisan con los de la capa anterior.
                if let (Some(p), Some(c)) = (prev, cur) {
                    let start = c.solid.row_range(y).start;
                    overlaps(p.solid.row(y), c.solid.row(y), |_, j| supported[c.solid.comp[start + j] as usize] = true);
                }
                // Aire: une los huecos con los de abajo; aire de afuera abajo = venteado,
                // aire de afuera arriba = el hueco se abre hacia arriba.
                air_row(prev, y, printer.res_x, &mut buf_a);
                air_row(cur, y, printer.res_x, &mut buf_b);
                let (pa, ca) = (&buf_a, &buf_b);
                let mut i = 0;
                let mut j = 0;
                while i < pa.len() && j < ca.len() {
                    let (a0, a1, ac) = pa[i];
                    let (b0, b1, bc) = ca[j];
                    if a0 < b1 && b0 < a1 {
                        let below = ac.map(|d| prev_nodes[d as usize]).filter(|&n| n != u32::MAX);
                        let above = bc.map(|e| nodes[e as usize]).filter(|&n| n != u32::MAX);
                        match (below, above) {
                            (Some(d), Some(e)) => sets.union(d, e),
                            (None, Some(e)) => sets.flag(e, |s| s.vented = true),
                            (Some(d), None) => sets.flag(d, |s| s.opened = true),
                            (None, None) => {}
                        }
                    }
                    if a1 <= b1 {
                        i += 1;
                    } else {
                        j += 1;
                    }
                }
            }
            if let Some(c) = cur {
                for (comp, ok) in supported.iter().enumerate() {
                    let area_mm2 = c.solid.area[comp] as f32 * px_mm2;
                    if *ok {
                        continue;
                    }
                    if area_mm2 < MIN_ISLAND_MM2 {
                        report.tiny_islands += 1;
                    } else {
                        report.islands.push(Island { layer: k, pos: to_mm(c.solid.sample[comp], bottom), area_mm2 });
                    }
                }
            }
        }

        // Succión en esta capa: área de cada hueco que sigue sin salida.
        let mut this_layer: Vec<(u32, u64)> = Vec::new();
        for (comp, &n) in nodes.iter().enumerate() {
            if n == u32::MAX {
                continue;
            }
            let r = sets.find(n);
            sets.sets[r as usize].last = k;
            let area = cur.map_or(0, |c| c.air.area[comp]);
            match this_layer.iter_mut().find(|(q, _)| *q == r) {
                Some(e) => e.1 += area,
                None => this_layer.push((r, area)),
            }
        }
        for (r, area) in this_layer {
            let s = &mut sets.sets[r as usize];
            if !s.vented {
                s.suction_layers += 1;
                s.max_suction = s.max_suction.max(area);
            }
        }
        prev_nodes = nodes;
    }

    let last = layers.len().saturating_sub(1);
    let mut seen = vec![false; sets.sets.len()];
    for n in 0..sets.sets.len() as u32 {
        let r = sets.find(n) as usize;
        if std::mem::replace(&mut seen[r], true) {
            continue;
        }
        let s = &sets.sets[r];
        let cavity = Cavity {
            first_layer: s.first,
            last_layer: s.last,
            floor: s.floor,
            pockets: {
                // El piso del hueco y los bolsillos que por sí solos son un problema.
                let min_px = (MIN_CAVITY_MM2 / px_mm2).ceil() as u64;
                let mut p = vec![s.floor];
                for q in s.pockets.iter().filter(|p| p.1 > 2 && p.2 >= min_px) {
                    if !p.contains(&q.0) {
                        p.push(q.0);
                    }
                }
                p
            },
            volume_ml: s.px as f32 * px_mm2 * layer_height / 1000.0,
            suction_layers: s.suction_layers,
            max_suction_mm2: s.max_suction as f32 * px_mm2,
            trapped: !s.vented && !s.opened && s.last < last,
            on_plate: s.on_plate,
        };
        if cavity.is_problem() {
            report.cavities.push(cavity);
        }
    }
    report.cavities.sort_by_key(|c| c.first_layer);
    report
}

/// Quita las islas de hasta `max_px` píxeles (SLA15), de abajo hacia arriba: quitar una
/// puede dejar sin apoyo la de arriba, que se mira con la capa ya limpia. La capa 0 está
/// sobre la placa. Devuelve cuántas quitó.
pub fn remove_islands(layers: &mut [Layer], printer: &ResinPrinter, max_px: u64) -> usize {
    if max_px == 0 || layers.len() < 2 {
        return 0;
    }
    let mut removed = 0;
    let mut prev = LayerInfo::new(&layers[0], printer.res_x, printer.res_y);
    for l in layers.iter_mut().skip(1) {
        let Some(cur) = LayerInfo::new(l, printer.res_x, printer.res_y) else {
            prev = None;
            continue;
        };
        let mut supported = vec![false; cur.solid.ncomp];
        if let Some(p) = &prev {
            for y in row_range(Some(p), Some(&cur)) {
                let start = cur.solid.row_range(y).start;
                overlaps(p.solid.row(y), cur.solid.row(y), |_, j| supported[cur.solid.comp[start + j] as usize] = true);
            }
        }
        let gone: Vec<bool> = (0..cur.solid.ncomp).map(|c| !supported[c] && cur.solid.area[c] <= max_px).collect();
        if !gone.iter().any(|g| *g) {
            prev = Some(cur);
            continue;
        }
        removed += gone.iter().filter(|g| **g).count();
        let mut cut: Vec<(u32, u32, u32)> = Vec::new();
        for r in 0..cur.solid.rows.len().saturating_sub(1) {
            for j in cur.solid.rows[r] as usize..cur.solid.rows[r + 1] as usize {
                if gone[cur.solid.comp[j] as usize] {
                    let (a, b) = cur.solid.runs[j];
                    cut.push((cur.solid.y0 + r as u32, a, b));
                }
            }
        }
        l.spans = crate::sla_hollow::subtract(&l.spans, &cut, |s| (s.y, s.x0, s.x1), |s: crate::sla::Span, a, b| crate::sla::Span { x0: a, x1: b, ..s });
        if !l.gray.is_empty() {
            // Los grises del borde de la isla también (un píxel alrededor).
            let wide: Vec<(u32, u32, u32)> = cut.iter().flat_map(|&(y, a, b)| (y.saturating_sub(1)..=y + 1).map(move |yy| (yy, a.saturating_sub(1), b + 1))).collect();
            let mut wide = wide;
            wide.sort_unstable();
            let mut merged: Vec<(u32, u32, u32)> = Vec::new();
            for w in wide {
                match merged.last_mut() {
                    Some(m) if m.0 == w.0 && w.1 <= m.2 => m.2 = m.2.max(w.2),
                    _ => merged.push(w),
                }
            }
            // Sin borrar grises de lo que queda: solo donde no hay tramo encendido.
            let keep_gray = |g: &crate::sla::GraySpan| l.spans.iter().any(|s| s.y == g.y && s.x0 < g.x1 && g.x0 < s.x1);
            let gray = std::mem::take(&mut l.gray);
            let (inside, rest): (Vec<_>, Vec<_>) = gray.into_iter().partition(|g| merged.iter().any(|m| m.0 == g.y && m.1 < g.x1 && g.x0 < m.2));
            l.gray = rest;
            l.gray.extend(inside.into_iter().filter(keep_gray));
            l.gray.sort_unstable_by_key(|g| (g.y, g.x0));
        }
        prev = LayerInfo::new(l, printer.res_x, printer.res_y);
    }
    removed
}

/// Cierra en cada capa los huecos encerrados (aire que no toca el borde de la capa) de
/// hasta `max_px` píxeles (SLA15). Devuelve cuántos cerró.
pub fn fill_holes(layers: &mut [Layer], printer: &ResinPrinter, max_px: u64) -> usize {
    if max_px == 0 {
        return 0;
    }
    layers
        .par_iter_mut()
        .map(|l| {
            let Some(info) = LayerInfo::new(l, printer.res_x, printer.res_y) else { return 0 };
            let fill: Vec<bool> = (0..info.air.ncomp).map(|c| !info.air_outer[c] && info.air.area[c] <= max_px).collect();
            let n = fill.iter().filter(|f| **f).count();
            if n == 0 {
                return 0;
            }
            let mut add: Vec<(u32, u32, u32)> = Vec::new();
            for r in 0..info.air.rows.len().saturating_sub(1) {
                for j in info.air.rows[r] as usize..info.air.rows[r + 1] as usize {
                    if fill[info.air.comp[j] as usize] {
                        let (a, b) = info.air.runs[j];
                        add.push((info.air.y0 + r as u32, a, b));
                    }
                }
            }
            let merge = |spans: &mut Vec<crate::sla::Span>| {
                spans.extend(add.iter().map(|&(y, x0, x1)| crate::sla::Span { y, x0, x1 }));
                spans.sort_unstable_by_key(|s| (s.y, s.x0));
                let mut out: Vec<crate::sla::Span> = Vec::with_capacity(spans.len());
                for s in spans.drain(..) {
                    match out.last_mut() {
                        Some(o) if o.y == s.y && s.x0 <= o.x1 => o.x1 = o.x1.max(s.x1),
                        _ => out.push(s),
                    }
                }
                *spans = out;
            };
            merge(&mut l.spans);
            if !l.gray.is_empty() {
                let mut g = crate::sla_hollow::subtract(&l.gray, &add, |s| (s.y, s.x0, s.x1), |s: crate::sla::GraySpan, a, b| crate::sla::GraySpan { x0: a, x1: b, ..s });
                g.extend(add.iter().map(|&(y, x0, x1)| crate::sla::GraySpan { y, x0, x1, v: 255 }));
                g.sort_unstable_by_key(|s| (s.y, s.x0));
                l.gray = g;
            }
            n
        })
        .sum()
}

/// Partes delgadas que suben sin anclaje (para los soportes laterales, SLA12b). Capa por
/// capa, cada componente hereda de las de abajo que pisa la menor altura desde el último
/// anclaje; anclan una sección de más de `max_area_mm2`, un punto de `anchors` adentro y
/// el no tener nada abajo (isla: lleva su soporte). Cuando una parte delgada pasa
/// `every_mm` sin anclaje, devuelve un punto adentro de ella (a media capa) y vuelve a cero.
pub fn slender(layers: &[Layer], printer: &ResinPrinter, layer_height: f32, anchors: &[[f32; 3]], max_area_mm2: f32, every_mm: f32) -> Vec<[f32; 3]> {
    let (pw, ph) = printer.pixel_size();
    let max_px = (max_area_mm2 / (pw * ph)).max(1.0) as u64;
    let infos: Vec<Option<LayerInfo>> = layers.par_iter().map(|l| LayerInfo::new(l, printer.res_x, printer.res_y)).collect();
    // Anclajes por capa, en píxeles.
    let mut by_layer: Vec<Vec<(u32, u32)>> = vec![Vec::new(); layers.len()];
    for a in anchors {
        let k = ((a[2] / layer_height).floor().max(0.0) as usize).min(layers.len().saturating_sub(1));
        if let Some(px) = printer.mm_to_pixel(a[0], a[1]) {
            let last = (k + 1).min(layers.len().saturating_sub(1));
            for near in &mut by_layer[k.saturating_sub(1)..=last] {
                near.push(px);
            }
        }
    }
    let mut out = Vec::new();
    let mut prev_since: Vec<f32> = Vec::new();
    for k in 0..layers.len() {
        let Some(c) = infos[k].as_ref() else {
            prev_since.clear();
            continue;
        };
        let mut since = vec![f32::MAX; c.solid.ncomp];
        if k > 0
            && let Some(p) = infos[k - 1].as_ref()
        {
            for y in row_range(Some(p), Some(c)) {
                let start = c.solid.row_range(y).start;
                let pstart = p.solid.row_range(y).start;
                overlaps(p.solid.row(y), c.solid.row(y), |i, j| {
                    let comp = c.solid.comp[start + j] as usize;
                    let from = prev_since[p.solid.comp[pstart + i] as usize] + layer_height;
                    since[comp] = since[comp].min(from);
                });
            }
        }
        for &(x, y) in &by_layer[k] {
            for j in c.solid.row_range(y) {
                let (a, b) = c.solid.runs[j];
                if a <= x && x < b {
                    since[c.solid.comp[j] as usize] = 0.0;
                }
            }
        }
        for (comp, h) in since.iter_mut().enumerate() {
            if *h == f32::MAX || c.solid.area[comp] > max_px {
                *h = 0.0;
            } else if *h >= every_mm {
                let (x, y) = printer.pixel_to_mm(c.solid.sample[comp].0, c.solid.sample[comp].1);
                out.push([x, y, layers[k].z - layer_height / 2.0]);
                *h = 0.0;
            }
        }
        prev_since = since;
    }
    out
}

/// Filas que hay que mirar entre dos capas (la unión de sus cajas).
fn row_range(a: Option<&LayerInfo>, b: Option<&LayerInfo>) -> std::ops::Range<u32> {
    let r = |i: Option<&LayerInfo>| i.map(|i| (i.y0, i.y1));
    match (r(a), r(b)) {
        (Some((a0, a1)), Some((b0, b1))) => a0.min(b0)..a1.max(b1),
        (Some((a0, a1)), None) | (None, Some((a0, a1))) => a0..a1,
        (None, None) => 0..0,
    }
}

/// Los tramos de aire de la fila `y` en toda la pantalla: `None` = aire de afuera.
fn air_row(info: Option<&LayerInfo>, y: u32, res_x: u32, out: &mut Vec<(u32, u32, Option<u32>)>) {
    out.clear();
    let Some(info) = info.filter(|i| y >= i.y0 && y < i.y1) else {
        out.push((0, res_x, None));
        return;
    };
    if info.x0 > 0 {
        out.push((0, info.x0, None));
    }
    let row = info.air.row_range(y);
    for j in row {
        let (a, b) = info.air.runs[j];
        let c = info.air.comp[j];
        out.push((a, b, if info.air_outer[c as usize] { None } else { Some(c) }));
    }
    if info.x1 < res_x {
        out.push((info.x1, res_x, None));
    }
}

/// Llama `f(i, j)` por cada par de tramos que se pisan (los dos ordenados por x).
fn overlaps(a: &[(u32, u32)], b: &[(u32, u32)], mut f: impl FnMut(usize, usize)) {
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i].0 < b[j].1 && b[j].0 < a[i].1 {
            f(i, j);
        }
        if a[i].1 <= b[j].1 {
            i += 1;
        } else {
            j += 1;
        }
    }
}

/// Tramos de una capa por fila, con su componente conexa.
#[derive(Default)]
struct Runs {
    y0: u32,
    /// `rows[y - y0]..rows[y - y0 + 1]` son los tramos de la fila `y`.
    rows: Vec<u32>,
    runs: Vec<(u32, u32)>,
    comp: Vec<u32>,
    ncomp: usize,
    /// Por componente: píxeles y un píxel adentro (el más cerca del centro).
    area: Vec<u64>,
    sample: Vec<(u32, u32)>,
}

impl Runs {
    fn row_range(&self, y: u32) -> std::ops::Range<usize> {
        if y < self.y0 || (y - self.y0) as usize + 1 >= self.rows.len() {
            return 0..0;
        }
        let i = (y - self.y0) as usize;
        self.rows[i] as usize..self.rows[i + 1] as usize
    }

    fn row(&self, y: u32) -> &[(u32, u32)] {
        &self.runs[self.row_range(y)]
    }

    /// Componentes conexas (vecinos de arriba y abajo que se pisan), área y píxel de muestra.
    fn label(&mut self) {
        let n = self.runs.len();
        let mut parent: Vec<u32> = (0..n as u32).collect();
        fn find(p: &mut [u32], mut x: u32) -> u32 {
            while p[x as usize] != x {
                p[x as usize] = p[p[x as usize] as usize];
                x = p[x as usize];
            }
            x
        }
        for r in 1..self.rows.len().saturating_sub(1) {
            let (a, b) = (self.rows[r - 1] as usize..self.rows[r] as usize, self.rows[r] as usize..self.rows[r + 1] as usize);
            let (sa, sb) = (a.start, b.start);
            overlaps(&self.runs[a], &self.runs[b], |i, j| {
                let (x, y) = (find(&mut parent, (sa + i) as u32), find(&mut parent, (sb + j) as u32));
                if x != y {
                    parent[x.max(y) as usize] = x.min(y);
                }
            });
        }
        let mut id = vec![u32::MAX; n];
        self.comp = vec![0; n];
        let mut ncomp = 0;
        for i in 0..n {
            let r = find(&mut parent, i as u32) as usize;
            if id[r] == u32::MAX {
                id[r] = ncomp;
                ncomp += 1;
            }
            self.comp[i] = id[r];
        }
        self.ncomp = ncomp as usize;

        // Centro de cada componente y el píxel adentro más cercano.
        let mut sum = vec![(0f64, 0f64, 0u64); self.ncomp];
        let mut run_row = Vec::with_capacity(n);
        for r in 0..self.rows.len().saturating_sub(1) {
            for _ in self.rows[r]..self.rows[r + 1] {
                run_row.push(self.y0 + r as u32);
            }
        }
        for i in 0..n {
            let (a, b) = self.runs[i];
            let len = (b - a) as u64;
            let s = &mut sum[self.comp[i] as usize];
            s.0 += (a as f64 + b as f64 - 1.0) / 2.0 * len as f64;
            s.1 += run_row[i] as f64 * len as f64;
            s.2 += len;
        }
        self.area = sum.iter().map(|s| s.2).collect();
        let mut best = vec![(f64::MAX, (0u32, 0u32)); self.ncomp];
        for ((&(a, b), &c), &y) in self.runs.iter().zip(&self.comp).zip(&run_row) {
            let c = c as usize;
            let (cx, cy) = (sum[c].0 / sum[c].2 as f64, sum[c].1 / sum[c].2 as f64);
            let x = cx.round().clamp(a as f64, (b - 1) as f64);
            let d = (x - cx).powi(2) + (y as f64 - cy).powi(2);
            if d < best[c].0 {
                best[c] = (d, (x as u32, y));
            }
        }
        self.sample = best.into_iter().map(|b| b.1).collect();
    }
}

/// Una capa en tramos: los encendidos y el aire dentro de su caja (agrandada un píxel).
struct LayerInfo {
    x0: u32,
    x1: u32,
    y0: u32,
    y1: u32,
    solid: Runs,
    air: Runs,
    /// El aire de esa componente toca el borde de la caja: es aire de afuera.
    air_outer: Vec<bool>,
}

impl LayerInfo {
    fn new(layer: &Layer, res_x: u32, res_y: u32) -> Option<Self> {
        let first = layer.spans.first()?;
        let last = layer.spans.last()?;
        let x0 = layer.spans.iter().map(|s| s.x0).min()?.saturating_sub(1);
        let x1 = (layer.spans.iter().map(|s| s.x1).max()? + 1).min(res_x);
        let y0 = first.y.saturating_sub(1);
        let y1 = (last.y + 2).min(res_y);

        let mut solid = Runs { y0, ..Default::default() };
        let mut air = Runs { y0, ..Default::default() };
        let mut k = 0;
        for y in y0..y1 {
            solid.rows.push(solid.runs.len() as u32);
            air.rows.push(air.runs.len() as u32);
            let mut x = x0;
            while k < layer.spans.len() && layer.spans[k].y == y {
                let s = layer.spans[k];
                solid.runs.push((s.x0, s.x1));
                if s.x0 > x {
                    air.runs.push((x, s.x0));
                }
                x = s.x1;
                k += 1;
            }
            if x < x1 {
                air.runs.push((x, x1));
            }
        }
        solid.rows.push(solid.runs.len() as u32);
        air.rows.push(air.runs.len() as u32);
        solid.label();
        air.label();

        let mut air_outer = vec![false; air.ncomp];
        for r in 0..(y1 - y0) as usize {
            let y = y0 + r as u32;
            for j in air.rows[r] as usize..air.rows[r + 1] as usize {
                let (a, b) = air.runs[j];
                if y == y0 || y + 1 == y1 || a == x0 || b == x1 {
                    air_outer[air.comp[j] as usize] = true;
                }
            }
        }
        Some(Self { x0, x1, y0, y1, solid, air, air_outer })
    }
}

/// Huecos unidos entre capas (unión-búsqueda con datos en la raíz).
#[derive(Debug, Clone, Default)]
struct Set {
    first: usize,
    last: usize,
    px: u64,
    floor: [f32; 3],
    floor_area: u64,
    vented: bool,
    opened: bool,
    on_plate: bool,
    suction_layers: u32,
    max_suction: u64,
    /// Bolsillos que succionaron por su cuenta y terminaron al juntarse con aire que no
    /// succionaba (venteado): piso, capas y área máxima en píxeles.
    pockets: Vec<([f32; 3], u32, u64)>,
}

#[derive(Default)]
struct Sets {
    parent: Vec<u32>,
    sets: Vec<Set>,
}

impl Sets {
    fn add(&mut self, s: Set) -> u32 {
        self.parent.push(self.sets.len() as u32);
        self.sets.push(s);
        (self.sets.len() - 1) as u32
    }

    fn find(&mut self, mut x: u32) -> u32 {
        while self.parent[x as usize] != x {
            self.parent[x as usize] = self.parent[self.parent[x as usize] as usize];
            x = self.parent[x as usize];
        }
        x
    }

    fn flag(&mut self, x: u32, f: impl FnOnce(&mut Set)) {
        let r = self.find(x);
        f(&mut self.sets[r as usize]);
    }

    fn union(&mut self, a: u32, b: u32) {
        let (a, b) = (self.find(a), self.find(b));
        if a == b {
            return;
        }
        let (keep, gone) = (a.min(b), a.max(b));
        self.parent[gone as usize] = keep;
        let g = self.sets[gone as usize].clone();
        let s = &mut self.sets[keep as usize];
        // Lo que succionó hasta acá por separado queda como bolsillo (con un venteo en el
        // piso de cada uno, el resto ya no succiona).
        if s.suction_layers > 0 && (g.suction_layers > 0 || g.vented) {
            s.pockets.push((s.floor, s.suction_layers, s.max_suction));
        }
        if g.suction_layers > 0 && (s.suction_layers > 0 || s.vented) {
            s.pockets.push((g.floor, g.suction_layers, g.max_suction));
        }
        // El piso (donde va el venteo) es el del que succionó; si los dos, el más bajo.
        let take = match (s.suction_layers > 0, g.suction_layers > 0) {
            (false, true) => true,
            (true, false) => false,
            _ => g.first < s.first || (g.first == s.first && g.floor_area > s.floor_area),
        };
        if take {
            s.floor = g.floor;
            s.floor_area = g.floor_area;
        }
        s.first = s.first.min(g.first);
        s.last = s.last.max(g.last);
        s.px += g.px;
        s.vented |= g.vented;
        s.opened |= g.opened;
        s.on_plate |= g.on_plate;
        s.suction_layers = s.suction_layers.max(g.suction_layers);
        s.max_suction = s.max_suction.max(g.max_suction);
        s.pockets.extend(g.pockets.iter().copied());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sla::tests::cube;
    use crate::sla::{slice, LD_002H};

    fn inverted(t: Vec<[[f32; 3]; 3]>) -> Vec<[[f32; 3]; 3]> {
        t.into_iter().map(|t| [t[0], t[2], t[1]]).collect()
    }

    fn run(tris: &[[[f32; 3]; 3]]) -> ResinReport {
        let layers = slice(tris, &LD_002H, 0.05).unwrap();
        check(&layers, &LD_002H, 0.05)
    }

    #[test]
    fn cubo_en_la_placa_no_tiene_problemas() {
        let r = run(&cube([20.0, 20.0, 0.0], [10.0, 10.0, 5.0]));
        assert!(r.islands.is_empty(), "{:?}", r.islands);
        assert!(r.cavities.is_empty());
        assert!((r.max_area_mm2 - 100.0).abs() < 2.0);
    }

    #[test]
    fn pieza_flotando_es_isla() {
        let mut t = cube([20.0, 20.0, 0.0], [10.0, 10.0, 2.0]);
        t.extend(cube([50.0, 50.0, 3.0], [4.0, 4.0, 2.0]));
        let r = run(&t);
        assert_eq!(r.islands.len(), 1);
        let i = &r.islands[0];
        assert_eq!(i.layer, 60);
        assert!((i.pos[0] - 52.0).abs() < 0.1 && (i.pos[1] - 52.0).abs() < 0.1 && (i.pos[2] - 3.0).abs() < 0.01, "{i:?}");
        assert!((i.area_mm2 - 16.0).abs() < 0.5);
    }

    #[test]
    fn quitar_islas_chicas_y_cerrar_huecos_chicos() {
        // Base, una isla chica (0,2 × 0,2 mm, unos 16 px) y una grande (4 × 4) más arriba.
        let mut t = cube([20.0, 20.0, 0.0], [10.0, 10.0, 2.0]);
        t.extend(cube([40.0, 40.0, 3.0], [0.2, 0.2, 0.2]));
        t.extend(cube([50.0, 50.0, 3.0], [4.0, 4.0, 2.0]));
        let mut layers = slice(&t, &LD_002H, 0.05).unwrap();
        assert_eq!(check(&layers, &LD_002H, 0.05).islands.len(), 2);
        // La chica mide 4 capas: sin la primera, la de arriba también queda suelta.
        assert_eq!(remove_islands(&mut layers, &LD_002H, 30), 4);
        let r = check(&layers, &LD_002H, 0.05);
        assert_eq!(r.islands.len(), 1, "queda la grande");
        assert!(layers[60..64].iter().all(|l| l.spans.iter().all(|s| s.x0 > 900)), "la chica ya no está");

        // Placa con un agujero pasante de 0,2 mm y otro de 2 mm: se cierra solo el chico.
        let mut t = cube([20.0, 20.0, 1.0], [10.0, 10.0, 1.0]);
        t.extend(inverted(cube([22.0, 22.0, 0.5], [0.2, 0.2, 2.0])));
        t.extend(inverted(cube([25.0, 25.0, 0.5], [2.0, 2.0, 2.0])));
        let mut layers = slice(&t, &LD_002H, 0.05).unwrap();
        let before = layers[30].area();
        assert_eq!(fill_holes(&mut layers, &LD_002H, 50), 20);
        let gained = layers[30].area() - before;
        assert!((10..=30).contains(&gained), "{gained}");
    }

    #[test]
    fn caja_hueca_cerrada_atrapa_resina() {
        // Caja elevada (piso en z = 2) sobre un pie, con hueco cerrado de 6 × 6 × 4.
        let mut t = cube([20.0, 20.0, 0.0], [2.0, 2.0, 2.0]);
        t.extend(cube([16.0, 16.0, 2.0], [10.0, 10.0, 8.0]));
        t.extend(inverted(cube([18.0, 18.0, 4.0], [6.0, 6.0, 4.0])));
        let r = run(&t);
        assert_eq!(r.cavities.len(), 1, "{:?}", r.cavities);
        let c = &r.cavities[0];
        assert!(c.trapped && !c.on_plate, "{c:?}");
        assert_eq!(c.first_layer, 80);
        assert_eq!(c.suction_layers, 80);
        assert!((c.max_suction_mm2 - 36.0).abs() < 1.0);
        assert!((c.volume_ml - 0.144).abs() < 0.005);
        assert!((c.floor[0] - 21.0).abs() < 0.1 && (c.floor[2] - 4.0).abs() < 0.01);
    }

    #[test]
    fn vaso_boca_arriba_succiona_pero_no_atrapa() {
        // Vaso elevado: hueco abierto arriba.
        let mut t = cube([20.0, 20.0, 0.0], [2.0, 2.0, 2.0]);
        t.extend(cube([16.0, 16.0, 2.0], [10.0, 10.0, 6.0]));
        t.extend(inverted(cube([18.0, 18.0, 4.0], [6.0, 6.0, 4.5])));
        let r = run(&t);
        assert_eq!(r.cavities.len(), 1);
        assert!(!r.cavities[0].trapped);
        assert_eq!(r.cavities[0].suction_layers, 80);
    }

    #[test]
    fn hueco_con_salida_abajo_no_es_problema() {
        // Tubo elevado: el hueco sigue hasta el aire de abajo.
        let mut t = cube([20.0, 20.0, 0.0], [2.0, 2.0, 2.0]);
        t.extend(cube([16.0, 16.0, 2.0], [10.0, 10.0, 6.0]));
        t.extend(inverted(cube([24.0, 18.0, 1.5], [1.0, 6.0, 7.0])));
        t.extend(inverted(cube([18.0, 18.0, 4.0], [7.0, 6.0, 3.0])));
        let r = run(&t);
        assert!(r.cavities.is_empty(), "{:?}", r.cavities);
    }
}
