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
    /// Reducir triángulos antes de escribir. `None` = geometría intacta.
    pub simplify: Option<Simplification>,
    /// Comprimir la geometría con `KHR_draco_mesh_compression`. `None` = sin comprimir.
    pub draco: Option<DracoOptions>,
}

/// Reducción de triángulos (meshoptimizer). Respeta costuras de UV y bordes
/// de material: solo colapsa aristas dentro de regiones continuas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Simplification {
    /// Fracción de triángulos a conservar (0-1].
    pub ratio: f32,
    /// Desviación máxima permitida, relativa al tamaño de la malla (0.01 = 1 %).
    /// Si se alcanza antes que `ratio`, la reducción se detiene ahí.
    pub max_error: f32,
}

impl Default for Simplification {
    fn default() -> Self {
        Self { ratio: 0.5, max_error: 0.01 }
    }
}

/// Compresión Draco. Los bits de cuantización fijan la precisión de cada
/// atributo: menos bits, archivo más chico y más error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DracoOptions {
    /// Nivel de compresión 0-10 (más alto: más chico y más lento de decodificar).
    pub compression_level: u8,
    pub position_bits: u8,
    pub normal_bits: u8,
    pub texcoord_bits: u8,
    pub color_bits: u8,
    /// Pesos de skinning, tangentes y demás atributos genéricos.
    pub generic_bits: u8,
}

impl Default for DracoOptions {
    /// Los de glTF-Transform y Blender salvo las posiciones: 14 bits mueven
    /// superficies casi coincidentes (capas dobles de modelos escaneados o
    /// esculpidos) y aparecen manchas; 16 bits cuestan ~10 % más de geometría.
    fn default() -> Self {
        Self {
            compression_level: 7,
            position_bits: 16,
            normal_bits: 10,
            texcoord_bits: 12,
            color_bits: 8,
            generic_bits: 12,
        }
    }
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
            simplify: None,
            draco: None,
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
            || self.simplify.is_some()
            || !self.export_animations
    }

    /// Retorna true si alguna opción de procesamiento de texturas está activa.
    pub(crate) fn needs_texture_processing(&self) -> bool {
        self.texture_quality.is_some() || self.max_texture_size.is_some()
    }
}
