//! Biblioteca de piezas maker: motores, perfiles de aluminio, placas
//! (Arduino, Raspberry Pi, ESP), imanes, tornillería, piezas de movimiento y
//! módulos. Cada una se arma desde su tabla de medidas (sin archivos
//! descargados) y se ubica con un marco, como las piezas estándar.
//!
//! El marco de cada pieza tiene el origen en la cara con la que se apoya en
//! lo que uno diseña y Z saliendo de ese material (al elegir una cara, Z es su
//! normal): la pieza queda afuera y lo que entra en el material (el eje de un
//! motor que cruza la placa, un imán embutido) va en −Z. Cada módulo dice el
//! suyo.
//!
//! Además de la pieza (`LibraryRole::Part`), cada una sabe dar lo que hay que
//! quitar para alojarla (`Holes`: agujeros de montaje, paso del eje, bolsillo
//! del imán, ventanas de los conectores) y, si corresponde, lo que hay que
//! agregar para sostenerla (`Mounts`: postes de una placa).

use cad_occt::{Curve, Frame, Shape};
use serde::{Deserialize, Serialize};

use crate::geom::P3;
use crate::standard::{Local, e};

mod boards;
mod boards_data;
mod fasteners;
mod magnets;
mod modules;
mod motion;
mod motors;
mod profiles;
mod servos;

type R<T> = Result<T, String>;

/// Qué se arma de una pieza de la biblioteca.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryRole {
    /// La pieza misma (de referencia o comprada)
    #[default]
    Part,
    /// Lo que se quita para alojarla: agujeros, paso del eje, bolsillos, ventanas
    Holes,
    /// Lo que se agrega para sostenerla (postes de una placa)
    Mounts,
}

fn is_part(r: &LibraryRole) -> bool {
    *r == LibraryRole::Part
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

/// Una pieza de la biblioteca en un diseño.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryPart {
    /// Qué pieza («nema17», «arduino_uno», «magnet_disc»…)
    pub item: String,
    /// Su variante («48» mm de cuerpo, «10x3»…; vacía = la primera)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub variant: String,
    /// Largo de las que se cortan a medida (perfiles, varillas, tornillos)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<f64>,
    #[serde(default, skip_serializing_if = "is_part")]
    pub role: LibraryRole,
    /// Agujeros: hasta dónde entran en el material (0 = lo de la pieza)
    #[serde(default, skip_serializing_if = "is_zero")]
    pub depth: f64,
    /// Agujeros y bolsillos: holgura extra en el diámetro (para imprimir)
    #[serde(default, skip_serializing_if = "is_zero")]
    pub clearance: f64,
    /// Placas: altura de los postes (la placa queda encima de ellos)
    #[serde(default, skip_serializing_if = "is_zero")]
    pub lift: f64,
}

impl LibraryPart {
    pub fn new(item: &str, variant: &str) -> Self {
        LibraryPart { item: item.into(), variant: variant.into(), length: None, role: LibraryRole::Part, depth: 0.0, clearance: 0.0, lift: 0.0 }
    }
}

/// Categorías del panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Motors,
    Profiles,
    Boards,
    Magnets,
    Fasteners,
    Motion,
    Modules,
}

/// Una variante para elegir en el panel.
#[derive(Debug, Clone, Serialize)]
pub struct Variant {
    pub id: String,
    pub label: String,
}

/// Largo a medida: el de fábrica y los límites.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct LengthSpec {
    pub default: f64,
    pub min: f64,
    pub max: f64,
}

/// Una pieza del catálogo, para el panel.
#[derive(Debug, Clone, Serialize)]
pub struct CatalogItem {
    pub id: &'static str,
    pub category: Category,
    pub name: &'static str,
    pub description: &'static str,
    pub variants: Vec<Variant>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length: Option<LengthSpec>,
    /// Sabe dar sus agujeros / sus postes
    pub holes: bool,
    pub mounts: bool,
    /// Color con que se muestra (#rrggbb)
    pub color: &'static str,
}

/// Cómo se arma una pieza del catálogo.
pub(crate) struct Item {
    pub meta: CatalogItem,
    pub build: fn(&Local, &LibraryPart) -> R<Shape>,
    pub holes: Option<fn(&Local, &LibraryPart) -> R<Shape>>,
    pub mounts: Option<fn(&Local, &LibraryPart) -> R<Shape>>,
}

fn items() -> &'static [Item] {
    static ITEMS: std::sync::OnceLock<Vec<Item>> = std::sync::OnceLock::new();
    ITEMS.get_or_init(|| {
        let mut v = Vec::new();
        v.extend(motors::items());
        v.extend(profiles::items());
        v.extend(boards::items());
        v.extend(magnets::items());
        v.extend(fasteners::items());
        v.extend(motion::items());
        v.extend(modules::items());
        v
    })
}

fn item(id: &str) -> R<&'static Item> {
    items().iter().find(|i| i.meta.id == id).ok_or_else(|| format!("No conozco la pieza «{id}»"))
}

/// El catálogo entero, para el panel.
pub fn catalog() -> Vec<CatalogItem> {
    items().iter().map(|i| i.meta.clone()).collect()
}

/// La variante elegida (la primera si no hay o no existe).
pub(crate) fn variant<'a, T>(table: &'a [(&'static str, T)], id: &str) -> &'a T {
    &table.iter().find(|(k, _)| *k == id).unwrap_or(&table[0]).1
}

/// Nombre para mostrar: «NEMA 17 · 48 mm», «Perfil 2020 · 350 mm».
pub fn describe(p: &LibraryPart) -> String {
    let Ok(it) = item(&p.item) else { return p.item.clone() };
    let variant = it.meta.variants.iter().find(|v| v.id == p.variant).or(it.meta.variants.first());
    let mut out = it.meta.name.to_string();
    if let Some(v) = variant.filter(|v| !v.label.is_empty() && it.meta.variants.len() > 1) {
        out.push_str(" · ");
        out.push_str(&v.label);
    }
    if let (Some(_), Some(l)) = (it.meta.length, p.length) {
        out.push_str(&format!(" · {} mm", fmt_mm(l)));
    }
    match p.role {
        LibraryRole::Part => out,
        LibraryRole::Holes => format!("Agujeros para {out}"),
        LibraryRole::Mounts => format!("Soportes para {out}"),
    }
}

/// Milímetros sin ceros de más: 350, 12.5
fn fmt_mm(v: f64) -> String {
    let s = format!("{:.2}", v);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Largo efectivo: el pedido (acotado) o el de fábrica.
pub(crate) fn length_of(it: &CatalogItem, p: &LibraryPart) -> f64 {
    let Some(spec) = it.length else { return 0.0 };
    p.length.unwrap_or(spec.default).clamp(spec.min, spec.max)
}

/// La forma de la pieza (o de sus agujeros o postes) en el marco `frame`.
pub fn build(frame: Frame, p: &LibraryPart) -> R<Shape> {
    let it = item(&p.item)?;
    let l = Local::new(frame);
    match p.role {
        LibraryRole::Part => (it.build)(&l, p),
        LibraryRole::Holes => (it.holes.ok_or_else(|| format!("{} no tiene agujeros para hacer", it.meta.name))?)(&l, p),
        LibraryRole::Mounts => (it.mounts.ok_or_else(|| format!("{} no lleva soportes", it.meta.name))?)(&l, p),
    }
}

/// Las de rol pieza son compradas (no se fabrican ni se mandan a imprimir).
pub fn is_purchased(p: &LibraryPart) -> bool {
    p.role == LibraryRole::Part
}

// ─── Ayudas de geometría en el marco local ──────────────────────────────────

/// Cilindro de radio `r` con la base en (x, y, z0) y alto `h` (negativo: hacia −Z).
pub(crate) fn cyl(l: &Local, x: f64, y: f64, z0: f64, r: f64, h: f64) -> R<Shape> {
    let (z0, h) = if h < 0.0 { (z0 + h, -h) } else { (z0, h) };
    let f = Frame { origin: l.p(x, y, z0), z: l.z, x: l.x };
    Shape::cylinder(f, r, h).map_err(e)
}

/// Caja con la esquina en (x, y, z0) y medidas (dx, dy, dz) a lo largo de X, Y, Z.
pub(crate) fn bx(l: &Local, x: f64, y: f64, z0: f64, dx: f64, dy: f64, dz: f64) -> R<Shape> {
    let (x, dx) = if dx < 0.0 { (x + dx, -dx) } else { (x, dx) };
    let (y, dy) = if dy < 0.0 { (y + dy, -dy) } else { (y, dy) };
    let (z0, dz) = if dz < 0.0 { (z0 + dz, -dz) } else { (z0, dz) };
    let f = Frame { origin: l.p(x, y, z0), z: l.z, x: l.x };
    Shape::make_box(f, dx, dy, dz).map_err(e)
}

/// Caja centrada en (cx, cy) en el plano, de z0 a z0 + dz.
pub(crate) fn bxc(l: &Local, cx: f64, cy: f64, z0: f64, dx: f64, dy: f64, dz: f64) -> R<Shape> {
    bx(l, cx - dx / 2.0, cy - dy / 2.0, z0, dx, dy, dz)
}

/// Prisma de un polígono del plano local (x, y) desde z0, alto h.
pub(crate) fn poly(l: &Local, pts: &[(f64, f64)], z0: f64, h: f64) -> R<Shape> {
    let (z0, h) = if h < 0.0 { (z0 + h, -h) } else { (z0, h) };
    let p: Vec<P3> = pts.iter().map(|&(x, y)| l.p(x, y, z0)).collect();
    Shape::polygon(&p).map_err(e)?.prism(crate::geom::scale(l.z, h)).map_err(e)
}

/// Prisma de una cara con agujeros: contorno y huecos como polígonos (x, y) y círculos (x, y, r).
pub(crate) fn poly_with_holes(l: &Local, outer: &[(f64, f64)], polys: &[Vec<(f64, f64)>], circles: &[(f64, f64, f64)], z0: f64, h: f64) -> R<Shape> {
    let lines = |pts: &[(f64, f64)]| -> Vec<Curve> {
        let n = pts.len();
        (0..n).map(|i| Curve::Line(l.p(pts[i].0, pts[i].1, z0), l.p(pts[(i + 1) % n].0, pts[(i + 1) % n].1, z0))).collect()
    };
    let mut loops = vec![lines(outer)];
    loops.extend(polys.iter().map(|p| lines(p)));
    loops.extend(circles.iter().map(|&(x, y, r)| vec![Curve::Circle { center: l.p(x, y, z0), normal: l.z, radius: r }]));
    Shape::face(&loops).map_err(e)?.prism(crate::geom::scale(l.z, h)).map_err(e)
}

/// Rectángulo con las esquinas achaflanadas (centrado), como polígono.
pub(crate) fn chamfered_rect(w: f64, h: f64, c: f64) -> Vec<(f64, f64)> {
    let (a, b) = (w / 2.0, h / 2.0);
    if c <= 0.0 {
        return vec![(-a, -b), (a, -b), (a, b), (-a, b)];
    }
    vec![(-a + c, -b), (a - c, -b), (a, -b + c), (a, b - c), (a - c, b), (-a + c, b), (-a, b - c), (-a, -b + c)]
}

/// Rectángulo con las esquinas redondeadas (radio `r`, `n` tramos por esquina), centrado.
pub(crate) fn rounded_rect(w: f64, h: f64, r: f64) -> Vec<(f64, f64)> {
    let (a, b) = (w / 2.0, h / 2.0);
    let r = r.min(a).min(b);
    if r <= 0.0 {
        return vec![(-a, -b), (a, -b), (a, b), (-a, b)];
    }
    let mut out = Vec::new();
    for (cx, cy, a0) in [(a - r, -b + r, -90.0f64), (a - r, b - r, 0.0), (-a + r, b - r, 90.0), (-a + r, -b + r, 180.0)] {
        for k in 0..=4 {
            let t = (a0 + 22.5 * k as f64).to_radians();
            out.push((cx + r * t.cos(), cy + r * t.sin()));
        }
    }
    out
}

/// Junta varias formas en una (una sola booleana).
pub(crate) fn fuse(shapes: Vec<Shape>) -> R<Shape> {
    match shapes.len() {
        0 => Err("nada que juntar".into()),
        1 => Ok(shapes.into_iter().next().unwrap()),
        _ => Shape::fuse_all(&shapes).map_err(e),
    }
}

/// Holgura de los agujeros: la pedida o la de fábrica.
pub(crate) fn clearance(p: &LibraryPart, default: f64) -> f64 {
    if p.clearance > 0.0 { p.clearance } else { default }
}

/// Profundidad de los agujeros: la pedida o la de fábrica.
pub(crate) fn depth(p: &LibraryPart, default: f64) -> f64 {
    if p.depth > 0.0 { p.depth } else { default }
}

/// Item con su ficha (atajo para los módulos).
#[allow(clippy::too_many_arguments)]
pub(crate) fn def(
    id: &'static str,
    category: Category,
    name: &'static str,
    description: &'static str,
    variants: Vec<Variant>,
    length: Option<LengthSpec>,
    color: &'static str,
    build: fn(&Local, &LibraryPart) -> R<Shape>,
    holes: Option<fn(&Local, &LibraryPart) -> R<Shape>>,
    mounts: Option<fn(&Local, &LibraryPart) -> R<Shape>>,
) -> Item {
    Item { meta: CatalogItem { id, category, name, description, variants, length, holes: holes.is_some(), mounts: mounts.is_some(), color }, build, holes, mounts }
}

/// Variantes desde una tabla (id y etiqueta).
pub(crate) fn variants(list: &[(&str, &str)]) -> Vec<Variant> {
    list.iter().map(|(id, label)| Variant { id: (*id).into(), label: (*label).into() }).collect()
}

#[cfg(test)]
mod tests;
