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
    /// Plantilla generada (forma de cuerpo + apéndices): se ajusta a la malla
    /// igual que los presets
    Template(BasicSkeleton),
    /// Ya colocado sobre la malla (ajustado o editado): se usa tal cual
    Custom(BasicSkeleton),
}

/// Transformación de gizmo aplicada sobre el esqueleto base
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkeletonTransformParams {
    pub scale: f64,
    pub translation: [f64; 3],
    /// Euler XYZ en grados
    pub rotation: [f64; 3],
    /// Centro de la escala y la rotación, en el espacio de la base. Se fija al
    /// empezar a transformar (centro de la caja de la base) y no cambia al
    /// editar huesos: si no, mover una articulación correría las demás
    pub pivot: [f64; 3],
}

impl SkeletonTransformParams {
    /// Sin escala, giro ni desplazamiento (el pivote no importa)
    pub fn is_identity(&self) -> bool {
        self.scale == 1.0 && self.translation == [0.0; 3] && self.rotation == [0.0; 3]
    }
}

impl Default for SkeletonTransformParams {
    fn default() -> Self {
        Self { scale: 1.0, translation: [0.0; 3], rotation: [0.0; 3], pivot: [0.0; 3] }
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
/// El modelo tal como se importó (se guarda en el proyecto y permite revertir)
pub struct OriginalModel {
    /// Nombre del archivo importado
    pub name: String,
    /// Formato de origen ("GLB", "STL"…)
    pub format: String,
    pub scene: Scene,
}

pub struct AppState {
    pub original_model: Mutex<Option<OriginalModel>>,
    /// Huella del último guardado del proyecto (el automático no reescribe si no cambió)
    pub last_saved_hash: Mutex<Option<u64>>,

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

    /// Preset elegido: la plantilla que parte de cero el ajuste automático,
    /// aunque el esqueleto visible ya esté editado
    pub skeleton_preset: Mutex<Option<SkeletonType>>,

    /// Campo de distancias de la malla activa para centrar articulaciones
    /// (se calcula la primera vez y se descarta si cambia la malla activa)
    pub joint_centering: Mutex<Option<std::sync::Arc<pinocchio_embedding::JointCentering>>>,

    /// Resultado del autorig, sobre la malla activa cuando se calculó
    pub result: Mutex<Option<PinocchioOutput>>,

    /// El resultado del autorig se calculó sobre la malla de quads (sus
    /// vértices son los de `quad_mesh`); si no, sobre `mesh`
    pub rig_on_quad: AtomicBool,

    /// Las etapas posteriores a la retopología (UV, esqueleto, pesos) usan la
    /// malla de quads si existe. Se activa al retopologizar.
    pub use_retopology: AtomicBool,

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
            original_model: Mutex::new(None),
            last_saved_hash: Mutex::new(None),
            scene: Mutex::new(None),
            mesh: Mutex::new(None),
            skeleton: Mutex::new(None),
            original_skeleton: Mutex::new(None),
            skeleton_transform: Mutex::new(SkeletonTransformParams::default()),
            skeleton_preset: Mutex::new(None),
            joint_centering: Mutex::new(None),
            result: Mutex::new(None),
            rig_on_quad: AtomicBool::new(false),
            use_retopology: AtomicBool::new(true),
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
    /// La malla activa para esqueleto y pesos es la de quads
    pub fn active_is_quad(&self) -> bool {
        self.use_retopology.load(Ordering::SeqCst) && self.quad_mesh.lock().unwrap().is_some()
    }

    /// La malla activa cambió: el rig y el campo para centrar ya no le corresponden
    pub fn active_mesh_changed(&self) {
        *self.result.lock().unwrap() = None;
        *self.joint_centering.lock().unwrap() = None;
    }

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
        *self.joint_centering.lock().unwrap() = None;
    }

    /// Descarta todo lo derivado de la malla actual (resultados, backups,
    /// diagnósticos, piezas). Se llama al importar un modelo nuevo.
    pub fn reset_derived(&self) {
        *self.result.lock().unwrap() = None;
        *self.quad_mesh.lock().unwrap() = None;
        *self.quad_skin.lock().unwrap() = None;
        *self.joint_centering.lock().unwrap() = None;
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
