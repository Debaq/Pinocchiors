//! Imanes de neodimio: discos, aros avellanados y bloques.
//!
//! Marco: origen en el centro de la cara de arriba, el imán hacia −Z. Puesto
//! en una cara queda embutido a ras: sus agujeros son el bolsillo donde entra.

use super::{Category, Item, LibraryPart, R, bxc, clearance, cyl, def, depth, variant, variants};
use crate::standard::Local;
use cad_occt::{Frame, Shape};

/// Discos: diámetro × alto.
const DISCS: &[(&str, (f64, f64))] = &[
    ("10x3", (10.0, 3.0)),
    ("3x1", (3.0, 1.0)),
    ("5x2", (5.0, 2.0)),
    ("6x2", (6.0, 2.0)),
    ("6x3", (6.0, 3.0)),
    ("8x2", (8.0, 2.0)),
    ("8x3", (8.0, 3.0)),
    ("10x2", (10.0, 2.0)),
    ("10x5", (10.0, 5.0)),
    ("12x3", (12.0, 3.0)),
    ("15x3", (15.0, 3.0)),
    ("20x3", (20.0, 3.0)),
    ("20x5", (20.0, 5.0)),
];

/// Aros avellanados a 90°: exterior, alto, agujero, avellanado (diámetro en la cara).
const RINGS: &[(&str, (f64, f64, f64, f64))] = &[
    // supermagnete CS-S-12-04 y CS-S-15-04 (para M3); el 10×3 de una sola tienda
    ("12x4-m3", (12.0, 4.0, 4.5, 9.46)),
    ("15x4-m3", (15.0, 4.0, 4.5, 9.46)),
    ("10x3-m3", (10.0, 3.0, 3.0, 6.7)),
];

/// Bloques: largo × ancho × alto.
const BLOCKS: &[(&str, (f64, f64, f64))] = &[("10x5x2", (10.0, 5.0, 2.0)), ("20x10x2", (20.0, 10.0, 2.0)), ("20x10x5", (20.0, 10.0, 5.0))];

/// Holgura de fábrica del bolsillo en el diámetro (a presión en PLA/PETG)
const POCKET: f64 = 0.2;

fn disc(l: &Local, p: &LibraryPart) -> R<Shape> {
    let (d, h) = *variant(DISCS, &p.variant);
    cyl(l, 0.0, 0.0, 0.0, d / 2.0, -h)
}

fn disc_holes(l: &Local, p: &LibraryPart) -> R<Shape> {
    let (d, h) = *variant(DISCS, &p.variant);
    cyl(l, 0.0, 0.0, 0.01, (d + clearance(p, POCKET)) / 2.0, -depth(p, h + 0.2) - 0.01)
}

fn ring(l: &Local, p: &LibraryPart) -> R<Shape> {
    let (d, h, hole, csk) = *variant(RINGS, &p.variant);
    let body = cyl(l, 0.0, 0.0, 0.0, d / 2.0, -h)?;
    // Avellanado a 90° desde la cara de arriba hasta el agujero
    let k = ((csk - hole) / 2.0).min(h);
    let seat = Shape::cone(Frame { origin: l.p(0.0, 0.0, -k), z: l.z, x: l.x }, hole / 2.0, csk / 2.0 + 0.01, k + 0.01).map_err(super::e)?;
    let bore = cyl(l, 0.0, 0.0, 0.01, hole / 2.0, -h - 0.02)?;
    body.cut(&seat.fuse_with(&bore)?).map_err(super::e)
}

fn ring_holes(l: &Local, p: &LibraryPart) -> R<Shape> {
    let (d, h, _, _) = *variant(RINGS, &p.variant);
    cyl(l, 0.0, 0.0, 0.01, (d + clearance(p, POCKET)) / 2.0, -depth(p, h + 0.2) - 0.01)
}

fn block(l: &Local, p: &LibraryPart) -> R<Shape> {
    let (a, b, h) = *variant(BLOCKS, &p.variant);
    bxc(l, 0.0, 0.0, -h, a, b, h)
}

fn block_holes(l: &Local, p: &LibraryPart) -> R<Shape> {
    let (a, b, h) = *variant(BLOCKS, &p.variant);
    let c = clearance(p, POCKET);
    let d = depth(p, h + 0.2);
    bxc(l, 0.0, 0.0, 0.01, a + c, b + c, -d - 0.01)
}

/// Unión de dos formas (atajo)
trait FuseWith {
    fn fuse_with(&self, other: &Shape) -> R<Shape>;
}

impl FuseWith for Shape {
    fn fuse_with(&self, other: &Shape) -> R<Shape> {
        self.union(other).map_err(super::e)
    }
}

pub(crate) fn items() -> Vec<Item> {
    let label = |k: &str| k.replace('x', " × ").replace("-m", " · M") + " mm";
    let disc_v: Vec<(String, String)> = DISCS.iter().map(|(k, _)| ((*k).to_string(), format!("Ø{}", label(k)))).collect();
    let ring_v: Vec<(String, String)> = RINGS.iter().map(|(k, _)| ((*k).to_string(), format!("Ø{}", label(k)))).collect();
    let block_v: Vec<(String, String)> = BLOCKS.iter().map(|(k, _)| ((*k).to_string(), label(k))).collect();
    let v = |list: &[(String, String)]| variants(&list.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect::<Vec<_>>());
    vec![
        def("magnet_disc", Category::Magnets, "Imán de disco", "Neodimio, embutido a ras en la cara elegida; sus agujeros son el bolsillo (a presión)", v(&disc_v), None, "#b8bcc4", disc, Some(disc_holes), None),
        def("magnet_ring", Category::Magnets, "Imán avellanado", "Aro de neodimio con avellanado para tornillo, a ras en la cara elegida", v(&ring_v), None, "#b8bcc4", ring, Some(ring_holes), None),
        def("magnet_block", Category::Magnets, "Imán de bloque", "Bloque de neodimio, embutido a ras en la cara elegida", v(&block_v), None, "#b8bcc4", block, Some(block_holes), None),
    ]
}
