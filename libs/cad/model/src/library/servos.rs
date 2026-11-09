//! Servos de modelismo y motorreductores chicos (TT amarillo, N20).
//!
//! Servos: origen bajo las orejas, en el centro del cuerpo; Z hacia el eje.
//! Puesto en una placa, las orejas se apoyan en ella y el cuerpo la cruza por
//! su ventana (sus agujeros: la ventana y los de los tornillos).
//!
//! Motorreductores: origen en la cara que se atornilla, en el eje de salida,
//! con el cuerpo en +Z y el eje cruzando la placa en −Z.
//!
//! Medidas: dibujo TowerPro del SG90, Pololu (N20), Adafruit (TT); el MG90S,
//! MG996R y DS3218 no tienen plano oficial y salen de librerías CAD y del
//! dibujo del DS3235 (misma familia): ±1 mm.

use super::{Category, Item, LibraryPart, R, bx, bxc, clearance, cyl, def, depth, fuse, variants};
use crate::standard::Local;
use cad_occt::Shape;

pub(crate) struct Servo {
    /// Cuerpo: largo, ancho y alto hasta la tapa (desde la base)
    l: f64,
    w: f64,
    top: f64,
    /// Orejas: largo total, espesor y altura de su cara de abajo desde la base
    ears: f64,
    ear_t: f64,
    ear_h: f64,
    /// Agujeros: diámetro, separación a lo largo y a lo ancho (0 = uno por oreja)
    hole: f64,
    hole_dx: f64,
    hole_dy: f64,
    /// Eje: distancia desde un extremo del cuerpo; resalte y estriado
    shaft: f64,
    boss_d: f64,
    boss_h: f64,
    spline_d: f64,
    spline_h: f64,
}

const SG90: Servo = Servo { l: 22.2, w: 11.8, top: 22.7, ears: 32.2, ear_t: 2.5, ear_h: 16.0, hole: 2.0, hole_dx: 27.1, hole_dy: 0.0, shaft: 5.9, boss_d: 11.8, boss_h: 4.0, spline_d: 4.6, spline_h: 3.2 };
const MG90S: Servo = Servo { l: 22.9, w: 12.4, top: 24.46, ears: 32.4, ear_t: 2.53, ear_h: 18.31, hole: 2.2, hole_dx: 28.32, hole_dy: 0.0, shaft: 6.19, boss_d: 12.4, boss_h: 4.0, spline_d: 4.7, spline_h: 4.0 };
const MG996R: Servo = Servo { l: 40.3, w: 20.0, top: 36.6, ears: 53.5, ear_t: 2.5, ear_h: 26.0, hole: 4.4, hole_dx: 48.2, hole_dy: 10.0, shaft: 10.0, boss_d: 13.0, boss_h: 2.5, spline_d: 5.8, spline_h: 3.8 };
const DS3218: Servo = Servo { l: 40.0, w: 20.0, top: 35.0, ears: 54.5, ear_t: 2.5, ear_h: 25.2, hole: 4.4, hole_dx: 49.5, hole_dy: 10.0, shaft: 10.0, boss_d: 13.0, boss_h: 1.6, spline_d: 5.9, spline_h: 3.8 };

const SERVOS: &[(&str, &Servo)] = &[("servo_sg90", &SG90), ("servo_mg90s", &MG90S), ("servo_mg996r", &MG996R), ("servo_ds3218", &DS3218)];

fn servo(p: &LibraryPart) -> &'static Servo {
    SERVOS.iter().find(|(k, _)| *k == p.item).map_or(&SG90, |(_, s)| s)
}

/// Centros de los agujeros de las orejas
fn ear_holes(s: &Servo) -> Vec<(f64, f64)> {
    let ys: Vec<f64> = if s.hole_dy > 0.0 { vec![-s.hole_dy / 2.0, s.hole_dy / 2.0] } else { vec![0.0] };
    [-s.hole_dx / 2.0, s.hole_dx / 2.0].iter().flat_map(|&x| ys.iter().map(move |&y| (x, y))).collect()
}

fn build_servo(l: &Local, p: &LibraryPart) -> R<Shape> {
    let s = servo(p);
    let base = -s.ear_h;
    let body = bxc(l, 0.0, 0.0, base, s.l, s.w, s.top)?;
    let mut ears = bxc(l, 0.0, 0.0, 0.0, s.ears, s.w, s.ear_t)?;
    let holes: Vec<Shape> = ear_holes(s).into_iter().map(|(x, y)| cyl(l, x, y, -0.01, s.hole / 2.0, s.ear_t + 0.02)).collect::<R<_>>()?;
    ears = ears.cut(&fuse(holes)?).map_err(super::e)?;
    let x = -s.l / 2.0 + s.shaft;
    let top = base + s.top;
    let boss = cyl(l, x, 0.0, top - 0.01, s.boss_d / 2.0, s.boss_h + 0.01)?;
    let spline = cyl(l, x, 0.0, top + s.boss_h - 0.01, s.spline_d / 2.0, s.spline_h + 0.01)?;
    fuse(vec![body, ears, boss, spline])
}

/// La ventana del cuerpo (con holgura) y los agujeros de los tornillos, hacia −Z.
fn servo_holes(l: &Local, p: &LibraryPart) -> R<Shape> {
    let s = servo(p);
    let c = clearance(p, 0.5);
    let d = depth(p, 10.0);
    let mut parts = vec![bxc(l, 0.0, 0.0, 0.01, s.l + c, s.w + c, -d - 0.01)?];
    for (x, y) in ear_holes(s) {
        // Para que el tornillo autorroscante muerda
        parts.push(cyl(l, x, y, 0.01, s.hole * 0.4, -d - 0.01)?);
    }
    fuse(parts)
}

/// Motorreductor amarillo «TT» (doble eje), según el dibujo de Adafruit.
fn build_tt(l: &Local, _p: &LibraryPart) -> R<Shape> {
    // Caja reductora de 37 (el eje a 14 de su punta) y motor hasta los 70
    let gearbox = bx(l, -14.0, -11.22, 0.0, 37.0, 22.44, 18.6)?;
    let can = bx(l, 23.0, -9.5, -0.2, 33.0, 19.0, 19.0)?;
    // Eje doble D (5,4 con planos a 3,7): 8,7 hacia la placa y 9,3 del otro lado
    let flat = |z0: f64, h: f64| -> R<Shape> { bxc(l, 0.0, 0.0, z0, 5.4, 3.7, h)?.intersect(&cyl(l, 0.0, 0.0, z0, 2.7, h)?).map_err(super::e) };
    let a = flat(-8.7, 8.71)?;
    let b = flat(18.59, 9.31)?;
    let mut body = fuse(vec![gearbox, can, a, b])?;
    for y in [-8.8, 8.8] {
        body = body.cut(&cyl(l, 25.25, y, -0.01, 1.5, 18.62)?).map_err(super::e)?;
    }
    Ok(body)
}

fn tt_holes(l: &Local, p: &LibraryPart) -> R<Shape> {
    let c = clearance(p, 0.4);
    let d = depth(p, 10.0);
    let mut parts = vec![cyl(l, 0.0, 0.0, 0.01, (7.0 + c) / 2.0, -d - 0.01)?];
    for y in [-8.8, 8.8] {
        parts.push(cyl(l, 25.25, y, 0.01, (3.2 + c) / 2.0, -d - 0.01)?);
    }
    fuse(parts)
}

/// Micro motorreductor N20 (Pololu, sin encoder).
fn build_n20(l: &Local, _p: &LibraryPart) -> R<Shape> {
    let gearbox = bxc(l, 0.0, 0.0, 0.0, 12.0, 10.0, 9.0)?;
    // El motor es redondo de 12 con planos arriba y abajo
    let can = cyl(l, 0.0, 0.0, 9.0, 6.0, 15.4)?.intersect(&bxc(l, 0.0, 0.0, 9.0, 12.0, 10.0, 15.4)?).map_err(super::e)?;
    let boss = cyl(l, 0.0, 0.0, 0.01, 2.0, -0.71)?;
    let shaft = cyl(l, 0.0, 0.0, 0.01, 1.5, -10.01)?.cut(&bx(l, 1.0, -2.0, -10.02, 2.0, 4.0, 7.0)?).map_err(super::e)?;
    let mut body = fuse(vec![gearbox, can, boss, shaft])?;
    for x in [-4.5, 4.5] {
        body = body.cut(&cyl(l, x, 0.0, -0.01, 0.65, 2.0)?).map_err(super::e)?;
    }
    Ok(body)
}

fn n20_holes(l: &Local, p: &LibraryPart) -> R<Shape> {
    let c = clearance(p, 0.4);
    let d = depth(p, 10.0);
    let mut parts = vec![cyl(l, 0.0, 0.0, 0.01, (4.0 + c) / 2.0, -d - 0.01)?];
    for x in [-4.5, 4.5] {
        parts.push(cyl(l, x, 0.0, 0.01, (1.8 + c) / 2.0, -d - 0.01)?);
    }
    fuse(parts)
}

pub(crate) fn items() -> Vec<Item> {
    let none = variants(&[]);
    let servo_def = |id: &'static str, name: &'static str, desc: &'static str| def(id, Category::Motors, name, desc, none.clone(), None, "#2b5fa8", build_servo, Some(servo_holes), None);
    vec![
        servo_def("servo_sg90", "Servo SG90", "Micro servo 9 g (dibujo TowerPro); apoyado bajo las orejas: sus agujeros son la ventana y los tornillos"),
        servo_def("servo_mg90s", "Servo MG90S", "Micro servo de engranajes metálicos; medidas de clones (±1 mm)"),
        servo_def("servo_mg996r", "Servo MG996R", "Servo estándar 40×20; orejas y agujeros de clones (±1,5 mm)"),
        servo_def("servo_ds3218", "Servo DS3218 (aprox.)", "Servo 40×20 de 20 kg; sin plano propio: medidas del DS3235, de la misma familia"),
        def("motor_tt", Category::Motors, "Motorreductor TT", "Amarillo de doble eje (3–6 V); apoyado por la cara de los agujeros, un eje cruza la placa", none.clone(), None, "#e6b422", build_tt, Some(tt_holes), None),
        def("motor_n20", Category::Motors, "Micro motorreductor N20", "12×10 de caja metálica (Pololu); apoyado por la cara del eje, con dos M1.6", none, None, "#9aa0a8", build_n20, Some(n20_holes), None),
    ]
}
