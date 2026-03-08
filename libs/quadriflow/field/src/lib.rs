//! # quadriflow-field
//!
//! Orientation and position field computation for quad remeshing.
//!
//! This module implements the field-aligned approach from:
//! "QuadriFlow: A Scalable and Robust Method for Quadrangulation" (SGP 2018)
//!
//! ## Components
//!
//! - **Orientation Field**: 4-RoSy field aligned to surface features
//! - **Position Field**: Integer-grid aligned position field
//! - **Singularities**: Detection and minimization of field singularities

pub mod orientation;
pub mod position;
pub mod singularity;

pub use orientation::OrientationField;
pub use position::{FaceTransition, PositionField, PositionFieldConfig};
pub use singularity::{
    count_singularities, detect_singularities, euler_from_singularities, Singularity,
    SingularityInfo, SingularityType,
};
