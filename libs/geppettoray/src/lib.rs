//! GeppettoRay: motor de render propio de Pinocchio (path tracing en CPU).
//!
//! Fase 3a: escena a triángulos en mundo, BVH, cámara de agujero, material
//! difuso + emisión, luz de entorno uniforme y un sol. Se trabaja en lineal y
//! se guarda en sRGB al final.

mod bvh;
mod camera;
mod image_out;
mod render;
mod rng;
mod scene;

pub use bvh::{Aabb, Bvh, Hit, Ray};
pub use camera::Camera;
pub use image_out::{Image, SaveError, linear_to_srgb};
pub use render::{Environment, Settings, Sun, render};
pub use scene::{RayMaterial, RayScene, Triangle};
