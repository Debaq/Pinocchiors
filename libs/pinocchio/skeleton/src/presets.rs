//! Esqueletos predefinidos

// Los presets se arman hueso a hueso con un comentario por parte del cuerpo
#![allow(clippy::vec_init_then_push)]

use crate::{Bone, Skeleton};
use pinocchio_math::Vector3;

/// Esqueleto humanoide
#[derive(Debug, Clone)]
pub struct HumanSkeleton {
    bones: Vec<Bone>,
}

impl HumanSkeleton {
    /// Crea un nuevo esqueleto humanoide
    pub fn new() -> Self {
        let mut bones = Vec::new();

        // Raíz (pelvis)
        bones.push(Bone::new("pelvis", Vector3::new(0.0, 0.5, 0.0)));

        // Columna
        bones.push(Bone::with_parent("spine", Vector3::new(0.0, 0.65, 0.0), 0));
        bones.push(Bone::with_parent("chest", Vector3::new(0.0, 0.8, 0.0), 1));
        bones.push(Bone::with_parent("neck", Vector3::new(0.0, 0.9, 0.0), 2));
        bones.push(Bone::with_parent("head", Vector3::new(0.0, 1.0, 0.0), 3).as_leaf());

        // Brazo izquierdo
        bones.push(Bone::with_parent("shoulder_l", Vector3::new(-0.1, 0.85, 0.0), 2));
        bones.push(Bone::with_parent("elbow_l", Vector3::new(-0.25, 0.65, 0.0), 5));
        bones.push(Bone::with_parent("wrist_l", Vector3::new(-0.35, 0.5, 0.0), 6));
        bones.push(Bone::with_parent("hand_l", Vector3::new(-0.4, 0.45, 0.0), 7).as_leaf());

        // Brazo derecho
        bones.push(Bone::with_parent("shoulder_r", Vector3::new(0.1, 0.85, 0.0), 2));
        bones.push(Bone::with_parent("elbow_r", Vector3::new(0.25, 0.65, 0.0), 9));
        bones.push(Bone::with_parent("wrist_r", Vector3::new(0.35, 0.5, 0.0), 10));
        bones.push(Bone::with_parent("hand_r", Vector3::new(0.4, 0.45, 0.0), 11).as_leaf());

        // Pierna izquierda
        bones.push(Bone::with_parent("hip_l", Vector3::new(-0.1, 0.45, 0.0), 0));
        bones.push(Bone::with_parent("knee_l", Vector3::new(-0.1, 0.25, 0.0), 13));
        bones.push(Bone::with_parent("ankle_l", Vector3::new(-0.1, 0.05, 0.0), 14));
        bones.push(Bone::with_parent("foot_l", Vector3::new(-0.1, 0.0, 0.05), 15).as_leaf());

        // Pierna derecha
        bones.push(Bone::with_parent("hip_r", Vector3::new(0.1, 0.45, 0.0), 0));
        bones.push(Bone::with_parent("knee_r", Vector3::new(0.1, 0.25, 0.0), 17));
        bones.push(Bone::with_parent("ankle_r", Vector3::new(0.1, 0.05, 0.0), 18));
        bones.push(Bone::with_parent("foot_r", Vector3::new(0.1, 0.0, 0.05), 19).as_leaf());

        Self { bones }
    }
}

impl Default for HumanSkeleton {
    fn default() -> Self {
        Self::new()
    }
}

impl Skeleton for HumanSkeleton {
    fn num_bones(&self) -> usize {
        self.bones.len()
    }

    fn get_bone(&self, index: usize) -> Option<&Bone> {
        self.bones.get(index)
    }

    fn bones(&self) -> &[Bone] {
        &self.bones
    }

    fn scale(&mut self, factor: f64) {
        for bone in &mut self.bones {
            bone.position *= factor;
        }
    }

    fn translate(&mut self, offset: Vector3) {
        for bone in &mut self.bones {
            bone.position += offset;
        }
    }
}

/// Esqueleto de cuadrúpedo genérico
#[derive(Debug, Clone)]
pub struct QuadSkeleton {
    bones: Vec<Bone>,
}

impl QuadSkeleton {
    /// Crea un nuevo esqueleto de cuadrúpedo
    pub fn new() -> Self {
        let mut bones = Vec::new();

        // Cuerpo
        bones.push(Bone::new("hip", Vector3::new(0.0, 0.5, -0.3)));
        bones.push(Bone::with_parent("spine", Vector3::new(0.0, 0.55, 0.0), 0));
        bones.push(Bone::with_parent("chest", Vector3::new(0.0, 0.55, 0.3), 1));

        // Cabeza
        bones.push(Bone::with_parent("neck", Vector3::new(0.0, 0.6, 0.4), 2));
        bones.push(Bone::with_parent("head", Vector3::new(0.0, 0.6, 0.55), 3).as_leaf());

        // Cola
        bones.push(Bone::with_parent("tail_base", Vector3::new(0.0, 0.45, -0.4), 0));
        bones.push(Bone::with_parent("tail_end", Vector3::new(0.0, 0.4, -0.55), 5).as_leaf());

        // Patas delanteras
        bones.push(Bone::with_parent("shoulder_l", Vector3::new(-0.15, 0.5, 0.25), 2));
        bones.push(Bone::with_parent("elbow_l", Vector3::new(-0.15, 0.3, 0.25), 7));
        bones.push(Bone::with_parent("paw_fl", Vector3::new(-0.15, 0.0, 0.25), 8).as_leaf());

        bones.push(Bone::with_parent("shoulder_r", Vector3::new(0.15, 0.5, 0.25), 2));
        bones.push(Bone::with_parent("elbow_r", Vector3::new(0.15, 0.3, 0.25), 10));
        bones.push(Bone::with_parent("paw_fr", Vector3::new(0.15, 0.0, 0.25), 11).as_leaf());

        // Patas traseras
        bones.push(Bone::with_parent("hip_l", Vector3::new(-0.15, 0.45, -0.25), 0));
        bones.push(Bone::with_parent("knee_l", Vector3::new(-0.15, 0.25, -0.25), 13));
        bones.push(Bone::with_parent("paw_bl", Vector3::new(-0.15, 0.0, -0.25), 14).as_leaf());

        bones.push(Bone::with_parent("hip_r", Vector3::new(0.15, 0.45, -0.25), 0));
        bones.push(Bone::with_parent("knee_r", Vector3::new(0.15, 0.25, -0.25), 16));
        bones.push(Bone::with_parent("paw_br", Vector3::new(0.15, 0.0, -0.25), 17).as_leaf());

        Self { bones }
    }
}

impl Default for QuadSkeleton {
    fn default() -> Self {
        Self::new()
    }
}

impl Skeleton for QuadSkeleton {
    fn num_bones(&self) -> usize {
        self.bones.len()
    }

    fn get_bone(&self, index: usize) -> Option<&Bone> {
        self.bones.get(index)
    }

    fn bones(&self) -> &[Bone] {
        &self.bones
    }

    fn scale(&mut self, factor: f64) {
        for bone in &mut self.bones {
            bone.position *= factor;
        }
    }

    fn translate(&mut self, offset: Vector3) {
        for bone in &mut self.bones {
            bone.position += offset;
        }
    }
}

/// Esqueleto de caballo
#[derive(Debug, Clone)]
pub struct HorseSkeleton {
    bones: Vec<Bone>,
}

impl HorseSkeleton {
    /// Crea un nuevo esqueleto de caballo
    pub fn new() -> Self {
        // Similar a QuadSkeleton pero con proporciones de caballo
        let mut quad = QuadSkeleton::new();
        // Ajustar proporciones para caballo (cuello más largo, piernas más largas)
        for bone in quad.bones.iter_mut() {
            if bone.name.contains("neck") {
                bone.position += Vector3::new(0.0, 0.1, 0.1);
            }
            if bone.name.contains("head") {
                bone.position += Vector3::new(0.0, 0.15, 0.2);
            }
        }
        Self { bones: quad.bones }
    }
}

impl Default for HorseSkeleton {
    fn default() -> Self {
        Self::new()
    }
}

impl Skeleton for HorseSkeleton {
    fn num_bones(&self) -> usize {
        self.bones.len()
    }

    fn get_bone(&self, index: usize) -> Option<&Bone> {
        self.bones.get(index)
    }

    fn bones(&self) -> &[Bone] {
        &self.bones
    }

    fn scale(&mut self, factor: f64) {
        for bone in &mut self.bones {
            bone.position *= factor;
        }
    }

    fn translate(&mut self, offset: Vector3) {
        for bone in &mut self.bones {
            bone.position += offset;
        }
    }
}

/// Esqueleto de centauro (humano + caballo)
#[derive(Debug, Clone)]
pub struct CentaurSkeleton {
    bones: Vec<Bone>,
}

impl CentaurSkeleton {
    /// Crea un nuevo esqueleto de centauro
    pub fn new() -> Self {
        let mut bones = Vec::new();

        // Cuerpo de caballo (parte inferior)
        bones.push(Bone::new("hip", Vector3::new(0.0, 0.6, -0.3)));
        bones.push(Bone::with_parent("spine_horse", Vector3::new(0.0, 0.65, 0.0), 0));
        bones.push(Bone::with_parent("chest_horse", Vector3::new(0.0, 0.7, 0.3), 1));

        // Torso humano (arriba del cuerpo de caballo)
        bones.push(Bone::with_parent("pelvis_human", Vector3::new(0.0, 0.75, 0.35), 2));
        bones.push(Bone::with_parent("spine_human", Vector3::new(0.0, 0.9, 0.35), 3));
        bones.push(Bone::with_parent("chest_human", Vector3::new(0.0, 1.05, 0.35), 4));
        bones.push(Bone::with_parent("neck", Vector3::new(0.0, 1.15, 0.35), 5));
        bones.push(Bone::with_parent("head", Vector3::new(0.0, 1.25, 0.35), 6).as_leaf());

        // Brazos humanos
        bones.push(Bone::with_parent("shoulder_l", Vector3::new(-0.15, 1.0, 0.35), 5));
        bones.push(Bone::with_parent("elbow_l", Vector3::new(-0.3, 0.85, 0.35), 8));
        bones.push(Bone::with_parent("hand_l", Vector3::new(-0.4, 0.7, 0.35), 9).as_leaf());

        bones.push(Bone::with_parent("shoulder_r", Vector3::new(0.15, 1.0, 0.35), 5));
        bones.push(Bone::with_parent("elbow_r", Vector3::new(0.3, 0.85, 0.35), 11));
        bones.push(Bone::with_parent("hand_r", Vector3::new(0.4, 0.7, 0.35), 12).as_leaf());

        // Cola
        bones.push(Bone::with_parent("tail_base", Vector3::new(0.0, 0.55, -0.4), 0));
        bones.push(Bone::with_parent("tail_end", Vector3::new(0.0, 0.5, -0.55), 14).as_leaf());

        // Patas delanteras (caballo)
        bones.push(Bone::with_parent("shoulder_fl", Vector3::new(-0.15, 0.6, 0.25), 2));
        bones.push(Bone::with_parent("elbow_fl", Vector3::new(-0.15, 0.35, 0.25), 16));
        bones.push(Bone::with_parent("hoof_fl", Vector3::new(-0.15, 0.0, 0.25), 17).as_leaf());

        bones.push(Bone::with_parent("shoulder_fr", Vector3::new(0.15, 0.6, 0.25), 2));
        bones.push(Bone::with_parent("elbow_fr", Vector3::new(0.15, 0.35, 0.25), 19));
        bones.push(Bone::with_parent("hoof_fr", Vector3::new(0.15, 0.0, 0.25), 20).as_leaf());

        // Patas traseras (caballo)
        bones.push(Bone::with_parent("hip_bl", Vector3::new(-0.15, 0.55, -0.25), 0));
        bones.push(Bone::with_parent("knee_bl", Vector3::new(-0.15, 0.3, -0.25), 22));
        bones.push(Bone::with_parent("hoof_bl", Vector3::new(-0.15, 0.0, -0.25), 23).as_leaf());

        bones.push(Bone::with_parent("hip_br", Vector3::new(0.15, 0.55, -0.25), 0));
        bones.push(Bone::with_parent("knee_br", Vector3::new(0.15, 0.3, -0.25), 25));
        bones.push(Bone::with_parent("hoof_br", Vector3::new(0.15, 0.0, -0.25), 26).as_leaf());

        Self { bones }
    }
}

impl Default for CentaurSkeleton {
    fn default() -> Self {
        Self::new()
    }
}

impl Skeleton for CentaurSkeleton {
    fn num_bones(&self) -> usize {
        self.bones.len()
    }

    fn get_bone(&self, index: usize) -> Option<&Bone> {
        self.bones.get(index)
    }

    fn bones(&self) -> &[Bone] {
        &self.bones
    }

    fn scale(&mut self, factor: f64) {
        for bone in &mut self.bones {
            bone.position *= factor;
        }
    }

    fn translate(&mut self, offset: Vector3) {
        for bone in &mut self.bones {
            bone.position += offset;
        }
    }
}

/// Esqueleto de ave (22 huesos)
#[derive(Debug, Clone)]
pub struct BirdSkeleton {
    bones: Vec<Bone>,
}

impl BirdSkeleton {
    /// Crea un nuevo esqueleto de ave
    pub fn new() -> Self {
        let mut bones = Vec::new();

        // Raíz (pelvis)
        bones.push(Bone::new("pelvis", Vector3::new(0.0, 0.4, 0.0)));

        // Columna
        bones.push(Bone::with_parent("spine", Vector3::new(0.0, 0.45, 0.05), 0));
        bones.push(Bone::with_parent("chest", Vector3::new(0.0, 0.5, 0.1), 1));
        bones.push(Bone::with_parent("neck", Vector3::new(0.0, 0.55, 0.15), 2));
        bones.push(Bone::with_parent("head", Vector3::new(0.0, 0.6, 0.25), 3).as_leaf());

        // Cola
        bones.push(Bone::with_parent("tail_base", Vector3::new(0.0, 0.38, -0.1), 0));
        bones.push(Bone::with_parent("tail_mid", Vector3::new(0.0, 0.35, -0.2), 5));
        bones.push(Bone::with_parent("tail_tip", Vector3::new(0.0, 0.32, -0.3), 6).as_leaf());

        // Ala izquierda
        bones.push(Bone::with_parent("humerus_l", Vector3::new(-0.1, 0.48, 0.08), 2));
        bones.push(Bone::with_parent("radius_l", Vector3::new(-0.25, 0.5, 0.05), 8));
        bones.push(Bone::with_parent("carpus_l", Vector3::new(-0.4, 0.52, 0.02), 9));
        bones.push(Bone::with_parent("digits_l", Vector3::new(-0.55, 0.53, 0.0), 10).as_leaf());

        // Ala derecha
        bones.push(Bone::with_parent("humerus_r", Vector3::new(0.1, 0.48, 0.08), 2));
        bones.push(Bone::with_parent("radius_r", Vector3::new(0.25, 0.5, 0.05), 12));
        bones.push(Bone::with_parent("carpus_r", Vector3::new(0.4, 0.52, 0.02), 13));
        bones.push(Bone::with_parent("digits_r", Vector3::new(0.55, 0.53, 0.0), 14).as_leaf());

        // Pata izquierda
        bones.push(Bone::with_parent("femur_l", Vector3::new(-0.05, 0.35, 0.0), 0));
        bones.push(Bone::with_parent("tibiotarsus_l", Vector3::new(-0.05, 0.2, 0.0), 16));
        bones.push(Bone::with_parent("tarsometatarsus_l", Vector3::new(-0.05, 0.08, 0.0), 17));
        bones.push(Bone::with_parent("foot_l", Vector3::new(-0.05, 0.0, 0.03), 18).as_leaf());

        // Pata derecha
        bones.push(Bone::with_parent("femur_r", Vector3::new(0.05, 0.35, 0.0), 0));
        bones.push(Bone::with_parent("tibiotarsus_r", Vector3::new(0.05, 0.2, 0.0), 20));
        bones.push(Bone::with_parent("tarsometatarsus_r", Vector3::new(0.05, 0.08, 0.0), 21));
        bones.push(Bone::with_parent("foot_r", Vector3::new(0.05, 0.0, 0.03), 22).as_leaf());

        Self { bones }
    }
}

impl Default for BirdSkeleton {
    fn default() -> Self {
        Self::new()
    }
}

impl Skeleton for BirdSkeleton {
    fn num_bones(&self) -> usize {
        self.bones.len()
    }

    fn get_bone(&self, index: usize) -> Option<&Bone> {
        self.bones.get(index)
    }

    fn bones(&self) -> &[Bone] {
        &self.bones
    }

    fn scale(&mut self, factor: f64) {
        for bone in &mut self.bones {
            bone.position *= factor;
        }
    }

    fn translate(&mut self, offset: Vector3) {
        for bone in &mut self.bones {
            bone.position += offset;
        }
    }
}

/// Esqueleto de araña (26 huesos)
#[derive(Debug, Clone)]
pub struct SpiderSkeleton {
    bones: Vec<Bone>,
}

impl SpiderSkeleton {
    /// Crea un nuevo esqueleto de araña
    pub fn new() -> Self {
        let mut bones = Vec::new();

        // Cefalotórax (raíz)
        bones.push(Bone::new("cephalothorax", Vector3::new(0.0, 0.15, 0.0)));

        // Abdomen
        bones.push(Bone::with_parent("abdomen", Vector3::new(0.0, 0.15, -0.2), 0).as_leaf());

        // Pedipalpos
        bones.push(Bone::with_parent("pedipalp_l", Vector3::new(-0.05, 0.12, 0.1), 0).as_leaf());
        bones.push(Bone::with_parent("pedipalp_r", Vector3::new(0.05, 0.12, 0.1), 0).as_leaf());

        // 8 patas (4 a cada lado)
        // Pata 1 izquierda
        bones.push(Bone::with_parent("leg1_coxa_l", Vector3::new(-0.08, 0.12, 0.05), 0));
        bones.push(Bone::with_parent("leg1_femur_l", Vector3::new(-0.15, 0.1, 0.08), 4));
        bones.push(Bone::with_parent("leg1_patella_l", Vector3::new(-0.2, 0.05, 0.1), 5));
        bones.push(Bone::with_parent("leg1_tibia_l", Vector3::new(-0.25, 0.02, 0.12), 6));
        bones.push(Bone::with_parent("leg1_tarsus_l", Vector3::new(-0.3, 0.0, 0.15), 7).as_leaf());

        // Pata 1 derecha
        bones.push(Bone::with_parent("leg1_coxa_r", Vector3::new(0.08, 0.12, 0.05), 0));
        bones.push(Bone::with_parent("leg1_femur_r", Vector3::new(0.15, 0.1, 0.08), 9));
        bones.push(Bone::with_parent("leg1_patella_r", Vector3::new(0.2, 0.05, 0.1), 10));
        bones.push(Bone::with_parent("leg1_tibia_r", Vector3::new(0.25, 0.02, 0.12), 11));
        bones.push(Bone::with_parent("leg1_tarsus_r", Vector3::new(0.3, 0.0, 0.15), 12).as_leaf());

        // Pata 2 izquierda
        bones.push(Bone::with_parent("leg2_coxa_l", Vector3::new(-0.08, 0.12, 0.0), 0));
        bones.push(Bone::with_parent("leg2_femur_l", Vector3::new(-0.18, 0.08, 0.0), 14));
        bones.push(Bone::with_parent("leg2_tarsus_l", Vector3::new(-0.28, 0.0, 0.0), 15).as_leaf());

        // Pata 2 derecha
        bones.push(Bone::with_parent("leg2_coxa_r", Vector3::new(0.08, 0.12, 0.0), 0));
        bones.push(Bone::with_parent("leg2_femur_r", Vector3::new(0.18, 0.08, 0.0), 17));
        bones.push(Bone::with_parent("leg2_tarsus_r", Vector3::new(0.28, 0.0, 0.0), 18).as_leaf());

        // Pata 3 izquierda
        bones.push(Bone::with_parent("leg3_coxa_l", Vector3::new(-0.08, 0.12, -0.05), 0));
        bones.push(Bone::with_parent("leg3_femur_l", Vector3::new(-0.18, 0.08, -0.08), 20));
        bones.push(Bone::with_parent("leg3_tarsus_l", Vector3::new(-0.28, 0.0, -0.1), 21).as_leaf());

        // Pata 3 derecha
        bones.push(Bone::with_parent("leg3_coxa_r", Vector3::new(0.08, 0.12, -0.05), 0));
        bones.push(Bone::with_parent("leg3_femur_r", Vector3::new(0.18, 0.08, -0.08), 23));
        bones.push(Bone::with_parent("leg3_tarsus_r", Vector3::new(0.28, 0.0, -0.1), 24).as_leaf());

        Self { bones }
    }
}

impl Default for SpiderSkeleton {
    fn default() -> Self {
        Self::new()
    }
}

impl Skeleton for SpiderSkeleton {
    fn num_bones(&self) -> usize {
        self.bones.len()
    }

    fn get_bone(&self, index: usize) -> Option<&Bone> {
        self.bones.get(index)
    }

    fn bones(&self) -> &[Bone] {
        &self.bones
    }

    fn scale(&mut self, factor: f64) {
        for bone in &mut self.bones {
            bone.position *= factor;
        }
    }

    fn translate(&mut self, offset: Vector3) {
        for bone in &mut self.bones {
            bone.position += offset;
        }
    }
}

/// Esqueleto de serpiente (configurable)
#[derive(Debug, Clone)]
pub struct SerpentSkeleton {
    bones: Vec<Bone>,
}

impl SerpentSkeleton {
    /// Crea un nuevo esqueleto de serpiente con 14 segmentos (16 huesos total)
    pub fn new() -> Self {
        Self::with_segments(14)
    }

    /// Crea un esqueleto de serpiente con n segmentos
    ///
    /// Estructura: head → vertebra_1 → vertebra_2 → ... → vertebra_n → tail_tip
    pub fn with_segments(n: usize) -> Self {
        let mut bones = Vec::new();

        // Cabeza (raíz)
        bones.push(Bone::new("head", Vector3::new(0.0, 0.1, 0.5)));

        // Vértebras
        let total_length = 1.0;
        let segment_length = total_length / (n + 1) as f64;

        for i in 0..n {
            let z = 0.5 - segment_length * (i + 1) as f64;
            let name = format!("vertebra_{}", i + 1);
            bones.push(Bone::with_parent(name, Vector3::new(0.0, 0.1, z), i));
        }

        // Cola
        bones.push(Bone::with_parent("tail_tip", Vector3::new(0.0, 0.1, -0.5), n).as_leaf());

        Self { bones }
    }

    /// Número de segmentos (excluyendo cabeza y cola)
    pub fn num_segments(&self) -> usize {
        self.bones.len().saturating_sub(2)
    }
}

impl Default for SerpentSkeleton {
    fn default() -> Self {
        Self::new()
    }
}

impl Skeleton for SerpentSkeleton {
    fn num_bones(&self) -> usize {
        self.bones.len()
    }

    fn get_bone(&self, index: usize) -> Option<&Bone> {
        self.bones.get(index)
    }

    fn bones(&self) -> &[Bone] {
        &self.bones
    }

    fn scale(&mut self, factor: f64) {
        for bone in &mut self.bones {
            bone.position *= factor;
        }
    }

    fn translate(&mut self, offset: Vector3) {
        for bone in &mut self.bones {
            bone.position += offset;
        }
    }
}

/// Esqueleto mecánico/robot (24 huesos)
#[derive(Debug, Clone)]
pub struct MechSkeleton {
    bones: Vec<Bone>,
}

impl MechSkeleton {
    /// Crea un nuevo esqueleto mecánico
    pub fn new() -> Self {
        let mut bones = Vec::new();

        // Torso base (raíz)
        bones.push(Bone::new("torso_base", Vector3::new(0.0, 0.5, 0.0)));

        // Torso superior y cabeza
        bones.push(Bone::with_parent("torso_upper", Vector3::new(0.0, 0.7, 0.0), 0));
        bones.push(Bone::with_parent("head_mount", Vector3::new(0.0, 0.85, 0.0), 1));
        bones.push(Bone::with_parent("sensor_head", Vector3::new(0.0, 0.95, 0.0), 2).as_leaf());

        // Brazo izquierdo
        bones.push(Bone::with_parent("shoulder_joint_l", Vector3::new(-0.15, 0.75, 0.0), 1));
        bones.push(Bone::with_parent("upper_arm_l", Vector3::new(-0.25, 0.7, 0.0), 4));
        bones.push(Bone::with_parent("elbow_l", Vector3::new(-0.35, 0.55, 0.0), 5));
        bones.push(Bone::with_parent("forearm_l", Vector3::new(-0.4, 0.45, 0.0), 6));
        bones.push(Bone::with_parent("wrist_l", Vector3::new(-0.45, 0.35, 0.0), 7));
        bones.push(Bone::with_parent("hand_l", Vector3::new(-0.48, 0.28, 0.0), 8).as_leaf());

        // Brazo derecho
        bones.push(Bone::with_parent("shoulder_joint_r", Vector3::new(0.15, 0.75, 0.0), 1));
        bones.push(Bone::with_parent("upper_arm_r", Vector3::new(0.25, 0.7, 0.0), 10));
        bones.push(Bone::with_parent("elbow_r", Vector3::new(0.35, 0.55, 0.0), 11));
        bones.push(Bone::with_parent("forearm_r", Vector3::new(0.4, 0.45, 0.0), 12));
        bones.push(Bone::with_parent("wrist_r", Vector3::new(0.45, 0.35, 0.0), 13));
        bones.push(Bone::with_parent("hand_r", Vector3::new(0.48, 0.28, 0.0), 14).as_leaf());

        // Pierna izquierda
        bones.push(Bone::with_parent("hip_joint_l", Vector3::new(-0.1, 0.45, 0.0), 0));
        bones.push(Bone::with_parent("upper_leg_l", Vector3::new(-0.12, 0.35, 0.0), 16));
        bones.push(Bone::with_parent("knee_l", Vector3::new(-0.12, 0.22, 0.0), 17));
        bones.push(Bone::with_parent("lower_leg_l", Vector3::new(-0.12, 0.12, 0.0), 18));
        bones.push(Bone::with_parent("ankle_l", Vector3::new(-0.12, 0.05, 0.0), 19));
        bones.push(Bone::with_parent("foot_l", Vector3::new(-0.12, 0.0, 0.05), 20).as_leaf());

        // Pierna derecha
        bones.push(Bone::with_parent("hip_joint_r", Vector3::new(0.1, 0.45, 0.0), 0));
        bones.push(Bone::with_parent("upper_leg_r", Vector3::new(0.12, 0.35, 0.0), 22));
        bones.push(Bone::with_parent("knee_r", Vector3::new(0.12, 0.22, 0.0), 23));
        bones.push(Bone::with_parent("lower_leg_r", Vector3::new(0.12, 0.12, 0.0), 24));
        bones.push(Bone::with_parent("ankle_r", Vector3::new(0.12, 0.05, 0.0), 25));
        bones.push(Bone::with_parent("foot_r", Vector3::new(0.12, 0.0, 0.05), 26).as_leaf());

        Self { bones }
    }
}

impl Default for MechSkeleton {
    fn default() -> Self {
        Self::new()
    }
}

impl Skeleton for MechSkeleton {
    fn num_bones(&self) -> usize {
        self.bones.len()
    }

    fn get_bone(&self, index: usize) -> Option<&Bone> {
        self.bones.get(index)
    }

    fn bones(&self) -> &[Bone] {
        &self.bones
    }

    fn scale(&mut self, factor: f64) {
        for bone in &mut self.bones {
            bone.position *= factor;
        }
    }

    fn translate(&mut self, offset: Vector3) {
        for bone in &mut self.bones {
            bone.position += offset;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_human_skeleton() {
        let skel = HumanSkeleton::new();
        assert!(skel.num_bones() > 10);
        assert!(skel.get_bone(0).is_some());
    }

    #[test]
    fn test_quad_skeleton() {
        let skel = QuadSkeleton::new();
        assert!(skel.num_bones() > 10);
    }

    #[test]
    fn test_centaur_skeleton() {
        let skel = CentaurSkeleton::new();
        assert!(skel.num_bones() > 20);
    }

    #[test]
    fn test_skeleton_hierarchy() {
        let skel = HumanSkeleton::new();
        // Verificar que hay una cadena desde la cabeza hasta la raíz
        let head_idx = skel.bones()
            .iter()
            .position(|b| b.name == "head")
            .unwrap();
        assert!(skel.get_depth(head_idx) > 2);
    }

    #[test]
    fn test_bird_skeleton() {
        let skel = BirdSkeleton::new();
        assert_eq!(skel.num_bones(), 24);
        // Verificar estructura: pelvis → spine → chest → neck → head
        let head_idx = skel.bones().iter().position(|b| b.name == "head").unwrap();
        assert_eq!(skel.get_depth(head_idx), 4);
    }

    #[test]
    fn test_spider_skeleton() {
        let skel = SpiderSkeleton::new();
        assert_eq!(skel.num_bones(), 26);
        // Verificar raíz
        assert_eq!(skel.get_bone(0).unwrap().name, "cephalothorax");
    }

    #[test]
    fn test_serpent_skeleton() {
        let skel = SerpentSkeleton::new();
        assert_eq!(skel.num_bones(), 16); // head + 14 vertebrae + tail_tip
        assert_eq!(skel.num_segments(), 14);

        // Con diferentes segmentos
        let skel5 = SerpentSkeleton::with_segments(5);
        assert_eq!(skel5.num_bones(), 7);
        assert_eq!(skel5.num_segments(), 5);
    }

    #[test]
    fn test_mech_skeleton() {
        let skel = MechSkeleton::new();
        assert_eq!(skel.num_bones(), 28);
        // Verificar raíz
        assert_eq!(skel.get_bone(0).unwrap().name, "torso_base");
    }
}
