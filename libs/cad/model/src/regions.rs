//! Regiones cerradas de un sketch (lo que se puede extruir).
//!
//! Las líneas, arcos y splines abiertas forman un grafo plano: cada cara acotada
//! del grafo es un lazo. Círculos y splines cerradas son lazos sueltos. Un lazo
//! dentro de otro es su agujero; el de adentro también es región propia.
//!
//! Las curvas que se cruzan (o donde el extremo de una toca el medio de otra)
//! se parten ahí: dos cuadrados superpuestos dan tres regiones (lo de cada uno
//! y lo común), como las áreas que delimitan las líneas en el dibujo. Las ramas
//! sueltas se ignoran.

use std::collections::HashMap;
use std::f64::consts::PI;

use serde::{Deserialize, Serialize};

use crate::geom::{P2, dist2};
use crate::sketch::{Geometry, Sketch, SketchError};

/// Un tramo de lazo: entidad recorrida en su sentido o al revés.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LoopPiece {
    pub entity: u32,
    pub reversed: bool,
    /// Solo un tramo de la entidad (partida donde la cruza otra): desde dónde
    /// hasta dónde, en el sentido natural de la entidad.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<[P2; 2]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Loop {
    pub pieces: Vec<LoopPiece>,
    /// Polígono muestreado (antihorario) para contención y dibujo.
    pub polygon: Vec<P2>,
    pub area: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Region {
    pub outer: Loop,
    pub holes: Vec<Loop>,
    /// Cuántos lazos la contienen: las de profundidad par forman el perfil
    /// por defecto (anillo sí, su agujero no, isla dentro del agujero sí).
    pub depth: usize,
    /// Un punto interior (para elegirla con un clic y referenciarla).
    pub sample: P2,
}

impl Region {
    pub fn contains(&self, p: P2) -> bool {
        point_in_polygon(p, &self.outer.polygon) && !self.holes.iter().any(|h| point_in_polygon(p, &h.polygon))
    }

    pub fn area(&self) -> f64 {
        self.outer.area - self.holes.iter().map(|h| h.area).sum::<f64>()
    }
}

const SEGMENTS_PER_TURN: f64 = 64.0;

/// Puntos de una entidad en su sentido natural (para muestrear y orientar).
pub fn sample_entity(s: &Sketch, id: u32) -> Result<Vec<P2>, SketchError> {
    sample_geometry(&s.entity(id)?.geometry, &|p| s.point(p))
}

/// Puntos de una geometría con los puntos que da `point` (con un índice armado
/// una vez: buscar cada punto en la lista del sketch es lineal).
fn sample_geometry(g: &Geometry, point: &impl Fn(u32) -> Result<P2, SketchError>) -> Result<Vec<P2>, SketchError> {
    Ok(match g {
        Geometry::Line { start, end } => vec![point(*start)?, point(*end)?],
        Geometry::Circle { center, radius } => {
            let c = point(*center)?;
            (0..=64).map(|i| {
                let a = i as f64 / 64.0 * 2.0 * PI;
                [c[0] + radius * a.cos(), c[1] + radius * a.sin()]
            }).collect()
        }
        Geometry::Arc { center, start, end } => {
            let (c, a, b) = (point(*center)?, point(*start)?, point(*end)?);
            let r = dist2(c, a);
            let a0 = (a[1] - c[1]).atan2(a[0] - c[0]);
            let sweep = arc_sweep(c, a, b);
            let n = ((sweep / (2.0 * PI) * SEGMENTS_PER_TURN).ceil() as usize).max(2);
            let mut pts: Vec<P2> = (0..=n).map(|i| {
                let t = a0 + sweep * i as f64 / n as f64;
                [c[0] + r * t.cos(), c[1] + r * t.sin()]
            }).collect();
            // Extremos exactos (el radio de `end` puede diferir levemente)
            *pts.last_mut().unwrap() = b;
            pts
        }
        Geometry::Spline { points, closed, .. } => {
            let mut pts: Vec<P2> = points.iter().map(|p| point(*p)).collect::<Result<_, _>>()?;
            if *closed && let Some(&f) = pts.first() {
                pts.push(f);
            }
            catmull_rom(&pts)
        }
        Geometry::Point { point: p } => vec![point(*p)?],
        Geometry::Unknown => vec![],
        Geometry::Ellipse { center, major, minor } => {
            let (c, a, b) = (point(*center)?, point(*major)?, point(*minor)?);
            let (u, v) = ([a[0] - c[0], a[1] - c[1]], [b[0] - c[0], b[1] - c[1]]);
            (0..=64)
                .map(|i| {
                    let t = i as f64 / 64.0 * 2.0 * PI;
                    [c[0] + u[0] * t.cos() + v[0] * t.sin(), c[1] + u[1] * t.cos() + v[1] * t.sin()]
                })
                .collect()
        }
        Geometry::EllipseArc { center, major, minor, start, end } => {
            let (c, a, b) = (point(*center)?, point(*major)?, point(*minor)?);
            let (p0, p1) = (point(*start)?, point(*end)?);
            let (u, v) = ([a[0] - c[0], a[1] - c[1]], [b[0] - c[0], b[1] - c[1]]);
            // Antihorario en el plano, sea cual sea el lado del semieje menor
            let sense = if u[0] * v[1] - u[1] * v[0] >= 0.0 { 1.0 } else { -1.0 };
            let param = |p: P2| {
                let q = [p[0] - c[0], p[1] - c[1]];
                let d = |x: [f64; 2], y: [f64; 2]| x[0] * y[0] + x[1] * y[1];
                (d(q, v) / d(v, v)).atan2(d(q, u) / d(u, u))
            };
            let t0 = param(p0);
            let mut sweep = (param(p1) - t0) * sense;
            while sweep <= 1e-12 {
                sweep += 2.0 * PI;
            }
            let n = ((sweep / (2.0 * PI) * SEGMENTS_PER_TURN).ceil() as usize).max(4);
            let mut pts: Vec<P2> = (0..=n)
                .map(|i| {
                    let t = t0 + sense * sweep * i as f64 / n as f64;
                    [c[0] + u[0] * t.cos() + v[0] * t.sin(), c[1] + u[1] * t.cos() + v[1] * t.sin()]
                })
                .collect();
            // Extremos exactos: así se unen con lo que comparte el punto
            pts[0] = p0;
            *pts.last_mut().unwrap() = p1;
            pts
        }
        Geometry::BSpline { poles, degree, closed, weights, knots } => {
            let pts: Vec<nalgebra::Vector2<f64>> = poles.iter().map(|p| point(*p).map(|q| nalgebra::Vector2::new(q[0], q[1]))).collect::<Result<_, _>>()?;
            let curve = cad_solver::BSpline::new(&pts, weights, *degree as usize, *closed, knots)
                .ok_or_else(|| SketchError::Unsupported("B-spline con nudos que no cuadran con los polos".into()))?;
            // 16 por tramo y al menos 64 en total (una cónica es un tramo solo)
            let spans = curve.breaks().len() - 1;
            let mut out: Vec<P2> = curve.sample(16.max(64usize.div_ceil(spans.max(1)))).into_iter().map(|q| [q.x, q.y]).collect();
            // Extremos exactos (abierta: el primer y el último polo)
            if !*closed && let (Some(f), Some(l)) = (pts.first(), pts.last()) {
                out[0] = [f.x, f.y];
                *out.last_mut().unwrap() = [l.x, l.y];
            }
            out
        }
    })
}

/// Barrido antihorario de un arco, en (0, 2π].
pub fn arc_sweep(c: P2, a: P2, b: P2) -> f64 {
    let a0 = (a[1] - c[1]).atan2(a[0] - c[0]);
    let a1 = (b[1] - c[1]).atan2(b[0] - c[0]);
    let mut s = a1 - a0;
    while s <= 1e-12 {
        s += 2.0 * PI;
    }
    s
}

/// Aproximación visual de la spline interpolada (OCCT usa su propia B-spline).
fn catmull_rom(p: &[P2]) -> Vec<P2> {
    if p.len() < 3 {
        return p.to_vec();
    }
    let n = p.len();
    let mut out: Vec<P2> = (0..(n - 1) * 8).map(|k| catmull_at(p, k as f64)).collect();
    out.push(p[n - 1]);
    out
}

/// Punto de la spline interpolada en `k` (8 muestras por tramo, como `catmull_rom`).
fn catmull_at(p: &[P2], k: f64) -> P2 {
    let n = p.len();
    let closed = dist2(p[0], p[n - 1]) < 1e-12;
    let get = |i: isize| -> P2 {
        if closed {
            let m = (n - 1) as isize;
            p[i.rem_euclid(m) as usize]
        } else {
            p[i.clamp(0, n as isize - 1) as usize]
        }
    };
    let i = ((k / 8.0).floor().max(0.0) as usize).min(n - 2);
    let t = k / 8.0 - i as f64;
    let (p0, p1, p2, p3) = (get(i as isize - 1), get(i as isize), get(i as isize + 1), get(i as isize + 2));
    let (t2, t3) = (t * t, t * t * t);
    let f = |a: f64, b: f64, c: f64, d: f64| 0.5 * (2.0 * b + (-a + c) * t + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2 + (-a + 3.0 * b - 3.0 * c + d) * t3);
    [f(p0[0], p1[0], p2[0], p3[0]), f(p0[1], p1[1], p2[1], p3[1])]
}

/// La curva en función de la posición en su muestreo (`k`: tramo + fracción,
/// como los puntos de `sample_geometry`, que tiene `n` puntos): para llevar
/// los cruces de las cuerdas a la curva.
type Eval = Box<dyn Fn(f64) -> P2>;

fn curve_eval(g: &Geometry, point: &impl Fn(u32) -> Result<P2, SketchError>, n: usize) -> Option<Eval> {
    let segs = n.checked_sub(1).filter(|&m| m > 0)? as f64;
    Some(match g {
        Geometry::Line { start, end } => {
            let (a, b) = (point(*start).ok()?, point(*end).ok()?);
            Box::new(move |k| [a[0] + k * (b[0] - a[0]), a[1] + k * (b[1] - a[1])])
        }
        Geometry::Circle { center, radius } => {
            let (c, r) = (point(*center).ok()?, *radius);
            Box::new(move |k| {
                let a = k / segs * 2.0 * PI;
                [c[0] + r * a.cos(), c[1] + r * a.sin()]
            })
        }
        Geometry::Arc { center, start, end } => {
            let (c, a, b) = (point(*center).ok()?, point(*start).ok()?, point(*end).ok()?);
            let r = dist2(c, a);
            let a0 = (a[1] - c[1]).atan2(a[0] - c[0]);
            let sweep = arc_sweep(c, a, b);
            Box::new(move |k| {
                let t = a0 + sweep * k / segs;
                [c[0] + r * t.cos(), c[1] + r * t.sin()]
            })
        }
        Geometry::Ellipse { center, major, minor } => {
            let (c, a, b) = (point(*center).ok()?, point(*major).ok()?, point(*minor).ok()?);
            let (u, v) = ([a[0] - c[0], a[1] - c[1]], [b[0] - c[0], b[1] - c[1]]);
            Box::new(move |k| {
                let t = k / segs * 2.0 * PI;
                [c[0] + u[0] * t.cos() + v[0] * t.sin(), c[1] + u[1] * t.cos() + v[1] * t.sin()]
            })
        }
        Geometry::EllipseArc { center, major, minor, start, end } => {
            let (c, a, b) = (point(*center).ok()?, point(*major).ok()?, point(*minor).ok()?);
            let (p0, p1) = (point(*start).ok()?, point(*end).ok()?);
            let (u, v) = ([a[0] - c[0], a[1] - c[1]], [b[0] - c[0], b[1] - c[1]]);
            let sense = if u[0] * v[1] - u[1] * v[0] >= 0.0 { 1.0 } else { -1.0 };
            let d = |x: [f64; 2], y: [f64; 2]| x[0] * y[0] + x[1] * y[1];
            let param = |p: P2| {
                let q = [p[0] - c[0], p[1] - c[1]];
                (d(q, v) / d(v, v)).atan2(d(q, u) / d(u, u))
            };
            let t0 = param(p0);
            let mut sweep = (param(p1) - t0) * sense;
            while sweep <= 1e-12 {
                sweep += 2.0 * PI;
            }
            Box::new(move |k| {
                let t = t0 + sense * sweep * k / segs;
                [c[0] + u[0] * t.cos() + v[0] * t.sin(), c[1] + u[1] * t.cos() + v[1] * t.sin()]
            })
        }
        Geometry::Spline { points, closed, .. } => {
            let mut pts: Vec<P2> = points.iter().map(|p| point(*p)).collect::<Result<_, _>>().ok()?;
            if *closed && let Some(&f) = pts.first() {
                pts.push(f);
            }
            if pts.len() < 3 {
                return None;
            }
            Box::new(move |k| catmull_at(&pts, k))
        }
        Geometry::BSpline { poles, degree, closed, weights, knots } => {
            let pts: Vec<nalgebra::Vector2<f64>> = poles.iter().map(|p| point(*p).map(|q| nalgebra::Vector2::new(q[0], q[1]))).collect::<Result<_, _>>().ok()?;
            let curve = cad_solver::BSpline::new(&pts, weights, *degree as usize, *closed, knots)?;
            let br = curve.breaks();
            let spans = br.len() - 1;
            let per = 16.max(64usize.div_ceil(spans.max(1))) as f64;
            Box::new(move |k| {
                let k = k.clamp(0.0, segs);
                let j = ((k / per).floor() as usize).min(spans - 1);
                let u = br[j] + (br[j + 1] - br[j]) * (k - j as f64 * per) / per;
                let q = curve.eval(u);
                [q.x, q.y]
            })
        }
        Geometry::Point { .. } | Geometry::Unknown => return None,
    })
}

/// Lleva un cruce de las cuerdas (posiciones `ka`, `kb` en el muestreo de
/// cada curva) al cruce de las curvas: Newton sobre las dos posiciones. None
/// si no converge cerca.
fn refine_on_curves(fa: &Eval, fb: &Eval, ka: f64, kb: f64, eps: f64) -> Option<(f64, f64, P2)> {
    let (mut a, mut b) = (ka, kb);
    let h = 1e-4;
    let deriv = |f: &Eval, k: f64| {
        let (p, q) = (f(k + h), f(k - h));
        [(p[0] - q[0]) / (2.0 * h), (p[1] - q[1]) / (2.0 * h)]
    };
    for _ in 0..16 {
        let (pa, pb) = (fa(a), fb(b));
        let f = [pa[0] - pb[0], pa[1] - pb[1]];
        if f[0].hypot(f[1]) <= eps {
            return Some((a, b, [(pa[0] + pb[0]) / 2.0, (pa[1] + pb[1]) / 2.0]));
        }
        let (da, db) = (deriv(fa, a), deriv(fb, b));
        // [da, −db]·[x, y] = −f
        let (m00, m01, m10, m11) = (da[0], -db[0], da[1], -db[1]);
        let det = m00 * m11 - m01 * m10;
        if det.abs() < 1e-300 || !det.is_finite() {
            return None;
        }
        let (r0, r1) = (-f[0], -f[1]);
        a += (r0 * m11 - m01 * r1) / det;
        b += (m00 * r1 - r0 * m10) / det;
        if !(a - ka).is_finite() || (a - ka).abs() > 2.0 || (b - kb).abs() > 2.0 {
            return None;
        }
    }
    None
}

pub fn signed_area(poly: &[P2]) -> f64 {
    let n = poly.len();
    (0..n).map(|i| {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        a[0] * b[1] - b[0] * a[1]
    }).sum::<f64>() / 2.0
}

pub fn point_in_polygon(p: P2, poly: &[P2]) -> bool {
    let mut inside = false;
    let n = poly.len();
    let mut j = n.wrapping_sub(1);
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Punto interior de un polígono simple (no siempre el centroide: puede caer
/// fuera en formas en C). Busca sobre horizontales el tramo interior más ancho.
fn interior_point(poly: &[P2], holes: &[&[P2]]) -> P2 {
    let (mut y0, mut y1) = (f64::MAX, f64::MIN);
    for p in poly {
        y0 = y0.min(p[1]);
        y1 = y1.max(p[1]);
    }
    let mut best = (0.0, poly[0]);
    for k in 1..16 {
        let y = y0 + (y1 - y0) * (k as f64 + 0.13) / 16.0;
        let mut xs: Vec<f64> = Vec::new();
        for ring in std::iter::once(poly).chain(holes.iter().copied()) {
            let n = ring.len();
            for i in 0..n {
                let (a, b) = (ring[i], ring[(i + 1) % n]);
                if (a[1] > y) != (b[1] > y) {
                    xs.push(a[0] + (y - a[1]) * (b[0] - a[0]) / (b[1] - a[1]));
                }
            }
        }
        xs.sort_by(f64::total_cmp);
        for w in xs.as_chunks::<2>().0 {
            let width = w[1] - w[0];
            if width > best.0 {
                best = (width, [(w[0] + w[1]) / 2.0, y]);
            }
        }
    }
    best.1
}

/// Media arista: (arista, recorrida al revés).
type Half = (usize, bool);

struct Edge {
    entity: u32,
    nodes: [usize; 2],
    /// Puntos en sentido natural (de nodes[0] a nodes[1]).
    pts: Vec<P2>,
    /// Tramo de la entidad (None: entera).
    span: Option<[P2; 2]>,
}

/// Forma exacta de una entidad, para que los cruces caigan justo sobre la curva
/// (el muestreo de un círculo queda hasta 0,1 % del radio adentro).
#[derive(Clone, Copy)]
enum Exact {
    Line(P2, P2),
    Circle(P2, f64),
}

fn exact(g: &Geometry, point: &impl Fn(u32) -> Result<P2, SketchError>) -> Option<Exact> {
    match g {
        Geometry::Line { start, end } => Some(Exact::Line(point(*start).ok()?, point(*end).ok()?)),
        Geometry::Circle { center, radius } => Some(Exact::Circle(point(*center).ok()?, *radius)),
        Geometry::Arc { center, start, .. } => {
            let c = point(*center).ok()?;
            Some(Exact::Circle(c, dist2(c, point(*start).ok()?)))
        }
        _ => None,
    }
}

/// Lleva un cruce aproximado al cruce exacto más cercano de dos formas.
fn refine(a: Exact, b: Exact, x: P2) -> P2 {
    let near = |cands: Vec<P2>| cands.into_iter().min_by(|p, q| dist2(*p, x).total_cmp(&dist2(*q, x))).unwrap_or(x);
    let line_circle = |p: P2, q: P2, c: P2, r: f64| {
        let d = [q[0] - p[0], q[1] - p[1]];
        let f = [p[0] - c[0], p[1] - c[1]];
        let (aa, bb, cc) = (d[0] * d[0] + d[1] * d[1], 2.0 * (f[0] * d[0] + f[1] * d[1]), f[0] * f[0] + f[1] * f[1] - r * r);
        let disc = (bb * bb - 4.0 * aa * cc).max(0.0).sqrt();
        if aa <= 0.0 {
            return x;
        }
        near([(-bb - disc) / (2.0 * aa), (-bb + disc) / (2.0 * aa)].iter().map(|t| [p[0] + t * d[0], p[1] + t * d[1]]).collect())
    };
    match (a, b) {
        (Exact::Line(..), Exact::Line(..)) => x,
        (Exact::Line(p, q), Exact::Circle(c, r)) | (Exact::Circle(c, r), Exact::Line(p, q)) => line_circle(p, q, c, r),
        (Exact::Circle(c0, r0), Exact::Circle(c1, r1)) => {
            let d = dist2(c0, c1);
            if d <= 0.0 {
                return x;
            }
            let a = (r0 * r0 - r1 * r1 + d * d) / (2.0 * d);
            let h = (r0 * r0 - a * a).max(0.0).sqrt();
            let u = [(c1[0] - c0[0]) / d, (c1[1] - c0[1]) / d];
            let m = [c0[0] + a * u[0], c0[1] + a * u[1]];
            near(vec![[m[0] - h * u[1], m[1] + h * u[0]], [m[0] + h * u[1], m[1] - h * u[0]]])
        }
    }
}

/// Cruce de los segmentos ab y cd: parámetros en cada uno (sin los paralelos).
fn seg_cross(a: P2, b: P2, c: P2, d: P2, eps: f64) -> Option<(f64, f64)> {
    let r = [b[0] - a[0], b[1] - a[1]];
    let q = [d[0] - c[0], d[1] - c[1]];
    let den = r[0] * q[1] - r[1] * q[0];
    let (lr, lq) = ((r[0] * r[0] + r[1] * r[1]).sqrt(), (q[0] * q[0] + q[1] * q[1]).sqrt());
    if den.abs() <= 1e-12 * lr * lq {
        return None;
    }
    let w = [c[0] - a[0], c[1] - a[1]];
    let t = (w[0] * q[1] - w[1] * q[0]) / den;
    let u = (w[0] * r[1] - w[1] * r[0]) / den;
    // Margen en longitud: un extremo apoyado justo sobre el otro tramo cuenta
    let (et, eu) = (eps / lr.max(1e-300), eps / lq.max(1e-300));
    ((-et..=1.0 + et).contains(&t) && (-eu..=1.0 + eu).contains(&u)).then_some((t.clamp(0.0, 1.0), u.clamp(0.0, 1.0)))
}

/// Segmentos encimados (colineales): los extremos de cada uno que caen en el
/// medio del otro, como (parámetro en ab, parámetro en cd, punto).
fn seg_overlap(a: P2, b: P2, c: P2, d: P2, eps: f64) -> Vec<(f64, f64, P2)> {
    let r = [b[0] - a[0], b[1] - a[1]];
    let q = [d[0] - c[0], d[1] - c[1]];
    let (lr2, lq2) = (r[0] * r[0] + r[1] * r[1], q[0] * q[0] + q[1] * q[1]);
    if lr2 < 1e-300 || lq2 < 1e-300 {
        return Vec::new();
    }
    let off = |p: P2| (r[0] * (p[1] - a[1]) - r[1] * (p[0] - a[0])).abs() / lr2.sqrt();
    if off(c) > eps || off(d) > eps {
        return Vec::new();
    }
    let on_ab = |p: P2| ((p[0] - a[0]) * r[0] + (p[1] - a[1]) * r[1]) / lr2;
    let on_cd = |p: P2| ((p[0] - c[0]) * q[0] + (p[1] - c[1]) * q[1]) / lq2;
    let (et, eu) = (eps / lr2.sqrt(), eps / lq2.sqrt());
    let inside = |t: f64, e: f64| t > e && t < 1.0 - e;
    let mut out = Vec::new();
    for (u, p) in [(0.0, c), (1.0, d)] {
        let t = on_ab(p);
        if inside(t, et) {
            out.push((t, u, p));
        }
    }
    for (t, p) in [(0.0, a), (1.0, b)] {
        let u = on_cd(p);
        if inside(u, eu) {
            out.push((t, u, p));
        }
    }
    out
}

/// Punto a mitad de largo de una polilínea.
fn half_way(pts: &[P2]) -> P2 {
    let total: f64 = pts.windows(2).map(|w| dist2(w[0], w[1])).sum();
    let mut left = total / 2.0;
    for w in pts.windows(2) {
        let l = dist2(w[0], w[1]);
        if l >= left && l > 0.0 {
            let t = left / l;
            return [w[0][0] + t * (w[1][0] - w[0][0]), w[0][1] + t * (w[1][1] - w[0][1])];
        }
        left -= l;
    }
    pts[pts.len() - 1]
}

/// Una curva a partir: sus puntos (cerrada: el último repite el primero) y los
/// cortes como (segmento + parámetro, punto).
struct Cuttable {
    entity: u32,
    pts: Vec<P2>,
    closed: bool,
    exact: Option<Exact>,
    /// La curva por posición en `pts` (para llevar los cruces a la curva)
    eval: Option<Eval>,
    cuts: Vec<(f64, P2)>,
}

/// Parte las curvas donde se cruzan o donde una toca el medio de otra.
fn cut_crossings(curves: &mut [Cuttable], tol: f64) {
    let bbox = |p: &[P2]| {
        p.iter().fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |b, q| [b[0].min(q[0]), b[1].min(q[1]), b[2].max(q[0]), b[3].max(q[1])])
    };
    let boxes: Vec<[f64; 4]> = curves.iter().map(|c| bbox(&c.pts)).collect();
    let seg_boxes: Vec<Vec<[f64; 4]>> = curves.iter().map(|c| c.pts.windows(2).map(bbox).collect()).collect();
    let touch = tol * 100.0;
    // Barrido por x: solo se miran los pares cuyas cajas se pisan en x (con
    // miles de curvas, todos contra todos era cuadrático)
    let mut order: Vec<usize> = (0..curves.len()).collect();
    order.sort_by(|&a, &b| boxes[a][0].total_cmp(&boxes[b][0]));
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    for (k, &a) in order.iter().enumerate() {
        for &b in &order[k + 1..] {
            if boxes[b][0] > boxes[a][2] + touch {
                break;
            }
            pairs.push((a.min(b), a.max(b)));
        }
    }
    // En el orden de antes (los cortes de cada curva salen en el mismo orden)
    pairs.sort_unstable();
    for (i, j) in pairs {
        {
            let (a, b) = (boxes[i], boxes[j]);
            if a[0] > b[2] + touch || b[0] > a[2] + touch || a[1] > b[3] + touch || b[1] > a[3] + touch {
                continue;
            }
            let mut found: Vec<(f64, f64, P2)> = Vec::new();
            for (si, w) in curves[i].pts.windows(2).enumerate() {
                let sb = seg_boxes[i][si];
                // Este segmento lejos de toda la otra curva
                if sb[0] > b[2] + touch || b[0] > sb[2] + touch || sb[1] > b[3] + touch || b[1] > sb[3] + touch {
                    continue;
                }
                for (sj, v) in curves[j].pts.windows(2).enumerate() {
                    let vb = seg_boxes[j][sj];
                    if sb[0] > vb[2] + touch || vb[0] > sb[2] + touch || sb[1] > vb[3] + touch || vb[1] > sb[3] + touch {
                        continue;
                    }
                    let (si, sj) = (si as f64, sj as f64);
                    let hits = match seg_cross(w[0], w[1], v[0], v[1], touch) {
                        Some((t, u)) => {
                            // Un extremo de una curva sobre la otra: el punto es ese extremo
                            if t == 0.0 || t == 1.0 {
                                vec![(si + t, sj + u, w[t as usize])]
                            } else if u == 0.0 || u == 1.0 {
                                vec![(si + t, sj + u, v[u as usize])]
                            } else {
                                let p = [w[0][0] + t * (w[1][0] - w[0][0]), w[0][1] + t * (w[1][1] - w[0][1])];
                                vec![settle(&curves[i], &curves[j], si + t, sj + u, p, tol)]
                            }
                        }
                        // Paralelos: si van encimados, se parten donde termina cada uno
                        None => seg_overlap(w[0], w[1], v[0], v[1], touch).into_iter().map(|(t, u, x)| (si + t, sj + u, x)).collect(),
                    };
                    for (ka, kb, x) in hits {
                        if !found.iter().any(|f| dist2(f.2, x) <= touch) {
                            found.push((ka, kb, x));
                        }
                    }
                }
            }
            for (ti, tj, x) in found {
                curves[i].cuts.push((ti, x));
                curves[j].cuts.push((tj, x));
            }
        }
    }
    // Curvas que se cruzan a sí mismas (una spline en ocho): las líneas, los
    // círculos y los arcos no pueden
    for (i, c) in curves.iter_mut().enumerate() {
        if c.exact.is_some() || c.pts.len() < 4 {
            continue;
        }
        let segs = c.pts.len() - 1;
        let mut found: Vec<(f64, f64, P2)> = Vec::new();
        for si in 0..segs {
            let sb = seg_boxes[i][si];
            for sj in si + 2..segs {
                // La primera y la última de una cerrada se tocan en la unión
                if c.closed && si == 0 && sj == segs - 1 {
                    continue;
                }
                let vb = seg_boxes[i][sj];
                if sb[0] > vb[2] || vb[0] > sb[2] || sb[1] > vb[3] || vb[1] > sb[3] {
                    continue;
                }
                let (w, v) = ([c.pts[si], c.pts[si + 1]], [c.pts[sj], c.pts[sj + 1]]);
                let Some((t, u)) = seg_cross(w[0], w[1], v[0], v[1], 0.0) else { continue };
                let p = [w[0][0] + t * (w[1][0] - w[0][0]), w[0][1] + t * (w[1][1] - w[0][1])];
                let (ka, kb, x) = settle(c, c, si as f64 + t, sj as f64 + u, p, tol);
                if (ka - kb).abs() > 0.5 && !found.iter().any(|f| dist2(f.2, x) <= touch) {
                    found.push((ka, kb, x));
                }
            }
        }
        for (ka, kb, x) in found {
            c.cuts.push((ka, x));
            c.cuts.push((kb, x));
        }
    }
}

/// Cruce de las cuerdas llevado a las curvas: en forma cerrada entre rectas
/// y círculos, por Newton si alguna es elipse o spline. Posiciones en el
/// muestreo de cada una y el punto.
fn settle(a: &Cuttable, b: &Cuttable, ka: f64, kb: f64, chord: P2, tol: f64) -> (f64, f64, P2) {
    if let (Some(ea), Some(eb)) = (a.exact, b.exact) {
        return (ka, kb, refine(ea, eb, chord));
    }
    let (Some(fa), Some(fb)) = (&a.eval, &b.eval) else { return (ka, kb, chord) };
    // Cerrada: la posición da la vuelta; abierta: dentro de la curva
    let wrap = |c: &Cuttable, k: f64| {
        let last = (c.pts.len() - 1) as f64;
        if c.closed { k.rem_euclid(last) } else { k.clamp(0.0, last) }
    };
    match refine_on_curves(fa, fb, ka, kb, tol) {
        Some((x, y, p)) => (wrap(a, x), wrap(b, y), p),
        None => (ka, kb, chord),
    }
}

/// Parte una curva en sus cortes: tramos (puntos en sentido natural, tramo).
fn split_curve(c: &Cuttable, tol: f64) -> Vec<(Vec<P2>, Option<[P2; 2]>)> {
    let n = c.pts.len();
    let at = |k: f64| -> P2 {
        let i = (k.floor() as usize).min(n - 2);
        let t = k - i as f64;
        let (a, b) = (c.pts[i], c.pts[i + 1]);
        [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]
    };
    let mut cuts = c.cuts.clone();
    cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
    let touch = tol * 100.0;
    // Sin cortes sobre los extremos de una abierta (ahí ya hay un nodo) ni repetidos
    let mut keep: Vec<(f64, P2)> = Vec::new();
    for (k, x) in cuts {
        if !c.closed && (dist2(x, c.pts[0]) <= touch || dist2(x, c.pts[n - 1]) <= touch) {
            continue;
        }
        // El mismo punto en otra parte de la curva no es repetido: es donde se cruza consigo misma
        let last = (n - 1) as f64;
        let same_place = |q: f64| (q - k).abs() < 1.0 || (c.closed && last - (q - k).abs() < 1.0);
        if keep.iter().any(|q| dist2(q.1, x) <= touch && same_place(q.0)) {
            continue;
        }
        keep.push((k, x));
    }
    // Tramo entre dos posiciones (k0 < k1) con sus extremos exactos
    let piece = |k0: f64, p0: P2, k1: f64, p1: P2| -> Vec<P2> {
        let mut out = vec![p0];
        out.extend((0..n).filter(|&i| (i as f64) > k0 && (i as f64) < k1).map(|i| c.pts[i]));
        out.push(p1);
        out
    };
    if c.closed {
        // Cerrada: con menos de dos cortes sigue siendo un lazo suelto
        if keep.len() < 2 {
            return vec![(c.pts.clone(), None)];
        }
        let last = (n - 1) as f64;
        let mut out = Vec::new();
        for w in 0..keep.len() {
            let (k0, p0) = keep[w];
            if w + 1 < keep.len() {
                let (k1, p1) = keep[w + 1];
                out.push((piece(k0, p0, k1, p1), Some([p0, p1])));
            } else {
                // El último da la vuelta por el comienzo
                let (k1, p1) = keep[0];
                let mut a = piece(k0, p0, last, at(last));
                a.pop();
                a.extend(piece(0.0, c.pts[0], k1, p1));
                out.push((a, Some([p0, p1])));
            }
        }
        return out;
    }
    if keep.is_empty() {
        return vec![(c.pts.clone(), None)];
    }
    let mut stops = vec![(0.0, c.pts[0])];
    stops.extend(keep);
    stops.push(((n - 1) as f64, c.pts[n - 1]));
    stops.windows(2).map(|w| (piece(w[0].0, w[0].1, w[1].0, w[1].1), Some([w[0].1, w[1].1]))).collect()
}

/// Puntos del tramo de una entidad que usa un lazo (en el sentido natural).
pub fn piece_points(s: &Sketch, piece: &LoopPiece) -> Result<Vec<P2>, SketchError> {
    let pts = sample_entity(s, piece.entity)?;
    let Some([from, to]) = piece.span else { return Ok(pts) };
    let closed = s.entity(piece.entity)?.geometry.is_closed();
    let c = Cuttable { entity: piece.entity, pts: pts.clone(), closed, exact: None, eval: None, cuts: Vec::new() };
    let pos = |p: P2| -> f64 {
        let mut best = (f64::MAX, 0.0);
        for (i, w) in pts.windows(2).enumerate() {
            let d = [w[1][0] - w[0][0], w[1][1] - w[0][1]];
            let l2 = d[0] * d[0] + d[1] * d[1];
            let t = if l2 > 0.0 { (((p[0] - w[0][0]) * d[0] + (p[1] - w[0][1]) * d[1]) / l2).clamp(0.0, 1.0) } else { 0.0 };
            let e = dist2(p, [w[0][0] + t * d[0], w[0][1] + t * d[1]]);
            if e < best.0 {
                best = (e, i as f64 + t);
            }
        }
        best.1
    };
    let mut c = c;
    c.cuts = vec![(pos(from), from), (pos(to), to)];
    // Cerrada: el tramo que empieza en `from`; abierta: el que va de uno a otro
    let parts = split_curve(&c, 0.0);
    Ok(parts
        .into_iter()
        .find(|(_, span)| span.is_some_and(|[a, b]| dist2(a, from) <= 1e-12 && dist2(b, to) <= 1e-12) || (!closed && span.is_some_and(|[a, _]| dist2(a, from) <= 1e-12)))
        .map(|(p, _)| p)
        .unwrap_or(pts))
}

/// Podar ramas sueltas: se sacan las aristas de los nodos con una sola, y los
/// nodos que quedan así siguen en la cola.
fn prune(edges: &[Edge], n_nodes: usize, alive: &mut [bool]) {
    let mut degree = vec![0usize; n_nodes];
    let mut touching: Vec<Vec<usize>> = vec![Vec::new(); n_nodes];
    for (i, e) in edges.iter().enumerate().filter(|(i, _)| alive[*i]) {
        for n in e.nodes {
            degree[n] += 1;
            touching[n].push(i);
        }
    }
    let mut queue: Vec<usize> = (0..n_nodes).filter(|&n| degree[n] < 2).collect();
    while let Some(n) = queue.pop() {
        for &i in &touching[n] {
            if !alive[i] {
                continue;
            }
            alive[i] = false;
            for m in edges[i].nodes {
                degree[m] -= 1;
                if degree[m] == 1 {
                    queue.push(m);
                }
            }
        }
    }
}

/// Todas las regiones cerradas del sketch (sin geometría de construcción).
pub fn find_regions(s: &Sketch) -> Result<Vec<Region>, SketchError> {
    let mut loops: Vec<Loop> = Vec::new();

    let index: HashMap<u32, P2> = s.points.iter().map(|p| (p.id, [p.x, p.y])).collect();
    let point = |id: u32| index.get(&id).copied().ok_or(SketchError::NoPoint(id));
    let mut curves: Vec<Cuttable> = Vec::new();
    for e in s.entities.iter().filter(|e| !e.construction && !matches!(e.geometry, Geometry::Point { .. })) {
        // Una curva que no se puede muestrear (le falta un punto…) no forma regiones, pero no tumba las demás
        let Ok(pts) = sample_geometry(&e.geometry, &point) else { continue };
        // También una abierta que termina donde empieza (una spline cerrada pasada a polos)
        let closed = e.geometry.is_closed() || e.geometry.ends().is_some_and(|(a, b)| a == b);
        if pts.len() < 2 || pts.iter().any(|p| !p[0].is_finite() || !p[1].is_finite()) {
            continue;
        }
        let eval = curve_eval(&e.geometry, &point, pts.len());
        curves.push(Cuttable { entity: e.id, pts, closed, exact: exact(&e.geometry, &point), eval, cuts: Vec::new() });
    }

    // Escala para tolerancias: lo que forma regiones (una construcción lejos
    // no puede agrandarla y fusionar nodos chicos)
    let span = curves.iter().flat_map(|c| c.pts.iter()).fold(0.0f64, |m, p| m.max(p[0].abs()).max(p[1].abs())).max(1.0);
    let tol = span * 1e-9;
    // Radio de fusión de nodos; un lazo con menos área que esto por su
    // perímetro no tiene adentro (círculo de radio casi 0, elipse aplastada)
    let merge = tol * 100.0;
    let flat = |poly: &[P2], area: f64| {
        let perimeter: f64 = (0..poly.len()).map(|i| dist2(poly[i], poly[(i + 1) % poly.len()])).sum();
        area <= merge * perimeter
    };

    // Nodos en una rejilla de celdas del tamaño del radio de fusión: se busca
    // en la celda y sus vecinas (con miles de extremos, la lista era cuadrática)
    let mut nodes: Vec<P2> = Vec::new();
    let mut cells: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    let cell = |p: P2| ((p[0] / merge).floor() as i64, (p[1] / merge).floor() as i64);
    let mut node_of = |p: P2| -> usize {
        let (cx, cy) = cell(p);
        let mut best: Option<usize> = None;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for &i in cells.get(&(cx + dx, cy + dy)).map(Vec::as_slice).unwrap_or(&[]) {
                    // El primero que se creó, como antes
                    if dist2(nodes[i], p) <= merge && best.is_none_or(|b| i < b) {
                        best = Some(i);
                    }
                }
            }
        }
        if let Some(i) = best {
            return i;
        }
        nodes.push(p);
        cells.entry((cx, cy)).or_default().push(nodes.len() - 1);
        nodes.len() - 1
    };
    let mut edges: Vec<Edge> = Vec::new();

    cut_crossings(&mut curves, tol);

    for c in &curves {
        for (pts, span) in split_curve(c, tol) {
            if span.is_none() && c.closed {
                let mut poly = pts.clone();
                poly.pop();
                let a = signed_area(&poly);
                if flat(&poly, a.abs()) {
                    continue;
                }
                let reversed = a < 0.0;
                if reversed {
                    poly.reverse();
                }
                loops.push(Loop { pieces: vec![LoopPiece { entity: c.entity, reversed, span: None }], area: a.abs(), polygon: poly });
                continue;
            }
            let (a, b) = (node_of(pts[0]), node_of(*pts.last().unwrap()));
            // Que vuelve a donde empezó: un lazo de una sola arista (el tramo de
            // una curva que se cruza a sí misma, un arco de vuelta entera); sin
            // largo, nada
            let length: f64 = pts.windows(2).map(|w| dist2(w[0], w[1])).sum();
            if a == b && (pts.len() < 3 || length <= merge * 4.0) {
                continue;
            }
            edges.push(Edge { entity: c.entity, nodes: [a, b], pts, span });
        }
    }

    // Tramos encimados (una línea sobre otra): queda uno solo, si no las caras
    // no se arman (dos aristas iguales salen con el mismo ángulo de un nodo)
    // (las candidatas a gemela son las que unen los mismos dos nodos)
    let mut kept: Vec<Edge> = Vec::new();
    let mut by_ends: HashMap<(usize, usize), Vec<(usize, P2)>> = HashMap::new();
    for e in edges {
        let key = (e.nodes[0].min(e.nodes[1]), e.nodes[0].max(e.nodes[1]));
        let mid = half_way(&e.pts);
        let same = by_ends.entry(key).or_default();
        if !same.iter().any(|&(_, m)| dist2(m, mid) <= tol * 1000.0) {
            same.push((kept.len(), mid));
            kept.push(e);
        }
    }
    let edges = kept;

    let half_pts = |h: Half| -> Vec<P2> {
        let mut p = edges[h.0].pts.clone();
        if h.1 {
            p.reverse();
        }
        p
    };
    let to_node = |h: Half| if h.1 { edges[h.0].nodes[0] } else { edges[h.0].nodes[1] };
    let mut alive = vec![true; edges.len()];
    // Caras; si alguna pasa dos veces por la misma arista, esa arista es un
    // puente (una línea del contorno a una isla, o entre dos contornos): se
    // saca y se vuelve a armar. Con el puente, el contorno y la isla salían
    // como una sola cara en forma de cerradura y la isla quedaba sin ser agujero.
    let faces: Vec<Vec<Half>> = loop {
        prune(&edges, nodes.len(), &mut alive);
        // Medias aristas: (arista, sentido). Ángulo de salida medido hacia un punto
        // cercano sobre la curva (desempata curvas tangentes mejor que la tangente).
        let mut outgoing: HashMap<usize, Vec<(Half, f64)>> = HashMap::new();
        for (i, e) in edges.iter().enumerate() {
            if !alive[i] {
                continue;
            }
            for rev in [false, true] {
                let p = half_pts((i, rev));
                let from = if rev { e.nodes[1] } else { e.nodes[0] };
                let k = (p.len() / 8).clamp(1, p.len() - 1);
                let ang = (p[k][1] - p[0][1]).atan2(p[k][0] - p[0][0]);
                outgoing.entry(from).or_default().push(((i, rev), ang));
            }
        }
        for list in outgoing.values_mut() {
            list.sort_by(|a, b| a.1.total_cmp(&b.1));
        }
        let mut used: HashMap<Half, usize> = HashMap::new();
        let mut faces: Vec<Vec<Half>> = Vec::new();
        for i in (0..edges.len()).filter(|&i| alive[i]) {
            for rev in [false, true] {
                let start = (i, rev);
                if used.contains_key(&start) {
                    continue;
                }
                let mut face = Vec::new();
                let mut h = start;
                let mut broken = false;
                loop {
                    used.insert(h, faces.len());
                    face.push(h);
                    // Siguiente: en el nodo destino, la anterior (en orden antihorario)
                    // a la media arista de vuelta: deja la cara a la izquierda.
                    let v = to_node(h);
                    let (Some(list), twin) = (outgoing.get(&v), (h.0, !h.1)) else {
                        broken = true;
                        break;
                    };
                    // Grafo inconsistente (aristas casi encimadas): esta cara se salta
                    let Some(pos) = list.iter().position(|x| x.0 == twin) else {
                        broken = true;
                        break;
                    };
                    h = list[(pos + list.len() - 1) % list.len()].0;
                    if h == start || face.len() > edges.len() * 2 {
                        break;
                    }
                }
                // Rota: igual queda anotada (sus medias aristas no se recorren de nuevo)
                faces.push(if broken { Vec::new() } else { face });
            }
        }
        let bridges: Vec<usize> = (0..edges.len())
            .filter(|&i| alive[i] && used.get(&(i, false)).is_some_and(|&f| !faces[f].is_empty() && used.get(&(i, true)) == Some(&f)))
            .collect();
        if bridges.is_empty() {
            break faces;
        }
        for i in bridges {
            alive[i] = false;
        }
    };
    for face in faces.into_iter().filter(|f| !f.is_empty()) {
        let mut poly: Vec<P2> = Vec::new();
        for &h in &face {
            let p = half_pts(h);
            poly.extend_from_slice(&p[..p.len() - 1]);
        }
        let a = signed_area(&poly);
        if a > 0.0 && !flat(&poly, a) {
            let pieces = face.iter().map(|&(e, r)| LoopPiece { entity: edges[e].entity, reversed: r, span: edges[e].span }).collect();
            loops.push(Loop { pieces, polygon: poly, area: a });
        }
    }

    // Contención: el padre de un lazo es el lazo más chico que lo contiene.
    // El punto interior y la caja de cada uno, una vez; los candidatos, de
    // menor a mayor área: el primero que lo contiene es el padre
    let n = loops.len();
    let probes: Vec<P2> = loops.iter().map(|l| interior_point(&l.polygon, &[])).collect();
    let boxes: Vec<[f64; 4]> = loops
        .iter()
        .map(|l| l.polygon.iter().fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |b, q| [b[0].min(q[0]), b[1].min(q[1]), b[2].max(q[0]), b[3].max(q[1])]))
        .collect();
    let slack = tol * 10.0;
    let contains = |o: usize, i: usize| -> bool {
        let (outer, inner) = (&loops[o], &loops[i]);
        let (bo, bi) = (boxes[o], boxes[i]);
        inner.area < outer.area
            && bi[0] >= bo[0] - slack
            && bi[1] >= bo[1] - slack
            && bi[2] <= bo[2] + slack
            && bi[3] <= bo[3] + slack
            && point_in_polygon(probes[i], &outer.polygon)
            && inner.polygon.iter().step_by((inner.polygon.len() / 8).max(1)).all(|p| point_in_polygon(*p, &outer.polygon) || on_ring(*p, &outer.polygon, slack))
    };
    let mut by_area: Vec<usize> = (0..n).collect();
    by_area.sort_by(|&a, &b| loops[a].area.total_cmp(&loops[b].area).then(a.cmp(&b)));
    let mut parent: Vec<Option<usize>> = vec![None; n];
    for (k, &i) in by_area.iter().enumerate() {
        parent[i] = by_area[k + 1..].iter().copied().find(|&j| contains(j, i));
    }
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    for j in 0..n {
        if let Some(p) = parent[j] {
            children[p].push(j);
        }
    }
    let depth = |mut i: usize| {
        let mut d = 0;
        while let Some(p) = parent[i] {
            d += 1;
            i = p;
        }
        d
    };
    let mut regions = Vec::new();
    for i in 0..n {
        let holes: Vec<Loop> = children[i].iter().map(|&j| loops[j].clone()).collect();
        let hole_polys: Vec<&[P2]> = holes.iter().map(|h| h.polygon.as_slice()).collect();
        let sample = interior_point(&loops[i].polygon, &hole_polys);
        regions.push(Region { outer: loops[i].clone(), holes, depth: depth(i), sample });
    }
    Ok(regions)
}

fn on_ring(p: P2, ring: &[P2], tol: f64) -> bool {
    let n = ring.len();
    (0..n).any(|i| {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len2 = dx * dx + dy * dy;
        let t = if len2 > 0.0 { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
        dist2(p, [a[0] + t * dx, a[1] + t * dy]) <= tol
    })
}
