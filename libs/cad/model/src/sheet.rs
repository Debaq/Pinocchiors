//! Desarrollo de chapa metálica: la chapa plana que, doblada, da la pieza.
//!
//! Sirve para cualquier sólido de espesor constante hecho de paredes planas y
//! dobleces cilíndricos (la chapa con sus pestañas, una extrusión delgada con
//! redondeos, un STEP de chapa). Se recorre un lado de la chapa desde una
//! cara plana fija pasando solo por aristas tangentes (pared → doblez →
//! pared): así nunca se cruza al canto ni al otro lado. Cada pared se lleva
//! al plano con un movimiento rígido y cada doblez se estira a su largo
//! desarrollado, θ·(R + K·t) (fibra neutra). Lo que queda en el borde de ese
//! lado (contorno, agujeros, recortes) es el contorno del desarrollo.

use crate::geom::{P2, P3, cross, dot, norm, normalize, scale, sub};
use cad_occt::{CurveKind, FaceInfo, Shape, SurfaceKind};
use serde::Serialize;
use std::collections::VecDeque;

/// Línea de doblez en el desarrollo.
#[derive(Debug, Clone, Serialize)]
pub struct FlatBend {
    /// El medio de la zona de doblez, de punta a punta
    pub line: [P2; 2],
    /// Grados que dobla
    pub angle: f64,
    /// Radio interior
    pub radius: f64,
    /// Hacia el lado que se ve en el desarrollo (si no, hacia atrás)
    pub up: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct FlatPattern {
    /// Tramos del contorno y de los agujeros (en mm, en el plano)
    pub outline: Vec<Vec<P2>>,
    pub bends: Vec<FlatBend>,
    pub thickness: f64,
    pub k_factor: f64,
    pub min: P2,
    pub max: P2,
}

/// Cómo se lleva un punto de una pared al plano: `c + a2·((x−q0)·a3) + b2·((x−q0)·b3)`.
#[derive(Debug, Clone, Copy)]
struct PlaneMap {
    q0: P3,
    a3: P3,
    b3: P3,
    c: P2,
    a2: P2,
    b2: P2,
}

impl PlaneMap {
    fn lin(&self, v: P3) -> P2 {
        let (a, b) = (dot(v, self.a3), dot(v, self.b3));
        [self.a2[0] * a + self.b2[0] * b, self.a2[1] * a + self.b2[1] * b]
    }
    fn map(&self, x: P3) -> P2 {
        let l = self.lin(sub(x, self.q0));
        [self.c[0] + l[0], self.c[1] + l[1]]
    }
}

/// Un doblez estirado: el ángulo alrededor del eje se vuelve distancia.
#[derive(Debug, Clone, Copy)]
struct BendMap {
    /// Punto del borde con la pared de antes y su imagen
    s1: P3,
    base: P2,
    /// Eje del cilindro
    origin: P3,
    axis: P3,
    /// Dirección radial en `s1`
    r1: P3,
    sign: f64,
    theta: f64,
    length: f64,
    along: P2,
    across: P2,
}

impl BendMap {
    fn radial(&self, x: P3) -> P3 {
        let v = sub(x, self.origin);
        normalize(sub(v, scale(self.axis, dot(v, self.axis))))
    }
    fn map(&self, x: P3) -> P2 {
        let rx = self.radial(x);
        let phi = self.sign * dot(cross(self.r1, rx), self.axis).atan2(dot(self.r1, rx));
        let s = (phi / self.theta).clamp(0.0, 1.0) * self.length;
        let a = dot(sub(x, self.s1), self.axis);
        [self.base[0] + self.along[0] * a + self.across[0] * s, self.base[1] + self.along[1] * a + self.across[1] * s]
    }
}

enum FaceMap {
    Plane(PlaneMap),
    Bend(BendMap),
}

impl FaceMap {
    fn map(&self, x: P3) -> P2 {
        match self {
            FaceMap::Plane(m) => m.map(x),
            FaceMap::Bend(m) => m.map(x),
        }
    }
}

fn unit2(v: P2) -> P2 {
    let l = (v[0] * v[0] + v[1] * v[1]).sqrt();
    [v[0] / l, v[1] / l]
}

/// Lado del borde `e` hacia donde está `inside`: ±(n × u), en el plano de normal `n`.
fn toward(n: P3, u: P3, from: P3, inside: P3) -> P3 {
    let w = normalize(cross(n, u));
    if dot(w, sub(inside, from)) >= 0.0 { w } else { scale(w, -1.0) }
}

/// Desarrollo de `shape` (una pieza). `fixed`: la cara plana que queda quieta
/// (sin ella, la plana más grande); `thickness` sin dar se mide.
pub fn flat_pattern(shape: &Shape, k_factor: f64, thickness: Option<f64>, fixed: Option<usize>) -> Result<FlatPattern, String> {
    let e = |x: cad_occt::Error| x.to_string();
    let faces: Vec<FaceInfo> = shape.faces().map_err(e)?;
    let edges = shape.edges().map_err(e)?;
    let pairs = shape.edge_face_pairs().map_err(e)?;
    let mut face_edges = vec![Vec::new(); faces.len()];
    for (k, p) in pairs.iter().enumerate() {
        for f in p.iter().flatten() {
            face_edges[*f].push(k);
        }
    }
    let other = |edge: usize, f: usize| -> Option<usize> {
        let p = pairs[edge];
        match (p[0], p[1]) {
            (Some(a), Some(b)) if a == f => Some(b),
            (Some(a), Some(b)) if b == f => Some(a),
            _ => None,
        }
    };
    let start = match fixed {
        Some(f) if faces.get(f).is_some_and(|x| x.surface == SurfaceKind::Plane) => f,
        Some(_) => return Err("la cara fija tiene que ser plana".into()),
        None => (0..faces.len())
            .filter(|&k| faces[k].surface == SurfaceKind::Plane)
            .max_by(|&a, &b| faces[a].area.total_cmp(&faces[b].area))
            .ok_or("la pieza no tiene caras planas")?,
    };
    let n0 = normalize(faces[start].normal);
    let t = match thickness {
        Some(t) => t,
        None => shape
            .ray_hit(sub(faces[start].point, scale(n0, 1e-6)), scale(n0, -1.0))
            .ok_or("no se pudo medir el espesor")?,
    };
    if t <= 0.0 {
        return Err("el espesor tiene que ser mayor que cero".into());
    }
    // La cara fija: X a lo largo de su arista recta más larga
    let longest = face_edges[start]
        .iter()
        .filter(|&&k| edges[k].curve == CurveKind::Line)
        .max_by(|&&a, &&b| edges[a].length.total_cmp(&edges[b].length));
    let a3 = match longest {
        Some(&k) => normalize(sub(edges[k].end, edges[k].start)),
        None => normalize(cross(n0, if n0[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] })),
    };
    let fixed_map = PlaneMap { q0: faces[start].point, a3, b3: cross(n0, a3), c: [0.0, 0.0], a2: [1.0, 0.0], b2: [0.0, 1.0] };

    let mut maps: Vec<Option<FaceMap>> = (0..faces.len()).map(|_| None).collect();
    maps[start] = Some(FaceMap::Plane(fixed_map));
    let mut bends = Vec::new();
    let mut queue = VecDeque::from([start]);
    // ¿Sigue el lado por esta arista? Recta, a lo largo del eje del cilindro y tangente
    let tangent = |p: &FaceInfo, c: &FaceInfo, edge: usize| -> bool {
        let (Some(ax), Some(_)) = (c.axis, c.radius) else { return false };
        let ed = &edges[edge];
        if ed.curve != CurveKind::Line {
            return false;
        }
        let u = normalize(ax.dir);
        if dot(normalize(sub(ed.end, ed.start)), u).abs() < 0.999 {
            return false;
        }
        let v = sub(ed.start, ax.origin);
        let radial = normalize(sub(v, scale(u, dot(v, u))));
        dot(normalize(p.normal), radial).abs() > 0.999
    };
    while let Some(f) = queue.pop_front() {
        let Some(FaceMap::Plane(mp)) = maps[f] else { continue };
        let np = normalize(faces[f].normal);
        for &k in &face_edges[f] {
            let Some(c) = other(k, f) else { continue };
            if maps[c].is_some() || faces[c].surface != SurfaceKind::Cylinder || !tangent(&faces[f], &faces[c], k) {
                continue;
            }
            let ci = &faces[c];
            let (ax, r) = (ci.axis.unwrap(), ci.radius.unwrap());
            let u = normalize(ax.dir);
            let s1 = edges[k].start;
            let radial = |x: P3| {
                let v = sub(x, ax.origin);
                normalize(sub(v, scale(u, dot(v, u))))
            };
            // Cara convexa: el lado de afuera del doblez
            let convex = dot(ci.normal, radial(ci.point)) > 0.0;
            if dot(np, if convex { radial(s1) } else { scale(radial(s1), -1.0) }) < 0.99 {
                continue;
            }
            let inner = if convex { r - t } else { r };
            if inner < -1e-9 {
                return Err("un doblez tiene radio menor que el espesor".into());
            }
            // El otro borde recto del doblez y la pared que sigue
            let r1 = radial(s1);
            let e2 = face_edges[c].iter().copied().find(|&j| {
                j != k
                    && edges[j].curve == CurveKind::Line
                    && dot(normalize(sub(edges[j].end, edges[j].start)), u).abs() > 0.999
                    && norm(sub(radial(edges[j].start), r1)) > 1e-6
            });
            let Some(e2) = e2 else { continue };
            let s2 = edges[e2].start;
            let signed = dot(cross(r1, radial(s2)), u).atan2(dot(r1, radial(s2)));
            let (sign, theta) = (signed.signum(), signed.abs());
            if theta < 1e-9 {
                continue;
            }
            let length = theta * (inner + k_factor * t);
            let t3 = toward(np, u, s1, ci.point);
            let along = mp.lin(u);
            let across = unit2(mp.lin(t3));
            let base = mp.map(s1);
            let bm = BendMap { s1, base, origin: ax.origin, axis: u, r1, sign, theta, length, along, across };
            maps[c] = Some(FaceMap::Bend(bm));
            // Línea de doblez: por el medio, de punta a punta de la arista
            let (a, b) = (dot(sub(edges[k].start, s1), u), dot(sub(edges[k].end, s1), u));
            let at = |x: f64| [base[0] + along[0] * x + across[0] * length / 2.0, base[1] + along[1] * x + across[1] * length / 2.0];
            bends.push(FlatBend { line: [at(a), at(b)], angle: theta.to_degrees(), radius: inner, up: !convex });
            // La pared de después
            let Some(q) = other(e2, c) else { continue };
            if maps[q].is_some() || faces[q].surface != SurfaceKind::Plane || !tangent(&faces[q], ci, e2) {
                continue;
            }
            let nq = normalize(faces[q].normal);
            let b3 = toward(nq, u, s2, faces[q].point);
            let shift = dot(sub(s2, s1), u);
            let c2 = [base[0] + along[0] * shift + across[0] * length, base[1] + along[1] * shift + across[1] * length];
            maps[q] = Some(FaceMap::Plane(PlaneMap { q0: s2, a3: u, b3, c: c2, a2: along, b2: across }));
            queue.push_back(q);
        }
    }
    // El borde de ese lado: las aristas que dan a una cara de afuera del lado
    let mut outline = Vec::new();
    for (f, m) in maps.iter().enumerate() {
        let Some(m) = m else { continue };
        for &k in &face_edges[f] {
            if other(k, f).is_some_and(|o| maps[o].is_some()) {
                continue;
            }
            let ed = &edges[k];
            let pts: Vec<P3> = if ed.curve == CurveKind::Line {
                vec![ed.start, ed.end]
            } else {
                shape.edge_shape(k).and_then(|s| s.sample_curve(33)).map_err(e)?.into_iter().map(|(p, _)| p).collect()
            };
            outline.push(pts.into_iter().map(|p| m.map(p)).collect::<Vec<P2>>());
        }
    }
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    for p in outline.iter().flatten() {
        for i in 0..2 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }
    // Quieta en el origen: la esquina de abajo a la izquierda en (0, 0)
    let shift = |p: P2| [p[0] - min[0], p[1] - min[1]];
    let outline: Vec<Vec<P2>> = outline.into_iter().map(|l| l.into_iter().map(shift).collect()).collect();
    let bends = bends.into_iter().map(|b| FlatBend { line: b.line.map(shift), ..b }).collect();
    Ok(FlatPattern { outline, bends, thickness: t, k_factor, min: [0.0, 0.0], max: [max[0] - min[0], max[1] - min[1]] })
}
