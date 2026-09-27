//! Trait y tipos para esqueletos

use crate::Bone;
use pinocchio_math::{Real, Rect, Vector3};

/// Trait que define un esqueleto
pub trait Skeleton {
    /// Número de huesos en el esqueleto
    fn num_bones(&self) -> usize;

    /// Obtiene un hueso por índice
    fn get_bone(&self, index: usize) -> Option<&Bone>;

    /// Obtiene todos los huesos
    fn bones(&self) -> &[Bone];

    /// Obtiene el índice del hueso raíz
    fn root(&self) -> usize {
        0
    }

    /// Obtiene los índices de los hijos de un hueso
    fn get_children(&self, bone_index: usize) -> Vec<usize> {
        self.bones()
            .iter()
            .enumerate()
            .filter(|(_, b)| b.parent == Some(bone_index))
            .map(|(i, _)| i)
            .collect()
    }

    /// Verifica si un hueso es hoja (sin hijos)
    fn is_leaf(&self, bone_index: usize) -> bool {
        self.get_children(bone_index).is_empty()
    }

    /// Obtiene la posición de un hueso
    fn get_position(&self, bone_index: usize) -> Option<Vector3> {
        self.get_bone(bone_index).map(|b| b.position)
    }

    /// Obtiene el padre de un hueso
    fn get_parent(&self, bone_index: usize) -> Option<usize> {
        self.get_bone(bone_index).and_then(|b| b.parent)
    }

    /// Escala el esqueleto por un factor
    fn scale(&mut self, factor: f64);

    /// Traslada el esqueleto
    fn translate(&mut self, offset: Vector3);

    /// Crea una copia escalada del esqueleto
    fn scaled(&self, factor: f64) -> Vec<Bone> {
        self.bones()
            .iter()
            .map(|b| Bone {
                name: b.name.clone(),
                position: b.position * factor,
                parent: b.parent,
                is_leaf: b.is_leaf,
            })
            .collect()
    }

    /// Obtiene el grafo del esqueleto como lista de aristas (parent, child)
    fn get_graph_edges(&self) -> Vec<(usize, usize)> {
        self.bones()
            .iter()
            .enumerate()
            .filter_map(|(i, b)| b.parent.map(|p| (p, i)))
            .collect()
    }

    /// Obtiene la profundidad de un hueso en la jerarquía
    fn get_depth(&self, bone_index: usize) -> usize {
        let mut depth = 0;
        let mut current = bone_index;
        while let Some(parent) = self.get_parent(current) {
            depth += 1;
            current = parent;
        }
        depth
    }
}

/// Implementación básica de esqueleto
#[derive(Debug, Clone, Default)]
pub struct BasicSkeleton {
    bones: Vec<Bone>,
}

impl BasicSkeleton {
    /// Crea un esqueleto vacío
    pub fn new() -> Self {
        Self { bones: Vec::new() }
    }

    /// Crea un esqueleto desde una lista de huesos
    pub fn from_bones(bones: Vec<Bone>) -> Self {
        Self { bones }
    }

    /// Añade un hueso y devuelve su índice
    pub fn add_bone(&mut self, bone: Bone) -> usize {
        let index = self.bones.len();
        self.bones.push(bone);
        index
    }

    /// Obtiene una referencia mutable a los huesos
    pub fn bones_mut(&mut self) -> &mut [Bone] {
        &mut self.bones
    }
}

impl Skeleton for BasicSkeleton {
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
            bone.position = bone.position * factor;
        }
    }

    fn translate(&mut self, offset: Vector3) {
        for bone in &mut self.bones {
            bone.position = bone.position + offset;
        }
    }
}

/// Copia el esqueleto aplicando `f` a la posición de cada hueso
pub fn map_positions<S: Skeleton + ?Sized>(skeleton: &S, f: impl Fn(Vector3) -> Vector3) -> BasicSkeleton {
    BasicSkeleton::from_bones(
        skeleton
            .bones()
            .iter()
            .map(|b| Bone {
                position: f(b.position),
                ..b.clone()
            })
            .collect(),
    )
}

/// Bounding box de las posiciones de los huesos (`None` si no hay huesos)
pub fn skeleton_bounds<S: Skeleton + ?Sized>(skeleton: &S) -> Option<Rect> {
    let mut bones = skeleton.bones().iter();
    let first = bones.next()?;
    let mut rect = Rect::from_point(first.position);
    for b in bones {
        rect.expand_to_point(b.position);
    }
    Some(rect)
}

/// Escala uniformemente y centra el esqueleto dentro de `target`.
///
/// Si el esqueleto es "alto" (su extensión en Y es al menos la mitad de su
/// mayor extensión: bípedos, cuadrúpedos), la escala hace que su altura ocupe
/// `fill` veces la altura de `target`, independiente de la pose de los brazos.
/// Si no (arañas, serpientes, alas muy abiertas), se usa la mayor extensión.
pub fn fit_to_bounds<S: Skeleton + ?Sized>(skeleton: &S, target: &Rect, fill: Real) -> BasicSkeleton {
    let Some(bounds) = skeleton_bounds(skeleton) else {
        return BasicSkeleton::new();
    };
    let skel_size = bounds.size();
    let target_size = target.size();
    let skel_max = skel_size.max_component();

    let scale = if skel_max <= 1e-12 {
        1.0
    } else if skel_size.y() >= 0.5 * skel_max {
        fill * target_size.y() / skel_size.y()
    } else {
        fill * target_size.max_component() / skel_max
    };

    let skel_center = bounds.center();
    let target_center = target.center();
    map_positions(skeleton, |p| (p - skel_center) * scale + target_center)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_simple_skeleton() -> BasicSkeleton {
        let mut skel = BasicSkeleton::new();
        skel.add_bone(Bone::new("root", Vector3::zero()));
        skel.add_bone(Bone::with_parent("spine", Vector3::new(0.0, 1.0, 0.0), 0));
        skel.add_bone(Bone::with_parent("head", Vector3::new(0.0, 2.0, 0.0), 1));
        skel
    }

    #[test]
    fn test_skeleton_basics() {
        let skel = make_simple_skeleton();
        assert_eq!(skel.num_bones(), 3);
        assert_eq!(skel.root(), 0);
    }

    #[test]
    fn test_children() {
        let skel = make_simple_skeleton();
        assert_eq!(skel.get_children(0), vec![1]);
        assert_eq!(skel.get_children(1), vec![2]);
        assert!(skel.get_children(2).is_empty());
    }

    #[test]
    fn test_depth() {
        let skel = make_simple_skeleton();
        assert_eq!(skel.get_depth(0), 0);
        assert_eq!(skel.get_depth(1), 1);
        assert_eq!(skel.get_depth(2), 2);
    }

    #[test]
    fn test_scale() {
        let mut skel = make_simple_skeleton();
        skel.scale(2.0);
        assert!((skel.get_position(1).unwrap().y() - 2.0).abs() < 1e-10);
    }

    #[test]
    fn test_fit_to_bounds_tall_skeleton() {
        // Plantilla en y ∈ [0, 2]; malla de 170 de alto, desplazada
        let skel = make_simple_skeleton();
        let target = Rect::new(Vector3::new(100.0, 0.0, -20.0), Vector3::new(160.0, 170.0, 20.0));
        let fitted = fit_to_bounds(&skel, &target, 0.9);

        let b = skeleton_bounds(&fitted).unwrap();
        assert!((b.size().y() - 153.0).abs() < 1e-9);
        assert!(b.center().distance(&target.center()) < 1e-9);
        // Jerarquía y nombres intactos
        assert_eq!(fitted.get_parent(2), Some(1));
        assert_eq!(fitted.bones()[2].name, "head");
    }

    #[test]
    fn test_fit_to_bounds_flat_skeleton() {
        // Esqueleto horizontal (serpiente): se ajusta por la mayor extensión
        let mut skel = BasicSkeleton::new();
        skel.add_bone(Bone::new("a", Vector3::new(0.0, 0.0, 0.0)));
        skel.add_bone(Bone::with_parent("b", Vector3::new(0.0, 0.0, 4.0), 0));
        let target = Rect::new(Vector3::new(0.0, 0.0, 0.0), Vector3::new(1.0, 0.2, 10.0));
        let b = skeleton_bounds(&fit_to_bounds(&skel, &target, 0.9)).unwrap();
        assert!((b.size().z() - 9.0).abs() < 1e-9);
    }
}
