//! Representación intermedia de escena 3D.
//!
//! Este crate define las estructuras de datos que actúan como formato pivote
//! entre todos los formatos soportados (glTF, USD, STL, etc).
//!
//! El pipeline de conversión es:
//! ```text
//! Formato origen → Scene (intermedio) → Formato destino
//! ```

mod scene;
mod mesh;
mod material;
mod texture;
mod skeleton;
mod animation;
mod transform;
mod world;

pub use scene::{Scene, Node, SceneError};
pub use mesh::{Mesh, Primitive, VertexAttribute, IndexData};
pub use material::{Material, AlphaMode};
pub use texture::{Texture, TextureFormat, TextureRef};
pub use skeleton::{Skeleton, Joint};
pub use animation::{Animation, Channel, Interpolation, KeyframeTimes, KeyframeValues};
pub use transform::Transform;
pub use world::{MeshInstance, WorldPrimitive};

/// Re-export de `glam` para construir transformaciones sin depender del crate
pub use glam;
