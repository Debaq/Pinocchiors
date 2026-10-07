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
    /// Árbol libre, sin anatomía: tallo con ramas (plantas, cuerdas, props).
    Tree,
}

/// Cómo apoyan las patas del cuadrúpedo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Feet {
    /// Dos tramos (hombro → codo → pata): la plantilla básica.
    #[default]
    Simple,
    /// Sobre la planta entera: oso, mapache.
    Plantigrade,
    /// Sobre los dedos, con muñeca y corvejón altos: perro, felino.
    Digitigrade,
    /// Sobre la punta (casco o almohadilla): caballo, camélidos, ciervo.
    /// Carpo, corvejón y menudillo.
    Unguligrade,
}

/// Forma del cuello.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NeckShape {
    /// Sube adelante desde los hombros.
    #[default]
    Rising,
    /// Baja adelante y vuelve a subir (U): camello, dromedario.
    Swan,
    /// Casi vertical y recto: llama, alpaca.
    Upright,
    /// Horizontal, la cabeza al frente: reptiles, tortuga.
    Level,
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
    /// Segmentos de cada cuerno (par fijo arriba de la cabeza).
    pub horns: usize,
    /// Mandíbula: un hueso que abre la boca.
    pub jaw: bool,
    /// Segmentos de cada colmillo (hacia adelante y abajo desde la cabeza).
    pub tusks: usize,
    /// Pares de tentáculos largos del cuerpo radial (calamar: 1).
    pub tentacles: usize,
    /// Aleta caudal horizontal al final de la cola (delfín, ballena).
    pub flukes: bool,
    /// Largo de las patas respecto del cuerpo (1 = el de la plantilla;
    /// elefante menos, jirafa más). Solo bípedos y cuadrúpedos.
    pub leg_length: Real,
    /// Tipo de pata (solo cuadrúpedos).
    pub feet: Feet,
    /// Forma del cuello (solo cuadrúpedos).
    pub neck_shape: NeckShape,
    /// Jorobas sobre el lomo (0, 1 o 2; solo cuadrúpedos).
    pub humps: usize,
    /// Largo de los brazos respecto de la plantilla (mono araña: más). Bípedos.
    pub arm_length: Real,
    /// Largo de la cola respecto de la plantilla (cola prensil: más).
    pub tail_length: Real,
    /// Patas abiertas al costado y cuerpo cerca del suelo: lagarto,
    /// cocodrilo, tortuga (solo cuadrúpedos).
    pub sprawl: bool,
    /// Extremidades que faltan (bits, ver [`Limb`]): de cada una queda solo
    /// la raíz (hombro o cadera), para el muñón. Bípedos y cuadrúpedos.
    pub missing: u8,
    /// Cantidad de cabezas, cada una con su cuello y sus apéndices
    /// (hidra, cerbero). Bípedos y cuadrúpedos.
    pub heads: usize,
    /// Cantidad de colas, cada una con `tail` segmentos.
    pub tails: usize,
}

/// Bit de cada extremidad en [`BodyPlan::missing`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limb {
    /// Brazo izquierdo o pata delantera izquierda.
    FrontLeft = 1,
    /// Brazo derecho o pata delantera derecha.
    FrontRight = 2,
    /// Pierna izquierda o pata trasera izquierda.
    BackLeft = 4,
    /// Pierna derecha o pata trasera derecha.
    BackRight = 8,
}

impl Limb {
    fn of(front: bool, side: &str) -> Self {
        match (front, side == "_l") {
            (true, true) => Limb::FrontLeft,
            (true, false) => Limb::FrontRight,
            (false, true) => Limb::BackLeft,
            (false, false) => Limb::BackRight,
        }
    }
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
            BodyShape::Tree => (4, 3, 3),
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
            horns: 0,
            jaw: false,
            tusks: 0,
            tentacles: 0,
            flukes: false,
            leg_length: 1.0,
            feet: Feet::Simple,
            neck_shape: NeckShape::Rising,
            humps: 0,
            arm_length: 1.0,
            tail_length: 1.0,
            sprawl: false,
            missing: 0,
            heads: 1,
            tails: 1,
        }
    }

    /// Variantes con nombre: `(id, nombre, descripción)`.
    pub fn variants() -> &'static [(&'static str, &'static str, &'static str)] {
        &[
            ("elephant", "Elefante", "Cuadrúpedo de patas cortas con trompa, colmillos, orejas y cola"),
            ("horse", "Caballo / burro", "Patas con carpo, corvejón y menudillo; cuello y mandíbula"),
            ("camel", "Camello", "Camélido de dos jorobas con cuello en U"),
            ("dromedary", "Dromedario", "Camélido de una joroba con cuello en U"),
            ("llama", "Llama / alpaca", "Camélido sin joroba con cuello vertical (guanaco, vicuña)"),
            ("deer", "Ciervo / antílope", "Patas con casco, astas y cola corta"),
            ("bull", "Toro / cabra", "Cuadrúpedo con cuernos y mandíbula"),
            ("giraffe", "Jirafa", "Cuadrúpedo de cuello largo"),
            ("dog", "Perro / felino", "Cuadrúpedo digitígrado con cola larga y orejas"),
            ("bear", "Oso", "Cuadrúpedo plantígrado con cola corta"),
            ("lizard", "Lagarto / iguana", "Patas abiertas al costado, cuerpo bajo y cola larga (salamandra, geco)"),
            ("crocodile", "Cocodrilo / caimán", "Patas abiertas, mandíbula y cola larga y gruesa"),
            ("turtle", "Tortuga", "Patas abiertas y cortas, cuello que se estira y cola corta"),
            ("spider_monkey", "Mono araña / gibón", "Bípedo de brazos largos con cola prensil"),
            ("dragon", "Dragón", "Cuadrúpedo con alas, cuello y cola largos"),
            ("octopus", "Pulpo", "Cuerpo radial con 8 tentáculos"),
            ("squid", "Calamar", "Cuerpo radial con 8 brazos, 2 tentáculos largos y aletas"),
            ("fish", "Pez", "Eje horizontal con aletas y cola"),
            ("dolphin", "Delfín / ballena", "Cola larga con aleta caudal horizontal"),
            ("crab", "Cangrejo", "Artrópodo de 8 patas con pinzas"),
            ("insect", "Insecto", "Artrópodo de 6 patas con antenas y alas"),
            ("scorpion", "Escorpión", "Artrópodo con pinzas y cola"),
            ("trex", "Dinosaurio bípedo", "Bípedo digitígrado con cola larga y brazos cortos"),
            ("bird_walker", "Ave", "Bípedo digitígrado con alas"),
            ("snake", "Serpiente", "Cadena de 16 segmentos"),
            ("tree", "Árbol / planta", "Tallo con ramas, sin anatomía"),
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
                p.tusks = 2;
                p.leg_length = 0.8;
            }),
            "horse" => with(Quadruped, |p| {
                p.feet = Feet::Unguligrade;
                p.neck = 3;
                p.tail = 3;
                p.ears = 1;
                p.jaw = true;
                p.leg_length = 1.15;
            }),
            "camel" | "dromedary" => {
                let humps = if id == "camel" { 2 } else { 1 };
                let mut plan = Self::new(Quadruped);
                plan.feet = Feet::Unguligrade;
                plan.neck_shape = NeckShape::Swan;
                plan.neck = 4;
                plan.humps = humps;
                plan.tail = 2;
                plan.ears = 1;
                plan.jaw = true;
                plan.leg_length = 1.3;
                Some(plan)
            }
            "llama" => with(Quadruped, |p| {
                p.feet = Feet::Unguligrade;
                p.neck_shape = NeckShape::Upright;
                p.neck = 4;
                p.tail = 2;
                p.ears = 1;
                p.jaw = true;
                p.leg_length = 1.1;
            }),
            "deer" => with(Quadruped, |p| {
                p.feet = Feet::Unguligrade;
                p.neck = 2;
                p.horns = 3;
                p.ears = 1;
                p.tail = 1;
                p.leg_length = 1.15;
            }),
            "bull" => with(Quadruped, |p| {
                p.feet = Feet::Unguligrade;
                p.horns = 2;
                p.jaw = true;
                p.ears = 1;
                p.tail = 3;
            }),
            "giraffe" => with(Quadruped, |p| {
                p.feet = Feet::Unguligrade;
                p.neck = 6;
                p.tail = 3;
                p.horns = 1;
                p.leg_length = 1.3;
            }),
            "dog" => with(Quadruped, |p| {
                p.feet = Feet::Digitigrade;
                p.tail = 4;
                p.ears = 1;
            }),
            "bear" => with(Quadruped, |p| {
                p.feet = Feet::Plantigrade;
                p.tail = 1;
                p.ears = 1;
                p.jaw = true;
                p.leg_length = 0.85;
            }),
            "lizard" => with(Quadruped, |p| {
                p.sprawl = true;
                p.feet = Feet::Plantigrade;
                p.neck_shape = NeckShape::Level;
                p.tail = 8;
                p.tail_length = 1.6;
            }),
            "crocodile" => with(Quadruped, |p| {
                p.sprawl = true;
                p.feet = Feet::Plantigrade;
                p.neck_shape = NeckShape::Level;
                p.jaw = true;
                p.tail = 10;
                p.tail_length = 1.8;
                p.leg_length = 0.8;
            }),
            "turtle" => with(Quadruped, |p| {
                p.sprawl = true;
                p.neck_shape = NeckShape::Level;
                p.neck = 3;
                p.tail = 1;
                p.leg_length = 0.8;
            }),
            "spider_monkey" => with(Biped, |p| {
                p.arm_length = 1.6;
                p.tail = 12;
                p.tail_length = 2.2;
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
                p.limbs = 8;
                p.limb_segments = 5;
                p.tentacles = 1;
                p.fins = true;
            }),
            "fish" => with(Fish, |p| p.tail = 4),
            "dolphin" => with(Fish, |p| {
                p.tail = 6;
                p.flukes = true;
            }),
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
                p.jaw = true;
            }),
            "bird_walker" => with(DigitigradeBiped, |p| {
                p.tail = 2;
                p.wings = 3;
            }),
            "snake" => with(Serpent, |p| p.tail = 16),
            "tree" => with(Tree, |p| {
                p.tail = 4;
                p.limbs = 4;
                p.limb_segments = 3;
            }),
            _ => None,
        }
    }

    /// La extremidad está (no se quitó).
    pub fn has(&self, limb: Limb) -> bool {
        self.missing & limb as u8 == 0
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
            BodyShape::Tree => self.tree(&mut b),
        }
        // Largo de las patas: lo que está debajo de la línea del cuerpo se
        // estira o se acorta, lo de arriba sube o baja lo mismo
        let body_line = match self.shape {
            BodyShape::Quadruped if self.sprawl => Some(0.17),
            BodyShape::Biped | BodyShape::Quadruped => Some(0.45),
            BodyShape::DigitigradeBiped => Some(0.55),
            _ => None,
        };
        if let Some(line) = body_line {
            let k = self.leg_length.clamp(0.3, 3.0);
            if (k - 1.0).abs() > 1e-9 {
                for bone in &mut b.bones {
                    let p = bone.position;
                    let y = if p.y() <= line { p.y() * k } else { p.y() + line * (k - 1.0) };
                    bone.position = Vector3::new(p.x(), y, p.z());
                }
            }
        }
        BasicSkeleton::from_bones(b.bones)
    }

    /// Cuellos desde `chest` hasta cada cabeza, con sus apéndices. `tip` es
    /// la punta del cuello, la cabeza va de ahí a `head` y `bend` curva el
    /// cuello. Con varias cabezas se abren en abanico hacia los costados y
    /// los huesos de la segunda en adelante llevan el número (`neck2_1`,
    /// `head2`, `ear2_tip_l`…).
    fn neck_and_head(&self, b: &mut Builder, chest: usize, tip: Vector3, head: Vector3, bend: Vector3) {
        let n = self.heads.clamp(1, 9);
        let spread = 1.6 * head.distance(&tip);
        for k in 0..n {
            let x = (k as Real - (n - 1) as Real / 2.0) * spread;
            let shift = Vector3::new(x, 0.0, 0.0);
            let tag = if k == 0 { String::new() } else { (k + 1).to_string() };
            self.one_head(b, chest, tip + shift, head + shift, bend + shift * 0.5, &tag);
        }
    }

    /// Cuello desde `chest` hasta la cabeza, y apéndices de la cabeza.
    /// `base` y `tip` son el arranque y la punta del cuello; la cabeza va de
    /// la punta del cuello a `head`.
    fn one_head(&self, b: &mut Builder, chest: usize, tip: Vector3, head: Vector3, bend: Vector3, tag: &str) {
        let start = b.position(chest);
        let points = curve(start, tip, bend, self.neck.max(1));
        let neck = b.chain(&format!("neck{tag}"), chest, &points, false);
        let head_bone = b.bone(format!("head{tag}"), head, neck);
        let forward = (head - tip).try_normalize().unwrap_or(Vector3::unit_z());
        let size = head.distance(&tip);
        if self.trunk > 0 {
            // Cuelga hasta cerca del suelo, un poco por delante de la cabeza
            let end = v(head.x(), 0.12, head.z() + 0.1);
            let control = head + forward * (1.5 * size) - Vector3::unit_y() * (0.3 * size);
            let points = curve(head, end, control, self.trunk);
            b.chain(&format!("trunk{tag}"), head_bone, &points, true);
        }
        if self.antennae > 0 {
            for (s, side) in SIDES {
                let root = head + Vector3::new(0.3 * s * size, 0.3 * size, 0.0);
                let end = root + forward * (1.5 * size) + Vector3::new(0.8 * s * size, 1.2 * size, 0.0);
                let points = curve(root, end, root + Vector3::new(0.2 * s * size, 1.0 * size, 0.2 * size), self.antennae);
                b.chain_sided(&format!("antenna{tag}"), head_bone, &points, side);
            }
        } else {
            b.mark_leaf(head_bone);
        }
        if self.horns > 0 {
            // Fijos arriba de la cabeza, hacia afuera y atrás
            for (s, side) in SIDES {
                let root = head + Vector3::new(0.3 * s * size, 0.4 * size, -0.1 * size);
                let end = root + Vector3::new(0.5 * s * size, 0.9 * size, -0.4 * size);
                let points = curve(root, end, root + Vector3::new(0.1 * s * size, 0.6 * size, 0.1 * size), self.horns);
                b.chain_sided(&format!("horn{tag}"), head_bone, &points, side);
            }
        }
        if self.tusks > 0 {
            // Desde la base de la cabeza, hacia adelante y abajo
            for (s, side) in SIDES {
                let root = head + Vector3::new(0.25 * s * size, -0.35 * size, 0.0);
                let end = root + forward * (0.9 * size) + Vector3::new(0.1 * s * size, -0.5 * size, 0.0);
                let control = root + forward * (0.6 * size) + Vector3::new(0.05 * s * size, -0.35 * size, 0.0);
                let points = curve(root, end, control, self.tusks);
                b.chain_sided(&format!("tusk{tag}"), head_bone, &points, side);
            }
        }
        if self.jaw {
            // Bisagra en la base de la cabeza (gira con el cuello) y punta bajo el hocico
            let hinge = tip + Vector3::new(0.0, -0.2 * size, 0.0) + forward * (0.1 * size);
            let jaw = b.bone(format!("jaw{tag}"), hinge, neck);
            let end = head + Vector3::new(0.0, -0.4 * size, 0.0) + forward * (0.1 * size);
            let jaw_tip = b.bone(format!("jaw{tag}_tip"), end, jaw);
            b.mark_leaf(jaw_tip);
        }
        if self.ears > 0 {
            // Colgando de la base de la cabeza, a los lados
            for (s, side) in SIDES {
                let root = tip + Vector3::new(0.45 * s * size, 0.25 * size, 0.0);
                let end = root + Vector3::new(0.7 * s * size, -0.5 * size, -0.3 * size);
                let points = curve(root, end, root + Vector3::new(0.5 * s * size, 0.0, -0.2 * size), self.ears + 1);
                b.chain_sided(&format!("ear{tag}"), neck, &points, side);
            }
        }
    }

    /// Cola desde `from`, hacia `end` curvándose por `bend`.
    fn add_tail(&self, b: &mut Builder, from: usize, end: Vector3, bend: Vector3) {
        if self.tail == 0 {
            return;
        }
        let start = b.position(from);
        let k = self.tail_length.clamp(0.3, 4.0);
        // Una cola alargada no atraviesa el suelo
        let lift = |p: Vector3| Vector3::new(p.x(), p.y().max(0.08), p.z());
        let (end, bend) = (start + (end - start) * k, start + (bend - start) * k);
        // Varias colas: abiertas en abanico hacia los costados (`tail`, `tail2_1`…)
        let n = self.tails.clamp(1, 9);
        let spread = 0.35 * end.distance(&start);
        for i in 0..n {
            let x = (i as Real - (n - 1) as Real / 2.0) * spread;
            let shift = Vector3::new(x, 0.0, 0.0);
            let points = curve(start, lift(end + shift), lift(bend + shift * 0.5), self.tail);
            let name = if i == 0 { "tail".to_string() } else { format!("tail{}", i + 1) };
            b.chain(&name, from, &points, true);
        }
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
        let arm = self.arm_length.clamp(0.5, 2.5);
        for (s, side) in SIDES {
            let at_shoulder = v(0.1 * s, 0.85, 0.0);
            let reach = |p: Vector3| at_shoulder + (p - at_shoulder) * arm;
            let arm_joints = [
                ("shoulder", at_shoulder),
                ("elbow", reach(v(0.25 * s, 0.65, 0.0))),
                ("wrist", reach(v(0.35 * s, 0.5, 0.0))),
                ("hand", reach(v(0.4 * s, 0.45, 0.0))),
            ];
            b.limb(&arm_joints, chest, side, self.has(Limb::of(true, side)));
            let leg_joints = [
                ("hip", v(0.1 * s, 0.45, 0.0)),
                ("knee", v(0.1 * s, 0.25, 0.0)),
                ("ankle", v(0.1 * s, 0.05, 0.0)),
                ("foot", v(0.1 * s, 0.0, 0.05)),
            ];
            b.limb(&leg_joints, pelvis, side, self.has(Limb::of(false, side)));
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
            let leg_joints = [
                ("hip", v(0.12 * s, 0.55, 0.0)),
                ("knee", v(0.14 * s, 0.35, 0.1)),
                ("ankle", v(0.14 * s, 0.12, -0.06)),
                ("foot", v(0.14 * s, 0.0, 0.1)),
            ];
            b.limb(&leg_joints, pelvis, side, self.has(Limb::of(false, side)));
            if self.wings == 0 {
                let at_shoulder = v(0.1 * s, 0.66, 0.3);
                let arm = self.arm_length.clamp(0.5, 2.5);
                let reach = |p: Vector3| at_shoulder + (p - at_shoulder) * arm;
                let arm_joints = [
                    ("shoulder", at_shoulder),
                    ("elbow", reach(v(0.14 * s, 0.58, 0.34))),
                    ("hand", reach(v(0.12 * s, 0.54, 0.42))),
                ];
                b.limb(&arm_joints, chest, side, self.has(Limb::of(true, side)));
            }
        }
        self.add_tail(b, pelvis, v(0.0, 0.45, -0.9), v(0.0, 0.6, -0.45));
        self.add_wings(b, chest, v(0.1, 0.72, 0.22), 0.6);
    }

    fn quadruped(&self, b: &mut Builder) {
        // Con las patas abiertas el cuerpo va cerca del suelo: todo lo que
        // no es pata baja lo mismo
        let drop = if self.sprawl { 0.3 } else { 0.0 };
        let up = |p: Vector3| p - v(0.0, drop, 0.0);
        let hip = b.root("hip", up(v(0.0, 0.5, -0.3)));
        let spine = b.bone("spine", up(v(0.0, 0.55, 0.0)), hip);
        let chest = b.bone("chest", up(v(0.0, 0.55, 0.3)), spine);
        let n = self.neck as Real;
        let (neck_tip, head, bend) = match self.neck_shape {
            // Cuello largo: sube más
            NeckShape::Rising => {
                let rise = if self.neck > 1 { 0.12 * n } else { 0.05 };
                let tip = v(0.0, 0.6 + rise, 0.4 + 0.02 * n);
                (tip, tip + v(0.0, 0.02, 0.14), v(0.0, 0.55 + 0.6 * rise, 0.42))
            }
            // Baja adelante de los hombros y sube: la cabeza queda al frente
            NeckShape::Swan => {
                let tip = v(0.0, 0.55 + 0.06 * n, 0.3 + 0.12 * n);
                (tip, tip + v(0.0, -0.01, 0.15), v(0.0, 0.3, 0.3 + 0.08 * n))
            }
            // Recto hacia arriba, apenas inclinado adelante
            NeckShape::Upright => {
                let tip = v(0.0, 0.6 + 0.09 * n, 0.36 + 0.015 * n);
                (tip, tip + v(0.0, 0.01, 0.13), v(0.0, 0.58 + 0.045 * n, 0.33 + 0.008 * n))
            }
            // Al frente, apenas por encima del lomo
            NeckShape::Level => {
                let tip = v(0.0, 0.56 + 0.005 * n, 0.36 + 0.07 * n);
                (tip, tip + v(0.0, 0.0, 0.16), v(0.0, 0.55, 0.33 + 0.035 * n))
            }
        };
        self.neck_and_head(b, chest, up(neck_tip), up(head), up(bend));
        for (s, side) in SIDES {
            let (front, back) = if s < 0.0 { ("_fl", "_bl") } else { ("_fr", "_br") };
            let x = 0.15 * s;
            // Delantera: hombro → codo → (muñeca o carpo) → (menudillo) → pata
            let front_leg: &[(&str, Vector3)] = match self.feet {
                Feet::Simple => &[("shoulder", v(x, 0.5, 0.25)), ("elbow", v(x, 0.28, 0.27))],
                Feet::Plantigrade => &[("shoulder", v(x, 0.5, 0.25)), ("elbow", v(x, 0.28, 0.23)), ("wrist", v(x, 0.04, 0.25))],
                Feet::Digitigrade => &[("shoulder", v(x, 0.5, 0.25)), ("elbow", v(x, 0.3, 0.22)), ("wrist", v(x, 0.08, 0.25))],
                Feet::Unguligrade => &[
                    ("shoulder", v(x, 0.5, 0.27)),
                    ("elbow", v(x, 0.36, 0.22)),
                    ("wrist", v(x, 0.18, 0.25)),
                    ("fetlock", v(x, 0.05, 0.25)),
                ],
            };
            let paw_front = match self.feet {
                Feet::Simple => v(x, 0.0, 0.25),
                Feet::Plantigrade => v(x, 0.0, 0.31),
                Feet::Digitigrade => v(x, 0.0, 0.29),
                Feet::Unguligrade => v(x, 0.0, 0.28),
            };
            // Trasera: cadera → rodilla → (tobillo o corvejón) → (menudillo) → pata
            let back_leg: &[(&str, Vector3)] = match self.feet {
                Feet::Simple => &[("hip", v(x, 0.45, -0.28)), ("knee", v(x, 0.25, -0.32))],
                Feet::Plantigrade => &[("hip", v(x, 0.45, -0.28)), ("knee", v(x, 0.25, -0.24)), ("ankle", v(x, 0.04, -0.3))],
                Feet::Digitigrade => &[("hip", v(x, 0.45, -0.28)), ("knee", v(x, 0.3, -0.22)), ("hock", v(x, 0.12, -0.34))],
                Feet::Unguligrade => &[
                    ("hip", v(x, 0.45, -0.28)),
                    ("knee", v(x, 0.31, -0.22)),
                    ("hock", v(x, 0.2, -0.36)),
                    ("fetlock", v(x, 0.05, -0.33)),
                ],
            };
            let paw_back = match self.feet {
                Feet::Simple => v(x, 0.0, -0.28),
                Feet::Plantigrade => v(x, 0.0, -0.22),
                Feet::Digitigrade => v(x, 0.0, -0.3),
                Feet::Unguligrade => v(x, 0.0, -0.3),
            };
            // Abiertas: el codo y la rodilla salen al costado a la altura del
            // cuerpo y el antebrazo baja al suelo (con planta, la mano mira
            // adelante y afuera)
            let sprawled = |shoulder: (&'static str, Vector3), bend: (&'static str, Vector3), low: (&'static str, Vector3), paw: Vector3, full: Vector3| {
                if self.feet == Feet::Simple { (vec![shoulder, bend], paw) } else { (vec![shoulder, bend, low], full) }
            };
            let (front_leg, paw_front, back_leg, paw_back) = if self.sprawl {
                let z = |front: bool, d: Real| if front { 0.28 + d } else { -0.28 + d };
                let (f, pf) = sprawled(
                    ("shoulder", v(0.1 * s, 0.22, z(true, 0.0))),
                    ("elbow", v(0.27 * s, 0.2, z(true, 0.0))),
                    ("wrist", v(0.31 * s, 0.03, z(true, 0.03))),
                    v(0.31 * s, 0.0, z(true, 0.04)),
                    v(0.36 * s, 0.0, z(true, 0.1)),
                );
                let (bk, pb) = sprawled(
                    ("hip", v(0.1 * s, 0.2, z(false, 0.0))),
                    ("knee", v(0.28 * s, 0.18, z(false, 0.03))),
                    ("ankle", v(0.31 * s, 0.03, z(false, -0.01))),
                    v(0.31 * s, 0.0, z(false, 0.01)),
                    v(0.37 * s, 0.0, z(false, 0.03)),
                );
                (f, pf, bk, pb)
            } else {
                (front_leg.to_vec(), paw_front, back_leg.to_vec(), paw_back)
            };
            for (joints, paw, parent, suffix, is_front) in [(&front_leg, paw_front, chest, front, true), (&back_leg, paw_back, hip, back, false)] {
                // Sin la pata queda solo el hombro o la cadera (el muñón)
                let present = self.has(Limb::of(is_front, side));
                let mut last = parent;
                for &(name, p) in joints.iter().take(if present { joints.len() } else { 1 }) {
                    // El menudillo lleva el sufijo de la pata (delantera o trasera)
                    let name = if name == "fetlock" { format!("fetlock{suffix}") } else { side_name(name, side) };
                    last = b.bone(name, p, last);
                }
                if present {
                    last = b.bone(format!("paw{suffix}"), paw, last);
                }
                b.mark_leaf(last);
            }
        }
        // Jorobas sobre el lomo: una en el medio o una sobre cada hombro y cadera
        match self.humps {
            0 => {}
            1 => {
                let hump = b.bone("hump", up(v(0.0, 0.76, 0.02)), spine);
                b.mark_leaf(hump);
            }
            _ => {
                let front = b.bone("hump_front", up(v(0.0, 0.74, 0.16)), chest);
                b.mark_leaf(front);
                let back = b.bone("hump_back", up(v(0.0, 0.74, -0.14)), spine);
                b.mark_leaf(back);
            }
        }
        let length = 0.15 + 0.08 * self.tail as Real;
        // Arrastrada, la cola baja menos: va casi horizontal
        let fall = if self.sprawl { 0.12 } else { 0.4 };
        self.add_tail(b, hip, up(v(0.0, 0.45 - fall * length, -0.35 - length)), up(v(0.0, 0.5, -0.35 - 0.6 * length)));
        self.add_wings(b, chest, up(v(0.12, 0.6, 0.22)), 0.8);
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
        // Tentáculos largos, hacia adelante, entre los brazos del frente
        for i in 0..self.tentacles {
            let angle = 0.2 + 0.25 * i as Real;
            for (s, side) in SIDES {
                let dir = v((s * angle).sin(), 0.0, (s * angle).cos());
                let points = curve(v(0.0, 0.3, 0.0) + dir * 0.1, dir * 1.6 + v(0.0, 0.02, 0.0), dir * 0.8 + v(0.0, 0.05, 0.0), segments + 2);
                b.chain_sided(&format!("tentacle{}", i + 1), body, &points, side);
            }
        }
        // Aletas del manto, arriba a los costados
        if self.fins {
            for (s, side) in SIDES {
                let root = b.bone_sided("fin", v(0.08 * s, 0.9, 0.0), head, side);
                let tip = b.bone_sided("fin_tip", v(0.3 * s, 0.95, -0.05), root, side);
                b.mark_leaf(tip);
            }
        }
    }

    fn fish(&self, b: &mut Builder) {
        let body = b.root("body", v(0.0, 0.5, 0.05));
        let head = b.bone("head", v(0.0, 0.5, 0.5), body);
        b.mark_leaf(head);
        let length = 0.35 + 0.05 * self.tail as Real;
        self.add_tail(b, body, v(0.0, 0.5, 0.05 - length), v(0.0, 0.5, 0.05 - 0.5 * length));
        if self.flukes {
            // Aleta caudal horizontal al final de la cola (o del cuerpo, sin cola)
            let end = b.bones.len() - 1;
            let from = if self.tail > 0 { end } else { body };
            let at = b.position(from);
            for (s, side) in SIDES {
                let tip = b.bone_sided("fluke", at + v(0.18 * s, 0.0, -0.08), from, side);
                b.mark_leaf(tip);
            }
        }
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

    fn tree(&self, b: &mut Builder) {
        let base = b.root("base", v(0.0, 0.0, 0.0));
        let n = self.tail.max(2);
        // Tallo recto hacia arriba
        let mut stem = Vec::with_capacity(n);
        let mut last = base;
        for i in 1..=n {
            let name = if i == n { "stem_tip".to_string() } else { format!("stem_{i}") };
            last = b.bone(name, v(0.0, i as Real / n as Real, 0.0), last);
            stem.push(last);
        }
        b.mark_leaf(last);
        // Ramas repartidas a lo largo del tallo, girando alrededor (ángulo áureo)
        let segments = self.limb_segments.max(1);
        for i in 0..self.limbs {
            let from = stem[((i + 1) * (n - 1) / (self.limbs + 1)).min(n - 1)];
            let start = b.position(from);
            let angle = 2.399_963 * i as Real;
            let dir = v(angle.sin(), 0.0, angle.cos());
            let points = curve(start, start + dir * 0.45 + v(0.0, 0.25, 0.0), start + dir * 0.3 + v(0.0, 0.02, 0.0), segments);
            b.chain(&format!("branch{}", i + 1), from, &points, true);
        }
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

    /// Extremidad con nombres de lado (`shoulder_l`, `elbow_l`…) desde
    /// `parent`. Si falta (`present` falso) queda solo la primera
    /// articulación, como hoja: el muñón.
    fn limb(&mut self, joints: &[(&str, Vector3)], parent: usize, side: &str, present: bool) {
        let mut last = parent;
        for &(name, p) in joints.iter().take(if present { joints.len() } else { 1 }) {
            last = self.bone_sided(name, p, last, side);
        }
        self.mark_leaf(last);
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
        for id in ["elephant", "octopus", "crab", "dragon", "fish", "squid", "dolphin", "bull", "camel", "horse", "bear", "dog", "spider_monkey", "lizard", "crocodile", "turtle"] {
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
    fn new_appendages() {
        let bull = names(&BodyPlan::variant("bull").unwrap().build()).join(" ");
        assert!(bull.contains("horn_tip_l") && bull.contains("horn_tip_r") && bull.contains("jaw_tip"), "{bull}");
        let elephant = names(&BodyPlan::variant("elephant").unwrap().build()).join(" ");
        assert!(elephant.contains("tusk_tip_l") && elephant.contains("tusk_tip_r"), "{elephant}");
        let squid = BodyPlan::variant("squid").unwrap().build();
        let tentacle_tips: Vec<_> = squid.bones().iter().filter(|b| b.name.starts_with("tentacle") && b.name.contains("_tip")).collect();
        assert_eq!(tentacle_tips.len(), 2);
        assert!(squid.bones().iter().any(|b| b.name == "fin_tip_l"));
        let dolphin = names(&BodyPlan::variant("dolphin").unwrap().build()).join(" ");
        assert!(dolphin.contains("fluke_l") && dolphin.contains("fluke_r"), "{dolphin}");
        let tree = BodyPlan::variant("tree").unwrap().build();
        assert_eq!(tree.bones().iter().filter(|b| b.name.starts_with("branch") && b.name.ends_with("_tip")).count(), 4);
    }

    #[test]
    fn leg_length_moves_the_body() {
        let height = |k: Real| {
            let mut plan = BodyPlan::new(BodyShape::Quadruped);
            plan.leg_length = k;
            let skeleton = plan.build();
            let find = |name: &str| skeleton.bones().iter().find(|b| b.name == name).unwrap().position.y();
            (find("hip"), find("knee_l"), find("paw_bl"))
        };
        let (hip, knee, paw) = height(1.0);
        let (short_hip, short_knee, short_paw) = height(0.8);
        assert!(short_hip < hip && short_knee < knee, "patas cortas: el cuerpo baja");
        assert!((paw - short_paw).abs() < 1e-9, "las patas siguen en el suelo");
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

    fn chain_to(skeleton: &BasicSkeleton, leaf: &str) -> Vec<String> {
        let mut b = skeleton.bones().iter().position(|b| b.name == leaf).unwrap();
        let mut out = vec![skeleton.bones()[b].name.clone()];
        while let Some(p) = skeleton.bones()[b].parent {
            b = p;
            out.push(skeleton.bones()[b].name.clone());
        }
        out.reverse();
        out
    }

    #[test]
    fn feet_add_leg_sections() {
        let leg = |feet: Feet, leaf: &str| {
            let mut plan = BodyPlan::new(BodyShape::Quadruped);
            plan.feet = feet;
            chain_to(&plan.build(), leaf).join(" ")
        };
        assert_eq!(leg(Feet::Simple, "paw_fl"), "hip spine chest shoulder_l elbow_l paw_fl");
        assert_eq!(leg(Feet::Digitigrade, "paw_bl"), "hip hip_l knee_l hock_l paw_bl");
        assert_eq!(leg(Feet::Plantigrade, "paw_br"), "hip hip_r knee_r ankle_r paw_br");
        assert_eq!(leg(Feet::Unguligrade, "paw_fr"), "hip spine chest shoulder_r elbow_r wrist_r fetlock_fr paw_fr");
        assert_eq!(leg(Feet::Unguligrade, "paw_bl"), "hip hip_l knee_l hock_l fetlock_bl paw_bl");
        // Los cascos tocan el suelo y el carpo dobla al revés que el corvejón
        let horse = BodyPlan::variant("horse").unwrap().build();
        let at = |name: &str| horse.bones().iter().find(|b| b.name == name).unwrap().position;
        assert!(at("paw_fl").y().abs() < 1e-9 && at("paw_bl").y().abs() < 1e-9);
        assert!(at("knee_l").z() > at("hock_l").z(), "la rodilla trasera va adelante del corvejón");
        assert!(at("elbow_l").z() < at("wrist_l").z(), "el codo va atrás del carpo");
    }

    #[test]
    fn camelids_have_their_neck_and_humps() {
        let skeleton = |id: &str| BodyPlan::variant(id).unwrap().build();
        let at = |s: &BasicSkeleton, name: &str| s.bones().iter().find(|b| b.name == name).unwrap().position;
        let camel = skeleton("camel");
        assert!(camel.bones().iter().any(|b| b.name == "hump_front") && camel.bones().iter().any(|b| b.name == "hump_back"));
        assert!(skeleton("dromedary").bones().iter().any(|b| b.name == "hump"));
        assert!(!skeleton("llama").bones().iter().any(|b| b.name.starts_with("hump")));
        // Camello: el cuello baja antes de subir; llama: sube derecho
        let neck_ys = |s: &BasicSkeleton| (1..=4).map(|i| at(s, &if i == 4 { "neck_4".into() } else { format!("neck_{i}") }).y()).collect::<Vec<_>>();
        let camel_neck = neck_ys(&camel);
        assert!(camel_neck[0] < at(&camel, "chest").y(), "el cuello del camello baja: {camel_neck:?}");
        assert!(camel_neck[3] > at(&camel, "chest").y());
        let llama = skeleton("llama");
        let llama_neck = neck_ys(&llama);
        assert!(llama_neck.windows(2).all(|w| w[1] > w[0]), "la llama sube derecho: {llama_neck:?}");
        let head = at(&llama, "neck_4");
        assert!(head.y() - at(&llama, "chest").y() > 2.0 * (head.z() - at(&llama, "chest").z()), "cuello casi vertical");
    }

    #[test]
    fn spider_monkey_has_long_arms_and_tail() {
        let monkey = BodyPlan::variant("spider_monkey").unwrap().build();
        let human = BodyPlan::new(BodyShape::Biped).build();
        let at = |s: &BasicSkeleton, name: &str| s.bones().iter().find(|b| b.name == name).unwrap().position;
        let arm = |s: &BasicSkeleton| at(s, "hand_l").distance(&at(s, "shoulder_l"));
        assert!(arm(&monkey) > 1.5 * arm(&human));
        assert_eq!(monkey.bones().iter().filter(|b| b.name.starts_with("tail")).count(), 12);
        assert!(at(&monkey, "tail_tip").distance(&at(&monkey, "pelvis")) > 0.8);
    }

    #[test]
    fn reptiles_sprawl_close_to_the_ground() {
        for id in ["lizard", "crocodile", "turtle"] {
            let skeleton = BodyPlan::variant(id).unwrap().build();
            let at = |name: &str| skeleton.bones().iter().find(|b| b.name == name).unwrap().position;
            for (paw, top) in [("paw_fl", "shoulder_l"), ("paw_br", "hip_r")] {
                let (paw, top) = (at(paw), at(top));
                assert!(paw.y().abs() < 1e-9, "{id}: la pata toca el suelo");
                // Más afuera que abajo: abierta al costado
                assert!((paw.x() - top.x()).abs() > 0.8 * (top.y() - paw.y()), "{id}: pata abierta");
            }
            assert!(at("chest").y() < 0.3, "{id}: cuerpo bajo");
            let tail = skeleton.bones().iter().filter(|b| b.name.starts_with("tail")).map(|b| b.position.y());
            assert!(tail.fold(Real::MAX, Real::min) > 0.0, "{id}: la cola no atraviesa el suelo");
            assert!(at("head").z() > at("chest").z() + 0.1 && (at("head").y() - at("chest").y()).abs() < 0.1, "{id}: cabeza al frente");
        }
        let mut lizard = BodyPlan::variant("lizard").unwrap();
        lizard.leg_length = 1.5;
        let tall = lizard.build();
        assert!(tall.bones().iter().find(|b| b.name == "chest").unwrap().position.y() > 0.25, "patas más largas suben el cuerpo");
    }

    #[test]
    fn missing_limbs_leave_a_stump() {
        let mut plan = BodyPlan::new(BodyShape::Biped);
        plan.missing = Limb::FrontLeft as u8 | Limb::BackRight as u8;
        let skeleton = plan.build();
        let names = names(&skeleton);
        assert!(names.contains(&"shoulder_l") && !names.contains(&"elbow_l") && !names.contains(&"hand_l"));
        assert!(names.contains(&"hip_r") && !names.contains(&"knee_r"));
        assert!(names.contains(&"hand_r") && names.contains(&"foot_l"));
        let stump = skeleton.bones().iter().find(|b| b.name == "shoulder_l").unwrap();
        assert!(stump.is_leaf, "el muñón es hoja");

        let mut horse = BodyPlan::variant("horse").unwrap();
        horse.missing = Limb::BackLeft as u8;
        let names_horse: Vec<String> = horse.build().bones().iter().map(|b| b.name.clone()).collect();
        assert!(names_horse.contains(&"hip_l".into()) && !names_horse.iter().any(|n| n == "knee_l" || n == "paw_bl" || n == "fetlock_bl"));
        assert!(names_horse.contains(&"paw_br".into()));
    }

    #[test]
    fn several_heads_and_tails() {
        let mut plan = BodyPlan::variant("dog").unwrap();
        plan.heads = 3;
        plan.tails = 2;
        plan.horns = 1;
        let skeleton = plan.build();
        let names = names(&skeleton);
        for name in ["head", "head2", "head3", "neck2_1", "ear3_tip_l", "horn2_tip_r", "tail_tip", "tail2_tip"] {
            assert!(names.contains(&name), "{name} en {names:?}");
        }
        let at = |name: &str| skeleton.bones().iter().find(|b| b.name == name).unwrap().position;
        // En abanico: una al medio y una a cada lado, sin tocarse
        assert!(at("head").x() < at("head2").x() - 0.1 && at("head2").x() < at("head3").x() - 0.1);
        assert!((at("head2").x()).abs() < 1e-9, "la del medio queda centrada");
        assert!((at("tail_tip").x() + at("tail2_tip").x()).abs() < 1e-9 && at("tail_tip").x() < 0.0);
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), names.len(), "nombres únicos");
    }
}
