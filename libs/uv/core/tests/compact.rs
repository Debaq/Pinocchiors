use converter_scene::{Material, Texture, TextureFormat, TextureRef};
use uv_core::{compact_skin, corner_frames, unwrap, CornerFrames, Layout, Skin, SkinInfo, UnwrapOptions};

/// Cubo de `n × n` quads por lado inflado a esfera.
fn sphere(n: usize) -> (Vec<[f64; 3]>, Vec<[usize; 4]>) {
    let mut points = Vec::new();
    let mut index = std::collections::HashMap::new();
    let mut faces = Vec::new();
    for axis in 0..3 {
        for sign in [-1.0, 1.0] {
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            let mut vertex = |i: usize, j: usize| {
                let mut p = [0.0; 3];
                p[axis] = sign;
                p[u] = -1.0 + 2.0 * i as f64 / n as f64;
                p[v] = -1.0 + 2.0 * j as f64 / n as f64;
                let key = p.map(|c: f64| (c * 1e6).round() as i64);
                *index.entry(key).or_insert_with(|| {
                    let len = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
                    points.push(p.map(|c| c / len));
                    points.len() - 1
                })
            };
            for j in 0..n {
                for i in 0..n {
                    let mut q = [vertex(i, j), vertex(i + 1, j), vertex(i + 1, j + 1), vertex(i, j + 1)];
                    if sign < 0.0 {
                        q.reverse();
                    }
                    faces.push(q);
                }
            }
        }
    }
    (points, faces)
}

fn png(size: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Texture {
    let image = image::RgbaImage::from_fn(size, size, |x, y| image::Rgba(f(x, y)));
    let mut data = Vec::new();
    image.write_to(&mut std::io::Cursor::new(&mut data), image::ImageFormat::Png).unwrap();
    Texture { name: "t".into(), data, format: TextureFormat::Png, width: size, height: size }
}

fn texel(texture: &Texture, uv: [f32; 2]) -> [u8; 4] {
    let image = image::load_from_memory(&texture.data).unwrap().to_rgba8();
    let (w, h) = image.dimensions();
    let (x, y) = (((uv[0] * w as f32) as u32).min(w - 1), ((uv[1] * h as f32) as u32).min(h - 1));
    image.get_pixel(x, y).0
}

/// Normal en espacio objeto de un texel de normal map en el centro de la cara
fn object_normal(frames: &CornerFrames<4>, face: usize, t: [u8; 4]) -> [f32; 3] {
    let avg = |v: [[f32; 3]; 4]| [0, 1, 2].map(|k| v.iter().map(|c| c[k]).sum::<f32>() / 4.0);
    let n = avg(frames.normals[face]);
    let tangents = frames.tangents[face];
    let tan = avg(tangents.map(|t| [t[0], t[1], t[2]]));
    let w = tangents[0][3];
    let b = [n[1] * tan[2] - n[2] * tan[1], n[2] * tan[0] - n[0] * tan[2], n[0] * tan[1] - n[1] * tan[0]].map(|c| c * w);
    let local = [0, 1, 2].map(|k| t[k] as f32 / 255.0 * 2.0 - 1.0);
    let o = [0, 1, 2].map(|k| tan[k] * local[0] + b[k] * local[1] + n[k] * local[2]);
    let len = (o[0] * o[0] + o[1] * o[1] + o[2] * o[2]).sqrt();
    o.map(|c| c / len)
}

#[test]
fn compacting_keeps_colors_and_normals_on_the_surface() {
    let (positions, faces) = sphere(10);
    let paintable = unwrap(&positions, &faces, &UnwrapOptions { layout: Layout::Paintable, texture_size: 512, ..Default::default() });
    // Degradado suave de color y un normal map inclinado hacia +u
    let size = 512;
    let color = png(size, |x, y| [(x * 255 / size) as u8, (y * 255 / size) as u8, 90, 255]);
    let normal = png(size, |_, _| [200, 128, 220, 255]);
    let skin = Skin::<4> {
        corners: paintable.corners.clone(),
        face_material: vec![Some(0); faces.len()],
        materials: vec![Material {
            base_color_texture: Some(TextureRef { texture_index: 0, tex_coord_set: 0 }),
            normal_texture: Some(TextureRef { texture_index: 1, tex_coord_set: 0 }),
            ..Material::default()
        }],
        textures: vec![color, normal],
        info: SkinInfo::Unwrapped { num_charts: paintable.num_charts, stretch: paintable.stretch, coverage: paintable.coverage, texture_size: size },
        parts: None,
    };

    let compact = compact_skin(&skin, &positions, &faces, size, 4);
    let SkinInfo::Unwrapped { coverage, num_charts, .. } = compact.info else { panic!() };
    eprintln!("atlas: para pintar {:.1} % → compacto {:.1} %", 100.0 * paintable.coverage, 100.0 * coverage);
    assert!(coverage > paintable.coverage + 0.05, "{coverage} vs {}", paintable.coverage);
    assert_eq!(num_charts, paintable.num_charts, "mismas cartas");

    let material = &compact.materials[0];
    let new_color = &compact.textures[material.base_color_texture.as_ref().unwrap().texture_index];
    let new_normal = &compact.textures[material.normal_texture.as_ref().unwrap().texture_index];
    let old_frames = corner_frames(&positions, &faces, &skin.corners);
    let new_frames = corner_frames(&positions, &faces, &compact.corners);
    let center = |uvs: &[[f32; 2]; 4]| [0, 1].map(|k| uvs.iter().map(|c| c[k]).sum::<f32>() / 4.0);
    let mut worst_color = 0i32;
    let mut angles: Vec<f32> = Vec::new();
    for f in 0..faces.len() {
        let (old_uv, new_uv) = (center(&skin.corners[f]), center(&compact.corners[f]));
        let (a, b) = (texel(&skin.textures[0], old_uv), texel(new_color, new_uv));
        worst_color = worst_color.max((0..3).map(|k| (a[k] as i32 - b[k] as i32).abs()).max().unwrap());
        let (na, nb) = (object_normal(&old_frames, f, texel(&skin.textures[1], old_uv)), object_normal(&new_frames, f, texel(new_normal, new_uv)));
        let dot = (na[0] * nb[0] + na[1] * nb[1] + na[2] * nb[2]).clamp(-1.0, 1.0);
        angles.push(dot.acos().to_degrees());
    }
    angles.sort_by(f32::total_cmp);
    let (median, p95, worst) = (angles[angles.len() / 2], angles[angles.len() * 95 / 100], angles[angles.len() - 1]);
    eprintln!("peor color {worst_color}; normal: mediana {median:.2}°, p95 {p95:.2}°, peor {worst:.2}°");
    assert!(worst_color <= 6, "color {worst_color}");
    assert!(median < 1.0 && p95 < 3.0, "normal: mediana {median}°, p95 {p95}°");
}

#[test]
fn compacted_scene_keeps_the_vertices() {
    use converter_scene::VertexAttribute;
    let (positions, faces) = sphere(8);
    let paintable = unwrap(&positions, &faces, &UnwrapOptions { layout: Layout::Paintable, texture_size: 256, ..Default::default() });
    let skin = Skin::<4> {
        corners: paintable.corners.clone(),
        face_material: vec![Some(0); faces.len()],
        materials: vec![Material {
            base_color_texture: Some(TextureRef { texture_index: 0, tex_coord_set: 0 }),
            ..Material::default()
        }],
        textures: vec![png(256, |x, y| [x as u8, y as u8, 0, 255])],
        info: SkinInfo::Unwrapped { num_charts: paintable.num_charts, stretch: paintable.stretch, coverage: paintable.coverage, texture_size: 256 },
        parts: None,
    };
    // Escena como la deja desplegar la malla original: vértices partidos en las costuras
    let (scene, _) = uv_core::skin_scene(&positions, &faces, Some(&skin), &converter_scene::Scene::new());
    let compact = uv_core::compacted_scene(&scene, 256, 4).expect("se puede compactar");

    let attributes = |s: &converter_scene::Scene| s.meshes[0].primitives[0].attributes.clone();
    let (before, after) = (attributes(&scene), attributes(&compact));
    let get_uv = |a: &[VertexAttribute]| a.iter().find_map(|x| if let VertexAttribute::TexCoords(0, t) = x { Some(t.clone()) } else { None }).unwrap();
    let get_pos = |a: &[VertexAttribute]| a.iter().find_map(|x| if let VertexAttribute::Positions(p) = x { Some(p.clone()) } else { None }).unwrap();
    assert_eq!(get_pos(&before), get_pos(&after), "mismos vértices en el mismo orden");
    assert_ne!(get_uv(&before), get_uv(&after));
    assert!(get_uv(&after).iter().flatten().all(|c| (0.0..=1.0).contains(c)));
    assert_eq!(compact.materials.len(), 1);
    assert!(compact.materials[0].base_color_texture.is_some());
}
