//! Pinocchio Embedding - Embedding del esqueleto en la malla
//!
//! Este crate proporciona:
//! - Muestreo de superficie medial
//! - Empaquetado de esferas
//! - Grafo topológico de esferas
//! - Embedding discreto y refinamiento

pub mod medial_surface;
pub mod sphere_packing;
pub mod sphere_graph;
pub mod embedding;
pub mod chain;
pub mod fit;
pub mod center;

pub use medial_surface::{medial_spheres_from_field, sample_medial_surface, sample_medial_surface_adaptive, MedialSphere, AdaptiveSamplingConfig};
pub use sphere_packing::pack_spheres;
pub use sphere_graph::SphereGraph;
pub use chain::chain_embed;
pub use fit::{fit_skeleton, Extremity, FitOptions, FitResult};
pub use center::JointCentering;
pub use embedding::{
    discrete_embed, refine_embedding, refine_embedding_global,
    full_embedding_pipeline, EmbeddingResult, EmbeddingError,
    RefinementConfig,
};
