//! Sombreado con ángulo de pliegue para el render.
//!
//! Las piezas de Diseñar llegan soldadas (un cubo tiene 8 vértices, para que la malla sea
//! cerrada en Reparar y Fabricar) y con una sola normal por vértice, promedio de todas sus
//! caras: las aristas vivas salen suavizadas y el render muestra degradados y manchas negras.
//! Aquí se vuelve a calcular la normal de cada esquina de cara promediando solo las caras
//! vecinas que no se desvían más del ángulo de pliegue, y los vértices se duplican donde la
//! normal cambia. La geometría no cambia; las normales con las que vino el modelo se respetan
//! cuando no son el promedio de sus caras (las de un modelo con normales de artista).

use converter_scene::{IndexData, Scene, VertexAttribute};
use std::collections::HashMap;

/// Ángulo (grados) desde el que una arista se considera viva
pub const CREASE_DEG: f32 = 40.0;

type V3 = [f32; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn len(a: V3) -> f32 {
    dot(a, a).sqrt()
}
fn unit(a: V3) -> Option<V3> {
    let l = len(a);
    (l > 1e-20).then(|| [a[0] / l, a[1] / l, a[2] / l])
}

fn gather<T: Copy>(data: &[T], src: &[u32]) -> Vec<T> {
    src.iter().map(|&i| data[i as usize]).collect()
}

/// Aplica el sombreado con pliegue a toda la escena (salta lo que tiene esqueleto)
pub fn crease_shade(scene: &mut Scene, crease_deg: f32) {
    let cos_limit = crease_deg.to_radians().cos();
    for mesh in &mut scene.meshes {
        for prim in &mut mesh.primitives {
            shade_primitive(prim, cos_limit);
        }
    }
}

fn shade_primitive(prim: &mut converter_scene::Primitive, cos_limit: f32) {
    // Con esqueleto las normales son del artista y se deforman: no se tocan
    if prim.attributes.iter().any(|a| matches!(a, VertexAttribute::JointIndices(_) | VertexAttribute::JointWeights(_))) {
        return;
    }
    let Some(positions) = prim.attributes.iter().find_map(|a| match a {
        VertexAttribute::Positions(p) => Some(p.clone()),
        _ => None,
    }) else {
        return;
    };
    let Some(stored) = prim.attributes.iter().find_map(|a| match a {
        VertexAttribute::Normals(n) => Some(n.clone()),
        _ => None,
    }) else {
        return;
    };
    let tris: Vec<u32> = match &prim.indices {
        Some(IndexData::U32(i)) => i.clone(),
        Some(IndexData::U16(i)) => i.iter().map(|&x| x as u32).collect(),
        None => (0..positions.len() as u32).collect(),
    };
    if tris.len() < 3 || stored.len() != positions.len() {
        return;
    }
    let face_count = tris.len() / 3;

    // Normal de cada cara (sin normalizar: pesa por el área) y su dirección
    let mut face_n: Vec<V3> = Vec::with_capacity(face_count);
    let mut face_u: Vec<Option<V3>> = Vec::with_capacity(face_count);
    for t in tris.chunks_exact(3) {
        let [a, b, c] = [t[0], t[1], t[2]].map(|i| positions[i as usize]);
        let n = cross(sub(b, a), sub(c, a));
        face_n.push(n);
        face_u.push(unit(n));
    }
    // Caras que tocan cada vértice
    let mut around: Vec<Vec<u32>> = vec![Vec::new(); positions.len()];
    for (f, t) in tris.chunks_exact(3).enumerate() {
        for &i in t {
            around[i as usize].push(f as u32);
        }
    }
    // Un vértice se recalcula solo si su normal es el promedio de sus caras
    let recompute: Vec<bool> = (0..positions.len())
        .map(|v| {
            let mut sum = [0.0f32; 3];
            for &f in &around[v] {
                for k in 0..3 {
                    sum[k] += face_n[f as usize][k];
                }
            }
            match (unit(sum), unit(stored[v])) {
                (Some(avg), Some(s)) => dot(avg, s) > 0.999,
                _ => false,
            }
        })
        .collect();
    if !recompute.iter().any(|&r| r) {
        return;
    }

    // Una esquina de cada cara: la normal con las caras vecinas dentro del pliegue
    let mut src: Vec<u32> = Vec::new();
    let mut normals: Vec<V3> = Vec::new();
    let mut indices: Vec<u32> = Vec::with_capacity(tris.len());
    let mut seen: HashMap<(u32, [u32; 3]), u32> = HashMap::new();
    for (f, t) in tris.chunks_exact(3).enumerate() {
        for &v in t {
            let n = if recompute[v as usize] {
                let mut sum = [0.0f32; 3];
                if let Some(fu) = face_u[f] {
                    for &g in &around[v as usize] {
                        if face_u[g as usize].is_some_and(|gu| dot(fu, gu) >= cos_limit) {
                            for k in 0..3 {
                                sum[k] += face_n[g as usize][k];
                            }
                        }
                    }
                }
                unit(sum).or_else(|| unit(stored[v as usize])).unwrap_or(stored[v as usize])
            } else {
                stored[v as usize]
            };
            let key = (v, n.map(f32::to_bits));
            let id = *seen.entry(key).or_insert_with(|| {
                src.push(v);
                normals.push(n);
                (src.len() - 1) as u32
            });
            indices.push(id);
        }
    }

    for attr in &mut prim.attributes {
        match attr {
            VertexAttribute::Positions(p) => *p = gather(p, &src),
            VertexAttribute::Normals(n) => *n = normals.clone(),
            VertexAttribute::Tangents(x) => *x = gather(x, &src),
            VertexAttribute::TexCoords(_, x) => *x = gather(x, &src),
            VertexAttribute::Colors(x) => *x = gather(x, &src),
            VertexAttribute::JointIndices(_) | VertexAttribute::JointWeights(_) => {}
        }
    }
    prim.indices = Some(IndexData::U32(indices));
}

#[cfg(test)]
mod tests {
    use super::*;
    use converter_scene::{Mesh, Primitive};

    /// Cubo soldado de 8 vértices con normales promediadas, como el de una pieza de Diseñar
    fn welded_box() -> Scene {
        let positions: Vec<V3> = (0..8).map(|i| [(i & 1) as f32, ((i >> 1) & 1) as f32, ((i >> 2) & 1) as f32]).collect();
        let quads: [[u32; 4]; 6] = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
        let mut indices = Vec::new();
        for q in quads {
            for t in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
                // Hacia fuera del cubo (su centro está en 0,5)
                let [a, b, c] = t.map(|i| positions[i as usize]);
                let n = cross(sub(b, a), sub(c, a));
                let out = [a[0] - 0.5, a[1] - 0.5, a[2] - 0.5];
                if dot(n, out) >= 0.0 {
                    indices.extend(t);
                } else {
                    indices.extend([t[0], t[2], t[1]]);
                }
            }
        }
        // Normal de cada vértice: el promedio por área de sus caras, como lo deja `weld_scene`
        let mut sums = vec![[0.0f32; 3]; positions.len()];
        for t in indices.chunks_exact(3) {
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| positions[i as usize]);
            let n = cross(sub(b, a), sub(c, a));
            for &i in t {
                for k in 0..3 {
                    sums[i as usize][k] += n[k];
                }
            }
        }
        let normals: Vec<V3> = sums.into_iter().map(|s| unit(s).unwrap()).collect();
        let mut scene = Scene::new();
        scene.meshes.push(Mesh {
            name: "caja".into(),
            primitives: vec![Primitive {
                attributes: vec![VertexAttribute::Positions(positions), VertexAttribute::Normals(normals)],
                indices: Some(IndexData::U32(indices)),
                material: None,
            }],
        });
        scene
    }

    fn data(scene: &Scene) -> (Vec<V3>, Vec<V3>, Vec<u32>) {
        let p = &scene.meshes[0].primitives[0];
        let pos = p.attributes.iter().find_map(|a| if let VertexAttribute::Positions(x) = a { Some(x.clone()) } else { None }).unwrap();
        let nor = p.attributes.iter().find_map(|a| if let VertexAttribute::Normals(x) = a { Some(x.clone()) } else { None }).unwrap();
        let Some(IndexData::U32(i)) = &p.indices else { panic!("sin índices") };
        (pos, nor, i.clone())
    }

    #[test]
    fn box_edges_become_hard() {
        let mut scene = welded_box();
        crease_shade(&mut scene, CREASE_DEG);
        let (pos, nor, idx) = data(&scene);
        // Cada vértice de la caja se parte en tres (una normal por cara que lo toca)
        assert_eq!(pos.len(), 24);
        assert_eq!(nor.len(), 24);
        for t in idx.chunks_exact(3) {
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| pos[i as usize]);
            let face = unit(cross(sub(b, a), sub(c, a))).unwrap();
            for &i in t {
                assert!(dot(face, nor[i as usize]) > 0.9999, "la normal de la esquina no es la de su cara");
            }
        }
    }

    #[test]
    fn smooth_curve_stays_smooth() {
        // Un prisma de 24 lados: sus caras laterales giran 15° entre sí (bajo el pliegue)
        let n = 24u32;
        let mut positions: Vec<V3> = Vec::new();
        for z in [0.0f32, 1.0] {
            for i in 0..n {
                let a = std::f32::consts::TAU * i as f32 / n as f32;
                positions.push([a.cos(), a.sin(), z]);
            }
        }
        let mut indices = Vec::new();
        for i in 0..n {
            let j = (i + 1) % n;
            indices.extend([i, j, n + j, i, n + j, n + i]);
        }
        let normals: Vec<V3> = positions.iter().map(|p| unit([p[0], p[1], 0.0]).unwrap()).collect();
        let mut scene = Scene::new();
        scene.meshes.push(Mesh {
            name: "prisma".into(),
            primitives: vec![Primitive {
                attributes: vec![VertexAttribute::Positions(positions), VertexAttribute::Normals(normals)],
                indices: Some(IndexData::U32(indices)),
                material: None,
            }],
        });
        crease_shade(&mut scene, CREASE_DEG);
        let (pos, _, _) = data(&scene);
        assert_eq!(pos.len(), (2 * n) as usize, "la pared curva no se parte");
    }

    #[test]
    fn authored_normals_are_kept() {
        // Normales que no son el promedio de sus caras (todas hacia arriba): no se tocan
        let mut scene = welded_box();
        if let VertexAttribute::Normals(n) = &mut scene.meshes[0].primitives[0].attributes[1] {
            n.iter_mut().for_each(|v| *v = [0.0, 0.0, 1.0]);
        }
        crease_shade(&mut scene, CREASE_DEG);
        let (pos, nor, _) = data(&scene);
        assert_eq!(pos.len(), 8);
        assert!(nor.iter().all(|n| *n == [0.0, 0.0, 1.0]));
    }
}
