//! Preparar una pieza para resina de punta a punta (SLA3–SLA4 de `PLAN_SLA.md`):
//! elevar, ventear las ventosas y huecos cerrados, poner soportes, laminar y revisar.
//!
//! Estrategias contra la succión:
//! - **Venteo**: un agujero vertical desde el piso de cada ventosa hasta el aire de abajo
//!   (una cáscara dada vuelta que resta al laminar). Es lo que hace un laminador de resina
//!   con los «agujeros de drenaje»; si la pieza ya se vació con `print3d::hollow` con
//!   drenaje, no queda nada que ventear.
//! - **Inclinar**: [`crate::sla_orient::auto_orient`] busca el giro que deja menos área por capa
//!   (menos fuerza al despegar) y menos succión.

use serde::{Deserialize, Serialize};

use crate::sla::{slice, slice_with, Layer, ResinPrinter, ResinSettings};
use crate::sla_check::{check, slender, Cavity, ResinReport};
use crate::sla_supports::{auto_points, build, frustum, side_points, zone_points, SkipReason, SupportPoint, SupportSettings, Tri};
use crate::SlicerError;

/// Qué hacer al preparar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PrepareSettings {
    /// Soportes (y balsa); `None`: la pieza va directo sobre la placa.
    pub supports: Option<SupportSettings>,
    /// Ventear ventosas y huecos cerrados.
    pub vents: bool,
    pub vent_diameter: f32,
}

impl Default for PrepareSettings {
    fn default() -> Self {
        Self { supports: Some(SupportSettings::default()), vents: true, vent_diameter: 2.5 }
    }
}

/// Un agujero de venteo: columna vertical en `(x, y)` entre `bottom` y `top`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VentHole {
    pub pos: [f32; 2],
    pub bottom: f32,
    pub top: f32,
    pub radius: f32,
}

/// Lo que sale de preparar.
#[derive(Debug, Clone, Default)]
pub struct Prepared {
    /// Cuánto se subió la pieza (mm).
    pub lift: f32,
    /// La pieza ya elevada.
    pub model: Vec<Tri>,
    pub vents: Vec<VentHole>,
    pub points: Vec<SupportPoint>,
    /// Puntos (índice en `points`) que no se pudieron poner.
    pub skipped: Vec<(usize, SkipReason)>,
    pub supports: Vec<Tri>,
    /// Capas finales (pieza + soportes − venteos), listas para escribir el archivo.
    pub layers: Vec<Layer>,
    /// Revisión de las capas finales.
    pub report: ResinReport,
    /// Huecos que no se pudieron ventear (piso en la placa o macizo hasta la placa).
    pub unvented: Vec<Cavity>,
    pub notes: Vec<String>,
    /// Columnas hasta la balsa, puntas inclinadas según la cara y refuerzos.
    pub columns: usize,
    pub tilted: usize,
    pub braces: usize,
}

/// Prepara `model` (en mm de la cama; se respeta su x, y). Con `points` se usan esos
/// puntos de soporte (editados a mano) en vez de los automáticos.
pub fn prepare(
    model: &[Tri],
    printer: &ResinPrinter,
    settings: &ResinSettings,
    prep: &PrepareSettings,
    points: Option<&[SupportPoint]>,
) -> Result<Prepared, SlicerError> {
    if model.is_empty() {
        return Err(SlicerError::Empty);
    }
    let h = settings.layer_height;
    let zmin = model.iter().flatten().map(|v| v[2]).fold(f32::MAX, f32::min);
    let lift = prep.supports.as_ref().map_or(0.0, |s| s.model_bottom()) - zmin;
    let model: Vec<Tri> = model.iter().map(|t| t.map(|v| [v[0], v[1], v[2] + lift])).collect();
    let mut out = Prepared { lift, ..Default::default() };

    // 1. Venteos.
    let mut vent_tris = Vec::new();
    if prep.vents {
        let layers = slice(&model, printer, h)?;
        let report = check(&layers, printer, h);
        for c in report.cavities {
            match vent_for(&c, &layers, printer, h, prep.vent_diameter / 2.0) {
                Some(v) => {
                    // Cáscara dada vuelta: resta al laminar.
                    let mut t = Vec::new();
                    frustum(&mut t, [v.pos[0], v.pos[1], v.bottom], v.radius, [v.pos[0], v.pos[1], v.top], v.radius, 16);
                    vent_tris.extend(t.into_iter().map(|t| [t[0], t[2], t[1]]));
                    out.vents.push(v);
                }
                None => out.unvented.push(c),
            }
        }
        if !out.unvented.is_empty() {
            out.notes.push(format!(
                "{} hueco(s) sin salida que no se pudieron ventear por abajo: subir la pieza con soportes o inclinarla",
                out.unvented.len()
            ));
        }
    }
    let mut part = model.clone();
    part.extend_from_slice(&vent_tris);

    // 2. Soportes.
    if let Some(s) = &prep.supports {
        let tip = s.tip_diameter / 2.0 + 0.2;
        let vent_at = |p: &[f32; 3]| {
            out.vents.iter().find(|v| {
                (p[0] - v.pos[0]).powi(2) + (p[1] - v.pos[1]).powi(2) < (v.radius + tip).powi(2) && p[2] > v.bottom - 1.0 && p[2] < v.top + 1.0
            })
        };
        let near_vent = |p: &[f32; 3]| vent_at(p).is_some();
        // Un punto sobre un venteo se corre al borde del agujero.
        let adjust = |p: &[f32; 3]| match vent_at(p) {
            None => Some(*p),
            Some(v) => {
                let (dx, dy) = (p[0] - v.pos[0], p[1] - v.pos[1]);
                let d = (dx * dx + dy * dy).sqrt().max(1e-3);
                let r = v.radius + tip + 0.05;
                let q = [v.pos[0] + dx / d * r, v.pos[1] + dy / d * r, p[2]];
                (!near_vent(&q)).then_some(q)
            }
        };
        let pts = match points {
            // Los editados ya traen sus laterales; las zonas se aplican igual.
            Some(p) => zone_points(&model, p, s, adjust),
            None => {
                let layers = slice(&part, printer, h)?;
                let report = check(&layers, printer, h);
                let mut pts = auto_points(&model, &report.islands, s, adjust);
                if s.side_supports {
                    let anchors: Vec<[f32; 3]> = pts.iter().map(|p| p.pos).collect();
                    let inside = slender(&layers, printer, h, &anchors, s.side_max_area, s.side_every);
                    let side = side_points(&model, &inside, s);
                    if side.len() < inside.len() {
                        out.notes.push(format!("{} parte(s) delgada(s) sin lugar para un soporte lateral", inside.len() - side.len()));
                    }
                    pts.extend(side.into_iter().filter(|p| !near_vent(&p.pos)));
                }
                pts
            }
        };
        let sup = build(&model, &pts, s, |x, y, z| near_vent(&[x, y, z]));
        out.skipped = sup.skipped;
        (out.columns, out.tilted, out.braces) = (sup.pillars, sup.tilted, sup.braces);
        out.points = pts;
        out.supports = sup.triangles;
    }

    // 3. Laminado final y revisión.
    let mut all = part;
    all.extend_from_slice(&out.supports);
    out.layers = slice_with(&all, printer, settings)?;
    out.report = check(&out.layers, printer, h);
    if !out.report.islands.is_empty() {
        out.notes.push(format!("quedan {} isla(s) sin soporte", out.report.islands.len()));
    }
    out.model = model;
    Ok(out)
}

/// Columna de venteo desde el piso del hueco hacia abajo hasta el primer aire.
fn vent_for(c: &Cavity, layers: &[Layer], printer: &ResinPrinter, h: f32, radius: f32) -> Option<VentHole> {
    if c.on_plate || c.first_layer == 0 {
        return None;
    }
    let (px, py) = printer.mm_to_pixel(c.floor[0], c.floor[1])?;
    let mut j = c.first_layer - 1;
    while lit(&layers[j], px, py) {
        if j == 0 {
            return None;
        }
        j -= 1;
    }
    // Desde arriba del medio de la capa de aire j hasta arriba del medio de la última del piso.
    Some(VentHole {
        pos: [c.floor[0], c.floor[1]],
        bottom: (j as f32 + 0.75) * h,
        top: (c.first_layer as f32 + 0.25) * h,
        radius,
    })
}

/// Si el píxel está encendido en la capa.
fn lit(l: &Layer, px: u32, py: u32) -> bool {
    let i = l.spans.partition_point(|s| (s.y, s.x1) <= (py, px));
    l.spans.get(i).is_some_and(|s| s.y == py && s.x0 <= px)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sla_supports::{self, PointKind, SupportZone, ZoneKind};
    use crate::sla::tests::cube;
    use crate::sla::LD_002H;

    fn inverted(t: Vec<Tri>) -> Vec<Tri> {
        t.into_iter().map(|t| [t[0], t[2], t[1]]).collect()
    }

    /// Caja de 10 mm con hueco cerrado de 6 mm.
    fn hollow_box() -> Vec<Tri> {
        let mut t = cube([20.0, 20.0, 0.0], [10.0, 10.0, 10.0]);
        t.extend(inverted(cube([22.0, 22.0, 2.0], [6.0, 6.0, 6.0])));
        t
    }

    #[test]
    fn caja_hueca_queda_venteada_y_soportada() {
        let r = prepare(&hollow_box(), &LD_002H, &ResinSettings::default(), &PrepareSettings::default(), None).unwrap();
        assert_eq!(r.vents.len(), 1, "{:?}", r.notes);
        assert!(r.unvented.is_empty());
        assert!(r.report.cavities.is_empty(), "{:?} {:?}", r.report.cavities, r.vents);
        assert!(r.report.islands.is_empty(), "{:?}", r.report.islands);
        assert!(!r.points.is_empty());
        assert!((r.lift - 6.0).abs() < 1e-5);
        // El venteo atraviesa el piso (z 6–8) y no corta las puntas de abajo.
        let v = r.vents[0];
        assert!(v.bottom < 6.0 && v.bottom > 5.9 && v.top > 8.0 && v.top < 8.1, "{v:?}");
    }

    #[test]
    fn sin_soportes_el_hueco_en_la_placa_no_se_ventea() {
        let prep = PrepareSettings { supports: None, ..Default::default() };
        // Vaso boca arriba sobre la placa: el piso es macizo hasta la placa.
        let mut t = cube([20.0, 20.0, 0.0], [10.0, 10.0, 6.0]);
        t.extend(inverted(cube([22.0, 22.0, 2.0], [6.0, 6.0, 6.0])));
        let r = prepare(&t, &LD_002H, &ResinSettings::default(), &prep, None).unwrap();
        assert!(r.vents.is_empty());
        assert_eq!(r.unvented.len(), 1);
        assert!(!r.notes.is_empty());
    }

    fn count(r: &Prepared, kind: PointKind) -> usize {
        r.points.iter().filter(|p| p.kind == kind).count()
    }

    #[test]
    fn zonas_sin_soportes_y_con_mas() {
        let plate = cube([20.0, 20.0, 0.0], [20.0, 20.0, 3.0]);
        let base = prepare(&plate, &LD_002H, &ResinSettings::default(), &PrepareSettings::default(), None).unwrap();
        let bottom = base.points[0].pos[2];
        let in_zone = |r: &Prepared, c: [f32; 3], rad: f32| r.points.iter().filter(|p| sla_supports::zone_at(&[SupportZone { center: c, radius: rad, kind: ZoneKind::Block }], &p.pos).is_some()).count();
        let c = [25.0, 25.0, bottom];
        let before = in_zone(&base, c, 4.0);
        assert!(before > 0);

        let with = |kind| {
            let mut prep = PrepareSettings::default();
            prep.supports.as_mut().unwrap().zones = vec![SupportZone { center: c, radius: 4.0, kind }];
            prep
        };
        let blocked = prepare(&plate, &LD_002H, &ResinSettings::default(), &with(ZoneKind::Block), None).unwrap();
        assert_eq!(in_zone(&blocked, c, 4.0), 0);
        let dense = prepare(&plate, &LD_002H, &ResinSettings::default(), &with(ZoneKind::Reinforce), None).unwrap();
        assert!(in_zone(&dense, c, 4.0) > before * 2, "{} vs {before}", in_zone(&dense, c, 4.0));

        // Sobre puntos editados: saca y agrega igual.
        let edited = prepare(&plate, &LD_002H, &ResinSettings::default(), &with(ZoneKind::Block), Some(&base.points)).unwrap();
        assert_eq!(in_zone(&edited, c, 4.0), 0);
        assert!(edited.points.len() < base.points.len());
        let edited = prepare(&plate, &LD_002H, &ResinSettings::default(), &with(ZoneKind::Reinforce), Some(&base.points)).unwrap();
        assert!(edited.points.len() > base.points.len());
    }

    #[test]
    fn un_pin_alto_y_delgado_lleva_laterales() {
        let pin = cube([30.0, 30.0, 0.0], [1.2, 1.2, 26.0]);
        let mut prep = PrepareSettings::default();
        let r = prepare(&pin, &LD_002H, &ResinSettings::default(), &prep, None).unwrap();
        assert_eq!(count(&r, PointKind::Side), 0);
        prep.supports.as_mut().unwrap().side_supports = true;
        let r = prepare(&pin, &LD_002H, &ResinSettings::default(), &prep, None).unwrap();
        let side: Vec<&SupportPoint> = r.points.iter().filter(|p| p.kind == PointKind::Side).collect();
        assert_eq!(side.len(), 2, "{:?} {:?}", r.points, r.notes);
        assert!(r.skipped.is_empty(), "{:?}", r.skipped);
        assert!(r.report.islands.is_empty());
        // En la pared del pin, uno cada 10 mm.
        for p in &side {
            let wall = [p.pos[0] - 30.0, p.pos[0] - 31.2, p.pos[1] - 30.0, p.pos[1] - 31.2].iter().any(|d| d.abs() < 0.01);
            assert!(wall, "{p:?}");
        }
        assert!((side[1].pos[2] - side[0].pos[2] - 10.0).abs() < 0.2, "{side:?}");
        assert!(r.tilted >= 2);
    }
}
