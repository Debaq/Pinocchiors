//! Prueba a mano contra GIMP, Krita o Photoshop.
//!
//! ```text
//! cargo run -p converter-layers --example layers -- muestra salida.xcf|salida.psd
//! cargo run -p converter-layers --example layers -- leer archivo.(xcf|psd|png) [combinada.png]
//! ```

use converter_layers::{LayeredFormat, LayeredImage, Layer, draw_lines, fill_triangles};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("muestra") => {
            let path = &args[1];
            let (w, h) = (300, 200);
            let texture: Vec<u8> = (0..w * h).flat_map(|i| [(i % w) as u8, (i / w) as u8, 128, 255]).collect();
            let mut islands = Layer::new("Islas", w, h);
            fill_triangles(&mut islands, [[[20.0, 20.0], [150.0, 30.0], [60.0, 180.0]]], [255, 121, 198, 90]);
            islands.visible = false;
            let mut wire = Layer::new("Malla UV", w, h);
            draw_lines(&mut wire, [[[20.0, 20.0], [150.0, 30.0]], [[150.0, 30.0], [60.0, 180.0]], [[60.0, 180.0], [20.0, 20.0]]], [80, 250, 123], 2.0);
            wire.opacity = 0.8;
            let image = LayeredImage { width: w, height: h, layers: vec![Layer::from_rgba("Textura", w, h, texture), islands, wire] };
            let ext = path.rsplit('.').next().unwrap_or("");
            let format = LayeredFormat::from_extension(ext).expect("extensión .xcf o .psd");
            std::fs::write(path, image.write(format)).unwrap();
        }
        Some("leer") => {
            let bytes = std::fs::read(&args[1]).unwrap();
            let image = LayeredImage::read(&bytes).unwrap_or_else(|e| panic!("{e}"));
            println!("{} × {}, {} capas (de abajo hacia arriba)", image.width, image.height, image.layers.len());
            for l in &image.layers {
                println!("  {:?} {}×{} en ({}, {}) visible={} opacidad={:.2}", l.name, l.width, l.height, l.x, l.y, l.visible, l.opacity);
            }
            if let Some(out) = args.get(2) {
                let pixels = image.composite(|_| true);
                image::RgbaImage::from_raw(image.width, image.height, pixels).unwrap().save(out).unwrap();
            }
        }
        _ => eprintln!("uso: layers muestra <salida.xcf|psd> | leer <archivo> [combinada.png]"),
    }
}
