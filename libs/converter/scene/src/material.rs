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
#[cfg_attr(feature = "serde", serde(default))]
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

    /// Vidrio y líquidos: cuánta luz atraviesa el material (KHR_materials_transmission)
    pub transmission_factor: f32,
    /// Índice de refracción (KHR_materials_ior): agua 1.33, vidrio 1.5, diamante 2.4
    pub ior: f32,
    /// Capa de barniz sobre el material, para lo húmedo o lacado (KHR_materials_clearcoat)
    pub clearcoat_factor: f32,
    pub clearcoat_roughness_factor: f32,
    /// Repetición de las texturas (escala UV, KHR_texture_transform)
    pub uv_scale: [f32; 2],
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
            transmission_factor: 0.0,
            ior: 1.5,
            clearcoat_factor: 0.0,
            clearcoat_roughness_factor: 0.0,
            uv_scale: [1.0, 1.0],
        }
    }
}
