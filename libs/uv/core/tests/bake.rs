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
    let surface = UvSurface::new([UvPart { group: 3, positions: &pos, uvs: &uv, normals: None, triangles: &tris }]).unwrap();

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

#[test]
fn bake_follows_the_normal_past_a_facing_piece() {
    // Pieza A: plano z = 0 mirando a +z (grupo 0). Pieza B: plano z = 0,3
    // mirando a −z (grupo 1), enfrentada a A como la pata frente al colmillo.
    let (pos, uv, tris) = source();
    let pos_b: Vec<[f32; 3]> = pos.iter().map(|p| [p[0], p[1], 0.3]).collect();
    let tris_b: Vec<[u32; 3]> = tris.iter().map(|t| [t[0], t[2], t[1]]).collect();
    let surface = UvSurface::new([
        UvPart { group: 0, positions: &pos, uvs: &uv, normals: None, triangles: &tris },
        UvPart { group: 1, positions: &pos_b, uvs: &uv, normals: None, triangles: &tris_b },
    ])
    .unwrap();

    // Malla destino de A, corrida 0,2 hacia B: el punto más cercano es B
    let n = 5;
    let positions: Vec<[f64; 3]> =
        (0..=n).flat_map(|j| (0..=n).map(move |i| [2.0 * i as f64 / n as f64, 2.0 * j as f64 / n as f64, 0.2])).collect();
    let idx = |i: usize, j: usize| j * (n + 1) + i;
    let faces: Vec<[usize; 4]> =
        (0..n).flat_map(|j| (0..n).map(move |i| [idx(i, j), idx(i + 1, j), idx(i + 1, j + 1), idx(i, j + 1)])).collect();
    let layout = unwrap(&positions, &faces, &UnwrapOptions::default());
    let frames = corner_frames(&positions, &faces, &layout.corners);

    let group = |ctx: &TexelContext| [ctx.group as u8 * 255, 0, 0, 255];
    let channels: [BakeChannel; 1] = [&group];
    let baked = bake(&surface, &positions, &faces, &layout.corners, &frames, 128, 128, 2, &channels);
    assert!(baked.coverage > 0.3, "cobertura {}", baked.coverage);
    let wrong = baked.images[0].iter().filter(|p| p[0] != 0).count();
    assert_eq!(wrong, 0, "texels tomados de la pieza enfrentada");
}
