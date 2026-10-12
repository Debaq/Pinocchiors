//! Retopología por objeto: cada grupo de caras (un material, una pieza) se
//! retopologiza aparte y los quads se juntan en una sola malla.
//!
//! Remallar todo junto usa un solo tamaño de quad para la escena: las piezas
//! chicas quedan sin quads o se funden con las vecinas. Aquí cada objeto recibe
//! una parte de las caras objetivo según su área, con un piso para que
//! ninguno quede vacío.

use crate::{remesh_with_callback, QuadFace, QuadMesh, RemeshConfig, RemeshError, RemeshStage, Symmetry};
use pinocchio_math::Vector3;
use pinocchio_mesh::Mesh;
use std::collections::HashMap;

/// Quads mínimos de un objeto.
pub const MIN_OBJECT_QUADS: usize = 24;

/// Qué pasó con un objeto.
#[derive(Debug, Clone)]
pub struct ObjectReport {
    /// Grupo de caras de la entrada.
    pub group: usize,
    pub faces_in: usize,
    /// Quads pedidos.
    pub target: usize,
    pub quads_out: usize,
    /// Por qué no se retopologizó (el objeto no está en la salida).
    pub error: Option<String>,
}

/// Resultado de [`remesh_groups`].
#[derive(Debug, Clone)]
pub struct ObjectsRemesh {
    pub mesh: QuadMesh,
    /// Grupo de cada quad de `mesh`.
    pub face_group: Vec<usize>,
    pub reports: Vec<ObjectReport>,
}

/// Reparto de `total` quads entre objetos según su área, con piso
/// [`MIN_OBJECT_QUADS`].
pub fn split_budget(total: usize, areas: &[f64]) -> Vec<usize> {
    let sum: f64 = areas.iter().sum();
    areas
        .iter()
        .map(|&a| {
            let share = if sum > 0.0 { a / sum } else { 1.0 / areas.len().max(1) as f64 };
            ((total as f64 * share).round() as usize).max(MIN_OBJECT_QUADS)
        })
        .collect()
}

/// Retopologiza cada grupo de `face_group` (un valor por triángulo de `mesh`).
/// La simetría se ignora: su plano sería el de la caja de cada objeto.
pub fn remesh_groups<F>(
    mesh: &Mesh,
    face_group: &[usize],
    config: &RemeshConfig,
    mut on_progress: F,
) -> Result<ObjectsRemesh, RemeshError>
where
    F: FnMut(RemeshStage, &str),
{
    if mesh.num_faces() == 0 || face_group.len() != mesh.num_faces() {
        return Err(RemeshError::EmptyMesh);
    }
    let mut groups: Vec<usize> = face_group.to_vec();
    groups.sort_unstable();
    groups.dedup();

    // Caras y área de cada grupo
    let mut faces_of: HashMap<usize, Vec<usize>> = HashMap::new();
    for (f, &g) in face_group.iter().enumerate() {
        faces_of.entry(g).or_default().push(f);
    }
    let areas: Vec<f64> = groups.iter().map(|g| faces_of[g].iter().map(|&f| mesh.get_face_area(f)).sum()).collect();
    let targets = split_budget(config.target_faces, &areas);

    let mut out = QuadMesh::default();
    let mut quad_group = Vec::new();
    let mut reports = Vec::new();
    let total = groups.len();
    for (i, &group) in groups.iter().enumerate() {
        let faces = &faces_of[&group];
        // Submalla con los vértices que usa el grupo
        let mut remap: HashMap<usize, usize> = HashMap::new();
        let mut positions: Vec<Vector3> = Vec::new();
        let triangles: Vec<[usize; 3]> = faces
            .iter()
            .map(|&f| {
                mesh.get_face_vertices(f).map(|v| {
                    *remap.entry(v).or_insert_with(|| {
                        positions.push(mesh.vertices[v].position);
                        positions.len() - 1
                    })
                })
            })
            .collect();
        let sub = Mesh::try_from_triangles(&positions, &triangles).map_err(RemeshError::InvalidConfig)?;

        let sub_config = RemeshConfig { target_faces: targets[i], symmetry: Symmetry::None, ..config.clone() };
        let label = format!("Objeto {} de {total}: ", i + 1);
        let mut result = remesh_with_callback(&sub, &sub_config, |stage, message| {
            on_progress(stage, &format!("{label}{message}"));
        });
        // Un objeto muy chico a veces no da ningún quad: se reintenta más fino
        if matches!(result, Err(RemeshError::ExtractionFailed)) {
            let finer = RemeshConfig { target_faces: targets[i] * 4, ..sub_config.clone() };
            result = remesh_with_callback(&sub, &finer, |_, _| {});
        }
        let mut report = ObjectReport { group, faces_in: faces.len(), target: targets[i], quads_out: 0, error: None };
        match result {
            Ok(quads) => {
                let offset = out.vertices.len();
                out.vertices.extend(quads.vertices);
                out.faces.extend(quads.faces.iter().map(|q| QuadFace { v: q.v.map(|v| v + offset) }));
                quad_group.extend(std::iter::repeat_n(group, quads.faces.len()));
                report.quads_out = quads.faces.len();
            }
            Err(e) => report.error = Some(e.to_string()),
        }
        reports.push(report);
    }
    if out.faces.is_empty() {
        return Err(RemeshError::ExtractionFailed);
    }
    on_progress(RemeshStage::Done, &format!("{} vértices, {} quads", out.num_vertices(), out.num_faces()));
    Ok(ObjectsRemesh { mesh: out, face_group: quad_group, reports })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_follows_area_with_a_floor() {
        let b = split_budget(1000, &[90.0, 9.9, 0.1]);
        assert_eq!(b[0], 900);
        assert_eq!(b[1], 99);
        assert_eq!(b[2], MIN_OBJECT_QUADS, "el piso evita objetos vacíos");
    }
}
