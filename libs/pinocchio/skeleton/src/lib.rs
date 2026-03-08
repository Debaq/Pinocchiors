//! Pinocchio Skeleton - Definiciones de esqueletos y presets
//!
//! Este crate proporciona:
//! - Trait `Skeleton` para representar esqueletos
//! - Presets: HumanSkeleton, QuadSkeleton, HorseSkeleton, CentaurSkeleton,
//!   BirdSkeleton, SpiderSkeleton, SerpentSkeleton, MechSkeleton
//! - Carga de esqueletos desde JSON
//! - Carga de esqueletos desde glTF (con feature `converter`)

pub mod skeleton;
pub mod bone;
pub mod presets;
pub mod json_loader;

#[cfg(feature = "converter")]
pub mod gltf_loader;

pub use skeleton::{Skeleton, BasicSkeleton};
pub use bone::Bone;
pub use presets::{
    HumanSkeleton, QuadSkeleton, HorseSkeleton, CentaurSkeleton,
    BirdSkeleton, SpiderSkeleton, SerpentSkeleton, MechSkeleton,
};
pub use json_loader::{
    load_skeleton_json, save_skeleton_json,
    load_skeleton_from_file, save_skeleton_to_file,
    SkeletonJson, BoneJson, SkeletonMetadata, JsonLoadError,
};

#[cfg(feature = "converter")]
pub use gltf_loader::from_scene_skeleton;
