/// Opciones unificadas de conversión.
#[derive(Debug, Clone)]
pub struct ConvertOptions {
    /// Factor de escala. `None` = sin escala.
    pub scale_factor: Option<f64>,
    /// Tamaño máximo de textura. `None` = sin límite.
    pub max_texture_size: Option<u32>,
    /// Separar textura ORM en canales individuales.
    pub split_orm_channels: bool,
    /// Compatibilidad AR Quick Look (Apple).
    pub arkit_compatible: bool,
    /// Frames por segundo para animaciones USD. Default: 24.0.
    pub fps: f64,
    /// Exportar animaciones. Default: true.
    pub export_animations: bool,
    /// Tolerancia para eliminar keyframes redundantes.
    pub keyframe_tolerance: f32,
    /// Calidad JPEG para texturas (1-100). `None` = mantener original.
    pub texture_quality: Option<u8>,
    /// Deduplicar vértices, optimizar índices, strip degenerados.
    pub optimize_geometry: bool,
    /// Generar normales si faltan.
    pub generate_normals: bool,
    /// Bakear transforms en geometría (no aplica con skeletons).
    pub flatten_transforms: bool,
    /// Eliminar materiales/texturas no referenciados.
    pub strip_unused: bool,
    /// Reducir triángulos (solo glTF/GLB). `None` = geometría intacta.
    pub simplify: Option<converter_gltf_io::Simplification>,
    /// Compresión Draco (solo glTF/GLB). `None` = sin comprimir.
    pub draco: Option<converter_gltf_io::DracoOptions>,
}

impl Default for ConvertOptions {
    fn default() -> Self {
        Self {
            scale_factor: None,
            max_texture_size: None,
            split_orm_channels: false,
            arkit_compatible: false,
            fps: 24.0,
            export_animations: true,
            keyframe_tolerance: 1e-4,
            texture_quality: None,
            optimize_geometry: false,
            generate_normals: false,
            flatten_transforms: false,
            strip_unused: false,
            simplify: None,
            draco: None,
        }
    }
}

impl ConvertOptions {
    /// Convierte a opciones USDA.
    pub(crate) fn to_usda_options(&self) -> converter_usda::UsdaExportOptions {
        converter_usda::UsdaExportOptions {
            scale_factor: self.scale_factor,
            max_texture_size: self.max_texture_size,
            split_orm_channels: self.split_orm_channels,
            arkit_compatible: self.arkit_compatible,
            fps: self.fps,
            export_animations: self.export_animations,
            keyframe_tolerance: self.keyframe_tolerance,
        }
    }

    /// Convierte a opciones de exportación GLB.
    pub(crate) fn to_glb_options(&self) -> converter_gltf_io::GlbExportOptions {
        converter_gltf_io::GlbExportOptions {
            texture_quality: self.texture_quality,
            max_texture_size: self.max_texture_size,
            optimize_geometry: self.optimize_geometry,
            generate_normals: self.generate_normals,
            flatten_transforms: self.flatten_transforms,
            scale_factor: self.scale_factor,
            export_animations: self.export_animations,
            strip_unused: self.strip_unused,
            simplify: self.simplify,
            draco: self.draco,
        }
    }
}
