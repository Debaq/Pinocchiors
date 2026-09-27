use converter_scene::{IndexData, Primitive, VertexAttribute};
use std::collections::HashMap;

/// Optimiza la geometría de una primitiva:
/// 1. Deduplicar vértices
/// 2. Strip triángulos degenerados
/// 3. Convertir índices U32 → U16 si es posible
pub(crate) fn optimize_primitive(prim: &mut Primitive) {
    deduplicate_vertices(prim);
    strip_degenerate_triangles(prim);
    downcast_indices(prim);
}

/// Deduplica vértices: vértices con datos idénticos se fusionan
/// y los índices se remapean.
fn deduplicate_vertices(prim: &mut Primitive) {
    let vertex_count = prim
        .attributes
        .iter()
        .find_map(|a| match a {
            VertexAttribute::Positions(p) => Some(p.len()),
            _ => None,
        })
        .unwrap_or(0);

    if vertex_count == 0 {
        return;
    }

    // Construir clave binaria por vértice
    let mut vertex_keys: Vec<Vec<u8>> = Vec::with_capacity(vertex_count);
    for i in 0..vertex_count {
        let mut key = Vec::new();
        for attr in &prim.attributes {
            match attr {
                VertexAttribute::Positions(v) => append_f32x3(&mut key, &v[i]),
                VertexAttribute::Normals(v) => append_f32x3(&mut key, &v[i]),
                VertexAttribute::Tangents(v) => append_f32x4(&mut key, &v[i]),
                VertexAttribute::TexCoords(_, v) => append_f32x2(&mut key, &v[i]),
                VertexAttribute::Colors(v) => append_f32x4(&mut key, &v[i]),
                VertexAttribute::JointIndices(v) => append_u16x4(&mut key, &v[i]),
                VertexAttribute::JointWeights(v) => append_f32x4(&mut key, &v[i]),
            }
        }
        vertex_keys.push(key);
    }

    // Dedup: old_index → new_index
    let mut unique_map: HashMap<&[u8], u32> = HashMap::new();
    let mut remap = Vec::with_capacity(vertex_count);
    let mut unique_indices: Vec<usize> = Vec::new();

    for (old_idx, key) in vertex_keys.iter().enumerate() {
        let next_new = unique_indices.len() as u32;
        let &mut new_idx = unique_map.entry(key.as_slice()).or_insert(next_new);
        if new_idx == next_new {
            unique_indices.push(old_idx);
        }
        remap.push(new_idx);
    }

    if unique_indices.len() == vertex_count {
        return; // No hay duplicados
    }

    // Reindexar atributos
    for attr in &mut prim.attributes {
        match attr {
            VertexAttribute::Positions(v) => *v = reindex_vec3(v, &unique_indices),
            VertexAttribute::Normals(v) => *v = reindex_vec3(v, &unique_indices),
            VertexAttribute::Tangents(v) => *v = reindex_vec4(v, &unique_indices),
            VertexAttribute::TexCoords(_, v) => *v = reindex_vec2(v, &unique_indices),
            VertexAttribute::Colors(v) => *v = reindex_vec4(v, &unique_indices),
            VertexAttribute::JointIndices(v) => *v = reindex_u16x4(v, &unique_indices),
            VertexAttribute::JointWeights(v) => *v = reindex_vec4(v, &unique_indices),
        }
    }

    // Remapear índices
    match &mut prim.indices {
        Some(IndexData::U16(idx)) => {
            for i in idx.iter_mut() {
                *i = remap[*i as usize] as u16;
            }
        }
        Some(IndexData::U32(idx)) => {
            for i in idx.iter_mut() {
                *i = remap[*i as usize];
            }
        }
        None => {}
    }
}

/// Elimina triángulos degenerados (2+ índices iguales).
fn strip_degenerate_triangles(prim: &mut Primitive) {
    let indices = match &mut prim.indices {
        Some(idx) => idx,
        None => return,
    };

    match indices {
        IndexData::U16(idx) => {
            let mut cleaned = Vec::with_capacity(idx.len());
            for tri in idx.as_chunks::<3>().0 {
                if tri[0] != tri[1] && tri[1] != tri[2] && tri[0] != tri[2] {
                    cleaned.extend_from_slice(tri);
                }
            }
            *idx = cleaned;
        }
        IndexData::U32(idx) => {
            let mut cleaned = Vec::with_capacity(idx.len());
            for tri in idx.as_chunks::<3>().0 {
                if tri[0] != tri[1] && tri[1] != tri[2] && tri[0] != tri[2] {
                    cleaned.extend_from_slice(tri);
                }
            }
            *idx = cleaned;
        }
    }
}

/// Convierte índices U32 a U16 si el mayor índice cabe en 16 bits.
fn downcast_indices(prim: &mut Primitive) {
    let indices = match &prim.indices {
        Some(idx) => idx,
        None => return,
    };

    if let IndexData::U32(idx) = indices {
        let max_val = idx.iter().copied().max().unwrap_or(0);
        if max_val < 65536 {
            let u16_idx: Vec<u16> = idx.iter().map(|&i| i as u16).collect();
            prim.indices = Some(IndexData::U16(u16_idx));
        }
    }
}

// -- Helpers para serialización binaria de claves --

fn append_f32x3(key: &mut Vec<u8>, v: &[f32; 3]) {
    for &f in v {
        key.extend_from_slice(&f.to_bits().to_le_bytes());
    }
}

fn append_f32x4(key: &mut Vec<u8>, v: &[f32; 4]) {
    for &f in v {
        key.extend_from_slice(&f.to_bits().to_le_bytes());
    }
}

fn append_f32x2(key: &mut Vec<u8>, v: &[f32; 2]) {
    for &f in v {
        key.extend_from_slice(&f.to_bits().to_le_bytes());
    }
}

fn append_u16x4(key: &mut Vec<u8>, v: &[u16; 4]) {
    for &u in v {
        key.extend_from_slice(&u.to_le_bytes());
    }
}

// -- Helpers para reindexación --

fn reindex_vec3(data: &[[f32; 3]], indices: &[usize]) -> Vec<[f32; 3]> {
    indices.iter().map(|&i| data[i]).collect()
}

fn reindex_vec4(data: &[[f32; 4]], indices: &[usize]) -> Vec<[f32; 4]> {
    indices.iter().map(|&i| data[i]).collect()
}

fn reindex_vec2(data: &[[f32; 2]], indices: &[usize]) -> Vec<[f32; 2]> {
    indices.iter().map(|&i| data[i]).collect()
}

fn reindex_u16x4(data: &[[u16; 4]], indices: &[usize]) -> Vec<[u16; 4]> {
    indices.iter().map(|&i| data[i]).collect()
}
