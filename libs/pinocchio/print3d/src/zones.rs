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

/// Busca agujeros cilíndricos: paredes lisas y curvas cuyas normales miran hacia un eje, que dan casi la vuelta
/// entera y miden entre `min_diameter` y `max_diameter` (unidades de la malla). Sirve para no texturizar donde va
/// un tornillo o una clavija. No toma como agujero un cilindro por fuera (un pin) ni una pared plana
pub fn detect_cylindrical_holes(mesh: &Mesh, min_diameter: Real, max_diameter: Real) -> Vec<CylinderHole> {
    let mut out = Vec::new();
    for comp in smooth_components(mesh, 35.0) {
        if comp.len() < 8 {
            continue;
        }
        let (mut cov, mut total) = ([[0.0; 3]; 3], 0.0);
        for &f in &comp {
            let (n, a) = (mesh.get_face_normal(f), mesh.get_face_area(f));
            let c = [n.x(), n.y(), n.z()];
            for i in 0..3 {
                for j in 0..3 {
                    cov[i][j] += a * c[i] * c[j];
                }
            }
            total += a;
        }
        if total <= 0.0 {
            continue;
        }
        for row in &mut cov {
            for x in row.iter_mut() {
                *x /= total;
            }
        }
        // Las normales de un cilindro caen en un plano: un autovalor ≈ 0 (el eje) y los otros dos con curvatura
        let (vals, vecs) = eigen_symmetric(cov);
        if vals[0] > 0.02 || vals[1] < 0.1 {
            continue;
        }
        let axis = vecs[0].normalize();
        let (u, v) = crate::contact::plane_basis(axis);
        // Cuánto de la vuelta cubren las normales
        let mut angles: Vec<Real> = comp
            .iter()
            .map(|&f| {
                let n = mesh.get_face_normal(f);
                n.dot(&v).atan2(n.dot(&u))
            })
            .collect();
        angles.sort_by(Real::total_cmp);
        let mut gap = angles.first().map_or(0.0, |a0| a0 + std::f64::consts::TAU - angles.last().copied().unwrap_or(*a0));
        for w in angles.windows(2) {
            gap = gap.max(w[1] - w[0]);
        }
        if std::f64::consts::TAU - gap < 270f64.to_radians() {
            continue;
        }
        // Círculo por mínimos cuadrados (Kasa) con los centros de las caras proyectados
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
            continue;
        }
        let (b1, b2) = (0.5 * (suuu + suvv), 0.5 * (svvv + svuu));
        let (cx, cy) = ((b1 * svv - b2 * suv) / det, (b2 * suu - b1 * suv) / det);
        let radii: Vec<Real> = pts.iter().map(|p| ((p.0 - mx - cx).powi(2) + (p.1 - my - cy).powi(2)).sqrt()).collect();
        let radius = radii.iter().sum::<Real>() / m;
        if radius <= 1e-12 || !(min_diameter * 0.5..=max_diameter * 0.5).contains(&radius) {
            continue;
        }
        let spread = (radii.iter().map(|r| (r - radius).powi(2)).sum::<Real>() / m).sqrt();
        if spread / radius > 0.08 {
            continue;
        }
        // Cóncavo: la normal de cada cara mira hacia el eje
        let concave: Real = comp
            .iter()
            .zip(&pts)
            .map(|(&f, p)| {
                let radial = u * (p.0 - mx - cx) + v * (p.1 - my - cy);
                mesh.get_face_normal(f).dot(&radial.try_normalize().unwrap_or_else(Vector3::zero))
            })
            .sum::<Real>()
            / m;
        if concave > -0.8 {
            continue;
        }
        let mid = pts.iter().map(|p| p.2).sum::<Real>() / m;
        out.push(CylinderHole { faces: { let mut f = comp.clone(); f.sort_unstable(); f }, center: u * (mx + cx) + v * (my + cy) + axis * mid, axis, radius });
    }
    out
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
