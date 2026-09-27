//! Retopología de superficies sintéticas: cantidad de caras, topología,
//! fidelidad a la superficie, calidad de los quads y restricciones.

use pinocchio_math::nalgebra::Vector3 as V3;
use pinocchio_math::Vector3;
use pinocchio_mesh::Mesh;
use quadriflow_core::{remesh, remesh_with_callback, QuadMesh, RemeshConfig, RemeshError, RemeshStage};
use std::collections::HashMap;
use std::f64::consts::PI;

fn config(target_faces: usize) -> RemeshConfig {
    RemeshConfig { target_faces, ..Default::default() }
}

/// Grilla (u, v) ∈ [0,1]² mapeada por `f`, opcionalmente cerrada en u y/o v.
fn param_surface(nu: usize, nv: usize, wrap_u: bool, wrap_v: bool, f: impl Fn(f64, f64) -> [f64; 3]) -> (Vec<Vector3>, Vec<[usize; 3]>) {
    let cu = if wrap_u { nu } else { nu + 1 };
    let cv = if wrap_v { nv } else { nv + 1 };
    let mut positions = Vec::new();
    for i in 0..cu {
        for j in 0..cv {
            let [x, y, z] = f(i as f64 / nu as f64, j as f64 / nv as f64);
            positions.push(Vector3::new(x, y, z));
        }
    }
    let idx = |i: usize, j: usize| (i % cu) * cv + (j % cv);
    let mut triangles = Vec::new();
    for i in 0..nu {
        for j in 0..nv {
            let (a, b, c, d) = (idx(i, j), idx(i + 1, j), idx(i + 1, j + 1), idx(i, j + 1));
            triangles.push([a, b, c]);
            triangles.push([a, c, d]);
        }
    }
    (positions, triangles)
}

/// Icosfera (octaedro subdividido) de radio `r`, normales hacia afuera.
fn sphere_triangles(r: f64) -> (Vec<Vector3>, Vec<[usize; 3]>) {
    let mut v: Vec<V3<f64>> = [[1., 0., 0.], [-1., 0., 0.], [0., 1., 0.], [0., -1., 0.], [0., 0., 1.], [0., 0., -1.]]
        .iter()
        .map(|p| V3::new(p[0], p[1], p[2]))
        .collect();
    let mut f = vec![[0, 2, 4], [2, 1, 4], [1, 3, 4], [3, 0, 4], [2, 0, 5], [1, 2, 5], [3, 1, 5], [0, 3, 5]];
    for _ in 0..4 {
        let mut mid: HashMap<(usize, usize), usize> = HashMap::new();
        let mut split = |a: usize, b: usize, v: &mut Vec<V3<f64>>| {
            *mid.entry((a.min(b), a.max(b))).or_insert_with(|| {
                v.push((v[a] + v[b]) / 2.0);
                v.len() - 1
            })
        };
        f = f
            .iter()
            .flat_map(|t| {
                let (ab, bc, ca) = (split(t[0], t[1], &mut v), split(t[1], t[2], &mut v), split(t[2], t[0], &mut v));
                [[t[0], ab, ca], [ab, t[1], bc], [ca, bc, t[2]], [ab, bc, ca]]
            })
            .collect();
    }
    (v.iter().map(|p| Vector3(p.normalize() * r)).collect(), f)
}

fn sphere(r: f64) -> Mesh {
    let (p, t) = sphere_triangles(r);
    Mesh::from_triangles(&p, &t)
}

fn torus() -> Mesh {
    let (p, t) = param_surface(48, 16, true, true, |u, v| {
        let (a, b) = (2.0 * PI * u, 2.0 * PI * v);
        [(1.0 + 0.4 * b.cos()) * a.cos(), 0.4 * b.sin(), -(1.0 + 0.4 * b.cos()) * a.sin()]
    });
    Mesh::from_triangles(&p, &t)
}

/// Cilindro de radio 1 y alto 2; con `caps`, cerrado con tapas planas.
fn cylinder(caps: bool) -> Mesh {
    let (nu, nv) = (48, 8);
    let (mut p, mut t) = param_surface(nu, nv, true, false, |u, v| {
        let a = 2.0 * PI * u;
        [a.cos(), 2.0 * v, -a.sin()]
    });
    if caps {
        let ring = |i: usize, j: usize| (i % nu) * (nv + 1) + j;
        let (bottom, top) = (p.len(), p.len() + 1);
        p.push(Vector3::new(0.0, 0.0, 0.0));
        p.push(Vector3::new(0.0, 2.0, 0.0));
        for i in 0..nu {
            t.push([bottom, ring(i + 1, 0), ring(i, 0)]);
            t.push([top, ring(i, nv), ring(i + 1, nv)]);
        }
    }
    Mesh::from_triangles(&p, &t)
}

fn assert_closed_manifold(q: &QuadMesh, euler: i64) {
    let topo = q.topology();
    assert_eq!(topo.boundary_edges, 0, "{topo:?}");
    assert_eq!(topo.non_manifold_edges, 0, "{topo:?}");
    assert_eq!(topo.non_manifold_vertices, 0, "{topo:?}");
    assert_eq!(topo.flipped_edges, 0, "{topo:?}");
    assert_eq!(topo.degenerate_faces, 0, "{topo:?}");
    assert_eq!(topo.euler_characteristic, euler, "{topo:?}");
}

fn assert_face_count(q: &QuadMesh, target: usize, tolerance: f64) {
    let ratio = q.num_faces() as f64 / target as f64;
    assert!((1.0 - tolerance..=1.0 + tolerance).contains(&ratio), "{} caras para {target}", q.num_faces());
}

/// Desviación media de los ángulos internos respecto de 90°, en grados.
fn mean_angle_deviation(q: &QuadMesh) -> f64 {
    let mut total = 0.0;
    for f in &q.faces {
        for k in 0..4 {
            let p = q.vertices[f.v[k]];
            let (a, b) = (q.vertices[f.v[(k + 1) % 4]] - p, q.vertices[f.v[(k + 3) % 4]] - p);
            total += (a.angle(&b).to_degrees() - 90.0).abs();
        }
    }
    total / (4 * q.num_faces()) as f64
}

fn face_normal(q: &QuadMesh, f: usize) -> V3<f64> {
    let [a, b, c, d] = q.faces[f].v.map(|i| q.vertices[i]);
    (c - a).cross(&(d - b))
}

#[test]
fn sphere_is_closed_regular_and_outward() {
    let q = remesh(&sphere(1.0), &config(300)).unwrap();
    assert_closed_manifold(&q, 2);
    assert_face_count(&q, 300, 0.15);
    for v in &q.vertices {
        assert!((v.norm() - 1.0).abs() < 0.01, "vértice fuera de la esfera: {v:?}");
    }
    let outward = (0..q.num_faces()).filter(|&f| {
        let center = q.faces[f].v.iter().map(|&i| q.vertices[i]).sum::<V3<f64>>();
        face_normal(&q, f).dot(&center) > 0.0
    });
    assert_eq!(outward.count(), q.num_faces());
    assert!(mean_angle_deviation(&q) < 10.0);
}

#[test]
fn torus_keeps_its_genus() {
    let q = remesh(&torus(), &config(400)).unwrap();
    assert_closed_manifold(&q, 0);
    assert_face_count(&q, 400, 0.15);
    for v in &q.vertices {
        let ring = (v.x * v.x + v.z * v.z).sqrt() - 1.0;
        // La entrada es facetada: la flecha de sus caras ronda 0.01
        assert!(((ring * ring + v.y * v.y).sqrt() - 0.4).abs() < 0.02);
    }
}

#[test]
fn flat_square_becomes_a_regular_grid() {
    let (p, t) = param_surface(20, 20, false, false, |u, v| [u, 0.0, -v]);
    let q = remesh(&Mesh::from_triangles(&p, &t), &config(100)).unwrap();
    assert_eq!(q.num_faces(), 100);
    let topo = q.topology();
    assert_eq!(topo.boundary_edges, 40);
    assert_eq!(topo.euler_characteristic, 1);
    assert!(mean_angle_deviation(&q) < 1.0);
    for f in 0..q.num_faces() {
        assert!(face_normal(&q, f).normalize().y > 0.999, "misma orientación que la entrada");
    }
}

#[test]
fn open_cylinder_boundaries_stay_on_the_rims() {
    let q = remesh(&cylinder(false), &config(300)).unwrap();
    let topo = q.topology();
    assert_eq!(topo.euler_characteristic, 0);
    assert_eq!(topo.non_manifold_edges + topo.flipped_edges, 0);

    let mut edge_use: HashMap<(usize, usize), u32> = HashMap::new();
    for f in &q.faces {
        for k in 0..4 {
            let (a, b) = (f.v[k], f.v[(k + 1) % 4]);
            *edge_use.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    let boundary: Vec<usize> = edge_use.iter().filter(|(_, n)| **n == 1).flat_map(|(&(a, b), _)| [a, b]).collect();
    assert!(!boundary.is_empty());
    for v in boundary {
        let y = q.vertices[v].y;
        assert!(y.abs() < 1e-9 || (y - 2.0).abs() < 1e-9, "vértice de borde en y = {y}");
    }
}

#[test]
fn sharp_edges_are_not_crossed() {
    let on_side = |p: &V3<f64>| ((p.x * p.x + p.z * p.z).sqrt() - 1.0).abs() < 3e-3;
    let on_cap = |p: &V3<f64>| p.y.abs() < 1e-9 || (p.y - 2.0).abs() < 1e-9;
    let crossing = |q: &QuadMesh| {
        q.faces
            .iter()
            .filter(|f| {
                let ps = f.v.map(|i| q.vertices[i]);
                !(ps.iter().all(on_side) || ps.iter().all(on_cap))
            })
            .count()
    };

    let config = RemeshConfig { target_faces: 400, preserve_sharp: true, ..Default::default() };
    let q = remesh(&cylinder(true), &config).unwrap();
    assert_closed_manifold(&q, 2);
    assert_eq!(crossing(&q), 0);

    // Sin la opción, los quads redondean el borde
    let loose = remesh(&cylinder(true), &RemeshConfig { preserve_sharp: false, ..config }).unwrap();
    assert!(crossing(&loose) > 0);
}

#[test]
fn result_does_not_depend_on_scale() {
    // Escalar por una potencia de 2 es exacto en coma flotante: misma malla
    let small = remesh(&sphere(1.0), &config(300)).unwrap();
    let large = remesh(&sphere(256.0), &config(300)).unwrap();
    assert_eq!(small.faces, large.faces);
    for (a, b) in small.vertices.iter().zip(&large.vertices) {
        assert!((a * 256.0 - b).norm() < 1e-9 * 256.0);
    }

    // Otras escalas solo cambian desempates de redondeo
    let odd = remesh(&sphere(170.0), &config(300)).unwrap();
    assert_closed_manifold(&odd, 2);
    assert_face_count(&odd, 300, 0.15);
}

#[test]
fn unwelded_input_is_welded_first() {
    // Cada triángulo con vértices propios, como las costuras UV de un glTF
    let (p, t) = sphere_triangles(1.0);
    let positions: Vec<Vector3> = t.iter().flat_map(|tri| tri.map(|i| p[i])).collect();
    let triangles: Vec<[usize; 3]> = (0..t.len()).map(|i| [3 * i, 3 * i + 1, 3 * i + 2]).collect();
    let q = remesh(&Mesh::from_triangles(&positions, &triangles), &config(300)).unwrap();
    assert_closed_manifold(&q, 2);
}

#[test]
fn coarse_input_is_refined() {
    // Cubo de 12 triángulos para 600 quads
    let p: Vec<Vector3> = (0..8)
        .map(|i| Vector3::new((i & 1) as f64, ((i >> 1) & 1) as f64, ((i >> 2) & 1) as f64))
        .collect();
    let t = [[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
    let config = RemeshConfig { target_faces: 600, preserve_sharp: true, ..Default::default() };
    let q = remesh(&Mesh::from_triangles(&p, &t), &config).unwrap();
    assert_closed_manifold(&q, 2);
    assert_face_count(&q, 600, 0.1);
    assert!(mean_angle_deviation(&q) < 2.0);
}

#[test]
fn output_is_deterministic() {
    let a = remesh(&torus(), &config(300)).unwrap();
    let b = remesh(&torus(), &config(300)).unwrap();
    assert_eq!(a.faces, b.faces);
    assert_eq!(a.vertices, b.vertices);
}

#[test]
fn reports_stages_in_order() {
    let mut stages = Vec::new();
    remesh_with_callback(&sphere(1.0), &config(100), |stage, _| stages.push(stage)).unwrap();
    assert_eq!(
        stages,
        [
            RemeshStage::Preprocess,
            RemeshStage::Hierarchy,
            RemeshStage::OrientationField,
            RemeshStage::PositionField,
            RemeshStage::Extraction,
            RemeshStage::Done,
        ]
    );
    let progress: Vec<u32> = stages.iter().map(|s| s.progress()).collect();
    assert!(progress.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn invalid_input_is_rejected() {
    assert!(matches!(remesh(&Mesh::new(), &config(100)), Err(RemeshError::EmptyMesh)));
    assert!(matches!(remesh(&sphere(1.0), &config(0)), Err(RemeshError::InvalidConfig(_))));
    let bad_angle = RemeshConfig { sharp_angle: f64::NAN, ..config(100) };
    assert!(matches!(remesh(&sphere(1.0), &bad_angle), Err(RemeshError::InvalidConfig(_))));
    // Todos los triángulos degenerados
    let p = vec![Vector3::new(0.0, 0.0, 0.0); 3];
    assert!(matches!(remesh(&Mesh::from_triangles(&p, &[[0, 1, 2]]), &config(10)), Err(RemeshError::EmptyMesh)));
}
