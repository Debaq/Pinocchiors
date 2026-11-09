//! Mide cuánto tarda cada paso del CAD con un proyecto real (p. ej. diseñar
//! sobre un escaneo grande): abrir, rehacer el diseño, la malla del visor, y
//! rehacerlo de nuevo sin cada operación (como al agregarla en la app).
//!
//! `cargo run --release -p pinocchio-app --example cad_perfil -- proyecto.pinocchio`

use std::time::Instant;

use pinocchio_app_lib::AppState;
use pinocchio_app_lib::cad::bridge::{Reply, dispatch};
use serde_json::{Value, json};

fn paso(state: &AppState, nombre: &str, cmd: &str, args: Value) -> Option<Value> {
    let t = Instant::now();
    let r = dispatch(state, cmd, &args);
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    match r {
        Ok(Reply::Json(v)) => {
            // Del resultado de un recálculo: piezas, operaciones recalculadas y errores
            let extra = match v.get("status") {
                Some(st) => {
                    let errs: Vec<String> = st.as_array().into_iter().flatten().filter_map(|f| f.get("message").and_then(|e| e.as_str()).map(|e| format!("#{} {}: {}", f["id"], f["state"].as_str().unwrap_or(""), e.chars().take(160).collect::<String>()))).collect();
                    let ms: Vec<String> = st.as_array().into_iter().flatten().map(|f| format!("{:.0}", f["ms"].as_f64().unwrap_or(0.0))).collect();
                    format!("  piezas={} recalculadas={} ms por operación={ms:?} avisos={errs:?}", v["parts"].as_array().map_or(0, |a| a.len()), v["recomputed"])
                }
                None => String::new(),
            };
            println!("{nombre:<46} {ms:>9.1} ms{extra}");
            Some(v)
        }
        Ok(Reply::Bytes(b)) => {
            println!("{nombre:<46} {ms:>9.1} ms  ({:.1} MB)", b.len() as f64 / 1e6);
            None
        }
        Err(e) => {
            println!("{nombre:<46} {ms:>9.1} ms  ERROR {e}");
            None
        }
    }
}

fn main() {
    let path = std::env::args().nth(1).expect("falta el proyecto");
    println!("OpenCASCADE {}", cad_model_version());
    let state = AppState::new();
    let t = Instant::now();
    pinocchio_app_lib::project::open_project_impl(&state, &path).expect("abrir");
    println!("{:<46} {:>9.1} ms", "abrir proyecto", t.elapsed().as_secs_f64() * 1000.0);

    let mut doc = paso(&state, "documento", "cad_get_document", json!({})).unwrap_or(Value::Null);
    // CAD_PERFIL_LARGOS="20,15": otros valores para las cotas de largo de los sketches (en orden)
    if let Ok(v) = std::env::var("CAD_PERFIL_LARGOS") {
        let mut vals = v.split(',').filter_map(|x| x.trim().parse::<f64>().ok());
        for f in doc["features"].as_array_mut().into_iter().flatten() {
            if f["kind"]["type"] == "relief" {
                f["kind"]["regions"] = json!({ "type": "all" });
            }
            if f["kind"]["type"] != "sketch" {
                continue;
            }
            for c in f["kind"]["sketch"]["constraints"].as_array_mut().into_iter().flatten() {
                if c["type"] == "length"
                    && let Some(x) = vals.next()
                {
                    c["value"] = json!(x);
                }
            }
        }
        paso(&state, "cotas cambiadas", "cad_set_document", json!({ "document": doc.clone() }));
    }
    let features = doc["features"].as_array().cloned().unwrap_or_default();
    println!("operaciones: {}", features.iter().map(|f| f["kind"]["type"].as_str().unwrap_or("?").to_string()).collect::<Vec<_>>().join(", "));

    // Definición de cada operación (sin los arreglos grandes)
    for f in &features {
        let mut k = f["kind"].clone();
        if let Some(o) = k.as_object_mut() {
            o.retain(|_, v| !(v.is_array() && v.as_array().is_some_and(|a| a.len() > 12)) && !v.get("hash").is_some());
        }
        let txt = k.to_string();
        println!("  #{} {}", f["id"], txt.chars().take(400).collect::<String>());
    }
    let r = paso(&state, "rehacer el diseño (primera vez)", "cad_evaluate", json!({}));
    if let Some(r) = &r {
        for pv in r["parts"].as_array().into_iter().flatten() {
            println!("  pieza {} vol={:.1} área={:.1} caras={:?} centro={}", pv["name"], pv["volume"].as_f64().unwrap_or(0.0), pv["area"].as_f64().unwrap_or(0.0), pv["faces"], pv["center"]);
        }
        println!("  cuerpo {}", r["body"].to_string().chars().take(300).collect::<String>());
        for rf in r["references"].as_array().into_iter().flatten() {
            println!("  referencia {}", rf.to_string().chars().take(300).collect::<String>());
        }
    }
    paso(&state, "malla del visor", "cad_mesh", json!({}));
    paso(&state, "rehacer el diseño (otra vez, sin cambios)", "cad_evaluate", json!({}));
    paso(&state, "malla del visor (otra vez)", "cad_mesh", json!({}));

    // Como la app al agregar cada operación: el documento hasta ella
    for n in 1..=features.len() {
        let mut d = doc.clone();
        d["features"] = Value::Array(features[..n].to_vec());
        let kind = features[n - 1]["kind"]["type"].as_str().unwrap_or("?").to_string();
        paso(&state, &format!("agregar #{n} {kind}: rehacer"), "cad_set_document", json!({ "document": d }));
        paso(&state, &format!("agregar #{n} {kind}: malla"), "cad_mesh", json!({}));
    }
}

fn cad_model_version() -> String {
    cad_model::occt::occt_version()
}
