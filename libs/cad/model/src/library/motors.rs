//! Motores paso a paso NEMA (ICS 16).
//!
//! Marco: origen en el centro de la cara delantera (la del eje), Z hacia el
//! cuerpo. Apoyado en la cara de atrás de una placa, el cuerpo queda afuera y
//! el resalte y el eje cruzan la placa (−Z).

use super::{Category, Item, LibraryPart, R, bx, chamfered_rect, clearance, cyl, def, depth, fuse, poly, variant, variants};
use crate::standard::Local;
use cad_occt::Shape;

/// Medidas de un motor paso a paso (mm).
pub(crate) struct Nema {
    /// Lado de la cara y chaflán de las esquinas del cuerpo
    face: f64,
    corner: f64,
    /// Separación entre agujeros (centro a centro, en cuadrado)
    holes: f64,
    /// Rosca de los agujeros (M) y su profundidad; con `through`, pasantes en la brida
    screw: f64,
    screw_depth: f64,
    through: Option<Through>,
    /// Resalte centrador
    boss_d: f64,
    boss_h: f64,
    /// Eje: diámetro, largo desde la cara y plano en D (medida sobre el plano; 0 = redondo) y su largo
    shaft_d: f64,
    shaft_len: f64,
    flat: f64,
    flat_len: f64,
    /// Largos de cuerpo de venta (id de variante = largo)
    lengths: &'static [(&'static str, f64)],
}

/// Agujeros pasantes en la brida delantera (NEMA 23 y 34).
pub(crate) struct Through {
    d: f64,
    flange: f64,
}

// Medidas de los dibujos de Moons', StepperOnline/OMC, Novanta y Pololu (los
// que siguen NEMA ICS 16). El chaflán de las esquinas y la brida de los 23/34
// no están acotados en ninguno: son estimados del dibujo.
const NEMA8: Nema = Nema {
    face: 20.3,
    corner: 2.0,
    holes: 16.0,
    screw: 2.0,
    screw_depth: 3.0,
    through: None,
    boss_d: 15.0,
    boss_h: 1.5,
    shaft_d: 4.0,
    shaft_len: 10.0,
    flat: 3.5,
    flat_len: 7.0,
    lengths: &[("38", 38.0), ("28", 28.0), ("42", 42.0)],
};
const NEMA11: Nema = Nema {
    face: 28.3,
    corner: 2.5,
    holes: 23.0,
    screw: 2.5,
    screw_depth: 3.0,
    through: None,
    boss_d: 22.0,
    boss_h: 2.0,
    shaft_d: 5.0,
    shaft_len: 24.0,
    flat: 4.5,
    flat_len: 15.0,
    lengths: &[("32", 32.0), ("41", 41.0), ("45", 45.0), ("51", 51.0)],
};
const NEMA14: Nema = Nema {
    face: 35.3,
    corner: 4.0,
    holes: 26.0,
    screw: 3.0,
    screw_depth: 3.0,
    through: None,
    boss_d: 22.0,
    boss_h: 2.0,
    shaft_d: 5.0,
    shaft_len: 24.0,
    flat: 4.5,
    flat_len: 15.0,
    lengths: &[("34", 34.0), ("28", 28.0), ("36", 36.0), ("42", 42.0), ("55", 55.5)],
};
const NEMA17: Nema = Nema {
    face: 42.3,
    corner: 4.5,
    holes: 31.0,
    screw: 3.0,
    screw_depth: 4.5,
    through: None,
    boss_d: 22.0,
    boss_h: 2.0,
    shaft_d: 5.0,
    shaft_len: 24.0,
    flat: 4.5,
    flat_len: 15.0,
    lengths: &[("40", 40.0), ("20", 20.0), ("34", 34.0), ("48", 48.0), ("60", 60.0)],
};
const NEMA23: Nema = Nema {
    face: 56.4,
    corner: 3.0,
    holes: 47.14,
    screw: 5.0,
    screw_depth: 0.0,
    through: Some(Through { d: 5.1, flange: 5.0 }),
    boss_d: 38.1,
    boss_h: 1.6,
    shaft_d: 6.35,
    shaft_len: 21.0,
    flat: 5.85,
    flat_len: 15.0,
    lengths: &[("56", 56.0), ("45", 45.0), ("76", 76.0), ("112", 112.0)],
};
const NEMA34: Nema = Nema {
    face: 86.0,
    corner: 5.0,
    holes: 69.6,
    screw: 6.0,
    screw_depth: 0.0,
    through: Some(Through { d: 6.5, flange: 10.0 }),
    boss_d: 73.0,
    boss_h: 2.0,
    shaft_d: 12.7,
    shaft_len: 31.75,
    flat: 0.0,
    flat_len: 22.0,
    lengths: &[("80", 80.0), ("67", 67.5), ("97", 97.0), ("126", 126.5)],
};

const TABLE: &[(&str, &Nema)] = &[("nema8", &NEMA8), ("nema11", &NEMA11), ("nema14", &NEMA14), ("nema17", &NEMA17), ("nema23", &NEMA23), ("nema34", &NEMA34)];

fn nema(p: &LibraryPart) -> &'static Nema {
    *variant(TABLE, &p.item)
}

fn body_length(m: &Nema, p: &LibraryPart) -> f64 {
    *variant(m.lengths, &p.variant)
}

fn build(l: &Local, p: &LibraryPart) -> R<Shape> {
    let m = nema(p);
    let len = body_length(m, p);
    let mut body = poly(l, &chamfered_rect(m.face, m.face, m.corner), 0.0, len)?;
    // Agujeros de montaje: roscados en el cuerpo o pasantes en la brida (con el cuerpo más angosto detrás)
    let h = m.holes / 2.0;
    let mut cuts = Vec::new();
    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        match &m.through {
            Some(t) => cuts.push(cyl(l, sx * h, sy * h, -0.01, t.d / 2.0, t.flange + 0.02)?),
            None => cuts.push(cyl(l, sx * h, sy * h, -0.01, m.screw * 0.42, m.screw_depth + 0.01)?),
        }
    }
    if let Some(t) = &m.through {
        // Detrás de la brida el cuerpo deja libres las cabezas de los tornillos
        let pocket = m.face - m.holes + 2.0;
        for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            cuts.push(bx(l, sx * (m.face / 2.0 + 0.01), sy * (m.face / 2.0 + 0.01), t.flange, -sx * pocket, -sy * pocket, len)?);
        }
    }
    body = body.cut(&fuse(cuts)?).map_err(super::e)?;
    let boss = cyl(l, 0.0, 0.0, 0.0, m.boss_d / 2.0, -m.boss_h)?;
    let mut shaft = cyl(l, 0.0, 0.0, 0.0, m.shaft_d / 2.0, -m.shaft_len)?;
    if m.flat > 0.0 {
        // Plano en D desde la punta
        let flat_len = m.flat_len.min(m.shaft_len);
        let cut = bx(l, m.flat - m.shaft_d / 2.0, -m.shaft_d, -m.shaft_len - 0.01, m.shaft_d, 2.0 * m.shaft_d, flat_len + 0.01)?;
        shaft = shaft.cut(&cut).map_err(super::e)?;
    }
    fuse(vec![body, boss, shaft])
}

/// Paso del resalte (con holgura) y agujeros pasantes de los tornillos, hacia −Z.
fn holes(l: &Local, p: &LibraryPart) -> R<Shape> {
    let m = nema(p);
    let c = clearance(p, 0.4);
    let d = depth(p, 20.0);
    let h = m.holes / 2.0;
    let screw = crate::standard::size_by_nominal(m.screw).map_or(m.screw + 0.4, |s| s.clearance);
    let mut parts = vec![cyl(l, 0.0, 0.0, 0.01, (m.boss_d + c) / 2.0, -d - 0.01)?];
    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        parts.push(cyl(l, sx * h, sy * h, 0.01, (screw + c) / 2.0, -d - 0.01)?);
    }
    fuse(parts)
}

pub(crate) fn items() -> Vec<Item> {
    let nema_item = |id: &'static str, name: &'static str, m: &Nema| {
        let list: Vec<(&str, String)> = m.lengths.iter().map(|(k, v)| (*k, format!("{v} mm"))).collect();
        let list: Vec<(&str, &str)> = list.iter().map(|(k, v)| (*k, v.as_str())).collect();
        def(
            id,
            Category::Motors,
            name,
            "Motor paso a paso: apoyado por la cara del eje; el resalte y el eje cruzan la placa",
            variants(&list),
            None,
            "#3a3d42",
            build,
            Some(holes),
            None,
        )
    };
    let mut out = vec![
        nema_item("nema8", "NEMA 8", &NEMA8),
        nema_item("nema11", "NEMA 11", &NEMA11),
        nema_item("nema14", "NEMA 14", &NEMA14),
        nema_item("nema17", "NEMA 17", &NEMA17),
        nema_item("nema23", "NEMA 23", &NEMA23),
        nema_item("nema34", "NEMA 34", &NEMA34),
    ];
    out.extend(super::servos::items());
    out
}
