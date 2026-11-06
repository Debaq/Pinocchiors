//! `.cxdlp` v2 de Creality para la Halot-Sky (CL-89), SLA17 de `PLAN_SLA.md`.
//!
//! Formato (big-endian), igual que el archivo de referencia hecho con Lychee
//! (`~/Escritorio/pinocchio-cubo-20mm.cxdlp`) y la plantilla de UVtools:
//! - Cabecera: `CXSW3DV2\0` (con su largo), versión 2, modelo `CL-89\0`, capas, ancho y
//!   alto en píxeles y 64 bytes en cero.
//! - Tres miniaturas RGB565 big-endian (116², 290², 290²), cada una seguida de `\r\n`.
//! - Plataforma y alto de capa como texto UTF-16BE con su largo («192.00», «120.00», «0.050»).
//! - 11 valores u16: exposición, espera antes de exponer (nunca 0), exposición de base, capas
//!   de base, subida y velocidad de base, subida y velocidad, bajada, PWM de base y normal.
//! - Área de cada capa (píxeles encendidos), `\r\n`; cada capa: área, cantidad de líneas y
//!   **líneas verticales** de 6 bytes (y inicial 13 bits, y final 13 bits *incluida*, x 14
//!   bits, gris), ordenadas por x y después por y, y `\r\n`.
//! - Pie `CXSW3DV2\0` y un u32 con el XOR de todos los bytes anteriores.
//!
//! **Lo que no está confirmado** (ver [`ExposureUnit`] y [`Mirror`]): UVtools (desde 3.2.0)
//! guarda la exposición normal en **décimas** de segundo y no espeja; Lychee la guardó en
//! **segundos** (3 = 3 s en tu archivo) y espeja en Y. La exposición de base va en segundos
//! en los dos y las velocidades en mm/s enteros. Se decide con [`probe`]: la pantalla de la
//! impresora muestra la exposición del archivo sin imprimir.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::ctb::Image;
use crate::sla::{Layer, ResinPrinter, ResinSettings};
use crate::SlicerError;

const MAGIC: &[u8] = b"CXSW3DV2\0";
/// Miniaturas: tamaños en el orden del archivo.
pub const PREVIEWS: [(u32, u32); 3] = [(116, 116), (290, 290), (290, 290)];
/// Valor crudo de la exposición normal en el archivo de prueba: 3,5 s si son décimas, 35 s si son segundos.
pub const PROBE_RAW_EXPOSURE: u16 = 35;

/// En qué unidad guarda la impresora la exposición normal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExposureUnit {
    /// Décimas de segundo (UVtools desde 3.2.0).
    Tenths,
    /// Segundos enteros (Lychee).
    Seconds,
}

/// Espejo de la imagen al escribirla.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mirror {
    /// Sin espejo (UVtools).
    #[default]
    None,
    X,
    /// Lychee.
    Y,
    /// Los dos (girar 180°).
    Xy,
}

/// Lo que hay que saber de la impresora para escribir el `.cxdlp`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CxdlpOptions {
    /// `None`: todavía no se confirmó, no se escribe el archivo.
    pub exposure_unit: Option<ExposureUnit>,
    pub mirror: Mirror,
}

/// Los 11 valores del archivo, ya en sus unidades.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Params {
    pub exposure: u16,
    pub wait_before: u16,
    pub bottom_exposure: u16,
    pub bottom_layers: u16,
    pub bottom_lift: u16,
    pub bottom_lift_speed: u16,
    pub lift: u16,
    pub lift_speed: u16,
    pub retract_speed: u16,
    pub bottom_pwm: u16,
    pub pwm: u16,
}

impl Params {
    pub fn from_settings(s: &ResinSettings, unit: ExposureUnit) -> Params {
        let r = |v: f32| v.round().clamp(0.0, u16::MAX as f32) as u16;
        // Velocidades: mm/min → mm/s enteros (como UVtools, redondeando).
        let speed = |v: f32| r(v / 60.0).max(1);
        Params {
            exposure: match unit {
                ExposureUnit::Tenths => r(s.exposure * 10.0),
                ExposureUnit::Seconds => r(s.exposure),
            },
            wait_before: r(s.light_off).max(1),
            bottom_exposure: r(s.bottom_exposure),
            bottom_layers: s.bottom_layers.min(u16::MAX as u32) as u16,
            bottom_lift: r(s.bottom_lift),
            bottom_lift_speed: speed(s.bottom_lift_speed),
            lift: r(s.lift),
            lift_speed: speed(s.lift_speed),
            retract_speed: speed(s.retract_speed),
            bottom_pwm: s.bottom_pwm as u16,
            pwm: s.pwm as u16,
        }
    }

    fn values(&self) -> [u16; 11] {
        [
            self.exposure,
            self.wait_before,
            self.bottom_exposure,
            self.bottom_layers,
            self.bottom_lift,
            self.bottom_lift_speed,
            self.lift,
            self.lift_speed,
            self.retract_speed,
            self.bottom_pwm,
            self.pwm,
        ]
    }
}

/// Lo que se redondea al escribir (el formato no guarda decimales): avisos para el usuario.
pub fn rounding_notes(s: &ResinSettings, unit: ExposureUnit) -> Vec<String> {
    let p = Params::from_settings(s, unit);
    let mut out = Vec::new();
    let shown = match unit {
        ExposureUnit::Tenths => p.exposure as f32 / 10.0,
        ExposureUnit::Seconds => p.exposure as f32,
    };
    if (shown - s.exposure).abs() > 0.01 {
        out.push(format!("la exposición {:.2} s queda en {shown} s (el archivo no guarda más decimales)", s.exposure));
    }
    if (p.bottom_exposure as f32 - s.bottom_exposure).abs() > 0.01 {
        out.push(format!("la exposición de base {:.1} s queda en {} s (segundos enteros)", s.bottom_exposure, p.bottom_exposure));
    }
    for (name, v, mm_s) in [("subida", s.lift_speed, p.lift_speed), ("subida de base", s.bottom_lift_speed, p.bottom_lift_speed), ("bajada", s.retract_speed, p.retract_speed)] {
        if (mm_s as f32 * 60.0 - v).abs() > 0.5 {
            out.push(format!("la velocidad de {name} {v:.0} mm/min queda en {} mm/min (mm/s enteros)", mm_s as u32 * 60));
        }
    }
    out
}

/// Escribe el `.cxdlp`. `raw_exposure` reemplaza el valor crudo de la exposición normal
/// (para el archivo de prueba).
pub fn write_cxdlp(
    layers: &[Layer],
    printer: &ResinPrinter,
    settings: &ResinSettings,
    unit: ExposureUnit,
    mirror: Mirror,
    previews: &[Image; 3],
    raw_exposure: Option<u16>,
) -> Result<Vec<u8>, SlicerError> {
    if layers.is_empty() {
        return Err(SlicerError::Empty);
    }
    if layers.len() > u16::MAX as usize {
        return Err(SlicerError::Settings(format!("{} capas: el .cxdlp admite hasta {}", layers.len(), u16::MAX)));
    }
    let (w, h) = (printer.res_x, printer.res_y);
    let encoded: Vec<(u32, Vec<u8>)> = layers.par_iter().map(|l| encode_layer(l, w, h, mirror)).collect();

    let mut o = Vec::with_capacity(1 << 20);
    let u16b = |o: &mut Vec<u8>, v: u16| o.extend_from_slice(&v.to_be_bytes());
    let u32b = |o: &mut Vec<u8>, v: u32| o.extend_from_slice(&v.to_be_bytes());
    u32b(&mut o, MAGIC.len() as u32);
    o.extend_from_slice(MAGIC);
    u16b(&mut o, 2);
    let model = format!("{}\0", printer.name);
    u32b(&mut o, model.len() as u32);
    o.extend_from_slice(model.as_bytes());
    u16b(&mut o, layers.len() as u16);
    u16b(&mut o, w as u16);
    u16b(&mut o, h as u16);
    o.extend_from_slice(&[0; 64]);
    for (img, size) in previews.iter().zip(PREVIEWS) {
        if (img.width, img.height) != size {
            return Err(SlicerError::Settings(format!("miniatura de {}×{}, el .cxdlp pide {}×{}", img.width, img.height, size.0, size.1)));
        }
        for p in img.rgba.as_chunks::<4>().0 {
            let a = p[3] as u32;
            let c = |v: u8| v as u32 * a / 255;
            let v = ((c(p[0]) >> 3) << 11) | ((c(p[1]) >> 2) << 5) | (c(p[2]) >> 3);
            u16b(&mut o, v as u16);
        }
        o.extend_from_slice(b"\r\n");
    }
    for text in [format!("{:.2}", printer.bed_x), format!("{:.2}", printer.bed_y), format!("{:.3}", settings.layer_height)] {
        let utf16: Vec<u8> = text.encode_utf16().flat_map(|c| c.to_be_bytes()).collect();
        u32b(&mut o, utf16.len() as u32);
        o.extend_from_slice(&utf16);
    }
    let mut params = Params::from_settings(settings, unit);
    if let Some(raw) = raw_exposure {
        params.exposure = raw;
    }
    for v in params.values() {
        u16b(&mut o, v);
    }
    for (area, _) in &encoded {
        u32b(&mut o, *area);
    }
    o.extend_from_slice(b"\r\n");
    for (area, lines) in &encoded {
        u32b(&mut o, *area);
        u32b(&mut o, (lines.len() / 6) as u32);
        o.extend_from_slice(lines);
        o.extend_from_slice(b"\r\n");
    }
    u32b(&mut o, MAGIC.len() as u32);
    o.extend_from_slice(MAGIC);
    let x = o.iter().fold(0u8, |a, b| a ^ b);
    u32b(&mut o, x as u32);
    Ok(o)
}

/// Una capa en líneas verticales: (píxeles encendidos, bytes de las líneas).
fn encode_layer(layer: &Layer, w: u32, h: u32, mirror: Mirror) -> (u32, Vec<u8>) {
    // Imagen traspuesta (una columna por x) de la caja de la capa, con el espejo aplicado.
    let spans: Vec<_> = layer.image_spans().filter(|s| s.v > 0).collect();
    if spans.is_empty() {
        return (0, Vec::new());
    }
    let map = |x: u32, y: u32| match mirror {
        Mirror::None => (x, y),
        Mirror::X => (w - 1 - x, y),
        Mirror::Y => (x, h - 1 - y),
        Mirror::Xy => (w - 1 - x, h - 1 - y),
    };
    let mut x0 = u32::MAX;
    let mut x1 = 0;
    let mut y0 = u32::MAX;
    let mut y1 = 0;
    for s in &spans {
        for (x, y) in [map(s.x0, s.y), map(s.x1 - 1, s.y)] {
            x0 = x0.min(x);
            x1 = x1.max(x + 1);
            y0 = y0.min(y);
            y1 = y1.max(y + 1);
        }
    }
    let (cw, ch) = ((x1 - x0) as usize, (y1 - y0) as usize);
    let mut col = vec![0u8; cw * ch];
    let mut area = 0u32;
    for s in &spans {
        for x in s.x0..s.x1 {
            let (mx, my) = map(x, s.y);
            col[(mx - x0) as usize * ch + (my - y0) as usize] = s.v;
        }
        area += s.x1 - s.x0;
    }
    let mut out = Vec::new();
    for cx in 0..cw {
        let c = &col[cx * ch..(cx + 1) * ch];
        let mut y = 0;
        while y < ch {
            let v = c[y];
            if v == 0 {
                y += 1;
                continue;
            }
            let a = y;
            while y < ch && c[y] == v {
                y += 1;
            }
            let (sy, ey, x) = ((y0 as usize + a) as u64, (y0 as usize + y - 1) as u64, (x0 as usize + cx) as u64);
            let bits = (sy << 27) | (ey << 14) | x;
            out.extend_from_slice(&bits.to_be_bytes()[3..8]);
            out.push(v);
        }
    }
    (area, out)
}

/// Lo que se lee de un `.cxdlp` (para las pruebas y para revisar archivos).
#[derive(Debug, Clone, PartialEq)]
pub struct CxdlpFile {
    pub version: u16,
    pub model: String,
    pub res: (u32, u32),
    pub bed: (String, String),
    pub layer_height: String,
    pub params: Params,
    pub areas: Vec<u32>,
    /// Por capa: las líneas (y inicial, y final incluida, x, gris).
    pub layers: Vec<Vec<(u32, u32, u32, u8)>>,
}

impl CxdlpFile {
    pub fn parse(b: &[u8]) -> Result<CxdlpFile, SlicerError> {
        let bad = |m: &str| SlicerError::Format(format!("cxdlp: {m}"));
        let mut o = 0usize;
        let take = |o: &mut usize, n: usize| -> Result<&[u8], SlicerError> {
            let s = b.get(*o..*o + n).ok_or_else(|| bad("archivo cortado"))?;
            *o += n;
            Ok(s)
        };
        let u16r = |o: &mut usize| -> Result<u16, SlicerError> { Ok(u16::from_be_bytes(take(o, 2)?.try_into().unwrap())) };
        let u32r = |o: &mut usize| -> Result<u32, SlicerError> { Ok(u32::from_be_bytes(take(o, 4)?.try_into().unwrap())) };
        let n = u32r(&mut o)? as usize;
        if take(&mut o, n)? != MAGIC {
            return Err(bad("no es un .cxdlp v2"));
        }
        let version = u16r(&mut o)?;
        let n = u32r(&mut o)? as usize;
        let model = String::from_utf8_lossy(take(&mut o, n)?).trim_end_matches('\0').to_string();
        let (count, w, h) = (u16r(&mut o)? as usize, u16r(&mut o)? as u32, u16r(&mut o)? as u32);
        take(&mut o, 64)?;
        for (pw, ph) in PREVIEWS {
            take(&mut o, (pw * ph * 2) as usize + 2)?;
        }
        let text = |o: &mut usize| -> Result<String, SlicerError> {
            let n = u32r(o)? as usize;
            let raw = take(o, n)?;
            let units: Vec<u16> = raw.as_chunks::<2>().0.iter().map(|c| u16::from_be_bytes(*c)).collect();
            Ok(String::from_utf16_lossy(&units))
        };
        let (bx, by, lh) = (text(&mut o)?, text(&mut o)?, text(&mut o)?);
        let mut v = [0u16; 11];
        for x in &mut v {
            *x = u16r(&mut o)?;
        }
        let params = Params {
            exposure: v[0],
            wait_before: v[1],
            bottom_exposure: v[2],
            bottom_layers: v[3],
            bottom_lift: v[4],
            bottom_lift_speed: v[5],
            lift: v[6],
            lift_speed: v[7],
            retract_speed: v[8],
            bottom_pwm: v[9],
            pwm: v[10],
        };
        let areas = (0..count).map(|_| u32r(&mut o)).collect::<Result<Vec<_>, _>>()?;
        take(&mut o, 2)?;
        let mut layers = Vec::with_capacity(count);
        for _ in 0..count {
            let _area = u32r(&mut o)?;
            let lines = u32r(&mut o)? as usize;
            let data = take(&mut o, lines * 6)?;
            layers.push(
                data.as_chunks::<6>()
                    .0
                    .iter()
                    .map(|c| {
                        let v = u64::from_be_bytes([0, 0, 0, c[0], c[1], c[2], c[3], c[4]]);
                        ((v >> 27) as u32, ((v >> 14) & 0x1FFF) as u32, (v & 0x3FFF) as u32, c[5])
                    })
                    .collect(),
            );
            take(&mut o, 2)?;
        }
        let n = u32r(&mut o)? as usize;
        take(&mut o, n)?;
        let x = b[..o].iter().fold(0u8, |a, b| a ^ b);
        if u32r(&mut o)? != x as u32 {
            return Err(bad("checksum distinto"));
        }
        Ok(CxdlpFile { version, model, res: (w, h), bed: (bx, by), layer_height: lh, params, areas, layers })
    }
}

/// Pieza de prueba: una «F» de 30 × 40 mm y 1 mm de alto (sin soportes) en la esquina de
/// x e y chicas (adelante a la izquierda mirando la impresora de frente). Al elegir el
/// archivo en la impresora (sin imprimir) la pantalla muestra la exposición normal: con
/// [`PROBE_RAW_EXPOSURE`] = 35, «3.5» son décimas y «35» segundos. Impresa (unos minutos),
/// la esquina de la placa donde quedó dice el espejo: adelante a la izquierda ninguno, a la
/// derecha X, atrás a la izquierda Y, atrás a la derecha los dos.
pub fn probe_piece(printer: &ResinPrinter) -> Vec<[[f32; 3]; 3]> {
    let (x, y) = (printer.bed_x * 0.15, printer.bed_y * 0.15);
    let mut t = Vec::new();
    // Palo vertical, brazo de arriba y brazo del medio (la F mira hacia +x, arriba es +y).
    for (a, b) in [([0.0, 0.0], [8.0, 40.0]), ([8.0, 32.0], [30.0, 40.0]), ([8.0, 16.0], [22.0, 24.0])] {
        crate::sla_label::cuboid(&mut t, [x + a[0], y + a[1], 0.0], [x + b[0], y + b[1], 1.0]);
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sla::{slice, HALOT_SKY};

    fn blank() -> [Image; 3] {
        PREVIEWS.map(|(w, h)| Image { width: w, height: h, rgba: vec![0; (w * h * 4) as usize] })
    }

    /// El cubo de 20 mm con el escalón de 10 mm del archivo de referencia, centrado.
    fn reference_piece() -> Vec<[[f32; 3]; 3]> {
        let mut t = crate::sla::tests::cube([86.0, 50.0, 0.0], [20.0, 20.0, 20.0]);
        t.extend(crate::sla::tests::cube([91.0, 55.0, 20.0], [10.0, 10.0, 5.0]));
        t
    }

    #[test]
    fn ida_y_vuelta_y_parametros() {
        let s = ResinSettings { exposure: 3.5, bottom_exposure: 30.0, bottom_layers: 6, light_off: 0.0, lift: 7.0, bottom_lift: 7.0, lift_speed: 70.0, bottom_lift_speed: 65.0, retract_speed: 150.0, ..Default::default() };
        let layers = slice(&reference_piece(), &HALOT_SKY, s.layer_height).unwrap();
        let b = write_cxdlp(&layers, &HALOT_SKY, &s, ExposureUnit::Tenths, Mirror::None, &blank(), None).unwrap();
        let f = CxdlpFile::parse(&b).unwrap();
        assert_eq!((f.version, f.model.as_str(), f.res), (2, "CL-89", (3840, 2400)));
        assert_eq!((f.bed.0.as_str(), f.bed.1.as_str(), f.layer_height.as_str()), ("192.00", "120.00", "0.050"));
        assert_eq!(f.params.values(), [35, 1, 30, 6, 7, 1, 7, 1, 3, 255, 255]);
        assert_eq!(f.layers.len(), 500);
        // Mismas formas que el de Lychee: 400 × 400 px abajo, 200 × 200 arriba, líneas por x.
        assert_eq!((f.areas[0], f.layers[0].len()), (160000, 400));
        assert_eq!((f.areas[450], f.layers[450].len()), (40000, 200));
        let l = &f.layers[0];
        assert!(l.windows(2).all(|w| w[0].2 < w[1].2));
        assert_eq!(l[0].1 - l[0].0 + 1, 400);
        // Segundos: 3,5 → 4.
        let b = write_cxdlp(&layers, &HALOT_SKY, &s, ExposureUnit::Seconds, Mirror::None, &blank(), None).unwrap();
        assert_eq!(CxdlpFile::parse(&b).unwrap().params.exposure, 4);
        assert!(!rounding_notes(&s, ExposureUnit::Seconds).is_empty());
    }

    #[test]
    fn el_espejo_da_vuelta_la_columna() {
        let layers = slice(&probe_piece(&HALOT_SKY), &HALOT_SKY, 0.05).unwrap();
        let s = ResinSettings::default();
        let read = |m| CxdlpFile::parse(&write_cxdlp(&layers, &HALOT_SKY, &s, ExposureUnit::Tenths, m, &blank(), Some(PROBE_RAW_EXPOSURE)).unwrap()).unwrap();
        let (n, y, x) = (read(Mirror::None), read(Mirror::Y), read(Mirror::X));
        let both = read(Mirror::Xy);
        assert_eq!(both.layers[5].iter().map(|l| l.2).min(), x.layers[5].iter().map(|l| l.2).min());
        assert_eq!(both.layers[5].iter().map(|l| l.0).min(), y.layers[5].iter().map(|l| l.0).min());
        assert_eq!(n.params.exposure, 35);
        let ys = |f: &CxdlpFile| (f.layers[5].iter().map(|l| l.0).min().unwrap(), f.layers[5].iter().map(|l| l.1).max().unwrap());
        let xs = |f: &CxdlpFile| (f.layers[5].iter().map(|l| l.2).min().unwrap(), f.layers[5].iter().map(|l| l.2).max().unwrap());
        let ((a, b), (c, d)) = (ys(&n), ys(&y));
        assert_eq!((c, d), (2399 - b, 2399 - a));
        assert_eq!(xs(&n), xs(&y));
        let ((a, b), (c, d)) = (xs(&n), xs(&x));
        assert_eq!((c, d), (3839 - b, 3839 - a));
        assert_eq!(n.areas[5], y.areas[5]);
    }

    #[test]
    fn el_archivo_de_referencia_se_lee() {
        let path = std::path::Path::new(&std::env::var("HOME").unwrap_or_default()).join("Escritorio/pinocchio-cubo-20mm.cxdlp");
        let Ok(b) = std::fs::read(path) else { return };
        let f = CxdlpFile::parse(&b).unwrap();
        assert_eq!(f.params.values(), [3, 1, 30, 6, 7, 1, 7, 1, 1, 255, 255]);
        assert_eq!((f.areas[0], f.layers[0].len(), f.layers.len()), (160000, 400, 500));
    }

    /// Con el STL y el `.cxdlp` de Lychee en el escritorio: en la misma posición, las capas
    /// salen igual byte a byte (todo menos miniaturas, velocidades y checksum).
    #[test]
    fn igual_al_de_lychee() {
        let home = std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Escritorio");
        let (Ok(stl), Ok(reference)) = (std::fs::read(home.join("pinocchio-cubo-20mm.stl")), std::fs::read(home.join("pinocchio-cubo-20mm.cxdlp"))) else { return };
        let n = u32::from_le_bytes(stl[80..84].try_into().unwrap()) as usize;
        let f = |o: usize| f32::from_le_bytes(stl[o..o + 4].try_into().unwrap());
        let mut tris: Vec<[[f32; 3]; 3]> = (0..n).map(|i| std::array::from_fn(|k| std::array::from_fn(|c| f(84 + 50 * i + 12 + 12 * k + 4 * c)))).collect();
        // Lychee la dejó en los píxeles x 1718.., y 1000.. (el cubo de 20 mm).
        let (mut min, mut max) = ([f32::MAX; 3], [f32::MIN; 3]);
        for v in tris.iter().flatten() {
            for c in 0..3 {
                min[c] = min[c].min(v[c]);
                max[c] = max[c].max(v[c]);
            }
        }
        let shift = [1718.0 * 0.05 + 0.025 - min[0], 1000.0 * 0.05 + 0.025 - min[1], -min[2]];
        for v in tris.iter_mut().flatten() {
            for c in 0..3 {
                v[c] += shift[c];
            }
        }
        let s = ResinSettings { exposure: 3.0, bottom_exposure: 30.0, bottom_layers: 6, lift: 7.0, bottom_lift: 7.0, lift_speed: 70.0, bottom_lift_speed: 65.0, retract_speed: 60.0, ..Default::default() };
        let layers = slice(&tris, &HALOT_SKY, 0.05).unwrap();
        let ours = write_cxdlp(&layers, &HALOT_SKY, &s, ExposureUnit::Seconds, Mirror::None, &blank(), None).unwrap();
        assert_eq!(ours.len(), reference.len());
        let previews_end = 95 + PREVIEWS.iter().map(|(w, h)| (w * h * 2 + 2) as usize).sum::<usize>();
        assert_eq!(ours[..95], reference[..95], "cabecera");
        assert_eq!(ours[previews_end..ours.len() - 4], reference[previews_end..reference.len() - 4], "texto, valores y capas");
    }
}
