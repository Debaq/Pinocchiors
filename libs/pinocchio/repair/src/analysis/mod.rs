//! Análisis de mallas: solo lectura.
//!
//! La topología se evalúa sobre la malla soldada, así un modelo con vértices
//! duplicados en las costuras (glTF, OBJ) no aparece lleno de agujeros falsos:
//! los duplicados se informan aparte.

pub mod degenerate;
pub mod intersections;
pub mod weld;

use crate::config::{AnalysisConfig, MeshDiagnostics};
use crate::topology::{self, EdgeTopology, UnionFind};
use crate::trimesh::{is_finite, TriMesh};
use degenerate::{classify_face, FaceQuality};

/// Analiza una malla indexada
pub fn analyze_trimesh(mesh: &TriMesh, config: &AnalysisConfig) -> MeshDiagnostics {
    let mut d = MeshDiagnostics {
        num_vertices: mesh.num_vertices(),
        num_faces: mesh.num_faces(),
        normals_consistent: true,
        ..Default::default()
    };

    let n = mesh.num_vertices();
    let valid = |t: &[usize; 3]| {
        t.iter().all(|&v| v < n && is_finite(&mesh.positions[v])) && t[0] != t[1] && t[1] != t[2] && t[0] != t[2]
    };

    // Vértices sin usar (sobre las caras con índices en rango)
    let mut used = vec![false; n];
    for t in &mesh.triangles {
        for &v in t.iter().filter(|&&v| v < n) {
            used[v] = true;
        }
    }
    d.unreferenced_vertices = used.iter().filter(|&&u| !u).count();

    // Coser costuras (vértices duplicados a lo largo de bordes coincidentes)
    let mut valid_faces = Vec::with_capacity(mesh.num_faces());
    for t in &mesh.triangles {
        if valid(t) {
            valid_faces.push(*t);
        } else {
            d.invalid_faces += 1;
        }
    }
    let diagonal = mesh.diagonal();
    let map = weld::stitch_map(&mesh.positions, &valid_faces, config.duplicate_tolerance * diagonal);
    d.duplicate_vertices = (0..n).filter(|&v| used[v] && map[v] != v).count();

    let mut triangles = Vec::with_capacity(valid_faces.len());
    for t in &valid_faces {
        let w = t.map(|v| map[v]);
        // Una cara que se colapsa al coser es degenerada, no inválida
        if w[0] == w[1] || w[1] == w[2] || w[0] == w[2] {
            d.degenerate_faces += 1;
            continue;
        }
        triangles.push(w);
    }
    let welded = TriMesh::new(mesh.positions.clone(), triangles);

    // Forma de los triángulos
    let tolerance = config.degenerate_tolerance * diagonal;
    for f in 0..welded.num_faces() {
        match classify_face(welded.corners(f), tolerance, config.needle_angle_threshold, config.cap_angle_threshold) {
            FaceQuality::Degenerate => d.degenerate_faces += 1,
            FaceQuality::Needle => d.needle_faces += 1,
            FaceQuality::Cap => d.cap_faces += 1,
            FaceQuality::Good => {}
        }
    }

    // Caras repetidas
    let mut keys: Vec<[usize; 3]> = welded.triangles.iter().map(|t| sorted(*t)).collect();
    keys.sort_unstable();
    d.duplicate_faces = keys.windows(2).filter(|w| w[0] == w[1]).count();

    // Aristas
    let topo = EdgeTopology::build(&welded.triangles);
    let mut boundary_vertices = UnionFind::new(n);
    let mut on_boundary = Vec::new();
    for e in 0..topo.num_edges() {
        match topo.valence(e) {
            1 => {
                d.boundary_edges += 1;
                let [a, b] = topo.edges[e];
                boundary_vertices.union(a, b);
                on_boundary.push(a);
            }
            2 => {
                let [f, g] = [topo.faces(e)[0], topo.faces(e)[1]];
                if topology::directed(&welded.triangles, f) == topology::directed(&welded.triangles, g) {
                    d.inconsistent_edges += 1;
                }
            }
            _ => d.non_manifold_edges += 1,
        }
    }
    // Agujeros: grupos conexos de aristas de borde
    let mut roots: Vec<usize> = on_boundary.iter().map(|&v| boundary_vertices.find(v)).collect();
    roots.sort_unstable();
    roots.dedup();
    d.boundary_loops = roots.len();

    d.non_manifold_vertices = count_non_manifold_vertices(&welded.triangles, &topo);
    d.connected_components = topology::face_components(&topo, welded.num_faces()).1;
    d.normals_consistent = d.inconsistent_edges == 0;
    d.is_closed = d.boundary_edges == 0 && d.non_manifold_edges == 0;
    d.is_manifold = d.non_manifold_edges == 0 && d.non_manifold_vertices == 0;

    d.area = welded.area();
    let volume = welded.signed_volume();
    d.volume = volume.abs();
    if d.is_closed && d.normals_consistent && welded.num_faces() > 0 {
        d.normals_outward = Some(volume > 0.0);
    }

    if config.check_self_intersections {
        d.self_intersections =
            intersections::find_self_intersections(&welded, 1e-9 * diagonal).count();
    }

    d
}

/// Vértices cuyas caras forman más de un abanico conectado
pub fn count_non_manifold_vertices(triangles: &[[usize; 3]], topo: &EdgeTopology) -> usize {
    let mut fans = topology::corner_fans(triangles, topology::manifold_links(topo));
    let nv = triangles.iter().flatten().copied().max().map_or(0, |m| m + 1);
    let mut first_fan = vec![usize::MAX; nv];
    let mut non_manifold = vec![false; nv];
    for c in 0..triangles.len() * 3 {
        let v = triangles[c / 3][c % 3];
        let fan = fans.find(c);
        if first_fan[v] == usize::MAX {
            first_fan[v] = fan;
        } else if first_fan[v] != fan {
            non_manifold[v] = true;
        }
    }
    non_manifold.iter().filter(|&&b| b).count()
}

pub(crate) fn sorted(mut t: [usize; 3]) -> [usize; 3] {
    t.sort_unstable();
    t
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinocchio_math::Vector3;

    fn cube(open_top: bool) -> TriMesh {
        let p = [
            [0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [0., 1., 0.],
            [0., 0., 1.], [1., 0., 1.], [1., 1., 1.], [0., 1., 1.],
        ];
        let mut t = vec![
            [0, 2, 1], [0, 3, 2], // abajo
            [0, 1, 5], [0, 5, 4], // frente
            [1, 2, 6], [1, 6, 5], // derecha
            [2, 3, 7], [2, 7, 6], // atrás
            [3, 0, 4], [3, 4, 7], // izquierda
        ];
        if !open_top {
            t.extend([[4, 5, 6], [4, 6, 7]]);
        }
        TriMesh::new(p.iter().map(|&[x, y, z]| Vector3::new(x, y, z)).collect(), t)
    }

    #[test]
    fn closed_cube_is_healthy() {
        let d = analyze_trimesh(&cube(false), &AnalysisConfig { check_self_intersections: true, ..Default::default() });
        assert!(d.is_closed && d.is_manifold && d.normals_consistent);
        assert_eq!(d.normals_outward, Some(true));
        assert!((d.volume - 1.0).abs() < 1e-12);
        assert!(d.is_healthy(), "{d:#?}");
        assert!(!d.needs_repair());
    }

    #[test]
    fn open_cube_has_one_hole() {
        let d = analyze_trimesh(&cube(true), &AnalysisConfig::default());
        assert_eq!(d.boundary_loops, 1);
        assert_eq!(d.boundary_edges, 4);
        assert!(!d.is_closed);
        assert!(d.needs_repair());
    }

    #[test]
    fn flipped_face_is_inconsistent_not_a_hole() {
        let mut m = cube(false);
        m.triangles[3] = [0, 4, 5];
        let d = analyze_trimesh(&m, &AnalysisConfig::default());
        assert_eq!(d.boundary_loops, 0);
        assert_eq!(d.inconsistent_edges, 3);
        assert!(!d.normals_consistent);
    }

    #[test]
    fn unwelded_seams_are_duplicates_not_holes() {
        let m = cube(false);
        // Cada cara con sus propios vértices, como un STL sin indexar
        let positions: Vec<Vector3> = m.triangles.iter().flat_map(|t| t.map(|v| m.positions[v])).collect();
        let triangles = (0..m.num_faces()).map(|f| [3 * f, 3 * f + 1, 3 * f + 2]).collect();
        let d = analyze_trimesh(&TriMesh::new(positions, triangles), &AnalysisConfig::default());
        assert_eq!(d.duplicate_vertices, 36 - 8);
        assert!(d.is_closed);
    }

    #[test]
    fn two_cubes_sharing_an_edge_are_non_manifold() {
        let a = cube(false);
        let mut m = a.clone();
        // Segundo cubo desplazado (1,1,0): comparte la arista vertical 6-2
        let offset = m.positions.len();
        m.positions.extend(a.positions.iter().map(|&p| p + Vector3::new(1.0, 1.0, 0.0)));
        m.triangles.extend(a.triangles.iter().map(|t| t.map(|v| v + offset)));
        // Cuerpos con vértices propios que solo se tocan: no son duplicados
        let apart = analyze_trimesh(&m, &AnalysisConfig::default());
        assert_eq!((apart.duplicate_vertices, apart.non_manifold_edges), (0, 0));
        // Compartiendo los vértices de la arista: non-manifold
        let map = weld::weld_map(&m.positions, 1e-9);
        for t in &mut m.triangles {
            *t = t.map(|v| map[v]);
        }
        let d = analyze_trimesh(&m, &AnalysisConfig::default());
        assert_eq!(d.non_manifold_edges, 1);
        assert_eq!(d.non_manifold_vertices, 2);
        assert!(!d.is_manifold);
    }

    #[test]
    fn invalid_faces_are_counted() {
        let mut m = cube(false);
        m.triangles.push([0, 0, 1]);
        m.triangles.push([0, 1, 99]);
        let d = analyze_trimesh(&m, &AnalysisConfig::default());
        assert_eq!(d.invalid_faces, 2);
    }
}
