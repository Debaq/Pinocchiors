//! Orientación consistente y separación de geometría non-manifold.
//!
//! Sigue la idea de `orient_polygon_soup` de CGAL: se orientan las caras
//! propagando por las aristas con exactamente dos caras, y luego cada vértice
//! se duplica una vez por cada abanico de caras que lo rodea. El resultado es
//! siempre una malla manifold orientable en la que ninguna arista tiene más de
//! dos caras, sin borrar una sola cara.
//!
//! En las aristas con más de dos caras, las caras se ordenan por ángulo
//! alrededor de la arista y cada una se empareja con su vecina hacia el lado
//! interior (el opuesto a su normal), si la elección es mutua. Así:
//!
//! - Dos cuerpos cerrados que se tocan por una arista o un vértice quedan como
//!   dos cuerpos cerrados independientes, sin abrirse en la línea de contacto.
//! - Una aleta pegada a una superficie cerrada se separa como pieza abierta.

use crate::topology::{self, EdgeTopology, FaceEdge};
use crate::trimesh::{signed_volume_of, TriMesh};
use pinocchio_math::Vector3;

/// Resultado de [`orient_and_split`]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ManifoldReport {
    /// Caras volteadas
    pub faces_flipped: usize,
    /// Vértices nuevos creados al separar abanicos
    pub vertices_split: usize,
    /// Aristas que siguen con orientación opuesta (superficie no orientable,
    /// como una banda de Möbius)
    pub non_orientable_edges: usize,
}

/// Orienta las caras de forma consistente (`orient`) y separa los abanicos
/// non-manifold (`split`).
///
/// En cada pieza abierta se conserva la orientación de la mayoría de las
/// caras; las piezas cerradas quedan hacia afuera.
pub fn orient_and_split(mesh: &mut TriMesh, orient: bool, split: bool) -> ManifoldReport {
    let topo = EdgeTopology::build(&mesh.triangles);
    let nf = mesh.num_faces();

    let mut flip = vec![false; nf];
    let mut cut = vec![false; topo.num_edges()];
    if orient {
        orient_components(mesh, &topo, &mut flip, &mut cut);
    }
    let mut report = ManifoldReport { faces_flipped: flip.iter().filter(|&&f| f).count(), ..Default::default() };

    let mut triangles = mesh.triangles.clone();
    for (t, &f) in triangles.iter_mut().zip(&flip) {
        if f {
            t.swap(1, 2);
        }
    }

    if split {
        // Pegar por las aristas manifold consistentes y por los pares radiales
        let manifold: Vec<(usize, usize, [usize; 2])> = (0..topo.num_edges())
            .filter(|&e| topo.valence(e) == 2 && !cut[e])
            .map(|e| (topo.faces(e)[0].face, topo.faces(e)[1].face, topo.edges[e]))
            .collect();
        let radial: Vec<(usize, usize, usize)> = (0..topo.num_edges())
            .filter(|&e| topo.valence(e) > 2)
            .flat_map(|e| radial_pairs(&mesh.positions, &triangles, &topo, e).into_iter().map(move |(f, g)| (e, f, g)))
            .collect();

        // Si los pares radiales unen dos cuerpos por varios lados, los
        // abanicos pueden volver a juntar más de dos caras en una arista: se
        // anulan los pares de esas aristas y se reintenta. Sin pares radiales
        // el resultado es siempre manifold.
        let mut disabled = vec![false; topo.num_edges()];
        const ATTEMPTS: usize = 8;
        for attempt in 0..=ATTEMPTS {
            let links = manifold.iter().copied().chain(
                radial
                    .iter()
                    .filter(|&&(e, _, _)| attempt < ATTEMPTS && !disabled[e])
                    .map(|&(e, f, g)| (f, g, topo.edges[e])),
            );
            let (split_tris, origin) = split_fans(&triangles, mesh.num_vertices(), links);
            let mut bad = false;
            // Sin pares radiales la separación es manifold por construcción
            if !radial.is_empty() && attempt < ATTEMPTS {
                let check = EdgeTopology::build(&split_tris);
                for e in (0..check.num_edges()).filter(|&e| check.valence(e) > 2) {
                    let [a, b] = check.edges[e];
                    if let Some(original) = topo.find(origin[a], origin[b]) {
                        disabled[original] = true;
                        bad = true;
                    }
                }
            }
            if !bad || attempt == ATTEMPTS {
                let nv = mesh.num_vertices();
                report.vertices_split = origin.len() - nv;
                let copies: Vec<Vector3> = origin[nv..].iter().map(|&v| mesh.positions[v]).collect();
                mesh.positions.extend(copies);
                triangles = split_tris;
                break;
            }
        }
    }
    mesh.triangles = triangles;

    if orient && cut.iter().any(|&c| c) {
        let topo = EdgeTopology::build(&mesh.triangles);
        report.non_orientable_edges = (0..topo.num_edges())
            .filter(|&e| {
                topo.valence(e) == 2
                    && topology::directed(&mesh.triangles, topo.faces(e)[0])
                        == topology::directed(&mesh.triangles, topo.faces(e)[1])
            })
            .count();
    }
    report
}

/// Asigna un vértice por abanico. Devuelve los triángulos nuevos y, para cada
/// vértice (los existentes más las copias), el vértice original.
fn split_fans(
    triangles: &[[usize; 3]],
    num_vertices: usize,
    links: impl IntoIterator<Item = (usize, usize, [usize; 2])>,
) -> (Vec<[usize; 3]>, Vec<usize>) {
    let mut fans = topology::corner_fans(triangles, links);
    let mut vertex_of_fan = vec![usize::MAX; 3 * triangles.len()];
    let mut origin: Vec<usize> = (0..num_vertices).collect();
    let mut claimed = vec![false; num_vertices];
    let mut out = triangles.to_vec();
    for c in 0..3 * triangles.len() {
        let fan = fans.find(c);
        if vertex_of_fan[fan] == usize::MAX {
            let v = triangles[c / 3][c % 3];
            vertex_of_fan[fan] = if claimed[v] {
                origin.push(v);
                origin.len() - 1
            } else {
                claimed[v] = true;
                v
            };
        }
        out[c / 3][c % 3] = vertex_of_fan[fan];
    }
    (out, origin)
}

/// Cose las rendijas que puede dejar [`orient_and_split`] donde varias
/// superficies se apoyan en el mismo plano: pares de aristas de borde con
/// extremos en la misma posición y sentidos opuestos se vuelven a unir. Los
/// grupos de tres o más aristas coincidentes se dejan como están.
///
/// Devuelve cuántos pares cosió. Conviene volver a llamar a
/// [`orient_and_split`] después, por si la costura dejó algún vértice
/// non-manifold.
pub fn zip_slits(mesh: &mut TriMesh) -> usize {
    let topo = EdgeTopology::build(&mesh.triangles);
    let group = crate::analysis::weld::weld_map(&mesh.positions, 0.0);
    let mut boundary: Vec<([usize; 2], (usize, usize))> = (0..topo.num_edges())
        .filter(|&e| topo.valence(e) == 1)
        .map(|e| {
            let (a, b) = topology::directed(&mesh.triangles, topo.faces(e)[0]);
            ([group[a].min(group[b]), group[a].max(group[b])], (a, b))
        })
        .collect();
    boundary.sort_unstable();

    let mut uf = topology::UnionFind::new(mesh.num_vertices());
    let mut zipped = 0;
    for run in boundary.chunk_by(|x, y| x.0 == y.0) {
        if let [(_, (a0, b0)), (_, (a1, b1))] = *run
            && group[a0] == group[b1]
            && group[b0] == group[a1]
        {
            uf.union(a0, b1);
            uf.union(b0, a1);
            zipped += 1;
        }
    }
    if zipped > 0 {
        for t in &mut mesh.triangles {
            *t = t.map(|v| uf.find(v));
        }
        mesh.retain_faces(|_, t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2]);
    }
    zipped
}

/// Propaga la orientación por las aristas de dos caras. Marca en `cut` las
/// aristas donde la propagación choca consigo misma (no orientable). Cada
/// pieza cerrada queda hacia afuera; cada pieza abierta, con la orientación
/// de la mayoría de sus caras.
fn orient_components(mesh: &TriMesh, topo: &EdgeTopology, flip: &mut [bool], cut: &mut [bool]) {
    let tris = &mesh.triangles;
    let nf = tris.len();
    let forward = |f: usize, l: u8| tris[f][l as usize] < tris[f][(l as usize + 1) % 3];

    let mut component = vec![usize::MAX; nf];
    let mut members: Vec<Vec<usize>> = Vec::new();
    let mut stack = Vec::new();
    for seed in 0..nf {
        if component[seed] != usize::MAX {
            continue;
        }
        let id = members.len();
        let mut faces = Vec::new();
        component[seed] = id;
        stack.push(seed);
        while let Some(f) = stack.pop() {
            faces.push(f);
            for l in 0..3u8 {
                let e = topo.face_edges[f][l as usize];
                if topo.valence(e) != 2 {
                    continue;
                }
                let other = *topo.faces(e).iter().find(|fe| fe.face != f).expect("dos caras");
                let g = other.face;
                // Consistentes si recorren la arista en sentidos opuestos
                let want = forward(g, other.edge) == (forward(f, l) != flip[f]);
                if component[g] == usize::MAX {
                    component[g] = id;
                    flip[g] = want;
                    stack.push(g);
                } else if flip[g] != want {
                    cut[e] = true;
                }
            }
        }
        members.push(faces);
    }

    // Una pieza es cerrada si cada arista tiene un número par de sus caras
    let mut closed = vec![true; members.len()];
    let mut local: Vec<usize> = Vec::new();
    for e in 0..topo.num_edges() {
        local.clear();
        local.extend(topo.faces(e).iter().map(|fe| component[fe.face]));
        local.sort_unstable();
        for run in local.chunk_by(|a, b| a == b) {
            if run.len() % 2 == 1 {
                closed[run[0]] = false;
            }
        }
    }

    for (id, faces) in members.iter().enumerate() {
        let invert = if closed[id] {
            let oriented = faces.iter().map(|&f| {
                let mut t = tris[f];
                if flip[f] {
                    t.swap(1, 2);
                }
                t
            });
            let oriented: Vec<[usize; 3]> = oriented.collect();
            signed_volume_of(&mesh.positions, oriented.iter()) < 0.0
        } else {
            2 * faces.iter().filter(|&&f| flip[f]).count() > faces.len()
        };
        if invert {
            for &f in faces {
                flip[f] = !flip[f];
            }
        }
    }
}

/// Empareja las caras de una arista non-manifold. `triangles` ya orientados.
///
/// Las caras se ordenan por ángulo alrededor de la arista; cada una elige a
/// su vecina angular del lado opuesto a su normal (el interior del sólido que
/// delimita). Solo se emparejan las elecciones mutuas.
fn radial_pairs(positions: &[Vector3], triangles: &[[usize; 3]], topo: &EdgeTopology, edge: usize) -> Vec<(usize, usize)> {
    let [a, b] = topo.edges[edge];
    let (pa, pb) = (positions[a], positions[b]);
    let Some(axis) = (pb - pa).try_normalize() else {
        return Vec::new();
    };

    struct Radial {
        face: usize,
        angle: f64,
        /// +1 si la normal gira en sentido positivo desde la cara, -1 si no
        side: f64,
    }
    let faces: &[FaceEdge] = topo.faces(edge);
    let mut radial: Vec<Radial> = Vec::with_capacity(faces.len());
    let mut basis: Option<(Vector3, Vector3)> = None;
    for fe in faces {
        let t = triangles[fe.face];
        let o = positions[topology::opposite(&t, fe.edge)];
        let w = (o - pa) - axis * (o - pa).dot(&axis);
        let normal = (positions[t[1]] - positions[t[0]]).cross(&(positions[t[2]] - positions[t[0]]));
        let Some(w_unit) = w.try_normalize() else { continue };
        let (e1, e2) = *basis.get_or_insert_with(|| (w_unit, axis.cross(&w_unit)));
        let side = w.cross(&normal).dot(&axis);
        if side == 0.0 {
            continue;
        }
        radial.push(Radial { face: fe.face, angle: w.dot(&e2).atan2(w.dot(&e1)), side: side.signum() });
    }
    if radial.len() < 2 {
        return Vec::new();
    }
    radial.sort_by(|x, y| x.angle.total_cmp(&y.angle));

    let n = radial.len();
    // El interior está hacia -side: el vecino en ese sentido angular
    let partner = |i: usize| if radial[i].side > 0.0 { (i + n - 1) % n } else { (i + 1) % n };
    (0..n)
        .filter(|&i| {
            let j = partner(i);
            j != i && partner(j) == i && i < j
        })
        .map(|i| (radial[i].face, radial[partner(i)].face))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::analyze_trimesh;
    use crate::config::AnalysisConfig;
    use pinocchio_math::Vector3;

    fn cube(offset: Vector3) -> TriMesh {
        let p = [
            [0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [0., 1., 0.],
            [0., 0., 1.], [1., 0., 1.], [1., 1., 1.], [0., 1., 1.],
        ];
        TriMesh::new(
            p.iter().map(|&[x, y, z]| Vector3::new(x, y, z) + offset).collect(),
            vec![
                [0, 2, 1], [0, 3, 2], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5],
                [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7], [4, 5, 6], [4, 6, 7],
            ],
        )
    }

    fn merge(a: &TriMesh, b: &TriMesh) -> TriMesh {
        let mut m = a.clone();
        let o = m.positions.len();
        m.positions.extend(&b.positions);
        m.triangles.extend(b.triangles.iter().map(|t| t.map(|v| v + o)));
        m
    }

    /// Fusiona todos los vértices coincidentes (no solo las costuras)
    fn welded(mut m: TriMesh) -> TriMesh {
        let map = crate::analysis::weld::weld_map(&m.positions, 1e-9);
        for t in &mut m.triangles {
            *t = t.map(|v| map[v]);
        }
        m.remove_unreferenced_vertices();
        m
    }

    #[test]
    fn flips_minority_faces() {
        let mut m = cube(Vector3::zero());
        m.triangles[4].swap(1, 2);
        m.triangles[9].swap(1, 2);
        let r = orient_and_split(&mut m, true, true);
        assert_eq!(r.faces_flipped, 2);
        assert_eq!(r.vertices_split, 0);
        let d = analyze_trimesh(&m, &AnalysisConfig::default());
        assert!(d.normals_consistent && d.is_closed);
        assert_eq!(d.normals_outward, Some(true));
    }

    #[test]
    fn cubes_sharing_an_edge_become_two_closed_cubes() {
        let mut m = welded(merge(&cube(Vector3::zero()), &cube(Vector3::new(1.0, 1.0, 0.0))));
        let r = orient_and_split(&mut m, true, true);
        assert_eq!(r.vertices_split, 2);
        let d = analyze_trimesh(&m, &AnalysisConfig { duplicate_tolerance: 0.0, ..Default::default() });
        assert!(d.is_closed && d.is_manifold, "{d:#?}");
        assert_eq!(d.connected_components, 2);
        assert_eq!(m.num_faces(), 24);
    }

    /// Parte la arista a-b de todas sus caras en el vértice nuevo `m`
    fn split_edge(m: &mut TriMesh, a: usize, b: usize, at: Vector3) {
        let mid = m.positions.len();
        m.positions.push(at);
        let mut extra = Vec::new();
        for t in &mut m.triangles {
            if let Some(i) = (0..3).find(|&i| [t[i], t[(i + 1) % 3]] == [a, b] || [t[i], t[(i + 1) % 3]] == [b, a]) {
                let (x, y, o) = (t[i], t[(i + 1) % 3], t[(i + 2) % 3]);
                *t = [x, mid, o];
                extra.push([mid, y, o]);
            }
        }
        m.triangles.extend(extra);
    }

    /// Dos cubos que se tocan a lo largo de una arista partida en dos: el
    /// vértice del medio está sobre dos aristas non-manifold seguidas. Separar
    /// solo por abanicos abriría cada cubo en una rendija.
    #[test]
    fn contact_line_does_not_open_slits() {
        let mut a = cube(Vector3::zero());
        split_edge(&mut a, 2, 6, Vector3::new(1.0, 1.0, 0.5));
        let mut b = cube(Vector3::new(1.0, 1.0, 0.0));
        split_edge(&mut b, 0, 4, Vector3::new(1.0, 1.0, 0.5));
        let mut m = welded(merge(&a, &b));
        let before = analyze_trimesh(&m, &AnalysisConfig::default());
        assert_eq!(before.non_manifold_edges, 2);

        let r = orient_and_split(&mut m, true, true);
        assert_eq!(r.vertices_split, 3);
        let d = analyze_trimesh(&m, &AnalysisConfig::default());
        assert!(d.is_closed && d.is_manifold, "{d:#?}");
        assert_eq!(d.boundary_edges, 0);
        assert_eq!(d.connected_components, 2);
        assert!((d.volume - 2.0).abs() < 1e-12);
    }

    #[test]
    fn cubes_sharing_a_vertex_are_separated() {
        let mut m = welded(merge(&cube(Vector3::zero()), &cube(Vector3::new(1.0, 1.0, 1.0))));
        let r = orient_and_split(&mut m, true, true);
        assert_eq!(r.vertices_split, 1);
        let d = analyze_trimesh(&m, &AnalysisConfig { duplicate_tolerance: 0.0, ..Default::default() });
        assert!(d.is_closed && d.is_manifold);
    }

    #[test]
    fn fin_on_closed_surface_is_detached() {
        let mut m = cube(Vector3::zero());
        // Aleta sobre la arista 4-5 (arriba al frente)
        m.positions.push(Vector3::new(0.5, -1.0, 1.5));
        m.triangles.push([4, 5, 8]);
        let r = orient_and_split(&mut m, true, true);
        assert_eq!(r.vertices_split, 2);
        let d = analyze_trimesh(&m, &AnalysisConfig { duplicate_tolerance: 0.0, ..Default::default() });
        assert_eq!(d.non_manifold_edges, 0);
        assert!(d.is_manifold);
        assert_eq!(d.boundary_loops, 1, "solo el borde de la aleta");
        assert_eq!(d.connected_components, 2);
    }
}
