//! Zonas de caras elegidas para texturizar: crecer por ángulo, invertir, área

use std::collections::VecDeque;

use pinocchio_math::{Real, Vector3};
use pinocchio_mesh::Mesh;

/// Caras vecinas por arista, uniendo los vértices que están en el mismo lugar (las costuras de UV y las
/// aristas duras parten los vértices en las mallas importadas, y por índice no se verían vecinas)
pub fn face_neighbors(mesh: &Mesh) -> Vec<Vec<usize>> {
    use std::collections::HashMap;
    let (pos, tris) = crate::geom::triangles(mesh);
    let weld = crate::geom::weld_ids(&pos, (crate::geom::diagonal(&pos) * 1e-7).max(1e-9));
    let mut edges: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
    for (f, t) in tris.iter().enumerate() {
        for i in 0..3 {
            let (a, b) = (weld[t[i]], weld[t[(i + 1) % 3]]);
            if a != b {
                edges.entry((a.min(b), a.max(b))).or_default().push(f);
            }
        }
    }
    let mut out = vec![Vec::new(); tris.len()];
    for faces in edges.values() {
        for &f in faces {
            for &g in faces {
                if f != g && !out[f].contains(&g) {
                    out[f].push(g);
                }
            }
        }
    }
    out
}

/// Amplía `selected` con las caras que se alcanzan cruzando solo aristas casi planas: el ángulo entre la
/// normal de una cara y la de su vecina no pasa de `max_angle_deg`. Con 0 queda igual; con 180 son todas
/// las caras conectadas. Devuelve las caras ordenadas y sin repetir
pub fn grow_by_angle(mesh: &Mesh, selected: &[usize], max_angle_deg: Real) -> Vec<usize> {
    let n = mesh.num_faces();
    let neighbors = face_neighbors(mesh);
    let normals: Vec<_> = (0..n).map(|f| mesh.get_face_normal(f)).collect();
    let min_dot = max_angle_deg.clamp(0.0, 180.0).to_radians().cos();
    let mut inside = vec![false; n];
    let mut queue: VecDeque<usize> = VecDeque::new();
    for &f in selected.iter().filter(|&&f| f < n) {
        if !inside[f] {
            inside[f] = true;
            queue.push_back(f);
        }
    }
    while let Some(f) = queue.pop_front() {
        for &g in &neighbors[f] {
            if !inside[g] && normals[f].dot(&normals[g]) >= min_dot - 1e-12 {
                inside[g] = true;
                queue.push_back(g);
            }
        }
    }
    (0..n).filter(|&f| inside[f]).collect()
}

/// Todas las caras que no están en `selected`
pub fn invert_zone(face_count: usize, selected: &[usize]) -> Vec<usize> {
    let mut inside = vec![false; face_count];
    for &f in selected.iter().filter(|&&f| f < face_count) {
        inside[f] = true;
    }
    (0..face_count).filter(|&f| !inside[f]).collect()
}

/// Área de las caras elegidas (en las unidades de la malla al cuadrado)
pub fn zone_area(mesh: &Mesh, faces: &[usize]) -> Real {
    faces.iter().filter(|&&f| f < mesh.num_faces()).map(|&f| mesh.get_face_area(f)).sum()
}

fn face_center(mesh: &Mesh, f: usize) -> Vector3 {
    let v = mesh.get_face_vertices(f);
    (mesh.vertices[v[0]].position + mesh.vertices[v[1]].position + mesh.vertices[v[2]].position) * (1.0 / 3.0)
}

/// Un agujero cilíndrico (cóncavo) encontrado en la malla
#[derive(Debug, Clone, PartialEq)]
pub struct CylinderHole {
    /// Caras de la pared del agujero
    pub faces: Vec<usize>,
    /// Un punto del eje y su dirección (unitaria)
    pub center: Vector3,
    pub axis: Vector3,
    pub radius: Real,
}

/// Autovectores de una matriz simétrica 3×3 (Jacobi): (valores, vectores columna), de menor a mayor valor
#[allow(clippy::needless_range_loop)]
fn eigen_symmetric(m: [[Real; 3]; 3]) -> ([Real; 3], [Vector3; 3]) {
    let mut a = m;
    let mut v = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for _ in 0..32 {
        let off = a[0][1].abs() + a[0][2].abs() + a[1][2].abs();
        if off < 1e-15 {
            break;
        }
        for (p, q) in [(0, 1), (0, 2), (1, 2)] {
            if a[p][q].abs() < 1e-300 {
                continue;
            }
            let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
            let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
            let t = if theta == 0.0 { 1.0 } else { t };
            let c = 1.0 / (t * t + 1.0).sqrt();
            let s = t * c;
            for row in &mut a {
                let (akp, akq) = (row[p], row[q]);
                row[p] = c * akp - s * akq;
                row[q] = s * akp + c * akq;
            }
            for k in 0..3 {
                let (apk, aqk) = (a[p][k], a[q][k]);
                a[p][k] = c * apk - s * aqk;
                a[q][k] = s * apk + c * aqk;
            }
            for row in &mut v {
                let (vkp, vkq) = (row[p], row[q]);
                row[p] = c * vkp - s * vkq;
                row[q] = s * vkp + c * vkq;
            }
        }
    }
    let mut order = [0usize, 1, 2];
    order.sort_by(|&i, &j| a[i][i].total_cmp(&a[j][j]));
    (order.map(|i| a[i][i]), order.map(|i| Vector3::new(v[0][i], v[1][i], v[2][i])))
}

/// Componentes de caras unidas por aristas casi planas (ángulo entre vecinas de hasta `max_angle_deg`)
fn smooth_components(mesh: &Mesh, max_angle_deg: Real) -> Vec<Vec<usize>> {
    let n = mesh.num_faces();
    let neighbors = face_neighbors(mesh);
    let normals: Vec<_> = (0..n).map(|f| mesh.get_face_normal(f)).collect();
    let min_dot = max_angle_deg.clamp(0.0, 180.0).to_radians().cos();
    let mut seen = vec![false; n];
    let mut out = Vec::new();
    for start in 0..n {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        let mut comp = vec![start];
        let mut queue = VecDeque::from([start]);
        while let Some(f) = queue.pop_front() {
            for &g in &neighbors[f] {
                if !seen[g] && normals[f].dot(&normals[g]) >= min_dot - 1e-12 {
                    seen[g] = true;
                    comp.push(g);
                    queue.push_back(g);
                }
            }
        }
        out.push(comp);
    }
    out
}

/// Qué es lo que conviene dejar liso (no texturizar)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FastenerKind {
    /// Agujero cilíndrico (cóncavo): tornillo, clavija
    Hole,
    /// Cilindro por fuera (convexo) y corto: perno de unión, clavija saliente
    Pin,
    /// Superficie helicoidal: rosca (macho o hembra)
    Thread,
    /// Cono: avellanado o asiento de cabeza de tornillo
    Cone,
}

/// Algo encontrado que conviene proteger: sus caras, el eje y el radio medio
#[derive(Debug, Clone, PartialEq)]
pub struct Fastener {
    pub kind: FastenerKind,
    pub faces: Vec<usize>,
    pub center: Vector3,
    pub axis: Vector3,
    pub radius: Real,
}

/// Normal media y covarianza (sin centrar) de las normales de una componente, pesadas por área
fn normal_stats(mesh: &Mesh, comp: &[usize]) -> Option<(Vector3, [[Real; 3]; 3])> {
    let (mut cov, mut mean, mut total) = ([[0.0; 3]; 3], Vector3::zero(), 0.0);
    for &f in comp {
        let (n, a) = (mesh.get_face_normal(f), mesh.get_face_area(f));
        let c = [n.x(), n.y(), n.z()];
        for i in 0..3 {
            for j in 0..3 {
                cov[i][j] += a * c[i] * c[j];
            }
        }
        mean += n * a;
        total += a;
    }
    if total <= 0.0 {
        return None;
    }
    for row in &mut cov {
        for x in row.iter_mut() {
            *x /= total;
        }
    }
    Some((mean * (1.0 / total), cov))
}

/// Cuánto de la vuelta alrededor de `axis` cubren las normales (radianes)
fn normal_coverage(mesh: &Mesh, comp: &[usize], u: Vector3, v: Vector3) -> Real {
    let mut angles: Vec<Real> = comp
        .iter()
        .filter_map(|&f| {
            let n = mesh.get_face_normal(f);
            let (x, y) = (n.dot(&u), n.dot(&v));
            (x * x + y * y > 1e-6).then(|| y.atan2(x))
        })
        .collect();
    if angles.is_empty() {
        return 0.0;
    }
    angles.sort_by(Real::total_cmp);
    let mut gap = angles[0] + std::f64::consts::TAU - angles[angles.len() - 1];
    for w in angles.windows(2) {
        gap = gap.max(w[1] - w[0]);
    }
    std::f64::consts::TAU - gap
}

/// Círculo por mínimos cuadrados (Kasa) de los centros de las caras proyectados en el plano (u, v): centro en el
/// plano, radio medio, dispersión relativa de los radios y los puntos (u, v, a lo largo del eje)
#[allow(clippy::type_complexity)]
fn circle_fit(mesh: &Mesh, comp: &[usize], u: Vector3, v: Vector3, axis: Vector3) -> Option<((Real, Real), Real, Real, Vec<(Real, Real, Real)>)> {
    let pts: Vec<(Real, Real, Real)> = comp
        .iter()
        .map(|&f| {
            let c = face_center(mesh, f);
            (c.dot(&u), c.dot(&v), c.dot(&axis))
        })
        .collect();
    let m = pts.len() as Real;
    let (mx, my) = (pts.iter().map(|p| p.0).sum::<Real>() / m, pts.iter().map(|p| p.1).sum::<Real>() / m);
    let (mut suu, mut suv, mut svv, mut suuu, mut svvv, mut suvv, mut svuu) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    for p in &pts {
        let (x, y) = (p.0 - mx, p.1 - my);
        suu += x * x;
        svv += y * y;
        suv += x * y;
        suuu += x * x * x;
        svvv += y * y * y;
        suvv += x * y * y;
        svuu += y * x * x;
    }
    let det = suu * svv - suv * suv;
    if det.abs() < 1e-18 {
        return None;
    }
    let (b1, b2) = (0.5 * (suuu + suvv), 0.5 * (svvv + svuu));
    let (cx, cy) = ((b1 * svv - b2 * suv) / det + mx, (b2 * suu - b1 * suv) / det + my);
    let radii: Vec<Real> = pts.iter().map(|p| ((p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sqrt()).collect();
    let radius = radii.iter().sum::<Real>() / m;
    if radius <= 1e-12 {
        return None;
    }
    let spread = (radii.iter().map(|r| (r - radius).powi(2)).sum::<Real>() / m).sqrt() / radius;
    Some(((cx, cy), radius, spread, pts))
}

/// Busca lo que conviene dejar liso al texturizar, de entre `min_diameter` y `max_diameter` (unidades de la malla):
/// agujeros cilíndricos, pernos (cilindros por fuera, de largo hasta tres diámetros y que son un detalle: menos del 20 %
/// de la superficie del modelo, así un cilindro que es la pieza entera no cuenta), roscas (superficies
/// helicoidales: se protege toda la franja entre el fondo y la cresta) y conos (avellanados). Solo devuelve las clases
/// de `kinds`
pub fn detect_fasteners(mesh: &Mesh, min_diameter: Real, max_diameter: Real, kinds: &[FastenerKind]) -> Vec<Fastener> {
    let want = |k: FastenerKind| kinds.contains(&k);
    let in_range = |r: Real| (min_diameter * 0.5..=max_diameter * 0.5).contains(&r);
    let mut out: Vec<Fastener> = Vec::new();
    // Roscas y conos se juntan por eje (cada flanco de una rosca es una componente aparte)
    let mut helical: Vec<Fastener> = Vec::new();
    let total_area: Real = (0..mesh.num_faces()).map(|f| mesh.get_face_area(f)).sum();
    for comp in smooth_components(mesh, 35.0) {
        if comp.len() < 8 {
            continue;
        }
        let Some((mean, cov)) = normal_stats(mesh, &comp) else { continue };
        // Cilindro: las normales caen en un plano (un autovalor ≈ 0, el eje) y los otros dos con curvatura
        let (vals, vecs) = eigen_symmetric(cov);
        if vals[0] <= 0.02 && vals[1] >= 0.1 {
            let axis = vecs[0].normalize();
            let (u, v) = crate::contact::plane_basis(axis);
            if normal_coverage(mesh, &comp, u, v) < 270f64.to_radians() {
                continue;
            }
            let Some(((cx, cy), radius, spread, pts)) = circle_fit(mesh, &comp, u, v, axis) else { continue };
            if !in_range(radius) || spread > 0.08 {
                continue;
            }
            // Cóncavo (la normal mira al eje) o convexo (mira hacia afuera)
            let m = pts.len() as Real;
            let facing: Real = comp
                .iter()
                .zip(&pts)
                .map(|(&f, p)| {
                    let radial = u * (p.0 - cx) + v * (p.1 - cy);
                    mesh.get_face_normal(f).dot(&radial.try_normalize().unwrap_or_else(Vector3::zero))
                })
                .sum::<Real>()
                / m;
            let (lo, hi) = pts.iter().fold((Real::INFINITY, Real::NEG_INFINITY), |(lo, hi), p| (lo.min(p.2), hi.max(p.2)));
            let kind = if facing < -0.8 {
                FastenerKind::Hole
            } else if facing > 0.8 && hi - lo <= 6.0 * radius && comp.iter().map(|&f| mesh.get_face_area(f)).sum::<Real>() < 0.2 * total_area {
                FastenerKind::Pin
            } else {
                continue;
            };
            if want(kind) {
                let mid = (lo + hi) * 0.5;
                let mut faces = comp.clone();
                faces.sort_unstable();
                out.push(Fastener { kind, faces, center: u * cx + v * cy + axis * mid, axis, radius });
            }
            continue;
        }
        // Cono o hélice: las normales giran alrededor del eje con la misma inclinación. Centradas en su media,
        // caen en un plano perpendicular al eje
        if !(want(FastenerKind::Thread) || want(FastenerKind::Cone)) {
            continue;
        }
        let m = [mean.x(), mean.y(), mean.z()];
        let mut centered = cov;
        for i in 0..3 {
            for j in 0..3 {
                centered[i][j] -= m[i] * m[j];
            }
        }
        let (vals, vecs) = eigen_symmetric(centered);
        let tilt = mean.length();
        if vals[0] > 0.02 || vals[1] < 0.04 || !(0.2..=0.97).contains(&tilt) {
            continue;
        }
        let axis = vecs[0].normalize();
        let (u, v) = crate::contact::plane_basis(axis);
        if normal_coverage(mesh, &comp, u, v) < 270f64.to_radians() {
            continue;
        }
        let Some(((cx, cy), radius, spread, _)) = circle_fit(mesh, &comp, u, v, axis) else { continue };
        if !in_range(radius) || spread > 0.3 {
            continue;
        }
        let mut faces = comp.clone();
        faces.sort_unstable();
        helical.push(Fastener { kind: FastenerKind::Cone, faces, center: u * cx + v * cy, axis, radius });
    }

    // Juntar los flancos del mismo eje y decidir rosca o cono; después se suma toda la franja de la rosca
    let mut groups: Vec<Fastener> = Vec::new();
    for h in helical {
        let same = groups.iter_mut().find(|g| {
            let d = h.center - g.center;
            let off_axis = (d - g.axis * d.dot(&g.axis)).length();
            g.axis.dot(&h.axis).abs() > 0.98 && off_axis < 0.2 * g.radius.max(h.radius)
        });
        match same {
            Some(g) => {
                g.faces.extend(h.faces);
                g.radius = (g.radius + h.radius) * 0.5;
            }
            None => groups.push(h),
        }
    }
    let centers: Vec<Vector3> = (0..mesh.num_faces()).map(|f| face_center(mesh, f)).collect();
    for mut g in groups {
        let (u, v) = crate::contact::plane_basis(g.axis);
        let c0 = g.center;
        let polar = |p: Vector3| {
            let d = p - c0;
            let z = d.dot(&g.axis);
            ((d.dot(&u).powi(2) + d.dot(&v).powi(2)).sqrt(), z)
        };
        let (mut rlo, mut rhi, mut zlo, mut zhi) = (Real::INFINITY, Real::NEG_INFINITY, Real::INFINITY, Real::NEG_INFINITY);
        for &f in &g.faces {
            for &i in &mesh.get_face_vertices(f) {
                let (r, z) = polar(mesh.vertices[i].position);
                rlo = rlo.min(r);
                rhi = rhi.max(r);
                zlo = zlo.min(z);
                zhi = zhi.max(z);
            }
        }
        let band = (rhi - rlo).max(1e-9);
        // Una hélice avanza a lo largo del eje mucho más que lo que mide su franja; un cono, casi lo mismo
        g.kind = if zhi - zlo > 2.5 * band { FastenerKind::Thread } else { FastenerKind::Cone };
        if !want(g.kind) {
            continue;
        }
        if g.kind == FastenerKind::Thread {
            // Toda la franja: crestas, fondos y la boca, aunque no se hayan visto como flancos
            let pad = 0.15 * band;
            let mut set: std::collections::BTreeSet<usize> = g.faces.iter().copied().collect();
            for (f, &c) in centers.iter().enumerate() {
                let (r, z) = polar(c);
                if r >= rlo - pad && r <= rhi + pad && z >= zlo - pad && z <= zhi + pad {
                    set.insert(f);
                }
            }
            g.faces = set.into_iter().collect();
            g.center = c0 + g.axis * ((zlo + zhi) * 0.5);
        } else {
            g.faces.sort_unstable();
        }
        g.radius = (rlo + rhi) * 0.5;
        out.push(g);
    }
    out
}

/// Busca agujeros cilíndricos: paredes lisas y curvas cuyas normales miran hacia un eje, que dan casi la vuelta
/// entera y miden entre `min_diameter` y `max_diameter` (unidades de la malla). Sirve para no texturizar donde va
/// un tornillo o una clavija. No toma como agujero un cilindro por fuera (un pin) ni una pared plana
pub fn detect_cylindrical_holes(mesh: &Mesh, min_diameter: Real, max_diameter: Real) -> Vec<CylinderHole> {
    detect_fasteners(mesh, min_diameter, max_diameter, &[FastenerKind::Hole])
        .into_iter()
        .map(|f| CylinderHole { faces: f.faces, center: f.center, axis: f.axis, radius: f.radius })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::printability::tests::make_box;
    use pinocchio_math::Vector3;

    /// Triángulos de la cara de arriba (z = max) de la caja
    fn top_faces(m: &Mesh) -> Vec<usize> {
        let zmax = m.vertices.iter().map(|v| v.position.z()).fold(Real::NEG_INFINITY, Real::max);
        (0..m.num_faces()).filter(|&f| m.get_face_vertices(f).iter().all(|&i| (m.vertices[i].position.z() - zmax).abs() < 1e-9)).collect()
    }

    #[test]
    fn crecer_por_angulo_se_queda_en_la_cara_plana() {
        let b = make_box(20.0, 10.0, 5.0);
        let top = top_faces(&b);
        assert_eq!(top.len(), 2);
        // Un triángulo de la tapa y 10°: llega al otro de la misma tapa y a nada más (las aristas de la caja son de 90°)
        assert_eq!(grow_by_angle(&b, &top[..1], 10.0), top);
        // Con 100° cruza las aristas vivas y toma toda la caja
        assert_eq!(grow_by_angle(&b, &top[..1], 100.0).len(), b.num_faces());
        // Con 0° no crece, pero el otro triángulo coplanar sigue entrando (ángulo 0 entre normales)
        assert_eq!(grow_by_angle(&b, &top[..1], 0.0), top);
        // Las caras fuera de rango se ignoran
        assert_eq!(grow_by_angle(&b, &[999], 90.0), Vec::<usize>::new());
    }

    #[test]
    fn crece_aunque_los_vertices_esten_partidos() {
        // Caja "sopa": cada triángulo con sus 3 vértices propios (como un STL sin unir)
        let b = make_box(20.0, 10.0, 5.0);
        let (pos, tris) = crate::geom::triangles(&b);
        let soup_pos: Vec<_> = tris.iter().flat_map(|t| t.iter().map(|&i| pos[i])).collect();
        let soup_tris: Vec<[usize; 3]> = (0..tris.len()).map(|f| [3 * f, 3 * f + 1, 3 * f + 2]).collect();
        let soup = Mesh::from_triangles(&soup_pos, &soup_tris);
        let top = top_faces(&soup);
        assert_eq!(top.len(), 2);
        assert_eq!(grow_by_angle(&soup, &top[..1], 10.0), top);
        assert_eq!(grow_by_angle(&soup, &top[..1], 100.0).len(), soup.num_faces());
    }

    #[test]
    fn invertir_y_area() {
        let b = make_box(20.0, 10.0, 5.0);
        let top = top_faces(&b);
        let rest = invert_zone(b.num_faces(), &top);
        assert_eq!(rest.len(), b.num_faces() - top.len());
        assert!(rest.iter().all(|f| !top.contains(f)));
        assert!((zone_area(&b, &top) - 200.0).abs() < 1e-9);
        assert!((zone_area(&b, &top) + zone_area(&b, &rest) - 2.0 * (20.0 * 10.0 + 20.0 * 5.0 + 10.0 * 5.0)).abs() < 1e-9);
        assert_eq!(invert_zone(4, &[]), vec![0, 1, 2, 3]);
        assert_eq!(invert_zone(3, &[0, 1, 2, 9]), Vec::<usize>::new());
    }

    fn box_with_hole() -> Mesh {
        use crate::planar_joints::{add_hole, footprint_polygon, PlanarShape};
        let b = make_box(40.0, 40.0, 20.0);
        let poly = footprint_polygon(&PlanarShape::Round { diameter: 8.0 }, Vector3::new(20.0, 20.0, 20.0), Vector3::unit_z(), 0.0);
        add_hole(&b, &poly, Vector3::unit_z(), 10.0, 1.0).unwrap()
    }

    #[test]
    fn encuentra_el_agujero_cilindrico() {
        let m = box_with_hole();
        let holes = detect_cylindrical_holes(&m, 3.0, 20.0);
        assert_eq!(holes.len(), 1, "{holes:?}");
        let h = &holes[0];
        assert!((h.radius - 4.0).abs() < 0.2, "radio {}", h.radius);
        assert!(h.axis.z().abs() > 0.99, "eje {:?}", h.axis);
        assert!((h.center.x() - 20.0).abs() < 0.1 && (h.center.y() - 20.0).abs() < 0.1);
        // La pared tiene 24 lados de 2 triángulos; el suelo del agujero es plano y no cuenta
        assert_eq!(h.faces.len(), 48);
        // Fuera del rango de diámetros no cuenta
        assert!(detect_cylindrical_holes(&m, 10.0, 30.0).is_empty());
        assert!(detect_cylindrical_holes(&m, 1.0, 6.0).is_empty());
    }

    /// Varilla roscada (una entrada) de radio mayor `r_major`, paso `pitch` y `turns` vueltas: dos flancos que
    /// comparten la cresta, con 48 lados por vuelta
    fn threaded_rod(r_major: Real, pitch: Real, turns: usize) -> Mesh {
        let depth = 0.54 * pitch;
        let sides = 48 * turns;
        let profile = [(0.0, r_major - depth), (0.25, r_major - depth * 0.5), (0.5, r_major), (0.75, r_major - depth * 0.5), (1.0, r_major - depth)];
        let mut pos = Vec::new();
        for i in 0..=sides {
            let phi = std::f64::consts::TAU * i as Real / 48.0;
            for &(s, r) in &profile {
                pos.push(Vector3::new(r * phi.cos(), r * phi.sin(), pitch * phi / std::f64::consts::TAU + pitch * s));
            }
        }
        let k = profile.len();
        let mut tris = Vec::new();
        for i in 0..sides {
            for j in 0..k - 1 {
                let (a, b, c, d) = (i * k + j, (i + 1) * k + j, (i + 1) * k + j + 1, i * k + j + 1);
                tris.push([a, b, c]);
                tris.push([a, c, d]);
            }
        }
        Mesh::from_triangles(&pos, &tris)
    }

    #[test]
    fn encuentra_una_rosca_y_toda_su_franja() {
        let rod = threaded_rod(5.0, 1.5, 6);
        let found = detect_fasteners(&rod, 2.0, 20.0, &[FastenerKind::Hole, FastenerKind::Pin, FastenerKind::Thread, FastenerKind::Cone]);
        let threads: Vec<_> = found.iter().filter(|f| f.kind == FastenerKind::Thread).collect();
        assert_eq!(threads.len(), 1, "{:?}", found.iter().map(|f| (f.kind, f.faces.len(), f.radius)).collect::<Vec<_>>());
        let t = threads[0];
        assert!(t.axis.z().abs() > 0.99, "eje {:?}", t.axis);
        assert!(t.radius > 4.0 && t.radius < 5.0, "radio {}", t.radius);
        assert!(t.center.x().abs() < 0.2 && t.center.y().abs() < 0.2);
        // Toda la superficie es rosca
        assert_eq!(t.faces.len(), rod.num_faces());
        // Sin pedir roscas, nada; fuera del rango de diámetros, nada
        assert!(detect_fasteners(&rod, 2.0, 20.0, &[FastenerKind::Hole, FastenerKind::Pin]).is_empty());
        assert!(detect_fasteners(&rod, 12.0, 40.0, &[FastenerKind::Thread]).is_empty());
        // La misma superficie vista desde adentro (una tuerca, rosca hembra) también es rosca
        let (pos, tris) = crate::geom::triangles(&rod);
        let flipped: Vec<[usize; 3]> = tris.iter().map(|t| [t[0], t[2], t[1]]).collect();
        let nut = Mesh::from_triangles(&pos, &flipped);
        let found = detect_fasteners(&nut, 2.0, 20.0, &[FastenerKind::Thread]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].faces.len(), nut.num_faces());
    }

    #[test]
    fn un_avellanado_es_un_cono() {
        // Cono de 90° entre radio 3 y 6 (32 lados, 4 anillos)
        let mut pos = Vec::new();
        for i in 0..=4 {
            let r = 3.0 + 3.0 * i as Real / 4.0;
            for j in 0..32 {
                let ph = std::f64::consts::TAU * j as Real / 32.0;
                pos.push(Vector3::new(r * ph.cos(), r * ph.sin(), r - 3.0));
            }
        }
        let mut tris = Vec::new();
        for i in 0..4 {
            for j in 0..32 {
                let (a, b, c, d) = (i * 32 + j, i * 32 + (j + 1) % 32, (i + 1) * 32 + (j + 1) % 32, (i + 1) * 32 + j);
                tris.push([a, c, b]);
                tris.push([a, d, c]);
            }
        }
        let cone = Mesh::from_triangles(&pos, &tris);
        let found = detect_fasteners(&cone, 2.0, 20.0, &[FastenerKind::Thread, FastenerKind::Cone]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].kind, FastenerKind::Cone);
        assert!(detect_fasteners(&cone, 2.0, 20.0, &[FastenerKind::Thread]).is_empty());
    }

    #[test]
    fn encuentra_un_perno_y_no_lo_toma_por_agujero() {
        use crate::planar_joints::{add_pin, footprint_polygon, PlanarShape};
        let b = make_box(40.0, 40.0, 20.0);
        let poly = footprint_polygon(&PlanarShape::Round { diameter: 8.0 }, Vector3::new(20.0, 20.0, 20.0), Vector3::unit_z(), 0.0);
        let pin = add_pin(&b, &poly, Vector3::unit_z(), 6.0).unwrap();
        let found = detect_fasteners(&pin, 2.0, 20.0, &[FastenerKind::Hole, FastenerKind::Pin, FastenerKind::Thread, FastenerKind::Cone]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].kind, FastenerKind::Pin);
        assert!((found[0].radius - 4.0).abs() < 0.2);
        // La caja sola no tiene nada
        assert!(detect_fasteners(&b, 1.0, 100.0, &[FastenerKind::Hole, FastenerKind::Pin, FastenerKind::Thread, FastenerKind::Cone]).is_empty());
    }

    #[test]
    fn un_cilindro_que_es_la_pieza_entera_no_es_perno() {
        // Cilindro cerrado de Ø30 × 40 (como un vaso o un mango): los lados son casi toda la superficie
        let (n, r, h) = (64, 15.0, 40.0);
        let mut pos = vec![Vector3::new(0.0, 0.0, 0.0), Vector3::new(0.0, 0.0, h)];
        for i in 0..n {
            let a = std::f64::consts::TAU * i as Real / n as Real;
            pos.push(Vector3::new(r * a.cos(), r * a.sin(), 0.0));
            pos.push(Vector3::new(r * a.cos(), r * a.sin(), h));
        }
        let mut tris = Vec::new();
        for i in 0..n {
            let j = (i + 1) % n;
            let (b0, t0, b1, t1) = (2 + 2 * i, 3 + 2 * i, 2 + 2 * j, 3 + 2 * j);
            tris.extend([[b0, b1, t1], [b0, t1, t0], [0, b1, b0], [1, t0, t1]]);
        }
        let m = Mesh::from_triangles(&pos, &tris);
        assert!(detect_fasteners(&m, 1.0, 30.0, &[FastenerKind::Pin]).is_empty());
    }

    #[test]
    fn una_esfera_no_es_rosca_ni_perno() {
        // Esfera UV de radio 5
        let (rings, segs) = (24, 48);
        let mut pos = Vec::new();
        for i in 0..=rings {
            let th = std::f64::consts::PI * i as Real / rings as Real;
            for j in 0..segs {
                let ph = std::f64::consts::TAU * j as Real / segs as Real;
                pos.push(Vector3::new(5.0 * th.sin() * ph.cos(), 5.0 * th.sin() * ph.sin(), 5.0 * th.cos()));
            }
        }
        let mut tris = Vec::new();
        for i in 0..rings {
            for j in 0..segs {
                let (a, b, c, d) = (i * segs + j, i * segs + (j + 1) % segs, (i + 1) * segs + (j + 1) % segs, (i + 1) * segs + j);
                tris.push([a, b, c]);
                tris.push([a, c, d]);
            }
        }
        let sphere = Mesh::from_triangles(&pos, &tris);
        assert!(detect_fasteners(&sphere, 1.0, 100.0, &[FastenerKind::Hole, FastenerKind::Pin, FastenerKind::Thread, FastenerKind::Cone]).is_empty());
    }

    #[test]
    fn un_perno_una_caja_y_una_esfera_no_son_agujeros() {
        use crate::planar_joints::{add_pin, footprint_polygon, PlanarShape};
        let b = make_box(40.0, 40.0, 20.0);
        assert!(detect_cylindrical_holes(&b, 1.0, 100.0).is_empty());
        let poly = footprint_polygon(&PlanarShape::Round { diameter: 8.0 }, Vector3::new(20.0, 20.0, 20.0), Vector3::unit_z(), 0.0);
        let pin = add_pin(&b, &poly, Vector3::unit_z(), 6.0).unwrap();
        assert!(detect_cylindrical_holes(&pin, 1.0, 100.0).is_empty(), "un perno (convexo) no es un agujero");
    }
}
