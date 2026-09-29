//! Diagnóstico sin interfaz: conecta el escáner y muestra el estado unos segundos.
//!
//! `cargo run --release -p orizon3d-core --example probe`

use std::time::Duration;

use orizon3d_core::{ScanSettings, Scanner, ScannerState};

fn main() {
    let scanner = Scanner::connect(ScanSettings::default());
    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(250));
        let s = scanner.status();
        match &s.state {
            ScannerState::Error(e) => {
                println!("Error: {e}");
                return;
            }
            ScannerState::Streaming => println!(
                "{} · {:.0} FPS · distancia {:.0} cm · cobertura {:.0} %",
                s.device.as_ref().map_or("?", |d| d.name.as_str()),
                s.fps,
                s.distance_cm,
                s.coverage * 100.0
            ),
            other => println!("{other:?}"),
        }
    }
    match scanner.build_mesh(&Default::default()) {
        Ok(mesh) => println!("Malla del cuadro actual: {} vértices, {} triángulos", mesh.vertices.len(), mesh.tris.len()),
        Err(e) => println!("Malla: {e}"),
    }
}
