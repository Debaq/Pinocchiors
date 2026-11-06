//! Formato `.ctb` de Chitubox, versión 2 (el que lee la LD-002H). Little-endian.
//!
//! Orden del archivo, igual que el de referencia de SLA0 (`PLAN_SLA.md`):
//! cabecera (112) · miniatura grande (32 + datos) · miniatura chica (32 + datos) ·
//! parámetros de impresión (60) · datos del laminador (76) · nombre de la máquina ·
//! tabla de capas (36 por capa) · capas.
//!
//! Cada capa va en RLE de 7 bits y cifrada con la clave de la cabecera («mngo»).
//! Las miniaturas van en RGB565 sin rachas (`RRRRR GGGGG 0 BBBBB`).

use crate::sla::{print_time, volume_ml, Layer, ResinPrinter, ResinSettings};
use crate::SlicerError;

const MAGIC: u32 = 0x12FD_0086;
const VERSION: u32 = 2;
const HEADER: usize = 112;
const PREVIEW_HEADER: usize = 32;
const PARAMS: usize = 60;
const SLICER_INFO: usize = 76;
const LAYER_ENTRY: usize = 36;
/// Clave del archivo de referencia («mngo» en little-endian).
const KEY: u32 = 0x6F67_6E6D;
/// Valores fijos de los datos del laminador del archivo de referencia.
const PER_LAYER_SETTINGS: u32 = 7;
const MYSTERIOUS_ID: u32 = 0x6D61_6E67;
const SOFTWARE_VERSION: u32 = 0x0106_0300;

/// Una imagen RGBA con la fila de arriba primero.
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Tamaños de las miniaturas del archivo de referencia.
pub const LARGE_PREVIEW: (u32, u32) = (210, 210);
pub const SMALL_PREVIEW: (u32, u32) = (200, 200);

/// Miniatura de las piezas para la pantalla de la impresora (fila de arriba primero).
pub fn preview_image(parts: &[Vec<[[f32; 3]; 3]>], (width, height): (u32, u32), color: [u8; 3]) -> Image {
    let bottom_up = crate::thumbnail::render_thumbnail(parts, width, height, color);
    let row = width as usize * 4;
    let rgba = bottom_up.chunks_exact(row).rev().flatten().copied().collect();
    Image { width, height, rgba }
}

/// Escribe el `.ctb` completo en memoria.
pub fn write_ctb(
    layers: &[Layer],
    printer: &ResinPrinter,
    settings: &ResinSettings,
    large: &Image,
    small: &Image,
) -> Result<Vec<u8>, SlicerError> {
    use rayon::prelude::*;
    if layers.is_empty() {
        return Err(SlicerError::Empty);
    }
    let encoded: Vec<Vec<u8>> = layers
        .par_iter()
        .enumerate()
        .map(|(i, l)| {
            let mut d = encode_layer(l, printer.res_x, printer.res_y);
            crypt(KEY, i as u32, &mut d);
            d
        })
        .collect();

    let large_px = preview_pixels(large);
    let small_px = preview_pixels(small);
    let name = printer.name.as_bytes();

    let large_at = HEADER;
    let small_at = large_at + PREVIEW_HEADER + large_px.len();
    let params_at = small_at + PREVIEW_HEADER + small_px.len();
    let info_at = params_at + PARAMS;
    let name_at = info_at + SLICER_INFO;
    let table_at = name_at + name.len();
    let data_at = table_at + LAYER_ENTRY * layers.len();
    let total = data_at + encoded.iter().map(Vec::len).sum::<usize>();
    if total > u32::MAX as usize {
        return Err(SlicerError::Settings("el archivo pasa de 4 GB".into()));
    }

    let s = settings;
    let height = layers.last().map(|l| l.z).unwrap_or(0.0);
    let volume = volume_ml(layers, printer, s.layer_height);
    let mut w = Writer(Vec::with_capacity(total));

    // Cabecera.
    w.u32(MAGIC);
    w.u32(VERSION);
    w.f32(printer.bed_x);
    w.f32(printer.bed_y);
    w.f32(printer.bed_z);
    w.u32(0);
    w.u32(0);
    w.f32(height);
    w.f32(s.layer_height);
    w.f32(s.exposure);
    w.f32(s.bottom_exposure);
    w.f32(s.light_off);
    w.u32(s.bottom_layers);
    w.u32(printer.res_x);
    w.u32(printer.res_y);
    w.u32(large_at as u32);
    w.u32(table_at as u32);
    w.u32(layers.len() as u32);
    w.u32(small_at as u32);
    w.u32(print_time(layers.len(), s).round() as u32);
    w.u32(printer.mirror as u32);
    w.u32(params_at as u32);
    w.u32(PARAMS as u32);
    w.u32(1); // antialias: sin grises
    w.u16(s.pwm as u16);
    w.u16(s.bottom_pwm as u16);
    w.u32(KEY);
    w.u32(info_at as u32);
    w.u32(SLICER_INFO as u32);
    debug_assert_eq!(w.0.len(), HEADER);

    // Miniaturas.
    for (img, px) in [(large, &large_px), (small, &small_px)] {
        let at = w.0.len() + PREVIEW_HEADER;
        w.u32(img.width);
        w.u32(img.height);
        w.u32(at as u32);
        w.u32(px.len() as u32);
        w.zeros(16);
        w.0.extend_from_slice(px);
    }

    // Parámetros de impresión.
    debug_assert_eq!(w.0.len(), params_at);
    w.f32(s.bottom_lift);
    w.f32(s.bottom_lift_speed);
    w.f32(s.lift);
    w.f32(s.lift_speed);
    w.f32(s.retract_speed);
    w.f32(volume);
    w.f32(volume * s.density);
    w.f32(volume * s.cost_per_ml);
    w.f32(s.bottom_light_off);
    w.f32(s.light_off);
    w.u32(s.bottom_layers);
    w.zeros(16);

    // Datos del laminador.
    debug_assert_eq!(w.0.len(), info_at);
    w.zeros(28);
    w.u32(name_at as u32);
    w.u32(name.len() as u32);
    w.u32(PER_LAYER_SETTINGS);
    w.u32(MYSTERIOUS_ID);
    w.u32(1); // antialias
    w.u32(SOFTWARE_VERSION);
    w.zeros(24);
    w.0.extend_from_slice(name);

    // Tabla de capas.
    debug_assert_eq!(w.0.len(), table_at);
    let mut at = data_at;
    for (i, (l, d)) in layers.iter().zip(&encoded).enumerate() {
        let bottom = (i as u32) < s.bottom_layers;
        w.f32(l.z);
        w.f32(if bottom { s.bottom_exposure } else { s.exposure });
        w.f32(if bottom { s.bottom_light_off } else { s.light_off });
        w.u32(at as u32);
        w.u32(d.len() as u32);
        w.zeros(16);
        at += d.len();
    }
    for d in &encoded {
        w.0.extend_from_slice(d);
    }
    debug_assert_eq!(w.0.len(), total);
    Ok(w.0)
}

struct Writer(Vec<u8>);

impl Writer {
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn f32(&mut self, v: f32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn zeros(&mut self, n: usize) {
        self.0.resize(self.0.len() + n, 0);
    }
}

/// RGB565 de Chitubox sin rachas: `RRRRR GGGGG 0 BBBBB`. La transparencia se mezcla con negro.
fn preview_pixels(img: &Image) -> Vec<u8> {
    let mut out = Vec::with_capacity(img.width as usize * img.height as usize * 2);
    for p in img.rgba.as_chunks::<4>().0 {
        let a = p[3] as u32;
        let c = |v: u8| (v as u32 * a / 255) >> 3;
        let v = (c(p[0]) << 11) | (c(p[1]) << 6) | c(p[2]);
        out.extend_from_slice(&(v as u16).to_le_bytes());
    }
    out
}

/// Capa en RLE de 7 bits: cada byte es un gris de 7 bits; con el bit alto, le sigue el
/// largo de la racha (1–4 bytes). Las rachas cruzan las filas.
pub fn encode_layer(layer: &Layer, res_x: u32, res_y: u32) -> Vec<u8> {
    let mut out = Vec::new();
    let total = res_x as u64 * res_y as u64;
    let mut pos = 0u64;
    for s in &layer.spans {
        let start = s.y as u64 * res_x as u64 + s.x0 as u64;
        let end = s.y as u64 * res_x as u64 + s.x1 as u64;
        run(&mut out, 0, start - pos);
        run(&mut out, 0x7F, end - start);
        pos = end;
    }
    run(&mut out, 0, total - pos);
    out
}

fn run(out: &mut Vec<u8>, code: u8, mut len: u64) {
    const MAX: u64 = 0x0FFF_FFFF;
    while len > 0 {
        let n = len.min(MAX) as u32;
        len -= n as u64;
        if n == 1 {
            out.push(code);
        } else {
            out.push(code | 0x80);
            if n < 0x80 {
                out.push(n as u8);
            } else if n < 0x4000 {
                out.extend_from_slice(&[0x80 | (n >> 8) as u8, n as u8]);
            } else if n < 0x20_0000 {
                out.extend_from_slice(&[0xC0 | (n >> 16) as u8, (n >> 8) as u8, n as u8]);
            } else {
                out.extend_from_slice(&[0xE0 | (n >> 24) as u8, (n >> 16) as u8, (n >> 8) as u8, n as u8]);
            }
        }
    }
}

/// Cifra (o descifra: es un XOR) los datos de la capa `layer` con la clave `seed`.
pub fn crypt(seed: u32, layer: u32, data: &mut [u8]) {
    if seed == 0 {
        return;
    }
    let init = seed.wrapping_mul(0x2d83_cdac).wrapping_add(0xd8a8_3423);
    let mut key = layer.wrapping_mul(0x1e15_30cd).wrapping_add(0xec3d_47cd).wrapping_mul(init);
    for (n, b) in data.iter_mut().enumerate() {
        *b ^= (key >> (8 * (n & 3))) as u8;
        if n & 3 == 3 {
            key = key.wrapping_add(init);
        }
    }
}

/// Lo que se lee de un `.ctb` v2 (para verificar lo que escribimos y para la vista previa).
#[derive(Debug, Clone, PartialEq)]
pub struct CtbFile {
    pub version: u32,
    pub bed: [f32; 3],
    pub res: (u32, u32),
    pub layer_height: f32,
    pub exposure: f32,
    pub bottom_exposure: f32,
    pub bottom_layers: u32,
    pub mirror: bool,
    pub print_time: u32,
    pub volume_ml: f32,
    pub machine: String,
    /// Por capa: z, exposición y los bytes cifrados.
    pub layers: Vec<(f32, f32, std::ops::Range<usize>)>,
    key: u32,
}

impl CtbFile {
    pub fn parse(b: &[u8]) -> Result<Self, SlicerError> {
        let bad = |m: &str| SlicerError::Format(format!("ctb: {m}"));
        let u = |o: usize| -> Result<u32, SlicerError> {
            b.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap())).ok_or_else(|| bad("archivo cortado"))
        };
        let f = |o: usize| u(o).map(f32::from_bits);
        if u(0)? != MAGIC {
            return Err(bad("no es un .ctb"));
        }
        let version = u(4)?;
        let (table, count, params, info) = (u(64)? as usize, u(68)? as usize, u(84)? as usize, u(104)? as usize);
        let (name_at, name_len) = (u(info + 28)? as usize, u(info + 32)? as usize);
        let machine = b.get(name_at..name_at + name_len).ok_or_else(|| bad("nombre cortado"))?;
        let mut layers = Vec::with_capacity(count);
        for i in 0..count {
            let o = table + i * LAYER_ENTRY;
            let (at, len) = (u(o + 12)? as usize, u(o + 16)? as usize);
            if at + len > b.len() {
                return Err(bad("capa fuera del archivo"));
            }
            layers.push((f(o)?, f(o + 4)?, at..at + len));
        }
        Ok(Self {
            version,
            bed: [f(8)?, f(12)?, f(16)?],
            res: (u(52)?, u(56)?),
            layer_height: f(32)?,
            exposure: f(36)?,
            bottom_exposure: f(40)?,
            bottom_layers: u(48)?,
            mirror: u(80)? != 0,
            print_time: u(76)?,
            volume_ml: f(params + 20)?,
            machine: String::from_utf8_lossy(machine).into_owned(),
            layers,
            key: u(100)?,
        })
    }

    /// La máscara de la capa `i` (`res.0 × res.1`, fila 0 primero).
    pub fn mask(&self, b: &[u8], i: usize) -> Result<Vec<u8>, SlicerError> {
        let mut d = b[self.layers[i].2.clone()].to_vec();
        crypt(self.key, i as u32, &mut d);
        decode_layer(&d, self.res.0 as usize * self.res.1 as usize)
    }
}

/// Inversa de [`encode_layer`].
pub fn decode_layer(d: &[u8], pixels: usize) -> Result<Vec<u8>, SlicerError> {
    let bad = || SlicerError::Format("ctb: RLE inválido".into());
    let mut out = Vec::with_capacity(pixels);
    let mut n = 0;
    while n < d.len() {
        let mut code = d[n];
        let mut len = 1usize;
        if code & 0x80 != 0 {
            code &= 0x7F;
            n += 1;
            let s = *d.get(n).ok_or_else(bad)? as usize;
            let extra = match s {
                _ if s & 0x80 == 0 => 0,
                _ if s & 0xC0 == 0x80 => 1,
                _ if s & 0xE0 == 0xC0 => 2,
                _ if s & 0xF0 == 0xE0 => 3,
                _ => return Err(bad()),
            };
            len = s & [0x7F, 0x3F, 0x1F, 0x0F][extra];
            for _ in 0..extra {
                n += 1;
                len = (len << 8) | *d.get(n).ok_or_else(bad)? as usize;
            }
        }
        let gray = if code == 0 { 0 } else { (code << 1) | 1 };
        out.resize(out.len() + len, gray);
        n += 1;
    }
    if out.len() != pixels {
        return Err(SlicerError::Format(format!("ctb: la capa trae {} píxeles y deberían ser {pixels}", out.len())));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sla::{slice, Span, LD_002H};

    fn blank(w: u32, h: u32) -> Image {
        Image { width: w, height: h, rgba: vec![255; (w * h * 4) as usize] }
    }

    #[test]
    fn rle_ida_y_vuelta() {
        let l = Layer {
            z: 0.05,
            spans: vec![
                Span { y: 0, x0: 0, x1: 1 },
                Span { y: 0, x0: 3, x1: 200 },
                Span { y: 7, x0: 10, x1: 20_000 % 1620 },
                Span { y: 2559, x0: 1619, x1: 1620 },
            ],
        };
        let d = encode_layer(&l, 1620, 2560);
        assert_eq!(decode_layer(&d, 1620 * 2560).unwrap(), l.to_mask(1620, 2560));
        // Una racha larga de 4 bytes.
        let mut big = Vec::new();
        run(&mut big, 0, 0x30_0000);
        assert_eq!(big.len(), 5);
        assert_eq!(decode_layer(&big, 0x30_0000).unwrap().len(), 0x30_0000);
    }

    #[test]
    fn cifrado_es_su_propia_inversa() {
        let mut d: Vec<u8> = (0..1000u32).map(|i| i as u8).collect();
        crypt(KEY, 17, &mut d);
        assert_ne!(d[..8], [0, 1, 2, 3, 4, 5, 6, 7]);
        crypt(KEY, 17, &mut d);
        assert!(d.iter().enumerate().all(|(i, &b)| b == i as u8));
    }

    #[test]
    fn archivo_completo() {
        let tris = crate::sla::tests::cube([31.31, 55.28, 0.0], [20.0, 20.0, 25.0]);
        let s = ResinSettings::default();
        let layers = slice(&tris, &LD_002H, s.layer_height).unwrap();
        let b = write_ctb(&layers, &LD_002H, &s, &blank(210, 210), &blank(200, 200)).unwrap();
        let f = CtbFile::parse(&b).unwrap();
        assert_eq!(f.version, 2);
        assert_eq!(f.res, (1620, 2560));
        assert_eq!(f.layers.len(), 500);
        assert_eq!(f.machine, "Creality LD-002H");
        assert!(f.mirror);
        assert_eq!(f.layers[7].1, 20.0);
        assert_eq!(f.layers[8].1, 3.5);
        assert!((f.layers[499].0 - 25.0).abs() < 1e-4);
        assert!((f.volume_ml - 10.0).abs() < 0.1, "{}", f.volume_ml);
        for i in [0, 250, 499] {
            assert_eq!(f.mask(&b, i).unwrap(), layers[i].to_mask(1620, 2560));
        }
    }
}
