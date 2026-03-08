/// Formato de imagen de la textura.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureFormat {
    Png,
    Jpeg,
    WebP,
}

/// Datos de una textura embebida.
#[derive(Debug, Clone)]
pub struct Texture {
    pub name: String,
    pub data: Vec<u8>,
    pub format: TextureFormat,
    pub width: u32,
    pub height: u32,
}

/// Referencia a una textura en la escena con canal UV.
#[derive(Debug, Clone)]
pub struct TextureRef {
    pub texture_index: usize,
    pub tex_coord_set: u32,
}
