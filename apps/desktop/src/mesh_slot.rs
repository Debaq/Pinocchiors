//! Malla de edición (half-edge) con construcción diferida
//!
//! Armar la half-edge de un modelo de millones de caras cuesta segundos y más
//! de un gigabyte, y el visor no la necesita para mostrarlo. Con un modelo
//! grande se deja pendiente y se arma en segundo plano o al primer uso;
//! quien pide la malla con [`MeshSlot::lock`] espera a que esté lista.

use converter_scene::Scene;
use pinocchio_mesh::Mesh;
use std::sync::{Arc, Condvar, LockResult, Mutex, MutexGuard};

/// Cuándo se arma la malla de edición de un modelo grande
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MeshBuild {
    /// Al importar, antes de devolver el modelo
    Now,
    /// Después de mostrar el modelo, en otro hilo
    #[default]
    Background,
    /// La primera vez que algo la necesita
    Lazy,
}

impl MeshBuild {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "now" => Some(Self::Now),
            "background" => Some(Self::Background),
            "lazy" => Some(Self::Lazy),
            _ => None,
        }
    }
}

/// Desde esta cantidad de caras se difiere; por debajo se arma al instante
pub const DEFER_FROM_FACES: usize = 150_000;

enum Job {
    Idle,
    /// Pendiente: se arma al primer uso
    Lazy(Box<Scene>),
    /// Armándose en un hilo (la generación detecta si se descartó)
    Building,
}

struct Control {
    job: Job,
    generation: u64,
}

struct Inner {
    mesh: Mutex<Option<Mesh>>,
    control: Mutex<Control>,
    done: Condvar,
}

pub struct MeshSlot(Arc<Inner>);

impl Default for MeshSlot {
    fn default() -> Self {
        Self(Arc::new(Inner {
            mesh: Mutex::new(None),
            control: Mutex::new(Control { job: Job::Idle, generation: 0 }),
            done: Condvar::new(),
        }))
    }
}

impl MeshSlot {
    /// La malla, esperando a que termine de armarse si estaba pendiente
    pub fn lock(&self) -> LockResult<MutexGuard<'_, Option<Mesh>>> {
        self.ensure();
        self.0.mesh.lock()
    }

    /// Descarta lo pendiente y deja la malla vacía
    pub fn clear(&self) {
        self.cancel();
        *self.0.mesh.lock().unwrap() = None;
    }

    /// Hay modelo (armado o pendiente), sin forzar su construcción
    pub fn is_loaded(&self) -> bool {
        !matches!(self.0.control.lock().unwrap().job, Job::Idle) || self.0.mesh.lock().unwrap().is_some()
    }

    /// La malla de edición ya está armada (o no hay modelo)
    pub fn is_ready(&self) -> bool {
        matches!(self.0.control.lock().unwrap().job, Job::Idle)
    }

    /// Deja `scene` como pendiente para armarla al primer uso
    pub fn defer(&self, scene: Scene) {
        self.cancel();
        *self.0.mesh.lock().unwrap() = None;
        self.0.control.lock().unwrap().job = Job::Lazy(Box::new(scene));
    }

    /// Arma la malla de `scene` en otro hilo; quien la pida antes espera
    pub fn build_in_background(&self, scene: Scene) {
        self.cancel();
        *self.0.mesh.lock().unwrap() = None;
        let generation = {
            let mut control = self.0.control.lock().unwrap();
            control.job = Job::Building;
            control.generation
        };
        let inner = Arc::clone(&self.0);
        std::thread::spawn(move || {
            let built = pinocchio_mesh::scene_to_mesh(&scene);
            drop(scene);
            inner.publish(generation, built);
        });
    }

    fn cancel(&self) {
        let mut control = self.0.control.lock().unwrap();
        control.generation += 1;
        control.job = Job::Idle;
        self.0.done.notify_all();
    }

    fn ensure(&self) {
        let mut control = self.0.control.lock().unwrap();
        loop {
            match std::mem::replace(&mut control.job, Job::Idle) {
                Job::Idle => return,
                Job::Building => {
                    control.job = Job::Building;
                    control = self.0.done.wait(control).unwrap();
                }
                Job::Lazy(scene) => {
                    control.job = Job::Building;
                    let generation = control.generation;
                    drop(control);
                    let built = pinocchio_mesh::scene_to_mesh(&scene);
                    self.0.publish(generation, built);
                    control = self.0.control.lock().unwrap();
                }
            }
        }
    }
}

impl Inner {
    /// Guarda la malla armada si nadie descartó el trabajo mientras tanto
    fn publish(&self, generation: u64, built: Option<Mesh>) {
        let mut control = self.control.lock().unwrap();
        if control.generation == generation {
            *self.mesh.lock().unwrap() = built;
            control.job = Job::Idle;
        }
        self.done.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use converter_scene::{IndexData, Mesh as SceneMesh, Node, Primitive, Transform, VertexAttribute};

    fn triangle_scene() -> Scene {
        let mut scene = Scene::new();
        scene.meshes.push(SceneMesh {
            name: "t".into(),
            primitives: vec![Primitive {
                attributes: vec![VertexAttribute::Positions(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]])],
                indices: Some(IndexData::U32(vec![0, 1, 2])),
                material: None,
            }],
        });
        scene.nodes.push(Node { name: "n".into(), transform: Transform::identity(), mesh: Some(0), skin: None, children: Vec::new() });
        scene.root_nodes.push(0);
        scene
    }

    #[test]
    fn lazy_builds_on_first_use() {
        let slot = MeshSlot::default();
        slot.defer(triangle_scene());
        assert!(slot.is_loaded() && !slot.is_ready());
        assert_eq!(slot.lock().unwrap().as_ref().unwrap().num_faces(), 1);
        assert!(slot.is_ready());
    }

    #[test]
    fn background_build_is_waited_for() {
        let slot = MeshSlot::default();
        slot.build_in_background(triangle_scene());
        assert_eq!(slot.lock().unwrap().as_ref().unwrap().num_vertices(), 3);
        assert!(slot.is_ready());
    }

    #[test]
    fn clear_discards_pending() {
        let slot = MeshSlot::default();
        slot.defer(triangle_scene());
        slot.clear();
        assert!(!slot.is_loaded());
        assert!(slot.lock().unwrap().is_none());
    }
}
