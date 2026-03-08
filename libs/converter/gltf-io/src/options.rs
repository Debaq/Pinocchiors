/// Opciones de exportación GLB.
///
/// Todos los campos opcionales/desactivados por defecto para mantener
/// el comportamiento existente cuando se usa `Default::default()`.
#[derive(Debug, Clone)]
pub struct GlbExportOptions {
    /// Calidad JPEG (1-100). `None` = mantener formato original.
    /// Si se define, las texturas opacas se recomprimen a JPEG con esta calidad.
    pub texture_quality: Option<u8>,
    /// Tamaño máximo de textura (ancho o alto). `None` = sin límite.
    pub max_texture_size: Option<u32>,
    /// Deduplicar vértices, optimizar índices U32→U16, strip degenerados.
    pub optimize_geometry: bool,
    /// Generar normales si faltan en una primitiva.
    pub generate_normals: bool,
    /// Bakear transforms de nodos en la geometría (no aplica si hay skeletons).
    pub flatten_transforms: bool,
    /// Factor de escala para geometría. `None` = sin escala.
    pub scale_factor: Option<f64>,
    /// Exportar animaciones. Default: true.
    pub export_animations: bool,
    /// Eliminar materiales y texturas no referenciados.
    pub strip_unused: bool,
}

impl Default for GlbExportOptions {
    fn default() -> Self {
        Self {
            texture_quality: None,
            max_texture_size: None,
            optimize_geometry: false,
            generate_normals: false,
            flatten_transforms: false,
            scale_factor: None,
            export_animations: true,
            strip_unused: false,
        }
    }
}

impl GlbExportOptions {
    /// Retorna true si alguna opción de preprocesamiento está activa.
    pub(crate) fn needs_preprocessing(&self) -> bool {
        self.scale_factor.is_some()
            || self.flatten_transforms
            || self.generate_normals
            || self.optimize_geometry
            || self.strip_unused
            || !self.export_animations
    }

    /// Retorna true si alguna opción de procesamiento de texturas está activa.
    pub(crate) fn needs_texture_processing(&self) -> bool {
        self.texture_quality.is_some() || self.max_texture_size.is_some()
    }
}
