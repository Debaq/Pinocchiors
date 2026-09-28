//! Piezas sueltas: componentes de la malla sin ningún hueso adentro.
//!
//! Colmillos, ojos, dientes o accesorios suelen ser piezas separadas del
//! cuerpo. La difusión de calor les asigna el hueso visible más cercano, que
//! puede ser cualquiera (la punta de un colmillo queda cerca de la pata
//! delantera). Lo correcto es que la pieza se mueva rígida con la parte del
//! cuerpo donde se apoya: todos sus vértices toman los pesos del punto del
//! cuerpo más cercano a ella.

use pinocchio_math::{Real, Vector3};
use pinocchio_mesh::Mesh;
use pinocchio_skeleton::Skeleton;
use pinocchio_spatial::{Bvh, Triangle};
use rayon::prelude::*;

/// Componentes conexas por vértices compartidos: índice de componente de
/// cada vértice y cantidad de componentes.
fn components(mesh: &Mesh) -> (Vec<usize>, usize) {
    let n = mesh.num_vertices();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    for f in 0..mesh.num_faces() {
        let [a, b, c] = mesh.get_face_vertices(f);
        for (x, y) in [(a, b), (b, c)] {
            let (rx, ry) = (find(&mut parent, x), find(&mut parent, y));
            if rx != ry {
                parent[rx] = ry;
            }
        }
    }
    let mut ids = std::collections::HashMap::new();
    let component = (0..n)
        .map(|v| {
            let root = find(&mut parent, v);
            let next = ids.len();
            *ids.entry(root).or_insert(next)
        })
        .collect();
    (component, ids.len())
}

/// Paridad de cruces de un rayo desde `p` con los triángulos `tris`: impar =
/// adentro. La dirección es oblicua para no rozar aristas alineadas a los ejes.
fn inside(p: &Vector3, tris: &[[Vector3; 3]]) -> bool {
    let dir = Vector3::new(0.5773, 0.5774, 0.5775);
    let mut crossings = 0;
    for [a, b, c] in tris {
        // Möller–Trumbore
        let (e1, e2) = (*b - *a, *c - *a);
        let h = dir.cross(&e2);
        let det = e1.dot(&h);
        if det.abs() < 1e-14 {
            continue;
        }
        let inv = 1.0 / det;
        let s = *p - *a;
        let u = s.dot(&h) * inv;
        if !(0.0..=1.0).contains(&u) {
            continue;
        }
        let q = s.cross(&e1);
        let v = dir.dot(&q) * inv;
        if v < 0.0 || u + v > 1.0 {
            continue;
        }
        if e2.dot(&q) * inv > 0.0 {
            crossings += 1;
        }
    }
    crossings % 2 == 1
}

/// Hace rígidas las piezas sueltas sin huesos: cada una toma los pesos del
/// punto más cercano de las piezas con huesos. `weights` es `[vértice][hueso]`.
/// La componente de mayor área siempre cuenta como pieza con huesos.
pub fn attach_detached_parts<S: Skeleton>(mesh: &Mesh, skeleton: &S, weights: &mut [Vec<Real>]) {
    let (component, count) = components(mesh);
    if count < 2 {
        return;
    }
    let mut faces_of: Vec<Vec<usize>> = vec![Vec::new(); count];
    for f in 0..mesh.num_faces() {
        faces_of[component[mesh.get_face_vertices(f)[0]]].push(f);
    }
    let area = |c: usize| -> Real {
        faces_of[c]
            .iter()
            .map(|&f| {
                let [a, b, cc] = mesh.get_face_positions(f);
                (b - a).cross(&(cc - a)).length()
            })
            .sum()
    };
    let largest = (0..count).max_by(|&a, &b| area(a).total_cmp(&area(b))).expect("hay componentes");

    // Puntos de muestra de cada hueso: articulaciones y el medio del segmento
    let samples: Vec<Vector3> = (0..skeleton.num_bones())
        .flat_map(|b| {
            let p = skeleton.get_bone(b).expect("hueso").position;
            let mid = skeleton.get_parent(b).map(|q| (p + skeleton.get_bone(q).expect("padre").position) * 0.5);
            std::iter::once(p).chain(mid)
        })
        .collect();

    let boned: Vec<bool> = (0..count)
        .into_par_iter()
        .map(|c| {
            if c == largest || faces_of[c].is_empty() {
                return true;
            }
            let tris: Vec<[Vector3; 3]> = faces_of[c].iter().map(|&f| mesh.get_face_positions(f)).collect();
            samples.iter().any(|p| inside(p, &tris))
        })
        .collect();
    if boned.iter().all(|&b| b) {
        return;
    }

    // Superficie de las piezas con huesos
    let body: Vec<usize> = (0..mesh.num_faces()).filter(|&f| boned[component[mesh.get_face_vertices(f)[0]]]).collect();
    let bvh = Bvh::build(
        body.iter()
            .map(|&f| {
                let [a, b, c] = mesh.get_face_positions(f);
                Triangle::new(a, b, c)
            })
            .collect(),
    );

    let num_bones = weights.first().map_or(0, Vec::len);
    for c in (0..count).filter(|&c| !boned[c]) {
        // Vértice de la pieza más cercano al cuerpo, y su punto de apoyo
        let vertices: Vec<usize> = (0..mesh.num_vertices()).filter(|&v| component[v] == c).collect();
        let Some((hit, _)) = vertices
            .par_iter()
            .filter_map(|&v| bvh.query_closest(&mesh.vertices[v].position).map(|h| (h, v)))
            .min_by(|a, b| a.0.distance.total_cmp(&b.0.distance))
        else {
            continue;
        };
        let face = body[hit.triangle];
        let corners = mesh.get_face_vertices(face);
        let [a, b, cc] = mesh.get_face_positions(face);
        let bary = barycentric(&hit.point, &a, &b, &cc);
        let mut support = vec![0.0; num_bones];
        for (w, &v) in bary.iter().zip(&corners) {
            for (s, &x) in support.iter_mut().zip(&weights[v]) {
                *s += w * x;
            }
        }
        let sum: Real = support.iter().sum();
        if sum > 1e-12 {
            support.iter_mut().for_each(|s| *s /= sum);
        }
        for &v in &vertices {
            weights[v] = support.clone();
        }
    }
}

/// Baricéntricas de `p` (sobre el triángulo) respecto de `a, b, c`.
fn barycentric(p: &Vector3, a: &Vector3, b: &Vector3, c: &Vector3) -> [Real; 3] {
    let (v0, v1, v2) = (*b - *a, *c - *a, *p - *a);
    let (d00, d01, d11) = (v0.dot(&v0), v0.dot(&v1), v1.dot(&v1));
    let (d20, d21) = (v2.dot(&v0), v2.dot(&v1));
    let denom = d00 * d11 - d01 * d01;
    if denom.abs() < 1e-30 {
        return [1.0, 0.0, 0.0];
    }
    let v = ((d11 * d20 - d01 * d21) / denom).clamp(0.0, 1.0);
    let w = ((d00 * d21 - d01 * d20) / denom).clamp(0.0, 1.0);
    let u = (1.0 - v - w).max(0.0);
    let sum = u + v + w;
    [u / sum, v / sum, w / sum]
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinocchio_skeleton::{BasicSkeleton, Bone};

    /// Caja cerrada de 12 triángulos entre `lo` y `hi`, con vértices desde `base`.
    fn cube(lo: [Real; 3], hi: [Real; 3], positions: &mut Vec<Vector3>, triangles: &mut Vec<[usize; 3]>) {
        let base = positions.len();
        for i in 0..8 {
            let pick = |k: usize, bit: usize| if i >> bit & 1 == 1 { hi[k] } else { lo[k] };
            positions.push(Vector3::new(pick(0, 0), pick(1, 1), pick(2, 2)));
        }
        for [a, b, c] in [
            [0, 2, 1], [1, 2, 3], [4, 5, 6], [5, 7, 6], [0, 1, 4], [1, 5, 4],
            [2, 6, 3], [3, 6, 7], [0, 4, 2], [2, 4, 6], [1, 3, 5], [3, 7, 5],
        ] {
            triangles.push([base + a, base + b, base + c]);
        }
    }

    #[test]
    fn boneless_piece_follows_the_body_it_touches() {
        // Cuerpo alto con dos huesos; una pieza suelta junto a su parte alta
        let (mut positions, mut triangles) = (Vec::new(), Vec::new());
        cube([0.0, 0.0, 0.0], [1.0, 4.0, 1.0], &mut positions, &mut triangles);
        cube([1.2, 3.0, 0.0], [2.0, 3.8, 1.0], &mut positions, &mut triangles);
        let mesh = Mesh::from_triangles(&positions, &triangles);
        let mut skel = BasicSkeleton::new();
        skel.add_bone(Bone::new("root", Vector3::new(0.5, 0.2, 0.5)));
        skel.add_bone(Bone::with_parent("low", Vector3::new(0.5, 2.0, 0.5), 0));
        skel.add_bone(Bone::with_parent("high", Vector3::new(0.5, 3.8, 0.5), 1));

        // Pesos del cuerpo: abajo "low", arriba "high"; la pieza, basura
        let mut weights: Vec<Vec<Real>> = mesh
            .vertices
            .iter()
            .enumerate()
            .map(|(v, vert)| {
                if v >= 8 {
                    vec![0.0, 1.0, 0.0]
                } else if vert.position.y() > 2.0 {
                    vec![0.0, 0.0, 1.0]
                } else {
                    vec![0.0, 1.0, 0.0]
                }
            })
            .collect();
        attach_detached_parts(&mesh, &skel, &mut weights);
        // Rígida (todos iguales) y dominada por la parte alta, donde se apoya
        for w in &weights[8..] {
            assert_eq!(w, &weights[8]);
            assert!(w[2] > 0.6 && w[1] < 0.4, "la pieza sigue a la parte alta: {w:?}");
        }
        // El cuerpo no cambia
        assert_eq!(weights[0], vec![0.0, 1.0, 0.0]);
    }

    #[test]
    fn piece_with_a_bone_inside_keeps_its_weights() {
        let (mut positions, mut triangles) = (Vec::new(), Vec::new());
        cube([0.0, 0.0, 0.0], [1.0, 4.0, 1.0], &mut positions, &mut triangles);
        cube([1.2, 0.0, 0.0], [2.0, 1.0, 1.0], &mut positions, &mut triangles);
        let mesh = Mesh::from_triangles(&positions, &triangles);
        let mut skel = BasicSkeleton::new();
        skel.add_bone(Bone::new("root", Vector3::new(0.5, 0.2, 0.5)));
        skel.add_bone(Bone::with_parent("arm", Vector3::new(1.6, 0.5, 0.5), 0));
        let mut weights = vec![vec![0.3, 0.7]; mesh.num_vertices()];
        let before = weights.clone();
        attach_detached_parts(&mesh, &skel, &mut weights);
        assert_eq!(weights, before);
    }
}
