//! Limpieza: caras inválidas, soldadura, degeneradas, duplicadas y piezas
//! sueltas.

use crate::analysis::degenerate::FaceShape;
use crate::analysis::{sorted, weld};
use crate::topology::{self, EdgeTopology, UnionFind};
use crate::trimesh::{is_finite, TriMesh};
use pinocchio_math::Real;

/// Elimina las caras con índices fuera de rango o repetidos, o con vértices
/// de coordenadas no finitas. Devuelve cuántas eliminó.
pub fn remove_invalid_faces(mesh: &mut TriMesh) -> usize {
    let positions = &mesh.positions;
    let n = positions.len();
    let before = mesh.triangles.len();
    mesh.triangles.retain(|t| {
        t.iter().all(|&v| v < n && is_finite(&positions[v])) && t[0] != t[1] && t[1] != t[2] && t[0] != t[2]
    });
    before - mesh.triangles.len()
}

/// Suelda los vértices a distancia ≤ `tolerance` (absoluta) que están sobre
/// aristas de borde coincidentes (ver [`weld::stitch_map`]).
///
/// Devuelve `(vértices fusionados, caras colapsadas)`: las caras que al soldar
/// quedan con un vértice repetido se eliminan. Los vértices que quedan sin
/// uso no se borran aquí (ver [`TriMesh::remove_unreferenced_vertices`]).
pub fn weld_vertices(mesh: &mut TriMesh, tolerance: Real) -> (usize, usize) {
    let map = weld::stitch_map(&mesh.positions, &mesh.triangles, tolerance);
    let mut used = vec![false; mesh.positions.len()];
    for t in &mesh.triangles {
        for &v in t {
            used[v] = true;
        }
    }
    let merged = (0..map.len()).filter(|&v| used[v] && map[v] != v).count();
    for t in &mut mesh.triangles {
        *t = t.map(|v| map[v]);
    }
    let collapsed = mesh.retain_faces(|_, t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2]);
    (merged, collapsed)
}

/// Corrige las caras de área nula sin abrir agujeros.
///
/// Una cara es degenerada si su altura sobre la arista más larga `ab` no
/// supera `tolerance`: el vértice opuesto `c` está (casi) sobre `ab`.
///
/// - **Aguja**: si `c` está muy cerca de `a` o `b` (≤ 10·`tolerance`), se
///   colapsa esa arista corta y la cara desaparece con ella.
/// - **Gorra**: si no, se elimina la cara y cada vecina por `ab` se parte en
///   `c`, así el borde que dejaba queda cubierto (es la corrección de una
///   unión en T). Si `ab` es de borde, o si la gorra cuelga solo de `ab`,
///   basta con eliminarla.
///
/// Ambas operaciones mueven la superficie a lo sumo 10·`tolerance`. Devuelve
/// el número de caras degeneradas corregidas.
pub fn fix_degenerate_faces(mesh: &mut TriMesh, tolerance: Real) -> usize {
    let mut fixed = 0;
    for _ in 0..16 {
        let changed = collapse_needles(mesh, tolerance) + split_caps(mesh, tolerance);
        if changed == 0 {
            break;
        }
        fixed += changed;
    }
    fixed
}

/// Distancia bajo la cual el vértice de una cara degenerada se colapsa sobre
/// el extremo más cercano de la arista larga, en múltiplos de la tolerancia
const NEEDLE_FACTOR: Real = 10.0;

fn is_needle(shape: &FaceShape, tolerance: Real) -> bool {
    shape.nearest_to_apex().2 <= NEEDLE_FACTOR * tolerance
}

fn collapse_needles(mesh: &mut TriMesh, tolerance: Real) -> usize {
    let mut uf = UnionFind::new(mesh.num_vertices());
    let mut any = false;
    for (f, t) in mesh.triangles.iter().enumerate() {
        let shape = FaceShape::new(mesh.corners(f));
        if !shape.is_degenerate(tolerance) {
            continue;
        }
        if shape.lengths[shape.longest] <= NEEDLE_FACTOR * tolerance {
            // Los tres vértices son prácticamente el mismo punto
            any |= uf.union(t[0], t[1]) | uf.union(t[1], t[2]);
        } else if is_needle(&shape, tolerance) {
            let (apex, end, _) = shape.nearest_to_apex();
            any |= uf.union(t[apex], t[end]);
        }
    }
    if !any {
        return 0;
    }
    // Representante: el menor índice de cada grupo (conserva su posición)
    let n = mesh.num_vertices();
    let mut smallest = vec![usize::MAX; n];
    for v in 0..n {
        let r = uf.find(v);
        smallest[r] = smallest[r].min(v);
    }
    for t in &mut mesh.triangles {
        *t = t.map(|v| smallest[uf.find(v)]);
    }
    mesh.retain_faces(|_, t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2])
}

fn split_caps(mesh: &mut TriMesh, tolerance: Real) -> usize {
    let topo = EdgeTopology::build(&mesh.triangles);
    let original = mesh.num_faces();
    let mut touched = vec![false; original];
    let mut removed = vec![false; original];
    let mut fixed = 0;

    for f in 0..original {
        if touched[f] {
            continue;
        }
        let shape = FaceShape::new(mesh.corners(f));
        if !shape.is_degenerate(tolerance) || is_needle(&shape, tolerance) {
            continue;
        }
        let t = mesh.triangles[f];
        let long = shape.longest;
        let c = topology::opposite(&t, long as u8);
        let e = topo.face_edges[f][long];
        // Astilla colgante: sus aristas cortas no tocan nada, sobra entera
        if (1..3).all(|k| topo.valence(topo.face_edges[f][(long + k) % 3]) == 1) {
            touched[f] = true;
            removed[f] = true;
            fixed += 1;
            continue;
        }
        let others: Vec<_> = topo.faces(e).iter().copied().filter(|fe| fe.face != f).collect();
        if others.iter().any(|fe| touched[fe.face]) {
            continue;
        }
        // Partir la vecina crea la arista c-d; si ya existe quedaría non-manifold
        let creates_existing_edge = others.iter().any(|fe| {
            let d = topology::opposite(&mesh.triangles[fe.face], fe.edge);
            d != c && topo.find(c, d).is_some()
        });
        if creates_existing_edge {
            continue;
        }
        for fe in others {
            let g = fe.face;
            let d = topology::opposite(&mesh.triangles[g], fe.edge);
            touched[g] = true;
            if d == c {
                // La vecina es la misma gorra con la otra orientación
                removed[g] = true;
                continue;
            }
            let (x, y) = topology::directed(&mesh.triangles, fe);
            mesh.triangles[g] = [x, c, d];
            mesh.triangles.push([c, y, d]);
        }
        touched[f] = true;
        removed[f] = true;
        fixed += 1;
    }

    if fixed > 0 {
        mesh.retain_faces(|i, _| i >= original || !removed[i]);
    }
    fixed
}

/// Elimina las caras repetidas (mismos tres vértices).
///
/// Las copias con orientación opuesta se cancelan de a pares: dos cuerpos que
/// se tocan por una cara comparten esa pared dos veces, una por cada lado, y
/// quitarla une los cuerpos en un sólido cerrado. De cada grupo queda a lo
/// sumo una cara con la orientación mayoritaria. Devuelve cuántas eliminó.
pub fn remove_duplicate_faces(mesh: &mut TriMesh) -> usize {
    let mut order: Vec<usize> = (0..mesh.num_faces()).collect();
    let keys: Vec<[usize; 3]> = mesh.triangles.iter().map(|t| sorted(*t)).collect();
    order.sort_unstable_by_key(|&f| (keys[f], f));

    let mut keep = vec![true; mesh.num_faces()];
    let mut start = 0;
    while start < order.len() {
        let mut end = start + 1;
        while end < order.len() && keys[order[end]] == keys[order[start]] {
            end += 1;
        }
        if end - start > 1 {
            let group = &order[start..end];
            let reference = mesh.triangles[group[0]];
            let (same, opposite): (Vec<usize>, Vec<usize>) =
                group.iter().partition(|&&f| same_orientation(&mesh.triangles[f], &reference));
            let survivor = match same.len().cmp(&opposite.len()) {
                std::cmp::Ordering::Greater => Some(same[0]),
                std::cmp::Ordering::Less => Some(opposite[0]),
                std::cmp::Ordering::Equal => None,
            };
            for &f in group {
                keep[f] = Some(f) == survivor;
            }
        }
        start = end;
    }
    mesh.retain_faces(|f, _| keep[f])
}

fn same_orientation(a: &[usize; 3], b: &[usize; 3]) -> bool {
    let i = b.iter().position(|&v| v == a[0]).expect("mismos vértices");
    b[(i + 1) % 3] == a[1]
}

/// Elimina las piezas (componentes conexas) cuya área es menor que `ratio`
/// veces el área de la pieza más grande.
///
/// Devuelve `(piezas eliminadas, caras eliminadas)`.
pub fn remove_small_components(mesh: &mut TriMesh, ratio: Real) -> (usize, usize) {
    let topo = EdgeTopology::build(&mesh.triangles);
    let (component, count) = topology::face_components(&topo, mesh.num_faces());
    if count < 2 {
        return (0, 0);
    }
    let mut area = vec![0.0; count];
    for (f, &c) in component.iter().enumerate() {
        area[c] += mesh.face_area(f);
    }
    let largest = area.iter().cloned().fold(0.0, Real::max);
    let small: Vec<bool> = area.iter().map(|&a| a < ratio * largest).collect();
    let removed_components = small.iter().filter(|&&s| s).count();
    let removed_faces = mesh.retain_faces(|f, _| !small[component[f]]);
    (removed_components, removed_faces)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::analyze_trimesh;
    use crate::config::AnalysisConfig;
    use pinocchio_math::Vector3;

    fn v(x: Real, y: Real, z: Real) -> Vector3 {
        Vector3::new(x, y, z)
    }

    #[test]
    fn invalid_faces() {
        let mut m = TriMesh::new(
            vec![v(0., 0., 0.), v(1., 0., 0.), v(0., 1., 0.), v(Real::NAN, 0., 0.)],
            vec![[0, 1, 2], [0, 0, 1], [0, 1, 7], [0, 1, 3]],
        );
        assert_eq!(remove_invalid_faces(&mut m), 3);
        assert_eq!(m.triangles, vec![[0, 1, 2]]);
    }

    #[test]
    fn weld_collapses_faces() {
        let mut m = TriMesh::new(
            vec![v(0., 0., 0.), v(1., 0., 0.), v(0., 1., 0.), v(0., 0., 0.), v(1., 0., 0.), v(0.5, -1., 0.)],
            vec![[0, 1, 2], [3, 5, 4], [0, 3, 2]],
        );
        let (merged, collapsed) = weld_vertices(&mut m, 1e-9);
        assert_eq!((merged, collapsed), (2, 1));
        assert_eq!(m.triangles, vec![[0, 1, 2], [0, 5, 1]]);
    }

    /// Tira de dos triángulos con una unión en T: el vértice 4 está sobre la
    /// arista 0-1 y la gorra [0, 4, 1] la tapa. Quitar la gorra sin más
    /// dejaría un agujero.
    #[test]
    fn cap_is_absorbed_without_opening_a_hole() {
        let mut m = TriMesh::new(
            vec![v(0., 0., 0.), v(2., 0., 0.), v(1., 1., 0.), v(1., -1., 0.), v(1., 0., 0.)],
            vec![[0, 1, 2], [0, 3, 4], [4, 3, 1], [0, 4, 1]],
        );
        let before = analyze_trimesh(&m, &AnalysisConfig::default());
        assert_eq!(before.degenerate_faces, 1);
        let area = m.area();

        assert_eq!(fix_degenerate_faces(&mut m, 1e-9), 1);
        let after = analyze_trimesh(&m, &AnalysisConfig::default());
        assert_eq!(after.degenerate_faces, 0);
        assert_eq!(after.boundary_loops, 1, "solo el contorno exterior");
        assert_eq!(after.boundary_edges, 4);
        assert!(after.normals_consistent);
        assert!((m.area() - area).abs() < 1e-12);
    }

    #[test]
    fn duplicate_faces_cancel_by_orientation() {
        let mut m = TriMesh::new(
            vec![v(0., 0., 0.), v(1., 0., 0.), v(0., 1., 0.)],
            vec![[0, 1, 2], [1, 2, 0], [0, 2, 1]],
        );
        // Dos con una orientación y una con la otra: queda una
        assert_eq!(remove_duplicate_faces(&mut m), 2);
        assert_eq!(m.triangles, vec![[0, 1, 2]]);

        let mut wall = TriMesh::new(m.positions.clone(), vec![[0, 1, 2], [0, 2, 1]]);
        assert_eq!(remove_duplicate_faces(&mut wall), 2);
        assert!(wall.triangles.is_empty());
    }

    #[test]
    fn small_components() {
        let mut m = TriMesh::new(
            vec![v(0., 0., 0.), v(10., 0., 0.), v(0., 10., 0.), v(5., 5., 5.), v(5.1, 5., 5.), v(5., 5.1, 5.)],
            vec![[0, 1, 2], [3, 4, 5]],
        );
        assert_eq!(remove_small_components(&mut m, 0.01), (1, 1));
        assert_eq!(m.triangles, vec![[0, 1, 2]]);
    }
}
