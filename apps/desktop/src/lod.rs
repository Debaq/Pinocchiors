//! LOD al animar: una malla decimada con sus pesos de skinning, solo para el
//! visor mientras se reproduce o se edita una animación.
//!
//! Reutiliza la simplificación de "Remallar" (`quadriflow_core::remesh::simplify`,
//! meshoptimizer). Esa simplificación devuelve triángulos que usan **vértices
//! de la entrada**, sin moverlos ni crear otros: cada vértice del LOD es un
//! vértice de la malla completa, así que hereda su fila de pesos tal cual
//! (suman lo mismo, con los mismos índices de hueso) y no hace falta trasladar
//! pesos por punto más cercano como en `move_rig`, que además sería lento y
//! menos exacto.
//!
//! Nada de esto toca la escena, el rig ni el proyecto: exportar, guardar,
//! renderizar y capturar siguen usando la malla completa.

use crate::commands::{in_background, MeshData, WeightsData};
use crate::state::AppState;
use quadriflow_core::remesh::{self, SimplifyInput, SimplifyOptions};
use tauri::ipc::Response;
use tauri::AppHandle;

/// Desde cuántas caras el visor usa LOD al animar
pub const LOD_FROM_FACES: usize = 100_000;
/// Caras que se buscan en el LOD
pub const LOD_TARGET_FACES: usize = 50_000;

/// Peso de las normales y de las UV frente a la posición al simplificar (los
/// mismos valores que "Remallar")
const NORMAL_WEIGHT: f32 = 0.5;
const UV_WEIGHT: f32 = 1.0;

/// Malla decimada con los pesos de sus vértices
pub struct Lod {
    pub mesh: MeshData,
    pub weights: WeightsData,
    /// Vértice de la malla completa de cada vértice del LOD
    pub source_vertex: Vec<u32>,
    /// Vértices de la malla completa de la que sale (el visor comprueba que sea la suya)
    pub source_vertices: usize,
}

impl Lod {
    /// `u32` vértices de origen, `u32` bytes de la malla, la malla y los pesos
    /// (ambos en el formato de `get_mesh_data` y `get_weights_data`)
    pub fn to_bytes(&self) -> Vec<u8> {
        let mesh = self.mesh.to_bytes();
        let weights = self.weights.to_bytes();
        let mut out = Vec::with_capacity(8 + mesh.len() + weights.len());
        out.extend_from_slice(&(self.source_vertices as u32).to_le_bytes());
        out.extend_from_slice(&(mesh.len() as u32).to_le_bytes());
        out.extend_from_slice(&mesh);
        out.extend_from_slice(&weights);
        out
    }
}

/// Decima `mesh` a unas `target_faces` caras llevando los pesos. `None` si no
/// hay nada que reducir (ya es más liviana que el objetivo), si los pesos no
/// son de esta malla o si la simplificación no logra reducirla.
pub fn build_lod(mesh: &MeshData, weights: &WeightsData, target_faces: usize) -> Option<Lod> {
    let vertex_count = mesh.positions.len() / 3;
    let faces = mesh.indices.len() / 3;
    if vertex_count == 0 || faces <= target_faces || weights.num_vertices != vertex_count || weights.max_influences == 0 {
        return None;
    }
    if weights.weights.len() != vertex_count * weights.max_influences * 2 {
        return None;
    }

    let positions: Vec<[f32; 3]> = mesh.positions.chunks_exact(3).map(|p| [p[0], p[1], p[2]]).collect();
    let uvs = mesh.uvs.as_deref().filter(|u| u.len() == vertex_count * 2);
    let stride = if uvs.is_some() { 5 } else { 3 };
    let mut attributes = Vec::with_capacity(vertex_count * stride);
    for v in 0..vertex_count {
        attributes.extend_from_slice(&mesh.normals[v * 3..v * 3 + 3]);
        if let Some(uvs) = uvs {
            attributes.extend_from_slice(&uvs[v * 2..v * 2 + 2]);
        }
    }
    let attribute_weights: Vec<f32> = if uvs.is_some() {
        vec![NORMAL_WEIGHT, NORMAL_WEIGHT, NORMAL_WEIGHT, UV_WEIGHT, UV_WEIGHT]
    } else {
        vec![NORMAL_WEIGHT; 3]
    };

    // Grupo de cada vértice (el límite entre grupos queda como costura)
    let mut vertex_group = vec![0u32; vertex_count];
    for (g, group) in mesh.groups.iter().enumerate() {
        let (start, count) = (group[0] as usize, group[1] as usize);
        for &i in mesh.indices.get(start..(start + count).min(mesh.indices.len()))? {
            *vertex_group.get_mut(i as usize)? = g as u32;
        }
    }
    let groups: &[u32] = if mesh.groups.is_empty() { &[] } else { &vertex_group };

    let input = SimplifyInput { positions: &positions, indices: &mesh.indices, attributes: &attributes, attribute_weights: &attribute_weights, groups };
    let options = SimplifyOptions { target_triangles: target_faces, ..SimplifyOptions::default() };
    let result = remesh::simplify(&input, &options);
    let triangles = result.indices.len() / 3;
    if triangles == 0 || triangles >= faces {
        return None;
    }

    // Triángulos agrupados como los grupos de origen (un grupo = un material
    // y un nodo), en orden
    let group_count = mesh.groups.len().max(1);
    let mut by_group: Vec<Vec<u32>> = vec![Vec::new(); group_count];
    for tri in result.indices.chunks_exact(3) {
        let g = if mesh.groups.is_empty() { 0 } else { vertex_group[tri[0] as usize] as usize };
        by_group[g].extend_from_slice(tri);
    }

    // Solo los vértices que quedan, en el orden en que aparecen
    let mut new_index = vec![u32::MAX; vertex_count];
    let mut source_vertex: Vec<u32> = Vec::new();
    let mut indices: Vec<u32> = Vec::with_capacity(result.indices.len());
    let mut groups_out: Vec<[u32; 3]> = Vec::new();
    for (g, tris) in by_group.iter().enumerate() {
        let start = indices.len() as u32;
        for &old in tris {
            let slot = &mut new_index[old as usize];
            if *slot == u32::MAX {
                *slot = source_vertex.len() as u32;
                source_vertex.push(old);
            }
            indices.push(*slot);
        }
        if !mesh.groups.is_empty() {
            groups_out.push([start, indices.len() as u32 - start, mesh.groups[g][2]]);
        }
    }

    let pick3 = |data: &[f32]| source_vertex.iter().flat_map(|&v| data[v as usize * 3..v as usize * 3 + 3].iter().copied()).collect::<Vec<f32>>();
    let k = weights.max_influences;
    let weight_rows: Vec<f32> = source_vertex.iter().flat_map(|&v| weights.weights[v as usize * k * 2..(v as usize + 1) * k * 2].iter().copied()).collect();
    let out_mesh = MeshData {
        positions: pick3(&mesh.positions),
        normals: pick3(&mesh.normals),
        indices,
        uvs: uvs.map(|u| source_vertex.iter().flat_map(|&v| u[v as usize * 2..v as usize * 2 + 2].iter().copied()).collect()),
        groups: groups_out,
        group_nodes: mesh.group_nodes.clone(),
        quad_indices: Vec::new(),
        group_computed_normals: mesh.group_computed_normals.clone(),
        creases: if mesh.creases.len() == vertex_count { source_vertex.iter().map(|&v| mesh.creases[v as usize]).collect() } else { Vec::new() },
    };
    Some(Lod {
        weights: WeightsData {
            num_vertices: source_vertex.len(),
            num_bones: weights.num_bones,
            bone_names: weights.bone_names.clone(),
            weights: weight_rows,
            max_influences: k,
        },
        mesh: out_mesh,
        source_vertex,
        source_vertices: vertex_count,
    })
}

/// La malla del visor decimada con sus pesos, o vacío si no hace falta ni se
/// puede (menos de [`LOD_FROM_FACES`] caras, el rig sobre la malla de quads, sin
/// pesos…). Corre en un hilo aparte: no congela la interfaz.
#[tauri::command]
pub async fn animation_lod(app: AppHandle) -> Result<Response, String> {
    let bytes = in_background(app, |state| Ok(animation_lod_impl(state).map(|l| l.to_bytes()).unwrap_or_default())).await?;
    Ok(Response::new(bytes))
}

fn animation_lod_impl(state: &AppState) -> Option<Lod> {
    // Con el rig sobre la malla retopologizada el visor muestra esa malla (ya liviana)
    if state.rig_on_quad.load(std::sync::atomic::Ordering::SeqCst) {
        return None;
    }
    // Barato: sin pesos, o con poca malla, no se arma la malla del visor
    let faces = state.mesh.lock().unwrap().as_ref()?.num_faces();
    if faces < LOD_FROM_FACES {
        return None;
    }
    let weights = crate::commands::get_weights_data_impl(state).ok()?;
    let mesh = crate::commands::get_mesh_data_impl(state).ok()?;
    if mesh.indices.len() / 3 < LOD_FROM_FACES {
        return None;
    }
    build_lod(&mesh, &weights, LOD_TARGET_FACES)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Grilla `n × n` ondulada con dos materiales (mitades) y UV; pesos entre 3 huesos
    fn grid(n: usize) -> (MeshData, WeightsData) {
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut uvs = Vec::new();
        for y in 0..=n {
            for x in 0..=n {
                let (u, v) = (x as f32 / n as f32, y as f32 / n as f32);
                positions.extend([u, v, 0.1 * (u * 9.0).sin() * (v * 7.0).cos()]);
                normals.extend([0.0, 0.0, 1.0]);
                uvs.extend([u, v]);
            }
        }
        let row = n + 1;
        let mut left = Vec::new();
        let mut right = Vec::new();
        for y in 0..n {
            for x in 0..n {
                let a = (y * row + x) as u32;
                let tris = [a, a + 1, a + row as u32, a + 1, a + row as u32 + 1, a + row as u32];
                if x < n / 2 { left.extend(tris) } else { right.extend(tris) }
            }
        }
        let groups = vec![[0, left.len() as u32, 0], [left.len() as u32, right.len() as u32, 1]];
        let indices: Vec<u32> = left.into_iter().chain(right).collect();
        let count = (n + 1) * (n + 1);
        let mut weights = Vec::new();
        for i in 0..count {
            let x = (i % row) as f32 / n as f32;
            // Bordes del reparto: unos vértices con un solo hueso, otros mezclados
            let t = x * 2.0;
            let (b, f) = (t.floor().min(1.0), t.fract());
            weights.extend([b, 1.0 - f, b + 1.0, f, 0.0, 0.0, 0.0, 0.0]);
        }
        (
            MeshData {
                positions,
                normals,
                indices,
                uvs: Some(uvs),
                groups,
                group_nodes: vec![0, 1],
                quad_indices: Vec::new(),
                group_computed_normals: vec![0, 0],
                creases: Vec::new(),
            },
            WeightsData { num_vertices: count, num_bones: 3, bone_names: vec!["a".into(), "b".into(), "c".into()], weights, max_influences: 4 },
        )
    }

    #[test]
    fn llega_cerca_del_objetivo_y_los_pesos_son_validos() {
        let (mesh, weights) = grid(100); // 20 000 caras
        let lod = build_lod(&mesh, &weights, 2000).expect("hay LOD");
        let faces = lod.mesh.indices.len() / 3;
        assert!(faces < 20_000 / 3, "debe reducir mucho: {faces}");
        assert!(faces >= 1000, "sin colapsar de más: {faces}");

        let n = lod.mesh.positions.len() / 3;
        assert_eq!(lod.weights.num_vertices, n);
        assert_eq!(lod.weights.weights.len(), n * 4 * 2);
        assert_eq!(lod.mesh.normals.len(), n * 3);
        assert_eq!(lod.mesh.uvs.as_ref().unwrap().len(), n * 2);
        assert!(lod.mesh.indices.iter().all(|&i| (i as usize) < n));
        for (v, row) in lod.weights.weights.chunks_exact(8).enumerate() {
            let sum: f32 = row.chunks_exact(2).map(|p| p[1]).sum();
            assert!((sum - 1.0).abs() < 1e-4, "vértice {v}: suman {sum}");
            assert!(row.chunks_exact(2).all(|p| p[0] < 3.0 && p[1] >= 0.0), "índices de hueso intactos");
            // La fila es idéntica a la del vértice de origen, y la posición también
            let src = lod.source_vertex[v] as usize;
            assert_eq!(row, &weights.weights[src * 8..src * 8 + 8]);
            assert_eq!(&lod.mesh.positions[v * 3..v * 3 + 3], &mesh.positions[src * 3..src * 3 + 3]);
        }
    }

    #[test]
    fn conserva_los_grupos_de_material() {
        let (mesh, weights) = grid(100);
        let lod = build_lod(&mesh, &weights, 2000).unwrap();
        assert_eq!(lod.mesh.groups.len(), 2);
        assert_eq!(lod.mesh.groups.iter().map(|g| g[2]).collect::<Vec<_>>(), vec![0, 1]);
        // Los grupos cubren todos los índices, uno tras otro
        assert_eq!(lod.mesh.groups[0][0], 0);
        assert_eq!(lod.mesh.groups[0][1], lod.mesh.groups[1][0]);
        assert_eq!(lod.mesh.groups[1][0] + lod.mesh.groups[1][1], lod.mesh.indices.len() as u32);
        assert!(lod.mesh.groups.iter().all(|g| g[1] > 0));
        assert_eq!(lod.mesh.group_nodes, vec![0, 1]);
    }

    #[test]
    fn sin_lod_si_ya_es_liviana_o_los_pesos_no_son_de_la_malla() {
        let (mesh, weights) = grid(20); // 800 caras
        assert!(build_lod(&mesh, &weights, 2000).is_none());
        let (mesh, mut weights) = grid(100);
        weights.num_vertices -= 1;
        assert!(build_lod(&mesh, &weights, 2000).is_none());
    }

    #[test]
    fn el_binario_lleva_malla_y_pesos() {
        let (mesh, weights) = grid(100);
        let lod = build_lod(&mesh, &weights, 2000).unwrap();
        let bytes = lod.to_bytes();
        let source = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
        let mesh_len = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        assert_eq!(source, 101 * 101);
        assert_eq!(mesh_len % 4, 0);
        assert_eq!(bytes.len(), 8 + mesh_len + lod.weights.to_bytes().len());
    }
}
