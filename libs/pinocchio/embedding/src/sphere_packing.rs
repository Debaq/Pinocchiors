//! Empaquetado de esferas para reducción

use crate::medial_surface::MedialSphere;
use pinocchio_math::Real;

/// Reduce un conjunto de esferas a un máximo especificado
///
/// Usa un algoritmo greedy que mantiene las esferas más representativas
pub fn pack_spheres(spheres: &[MedialSphere], max_spheres: usize) -> Vec<MedialSphere> {
    if spheres.len() <= max_spheres {
        return spheres.to_vec();
    }

    // Calcular importancia de cada esfera
    let mut importance: Vec<(usize, Real)> = spheres
        .iter()
        .enumerate()
        .map(|(i, s)| {
            // Importancia basada en radio y aislamiento
            let radius_score = s.radius;
            let isolation_score = compute_isolation(i, spheres);
            (i, radius_score * (1.0 + isolation_score))
        })
        .collect();

    // Ordenar por importancia (mayor primero)
    importance.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Seleccionar las top esferas, evitando redundancia
    let mut selected = Vec::new();
    let mut selected_indices = Vec::new();

    for (idx, _) in importance {
        if selected.len() >= max_spheres {
            break;
        }

        // Verificar que no sea muy similar a una ya seleccionada
        let sphere = &spheres[idx];
        let is_redundant = selected_indices.iter().any(|&sel_idx| {
            let sel_sphere: &MedialSphere = &spheres[sel_idx];
            let dist = sphere.center.distance(&sel_sphere.center);
            let overlap = (sphere.radius + sel_sphere.radius) - dist;
            overlap > sphere.radius * 0.5 // Más del 50% de overlap
        });

        if !is_redundant {
            selected.push(sphere.clone());
            selected_indices.push(idx);
        }
    }

    selected
}

/// Calcula el aislamiento de una esfera (distancia promedio a vecinos)
fn compute_isolation(index: usize, spheres: &[MedialSphere]) -> Real {
    if spheres.len() <= 1 {
        return 1.0;
    }

    let sphere = &spheres[index];
    let mut total_dist = 0.0;
    let mut count = 0;

    for (i, other) in spheres.iter().enumerate() {
        if i != index {
            total_dist += sphere.center.distance(&other.center);
            count += 1;
        }
    }

    if count > 0 {
        total_dist / count as Real
    } else {
        1.0
    }
}

/// Agrupa esferas cercanas en clusters
pub fn cluster_spheres(
    spheres: &[MedialSphere],
    num_clusters: usize,
) -> Vec<Vec<usize>> {
    if spheres.is_empty() || num_clusters == 0 {
        return Vec::new();
    }

    let n = spheres.len();
    let k = num_clusters.min(n);

    // K-means simplificado
    // Inicializar centroides con las esferas más grandes y separadas
    let mut centroids: Vec<usize> = Vec::with_capacity(k);
    let mut used = vec![false; n];

    // Primera esfera: la más grande
    let first = spheres
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.radius.partial_cmp(&b.1.radius).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(i, _)| i)
        .unwrap_or(0);
    centroids.push(first);
    used[first] = true;

    // Resto: las más alejadas de los centroides existentes
    while centroids.len() < k {
        let mut best_idx = 0;
        let mut best_min_dist = Real::NEG_INFINITY;

        for (i, sphere) in spheres.iter().enumerate() {
            if used[i] {
                continue;
            }

            let min_dist = centroids
                .iter()
                .map(|&c| sphere.center.distance(&spheres[c].center))
                .fold(Real::INFINITY, |a, b| a.min(b));

            if min_dist > best_min_dist {
                best_min_dist = min_dist;
                best_idx = i;
            }
        }

        centroids.push(best_idx);
        used[best_idx] = true;
    }

    // Asignar cada esfera al cluster más cercano
    let mut clusters = vec![Vec::new(); k];

    for (i, sphere) in spheres.iter().enumerate() {
        let closest = centroids
            .iter()
            .enumerate()
            .min_by(|a, b| {
                let da = sphere.center.distance(&spheres[*a.1].center);
                let db = sphere.center.distance(&spheres[*b.1].center);
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(idx, _)| idx)
            .unwrap_or(0);

        clusters[closest].push(i);
    }

    clusters
}

/// Une esferas de un cluster en una esfera representativa
pub fn merge_cluster(spheres: &[MedialSphere], indices: &[usize]) -> MedialSphere {
    if indices.is_empty() {
        return MedialSphere::new(pinocchio_math::Vector3::zero(), 0.0);
    }

    // Centro ponderado por radio
    let mut total_weight = 0.0;
    let mut weighted_center = pinocchio_math::Vector3::zero();
    let mut max_radius: Real = 0.0;

    for &idx in indices {
        let sphere = &spheres[idx];
        let weight = sphere.radius;
        weighted_center += sphere.center * weight;
        total_weight += weight;
        max_radius = max_radius.max(sphere.radius);
    }

    if total_weight > 1e-10 {
        weighted_center /= total_weight;
    }

    MedialSphere::new(weighted_center, max_radius)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinocchio_math::Vector3;

    #[test]
    fn test_pack_spheres() {
        let spheres = vec![
            MedialSphere::new(Vector3::new(0.0, 0.0, 0.0), 1.0),
            MedialSphere::new(Vector3::new(0.1, 0.0, 0.0), 0.9), // Muy cercana a la primera
            MedialSphere::new(Vector3::new(5.0, 0.0, 0.0), 0.8),
            MedialSphere::new(Vector3::new(10.0, 0.0, 0.0), 0.7),
        ];

        let packed = pack_spheres(&spheres, 2);
        assert_eq!(packed.len(), 2);
    }

    #[test]
    fn test_cluster_spheres() {
        let spheres = vec![
            MedialSphere::new(Vector3::new(0.0, 0.0, 0.0), 1.0),
            MedialSphere::new(Vector3::new(0.5, 0.0, 0.0), 1.0),
            MedialSphere::new(Vector3::new(10.0, 0.0, 0.0), 1.0),
            MedialSphere::new(Vector3::new(10.5, 0.0, 0.0), 1.0),
        ];

        let clusters = cluster_spheres(&spheres, 2);
        assert_eq!(clusters.len(), 2);

        // Cada cluster debería tener 2 esferas cercanas
        for cluster in &clusters {
            assert!(!cluster.is_empty());
        }
    }

    #[test]
    fn test_merge_cluster() {
        let spheres = vec![
            MedialSphere::new(Vector3::new(0.0, 0.0, 0.0), 1.0),
            MedialSphere::new(Vector3::new(2.0, 0.0, 0.0), 1.0),
        ];

        let merged = merge_cluster(&spheres, &[0, 1]);

        // Centro debería estar en el medio
        assert!((merged.center.x() - 1.0).abs() < 1e-10);
    }
}
