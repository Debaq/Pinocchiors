//! Vuelca la vista previa de un gcode: `preview.bin` (binario de `Preview::to_bytes`) y `preview.json`.
//!
//!     cargo run -p pinocchio-slicer --example preview_dump -- placa.gcode salida/
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (gcode, out) = (&args[1], std::path::Path::new(&args[2]));
    let text = std::fs::read_to_string(gcode).expect("gcode");
    let p = pinocchio_slicer::parse_gcode(&text);
    std::fs::write(out.join("preview.bin"), p.to_bytes()).unwrap();
    std::fs::write(out.join("preview.json"), serde_json::to_string(&p).unwrap()).unwrap();
    println!("{} segmentos, {} viajes, {} capas", p.segments, p.travel_segments, p.layer_z.len());
}
