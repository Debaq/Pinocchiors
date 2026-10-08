//! Documento CAD paramétrico de Pinocchiors.
//!
//! Un [`Document`] es una lista ordenada de operaciones ([`Feature`]): sketches
//! con restricciones, extrusiones, revoluciones, redondeos, booleanas, patrones…
//! Cada una guarda su receta y [`Document::evaluate`] recalcula el sólido con
//! OpenCASCADE (`cad-occt`). Las caras y aristas se nombran por geometría
//! ([`FaceRef`], [`EdgeRef`]) para sobrevivir a los recálculos.
//!
//! Unidades: mm y grados. Ejes: Z arriba.

pub mod assembly;
pub mod document;
pub mod eval;
pub mod expr;
pub mod feature;
pub mod geom;
pub mod measure;
pub mod regions;
pub mod sheet;
pub mod sketch;
pub mod standard;

pub use cad_occt::{self as occt, Shape};
pub use assembly::{Assembly, AssemblySolution, Connector, Instance, Mate, MateKind};
pub use document::{Configuration, Document, Folder, Material, NamedVersion, PartProps, ModelError, Parameter, Resolution, ResolvedValue};
pub use eval::{EvalCache, Evaluation, FeatureState, FeatureStatus, MissingRef, Part, RefGeom, SketchResult};
pub use feature::*;
pub use geom::{P2, P3, Plane};
pub use measure::{Distance, ItemMeasure, MeasureItem, Measurement, measure};
pub use regions::{Loop, LoopPiece, Region, find_regions};
pub use sketch::{Geometry, Sketch, SketchConstraint, SketchEntity, SketchError, SketchPoint, SketchStatus, SketchText, SketchUse, SolveReport};
