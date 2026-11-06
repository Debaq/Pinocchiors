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
use crate::sla_hollow::{drain_holes, plugs, Hollow, HollowSettings};
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
    /// Vaciar la pieza (SLA14); `None`: maciza.
    pub hollow: Option<HollowSettings>,
    /// Drenajes puestos a mano (mm de la impresora, como los puntos de soporte).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub drains: Vec<[f32; 3]>,
    pub drain_diameter: f32,
    /// Un tapón imprimible por drenaje, en la placa al lado de la balsa.
    pub plugs: bool,
}

impl Default for PrepareSettings {
    fn default() -> Self {
        Self {
            supports: Some(SupportSettings::default()),
            vents: true,
            vent_diameter: 2.5,
            hollow: None,
            drains: Vec::new(),
            drain_diameter: 3.0,
            plugs: true,
        }
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
    /// Resina que se ahorró al vaciar (ml) y tapones puestos en la placa.
    pub hollowed_ml: f32,
    pub plugs: usize,
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

    // 0. Vaciado y drenajes a mano: el interior sale de las capas de la pieza sola y se
    // apaga en cada laminado que sigue.
    let base = slice(&model, printer, h)?;
    let hollow = prep.hollow.as_ref().and_then(|hs| Hollow::new(&base, printer, h, hs));
    if prep.hollow.is_some() && hollow.is_none() {
        out.notes.push("la pieza es muy delgada para vaciarla con esa pared".into());
    }
    let wall = prep.hollow.as_ref().map_or(2.0, |hs| hs.wall);
    let mut part = model.clone();
    part.extend(drain_holes(&model, &prep.drains, prep.drain_diameter, wall + 1.0));
    let cut = |mut layers: Vec<Layer>| {
        if let Some(hw) = &hollow {
            hw.apply(&mut layers);
        }
        layers
    };
    let sliced = |tris: &[Tri]| -> Result<Vec<Layer>, SlicerError> { Ok(cut(slice(tris, printer, h)?)) };

    // 1. Venteos, en rondas: un hueco puede ser varios bolsillos que se juntan más arriba;
    // con el primer venteo se ve cuál sigue succionando.
    let mut vent_tris = Vec::new();
    if prep.vents {
        let mut layers = if prep.drains.is_empty() { cut(base) } else { sliced(&part)? };
        for _ in 0..4 {
            let report = check(&layers, printer, h);
            out.unvented.clear();
            let mut added = 0;
            for c in report.cavities {
                // Un venteo por bolsillo (salvo que ya haya uno ahí mismo).
                let mut any = false;
                for floor in &c.pockets {
                    let Some(v) = vent_for(*floor, &c, &layers, printer, h, prep.vent_diameter / 2.0) else { continue };
                    any = true;
                    if out.vents.iter().any(|o| (o.pos[0] - v.pos[0]).hypot(o.pos[1] - v.pos[1]) < 2.0 * o.radius && v.bottom < o.top && o.bottom < v.top) {
                        continue;
                    }
                    // Cáscara dada vuelta: resta al laminar.
                    let mut t = Vec::new();
                    frustum(&mut t, [v.pos[0], v.pos[1], v.bottom], v.radius, [v.pos[0], v.pos[1], v.top], v.radius, 16);
                    vent_tris.extend(t.into_iter().map(|t| [t[0], t[2], t[1]]));
                    out.vents.push(v);
                    added += 1;
                }
                if !any {
                    out.unvented.push(c);
                }
            }
            if added == 0 {
                break;
            }
            let mut with = part.clone();
            with.extend_from_slice(&vent_tris);
            layers = sliced(&with)?;
        }
        if !out.unvented.is_empty() {
            out.notes.push(format!(
                "{} hueco(s) sin salida que no se pudieron ventear por abajo: subir la pieza con soportes o inclinarla",
                out.unvented.len()
            ));
        }
    }
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
        // Drenajes a mano: ningún punto adentro ni pegado al borde (el agujero se come la punta).
        let drain_r = prep.drain_diameter / 2.0 + tip + 0.3;
        let drain_at = |p: &[f32; 3]| prep.drains.iter().find(|d| (0..3).map(|c| (p[c] - d[c]).powi(2)).sum::<f32>() < drain_r * drain_r);
        let near_hole = |p: &[f32; 3]| near_vent(p) || drain_at(p).is_some();
        // Un punto sobre un venteo o un drenaje se corre al borde del agujero.
        let away = |p: &[f32; 3], c: [f32; 2], r: f32| {
            let (dx, dy) = (p[0] - c[0], p[1] - c[1]);
            let d = (dx * dx + dy * dy).sqrt().max(1e-3);
            [c[0] + dx / d * r, c[1] + dy / d * r, p[2]]
        };
        let adjust = |p: &[f32; 3]| {
            let mut q = *p;
            if let Some(v) = vent_at(&q) {
                q = away(&q, v.pos, v.radius + tip + 0.05);
            }
            if let Some(d) = drain_at(&q) {
                q = away(&q, [d[0], d[1]], drain_r + 0.05);
            }
            (!near_hole(&q)).then_some(q)
        };
        let pts = match points {
            // Los editados ya traen sus laterales; las zonas se aplican igual.
            Some(p) => zone_points(&model, p, s, adjust),
            None => {
                let layers = sliced(&part)?;
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
        let sup = build(&model, &pts, s, |x, y, z| near_hole(&[x, y, z]));
        out.skipped = sup.skipped;
        (out.columns, out.tilted, out.braces) = (sup.pillars, sup.tilted, sup.braces);
        out.points = pts;
        out.supports = sup.triangles;
    }

    // Tapones de los drenajes, parados en la placa al lado de lo que ya hay.
    if prep.plugs && !prep.drains.is_empty() {
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        for v in model.iter().chain(&out.supports).flatten() {
            for c in 0..2 {
                lo[c] = lo[c].min(v[c]);
                hi[c] = hi[c].max(v[c]);
            }
        }
        let (t, n) = plugs(prep.drains.len(), prep.drain_diameter, wall, 0.1, (lo, hi), printer);
        out.supports.extend(t);
        out.plugs = n;
        if n < prep.drains.len() {
            out.notes.push(format!("{} tapón(es) no entraron en la placa", prep.drains.len() - n));
        }
    }

    // 3. Laminado final y revisión.
    let mut all = part;
    all.extend_from_slice(&out.supports);
    out.layers = slice_with(&all, printer, settings)?;
    if let Some(hw) = &hollow {
        let (pw, ph) = printer.pixel_size();
        out.hollowed_ml = hw.apply(&mut out.layers) as f32 * pw * ph * h / 1000.0;
    }
    out.report = check(&out.layers, printer, h);
    if !out.report.islands.is_empty() {
        out.notes.push(format!("quedan {} isla(s) sin soporte", out.report.islands.len()));
    }
    out.model = model;
    Ok(out)
}

/// Columna de venteo desde `floor` (el piso de un bolsillo del hueco) hacia abajo hasta el primer aire.
fn vent_for(floor: [f32; 3], c: &Cavity, layers: &[Layer], printer: &ResinPrinter, h: f32, radius: f32) -> Option<VentHole> {
    // La capa del piso (la del bolsillo, que puede estar más arriba que la primera del hueco).
    let first = ((floor[2] / h).round() as usize).max(c.first_layer);
    if c.on_plate || first == 0 {
        return None;
    }
    let (px, py) = printer.mm_to_pixel(floor[0], floor[1])?;
    let mut j = first - 1;
    while lit(&layers[j], px, py) {
        if j == 0 {
            return None;
        }
        j -= 1;
    }
    // Desde arriba del medio de la capa de aire j hasta arriba del medio de la última del piso.
    Some(VentHole {
        pos: [floor[0], floor[1]],
        bottom: (j as f32 + 0.75) * h,
        top: (first as f32 + 0.25) * h,
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
    use crate::sla_hollow::HollowSettings;
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

    #[test]
    fn cubo_vaciado_queda_venteado_y_ahorra_resina() {
        let c = cube([20.0, 20.0, 0.0], [20.0, 20.0, 20.0]);
        let solid = prepare(&c, &LD_002H, &ResinSettings::default(), &PrepareSettings::default(), None).unwrap();
        let prep = PrepareSettings { hollow: Some(HollowSettings::default()), ..Default::default() };
        let r = prepare(&c, &LD_002H, &ResinSettings::default(), &prep, None).unwrap();
        assert!(r.hollowed_ml > 2.5, "{}", r.hollowed_ml);
        assert_eq!(r.vents.len(), 1, "{:?}", r.notes);
        assert!(r.report.cavities.is_empty() && r.report.islands.is_empty(), "{:?} {:?}", r.report.cavities, r.report.islands);
        let ml = |p: &Prepared| crate::sla::volume_ml(&p.layers, &LD_002H, 0.05);
        assert!((ml(&solid) - ml(&r) - r.hollowed_ml).abs() < 0.3, "{} {} {}", ml(&solid), ml(&r), r.hollowed_ml);
    }

    #[test]
    fn drenaje_a_mano_con_tapon() {
        let c = cube([20.0, 20.0, 0.0], [20.0, 20.0, 20.0]);
        // En la cara de abajo (z = 0 antes de elevar: la de la impresora es lift + 0).
        let lift = SupportSettings::default().model_bottom();
        let prep = PrepareSettings { hollow: Some(HollowSettings::default()), drains: vec![[26.0, 26.0, lift]], ..Default::default() };
        let r = prepare(&c, &LD_002H, &ResinSettings::default(), &prep, None).unwrap();
        // El drenaje abajo ya ventea el hueco: no hace falta otro.
        assert!(r.vents.is_empty(), "{:?}", r.vents);
        assert!(r.report.cavities.is_empty(), "{:?}", r.report.cavities);
        assert_eq!(r.plugs, 1);
        assert!(r.report.islands.is_empty(), "{:?}", r.report.islands);
    }
}
