//! Pinocchio Mesh - Estructura de datos half-edge y I/O
//!
//! Este crate proporciona:
//! - Malla half-edge (MeshVertex, MeshEdge, Mesh)
//! - I/O para formatos OBJ, PLY, STL
//! - Operaciones de normalización y verificación de integridad
//! - Decimación para reducir mallas grandes
//! - Morph targets (blend shapes)
//!
//! ## Feature: `converter`
//!
//! Cuando el feature `converter` está activo, se habilita integración con
//! `converter-scene` para cargar formatos 3D via la representación intermedia `Scene`.
//!
//! ```toml
//! pinocchio-mesh = { version = "0.1", features = ["converter"] }
//! ```

mod vertex;
mod edge;
mod mesh;
pub mod io;
mod decimation;
pub mod morph_target;

#[cfg(feature = "converter")]
mod adapter;

pub use vertex::MeshVertex;
pub use edge::MeshEdge;
pub use mesh::Mesh;
pub use io::{load_obj, load_glb, load_mesh, MeshLoadError};
pub use decimation::{decimate, decimate_with_resolution};
pub use morph_target::{MorphDelta, MorphTarget, MeshWithMorphs};

#[cfg(feature = "converter")]
pub use io::load_stl;

#[cfg(feature = "converter")]
pub use adapter::{scene_to_mesh, load_glb_via_converter, load_stl_via_converter};
