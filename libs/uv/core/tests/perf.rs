//! Benchmarks de rendimiento de uv-core (desplegado + empaquetado y horneado por rayos).
//! No son pruebas funcionales: van con `#[ignore = "perf"]` y se ejecutan a mano.
//!
//! Comando (el perfil release tiene LTO y tarda muchísimo, por eso se desactiva):
//!
//!   CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
//!     cargo test --release -p uv-core --test perf -- --ignored --nocapture --test-threads=1
//!
//! Escalado con hilos: repetir con `RAYON_NUM_THREADS=1` y sin la variable.
//! Tamaño: `PERF_SCALE=small` (humo, diminuto); por defecto mallas de ~100k-270k triángulos
//! y texturas de 2048.
//! Cada benchmark hace 1 iteración de calentamiento + mediana de 3 repeticiones e
//! imprime: `PERF crate=uv-core bench=<nombre> threads=<n> tris=<n> ms=<mediana>`.
//! Cada uno verifica un resultado sensato (no medir un camino vacío).
//!
//! `pack` no es público: se mide a través de `unwrap` (cartas + despliegue + empaquetado) y
//! de `unwrapped_skin` (lo que llama la app: desplegar + hornear).

use converter_scene::{IndexData, Material, Mesh, Primitive, Scene, VertexAttribute};
use std::time::Instant;
use uv_core::{bake, corner_frames, unwrap, BakeChannel, BakeOptions, TexelContext, UnwrapOptions, UvPart, UvSurface};

fn small() -> bool {
    std::env::var("PERF_SCALE").is_ok_and(|v| v == "small")
}

/// Mediana de 3 repeticiones tras un calentamiento; imprime la línea PERF.
fn bench<R>(name: &str, tris: usize, mut f: impl FnMut() -> R) -> R {
    let mut last = f();
    let mut ms = Vec::new();
    for _ in 0..3 {
        let t = Instant::now();
        last = f();
        ms.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    ms.sort_by(|a, b| a.total_cmp(b));
    println!("PERF crate=uv-core bench={name} threads={} tris={tris} ms={:.2}", rayon::current_num_threads(), ms[1]);
    last
}

/// Esfera-cubo ondulada de radio ~1: `n × n` quads por cara (6·n² quads).
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
                    let d = p.map(|c| c / len);
                    let r = 1.0 + 0.03 * (d[0] * 9.0).sin() * (d[1] * 7.0).cos() * (d[2] * 11.0).sin();
                    points.push(d.map(|c| c * r));
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

fn triangulate(quads: &[[usize; 4]]) -> Vec<[u32; 3]> {
    quads.iter().flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]]).map(|t| t.map(|i| i as u32)).collect()
}

/// (n del original denso, n de la malla destino, tamaño de textura, n para desplegar)
fn sizes() -> (usize, usize, u32, usize) {
    if small() { (10, 4, 128, 10) } else { (150, 24, 2048, 60) }
}

/// Superficie densa con UV esféricas (lat/long), como un original con mapa.
fn dense_surface(n: usize) -> (UvSurface, usize) {
    let (points, quads) = cube_sphere(n);
    let pos: Vec<[f32; 3]> = points.iter().map(|p| p.map(|c| c as f32)).collect();
    let uv: Vec<[f32; 2]> = points
        .iter()
        .map(|p| {
            let len = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            [(p[1].atan2(p[0]) / std::f64::consts::TAU + 0.5) as f32, (0.5 + 0.5 * p[2] / len) as f32]
        })
        .collect();
    let tris = triangulate(&quads);
    let count = tris.len();
    let surface =
        UvSurface::new([UvPart { group: 1, positions: &pos, uvs: &uv, normals: None, colors: None, triangles: &tris }]).expect("superficie");
    (surface, count)
}

/// Desplegado completo (cartas + LSCM/ARAP + empaquetado por rasterizado) de una malla de quads.
#[test]
#[ignore = "perf"]
fn unwrap_and_pack() {
    let (_, _, size, n) = sizes();
    let (points, faces) = cube_sphere(n);
    let options = UnwrapOptions { texture_size: size, ..Default::default() };
    let out = bench("unwrap_and_pack", faces.len() * 2, || unwrap(&points, &faces, &options));
    assert_eq!(out.corners.len(), faces.len());
    assert!(out.num_charts > 0);
    assert!(out.corners.iter().flatten().flatten().all(|c| (-1e-3..=1.001).contains(c)), "UV fuera de [0,1]");
}

/// Construcción de la superficie de consulta (BVH de triángulos del original denso).
#[test]
#[ignore = "perf"]
fn surface_build() {
    let (n, ..) = sizes();
    let (points, quads) = cube_sphere(n);
    let pos: Vec<[f32; 3]> = points.iter().map(|p| p.map(|c| c as f32)).collect();
    let uv: Vec<[f32; 2]> = pos.iter().map(|p| [p[0] * 0.5 + 0.5, p[1] * 0.5 + 0.5]).collect();
    let tris = triangulate(&quads);
    let s = bench("surface_build", tris.len(), || {
        UvSurface::new([UvPart { group: 1, positions: &pos, uvs: &uv, normals: None, colors: None, triangles: &tris }])
    });
    assert_eq!(s.expect("superficie").num_triangles(), tris.len());
}

/// Horneado por rayos: original denso (~270k tris) -> malla liviana, 2 canales (color + normal).
#[test]
#[ignore = "perf"]
fn bake_rays() {
    let (dense_n, target_n, size, _) = sizes();
    let (surface, dense_tris) = dense_surface(dense_n);
    let (positions, faces) = cube_sphere(target_n);
    let layout = unwrap(&positions, &faces, &UnwrapOptions { texture_size: size, ..Default::default() });
    let frames = corner_frames(&positions, &faces, &layout.corners);

    let color = |ctx: &TexelContext| [(ctx.uv[0] * 255.0) as u8, (ctx.uv[1] * 255.0) as u8, ctx.group as u8, 255];
    let normal = |ctx: &TexelContext| {
        let n = ctx.source_frame[2];
        [n.map(|c| ((0.5 * c + 0.5) * 255.0) as u8)[0], n.map(|c| ((0.5 * c + 0.5) * 255.0) as u8)[1], n.map(|c| ((0.5 * c + 0.5) * 255.0) as u8)[2], 255]
    };
    let channels: [BakeChannel; 2] = [&color, &normal];
    let baked = bench("bake_rays", dense_tris, || bake(&surface, &positions, &faces, &layout.corners, &frames, size, size, 4, &channels));
    assert!(baked.coverage > 0.3, "cobertura {}", baked.coverage);
    assert_eq!(baked.images.len(), 2);
    assert!(baked.images[0].iter().any(|p| p[2] == 1), "ningún texel tomó el grupo del original");
}

/// Lo que llama la app (retopología + textura): original denso SIN UV -> `unwrapped_skin`
/// (superficie de material + desplegado + horneado de color y normal).
#[test]
#[ignore = "perf"]
fn unwrapped_skin_end_to_end() {
    let (dense_n, target_n, size, _) = sizes();
    let size = if small() { size } else { 1024 };
    let (dense, quads) = cube_sphere(dense_n);
    let positions: Vec<[f32; 3]> = dense.iter().map(|p| p.map(|c| c as f32)).collect();
    let colors: Vec<[f32; 4]> = dense.iter().map(|p| if p[2] > 0.0 { [1.0, 0.0, 0.0, 1.0] } else { [0.0, 0.0, 1.0, 1.0] }).collect();
    let indices: Vec<u32> = triangulate(&quads).into_iter().flatten().collect();
    let dense_tris = indices.len() / 3;
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
    let (points, faces) = cube_sphere(target_n);
    let options = BakeOptions { texture_size: size, ..Default::default() };
    let skin = bench("unwrapped_skin", dense_tris, || uv_core::unwrapped_skin(&scene, None, &points, &faces, &options));
    assert_eq!(skin.corners.len(), faces.len());
    assert!(skin.textures.iter().any(|t| t.name == "baked_base_color"), "no se horneó el color");
    match skin.info {
        uv_core::SkinInfo::Unwrapped { coverage, .. } => assert!(coverage > 0.3, "cobertura {coverage}"),
        other => panic!("info inesperada {other:?}"),
    }
}
