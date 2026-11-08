//! Banco de OpenCASCADE: mismas piezas y pasos para comparar versiones (PLAN_OCCT8 O3).
//!
//!   cargo run --release -p cad-occt --example bench_occt [filtro]
//!
//! Cada caso imprime el tiempo y una huella de la geometría (volumen, caras,
//! aristas, válido) para ver si dos versiones dan lo mismo. Un caso que pasa de
//! `LIMITE` segundos se marca COLGADO y el banco sigue con el siguiente.

use cad_occt::{Axis, Curve, Frame, Shape};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const LIMITE: u64 = 120;

fn huella(s: &Shape) -> String {
    let m = s.mass().map(|m| m.volume).unwrap_or(f64::NAN);
    format!("vol={m:.3} caras={} aristas={} valido={}", s.face_count(), s.edge_count(), s.is_valid())
}

fn caso(nombre: &str, f: impl FnOnce() -> Result<String, String> + Send + 'static) {
    // `bench_occt <texto>`: solo los casos cuyo nombre lo contiene
    if let Some(filtro) = std::env::args().nth(1)
        && !nombre.contains(&filtro)
    {
        return;
    }
    let (tx, rx) = mpsc::channel();
    let t0 = Instant::now();
    std::thread::spawn(move || {
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or_else(|_| Err("pánico".into()));
        let _ = tx.send(r);
    });
    match rx.recv_timeout(Duration::from_secs(LIMITE)) {
        Ok(Ok(info)) => println!("{nombre:<34} {:>9.1} ms  {info}", t0.elapsed().as_secs_f64() * 1e3),
        Ok(Err(e)) => println!("{nombre:<34} {:>9.1} ms  ERROR {e}", t0.elapsed().as_secs_f64() * 1e3),
        Err(_) => println!("{nombre:<34} {:>9} ms  COLGADO (>{LIMITE} s)", "-"),
    }
}

fn e(x: cad_occt::Error) -> String {
    x.0
}

fn caja(x: f64, y: f64, z: f64, dx: f64, dy: f64, dz: f64) -> Result<Shape, String> {
    Shape::make_box(Frame::at([x, y, z]), dx, dy, dz).map_err(e)
}

/// Placa 200×200×10 con n×n agujeros de radio 3.
fn placa_perforada(n: usize) -> Result<Shape, String> {
    let placa = caja(0.0, 0.0, 0.0, 200.0, 200.0, 10.0)?;
    let paso = 200.0 / n as f64;
    let mut cil = Vec::new();
    for i in 0..n {
        for j in 0..n {
            let c = [paso * (i as f64 + 0.5), paso * (j as f64 + 0.5), -1.0];
            cil.push(Shape::cylinder(Frame::at(c), 3.0_f64.min(paso * 0.3), 12.0).map_err(e)?);
        }
    }
    let todos = Shape::fuse_all(&cil).map_err(e)?;
    placa.cut(&todos).map_err(e)
}

/// Pieza mecánica: base con nervio, agujeros y redondeos (para HLR, malla, STEP).
fn pieza() -> Result<Shape, String> {
    let base = caja(0.0, 0.0, 0.0, 80.0, 50.0, 12.0)?;
    let nervio = caja(30.0, 0.0, 12.0, 20.0, 50.0, 25.0)?;
    let mut s = base.union(&nervio).map_err(e)?;
    for (x, y) in [(10.0, 10.0), (70.0, 10.0), (10.0, 40.0), (70.0, 40.0)] {
        let c = Shape::cylinder(Frame::at([x, y, -1.0]), 4.0, 14.0).map_err(e)?;
        s = s.cut(&c).map_err(e)?;
    }
    let eje = Shape::cylinder(Frame { origin: [25.0, 25.0, 28.0], z: [1.0, 0.0, 0.0], x: [0.0, 1.0, 0.0] }, 6.0, 30.0)
        .map_err(e)?;
    s = s.cut(&eje).map_err(e)?;
    // Redondear los cantos rectos verticales de la base
    let edges = s.edges().map_err(e)?;
    let sel: Vec<usize> = edges
        .iter()
        .enumerate()
        .filter(|(_, ed)| ed.circle.is_none() && (ed.tangent[2].abs() > 0.99) && ed.mid[2] < 12.0)
        .map(|(i, _)| i)
        .collect();
    s.fillet(&sel, 3.0).map_err(e)
}

fn todas_aristas(s: &Shape) -> Vec<usize> {
    (0..s.edge_count()).collect()
}

fn vistas() -> [([f64; 3], [f64; 3]); 4] {
    [
        ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0]),
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
        ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([1.0, -1.0, 1.0], [1.0, 1.0, 0.0]),
    ]
}

fn main() {
    println!("OpenCASCADE {}", cad_occt::occt_version());
    println!("{:<34} {:>12}  resultado", "caso", "tiempo");

    caso("placa 10x10 agujeros", || placa_perforada(10).map(|s| huella(&s)));
    caso("placa 25x25 agujeros", || placa_perforada(25).map(|s| huella(&s)));
    caso("placa 40 agujeros secuencial", || {
        let mut s = caja(0.0, 0.0, 0.0, 200.0, 100.0, 10.0)?;
        for i in 0..40 {
            let c = Shape::cylinder(Frame::at([5.0 + 4.8 * i as f64, 50.0 + 20.0 * (i as f64).sin(), -1.0]), 2.0, 12.0)
                .map_err(e)?;
            s = s.cut(&c).map_err(e)?;
        }
        Ok(huella(&s))
    });
    caso("booleanas curvas", || {
        let esf = Shape::sphere([0.0; 3], 30.0).map_err(e)?;
        let toro = Shape::torus(Frame::WORLD, 28.0, 8.0).map_err(e)?;
        let cono = Shape::cone(Frame::at([0.0, 0.0, -40.0]), 15.0, 2.0, 80.0).map_err(e)?;
        let s = esf.cut(&toro).map_err(e)?.union(&cono).map_err(e)?;
        let cil = Shape::cylinder(Frame { origin: [-50.0, 0.0, 10.0], z: [1.0, 0.0, 0.0], x: [0.0, 1.0, 0.0] }, 9.0, 100.0)
            .map_err(e)?;
        Ok(huella(&s.cut(&cil).map_err(e)?))
    });
    caso("pieza (booleanas + redondeo)", || pieza().map(|s| huella(&s)));
    caso("redondeo 12 aristas caja", || {
        let b = caja(0.0, 0.0, 0.0, 40.0, 30.0, 20.0)?;
        Ok(huella(&b.fillet(&todas_aristas(&b), 4.0).map_err(e)?))
    });
    caso("redondeo todas aristas pieza", || {
        let p = pieza()?;
        Ok(huella(&p.fillet(&todas_aristas(&p), 0.8).map_err(e)?))
    });
    caso("redondeo agujeros placa 10x10", || {
        let p = placa_perforada(10)?;
        let sel: Vec<usize> = p.edges().map_err(e)?.iter().enumerate().filter(|(_, ed)| ed.circle.is_some()).map(|(i, _)| i).collect();
        Ok(huella(&p.fillet(&sel, 0.8).map_err(e)?))
    });
    caso("redondeo variable", || {
        let b = caja(0.0, 0.0, 0.0, 40.0, 30.0, 20.0)?;
        Ok(huella(&b.fillet_variable(&[0, 1, 2, 3], 1.0, 5.0).map_err(e)?))
    });
    caso("chaflán todas aristas pieza", || {
        let p = pieza()?;
        Ok(huella(&p.chamfer(&todas_aristas(&p), 0.6).map_err(e)?))
    });
    caso("vaciado caja redondeada", || {
        let b = caja(0.0, 0.0, 0.0, 60.0, 40.0, 30.0)?;
        let b = b.fillet(&todas_aristas(&b), 5.0).map_err(e)?;
        let arriba = b.faces().map_err(e)?.iter().position(|f| f.normal[2] > 0.99 && f.center[2] > 29.0).ok_or("sin cara de arriba")?;
        Ok(huella(&b.shell(&[arriba], -2.0).map_err(e)?))
    });
    caso("vaciado pieza", || {
        let p = pieza()?;
        let abajo = p.faces().map_err(e)?.iter().position(|f| f.normal[2] < -0.99).ok_or("sin cara de abajo")?;
        Ok(huella(&p.shell(&[abajo], -1.5).map_err(e)?))
    });
    caso("loft 5 secciones", || {
        let secs: Vec<Shape> = (0..5)
            .map(|i| {
                let z = i as f64 * 20.0;
                let r = 10.0 + 6.0 * (i as f64).sin().abs();
                Shape::wire(&[Curve::Circle { center: [3.0 * i as f64, 0.0, z], normal: [0.0, 0.0, 1.0], radius: r }])
            })
            .collect::<Result<_, _>>()
            .map_err(e)?;
        let l = Shape::loft(&secs, true, false).map_err(e)?;
        let b = caja(-30.0, -30.0, 30.0, 60.0, 60.0, 10.0)?;
        Ok(huella(&l.union(&b).map_err(e)?))
    });
    caso("rosca M10x40", || {
        let t = Shape::thread(Frame::WORLD, 4.2, 5.0, 1.5, 40.0, false).map_err(e)?;
        Ok(huella(&t))
    });
    caso("plano 2D: HLR 4 vistas", || {
        let p = pieza()?;
        let mut n = 0;
        for (eye, x) in vistas() {
            n += p.hlr(eye, x, 0.05).map_err(e)?.len();
        }
        Ok(format!("líneas={n}"))
    });
    caso("plano 2D: HLR placa 10x10", || {
        let p = placa_perforada(10)?;
        let mut n = 0;
        for (eye, x) in vistas() {
            n += p.hlr(eye, x, 0.05).map_err(e)?.len();
        }
        Ok(format!("líneas={n}"))
    });
    caso("malla vista pieza (fina)", || {
        let p = pieza()?;
        let t = p.tessellate(0.01, 0.1).map_err(e)?;
        Ok(format!("triángulos={} vértices={}", t.triangles.len(), t.positions.len()))
    });
    caso("malla vista placa 25x25", || {
        let p = placa_perforada(25)?;
        let t = p.tessellate(0.05, 0.3).map_err(e)?;
        Ok(format!("triángulos={} vértices={}", t.triangles.len(), t.positions.len()))
    });
    caso("masa x200 placa 10x10", || {
        let p = placa_perforada(10)?;
        let mut v = 0.0;
        for _ in 0..200 {
            v = p.mass().map_err(e)?.volume;
        }
        Ok(format!("vol={v:.3}"))
    });
    caso("STEP ida y vuelta pieza", || {
        let p = pieza()?;
        let b = p.to_step().map_err(e)?;
        let q = Shape::from_step(&b).map_err(e)?;
        Ok(format!("bytes={} {}", b.len(), huella(&q)))
    });
    caso("STEP ida y vuelta placa 25x25", || {
        let p = placa_perforada(25)?;
        let b = p.to_step().map_err(e)?;
        let q = Shape::from_step(&b).map_err(e)?;
        Ok(format!("bytes={} {}", b.len(), huella(&q)))
    });
    caso("revolución + booleana", || {
        let perfil = Shape::polygon(&[[5.0, 0.0, 0.0], [20.0, 0.0, 0.0], [20.0, 0.0, 5.0], [12.0, 0.0, 30.0], [5.0, 0.0, 30.0]])
            .map_err(e)?;
        let r = perfil.revolve(Axis { origin: [0.0; 3], dir: [0.0, 0.0, 1.0] }, std::f64::consts::TAU).map_err(e)?;
        let c = caja(-3.0, -40.0, 10.0, 6.0, 80.0, 40.0)?;
        Ok(huella(&r.cut(&c).map_err(e)?))
    });

    // Paralelismo: la misma tarea (pieza + redondeo + HLR + malla) en 1 hilo y en 4.
    let tarea = || -> Result<usize, String> {
        let p = pieza()?;
        let p = p.chamfer(&todas_aristas(&p), 0.4).map_err(e)?;
        let mut n = p.tessellate(0.02, 0.2).map_err(e)?.triangles.len();
        for (eye, x) in vistas() {
            n += p.hlr(eye, x, 0.05).map_err(e)?.len();
        }
        Ok(n)
    };
    caso("tarea x4 en 1 hilo", move || {
        let mut n = 0;
        for _ in 0..4 {
            n += tarea()?;
        }
        Ok(format!("total={n}"))
    });
    caso("tarea x4 en 4 hilos", move || {
        let hs: Vec<_> = (0..4).map(|_| std::thread::spawn(tarea)).collect();
        let mut n = 0;
        for h in hs {
            n += h.join().map_err(|_| "pánico en hilo".to_string())??;
        }
        Ok(format!("total={n}"))
    });
}
