//! XCF, el formato de GIMP. Referencia: `devel-docs/xcf.txt` de GIMP.
//!
//! Se escribe la versión 10 (GIMP 2.10 en adelante; punteros de 32 bits):
//! RGBA de 8 bits no lineal, RLE, capas en modo normal.
//! Se lee de la 0 a la actual: punteros de 64 bits desde la 11, precisión
//! desde la 4 (enteros de 8/16/32 bits y flotantes de 16/32/64, lineales o
//! no), RLE o zlib, RGB, gris o paleta, grupos y máscaras de capa.

use std::io::Read;

use crate::{LayeredImage, Layer, LayersError, Reader, Result, clean_name, linear_to_srgb8, unit_to8};

const TILE: usize = 64;

const PROP_END: u32 = 0;
const PROP_COLORMAP: u32 = 1;
const PROP_OPACITY: u32 = 6;
const PROP_MODE: u32 = 7;
const PROP_VISIBLE: u32 = 8;
const PROP_APPLY_MASK: u32 = 11;
const PROP_OFFSETS: u32 = 15;
const PROP_COMPRESSION: u32 = 17;
const PROP_GROUP_ITEM: u32 = 29;
const PROP_ITEM_PATH: u32 = 30;
const PROP_FLOAT_OPACITY: u32 = 33;

/// `GIMP_LAYER_MODE_NORMAL` (0 es el normal heredado de GIMP 2.8)
const MODE_NORMAL: u32 = 28;

// ─── Escritura ──────────────────────────────────────────────────────────────

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn put_prop(out: &mut Vec<u8>, id: u32, payload: &[u8]) {
    put_u32(out, id);
    put_u32(out, payload.len() as u32);
    out.extend_from_slice(payload);
}

fn put_string(out: &mut Vec<u8>, s: &str) {
    put_u32(out, s.len() as u32 + 1);
    out.extend_from_slice(s.as_bytes());
    out.push(0);
}

fn patch(out: &mut [u8], at: usize, v: u32) {
    out[at..at + 4].copy_from_slice(&v.to_be_bytes());
}

pub fn write(image: &LayeredImage) -> Vec<u8> {
    let mut out = b"gimp xcf v010\0".to_vec();
    put_u32(&mut out, image.width);
    put_u32(&mut out, image.height);
    put_u32(&mut out, 0); // RGB
    put_u32(&mut out, 150); // 8 bits, no lineal (sRGB)
    put_prop(&mut out, PROP_COMPRESSION, &[1]); // RLE
    put_prop(&mut out, PROP_END, &[]);
    // Punteros a capas (de arriba hacia abajo), 0, y a canales, 0
    let slots = out.len();
    out.resize(slots + 4 * (image.layers.len() + 2), 0);
    for (k, layer) in image.layers.iter().rev().enumerate() {
        let at = out.len() as u32;
        patch(&mut out, slots + 4 * k, at);
        write_layer(&mut out, layer);
    }
    out
}

fn write_layer(out: &mut Vec<u8>, layer: &Layer) {
    let (w, h) = (layer.width.max(1), layer.height.max(1));
    put_u32(out, w);
    put_u32(out, h);
    put_u32(out, 1); // RGBA
    put_string(out, &layer.name);
    put_prop(out, PROP_OPACITY, &((layer.opacity.clamp(0.0, 1.0) * 255.0).round() as u32).to_be_bytes());
    put_prop(out, PROP_VISIBLE, &(layer.visible as u32).to_be_bytes());
    let mut offsets = layer.x.to_be_bytes().to_vec();
    offsets.extend_from_slice(&layer.y.to_be_bytes());
    put_prop(out, PROP_OFFSETS, &offsets);
    put_prop(out, PROP_MODE, &MODE_NORMAL.to_be_bytes());
    put_prop(out, PROP_END, &[]);
    let hierarchy_slot = out.len();
    put_u32(out, 0);
    put_u32(out, 0); // sin máscara
    let at = out.len() as u32;
    patch(out, hierarchy_slot, at);

    // Jerarquía: un solo nivel (GIMP no lee los otros)
    put_u32(out, w);
    put_u32(out, h);
    put_u32(out, 4);
    let level_slot = out.len();
    put_u32(out, 0);
    put_u32(out, 0);
    let at = out.len() as u32;
    patch(out, level_slot, at);

    put_u32(out, w);
    put_u32(out, h);
    let (tx, ty) = ((w as usize).div_ceil(TILE), (h as usize).div_ceil(TILE));
    let tiles_slot = out.len();
    out.resize(tiles_slot + 4 * (tx * ty + 1), 0);
    let empty = [0u8; 4];
    for t in 0..tx * ty {
        let at = out.len() as u32;
        patch(out, tiles_slot + 4 * t, at);
        let (x0, y0) = ((t % tx) * TILE, (t / tx) * TILE);
        let (tw, th) = (TILE.min(w as usize - x0), TILE.min(h as usize - y0));
        for c in 0..4 {
            let plane: Vec<u8> = (0..tw * th)
                .map(|i| {
                    let (x, y) = (x0 + i % tw, y0 + i / tw);
                    if layer.width == 0 || layer.height == 0 {
                        empty[c]
                    } else {
                        layer.pixels[(y * layer.width as usize + x) * 4 + c]
                    }
                })
                .collect();
            rle_encode(&plane, out);
        }
    }
}

/// RLE de XCF sobre un plano de bytes
fn rle_encode(data: &[u8], out: &mut Vec<u8>) {
    let n = data.len();
    let mut i = 0;
    while i < n {
        let mut run = 1;
        while i + run < n && run < 32768 && data[i + run] == data[i] {
            run += 1;
        }
        if run >= 3 {
            if run <= 127 {
                out.push((run - 1) as u8);
            } else {
                out.push(127);
                out.extend_from_slice(&(run as u16).to_be_bytes());
            }
            out.push(data[i]);
            i += run;
            continue;
        }
        let start = i;
        while i < n && i - start < 32768 {
            if i + 2 < n && data[i] == data[i + 1] && data[i] == data[i + 2] {
                break;
            }
            i += 1;
        }
        let count = i - start;
        if count <= 127 {
            out.push((256 - count) as u8);
        } else {
            out.push(128);
            out.extend_from_slice(&(count as u16).to_be_bytes());
        }
        out.extend_from_slice(&data[start..i]);
    }
}

fn rle_decode(r: &mut Reader, expected: usize, out: &mut Vec<u8>) -> Result<()> {
    let target = out.len() + expected;
    while out.len() < target {
        let op = r.u8()?;
        match op {
            0..=126 => {
                let b = r.u8()?;
                out.extend(std::iter::repeat_n(b, op as usize + 1));
            }
            127 => {
                let n = r.u16()? as usize;
                let b = r.u8()?;
                out.extend(std::iter::repeat_n(b, n));
            }
            128 => {
                let n = r.u16()? as usize;
                out.extend_from_slice(r.bytes(n)?);
            }
            _ => out.extend_from_slice(r.bytes(256 - op as usize)?),
        }
    }
    if out.len() > target {
        return Err(LayersError::Invalid("RLE de XCF se pasa del mosaico".into()));
    }
    Ok(())
}

// ─── Lectura ────────────────────────────────────────────────────────────────

/// Tipo de muestra según la precisión de la imagen
#[derive(Clone, Copy)]
struct Precision {
    bytes: usize,
    float: bool,
    /// Los valores de color están en luz lineal
    linear: bool,
}

impl Precision {
    fn from_code(version: u32, code: u32) -> Result<Self> {
        let p = |bytes, float, linear| Ok(Self { bytes, float, linear });
        if version < 7 {
            return match code {
                0 => p(1, false, false),
                1 => p(2, false, false),
                2 => p(4, false, true),
                3 => p(2, true, true),
                4 => p(4, true, true),
                _ => Err(LayersError::Unsupported(format!("precisión {code}"))),
            };
        }
        let linear = code % 100 == 0;
        match code / 100 {
            1 => p(1, false, linear),
            2 => p(2, false, linear),
            3 => p(4, false, linear),
            5 => p(2, true, linear),
            6 => p(4, true, linear),
            7 => p(8, true, linear),
            _ => Err(LayersError::Unsupported(format!("precisión {code}"))),
        }
    }

    /// Muestra big-endian a 0..1
    fn unit(&self, b: &[u8]) -> f32 {
        match (self.bytes, self.float) {
            (1, _) => b[0] as f32 / 255.0,
            (2, false) => u16::from_be_bytes([b[0], b[1]]) as f32 / 65535.0,
            (4, false) => u32::from_be_bytes(b.try_into().unwrap()) as f32 / u32::MAX as f32,
            (2, true) => half_to_f32(u16::from_be_bytes([b[0], b[1]])),
            (4, true) => f32::from_be_bytes(b.try_into().unwrap()),
            _ => f64::from_be_bytes(b.try_into().unwrap()) as f32,
        }
    }

    fn color8(&self, b: &[u8]) -> u8 {
        if self.bytes == 1 && !self.linear {
            b[0]
        } else if self.linear {
            linear_to_srgb8(self.unit(b))
        } else {
            unit_to8(self.unit(b))
        }
    }

    fn alpha8(&self, b: &[u8]) -> u8 {
        if self.bytes == 1 { b[0] } else { unit_to8(self.unit(b)) }
    }
}

fn half_to_f32(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = ((h >> 10) & 0x1f) as i32;
    let frac = (h & 0x3ff) as f32;
    match exp {
        0 => sign * frac * 2f32.powi(-24),
        31 => if frac == 0.0 { sign * f32::INFINITY } else { f32::NAN },
        _ => sign * (1.0 + frac / 1024.0) * 2f32.powi(exp - 15),
    }
}

struct Context<'a> {
    data: &'a [u8],
    version: u32,
    precision: Precision,
    compression: u8,
    colormap: Vec<[u8; 3]>,
}

impl Context<'_> {
    fn pointer(&self, r: &mut Reader) -> Result<usize> {
        Ok(if self.version >= 11 { r.u64()? as usize } else { r.u32()? as usize })
    }
}

/// Propiedades que importan de una capa o máscara
#[derive(Default)]
struct Props {
    opacity: Option<f32>,
    visible: Option<bool>,
    offsets: (i32, i32),
    apply_mask: bool,
    group: bool,
    path: Vec<u32>,
    compression: Option<u8>,
    colormap: Option<Vec<[u8; 3]>>,
}

fn read_props(r: &mut Reader) -> Result<Props> {
    let mut props = Props { apply_mask: true, ..Default::default() };
    loop {
        let id = r.u32()?;
        let len = r.u32()? as usize;
        if id == PROP_END {
            return Ok(props);
        }
        if id == PROP_COLORMAP {
            // El largo guardado no es confiable en archivos viejos
            let n = r.u32()? as usize;
            let bytes = r.bytes(3 * n)?;
            props.colormap = Some(bytes.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect());
            continue;
        }
        let payload = r.bytes(len)?;
        let mut p = Reader::new(payload);
        match id {
            PROP_OPACITY if props.opacity.is_none() => props.opacity = Some(p.u32()? as f32 / 255.0),
            PROP_FLOAT_OPACITY => props.opacity = Some(f32::from_bits(p.u32()?)),
            PROP_VISIBLE => props.visible = Some(p.u32()? != 0),
            PROP_OFFSETS => props.offsets = (p.i32()?, p.i32()?),
            PROP_APPLY_MASK => props.apply_mask = p.u32()? != 0,
            PROP_GROUP_ITEM => props.group = true,
            PROP_ITEM_PATH => props.path = (0..len / 4).map(|_| p.u32()).collect::<Result<_>>()?,
            PROP_COMPRESSION => props.compression = Some(p.u8()?),
            _ => {}
        }
    }
}

pub fn read(bytes: &[u8]) -> Result<LayeredImage> {
    let mut r = Reader::new(bytes);
    let magic = r.bytes(14)?;
    let version = match &magic[9..13] {
        b"file" => 0,
        v if v[0] == b'v' => std::str::from_utf8(&v[1..])
            .ok()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| LayersError::Invalid("versión de XCF".into()))?,
        _ => return Err(LayersError::Invalid("firma de XCF".into())),
    };
    let width = r.u32()?;
    let height = r.u32()?;
    let base = r.u32()?;
    let precision = if version >= 4 { Precision::from_code(version, r.u32()?)? } else { Precision::from_code(0, 0)? };
    if base > 2 {
        return Err(LayersError::Unsupported(format!("tipo de imagen {base}")));
    }
    let props = read_props(&mut r)?;
    let ctx = Context {
        data: bytes,
        version,
        precision,
        compression: props.compression.unwrap_or(0),
        colormap: props.colormap.unwrap_or_default(),
    };
    let mut pointers = Vec::new();
    loop {
        let p = ctx.pointer(&mut r)?;
        if p == 0 {
            break;
        }
        pointers.push(p);
    }

    // De arriba hacia abajo; los grupos van antes que sus hijos
    let mut layers = Vec::new();
    let mut stack: Vec<(usize, bool, f32)> = Vec::new();
    for p in pointers {
        let (layer, props) = read_layer(&ctx, p)?;
        let depth = props.path.len().saturating_sub(1);
        stack.retain(|&(d, _, _)| d < depth);
        let visible = props.visible.unwrap_or(true);
        let opacity = props.opacity.unwrap_or(1.0);
        if props.group {
            stack.push((depth, visible, opacity));
            continue;
        }
        let mut layer = layer;
        for &(_, v, o) in &stack {
            layer.visible &= v;
            layer.opacity *= o;
        }
        layers.push(layer);
    }
    layers.reverse();
    Ok(LayeredImage { width, height, layers })
}

fn read_layer(ctx: &Context, at: usize) -> Result<(Layer, Props)> {
    let mut r = Reader::at(ctx.data, at);
    let width = r.u32()?;
    let height = r.u32()?;
    let kind = r.u32()?;
    let name_len = r.u32()? as usize;
    let name = clean_name(r.bytes(name_len)?);
    let props = read_props(&mut r)?;
    let hierarchy = ctx.pointer(&mut r)?;
    let mask = ctx.pointer(&mut r)?;
    let mut layer = Layer {
        name,
        x: props.offsets.0,
        y: props.offsets.1,
        width,
        height,
        pixels: Vec::new(),
        visible: props.visible.unwrap_or(true),
        opacity: props.opacity.unwrap_or(1.0),
    };
    if props.group {
        return Ok((layer, props));
    }
    // Componentes por tipo: RGB, RGBA, gris, gris+alfa, paleta, paleta+alfa
    let (components, has_alpha) = match kind {
        0 => (3, false),
        1 => (4, true),
        2 => (1, false),
        3 => (2, true),
        4 => (1, false),
        5 => (2, true),
        _ => return Err(LayersError::Unsupported(format!("tipo de capa {kind}"))),
    };
    let precision = ctx.precision;
    let raw = read_hierarchy(ctx, hierarchy, width as usize, height as usize, components * precision.bytes)?;
    let bpc = precision.bytes;
    let bpp = components * bpc;
    let mut rgba = vec![255u8; width as usize * height as usize * 4];
    for (px, sample) in rgba.chunks_exact_mut(4).zip(raw.chunks_exact(bpp)) {
        let comp = |c: usize| &sample[c * bpc..(c + 1) * bpc];
        match kind {
            0 | 1 => {
                for c in 0..3 {
                    px[c] = precision.color8(comp(c));
                }
            }
            2 | 3 => px[..3].fill(precision.color8(comp(0))),
            _ => {
                let color = ctx.colormap.get(sample[0] as usize).copied().unwrap_or([0; 3]);
                px[..3].copy_from_slice(&color);
            }
        }
        if has_alpha {
            px[3] = precision.alpha8(comp(components - 1));
        }
    }
    if mask != 0 && props.apply_mask {
        let mut m = Reader::at(ctx.data, mask);
        let (mw, mh) = (m.u32()? as usize, m.u32()? as usize);
        let name_len = m.u32()? as usize;
        m.skip(name_len)?;
        read_props(&mut m)?;
        let mask_hierarchy = ctx.pointer(&mut m)?;
        if (mw, mh) == (width as usize, height as usize) {
            let values = read_hierarchy(ctx, mask_hierarchy, mw, mh, bpc)?;
            for (px, v) in rgba.chunks_exact_mut(4).zip(values.chunks_exact(bpc)) {
                px[3] = ((px[3] as u32 * precision.alpha8(v) as u32 + 127) / 255) as u8;
            }
        }
    }
    layer.pixels = rgba;
    Ok((layer, props))
}

/// Píxeles intercalados (`bpp` bytes por píxel) del primer nivel
fn read_hierarchy(ctx: &Context, at: usize, width: usize, height: usize, bpp: usize) -> Result<Vec<u8>> {
    let mut r = Reader::at(ctx.data, at);
    let (hw, hh, hbpp) = (r.u32()? as usize, r.u32()? as usize, r.u32()? as usize);
    if (hw, hh, hbpp) != (width, height, bpp) {
        return Err(LayersError::Invalid(format!("jerarquía {hw}×{hh}×{hbpp}, se esperaba {width}×{height}×{bpp}")));
    }
    let level = ctx.pointer(&mut r)?;
    let mut r = Reader::at(ctx.data, level);
    let (lw, lh) = (r.u32()? as usize, r.u32()? as usize);
    if (lw, lh) != (width, height) {
        return Err(LayersError::Invalid("nivel de otro tamaño".into()));
    }
    let (tx, ty) = (width.div_ceil(TILE), height.div_ceil(TILE));
    let mut out = vec![0u8; width * height * bpp];
    let mut tile = Vec::with_capacity(TILE * TILE * bpp);
    for t in 0..tx * ty {
        let offset = ctx.pointer(&mut r)?;
        let (x0, y0) = ((t % tx) * TILE, (t / tx) * TILE);
        let (tw, th) = (TILE.min(width - x0), TILE.min(height - y0));
        let n = tw * th;
        tile.clear();
        let mut d = Reader::at(ctx.data, offset);
        match ctx.compression {
            0 => tile.extend_from_slice(d.bytes(n * bpp)?),
            1 => {
                // Un plano por byte de píxel, luego se intercalan
                let mut planes = Vec::with_capacity(n * bpp);
                for _ in 0..bpp {
                    rle_decode(&mut d, n, &mut planes)?;
                }
                tile.resize(n * bpp, 0);
                for b in 0..bpp {
                    for i in 0..n {
                        tile[i * bpp + b] = planes[b * n + i];
                    }
                }
            }
            2 => {
                flate2::read::ZlibDecoder::new(d.remaining())
                    .take((n * bpp) as u64)
                    .read_to_end(&mut tile)
                    .map_err(|e| LayersError::Invalid(format!("zlib: {e}")))?;
                if tile.len() != n * bpp {
                    return Err(LayersError::Invalid("mosaico zlib corto".into()));
                }
            }
            c => return Err(LayersError::Unsupported(format!("compresión {c}"))),
        }
        for y in 0..th {
            let dst = ((y0 + y) * width + x0) * bpp;
            out[dst..dst + tw * bpp].copy_from_slice(&tile[y * tw * bpp..(y + 1) * tw * bpp]);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rle_roundtrip() {
        let long_run = vec![9u8; 40000];
        let noise: Vec<u8> = (0..40000u32).map(|i| (i.wrapping_mul(2654435761) >> 24) as u8).collect();
        let mixed: Vec<u8> = [&[1u8, 2, 2, 3, 3, 3, 3][..], &[0; 200], &[5, 6]].concat();
        for data in [vec![], vec![4], long_run, noise, mixed] {
            let mut enc = Vec::new();
            rle_encode(&data, &mut enc);
            let mut dec = Vec::new();
            rle_decode(&mut Reader::new(&enc), data.len(), &mut dec).unwrap();
            assert_eq!(dec, data);
        }
    }

    #[test]
    fn half_floats() {
        assert_eq!(half_to_f32(0x3c00), 1.0);
        assert_eq!(half_to_f32(0x3800), 0.5);
        assert_eq!(half_to_f32(0), 0.0);
    }
}
