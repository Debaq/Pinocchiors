//! Material por triángulos: parte las primitivas para que las caras elegidas
//! queden en una primitiva propia con su material.

use std::collections::BTreeMap;

use crate::{IndexData, Primitive, Scene, VertexAttribute};

/// Triángulos válidos de una primitiva, en el orden de [`Scene::world_primitives`]
/// (`None` si no tiene posiciones)
fn valid_triangles(prim: &Primitive) -> Option<Vec<[u32; 3]>> {
    let count = prim.attributes.iter().find_map(|a| match a {
        VertexAttribute::Positions(p) => Some(p.len()),
        _ => None,
    })?;
    let indices: Vec<u32> = match &prim.indices {
        Some(IndexData::U16(i)) => i.iter().map(|&i| i as u32).collect(),
        Some(IndexData::U32(i)) => i.clone(),
        None => (0..count as u32).collect(),
    };
    Some(
        indices
            .as_chunks::<3>()
            .0
            .iter()
            .filter(|t| t.iter().all(|&i| (i as usize) < count))
            .copied()
            .collect(),
    )
}

/// Una primitiva con solo estos triángulos, con los vértices que usan
fn extract(prim: &Primitive, triangles: &[[u32; 3]]) -> Primitive {
    let count = prim.attributes.iter().find_map(|a| match a {
        VertexAttribute::Positions(p) => Some(p.len()),
        _ => None,
    });
    let count = count.unwrap_or(0);
    let mut new_of = vec![u32::MAX; count];
    let mut old_of: Vec<u32> = Vec::new();
    let indices: Vec<u32> = triangles
        .iter()
        .flatten()
        .map(|&i| {
            if new_of[i as usize] == u32::MAX {
                new_of[i as usize] = old_of.len() as u32;
                old_of.push(i);
            }
            new_of[i as usize]
        })
        .collect();

    fn pick<T: Clone>(values: &[T], count: usize, old_of: &[u32]) -> Vec<T> {
        if values.len() != count {
            return values.to_vec();
        }
        old_of.iter().map(|&i| values[i as usize].clone()).collect()
    }
    let attributes = prim
        .attributes
        .iter()
        .map(|a| match a {
            VertexAttribute::Positions(v) => VertexAttribute::Positions(pick(v, count, &old_of)),
            VertexAttribute::Normals(v) => VertexAttribute::Normals(pick(v, count, &old_of)),
            VertexAttribute::Tangents(v) => VertexAttribute::Tangents(pick(v, count, &old_of)),
            VertexAttribute::TexCoords(set, v) => VertexAttribute::TexCoords(*set, pick(v, count, &old_of)),
            VertexAttribute::Colors(v) => VertexAttribute::Colors(pick(v, count, &old_of)),
            VertexAttribute::JointIndices(v) => VertexAttribute::JointIndices(pick(v, count, &old_of)),
            VertexAttribute::JointWeights(v) => VertexAttribute::JointWeights(pick(v, count, &old_of)),
        })
        .collect();
    let indices = match &prim.indices {
        Some(IndexData::U16(_)) => IndexData::U16(indices.iter().map(|&i| i as u16).collect()),
        _ => IndexData::U32(indices),
    };
    Primitive { attributes, indices: Some(indices), material: prim.material }
}

impl Scene {
    /// Pone `material` a los triángulos `faces`, numerados como los dibuja el
    /// visor: los de [`Scene::world_primitives`] uno tras otro. Una primitiva
    /// con solo parte de sus caras elegidas se parte en dos (la parte elegida
    /// va al final de la malla); los quads no se separan. Devuelve si la
    /// geometría cambió (hubo que partir alguna primitiva).
    pub fn assign_material_to_faces(&mut self, faces: &[u32], material: Option<usize>) -> bool {
        let mut faces = faces.to_vec();
        faces.sort_unstable();
        faces.dedup();

        // Caras elegidas de cada primitiva (una malla instanciada varias veces
        // se parte una sola vez: las instancias comparten la geometría)
        let mut chosen: BTreeMap<(usize, usize), Vec<bool>> = BTreeMap::new();
        let mut base = 0usize;
        for instance in self.mesh_instances() {
            for (index, prim) in self.meshes[instance.mesh].primitives.iter().enumerate() {
                let Some(count) = valid_triangles(prim).map(|t| t.len()) else { continue };
                let from = faces.partition_point(|&f| (f as usize) < base);
                let to = faces.partition_point(|&f| (f as usize) < base + count);
                if from < to {
                    let flags = chosen.entry((instance.mesh, index)).or_insert_with(|| vec![false; count]);
                    for &f in &faces[from..to] {
                        flags[f as usize - base] = true;
                    }
                }
                base += count;
            }
        }

        let mut split = false;
        for ((mesh, index), mut flags) in chosen {
            let prim = &self.meshes[mesh].primitives[index];
            if prim.material == material {
                continue;
            }
            let Some(triangles) = valid_triangles(prim) else { continue };
            let pairs = self.quad_pairs(mesh, index).min(triangles.len() / 2);
            for k in 0..pairs {
                let any = flags[2 * k] || flags[2 * k + 1];
                flags[2 * k] = any;
                flags[2 * k + 1] = any;
            }
            let picked = flags.iter().filter(|&&f| f).count();
            if picked == triangles.len() {
                self.meshes[mesh].primitives[index].material = material;
                continue;
            }

            // Cada parte con sus quads al comienzo, como se guardan
            let part = |wanted: bool| -> (Vec<[u32; 3]>, usize) {
                let mut out = Vec::new();
                let mut quads = 0;
                for k in 0..pairs {
                    if flags[2 * k] == wanted {
                        out.extend([triangles[2 * k], triangles[2 * k + 1]]);
                        quads += 1;
                    }
                }
                for i in 2 * pairs..triangles.len() {
                    if flags[i] == wanted {
                        out.push(triangles[i]);
                    }
                }
                (out, quads)
            };
            let (kept, kept_pairs) = part(false);
            let (chosen_tris, chosen_pairs) = part(true);
            let kept_prim = extract(prim, &kept);
            let mut new_prim = extract(prim, &chosen_tris);
            new_prim.material = material;

            let primitives = &mut self.meshes[mesh].primitives;
            primitives[index] = kept_prim;
            primitives.push(new_prim);
            let new_index = primitives.len() - 1;
            self.set_quad_pairs(mesh, index, kept_pairs);
            self.set_quad_pairs(mesh, new_index, chosen_pairs);
            split = true;
        }
        split
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Mesh;

    /// Un cuadrado de dos quads (4 triángulos) en una sola primitiva
    fn scene(pairs: usize) -> Scene {
        let positions: Vec<[f32; 3]> = (0..6).map(|i| [i as f32, 0.0, 0.0]).collect();
        let prim = Primitive {
            attributes: vec![
                VertexAttribute::Positions(positions),
                VertexAttribute::TexCoords(0, (0..6).map(|i| [i as f32, 0.0]).collect()),
            ],
            indices: Some(IndexData::U32(vec![0, 1, 2, 0, 2, 3, 2, 3, 4, 3, 4, 5])),
            material: Some(0),
        };
        let mut scene = Scene::new();
        scene.meshes.push(Mesh { name: "m".into(), primitives: vec![prim] });
        scene.materials.push(Default::default());
        scene.materials.push(Default::default());
        scene.set_quad_pairs(0, 0, pairs);
        scene
    }

    #[test]
    fn partial_selection_splits_the_primitive() {
        let mut s = scene(0);
        assert!(s.assign_material_to_faces(&[1, 3], Some(1)));
        let prims = &s.meshes[0].primitives;
        assert_eq!(prims.len(), 2);
        assert_eq!((prims[0].material, prims[1].material), (Some(0), Some(1)));
        assert_eq!(valid_triangles(&prims[0]).unwrap().len(), 2);
        assert_eq!(valid_triangles(&prims[1]).unwrap().len(), 2);
        // Los vértices se compactan y llevan sus UV
        let VertexAttribute::TexCoords(_, uv) = &prims[1].attributes[1] else { panic!() };
        let VertexAttribute::Positions(p) = &prims[1].attributes[0] else { panic!() };
        assert_eq!(uv.len(), p.len());
        assert!(p.len() < 6);
    }

    #[test]
    fn all_faces_just_change_the_material() {
        let mut s = scene(0);
        assert!(!s.assign_material_to_faces(&[0, 1, 2, 3], Some(1)));
        assert_eq!(s.meshes[0].primitives.len(), 1);
        assert_eq!(s.meshes[0].primitives[0].material, Some(1));
    }

    #[test]
    fn same_material_changes_nothing() {
        let mut s = scene(0);
        assert!(!s.assign_material_to_faces(&[0], Some(0)));
        assert_eq!(s.meshes[0].primitives.len(), 1);
    }

    #[test]
    fn quads_stay_together() {
        let mut s = scene(2);
        // Un solo triángulo del primer quad arrastra al otro
        assert!(s.assign_material_to_faces(&[0], Some(1)));
        assert_eq!(valid_triangles(&s.meshes[0].primitives[1]).unwrap().len(), 2);
        assert_eq!((s.quad_pairs(0, 0), s.quad_pairs(0, 1)), (1, 1));
    }
}
