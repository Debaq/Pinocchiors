//! Reducción de triángulos con meshoptimizer.
//!
//! Colapsa aristas minimizando el error cuadrático. Los vértices que comparten
//! posición pero no atributos (costuras de UV, aristas vivas con normales
//! partidas) se tratan como bordes: la silueta de las islas UV se conserva.

use crate::geometry;
use crate::options::Simplification;
use converter_scene::{IndexData, Primitive, VertexAttribute};

/// Reduce la primitiva según `options`. Deja los vértices compactos y en
/// orden de primer uso (mejor caché de vértices y mejor compresión).
pub(crate) fn simplify_primitive(prim: &mut Primitive, options: &Simplification) {
    let Some(positions) = prim.attributes.iter().find_map(|a| match a {
        VertexAttribute::Positions(p) => Some(p),
        _ => None,
    }) else {
        return;
    };
    let indices = geometry::indices_u32(prim);
    let ratio = options.ratio.clamp(0.0, 1.0);
    if indices.len() < 3 || ratio >= 1.0 {
        return;
    }

    let bytes: Vec<u8> = positions.iter().flatten().flat_map(|f| f.to_le_bytes()).collect();
    let Ok(adapter) = meshopt::VertexDataAdapter::new(&bytes, 12, 0) else {
        return;
    };
    let target = ((indices.len() as f32 * ratio) as usize / 3 * 3).max(3);
    let simplified = meshopt::simplify(
        &indices,
        &adapter,
        target,
        options.max_error.max(0.0),
        meshopt::SimplifyOptions::None,
        None,
    );
    if simplified.is_empty() || simplified.len() == indices.len() {
        return;
    }
    let simplified = meshopt::optimize_vertex_cache(&simplified, positions.len());

    // Compactar: solo los vértices usados, en orden de primer uso
    let mut remap = vec![u32::MAX; positions.len()];
    let mut kept = Vec::new();
    let indices: Vec<u32> = simplified
        .iter()
        .map(|&i| {
            let slot = &mut remap[i as usize];
            if *slot == u32::MAX {
                *slot = kept.len() as u32;
                kept.push(i as usize);
            }
            *slot
        })
        .collect();
    geometry::reindex_attributes(prim, &kept);
    prim.indices = Some(if kept.len() <= u16::MAX as usize + 1 {
        IndexData::U16(indices.iter().map(|&i| i as u16).collect())
    } else {
        IndexData::U32(indices)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Grilla plana de n×n cuadrados (2 triángulos cada uno)
    fn grid(n: u32) -> Primitive {
        let mut positions = Vec::new();
        let mut uvs = Vec::new();
        for y in 0..=n {
            for x in 0..=n {
                let (u, v) = (x as f32 / n as f32, y as f32 / n as f32);
                // Ondulación suave para que no sea trivialmente plana
                positions.push([u, v, 0.02 * (u * 6.0).sin()]);
                uvs.push([u, v]);
            }
        }
        let mut indices = Vec::new();
        for y in 0..n {
            for x in 0..n {
                let a = y * (n + 1) + x;
                let (b, c, d) = (a + 1, a + n + 1, a + n + 2);
                indices.extend_from_slice(&[a, b, d, a, d, c]);
            }
        }
        Primitive {
            attributes: vec![VertexAttribute::Positions(positions), VertexAttribute::TexCoords(0, uvs)],
            indices: Some(IndexData::U32(indices)),
            material: None,
        }
    }

    fn triangle_count(prim: &Primitive) -> usize {
        geometry::indices_u32(prim).len() / 3
    }

    #[test]
    fn reduces_towards_ratio_and_compacts_vertices() {
        let mut prim = grid(40);
        let before = triangle_count(&prim);
        simplify_primitive(&mut prim, &Simplification { ratio: 0.25, max_error: 0.05 });
        let after = triangle_count(&prim);
        assert!(after <= before / 3, "{before} → {after}");
        assert!(after > 0);

        let n = geometry::vertex_count(&prim);
        let indices = geometry::indices_u32(&prim);
        assert!(indices.iter().all(|&i| (i as usize) < n));
        // Todos los vértices quedan referenciados
        let mut used = vec![false; n];
        indices.iter().for_each(|&i| used[i as usize] = true);
        assert!(used.iter().all(|&u| u));
        // Los atributos siguen alineados
        let uvs = prim.attributes.iter().find_map(|a| match a {
            VertexAttribute::TexCoords(_, v) => Some(v.len()),
            _ => None,
        });
        assert_eq!(uvs, Some(n));
    }

    #[test]
    fn error_limit_stops_reduction() {
        let mut loose = grid(40);
        let mut strict = grid(40);
        simplify_primitive(&mut loose, &Simplification { ratio: 0.01, max_error: 0.1 });
        simplify_primitive(&mut strict, &Simplification { ratio: 0.01, max_error: 1e-5 });
        assert!(triangle_count(&strict) > triangle_count(&loose));
    }

    #[test]
    fn ratio_one_leaves_mesh_untouched() {
        let mut prim = grid(4);
        simplify_primitive(&mut prim, &Simplification { ratio: 1.0, max_error: 1.0 });
        assert_eq!(triangle_count(&prim), 32);
    }
}
