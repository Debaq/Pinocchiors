//! Piezas de movimiento: rodamientos, rodamientos lineales, varillas, tornillo
//! T8 y su tuerca, acople flexible, poleas GT2 y rueda V-slot.
//!
//! Rodamientos (de bolas y lineales): origen en el centro de una cara, el
//! rodamiento hacia −Z: puesto en una cara queda embutido a ras y sus agujeros
//! son el alojamiento (a presión) y el paso del eje. Varillas, tornillos,
//! acoples, poleas y ruedas: origen en el centro de una punta, hacia +Z.
//!
//! Medidas: catálogos NSK, igus, Misumi (rodamientos), wfint (lineales),
//! dibujo de la tuerca T8; las poleas no son estándar entre fabricantes y la
//! rueda V-slot es la de OpenBuilds: aproximadas.

use super::{Category, Item, LengthSpec, LibraryPart, R, clearance, cyl, def, depth, fuse, length_of, variant, variants};
use crate::standard::Local;
use cad_occt::{Frame, Shape};

/// Rodamientos de bolas: d, D, B y pestaña (diámetro, espesor; 0 = sin pestaña).
const BEARINGS: &[(&str, (f64, f64, f64, f64, f64))] = &[
    ("608", (8.0, 22.0, 7.0, 0.0, 0.0)),
    ("625", (5.0, 16.0, 5.0, 0.0, 0.0)),
    ("623", (3.0, 10.0, 4.0, 0.0, 0.0)),
    ("688", (8.0, 16.0, 5.0, 0.0, 0.0)),
    ("6001", (12.0, 28.0, 8.0, 0.0, 0.0)),
    ("6200", (10.0, 30.0, 9.0, 0.0, 0.0)),
    ("mr105", (5.0, 10.0, 4.0, 0.0, 0.0)),
    ("f608", (8.0, 22.0, 7.0, 25.0, 1.5)),
];

fn bearing(l: &Local, p: &LibraryPart) -> R<Shape> {
    let (d, od, b, fd, ft) = *variant(BEARINGS, &p.variant);
    let mut ring = cyl(l, 0.0, 0.0, 0.0, od / 2.0, -b)?;
    if fd > 0.0 {
        ring = ring.union(&cyl(l, 0.0, 0.0, 0.0, fd / 2.0, -ft)?).map_err(super::e)?;
    }
    // Las pistas: un surco en cada cara entre el aro de adentro y el de afuera
    let groove = |z: f64, h: f64| cyl(l, 0.0, 0.0, z, (d + od) / 4.0 + (od - d) / 8.0, h)?.cut(&cyl(l, 0.0, 0.0, z - 0.01, (d + od) / 4.0 - (od - d) / 8.0, h.abs() + 0.02)?).map_err(super::e);
    let race = fuse(vec![groove(0.01, -0.31)?, groove(-b - 0.01, 0.31)?])?;
    ring.cut(&cyl(l, 0.0, 0.0, 0.01, d / 2.0, -b - 0.02)?).map_err(super::e)?.cut(&race).map_err(super::e)
}

fn bearing_holes(l: &Local, p: &LibraryPart) -> R<Shape> {
    let (d, od, b, fd, ft) = *variant(BEARINGS, &p.variant);
    let c = clearance(p, 0.1);
    let mut parts = vec![cyl(l, 0.0, 0.0, 0.01, (od + c) / 2.0, -depth(p, b) - 0.01)?, cyl(l, 0.0, 0.0, 0.01, (d + 2.0) / 2.0, -depth(p, b) - 10.0)?];
    if fd > 0.0 {
        parts.push(cyl(l, 0.0, 0.0, 0.01, (fd + c) / 2.0, -ft - 0.01)?);
    }
    fuse(parts)
}

/// Lineales: d, D, L, separación de las ranuras de los seguros, ancho y fondo.
const LINEAR: &[(&str, (f64, f64, f64, f64, f64, f64))] = &[
    ("lm8uu", (8.0, 15.0, 24.0, 17.5, 1.1, 14.3)),
    ("lm10uu", (10.0, 19.0, 29.0, 22.0, 1.3, 18.0)),
    ("lm12uu", (12.0, 21.0, 30.0, 23.0, 1.3, 20.0)),
    ("lm8luu", (8.0, 15.0, 45.0, 35.0, 1.1, 14.3)),
];

fn linear(l: &Local, p: &LibraryPart) -> R<Shape> {
    let (d, od, len, gs, gw, gd) = *variant(LINEAR, &p.variant);
    let body = cyl(l, 0.0, 0.0, 0.0, od / 2.0, -len)?;
    let mut cuts = vec![cyl(l, 0.0, 0.0, 0.01, d / 2.0, -len - 0.02)?];
    // Ranuras de los seguros (entre bordes exteriores: tomadas como entre centros)
    for z in [-(len - gs) / 2.0, -(len + gs) / 2.0] {
        let ring = cyl(l, 0.0, 0.0, z - gw / 2.0, od / 2.0 + 0.1, gw)?.cut(&cyl(l, 0.0, 0.0, z - gw / 2.0 - 0.01, gd / 2.0, gw + 0.02)?).map_err(super::e)?;
        cuts.push(ring);
    }
    body.cut(&fuse(cuts)?).map_err(super::e)
}

fn linear_holes(l: &Local, p: &LibraryPart) -> R<Shape> {
    let (d, od, len, ..) = *variant(LINEAR, &p.variant);
    let c = clearance(p, 0.1);
    fuse(vec![cyl(l, 0.0, 0.0, 0.01, (od + c) / 2.0, -depth(p, len) - 0.01)?, cyl(l, 0.0, 0.0, 0.01, (d + 2.0) / 2.0, -depth(p, len) - 10.0)?])
}

fn length(p: &LibraryPart) -> f64 {
    super::item(&p.item).map_or(300.0, |it| length_of(&it.meta, p))
}

/// Varilla lisa (h6) o tornillo trapezoidal T8 (sin el filete: su diámetro mayor).
fn rod(l: &Local, p: &LibraryPart) -> R<Shape> {
    let d = if p.item == "leadscrew_t8" { 8.0 } else { p.variant.parse::<f64>().unwrap_or(8.0) };
    cyl(l, 0.0, 0.0, 0.0, d / 2.0, length(p))
}

/// Alojamiento de la punta de la varilla (10 mm por defecto).
fn rod_holes(l: &Local, p: &LibraryPart) -> R<Shape> {
    let d = if p.item == "leadscrew_t8" { 8.0 } else { p.variant.parse::<f64>().unwrap_or(8.0) };
    cyl(l, 0.0, 0.0, 0.01, (d + clearance(p, 0.2)) / 2.0, -depth(p, 10.0) - 0.01)
}

/// Tuerca de bronce T8: brida de 22×3,5 con 4 agujeros de 3,5 en un círculo de 16 y cubo de 10,2.
/// Marco: la cara de la brida del lado del resalte corto; el cubo largo hacia +Z.
fn t8_nut(l: &Local, _p: &LibraryPart) -> R<Shape> {
    let flange = cyl(l, 0.0, 0.0, 0.0, 11.0, 3.5)?;
    let hub = cyl(l, 0.0, 0.0, 3.49, 5.1, 10.01)?;
    let spigot = cyl(l, 0.0, 0.0, 0.01, 5.1, -1.51)?;
    let mut cuts = vec![cyl(l, 0.0, 0.0, -1.6, 4.0, 15.2)?];
    for k in 0..4 {
        let a = (45.0 + 90.0 * k as f64).to_radians();
        cuts.push(cyl(l, 8.0 * a.cos(), 8.0 * a.sin(), -0.01, 1.75, 3.52)?);
    }
    fuse(vec![flange, hub, spigot])?.cut(&fuse(cuts)?).map_err(super::e)
}

/// Paso del resalte y del tornillo, y los 4 agujeros para tornillos M3 (para que muerdan).
fn t8_nut_holes(l: &Local, p: &LibraryPart) -> R<Shape> {
    let c = clearance(p, 0.4);
    let d = depth(p, 10.0);
    let mut parts = vec![cyl(l, 0.0, 0.0, 0.01, (10.2 + c) / 2.0, -d - 0.01)?];
    for k in 0..4 {
        let a = (45.0 + 90.0 * k as f64).to_radians();
        parts.push(cyl(l, 8.0 * a.cos(), 8.0 * a.sin(), 0.01, 1.25, -d - 0.01)?);
    }
    fuse(parts)
}

/// Acople flexible 5×8 (Ø19 × 25): un agujero de cada lado.
fn coupler(l: &Local, _p: &LibraryPart) -> R<Shape> {
    let body = cyl(l, 0.0, 0.0, 0.0, 9.5, 25.0)?;
    // Las ranuras helicoidales del medio, como tres cortes finos
    let mut cuts = vec![cyl(l, 0.0, 0.0, -0.01, 2.5, 12.51)?, cyl(l, 0.0, 0.0, 12.5, 4.0, 12.51)?];
    for z in [9.0, 12.0, 15.0] {
        cuts.push(cyl(l, 0.0, 0.0, z, 9.6, 0.6)?.cut(&cyl(l, 0.0, 0.0, z - 0.01, 5.0, 0.62)?).map_err(super::e)?);
    }
    body.cut(&fuse(cuts)?).map_err(super::e)
}

/// Poleas GT2: diámetro de los dientes, pestañas, cubo (diámetro, largo), ancho de la correa y agujero.
const PULLEYS: &[(&str, (f64, f64, f64, f64, f64, f64))] = &[
    ("20t-5-6", (12.22, 16.0, 13.0, 7.0, 7.0, 5.0)),
    ("16t-5-6", (9.68, 13.0, 13.0, 8.0, 7.0, 5.0)),
    ("20t-8-6", (12.22, 16.0, 16.0, 7.0, 7.0, 8.0)),
    ("20t-5-10", (12.22, 16.0, 13.0, 8.0, 11.0, 5.0)),
];

/// Cubo abajo, pestaña, dientes (como cilindro), pestaña.
fn pulley(l: &Local, p: &LibraryPart) -> R<Shape> {
    let (teeth, flange, hub_d, hub_l, belt, bore) = *variant(PULLEYS, &p.variant);
    let hub = cyl(l, 0.0, 0.0, 0.0, hub_d / 2.0, hub_l)?;
    let f1 = cyl(l, 0.0, 0.0, hub_l - 0.01, flange / 2.0, 1.01)?;
    let t = cyl(l, 0.0, 0.0, hub_l + 0.99, teeth / 2.0, belt + 0.02)?;
    let f2 = cyl(l, 0.0, 0.0, hub_l + 1.0 + belt, flange / 2.0, 1.0)?;
    let total = hub_l + 2.0 + belt;
    let mut body = fuse(vec![hub, f1, t, f2])?.cut(&cyl(l, 0.0, 0.0, -0.01, bore / 2.0, total + 0.02)?).map_err(super::e)?;
    // Agujero del prisionero M3 en el cubo
    let side = Frame { origin: l.p(0.0, 0.0, hub_l / 2.0), z: l.x, x: l.y };
    body = body.cut(&Shape::cylinder(side, 1.25, hub_d).map_err(super::e)?).map_err(super::e)?;
    Ok(body)
}

/// Rueda V-slot (OpenBuilds): Ø23,9 × 10,23 con dos 625 adentro.
fn v_wheel(l: &Local, _p: &LibraryPart) -> R<Shape> {
    let (w, r_tip, r_side) = (10.23, 23.9 / 2.0, 9.6);
    let core = cyl(l, 0.0, 0.0, 0.0, r_side, w)?;
    // La V del borde: dos conos que se juntan en el medio
    let up = Shape::cone(Frame { origin: l.p(0.0, 0.0, w / 2.0 - (r_tip - r_side)), z: l.z, x: l.x }, r_side, r_tip, r_tip - r_side).map_err(super::e)?;
    let down = Shape::cone(Frame { origin: l.p(0.0, 0.0, w / 2.0 + (r_tip - r_side)), z: crate::geom::scale(l.z, -1.0), x: l.x }, r_side, r_tip, r_tip - r_side).map_err(super::e)?;
    let body = fuse(vec![core, up, down])?;
    // El agujero de los rodamientos 625 y su paso de 5
    let bore = fuse(vec![cyl(l, 0.0, 0.0, -0.01, 8.0, 5.01)?, cyl(l, 0.0, 0.0, w - 5.0, 8.0, 5.01)?, cyl(l, 0.0, 0.0, -0.01, 2.5, w + 0.02)?])?;
    let inner = cyl(l, 0.0, 0.0, 0.3, 8.0, w - 0.6)?.cut(&cyl(l, 0.0, 0.0, 0.29, 2.5, w)?).map_err(super::e)?;
    body.cut(&bore).map_err(super::e)?.union(&inner).map_err(super::e)
}

pub(crate) fn items() -> Vec<Item> {
    let none = variants(&[]);
    let rod_len = Some(LengthSpec { default: 300.0, min: 10.0, max: 2000.0 });
    vec![
        def(
            "bearing",
            Category::Motion,
            "Rodamiento de bolas",
            "A ras en la cara elegida; sus agujeros son el alojamiento a presión y el paso del eje",
            variants(&[("608", "608 · 8×22×7"), ("625", "625 · 5×16×5"), ("623", "623 · 3×10×4"), ("688", "688 · 8×16×5"), ("6001", "6001 · 12×28×8"), ("6200", "6200 · 10×30×9"), ("mr105", "MR105 · 5×10×4"), ("f608", "F608 · con pestaña")]),
            None,
            "#9aa0a8",
            bearing,
            Some(bearing_holes),
            None,
        ),
        def(
            "linear_bearing",
            Category::Motion,
            "Rodamiento lineal",
            "LMxUU a ras en la cara elegida (eje a lo largo de Z); sus agujeros son el alojamiento",
            variants(&[("lm8uu", "LM8UU · 8×15×24"), ("lm10uu", "LM10UU · 10×19×29"), ("lm12uu", "LM12UU · 12×21×30"), ("lm8luu", "LM8LUU · 8×15×45")]),
            None,
            "#9aa0a8",
            linear,
            Some(linear_holes),
            None,
        ),
        def("rod", Category::Motion, "Varilla lisa", "Acero h6 cortado a medida", variants(&[("8", "Ø8"), ("10", "Ø10"), ("12", "Ø12")]), rod_len, "#b0b4ba", rod, Some(rod_holes), None),
        def("leadscrew_t8", Category::Motion, "Tornillo trapezoidal T8", "Tr8 paso 2 (avance 2 u 8), a medida; sin el filete dibujado", variants(&[("8", "Avance 8 (4 entradas)"), ("2", "Avance 2")]), rod_len, "#b0b4ba", rod, Some(rod_holes), None),
        def("t8_nut", Category::Motion, "Tuerca T8 de bronce", "Con brida de 22 y 4 agujeros de 3,5; sus agujeros: paso del cubo y 4 para M3", none.clone(), None, "#c9a24a", t8_nut, Some(t8_nut_holes), None),
        def("coupler_5x8", Category::Motion, "Acople flexible 5×8", "Ø19 × 25 (eje del motor de 5 y tornillo de 8)", none.clone(), None, "#b0b4ba", coupler, None, None),
        def(
            "pulley_gt2",
            Category::Motion,
            "Polea GT2 (aprox.)",
            "Dientes y agujero exactos; pestañas y cubo varían según el fabricante",
            variants(&[("20t-5-6", "20 dientes · eje 5 · correa 6"), ("16t-5-6", "16 dientes · eje 5 · correa 6"), ("20t-8-6", "20 dientes · eje 8 · correa 6"), ("20t-5-10", "20 dientes · eje 5 · correa 10")]),
            None,
            "#b0b4ba",
            pulley,
            None,
            None,
        ),
        def("v_wheel", Category::Motion, "Rueda V-slot", "OpenBuilds Ø23,9 × 10,2 con dos rodamientos 625", none, None, "#3a3d42", v_wheel, None, None),
    ]
}
