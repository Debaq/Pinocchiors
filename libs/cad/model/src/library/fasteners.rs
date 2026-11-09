//! Tornillería métrica para el ensamble (las medidas de [`crate::standard`]):
//! tornillos Allen, avellanados y hexagonales, tuercas, arandelas e insertos
//! térmicos.
//!
//! Marco como en las piezas estándar: origen en la cara de apoyo, Z saliendo
//! del material. La cabeza del tornillo queda en +Z y la caña entra en −Z; la
//! tuerca y la arandela crecen en +Z; el inserto entra entero en −Z.

use super::{Category, Item, LengthSpec, LibraryPart, R, clearance, cyl, def, depth, fuse, length_of, poly, variant, variants};
use crate::feature::BoltHead;
use crate::standard::{self, Local};
use cad_occt::{Frame, Shape};

const SIZES: &[(&str, &str)] = &[("M3", "M3"), ("M2", "M2"), ("M2.5", "M2.5"), ("M4", "M4"), ("M5", "M5"), ("M6", "M6"), ("M8", "M8")];

fn metric(p: &LibraryPart) -> R<&'static standard::MetricSize> {
    let name = if p.variant.is_empty() { "M3" } else { p.variant.as_str() };
    standard::size(name).ok_or_else(|| format!("No conozco la medida {name}"))
}

fn head_of(item: &str) -> BoltHead {
    match item {
        "bolt_countersunk" => BoltHead::Countersunk,
        "bolt_hex" => BoltHead::Hex,
        _ => BoltHead::Socket,
    }
}

fn bolt_length(p: &LibraryPart) -> f64 {
    super::item(&p.item).map_or(10.0, |it| length_of(&it.meta, p))
}

fn bolt(l: &Local, p: &LibraryPart) -> R<Shape> {
    let m = metric(p)?;
    standard::bolt(frame(l), m.name, bolt_length(p), head_of(&p.item), false)
}

/// Pasante (ISO 273) del largo de la caña; el avellanado lleva su asiento.
fn bolt_holes(l: &Local, p: &LibraryPart) -> R<Shape> {
    let m = metric(p)?;
    let c = clearance(p, 0.2);
    let len = depth(p, bolt_length(p));
    let hole = cyl(l, 0.0, 0.0, 0.01, (m.clearance + c) / 2.0, -len - 0.01)?;
    if head_of(&p.item) != BoltHead::Countersunk {
        return Ok(hole);
    }
    let k = m.csk_k();
    let seat = Shape::cone(Frame { origin: l.p(0.0, 0.0, -k), z: l.z, x: l.x }, (m.clearance + c) / 2.0, (m.csk_dk + c) / 2.0 + 0.01, k + 0.01).map_err(super::e)?;
    hole.union(&seat).map_err(super::e)
}

fn nut(l: &Local, p: &LibraryPart) -> R<Shape> {
    standard::nut(frame(l), metric(p)?.name, false)
}

/// Trampa de tuerca: el hexágono (con holgura) de su alto hacia −Z y el pasante del tornillo más abajo.
fn nut_holes(l: &Local, p: &LibraryPart) -> R<Shape> {
    let m = metric(p)?;
    let c = clearance(p, 0.3);
    let r = (m.hex_s + c) / 3f64.sqrt();
    let hex: Vec<(f64, f64)> = (0..6).map(|i| (30.0 + 60.0 * i as f64).to_radians()).map(|a| (r * a.cos(), r * a.sin())).collect();
    let trap = poly(l, &hex, 0.01, -(m.nut_m + 0.2) - 0.01)?;
    let bolt = cyl(l, 0.0, 0.0, 0.01, (m.clearance + 0.2) / 2.0, -depth(p, m.nut_m + 10.0) - 0.01)?;
    fuse(vec![trap, bolt])
}

fn washer(l: &Local, p: &LibraryPart) -> R<Shape> {
    standard::washer(frame(l), metric(p)?.name)
}

fn washer_holes(l: &Local, p: &LibraryPart) -> R<Shape> {
    let m = metric(p)?;
    cyl(l, 0.0, 0.0, 0.01, (m.clearance + clearance(p, 0.2)) / 2.0, -depth(p, 10.0) - 0.01)
}

/// Insertos térmicos: exterior, largo y agujero recomendado para imprimir.
pub(crate) const INSERTS: &[(&str, (f64, f64, f64))] = &[
    ("M3", (4.6, 5.7, 4.0)),
    ("M2", (3.6, 4.0, 3.2)),
    ("M2.5", (3.6, 5.7, 3.6)),
    ("M4", (5.6, 8.1, 5.6)),
    ("M5", (6.4, 9.5, 6.4)),
];

fn insert(l: &Local, p: &LibraryPart) -> R<Shape> {
    let (d, len, _) = *variant(INSERTS, &p.variant);
    let m = standard::size(if p.variant.is_empty() { "M3" } else { &p.variant }).map_or(3.0, |m| m.d);
    let body = cyl(l, 0.0, 0.0, 0.0, d / 2.0, -len)?;
    body.cut(&cyl(l, 0.0, 0.0, 0.01, m * 0.42, -len - 0.02)?).map_err(super::e)
}

/// El agujero donde se funde (un poco más hondo que el inserto).
fn insert_holes(l: &Local, p: &LibraryPart) -> R<Shape> {
    let (_, len, hole) = *variant(INSERTS, &p.variant);
    cyl(l, 0.0, 0.0, 0.01, (hole + p.clearance) / 2.0, -depth(p, len + 1.0) - 0.01)
}

fn frame(l: &Local) -> Frame {
    Frame { origin: l.o, z: l.z, x: l.x }
}

pub(crate) fn items() -> Vec<Item> {
    let lengths = Some(LengthSpec { default: 10.0, min: 3.0, max: 120.0 });
    let sizes = variants(SIZES);
    vec![
        def("bolt_socket", Category::Fasteners, "Tornillo Allen", "ISO 4762, cabeza cilíndrica; apoyado por debajo de la cabeza", sizes.clone(), lengths, "#5a5e66", bolt, Some(bolt_holes), None),
        def("bolt_countersunk", Category::Fasteners, "Tornillo avellanado", "ISO 10642; la cara de la cabeza a ras con la cara elegida", sizes.clone(), lengths, "#5a5e66", bolt, Some(bolt_holes), None),
        def("bolt_hex", Category::Fasteners, "Tornillo hexagonal", "ISO 4017; apoyado por debajo de la cabeza", sizes.clone(), lengths, "#5a5e66", bolt, Some(bolt_holes), None),
        def("nut_hex", Category::Fasteners, "Tuerca hexagonal", "ISO 4032; sus agujeros son la trampa de tuerca", sizes.clone(), None, "#5a5e66", nut, Some(nut_holes), None),
        def("washer_flat", Category::Fasteners, "Arandela", "ISO 7089", sizes, None, "#8a8e96", washer, Some(washer_holes), None),
        def(
            "heat_insert",
            Category::Fasteners,
            "Inserto térmico",
            "Inserto de latón para fundir en el plástico; su agujero es el recomendado para imprimir",
            variants(&INSERTS.iter().map(|(k, _)| (*k, *k)).collect::<Vec<_>>()),
            None,
            "#c9a24a",
            insert,
            Some(insert_holes),
            None,
        ),
    ]
}
