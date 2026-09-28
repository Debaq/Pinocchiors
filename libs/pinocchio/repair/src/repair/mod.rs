//! Pasos de reparación. Cada uno modifica una [`crate::TriMesh`] y puede
//! usarse por separado; [`crate::repair_all`] los encadena en el orden
//! correcto.

pub mod cleanup;
pub mod holes;
pub mod manifold;
pub mod orient;
