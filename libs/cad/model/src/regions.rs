//! Regiones cerradas de un sketch (lo que se puede extruir).
//!
//! Las líneas, arcos y splines abiertas forman un grafo plano: cada cara acotada
//! del grafo es un lazo. Círculos y splines cerradas son lazos sueltos. Un lazo
//! dentro de otro es su agujero; el de adentro también es región propia.
//!
//! Supone que las curvas no se cruzan (se tocan en sus extremos), que es lo que
//! dejan las herramientas de dibujo. Las ramas sueltas se ignoran.

use std::collections::HashMap;
use std::f64::consts::PI;

use serde::{Deserialize, Serialize};

use crate::geom::{P2, dist2};
use crate::sketch::{Geometry, Sketch, SketchError};

/// Un tramo de lazo: entidad recorrida en su sentido o al revés.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoopPiece {
    pub entity: u32,
    pub reversed: bool,
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
    let e = s.entity(id)?;
    Ok(match &e.geometry {
        Geometry::Line { start, end } => vec![s.point(*start)?, s.point(*end)?],
        Geometry::Circle { center, radius } => {
            let c = s.point(*center)?;
            (0..=64).map(|i| {
                let a = i as f64 / 64.0 * 2.0 * PI;
                [c[0] + radius * a.cos(), c[1] + radius * a.sin()]
            }).collect()
        }
        Geometry::Arc { center, start, end } => {
            let (c, a, b) = (s.point(*center)?, s.point(*start)?, s.point(*end)?);
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
        Geometry::Spline { points, closed } => {
            let mut pts: Vec<P2> = points.iter().map(|p| s.point(*p)).collect::<Result<_, _>>()?;
            if *closed && let Some(&f) = pts.first() {
                pts.push(f);
            }
            catmull_rom(&pts)
        }
        Geometry::Point { point } => vec![s.point(*point)?],
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
    let closed = dist2(p[0], p[p.len() - 1]) < 1e-12;
    let n = p.len();
    let get = |i: isize| -> P2 {
        if closed {
            let m = (n - 1) as isize;
            p[i.rem_euclid(m) as usize]
        } else {
            p[i.clamp(0, n as isize - 1) as usize]
        }
    };
    let mut out = Vec::new();
    for i in 0..n - 1 {
        let (p0, p1, p2, p3) = (get(i as isize - 1), get(i as isize), get(i as isize + 1), get(i as isize + 2));
        for k in 0..8 {
            let t = k as f64 / 8.0;
            let (t2, t3) = (t * t, t * t * t);
            let f = |a: f64, b: f64, c: f64, d: f64| {
                0.5 * (2.0 * b + (-a + c) * t + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2 + (-a + 3.0 * b - 3.0 * c + d) * t3)
            };
            out.push([f(p0[0], p1[0], p2[0], p3[0]), f(p0[1], p1[1], p2[1], p3[1])]);
        }
    }
    out.push(p[n - 1]);
    out
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
}

/// Todas las regiones cerradas del sketch (sin geometría de construcción).
pub fn find_regions(s: &Sketch) -> Result<Vec<Region>, SketchError> {
    let mut loops: Vec<Loop> = Vec::new();

    // Escala para tolerancias
    let span = s.points.iter().fold(0.0f64, |m, p| m.max(p.x.abs()).max(p.y.abs())).max(1.0);
    let tol = span * 1e-9;

    let mut nodes: Vec<P2> = Vec::new();
    let mut node_of = |p: P2| -> usize {
        if let Some(i) = nodes.iter().position(|q| dist2(*q, p) <= tol) {
            return i;
        }
        nodes.push(p);
        nodes.len() - 1
    };
    let mut edges: Vec<Edge> = Vec::new();

    for e in s.entities.iter().filter(|e| !e.construction && !matches!(e.geometry, Geometry::Point { .. })) {
        let pts = sample_entity(s, e.id)?;
        let closed_alone = matches!(e.geometry, Geometry::Circle { .. } | Geometry::Spline { closed: true, .. });
        if closed_alone {
            let mut poly = pts.clone();
            poly.pop();
            let a = signed_area(&poly);
            let reversed = a < 0.0;
            if reversed {
                poly.reverse();
            }
            loops.push(Loop { pieces: vec![LoopPiece { entity: e.id, reversed }], area: a.abs(), polygon: poly });
            continue;
        }
        let (a, b) = (node_of(pts[0]), node_of(*pts.last().unwrap()));
        if a == b {
            continue; // curva abierta que vuelve sobre sí misma: no soportada
        }
        edges.push(Edge { entity: e.id, nodes: [a, b], pts });
    }

    // Podar ramas sueltas
    let mut alive = vec![true; edges.len()];
    loop {
        let mut degree = vec![0usize; nodes.len()];
        for (i, e) in edges.iter().enumerate() {
            if alive[i] {
                degree[e.nodes[0]] += 1;
                degree[e.nodes[1]] += 1;
            }
        }
        let mut changed = false;
        for (i, e) in edges.iter().enumerate() {
            if alive[i] && (degree[e.nodes[0]] < 2 || degree[e.nodes[1]] < 2) {
                alive[i] = false;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    // Medias aristas: (arista, sentido). Ángulo de salida medido hacia un punto
    // cercano sobre la curva (desempata curvas tangentes mejor que la tangente).
    let half_pts = |h: Half| -> Vec<P2> {
        let mut p = edges[h.0].pts.clone();
        if h.1 {
            p.reverse();
        }
        p
    };
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
    let to_node = |h: Half| if h.1 { edges[h.0].nodes[0] } else { edges[h.0].nodes[1] };

    let mut used: HashMap<Half, bool> = HashMap::new();
    for (i, _) in edges.iter().enumerate().filter(|(i, _)| alive[*i]) {
        for rev in [false, true] {
            let start = (i, rev);
            if used.contains_key(&start) {
                continue;
            }
            let mut face = Vec::new();
            let mut h = start;
            loop {
                used.insert(h, true);
                face.push(h);
                // Siguiente: en el nodo destino, la anterior (en orden antihorario)
                // a la media arista de vuelta: deja la cara a la izquierda.
                let v = to_node(h);
                let list = &outgoing[&v];
                let twin = (h.0, !h.1);
                let pos = list.iter().position(|x| x.0 == twin).unwrap();
                h = list[(pos + list.len() - 1) % list.len()].0;
                if h == start || face.len() > edges.len() * 2 {
                    break;
                }
            }
            let mut poly: Vec<P2> = Vec::new();
            for &h in &face {
                let p = half_pts(h);
                poly.extend_from_slice(&p[..p.len() - 1]);
            }
            let a = signed_area(&poly);
            if a > tol {
                let pieces = face.iter().map(|&(e, r)| LoopPiece { entity: edges[e].entity, reversed: r }).collect();
                loops.push(Loop { pieces, polygon: poly, area: a });
            }
        }
    }

    // Contención: el padre de un lazo es el lazo más chico que lo contiene.
    let probe = |l: &Loop| interior_point(&l.polygon, &[]);
    let contains = |outer: &Loop, inner: &Loop| -> bool {
        inner.area < outer.area && point_in_polygon(probe(inner), &outer.polygon)
            && inner.polygon.iter().step_by((inner.polygon.len() / 8).max(1)).all(|p| point_in_polygon(*p, &outer.polygon) || on_ring(*p, &outer.polygon, tol * 10.0))
    };
    let n = loops.len();
    let mut parent: Vec<Option<usize>> = vec![None; n];
    for i in 0..n {
        for j in 0..n {
            if i != j && contains(&loops[j], &loops[i]) && parent[i].is_none_or(|p| loops[j].area < loops[p].area) {
                parent[i] = Some(j);
            }
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
        let holes: Vec<Loop> = (0..n).filter(|&j| parent[j] == Some(i)).map(|j| loops[j].clone()).collect();
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
