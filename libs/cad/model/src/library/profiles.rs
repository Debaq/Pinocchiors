//! Perfiles de aluminio (V-slot y T-slot) cortados a medida, y sus accesorios.
//!
//! Marco: origen en el centro de la sección en una punta, el perfil hacia +Z.
//! Sus agujeros son el alojamiento de la punta (la sección con holgura, hacia
//! −Z) con el agujero del tornillo del centro: para piezas impresas que lo
//! reciben.
//!
//! Medidas: V-slot del modelo abierto bd_warehouse (reproduce OpenBuilds);
//! T-slot de los planos de Misumi (HFS5-2020, HFS6-3030). El 4040 y el 4080
//! de ranura 8 no tienen plano completo: medidas típicas.

use super::{Category, Item, LengthSpec, LibraryPart, R, bx, bxc, clearance, cyl, def, depth, fuse, length_of, poly, rounded_rect, variants};
use crate::standard::Local;
use cad_occt::Shape;

/// Una sección: módulos de `a` en fila (a lo largo de Y), ranura por media
/// ranura (lateral, profundidad desde la cara), agujeros del centro de cada
/// módulo y de las esquinas.
pub(crate) struct Profile {
    a: f64,
    modules: usize,
    corner: f64,
    half_slot: &'static [(f64, f64)],
    bore: f64,
    /// Agujeros de las esquinas de cada módulo: diámetro y distancia entre centros
    corner_holes: Option<(f64, f64)>,
}

const VSLOT: &[(f64, f64)] = &[(0.0, 6.31), (0.21, 6.1), (2.839, 6.1), (5.5, 3.439), (5.5, 1.8), (3.125, 1.8), (3.125, 1.455), (4.58, 0.0), (0.0, 0.0)];
const TSLOT6: &[(f64, f64)] = &[(0.0, 6.0), (2.59, 6.0), (6.0, 2.59), (6.0, 2.0), (3.0, 2.0), (3.0, 0.0), (0.0, 0.0)];
const TSLOT8: &[(f64, f64)] = &[(0.0, 9.0), (4.59, 9.0), (8.25, 5.34), (8.25, 2.0), (4.0, 2.0), (4.0, 0.0), (0.0, 0.0)];
const TSLOT8_40: &[(f64, f64)] = &[(0.0, 12.0), (4.6, 12.0), (8.5, 8.1), (8.5, 2.2), (4.1, 2.2), (4.1, 0.0), (0.0, 0.0)];

const PROFILES: &[(&str, Profile)] = &[
    ("vslot_2020", Profile { a: 20.0, modules: 1, corner: 1.5, half_slot: VSLOT, bore: 4.2, corner_holes: None }),
    ("vslot_2040", Profile { a: 20.0, modules: 2, corner: 1.5, half_slot: VSLOT, bore: 4.2, corner_holes: None }),
    ("vslot_2060", Profile { a: 20.0, modules: 3, corner: 1.5, half_slot: VSLOT, bore: 4.2, corner_holes: None }),
    ("vslot_2080", Profile { a: 20.0, modules: 4, corner: 1.5, half_slot: VSLOT, bore: 4.2, corner_holes: None }),
    ("tslot_2020", Profile { a: 20.0, modules: 1, corner: 1.0, half_slot: TSLOT6, bore: 4.2, corner_holes: None }),
    ("tslot_2040", Profile { a: 20.0, modules: 2, corner: 1.0, half_slot: TSLOT6, bore: 4.2, corner_holes: None }),
    ("tslot_3030", Profile { a: 30.0, modules: 1, corner: 2.0, half_slot: TSLOT8, bore: 6.8, corner_holes: Some((4.2, 23.2)) }),
    ("tslot_4040", Profile { a: 40.0, modules: 1, corner: 3.0, half_slot: TSLOT8_40, bore: 6.8, corner_holes: None }),
    ("tslot_4080", Profile { a: 40.0, modules: 2, corner: 3.0, half_slot: TSLOT8_40, bore: 6.8, corner_holes: None }),
];

fn profile(p: &LibraryPart) -> &'static Profile {
    PROFILES.iter().find(|(k, _)| *k == p.item).map_or(&PROFILES[0].1, |(_, v)| v)
}

/// La ranura entera en coordenadas de su cara (lateral, profundidad), un poco afuera de la cara
fn slot_outline(half: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let lift = |(u, v): (f64, f64)| (u, if v <= 1e-9 { -0.05 } else { v });
    let inner: Vec<(f64, f64)> = half.iter().copied().filter(|p| p.0 > 1e-9).map(lift).collect();
    let apex = half.iter().find(|p| p.0 <= 1e-9 && p.1 > 1e-9).copied();
    let mut out = Vec::new();
    if let Some(a) = apex {
        out.push(a);
    }
    out.extend(inner.iter().copied());
    out.extend(inner.iter().rev().map(|&(u, v)| (-u, v)));
    out
}

fn build(l: &Local, p: &LibraryPart) -> R<Shape> {
    let pr = profile(p);
    let len = super::item(&p.item).map_or(300.0, |it| length_of(&it.meta, p));
    let h = pr.a * pr.modules as f64;
    let body = poly(l, &rounded_rect(pr.a, h, pr.corner), 0.0, len)?;
    let slot = slot_outline(pr.half_slot);
    // Cara: centro, normal hacia afuera y tangente (lateral)
    let mut faces: Vec<((f64, f64), (f64, f64), (f64, f64))> = Vec::new();
    for k in 0..pr.modules {
        let yc = -h / 2.0 + pr.a * (k as f64 + 0.5);
        faces.push(((pr.a / 2.0, yc), (1.0, 0.0), (0.0, 1.0)));
        faces.push(((-pr.a / 2.0, yc), (-1.0, 0.0), (0.0, -1.0)));
    }
    faces.push(((0.0, h / 2.0), (0.0, 1.0), (-1.0, 0.0)));
    faces.push(((0.0, -h / 2.0), (0.0, -1.0), (1.0, 0.0)));
    let mut cuts = Vec::new();
    for (c, n, t) in faces {
        let pts: Vec<(f64, f64)> = slot.iter().map(|&(u, v)| (c.0 + t.0 * u - n.0 * v, c.1 + t.1 * u - n.1 * v)).collect();
        cuts.push(poly(l, &pts, -0.01, len + 0.02)?);
    }
    for k in 0..pr.modules {
        let yc = -h / 2.0 + pr.a * (k as f64 + 0.5);
        cuts.push(cyl(l, 0.0, yc, -0.01, pr.bore / 2.0, len + 0.02)?);
        if let Some((d, s)) = pr.corner_holes {
            for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                cuts.push(cyl(l, sx * s / 2.0, yc + sy * s / 2.0, -0.01, d / 2.0, len + 0.02)?);
            }
        }
    }
    body.cut(&fuse(cuts)?).map_err(super::e)
}

/// Alojamiento de la punta (la sección con holgura) y el agujero del tornillo del centro.
fn holes(l: &Local, p: &LibraryPart) -> R<Shape> {
    let pr = profile(p);
    let c = clearance(p, 0.3);
    let d = depth(p, 10.0);
    let h = pr.a * pr.modules as f64;
    let mut parts = vec![poly(l, &rounded_rect(pr.a + c, h + c, pr.corner), 0.01, -d - 0.01)?];
    let screw = if pr.bore > 5.0 { 8.4 } else { 5.5 };
    for k in 0..pr.modules {
        let yc = -h / 2.0 + pr.a * (k as f64 + 0.5);
        parts.push(cyl(l, 0.0, yc, -d + 0.01, screw / 2.0, -10.0)?);
    }
    fuse(parts)
}

/// Escuadra de esquina para perfil 20 (20×20×20, espesor 3, agujeros de 5,5).
/// Marco: la esquina interior en el origen, un ala sobre el plano XY (+X) y la otra sobre XZ (+Z).
fn bracket(l: &Local, _p: &LibraryPart) -> R<Shape> {
    let (s, t, w) = (20.0, 3.0, 20.0);
    let a = bx(l, 0.0, -w / 2.0, -t, s, w, t)?;
    let b = bx(l, -t, -w / 2.0, -t, t, w, s + t)?;
    // Refuerzo diagonal en el medio
    let rib = poly(l, &[(0.0, 0.0), (s - 4.0, 0.0), (0.0, s - 4.0)], 0.0, 1.0)?;
    let rib = rib.transform_to_xz(l)?;
    let mut body = fuse(vec![a, b, rib])?;
    body = body.cut(&cyl(l, s / 2.0 + 1.0, 0.0, -t - 0.01, 2.75, t + 0.02)?).map_err(super::e)?;
    let side = cad_occt::Frame { origin: l.p(-t - 0.01, 0.0, s / 2.0 + 1.0), z: l.x, x: l.y };
    body.cut(&Shape::cylinder(side, 2.75, t + 0.02).map_err(super::e)?).map_err(super::e)
}

trait ToXz {
    fn transform_to_xz(&self, l: &Local) -> R<Shape>;
}

impl ToXz for Shape {
    /// El triángulo del refuerzo (dibujado en XY, de 1 de alto) llevado al plano XZ, centrado en Y
    fn transform_to_xz(&self, l: &Local) -> R<Shape> {
        let axis = cad_occt::Axis { origin: l.o, dir: l.x };
        self.rotate(axis, std::f64::consts::FRAC_PI_2).map_err(super::e)?.translate(crate::geom::scale(l.y, 0.5)).map_err(super::e)
    }
}

/// Tuercas en T (martillo) para la ranura: ancho, largo, alto y rosca.
const TNUTS: &[(&str, (f64, f64, f64, f64))] = &[("m5-6", (6.0, 10.3, 4.4, 5.0)), ("m3-6", (6.0, 10.0, 4.0, 3.0)), ("m4-6", (6.0, 10.0, 4.0, 4.0)), ("m5-8", (8.0, 16.0, 6.0, 5.0)), ("m6-8", (8.0, 16.0, 6.0, 6.0))];

/// Marco: centro de la cara de arriba (la que apoya contra la boca de la ranura), la tuerca hacia −Z.
fn tnut(l: &Local, p: &LibraryPart) -> R<Shape> {
    let (w, len, h, m) = *super::variant(TNUTS, &p.variant);
    let body = bxc(l, 0.0, 0.0, -h, w * 1.6, len, h * 0.6)?;
    let neck = bxc(l, 0.0, 0.0, -h * 0.4, w - 0.2, len, h * 0.4)?;
    fuse(vec![body, neck])?.cut(&cyl(l, 0.0, 0.0, 0.01, m * 0.42, -h - 0.02)?).map_err(super::e)
}

pub(crate) fn items() -> Vec<Item> {
    let length = Some(LengthSpec { default: 300.0, min: 10.0, max: 3000.0 });
    let none = variants(&[]);
    let p = |id: &'static str, name: &'static str, desc: &'static str| def(id, Category::Profiles, name, desc, none.clone(), length, "#c4c8ce", build, Some(holes), None);
    vec![
        p("vslot_2020", "Perfil V-slot 2020", "OpenBuilds, ranura en V para ruedas; agujero central para M5"),
        p("vslot_2040", "Perfil V-slot 2040", "OpenBuilds, dos módulos de 20"),
        p("vslot_2060", "Perfil V-slot 2060", "OpenBuilds, tres módulos de 20"),
        p("vslot_2080", "Perfil V-slot 2080", "OpenBuilds, cuatro módulos de 20"),
        p("tslot_2020", "Perfil 2020 ranura 6", "T-slot tipo B (Misumi HFS5-2020); agujero central para M5"),
        p("tslot_2040", "Perfil 2040 ranura 6", "T-slot tipo B, dos módulos de 20"),
        p("tslot_3030", "Perfil 3030 ranura 8", "T-slot tipo B (Misumi HFS6-3030); agujero central para M8 y cuatro de esquina"),
        p("tslot_4040", "Perfil 4040 ranura 8 (aprox.)", "T-slot 40×40 ranura 8: medidas típicas, sin plano oficial completo"),
        p("tslot_4080", "Perfil 4080 ranura 8 (aprox.)", "T-slot 40×80 ranura 8: medidas típicas, sin plano oficial completo"),
        def("bracket_2020", Category::Profiles, "Escuadra 2020", "Escuadra de esquina 20×20×20 para perfil 20 (agujeros para M5)", none, None, "#c4c8ce", bracket, None, None),
        def(
            "tnut",
            Category::Profiles,
            "Tuerca en T",
            "Tuerca martillo para la ranura; apoyada por la cara que toca la boca de la ranura",
            variants(&[("m5-6", "M5 · ranura 6"), ("m3-6", "M3 · ranura 6"), ("m4-6", "M4 · ranura 6"), ("m5-8", "M5 · ranura 8"), ("m6-8", "M6 · ranura 8")]),
            None,
            "#8a8e96",
            tnut,
            None,
            None,
        ),
    ]
}
