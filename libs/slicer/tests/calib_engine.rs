//! Lamina cada prueba de calibración con el motor real. Se salta si no está compilado.

use pinocchio_slicer::calib::{prepare, CalibRange, CalibTest};
use pinocchio_slicer::{build_config, find_engine, index, run, SliceJob, SliceSettings};
use std::sync::atomic::AtomicBool;

fn config(printer: &str) -> serde_json::Map<String, serde_json::Value> {
    let p = index().printers.iter().find(|p| p.name == printer).unwrap();
    build_config(&SliceSettings { printer: p.name.clone(), process: p.default_process.clone(), filament: p.default_filament.clone(), overrides: Default::default() }).unwrap()
}

fn slice(test: CalibTest, range: CalibRange, printer: &str) -> Option<(String, pinocchio_slicer::Summary)> {
    let engine = find_engine()?;
    let t = prepare(test, &range, &config(printer)).unwrap();
    let work_dir = std::env::temp_dir().join(format!("pinocchio-calib-{test:?}-{}-{}", printer.split_whitespace().next().unwrap_or(""), std::process::id()));
    let job = SliceJob { config: t.config, parts: t.parts, work_dir: work_dir.clone(), calib: Some(t.calib) };
    let summary = run(&job, &engine, |_| {}, &AtomicBool::new(false)).unwrap_or_else(|e| panic!("{test:?}: {e}"));
    let gcode = std::fs::read_to_string(&summary.gcode).unwrap();
    std::fs::remove_dir_all(&work_dir).ok();
    Some((gcode, summary))
}

const K1: &str = "Creality K1 (0.4 nozzle)";

#[test]
fn torre_de_temperatura_baja_5_grados_por_bloque() {
    let Some((gcode, s)) = slice(CalibTest::Temperature, CalibRange { start: Some(230.0), end: Some(210.0), ..Default::default() }, K1) else { return };
    // 5 bloques de 10 mm a 0,2 mm de capa
    assert!((240..=260).contains(&s.layers), "capas {}", s.layers);
    for t in [230, 225, 220, 215, 210] {
        assert!(gcode.contains(&format!("M104 S{t}")), "falta M104 S{t}");
    }
}

#[test]
fn torre_de_pa_y_de_retraccion() {
    let Some((pa, s)) = slice(CalibTest::PressureAdvance, CalibRange { start: Some(0.0), end: Some(0.04), step: Some(0.002), ..Default::default() }, K1) else { return };
    // 21 mm de torre
    assert!(s.layers >= 100 && s.layers <= 110, "capas {}", s.layers);
    assert!(pa.contains("SET_PRESSURE_ADVANCE ADVANCE=0.04") || pa.contains("SET_PRESSURE_ADVANCE ADVANCE=0.040"), "falta el PA final");
    let Some((r, _)) = slice(CalibTest::Retraction, CalibRange { start: Some(0.0), end: Some(1.0), step: Some(0.1), ..Default::default() }, K1) else { return };
    assert!(r.contains("Calib_Retraction_tower"), "la torre de retracción no cambia la retracción");
}

#[test]
fn flujo_lamina_las_once_piezas() {
    let Some((gcode, _)) = slice(CalibTest::FlowCoarse, CalibRange::default(), K1) else { return };
    assert_eq!(gcode.matches("EXCLUDE_OBJECT_DEFINE").count(), 11);
}

#[test]
fn las_demas_laminan() {
    for test in [CalibTest::FirstLayer, CalibTest::FlowFine, CalibTest::VolumetricSpeed, CalibTest::Vfa, CalibTest::ShaperFrequency, CalibTest::ShaperDamping, CalibTest::Cornering] {
        let Some((gcode, s)) = slice(test, CalibRange::default(), K1) else { return };
        assert!(s.layers > 0 && !gcode.is_empty(), "{test:?}");
        if test == CalibTest::FirstLayer {
            assert_eq!(s.layers, 1, "la primera capa es una sola capa");
        }
        if test == CalibTest::ShaperFrequency {
            assert!(gcode.contains("SET_INPUT_SHAPER"), "sin cambios de input shaping");
        }
    }
}

#[test]
fn la_ad5x_tambien() {
    let Some((gcode, _)) = slice(CalibTest::Temperature, CalibRange { start: Some(220.0), end: Some(200.0), ..Default::default() }, "Flashforge AD5X 0.4 nozzle") else { return };
    assert!(gcode.contains("M104 S205"));
}
