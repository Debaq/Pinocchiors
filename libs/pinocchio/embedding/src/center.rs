//! Centrar articulaciones en el volumen de la malla.
//!
//! Tras mover una articulación a mano desde una sola vista suele quedar
//! corrida hacia la superficie (o afuera). Centrarla la lleva al centro de la
//! sección del miembro: ascenso por el gradiente del campo de distancias con
//! signo, restringido al plano perpendicular al hueso, para que no se deslice
//! a lo largo del miembro ni salte a otro.

use pinocchio_math::{Real, Vector3};
use pinocchio_mesh::Mesh;
use pinocchio_skeleton::{BasicSkeleton, Bone, Skeleton};
use pinocchio_spatial::DistanceField;

/// Campo de distancias de una malla, reutilizable para centrar articulaciones.
pub struct JointCentering {
    field: DistanceField,
    cell: Real,
}

impl JointCentering {
    /// Campo con `resolution` celdas en el eje mayor de la malla.
    pub fn new(mesh: &Mesh, resolution: usize) -> Self {
        let bbox = mesh.bounding_box();
        let longest = bbox.longest_axis_length().max(1e-12);
        let padding = longest * 0.1;
        let size = bbox.size();
        let res = [size.x(), size.y(), size.z()]
            .map(|s| (((s + 2.0 * padding) / (longest + 2.0 * padding)) * resolution as Real).round().max(8.0) as usize);
        let field = DistanceField::from_mesh_signed(mesh, res, padding);
        let cell = (longest + 2.0 * padding) / resolution.max(1) as Real;
        Self { field, cell }
    }

    /// Distancia con signo a la superficie (positiva adentro).
    pub fn depth(&self, p: &Vector3) -> Real {
        self.field.sample(p)
    }

    fn gradient(&self, p: &Vector3) -> Vector3 {
        let h = 0.5 * self.cell;
        let axis = |d: Vector3| {
            let (a, b) = (self.field.sample(&(*p + d * h)), self.field.sample(&(*p - d * h)));
            if a.is_finite() && b.is_finite() { (a - b) / (2.0 * h) } else { 0.0 }
        };
        Vector3::new(axis(Vector3::unit_x()), axis(Vector3::unit_y()), axis(Vector3::unit_z()))
    }

    /// Punto más profundo alrededor de `p` en el plano perpendicular a
    /// `normal` (si `normal` es nula, en el espacio).
    pub fn center_point(&self, p: Vector3, normal: Vector3) -> Vector3 {
        let normal = normal.try_normalize();
        let mut point = p;
        let mut value = self.field.sample(&point);
        let mut step = self.cell;
        for _ in 0..400 {
            let mut g = self.gradient(&point);
            if let Some(n) = normal {
                g = g - n * n.dot(&g);
            }
            let Some(dir) = g.try_normalize() else { break };
            let candidate = point + dir * step;
            let v = self.field.sample(&candidate);
            if v > value {
                point = candidate;
                value = v;
            } else {
                step *= 0.5;
                if step < 0.05 * self.cell {
                    break;
                }
            }
        }
        point
    }

    /// El esqueleto con las articulaciones `joints` centradas en la sección
    /// del miembro. El plano de cada una es perpendicular a su hueso (la
    /// bisectriz si tiene un hijo, el hueso del padre si es hoja o bifurcación,
    /// o el primer hijo si es la raíz).
    pub fn center<S: Skeleton>(&self, skeleton: &S, joints: &[usize]) -> BasicSkeleton {
        let positions: Vec<Vector3> = skeleton.bones().iter().map(|b| b.position).collect();
        let mut bones: Vec<Bone> = skeleton.bones().to_vec();
        for &j in joints.iter().filter(|&&j| j < positions.len()) {
            let p = positions[j];
            let children = skeleton.get_children(j);
            let incoming = skeleton.get_parent(j).and_then(|q| (p - positions[q]).try_normalize());
            let outgoing = children.first().and_then(|&c| (positions[c] - p).try_normalize());
            let normal = match (incoming, outgoing, children.len()) {
                (Some(i), Some(o), 1) => (i + o).try_normalize().unwrap_or(i),
                (Some(i), _, _) => i,
                (None, Some(o), _) => o,
                (None, None, _) => Vector3::zero(),
            };
            bones[j].position = self.center_point(p, normal);
        }
        BasicSkeleton::from_bones(bones)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Prisma de sección cuadrada 1 × 1 a lo largo de Y, de 0 a 4
    fn bar() -> Mesh {
        let mut positions = Vec::new();
        for y in [0.0, 4.0] {
            for (x, z) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)] {
                positions.push(Vector3::new(x, y, z));
            }
        }
        let tris = [
            [0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4],
            [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7],
        ];
        Mesh::from_triangles(&positions, &tris)
    }

    #[test]
    fn joint_moves_to_the_middle_of_the_section_without_sliding() {
        let centering = JointCentering::new(&bar(), 64);
        let mut skel = BasicSkeleton::new();
        skel.add_bone(Bone::new("a", Vector3::new(0.5, 0.5, 0.5)));
        skel.add_bone(Bone::with_parent("b", Vector3::new(0.9, 2.0, 0.15), 0));
        skel.add_bone(Bone::with_parent("c", Vector3::new(0.5, 3.5, 0.5), 1));
        let centered = centering.center(&skel, &[1]);
        let p = centered.bones()[1].position;
        assert!((p.x() - 0.5).abs() < 0.06 && (p.z() - 0.5).abs() < 0.06, "centrada: {p:?}");
        assert!((p.y() - 2.0).abs() < 0.1, "no se desliza por el miembro: {p:?}");
        // Las demás no se tocan
        assert_eq!(centered.bones()[0].position, skel.bones()[0].position);
    }

    #[test]
    fn joint_outside_comes_back_inside() {
        let centering = JointCentering::new(&bar(), 64);
        let p = centering.center_point(Vector3::new(1.3, 2.0, 0.5), Vector3::unit_y());
        assert!(centering.depth(&p) > 0.4, "adentro y al centro: {p:?}");
    }
}
