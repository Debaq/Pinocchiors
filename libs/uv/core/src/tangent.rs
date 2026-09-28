//! Normales y tangentes por esquina de una malla con UV.
//!
//! El horneado de normales y la exportación usan exactamente estos valores:
//! el normal map horneado solo es correcto si el visor reconstruye el mismo
//! marco tangente (glTF: `B = w · (N × T)`).

use crate::surface::{tangent_handedness, uv_derivatives};
use pinocchio_math::Vector3;
use std::collections::HashMap;

/// Normal y tangente (con signo en `w`) de cada esquina de cada cara.
#[derive(Debug, Clone, PartialEq)]
pub struct CornerFrames<const N: usize> {
    pub normals: Vec<[[f32; 3]; N]>,
    pub tangents: Vec<[[f32; 4]; N]>,
}

/// Normales suaves por vértice (promedio ponderado por área de los
/// triángulos en abanico que lo tocan) y tangentes por (vértice, UV): los
/// vértices a ambos lados de una costura UV tienen tangentes propias.
pub fn corner_frames<const N: usize>(
    positions: &[[f64; 3]],
    faces: &[[usize; N]],
    corners: &[[[f32; 2]; N]],
) -> CornerFrames<N> {
    let points: Vec<Vector3> = positions.iter().map(|p| Vector3::new(p[0], p[1], p[2])).collect();
    let fan = |k: usize| [0, k, k + 1];

    let mut normal_sums = vec![Vector3::zero(); points.len()];
    for face in faces {
        for k in 1..N.saturating_sub(1) {
            let [a, b, c] = fan(k).map(|i| points[face[i]]);
            let n = (b - a).cross(&(c - a));
            for i in fan(k) {
                normal_sums[face[i]] += n;
            }
        }
    }
    let normals: Vec<Vector3> = normal_sums.iter().map(|n| n.try_normalize().unwrap_or(Vector3::unit_z())).collect();

    // Derivadas UV acumuladas por (vértice, UV exacta)
    let key = |v: usize, uv: [f32; 2]| (v, uv.map(f32::to_bits));
    let mut derivatives: HashMap<(usize, [u32; 2]), (Vector3, Vector3)> = HashMap::new();
    for (face, uvs) in faces.iter().zip(corners) {
        for k in 1..N.saturating_sub(1) {
            let ids = fan(k);
            let p = ids.map(|i| points[face[i]]);
            let uv = ids.map(|i| uvs[i].map(|c| c as f64));
            let (dpdu, dpdv) = uv_derivatives(p, uv);
            for i in ids {
                let entry = derivatives.entry(key(face[i], uvs[i])).or_insert((Vector3::zero(), Vector3::zero()));
                *entry = (entry.0 + dpdu, entry.1 + dpdv);
            }
        }
    }

    let mut frame_normals = Vec::with_capacity(faces.len());
    let mut frame_tangents = Vec::with_capacity(faces.len());
    for (face, uvs) in faces.iter().zip(corners) {
        frame_normals.push(std::array::from_fn(|k| {
            let n = normals[face[k]];
            [n.x() as f32, n.y() as f32, n.z() as f32]
        }));
        frame_tangents.push(std::array::from_fn(|k| {
            let n = normals[face[k]];
            let (dpdu, dpdv) = derivatives.get(&key(face[k], uvs[k])).copied().unwrap_or((Vector3::zero(), Vector3::zero()));
            let (t, w) = tangent_handedness(n, dpdu, dpdv);
            [t.x() as f32, t.y() as f32, t.z() as f32, w as f32]
        }));
    }
    CornerFrames { normals: frame_normals, tangents: frame_tangents }
}
