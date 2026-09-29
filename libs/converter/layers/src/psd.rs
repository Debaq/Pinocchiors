//! PSD (Photoshop, versión 1). Referencia: "Adobe Photoshop File Formats
//! Specification". Se escribe RGB de 8 bits con RLE (PackBits); se lee RGB o
//! gris de 8, 16 o 32 bits, sin compresión, RLE o zip, con grupos
//! (`lsct`), nombres Unicode (`luni`) y máscaras de capa.

use std::io::Read;

use crate::{LayeredImage, Layer, LayersError, Reader, Result, clean_name, linear_to_srgb8};

// ─── Escritura ──────────────────────────────────────────────────────────────

pub fn write(image: &LayeredImage) -> Vec<u8> {
    let (w, h) = (image.width, image.height);
    let mut out = Vec::new();
    out.extend_from_slice(b"8BPS");
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&[0; 6]);
    out.extend_from_slice(&4u16.to_be_bytes()); // RGBA
    out.extend_from_slice(&h.to_be_bytes());
    out.extend_from_slice(&w.to_be_bytes());
    out.extend_from_slice(&8u16.to_be_bytes());
    out.extend_from_slice(&3u16.to_be_bytes()); // RGB
    out.extend_from_slice(&0u32.to_be_bytes()); // datos de modo de color
    out.extend_from_slice(&0u32.to_be_bytes()); // recursos de imagen

    // Información de capas: registros de abajo hacia arriba, luego sus canales
    let mut info = Vec::new();
    // Negativo: el primer alfa guarda la transparencia de la imagen combinada
    info.extend_from_slice(&(-(image.layers.len() as i16)).to_be_bytes());
    let channels: Vec<Vec<Vec<u8>>> = image.layers.iter().map(layer_channels).collect();
    for (layer, data) in image.layers.iter().zip(&channels) {
        let top = layer.y;
        let left = layer.x;
        for v in [top, left, top + layer.height as i32, left + layer.width as i32] {
            info.extend_from_slice(&v.to_be_bytes());
        }
        info.extend_from_slice(&4u16.to_be_bytes());
        for (id, channel) in [-1i16, 0, 1, 2].iter().zip(data) {
            info.extend_from_slice(&id.to_be_bytes());
            info.extend_from_slice(&(channel.len() as u32).to_be_bytes());
        }
        info.extend_from_slice(b"8BIMnorm");
        info.push((layer.opacity.clamp(0.0, 1.0) * 255.0).round() as u8);
        info.push(0); // recorte
        info.push(if layer.visible { 0 } else { 0x02 }); // bit 1: oculta
        info.push(0);
        let extra = layer_extra(&layer.name);
        info.extend_from_slice(&(extra.len() as u32).to_be_bytes());
        info.extend_from_slice(&extra);
    }
    for data in &channels {
        for channel in data {
            info.extend_from_slice(channel);
        }
    }
    if info.len() % 2 == 1 {
        info.push(0);
    }
    let mut section = Vec::new();
    section.extend_from_slice(&(info.len() as u32).to_be_bytes());
    section.extend_from_slice(&info);
    section.extend_from_slice(&0u32.to_be_bytes()); // máscara global
    out.extend_from_slice(&(section.len() as u32).to_be_bytes());
    out.extend_from_slice(&section);

    // Imagen combinada: la ven los programas que no leen capas
    let merged = image.composite(|_| true);
    out.extend_from_slice(&1u16.to_be_bytes());
    let mut counts = Vec::new();
    let mut rows = Vec::new();
    for c in [0, 1, 2, 3] {
        for y in 0..h as usize {
            let row: Vec<u8> = (0..w as usize).map(|x| merged[(y * w as usize + x) * 4 + c]).collect();
            let start = rows.len();
            packbits(&row, &mut rows);
            counts.extend_from_slice(&((rows.len() - start) as u16).to_be_bytes());
        }
    }
    out.extend_from_slice(&counts);
    out.extend_from_slice(&rows);
    out
}

/// Datos de los canales A, R, G, B de una capa (compresión + filas)
fn layer_channels(layer: &Layer) -> Vec<Vec<u8>> {
    let (w, h) = (layer.width as usize, layer.height as usize);
    [3, 0, 1, 2]
        .iter()
        .map(|&c| {
            let mut out = 1u16.to_be_bytes().to_vec();
            let mut counts = Vec::with_capacity(h * 2);
            let mut rows = Vec::new();
            for y in 0..h {
                let row: Vec<u8> = (0..w).map(|x| layer.pixels[(y * w + x) * 4 + c]).collect();
                let start = rows.len();
                packbits(&row, &mut rows);
                counts.extend_from_slice(&((rows.len() - start) as u16).to_be_bytes());
            }
            out.extend_from_slice(&counts);
            out.extend_from_slice(&rows);
            out
        })
        .collect()
}

/// Máscara (vacía), rangos de fusión (vacíos), nombre Pascal y `luni`
fn layer_extra(name: &str) -> Vec<u8> {
    let mut extra = Vec::new();
    extra.extend_from_slice(&0u32.to_be_bytes());
    extra.extend_from_slice(&0u32.to_be_bytes());
    let ascii: Vec<u8> = name.chars().take(255).map(|c| if c.is_ascii() && !c.is_control() { c as u8 } else { b'_' }).collect();
    extra.push(ascii.len() as u8);
    extra.extend_from_slice(&ascii);
    while extra.len() % 4 != 0 {
        extra.push(0);
    }
    // Nombre Unicode (UTF-16BE)
    let utf16: Vec<u16> = name.encode_utf16().collect();
    let mut data = (utf16.len() as u32).to_be_bytes().to_vec();
    for c in utf16 {
        data.extend_from_slice(&c.to_be_bytes());
    }
    while data.len() % 4 != 0 {
        data.push(0);
    }
    extra.extend_from_slice(b"8BIMluni");
    extra.extend_from_slice(&(data.len() as u32).to_be_bytes());
    extra.extend_from_slice(&data);
    extra
}

/// PackBits: corridas de bytes iguales (2 a 128) o literales (1 a 128)
fn packbits(row: &[u8], out: &mut Vec<u8>) {
    let n = row.len();
    let mut i = 0;
    while i < n {
        let mut run = 1;
        while i + run < n && run < 128 && row[i + run] == row[i] {
            run += 1;
        }
        if run >= 2 {
            out.push((1 - run as i32) as i8 as u8);
            out.push(row[i]);
            i += run;
            continue;
        }
        let start = i;
        while i < n && i - start < 128 {
            if i + 1 < n && row[i] == row[i + 1] {
                break;
            }
            i += 1;
        }
        out.push((i - start - 1) as u8);
        out.extend_from_slice(&row[start..i]);
    }
}

fn unpackbits(r: &mut Reader, out: &mut Vec<u8>, expected: usize) -> Result<()> {
    let target = out.len() + expected;
    while out.len() < target {
        let n = r.u8()? as i8;
        if n >= 0 {
            out.extend_from_slice(r.bytes(n as usize + 1)?);
        } else if n != -128 {
            let b = r.u8()?;
            out.extend(std::iter::repeat_n(b, (1 - n as isize) as usize));
        }
    }
    out.truncate(target);
    Ok(())
}

// ─── Lectura ────────────────────────────────────────────────────────────────

struct Header {
    width: u32,
    height: u32,
    channels: u16,
    depth: u16,
    /// 1 = gris, 3 = RGB
    mode: u16,
}

pub fn read(bytes: &[u8]) -> Result<LayeredImage> {
    let mut r = Reader::new(bytes);
    r.skip(4)?;
    match r.u16()? {
        1 => {}
        2 => return Err(LayersError::Unsupported("PSB (documentos grandes)".into())),
        v => return Err(LayersError::Invalid(format!("versión {v}"))),
    }
    r.skip(6)?;
    let channels = r.u16()?;
    let height = r.u32()?;
    let width = r.u32()?;
    let depth = r.u16()?;
    let mode = r.u16()?;
    if !matches!(depth, 8 | 16 | 32) {
        return Err(LayersError::Unsupported(format!("{depth} bits por canal")));
    }
    if !matches!(mode, 1 | 3) {
        return Err(LayersError::Unsupported("modo de color distinto de RGB o gris".into()));
    }
    let header = Header { width, height, channels, depth, mode };
    let len = r.u32()? as usize;
    r.skip(len)?; // modo de color
    let len = r.u32()? as usize;
    r.skip(len)?; // recursos

    let section_len = r.u32()? as usize;
    let section_end = r.pos + section_len;
    let mut layers = Vec::new();
    if section_len > 0 {
        let info_len = r.u32()? as usize;
        let info_end = r.pos + info_len;
        if info_len > 0 {
            layers = read_layer_info(&mut Reader::at(bytes, r.pos), &header)?;
        } else {
            // 16 y 32 bits: la información va en un bloque adicional
            let mut g = Reader::at(bytes, info_end);
            let mask_len = g.u32()? as usize;
            g.skip(mask_len)?;
            while g.pos + 12 <= section_end {
                let sig = g.bytes(4)?;
                if sig != b"8BIM" && sig != b"8B64" {
                    break;
                }
                let key = g.bytes(4)?;
                let len = g.u32()? as usize;
                let start = g.pos;
                if key == b"Lr16" || key == b"Lr32" || key == b"Layr" {
                    layers = read_layer_info(&mut Reader::at(bytes, start), &header)?;
                    break;
                }
                g.pos = start + len.next_multiple_of(4);
            }
        }
    }
    r.pos = section_end;

    if layers.is_empty() {
        let pixels = read_merged(&mut r, &header)?;
        layers.push(Layer::from_rgba("Fondo", width, height, pixels));
    }
    Ok(LayeredImage { width, height, layers })
}

/// Canal de un registro: id y largo de sus datos
struct ChannelInfo {
    id: i16,
    len: usize,
}

struct Record {
    layer: Layer,
    channels: Vec<ChannelInfo>,
    /// Rectángulo de la máscara de usuario y su color fuera de él
    mask: Option<([i32; 4], u8)>,
    /// Tipo de sección de `lsct`: 1/2 abre carpeta, 3 cierra
    section: u32,
}

fn read_layer_info(r: &mut Reader, header: &Header) -> Result<Vec<Layer>> {
    let count = (r.u16()? as i16).unsigned_abs() as usize;
    let mut records = Vec::with_capacity(count);
    for _ in 0..count {
        let top = r.i32()?;
        let left = r.i32()?;
        let bottom = r.i32()?;
        let right = r.i32()?;
        let n = r.u16()? as usize;
        let mut channels = Vec::with_capacity(n);
        for _ in 0..n {
            channels.push(ChannelInfo { id: r.u16()? as i16, len: r.u32()? as usize });
        }
        r.skip(8)?; // 8BIM + modo de fusión
        let opacity = r.u8()? as f32 / 255.0;
        r.skip(1)?;
        let flags = r.u8()?;
        r.skip(1)?;
        let extra_len = r.u32()? as usize;
        let extra_end = r.pos + extra_len;
        let mask_len = r.u32()? as usize;
        let mask = if mask_len >= 18 {
            let rect = [r.i32()?, r.i32()?, r.i32()?, r.i32()?];
            let default = r.u8()?;
            let mask_flags = r.u8()?;
            r.skip(mask_len - 18)?;
            // Bit 1: máscara desactivada
            (mask_flags & 0x02 == 0).then_some((rect, default))
        } else {
            r.skip(mask_len)?;
            None
        };
        let ranges = r.u32()? as usize;
        r.skip(ranges)?;
        let name_len = r.u8()? as usize;
        let mut name = clean_name(r.bytes(name_len)?);
        r.skip((4 - (1 + name_len) % 4) % 4)?;
        let mut section = 0;
        while r.pos + 12 <= extra_end {
            r.skip(4)?;
            let key = r.bytes(4)?;
            let len = r.u32()? as usize;
            let start = r.pos;
            match key {
                b"luni" => {
                    let chars = r.u32()? as usize;
                    let units: Vec<u16> = (0..chars).map(|_| r.u16()).collect::<Result<_>>()?;
                    name = String::from_utf16_lossy(&units).trim_end_matches('\0').to_string();
                }
                b"lsct" | b"lsdk" => section = r.u32()?,
                _ => {}
            }
            r.pos = start + len;
            // Algunos escriben el largo sin redondear: alinear a 2
            if r.pos % 2 == 1 && r.pos < extra_end {
                r.pos += 1;
            }
        }
        r.pos = extra_end;
        let width = (right - left).max(0) as u32;
        let height = (bottom - top).max(0) as u32;
        let layer = Layer {
            name,
            x: left,
            y: top,
            width,
            height,
            pixels: Vec::new(),
            visible: flags & 0x02 == 0,
            opacity,
        };
        records.push(Record { layer, channels, mask, section });
    }

    // Datos de los canales, en el mismo orden
    for record in &mut records {
        let (w, h) = (record.layer.width as usize, record.layer.height as usize);
        let mut rgba = vec![0u8; w * h * 4];
        let mut has_alpha = false;
        let mut mask_pixels = None;
        for channel in &record.channels {
            let end = r.pos + channel.len;
            let (cw, ch) = match (channel.id, record.mask) {
                (-2, Some((rect, _))) => ((rect[3] - rect[1]).max(0) as usize, (rect[2] - rect[0]).max(0) as usize),
                _ => (w, h),
            };
            if channel.len >= 2 && cw * ch > 0 {
                let data = read_channel(&mut Reader::at(r.data, r.pos), cw, ch, header.depth, channel.len)?;
                match channel.id {
                    -1 => {
                        has_alpha = true;
                        for (px, v) in rgba.chunks_exact_mut(4).zip(&data) {
                            px[3] = *v;
                        }
                    }
                    0..=2 => {
                        let c = channel.id as usize;
                        for (px, v) in rgba.chunks_exact_mut(4).zip(&data) {
                            if header.mode == 1 {
                                px[..3].fill(*v);
                            } else {
                                px[c] = *v;
                            }
                        }
                    }
                    -2 => mask_pixels = Some(data),
                    _ => {}
                }
            }
            r.pos = end;
        }
        if !has_alpha {
            rgba.chunks_exact_mut(4).for_each(|px| px[3] = 255);
        }
        // Máscara de usuario: multiplica el alfa (afuera, su color por defecto)
        if let (Some((rect, default)), Some(mask)) = (record.mask, mask_pixels) {
            let mw = (rect[3] - rect[1]).max(0);
            for y in 0..h as i32 {
                for x in 0..w as i32 {
                    let (mx, my) = (x + record.layer.x - rect[1], y + record.layer.y - rect[0]);
                    let m = if mx >= 0 && my >= 0 && mx < mw && my < rect[2] - rect[0] {
                        mask[(my * mw + mx) as usize]
                    } else {
                        default
                    };
                    let a = &mut rgba[((y as usize * w) + x as usize) * 4 + 3];
                    *a = ((*a as u32 * m as u32 + 127) / 255) as u8;
                }
            }
        }
        record.layer.pixels = rgba;
    }

    // Grupos: de arriba hacia abajo, una carpeta (1/2) abre y el divisor (3)
    // la cierra. Una capa se ve si ella y todas sus carpetas se ven
    let mut layers = Vec::new();
    let mut stack: Vec<(bool, f32)> = Vec::new();
    for record in records.into_iter().rev() {
        match record.section {
            1 | 2 => stack.push((record.layer.visible, record.layer.opacity)),
            3 => {
                stack.pop();
            }
            _ => {
                let mut layer = record.layer;
                for &(visible, opacity) in &stack {
                    layer.visible &= visible;
                    layer.opacity *= opacity;
                }
                layers.push(layer);
            }
        }
    }
    layers.reverse();
    Ok(layers)
}

/// Un canal de `w × h` a 8 bits
fn read_channel(r: &mut Reader, w: usize, h: usize, depth: u16, len: usize) -> Result<Vec<u8>> {
    let compression = r.u16()?;
    let bpc = depth as usize / 8;
    let raw = match compression {
        0 => r.bytes(w * h * bpc)?.to_vec(),
        1 => {
            let counts: Vec<usize> = (0..h).map(|_| r.u16().map(usize::from)).collect::<Result<_>>()?;
            let mut out = Vec::with_capacity(w * h * bpc);
            for count in counts {
                let mut row = Reader::new(r.bytes(count)?);
                unpackbits(&mut row, &mut out, w * bpc)?;
            }
            out
        }
        2 | 3 => {
            let mut out = Vec::with_capacity(w * h * bpc);
            flate2::read::ZlibDecoder::new(r.bytes(len - 2)?)
                .read_to_end(&mut out)
                .map_err(|e| LayersError::Invalid(format!("zip: {e}")))?;
            if out.len() < w * h * bpc {
                return Err(LayersError::Invalid("canal zip corto".into()));
            }
            if compression == 3 {
                unpredict(&mut out, w, h, bpc)?;
            }
            out
        }
        c => return Err(LayersError::Unsupported(format!("compresión {c}"))),
    };
    Ok(to8(&raw, w * h, depth))
}

/// Deshace la predicción por diferencias de "zip con predicción"
fn unpredict(data: &mut [u8], w: usize, h: usize, bpc: usize) -> Result<()> {
    match bpc {
        1 => {
            for row in data.chunks_exact_mut(w).take(h) {
                for x in 1..w {
                    row[x] = row[x].wrapping_add(row[x - 1]);
                }
            }
        }
        2 => {
            for row in data.chunks_exact_mut(w * 2).take(h) {
                for x in 1..w {
                    let prev = u16::from_be_bytes([row[2 * x - 2], row[2 * x - 1]]);
                    let cur = u16::from_be_bytes([row[2 * x], row[2 * x + 1]]).wrapping_add(prev);
                    row[2 * x..2 * x + 2].copy_from_slice(&cur.to_be_bytes());
                }
            }
        }
        _ => {
            // 32 bits: diferencias por byte y luego los bytes por plano
            for row in data.chunks_exact_mut(w * 4).take(h) {
                for x in 1..w * 4 {
                    row[x] = row[x].wrapping_add(row[x - 1]);
                }
                let planes = row.to_vec();
                for x in 0..w {
                    for b in 0..4 {
                        row[x * 4 + b] = planes[b * w + x];
                    }
                }
            }
        }
    }
    Ok(())
}

/// Muestras big-endian de `depth` bits a 8 bits (32 bits es flotante lineal)
fn to8(raw: &[u8], n: usize, depth: u16) -> Vec<u8> {
    match depth {
        8 => raw[..n].to_vec(),
        16 => raw.chunks_exact(2).take(n).map(|c| ((u16::from_be_bytes([c[0], c[1]]) as u32 + 128) / 257) as u8).collect(),
        _ => raw.chunks_exact(4).take(n).map(|c| linear_to_srgb8(f32::from_be_bytes(c.try_into().unwrap()))).collect(),
    }
}

/// Imagen combinada (sin capas): canales planos, RLE con todas las cuentas primero
fn read_merged(r: &mut Reader, header: &Header) -> Result<Vec<u8>> {
    let (w, h) = (header.width as usize, header.height as usize);
    let n = header.channels as usize;
    let bpc = header.depth as usize / 8;
    let compression = r.u16()?;
    let mut planes = Vec::with_capacity(n);
    match compression {
        0 => {
            for _ in 0..n {
                planes.push(to8(r.bytes(w * h * bpc)?, w * h, header.depth));
            }
        }
        1 => {
            let counts: Vec<usize> = (0..n * h).map(|_| r.u16().map(usize::from)).collect::<Result<_>>()?;
            for c in 0..n {
                let mut plane = Vec::with_capacity(w * h * bpc);
                for count in &counts[c * h..(c + 1) * h] {
                    unpackbits(&mut Reader::new(r.bytes(*count)?), &mut plane, w * bpc)?;
                }
                planes.push(to8(&plane, w * h, header.depth));
            }
        }
        c => return Err(LayersError::Unsupported(format!("compresión {c} en la imagen combinada"))),
    }
    let gray = header.mode == 1;
    let color = if gray { 1 } else { 3 };
    let mut out = vec![255u8; w * h * 4];
    for (i, px) in out.chunks_exact_mut(4).enumerate() {
        for c in 0..3 {
            px[c] = planes[if gray { 0 } else { c }][i];
        }
        if let Some(alpha) = planes.get(color) {
            px[3] = alpha[i];
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packbits_roundtrip() {
        let rows: [&[u8]; 5] = [&[], &[7], &[1, 1, 1, 1, 2, 3, 4, 4], &[0; 300], &[1, 2, 3, 4, 5, 6, 7, 8, 9]];
        for row in rows {
            let mut packed = Vec::new();
            packbits(row, &mut packed);
            let mut out = Vec::new();
            unpackbits(&mut Reader::new(&packed), &mut out, row.len()).unwrap();
            assert_eq!(out, row);
        }
        let long: Vec<u8> = (0..1000u32).map(|i| (i * 7 % 13) as u8).collect();
        let mut packed = Vec::new();
        packbits(&long, &mut packed);
        let mut out = Vec::new();
        unpackbits(&mut Reader::new(&packed), &mut out, long.len()).unwrap();
        assert_eq!(out, long);
    }
}
