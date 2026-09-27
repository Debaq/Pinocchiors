//! Test end-to-end de autorig sobre un humanoide sintético.
//!
//! El humanoide es la unión de cápsulas (SDF) mallada con *surface nets*, en
//! coordenadas "de mundo" (170 de alto y desplazado), de modo que el test cubre
//! la normalización y el regreso a las coordenadas originales.

use pinocchio_core::math::Vector3;
use pinocchio_core::mesh::Mesh;
use pinocchio_core::skeleton::{HumanSkeleton, Skeleton};
use pinocchio_core::{autorig, PinocchioConfig, PinocchioOutput, SkeletonFit};
use std::sync::OnceLock;

const SCALE: f64 = 170.0;
const OFFSET: [f64; 3] = [50.0, 10.0, -30.0];

/// Cápsula: segmento `a-b` con radio `r`, en coordenadas de altura 1
struct Capsule {
    a: [f64; 3],
    b: [f64; 3],
    r: f64,
}

fn humanoid_capsules() -> Vec<Capsule> {
    let c = |a: [f64; 3], b: [f64; 3], r: f64| Capsule { a, b, r };
    let mut caps = vec![
        c([0.0, 0.48, 0.0], [0.0, 0.78, 0.0], 0.11), // torso
        c([0.0, 0.78, 0.0], [0.0, 0.88, 0.0], 0.04), // cuello
        c([0.0, 0.94, 0.0], [0.0, 0.94, 0.0], 0.07), // cabeza
    ];
    for side in [-1.0, 1.0] {
        caps.push(c([0.07 * side, 0.47, 0.0], [0.08 * side, 0.26, 0.0], 0.05)); // muslo
        caps.push(c([0.08 * side, 0.26, 0.0], [0.09 * side, 0.05, 0.0], 0.045)); // pierna
        caps.push(c([0.09 * side, 0.05, 0.0], [0.09 * side, 0.02, 0.07], 0.03)); // pie
        caps.push(c([0.15 * side, 0.76, 0.0], [0.28 * side, 0.60, 0.0], 0.035)); // brazo
        caps.push(c([0.28 * side, 0.60, 0.0], [0.38 * side, 0.45, 0.0], 0.03)); // antebrazo
        caps.push(c([0.38 * side, 0.45, 0.0], [0.42 * side, 0.40, 0.0], 0.03)); // mano
    }
    caps
}

/// Punto de "altura 1" a coordenadas de mundo
fn world(p: [f64; 3]) -> Vector3 {
    Vector3::new(p[0] * SCALE + OFFSET[0], p[1] * SCALE + OFFSET[1], p[2] * SCALE + OFFSET[2])
}

fn sdf(caps: &[Capsule], p: [f64; 3]) -> f64 {
    caps.iter()
        .map(|c| {
            let ab = [c.b[0] - c.a[0], c.b[1] - c.a[1], c.b[2] - c.a[2]];
            let ap = [p[0] - c.a[0], p[1] - c.a[1], p[2] - c.a[2]];
            let len2 = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
            let t = if len2 > 0.0 {
                ((ap[0] * ab[0] + ap[1] * ab[1] + ap[2] * ab[2]) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let d = [ap[0] - ab[0] * t, ap[1] - ab[1] * t, ap[2] - ab[2] * t];
            (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() - c.r
        })
        .fold(f64::INFINITY, f64::min)
}

/// Surface nets: un vértice por celda con cambio de signo, un quad por arista cruzada
fn surface_nets(caps: &[Capsule], min: [f64; 3], max: [f64; 3], n: usize) -> Mesh {
    let step = [(max[0] - min[0]) / n as f64, (max[1] - min[1]) / n as f64, (max[2] - min[2]) / n as f64];
    let np = n + 1;
    let idx = |i: usize, j: usize, k: usize| (k * np + j) * np + i;
    let point = |i: usize, j: usize, k: usize| {
        [min[0] + i as f64 * step[0], min[1] + j as f64 * step[1], min[2] + k as f64 * step[2]]
    };

    let mut values = vec![0.0; np * np * np];
    for k in 0..np {
        for j in 0..np {
            for i in 0..np {
                values[idx(i, j, k)] = sdf(caps, point(i, j, k));
            }
        }
    }

    // Vértice por celda
    let corners: [[usize; 3]; 8] = [[0, 0, 0], [1, 0, 0], [0, 1, 0], [1, 1, 0], [0, 0, 1], [1, 0, 1], [0, 1, 1], [1, 1, 1]];
    let edges: [(usize, usize); 12] = [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)];
    let mut cell_vertex = vec![usize::MAX; n * n * n];
    let cidx = |i: usize, j: usize, k: usize| (k * n + j) * n + i;
    let mut positions = Vec::new();
    for k in 0..n {
        for j in 0..n {
            for i in 0..n {
                let v: Vec<f64> = corners.iter().map(|c| values[idx(i + c[0], j + c[1], k + c[2])]).collect();
                if v.iter().all(|&x| x < 0.0) || v.iter().all(|&x| x >= 0.0) {
                    continue;
                }
                let mut acc = [0.0; 3];
                let mut count = 0.0;
                for &(a, b) in &edges {
                    if (v[a] < 0.0) != (v[b] < 0.0) {
                        let t = v[a] / (v[a] - v[b]);
                        let pa = point(i + corners[a][0], j + corners[a][1], k + corners[a][2]);
                        let pb = point(i + corners[b][0], j + corners[b][1], k + corners[b][2]);
                        for d in 0..3 {
                            acc[d] += pa[d] + (pb[d] - pa[d]) * t;
                        }
                        count += 1.0;
                    }
                }
                cell_vertex[cidx(i, j, k)] = positions.len();
                positions.push(world([acc[0] / count, acc[1] / count, acc[2] / count]));
            }
        }
    }

    // Quads por arista de la grilla con cambio de signo
    let mut triangles = Vec::new();
    for k in 1..n {
        for j in 1..n {
            for i in 1..n {
                let p0 = values[idx(i, j, k)];
                for axis in 0..3 {
                    let q = match axis {
                        0 => [i + 1, j, k],
                        1 => [i, j + 1, k],
                        _ => [i, j, k + 1],
                    };
                    let p1 = values[idx(q[0], q[1], q[2])];
                    if (p0 < 0.0) == (p1 < 0.0) {
                        continue;
                    }
                    // Las 4 celdas que comparten la arista
                    let (u, w) = ((axis + 1) % 3, (axis + 2) % 3);
                    let cell = |du: usize, dw: usize| {
                        let mut c = [i, j, k];
                        c[u] -= du;
                        c[w] -= dw;
                        cell_vertex[cidx(c[0], c[1], c[2])]
                    };
                    let quad = [cell(1, 1), cell(0, 1), cell(0, 0), cell(1, 0)];
                    let quad = if p0 < 0.0 { quad } else { [quad[3], quad[2], quad[1], quad[0]] };
                    triangles.push([quad[0], quad[1], quad[2]]);
                    triangles.push([quad[0], quad[2], quad[3]]);
                }
            }
        }
    }

    Mesh::from_triangles(&positions, &triangles)
}

fn humanoid_mesh() -> Mesh {
    surface_nets(&humanoid_capsules(), [-0.55, -0.1, -0.2], [0.55, 1.1, 0.25], 72)
}

/// Humanoide + resultado de autorig con el preset humano, calculados una sola vez
fn fixture() -> &'static (Mesh, PinocchioOutput) {
    static FIXTURE: OnceLock<(Mesh, PinocchioOutput)> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let mesh = humanoid_mesh();
        let config = PinocchioConfig { verify_mesh_integrity: false, ..Default::default() };
        let out = autorig(&mesh, &HumanSkeleton::new(), Some(config)).expect("autorig");
        (mesh, out)
    })
}

fn bone_index(name: &str) -> usize {
    HumanSkeleton::new().bones().iter().position(|b| b.name == name).unwrap()
}

/// Coordenadas de mundo → espacio de altura 1
fn unit(p: &Vector3) -> [f64; 3] {
    [(p.x() - OFFSET[0]) / SCALE, (p.y() - OFFSET[1]) / SCALE, (p.z() - OFFSET[2]) / SCALE]
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

#[test]
fn autorig_humanoid_joints_inside_and_in_place() {
    let caps = humanoid_capsules();
    let (_, out) = fixture();
    let joint = |name: &str| unit(&out.bone_positions[bone_index(name)]);

    // Todas las articulaciones dentro de la malla, en coordenadas de mundo
    for (i, bone) in HumanSkeleton::new().bones().iter().enumerate() {
        let p = unit(&out.bone_positions[i]);
        assert!(sdf(&caps, p) < 0.0, "{} fuera de la malla: {p:?}", bone.name);
    }

    // Columna ordenada de abajo hacia arriba
    let spine: Vec<f64> = ["pelvis", "spine", "chest", "neck", "head"].iter().map(|n| joint(n)[1]).collect();
    assert!(spine.windows(2).all(|w| w[0] < w[1]), "columna desordenada: {spine:?}");

    // Cerca de las articulaciones reales del modelo (tolerancia en altura = 1)
    let expected = [
        ("pelvis", [0.0, 0.48, 0.0], 0.08),
        ("head", [0.0, 0.96, 0.0], 0.08),
        ("knee_l", [-0.08, 0.26, 0.0], 0.06),
        ("knee_r", [0.08, 0.26, 0.0], 0.06),
        ("ankle_l", [-0.09, 0.05, 0.0], 0.08),
        ("elbow_l", [-0.28, 0.60, 0.0], 0.08),
        ("elbow_r", [0.28, 0.60, 0.0], 0.08),
        ("wrist_l", [-0.38, 0.45, 0.0], 0.10),
    ];
    for (name, truth, tol) in expected {
        let d = dist(joint(name), truth);
        assert!(d < tol, "{name}: {:?} a {d:.3} de {truth:?}", joint(name));
    }
}

#[test]
fn autorig_humanoid_weights_by_region() {
    let (mesh, out) = fixture();

    // Pesos normalizados y compactados
    for v in 0..mesh.num_vertices() {
        let w = out.get_weights(v);
        let sum: f64 = w.iter().sum();
        assert!((sum - 1.0).abs() < 1e-6, "vértice {v}: suma {sum}");
        assert!(w.iter().filter(|&&x| x > 1e-9).count() <= 4);
    }

    // El peso del hueso i corresponde al segmento padre(i) → i.
    // Las regiones se toman lejos de las articulaciones: el embedding ajusta la
    // plantilla y codo y muñeca quedan ~0.05 más cerca del torso que en el modelo.
    type Region = (&'static str, fn([f64; 3]) -> bool, &'static [&'static str]);
    let regions: [Region; 8] = [
        ("cabeza", |p| p[1] > 0.93, &["head", "neck"]),
        ("muslo izq.", |p| (-0.15..-0.03).contains(&p[0]) && (0.32..0.40).contains(&p[1]), &["knee_l"]),
        ("pierna izq.", |p| (-0.15..-0.03).contains(&p[0]) && (0.12..0.20).contains(&p[1]), &["ankle_l"]),
        ("pierna der.", |p| (0.03..0.15).contains(&p[0]) && (0.12..0.20).contains(&p[1]), &["ankle_r"]),
        ("brazo izq.", |p| (-0.21..-0.16).contains(&p[0]) && p[1] > 0.62, &["elbow_l"]),
        ("antebrazo izq.", |p| (-0.30..-0.27).contains(&p[0]), &["wrist_l"]),
        ("antebrazo der.", |p| (0.27..0.30).contains(&p[0]), &["wrist_r"]),
        ("mano der.", |p| p[0] > 0.40, &["hand_r", "wrist_r"]),
    ];
    for (label, in_region, allowed) in regions {
        let allowed: Vec<usize> = allowed.iter().map(|n| bone_index(n)).collect();
        let (mut total, mut ok) = (0, 0);
        for (v, vertex) in mesh.vertices.iter().enumerate() {
            if !in_region(unit(&vertex.position)) {
                continue;
            }
            total += 1;
            let dominant = out.get_dominant_bones(v, 1)[0].0;
            if allowed.contains(&dominant) {
                ok += 1;
            }
        }
        assert!(total > 10, "{label}: región vacía");
        let ratio = ok as f64 / total as f64;
        assert!(ratio > 0.95, "{label}: solo {ok}/{total} vértices con el hueso esperado");
    }
}

#[test]
fn autorig_rest_pose_is_identity() {
    // Con las transformaciones de reposo, la deformación devuelve la malla original
    let (mesh, out) = fixture();
    let deformed = out.deform(&out.bone_rest_transforms);
    assert_eq!(deformed.len(), mesh.num_vertices());
    for (p, v) in deformed.iter().zip(&mesh.vertices) {
        assert!(p.distance(&v.position) < 1e-6);
    }
}

#[test]
fn autorig_skeleton_fit_none_uses_given_placement() {
    // Esqueleto ya colocado sobre la malla (como tras editarlo en la GUI)
    let (mesh, _) = fixture();
    let placed = pinocchio_core::skeleton::fit_to_bounds(&HumanSkeleton::new(), &mesh.bounding_box(), 0.9);
    let config = PinocchioConfig { verify_mesh_integrity: false, ..Default::default() }
        .with_skeleton_fit(SkeletonFit::None);
    let out = autorig(mesh, &placed, Some(config)).expect("autorig");

    let caps = humanoid_capsules();
    for p in &out.bone_positions {
        assert!(sdf(&caps, unit(p)) < 0.0, "articulación fuera de la malla: {:?}", unit(p));
    }
    let knee = unit(&out.bone_positions[bone_index("knee_l")]);
    assert!(dist(knee, [-0.08, 0.26, 0.0]) < 0.06, "knee_l en {knee:?}");
}

