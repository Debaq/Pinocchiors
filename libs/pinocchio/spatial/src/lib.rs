//! Pinocchio Spatial - Estructuras de datos espaciales
//!
//! Este crate proporciona:
//! - Octree genérico para búsquedas espaciales
//! - Campo de distancias para mallas
//! - BVH para consultas de distancia aceleradas

mod octree;
mod distance_field;
mod bvh;

pub use octree::Octree;
pub use distance_field::DistanceField;
pub use bvh::{closest_point_on_triangle, Bvh, ClosestHit, Triangle};
