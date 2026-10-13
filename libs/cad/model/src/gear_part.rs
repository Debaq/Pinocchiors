//! Pieza «Engrane»: el perfil de [`crate::gears`] hecho sólido, con cubo,
//! agujero del eje, chavetero DIN 6885, aligeramientos y holgura de impresión.
//!
//! Marco: el eje del engrane es Z, la cara de abajo está en Z=0 y el engrane
//! crece hacia +Z; el cubo sale por la cara de arriba. En la cremallera X va
//! a lo largo, Y hacia las cabezas (Y=0 en la línea de rodadura) y se extruye
//! en Z.

use crate::feature::FeatureId;
use crate::geom::{add, cross, dot, normalize, scale, sub};
use crate::gears::{self, GearCircles, GearKind, GearSpec, GearWarning};
use crate::standard::{Local, MARGIN, e};
use cad_occt::{Curve, Frame, P3, Shape};
use serde::{Deserialize, Serialize};
use std::f64::consts::{PI, TAU};

type R<T> = Result<T, String>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GearPart {
    pub spec: GearSpec,
    /// Ancho del diente (mm)
    pub width: f64,
    /// Diámetro del agujero del eje (0: macizo)
    pub bore: f64,
    /// Cubo: diámetro y largo que sobresale de la cara de arriba (0: sin cubo)
    pub hub_diameter: f64,
    pub hub_length: f64,
    /// Chavetero DIN 6885 según el diámetro del eje
    pub keyway: bool,
    /// Corona: diámetro exterior. Cremallera: alto del cuerpo bajo el pie.
    pub outer: f64,
    /// Aligeramientos: agujeros pasantes en círculo
    pub hole_count: u32,
    pub hole_diameter: f64,
    pub hole_circle: f64,
    /// Aligeramiento con rayos: cantidad, ancho de cada rayo y grosor del aro bajo el pie (mm)
    pub spokes: u32,
    pub spoke_width: f64,
    pub rim: f64,
    /// Prisionero: diámetro del agujero radial en el cubo (0: sin él), a 90° del chavetero
    pub set_screw: f64,
    /// Holgura para impresión 3D: juego extra y cabeza más baja (mm)
    pub print_clearance: f64,
    /// La holgura de impresión la da la impresora del documento (ajuste justo); `print_clearance` se ignora
    pub auto_clearance: bool,
    /// Cuánto más chico sale un agujero, por lado (mm): agranda el eje, el chavetero y los aligeramientos.
    /// No se guarda: lo pone el cálculo cuando el diseño compensa para imprimir.
    #[serde(skip)]
    pub hole_comp: f64,
    /// Doble helicoidal (espina de pescado): dos mitades de mano contraria
    pub herringbone: bool,
    /// Canal central de la espina de pescado (mm; 0: sin canal)
    pub groove: f64,
    /// El módulo de la receta es el transversal (el del plano de giro), no el normal: el normal
    /// sale de `mt·cos β`. Solo importa con hélice; cónico y sin fin lo ignoran.
    pub transverse_module: bool,
    /// Cónico: las caras de atrás y de adelante son conos perpendiculares al cono primitivo (como en un
    /// cónico de verdad) en vez de planos perpendiculares al eje. Sale de una booleana con el cuerpo.
    pub cone_ends: bool,
    /// Rueda de sin fin: distancia entre centros con el sin fin (0: sin garganta).
    /// La rueda es cóncava para abrazar el filete; la pone `mesh` sola.
    pub throat: f64,
    /// Engranar con otro engrane anterior del historial (ver [`mesh`])
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<GearLink>,
}

/// Coordinación entre engranes: este engrane toma el módulo y el ángulo de
/// presión del otro, se pone a la distancia entre centros y gira lo que hace
/// falta para que los dientes calcen. No es una booleana, como `ThreadLink`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GearLink {
    pub feature: FeatureId,
    /// Dirección del centro de este engrane visto desde el otro: grados desde
    /// el X del otro, antihorario alrededor de su eje
    #[serde(default)]
    pub angle: f64,
    /// Corrimiento axial respecto de la cara de abajo del otro (mm)
    #[serde(default)]
    pub offset: f64,
    /// Distancia entre centros pedida: calcula la corrección de perfil de
    /// este engrane que la cumple (sin ella, la que sale de las correcciones)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distance: Option<f64>,
    /// Con una cremallera como fuente: dónde queda el piñón a lo largo de ella (mm desde su centro)
    #[serde(default, skip_serializing_if = "is_zero")]
    pub along: f64,
}

fn is_zero(x: &f64) -> bool {
    *x == 0.0
}

/// Un engrane ya calculado: su marco (origen en el centro de la cara de
/// abajo, Z el eje y X el centro de un hueco), para engranar otros con él.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GearAxis {
    pub feature: FeatureId,
    pub part: GearPart,
    pub origin: P3,
    pub z: P3,
    pub x: P3,
    /// Radio primitivo y distancia al engrane con el que engrana (0: ninguno)
    pub pitch_radius: f64,
    pub center_distance: f64,
}

impl Default for GearPart {
    fn default() -> Self {
        Self {
            spec: GearSpec::default(),
            width: 8.0,
            bore: 0.0,
            hub_diameter: 0.0,
            hub_length: 0.0,
            keyway: false,
            outer: 0.0,
            hole_count: 0,
            hole_diameter: 0.0,
            hole_circle: 0.0,
            spokes: 0,
            spoke_width: 0.0,
            rim: 0.0,
            set_screw: 0.0,
            print_clearance: 0.0,
            auto_clearance: false,
            hole_comp: 0.0,
            herringbone: false,
            groove: 0.0,
            transverse_module: false,
            cone_ends: false,
            throat: 0.0,
            link: None,
        }
    }
}

/// Chavetero paralelo DIN 6885-1: ancho, y profundidad en el cubo (t2)
pub fn keyway_for(bore: f64) -> Option<(f64, f64)> {
    const TABLE: &[(f64, f64, f64)] = &[
        (8.0, 2.0, 1.0),
        (10.0, 3.0, 1.4),
        (12.0, 4.0, 1.8),
        (17.0, 5.0, 2.3),
        (22.0, 6.0, 2.8),
        (30.0, 8.0, 3.3),
        (38.0, 10.0, 3.3),
        (44.0, 12.0, 3.3),
        (50.0, 14.0, 3.8),
        (58.0, 16.0, 4.3),
        (65.0, 18.0, 4.4),
        (75.0, 20.0, 4.9),
    ];
    if bore <= 6.0 {
        return None;
    }
    TABLE.iter().find(|(max, _, _)| bore <= *max).map(|&(_, b, t2)| (b, t2))
}

impl GearPart {
    /// Receta con la holgura de impresión ya aplicada
    pub fn effective_spec(&self) -> GearSpec {
        let mut s = self.spec;
        if self.transverse_module && matches!(s.kind, GearKind::External | GearKind::Internal | GearKind::Rack) {
            s.module *= s.helix_angle.to_radians().cos();
        }
        if self.print_clearance > 0.0 {
            s.backlash += self.print_clearance;
            s.addendum = (s.addendum - self.print_clearance / s.module).max(0.05);
        }
        s
    }

    fn validate(&self) -> R<()> {
        let s = &self.spec;
        if !(self.width > 0.0) {
            return Err("el ancho del engrane debe ser positivo".into());
        }
        if self.herringbone && (s.helix_angle == 0.0 || self.groove < 0.0 || self.groove >= self.width) {
            return Err("la espina de pescado pide ángulo de hélice y un canal menor que el ancho".into());
        }
        if self.bore < 0.0 || self.hub_diameter < 0.0 || self.hub_length < 0.0 || self.print_clearance < 0.0 {
            return Err("las medidas no pueden ser negativas".into());
        }
        let c = self.effective_spec().circles().map_err(|e| e.to_string())?;
        match s.kind {
            GearKind::External => {
                if self.bore > 0.0 && self.bore >= 2.0 * c.root - 1.0 {
                    return Err(format!("el agujero ({}) no cabe bajo el pie del diente (Ø{:.2})", self.bore, 2.0 * c.root));
                }
                if self.hub_length > 0.0 && self.hub_diameter <= self.bore {
                    return Err("el cubo tiene que ser más ancho que el agujero".into());
                }
            }
            GearKind::Internal => {
                if self.outer <= 2.0 * c.root + 1.0 {
                    return Err(format!("el diámetro exterior debe superar el del pie (Ø{:.2}) en al menos 0,5 mm por lado", 2.0 * c.root));
                }
            }
            GearKind::Rack => {
                if self.outer <= 0.0 {
                    return Err("la cremallera necesita el alto del cuerpo".into());
                }
            }
            GearKind::Worm => {
                let (r_root, ..) = s.worm_profile().map_err(|e| e.to_string())?;
                // El «cubo» del sin fin es un vástago liso después del filete
                if self.hub_length > 0.0 && (self.hub_diameter <= self.bore || self.hub_diameter > 2.0 * r_root) {
                    return Err(format!("el vástago tiene que ser más ancho que el agujero y no pasar del núcleo del sin fin (Ø{:.2})", 2.0 * r_root));
                }
                if self.bore > 0.0 && self.bore >= 2.0 * r_root - 1.0 {
                    return Err(format!("el agujero ({}) no cabe bajo el núcleo del sin fin (Ø{:.2})", self.bore, 2.0 * r_root));
                }
                if self.width < 2.0 * s.worm_dims().2 {
                    return Err("el sin fin tiene que medir al menos dos avances de largo".into());
                }
            }
            GearKind::Bevel => {
                let cone = s.cone_distance();
                if self.width >= cone * 0.6 {
                    return Err(format!("el ancho de cara ({}) tiene que ser bastante menor que la generatriz del cono ({:.2})", self.width, cone));
                }
                // El agujero y el cubo van bajo el pie del extremo chico
                let small = c.root * (1.0 - self.width / cone);
                if self.bore > 0.0 && self.bore >= 2.0 * small - 1.0 {
                    return Err(format!("el agujero ({}) no cabe bajo el pie del extremo chico (Ø{:.2})", self.bore, 2.0 * small));
                }
                if self.hub_length > 0.0 && (self.hub_diameter <= self.bore || self.hub_diameter >= 2.0 * small) {
                    return Err("el cubo tiene que ser más ancho que el agujero y caber bajo el pie del extremo chico".into());
                }
            }
        }
        if self.keyway && (!matches!(s.kind, GearKind::External | GearKind::Bevel | GearKind::Worm) || keyway_for(self.bore).is_none()) {
            return Err("el chavetero pide un agujero de eje de más de 6 mm hasta 75 mm".into());
        }
        // Radio de pie donde el cuerpo es más angosto (el cónico se achica hacia el vértice)
        let narrow = if s.kind == GearKind::Bevel { c.root * (1.0 - self.width / s.cone_distance()) } else { c.root };
        if self.hole_count > 0 {
            if !matches!(s.kind, GearKind::External | GearKind::Bevel) {
                return Err("los aligeramientos van en engranes exteriores y cónicos".into());
            }
            let (r_in, r_out) = (self.hole_circle / 2.0 - self.hole_diameter / 2.0, self.hole_circle / 2.0 + self.hole_diameter / 2.0);
            let hub_r = (self.bore / 2.0).max(self.hub_diameter / 2.0);
            if self.hole_diameter <= 0.0 || r_in <= hub_r || r_out >= narrow - 0.5 {
                return Err("los agujeros de aligeramiento tienen que quedar entre el cubo y el pie del diente".into());
            }
        }
        if self.spokes > 0 {
            if !matches!(s.kind, GearKind::External | GearKind::Bevel) || self.hole_count > 0 {
                return Err("los rayos van en engranes exteriores y cónicos, y no se combinan con los agujeros de aligeramiento".into());
            }
            if self.spokes < 2 || self.spoke_width <= 0.0 || self.rim <= 0.0 {
                return Err("los rayos piden al menos 2, un ancho de rayo y un grosor de aro".into());
            }
            let hub_r = (self.bore / 2.0).max(self.hub_diameter / 2.0) + 1.0;
            if hub_r + 1.0 >= narrow - self.rim || self.spoke_width * self.spokes as f64 >= TAU * hub_r {
                return Err("los rayos no caben: baja el aro o el ancho, o agranda el engrane".into());
            }
        }
        if self.set_screw > 0.0 {
            if !matches!(s.kind, GearKind::External | GearKind::Bevel | GearKind::Worm) || self.hub_length <= 0.0 || self.bore <= 0.0 {
                return Err("el prisionero va en el cubo (o el vástago del sin fin) de un engrane con agujero".into());
            }
            if self.set_screw >= (self.hub_diameter - self.bore) / 2.0 || self.set_screw >= self.hub_length {
                return Err("el prisionero es más grande que la pared o el largo del cubo".into());
            }
        }
        Ok(())
    }
}

fn world_point(l: &Local, p: P3) -> P3 {
    l.p(p[0], p[1], p[2])
}

fn world_dir(l: &Local, d: P3) -> P3 {
    [
        l.x[0] * d[0] + l.y[0] * d[1] + l.z[0] * d[2],
        l.x[1] * d[0] + l.y[1] * d[1] + l.z[1] * d[2],
        l.x[2] * d[0] + l.y[2] * d[1] + l.z[2] * d[2],
    ]
}

fn to_world(l: &Local, c: &Curve) -> Curve {
    let p = |q: &P3| world_point(l, *q);
    match c {
        Curve::Arc(a, m, b) => Curve::Arc(p(a), p(m), p(b)),
        Curve::Line(a, b) => Curve::Line(p(a), p(b)),
        Curve::SplineEnds { points, start, end } => {
            Curve::SplineEnds { points: points.iter().map(p).collect(), start: world_dir(l, *start), end: world_dir(l, *end) }
        }
        other => other.clone(),
    }
}

/// Perfil girado `rot0` alrededor del eje y barrido desde la altura `z0`
/// hasta `z0 + len`, girando `twist` en el camino (0: prisma recto).
fn prism(loop_curves: &[Curve], l: &Local, z0: f64, z1: f64) -> R<Shape> {
    twisted(loop_curves, l, z0, z1 - z0, 0.0, 0.0)
}

fn twisted(loop_curves: &[Curve], l: &Local, z0: f64, len: f64, rot0: f64, twist: f64) -> R<Shape> {
    let world = |curves: &[Curve], rot: f64, dz: f64| -> Vec<Curve> {
        curves
            .iter()
            .map(|c| {
                let c = if rot != 0.0 { gears::rotate_curve(c, rot) } else { c.clone() };
                shift(&to_world(l, &c), crate::geom::scale(l.z, dz))
            })
            .collect()
    };
    let cap0 = Shape::face(&[world(loop_curves, rot0, z0)]).map_err(e)?;
    if twist.abs() < 1e-12 {
        return cap0.prism(crate::geom::scale(l.z, len)).map_err(e);
    }
    // Secciones intermedias cada ~4°: la hélice entre ellas queda a micras
    let steps = ((twist.abs() / (PI / 45.0)).ceil() as usize + 1).max(3);
    let section = |curves: &[Curve], i: usize| {
        let f = i as f64 / steps as f64;
        Shape::wire(&world(curves, rot0 + twist * f, z0 + len * f)).map_err(e)
    };
    let sections = (0..=steps).map(|i| section(loop_curves, i)).collect::<R<Vec<_>>>()?;
    Shape::loft(&sections, true, false).map_err(e)
}

/// Dientes helicoidales de `z0` a `z1`: gira `rate` rad/mm (una espina de
/// pescado usa dos mitades de mano contraria, con un canal en medio).
fn helical(curves: &[Curve], l: &Local, spec: &GearSpec, g: &GearPart, z0: f64, z1: f64, root: f64) -> R<Shape> {
    let pitch_r = spec.circles().map_err(|e| e.to_string())?.pitch;
    let rate = spec.helix_angle.to_radians().tan() / pitch_r;
    if rate == 0.0 {
        return prism(curves, l, z0, z1);
    }
    if !g.herringbone {
        return twisted(curves, l, z0, z1 - z0, rate * z0, rate * (z1 - z0));
    }
    // El ancho de verdad es [0, width]; la herramienta de una corona la excede
    let (w, groove) = (g.width, g.groove);
    if groove == 0.0 {
        return vee(curves, l, z0.min(0.0), z1.max(w), w, rate);
    }
    let half = (w - groove) / 2.0;
    let lower = twisted(curves, l, z0.min(0.0), half - z0.min(0.0), rate * z0.min(0.0), rate * (half - z0.min(0.0)))?;
    let top0 = half + groove;
    let upper_end = z1.max(w);
    let upper = twisted(curves, l, top0, upper_end - top0, rate * (w - top0), -rate * (upper_end - top0))?;
    // Canal: el núcleo hasta el círculo de pie
    let core = Shape::cylinder(l.at(half - MARGIN, false), root, groove + 2.0 * MARGIN).map_err(e)?;
    Shape::fuse_all(&[lower, upper, core]).map_err(e)
}

/// Espina de pescado sin canal de `z0` a `z1`: el giro sube `rate` rad/mm hasta la mitad de `w` y
/// baja después. Un solo loft por secciones (con el vértice en medio) en vez de dos mitades unidas:
/// unir dos superficies libres que se tocan por una cara tardaba segundos.
fn vee(curves: &[Curve], l: &Local, z0: f64, z1: f64, w: f64, rate: f64) -> R<Shape> {
    let apex = w / 2.0;
    let turn = |z: f64| rate * (apex - (z - apex).abs());
    // Secciones cada ~4° como en `twisted`; el loft suave pasa por el vértice sin pasarse (< 0,01 % de volumen)
    let per_mm = rate.abs() / (PI / 45.0);
    let half = |a: f64, b: f64| ((b - a).abs() * per_mm).ceil().max(1.0) as usize;
    let mut zs: Vec<f64> = (0..=half(z0, apex)).map(|i| z0 + (apex - z0) * i as f64 / half(z0, apex) as f64).collect();
    zs.extend((1..=half(apex, z1)).map(|i| apex + (z1 - apex) * i as f64 / half(apex, z1) as f64));
    let sections = zs
        .iter()
        .map(|&z| {
            let rotated: Vec<Curve> = curves.iter().map(|c| shift(&to_world(l, &gears::rotate_curve(c, turn(z))), crate::geom::scale(l.z, z))).collect();
            Shape::wire(&rotated).map_err(e)
        })
        .collect::<R<Vec<_>>>()?;
    Shape::loft(&sections, true, false).map_err(e)
}

fn shift(c: &Curve, v: P3) -> Curve {
    let s = |p: &P3| [p[0] + v[0], p[1] + v[1], p[2] + v[2]];
    match c {
        Curve::Arc(a, m, b) => Curve::Arc(s(a), s(m), s(b)),
        Curve::Line(a, b) => Curve::Line(s(a), s(b)),
        Curve::SplineEnds { points, start, end } => Curve::SplineEnds { points: points.iter().map(s).collect(), start: *start, end: *end },
        other => other.clone(),
    }
}

/// Perfil plano escalado hacia el eje (el centro es el origen)
fn scale_curve(c: &Curve, k: f64) -> Curve {
    let m = |p: &P3| [p[0] * k, p[1] * k, p[2]];
    match c {
        Curve::Arc(a, mid, b) => Curve::Arc(m(a), m(mid), m(b)),
        Curve::SplineEnds { points, start, end } => Curve::SplineEnds { points: points.iter().map(m).collect(), start: *start, end: *end },
        other => other.clone(),
    }
}

/// Diámetro del agujero del eje ya compensado
fn bore_fit(g: &GearPart) -> f64 {
    g.bore + 2.0 * g.hole_comp
}

/// Chavetero DIN 6885 para el eje: ancho y profundidad ya compensados
fn keyway_fit(g: &GearPart) -> (f64, f64) {
    let (b, t2) = keyway_for(g.bore).unwrap();
    (b + 2.0 * g.hole_comp, t2 + g.hole_comp)
}

/// Rayos y agujeros de aligeramiento que atraviesan de la altura `z0` a la `z1`, con el aro que queda
/// bajo el pie del extremo más chico (`root`: radio de pie ahí).
fn lighten(mut body: Shape, l: &Local, g: &GearPart, root: f64, z0: f64, z1: f64) -> R<Shape> {
    if g.spokes > 0 {
        // Ventanas entre rayos: el anillo entre el cubo y el aro, menos los rayos
        let r_in = (g.bore / 2.0).max(g.hub_diameter / 2.0) + 1.0;
        let r_out = root - g.rim;
        let ring = cyl(l, 2.0 * r_out, z0 - MARGIN, z1 + MARGIN)?.cut(&cyl(l, 2.0 * r_in, z0 - 2.0 * MARGIN, z1 + 2.0 * MARGIN)?).map_err(e)?;
        let mut bars = Vec::new();
        for k in 0..g.spokes {
            let a = TAU * k as f64 / g.spokes as f64;
            let f = Frame { origin: l.p(0.0, 0.0, z0 - 2.0 * MARGIN), z: l.z, x: add(crate::geom::scale(l.x, a.cos()), crate::geom::scale(l.y, a.sin())) };
            bars.push(Shape::make_box(Frame { origin: add(f.origin, crate::geom::scale(cross(f.z, f.x), -g.spoke_width / 2.0)), ..f }, r_out + 1.0, g.spoke_width, z1 - z0 + 4.0 * MARGIN).map_err(e)?);
        }
        let windows = ring.cut(&Shape::fuse_all(&bars).map_err(e)?).map_err(e)?;
        body = body.cut(&windows).map_err(e)?;
    }
    for k in 0..g.hole_count {
        let a = TAU * k as f64 / g.hole_count as f64;
        let f = Frame { origin: l.p(g.hole_circle / 2.0 * a.cos(), g.hole_circle / 2.0 * a.sin(), z0 - MARGIN), z: l.z, x: l.x };
        let hole = Shape::cylinder(f, (g.hole_diameter + 2.0 * g.hole_comp) / 2.0, z1 - z0 + 2.0 * MARGIN).map_err(e)?;
        body = body.cut(&hole).map_err(e)?;
    }
    Ok(body)
}

/// Prisionero: agujero radial por el cubo (o el vástago) hasta el eje, hacia +Y, a la altura `mid`.
fn set_screw_hole(body: Shape, l: &Local, g: &GearPart, mid: f64) -> R<Shape> {
    let f = Frame { origin: l.p(0.0, 0.0, mid), z: l.y, x: l.x };
    body.cut(&Shape::cylinder(f, g.set_screw / 2.0, g.hub_diameter / 2.0 + MARGIN).map_err(e)?).map_err(e)
}

fn cyl(l: &Local, d: f64, z0: f64, z1: f64) -> R<Shape> {
    Shape::cylinder(l.at(z0, false), d / 2.0, z1 - z0).map_err(e)
}

/// Sólido del engrane en el marco dado
pub fn gear(frame: Frame, g: &GearPart) -> R<Shape> {
    g.validate()?;
    let l = Local::new(frame);
    let spec = g.effective_spec();
    if spec.kind == GearKind::Bevel {
        return bevel(&l, g, &spec);
    }
    if spec.kind == GearKind::Worm {
        return worm(&l, g, &spec);
    }
    let total = g.width + if spec.kind == GearKind::External { g.hub_length } else { 0.0 };
    let mut body = match spec.kind {
        GearKind::External => {
            // Rueda de sin fin: los dientes suben hacia las caras para abrazar el
            // filete, y un toro del radio del sin fin da la garganta
            let mut spec = spec;
            let mut throat = None;
            if g.throat > 0.0 {
                let tip = spec.circles().map_err(|e| e.to_string())?.tip;
                let radius = g.throat - tip;
                let half = g.width / 2.0;
                if radius <= half * 1.05 {
                    return Err("la rueda es más ancha que el sin fin que abraza: baja el ancho o sube la distancia entre centros".into());
                }
                spec.addendum += (radius - (radius * radius - half * half).sqrt()) / spec.module;
                throat = Some(radius);
            }
            let profile = spec.profile().map_err(|e| e.to_string())?;
            let wheel = helical(&profile.curves, &l, &spec, g, 0.0, g.width, profile.circles.root)?;
            match throat {
                Some(radius) => wheel.cut(&Shape::torus(l.at(g.width / 2.0, false), g.throat, radius).map_err(e)?).map_err(e)?,
                None => wheel,
            }
        }
        GearKind::Internal => {
            let profile = spec.profile().map_err(|e| e.to_string())?;
            let teeth = helical(&profile.curves, &l, &spec, g, -MARGIN, g.width + MARGIN, profile.circles.root)?;
            cyl(&l, g.outer, 0.0, g.width)?.cut(&teeth).map_err(e)?
        }
        GearKind::Bevel | GearKind::Worm => unreachable!(),
        GearKind::Rack => {
            let pts = spec.rack_outline(spec.teeth, g.outer).map_err(|e| e.to_string())?;
            let n = pts.len();
            let lines: Vec<Curve> = (0..n).map(|i| Curve::Line([pts[i][0], pts[i][1], 0.0], [pts[(i + 1) % n][0], pts[(i + 1) % n][1], 0.0])).collect();
            if spec.helix_angle == 0.0 {
                prism(&lines, &l, 0.0, g.width)?
            } else {
                // Dientes oblicuos: la sección transversal corrida en X por z·tanβ
                // (con el signo que engrana con el piñón de la hélice contraria)
                let dx = -spec.helix_angle.to_radians().tan() * g.width;
                let section = |dx: f64, dz: f64| {
                    let curves: Vec<Curve> = lines.iter().map(|c| shift(&to_world(&l, c), add(crate::geom::scale(l.x, dx), crate::geom::scale(l.z, dz)))).collect();
                    Shape::wire(&curves).map_err(e)
                };
                Shape::loft(&[section(0.0, 0.0)?, section(dx, g.width)?], true, false).map_err(e)?
            }
        }
    };
    if spec.kind == GearKind::External && g.hub_length > 0.0 {
        let hub = cyl(&l, g.hub_diameter, g.width - MARGIN, total)?;
        body = Shape::fuse_all(&[body, hub]).map_err(e)?;
    }
    if spec.kind == GearKind::External {
        if g.bore > 0.0 {
            body = body.cut(&cyl(&l, bore_fit(g), -MARGIN, total + MARGIN)?).map_err(e)?;
        }
        if g.keyway {
            let (b, t2) = keyway_fit(g);
            let f = Frame { origin: l.p(0.0, -b / 2.0, -MARGIN), z: l.z, x: l.x };
            let slot = Shape::make_box(f, bore_fit(g) / 2.0 + t2, b, total + 2.0 * MARGIN).map_err(e)?;
            body = body.cut(&slot).map_err(e)?;
        }
        body = lighten(body, &l, g, spec.circles().map_err(|e| e.to_string())?.root, 0.0, g.width)?;
        if g.set_screw > 0.0 {
            body = set_screw_hole(body, &l, g, g.width + g.hub_length / 2.0)?;
        }
    }
    Ok(body)
}

/// Nombre de cada cara del engrane según lo que es en la pieza («diente», «flanco», «agujero»…):
/// con ellos una referencia a la cara sigue valiendo al cambiar los dientes, el módulo o el ancho
/// (varias caras comparten nombre y entre ellas manda la cercanía al punto guardado).
pub fn face_roles(frame: Frame, g: &GearPart, classes: &[cad_occt::FaceClass]) -> Vec<String> {
    use cad_occt::SurfaceKind::*;
    let spec = g.effective_spec();
    let z = normalize(frame.z);
    let x = normalize(sub(frame.x, scale(z, dot(frame.x, z))));
    let y = cross(z, x);
    let axial = |p: P3| dot(sub(p, frame.origin), z);
    // Distancia de un punto de un eje paralelo al eje del engrane
    let off = |p: P3| {
        let d = sub(p, frame.origin);
        let r = sub(d, scale(z, dot(d, z)));
        dot(r, r).sqrt()
    };
    let (tol, c) = (0.01, spec.circles().ok());
    let (tip, root) = c.map_or((0.0, 0.0), |c| (c.tip, c.root));
    let hub = if spec.kind == GearKind::External || spec.kind == GearKind::Bevel || spec.kind == GearKind::Worm { g.hub_length } else { 0.0 };
    // Cónico: alturas del fondo y la cara de arriba (planos, o las del alma si los extremos son cónicos)
    let ends = (spec.kind == GearKind::Bevel && g.cone_ends).then(|| bevel_ends(&spec, g, spec.apex_height())).and_then(Result::ok);
    let bottom = ends.as_ref().map_or(0.0, |b| b.web_back);
    let top = match &ends {
        Some(b) => b.web_front,
        None if spec.kind == GearKind::Bevel => g.width * spec.cone_angle.to_radians().cos(),
        None => g.width,
    };
    let r_out = root - g.rim;
    let r_in = (g.bore / 2.0).max(g.hub_diameter / 2.0) + 1.0;
    // Cremallera: niveles (en Y) de las caras planas paralelas al eje, para separar cabeza y pie
    let rack_tip = classes
        .iter()
        .filter(|k| k.surface == Plane && dot(k.dir, z).abs() < 1e-3 && dot(k.dir, y) > 0.999)
        .map(|k| dot(sub(k.origin, frame.origin), y))
        .fold(f64::MIN, f64::max);
    classes
        .iter()
        .map(|k| {
            let name = match k.surface {
                Plane if dot(k.dir, z).abs() > 0.999 => {
                    let (d, s) = (axial(k.origin), dot(k.dir, z));
                    if s < 0.0 && (d - bottom).abs() < tol {
                        "cara de abajo"
                    } else if s > 0.0 && (d - top).abs() < tol {
                        "cara de arriba"
                    } else if (s > 0.0 && hub > 0.0 && (d - g.width - hub).abs() < tol) || (s < 0.0 && spec.kind == GearKind::Bevel && hub > 0.0 && (d - bottom + hub).abs() < tol) {
                        "cara del cubo"
                    } else {
                        "canal"
                    }
                }
                Plane if dot(k.dir, z).abs() < 1e-3 => {
                    let ny = dot(k.dir, y);
                    if spec.kind == GearKind::Rack && dot(k.dir, x).abs() > 0.999 {
                        "extremo"
                    } else if spec.kind == GearKind::Rack && ny.abs() > 0.999 {
                        if ny < 0.0 {
                            "base del cuerpo"
                        } else if (dot(sub(k.origin, frame.origin), y) - rack_tip).abs() < tol {
                            "cabeza de diente"
                        } else {
                            "pie de diente"
                        }
                    } else if g.keyway && dot(sub(k.origin, frame.origin), k.dir).abs() <= g.bore / 2.0 + 5.0 && off(k.origin) < g.bore / 2.0 + 12.0 {
                        "chavetero"
                    } else if g.spokes > 0 {
                        "rayo"
                    } else {
                        "flanco"
                    }
                }
                Cylinder if dot(k.dir, z).abs() > 0.999 => {
                    let r = k.radius;
                    if off(k.origin) < tol {
                        let bore = bore_fit(g) / 2.0;
                        if g.bore > 0.0 && k.reversed && (r - bore).abs() < tol {
                            "agujero"
                        } else if hub > 0.0 && !k.reversed && (r - g.hub_diameter / 2.0).abs() < tol {
                            "cubo"
                        } else if spec.kind == GearKind::Internal && g.outer > 0.0 && !k.reversed && (r - g.outer / 2.0).abs() < tol {
                            "exterior"
                        } else if g.spokes > 0 && (r - r_in).abs() < tol {
                            "cubo"
                        } else if g.spokes > 0 && (r - r_out).abs() < tol {
                            "aro"
                        } else if spec.kind == GearKind::Worm {
                            "núcleo"
                        } else if (r - tip).abs() < tol {
                            "cabeza de diente"
                        } else if (r - root).abs() < tol {
                            "pie de diente"
                        } else {
                            "flanco"
                        }
                    } else if g.hole_count > 0 {
                        "aligeramiento"
                    } else {
                        "flanco"
                    }
                }
                Cylinder if g.set_screw > 0.0 && dot(k.dir, z).abs() < 1e-3 => "prisionero",
                // Conos: los de los extremos son perpendiculares al cono primitivo (pasan por el punto primitivo
                // atrás, o `ancho/cos δ` más arriba adelante); el signo del semiángulo de OCCT no dice hacia
                // dónde crece, así que se prueban los dos
                Cone if dot(k.dir, z).abs() > 0.999 => {
                    let t = k.angle.tan().abs();
                    let r0 = |slope: f64| k.radius - axial(k.origin) * slope;
                    let at_pitch = |slope: f64| axial(k.origin) + (c.map_or(0.0, |c| c.pitch) - k.radius) / slope;
                    let front_z = g.width / spec.cone_angle.to_radians().cos();
                    if ends.is_some() && t > 1e-6 && [t, -t].iter().any(|&m| at_pitch(m).abs() < 0.05) {
                        "cono de atrás"
                    } else if ends.is_some() && t > 1e-6 && [t, -t].iter().any(|&m| (at_pitch(m) - front_z).abs() < 0.05) {
                        "cono de adelante"
                    } else if [t, -t].iter().any(|&m| (r0(m) - tip).abs() < 0.05) {
                        "cabeza de diente"
                    } else if [t, -t].iter().any(|&m| (r0(m) - root).abs() < 0.05) {
                        "pie de diente"
                    } else {
                        "flanco"
                    }
                }
                Torus if g.throat > 0.0 => "garganta",
                BSpline | Revolution | Extrusion | Offset | Other if spec.kind == GearKind::Worm => "filete",
                _ => "flanco",
            };
            name.to_string()
        })
        .collect()
}

/// Cota **entre** bolas de una corona (engrane interno): bolas de 1,7·m en dos huecos, opuestos
/// si hay dientes pares. Devuelve (Ø de la bola, cota). La bola va en el hueco, que se achica
/// hacia afuera: inv α_M = inv α + medio hueco − (Ø/2)/r_base. `None` si el engrane no es interno.
pub fn measurement_between_pins(spec: &GearSpec) -> Option<(f64, f64)> {
    if spec.kind != GearKind::Internal {
        return None;
    }
    let c = spec.circles().ok()?;
    let (m, z) = (spec.module, spec.teeth as f64);
    let (an, at) = (spec.pressure_angle.to_radians(), c.alpha_t);
    let inv = |a: f64| a.tan() - a;
    let d_pin = 1.7 * m;
    // Medio hueco en el primitivo: la corona tiene el hueco de un diente exterior con la misma corrección
    // (su diente pierde la mitad del juego, el hueco la gana)
    let half_space = PI / (2.0 * z) + 2.0 * spec.shift * an.tan() / z + spec.backlash / 2.0 / (m * z);
    let inv_m = inv(at) + half_space - d_pin / (2.0 * c.base);
    let mut am = at;
    for _ in 0..60 {
        let df = am.tan().powi(2);
        if df < 1e-14 {
            break;
        }
        am -= (inv(am) - inv_m) / df;
    }
    let even = spec.teeth % 2 == 0;
    // `c.base` es el radio base: entre centros opuestos va el diámetro
    let m_between = 2.0 * c.base / am.cos() * if even { 1.0 } else { (PI / (2.0 * z)).cos() } - d_pin;
    Some((d_pin, m_between))
}

/// Lo que el diálogo muestra de un engrane
#[derive(Debug, Clone, Serialize)]
pub struct GearInfo {
    pub pitch_diameter: f64,
    pub base_diameter: f64,
    pub tip_diameter: f64,
    pub root_diameter: f64,
    pub module_t: f64,
    pub pressure_angle_t: f64,
    pub min_teeth: f64,
    pub warnings: Vec<String>,
    /// Cota sobre `span_teeth` dientes (0: no aplica) y sobre bolas de
    /// `pin_diameter` (0: no aplica); engranes exteriores rectos o helicoidales
    pub span_teeth: u32,
    pub span: f64,
    pub pin_diameter: f64,
    pub over_pins: f64,
    /// Error de medidas (el engrane no se puede construir)
    pub error: Option<String>,
}

/// Cota sobre dientes (W) y sobre bolas (M) de un exterior; todo en el plano
/// transversal salvo el espesor (normal). Devuelve (k, W, d_bola, M).
pub fn measurements(spec: &GearSpec) -> Option<(u32, f64, f64, f64)> {
    if spec.kind != GearKind::External {
        return None;
    }
    let c = spec.circles().ok()?;
    let (m, z, x) = (spec.module, spec.teeth as f64, spec.shift);
    let (an, at) = (spec.pressure_angle.to_radians(), c.alpha_t);
    let inv = |a: f64| a.tan() - a;
    // Dientes a medir: los que abraza el calibre con el ángulo de presión
    let z_eq = z * inv(at) / inv(an);
    let k = ((z_eq * an / PI + 0.5).round() as i64).max(2) as u32;
    // Cada diente pierde la mitad del juego (`backlash/2`, normal) y la cota lo hace cos α_n veces eso
    let half_j = spec.backlash / 2.0;
    let w = m * an.cos() * (PI * (k as f64 - 0.5) + z * inv(at)) + 2.0 * x * m * an.sin() - half_j * an.cos();
    let d_pin = 1.7 * m;
    let inv_m = inv(at) + d_pin / (m * z * an.cos()) - PI / (2.0 * z) + 2.0 * x * an.tan() / z - half_j / (m * z);
    // Resolver inv(α_M) = inv_m por Newton
    let mut am = at;
    for _ in 0..60 {
        let df = am.tan().powi(2);
        if df < 1e-14 {
            break;
        }
        am -= (inv(am) - inv_m) / df;
    }
    let base = c.module_t * z * at.cos();
    let even = spec.teeth % 2 == 0;
    let m_over = base / am.cos() * if even { 1.0 } else { (PI / (2.0 * z)).cos() } + d_pin;
    Some((k, w, d_pin, m_over))
}

pub fn info(g: &GearPart) -> GearInfo {
    let spec = g.effective_spec();
    let mut warnings = Vec::new();
    let mut error = g.validate().err();
    let circles: Option<GearCircles> = spec.circles().ok();
    if !matches!(spec.kind, GearKind::Rack | GearKind::Worm) {
        match spec.profile() {
            Ok(p) => warnings = p.warnings.iter().map(GearWarning::message).collect(),
            Err(e) => error = error.or(Some(e.to_string())),
        }
    }
    let meas = measurements(&spec);
    let between = measurement_between_pins(&spec);
    let c = circles.unwrap_or(GearCircles { module_t: 0.0, alpha_t: 0.0, pitch: 0.0, base: 0.0, tip: 0.0, root: 0.0, thickness: 0.0 });
    GearInfo {
        pitch_diameter: 2.0 * c.pitch,
        base_diameter: 2.0 * c.base,
        tip_diameter: 2.0 * c.tip,
        root_diameter: 2.0 * c.root,
        module_t: c.module_t,
        pressure_angle_t: c.alpha_t.to_degrees(),
        min_teeth: spec.min_teeth_no_undercut(),
        warnings,
        span_teeth: meas.map_or(0, |m| m.0),
        span: meas.map_or(0.0, |m| m.1),
        pin_diameter: meas.map_or_else(|| between.map_or(0.0, |b| b.0), |m| m.2),
        over_pins: meas.map_or_else(|| between.map_or(0.0, |b| b.1), |m| m.3),
        error,
    }
}

/// Nombre de catálogo ("Engrane m1,5 · z24")
pub fn describe(g: &GearPart) -> String {
    let m = if (g.spec.module - g.spec.module.round()).abs() < 1e-9 { format!("{}", g.spec.module.round()) } else { format!("{}", g.spec.module).replace('.', ",") };
    let what = match g.spec.kind {
        GearKind::External => "Engrane",
        GearKind::Internal => "Corona",
        GearKind::Rack => "Cremallera",
        GearKind::Bevel => "Engrane cónico",
        GearKind::Worm => "Sin fin",
    };
    format!("{what} m{m} · z{}", g.spec.teeth)
}

/// Resultado de engranar un engrane con otro
#[derive(Debug, Clone, PartialEq)]
pub struct MeshInfo {
    pub center_distance: f64,
    pub contact_ratio: f64,
    pub interference: bool,
    /// Aviso de medidas (p. ej. el cono del cónico no es el que da la relación)
    pub note: Option<String>,
}

fn inv(a: f64) -> f64 {
    a.tan() - a
}

/// Engrane `g` engranado con `src`: su marco, sus medidas ya copiadas del otro
/// (módulo, ángulo de presión, hélice contraria) y los datos del par.
pub fn mesh(src: &GearAxis, g: &GearPart, link: &GearLink) -> Result<(Frame, GearPart, MeshInfo), String> {
    use GearKind::*;
    let mut b = g.clone();
    let a = &src.part;
    // El módulo normal del otro (si el suyo es transversal, se vuelve a pasar con la hélice nueva)
    b.spec.module = a.effective_spec().module;
    b.spec.pressure_angle = a.spec.pressure_angle;
    b.spec.helix_angle = if (a.spec.kind == Internal) == (b.spec.kind == Internal) { -a.spec.helix_angle } else { a.spec.helix_angle };
    if b.transverse_module && matches!(b.spec.kind, External | Internal | Rack) {
        b.spec.module /= b.spec.helix_angle.to_radians().cos();
    }
    let (ka, kb) = (a.spec.kind, b.spec.kind);
    if (ka == Bevel) != (kb == Bevel) {
        return Err("un cónico solo engrana con otro cónico".into());
    }
    if matches!((ka, kb), (Internal, Internal) | (Internal, Rack)) || (ka == Rack && kb != External) {
        return Err(match ka {
            Rack => "una cremallera solo engrana con un engrane exterior (el piñón)".to_string(),
            _ => "esa combinación no engrana (corona con corona o con cremallera)".to_string(),
        });
    }
    let z = normalize(src.z);
    let xa = normalize(sub(src.x, scale(z, dot(src.x, z))));
    let ya = cross(z, xa);
    let theta = link.angle.to_radians();
    let u = add(scale(xa, theta.cos()), scale(ya, theta.sin()));
    if ka == External && kb == Worm {
        return mesh_worm_from_wheel(src, b, link, [z, xa, ya, u], theta);
    }
    if ka == Worm || kb == Worm {
        return mesh_worm(src, b, link, [z, xa, ya, u], theta);
    }
    if ka == Rack {
        return mesh_from_rack(src, b, link, [z, xa, ya]);
    }
    if ka == Bevel || kb == Bevel {
        return mesh_bevel(src, b, link, [z, xa, ya, u], theta);
    }
    let ea = a.effective_spec();
    let (ca, mt, at) = {
        let c = ea.circles().map_err(|e| e.to_string())?;
        (c, c.module_t, c.alpha_t)
    };
    let p = PI * mt;
    let tan_n = ea.pressure_angle.to_radians().tan();
    let level = scale(z, link.offset);

    if kb == Rack {
        if link.distance.is_some() {
            return Err("la cremallera no tiene distancia entre centros: va a la línea primitiva del piñón".into());
        }
        let d = ca.pitch + ea.shift * ea.module;
        let e = cross(z, u);
        let q = add(src.origin, scale(u, d));
        let n = b.spec.teeth as f64;
        // La cremallera helicoidal se corre en X con la altura: la base del piñón está `offset` más abajo
        let skew = b.spec.helix_angle.to_radians().tan() * link.offset;
        let origin = add(add(sub(q, scale(e, ca.pitch * theta + skew)), scale(e, -(n / 2.0).floor() * p)), level);
        let info = MeshInfo { center_distance: d, contact_ratio: 0.0, interference: false, note: None };
        return Ok((Frame { origin, z, x: e }, b, info));
    }

    // Fijar la corrección de este engrane si se pidió una distancia
    let (pinion_first, internal) = match (ka, kb) {
        (External, External) => (true, false),
        (External, Internal) => (true, true),
        _ => (false, true),
    };
    if let Some(dist) = link.distance {
        let eb = b.effective_spec();
        let (za, zb) = (ea.teeth as f64, eb.teeth as f64);
        let dz = if !internal { za + zb } else { (zb - za).abs() };
        let a0 = mt * dz / 2.0 * at.cos();
        let cos_w = a0 / dist;
        if !(0.0..=1.0).contains(&cos_w) || dist <= 0.0 {
            return Err(format!("no hay corrección que dé {dist} mm entre centros con estos dientes (mínimo {:.3})", a0));
        }
        let d_inv = (inv(cos_w.acos()) - inv(at)) * dz / (2.0 * tan_n);
        b.spec.shift = match (ka, kb) {
            (External, External) => d_inv - a.spec.shift,
            (External, Internal) => a.spec.shift + d_inv,
            _ => a.spec.shift - d_inv,
        };
    }
    let (ea, eb) = (a.effective_spec(), b.effective_spec());
    let pair = if pinion_first { gears::pair(&ea, &eb) } else { gears::pair(&eb, &ea) }.map_err(|e| e.to_string())?;
    let (ra, rb) = (ca.pitch, eb.circles().map_err(|e| e.to_string())?.pitch);
    let dist = pair.center_distance;
    let psi = match (ka, kb) {
        (External, External) => theta + PI - (p / 2.0 - ra * theta) / rb,
        (External, Internal) => {
            // Ring (b) visto desde el piñón: la línea de centros va al revés
            // (el ángulo 0 de la corona es el centro de un diente, no de un hueco)
            let tv = theta + PI;
            tv - ra * tv / rb
        }
        _ => theta - ra * theta / rb,
    };
    // Si `b` es la corona su centro queda al otro lado de `u`; si es el
    // piñón (a es corona) queda del lado de `u` (centro del piñón en `u`)
    let origin = add(add(src.origin, scale(u, dist)), level);
    let x = add(scale(xa, psi.cos()), scale(ya, psi.sin()));
    let info = MeshInfo { center_distance: dist, contact_ratio: pair.contact_ratio, interference: pair.interference, note: None };
    Ok((Frame { origin, z, x }, b, info))
}

/// Piñón engranado con una cremallera que va primero: queda del lado de las cabezas, a la distancia
/// del círculo primitivo de la línea de rodadura, en la posición `link.along` a lo largo de ella, y
/// gira lo que hace falta para que sus dientes entren en los huecos.
fn mesh_from_rack(src: &GearAxis, mut b: GearPart, link: &GearLink, [z, xa, ya]: [P3; 3]) -> Result<(Frame, GearPart, MeshInfo), String> {
    if link.distance.is_some() {
        return Err("la cremallera no tiene distancia entre centros: el piñón va a su línea primitiva".into());
    }
    let a = &src.part;
    let (ea, eb) = (a.effective_spec(), b.effective_spec());
    let r = eb.circles().map_err(|e| e.to_string())?.pitch;
    let p = PI * ea.circles().map_err(|e| e.to_string())?.module_t;
    let d = r + eb.shift * eb.module;
    // Posición del contacto: el centro de la cremallera (el primer diente está en 0) más el corrimiento
    let s_q = (a.spec.teeth as f64 - 1.0) * p / 2.0 + link.along;
    // Con hélice, el diente se corre en X con la altura; el piñón empieza `offset` arriba de la base
    let s_eff = s_q + ea.helix_angle.to_radians().tan() * link.offset;
    let theta = s_eff / r;
    let u = scale(ya, -1.0);
    let e = xa;
    let origin = add(add(add(src.origin, scale(e, s_q)), scale(ya, d)), scale(z, link.offset));
    // x del piñón: u girado -θ alrededor del eje (u apunta del piñón a la cremallera)
    let x = sub(scale(u, theta.cos()), scale(cross(z, u), theta.sin()));
    let info = MeshInfo { center_distance: d, contact_ratio: 0.0, interference: false, note: None };
    let _ = &mut b;
    Ok((Frame { origin, z, x }, b, info))
}

/// Cónico recto: el tronco de cono por el vértice entre el plano del extremo
/// grande (por el punto primitivo) y otro a `width·cos δ` de él. Los flancos
/// son rectas por el vértice, así que los dientes son los del cónico de verdad;
/// los extremos quedan planos (perpendiculares al eje) en vez de cónicos, lo
/// que evita booleanas con superficies libres (fallaban según el ángulo).
fn bevel(l: &Local, g: &GearPart, spec: &GearSpec) -> R<Shape> {
    let profile = spec.profile().map_err(|e| e.to_string())?;
    let delta = spec.cone_angle.to_radians();
    let (cd, td) = (delta.cos(), delta.tan());
    let apex_h = spec.module * spec.teeth as f64 / 2.0 / td;
    let z_top = g.width * cd;
    let k = (apex_h - z_top) / apex_h;
    // Sección del diente a la altura z: escalada hacia el vértice y, en el espiral, girada por su espiral logarítmica
    // (φ = −sen δ · tan β · ln(s/s_medio): sale del ángulo de espiral constante sobre el cono desarrollado)
    let spiral = spec.spiral_angle.to_radians().tan() * delta.sin();
    // El giro es cero en la sección del medio de la cara: ahí vale la fase del cónico recto con la X del marco
    let s_mid = (apex_h - 0.5 * z_top) / apex_h;
    let section = |z: f64| {
        let s = (apex_h - z) / apex_h;
        let turn = -spiral * (s / s_mid).ln();
        let curves: Vec<Curve> = profile
            .curves
            .iter()
            .map(|c| {
                let c = scale_curve(c, s);
                let c = if turn != 0.0 { gears::rotate_curve(&c, turn) } else { c };
                shift(&to_world(l, &c), crate::geom::scale(l.z, z))
            })
            .collect();
        Shape::wire(&curves).map_err(e)
    };
    // Cuerpo del diente entre dos alturas: recto (dos secciones) o espiral (una cada ~4° de giro)
    let tooth_between = |z0: f64, z1: f64| -> R<Shape> {
        if spiral == 0.0 {
            return Shape::loft(&[section(z0)?, section(z1)?], true, true).map_err(e);
        }
        let s_of = |z: f64| (apex_h - z) / apex_h;
        let steps = ((spiral * (s_of(z0).ln() - s_of(z1).ln()).abs() / (PI / 45.0)).ceil() as usize + 1).max(3);
        let sections = (0..=steps).map(|i| section(z0 + (z1 - z0) * i as f64 / steps as f64)).collect::<R<Vec<_>>>()?;
        Shape::loft(&sections, true, false).map_err(e)
    };
    // Extremos: planos (z = 0 y z = ancho·cos δ) o conos perpendiculares al cono primitivo
    let ends = if g.cone_ends { Some(bevel_ends(spec, g, apex_h)?) } else { None };
    let (z_back, z_front) = ends.as_ref().map_or((0.0, z_top), |b| (b.web_back, b.web_front));
    let mut body = match &ends {
        None => tooth_between(0.0, z_top)?,
        Some(b) => {
            let tooth = tooth_between(b.z_lo, b.z_hi)?;
            let pts: Vec<P3> = b.profile.iter().map(|&(r, z)| l.p(r, 0.0, z)).collect();
            let blank = Shape::polygon(&pts).map_err(e)?.revolve(cad_occt::Axis { origin: l.o, dir: l.z }, TAU).map_err(e)?;
            tooth.intersect(&blank).map_err(|x| format!("los extremos cónicos fallaron con este cono ({x}): usa extremos planos"))?
        }
    };
    if body.mass().map_or(true, |m| m.volume < 1e-6) {
        return Err("los extremos cónicos dejaron el cuerpo vacío con este cono: usa extremos planos".into());
    }
    if g.hub_length > 0.0 {
        let hub = cyl(l, g.hub_diameter, z_back - g.hub_length, z_back + 0.5 * (z_front - z_back))?;
        body = Shape::fuse_all(&[body, hub]).map_err(e)?;
    }
    let (zb, zt) = (z_back - g.hub_length - MARGIN, ends.as_ref().map_or(z_top, |b| b.z_top_max) + MARGIN);
    if g.bore > 0.0 {
        body = body.cut(&cyl(l, bore_fit(g), zb, zt)?).map_err(e)?;
    }
    if g.keyway {
        let (kw, t2) = keyway_fit(g);
        let f = Frame { origin: l.p(0.0, -kw / 2.0, zb), z: l.z, x: l.x };
        let slot = Shape::make_box(f, bore_fit(g) / 2.0 + t2, kw, zt - zb).map_err(e)?;
        body = body.cut(&slot).map_err(e)?;
    }
    // Rayos y agujeros: el aro se mide en el extremo chico, que es donde el cuerpo es más angosto
    let root_small = spec.circles().map_err(|e| e.to_string())?.root * k;
    body = lighten(body, l, g, root_small, z_back, ends.as_ref().map_or(z_top, |b| b.z_top_max))?;
    if g.set_screw > 0.0 {
        body = set_screw_hole(body, l, g, z_back - g.hub_length / 2.0)?;
    }
    Ok(body)
}

/// Lo que pide un cónico de extremos cónicos: el contorno de revolución del cuerpo en (r, z) y las
/// alturas entre las que se talla el diente (con holgura) y de las bandas planas del centro.
struct BevelEnds {
    profile: Vec<(f64, f64)>,
    z_lo: f64,
    z_hi: f64,
    web_back: f64,
    web_front: f64,
    z_top_max: f64,
}

/// Cono de atrás: la recta por el punto primitivo perpendicular a la generatriz, z = (r − r_p)·tan δ; el
/// de adelante es el mismo corrido `ancho/cos δ`. Cada cono vale solo en el anillo de los dientes (desde
/// el pie del extremo que le toca hacia afuera) y hacia el centro el cuerpo sigue plano.
fn bevel_ends(spec: &GearSpec, g: &GearPart, apex_h: f64) -> R<BevelEnds> {
    let c = spec.circles().map_err(|e| e.to_string())?;
    let delta = spec.cone_angle.to_radians();
    let (cd, td) = (delta.cos(), delta.tan());
    if spec.cone_angle > 50.0 {
        return Err("los extremos cónicos llegan hasta un cono de 50° (con más, la booleana con los dientes falla): usa extremos planos".into());
    }
    let (r_p, w) = (c.pitch, g.width);
    let cone = spec.cone_distance();
    // Atrás (extremo grande) el cono empieza bajo el pie grande; adelante, bajo el pie chico
    let r_cb = c.root - 1.0;
    let r_cf = c.root * (1.0 - w / cone) - 1.0;
    if r_cf <= g.bore / 2.0 + 1.0 {
        return Err("el cuerpo del cónico es muy chico para los extremos cónicos: usa extremos planos".into());
    }
    let r_max = c.tip + 2.0;
    let back = |r: f64| (r - r_p) * td;
    let front = |r: f64| back(r) + w / cd;
    // El alma plana mide ancho·cos δ, sin pasar donde empieza el cono de adelante
    let web_back = back(r_cb);
    let web_front = (web_back + w * cd).min(front(r_cf) - 0.2 * w * cd);
    let z_hi_star = (w / cd + (c.tip - r_p) * td) / (1.0 + c.tip * td / apex_h);
    Ok(BevelEnds {
        profile: vec![(0.0, web_back), (r_cb, web_back), (r_max, back(r_max)), (r_max, front(r_max)), (r_cf, front(r_cf)), (r_cf, web_front), (0.0, web_front)],
        z_lo: web_back - 1.0,
        z_hi: z_hi_star + 1.0,
        web_back,
        web_front,
        z_top_max: front(r_max),
    })
}

/// Semiángulo del cono primitivo del primer cónico para que ruede con el segundo: tan δ₁ = senΣ / (z₂/z₁ + cosΣ).
pub fn bevel_cone(z1: f64, z2: f64, shaft_deg: f64) -> f64 {
    let sigma = shaft_deg.to_radians();
    sigma.sin().atan2(z2 / z1 + sigma.cos()).to_degrees()
}

/// Cónico con cónico: los vértices coinciden y los ejes forman el ángulo entre
/// ejes (`shaft_angle` del segundo). El cono del segundo es lo que falta para
/// sumarlo; si el del primero no es el que da la relación de dientes, avisa.
fn mesh_bevel(src: &GearAxis, mut b: GearPart, link: &GearLink, [z, xa, ya, u]: [P3; 4], theta: f64) -> Result<(Frame, GearPart, MeshInfo), String> {
    if link.offset != 0.0 || link.distance.is_some() {
        return Err("el cónico no tiene corrimiento ni distancia entre centros: los vértices coinciden".into());
    }
    let a = &src.part;
    let (ea, sigma_deg) = (a.effective_spec(), b.spec.shaft_angle);
    let sigma = sigma_deg.to_radians();
    let delta_b = sigma_deg - ea.cone_angle;
    if !(5.0..=85.0).contains(&delta_b) {
        return Err(format!("el cono del primero ({}°) y el ángulo entre ejes ({}°) no dejan un cono válido para el segundo", ea.cone_angle, sigma_deg));
    }
    b.spec.cone_angle = delta_b;
    // Espiral de la mano contraria (como las hélices de los helicoidales)
    b.spec.spiral_angle = -a.spec.spiral_angle;
    let eb = b.effective_spec();
    let (ca, cb) = (ea.circles().map_err(|e| e.to_string())?, eb.circles().map_err(|e| e.to_string())?);
    let (za, zb) = (ea.teeth as f64, eb.teeth as f64);
    let ideal = bevel_cone(za, zb, sigma_deg);
    let note = ((ideal - ea.cone_angle).abs() > 0.5).then(|| {
        format!("el cono del primer cónico no es el que da esta relación de dientes ({:.1}° en vez de {:.1}°): los conos primitivos no ruedan juntos", ea.cone_angle, ideal)
    });
    // Ejes que salen del vértice: w_a (hacia atrás), w_b a Σ del lado de u
    let wa = scale(z, -1.0);
    let wb = add(scale(wa, sigma.cos()), scale(u, sigma.sin()));
    let delta_a = ea.cone_angle.to_radians();
    let t = add(scale(wa, delta_a.cos()), scale(u, delta_a.sin()));
    let zb_axis = scale(wb, -1.0);
    let perp = sub(t, scale(wb, dot(t, wb)));
    let q = normalize(perp);
    let p = PI * ea.module;
    let phi = (p / 2.0 - ca.pitch * theta) / cb.pitch;
    let xb = add(scale(q, phi.cos()), scale(cross(zb_axis, q), -phi.sin()));
    let apex = add(src.origin, scale(z, ca.pitch / delta_a.tan()));
    let origin = sub(apex, scale(zb_axis, cb.pitch / delta_b.to_radians().tan()));
    let _ = (xa, ya);
    let info = MeshInfo { center_distance: 0.0, contact_ratio: 0.0, interference: false, note };
    Ok((Frame { origin, z: zb_axis, x: xb }, b, info))
}

/// Sin fin: núcleo y filete trapecial (de `starts` entradas) del eje Z, de 0 a `width`.
fn worm(l: &Local, g: &GearPart, spec: &GearSpec) -> R<Shape> {
    let (r_root, r_tip, hw_in, hw_out) = spec.worm_profile().map_err(|e| e.to_string())?;
    let (_, _, lead) = spec.worm_dims();
    let frame = Frame { origin: l.o, z: l.z, x: l.x };
    let mut body = Shape::thread_trapezoid(frame, r_root, r_tip, lead, spec.teeth, hw_in, hw_out, g.width, spec.left_hand).map_err(e)?;
    // Vástago liso después del filete (para el prisionero)
    if g.hub_length > 0.0 {
        body = Shape::fuse_all(&[body, cyl(l, g.hub_diameter, g.width - MARGIN, g.width + g.hub_length)?]).map_err(e)?;
    }
    let (zb, zt) = (-MARGIN, g.width + g.hub_length + MARGIN);
    if g.bore > 0.0 {
        body = body.cut(&cyl(l, bore_fit(g), zb, zt)?).map_err(e)?;
    }
    if g.keyway {
        let (kw, t2) = keyway_fit(g);
        let f = Frame { origin: l.p(0.0, -kw / 2.0, zb), z: l.z, x: l.x };
        let slot = Shape::make_box(f, bore_fit(g) / 2.0 + t2, kw, zt - zb).map_err(e)?;
        body = body.cut(&slot).map_err(e)?;
    }
    if g.set_screw > 0.0 {
        body = set_screw_hole(body, l, g, g.width + g.hub_length / 2.0)?;
    }
    Ok(body)
}

/// Sin fin (el primero) con su rueda: ejes cruzados a 90°, la rueda a la
/// distancia `d₁/2 + r₂` en la dirección `u`, abrazando el filete. La rueda toma
/// el módulo transversal axial del sin fin, el ángulo de avance como hélice y su misma mano.
fn mesh_worm(src: &GearAxis, mut b: GearPart, link: &GearLink, [z, _xa, _ya, u]: [P3; 4], theta: f64) -> Result<(Frame, GearPart, MeshInfo), String> {
    let a = &src.part;
    if a.spec.kind != GearKind::Worm || b.spec.kind != GearKind::External {
        return Err("el sin fin va primero y su rueda es un engrane exterior: engrana la rueda con el sin fin".into());
    }
    if link.distance.is_some() {
        return Err("la distancia entre centros del sin fin sale de su diámetro primitivo y la rueda".into());
    }
    let ea = a.effective_spec();
    let (_, gamma, lead) = ea.worm_dims();
    let s_h = if ea.left_hand { -1.0 } else { 1.0 };
    let m_x = ea.module;
    b.transverse_module = false;
    b.spec.module = m_x * gamma.cos();
    b.spec.pressure_angle = (ea.pressure_angle.to_radians().tan() * gamma.cos()).atan().to_degrees();
    b.spec.helix_angle = s_h * gamma.to_degrees();
    b.herringbone = false;
    let eb = b.effective_spec();
    let cb = eb.circles().map_err(|e| e.to_string())?;
    let (d1, _, _) = ea.worm_dims();
    let center = d1 / 2.0 + cb.pitch + eb.shift * eb.module;
    b.throat = center;
    let z_b = cross(z, u);
    let mid = a.width / 2.0 + link.offset;
    let c_b = add(add(src.origin, scale(z, mid)), scale(u, center));
    // Fase: en el plano central el filete pasa por z_c(θ) y la rueda tiene un hueco ahí
    let z_c = s_h * lead * theta / TAU;
    let phi = (mid - z_c) / cb.pitch;
    let rate = eb.helix_angle.to_radians().tan() / cb.pitch;
    let turn = rate * b.width / 2.0;
    let q = scale(u, -1.0);
    // x en el plano central, y luego en la cara de abajo (los dientes giran con la altura)
    let x_center = add(scale(q, phi.cos()), scale(z, -phi.sin()));
    let y_b = cross(z_b, x_center);
    let x_face = add(scale(x_center, turn.cos()), scale(y_b, -turn.sin()));
    let origin = sub(c_b, scale(z_b, b.width / 2.0));
    let note = worm_efficiency_note(gamma, ea.pressure_angle.to_radians());
    let info = MeshInfo { center_distance: center, contact_ratio: 0.0, interference: false, note };
    Ok((Frame { origin, z: z_b, x: x_face }, b, info))
}

/// Sin fin engranado con su rueda, que va primero: el sin fin copia el módulo axial y el ángulo de
/// presión que la rueda trae (módulo normal = m_x·cos γ), toma la mano de su hélice, queda a
/// `d₁/2 + r₂` en la dirección `link.angle` con el eje cruzado a 90°, y gira lo que hace falta
/// para que el filete caiga en un hueco. Avisa si la rueda no trae el avance ni la garganta que pide.
fn mesh_worm_from_wheel(src: &GearAxis, mut b: GearPart, link: &GearLink, [z, xa, _ya, q]: [P3; 4], theta: f64) -> Result<(Frame, GearPart, MeshInfo), String> {
    let a = &src.part;
    if a.spec.kind != GearKind::External || b.spec.kind != GearKind::Worm {
        return Err("la rueda del sin fin es un engrane exterior".into());
    }
    if link.distance.is_some() {
        return Err("la distancia entre centros del sin fin sale de su diámetro primitivo y la rueda".into());
    }
    let ea = a.effective_spec();
    if ea.helix_angle == 0.0 {
        return Err("la rueda de un sin fin lleva hélice (el ángulo de avance del sin fin)".into());
    }
    let beta = ea.helix_angle.to_radians();
    b.spec.left_hand = beta < 0.0;
    b.spec.helix_angle = 0.0;
    b.transverse_module = false;
    b.spec.module = ea.module / beta.cos();
    b.spec.pressure_angle = (ea.pressure_angle.to_radians().tan() / beta.cos()).atan().to_degrees();
    b.herringbone = false;
    let eb = b.effective_spec();
    let (d1, gamma, lead) = eb.worm_dims();
    let ca = ea.circles().map_err(|e| e.to_string())?;
    let center = d1 / 2.0 + ca.pitch + ea.shift * ea.module;
    let s_h = if b.spec.left_hand { -1.0 } else { 1.0 };
    // Ejes cruzados: el del sin fin es perpendicular al de la rueda y a la dirección del centro
    let z_wm = cross(z, q);
    let c_wm = add(add(src.origin, scale(z, a.width / 2.0 + link.offset)), scale(q, center));
    let origin = sub(c_wm, scale(z_wm, b.width / 2.0));
    // Fase: la x de la rueda en su plano central, y de ahí cuánto avanza el filete en ese plano
    let turn = beta.tan() / ca.pitch * a.width / 2.0;
    let x_center = add(scale(xa, turn.cos()), scale(cross(z, xa), turn.sin()));
    let phi = (-dot(x_center, z_wm)).atan2(dot(x_center, q));
    let z_c = b.width / 2.0 - phi * ca.pitch;
    let th = z_c * TAU / (s_h * lead);
    let u_f = scale(q, -1.0);
    let x = sub(scale(u_f, th.cos()), scale(cross(z_wm, u_f), th.sin()));
    let mut notes = Vec::new();
    if (gamma.to_degrees() - beta.abs().to_degrees()).abs() > 0.5 {
        notes.push(format!("el ángulo de avance del sin fin ({:.1}°) no es la hélice de la rueda ({:.1}°): ajusta las entradas o el diámetro primitivo", gamma.to_degrees(), beta.abs().to_degrees()));
    }
    if (a.throat - center).abs() > 0.01 {
        notes.push(format!("pon la garganta de la rueda en {center:.3} mm para que abrace al sin fin"));
    }
    if let Some(n) = worm_efficiency_note(gamma, eb.pressure_angle.to_radians()) {
        notes.push(n);
    }
    let _ = theta;
    let info = MeshInfo { center_distance: center, contact_ratio: 0.0, interference: false, note: (!notes.is_empty()).then(|| notes.join("; ")) };
    Ok((Frame { origin, z: z_wm, x }, b, info))
}

/// Dientes que debe tener un engrane de tipo `kind_b` para engranar con `src`
/// con la relación pedida (dientes de este / dientes de `src`) o, sin ella, a
/// la distancia entre centros pedida (sin corrección de perfil: la distancia
/// exacta la ajusta después `GearLink::distance` con la corrección).
pub fn design_teeth(src: &GearPart, kind_b: GearKind, ratio: Option<f64>, distance: Option<f64>) -> Result<u32, String> {
    use GearKind::*;
    let ka = src.spec.kind;
    let sign = match (ka, kb_ok(ka, kind_b)?) {
        (External, External) => 1.0,
        (External, Internal) => -1.0,
        _ => 0.0,
    };
    let za = src.spec.teeth as f64;
    let z = match (ratio, distance) {
        (Some(r), _) if r > 0.0 => za * r,
        (Some(_), _) => return Err("la relación debe ser positiva".into()),
        (None, Some(c)) if c > 0.0 => {
            let sum = 2.0 * c / src.effective_spec().module_t();
            if ka == Internal { za - sum } else if sign > 0.0 { sum - za } else { sum + za }
        }
        (None, Some(_)) => return Err("la distancia entre centros debe ser positiva".into()),
        (None, None) => return Err("pide una relación o una distancia entre centros".into()),
    };
    let z = z.round();
    if z < 4.0 || z > 1000.0 {
        return Err(format!("salen {z} dientes: fuera de lo posible"));
    }
    if ka == Internal && z >= za {
        return Err("el piñón de una corona tiene menos dientes que ella".into());
    }
    if kind_b == Internal && ka == External && z <= za {
        return Err("la corona tiene más dientes que su piñón".into());
    }
    Ok(z as u32)
}

fn kb_ok(ka: GearKind, kb: GearKind) -> Result<GearKind, String> {
    use GearKind::*;
    match (ka, kb) {
        (External, External) | (External, Internal) | (Internal, External) => Ok(kb),
        _ => Err("solo se calculan los dientes de pares exterior-exterior y piñón-corona".into()),
    }
}

/// Rendimiento del sin fin moviendo la rueda y si se autobloquea (la rueda no
/// lo mueve a él), con rozamiento μ = 0,1 típico de bronce sobre acero lubricado.
pub fn worm_efficiency(gamma: f64, pressure_angle: f64) -> (f64, bool) {
    const MU: f64 = 0.1;
    let phi = (MU / pressure_angle.cos()).atan();
    (gamma.tan() / (gamma + phi).tan(), gamma <= phi)
}

/// Aviso solo si se autobloquea o rinde menos de la mitad
fn worm_efficiency_note(gamma: f64, pressure_angle: f64) -> Option<String> {
    let (eta, locks) = worm_efficiency(gamma, pressure_angle);
    if !locks && eta >= 0.5 {
        return None;
    }
    let base = format!("sin fin: ángulo de avance {:.1}°, rendimiento ≈ {:.0} % (μ = 0,1)", gamma.to_degrees(), eta * 100.0);
    Some(if locks { format!("{base}; autobloqueante: la rueda no mueve al sin fin") } else { base })
}

/// Sección axial del sin fin como contorno de líneas (plano XY: X a lo largo del eje, con el origen en el
/// medio del filete; Y el radio): los filetes trapeciales de arriba, los de abajo corridos medio avance
/// (`entradas·paso/2`, lo que ya giró la hélice) y, si hay, el vástago y el agujero del eje.
fn worm_axial_outline(g: &GearPart, spec: &GearSpec) -> R<Vec<Curve>> {
    let (r_root, r_tip, hw_in, hw_out) = spec.worm_profile().map_err(|e| e.to_string())?;
    let pitch = PI * spec.module;
    let (w, x0) = (g.width, g.width / 2.0);
    let line = |a: (f64, f64), b: (f64, f64)| Curve::Line([a.0 - x0, a.1, 0.0], [b.0 - x0, b.1, 0.0]);
    // Un lado: puntos (z, r) del filete; `shift` corre los centros de los dientes
    let side = |shift: f64| -> Vec<(f64, f64)> {
        let mut out = vec![(0.0, r_root)];
        let mut c = (pitch / 2.0 + shift).rem_euclid(pitch);
        if c < hw_in {
            c += pitch;
        }
        while c + hw_in <= w + 1e-9 {
            out.extend([(c - hw_in, r_root), (c - hw_out, r_tip), (c + hw_out, r_tip), (c + hw_in, r_root)]);
            c += pitch;
        }
        out.push((w, r_root));
        out.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9);
        out
    };
    let top = side(0.0);
    let bottom: Vec<(f64, f64)> = side(spec.teeth as f64 * pitch / 2.0).into_iter().map(|(z, r)| (z, -r)).collect();
    let mut outline = top;
    let end = w + g.hub_length;
    if g.hub_length > 0.0 {
        let hr = g.hub_diameter / 2.0;
        outline.extend([(w, hr), (end, hr), (end, -hr), (w, -hr)]);
    }
    outline.extend(bottom.into_iter().rev());
    let n = outline.len();
    let mut curves: Vec<Curve> = (0..n).map(|i| line(outline[i], outline[(i + 1) % n])).collect();
    if g.bore > 0.0 {
        let b = bore_fit(g) / 2.0;
        let pts = [(0.0, b), (end, b), (end, -b), (0.0, -b)];
        curves.extend((0..4).map(|i| line(pts[i], pts[(i + 1) % 4])));
    }
    Ok(curves)
}

/// El perfil del engrane como bloque del sketch (origen = centro del engrane,
/// rígido): arcos de pie y de cabeza, splines de flancos con sus tangentes
/// exactas; la cremallera, su contorno de líneas. Se inserta con las copias
/// de bloque del sketch.
pub fn sketch_block(g: &GearPart) -> Result<crate::sketch::SketchBlock, String> {
    use crate::sketch::{Geometry, SketchBlock, SketchEntity, SketchPoint};
    use std::collections::HashMap;
    let spec = g.effective_spec();
    let curves: Vec<Curve> = match spec.kind {
        GearKind::Worm => worm_axial_outline(g, &spec)?,
        GearKind::Rack => {
            let body = if g.outer > 0.0 { g.outer } else { 3.0 * spec.module };
            let pts = spec.rack_outline(spec.teeth, body).map_err(|e| e.to_string())?;
            (0..pts.len()).map(|i| Curve::Line([pts[i][0], pts[i][1], 0.0], [pts[(i + 1) % pts.len()][0], pts[(i + 1) % pts.len()][1], 0.0])).collect()
        }
        _ => spec.profile().map_err(|e| e.to_string())?.curves,
    };
    let mut points = vec![SketchPoint { id: 0, x: 0.0, y: 0.0 }];
    let mut entities = Vec::new();
    let mut next = 1u32;
    let mut index: HashMap<(i64, i64), u32> = HashMap::new();
    let mut point = |p: [f64; 3], points: &mut Vec<SketchPoint>, next: &mut u32| -> u32 {
        let key = ((p[0] * 1e5).round() as i64, (p[1] * 1e5).round() as i64);
        *index.entry(key).or_insert_with(|| {
            points.push(SketchPoint { id: *next, x: p[0], y: p[1] });
            *next += 1;
            *next - 1
        })
    };
    for c in &curves {
        let geometry = match c {
            Curve::Line(a, b) => Geometry::Line { start: point(*a, &mut points, &mut next), end: point(*b, &mut points, &mut next) },
            Curve::Arc(a, _, b) => Geometry::Arc { center: 0, start: point(*a, &mut points, &mut next), end: point(*b, &mut points, &mut next) },
            Curve::SplineEnds { points: pts, start, end } => {
                let ids: Vec<u32> = pts.iter().map(|p| point(*p, &mut points, &mut next)).collect();
                // Manijas: la tangente sale del primer punto y llega al último, en el sentido de avance
                let len = |a: &P3, b: &P3| 0.3 * ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt().max(1e-3);
                let (p0, p1) = (pts[0], pts[pts.len() - 1]);
                let (l0, l1) = (len(&p0, &pts[1]), len(&p1, &pts[pts.len() - 2]));
                let h0 = SketchPoint { id: next, x: p0[0] + start[0] * l0, y: p0[1] + start[1] * l0 };
                let h1 = SketchPoint { id: next + 1, x: p1[0] + end[0] * l1, y: p1[1] + end[1] * l1 };
                next += 2;
                let (i0, i1) = (h0.id, h1.id);
                points.push(h0);
                points.push(h1);
                Geometry::Spline { points: ids, closed: false, start_handle: Some(i0), end_handle: Some(i1), handles: vec![] }
            }
            _ => continue,
        };
        entities.push(SketchEntity { id: next, construction: false, infinite: false, axis: false, layer: None, geometry });
        next += 1;
    }
    Ok(SketchBlock { id: 0, name: describe(g), points, entities, constraints: vec![] })
}

/// Tren planetario de una vez: sol, `planets` satélites y corona.
#[derive(Debug, Clone, Serialize)]
pub struct PlanetaryPlan {
    pub sun_teeth: u32,
    pub planet_teeth: u32,
    pub ring_teeth: u32,
    pub planets: u32,
    /// Dirección del centro de cada satélite visto desde el sol (°, desde su X)
    pub angles: Vec<f64>,
    /// Distancia del sol al centro de los satélites (mm)
    pub center_distance: f64,
    /// Relación de entrada (sol) a salida (portasatélites) con la corona quieta, y la del sol a la corona con el portasatélites quieto
    pub ratio_ring_fixed: f64,
    pub ratio_carrier_fixed: f64,
    /// Diámetro mayor del conjunto (corona) y del portasatélites que cabe (mm)
    pub warnings: Vec<String>,
}

/// Arma el plan de un planetario: con los dientes del sol y los del satélite **o** los de la corona
/// (`z_r = z_s + 2·z_p`), y cuántos satélites. Exige la condición de montaje: `(z_s + z_r)/N` entero
/// para que los satélites queden repartidos por igual y todos calcen con el sol y con la corona, y
/// que los satélites no se toquen entre sí.
pub fn planetary_plan(module: f64, sun_teeth: u32, planet_teeth: Option<u32>, ring_teeth: Option<u32>, planets: u32, pressure_angle: f64) -> Result<PlanetaryPlan, String> {
    if planets < 2 || planets > 8 {
        return Err("un planetario lleva de 2 a 8 satélites".into());
    }
    let (zp, zr) = match (planet_teeth, ring_teeth) {
        (Some(p), _) => (p, sun_teeth + 2 * p),
        (None, Some(r)) => {
            if r <= sun_teeth || (r - sun_teeth) % 2 != 0 {
                return Err(format!("con un sol de {sun_teeth} dientes la corona debe tener más y la misma paridad (z_r = z_s + 2·z_p); {} no sirve", r));
            }
            ((r - sun_teeth) / 2, r)
        }
        (None, None) => return Err("pide los dientes del satélite o los de la corona".into()),
    };
    if sun_teeth < 6 || zp < 6 {
        return Err("el sol y los satélites necesitan al menos 6 dientes".into());
    }
    if (sun_teeth + zr) % planets != 0 {
        // Satélites que sí sirven con estos dientes
        let ok: Vec<String> = (2..=8u32).filter(|n| (sun_teeth + zr) % n == 0).map(|n| n.to_string()).collect();
        return Err(format!(
            "condición de montaje: (z_sol + z_corona) = {} debe ser múltiplo de {planets} para repartir los satélites por igual{}",
            sun_teeth + zr,
            if ok.is_empty() { String::new() } else { format!("; con estos dientes sirven {} satélites", ok.join(", ")) }
        ));
    }
    let spec = |z| GearSpec { teeth: z, module, pressure_angle, ..Default::default() };
    let (sun, planet) = (spec(sun_teeth).circles().map_err(|e| e.to_string())?, spec(zp).circles().map_err(|e| e.to_string())?);
    let a = sun.pitch + planet.pitch;
    let mut warnings = Vec::new();
    // Los satélites vecinos no deben tocarse: el diámetro de cabeza cabe en la cuerda entre centros
    let chord = 2.0 * a * (PI / planets as f64).sin();
    if chord < 2.0 * planet.tip + 0.5 {
        return Err(format!("{planets} satélites no caben: la cuerda entre sus centros ({chord:.2} mm) no supera su diámetro exterior ({:.2} mm)", 2.0 * planet.tip));
    }
    if sun_teeth < 17 || zp < 17 {
        warnings.push("menos de 17 dientes socava el pie sin corrección de perfil".to_string());
    }
    // Interferencia del satélite con la corona al engranar por dentro (diferencia mínima de dientes)
    if zr - zp < 9 {
        warnings.push("la corona tiene muy pocos dientes más que el satélite: puede interferir al engranar por dentro".to_string());
    }
    Ok(PlanetaryPlan {
        sun_teeth,
        planet_teeth: zp,
        ring_teeth: zr,
        planets,
        angles: (0..planets).map(|k| 360.0 * k as f64 / planets as f64).collect(),
        center_distance: a,
        ratio_ring_fixed: 1.0 + zr as f64 / sun_teeth as f64,
        ratio_carrier_fixed: -(zr as f64) / sun_teeth as f64,
        warnings,
    })
}
