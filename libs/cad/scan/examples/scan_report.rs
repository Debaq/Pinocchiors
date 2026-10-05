//! Banco: detecta primitivas en un STL y mide tiempos.
//! `cargo run --release -p cad-scan --example scan_report -- pieza.stl [tolerancia_mm]`

use cad_scan::*;
use std::time::Instant;

fn main() {
    let path = std::env::args().nth(1).expect("falta el STL");
    let tol: Option<f64> = std::env::args().nth(2).map(|s| s.parse().unwrap());
    let mut f = std::fs::File::open(&path).unwrap();
    let stl = stl_io::read_stl(&mut f).unwrap();
    let v: Vec<[f64; 3]> = stl.vertices.iter().map(|p| [p[0] as f64, p[1] as f64, p[2] as f64]).collect();
    let t: Vec<[u32; 3]> = stl.faces.iter().map(|f| f.vertices.map(|i| i as u32)).collect();

    let t0 = Instant::now();
    let mesh = ScanMesh::new(&v, &t);
    println!("{} triángulos, {} vértices soldados, diagonal {:.1} mm ({:.0} ms)", mesh.face_count(), mesh.vertices.len(), mesh.diagonal(), t0.elapsed().as_secs_f64() * 1e3);

    let t0 = Instant::now();
    let found = detect_all(&mesh, &DetectOptions { tolerance: tol, ..Default::default() });
    println!("detect_all: {} zonas en {:.0} ms", found.len(), t0.elapsed().as_secs_f64() * 1e3);
    let total: f64 = mesh.face_areas.iter().sum();
    for d in found.iter().take(25) {
        let what = match &d.shape {
            DetectedShape::Plane { plane } => format!("plano   n=({:+.3},{:+.3},{:+.3})", plane.normal[0], plane.normal[1], plane.normal[2]),
            DetectedShape::Cylinder { radius, direction, .. } => format!("cilindro r={radius:.3} eje=({:+.2},{:+.2},{:+.2})", direction[0], direction[1], direction[2]),
            DetectedShape::Sphere { radius, .. } => format!("esfera  r={radius:.3}"),
        };
        println!("  {what:<44} {:5.1}% área  rms {:.4}  {} caras", d.area / total * 100.0, d.rms, d.faces.len());
    }
    let covered: f64 = found.iter().map(|d| d.area).sum();
    println!("cubierto: {:.1}% del área", covered / total * 100.0);

    if let Some(big) = found.iter().find(|d| matches!(d.shape, DetectedShape::Plane { .. })) {
        let t0 = Instant::now();
        let p = pick_plane(&mesh, big.faces[0], &PickOptions { tolerance: tol, ..Default::default() }).unwrap();
        let depth = p.depth(&mesh);
        println!("pick_plane en la zona mayor: {} caras, {} contornos, profundidad {:?} ({:.0} ms)", p.faces.len(), p.boundary.len(), depth, t0.elapsed().as_secs_f64() * 1e3);
    }
}
