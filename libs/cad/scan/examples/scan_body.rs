//! Banco del escaneo como cuerpo: prepara la malla (vóxeles con holgura +
//! isótropo), la pasa a sólido y le resta una caja que la corta por la mitad.
//!
//! `cargo run --release -p cad-scan --example scan_body -- malla N holgura [salida.stl]`
//! (`CORTE=0.37`: altura del corte de la caja, fracción del alto; `SOLO_PREPARAR=1`: sin OCCT;
//! `ESPESOR=2`: cáscara de ese espesor desde la piel corrida la holgura, negativo hacia adentro)

use std::time::Instant;

use cad_occt::{BooleanOp, Frame, Shape, ShapeKind};
use cad_scan::{BodyOptions, open_edges, prepare_body};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("falta la malla");
    let faces: usize = args.next().map(|s| s.parse().unwrap()).unwrap_or(20_000);
    let offset: f64 = args.next().map(|s| s.parse().unwrap()).unwrap_or(0.0);
    let out = args.next();

    let scene = converter_core::import(&path).unwrap();
    let mm = scene.meters_per_unit * 1000.0;
    let mut vertices: Vec<[f64; 3]> = Vec::new();
    let mut triangles: Vec<[u32; 3]> = Vec::new();
    for prim in scene.world_primitives_z_up() {
        let base = vertices.len() as u32;
        vertices.extend(prim.positions.iter().map(|p| p.map(|c| c as f64 * mm)));
        triangles.extend(prim.triangles.iter().map(|t| t.map(|i| i + base)));
    }
    println!("{}: {} triángulos, {} aristas sueltas", path, triangles.len(), open_edges(&triangles));

    let t0 = Instant::now();
    let thickness: f64 = std::env::var("ESPESOR").ok().and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let body = prepare_body(&vertices, &triangles, &BodyOptions { faces, offset, thickness }).unwrap();
    println!("preparado en {:.2} s: {} triángulos, lado {:.3} mm, {} aristas sueltas", t0.elapsed().as_secs_f64(), body.triangles.len(), body.edge, open_edges(&body.triangles));
    println!("{:?}", cad_scan::solid_problems(&body.vertices, &body.triangles));
    if let Some(out) = out {
        let mesh: Vec<stl_io::Triangle> = body
            .triangles
            .iter()
            .map(|t| {
                let v = t.map(|i| body.vertices[i as usize].map(|c| c as f32));
                stl_io::Triangle { normal: stl_io::Normal::new([0.0; 3]), vertices: v.map(stl_io::Vertex::new) }
            })
            .collect();
        stl_io::write_stl(&mut std::fs::File::create(&out).unwrap(), mesh.iter()).unwrap();
    }

    if std::env::var("SOLO_PREPARAR").is_ok() {
        return;
    }
    let t0 = Instant::now();
    let solid = Shape::from_mesh(&body.vertices, &body.triangles, 1e-4).unwrap();
    println!("a sólido: {:.2} s, {:?}, válido {}", t0.elapsed().as_secs_f64(), solid.kind(), solid.is_valid());
    assert_eq!(solid.kind(), ShapeKind::Solid);
    let mass = solid.mass().unwrap();
    println!("volumen {:.1} mm³", mass.volume);
    let (lo, hi) = (mass.bbox_min, mass.bbox_max);
    let pad = [0, 1, 2].map(|k| (hi[k] - lo[k]) * 0.1);
    let at: f64 = std::env::var("CORTE").ok().and_then(|s| s.parse().ok()).unwrap_or(0.5);
    let mid = lo[2] + (hi[2] - lo[2]) * at;
    let frame = Frame { origin: [lo[0] - pad[0], lo[1] - pad[1], lo[2] - pad[2]], ..Frame::WORLD };
    let block = Shape::make_box(frame, hi[0] - lo[0] + 2.0 * pad[0], hi[1] - lo[1] + 2.0 * pad[1], mid - lo[2] + pad[2]).unwrap();
    let t0 = Instant::now();
    let cut = block.boolean(&solid, BooleanOp::Cut).unwrap();
    println!("caja − cuerpo: {:.2} s, válido {}, volumen {:.1}", t0.elapsed().as_secs_f64(), cut.is_valid(), cut.mass().unwrap().volume);
}
