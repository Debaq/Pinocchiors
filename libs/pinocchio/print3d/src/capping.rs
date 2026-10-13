//! Cierre de cortes: de los segmentos de intersección a triángulos de tapa
//!
//! Soporta varios contornos, contornos con huecos (islas dentro de agujeros) y
//! polígonos cóncavos. Las tapas conservan todos los vértices del contorno,
//! incluidos los colineales, para que no queden uniones en T con los lados.

use pinocchio_math::{Real, Vector3};

use crate::contact::plane_basis;

type P2 = (Real, Real);

/// Triángulos que cierran el corte, antihorarios mirando contra `normal`
/// (es decir, con su normal exterior igual a `normal`)
pub(crate) fn cap_triangles(segments: &[(Vector3, Vector3)], normal: Vector3, origin: Vector3, tol: Real) -> Vec<[Vector3; 3]> {
    let loops = build_loops(segments, tol);
    if loops.is_empty() {
        return Vec::new();
    }
    let (u, v) = plane_basis(normal);
    let to2 = |p: &Vector3| -> P2 {
        let r = *p - origin;
        (r.dot(&u), r.dot(&v))
    };
    let pts2: Vec<Vec<P2>> = loops.iter().map(|l| l.iter().map(to2).collect()).collect();
    let areas: Vec<Real> = pts2.iter().map(|l| signed_area(l)).collect();

    // Profundidad de anidamiento: cuántos otros contornos contienen al contorno
    let depth: Vec<usize> = (0..loops.len())
        .map(|i| (0..loops.len()).filter(|&j| j != i && areas[j].abs() > areas[i].abs() && point_in_polygon(pts2[i][0], &pts2[j])).count())
        .collect();

    let mut out = Vec::new();
    for outer in (0..loops.len()).filter(|&i| depth[i] % 2 == 0) {
        // Huecos directos: un nivel más hondo y contenidos en este contorno
        let holes: Vec<usize> = (0..loops.len())
            .filter(|&h| depth[h] == depth[outer] + 1 && point_in_polygon(pts2[h][0], &pts2[outer]) && areas[h].abs() < areas[outer].abs())
            .collect();
        // Orientar: exterior antihorario, huecos horarios
        let orient = |i: usize, ccw: bool| -> (Vec<Vector3>, Vec<P2>) {
            let (mut p3, mut p2) = (loops[i].clone(), pts2[i].clone());
            if (areas[i] > 0.0) != ccw {
                p3.reverse();
                p2.reverse();
            }
            (p3, p2)
        };
        let (o3, o2) = orient(outer, true);
        let hole_rings: Vec<(Vec<Vector3>, Vec<P2>)> = holes.iter().map(|&h| orient(h, false)).collect();
        triangulate(&o3, &o2, &hole_rings, &mut out);
    }
    out
}

/// Encadena segmentos sueltos en contornos cerrados
fn build_loops(segments: &[(Vector3, Vector3)], tol: Real) -> Vec<Vec<Vector3>> {
    use std::collections::HashMap;
    let cell = tol.max(1e-12) * 4.0;
    let key = |p: &Vector3| ((p.x() / cell).floor() as i64, (p.y() / cell).floor() as i64, (p.z() / cell).floor() as i64);
    let mut grid: HashMap<(i64, i64, i64), Vec<usize>> = HashMap::new();
    let mut nodes: Vec<Vector3> = Vec::new();
    let mut node_of = |p: Vector3, nodes: &mut Vec<Vector3>| -> usize {
        let (kx, ky, kz) = key(&p);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(ids) = grid.get(&(kx + dx, ky + dy, kz + dz)) {
                        for &i in ids {
                            if (nodes[i] - p).length() <= tol {
                                return i;
                            }
                        }
                    }
                }
            }
        }
        nodes.push(p);
        grid.entry((kx, ky, kz)).or_default().push(nodes.len() - 1);
        nodes.len() - 1
    };
    let mut adj: Vec<Vec<(usize, bool)>> = Vec::new();
    for &(a, b) in segments {
        let (ia, ib) = (node_of(a, &mut nodes), node_of(b, &mut nodes));
        if ia == ib {
            continue;
        }
        if adj.len() < nodes.len() {
            adj.resize(nodes.len(), Vec::new());
        }
        adj[ia].push((ib, false));
        adj[ib].push((ia, false));
    }
    adj.resize(nodes.len(), Vec::new());

    let mut loops = Vec::new();
    for start in 0..nodes.len() {
        loop {
            let Some(first) = adj[start].iter().position(|e| !e.1) else { break };
            adj[start][first].1 = true;
            let mut path = vec![start];
            let mut cur = adj[start][first].0;
            // La arista inversa también queda usada
            if let Some(back) = adj[cur].iter().position(|e| e.0 == start && !e.1) {
                adj[cur][back].1 = true;
            }
            let mut closed = false;
            while path.len() <= nodes.len() + 1 {
                if cur == start {
                    closed = true;
                    break;
                }
                path.push(cur);
                let Some(next) = adj[cur].iter().position(|e| !e.1) else { break };
                adj[cur][next].1 = true;
                let to = adj[cur][next].0;
                if let Some(back) = adj[to].iter().position(|e| e.0 == cur && !e.1) {
                    adj[to][back].1 = true;
                }
                cur = to;
            }
            if closed && path.len() >= 3 {
                loops.push(path.into_iter().map(|i| nodes[i]).collect());
            }
        }
    }
    loops
}

fn signed_area(p: &[P2]) -> Real {
    let mut s = 0.0;
    for i in 0..p.len() {
        let (a, b) = (p[i], p[(i + 1) % p.len()]);
        s += a.0 * b.1 - b.0 * a.1;
    }
    s * 0.5
}

fn point_in_polygon(p: P2, poly: &[P2]) -> bool {
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) + a.0 {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn cross(o: P2, a: P2, b: P2) -> Real {
    (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
}

fn segments_cross(a: P2, b: P2, c: P2, d: P2) -> bool {
    let (d1, d2, d3, d4) = (cross(a, b, c), cross(a, b, d), cross(c, d, a), cross(c, d, b));
    ((d1 > 0.0) != (d2 > 0.0)) && ((d3 > 0.0) != (d4 > 0.0)) && d1 != 0.0 && d2 != 0.0 && d3 != 0.0 && d4 != 0.0
}

/// Une los huecos al contorno exterior con puentes y recorta orejas
fn triangulate(outer3: &[Vector3], outer2: &[P2], holes: &[(Vec<Vector3>, Vec<P2>)], out: &mut Vec<[Vector3; 3]>) {
    let mut ring3: Vec<Vector3> = outer3.to_vec();
    let mut ring2: Vec<P2> = outer2.to_vec();

    // Huecos de derecha a izquierda (por su punto más a la derecha)
    let mut order: Vec<usize> = (0..holes.len()).collect();
    let max_x = |h: &(Vec<Vector3>, Vec<P2>)| h.1.iter().map(|p| p.0).fold(Real::NEG_INFINITY, Real::max);
    order.sort_by(|&a, &b| max_x(&holes[b]).total_cmp(&max_x(&holes[a])));
    for hi in order {
        let (h3, h2) = &holes[hi];
        let hstart = (0..h2.len()).max_by(|&a, &b| h2[a].0.total_cmp(&h2[b].0)).unwrap();
        let hp = h2[hstart];
        // Candidatos del anillo por distancia al punto del hueco
        let mut cand: Vec<usize> = (0..ring2.len()).collect();
        cand.sort_by(|&a, &b| {
            let da = (ring2[a].0 - hp.0).powi(2) + (ring2[a].1 - hp.1).powi(2);
            let db = (ring2[b].0 - hp.0).powi(2) + (ring2[b].1 - hp.1).powi(2);
            da.total_cmp(&db)
        });
        let visible = |vi: usize, ring2: &[P2]| -> bool {
            let vp = ring2[vi];
            let n = ring2.len();
            for i in 0..n {
                let j = (i + 1) % n;
                if i == vi || j == vi {
                    continue;
                }
                if segments_cross(hp, vp, ring2[i], ring2[j]) {
                    return false;
                }
            }
            // y no cruza los demás huecos ni el propio
            for (k, (_, o2)) in holes.iter().enumerate() {
                for i in 0..o2.len() {
                    let j = (i + 1) % o2.len();
                    if k == hi && (o2[i] == hp || o2[j] == hp) {
                        continue;
                    }
                    if segments_cross(hp, vp, o2[i], o2[j]) {
                        return false;
                    }
                }
            }
            // el puente va por dentro del polígono
            let mid = ((hp.0 + vp.0) * 0.5, (hp.1 + vp.1) * 0.5);
            point_in_polygon(mid, ring2)
        };
        let vi = cand.iter().copied().find(|&c| visible(c, &ring2)).unwrap_or(cand[0]);
        // anillo: ... V, H, hueco (vuelta completa), H, V, ...
        let mut new3 = Vec::new();
        let mut new2 = Vec::new();
        for i in 0..=vi {
            new3.push(ring3[i]);
            new2.push(ring2[i]);
        }
        for k in 0..=h2.len() {
            let idx = (hstart + k) % h2.len();
            new3.push(h3[idx]);
            new2.push(h2[idx]);
        }
        new3.push(ring3[vi]);
        new2.push(ring2[vi]);
        for i in vi + 1..ring2.len() {
            new3.push(ring3[i]);
            new2.push(ring2[i]);
        }
        ring3 = new3;
        ring2 = new2;
    }
    ear_clip(&ring3, &ring2, out);
}

fn ear_clip(p3: &[Vector3], p2: &[P2], out: &mut Vec<[Vector3; 3]>) {
    let mut idx: Vec<usize> = (0..p2.len()).collect();
    let scale = p2.iter().fold(0.0 as Real, |m, p| m.max(p.0.abs()).max(p.1.abs())).max(1e-12);
    let eps = scale * scale * 1e-12;
    let inside = |a: P2, b: P2, c: P2, p: P2| -> bool {
        // estrictamente dentro (los puntos sobre el borde no cuentan)
        cross(a, b, p) > eps && cross(b, c, p) > eps && cross(c, a, p) > eps
    };
    let mut guard = 0usize;
    while idx.len() > 3 {
        guard += 1;
        let n = idx.len();
        // pasada 1: orejas convexas; pasada 2: orejas planas (colineales); pasada 3: forzar
        let mut chosen = None;
        'pass: for pass in 0..3 {
            for i in 0..n {
                let (a, b, c) = (idx[(i + n - 1) % n], idx[i], idx[(i + 1) % n]);
                let cr = cross(p2[a], p2[b], p2[c]);
                let ok = match pass {
                    0 => cr > eps,
                    1 => cr.abs() <= eps,
                    _ => true,
                };
                if !ok {
                    continue;
                }
                if pass < 2 && idx.iter().any(|&k| k != a && k != b && k != c && p2[k] != p2[a] && p2[k] != p2[b] && p2[k] != p2[c] && inside(p2[a], p2[b], p2[c], p2[k])) {
                    continue;
                }
                chosen = Some(i);
                break 'pass;
            }
        }
        let i = chosen.unwrap_or(0);
        let (a, b, c) = (idx[(i + n - 1) % n], idx[i], idx[(i + 1) % n]);
        out.push([p3[a], p3[b], p3[c]]);
        idx.remove(i);
        if guard > p2.len() * 4 {
            break;
        }
    }
    if idx.len() == 3 {
        out.push([p3[idx[0]], p3[idx[1]], p3[idx[2]]]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ring(pts: &[(Real, Real)]) -> Vec<(Vector3, Vector3)> {
        (0..pts.len())
            .map(|i| {
                let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
                (Vector3::new(a.0, a.1, 0.0), Vector3::new(b.0, b.1, 0.0))
            })
            .collect()
    }

    fn area_of(tris: &[[Vector3; 3]]) -> Real {
        tris.iter().map(|t| (t[1] - t[0]).cross(&(t[2] - t[0])).z() * 0.5).sum()
    }

    #[test]
    fn cuadrado_en_cualquier_sentido() {
        for pts in [vec![(0., 0.), (10., 0.), (10., 10.), (0., 10.)], vec![(0., 0.), (0., 10.), (10., 10.), (10., 0.)]] {
            let t = cap_triangles(&ring(&pts), Vector3::unit_z(), Vector3::zero(), 1e-6);
            assert_eq!(t.len(), 2);
            assert!((area_of(&t) - 100.0).abs() < 1e-9, "{}", area_of(&t));
        }
    }

    #[test]
    fn poligono_concavo_en_l() {
        let l = [(0., 0.), (20., 0.), (20., 10.), (10., 10.), (10., 20.), (0., 20.)];
        let t = cap_triangles(&ring(&l), Vector3::unit_z(), Vector3::zero(), 1e-6);
        assert!((area_of(&t) - 300.0).abs() < 1e-9, "{}", area_of(&t));
    }

    #[test]
    fn dos_islas() {
        let mut segs = ring(&[(0., 0.), (10., 0.), (10., 10.), (0., 10.)]);
        segs.extend(ring(&[(20., 0.), (30., 0.), (30., 10.), (20., 10.)]));
        let t = cap_triangles(&segs, Vector3::unit_z(), Vector3::zero(), 1e-6);
        assert!((area_of(&t) - 200.0).abs() < 1e-9);
    }

    #[test]
    fn cuadrado_con_agujero() {
        let mut segs = ring(&[(0., 0.), (30., 0.), (30., 30.), (0., 30.)]);
        segs.extend(ring(&[(10., 10.), (10., 20.), (20., 20.), (20., 10.)]));
        let t = cap_triangles(&segs, Vector3::unit_z(), Vector3::zero(), 1e-6);
        assert!((area_of(&t) - 800.0).abs() < 1e-6, "{}", area_of(&t));
    }

    #[test]
    fn isla_dentro_de_un_agujero() {
        let mut segs = ring(&[(0., 0.), (50., 0.), (50., 50.), (0., 50.)]);
        segs.extend(ring(&[(10., 10.), (10., 40.), (40., 40.), (40., 10.)]));
        segs.extend(ring(&[(20., 20.), (30., 20.), (30., 30.), (20., 30.)]));
        let t = cap_triangles(&segs, Vector3::unit_z(), Vector3::zero(), 1e-6);
        assert!((area_of(&t) - (2500.0 - 900.0 + 100.0)).abs() < 1e-6, "{}", area_of(&t));
    }

    #[test]
    fn lados_con_vertices_colineales() {
        // Cuadrado con puntos intermedios en los lados: se conservan todos
        let pts = [(0., 0.), (5., 0.), (10., 0.), (10., 5.), (10., 10.), (5., 10.), (0., 10.), (0., 5.)];
        let t = cap_triangles(&ring(&pts), Vector3::unit_z(), Vector3::zero(), 1e-6);
        assert!((area_of(&t) - 100.0).abs() < 1e-9);
        // cada vértice del contorno aparece en alguna tapa
        for p in pts {
            let q = Vector3::new(p.0, p.1, 0.0);
            assert!(t.iter().any(|tri| tri.iter().any(|v| (*v - q).length() < 1e-9)), "{p:?}");
        }
    }
}
