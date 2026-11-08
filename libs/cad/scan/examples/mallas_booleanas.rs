//! Banco (libs/cad/PLAN_MALLAS.md, M1): restar una herramienta chica a una malla
//! grande, con OpenCASCADE (la malla como sólido de una cara por triángulo) y
//! con manifold-rust (sobre triángulos). La herramienta son cilindros que
//! entran 1 mm en la cara de adelante, como un grabado.
//!
//! `cargo run --release -p cad-scan --example mallas_booleanas -- malla.stl [--occt]`
//!
//! Sin `--occt` solo mide la conversión a sólido OCCT (la resta OCCT tarda
//! minutos con mallas de 100 000+ triángulos).

use std::collections::HashMap;
use std::time::Instant;

use cad_occt::{Frame, Shape};
use manifold_rust::manifold::Manifold;
use manifold_rust::types::{BooleanEngine, MeshGL64};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("falta la malla");
    let with_occt = args.any(|a| a == "--occt");

    let t0 = Instant::now();
    let scene = converter_core::import(&path).unwrap();
    let mm = scene.meters_per_unit * 1000.0;
    let mut vertices: Vec<[f64; 3]> = Vec::new();
    let mut triangles: Vec<[u32; 3]> = Vec::new();
    for prim in scene.world_primitives_z_up() {
        let base = vertices.len() as u32;
        vertices.extend(prim.positions.iter().map(|p| p.map(|c| c as f64 * mm)));
        triangles.extend(prim.triangles.iter().map(|t| t.map(|i| i + base)));
    }
    let (vertices, triangles) = weld(&vertices, &triangles);
    println!("{path}: {} triángulos, {} vértices ({:.2} s leer)", triangles.len(), vertices.len(), t0.elapsed().as_secs_f64());
    let (lo, hi) = bounds(&vertices);

    // Herramienta: 5 cilindros de 2,5 mm de radio a lo largo de Y, desde afuera
    // de la cara de adelante (−Y) hasta 1 mm adentro de la piel, a 60 % del alto
    let z = lo[2] + (hi[2] - lo[2]) * 0.6;
    let mut cylinders = Vec::new();
    for k in 0..5 {
        let x = lo[0] + (hi[0] - lo[0]) * (0.3 + 0.1 * k as f64);
        // Primer punto de la piel desde −Y en esa (x, z): el vértice más cercano a la recta
        let front = vertices
            .iter()
            .filter(|v| (v[0] - x).abs() < 3.0 && (v[2] - z).abs() < 3.0)
            .map(|v| v[1])
            .fold(f64::INFINITY, f64::min);
        let start = lo[1] - 5.0;
        let frame = Frame { origin: [x, start, z], z: [0.0, 1.0, 0.0], x: [1.0, 0.0, 0.0] };
        cylinders.push(Shape::cylinder(frame, 2.5, front + 1.0 - start).unwrap());
    }
    let tool = Shape::fuse_all(&cylinders).unwrap();
    let tool_volume = tool.mass().unwrap().volume;

    // ── manifold-rust ──
    let t0 = Instant::now();
    let tess = tool.tessellate(0.01, 0.1).unwrap();
    let (tv, tt) = weld(&tess.positions, &tess.triangles);
    println!("herramienta: {} triángulos ({:.3} s teselar), volumen {tool_volume:.1}", tt.len(), t0.elapsed().as_secs_f64());

    let t0 = Instant::now();
    let body = to_manifold(&vertices, &triangles);
    println!("malla → manifold: {:.3} s, estado {:?}, volumen {:.1}", t0.elapsed().as_secs_f64(), body.status(), body.volume());
    let robust = if body.status() != manifold_rust::types::Error::NoError {
        let t0 = Instant::now();
        let r = Manifold::from_mesh_gl64_robust(&mesh_gl(&vertices, &triangles));
        println!("  robusta: {:.3} s, estado {:?}, volumen {:.1}", t0.elapsed().as_secs_f64(), r.status(), r.volume());
        Some(r)
    } else {
        None
    };
    let body = robust.as_ref().unwrap_or(&body);
    let cutter = to_manifold(&tv, &tt);
    println!("herramienta → manifold: estado {:?}, volumen {:.1}", cutter.status(), cutter.volume());

    let t0 = Instant::now();
    let cut = body.difference_with_engine(&cutter, BooleanEngine::Auto);
    let ms = t0.elapsed().as_secs_f64();
    println!(
        "RESTA manifold: {ms:.3} s, estado {:?}, {} triángulos, volumen {:.1} (quitó {:.1})",
        cut.status(),
        cut.num_tri(),
        cut.volume(),
        body.volume() - cut.volume()
    );
    let t0 = Instant::now();
    let out = cut.get_mesh_gl64(0);
    println!("  de vuelta a malla: {:.3} s, {} vértices", t0.elapsed().as_secs_f64(), out.vert_properties.len() / out.num_prop as usize);

    // ── OpenCASCADE ──
    let t0 = Instant::now();
    let solid = Shape::from_mesh(&vertices, &triangles, 1e-4).unwrap();
    println!("OCCT malla → sólido: {:.2} s, {} caras", t0.elapsed().as_secs_f64(), solid.face_count());
    let t0 = Instant::now();
    let valid = solid.is_valid();
    println!("OCCT validar: {:.2} s ({valid})", t0.elapsed().as_secs_f64());
    let t0 = Instant::now();
    let volume = solid.mass().map(|m| m.volume).unwrap_or(f64::NAN);
    println!("OCCT volumen: {:.2} s ({volume:.1})", t0.elapsed().as_secs_f64());
    let t0 = Instant::now();
    let view = solid.tessellate(0.05, 0.3).map(|t| t.triangles.len()).unwrap_or(0);
    println!("OCCT teselar para la vista: {:.2} s ({view} triángulos)", t0.elapsed().as_secs_f64());
    if with_occt {
        let t0 = Instant::now();
        match solid.cut(&tool) {
            Ok(r) => println!("RESTA OCCT: {:.2} s, volumen {:.1}", t0.elapsed().as_secs_f64(), r.mass().map(|m| m.volume).unwrap_or(f64::NAN)),
            Err(e) => println!("RESTA OCCT: falló en {:.2} s ({e})", t0.elapsed().as_secs_f64()),
        }
    }
}

fn bounds(v: &[[f64; 3]]) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in v {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    (lo, hi)
}

/// Vértices iguales en uno (el teselado de OCCT repite los de cada cara)
fn weld(v: &[[f64; 3]], t: &[[u32; 3]]) -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
    let (lo, hi) = bounds(v);
    let q = (0..3).map(|k| hi[k] - lo[k]).fold(0.0, f64::max).max(1e-9) * 1e-9;
    let mut map: HashMap<[i64; 3], u32> = HashMap::new();
    let mut out = Vec::new();
    let remap: Vec<u32> = v
        .iter()
        .map(|p| {
            *map.entry(p.map(|c| (c / q).round() as i64)).or_insert_with(|| {
                out.push(*p);
                (out.len() - 1) as u32
            })
        })
        .collect();
    let tris = t.iter().map(|t| t.map(|i| remap[i as usize])).filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2]).collect();
    (out, tris)
}

fn mesh_gl(v: &[[f64; 3]], t: &[[u32; 3]]) -> MeshGL64 {
    MeshGL64 {
        num_prop: 3,
        vert_properties: v.iter().flatten().copied().collect(),
        tri_verts: t.iter().flatten().map(|&i| i as u64).collect(),
        ..Default::default()
    }
}

fn to_manifold(v: &[[f64; 3]], t: &[[u32; 3]]) -> Manifold {
    Manifold::from_mesh_gl64(&mesh_gl(v, t))
}
