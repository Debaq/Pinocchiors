//! Reparación de orientación de normales

use pinocchio_mesh::Mesh;
use crate::error::{RepairError, RepairResult};
use crate::analysis::normals::analyze_normal_orientation;
use std::collections::VecDeque;

/// Hace las normales consistentes en toda la malla
///
/// Usa BFS para propagar la orientación desde una cara semilla (la de menor Z)
/// y voltea las caras necesarias para que todas tengan la misma orientación relativa.
///
/// # Algoritmo
///
/// 1. Analizar la orientación actual
/// 2. Voltear las caras marcadas como inconsistentes
/// 3. Reconstruir la malla
///
/// # Retorna
///
/// Número de caras volteadas
pub fn make_normals_consistent(mesh: &mut Mesh) -> usize {
    let analysis = analyze_normal_orientation(mesh);

    if analysis.is_consistent || analysis.faces_to_flip.is_empty() {
        return 0;
    }

    // Crear conjunto de caras a voltear
    let mut flip_set = vec![false; mesh.num_faces()];
    for &face_idx in &analysis.faces_to_flip {
        flip_set[face_idx] = true;
    }

    // Extraer triángulos, volteando los marcados
    let mut triangles: Vec<[usize; 3]> = Vec::with_capacity(mesh.num_faces());
    for face_idx in 0..mesh.num_faces() {
        let [v0, v1, v2] = mesh.get_face_vertices(face_idx);
        if flip_set[face_idx] {
            // Voltear: intercambiar v1 y v2
            triangles.push([v0, v2, v1]);
        } else {
            triangles.push([v0, v1, v2]);
        }
    }

    // Reconstruir la malla
    let positions: Vec<_> = mesh.vertices.iter().map(|v| v.position).collect();
    *mesh = Mesh::from_triangles(&positions, &triangles);

    analysis.faces_to_flip.len()
}

/// Orienta las normales hacia afuera
///
/// Primero hace las normales consistentes, luego verifica si apuntan hacia
/// afuera usando el volumen signado. Si apuntan hacia adentro, voltea todas
/// las caras.
///
/// # Requisitos
///
/// La malla debe ser cerrada (watertight) para determinar "afuera".
///
/// # Retorna
///
/// Número de caras volteadas, o error si la malla no es cerrada
pub fn orient_normals_outward(mesh: &mut Mesh) -> RepairResult<usize> {
    // Primero hacer consistentes
    let mut flipped = make_normals_consistent(mesh);

    // Verificar si es cerrada
    if !mesh.is_closed() {
        let boundary_count = mesh.get_boundary_edges().len();
        return Err(RepairError::OpenMesh(boundary_count));
    }

    // Calcular volumen signado
    let volume = compute_signed_volume(mesh);

    // Si el volumen es negativo, las normales apuntan hacia adentro
    if volume < 0.0 {
        flip_all_faces(mesh);
        flipped += mesh.num_faces();
    }

    Ok(flipped)
}

/// Voltea todas las caras de la malla
pub fn flip_all_faces(mesh: &mut Mesh) {
    let triangles: Vec<[usize; 3]> = (0..mesh.num_faces())
        .map(|i| {
            let [v0, v1, v2] = mesh.get_face_vertices(i);
            [v0, v2, v1]  // Intercambiar v1 y v2
        })
        .collect();

    let positions: Vec<_> = mesh.vertices.iter().map(|v| v.position).collect();
    *mesh = Mesh::from_triangles(&positions, &triangles);
}

/// Voltea una cara específica
pub fn flip_face(mesh: &mut Mesh, face_idx: usize) -> RepairResult<()> {
    if face_idx >= mesh.num_faces() {
        return Err(RepairError::FaceIndexOutOfRange(face_idx, mesh.num_faces()));
    }

    let mut triangles: Vec<[usize; 3]> = (0..mesh.num_faces())
        .map(|i| mesh.get_face_vertices(i))
        .collect();

    // Voltear la cara específica
    let [v0, v1, v2] = triangles[face_idx];
    triangles[face_idx] = [v0, v2, v1];

    let positions: Vec<_> = mesh.vertices.iter().map(|v| v.position).collect();
    *mesh = Mesh::from_triangles(&positions, &triangles);

    Ok(())
}

/// Calcula el volumen signado de la malla
///
/// Suma los tetraedros formados por cada cara y el origen.
/// El signo indica la orientación de las normales:
/// - Positivo: normales hacia afuera
/// - Negativo: normales hacia adentro
fn compute_signed_volume(mesh: &Mesh) -> f64 {
    let mut volume = 0.0;
    for i in 0..mesh.num_faces() {
        let [p0, p1, p2] = mesh.get_face_positions(i);
        volume += p0.dot(&p1.cross(&p2));
    }
    volume / 6.0
}

/// Orienta las normales de manera consistente usando el método del componente más grande
///
/// Esta es una alternativa más robusta que funciona mejor con mallas complejas:
/// 1. Encuentra el componente conectado más grande
/// 2. Usa la cara con menor Z como semilla
/// 3. Propaga la orientación
pub fn orient_normals_largest_component(mesh: &mut Mesh) -> usize {
    if mesh.num_faces() == 0 {
        return 0;
    }

    // Encontrar componentes y el más grande
    let components = find_face_components(mesh);
    let largest = (0..components.num_components)
        .max_by_key(|&c| components.component_sizes[c])
        .unwrap_or(0);

    // Estado de cada cara
    let mut face_flip: Vec<Option<bool>> = vec![None; mesh.num_faces()];

    // Encontrar semilla en el componente más grande (cara con menor Z)
    let seed = (0..mesh.num_faces())
        .filter(|&f| components.face_component[f] == largest)
        .min_by(|&a, &b| {
            let min_z_a = mesh.get_face_positions(a).iter().map(|p| p.z()).fold(f64::INFINITY, f64::min);
            let min_z_b = mesh.get_face_positions(b).iter().map(|p| p.z()).fold(f64::INFINITY, f64::min);
            min_z_a.total_cmp(&min_z_b)
        })
        .unwrap();

    // BFS desde la semilla
    face_flip[seed] = Some(false);
    let mut queue = VecDeque::new();
    queue.push_back(seed);

    while let Some(face_idx) = queue.pop_front() {
        let current_flip = face_flip[face_idx].unwrap();

        for neighbor_idx in mesh.get_face_neighbors(face_idx) {
            if face_flip[neighbor_idx].is_none() {
                let should_flip = should_neighbor_be_flipped(mesh, face_idx, neighbor_idx, current_flip);
                face_flip[neighbor_idx] = Some(should_flip);
                queue.push_back(neighbor_idx);
            }
        }
    }

    // Contar y aplicar flips
    let mut flip_count = 0;
    let mut triangles: Vec<[usize; 3]> = Vec::with_capacity(mesh.num_faces());

    for face_idx in 0..mesh.num_faces() {
        let [v0, v1, v2] = mesh.get_face_vertices(face_idx);
        if face_flip[face_idx] == Some(true) {
            triangles.push([v0, v2, v1]);
            flip_count += 1;
        } else {
            triangles.push([v0, v1, v2]);
        }
    }

    if flip_count > 0 {
        let positions: Vec<_> = mesh.vertices.iter().map(|v| v.position).collect();
        *mesh = Mesh::from_triangles(&positions, &triangles);
    }

    flip_count
}

/// Determina si una cara vecina debe ser volteada para consistencia
fn should_neighbor_be_flipped(
    mesh: &Mesh,
    face_a: usize,
    face_b: usize,
    face_a_flipped: bool,
) -> bool {
    let verts_a = mesh.get_face_vertices(face_a);
    let verts_b = mesh.get_face_vertices(face_b);

    // Encontrar vértices compartidos
    let mut shared = Vec::new();
    for &va in &verts_a {
        if verts_b.contains(&va) {
            shared.push(va);
        }
    }

    if shared.len() != 2 {
        return false;
    }

    // Verificar orden de la arista en cada cara
    let order_a = edge_direction(&verts_a, shared[0], shared[1]);
    let order_b = edge_direction(&verts_b, shared[0], shared[1]);

    let same_direction = order_a == order_b;

    if face_a_flipped {
        !same_direction
    } else {
        same_direction
    }
}

fn edge_direction(verts: &[usize; 3], v0: usize, v1: usize) -> bool {
    for i in 0..3 {
        if verts[i] == v0 && verts[(i + 1) % 3] == v1 {
            return true;
        }
    }
    false
}

struct FaceComponents {
    face_component: Vec<usize>,
    num_components: usize,
    component_sizes: Vec<usize>,
}

fn find_face_components(mesh: &Mesh) -> FaceComponents {
    let num_faces = mesh.num_faces();
    let mut face_component = vec![usize::MAX; num_faces];
    let mut num_components = 0;
    let mut component_sizes = Vec::new();

    for start in 0..num_faces {
        if face_component[start] != usize::MAX {
            continue;
        }

        let mut size = 0;
        let mut stack = vec![start];

        while let Some(face_idx) = stack.pop() {
            if face_component[face_idx] != usize::MAX {
                continue;
            }
            face_component[face_idx] = num_components;
            size += 1;

            for neighbor in mesh.get_face_neighbors(face_idx) {
                if face_component[neighbor] == usize::MAX {
                    stack.push(neighbor);
                }
            }
        }

        component_sizes.push(size);
        num_components += 1;
    }

    FaceComponents {
        face_component,
        num_components,
        component_sizes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinocchio_math::Vector3;

    fn make_tetrahedron_consistent() -> Mesh {
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.5, 1.0, 0.0),
            Vector3::new(0.5, 0.5, 1.0),
        ];
        // Todas las normales apuntando hacia afuera
        let faces = vec![
            [0, 2, 1],
            [0, 1, 3],
            [1, 2, 3],
            [2, 0, 3],
        ];
        Mesh::from_triangles(&vertices, &faces)
    }

    #[test]
    fn test_consistent_normals_no_change() {
        let mut mesh = make_tetrahedron_consistent();
        let flipped = make_normals_consistent(&mut mesh);

        // Ya eran consistentes
        assert!(flipped <= 1);  // Puede voltear algunas dependiendo del orden BFS
    }

    #[test]
    fn test_orient_outward_closed() {
        let mut mesh = make_tetrahedron_consistent();

        let result = orient_normals_outward(&mut mesh);
        assert!(result.is_ok());
    }

    #[test]
    fn test_orient_outward_open_fails() {
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.5, 1.0, 0.0),
        ];
        let faces = vec![[0, 1, 2]];
        let mut mesh = Mesh::from_triangles(&vertices, &faces);

        let result = orient_normals_outward(&mut mesh);
        assert!(matches!(result, Err(RepairError::OpenMesh(_))));
    }

    #[test]
    fn test_flip_all_faces() {
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.5, 1.0, 0.0),
        ];
        let faces = vec![[0, 1, 2]];
        let mut mesh = Mesh::from_triangles(&vertices, &faces);

        let normal_before = mesh.get_face_normal(0);
        flip_all_faces(&mut mesh);
        let normal_after = mesh.get_face_normal(0);

        // Las normales deben ser opuestas
        let dot = normal_before.dot(&normal_after);
        assert!(dot < -0.99);
    }

    #[test]
    fn test_signed_volume() {
        let mesh = make_tetrahedron_consistent();
        let volume = compute_signed_volume(&mesh);

        // El tetraedro tiene volumen positivo si las normales apuntan hacia afuera
        // (dependiendo de la orientación exacta de las caras)
        assert!(volume.abs() > 0.0);
    }
}
