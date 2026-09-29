//! Imágenes con capas para editar texturas en otras aplicaciones.
//!
//! - **XCF**: el formato propio de GIMP.
//! - **PSD**: Photoshop; también lo abren GIMP, Krita y Photopea.
//!
//! Se escriben capas RGBA de 8 bits (sin compresión con pérdida: RLE). Al
//! leer se aceptan las variantes que guardan esos programas (8/16/32 bits,
//! RLE o zlib, gris o paleta, grupos y máscaras) y se convierten a RGBA de 8
//! bits. [`LayeredImage::composite`] junta las capas visibles con el modo
//! normal: otros modos de fusión se toman como normal.

mod psd;
mod raster;
mod xcf;

pub use raster::{draw_lines, fill_triangles};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum LayersError {
    #[error("Archivo con capas inválido: {0}")]
    Invalid(String),
    #[error("No soportado: {0}")]
    Unsupported(String),
    #[error("Imagen: {0}")]
    Image(#[from] image::ImageError),
}

pub type Result<T> = std::result::Result<T, LayersError>;

/// Una capa: su rectángulo dentro del lienzo y sus píxeles.
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    pub name: String,
    /// Esquina superior izquierda en el lienzo (puede quedar afuera)
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    /// RGBA de 8 bits sin premultiplicar, fila por fila desde arriba
    pub pixels: Vec<u8>,
    pub visible: bool,
    /// Opacidad de la capa, de 0 a 1
    pub opacity: f32,
}

impl Layer {
    /// Capa transparente del tamaño dado, visible y opaca
    pub fn new(name: impl Into<String>, width: u32, height: u32) -> Self {
        Self::from_rgba(name, width, height, vec![0; width as usize * height as usize * 4])
    }

    pub fn from_rgba(name: impl Into<String>, width: u32, height: u32, pixels: Vec<u8>) -> Self {
        assert_eq!(pixels.len(), width as usize * height as usize * 4, "RGBA de {width}×{height}");
        Self { name: name.into(), x: 0, y: 0, width, height, pixels, visible: true, opacity: 1.0 }
    }

    fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }
}

/// Imagen con capas, de abajo hacia arriba.
#[derive(Debug, Clone, PartialEq)]
pub struct LayeredImage {
    pub width: u32,
    pub height: u32,
    pub layers: Vec<Layer>,
}

/// Formatos con capas que se pueden escribir.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayeredFormat {
    Psd,
    Xcf,
}

impl LayeredFormat {
    /// Por la extensión de un archivo (sin distinguir mayúsculas)
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "psd" => Some(Self::Psd),
            "xcf" => Some(Self::Xcf),
            _ => None,
        }
    }
}

impl LayeredImage {
    pub fn write(&self, format: LayeredFormat) -> Vec<u8> {
        match format {
            LayeredFormat::Psd => psd::write(self),
            LayeredFormat::Xcf => xcf::write(self),
        }
    }

    /// Lee un PSD, un XCF o, con cualquier otra firma, una imagen plana
    /// (PNG, JPEG, WebP) como una sola capa.
    pub fn read(bytes: &[u8]) -> Result<Self> {
        if bytes.starts_with(b"8BPS") {
            return psd::read(bytes);
        }
        if bytes.starts_with(b"gimp xcf ") {
            return xcf::read(bytes);
        }
        let image = image::load_from_memory(bytes)?.to_rgba8();
        let (width, height) = image.dimensions();
        Ok(Self { width, height, layers: vec![Layer::from_rgba("Fondo", width, height, image.into_raw())] })
    }

    /// Junta las capas visibles que acepta `include` (modo normal, en el
    /// espacio de color del archivo, como GIMP y Photoshop). RGBA sin
    /// premultiplicar del tamaño del lienzo.
    pub fn composite(&self, include: impl Fn(&Layer) -> bool) -> Vec<u8> {
        let (w, h) = (self.width as usize, self.height as usize);
        // Acumulado premultiplicado en punto flotante
        let mut acc = vec![[0f32; 4]; w * h];
        for layer in self.layers.iter().filter(|l| l.visible && l.opacity > 0.0 && !l.is_empty() && include(l)) {
            let x0 = layer.x.max(0) as usize;
            let y0 = layer.y.max(0) as usize;
            let x1 = (layer.x + layer.width as i32).clamp(0, w as i32) as usize;
            let y1 = (layer.y + layer.height as i32).clamp(0, h as i32) as usize;
            for y in y0..y1 {
                let ly = (y as i32 - layer.y) as usize;
                for x in x0..x1 {
                    let lx = (x as i32 - layer.x) as usize;
                    let p = &layer.pixels[(ly * layer.width as usize + lx) * 4..][..4];
                    let a = p[3] as f32 / 255.0 * layer.opacity;
                    if a <= 0.0 {
                        continue;
                    }
                    let dst = &mut acc[y * w + x];
                    for c in 0..3 {
                        dst[c] = p[c] as f32 / 255.0 * a + dst[c] * (1.0 - a);
                    }
                    dst[3] = a + dst[3] * (1.0 - a);
                }
            }
        }
        let mut out = vec![0u8; w * h * 4];
        for (px, a) in out.chunks_exact_mut(4).zip(&acc) {
            if a[3] > 0.0 {
                for c in 0..3 {
                    px[c] = (a[c] / a[3] * 255.0).round().clamp(0.0, 255.0) as u8;
                }
                px[3] = (a[3] * 255.0).round() as u8;
            }
        }
        out
    }
}

/// Lector big-endian sobre un arreglo de bytes, con errores en vez de pánico.
pub(crate) struct Reader<'a> {
    data: &'a [u8],
    pub pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn at(data: &'a [u8], pos: usize) -> Self {
        Self { data, pos }
    }

    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.data.len());
        let end = end.ok_or_else(|| LayersError::Invalid(format!("se corta en el byte {}", self.pos)))?;
        let slice = &self.data[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    pub fn skip(&mut self, n: usize) -> Result<()> {
        self.bytes(n).map(|_| ())
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }

    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.bytes(2)?.try_into().unwrap()))
    }

    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.bytes(4)?.try_into().unwrap()))
    }

    pub fn i32(&mut self) -> Result<i32> {
        Ok(self.u32()? as i32)
    }

    pub fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(self.bytes(8)?.try_into().unwrap()))
    }

    pub fn remaining(&self) -> &'a [u8] {
        &self.data[self.pos.min(self.data.len())..]
    }
}

/// Valor lineal (0..1) a sRGB de 8 bits
pub(crate) fn linear_to_srgb8(v: f32) -> u8 {
    let v = v.clamp(0.0, 1.0);
    let s = if v <= 0.003_130_8 { 12.92 * v } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0).round() as u8
}

/// Valor de 0..1 a 8 bits
pub(crate) fn unit_to8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Nombre sin caracteres de control (los archivos pueden traer basura)
pub(crate) fn clean_name(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim_end_matches('\0').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composite_blends_visible_layers_over() {
        let mut bottom = Layer::from_rgba("fondo", 2, 1, vec![255, 0, 0, 255, 0, 0, 255, 255]);
        bottom.opacity = 1.0;
        // Mitad de opacidad en blanco sobre el primer píxel, desplazada
        let mut top = Layer::from_rgba("arriba", 1, 1, vec![255, 255, 255, 255]);
        top.opacity = 0.5;
        let mut hidden = Layer::from_rgba("oculta", 2, 1, vec![0, 255, 0, 255, 0, 255, 0, 255]);
        hidden.visible = false;
        let image = LayeredImage { width: 2, height: 1, layers: vec![bottom, top, hidden] };
        let out = image.composite(|_| true);
        assert_eq!(&out[..4], &[255, 128, 128, 255]);
        assert_eq!(&out[4..], &[0, 0, 255, 255]);
        // Filtrada por nombre
        let only_top = image.composite(|l| l.name == "arriba");
        assert_eq!(&only_top[..4], &[255, 255, 255, 128]);
        assert_eq!(only_top[7], 0);
    }

    /// Tres capas con transparencia, desplazada, oculta y a media opacidad
    fn sample() -> LayeredImage {
        let (w, h) = (70, 67); // más de un mosaico de 64 en XCF
        let texture: Vec<u8> = (0..w * h).flat_map(|i| [(i % 251) as u8, (i / 7 % 256) as u8, 40, 255]).collect();
        let mut wire = Layer::new("Malla UV", 20, 10);
        wire.x = 5;
        wire.y = -3;
        wire.opacity = 0.6;
        for (i, p) in wire.pixels.chunks_exact_mut(4).enumerate() {
            if i % 3 == 0 {
                p.copy_from_slice(&[80, 250, 123, 200]);
            }
        }
        let mut islands = Layer::from_rgba("Islas ñandú", w, h, vec![10; (w * h * 4) as usize]);
        islands.visible = false;
        LayeredImage { width: w, height: h, layers: vec![Layer::from_rgba("Textura", w, h, texture), islands, wire] }
    }

    fn assert_same(a: &LayeredImage, b: &LayeredImage) {
        assert_eq!((a.width, a.height, a.layers.len()), (b.width, b.height, b.layers.len()));
        for (x, y) in a.layers.iter().zip(&b.layers) {
            assert_eq!(x.name, y.name);
            assert_eq!((x.x, x.y, x.width, x.height, x.visible), (y.x, y.y, y.width, y.height, y.visible), "{}", x.name);
            assert!((x.opacity - y.opacity).abs() < 0.01, "{}", x.name);
            assert!(x.pixels == y.pixels, "píxeles de {}", x.name);
        }
    }

    #[test]
    fn psd_roundtrip() {
        let image = sample();
        let bytes = image.write(LayeredFormat::Psd);
        assert_same(&image, &LayeredImage::read(&bytes).unwrap());
    }

    #[test]
    fn xcf_roundtrip() {
        let image = sample();
        let bytes = image.write(LayeredFormat::Xcf);
        assert_same(&image, &LayeredImage::read(&bytes).unwrap());
    }

    #[test]
    fn plain_images_read_as_one_layer() {
        let mut png = Vec::new();
        let img = image::RgbaImage::from_raw(2, 2, (0..16).collect()).unwrap();
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let read = LayeredImage::read(&png).unwrap();
        assert_eq!((read.width, read.height, read.layers.len()), (2, 2, 1));
        assert_eq!(read.layers[0].pixels, (0..16).collect::<Vec<u8>>());
    }
}
