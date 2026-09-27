//! Configuración para el proceso de auto-rigging

use pinocchio_math::Real;

/// Cómo se coloca el esqueleto respecto de la malla antes del embedding
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SkeletonFit {
    /// El esqueleto es una plantilla en su propio espacio (p. ej. los presets,
    /// definidos con altura 1): se escala y centra dentro de la malla.
    #[default]
    Auto,
    /// El esqueleto ya está colocado en las coordenadas de la malla (p. ej.
    /// ajustado a mano en un editor): se usa tal cual.
    None,
}

/// Configuración para el proceso de auto-rigging
#[derive(Debug, Clone)]
pub struct PinocchioConfig {
    /// Peso de difusión para heat diffusion (mayor = más suave)
    pub diffusion_weight: Real,

    /// Número máximo de esferas para el eje medial.
    /// Sin efecto desde el embedding por cadenas; se conserva por compatibilidad.
    pub max_medial_spheres: usize,

    /// Número de iteraciones para refinar el embedding.
    /// Sin efecto desde el embedding por cadenas; se conserva por compatibilidad.
    pub refine_iterations: usize,

    /// Número máximo de influencias de hueso por vértice
    pub max_bone_influences: usize,

    /// Umbral mínimo de peso para considerar una influencia
    pub weight_threshold: Real,

    /// Resolución del campo de distancias
    pub distance_field_resolution: [usize; 3],

    /// Si se debe normalizar la malla antes del procesamiento
    pub normalize_mesh: bool,

    /// Si se debe verificar la integridad de la malla
    pub verify_mesh_integrity: bool,

    /// Umbral de triángulos para activar decimación automática
    /// Si la malla tiene más triángulos que este valor, se decima
    pub auto_decimate_threshold: usize,

    /// Ratio de decimación (0.0-1.0) cuando se activa decimación automática
    pub decimate_ratio: Real,

    /// Colocación del esqueleto respecto de la malla
    pub skeleton_fit: SkeletonFit,
}

impl Default for PinocchioConfig {
    fn default() -> Self {
        Self {
            diffusion_weight: 1.0,
            max_medial_spheres: 1000,
            refine_iterations: 100,
            max_bone_influences: 4,
            weight_threshold: 0.001,
            distance_field_resolution: [64, 64, 64],
            normalize_mesh: true,
            verify_mesh_integrity: true,
            auto_decimate_threshold: 100_000,
            decimate_ratio: 0.1,
            skeleton_fit: SkeletonFit::Auto,
        }
    }
}

impl PinocchioConfig {
    /// Crea una configuración para procesamiento rápido (menor calidad)
    pub fn fast() -> Self {
        Self {
            diffusion_weight: 1.0,
            max_medial_spheres: 200,
            refine_iterations: 20,
            max_bone_influences: 4,
            weight_threshold: 0.01,
            distance_field_resolution: [32, 32, 32],
            normalize_mesh: true,
            verify_mesh_integrity: false,
            auto_decimate_threshold: 50_000,
            decimate_ratio: 0.05,
            skeleton_fit: SkeletonFit::Auto,
        }
    }

    /// Crea una configuración para alta calidad (más lento)
    pub fn high_quality() -> Self {
        Self {
            diffusion_weight: 0.5,
            max_medial_spheres: 2000,
            refine_iterations: 500,
            max_bone_influences: 8,
            weight_threshold: 0.0001,
            distance_field_resolution: [128, 128, 128],
            normalize_mesh: true,
            verify_mesh_integrity: true,
            auto_decimate_threshold: 500_000,
            decimate_ratio: 0.2,
            skeleton_fit: SkeletonFit::Auto,
        }
    }

    /// Builder: establece el peso de difusión
    pub fn with_diffusion_weight(mut self, weight: Real) -> Self {
        self.diffusion_weight = weight;
        self
    }

    /// Builder: establece el número máximo de esferas mediales
    pub fn with_max_spheres(mut self, max: usize) -> Self {
        self.max_medial_spheres = max;
        self
    }

    /// Builder: establece las iteraciones de refinamiento
    pub fn with_refine_iterations(mut self, iterations: usize) -> Self {
        self.refine_iterations = iterations;
        self
    }

    /// Builder: establece el máximo de influencias por vértice
    pub fn with_max_influences(mut self, max: usize) -> Self {
        self.max_bone_influences = max;
        self
    }

    /// Builder: establece la resolución del campo de distancias
    pub fn with_resolution(mut self, resolution: [usize; 3]) -> Self {
        self.distance_field_resolution = resolution;
        self
    }

    /// Builder: deshabilita la normalización de la malla
    pub fn without_normalization(mut self) -> Self {
        self.normalize_mesh = false;
        self
    }

    /// Builder: configura decimación automática
    pub fn with_auto_decimate(mut self, threshold: usize, ratio: Real) -> Self {
        self.auto_decimate_threshold = threshold;
        self.decimate_ratio = ratio.clamp(0.01, 1.0);
        self
    }

    /// Builder: establece cómo se coloca el esqueleto respecto de la malla
    pub fn with_skeleton_fit(mut self, fit: SkeletonFit) -> Self {
        self.skeleton_fit = fit;
        self
    }

    /// Builder: deshabilita decimación automática
    pub fn without_auto_decimate(mut self) -> Self {
        self.auto_decimate_threshold = usize::MAX;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = PinocchioConfig::default();
        assert!(config.diffusion_weight > 0.0);
        assert!(config.max_medial_spheres > 0);
    }

    #[test]
    fn test_builder() {
        let config = PinocchioConfig::default()
            .with_diffusion_weight(2.0)
            .with_max_spheres(500);

        assert!((config.diffusion_weight - 2.0).abs() < 1e-10);
        assert_eq!(config.max_medial_spheres, 500);
    }

    #[test]
    fn test_presets() {
        let fast = PinocchioConfig::fast();
        let hq = PinocchioConfig::high_quality();

        assert!(fast.max_medial_spheres < hq.max_medial_spheres);
        assert!(fast.refine_iterations < hq.refine_iterations);
    }
}
