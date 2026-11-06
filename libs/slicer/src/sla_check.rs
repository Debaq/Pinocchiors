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
        if g.first < s.first || (g.first == s.first && g.floor_area > s.floor_area) {
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
