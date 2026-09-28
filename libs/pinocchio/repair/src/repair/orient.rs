//! Orientación de las normales hacia afuera.

use crate::topology::{self, EdgeTopology};
use crate::trimesh::{signed_volume_of, TriMesh};
use pinocchio_math::{Real, Vector3};

/// Límite de trabajo (caras × puntos) para calcular el anidamiento de
/// cáscaras; por encima se asume que ninguna está dentro de otra.
const NESTING_BUDGET: usize = 200_000_000;

/// Puntos de cada cáscara que deben quedar dentro de otra para considerarla
/// anidada
const SAMPLES: usize = 8;

/// Orienta cada cáscara cerrada hacia afuera, respetando el anidamiento: una
/// cáscara dentro de un número impar de otras (la pared interior de un objeto
/// hueco) queda hacia adentro.
///
/// Las piezas abiertas no se tocan: no tienen un "afuera" definido. Requiere
/// que cada pieza ya esté orientada de forma consistente. Devuelve cuántas
/// caras volteó.
pub fn orient_outward(mesh: &mut TriMesh) -> usize {
    let topo = EdgeTopology::build(&mesh.triangles);
    let (component, count) = topology::face_components(&topo, mesh.num_faces());

    let mut faces: Vec<Vec<usize>> = vec![Vec::new(); count];
    for (f, &c) in component.iter().enumerate() {
        faces[c].push(f);
    }
    let mut closed = vec![true; count];
    for e in 0..topo.num_edges() {
        if topo.valence(e) != 2 {
            closed[component[topo.faces(e)[0].face]] = false;
        }
    }

    let shells: Vec<Shell> = (0..count)
        .filter(|&c| closed[c])
        .map(|c| Shell::new(mesh, &faces[c]))
        .collect();

    let pairs: usize = shells.iter().map(|s| s.faces.len()).sum::<usize>() * shells.len() * SAMPLES;
    let nesting = pairs <= NESTING_BUDGET;

    let mut flipped = 0;
    for (i, shell) in shells.iter().enumerate() {
        if shell.volume == 0.0 {
            continue;
        }
        let depth = if nesting {
            shells
                .iter()
                .enumerate()
                .filter(|&(j, other)| {
                    j != i && other.contains_box(shell) && shell.samples.iter().all(|&p| other.contains(mesh, p))
                })
                .count()
        } else {
            0
        };
        let want_positive = depth % 2 == 0;
        if (shell.volume > 0.0) != want_positive {
            for &f in &shell.faces {
                mesh.triangles[f].swap(1, 2);
            }
            flipped += shell.faces.len();
        }
    }
    flipped
}

struct Shell {
    faces: Vec<usize>,
    volume: Real,
    min: Vector3,
    max: Vector3,
    /// Puntos sobre la cáscara para probar si está dentro de otra
    samples: Vec<Vector3>,
}

impl Shell {
    fn new(mesh: &TriMesh, faces: &[usize]) -> Self {
        let volume = signed_volume_of(&mesh.positions, faces.iter().map(|&f| &mesh.triangles[f]));
        let mut min = Vector3::new(Real::INFINITY, Real::INFINITY, Real::INFINITY);
        let mut max = -min;
        for &f in faces {
            for p in mesh.corners(f) {
                min = min.min(&p);
                max = max.max(&p);
            }
        }
        // Centroides de caras repartidas por la cáscara: una cáscara está
        // dentro de otra solo si todos lo están (dos cuerpos solapados no
        // están anidados)
        let step = faces.len().div_ceil(SAMPLES);
        let samples = faces
            .iter()
            .step_by(step.max(1))
            .map(|&f| {
                let [a, b, c] = mesh.corners(f);
                (a + b + c) / 3.0
            })
            .collect();
        Self { faces: faces.to_vec(), volume, min, max, samples }
    }

    fn contains_box(&self, other: &Shell) -> bool {
        (0..3).all(|k| self.min[k] <= other.min[k] && other.max[k] <= self.max[k])
    }

    /// Número de vueltas (winding number) en `p`: ±1 dentro, 0 fuera
    fn contains(&self, mesh: &TriMesh, p: Vector3) -> bool {
        let solid_angle: Real = self
            .faces
            .iter()
            .map(|&f| {
                let [a, b, c] = mesh.corners(f).map(|q| q - p);
                let (la, lb, lc) = (a.length(), b.length(), c.length());
                let num = a.dot(&b.cross(&c));
                let den = la * lb * lc + a.dot(&b) * lc + a.dot(&c) * lb + b.dot(&c) * la;
                2.0 * num.atan2(den)
            })
            .sum();
        (solid_angle / (4.0 * std::f64::consts::PI)).abs() > 0.5
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube(center: Real, half: Real) -> TriMesh {
        let p = [
            [0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [0., 1., 0.],
            [0., 0., 1.], [1., 0., 1.], [1., 1., 1.], [0., 1., 1.],
        ];
        TriMesh::new(
            p.iter()
                .map(|&[x, y, z]| Vector3::new(x, y, z) * (2.0 * half) + Vector3::new(1., 1., 1.) * (center - half))
                .collect(),
            vec![
                [0, 2, 1], [0, 3, 2], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5],
                [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7], [4, 5, 6], [4, 6, 7],
            ],
        )
    }

    #[test]
    fn inverted_cube_is_flipped() {
        let mut m = cube(0.0, 1.0);
        for t in &mut m.triangles {
            t.swap(1, 2);
        }
        assert!(m.signed_volume() < 0.0);
        assert_eq!(orient_outward(&mut m), 12);
        assert!(m.signed_volume() > 0.0);
    }

    #[test]
    fn overlapping_cubes_are_not_nested() {
        // Dos cubos que se cruzan: el centro de una cara del segundo cae
        // dentro del primero, pero no están anidados
        let mut m = cube(0.0, 1.0);
        let other = cube(1.5, 1.0);
        let o = m.positions.len();
        m.positions.extend(&other.positions);
        m.triangles.extend(other.triangles.iter().map(|t| t.map(|v| v + o)));
        assert_eq!(orient_outward(&mut m), 0);
        assert!((m.signed_volume() - 16.0).abs() < 1e-9);
    }

    #[test]
    fn hollow_cube_keeps_inner_wall_inward() {
        let mut m = cube(0.0, 2.0);
        let inner = cube(0.0, 1.0);
        let o = m.positions.len();
        m.positions.extend(&inner.positions);
        // Interior mal orientado (hacia afuera)
        m.triangles.extend(inner.triangles.iter().map(|t| t.map(|v| v + o)));
        assert_eq!(orient_outward(&mut m), 12);
        // Volumen del hueco descontado: 4³ − 2³
        assert!((m.signed_volume() - 56.0).abs() < 1e-9);
    }
}
