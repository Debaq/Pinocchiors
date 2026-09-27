//! Propiedades de robustez frente a escenas malformadas (p. ej. archivos
//! importados con índices o grafos de nodos corruptos).

use converter_scene::{IndexData, Mesh, Node, Primitive, Scene, Transform, VertexAttribute};
use proptest::prelude::*;

/// Escena aleatoria: posiciones, índices arbitrarios (pueden estar fuera de
/// rango), normales con largo posiblemente distinto y nodos con hijos
/// arbitrarios (pueden formar ciclos o apuntar fuera).
fn arb_scene() -> impl Strategy<Value = Scene> {
    let prim = (
        prop::collection::vec(prop::array::uniform3(-10.0f32..10.0), 0..12),
        prop::collection::vec(0u32..20, 0..30),
        prop::option::of(0usize..14),
    );
    let nodes = prop::collection::vec((prop::option::of(0usize..3), prop::collection::vec(0usize..6, 0..3), -3.0f32..3.0), 0..5);
    (prop::collection::vec(prim, 1..3), nodes).prop_map(|(prims, nodes)| {
        let mut scene = Scene::new();
        scene.meshes.push(Mesh {
            name: "m".into(),
            primitives: prims
                .into_iter()
                .map(|(pos, idx, normals)| {
                    let mut attributes = vec![VertexAttribute::Positions(pos)];
                    if let Some(n) = normals {
                        attributes.push(VertexAttribute::Normals(vec![[0.0, 0.0, 1.0]; n]));
                    }
                    Primitive { attributes, indices: Some(IndexData::U32(idx)), material: None }
                })
                .collect(),
        });
        for (mesh, children, scale) in nodes {
            scene.nodes.push(Node {
                name: String::new(),
                transform: Transform::Trs {
                    translation: glam::Vec3::ZERO,
                    rotation: glam::Quat::IDENTITY,
                    scale: glam::Vec3::splat(scale),
                },
                mesh: mesh.filter(|&m| m < scene.meshes.len()),
                skin: None,
                children: children.into_iter().filter(|&c| c < 5).collect(),
            });
        }
        scene
    })
}

proptest! {
    #[test]
    fn world_primitives_never_panics_and_only_keeps_valid_triangles(scene in arb_scene()) {
        for prim in scene.world_primitives() {
            let n = prim.positions.len() as u32;
            prop_assert!(prim.triangles.iter().flatten().all(|&i| i < n));
            if let Some(normals) = &prim.normals {
                prop_assert_eq!(normals.len(), prim.positions.len());
            }
        }
        let _ = scene.compute_bounding_box();
    }

    #[test]
    fn validate_geometry_rejects_exactly_bad_indices_and_lengths(scene in arb_scene()) {
        let bad = scene.meshes.iter().flat_map(|m| &m.primitives).any(|p| {
            let count = p.attributes.iter().find_map(|a| match a {
                VertexAttribute::Positions(v) => Some(v.len()),
                _ => None,
            }).unwrap();
            let bad_len = p.attributes.iter().any(|a| matches!(a, VertexAttribute::Normals(v) if v.len() != count));
            let bad_idx = match &p.indices {
                Some(IndexData::U32(idx)) => idx.iter().any(|&i| i as usize >= count),
                _ => false,
            };
            bad_len || bad_idx
        });
        prop_assert_eq!(scene.validate_geometry().is_err(), bad);
    }
}
