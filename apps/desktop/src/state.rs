//! Estado compartido de la aplicación

use converter_scene::Scene;
use pinocchio_core::PinocchioOutput;
use pinocchio_mesh::Mesh;
use pinocchio_repair::MeshDiagnostics;
use pinocchio_skeleton::BasicSkeleton;
use quadriflow_core::QuadMesh;
use std::sync::atomic::{AtomicBool, Ordering};
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

/// Transformación de gizmo aplicada sobre el esqueleto base
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkeletonTransformParams {
    pub scale: f64,
    pub translation: [f64; 3],
    /// Euler XYZ en grados
    pub rotation: [f64; 3],
}

impl Default for SkeletonTransformParams {
    fn default() -> Self {
        Self { scale: 1.0, translation: [0.0; 3], rotation: [0.0; 3] }
    }
}

/// Marca un proceso largo en curso; libera la marca al soltarse (también si hay
/// un error o un panic)
pub struct ProcessingGuard<'a>(&'a AtomicBool);

impl Drop for ProcessingGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// Estado compartido de la aplicación
pub struct AppState {
    /// Escena importada (formato pivote)
    pub scene: Mutex<Option<Scene>>,

    /// Malla convertida para pinocchio
    pub mesh: Mutex<Option<Mesh>>,

    /// Tipo de esqueleto seleccionado
    pub skeleton: Mutex<Option<SkeletonType>>,

    /// Esqueleto base (preset, auto-fit o con huesos editados), antes de la
    /// transformación de gizmo
    pub original_skeleton: Mutex<Option<SkeletonType>>,

    /// Transformación de gizmo aplicada sobre `original_skeleton`
    pub skeleton_transform: Mutex<SkeletonTransformParams>,

    /// Resultado del autorig
    pub result: Mutex<Option<PinocchioOutput>>,

    /// Malla de quads resultante de retopología
    pub quad_mesh: Mutex<Option<QuadMesh>>,

    /// Piel de `quad_mesh`: UV trasladadas del original al retopologizar, o
    /// desplegado nuevo con texturas horneadas (paso UV)
    pub quad_skin: Mutex<Option<uv_core::Skin<4>>>,

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
    /// Backup de escena antes de escalar para impresión
    pub scene_before_print_scale: Mutex<Option<Scene>>,
}

impl AppState {
    /// Crea un nuevo estado vacío
    pub fn new() -> Self {
        Self {
            scene: Mutex::new(None),
            mesh: Mutex::new(None),
            skeleton: Mutex::new(None),
            original_skeleton: Mutex::new(None),
            skeleton_transform: Mutex::new(SkeletonTransformParams::default()),
            result: Mutex::new(None),
            quad_mesh: Mutex::new(None),
            quad_skin: Mutex::new(None),
            processing: AtomicBool::new(false),
            diagnostics: Mutex::new(None),
            mesh_before_repair: Mutex::new(None),
            scene_before_repair: Mutex::new(None),
            print3d_pieces: Mutex::new(None),
            mesh_before_print_scale: Mutex::new(None),
            scene_before_print_scale: Mutex::new(None),
        }
    }
}

impl AppState {
    /// Intenta marcar un proceso largo como en curso. `None` si ya hay uno.
    pub fn try_begin_processing(&self) -> Option<ProcessingGuard<'_>> {
        if self.processing.swap(true, Ordering::SeqCst) {
            None
        } else {
            Some(ProcessingGuard(&self.processing))
        }
    }

    /// La geometría cambió: el rig y la retopología ya no le corresponden.
    pub fn geometry_changed(&self) {
        *self.result.lock().unwrap() = None;
        *self.quad_mesh.lock().unwrap() = None;
        *self.quad_skin.lock().unwrap() = None;
    }

    /// Descarta todo lo derivado de la malla actual (resultados, backups,
    /// diagnósticos, piezas). Se llama al importar un modelo nuevo.
    pub fn reset_derived(&self) {
        *self.result.lock().unwrap() = None;
        *self.quad_mesh.lock().unwrap() = None;
        *self.quad_skin.lock().unwrap() = None;
        *self.diagnostics.lock().unwrap() = None;
        *self.mesh_before_repair.lock().unwrap() = None;
        *self.scene_before_repair.lock().unwrap() = None;
        *self.print3d_pieces.lock().unwrap() = None;
        *self.mesh_before_print_scale.lock().unwrap() = None;
        *self.scene_before_print_scale.lock().unwrap() = None;
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
