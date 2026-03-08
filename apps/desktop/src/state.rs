//! Estado compartido de la aplicación

use converter_scene::Scene;
use pinocchio_core::PinocchioOutput;
use pinocchio_mesh::Mesh;
use pinocchio_repair::MeshDiagnostics;
use pinocchio_skeleton::BasicSkeleton;
use quadriflow_core::QuadMesh;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

/// Tipos de esqueleto disponibles
#[derive(Debug, Clone)]
pub enum SkeletonType {
    Human,
    Quad,
    Horse,
    Centaur,
    Bird,
    Spider,
    Serpent,
    Mech,
    Custom(BasicSkeleton),
}

/// Estado compartido de la aplicación
pub struct AppState {
    /// Escena importada (formato pivote)
    pub scene: Mutex<Option<Scene>>,

    /// Malla convertida para pinocchio
    pub mesh: Mutex<Option<Mesh>>,

    /// Tipo de esqueleto seleccionado
    pub skeleton: Mutex<Option<SkeletonType>>,

    /// Esqueleto original (antes de transformaciones)
    pub original_skeleton: Mutex<Option<SkeletonType>>,

    /// Resultado del autorig
    pub result: Mutex<Option<PinocchioOutput>>,

    /// Malla de quads resultante de retopología
    pub quad_mesh: Mutex<Option<QuadMesh>>,

    /// Indica si hay un proceso en curso
    pub processing: AtomicBool,

    // ── Repair ──
    /// Diagnósticos del análisis de malla
    pub diagnostics: Mutex<Option<MeshDiagnostics>>,
    /// Backup de malla antes de reparación
    pub mesh_before_repair: Mutex<Option<Mesh>>,
    /// Backup de escena antes de reparación
    pub scene_before_repair: Mutex<Option<Scene>>,

    // ── Print3D ──
    /// Piezas resultantes de subdivisión
    pub print3d_pieces: Mutex<Option<Vec<pinocchio_print3d::LabeledPiece>>>,
    /// Backup de malla antes de escalar para impresión
    pub mesh_before_print_scale: Mutex<Option<Mesh>>,
}

impl AppState {
    /// Crea un nuevo estado vacío
    pub fn new() -> Self {
        Self {
            scene: Mutex::new(None),
            mesh: Mutex::new(None),
            skeleton: Mutex::new(None),
            original_skeleton: Mutex::new(None),
            result: Mutex::new(None),
            quad_mesh: Mutex::new(None),
            processing: AtomicBool::new(false),
            diagnostics: Mutex::new(None),
            mesh_before_repair: Mutex::new(None),
            scene_before_repair: Mutex::new(None),
            print3d_pieces: Mutex::new(None),
            mesh_before_print_scale: Mutex::new(None),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
