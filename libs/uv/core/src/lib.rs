//! Coordenadas UV: la "piel" de la malla.
//!
//! - [`UvSurface`]: malla de referencia con UV dividida en cartas (islas).
//! - [`transfer_uvs`]: lleva esas UV a otra malla que recubre la misma
//!   superficie, como la retopologizada.
//!
//! Plan completo (desplegado, empaquetado, horneado) en `libs/uv/ROADMAP.md`.

mod surface;
mod transfer;

pub use surface::{UvPart, UvSurface};
pub use transfer::{transfer_uvs, UvTransfer};
