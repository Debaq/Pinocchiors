//! Laminado para impresoras de resina (MSLA): cada capa es una máscara en blanco y negro a
//! la resolución de la pantalla LCD. Ver `libs/pinocchio/print3d/PLAN_SLA.md`.
//!
//! Coordenadas de entrada: milímetros, Z arriba, origen en la esquina de la cama
//! (x de 0 a `bed_x`, y de 0 a `bed_y`), z = 0 en la placa. El píxel `(px, py)` cubre
//! `[px·w, (px+1)·w) × [py·h, (py+1)·h)` y se pinta si su centro cae dentro de la pieza.
//!
//! Cada capa se corta a media altura y se rellena por filas donde el giro es positivo: las
//! cáscaras que se pisan se suman y una cáscara dada vuelta resta (así se hacen agujeros de
//! venteo sin booleanas). Si la malla entera está al revés se da vuelta sola. Las capas van
//! en paralelo.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::SlicerError;

/// Una impresora de resina: pantalla y volumen de impresión.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResinPrinter {
    /// Identificador estable (perfiles, proyecto).
    pub id: &'static str,
    /// Nombre que va dentro del archivo (la impresora lo muestra).
    pub name: &'static str,
    /// Píxeles de la pantalla (ancho × alto).
    pub res_x: u32,
    pub res_y: u32,
    /// Tamaño de la pantalla y altura máxima, en mm.
    pub bed_x: f32,
    pub bed_y: f32,
    pub bed_z: f32,
    /// Bandera de proyección espejada que lleva el archivo.
    pub mirror: bool,
    /// Dar vuelta las máscaras en X antes de escribirlas. Sin confirmar con una pieza
    /// asimétrica (el cubo de SLA0 es simétrico): por ahora en `false` en las dos.
    pub flip_x: bool,
}

impl ResinPrinter {
    /// Ancho y alto de un píxel, en mm.
    pub fn pixel_size(&self) -> (f32, f32) {
        (self.bed_x / self.res_x as f32, self.bed_y / self.res_y as f32)
    }

    /// Centro del píxel `(px, py)` de la máscara, en mm de la cama.
    pub fn pixel_to_mm(&self, px: u32, py: u32) -> (f32, f32) {
        let (pw, ph) = self.pixel_size();
        let px = if self.flip_x { self.res_x - 1 - px } else { px };
        ((px as f32 + 0.5) * pw, (py as f32 + 0.5) * ph)
    }

    /// Píxel de la máscara bajo el punto `(x, y)` en mm (`None` fuera de la pantalla).
    pub fn mm_to_pixel(&self, x: f32, y: f32) -> Option<(u32, u32)> {
        let (pw, ph) = self.pixel_size();
        let (fx, fy) = ((x / pw).floor(), (y / ph).floor());
        if fx < 0.0 || fy < 0.0 || fx >= self.res_x as f32 || fy >= self.res_y as f32 {
            return None;
        }
        let px = fx as u32;
        Some((if self.flip_x { self.res_x - 1 - px } else { px }, fy as u32))
    }

    /// Pantalla gruesa del mismo tamaño, con píxeles de `cell` mm (para comparar orientaciones rápido).
    pub fn coarse(&self, cell: f32) -> ResinPrinter {
        ResinPrinter {
            res_x: (self.bed_x / cell).round().max(1.0) as u32,
            res_y: (self.bed_y / cell).round().max(1.0) as u32,
            flip_x: false,
            ..self.clone()
        }
    }
}

/// Las impresoras de resina conocidas.
pub fn printers() -> [&'static ResinPrinter; 2] {
    [&LD_002H, &HALOT_SKY]
}

/// Busca una impresora por su `id`.
pub fn printer(id: &str) -> Option<&'static ResinPrinter> {
    printers().into_iter().find(|p| p.id == id)
}

/// Creality LD-002H: 1620 × 2560 px de 51 µm (datos del `.ctb` de referencia, SLA0).
pub const LD_002H: ResinPrinter = ResinPrinter {
    id: "ld-002h",
    name: "Creality LD-002H",
    res_x: 1620,
    res_y: 2560,
    bed_x: 82.62,
    bed_y: 130.56,
    bed_z: 160.0,
    mirror: true,
    flip_x: false,
};

/// Creality Halot-Sky (CL-89): 3840 × 2400 px de 50 µm (datos del `.cxdlp` de referencia).
pub const HALOT_SKY: ResinPrinter = ResinPrinter {
    id: "halot-sky",
    name: "CL-89",
    res_x: 3840,
    res_y: 2400,
    bed_x: 192.0,
    bed_y: 120.0,
    bed_z: 200.0,
    mirror: false,
    flip_x: false,
};

/// Parámetros de exposición y movimiento (dependen de la resina: siempre calibrar).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ResinSettings {
    /// Alto de capa, mm.
    pub layer_height: f32,
    /// Exposición normal y de las capas de base, s.
    pub exposure: f32,
    pub bottom_exposure: f32,
    pub bottom_layers: u32,
    /// Espera con la luz apagada antes de exponer, s.
    pub light_off: f32,
    pub bottom_light_off: f32,
    /// Subida tras cada capa: distancia (mm) y velocidad (mm/min).
    pub lift: f32,
    pub lift_speed: f32,
    pub bottom_lift: f32,
    pub bottom_lift_speed: f32,
    /// Bajada, mm/min.
    pub retract_speed: f32,
    /// Potencia de la luz (0–255).
    pub pwm: u8,
    pub bottom_pwm: u8,
    /// Densidad (g/ml) y precio por ml, para el resumen.
    pub density: f32,
    pub cost_per_ml: f32,
}

impl Default for ResinSettings {
    /// Los valores del `.ctb` de referencia de la LD-002H.
    fn default() -> Self {
        Self {
            layer_height: 0.05,
            exposure: 3.5,
            bottom_exposure: 20.0,
            bottom_layers: 8,
            light_off: 0.0,
            bottom_light_off: 0.0,
            lift: 7.0,
            lift_speed: 70.0,
            bottom_lift: 7.0,
            bottom_lift_speed: 50.0,
            retract_speed: 150.0,
            pwm: 255,
            bottom_pwm: 255,
            density: 1.1,
            cost_per_ml: 0.0,
        }
    }
}

/// Tramo encendido de una fila: píxeles `x0..x1` de la fila `y`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Span {
    pub y: u32,
    pub x0: u32,
    pub x1: u32,
}

/// Una capa: altura de su cara de arriba y sus tramos, ordenados por fila y luego por x.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Layer {
    pub z: f32,
    pub spans: Vec<Span>,
}

impl Layer {
    /// Píxeles encendidos.
    pub fn area(&self) -> u64 {
        self.spans.iter().map(|s| (s.x1 - s.x0) as u64).sum()
    }

    /// La máscara completa (`res_x × res_y`, fila 0 primero; 255 = luz).
    pub fn to_mask(&self, res_x: u32, res_y: u32) -> Vec<u8> {
        let mut m = vec![0u8; res_x as usize * res_y as usize];
        for s in &self.spans {
            let row = s.y as usize * res_x as usize;
            m[row + s.x0 as usize..row + s.x1 as usize].fill(255);
        }
        m
    }
}

/// Lamina los triángulos en capas de `layer_height` para la pantalla de `printer`.
pub fn slice(triangles: &[[[f32; 3]; 3]], printer: &ResinPrinter, layer_height: f32) -> Result<Vec<Layer>, SlicerError> {
    if triangles.is_empty() {
        return Err(SlicerError::Empty);
    }
    if layer_height.is_nan() || layer_height <= 0.0 {
        return Err(SlicerError::Settings(format!("alto de capa inválido: {layer_height}")));
    }
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for v in triangles.iter().flatten() {
        for k in 0..3 {
            min[k] = min[k].min(v[k]);
            max[k] = max[k].max(v[k]);
        }
    }
    let tol = 1e-3;
    if min[0] < -tol || min[1] < -tol || min[2] < -tol || max[0] > printer.bed_x + tol || max[1] > printer.bed_y + tol || max[2] > printer.bed_z + tol {
        return Err(SlicerError::OutOfBed(format!(
            "la pieza va de ({:.2}, {:.2}, {:.2}) a ({:.2}, {:.2}, {:.2}) mm y la impresora tiene {:.2} × {:.2} × {:.0} mm",
            min[0], min[1], min[2], max[0], max[1], max[2], printer.bed_x, printer.bed_y, printer.bed_z
        )));
    }

    // Capa i: cara de arriba en (i+1)·h, corte a media altura (i+0.5)·h.
    let count = ((max[2] / layer_height) - 1e-4).ceil().max(1.0) as usize;
    let first = |z: f32| ((z / layer_height - 0.5).ceil().max(0.0)) as usize;
    let mut buckets: Vec<Vec<u32>> = vec![Vec::new(); count];
    for (t, tri) in triangles.iter().enumerate() {
        let zmin = tri[0][2].min(tri[1][2]).min(tri[2][2]);
        let zmax = tri[0][2].max(tri[1][2]).max(tri[2][2]);
        // Corta si zmin <= zc < zmax.
        for bucket in buckets.iter_mut().take(first(zmax).min(count)).skip(first(zmin)) {
            bucket.push(t as u32);
        }
    }

    // Malla entera al revés (volumen negativo): se cuenta el giro al revés.
    let volume: f64 = triangles
        .par_iter()
        .map(|t| {
            let [a, b, c] = t.map(|v| v.map(f64::from));
            a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0])
        })
        .sum();
    let sign = if volume < 0.0 { -1 } else { 1 };

    let (pw, ph) = printer.pixel_size();
    // La z de la tabla, en decimal y redondeada a f32 al final (como Chitubox/Lychee).
    let h = (layer_height as f64 * 1e5).round() / 1e5;
    let layers = buckets
        .into_par_iter()
        .enumerate()
        .map(|(i, tris)| {
            let zc = (i as f32 + 0.5) * layer_height;
            Layer { z: ((i + 1) as f64 * h) as f32, spans: slice_layer(triangles, &tris, zc, pw, ph, printer, sign) }
        })
        .collect();
    Ok(layers)
}

/// Corta los triángulos `tris` a la altura `zc` y rellena por filas.
fn slice_layer(triangles: &[[[f32; 3]; 3]], tris: &[u32], zc: f32, pw: f32, ph: f32, printer: &ResinPrinter, sign: i32) -> Vec<Span> {
    // Cruces de cada segmento con las filas: (fila, x en píxeles, giro).
    let mut crossings: Vec<(u32, f32, i32)> = Vec::new();
    for &t in tris {
        let tri = &triangles[t as usize];
        let Some((a, b)) = cut(tri, zc) else { continue };
        let nx = normal_x(tri);
        if nx == 0.0 {
            continue;
        }
        // Normal hacia +x: al cruzar de izquierda a derecha se sale de la pieza.
        let wind = if nx > 0.0 { -sign } else { sign };
        let (ay, by) = (a[1] / ph - 0.5, b[1] / ph - 0.5);
        let (ax, bx) = (a[0] / pw - 0.5, b[0] / pw - 0.5);
        let (lo, hi) = if ay < by { (ay, by) } else { (by, ay) };
        // Filas r con lo <= r < hi (centro de la fila r en r + 0.5 → coordenada r).
        let r0 = lo.ceil().max(0.0) as i64;
        let r1 = (hi.ceil() as i64).min(printer.res_y as i64);
        for r in r0..r1 {
            let s = (r as f32 - ay) / (by - ay);
            crossings.push((r as u32, ax + s * (bx - ax), wind));
        }
    }
    crossings.sort_unstable_by(|p, q| p.0.cmp(&q.0).then(p.1.total_cmp(&q.1)));

    let mut spans = Vec::new();
    let mut i = 0;
    while i < crossings.len() {
        let row = crossings[i].0;
        let mut wind = 0;
        let mut start = 0.0;
        while i < crossings.len() && crossings[i].0 == row {
            let (_, x, w) = crossings[i];
            let before = wind;
            wind += w;
            if before <= 0 && wind > 0 {
                start = x;
            } else if before > 0 && wind <= 0 {
                // Centros x + 0.5 de los píxeles x con start <= x < end (coordenada ya corrida).
                let x0 = start.ceil().max(0.0) as i64;
                let x1 = (x.ceil() as i64).min(printer.res_x as i64);
                if x1 > x0 {
                    push_span(&mut spans, row, x0 as u32, x1 as u32);
                }
            }
            i += 1;
        }
    }
    if printer.flip_x {
        mirror(&mut spans, printer.res_x);
    }
    spans
}

/// Junta tramos que se tocan en la misma fila.
fn push_span(spans: &mut Vec<Span>, y: u32, x0: u32, x1: u32) {
    if let Some(last) = spans.last_mut()
        && last.y == y
        && x0 <= last.x1
    {
        last.x1 = last.x1.max(x1);
        return;
    }
    spans.push(Span { y, x0, x1 });
}

fn mirror(spans: &mut [Span], res_x: u32) {
    for s in spans.iter_mut() {
        (s.x0, s.x1) = (res_x - s.x1, res_x - s.x0);
    }
    spans.sort_unstable();
}

/// Segmento donde el plano z = zc corta el triángulo (vértices con z <= zc van abajo).
fn cut(tri: &[[f32; 3]; 3], zc: f32) -> Option<([f32; 2], [f32; 2])> {
    let above = [tri[0][2] > zc, tri[1][2] > zc, tri[2][2] > zc];
    let n = above.iter().filter(|&&a| a).count();
    if n == 0 || n == 3 {
        return None;
    }
    let mut pts = [[0f32; 2]; 2];
    let mut k = 0;
    for e in 0..3 {
        let (mut p, mut q) = (&tri[e], &tri[(e + 1) % 3]);
        if above[e] != above[(e + 1) % 3] {
            // Siempre del mismo extremo: la arista compartida da el mismo punto exacto en
            // los dos triángulos (si no, una fila puede perder o duplicar un cruce).
            if p[2] > q[2] {
                (p, q) = (q, p);
            }
            let t = (zc - p[2]) / (q[2] - p[2]);
            pts[k] = [p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1])];
            k += 1;
        }
    }
    Some((pts[0], pts[1]))
}

/// Componente x de la normal (sin normalizar; el signo es lo que importa).
fn normal_x(t: &[[f32; 3]; 3]) -> f32 {
    let u = [t[1][0] - t[0][0], t[1][1] - t[0][1], t[1][2] - t[0][2]];
    let v = [t[2][0] - t[0][0], t[2][1] - t[0][1], t[2][2] - t[0][2]];
    u[1] * v[2] - u[2] * v[1]
}

/// Volumen de resina (ml) de las capas.
pub fn volume_ml(layers: &[Layer], printer: &ResinPrinter, layer_height: f32) -> f32 {
    let (pw, ph) = printer.pixel_size();
    let px: u64 = layers.iter().map(Layer::area).sum();
    (px as f64 * pw as f64 * ph as f64 * layer_height as f64 / 1000.0) as f32
}

/// Tiempo de impresión estimado (s): exposición, espera, subida y bajada de cada capa.
pub fn print_time(layer_count: usize, s: &ResinSettings) -> f32 {
    let bottom = (s.bottom_layers as usize).min(layer_count);
    let normal = layer_count - bottom;
    let per = |exp: f32, off: f32, lift: f32, speed: f32| {
        let up = if speed > 0.0 { lift / speed * 60.0 } else { 0.0 };
        let down = if s.retract_speed > 0.0 { lift / s.retract_speed * 60.0 } else { 0.0 };
        exp + off + up + down
    };
    bottom as f32 * per(s.bottom_exposure, s.bottom_light_off, s.bottom_lift, s.bottom_lift_speed)
        + normal as f32 * per(s.exposure, s.light_off, s.lift, s.lift_speed)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Caja cerrada con normales hacia afuera.
    pub(crate) fn cube(o: [f32; 3], s: [f32; 3]) -> Vec<[[f32; 3]; 3]> {
        let p = |i: usize| -> [f32; 3] {
            [o[0] + s[0] * (i & 1) as f32, o[1] + s[1] * ((i >> 1) & 1) as f32, o[2] + s[2] * ((i >> 2) & 1) as f32]
        };
        let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
        let mut t = Vec::new();
        for q in quads {
            t.push([p(q[0]), p(q[1]), p(q[2])]);
            t.push([p(q[0]), p(q[2]), p(q[3])]);
        }
        t
    }

    #[test]
    fn cubo_de_20_mm() {
        let pr = LD_002H;
        let (pw, ph) = pr.pixel_size();
        let tris = cube([30.0, 50.0, 0.0], [20.0, 20.0, 20.0]);
        let layers = slice(&tris, &pr, 0.05).unwrap();
        assert_eq!(layers.len(), 400);
        assert!((layers[0].z - 0.05).abs() < 1e-6);
        for l in [&layers[0], &layers[200], &layers[399]] {
            let x0 = l.spans.iter().map(|s| s.x0).min().unwrap();
            let x1 = l.spans.iter().map(|s| s.x1).max().unwrap();
            let rows = l.spans.len() as f32;
            assert!(((x1 - x0) as f32 * pw - 20.0).abs() < pw, "ancho {}", (x1 - x0) as f32 * pw);
            assert!((rows * ph - 20.0).abs() < ph, "alto {}", rows * ph);
            assert_eq!(x0, (30.0 / pw - 0.5).ceil() as u32);
        }
        let v = volume_ml(&layers, &pr, 0.05);
        assert!((v - 8.0).abs() < 0.1, "volumen {v}");
    }

    #[test]
    fn cascaras_que_se_pisan_se_suman() {
        let mut tris = cube([10.0, 10.0, 0.0], [10.0, 10.0, 1.0]);
        tris.extend(cube([15.0, 10.0, 0.0], [10.0, 10.0, 1.0]));
        let layers = slice(&tris, &LD_002H, 0.05).unwrap();
        // Una sola franja continua por fila (sin hueco en la zona pisada).
        let row = layers[5].spans[0].y;
        assert_eq!(layers[5].spans.iter().filter(|s| s.y == row).count(), 1);
        let (pw, _) = LD_002H.pixel_size();
        let s = layers[5].spans[0];
        assert!(((s.x1 - s.x0) as f32 * pw - 15.0).abs() < pw);
    }

    #[test]
    fn hueco_por_dentro() {
        // Caja con una caja invertida adentro (pieza vaciada).
        let mut tris = cube([10.0, 10.0, 0.0], [20.0, 20.0, 2.0]);
        let inner: Vec<_> = cube([15.0, 15.0, 0.5], [10.0, 10.0, 1.0]).into_iter().map(|t| [t[0], t[2], t[1]]).collect();
        tris.extend(inner);
        let layers = slice(&tris, &LD_002H, 0.05).unwrap();
        let mid = &layers[20];
        let (pw, ph) = LD_002H.pixel_size();
        let full = (20.0 / pw) * (20.0 / ph);
        let hole = (10.0 / pw) * (10.0 / ph);
        assert!(((mid.area() as f32) - (full - hole)).abs() / full < 0.02);
        assert!(((layers[2].area() as f32) - full).abs() / full < 0.02);
    }

    #[test]
    fn fuera_de_la_cama() {
        let tris = cube([70.0, 10.0, 0.0], [20.0, 20.0, 1.0]);
        assert!(matches!(slice(&tris, &LD_002H, 0.05), Err(SlicerError::OutOfBed(_))));
    }

    #[test]
    fn tiempo_como_el_de_referencia() {
        // El .ctb de referencia (500 capas) dice 6335 s.
        let t = print_time(500, &ResinSettings::default());
        assert!((t - 6335.0).abs() < 100.0, "{t}");
    }
}
