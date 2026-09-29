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

// La escena usa Y arriba con el frente hacia +Z (glTF). STL, PLY y 3MF, como
// los escriben Blender, los CAD y los slicers, usan Z arriba con el frente
// hacia −Y. El paso entre ambos es un giro de 90° en X (sin espejo): lo que
// está abajo sigue abajo al importar y al exportar.

/// Punto o dirección con Z arriba → Y arriba: (x, y, z) → (x, z, −y)
pub fn z_up_to_y_up([x, y, z]: [f32; 3]) -> [f32; 3] {
    // 0 − v y no −v: sin −0, que los formatos de texto escriben como "-0"
    [x, z, 0.0 - y]
}

/// Punto o dirección con Y arriba → Z arriba: (x, y, z) → (x, −z, y)
pub fn y_up_to_z_up([x, y, z]: [f32; 3]) -> [f32; 3] {
    [x, 0.0 - z, y]
}

/// Re-export de `glam` para construir transformaciones sin depender del crate
pub use glam;
