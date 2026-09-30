//! Compactar un mapa: las mismas cartas reempaquetadas lo más apretado
//! posible y las texturas re-horneadas desde el mapa anterior.
//!
//! Sirve para pintar sobre un mapa legible ([`crate::Layout::Paintable`]) y
//! exportar uno que aprovecha la textura. Las cartas no cambian de forma: solo
//! se giran, escalan y mueven, así que el re-horneado es exacto salvo el
//! remuestreo (bilineal). El normal map se lleva al marco tangente nuevo.

use crate::pack::{pack, ChartShape};
use crate::skin::{bake_materials, BakeInput};
use crate::unwrap::covering_triangles;
use crate::{corner_frames, scene_surface, skin_scene, Skin, SkinInfo};
use converter_scene::{Scene, VertexAttribute};
use std::collections::HashMap;

fn find(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

/// Carta de cada cara: caras que comparten un vértice con la misma UV.
fn uv_charts<const N: usize>(faces: &[[usize; N]], corners: &[[[f32; 2]; N]]) -> (Vec<usize>, usize) {
    let mut parent: Vec<usize> = (0..faces.len()).collect();
    let mut first: HashMap<(usize, [u32; 2]), usize> = HashMap::new();
    for (f, (face, uvs)) in faces.iter().zip(corners).enumerate() {
        for k in 0..N {
            let key = (face[k], uvs[k].map(f32::to_bits));
            let g = *first.entry(key).or_insert(f);
            let (a, b) = (find(&mut parent, f), find(&mut parent, g));
            parent[a] = b;
        }
    }
    let mut ids = HashMap::new();
    let charts = (0..faces.len())
        .map(|f| {
            let root = find(&mut parent, f);
            let next = ids.len();
            *ids.entry(root).or_insert(next)
        })
        .collect();
    (charts, ids.len())
}

/// La misma piel con sus cartas reempaquetadas en un atlas compacto de
/// `texture_size` px con `padding` px entre cartas, y todos sus canales
/// re-horneados en un material. Las caras sin UV (todas en cero) quedan como
/// una carta más.
pub fn compact_skin<const N: usize>(
    skin: &Skin<N>,
    positions: &[[f64; 3]],
    faces: &[[usize; N]],
    texture_size: u32,
    padding: u32,
) -> Skin<N> {
    let size = texture_size.max(1);
    let (chart_of, num_charts) = uv_charts(faces, &skin.corners);

    // El mapa ya está espejado en v (como lo deja el empaquetado): se
    // deshace para que el empaquetado lo vuelva a espejar y conserve el sentido
    let flip = |uv: [f32; 2]| [uv[0] as f64, -(uv[1] as f64)];
    let mut shapes: Vec<ChartShape> =
        (0..num_charts).map(|_| ChartShape { triangles: Vec::new(), area_3d: 0.0, area_uv: 0.0 }).collect();
    for (f, (face, uvs)) in faces.iter().zip(&skin.corners).enumerate() {
        let shape = &mut shapes[chart_of[f]];
        let uv = uvs.map(flip);
        shape.triangles.extend(covering_triangles(uv));
        for k in 1..N.saturating_sub(1) {
            let [a, b, c] = [0, k, k + 1].map(|i| positions[face[i]]);
            let (e1, e2) = ([0, 1, 2].map(|j| b[j] - a[j]), [0, 1, 2].map(|j| c[j] - a[j]));
            let cross = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
            shape.area_3d += 0.5 * (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
            let [p, q, r] = [uv[0], uv[k], uv[k + 1]];
            shape.area_uv += 0.5 * ((q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])).abs();
        }
    }
    let (placements, coverage) = pack(&shapes, size, padding);
    let corners: Vec<[[f32; 2]; N]> = skin
        .corners
        .iter()
        .enumerate()
        .map(|(f, uvs)| uvs.map(|uv| placements[chart_of[f]].apply(flip(uv)).map(|c| c as f32)))
        .collect();

    // Re-hornear desde la misma malla con el mapa anterior
    let (source, _) = skin_scene(positions, faces, Some(skin), &Scene::new());
    let (material, textures) = match scene_surface(&source) {
        Some(surface) => {
            let frames = corner_frames(positions, faces, &corners);
            let input = BakeInput { surface: &surface, positions, faces, corners: &corners, frames: &frames, size };
            bake_materials(&source, &input, padding.max(2) * 2)
        }
        None => (skin.materials.first().cloned().unwrap_or_default(), Vec::new()),
    };
    let stretch = match skin.info {
        SkinInfo::Unwrapped { stretch, .. } => stretch,
        SkinInfo::Transferred { .. } => 1.0,
    };
    Skin {
        corners,
        face_material: vec![Some(0); faces.len()],
        materials: vec![material],
        info: SkinInfo::Unwrapped { num_charts, stretch, coverage, texture_size: if textures.is_empty() { 0 } else { size } },
        textures,
    }
}

/// La escena con su mapa UV compactado (ver [`compact_skin`]): mismos
/// vértices en el mismo orden (los pesos y el rig no cambian), UV nuevas y un
/// solo material con las texturas re-horneadas. `None` si alguna primitiva
/// con caras no tiene UV o una malla se instancia más de una vez (sus cartas
/// se pisarían).
pub fn compacted_scene(scene: &Scene, texture_size: u32, padding: u32) -> Option<Scene> {
    let prims = scene.world_primitives();
    // Una malla en dos nodos repite (malla, primitiva)
    let mut seen = std::collections::HashSet::new();
    let instanced = !prims.iter().all(|p| seen.insert((p.mesh, p.primitive)));
    if instanced || prims.iter().any(|p| p.uvs.is_none() && !p.triangles.is_empty()) || prims.is_empty() {
        return None;
    }

    // Todas las primitivas como una malla, en el orden de sus vértices
    let mut positions: Vec<[f64; 3]> = Vec::new();
    let mut faces: Vec<[usize; 3]> = Vec::new();
    let mut corners: Vec<[[f32; 2]; 3]> = Vec::new();
    let mut face_material = Vec::new();
    let mut offsets = Vec::new();
    for p in &prims {
        let offset = positions.len();
        offsets.push(offset);
        positions.extend(p.positions.iter().map(|q| q.map(|c| c as f64)));
        let uvs = p.uvs.as_deref().unwrap_or(&[]);
        for t in &p.triangles {
            faces.push(t.map(|i| offset + i as usize));
            corners.push(t.map(|i| uvs.get(i as usize).copied().unwrap_or([0.0; 2])));
            face_material.push(p.material);
        }
    }
    if faces.is_empty() {
        return None;
    }
    let skin = Skin {
        corners,
        face_material,
        materials: scene.materials.clone(),
        textures: scene.textures.clone(),
        info: SkinInfo::Transferred { seam_faces: 0 },
    };
    let compact = compact_skin(&skin, &positions, &faces, texture_size, padding);

    // UV nueva por vértice (cada vértice cae en una sola carta)
    let mut new_uvs: Vec<[f32; 2]> = vec![[0.0; 2]; positions.len()];
    for (face, uvs) in faces.iter().zip(&compact.corners) {
        for k in 0..3 {
            new_uvs[face[k]] = uvs[k];
        }
    }
    let mut out = scene.clone();
    for (p, &offset) in prims.iter().zip(&offsets) {
        let primitive = &mut out.meshes[p.mesh].primitives[p.primitive];
        let uvs = new_uvs[offset..offset + p.positions.len()].to_vec();
        for attribute in &mut primitive.attributes {
            if let VertexAttribute::TexCoords(0, t) = attribute {
                *t = uvs.clone();
            }
        }
        primitive.material = Some(0);
    }
    out.materials = compact.materials;
    out.textures = compact.textures;
    Some(out)
}
