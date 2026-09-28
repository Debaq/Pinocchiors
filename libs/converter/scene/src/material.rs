use crate::TextureRef;

/// Modo de alpha blending.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum AlphaMode {
    Opaque,
    Mask(f32),
    Blend,
}

/// Material PBR metallic-roughness (compatible glTF y UsdPreviewSurface).
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Material {
    pub name: String,

    // Base color
    pub base_color_factor: [f32; 4],
    pub base_color_texture: Option<TextureRef>,

    // Metallic-roughness
    pub metallic_factor: f32,
    pub roughness_factor: f32,
    pub metallic_roughness_texture: Option<TextureRef>,

    // Normal map
    pub normal_texture: Option<TextureRef>,
    pub normal_scale: f32,

    // Occlusion
    pub occlusion_texture: Option<TextureRef>,
    pub occlusion_strength: f32,

    // Emissive
    pub emissive_factor: [f32; 3],
    pub emissive_texture: Option<TextureRef>,

    // Alpha
    pub alpha_mode: AlphaMode,
    pub double_sided: bool,

    // Unlit (KHR_materials_unlit)
    pub unlit: bool,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            name: String::new(),
            base_color_factor: [1.0, 1.0, 1.0, 1.0],
            base_color_texture: None,
            metallic_factor: 1.0,
            roughness_factor: 1.0,
            metallic_roughness_texture: None,
            normal_texture: None,
            normal_scale: 1.0,
            occlusion_texture: None,
            occlusion_strength: 1.0,
            emissive_factor: [0.0, 0.0, 0.0],
            emissive_texture: None,
            alpha_mode: AlphaMode::Opaque,
            double_sided: false,
            unlit: false,
        }
    }
}
