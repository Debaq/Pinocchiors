//! Remallar: otras formas de ordenar una malla además de la retopología
//! (ver `PLAN_REMALLAR.md`).
//!
//! - [`simplify`]: quita triángulos conservando la forma, las costuras de UV y
//!   los atributos de los vértices que quedan (feature `simplify`).
//! - [`deviation`]: cuánto se aleja una malla de otra, para el "antes →
//!   después" de cualquier modo.

pub mod deviation;
#[cfg(feature = "simplify")]
pub mod simplify;

pub use deviation::{deviation, Deviation};
#[cfg(feature = "simplify")]
pub use simplify::{simplify, SimplifyInput, SimplifyOptions, Simplified};
