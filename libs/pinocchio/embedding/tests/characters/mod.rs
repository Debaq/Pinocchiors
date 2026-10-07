//! Personajes sintéticos: unión de cápsulas convertida en malla hermética con
//! *marching tetrahedra*. Se conocen sus articulaciones reales, así el ajuste
//! del esqueleto se mide con números.

#![allow(dead_code)]

use pinocchio_math::Vector3;
use pinocchio_mesh::Mesh;
use std::collections::HashMap;

/// Cápsula: segmento `a → b` con radio `r`.
pub struct Capsule(pub [f64; 3], pub [f64; 3], pub f64);

fn v(p: [f64; 3]) -> Vector3 {
    Vector3::new(p[0], p[1], p[2])
}

fn capsule_distance(p: Vector3, c: &Capsule) -> f64 {
    let (a, b) = (v(c.0), v(c.1));
    let ab = b - a;
    let t = ((p - a).dot(&ab) / ab.dot(&ab).max(1e-12)).clamp(0.0, 1.0);
    p.distance(&(a + ab * t)) - c.2
}

/// Malla de la unión de `capsules`, con celdas de lado `step`.
pub fn mesh(capsules: &[Capsule], step: f64) -> Mesh {
    let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
    for c in capsules {
        for p in [c.0, c.1] {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k] - c.2 - 2.0 * step);
                hi[k] = hi[k].max(p[k] + c.2 + 2.0 * step);
            }
        }
    }
    let n: [usize; 3] = std::array::from_fn(|k| ((hi[k] - lo[k]) / step).ceil() as usize + 1);
    let point = |i: usize, j: usize, k: usize| v([lo[0] + i as f64 * step, lo[1] + j as f64 * step, lo[2] + k as f64 * step]);
    let index = |i: usize, j: usize, k: usize| (k * n[1] + j) * n[0] + i;
    let mut value = vec![0.0; n[0] * n[1] * n[2]];
    for k in 0..n[2] {
        for j in 0..n[1] {
            for i in 0..n[0] {
                let p = point(i, j, k);
                // Nunca exactamente cero: evita vértices degenerados
                let d = capsules.iter().map(|c| capsule_distance(p, c)).fold(f64::MAX, f64::min);
                value[index(i, j, k)] = if d == 0.0 { 1e-9 } else { d };
            }
        }
    }

    let mut vertices: Vec<Vector3> = Vec::new();
    let mut edge_vertex: HashMap<(usize, usize), usize> = HashMap::new();
    let mut triangles: Vec<[usize; 3]> = Vec::new();
    // Seis tetraedros por cubo alrededor de la diagonal 0-7 (índice = x + 2y + 4z)
    const TETS: [[usize; 4]; 6] = [[0, 1, 3, 7], [0, 3, 2, 7], [0, 2, 6, 7], [0, 6, 4, 7], [0, 4, 5, 7], [0, 5, 1, 7]];
    for k in 0..n[2] - 1 {
        for j in 0..n[1] - 1 {
            for i in 0..n[0] - 1 {
                let corner = |c: usize| index(i + (c & 1), j + ((c >> 1) & 1), k + ((c >> 2) & 1));
                for tet in TETS {
                    let ids = tet.map(corner);
                    let inside: Vec<usize> = ids.iter().copied().filter(|&g| value[g] < 0.0).collect();
                    let outside: Vec<usize> = ids.iter().copied().filter(|&g| value[g] >= 0.0).collect();
                    if inside.is_empty() || outside.is_empty() {
                        continue;
                    }
                    let mut vertex = |a: usize, b: usize| {
                        *edge_vertex.entry((a.min(b), a.max(b))).or_insert_with(|| {
                            let (pa, pb) = (grid_point(a, n, lo, step), grid_point(b, n, lo, step));
                            let t = value[a] / (value[a] - value[b]);
                            vertices.push(pa + (pb - pa) * t);
                            vertices.len() - 1
                        })
                    };
                    let inside_center = inside.iter().fold(Vector3::zero(), |s, &g| s + grid_point(g, n, lo, step)) * (1.0 / inside.len() as f64);
                    let tris: Vec<[usize; 3]> = match (inside.len(), outside.len()) {
                        (1, 3) => vec![[vertex(inside[0], outside[0]), vertex(inside[0], outside[1]), vertex(inside[0], outside[2])]],
                        (3, 1) => vec![[vertex(outside[0], inside[0]), vertex(outside[0], inside[1]), vertex(outside[0], inside[2])]],
                        _ => {
                            let (a, b, c, d) = (
                                vertex(inside[0], outside[0]),
                                vertex(inside[0], outside[1]),
                                vertex(inside[1], outside[1]),
                                vertex(inside[1], outside[0]),
                            );
                            vec![[a, b, c], [a, c, d]]
                        }
                    };
                    for mut t in tris {
                        let [p0, p1, p2] = t.map(|x| vertices[x]);
                        let normal = (p1 - p0).cross(&(p2 - p0));
                        let center = (p0 + p1 + p2) * (1.0 / 3.0);
                        // Normal hacia afuera
                        if normal.dot(&(center - inside_center)) < 0.0 {
                            t.swap(1, 2);
                        }
                        if t[0] != t[1] && t[1] != t[2] && t[0] != t[2] {
                            triangles.push(t);
                        }
                    }
                }
            }
        }
    }
    Mesh::from_triangles(&vertices, &triangles)
}

fn grid_point(g: usize, n: [usize; 3], lo: [f64; 3], step: f64) -> Vector3 {
    let (i, j, k) = (g % n[0], (g / n[0]) % n[1], g / (n[0] * n[1]));
    v([lo[0] + i as f64 * step, lo[1] + j as f64 * step, lo[2] + k as f64 * step])
}

/// Personaje con sus articulaciones reales (nombre del hueso del preset).
pub struct Character {
    pub capsules: Vec<Capsule>,
    pub joints: Vec<(&'static str, [f64; 3])>,
}

impl Character {
    /// El mismo personaje girado 90° alrededor de Y (mira hacia +X).
    pub fn turned(self) -> Self {
        let r = |p: [f64; 3]| [p[2], p[1], -p[0]];
        Self {
            capsules: self.capsules.into_iter().map(|c| Capsule(r(c.0), r(c.1), c.2)).collect(),
            joints: self.joints.into_iter().map(|(n, p)| (n, r(p))).collect(),
        }
    }

    /// El mismo personaje con Z arriba (como muchos STL).
    pub fn z_up(self) -> Self {
        let r = |p: [f64; 3]| [p[0], -p[2], p[1]];
        Self {
            capsules: self.capsules.into_iter().map(|c| Capsule(r(c.0), r(c.1), c.2)).collect(),
            joints: self.joints.into_iter().map(|(n, p)| (n, r(p))).collect(),
        }
    }
}

/// Humano de 1,8 de alto en pose A, mirando hacia +Z.
pub fn human() -> Character {
    let mut capsules = vec![
        Capsule([0.0, 0.95, 0.0], [0.0, 1.35, 0.0], 0.16),
        Capsule([0.0, 1.35, 0.0], [0.0, 1.6, 0.0], 0.06),
        Capsule([0.0, 1.66, 0.02], [0.0, 1.72, 0.02], 0.11),
    ];
    let mut joints = vec![("pelvis", [0.0, 0.92, 0.0]), ("chest", [0.0, 1.4, 0.0]), ("head", [0.0, 1.75, 0.02])];
    for (s, side) in [(-1.0, "l"), (1.0, "r")] {
        capsules.push(Capsule([0.15 * s, 1.45, 0.0], [0.45 * s, 1.28, 0.0], 0.055));
        capsules.push(Capsule([0.45 * s, 1.28, 0.0], [0.68 * s, 1.12, 0.03], 0.045));
        capsules.push(Capsule([0.68 * s, 1.12, 0.03], [0.8 * s, 1.05, 0.05], 0.035));
        capsules.push(Capsule([0.1 * s, 0.9, 0.0], [0.12 * s, 0.5, 0.03], 0.075));
        capsules.push(Capsule([0.12 * s, 0.5, 0.03], [0.12 * s, 0.09, 0.0], 0.06));
        capsules.push(Capsule([0.12 * s, 0.07, 0.0], [0.12 * s, 0.05, 0.16], 0.045));
        let name = |base: &'static str| -> &'static str { Box::leak(format!("{base}_{side}").into_boxed_str()) };
        joints.push((name("elbow"), [0.45 * s, 1.28, 0.0]));
        joints.push((name("wrist"), [0.68 * s, 1.12, 0.03]));
        joints.push((name("hand"), [0.8 * s, 1.05, 0.05]));
        joints.push((name("knee"), [0.12 * s, 0.5, 0.03]));
        joints.push((name("ankle"), [0.12 * s, 0.09, 0.0]));
        joints.push((name("foot"), [0.12 * s, 0.05, 0.16]));
    }
    Character { capsules, joints }
}

/// Cuadrúpedo tipo elefante (con trompa y cola), mirando hacia +Z.
pub fn elephant() -> Character {
    let mut capsules = vec![
        Capsule([0.0, 1.2, -0.55], [0.0, 1.25, 0.55], 0.45),
        Capsule([0.0, 1.35, 0.7], [0.0, 1.45, 1.0], 0.3),
        Capsule([0.0, 1.3, 1.2], [0.0, 0.85, 1.4], 0.1),
        Capsule([0.0, 0.85, 1.4], [0.0, 0.35, 1.35], 0.07),
        Capsule([0.0, 1.25, -0.95], [0.0, 0.75, -1.15], 0.06),
    ];
    // La hoja "head" de la plantilla es la punta de la cabeza
    let mut joints = vec![("head", [0.0, 1.47, 1.25]), ("tail_end", [0.0, 0.75, -1.15])];
    for (s, side) in [(-1.0, "l"), (1.0, "r")] {
        for (z, front) in [(0.45, true), (-0.45, false)] {
            capsules.push(Capsule([0.28 * s, 1.0, z], [0.28 * s, 0.55, z], 0.15));
            capsules.push(Capsule([0.28 * s, 0.55, z], [0.28 * s, 0.12, z], 0.13));
            let (knee, paw) = match (front, side) {
                (true, "l") => ("elbow_l", "paw_fl"),
                (true, _) => ("elbow_r", "paw_fr"),
                (false, "l") => ("knee_l", "paw_bl"),
                (false, _) => ("knee_r", "paw_br"),
            };
            joints.push((knee, [0.28 * s, 0.55, z]));
            joints.push((paw, [0.28 * s, 0.12, z]));
        }
    }
    Character { capsules, joints }
}

/// Pulpo: manto arriba, cuerpo y 8 brazos que se afinan, en los mismos
/// ángulos que la plantilla (`arm{i}_tip_{l,r}`).
pub fn octopus() -> Character {
    let mut capsules = vec![Capsule([0.0, 0.6, 0.0], [0.0, 1.0, 0.0], 0.28), Capsule([0.0, 0.35, 0.0], [0.0, 0.5, 0.0], 0.24)];
    let mut joints = Vec::new();
    for i in 0..4 {
        let a = std::f64::consts::PI * (i as f64 + 0.5) / 4.0;
        for (s, side) in [(-1.0, "l"), (1.0, "r")] {
            let dir = [(s * a).sin(), 0.0, (s * a).cos()];
            let at = |r: f64, y: f64| [dir[0] * r, y, dir[2] * r];
            capsules.push(Capsule(at(0.12, 0.3), at(0.55, 0.12), 0.07));
            capsules.push(Capsule(at(0.55, 0.12), at(1.0, 0.06), 0.035));
            let name: &'static str = Box::leak(format!("arm{}_tip_{side}", i + 1).into_boxed_str());
            joints.push((name, at(1.0, 0.06)));
        }
    }
    Character { capsules, joints }
}

/// Pez: cuerpo horizontal, cabeza al frente (+Z), cola con aleta caudal,
/// pectorales y dorsal.
pub fn fish() -> Character {
    let capsules = vec![
        Capsule([0.0, 0.5, -0.3], [0.0, 0.5, 0.35], 0.15),
        Capsule([0.0, 0.5, 0.35], [0.0, 0.5, 0.5], 0.11),
        Capsule([0.0, 0.5, -0.3], [0.0, 0.5, -0.8], 0.06),
        Capsule([0.0, 0.5, -0.8], [0.0, 0.62, -0.95], 0.035),
        Capsule([0.0, 0.5, -0.8], [0.0, 0.38, -0.95], 0.035),
        Capsule([-0.12, 0.45, 0.2], [-0.35, 0.38, 0.1], 0.03),
        Capsule([0.12, 0.45, 0.2], [0.35, 0.38, 0.1], 0.03),
        Capsule([0.0, 0.6, 0.0], [0.0, 0.8, -0.1], 0.03),
    ];
    let joints = vec![
        ("head", [0.0, 0.5, 0.6]),
        ("pectoral_tip_l", [-0.35, 0.38, 0.1]),
        ("pectoral_tip_r", [0.35, 0.38, 0.1]),
        ("dorsal_tip", [0.0, 0.8, -0.1]),
    ];
    Character { capsules, joints }
}

/// Camello de dos jorobas: patas largas con carpo y corvejón, cuello en U.
pub fn camel() -> Character {
    let capsules = vec![
        Capsule([0.0, 1.25, -0.6], [0.0, 1.3, 0.6], 0.32),
        Capsule([0.0, 1.55, 0.3], [0.0, 1.62, 0.25], 0.2),
        Capsule([0.0, 1.55, -0.25], [0.0, 1.62, -0.3], 0.2),
        Capsule([0.0, 1.3, 0.75], [0.0, 1.05, 1.15], 0.13),
        Capsule([0.0, 1.05, 1.15], [0.0, 1.7, 1.45], 0.1),
        Capsule([0.0, 1.75, 1.45], [0.0, 1.68, 1.8], 0.1),
        Capsule([0.0, 1.3, -0.9], [0.0, 0.9, -1.0], 0.04),
    ];
    camelid(capsules, [0.0, 1.68, 1.88])
}

/// Llama: sin jorobas, cuello vertical.
pub fn llama() -> Character {
    let capsules = vec![
        Capsule([0.0, 1.25, -0.6], [0.0, 1.3, 0.6], 0.3),
        Capsule([0.0, 1.35, 0.7], [0.0, 2.0, 0.85], 0.11),
        Capsule([0.0, 2.05, 0.85], [0.0, 1.98, 1.2], 0.1),
        Capsule([0.0, 1.3, -0.9], [0.0, 0.9, -1.0], 0.04),
    ];
    camelid(capsules, [0.0, 1.98, 1.28])
}

/// Cuerpo de camélido con sus patas largas.
fn camelid(mut capsules: Vec<Capsule>, head: [f64; 3]) -> Character {
    let mut joints = vec![("head", head), ("tail_tip", [0.0, 0.9, -1.0])];
    for (s, side) in [(-1.0, "l"), (1.0, "r")] {
        let x = 0.25 * s;
        let front = [[x, 1.2, 0.5], [x, 0.95, 0.42], [x, 0.5, 0.48], [x, 0.1, 0.48], [x, 0.03, 0.55]];
        let back = [[x, 1.2, -0.5], [x, 0.85, -0.4], [x, 0.55, -0.62], [x, 0.1, -0.58], [x, 0.03, -0.52]];
        for leg in [front, back] {
            for w in leg.windows(2) {
                capsules.push(Capsule(w[0], w[1], 0.07));
            }
        }
        let name = |base: String| -> &'static str { Box::leak(base.into_boxed_str()) };
        joints.push((name(format!("wrist_{side}")), front[2]));
        joints.push((name(format!("hock_{side}")), back[2]));
        joints.push((name(format!("paw_f{side}")), front[4]));
        joints.push((name(format!("paw_b{side}")), back[4]));
    }
    Character { capsules, joints }
}
