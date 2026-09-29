use std::f64::consts::TAU;
use pinocchio_math::Vector3;
use uv_core::{transfer_uvs, UvPart, UvSurface};

/// Posiciones, UV y triángulos.
type Parts = (Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<[u32; 3]>);

/// Rejilla `n × n` en el plano z = 0 sobre [0, size]², con UV = (x, y) / size.
fn grid(n: usize, size: f32) -> Parts {
    let mut pos = Vec::new();
    let mut uv = Vec::new();
    for j in 0..=n {
        for i in 0..=n {
            let (x, y) = (i as f32 / n as f32, j as f32 / n as f32);
            pos.push([x * size, y * size, 0.0]);
            uv.push([x, y]);
        }
    }
    let idx = |i: usize, j: usize| (j * (n + 1) + i) as u32;
    let mut tris = Vec::new();
    for j in 0..n {
        for i in 0..n {
            tris.push([idx(i, j), idx(i + 1, j), idx(i + 1, j + 1)]);
            tris.push([idx(i, j), idx(i + 1, j + 1), idx(i, j + 1)]);
        }
    }
    (pos, uv, tris)
}

/// Cilindro abierto de radio 1 y alto 1 con `n` segmentos, UV u = ángulo / 2π
/// y costura en ángulo 0 (la columna final repite posiciones con u = 1).
fn cylinder(n: usize, rings: usize) -> Parts {
    let mut pos = Vec::new();
    let mut uv = Vec::new();
    for j in 0..=rings {
        for i in 0..=n {
            let a = TAU * i as f64 / n as f64;
            let z = j as f64 / rings as f64;
            pos.push([a.cos() as f32, a.sin() as f32, z as f32]);
            uv.push([i as f32 / n as f32, z as f32]);
        }
    }
    let idx = |i: usize, j: usize| (j * (n + 1) + i) as u32;
    let mut tris = Vec::new();
    for j in 0..rings {
        for i in 0..n {
            tris.push([idx(i, j), idx(i + 1, j), idx(i + 1, j + 1)]);
            tris.push([idx(i, j), idx(i + 1, j + 1), idx(i, j + 1)]);
        }
    }
    (pos, uv, tris)
}

#[test]
fn plane_uvs_follow_position() {
    let (pos, uv, tris) = grid(20, 2.0);
    let surface = UvSurface::new([UvPart { group: 7, positions: &pos, uvs: &uv, normals: None, triangles: &tris }]).unwrap();
    assert_eq!(surface.num_charts(), 1);

    // Quads más gruesos y desalineados respecto a la rejilla, algo sobre el plano
    let targets: Vec<[f64; 3]> = (0..4)
        .flat_map(|j| (0..4).map(move |i| [0.1 + 0.6 * i as f64, 0.13 + 0.6 * j as f64, 0.01]))
        .collect();
    let faces: Vec<[usize; 4]> = (0..3)
        .flat_map(|j| (0..3).map(move |i| [j * 4 + i, j * 4 + i + 1, (j + 1) * 4 + i + 1, (j + 1) * 4 + i]))
        .collect();

    let result = transfer_uvs(&surface, &targets, &faces);
    assert_eq!(result.seam_faces, 0);
    assert!(result.groups.iter().all(|&g| g == 7));
    for (face, uvs) in faces.iter().zip(&result.corners) {
        for (&v, uv) in face.iter().zip(uvs) {
            let expected = [targets[v][0] / 2.0, targets[v][1] / 2.0];
            assert!((uv[0] as f64 - expected[0]).abs() < 1e-5 && (uv[1] as f64 - expected[1]).abs() < 1e-5);
        }
    }
}

#[test]
fn faces_across_a_seam_stay_in_one_chart() {
    let (pos, uv, tris) = cylinder(64, 4);
    let surface = UvSurface::new([UvPart { group: 0, positions: &pos, uvs: &uv, normals: None, triangles: &tris }]).unwrap();
    assert_eq!(surface.num_charts(), 1, "la costura separa bordes de la misma isla, no islas");

    // Quads de 12 segmentos girados medio segmento: dos caras cruzan la costura
    let n = 12;
    let offset = 0.5 * TAU / n as f64;
    let targets: Vec<[f64; 3]> = (0..=2)
        .flat_map(|j| {
            (0..n).map(move |i| {
                let a = offset + TAU * i as f64 / n as f64 - TAU / n as f64;
                [a.cos(), a.sin(), 0.1 + 0.4 * j as f64]
            })
        })
        .collect();
    let faces: Vec<[usize; 4]> = (0..2)
        .flat_map(|j| (0..n).map(move |i| [j * n + i, j * n + (i + 1) % n, (j + 1) * n + (i + 1) % n, (j + 1) * n + i]))
        .collect();

    let result = transfer_uvs(&surface, &targets, &faces);
    assert_eq!(result.seam_faces, 2);
    for uvs in &result.corners {
        let us: Vec<f32> = uvs.iter().map(|uv| uv[0]).collect();
        let span = us.iter().cloned().fold(f32::MIN, f32::max) - us.iter().cloned().fold(f32::MAX, f32::min);
        // Sin el manejo de costuras la cara abarcaría casi todo el mapa (≈ 0.92)
        assert!(span < 0.1, "la cara salta de isla: u = {us:?}");
    }
    // Lejos de la costura, u sigue al ángulo
    let a = offset + TAU * 5.0 / n as f64 - TAU / n as f64;
    let u = result.corners[5][0][0] as f64;
    assert!((u - a / TAU).abs() < 0.01, "u = {u}, esperado {}", a / TAU);
}

#[test]
fn faces_take_the_group_of_their_part() {
    let (pos_a, uv_a, tris_a) = grid(4, 1.0);
    let pos_b: Vec<[f32; 3]> = pos_a.iter().map(|p| [p[0] + 1.0, p[1], p[2]]).collect();
    let surface = UvSurface::new([
        UvPart { group: 0, positions: &pos_a, uvs: &uv_a, normals: None, triangles: &tris_a },
        UvPart { group: 1, positions: &pos_b, uvs: &uv_a, normals: None, triangles: &tris_a },
    ])
    .unwrap();
    assert_eq!(surface.num_charts(), 2);

    let targets = [[0.2, 0.2, 0.0], [0.4, 0.2, 0.0], [0.4, 0.4, 0.0], [1.6, 0.2, 0.0], [1.8, 0.2, 0.0], [1.8, 0.4, 0.0]];
    let result = transfer_uvs(&surface, &targets, &[[0, 1, 2], [3, 4, 5]]);
    assert_eq!(result.groups, vec![0, 1]);
    assert!((result.corners[1][0][0] - 0.6).abs() < 1e-5);
}

#[test]
fn surface_without_uvs_is_none() {
    let (pos, _, tris) = grid(2, 1.0);
    assert!(UvSurface::new([UvPart { group: 0, positions: &pos, uvs: &[], normals: None, triangles: &tris }]).is_none());
}

/// Escena con dos piezas lado a lado: la izquierda con UV y textura, la
/// derecha de color liso (sin UV)
fn mixed_scene() -> converter_scene::Scene {
    use converter_scene::{IndexData, Material, Mesh, Node, Primitive, Scene, Transform, VertexAttribute};
    let (pos, uv, tris) = grid(4, 1.0);
    let right: Vec<[f32; 3]> = pos.iter().map(|p| [p[0] + 1.0, p[1], p[2]]).collect();
    let indices: Vec<u32> = tris.iter().flatten().copied().collect();
    let mut scene = Scene::new();
    scene.materials.push(Material { name: "con textura".into(), ..Material::default() });
    scene.materials.push(Material { name: "liso".into(), ..Material::default() });
    scene.meshes.push(Mesh {
        name: "piezas".into(),
        primitives: vec![
            Primitive {
                attributes: vec![VertexAttribute::Positions(pos), VertexAttribute::TexCoords(0, uv)],
                indices: Some(IndexData::U32(indices.clone())),
                material: Some(0),
            },
            Primitive {
                attributes: vec![VertexAttribute::Positions(right)],
                indices: Some(IndexData::U32(indices)),
                material: Some(1),
            },
        ],
    });
    scene.nodes.push(Node { name: "piezas".into(), transform: Transform::identity(), mesh: Some(0), skin: None, children: vec![] });
    scene.root_nodes.push(0);
    scene
}

#[test]
fn pieces_without_uvs_keep_their_material() {
    let scene = mixed_scene();
    let surface = uv_core::scene_surface(&scene).unwrap();
    let targets = [[0.2, 0.2, 0.0], [0.4, 0.2, 0.0], [0.4, 0.4, 0.0], [1.6, 0.2, 0.0], [1.8, 0.2, 0.0], [1.8, 0.4, 0.0]];
    let skin = uv_core::transferred_skin(&scene, &surface, &targets, &[[0, 1, 2], [3, 4, 5]]);
    // Antes la pieza lisa quedaba fuera y su cara tomaba la textura vecina
    assert_eq!(skin.face_material, vec![Some(0), Some(1)]);
    assert!((skin.corners[0][1][0] - 0.4).abs() < 1e-5);
}

#[test]
fn material_surface_works_without_any_uv() {
    let mut scene = mixed_scene();
    scene.meshes[0].primitives[0].attributes.retain(|a| !matches!(a, converter_scene::VertexAttribute::TexCoords(..)));
    assert!(uv_core::scene_surface(&scene).is_none());
    let surface = uv_core::material_surface(&scene).unwrap();
    let skin = uv_core::transferred_skin(&scene, &surface, &[[0.5, 0.5, 0.0], [1.5, 0.5, 0.0], [1.5, 0.7, 0.0]], &[[0, 1, 2]]);
    assert_eq!(skin.face_material, vec![Some(1)]);
}

#[test]
fn projection_follows_the_normal_not_the_nearest_piece() {
    // Piso z = 0 (mira a +z) y una pared x = 0,06 que mira a −x, más cerca
    // del punto que el piso
    let (floor_pos, floor_uv, floor_tris) = grid(4, 2.0);
    let floor_pos: Vec<[f32; 3]> = floor_pos.iter().map(|p| [p[0] - 1.0, p[1] - 1.0, 0.0]).collect();
    let wall_pos = vec![[0.06, -1.0, 0.0], [0.06, 1.0, 1.0], [0.06, 1.0, 0.0], [0.06, -1.0, 1.0]];
    let wall_uv = vec![[0.0, 0.0], [1.0, 1.0], [1.0, 0.0], [0.0, 1.0]];
    let wall_tris = vec![[0, 1, 2], [0, 3, 1]];
    let surface = UvSurface::new([
        UvPart { group: 0, positions: &floor_pos, uvs: &floor_uv, normals: None, triangles: &floor_tris },
        UvPart { group: 1, positions: &wall_pos, uvs: &wall_uv, normals: None, triangles: &wall_tris },
    ])
    .unwrap();

    let p = Vector3::new(0.0, 0.0, 0.1);
    let up = Vector3::new(0.0, 0.0, 1.0);
    assert_eq!(surface.group(surface.closest(&p).triangle), 1, "la pared es lo más cercano");
    let (tri, point) = surface.project(&p, &up, 0.5).expect("el piso está a 0,1 por la normal");
    assert_eq!(surface.group(tri), 0);
    assert!(point.distance(&Vector3::new(0.0, 0.0, 0.0)) < 1e-9);
    // Fuera de alcance, o si nada mira hacia el mismo lado, no hay proyección
    assert!(surface.project(&p, &up, 0.05).is_none());
    assert!(surface.project(&p, &(up * -1.0), 0.5).is_none());
}
