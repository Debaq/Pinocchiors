//! Módulos: pantallas (OLED 0,96", LCD 1602), drivers StepStick, sensor
//! HC-SR04, fuentes Mean Well y ventiladores.
//!
//! Pantallas y sensor: origen en el centro de la cara que va contra el panel
//! (por dentro), el módulo hacia +Z; sus agujeros son la ventana en el panel
//! y los de los tornillos. Fuentes y ventiladores: origen en el centro de la
//! cara de apoyo, hacia +Z. Drivers: centro de la cara de abajo del PCB.
//!
//! Medidas: hojas de datos de la OLED y del LCD 1602, Pololu (StepStick),
//! Mean Well LRS, San Ace/Noctua/Sunon (ventiladores). El HC-SR04 no tiene
//! plano: sus transductores son aproximados.

use super::{Category, Item, LibraryPart, R, bx, bxc, clearance, cyl, def, depth, fuse, poly_with_holes, rounded_rect, variant, variants};
use crate::standard::Local;
use cad_occt::Shape;

/// PCB centrado con agujeros (x, y, diámetro) desde z0
fn pcb(l: &Local, w: f64, h: f64, t: f64, holes: &[(f64, f64, f64)], z0: f64) -> R<Shape> {
    let circles: Vec<(f64, f64, f64)> = holes.iter().map(|&(x, y, d)| (x, y, d / 2.0)).collect();
    poly_with_holes(l, &rounded_rect(w, h, 0.5), &[], &circles, z0, t)
}

fn corners(sx: f64, sy: f64, d: f64) -> Vec<(f64, f64, f64)> {
    vec![(-sx / 2.0, -sy / 2.0, d), (sx / 2.0, -sy / 2.0, d), (sx / 2.0, sy / 2.0, d), (-sx / 2.0, sy / 2.0, d)]
}

/// OLED 0,96" I2C: PCB 27,3 con agujeros de 2,5 a 20,7 × 23,2; vidrio 26,7 × 19,26 hacia el panel.
fn oled(l: &Local, _p: &LibraryPart) -> R<Shape> {
    let glass = bxc(l, 0.0, -0.8, 0.0, 26.7, 19.26, 1.5)?;
    let board = pcb(l, 27.3, 27.3, 1.0, &corners(20.7, 23.2, 2.5), 1.5)?;
    // Los 4 pines del lado de arriba, hacia atrás
    let pins = bxc(l, 0.0, 12.0, 2.5, 10.16, 2.54, 8.5)?;
    fuse(vec![glass, board, pins])
}

fn oled_holes(l: &Local, p: &LibraryPart) -> R<Shape> {
    let c = clearance(p, 0.3);
    let d = depth(p, 10.0);
    // La zona visible (23,94 × 12,06) centrada en el área activa
    let mut parts = vec![bxc(l, 0.0, -0.8, 0.01, 23.94 + c, 12.06 + c, -d - 0.01)?];
    for (x, y, _) in corners(20.7, 23.2, 2.5) {
        parts.push(cyl(l, x, y, 0.01, 1.0, -d - 0.01)?);
    }
    fuse(parts)
}

/// LCD 1602: marco 71,2 × 25,2 hacia el panel, PCB 80 × 36 con agujeros de 2,5 a 75 × 31, 13,5 en total.
fn lcd(l: &Local, _p: &LibraryPart) -> R<Shape> {
    let bezel = bxc(l, 0.0, 0.0, 0.0, 71.2, 25.2, 7.0)?;
    let board = pcb(l, 80.0, 36.0, 1.6, &corners(75.0, 31.0, 2.5), 7.0)?;
    // El conector de 16 pines arriba a la izquierda, hacia atrás
    let pins = bx(l, -40.0 + 8.0, 18.0 - 3.8, 8.6, 38.1, 2.54, 4.9)?;
    fuse(vec![bezel, board, pins])
}

fn lcd_holes(l: &Local, p: &LibraryPart) -> R<Shape> {
    let c = clearance(p, 0.4);
    let d = depth(p, 10.0);
    let mut parts = vec![bxc(l, 0.0, 0.0, 0.01, 71.2 + c, 25.2 + c, -d - 0.01)?];
    for (x, y, _) in corners(75.0, 31.0, 2.5) {
        parts.push(cyl(l, x, y, 0.01, 1.0, -d - 0.01)?);
    }
    fuse(parts)
}

/// StepStick (A4988, DRV8825, TMC2209): PCB 15,24 × 20,32, dos filas de 8 pines a 12,7 hacia abajo y disipador.
fn stepstick(l: &Local, _p: &LibraryPart) -> R<Shape> {
    let board = bxc(l, 0.0, 0.0, 0.0, 15.24, 20.32, 1.6)?;
    let rows = fuse(vec![bxc(l, -6.35, 0.0, 0.0, 2.54, 20.32, -8.5)?, bxc(l, 6.35, 0.0, 0.0, 2.54, 20.32, -8.5)?])?;
    let sink = bxc(l, 0.0, 0.0, 1.6, 9.0, 9.0, 5.0)?;
    fuse(vec![board, rows, sink])
}

/// HC-SR04: PCB 45 × 20, dos transductores (aprox. Ø16 a 26) que cruzan el panel.
fn hcsr04(l: &Local, _p: &LibraryPart) -> R<Shape> {
    let board = bxc(l, 0.0, 0.0, 0.0, 45.0, 20.0, 1.6)?;
    let mut parts = vec![board];
    for x in [-13.0, 13.0] {
        parts.push(cyl(l, x, 0.0, 0.01, 8.0, -12.01)?);
    }
    parts.push(bxc(l, 0.0, -8.5, 1.6, 10.16, 2.54, 8.5)?);
    fuse(parts)
}

fn hcsr04_holes(l: &Local, p: &LibraryPart) -> R<Shape> {
    let c = clearance(p, 0.5);
    let d = depth(p, 10.0);
    fuse(vec![cyl(l, -13.0, 0.0, 0.01, (16.0 + c) / 2.0, -d - 0.01)?, cyl(l, 13.0, 0.0, 0.01, (16.0 + c) / 2.0, -d - 0.01)?])
}

/// Fuentes Mean Well: largo, ancho, alto, rosca de los agujeros de abajo y sus centros (desde el centro).
const PSUS: &[(&str, (f64, f64, f64, f64, &[(f64, f64)]))] = &[
    // LRS-150-12: dos M3 a 78 entre sí, a 24 del lado de los bornes y 32 del borde largo
    ("lrs-150-12", (159.0, 97.0, 30.0, 3.0, &[(-79.5 + 24.0, -48.5 + 32.0), (-79.5 + 24.0, -48.5 + 32.0 + 78.0)])),
    // LRS-350-24: cuatro M4 a 150 × 50, a 32,5 de la punta y del borde largo
    ("lrs-350-24", (215.0, 115.0, 30.0, 4.0, &[(-75.0, -25.0), (75.0, -25.0), (75.0, 25.0), (-75.0, 25.0)])),
];

fn psu(l: &Local, p: &LibraryPart) -> R<Shape> {
    let (len, w, h, m, holes) = *variant(PSUS, &p.variant);
    let body = bxc(l, 0.0, 0.0, 0.0, len, w, h)?;
    // Rejilla de ventilación arriba y los agujeros roscados de abajo
    let mut cuts = Vec::new();
    for (x, y) in holes {
        cuts.push(cyl(l, *x, *y, -0.01, m * 0.42, 3.01)?);
    }
    cuts.push(bxc(l, len / 2.0 - 12.0, 0.0, h - 8.0, 14.0, w - 6.0, 8.01)?);
    body.cut(&fuse(cuts)?).map_err(super::e)
}

fn psu_holes(l: &Local, p: &LibraryPart) -> R<Shape> {
    let (.., m, holes) = *variant(PSUS, &p.variant);
    let clear = crate::standard::size_by_nominal(m).map_or(m + 0.4, |s| s.clearance);
    let parts: Vec<Shape> = holes.iter().map(|(x, y)| cyl(l, *x, *y, 0.01, (clear + clearance(p, 0.0)) / 2.0, -depth(p, 10.0) - 0.01)).collect::<R<_>>()?;
    fuse(parts)
}

/// Ventiladores axiales: cuadro, espesor, separación de agujeros y su diámetro.
const FANS: &[(&str, (f64, f64, f64, f64))] = &[("4010", (40.0, 10.0, 32.0, 3.5)), ("3010", (30.0, 10.0, 24.0, 3.2)), ("4020", (40.0, 20.0, 32.0, 3.5)), ("6015", (60.0, 15.0, 50.0, 4.3))];

fn fan(l: &Local, p: &LibraryPart) -> R<Shape> {
    let (s, t, sp, hd) = *variant(FANS, &p.variant);
    let frame = super::poly(l, &rounded_rect(s, s, s * 0.06), 0.0, t)?;
    let mut cuts = vec![cyl(l, 0.0, 0.0, -0.01, s * 0.46, t + 0.02)?];
    for (x, y, d) in corners(sp, sp, hd) {
        cuts.push(cyl(l, x, y, -0.01, d / 2.0, t + 0.02)?);
    }
    let ring = frame.cut(&fuse(cuts)?).map_err(super::e)?;
    // El cubo con las aspas (como un disco más chico)
    let hub = cyl(l, 0.0, 0.0, 0.5, s * 0.22, t - 1.0)?;
    let blades = cyl(l, 0.0, 0.0, t * 0.3, s * 0.44, t * 0.4)?;
    fuse(vec![ring, hub, blades])
}

/// El paso del aire y los 4 agujeros para los tornillos del ventilador (que muerdan).
fn fan_holes(l: &Local, p: &LibraryPart) -> R<Shape> {
    let (s, _, sp, hd) = *variant(FANS, &p.variant);
    let d = depth(p, 10.0);
    let mut parts = vec![cyl(l, 0.0, 0.0, 0.01, s * 0.46 + clearance(p, 0.0) / 2.0, -d - 0.01)?];
    for (x, y, _) in corners(sp, sp, hd) {
        parts.push(cyl(l, x, y, 0.01, hd * 0.42, -d - 0.01)?);
    }
    fuse(parts)
}

pub(crate) fn items() -> Vec<Item> {
    let none = variants(&[]);
    vec![
        def("oled_096", Category::Modules, "Pantalla OLED 0,96\"", "I2C 128×64; por dentro del panel: sus agujeros son la ventana y los tornillos", none.clone(), None, "#1f3c88", oled, Some(oled_holes), None),
        def("lcd_1602", Category::Modules, "Pantalla LCD 1602", "Con PCB de 80×36; por dentro del panel: sus agujeros son la ventana del marco y los tornillos", none.clone(), None, "#2e7d32", lcd, Some(lcd_holes), None),
        def("stepstick", Category::Modules, "Driver StepStick", "A4988, DRV8825 o TMC2209 (15,24×20,32) con disipador", variants(&[("a4988", "A4988"), ("drv8825", "DRV8825"), ("tmc2209", "TMC2209")]), None, "#8b1e3f", stepstick, None, None),
        def("hc_sr04", Category::Modules, "Sensor HC-SR04 (aprox.)", "Ultrasonido 45×20; los transductores cruzan el panel (posición aproximada: sin plano oficial)", none, None, "#1f5fa8", hcsr04, Some(hcsr04_holes), None),
        def("psu_meanwell", Category::Modules, "Fuente Mean Well", "LRS en caja metálica; sus agujeros son los de los tornillos de abajo", variants(&[("lrs-150-12", "LRS-150-12 · 12 V 150 W"), ("lrs-350-24", "LRS-350-24 · 24 V 350 W")]), None, "#b8bcc4", psu, Some(psu_holes), None),
        def("fan_axial", Category::Modules, "Ventilador", "Axial; sus agujeros son el paso del aire y los 4 tornillos", variants(&[("4010", "40×40×10"), ("3010", "30×30×10"), ("4020", "40×40×20"), ("6015", "60×60×15")]), None, "#2a2c30", fan, Some(fan_holes), None),
    ]
}
