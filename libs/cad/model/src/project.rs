//! Geometría del modelo llevada al plano de un sketch («Usar»): aristas del
//! sólido, la intersección del plano con el sólido, su silueta y entidades de
//! otro sketch, como curvas del sketch; y cómo seguirlas cuando el modelo cambia.

use serde::{Deserialize, Serialize};

use crate::geom::{P2, P3, Plane, add, cross, dist2, dot, normalize, scale, sub};
use crate::occt::{CurveKind, Shape};
use crate::sketch::{Geometry, Sketch};

/// Puntos de las splines que reemplazan a las curvas que no son líneas ni
/// arcos (elipses inclinadas, B-splines, arcos vistos de costado).
pub const SPLINE_POINTS: usize = 16;

/// Curva proyectada, en coordenadas del plano del sketch. Los arcos van
/// antihorario, como en el sketch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Projected {
    Point { at: P2 },
    Line { start: P2, end: P2 },
    Circle { center: P2, radius: f64 },
    Arc { center: P2, start: P2, end: P2 },
    /// `major` y `minor`: extremos de los semiejes.
    Ellipse { center: P2, major: P2, minor: P2 },
    Spline {
        points: Vec<P2>,
        closed: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        handles: Option<[P2; 2]>,
    },
}

type R<T> = Result<T, String>;

fn err(e: impl ToString) -> String {
    e.to_string()
}

fn line_or_point(a: P2, b: P2) -> Projected {
    if dist2(a, b) < 1e-9 { Projected::Point { at: a } } else { Projected::Line { start: a, end: b } }
}

/// Arco de `a` a `b` que pasa por `m`, dado vuelta si hace falta para que vaya antihorario.
fn arc_ccw(c: P2, a: P2, m: P2, b: P2) -> Projected {
    let ccw = (a[0] - c[0]) * (m[1] - c[1]) - (a[1] - c[1]) * (m[0] - c[0]) > 0.0;
    let (start, end) = if ccw { (a, b) } else { (b, a) };
    Projected::Arc { center: c, start, end }
}

/// Círculo completo (centro, eje, radio en 3D) visto en el plano: círculo si
/// está de frente, segmento si está de canto, elipse si no.
fn circle_on(c: P3, axis: P3, r: f64, plane: &Plane) -> Projected {
    let (ax, n) = (normalize(axis), normalize(plane.normal));
    let l = |p: P3| plane.to_local(p);
    let cos = dot(ax, n).abs();
    if cos > 1.0 - 1e-9 {
        return Projected::Circle { center: l(c), radius: r };
    }
    // Semieje mayor: la dirección del círculo paralela al plano (no se acorta)
    let u = normalize(cross(ax, n));
    if cos < 1e-9 {
        return Projected::Line { start: l(add(c, scale(u, -r))), end: l(add(c, scale(u, r))) };
    }
    let w = cross(ax, u);
    Projected::Ellipse { center: l(c), major: l(add(c, scale(u, r))), minor: l(add(c, scale(w, r))) }
}

fn fractions(n: usize, closed: bool, from: f64, to: f64) -> Vec<f64> {
    let steps = if closed { n } else { n - 1 } as f64;
    (0..n).map(|j| from + (to - from) * j as f64 / steps).collect()
}

/// La arista (o un tramo, en fracciones de su parámetro) como spline por puntos.
fn sampled(shape: &Shape, index: usize, plane: &Plane, closed: bool, from: f64, to: f64) -> R<Projected> {
    let pts: Vec<P2> = shape.edge_points(index, &fractions(SPLINE_POINTS, closed, from, to)).map_err(err)?.into_iter().map(|p| plane.to_local(p)).collect();
    let span = pts.iter().map(|p| dist2(*p, pts[0])).fold(0.0, f64::max);
    if span < 1e-9 {
        return Ok(Projected::Point { at: pts[0] });
    }
    // Vista de canto (un círculo o una curva plana perpendicular al plano): un segmento
    let far = pts.iter().copied().max_by(|a, b| dist2(*a, pts[0]).total_cmp(&dist2(*b, pts[0]))).unwrap();
    let u = [(far[0] - pts[0][0]) / dist2(far, pts[0]), (far[1] - pts[0][1]) / dist2(far, pts[0])];
    let along = |p: &P2| (p[0] - pts[0][0]) * u[0] + (p[1] - pts[0][1]) * u[1];
    let off = |p: &P2| ((p[0] - pts[0][0]) * u[1] - (p[1] - pts[0][1]) * u[0]).abs();
    if pts.iter().all(|p| off(p) < 1e-7 * span.max(1.0)) {
        let (lo, hi) = pts.iter().map(along).fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), t| (a.min(t), b.max(t)));
        let at = |t: f64| [pts[0][0] + t * u[0], pts[0][1] + t * u[1]];
        return Ok(Projected::Line { start: at(lo), end: at(hi) });
    }
    // Una curva que resulta ser un círculo (el contorno de una esfera, por ejemplo)
    let n = pts.len();
    if let Some(c) = circumcenter(pts[0], pts[n / 3], pts[2 * n / 3]) {
        let r = dist2(c, pts[0]);
        if r < 1e6 * span && pts.iter().all(|p| (dist2(c, *p) - r).abs() < 1e-7 * r.max(1.0)) {
            return Ok(if closed { Projected::Circle { center: c, radius: r } } else { arc_ccw(c, pts[0], pts[n / 2], pts[n - 1]) });
        }
    }
    Ok(Projected::Spline { points: pts, closed, handles: None })
}

fn circumcenter(a: P2, b: P2, c: P2) -> Option<P2> {
    let d = 2.0 * (a[0] * (b[1] - c[1]) + b[0] * (c[1] - a[1]) + c[0] * (a[1] - b[1]));
    if d.abs() < 1e-12 {
        return None;
    }
    let [a2, b2, c2] = [a, b, c].map(|p| p[0] * p[0] + p[1] * p[1]);
    Some([(a2 * (b[1] - c[1]) + b2 * (c[1] - a[1]) + c2 * (a[1] - b[1])) / d, (a2 * (c[0] - b[0]) + b2 * (a[0] - c[0]) + c2 * (b[0] - a[0])) / d])
}

/// Arista `index` de `shape` proyectada en el plano.
pub fn edge(shape: &Shape, index: usize, plane: &Plane) -> R<Projected> {
    let info = shape.edge_info(index).map_err(err)?;
    let l = |p: P3| plane.to_local(p);
    let n = normalize(plane.normal);
    match (info.curve, info.circle) {
        (CurveKind::Line, _) => Ok(line_or_point(l(info.start), l(info.end))),
        (CurveKind::Circle, Some((c, axis, r))) => {
            if info.closed {
                Ok(circle_on(c, axis, r, plane))
            } else if dot(normalize(axis), n).abs() > 1.0 - 1e-9 {
                Ok(arc_ccw(l(c), l(info.start), l(info.mid), l(info.end)))
            } else {
                sampled(shape, index, plane, false, 0.0, 1.0)
            }
        }
        (CurveKind::Other, _) if info.length < 1e-12 => Err("arista degenerada".into()),
        _ => sampled(shape, index, plane, info.closed, 0.0, 1.0),
    }
}

/// Tramo `[from, to]` (fracciones del parámetro) de una arista que está en el plano.
fn edge_piece(shape: &Shape, index: usize, plane: &Plane, from: f64, to: f64) -> R<Projected> {
    if from <= 1e-9 && to >= 1.0 - 1e-9 {
        return edge(shape, index, plane);
    }
    let info = shape.edge_info(index).map_err(err)?;
    let pts = shape.edge_points(index, &[from, (from + to) / 2.0, to]).map_err(err)?;
    let [a, m, b] = [0, 1, 2].map(|k| plane.to_local(pts[k]));
    match (info.curve, info.circle) {
        (CurveKind::Line, _) => Ok(line_or_point(a, b)),
        (CurveKind::Circle, Some((c, axis, _))) if dot(normalize(axis), normalize(plane.normal)).abs() > 1.0 - 1e-9 => Ok(arc_ccw(plane.to_local(c), a, m, b)),
        _ => sampled(shape, index, plane, false, from, to),
    }
}

/// Curvas donde el plano corta el sólido.
pub fn section(body: &Shape, plane: &Plane) -> R<Vec<Projected>> {
    let cut = body.section(plane.origin, normalize(plane.normal)).map_err(err)?;
    let curves = (0..cut.edge_count()).filter_map(|i| edge(&cut, i, plane).ok()).filter(|p| !matches!(p, Projected::Point { .. })).collect();
    Ok(dedup(curves))
}

/// Contorno del sólido visto desde la normal del plano: el borde de su
/// sombra (con los agujeros pasantes), proyectado al plano.
///
/// Las candidatas son las líneas visibles de la vista (aristas, contornos de
/// superficies curvas y aristas suaves); un tramo es del contorno si a un
/// lado la recta normal al plano cruza el sólido y al otro no.
pub fn silhouette(body: &Shape, plane: &Plane) -> R<Vec<Projected>> {
    const K: usize = 16;
    let n = normalize(plane.normal);
    let lines = body.outline(plane.origin, n, plane.x_dir).map_err(err)?;
    let edges: Vec<usize> = (0..lines.edge_count()).filter(|&i| lines.edge_info(i).is_ok_and(|e| e.length > 1e-9)).collect();
    if edges.is_empty() {
        return Ok(Vec::new());
    }
    // Tamaño de la vista, para el corrimiento a cada lado de la línea
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    for &i in &edges {
        let e = lines.edge_info(i).map_err(err)?;
        for p in [e.start, e.end, e.mid] {
            let q = plane.to_local(p);
            for k in 0..2 {
                lo[k] = lo[k].min(q[k]);
                hi[k] = hi[k].max(q[k]);
            }
        }
    }
    let size = dist2(lo, hi).max(1e-6);
    let eps = (size * 1e-4).max(1e-7);
    let world = |p: P2| plane.to_world(p);

    // ¿Es del contorno el punto en la fracción f de cada arista pedida?
    let classify = |asks: &[(usize, f64)]| -> R<Vec<bool>> {
        let mut origins = Vec::with_capacity(2 * asks.len());
        for &(i, f) in asks {
            let d = 1e-4;
            let (f0, f1) = if f + d <= 1.0 { (f, f + d) } else { (f - d, f) };
            let pts = lines.edge_points(i, &[f, f0, f1]).map_err(err)?;
            let p = plane.to_local(pts[0]);
            let t = sub(pts[2], pts[1]);
            let t = plane.to_local(add(plane.origin, normalize(t)));
            let side = [-t[1] * eps, t[0] * eps];
            origins.push(world([p[0] + side[0], p[1] + side[1]]));
            origins.push(world([p[0] - side[0], p[1] - side[1]]));
        }
        let hits = body.lines_hit(&origins, n).map_err(err)?;
        Ok(hits.chunks_exact(2).map(|h| h[0] != h[1]).collect())
    };

    let asks: Vec<(usize, f64)> = edges.iter().flat_map(|&i| (0..K).map(move |j| (i, (j as f64 + 0.5) / K as f64))).collect();
    let marks = classify(&asks)?;
    // Tramos seguidos del contorno; los bordes se afinan por bisección entre
    // la muestra que es y la vecina que no es
    struct Piece {
        edge: usize,
        from: f64,
        to: f64,
    }
    struct Edge {
        piece: usize,
        start: bool,
        inside: f64,
        outside: f64,
    }
    let mut pieces: Vec<Piece> = Vec::new();
    let mut refine: Vec<Edge> = Vec::new();
    for (k, &i) in edges.iter().enumerate() {
        let m = &marks[k * K..(k + 1) * K];
        let frac = |j: usize| (j as f64 + 0.5) / K as f64;
        let mut j = 0;
        while j < K {
            if !m[j] {
                j += 1;
                continue;
            }
            let first = j;
            while j < K && m[j] {
                j += 1;
            }
            let last = j - 1;
            let p = pieces.len();
            pieces.push(Piece { edge: i, from: if first == 0 { 0.0 } else { frac(first) }, to: if last == K - 1 { 1.0 } else { frac(last) } });
            if first > 0 {
                refine.push(Edge { piece: p, start: true, inside: frac(first), outside: frac(first - 1) });
            }
            if last < K - 1 {
                refine.push(Edge { piece: p, start: false, inside: frac(last), outside: frac(last + 1) });
            }
        }
    }
    for _ in 0..24 {
        if refine.is_empty() {
            break;
        }
        let asks: Vec<(usize, f64)> = refine.iter().map(|r| (pieces[r.piece].edge, (r.inside + r.outside) / 2.0)).collect();
        let marks = classify(&asks)?;
        for (r, (m, (_, f))) in refine.iter_mut().zip(marks.into_iter().zip(asks)) {
            if m { r.inside = f } else { r.outside = f }
        }
    }
    for r in &refine {
        let f = (r.inside + r.outside) / 2.0;
        if r.start { pieces[r.piece].from = f } else { pieces[r.piece].to = f }
    }
    // Un círculo cerrado con el contorno cruzando el comienzo: un solo arco
    let mut out = Vec::new();
    let mut k = 0;
    while k < pieces.len() {
        let p = &pieces[k];
        let info = lines.edge_info(p.edge).map_err(err)?;
        let last_of_edge = pieces[k..].iter().rposition(|q| q.edge == p.edge).map(|r| k + r).unwrap_or(k);
        if info.closed && info.curve == CurveKind::Circle && p.from <= 1e-9 && last_of_edge > k && pieces[last_of_edge].to >= 1.0 - 1e-9 {
            let q = &pieces[last_of_edge];
            let pts = lines.edge_points(p.edge, &[q.from, (q.from + p.to + 1.0) / 2.0 % 1.0, p.to]).map_err(err)?;
            let c = info.circle.map(|(c, ..)| plane.to_local(c)).unwrap_or_default();
            let [a, m, b] = [0, 1, 2].map(|j| plane.to_local(pts[j]));
            out.push(arc_ccw(c, a, m, b));
            for q in &pieces[k + 1..last_of_edge] {
                out.push(edge_piece(&lines, q.edge, plane, q.from, q.to)?);
            }
            k = last_of_edge + 1;
            continue;
        }
        out.push(edge_piece(&lines, p.edge, plane, p.from, p.to)?);
        k += 1;
    }
    // Tramos de nada (muestras sueltas que rozaron una cara)
    out.retain(|p| length(p) > 10.0 * eps);
    Ok(dedup(out))
}

fn length(p: &Projected) -> f64 {
    match p {
        Projected::Point { .. } => 0.0,
        Projected::Line { start, end } => dist2(*start, *end),
        Projected::Circle { radius, .. } => 2.0 * std::f64::consts::PI * radius,
        Projected::Arc { center, start, end } => {
            let r = dist2(*center, *start);
            let a = (start[1] - center[1]).atan2(start[0] - center[0]);
            let b = (end[1] - center[1]).atan2(end[0] - center[0]);
            let mut sweep = b - a;
            if sweep <= 0.0 {
                sweep += 2.0 * std::f64::consts::PI;
            }
            r * sweep
        }
        Projected::Ellipse { center, major, .. } => 2.0 * std::f64::consts::PI * dist2(*center, *major),
        Projected::Spline { points, .. } => points.windows(2).map(|w| dist2(w[0], w[1])).sum(),
    }
}

/// Sin repetidas (dos aristas que se ven en el mismo lugar).
fn dedup(curves: Vec<Projected>) -> Vec<Projected> {
    let mut out: Vec<Projected> = Vec::new();
    for c in curves {
        if !out.iter().any(|o| cost(o, &c).is_some_and(|d| d < 1e-6)) {
            out.push(c);
        }
    }
    out
}

/// Entidad `id` de un sketch como curva de su plano.
pub fn of_entity(sketch: &Sketch, id: u32) -> Option<Projected> {
    let e = sketch.entity(id).ok()?;
    let p = |q: u32| sketch.point(q).ok();
    Some(match &e.geometry {
        Geometry::Point { point } => Projected::Point { at: p(*point)? },
        Geometry::Line { start, end } => Projected::Line { start: p(*start)?, end: p(*end)? },
        Geometry::Circle { center, radius } => Projected::Circle { center: p(*center)?, radius: *radius },
        Geometry::Arc { center, start, end } => Projected::Arc { center: p(*center)?, start: p(*start)?, end: p(*end)? },
        Geometry::Ellipse { center, major, minor } => Projected::Ellipse { center: p(*center)?, major: p(*major)?, minor: p(*minor)? },
        // Sin fórmula propia en el otro plano: por muestras
        Geometry::EllipseArc { .. } | Geometry::BSpline { .. } => {
            let mut pts = crate::regions::sample_entity(sketch, id).ok()?;
            let closed = e.geometry.is_closed();
            if closed {
                pts.pop();
            }
            Projected::Spline { points: resample(&pts, SPLINE_POINTS, closed), closed, handles: None }
        }
        Geometry::Spline { points, closed, start_handle, end_handle, .. } => Projected::Spline {
            points: points.iter().map(|q| p(*q)).collect::<Option<_>>()?,
            closed: *closed,
            handles: match (start_handle, end_handle) {
                (Some(a), Some(b)) => Some([p(*a)?, p(*b)?]),
                _ => None,
            },
        },
    })
}

/// Entidad `id` de otro sketch (en `from`) vista en `plane`: igual si los
/// planos son paralelos; si no, los círculos pasan a elipses y los arcos y
/// elipses a splines.
pub fn sketch_entity(sketch: &Sketch, from: &Plane, id: u32, plane: &Plane) -> R<Projected> {
    let src = of_entity(sketch, id).ok_or("esa entidad del otro sketch ya no existe")?;
    let map = |p: P2| plane.to_local(from.to_world(p));
    let c = dot(normalize(from.normal), normalize(plane.normal));
    let parallel = c.abs() > 1.0 - 1e-9;
    let around = |center: P2, a: f64, b: f64, r: (f64, f64), dirs: (P2, P2), closed: bool| -> Projected {
        // Muestras de un arco de elipse en el plano de origen
        let points = fractions(SPLINE_POINTS, closed, a, b)
            .into_iter()
            .map(|t| {
                let (ct, st) = (t.cos(), t.sin());
                map([center[0] + r.0 * ct * dirs.0[0] + r.1 * st * dirs.1[0], center[1] + r.0 * ct * dirs.0[1] + r.1 * st * dirs.1[1]])
            })
            .collect();
        Projected::Spline { points, closed, handles: None }
    };
    Ok(match src {
        Projected::Point { at } => Projected::Point { at: map(at) },
        Projected::Line { start, end } => line_or_point(map(start), map(end)),
        Projected::Circle { center, radius } if parallel => Projected::Circle { center: map(center), radius },
        Projected::Circle { center, radius } => circle_on(from.to_world(center), from.normal, radius, plane),
        Projected::Arc { center, start, end } if parallel => {
            let (a, b) = (map(start), map(end));
            // Visto desde el otro lado, el antihorario se invierte
            if c > 0.0 { Projected::Arc { center: map(center), start: a, end: b } } else { Projected::Arc { center: map(center), start: b, end: a } }
        }
        Projected::Arc { center, start, end } => {
            let r = dist2(center, start);
            let a = (start[1] - center[1]).atan2(start[0] - center[0]);
            let mut b = (end[1] - center[1]).atan2(end[0] - center[0]);
            if b <= a {
                b += 2.0 * std::f64::consts::PI;
            }
            around(center, a, b, (r, r), ([1.0, 0.0], [0.0, 1.0]), false)
        }
        Projected::Ellipse { center, major, minor } if parallel => Projected::Ellipse { center: map(center), major: map(major), minor: map(minor) },
        Projected::Ellipse { center, major, minor } => {
            let (ra, rb) = (dist2(center, major), dist2(center, minor));
            let u = [(major[0] - center[0]) / ra, (major[1] - center[1]) / ra];
            around(center, 0.0, 2.0 * std::f64::consts::PI, (ra, rb), (u, [-u[1], u[0]]), true)
        }
        Projected::Spline { points, closed, handles } => Projected::Spline {
            points: points.into_iter().map(map).collect(),
            closed,
            handles: handles.map(|[a, b]| [map(a), map(b)]),
        },
    })
}

/// Qué tan lejos están dos curvas del mismo tipo (`None` si no se pueden comparar).
pub fn cost(a: &Projected, b: &Projected) -> Option<f64> {
    use Projected as P;
    let d = dist2;
    Some(match (a, b) {
        (P::Point { at: p }, P::Point { at: q }) => d(*p, *q),
        (P::Line { start: a1, end: a2 }, P::Line { start: b1, end: b2 }) => (d(*a1, *b1) + d(*a2, *b2)).min(d(*a1, *b2) + d(*a2, *b1)),
        (P::Circle { center: c1, radius: r1 }, P::Circle { center: c2, radius: r2 }) => d(*c1, *c2) + (r1 - r2).abs(),
        (P::Arc { center: c1, start: s1, end: e1 }, P::Arc { center: c2, start: s2, end: e2 }) => d(*c1, *c2) + d(*s1, *s2) + d(*e1, *e2),
        (P::Ellipse { center: c1, major: a1, minor: m1 }, P::Ellipse { center: c2, major: a2, minor: m2 }) => {
            // El semieje puede salir para el otro lado
            let flip = |c: P2, p: P2| [2.0 * c[0] - p[0], 2.0 * c[1] - p[1]];
            d(*c1, *c2) + d(*a1, *a2).min(d(*a1, flip(*c2, *a2))) + d(*m1, *m2).min(d(*m1, flip(*c2, *m2)))
        }
        (P::Spline { points: p, closed: c1, .. }, P::Spline { points: q, closed: c2, .. }) if c1 == c2 && !p.is_empty() && !q.is_empty() => {
            let k = p.len().max(q.len()).max(2);
            let (p, q) = (resample(p, k, *c1), resample(q, k, *c2));
            let fwd: f64 = p.iter().zip(&q).map(|(x, y)| d(*x, *y)).sum();
            let back: f64 = p.iter().zip(q.iter().rev()).map(|(x, y)| d(*x, *y)).sum();
            fwd.min(back) / k as f64 * 3.0
        }
        _ => return None,
    })
}

/// `k` puntos repartidos por largo a lo largo de la polilínea.
fn resample(points: &[P2], k: usize, closed: bool) -> Vec<P2> {
    if points.len() == k || points.len() < 2 {
        return points.to_vec();
    }
    let mut pts = points.to_vec();
    if closed {
        pts.push(points[0]);
    }
    let cum: Vec<f64> = std::iter::once(0.0)
        .chain(pts.windows(2).scan(0.0, |acc, w| {
            *acc += dist2(w[0], w[1]);
            Some(*acc)
        }))
        .collect();
    let total = *cum.last().unwrap();
    if total < 1e-12 {
        return vec![points[0]; k];
    }
    let steps = if closed { k } else { k - 1 } as f64;
    (0..k)
        .map(|j| {
            let s = total * j as f64 / steps;
            let i = cum.partition_point(|&c| c <= s).clamp(1, pts.len() - 1);
            let t = ((s - cum[i - 1]) / (cum[i] - cum[i - 1]).max(1e-15)).clamp(0.0, 1.0);
            [pts[i - 1][0] + t * (pts[i][0] - pts[i - 1][0]), pts[i - 1][1] + t * (pts[i][1] - pts[i - 1][1])]
        })
        .collect()
}

/// Lleva la entidad `id` a la curva `to` (mismo tipo; si no, `false`). Las
/// líneas y splines conservan su sentido.
pub fn fit(sketch: &mut Sketch, id: u32, to: &Projected) -> bool {
    use Projected as P;
    let Ok(e) = sketch.entity(id) else { return false };
    let set = |s: &mut Sketch, q: u32, p: P2| s.set_point(q, p).is_ok();
    match (e.geometry.clone(), to) {
        (Geometry::Point { point }, P::Point { at }) => set(sketch, point, *at),
        (Geometry::Line { start, end }, P::Line { start: a, end: b }) => {
            let (Ok(s0), Ok(e0)) = (sketch.point(start), sketch.point(end)) else { return false };
            let (a, b) = if dist2(s0, *a) + dist2(e0, *b) <= dist2(s0, *b) + dist2(e0, *a) { (*a, *b) } else { (*b, *a) };
            set(sketch, start, a) && set(sketch, end, b)
        }
        (Geometry::Circle { center, .. }, P::Circle { center: c, radius }) => {
            if let Some(Geometry::Circle { radius: r, .. }) = sketch.entities.iter_mut().find(|x| x.id == id).map(|x| &mut x.geometry) {
                *r = *radius;
            }
            set(sketch, center, *c)
        }
        (Geometry::Arc { center, start, end }, P::Arc { center: c, start: a, end: b }) => set(sketch, center, *c) && set(sketch, start, *a) && set(sketch, end, *b),
        (Geometry::Ellipse { center, major, minor }, P::Ellipse { center: c, major: a, minor: b }) => {
            // Del mismo lado que estaban (la elipse es igual con el semieje dado vuelta)
            let near = |s: &Sketch, q: u32, p: P2| match s.point(q) {
                Ok(o) if dist2(o, p) > dist2(o, [2.0 * c[0] - p[0], 2.0 * c[1] - p[1]]) => [2.0 * c[0] - p[0], 2.0 * c[1] - p[1]],
                _ => p,
            };
            let (a, b) = (near(sketch, major, *a), near(sketch, minor, *b));
            set(sketch, center, *c) && set(sketch, major, a) && set(sketch, minor, b)
        }
        (Geometry::Spline { points, closed, start_handle, end_handle, .. }, P::Spline { points: q, closed: c, handles }) if closed == *c => {
            let mut q = resample(q, points.len(), closed);
            let mut handles = *handles;
            let now: Vec<P2> = points.iter().filter_map(|p| sketch.point(*p).ok()).collect();
            if now.len() == q.len() && !closed {
                let fwd: f64 = now.iter().zip(&q).map(|(x, y)| dist2(*x, *y)).sum();
                let back: f64 = now.iter().zip(q.iter().rev()).map(|(x, y)| dist2(*x, *y)).sum();
                if back < fwd {
                    q.reverse();
                    handles = handles.map(|[a, b]| [b, a]);
                }
            }
            let mut ok = points.iter().zip(&q).all(|(p, at)| set(sketch, *p, *at));
            if let (Some(h1), Some(h2), Some([a, b])) = (start_handle, end_handle, handles) {
                ok &= set(sketch, h1, a) && set(sketch, h2, b);
            }
            ok
        }
        _ => false,
    }
}

/// Reparte `curves` entre las entidades `entities` (cada una a la más
/// parecida de su tipo): pares (entidad, curva).
pub fn assign(sketch: &Sketch, entities: &[u32], curves: &[Projected]) -> Vec<(u32, usize)> {
    let mut pairs: Vec<(f64, u32, usize)> = Vec::new();
    for &e in entities {
        let Some(now) = of_entity(sketch, e) else { continue };
        for (k, c) in curves.iter().enumerate() {
            if let Some(d) = cost(&now, c) {
                pairs.push((d, e, k));
            }
        }
    }
    pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
    let (mut used_e, mut used_c) = (Vec::new(), vec![false; curves.len()]);
    let mut out = Vec::new();
    for (_, e, k) in pairs {
        if used_e.contains(&e) || used_c[k] {
            continue;
        }
        used_e.push(e);
        used_c[k] = true;
        out.push((e, k));
    }
    out
}

/// Agrega la curva al sketch como entidad nueva (puntos propios).
pub fn add_to(sketch: &mut Sketch, p: &Projected) -> u32 {
    let pt = |s: &mut Sketch, q: P2| s.add_point(q[0], q[1]);
    let g = match p {
        Projected::Point { at } => Geometry::Point { point: pt(sketch, *at) },
        Projected::Line { start, end } => Geometry::Line { start: pt(sketch, *start), end: pt(sketch, *end) },
        Projected::Circle { center, radius } => Geometry::Circle { center: pt(sketch, *center), radius: *radius },
        Projected::Arc { center, start, end } => Geometry::Arc { center: pt(sketch, *center), start: pt(sketch, *start), end: pt(sketch, *end) },
        Projected::Ellipse { center, major, minor } => Geometry::Ellipse { center: pt(sketch, *center), major: pt(sketch, *major), minor: pt(sketch, *minor) },
        Projected::Spline { points, closed, handles } => Geometry::Spline {
            points: points.iter().map(|q| pt(sketch, *q)).collect(),
            closed: *closed,
            start_handle: handles.map(|h| pt(sketch, h[0])),
            end_handle: handles.map(|h| pt(sketch, h[1])),
            handles: vec![],
        },
    };
    sketch.add_entity(g)
}
