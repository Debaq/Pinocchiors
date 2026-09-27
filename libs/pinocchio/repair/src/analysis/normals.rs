//! Análisis de orientación de normales

use pinocchio_mesh::Mesh;
use serde::Serialize;
use std::collections::VecDeque;

/// Resultado del análisis de orientación de normales
#[derive(Debug, Clone, Serialize)]
pub struct NormalAnalysis {
    /// Si todas las normales son consistentes (misma orientación relativa)
    pub is_consistent: bool,
    /// Número de componentes conectados
    pub components: usize,
    /// Caras que necesitan ser invertidas para consistencia
    pub faces_to_flip: Vec<usize>,
    /// Si las normales apuntan hacia afuera (solo válido si es cerrada)
    pub outward_facing: Option<bool>,
}

/// Analiza la orientación de las normales de la malla
///
/// Usa BFS para propagar la orientación desde una cara semilla y detectar
/// inconsistencias cuando dos caras adyacentes tienen orientaciones opuestas.
///
/// # Algoritmo
///
/// 1. Empezar desde la cara con el vértice de menor Z (probablemente exterior)
/// 2. BFS por adyacencia de caras
/// 3. Para cada cara visitada, verificar si su orientación es consistente con la vecina
/// 4. Marcar caras que necesitan ser invertidas
///
/// Complejidad: O(F) donde F es el número de caras
pub fn analyze_normal_orientation(mesh: &Mesh) -> NormalAnalysis {
    let num_faces = mesh.num_faces();

    if num_faces == 0 {
        return NormalAnalysis {
            is_consistent: true,
            components: 0,
            faces_to_flip: Vec::new(),
            outward_facing: None,
        };
    }

    // Estado de cada cara: None = no visitada, Some(false) = orientación original, Some(true) = invertir
    let mut face_state: Vec<Option<bool>> = vec![None; num_faces];
    let mut faces_to_flip = Vec::new();
    let mut components = 0;
    let mut is_consistent = true;

    // Procesar todos los componentes conectados
    loop {
        // Encontrar la siguiente cara no visitada (preferir la de menor Z como semilla)
        let seed = find_seed_face(mesh, &face_state);
        let Some(seed_face) = seed else {
            break;
        };

        components += 1;
        face_state[seed_face] = Some(false); // La semilla mantiene su orientación

        // BFS desde la semilla
        let mut queue = VecDeque::new();
        queue.push_back(seed_face);

        while let Some(face_idx) = queue.pop_front() {
            let current_flip = face_state[face_idx].unwrap();
            let neighbors = mesh.get_face_neighbors(face_idx);

            for neighbor_idx in neighbors {
                // Verificar si ya fue visitada
                if let Some(neighbor_flip) = face_state[neighbor_idx] {
                    // Ya visitada, verificar consistencia
                    let expected_flip = should_neighbor_flip(mesh, face_idx, neighbor_idx, current_flip);
                    if neighbor_flip != expected_flip {
                        is_consistent = false;
                    }
                } else {
                    // No visitada, calcular su orientación
                    let should_flip = should_neighbor_flip(mesh, face_idx, neighbor_idx, current_flip);
                    face_state[neighbor_idx] = Some(should_flip);
                    if should_flip {
                        faces_to_flip.push(neighbor_idx);
                    }
                    queue.push_back(neighbor_idx);
                }
            }
        }
    }

    // Verificar si las normales apuntan hacia afuera (solo si es cerrada y consistente)
    let outward_facing = if mesh.is_closed() && is_consistent {
        Some(check_outward_facing(mesh))
    } else {
        None
    };

    NormalAnalysis {
        is_consistent,
        components,
        faces_to_flip,
        outward_facing,
    }
}

/// Encuentra la cara semilla ideal (la que tiene el vértice con menor Z)
fn find_seed_face(mesh: &Mesh, face_state: &[Option<bool>]) -> Option<usize> {
    let mut best_face = None;
    let mut min_z = f64::INFINITY;

    for (face_idx, state) in face_state.iter().enumerate().take(mesh.num_faces()) {
        if state.is_some() {
            continue;
        }

        let [p0, p1, p2] = mesh.get_face_positions(face_idx);
        let face_min_z = p0.z().min(p1.z()).min(p2.z());

        if face_min_z < min_z {
            min_z = face_min_z;
            best_face = Some(face_idx);
        }
    }

    best_face
}

/// Determina si una cara vecina debe invertirse para ser consistente
///
/// Dos caras adyacentes son consistentes si la arista compartida tiene
/// direcciones opuestas en cada cara.
fn should_neighbor_flip(
    mesh: &Mesh,
    face_a: usize,
    face_b: usize,
    face_a_flipped: bool,
) -> bool {
    // Encontrar la arista compartida
    let verts_a = mesh.get_face_vertices(face_a);
    let verts_b = mesh.get_face_vertices(face_b);

    // Buscar los dos vértices compartidos
    let mut shared = Vec::new();
    for &va in &verts_a {
        if verts_b.contains(&va) {
            shared.push(va);
        }
    }

    if shared.len() != 2 {
        // No comparten exactamente una arista (no deberían ser vecinos)
        return false;
    }

    // Encontrar el orden de la arista compartida en cada cara
    let order_a = edge_order_in_face(&verts_a, shared[0], shared[1]);
    let order_b = edge_order_in_face(&verts_b, shared[0], shared[1]);

    // Para que sean consistentes, la arista debe tener direcciones opuestas
    // Si tienen la misma dirección, una necesita invertirse
    let same_direction = order_a == order_b;

    // Si face_a está invertida, la comparación se invierte
    if face_a_flipped {
        !same_direction
    } else {
        same_direction
    }
}

/// Determina el orden de una arista en una cara (true = v0->v1, false = v1->v0)
fn edge_order_in_face(verts: &[usize; 3], v0: usize, v1: usize) -> bool {
    for i in 0..3 {
        if verts[i] == v0 && verts[(i + 1) % 3] == v1 {
            return true;
        }
    }
    false
}

/// Verifica si las normales apuntan hacia afuera usando el volumen signado
fn check_outward_facing(mesh: &Mesh) -> bool {
    // Calcular volumen signado
    // Si es positivo, las normales apuntan hacia afuera
    let mut volume = 0.0;
    for i in 0..mesh.num_faces() {
        let [p0, p1, p2] = mesh.get_face_positions(i);
        volume += p0.dot(&p1.cross(&p2));
    }
    volume > 0.0
}

/// Cuenta cuántas caras adyacentes tienen normales inconsistentes con una cara dada
pub fn count_inconsistent_neighbors(mesh: &Mesh, face_idx: usize) -> usize {
    let face_verts = mesh.get_face_vertices(face_idx);
    let neighbors = mesh.get_face_neighbors(face_idx);
    let mut inconsistent = 0;

    for neighbor_idx in neighbors {
        let neighbor_verts = mesh.get_face_vertices(neighbor_idx);

        // Encontrar arista compartida
        let mut shared = Vec::new();
        for &v in &face_verts {
            if neighbor_verts.contains(&v) {
                shared.push(v);
            }
        }

        if shared.len() == 2 {
            let order_face = edge_order_in_face(&face_verts, shared[0], shared[1]);
            let order_neighbor = edge_order_in_face(&neighbor_verts, shared[0], shared[1]);

            // Deberían tener órdenes opuestos para ser consistentes
            if order_face == order_neighbor {
                inconsistent += 1;
            }
        }
    }

    inconsistent
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinocchio_math::Vector3;

    fn make_consistent_tetrahedron() -> Mesh {
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.5, 1.0, 0.0),
            Vector3::new(0.5, 0.5, 1.0),
        ];
        // Orientación consistente (todas hacia afuera)
        let faces = vec![
            [0, 2, 1],  // Base mirando hacia abajo
            [0, 1, 3],
            [1, 2, 3],
            [2, 0, 3],
        ];
        Mesh::from_triangles(&vertices, &faces)
    }

    #[test]
    fn test_consistent_normals() {
        let mesh = make_consistent_tetrahedron();
        let analysis = analyze_normal_orientation(&mesh);

        assert!(analysis.is_consistent);
        assert_eq!(analysis.components, 1);
    }

    #[test]
    fn test_single_triangle() {
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.5, 1.0, 0.0),
        ];
        let faces = vec![[0, 1, 2]];
        let mesh = Mesh::from_triangles(&vertices, &faces);

        let analysis = analyze_normal_orientation(&mesh);
        assert!(analysis.is_consistent);
        assert_eq!(analysis.components, 1);
        assert!(analysis.faces_to_flip.is_empty());
    }

    #[test]
    fn test_empty_mesh() {
        let mesh = Mesh::new();
        let analysis = analyze_normal_orientation(&mesh);

        assert!(analysis.is_consistent);
        assert_eq!(analysis.components, 0);
    }
}
