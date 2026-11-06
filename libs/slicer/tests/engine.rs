//! Lamina un cubo con el motor real. Se salta si no está compilado
//! (scripts/build-orca-engine.sh deja el binario en libs/slicer/bin).

use pinocchio_slicer::{build_config, find_engine, index, parse_gcode, run, Feature, SliceJob, SliceSettings};
use std::sync::atomic::AtomicBool;

fn cube(x: f32, y: f32, s: f32) -> Vec<[[f32; 3]; 3]> {
    let p = |a: f32, b: f32, c: f32| [x + a * s, y + b * s, c * s];
    let v = [p(0., 0., 0.), p(1., 0., 0.), p(1., 1., 0.), p(0., 1., 0.), p(0., 0., 1.), p(1., 0., 1.), p(1., 1., 1.), p(0., 1., 1.)];
    let f = [[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]];
    f.iter().map(|t| [v[t[0]], v[t[1]], v[t[2]]]).collect()
}

#[test]
fn lamina_un_cubo_para_la_k1() {
    let Some(engine) = find_engine() else {
        eprintln!("sin motor: prueba saltada");
        return;
    };
    let k1 = index().printers.iter().find(|p| p.name == "Creality K1 (0.4 nozzle)").unwrap();
    let settings = SliceSettings { printer: k1.name.clone(), process: k1.default_process.clone(), filament: k1.default_filament.clone(), overrides: Default::default() };
    let config = build_config(&settings).unwrap();
    let work_dir = std::env::temp_dir().join(format!("pinocchio-slicer-test-{}", std::process::id()));
    let job = SliceJob { config, parts: vec![cube(100.0, 100.0, 20.0)], work_dir: work_dir.clone() };
    let mut last = -1;
    let summary = run(&job, &engine, |e| if let pinocchio_slicer::Event::Progress { percent, .. } = e { last = percent }, &AtomicBool::new(false)).unwrap();
    assert!(last > 0, "sin progreso");
    let gcode = std::fs::read_to_string(&summary.gcode).unwrap();
    assert!(gcode.contains("START_PRINT"), "falta el inicio de Klipper");
    // La impresora sabe en qué capa va (Moonraker / pantalla)
    assert!(gcode.contains("SET_PRINT_STATS_INFO TOTAL_LAYER=10"), "falta el total de capas");
    assert!(gcode.contains("SET_PRINT_STATS_INFO CURRENT_LAYER=1\n") && gcode.contains("SET_PRINT_STATS_INFO CURRENT_LAYER=2\n"), "falta la capa actual");
    // Miniaturas de la K1 (96×96 y 300×300) en PNG dentro del gcode
    assert!(gcode.contains("; thumbnail begin 96x96"), "falta la miniatura chica");
    assert!(gcode.contains("; thumbnail begin 300x300"), "falta la miniatura grande");
    assert!(summary.roles.iter().any(|r| r.role == "Outer wall" && r.g > 0.0), "{:?}", summary.roles);
    let total: f64 = summary.roles.iter().map(|r| r.g).sum();
    assert!((total - summary.filament_g).abs() < 0.05 * summary.filament_g, "{total} vs {}", summary.filament_g);
    assert!(summary.time_s > 60.0, "tiempo {}", summary.time_s);
    assert!(summary.filament_mm > 100.0, "filamento {}", summary.filament_mm);
    // 20 mm a 0,2 mm por capa ≈ 100 capas
    assert!((95..=105).contains(&summary.layers), "capas {}", summary.layers);

    let p = parse_gcode(&gcode);
    assert!((95..=105).contains(&p.layer_z.len()), "capas leídas {}", p.layer_z.len());
    assert!(p.length_by_feature[Feature::OuterWall as usize] > 0.0);
    assert!(p.length_by_feature[Feature::SparseInfill as usize] > 0.0);
    // El cubo está donde lo pusimos (100–120 mm), no centrado
    assert!(p.min[0] > 95.0 && p.max[0] < 125.0, "x {:?}..{:?}", p.min, p.max);
    assert!((p.max[2] - 20.0).abs() < 0.3, "alto {}", p.max[2]);
    if std::env::var_os("KEEP_SLICE").is_none() {
        let _ = std::fs::remove_dir_all(work_dir);
    }
}

#[test]
fn cancelar_mata_el_motor() {
    let Some(engine) = find_engine() else { return };
    let k1 = index().printers.iter().find(|p| p.name == "Creality K1 (0.4 nozzle)").unwrap();
    let settings = SliceSettings { printer: k1.name.clone(), process: k1.default_process.clone(), filament: k1.default_filament.clone(), overrides: Default::default() };
    let job = SliceJob {
        config: build_config(&settings).unwrap(),
        parts: vec![cube(10.0, 10.0, 60.0)],
        work_dir: std::env::temp_dir().join(format!("pinocchio-slicer-cancel-{}", std::process::id())),
    };
    let cancel = AtomicBool::new(true);
    let r = run(&job, &engine, |_| {}, &cancel);
    assert!(matches!(r, Err(pinocchio_slicer::SlicerError::Cancelled)));
}

/// Octaedro parado sobre una punta en (cx, cy, 0): toca la cama en un punto
fn standing_octahedron(cx: f32, cy: f32, r: f32) -> Vec<[[f32; 3]; 3]> {
    let bottom = [cx, cy, 0.0];
    let top = [cx, cy, 2.0 * r];
    let ring = [[cx + r, cy, r], [cx, cy + r, r], [cx - r, cy, r], [cx, cy - r, r]];
    let mut out = Vec::new();
    for k in 0..4 {
        let (a, b) = (ring[k], ring[(k + 1) % 4]);
        out.push([top, a, b]);
        out.push([bottom, b, a]);
    }
    out
}

fn k1_job(parts: Vec<Vec<[[f32; 3]; 3]>>, tag: &str) -> SliceJob {
    let k1 = index().printers.iter().find(|p| p.name == "Creality K1 (0.4 nozzle)").unwrap();
    let settings = SliceSettings { printer: k1.name.clone(), process: k1.default_process.clone(), filament: k1.default_filament.clone(), overrides: Default::default() };
    SliceJob { config: build_config(&settings).unwrap(), parts, work_dir: std::env::temp_dir().join(format!("pinocchio-slicer-{tag}-{}", std::process::id())) }
}

/// Una pieza que toca la cama en un punto (la primera capa queda vacía sin
/// soportes): con los soportes apagados, se ponen solos y se avisa
#[test]
fn pone_soportes_solo_si_hacen_falta() {
    let Some(engine) = find_engine() else { return };
    let job = k1_job(vec![standing_octahedron(110.0, 110.0, 20.0)], "punta");
    assert_eq!(job.config.get("enable_support").and_then(|v| v.as_str()), Some("0"), "el perfil viene sin soportes");
    let summary = run(&job, &engine, |_| {}, &AtomicBool::new(false)).expect("lamina con soportes");
    assert!(summary.warnings.first().is_some_and(|w| w.starts_with("Se agregaron soportes")), "{:?}", summary.warnings);
    let p = parse_gcode(&std::fs::read_to_string(&summary.gcode).unwrap());
    assert!(p.length_by_feature[Feature::Support as usize] > 0.0, "sin soportes en el gcode");
    let _ = std::fs::remove_dir_all(&job.work_dir);

    // Un cubo no los necesita: no se ponen
    let job = k1_job(vec![cube(100.0, 100.0, 20.0)], "cubo-sin-soportes");
    let summary = run(&job, &engine, |_| {}, &AtomicBool::new(false)).unwrap();
    assert!(!summary.warnings.iter().any(|w| w.starts_with("Se agregaron soportes")), "{:?}", summary.warnings);
    let p = parse_gcode(&std::fs::read_to_string(&summary.gcode).unwrap());
    assert_eq!(p.length_by_feature[Feature::Support as usize], 0.0);
    let _ = std::fs::remove_dir_all(&job.work_dir);
}

/// AD5X: un filamento por carrete (como Orca) → el gcode usa `T0` y lista los 4 carretes;
/// al enviar se asigna `T0` al carrete elegido.
#[test]
fn lamina_un_cubo_para_la_ad5x() {
    let Some(engine) = find_engine() else {
        eprintln!("sin motor: prueba saltada");
        return;
    };
    let p = index().printers.iter().find(|p| p.name.contains("AD5X")).unwrap();
    assert_eq!(p.tools, 4);
    let settings = SliceSettings { printer: p.name.clone(), process: p.default_process.clone(), filament: p.default_filament.clone(), overrides: Default::default() };
    let config = build_config(&settings).unwrap();
    let work_dir = std::env::temp_dir().join(format!("pinocchio-slicer-ad5x-{}", std::process::id()));
    let job = SliceJob { config, parts: vec![cube(100.0, 100.0, 20.0)], work_dir: work_dir.clone() };
    let summary = run(&job, &engine, |_| {}, &AtomicBool::new(false)).unwrap();
    let gcode = std::fs::read_to_string(&summary.gcode).unwrap();
    let _ = std::fs::remove_dir_all(&work_dir);
    let tools: Vec<&str> = gcode.lines().filter(|l| l.len() == 2 && l.starts_with('T')).collect();
    assert_eq!(tools, ["T0"], "una sola herramienta, la T0");
    assert!(gcode.contains("; filament_type = PLA;PLA;PLA;PLA"), "faltan los 4 carretes al final");
    assert!(!gcode.contains("SET_PRINT_STATS_INFO"));
    assert!((95..=105).contains(&summary.layers), "capas {}", summary.layers);
    // La pantalla de la AD5X muestra la pieza con la miniatura de 140×110 que pide su perfil
    assert!(gcode.contains("; thumbnail begin 140x110"), "falta la miniatura de la AD5X");
}
