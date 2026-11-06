//! Lamina un STL binario para la LD-002H y escribe el `.ctb`, con la pieza centrada en la
//! pantalla. Para comparar con un archivo de Chitubox/Lychee (PLAN_SLA.md, SLA2).
//! Con `--preparar`: venteos, soportes automáticos, revisión e inclinaciones (SLA3–SLA4).
//!   [AA=4] [XY=0.05] [PATA=0.2] [AGUJEROS=0.1] cargo run --release -p pinocchio-slicer --example sla_ctb -- pieza.stl salida.ctb [--preparar]
use pinocchio_slicer::{ctb, sla, sla_check, sla_prepare};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let b = std::fs::read(&args[1]).expect("no se pudo leer el STL");
    let n = u32::from_le_bytes(b[80..84].try_into().unwrap()) as usize;
    let f = |o: usize| f32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    let mut tris: Vec<[[f32; 3]; 3]> =
        (0..n).map(|i| std::array::from_fn(|k| std::array::from_fn(|c| f(84 + 50 * i + 12 + 12 * k + 4 * c)))).collect();
    let pr = sla::LD_002H;
    let (mut min, mut max) = ([f32::MAX; 3], [f32::MIN; 3]);
    for v in tris.iter().flatten() {
        for c in 0..3 {
            min[c] = min[c].min(v[c]);
            max[c] = max[c].max(v[c]);
        }
    }
    let shift = [pr.bed_x / 2.0 - (min[0] + max[0]) / 2.0, pr.bed_y / 2.0 - (min[1] + max[1]) / 2.0, -min[2]];
    for v in tris.iter_mut().flatten() {
        for c in 0..3 {
            v[c] += shift[c];
        }
    }
    // Con AA=4 (2, 4 u 8) lamina con bordes en grises; XY, PATA y AGUJEROS (mm) compensan;
    // V3=1 escribe el .ctb v3 con subida dinámica.
    let env = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<f32>().ok());
    let v3 = env("V3").is_some_and(|v| v > 0.0);
    let mut s = sla::ResinSettings {
        antialias: env("AA").unwrap_or(1.0) as u8,
        xy_offset: env("XY").unwrap_or(0.0),
        elephant_foot: env("PATA").unwrap_or(0.0),
        hole_offset: env("AGUJEROS").unwrap_or(0.0),
        ctb_version: if v3 { 3 } else { 2 },
        ..Default::default()
    };
    s.dynamic_lift.enabled = v3;
    let t = std::time::Instant::now();
    let (layers, parts) = if args.iter().any(|a| a == "--preparar") {
        let before = sla_check::check(&sla::slice(&tris, &pr, s.layer_height).expect("laminado"), &pr, s.layer_height);
        println!("sin preparar: {} islas, {} huecos problema", before.islands.len(), before.cavities.len());
        let p = sla_prepare::prepare(&tris, &pr, &s, &Default::default(), None).expect("preparar");
        println!(
            "preparado en {:?}: {} venteos, {} puntos, {} triángulos de soporte; quedan {} islas, {} huecos; área máx {:.0} mm²",
            t.elapsed(),
            p.vents.len(),
            p.points.len(),
            p.supports.len(),
            p.report.islands.len(),
            p.report.cavities.len(),
            p.report.max_area_mm2
        );
        for n in &p.notes {
            println!("  nota: {n}");
        }
        let near = |q: &[f32; 3]| {
            p.points
                .iter()
                .map(|s| (((s.pos[0] - q[0]).powi(2) + (s.pos[1] - q[1]).powi(2)).sqrt(), s.pos[2] - q[2]))
                .min_by(|a, b| a.0.total_cmp(&b.0))
        };
        for i in p.report.islands.iter().take(8) {
            println!("  isla capa {} en {:?} área {:.3} mm²; punto más cerca (xy, dz) {:?}", i.layer, i.pos, i.area_mm2, near(&i.pos));
        }
        for v in &p.vents {
            println!("  venteo {v:?}");
        }
        if std::env::var("PUNTOS").is_ok() {
            for sp in p.points.iter().filter(|sp| (sp.pos[0] - 47.81).abs() < 0.5 && (sp.pos[1] - 62.86).abs() < 0.5) {
                println!("  punto {sp:?}");
            }
        }
        for c in p.report.cavities.iter().take(5) {
            println!("  hueco {c:?}");
        }
        let t2 = std::time::Instant::now();
        let opts = sla_prepare::tilt_options(&tris, &pr, s.layer_height, &[15.0, 30.0, 45.0]);
        println!("inclinaciones ({:?}):", t2.elapsed());
        for o in opts.iter().take(4) {
            println!("  {:?} {:>4}°: área máx {:>6.0} mm², succión {:>5.0} mm², alto {:>5.1} mm, puntaje {:.2}", o.axis, o.degrees, o.max_area_mm2, o.suction_mm2, o.height_mm, o.score);
        }
        (p.layers, vec![p.model, p.supports])
    } else {
        (sla::slice_with(&tris, &pr, &s).expect("laminado"), vec![tris])
    };
    let sliced = t.elapsed();
    // Con VISTA=archivo.ppm guarda una vista grande de lo preparado (para revisar a ojo).
    if let Ok(path) = std::env::var("VISTA") {
        let img = ctb::preview_image(&parts, (900, 900), [200, 200, 200]);
        let mut ppm = b"P6 900 900 255\n".to_vec();
        for p in img.rgba.as_chunks::<4>().0 {
            let a = p[3] as u32;
            ppm.extend((0..3).map(|c| ((p[c] as u32 * a + 40 * (255 - a)) / 255) as u8));
        }
        std::fs::write(path, ppm).expect("vista");
    }
    let large = ctb::preview_image(&parts, ctb::LARGE_PREVIEW, [200, 200, 200]);
    let small = ctb::preview_image(&parts, ctb::SMALL_PREVIEW, [200, 200, 200]);
    let out = ctb::write_ctb(&layers, &pr, &s, &large, &small).expect("ctb");
    std::fs::write(&args[2], &out).expect("no se pudo escribir");
    println!(
        "{} capas, {:.2} ml, {} bytes; laminar {:?}, total {:?}",
        layers.len(),
        sla::volume_ml(&layers, &pr, s.layer_height),
        out.len(),
        sliced,
        t.elapsed()
    );
}
