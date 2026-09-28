//! Esqueletos por forma de cuerpo más apéndices.
//!
//! Un [`BodyPlan`] combina una forma base (bípedo, cuadrúpedo, radial, pez…)
//! con apéndices de cantidad de segmentos configurable (trompa, cola, cuello
//! largo, alas, tentáculos…) y genera un [`BasicSkeleton`] plantilla: Y arriba,
//! mirando a +Z, alto del orden de 1. [`BodyPlan::variant`] da recetas con
//! nombre (elefante, pulpo, dragón…).
//!
//! Convenciones de nombres que usa el ajuste automático y la edición: la
//! cabeza empieza con `head`, la cola con `tail`, y los lados terminan en
//! `_l` / `_r` para el espejo.

use crate::{BasicSkeleton, Bone};
use pinocchio_math::{Real, Vector3};

/// Forma base del cuerpo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyShape {
    /// Humanoides, primates.
    Biped,
    /// Bípedo sobre los dedos, cuerpo horizontal: dinosaurios, aves, canguro.
    DigitigradeBiped,
    /// Cuatro patas: perro, felino, caballo, elefante.
    Quadruped,
    /// Cuerpo central con brazos alrededor: pulpo, calamar, estrella de mar.
    Radial,
    /// Eje horizontal con aletas: pez, tiburón, delfín.
    Fish,
    /// Tórax con pares de patas: insecto, araña, cangrejo, escorpión.
    Arthropod,
    /// Cadena sin extremidades: serpiente, gusano, anguila.
    Serpent,
}

/// Forma base y apéndices. Los campos que no aplican a la forma se ignoran;
/// un apéndice con 0 segmentos no se agrega.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyPlan {
    pub shape: BodyShape,
    /// Segmentos del cuello (1 = cuello corto normal).
    pub neck: usize,
    /// Segmentos de la cola (en la serpiente, del cuerpo entero).
    pub tail: usize,
    /// Segmentos de la trompa (desde la punta de la cabeza).
    pub trunk: usize,
    /// Segmentos de cada oreja móvil.
    pub ears: usize,
    /// Segmentos de cada ala.
    pub wings: usize,
    /// Brazos del cuerpo radial, o pares de patas del artrópodo.
    pub limbs: usize,
    /// Segmentos de cada brazo radial.
    pub limb_segments: usize,
    /// Aletas pectorales y dorsal (pez).
    pub fins: bool,
    /// Pinzas (artrópodo).
    pub pincers: bool,
    /// Segmentos de cada antena.
    pub antennae: usize,
}

impl BodyPlan {
    /// Plan sin apéndices extra para una forma.
    pub fn new(shape: BodyShape) -> Self {
        let (tail, limbs, limb_segments) = match shape {
            BodyShape::Biped => (0, 0, 0),
            BodyShape::DigitigradeBiped => (5, 0, 0),
            BodyShape::Quadruped => (2, 0, 0),
            BodyShape::Radial => (0, 8, 5),
            BodyShape::Fish => (4, 0, 0),
            BodyShape::Arthropod => (0, 4, 3),
            BodyShape::Serpent => (16, 0, 0),
        };
        Self {
            shape,
            neck: 1,
            tail,
            trunk: 0,
            ears: 0,
            wings: 0,
            limbs,
            limb_segments,
            fins: shape == BodyShape::Fish,
            pincers: false,
            antennae: 0,
        }
    }

    /// Variantes con nombre: `(id, nombre, descripción)`.
    pub fn variants() -> &'static [(&'static str, &'static str, &'static str)] {
        &[
            ("elephant", "Elefante", "Cuadrúpedo con trompa, orejas y cola"),
            ("giraffe", "Jirafa", "Cuadrúpedo de cuello largo"),
            ("dog", "Perro / felino", "Cuadrúpedo con cola larga y orejas"),
            ("dragon", "Dragón", "Cuadrúpedo con alas, cuello y cola largos"),
            ("octopus", "Pulpo", "Cuerpo radial con 8 tentáculos"),
            ("squid", "Calamar", "Cuerpo radial con 10 brazos"),
            ("fish", "Pez", "Eje horizontal con aletas y cola"),
            ("dolphin", "Delfín / ballena", "Pez grande con cola larga"),
            ("crab", "Cangrejo", "Artrópodo de 8 patas con pinzas"),
            ("insect", "Insecto", "Artrópodo de 6 patas con antenas y alas"),
            ("scorpion", "Escorpión", "Artrópodo con pinzas y cola"),
            ("trex", "Dinosaurio bípedo", "Bípedo digitígrado con cola larga y brazos cortos"),
            ("bird_walker", "Ave", "Bípedo digitígrado con alas"),
            ("snake", "Serpiente", "Cadena de 16 segmentos"),
        ]
    }

    /// Receta de una variante con nombre.
    pub fn variant(id: &str) -> Option<Self> {
        use BodyShape::*;
        let with = |shape, f: fn(&mut Self)| {
            let mut plan = Self::new(shape);
            f(&mut plan);
            Some(plan)
        };
        match id {
            "elephant" => with(Quadruped, |p| {
                p.trunk = 8;
                p.ears = 1;
                p.tail = 3;
            }),
            "giraffe" => with(Quadruped, |p| {
                p.neck = 6;
                p.tail = 3;
            }),
            "dog" => with(Quadruped, |p| {
                p.tail = 4;
                p.ears = 1;
            }),
            "dragon" => with(Quadruped, |p| {
                p.neck = 5;
                p.tail = 8;
                p.wings = 3;
            }),
            "octopus" => with(Radial, |p| {
                p.limbs = 8;
                p.limb_segments = 6;
            }),
            "squid" => with(Radial, |p| {
                p.limbs = 10;
                p.limb_segments = 5;
            }),
            "fish" => with(Fish, |p| p.tail = 4),
            "dolphin" => with(Fish, |p| p.tail = 6),
            "crab" => with(Arthropod, |p| {
                p.limbs = 4;
                p.pincers = true;
            }),
            "insect" => with(Arthropod, |p| {
                p.limbs = 3;
                p.antennae = 2;
                p.wings = 2;
            }),
            "scorpion" => with(Arthropod, |p| {
                p.limbs = 4;
                p.pincers = true;
                p.tail = 6;
            }),
            "trex" => with(DigitigradeBiped, |p| {
                p.tail = 6;
                p.neck = 2;
            }),
            "bird_walker" => with(DigitigradeBiped, |p| {
                p.tail = 2;
                p.wings = 3;
            }),
            "snake" => with(Serpent, |p| p.tail = 16),
            _ => None,
        }
    }

    /// Esqueleto plantilla del plan.
    pub fn build(&self) -> BasicSkeleton {
        let mut b = Builder::default();
        match self.shape {
            BodyShape::Biped => self.biped(&mut b),
            BodyShape::DigitigradeBiped => self.digitigrade(&mut b),
            BodyShape::Quadruped => self.quadruped(&mut b),
            BodyShape::Radial => self.radial(&mut b),
            BodyShape::Fish => self.fish(&mut b),
            BodyShape::Arthropod => self.arthropod(&mut b),
            BodyShape::Serpent => self.serpent(&mut b),
        }
        BasicSkeleton::from_bones(b.bones)
    }

    /// Cuello desde `chest` hasta la cabeza, y apéndices de la cabeza.
    /// `base` y `tip` son el arranque y la punta del cuello; la cabeza va de
    /// la punta del cuello a `head`.
    fn neck_and_head(&self, b: &mut Builder, chest: usize, tip: Vector3, head: Vector3, bend: Vector3) {
        let start = b.position(chest);
        let points = curve(start, tip, bend, self.neck.max(1));
        let neck = b.chain("neck", chest, &points, false);
        let head_bone = b.bone("head", head, neck);
        let forward = (head - tip).try_normalize().unwrap_or(Vector3::unit_z());
        let size = head.distance(&tip);
        if self.trunk > 0 {
            // Cuelga hasta cerca del suelo, un poco por delante de la cabeza
            let end = v(head.x(), 0.12, head.z() + 0.1);
            let control = head + forward * (1.5 * size) - Vector3::unit_y() * (0.3 * size);
            let points = curve(head, end, control, self.trunk);
            b.chain("trunk", head_bone, &points, true);
        }
        if self.antennae > 0 {
            for (s, side) in SIDES {
                let root = head + Vector3::new(0.3 * s * size, 0.3 * size, 0.0);
                let end = root + forward * (1.5 * size) + Vector3::new(0.8 * s * size, 1.2 * size, 0.0);
                let points = curve(root, end, root + Vector3::new(0.2 * s * size, 1.0 * size, 0.2 * size), self.antennae);
                b.chain_sided("antenna", head_bone, &points, side);
            }
        } else {
            b.mark_leaf(head_bone);
        }
        if self.ears > 0 {
            // Colgando de la base de la cabeza, a los lados
            for (s, side) in SIDES {
                let root = tip + Vector3::new(0.45 * s * size, 0.25 * size, 0.0);
                let end = root + Vector3::new(0.7 * s * size, -0.5 * size, -0.3 * size);
                let points = curve(root, end, root + Vector3::new(0.5 * s * size, 0.0, -0.2 * size), self.ears + 1);
                b.chain_sided("ear", neck, &points, side);
            }
        }
    }

    /// Cola desde `from`, hacia `end` curvándose por `bend`.
    fn add_tail(&self, b: &mut Builder, from: usize, end: Vector3, bend: Vector3) {
        if self.tail == 0 {
            return;
        }
        let start = b.position(from);
        let points = curve(start, end, bend, self.tail);
        b.chain("tail", from, &points, true);
    }

    /// Alas desde `from`, hacia afuera y arriba.
    fn add_wings(&self, b: &mut Builder, from: usize, root: Vector3, span: Real) {
        if self.wings == 0 {
            return;
        }
        for (s, side) in SIDES {
            let start = Vector3::new(root.x() * s, root.y(), root.z());
            let end = start + Vector3::new(s * span, 0.35 * span, -0.2 * span);
            let points = curve(start, end, start + Vector3::new(s * 0.5 * span, 0.5 * span, 0.0), self.wings + 1);
            b.chain_sided("wing", from, &points, side);
        }
    }

    fn biped(&self, b: &mut Builder) {
        let pelvis = b.root("pelvis", v(0.0, 0.5, 0.0));
        let spine = b.bone("spine", v(0.0, 0.65, 0.0), pelvis);
        let chest = b.bone("chest", v(0.0, 0.8, 0.0), spine);
        self.neck_and_head(b, chest, v(0.0, 0.9, 0.0), v(0.0, 1.0, 0.0), v(0.0, 0.85, 0.01));
        for (s, side) in SIDES {
            let shoulder = b.bone_sided("shoulder", v(0.1 * s, 0.85, 0.0), chest, side);
            let elbow = b.bone_sided("elbow", v(0.25 * s, 0.65, 0.0), shoulder, side);
            let wrist = b.bone_sided("wrist", v(0.35 * s, 0.5, 0.0), elbow, side);
            let hand = b.bone_sided("hand", v(0.4 * s, 0.45, 0.0), wrist, side);
            b.mark_leaf(hand);
            let hip = b.bone_sided("hip", v(0.1 * s, 0.45, 0.0), pelvis, side);
            let knee = b.bone_sided("knee", v(0.1 * s, 0.25, 0.0), hip, side);
            let ankle = b.bone_sided("ankle", v(0.1 * s, 0.05, 0.0), knee, side);
            let foot = b.bone_sided("foot", v(0.1 * s, 0.0, 0.05), ankle, side);
            b.mark_leaf(foot);
        }
        self.add_tail(b, pelvis, v(0.0, 0.2, -0.4), v(0.0, 0.35, -0.25));
        self.add_wings(b, chest, v(0.08, 0.82, -0.05), 0.5);
    }

    fn digitigrade(&self, b: &mut Builder) {
        let pelvis = b.root("pelvis", v(0.0, 0.6, 0.0));
        let spine = b.bone("spine", v(0.0, 0.66, 0.14), pelvis);
        let chest = b.bone("chest", v(0.0, 0.7, 0.26), spine);
        self.neck_and_head(b, chest, v(0.0, 0.88, 0.42), v(0.0, 0.92, 0.6), v(0.0, 0.84, 0.3));
        for (s, side) in SIDES {
            let hip = b.bone_sided("hip", v(0.12 * s, 0.55, 0.0), pelvis, side);
            let knee = b.bone_sided("knee", v(0.14 * s, 0.35, 0.1), hip, side);
            let ankle = b.bone_sided("ankle", v(0.14 * s, 0.12, -0.06), knee, side);
            let foot = b.bone_sided("foot", v(0.14 * s, 0.0, 0.1), ankle, side);
            b.mark_leaf(foot);
            if self.wings == 0 {
                let shoulder = b.bone_sided("shoulder", v(0.1 * s, 0.66, 0.3), chest, side);
                let elbow = b.bone_sided("elbow", v(0.14 * s, 0.58, 0.34), shoulder, side);
                let hand = b.bone_sided("hand", v(0.12 * s, 0.54, 0.42), elbow, side);
                b.mark_leaf(hand);
            }
        }
        self.add_tail(b, pelvis, v(0.0, 0.45, -0.9), v(0.0, 0.6, -0.45));
        self.add_wings(b, chest, v(0.1, 0.72, 0.22), 0.6);
    }

    fn quadruped(&self, b: &mut Builder) {
        let hip = b.root("hip", v(0.0, 0.5, -0.3));
        let spine = b.bone("spine", v(0.0, 0.55, 0.0), hip);
        let chest = b.bone("chest", v(0.0, 0.55, 0.3), spine);
        // Cuello largo: sube más
        let rise = if self.neck > 1 { 0.12 * self.neck as Real } else { 0.05 };
        let neck_tip = v(0.0, 0.6 + rise, 0.4 + 0.02 * self.neck as Real);
        let head = neck_tip + v(0.0, 0.02, 0.14);
        self.neck_and_head(b, chest, neck_tip, head, v(0.0, 0.55 + 0.6 * rise, 0.42));
        for (s, side) in SIDES {
            let shoulder = b.bone(side_name("shoulder", side), v(0.15 * s, 0.5, 0.25), chest);
            let elbow = b.bone(side_name("elbow", side), v(0.15 * s, 0.28, 0.27), shoulder);
            let paw = b.bone(if s < 0.0 { "paw_fl" } else { "paw_fr" }, v(0.15 * s, 0.0, 0.25), elbow);
            b.mark_leaf(paw);
            let hip_side = b.bone(side_name("hip", side), v(0.15 * s, 0.45, -0.28), hip);
            let knee = b.bone(side_name("knee", side), v(0.15 * s, 0.25, -0.32), hip_side);
            let paw = b.bone(if s < 0.0 { "paw_bl" } else { "paw_br" }, v(0.15 * s, 0.0, -0.28), knee);
            b.mark_leaf(paw);
        }
        let length = 0.15 + 0.08 * self.tail as Real;
        self.add_tail(b, hip, v(0.0, 0.45 - 0.4 * length, -0.35 - length), v(0.0, 0.5, -0.35 - 0.6 * length));
        self.add_wings(b, chest, v(0.12, 0.6, 0.22), 0.8);
    }

    fn radial(&self, b: &mut Builder) {
        let body = b.root("body", v(0.0, 0.35, 0.0));
        let head = b.bone("head", v(0.0, 1.0, 0.0), body);
        b.mark_leaf(head);
        let n = self.limbs.max(1);
        let segments = self.limb_segments.max(1);
        let arm = |angle: Real| {
            let dir = v(angle.sin(), 0.0, angle.cos());
            curve(v(0.0, 0.3, 0.0) + dir * 0.12, dir * 0.95 + v(0.0, 0.02, 0.0), dir * 0.5 + v(0.0, 0.05, 0.0), segments)
        };
        // Pares simétricos respecto del plano YZ (espejo _l / _r); si la
        // cantidad es impar, uno va al frente
        let pairs = n / 2;
        for i in 0..pairs {
            let angle = std::f64::consts::PI as Real * (i as Real + 0.5) / pairs as Real;
            for (s, side) in SIDES {
                b.chain_sided(&format!("arm{}", i + 1), body, &arm(s * angle), side);
            }
        }
        if n % 2 == 1 {
            b.chain("arm0", body, &arm(0.0), true);
        }
    }

    fn fish(&self, b: &mut Builder) {
        let body = b.root("body", v(0.0, 0.5, 0.05));
        let head = b.bone("head", v(0.0, 0.5, 0.5), body);
        b.mark_leaf(head);
        let length = 0.35 + 0.05 * self.tail as Real;
        self.add_tail(b, body, v(0.0, 0.5, 0.05 - length), v(0.0, 0.5, 0.05 - 0.5 * length));
        if self.fins {
            for (s, side) in SIDES {
                let root = b.bone_sided("pectoral", v(0.1 * s, 0.44, 0.25), body, side);
                let tip = b.bone_sided("pectoral_tip", v(0.3 * s, 0.36, 0.12), root, side);
                b.mark_leaf(tip);
            }
            let dorsal = b.bone("dorsal", v(0.0, 0.62, 0.08), body);
            let tip = b.bone("dorsal_tip", v(0.0, 0.8, -0.05), dorsal);
            b.mark_leaf(tip);
        }
    }

    fn arthropod(&self, b: &mut Builder) {
        let thorax = b.root("thorax", v(0.0, 0.3, 0.0));
        let head_tip = v(0.0, 0.32, 0.3);
        // Cabeza sin cuello: el "cuello" es un tramo mínimo del tórax
        let head = b.bone("head", head_tip, thorax);
        if self.antennae > 0 {
            for (s, side) in SIDES {
                let root = head_tip + v(0.05 * s, 0.03, 0.0);
                let end = root + v(0.2 * s, 0.25, 0.3);
                let points = curve(root, end, root + v(0.05 * s, 0.2, 0.1), self.antennae);
                b.chain_sided("antenna", head, &points, side);
            }
        } else {
            b.mark_leaf(head);
        }
        let pairs = self.limbs.max(1);
        let segments = self.limb_segments.max(2);
        for i in 0..pairs {
            let z = if pairs == 1 { 0.0 } else { 0.15 - 0.35 * i as Real / (pairs - 1) as Real };
            for (s, side) in SIDES {
                let start = v(0.12 * s, 0.3, z);
                let points = curve(start, v(0.55 * s, 0.0, z * 1.4), v(0.4 * s, 0.5, z * 1.2), segments);
                b.chain_sided(&format!("leg{}", i + 1), thorax, &points, side);
            }
        }
        if self.pincers {
            for (s, side) in SIDES {
                let points = [v(0.28 * s, 0.32, 0.38), v(0.3 * s, 0.3, 0.55), v(0.2 * s, 0.3, 0.68)];
                b.chain_sided("pincer", thorax, &points, side);
            }
        }
        if self.tail > 0 {
            // Cola de escorpión: hacia atrás y por arriba
            self.add_tail(b, thorax, v(0.0, 0.75, -0.25), v(0.0, 0.5, -0.6));
        } else {
            let abdomen = b.bone("abdomen", v(0.0, 0.28, -0.35), thorax);
            b.mark_leaf(abdomen);
        }
        self.add_wings(b, thorax, v(0.06, 0.36, 0.05), 0.5);
    }

    fn serpent(&self, b: &mut Builder) {
        let n = self.tail.max(2);
        let head = b.root("head", v(0.0, 0.05, 0.5));
        let points: Vec<Vector3> = (1..=n).map(|i| v(0.0, 0.05, 0.5 - i as Real / n as Real)).collect();
        b.chain("tail", head, &points, true);
    }
}

const SIDES: [(Real, &str); 2] = [(-1.0, "_l"), (1.0, "_r")];

fn v(x: Real, y: Real, z: Real) -> Vector3 {
    Vector3::new(x, y, z)
}

fn side_name(base: &str, side: &str) -> String {
    format!("{base}{side}")
}

/// `n` puntos (sin el inicio) sobre una curva cuadrática de `start` a `end`
/// con punto de control `control`.
fn curve(start: Vector3, end: Vector3, control: Vector3, n: usize) -> Vec<Vector3> {
    (1..=n.max(1))
        .map(|i| {
            let t = i as Real / n.max(1) as Real;
            start * ((1.0 - t) * (1.0 - t)) + control * (2.0 * t * (1.0 - t)) + end * (t * t)
        })
        .collect()
}

#[derive(Default)]
struct Builder {
    bones: Vec<Bone>,
}

impl Builder {
    fn root(&mut self, name: &str, p: Vector3) -> usize {
        self.bones.push(Bone::new(name, p));
        self.bones.len() - 1
    }

    fn bone(&mut self, name: impl Into<String>, p: Vector3, parent: usize) -> usize {
        self.bones.push(Bone::with_parent(name, p, parent));
        self.bones.len() - 1
    }

    fn bone_sided(&mut self, base: &str, p: Vector3, parent: usize, side: &str) -> usize {
        self.bone(side_name(base, side), p, parent)
    }

    fn position(&self, bone: usize) -> Vector3 {
        self.bones[bone].position
    }

    fn mark_leaf(&mut self, bone: usize) {
        self.bones[bone].is_leaf = true;
    }

    /// Cadena `prefix_1 … prefix_n` desde `parent`; la última se llama
    /// `prefix_tip` y es hoja si `leaf`. Devuelve el último hueso.
    fn chain(&mut self, prefix: &str, parent: usize, points: &[Vector3], leaf: bool) -> usize {
        let mut last = parent;
        for (i, &p) in points.iter().enumerate() {
            let name = if i + 1 == points.len() && leaf { format!("{prefix}_tip") } else { format!("{prefix}_{}", i + 1) };
            last = self.bone(name, p, last);
        }
        if leaf {
            self.mark_leaf(last);
        }
        last
    }

    /// Como [`Builder::chain`], con sufijo de lado en cada hueso (siempre hoja al final).
    fn chain_sided(&mut self, prefix: &str, parent: usize, points: &[Vector3], side: &str) -> usize {
        let mut last = parent;
        for (i, &p) in points.iter().enumerate() {
            let name = if i + 1 == points.len() { format!("{prefix}_tip{side}") } else { format!("{prefix}_{}{side}", i + 1) };
            last = self.bone(name, p, last);
        }
        self.mark_leaf(last);
        last
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{mirror_pairs, Skeleton};

    fn names(s: &BasicSkeleton) -> Vec<&str> {
        s.bones().iter().map(|b| b.name.as_str()).collect()
    }

    #[test]
    fn every_variant_builds_a_valid_tree() {
        for (id, _, _) in BodyPlan::variants() {
            let skeleton = BodyPlan::variant(id).unwrap().build();
            assert!(skeleton.num_bones() >= 5, "{id}");
            let roots = (0..skeleton.num_bones()).filter(|&b| skeleton.get_parent(b).is_none()).count();
            assert_eq!(roots, 1, "{id}: una sola raíz");
            for (i, bone) in skeleton.bones().iter().enumerate() {
                if let Some(p) = bone.parent {
                    assert!(p < i, "{id}: {} antes que su padre", bone.name);
                }
            }
            let mut unique = names(&skeleton);
            unique.sort();
            unique.dedup();
            assert_eq!(unique.len(), skeleton.num_bones(), "{id}: nombres repetidos");
        }
    }

    #[test]
    fn appendages_add_segments() {
        let elephant = BodyPlan::variant("elephant").unwrap().build();
        let names = names(&elephant);
        assert_eq!(names.iter().filter(|n| n.starts_with("trunk")).count(), 8);
        assert!(names.contains(&"trunk_tip") && names.contains(&"ear_tip_l") && names.contains(&"tail_tip"));
        let mut long = BodyPlan::variant("giraffe").unwrap();
        long.neck = 7;
        assert_eq!(long.build().bones().iter().filter(|b| b.name.starts_with("neck")).count(), 7);
    }

    #[test]
    fn sides_are_mirrored() {
        for id in ["elephant", "octopus", "crab", "dragon", "fish"] {
            let skeleton = BodyPlan::variant(id).unwrap().build();
            let pairs = mirror_pairs(&skeleton);
            for (i, pair) in pairs.iter().enumerate() {
                let Some(j) = *pair else { continue };
                let (a, b) = (skeleton.bones()[i].position, skeleton.bones()[j].position);
                assert!((a.x() + b.x()).abs() < 1e-9 && (a.y() - b.y()).abs() < 1e-9, "{id}: {} no es espejo", skeleton.bones()[i].name);
            }
            assert!(pairs.iter().filter(|p| p.is_some()).count() >= 4, "{id}: tiene pares");
        }
    }

    #[test]
    fn octopus_has_eight_arms_around_the_body() {
        let octopus = BodyPlan::variant("octopus").unwrap().build();
        let tips: Vec<_> = octopus.bones().iter().filter(|b| b.name.starts_with("arm") && b.name.contains("_tip")).collect();
        assert_eq!(tips.len(), 8);
        for tip in tips {
            let r = (tip.position.x().powi(2) + tip.position.z().powi(2)).sqrt();
            assert!(r > 0.8, "{} lejos del centro", tip.name);
        }
    }
}
