//! Relleno de agujeros en mallas

use pinocchio_mesh::Mesh;
use pinocchio_math::Vector3;
use crate::error::{RepairError, RepairResult};
use crate::config::{HoleFillConfig, HoleFillMethod};
use crate::analysis::boundary::{find_boundary_loops, BoundaryLoop};
use crate::triangulation::ear_clipping::triangulate_ear_clipping;
use crate::triangulation::liepa::{triangulate_liepa, LiepaConfig};

/// Rellena un agujero específico en la malla
///
/// # Argumentos
///
/// * `mesh` - La malla a modificar
/// * `boundary` - El boundary loop que define el agujero
/// * `config` - Configuración del relleno
///
/// # Retorna
///
/// Número de caras añadidas, o error si falla
pub fn fill_hole(
    mesh: &mut Mesh,
    boundary: &BoundaryLoop,
    config: &HoleFillConfig,
) -> RepairResult<usize> {
    if boundary.vertices.is_empty() {
        return Err(RepairError::InvalidBoundaryLoop);
    }

    if boundary.vertices.len() < 3 {
        return Err(RepairError::DegeneratePolygon(boundary.vertices.len()));
    }

    if boundary.vertices.len() > config.max_hole_size {
        return Err(RepairError::TriangulationFailed(format!(
            "agujero demasiado grande: {} vértices (máximo: {})",
            boundary.vertices.len(),
            config.max_hole_size
        )));
    }

    // Obtener las posiciones de los vértices del boundary
    let loop_vertices: Vec<Vector3> = boundary.vertices.iter()
        .map(|&v| mesh.vertices[v].position)
        .collect();

    // Calcular la normal del agujero
    let normal = boundary.normal(mesh);

    // Triangular según el método configurado
    let triangulation = match config.method {
        HoleFillMethod::EarClipping => {
            triangulate_ear_clipping(&loop_vertices, normal)?
        }
        HoleFillMethod::Liepa => {
            let liepa_config = LiepaConfig {
                refine: config.refine,
                max_area: f64::INFINITY,
                smooth_iterations: config.smooth_iterations,
            };
            triangulate_liepa(&loop_vertices, normal, &liepa_config)?
        }
    };

    // Convertir índices locales (en el loop) a índices globales (en la malla)
    let new_faces: Vec<[usize; 3]> = triangulation.triangles.iter()
        .map(|[a, b, c]| {
            // Invertir el winding para que las normales apunten hacia el interior correcto
            [boundary.vertices[*a], boundary.vertices[*c], boundary.vertices[*b]]
        })
        .collect();

    let faces_added = new_faces.len();

    // Reconstruir la malla con las nuevas caras
    let mut all_triangles: Vec<[usize; 3]> = (0..mesh.num_faces())
        .map(|i| mesh.get_face_vertices(i))
        .collect();

    all_triangles.extend(new_faces);

    let positions: Vec<Vector3> = mesh.vertices.iter()
        .map(|v| v.position)
        .collect();

    *mesh = Mesh::from_triangles(&positions, &all_triangles);

    // Aplicar suavizado si está configurado
    if config.smooth_iterations > 0 {
        smooth_filled_region(mesh, &boundary.vertices, config.smooth_iterations);
    }

    Ok(faces_added)
}

/// Rellena todos los agujeros de la malla
///
/// # Retorna
///
/// Tupla (agujeros_rellenados, caras_añadidas), o error
pub fn fill_all_holes(
    mesh: &mut Mesh,
    config: &HoleFillConfig,
) -> RepairResult<(usize, usize)> {
    let mut total_filled = 0;
    let mut total_faces = 0;

    loop {
        // Encontrar boundary loops (recalcular después de cada relleno)
        let loops = find_boundary_loops(mesh);

        if loops.is_empty() {
            break;
        }

        // Rellenar el loop más pequeño primero (más fácil de triangular)
        let smallest = loops.into_iter()
            .filter(|l| l.vertices.len() <= config.max_hole_size)
            .min_by_key(|l| l.vertices.len());

        match smallest {
            Some(boundary) => {
                match fill_hole(mesh, &boundary, config) {
                    Ok(faces) => {
                        total_filled += 1;
                        total_faces += faces;
                    }
                    Err(_) => {
                        // Si falla un agujero, continuar con los demás
                        break;
                    }
                }
            }
            None => break,  // No hay agujeros pequeños que podamos rellenar
        }
    }

    Ok((total_filled, total_faces))
}

/// Aplica suavizado Laplaciano a los vértices cerca de un agujero rellenado
fn smooth_filled_region(
    mesh: &mut Mesh,
    boundary_vertices: &[usize],
    iterations: usize,
) {
    use std::collections::HashSet;

    if iterations == 0 || boundary_vertices.is_empty() {
        return;
    }

    // Encontrar vértices a suavizar (los del boundary)
    let smoothable: HashSet<usize> = boundary_vertices.iter().cloned().collect();

    // Construir lista de vecinos para cada vértice
    let mut neighbors: Vec<Vec<usize>> = vec![Vec::new(); mesh.num_vertices()];

    for face_idx in 0..mesh.num_faces() {
        let [v0, v1, v2] = mesh.get_face_vertices(face_idx);
        neighbors[v0].push(v1);
        neighbors[v0].push(v2);
        neighbors[v1].push(v0);
        neighbors[v1].push(v2);
        neighbors[v2].push(v0);
        neighbors[v2].push(v1);
    }

    // Eliminar duplicados
    for n in &mut neighbors {
        n.sort();
        n.dedup();
    }

    // Aplicar suavizado
    for _ in 0..iterations {
        let mut new_positions: Vec<Vector3> = mesh.vertices.iter()
            .map(|v| v.position)
            .collect();

        for &v in &smoothable {
            if neighbors[v].is_empty() {
                continue;
            }

            // Promedio de vecinos
            let avg: Vector3 = neighbors[v].iter()
                .map(|&n| mesh.vertices[n].position)
                .fold(Vector3::zero(), |acc, p| acc + p)
                * (1.0 / neighbors[v].len() as f64);

            // Mezcla con posición original (factor 0.5)
            new_positions[v] = mesh.vertices[v].position * 0.5 + avg * 0.5;
        }

        // Aplicar nuevas posiciones
        for (i, pos) in new_positions.into_iter().enumerate() {
            mesh.vertices[i].position = pos;
        }
    }

    // Recalcular normales
    mesh.compute_vertex_normals();
}

/// Estima el tamaño promedio de arista alrededor de un boundary
pub fn estimate_edge_length(mesh: &Mesh, boundary: &BoundaryLoop) -> f64 {
    if boundary.vertices.len() < 2 {
        return 0.0;
    }

    let mut total_length = 0.0;
    let n = boundary.vertices.len();

    for i in 0..n {
        let v0 = boundary.vertices[i];
        let v1 = boundary.vertices[(i + 1) % n];
        let p0 = mesh.vertices[v0].position;
        let p1 = mesh.vertices[v1].position;
        total_length += (p1 - p0).length();
    }

    total_length / n as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_open_box() -> Mesh {
        // Cubo sin tapa superior (agujero de 4 vértices)
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),  // 0
            Vector3::new(1.0, 0.0, 0.0),  // 1
            Vector3::new(1.0, 1.0, 0.0),  // 2
            Vector3::new(0.0, 1.0, 0.0),  // 3
            Vector3::new(0.0, 0.0, 1.0),  // 4
            Vector3::new(1.0, 0.0, 1.0),  // 5
            Vector3::new(1.0, 1.0, 1.0),  // 6
            Vector3::new(0.0, 1.0, 1.0),  // 7
        ];
        let faces = vec![
            // Base
            [0, 2, 1], [0, 3, 2],
            // Frente
            [0, 1, 5], [0, 5, 4],
            // Derecha
            [1, 2, 6], [1, 6, 5],
            // Atrás
            [2, 3, 7], [2, 7, 6],
            // Izquierda
            [3, 0, 4], [3, 4, 7],
            // Tapa omitida
        ];
        Mesh::from_triangles(&vertices, &faces)
    }

    #[test]
    fn test_fill_single_hole() {
        let mut mesh = make_open_box();
        assert!(!mesh.is_closed());

        let loops = find_boundary_loops(&mesh);
        assert_eq!(loops.len(), 1);
        assert_eq!(loops[0].vertices.len(), 4);

        let config = HoleFillConfig::default();
        let result = fill_hole(&mut mesh, &loops[0], &config);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 2);  // Cuadrilátero → 2 triángulos
        assert!(mesh.is_closed());
    }

    #[test]
    fn test_fill_all_holes() {
        let mut mesh = make_open_box();
        let config = HoleFillConfig::default();

        let (filled, faces) = fill_all_holes(&mut mesh, &config).unwrap();

        assert_eq!(filled, 1);
        assert_eq!(faces, 2);
        assert!(mesh.is_closed());
    }

    #[test]
    fn test_fill_with_liepa() {
        let mut mesh = make_open_box();
        let loops = find_boundary_loops(&mesh);

        let config = HoleFillConfig {
            method: HoleFillMethod::Liepa,
            ..Default::default()
        };

        let result = fill_hole(&mut mesh, &loops[0], &config);
        assert!(result.is_ok());
        assert!(mesh.is_closed());
    }

    #[test]
    fn test_fill_with_smoothing() {
        let mut mesh = make_open_box();
        let loops = find_boundary_loops(&mesh);

        let config = HoleFillConfig {
            smooth_iterations: 2,
            ..Default::default()
        };

        let result = fill_hole(&mut mesh, &loops[0], &config);
        assert!(result.is_ok());
        assert!(mesh.integrity_check().is_ok());
    }

    #[test]
    fn test_estimate_edge_length() {
        let mesh = make_open_box();
        let loops = find_boundary_loops(&mesh);

        let avg_length = estimate_edge_length(&mesh, &loops[0]);
        assert!(avg_length > 0.9 && avg_length < 1.1);  // Cuadrado de lado 1
    }
}
