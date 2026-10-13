//! GeppettoRay: motor de render propio de Pinocchio (path tracing en CPU).
//!
//! Fase 3a: escena a triángulos en mundo, BVH, cámara de agujero, material
//! difuso + emisión, luz de entorno uniforme y un sol. Se trabaja en lineal y
//! se guarda en sRGB al final.

mod animation;
mod bsdf;
mod bvh;
mod camera;
mod denoise;
mod env;
mod glass;
mod image_out;
mod progressive;
mod render;
mod rng;
mod scene;
mod texture;

pub use bsdf::Bsdf;
pub use rng::Rng;
pub use bvh::{Aabb, Bvh, Hit, Ray};
pub use camera::{Camera, Lens, SENSOR_FULL_FRAME_HEIGHT_MM};
pub use image_out::{Image, SaveError, ToneMap, linear_to_srgb};
pub use glass::{absorption_sigma, fresnel_dielectric, reflect, refract};
pub use env::{EnvMap, Environment};
pub use denoise::{DenoiseSettings, denoise};
pub use animation::{AnimationOutcome, AnimationSettings, FrameProgress, render_animation};
pub use progressive::{Frame, Outcome, PassInfo, ProgressiveOptions, render_progressive};
pub use render::{Aov, Light, Settings, ShadowCatcher, Sun, render, render_aov};
pub use scene::{AnimatedScene, AreaLights, RayAlpha, RayMaterial, RayScene, Triangle, quad};
pub use texture::Texture;
