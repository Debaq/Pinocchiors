//! Banco de sketches grandes (plan urgente del sketch, U4): tiempos de
//! resolver, arrastrar, regiones y ayudas con 1000+ entidades.
//!
//! cargo test --release -p cad-model --test sketch_bench -- --ignored --nocapture
use std::time::Instant;

use cad_model::*;

/// Placa: k×k celdas, cada una un rectángulo (horizontales y verticales) con
/// un agujero redondo con diámetro. 5 entidades por celda.
fn plate(k: usize) -> Sketch {
    let mut s = Sketch::new();
    for i in 0..k {
        for j in 0..k {
            let (x, y) = (i as f64 * 12.0, j as f64 * 12.0);
            s.rectangle([x, y], [x + 10.0, y + 10.0]);
            let c = s.circle([x + 5.0, y + 5.0], 3.0);
            s.constrain(SketchConstraint::Diameter { entity: c, value: 6.0, reference: false, opts: Default::default() });
        }
    }
    s
}

/// Rejilla de líneas que se cruzan todas (como un DXF de cuadrícula).
fn grid(n: usize) -> Sketch {
    let mut s = Sketch::new();
    let size = n as f64 * 10.0;
    for i in 0..n {
        let t = i as f64 * 10.0 + 5.0;
        s.line([0.0, t], [size, t]);
        s.line([t, 0.0], [t, size]);
    }
    s
}

/// Texto largo importado: muchos contornos cerrados de polilínea sin restricciones.
fn text(letters: usize, sides: usize) -> Sketch {
    let mut s = Sketch::new();
    for l in 0..letters {
        let (cx, cy) = ((l % 40) as f64 * 8.0, (l / 40) as f64 * 12.0);
        let pts: Vec<[f64; 2]> = (0..sides)
            .map(|i| {
                let a = i as f64 / sides as f64 * std::f64::consts::TAU;
                let r = 3.0 + 0.8 * (a * 3.0).sin();
                [cx + r * a.cos(), cy + r * a.sin()]
            })
            .collect();
        s.polyline(&pts);
    }
    s
}

fn time<T>(label: &str, f: impl FnOnce() -> T) -> T {
    let t = Instant::now();
    let out = f();
    println!("  {label:<14} {:>9.1} ms", t.elapsed().as_secs_f64() * 1e3);
    out
}

fn run(name: &str, mut s: Sketch, assist: bool) {
    println!("{name}: {} entidades, {} puntos, {} restricciones", s.entities.len(), s.points.len(), s.constraints.len());
    let regions = time("regiones", || find_regions(&s).unwrap());
    println!("  ({} regiones)", regions.len());
    time("resolver", || s.solve().unwrap());
    let p = s.points[s.points.len() / 2].id;
    let at = s.point(p).unwrap();
    let r = time("arrastrar", || s.solve_drag(p, [at[0] + 0.5, at[1] + 0.25]).unwrap());
    println!("  (rápido: {})", r.quick);
    // Lo que viaja por IPC en cada arrastre: el sketch (ida y vuelta) y las regiones
    let regions = find_regions(&s).unwrap();
    let json = time("JSON", || serde_json::to_string(&s).unwrap().len() + serde_json::to_string(&regions).unwrap().len());
    let back = time("JSON leer", || serde_json::from_str::<Sketch>(&serde_json::to_string(&s).unwrap()).unwrap());
    assert_eq!(back.entities.len(), s.entities.len());
    println!("  ({} KB)", json / 1024);
    if assist {
        let sug = time("sugerir", || s.suggest().unwrap());
        println!("  ({} sugeridas)", sug.len());
        let def = time("definir", || s.auto_define(true).unwrap());
        println!("  ({} para definir)", def.len());
    }
}

#[test]
#[ignore]
fn bench_large_sketches() {
    run("placa 15×15", plate(15), false);
    run("rejilla 60+60", grid(60), false);
    run("texto 300×20", text(300, 20), false);
}

#[test]
#[ignore]
fn bench_assist() {
    run("placa 4×4", plate(4), true);
    run("placa 8×8", plate(8), true);
    run("placa 15×15", plate(15), true);
}
