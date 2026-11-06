//! Escribe el archivo de prueba de la Halot-Sky (SLA17): una «F» de 1 mm con la exposición
//! normal en el valor crudo 35 (la pantalla dice 3.5 s si son décimas, 35 s si son segundos).
//!   cargo run --release -p pinocchio-slicer --example cxdlp_prueba -- salida.cxdlp
use pinocchio_slicer::{ctb, cxdlp, sla};

fn main() {
    let path = std::env::args().nth(1).expect("falta la ruta del archivo");
    let pr = &sla::HALOT_SKY;
    let s = sla::ResinSettings { bottom_exposure: 30.0, bottom_layers: 6, ..Default::default() };
    let piece = cxdlp::probe_piece(pr);
    let layers = sla::slice(&piece, pr, s.layer_height).expect("laminado");
    let parts = vec![piece];
    let previews = cxdlp::PREVIEWS.map(|(w, h)| ctb::preview_image(&parts, (w, h), [180, 190, 200]));
    let bytes = cxdlp::write_cxdlp(&layers, pr, &s, cxdlp::ExposureUnit::Tenths, cxdlp::Mirror::None, &previews, Some(cxdlp::PROBE_RAW_EXPOSURE)).expect("cxdlp");
    std::fs::write(&path, &bytes).expect("no se pudo escribir");
    println!("{} capas, {} bytes → {path}", layers.len(), bytes.len());
}
