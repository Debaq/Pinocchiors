//! Piezas estándar métricas: tornillos (ISO 4017 cabeza hexagonal, ISO 4762
//! Allen, ISO 10642 avellanado Allen), tuercas hexagonales (ISO 4032) y
//! arandelas planas (ISO 7089), de M2 a M16.
//!
//! Todas se ubican con un marco cuyo origen está en la cara de apoyo y cuyo Z
//! sale del material: la cabeza del tornillo queda en +Z y la caña en −Z (el
//! avellanado queda con la cara de arriba en el origen, hundido en −Z); la
//! tuerca y la arandela crecen en +Z.

use crate::feature::BoltHead;
use crate::geom::{P3, add, cross, normalize, scale, sub};
use cad_occt::{Axis, Frame, Shape};

/// Medidas de una rosca métrica gruesa y de sus piezas (mm).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MetricSize {
    pub name: &'static str,
    /// Diámetro nominal y paso grueso
    pub d: f64,
    pub pitch: f64,
    /// Hexágono (entre caras) y alto de la cabeza hexagonal
    pub hex_s: f64,
    pub hex_k: f64,
    /// Alto de la tuerca
    pub nut_m: f64,
    /// Allen: diámetro de la cabeza, llave y profundidad del hexágono
    pub cap_dk: f64,
    pub cap_s: f64,
    pub cap_t: f64,
    /// Avellanado: diámetro de la cabeza, llave y profundidad
    pub csk_dk: f64,
    pub csk_s: f64,
    pub csk_t: f64,
    /// Arandela: interior, exterior y espesor
    pub washer_d1: f64,
    pub washer_d2: f64,
    pub washer_h: f64,
    /// Agujero pasante normal (ISO 273, serie media)
    pub clearance: f64,
}

macro_rules! size {
    ($n:literal, $d:literal, $p:literal, $s:literal, $k:literal, $m:literal, $dk:literal, $cs:literal, $ct:literal, $kdk:literal, $ks:literal, $kt:literal, $w1:literal, $w2:literal, $wh:literal, $cl:literal) => {
        MetricSize {
            name: $n,
            d: $d,
            pitch: $p,
            hex_s: $s,
            hex_k: $k,
            nut_m: $m,
            cap_dk: $dk,
            cap_s: $cs,
            cap_t: $ct,
            csk_dk: $kdk,
            csk_s: $ks,
            csk_t: $kt,
            washer_d1: $w1,
            washer_d2: $w2,
            washer_h: $wh,
            clearance: $cl,
        }
    };
}

/// Tabla de medidas (M2 no tiene avellanado ISO 10642: se usa 2,2·d).
pub const SIZES: &[MetricSize] = &[
    size!("M2", 2.0, 0.4, 4.0, 1.4, 1.6, 3.8, 1.5, 1.0, 4.4, 1.3, 0.8, 2.2, 5.0, 0.3, 2.4),
    size!("M2.5", 2.5, 0.45, 5.0, 1.7, 2.0, 4.5, 2.0, 1.1, 5.5, 1.5, 0.9, 2.7, 6.0, 0.5, 2.9),
    size!("M3", 3.0, 0.5, 5.5, 2.0, 2.4, 5.5, 2.5, 1.3, 6.72, 2.0, 1.1, 3.2, 7.0, 0.5, 3.4),
    size!("M4", 4.0, 0.7, 7.0, 2.8, 3.2, 7.0, 3.0, 2.0, 8.96, 2.5, 1.5, 4.3, 9.0, 0.8, 4.5),
    size!("M5", 5.0, 0.8, 8.0, 3.5, 4.7, 8.5, 4.0, 2.5, 11.2, 3.0, 1.9, 5.3, 10.0, 1.0, 5.5),
    size!("M6", 6.0, 1.0, 10.0, 4.0, 5.2, 10.0, 5.0, 3.0, 13.44, 4.0, 2.2, 6.4, 12.0, 1.6, 6.6),
    size!("M8", 8.0, 1.25, 13.0, 5.3, 6.8, 13.0, 6.0, 4.0, 17.92, 5.0, 3.0, 8.4, 16.0, 1.6, 9.0),
    size!("M10", 10.0, 1.5, 16.0, 6.4, 8.4, 16.0, 8.0, 5.0, 22.4, 6.0, 3.6, 10.5, 20.0, 2.0, 11.0),
    size!("M12", 12.0, 1.75, 18.0, 7.5, 10.8, 18.0, 10.0, 6.0, 26.88, 8.0, 4.3, 13.0, 24.0, 2.5, 13.5),
    size!("M16", 16.0, 2.0, 24.0, 10.0, 14.8, 24.0, 14.0, 8.0, 33.6, 10.0, 5.6, 17.0, 30.0, 3.0, 17.5),
];

pub fn size(name: &str) -> Option<&'static MetricSize> {
    SIZES.iter().find(|s| s.name.eq_ignore_ascii_case(name.trim()))
}

fn unknown(name: &str) -> String {
    format!("medida desconocida: {name} (hay {})", SIZES.iter().map(|s| s.name).collect::<Vec<_>>().join(", "))
}

impl MetricSize {
    /// Diámetro menor de la rosca exterior (perfil básico ISO 68-1)
    pub fn minor(&self) -> f64 {
        self.d - 1.082_532 * self.pitch
    }

    /// Alto de la cabeza avellanada a 90°
    pub fn csk_k(&self) -> f64 {
        (self.csk_dk - self.d) / 2.0
    }

    /// Largo roscado de un tornillo de largo `l` (2·d + 6, o todo si es corto)
    pub fn thread_length(&self, l: f64) -> f64 {
        (2.0 * self.d + 6.0).min(l)
    }
}

/// Marco local: origen, Z y X ortonormales.
struct Local {
    o: P3,
    z: P3,
    x: P3,
    y: P3,
}

impl Local {
    fn new(f: Frame) -> Self {
        let z = normalize(f.z);
        let x = normalize(sub(f.x, scale(z, crate::geom::dot(f.x, z))));
        Local { o: f.origin, z, x, y: cross(z, x) }
    }
    /// Punto en coordenadas locales
    fn p(&self, u: f64, v: f64, w: f64) -> P3 {
        add(self.o, add(scale(self.x, u), add(scale(self.y, v), scale(self.z, w))))
    }
    /// Marco con origen en la altura `w` (mismo Z, o el opuesto con `down`)
    fn at(&self, w: f64, down: bool) -> Frame {
        Frame { origin: self.p(0.0, 0.0, w), z: if down { scale(self.z, -1.0) } else { self.z }, x: self.x }
    }
    fn axis(&self, w: f64) -> Axis {
        Axis { origin: self.p(0.0, 0.0, w), dir: self.z }
    }
}

type R<T> = Result<T, String>;

fn e(x: cad_occt::Error) -> String {
    x.to_string()
}

/// Prisma hexagonal (entre caras `s`, caras perpendiculares a X) de `w0` a `w1`.
fn hex_prism(l: &Local, s: f64, w0: f64, w1: f64) -> R<Shape> {
    let r = s / 3f64.sqrt();
    let pts: Vec<P3> = (0..6)
        .map(|i| {
            let a = (30.0 + 60.0 * i as f64).to_radians();
            l.p(r * a.cos(), r * a.sin(), w0)
        })
        .collect();
    Shape::polygon(&pts).map_err(e)?.prism(scale(l.z, w1 - w0)).map_err(e)
}

/// Hexágono con las esquinas achaflanadas a 30° arriba (y abajo con `both`),
/// como las cabezas y tuercas reales.
fn chamfered_hex(l: &Local, s: f64, w0: f64, w1: f64, both: bool) -> R<Shape> {
    let h = w1 - w0;
    let big = s / 2.0 + h * 3f64.sqrt();
    let mut hex = hex_prism(l, s, w0, w1)?;
    hex = hex.intersect(&Shape::cone(l.at(w0, false), big, s / 2.0, h).map_err(e)?).map_err(e)?;
    if both {
        hex = hex.intersect(&Shape::cone(l.at(w1, true), big, s / 2.0, h).map_err(e)?).map_err(e)?;
    }
    Ok(hex)
}

/// Margen para que las herramientas no compartan caras con lo que cortan.
/// (La caña entra un cuarto de la cabeza: así la rosca no roza su cara de abajo.)
const MARGIN: f64 = 1e-3;

/// Caña de `w0` (punta) a `w1`, con la punta achaflanada a 45° del diámetro
/// menor al nominal; roscada desde la punta (2·d + 6) si `modeled`.
fn shank(l: &Local, m: &MetricSize, w0: f64, w1: f64, modeled: bool) -> R<Shape> {
    let r = m.d / 2.0;
    let len = w1 - w0;
    if !modeled {
        let r1 = m.minor() / 2.0;
        let c = (r - r1).min(len / 2.0);
        let tip = Shape::cone(l.at(w0, false), r - c, r, c).map_err(e)?;
        return tip.union(&Shape::cylinder(l.at(w0 + c, false), r, len - c).map_err(e)?).map_err(e);
    }
    // El chaflán sale en el mismo recorte de la rosca (una booleana menos)
    let lt = m.thread_length(len);
    let rod = Shape::thread_pointed(l.axis(w0), m.minor() / 2.0, r, m.pitch, lt, false).map_err(e)?;
    if lt < len - 1e-9 { rod.union(&Shape::cylinder(l.at(w0 + lt, false), r, len - lt).map_err(e)?).map_err(e) } else { Ok(rod) }
}

/// Hexágono de la llave Allen (entre caras `s`, profundidad `t` desde `top`).
fn socket(l: &Local, s: f64, t: f64, top: f64) -> R<Shape> {
    hex_prism(l, s, top - t, top + MARGIN)
}

/// Tornillo de largo `length` (bajo la cabeza; en el avellanado, el total).
pub fn bolt(frame: Frame, size_name: &str, length: f64, head: BoltHead, modeled: bool) -> R<Shape> {
    let m = size(size_name).ok_or_else(|| unknown(size_name))?;
    let l = Local::new(frame);
    if length <= 0.0 {
        return Err("el largo tiene que ser mayor que cero".into());
    }
    match head {
        BoltHead::Hex => {
            let head = chamfered_hex(&l, m.hex_s, 0.0, m.hex_k, false)?;
            head.union(&shank(&l, m, -length, 0.25 * m.hex_k, modeled)?).map_err(e)
        }
        BoltHead::Socket => {
            let head = Shape::cylinder(l.at(0.0, false), m.cap_dk / 2.0, m.d).map_err(e)?;
            let head = head.cut(&socket(&l, m.cap_s, m.cap_t, m.d)?).map_err(e)?;
            head.union(&shank(&l, m, -length, 0.25 * m.d, modeled)?).map_err(e)
        }
        BoltHead::Countersunk => {
            let k = m.csk_k();
            if length <= k {
                return Err(format!("el largo de un avellanado {} incluye la cabeza: más de {k:.2} mm", m.name));
            }
            let head = Shape::cone(l.at(-k, false), m.d / 2.0, m.csk_dk / 2.0, k).map_err(e)?;
            let head = head.cut(&socket(&l, m.csk_s, m.csk_t, 0.0)?).map_err(e)?;
            head.union(&shank(&l, m, -length, -0.8 * k, modeled)?).map_err(e)
        }
    }
}

/// Tuerca hexagonal; con `modeled` el agujero lleva la rosca interior.
pub fn nut(frame: Frame, size_name: &str, modeled: bool) -> R<Shape> {
    let m = size(size_name).ok_or_else(|| unknown(size_name))?;
    let l = Local::new(frame);
    let blank = chamfered_hex(&l, m.hex_s, 0.0, m.nut_m, true)?;
    // Sobresale medio paso de cada lado: las puntas del macho lejos de las caras
    let (from, span) = (-m.pitch / 2.0, m.nut_m + m.pitch);
    let hole = if modeled {
        // El macho con el núcleo al diámetro menor: lo que se quita es justo la rosca hembra
        Shape::thread(l.axis(from), m.minor() / 2.0, m.d / 2.0, m.pitch, span, false).map_err(e)?
    } else {
        Shape::cylinder(l.at(from, false), m.d / 2.0, span).map_err(e)?
    };
    blank.cut(&hole).map_err(e)
}

/// Arandela plana.
pub fn washer(frame: Frame, size_name: &str) -> R<Shape> {
    let m = size(size_name).ok_or_else(|| unknown(size_name))?;
    let l = Local::new(frame);
    let disk = Shape::cylinder(l.at(0.0, false), m.washer_d2 / 2.0, m.washer_h).map_err(e)?;
    disk.cut(&Shape::cylinder(l.at(-MARGIN, false), m.washer_d1 / 2.0, m.washer_h + 2.0 * MARGIN).map_err(e)?).map_err(e)
}

/// La medida que va en un agujero de diámetro `hole`: pasante (ISO 273) o
/// para roscar (broca o diámetro menor), la más cercana.
pub fn size_for_hole(hole: f64) -> &'static MetricSize {
    SIZES
        .iter()
        .min_by(|a, b| {
            // Pasante, broca para roscar (d − paso) o diámetro menor
            let gap = |m: &MetricSize| [m.clearance, m.d - m.pitch, m.minor()].iter().map(|x| (x - hole).abs()).fold(f64::MAX, f64::min);
            gap(a).total_cmp(&gap(b))
        })
        .unwrap()
}

/// Nombre de catálogo de una pieza estándar ("Tornillo Allen M6×20"), o
/// `None` si la forma no es una.
pub fn describe(shape: &crate::feature::PrimitiveShape) -> Option<String> {
    use crate::feature::PrimitiveShape as P;
    let mm = |v: f64| if (v - v.round()).abs() < 1e-9 { format!("{}", v.round()) } else { format!("{v}") };
    match shape {
        P::Bolt { size, length, head, .. } => {
            let kind = match head {
                BoltHead::Hex => "hexagonal",
                BoltHead::Socket => "Allen",
                BoltHead::Countersunk => "avellanado",
            };
            Some(format!("Tornillo {kind} {size}×{}", mm(*length)))
        }
        P::Nut { size, .. } => Some(format!("Tuerca {size}")),
        P::Washer { size } => Some(format!("Arandela {size}")),
        _ => None,
    }
}
