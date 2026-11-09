//! Placas de desarrollo (Arduino, Raspberry Pi, ESP): el PCB con su contorno y
//! agujeros de los planos oficiales y los conectores y componentes altos como
//! bloques (lo que importa para diseñarles una caja).
//!
//! Marco: origen en el centro de la placa, a la altura de su cara de abajo
//! cuando no tiene postes (`lift` la sube); Z hacia los componentes. Puesta
//! en el piso de una caja: sus agujeros son los de los tornillos en el piso y
//! las ventanas de los conectores en las paredes; sus soportes, los postes.

use super::boards_data::BOARDS;
use super::{Category, Item, LibraryPart, R, bx, clearance, cyl, def, depth, fuse, poly_with_holes, rounded_rect, variants};
use crate::standard::Local;
use cad_occt::Shape;

/// Por qué borde sale un conector (para abrirle una ventana en la pared).
#[derive(Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub(crate) enum Edge {
    Left,
    Right,
    Top,
    Bottom,
    None,
}

/// Un conector o componente: su caja en el plano (origen en la esquina inferior
/// izquierda de la placa) y su alto sobre el PCB; `below`, hacia abajo (los
/// pines de las placas para protoboard).
pub(crate) struct Comp {
    pub x: (f64, f64),
    pub y: (f64, f64),
    pub h: f64,
    pub edge: Edge,
    pub below: bool,
}

pub(crate) struct Board {
    pub id: &'static str,
    pub name: &'static str,
    /// Medidas del contorno (X, Y)
    pub size: (f64, f64),
    pub thickness: f64,
    /// Radio de las esquinas (sin `outline`)
    pub corner: f64,
    /// Contorno exacto (vacío: rectángulo con `corner`)
    pub outline: &'static [(f64, f64)],
    /// Agujeros de montaje: x, y, diámetro
    pub holes: &'static [(f64, f64, f64)],
    pub comps: &'static [Comp],
}

fn board(p: &LibraryPart) -> &'static Board {
    BOARDS.iter().find(|b| b.id == p.item).unwrap_or(&BOARDS[0])
}

/// De la esquina de la placa al marco centrado
fn centered(b: &Board) -> impl Fn(f64, f64) -> (f64, f64) + '_ {
    move |x, y| (x - b.size.0 / 2.0, y - b.size.1 / 2.0)
}

fn build(l: &Local, p: &LibraryPart) -> R<Shape> {
    let b = board(p);
    let c = centered(b);
    let z0 = p.lift.max(0.0);
    let outline: Vec<(f64, f64)> = if b.outline.is_empty() { rounded_rect(b.size.0, b.size.1, b.corner) } else { b.outline.iter().map(|&(x, y)| c(x, y)).collect() };
    let holes: Vec<(f64, f64, f64)> = b.holes.iter().map(|&(x, y, d)| {
        let (x, y) = c(x, y);
        (x, y, d / 2.0)
    }).collect();
    let pcb = poly_with_holes(l, &outline, &[], &holes, z0, b.thickness)?;
    let mut parts = vec![pcb];
    for k in b.comps {
        let (x0, y0) = c(k.x.0, k.y.0);
        let (x1, y1) = c(k.x.1, k.y.1);
        let (z, h) = if k.below { (z0, -k.h) } else { (z0 + b.thickness, k.h) };
        parts.push(bx(l, x0, y0, z, x1 - x0, y1 - y0, h)?);
    }
    fuse(parts)
}

/// Agujeros para tornillo en el piso (hacia −Z) y ventanas de los conectores que salen por un borde.
fn holes(l: &Local, p: &LibraryPart) -> R<Shape> {
    let b = board(p);
    let c = centered(b);
    let gap = clearance(p, 0.5);
    let z0 = p.lift.max(0.0);
    let mut parts = Vec::new();
    for &(x, y, d) in b.holes {
        let (x, y) = c(x, y);
        // Para que el tornillo muerda el plástico
        parts.push(cyl(l, x, y, 0.01, d * 0.4, -depth(p, 6.0) - 0.01)?);
    }
    const OUT: f64 = 15.0;
    let (w, h) = b.size;
    for k in b.comps.iter().filter(|k| k.edge != Edge::None && !k.below) {
        let (mut x0, mut x1, mut y0, mut y1) = (k.x.0 - gap, k.x.1 + gap, k.y.0 - gap, k.y.1 + gap);
        match k.edge {
            Edge::Left => (x0, x1) = (x0.min(0.0) - OUT, 2.0),
            Edge::Right => (x0, x1) = (w - 2.0, x1.max(w) + OUT),
            Edge::Bottom => (y0, y1) = (y0.min(0.0) - OUT, 2.0),
            Edge::Top => (y0, y1) = (h - 2.0, y1.max(h) + OUT),
            Edge::None => unreachable!(),
        }
        let (ax, ay) = c(x0, y0);
        let (bx_, by) = c(x1, y1);
        let z = z0 + b.thickness - gap;
        parts.push(bx(l, ax, ay, z, bx_ - ax, by - ay, k.h + 2.0 * gap)?);
    }
    fuse(parts)
}

/// Postes bajo los agujeros de montaje, del piso a la placa, con agujero para tornillo.
fn mounts(l: &Local, p: &LibraryPart) -> R<Shape> {
    let b = board(p);
    if b.holes.is_empty() {
        return Err(format!("{} no tiene agujeros de montaje", b.name));
    }
    let lift = if p.lift > 0.0 { p.lift } else { 5.0 };
    let c = centered(b);
    let mut posts = Vec::new();
    let mut bores = Vec::new();
    for &(x, y, d) in b.holes {
        let (x, y) = c(x, y);
        posts.push(cyl(l, x, y, -0.01, (d * 1.1).max(2.5), lift + 0.01)?);
        bores.push(cyl(l, x, y, 0.5, d * 0.4, lift)?);
    }
    fuse(posts)?.cut(&fuse(bores)?).map_err(super::e)
}

pub(crate) fn items() -> Vec<Item> {
    BOARDS
        .iter()
        .map(|b| {
            def(
                b.id,
                Category::Boards,
                b.name,
                if b.holes.is_empty() { "Contorno y conectores de su plano oficial" } else { "Contorno, agujeros y conectores de su plano oficial; agujeros: tornillos en el piso y ventanas de los conectores" },
                variants(&[]),
                None,
                "#1d6b46",
                build,
                // Sin agujeros ni conectores en un borde (la Pro Mini) no hay nada que quitar
                if b.holes.is_empty() && b.comps.iter().all(|k| k.edge == Edge::None || k.below) { None } else { Some(holes) },
                if b.holes.is_empty() { None } else { Some(mounts) },
            )
        })
        .collect()
}
