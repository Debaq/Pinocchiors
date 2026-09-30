//! Coordenadas UV: la "piel" de la malla.
//!
//! - [`UvSurface`]: malla de referencia con UV (costuras e islas).
//! - [`transfer_uvs`]: lleva esas UV a otra malla que recubre la misma
//!   superficie, como la retopologizada.
//! - [`unwrap`]: despliega una malla sin UV (o con UV que se quieren
//!   rehacer): cartas, LSCM + ARAP y empaquetado.
//! - [`bake`]: hornea texturas de la superficie de referencia sobre un
//!   mapa nuevo, sin costuras entre islas.
//! - [`Skin`]: todo lo anterior en términos de una escena (UV + materiales).
//!
//! Pendientes en `PENDIENTES.md` (raíz del repositorio).

mod bake;
mod charts;
mod compact;
mod geometry;
mod pack;
mod param;
mod parts;
mod skin;
mod surface;
mod tangent;
mod transfer;
mod unwrap;

pub use bake::{bake, BakeChannel, Baked, TexelContext};
pub use charts::ChartOptions;
pub use compact::{compact_skin, compacted_scene};
pub use skin::{
    checker_texture, material_surface, scene_surface, skin_scene, transferred_skin, unwrapped_skin, unwrapped_skin_by_parts,
    BakeOptions, Skin, SkinInfo, SkinParts,
};
pub use parts::clean_parts;
pub use surface::{UvPart, UvSurface};
pub use tangent::{corner_frames, mikk_tangents, CornerFrames};
pub use transfer::{original_regions, transfer_uvs, UvTransfer};
pub use unwrap::{unwrap, unwrap_by_parts, unwrap_with_regions, Layout, Unwrap, UnwrapOptions};
