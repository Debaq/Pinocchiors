//! La malla del modelo (o las piezas del corte) para la vista del conector: en
//! el formato de `cad_mesh` que lee `render::decode_cad_mesh`, ya en mm (Y
//! arriba como la escena), un color por pieza y las aristas vivas como líneas.

use std::collections::HashMap;

use pinocchio_mesh::Mesh;

use crate::AppState;

/// Arriba de esto no se buscan aristas vivas (la imagen igual se ve sombreada)
const EDGES_UP_TO: usize = 400_000;
/// Ángulo entre caras desde el que una arista se dibuja (grados)
const SHARP_DEG: f64 = 40.0;

/// `what`: "model" (la malla activa) o "pieces" (las piezas del corte, cada una una «cara» para el color)
pub fn view_bytes(state: &AppState, what: &str) -> Result<Vec<u8>, String> {
    let k = crate::commands::mm_per_unit(state);
    let meshes: Vec<Mesh> = match what {
        "pieces" => {
            let pieces = state.print3d_pieces.lock().unwrap();
            pieces.as_ref().ok_or("No hay piezas: primero divide el modelo")?.iter().map(|p| p.mesh.clone()).collect()
        }
        _ => vec![state.mesh.lock().unwrap().clone().ok_or("No hay malla cargada")?],
    };
    let (mut positions, mut normals, mut triangles, mut faces) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut segments: Vec<[[f32; 3]; 2]> = Vec::new();
    let total: usize = meshes.iter().map(Mesh::num_faces).sum();
    for (group, mesh) in meshes.iter().enumerate() {
        let base = positions.len() as u32;
        let p: Vec<[f64; 3]> = mesh.vertices.iter().map(|v| [v.position.x() * k, v.position.y() * k, v.position.z() * k]).collect();
        let tris: Vec<[usize; 3]> = (0..mesh.num_faces()).map(|f| mesh.get_face_vertices(f)).collect();
        let mut n = vec![[0.0f64; 3]; p.len()];
        let mut face_normals = Vec::with_capacity(tris.len());
        for t in &tris {
            let fnorm = cross(sub(p[t[1]], p[t[0]]), sub(p[t[2]], p[t[0]]));
            for &v in t {
                for i in 0..3 {
                    n[v][i] += fnorm[i];
                }
            }
            face_normals.push(unit(fnorm));
        }
        positions.extend(p.iter().map(|q| q.map(|x| x as f32)));
        normals.extend(n.into_iter().map(|q| unit(q).map(|x| x as f32)));
        triangles.extend(tris.iter().map(|t| t.map(|v| v as u32 + base)));
        faces.extend(std::iter::repeat_n(group as u32, tris.len()));
        if total <= EDGES_UP_TO {
            segments.extend(sharp_edges(&p, &tris, &face_normals));
        }
    }
    let mut out = Vec::new();
    let mut put = |w: u32| out.extend_from_slice(&w.to_le_bytes());
    put(positions.len() as u32);
    put(triangles.len() as u32);
    put(segments.len() as u32);
    put(segments.len() as u32 * 2);
    let floats = |v: &[[f32; 3]], out: &mut Vec<u8>| v.iter().flatten().for_each(|x| out.extend_from_slice(&x.to_le_bytes()));
    floats(&positions, &mut out);
    floats(&normals, &mut out);
    for w in triangles.iter().flatten().chain(&faces) {
        out.extend_from_slice(&w.to_le_bytes());
    }
    for i in 0..segments.len() {
        out.extend_from_slice(&(2 * i as u32 + 2).to_le_bytes());
    }
    let points: Vec<[f32; 3]> = segments.iter().flatten().copied().collect();
    floats(&points, &mut out);
    Ok(out)
}

/// Aristas entre caras que doblan más de [`SHARP_DEG`] y bordes abiertos
fn sharp_edges(p: &[[f64; 3]], tris: &[[usize; 3]], normals: &[[f64; 3]]) -> Vec<[[f32; 3]; 2]> {
    let mut sides: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
    for (f, t) in tris.iter().enumerate() {
        for i in 0..3 {
            let (a, b) = (t[i], t[(i + 1) % 3]);
            sides.entry((a.min(b), a.max(b))).or_default().push(f);
        }
    }
    let cos = SHARP_DEG.to_radians().cos();
    let point = |v: usize| p[v].map(|x| x as f32);
    sides
        .into_iter()
        .filter(|(_, fs)| match fs.as_slice() {
            [a, b] => dot(normals[*a], normals[*b]) < cos,
            _ => true,
        })
        .map(|((a, b), _)| [point(a), point(b)])
        .collect()
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn unit(a: [f64; 3]) -> [f64; 3] {
    let l = dot(a, a).sqrt();
    if l > 0.0 { a.map(|x| x / l) } else { [0.0, 1.0, 0.0] }
}
