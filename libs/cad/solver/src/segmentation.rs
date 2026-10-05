//! Segmentación de malla por region growing basado en normales.

use nalgebra::Vector3;
use std::collections::VecDeque;

/// Resultado de segmentación: lista de regiones, cada una con índices de caras.
#[derive(Debug, Clone)]
pub struct SegmentationResult {
    pub regions: Vec<Region>,
}

#[derive(Debug, Clone)]
pub struct Region {
    pub face_indices: Vec<usize>,
    pub average_normal: [f64; 3],
}

/// Segmentar malla por region growing.
///
/// - `face_normals`: normal de cada cara [nx, ny, nz]
/// - `face_adjacency`: para cada cara, lista de caras adyacentes
/// - `angle_threshold_deg`: ángulo máximo entre normales para agrupar (grados)
pub fn segment_by_normals(
    face_normals: &[[f64; 3]],
    face_adjacency: &[Vec<usize>],
    angle_threshold_deg: f64,
) -> SegmentationResult {
    let n_faces = face_normals.len();
    let cos_threshold = angle_threshold_deg.to_radians().cos();
    let mut visited = vec![false; n_faces];
    let mut regions = Vec::new();

    // Calcular curvatura aproximada por cara (variación de normal con vecinos)
    let mut curvature = vec![0.0f64; n_faces];
    for i in 0..n_faces {
        let ni = Vector3::from_row_slice(&face_normals[i]);
        if face_adjacency[i].is_empty() {
            continue;
        }
        let mut sum = 0.0;
        for &j in &face_adjacency[i] {
            let nj = Vector3::from_row_slice(&face_normals[j]);
            sum += 1.0 - ni.dot(&nj).abs();
        }
        curvature[i] = sum / face_adjacency[i].len() as f64;
    }

    // Ordenar seeds por menor curvatura (zonas más planas primero)
    let mut seeds: Vec<usize> = (0..n_faces).collect();
    seeds.sort_by(|&a, &b| curvature[a].partial_cmp(&curvature[b]).unwrap());

    for seed in seeds {
        if visited[seed] {
            continue;
        }

        let mut region_faces = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back(seed);
        visited[seed] = true;

        let seed_normal = Vector3::from_row_slice(&face_normals[seed]);
        let mut avg_normal = seed_normal;

        while let Some(current) = queue.pop_front() {
            region_faces.push(current);

            for &neighbor in &face_adjacency[current] {
                if visited[neighbor] {
                    continue;
                }

                let n_neighbor = Vector3::from_row_slice(&face_normals[neighbor]);
                let cos_angle = avg_normal.normalize().dot(&n_neighbor.normalize());

                if cos_angle >= cos_threshold {
                    visited[neighbor] = true;
                    queue.push_back(neighbor);

                    // Actualizar normal promedio incrementalmente
                    let count = region_faces.len() as f64 + 1.0;
                    avg_normal = avg_normal * ((count - 1.0) / count) + n_neighbor / count;
                }
            }
        }

        let avg = avg_normal.normalize();
        regions.push(Region {
            face_indices: region_faces,
            average_normal: [avg.x, avg.y, avg.z],
        });
    }

    SegmentationResult { regions }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_segment_cube_faces() {
        // Cubo: 6 caras con normales distintas, cada cara se agrupa sola
        let normals = vec![
            [1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
        ];
        // Sin adyacencia para simplificar
        let adj = vec![vec![]; 6];

        let result = segment_by_normals(&normals, &adj, 10.0);
        assert_eq!(result.regions.len(), 6);
    }

    #[test]
    fn test_segment_coplanar() {
        // 4 caras coplanares deberían agruparse en 1 región
        let normals = vec![
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
        ];
        let adj = vec![vec![1], vec![0, 2], vec![1, 3], vec![2]];

        let result = segment_by_normals(&normals, &adj, 10.0);
        assert_eq!(result.regions.len(), 1);
        assert_eq!(result.regions[0].face_indices.len(), 4);
    }
}
