use uv_core::{bake, corner_frames, unwrap, BakeChannel, TexelContext, UnwrapOptions, UvPart, UvSurface};

/// Plano z = 0 sobre [0, 2]² triangulado fino, con UV = posición / 2.
fn source() -> (Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<[u32; 3]>) {
    let n = 40;
    let mut pos = Vec::new();
    let mut uv = Vec::new();
    for j in 0..=n {
        for i in 0..=n {
            let (u, v) = (i as f32 / n as f32, j as f32 / n as f32);
            pos.push([2.0 * u, 2.0 * v, 0.0]);
            uv.push([u, v]);
        }
    }
    let idx = |i: usize, j: usize| (j * (n + 1) + i) as u32;
    let tris = (0..n)
        .flat_map(|j| (0..n).map(move |i| (i, j)))
        .flat_map(|(i, j)| [[idx(i, j), idx(i + 1, j), idx(i + 1, j + 1)], [idx(i, j), idx(i + 1, j + 1), idx(i, j + 1)]])
        .collect();
    (pos, uv, tris)
}

#[test]
fn baked_texels_match_the_source_texture() {
    let (pos, uv, tris) = source();
    let surface = UvSurface::new([UvPart { group: 3, positions: &pos, uvs: &uv, normals: None, colors: None, triangles: &tris }]).unwrap();

    // Malla destino: 5 × 5 quads sobre el mismo plano
    let n = 5;
    let positions: Vec<[f64; 3]> =
        (0..=n).flat_map(|j| (0..=n).map(move |i| [2.0 * i as f64 / n as f64, 2.0 * j as f64 / n as f64, 0.0])).collect();
    let idx = |i: usize, j: usize| j * (n + 1) + i;
    let faces: Vec<[usize; 4]> =
        (0..n).flat_map(|j| (0..n).map(move |i| [idx(i, j), idx(i + 1, j), idx(i + 1, j + 1), idx(i, j + 1)])).collect();
    let layout = unwrap(&positions, &faces, &UnwrapOptions::default());
    let frames = corner_frames(&positions, &faces, &layout.corners);

    // Color = UV original; normal en espacio tangente
    let color = |ctx: &TexelContext| [(ctx.uv[0] * 255.0).round() as u8, (ctx.uv[1] * 255.0).round() as u8, ctx.group as u8, 255];
    let normal = |ctx: &TexelContext| {
        let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let n = ctx.source_frame[2];
        let t = ctx.target_frame.map(|axis| ((0.5 * dot(n, axis) + 0.5) * 255.0).round() as u8);
        [t[0], t[1], t[2], 255]
    };
    let channels: [BakeChannel; 2] = [&color, &normal];
    let size = 256;
    let baked = bake(&surface, &positions, &faces, &layout.corners, &frames, size, size, 4, &channels);
    assert!(baked.coverage > 0.3, "cobertura {}", baked.coverage);

    // En el centro de cada quad: el texel debe tener la UV original del punto
    for (face, uvs) in faces.iter().zip(&layout.corners) {
        let center_uv = [0, 1].map(|k| uvs.iter().map(|c| c[k]).sum::<f32>() / 4.0);
        let center = [0, 1].map(|k| face.iter().map(|&v| positions[v][k]).sum::<f64>() / 4.0);
        let (x, y) = ((center_uv[0] * size as f32) as usize, (center_uv[1] * size as f32) as usize);
        let texel = baked.images[0][y * size as usize + x];
        let expected = [center[0] / 2.0, center[1] / 2.0].map(|c| (c * 255.0).round() as i32);
        assert!((texel[0] as i32 - expected[0]).abs() <= 3 && (texel[1] as i32 - expected[1]).abs() <= 3, "{texel:?} vs {expected:?}");
        assert_eq!(texel[2], 3, "grupo");
        // Plano: la normal del original es la del destino → (0, 0, 1)
        let n = baked.images[1][y * size as usize + x];
        assert!((n[0] as i32 - 128).abs() <= 2 && (n[1] as i32 - 128).abs() <= 2 && n[2] >= 253, "normal {n:?}");
    }

    // La dilatación llena todo el fondo
    assert!(baked.images[0].iter().all(|p| p[3] == 255));
}

/// Esfera cubo de radio 1: `n × n` quads por cara.
fn cube_sphere(n: usize) -> (Vec<[f64; 3]>, Vec<[usize; 4]>) {
    let mut points = Vec::new();
    let mut faces = Vec::new();
    for axis in 0..3 {
        for sign in [-1.0, 1.0] {
            let base = points.len();
            for j in 0..=n {
                for i in 0..=n {
                    let (u, v) = (2.0 * i as f64 / n as f64 - 1.0, 2.0 * j as f64 / n as f64 - 1.0);
                    let mut p = [0.0; 3];
                    p[axis] = sign;
                    p[(axis + 1) % 3] = u * sign;
                    p[(axis + 2) % 3] = v;
                    let len = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
                    points.push(p.map(|c| c / len));
                }
            }
            let idx = |i: usize, j: usize| base + j * (n + 1) + i;
            for j in 0..n {
                for i in 0..n {
                    faces.push([idx(i, j), idx(i + 1, j), idx(i + 1, j + 1), idx(i, j + 1)]);
                }
            }
        }
    }
    (points, faces)
}

#[test]
fn bakes_vertex_colors_and_relief_without_source_uvs() {
    use converter_scene::{IndexData, Material, Mesh, Primitive, Scene, VertexAttribute};

    // Original denso sin UV: rojo arriba (z > 0), azul abajo
    let (dense, quads) = cube_sphere(24);
    let positions: Vec<[f32; 3]> = dense.iter().map(|p| p.map(|c| c as f32)).collect();
    let colors: Vec<[f32; 4]> = dense.iter().map(|p| if p[2] > 0.0 { [1.0, 0.0, 0.0, 1.0] } else { [0.0, 0.0, 1.0, 1.0] }).collect();
    let indices: Vec<u32> = quads.iter().flat_map(|q| [q[0], q[1], q[2], q[0], q[2], q[3]]).map(|i| i as u32).collect();
    let mut scene = Scene::new();
    scene.materials.push(Material::default());
    scene.meshes.push(Mesh {
        name: "denso".into(),
        primitives: vec![Primitive {
            attributes: vec![VertexAttribute::Positions(positions), VertexAttribute::Colors(colors)],
            indices: Some(IndexData::U32(indices)),
            material: Some(0),
        }],
    });

    // Malla liviana
    let (points, faces) = cube_sphere(4);
    let options = uv_core::BakeOptions { texture_size: 256, ..Default::default() };
    let skin = uv_core::unwrapped_skin(&scene, None, &points, &faces, &options);
    let texture = |name: &str| {
        let t = skin.textures.iter().find(|t| t.name == name).unwrap_or_else(|| panic!("falta {name}"));
        image::load_from_memory(&t.data).unwrap().to_rgba8()
    };
    let (color, normal) = (texture("baked_base_color"), texture("baked_normal"));

    for (f, face) in faces.iter().enumerate() {
        let z = face.iter().map(|&v| points[v][2]).sum::<f64>() / 4.0;
        if z.abs() < 0.3 {
            continue;
        }
        let uv = [0, 1].map(|k| skin.corners[f].iter().map(|c| c[k]).sum::<f32>() / 4.0);
        let (x, y) = ((uv[0] * 256.0) as u32, (uv[1] * 256.0) as u32);
        let c = color.get_pixel(x, y).0;
        let expected = if z > 0.0 { [255, 0, 0] } else { [0, 0, 255] };
        assert_eq!(&c[..3], &expected, "cara {f} (z = {z:.2})");
        // Superficie lisa en ambas: normal casi sin desviar
        let n = normal.get_pixel(x, y).0;
        assert!(n[2] > 230, "normal {n:?} en la cara {f}");
    }
}
