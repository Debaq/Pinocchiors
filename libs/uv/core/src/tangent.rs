//! Normales y tangentes por esquina de una malla con UV.
//!
//! El horneado de normales y la exportación usan exactamente estos valores:
//! el normal map horneado solo es correcto si el visor reconstruye el mismo
//! marco tangente (glTF: `B = w · (N × T)`). Las tangentes son MikkTSpace,
//! las que recalculan Blender, Unity, Unreal y three.js cuando no leen las
//! del archivo.

use pinocchio_math::Vector3;

/// Normal y tangente (con signo en `w`) de cada esquina de cada cara.
#[derive(Debug, Clone, PartialEq)]
pub struct CornerFrames<const N: usize> {
    pub normals: Vec<[[f32; 3]; N]>,
    pub tangents: Vec<[[f32; 4]; N]>,
}

/// Normales suaves por vértice (promedio ponderado por área de los
/// triángulos en abanico que lo tocan) y tangentes MikkTSpace
/// ([`mikk_tangents`]).
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

    let frame_normals: Vec<[[f32; 3]; N]> = faces
        .iter()
        .map(|face| {
            std::array::from_fn(|k| {
                let n = normals[face[k]];
                [n.x() as f32, n.y() as f32, n.z() as f32]
            })
        })
        .collect();
    let frame_tangents = mikk_tangents(positions, faces, corners, &frame_normals);
    CornerFrames { normals: frame_normals, tangents: frame_tangents }
}

/// Tangentes MikkTSpace (el estándar de glTF, Blender, Unity y Unreal) para
/// las normales de `frames`, calculadas sobre los triángulos en abanico que
/// se exportan: así coinciden con las que recalcula quien abra el archivo.
pub fn mikk_tangents<const N: usize>(
    positions: &[[f64; 3]],
    faces: &[[usize; N]],
    corners: &[[[f32; 2]; N]],
    normals: &[[[f32; 3]; N]],
) -> Vec<[[f32; 4]; N]> {
    struct Fan<'a, const N: usize> {
        positions: &'a [[f64; 3]],
        faces: &'a [[usize; N]],
        corners: &'a [[[f32; 2]; N]],
        normals: &'a [[[f32; 3]; N]],
        /// (cara, esquina) de cada vértice de cada triángulo
        triangles: Vec<[(usize, usize); 3]>,
        out: Vec<[[f32; 4]; N]>,
    }
    impl<const N: usize> bevy_mikktspace::Geometry for Fan<'_, N> {
        fn num_faces(&self) -> usize {
            self.triangles.len()
        }
        fn num_vertices_of_face(&self, _: usize) -> usize {
            3
        }
        fn position(&self, t: usize, v: usize) -> [f32; 3] {
            let (f, k) = self.triangles[t][v];
            self.positions[self.faces[f][k]].map(|c| c as f32)
        }
        fn normal(&self, t: usize, v: usize) -> [f32; 3] {
            let (f, k) = self.triangles[t][v];
            self.normals[f][k]
        }
        fn tex_coord(&self, t: usize, v: usize) -> [f32; 2] {
            let (f, k) = self.triangles[t][v];
            self.corners[f][k]
        }
        fn set_tangent(&mut self, space: Option<bevy_mikktspace::TangentSpace>, t: usize, v: usize) {
            let (f, k) = self.triangles[t][v];
            if let Some(space) = space {
                // MikkTSpace toma la bitangente hacia +v; en glTF la v crece
                // hacia abajo de la imagen y B apunta a −v: se invierte el signo
                let [x, y, z, w] = space.tangent_encoded();
                self.out[f][k] = [x, y, z, -w];
            }
        }
    }

    let triangles = (0..faces.len())
        .flat_map(|f| (1..N.saturating_sub(1)).map(move |k| [(f, 0), (f, k), (f, k + 1)]))
        .collect();
    let mut fan = Fan { positions, faces, corners, normals, triangles, out: vec![[[1.0, 0.0, 0.0, 1.0]; N]; faces.len()] };
    // El error es un enum vacío: no puede fallar
    let _ = bevy_mikktspace::generate_tangents(&mut fan);
    fan.out
}
