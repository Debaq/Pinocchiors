//! Contactos planos entre piezas cortadas: las caras de corte que se tocan

use std::collections::HashMap;

use pinocchio_math::{Real, Vector3};
use pinocchio_mesh::Mesh;

use crate::geom::{diagonal, normal_area, triangles};
use crate::labeling::LabeledPiece;

/// Conjunto de triángulos coplanares con la misma normal (una cara de corte)
#[derive(Debug, Clone)]
pub struct PlanarPatch {
    /// Normal exterior
    pub normal: Vector3,
    /// `normal · p` para los puntos del plano
    pub offset: Real,
    /// Triángulos del parche (vértices en 3D)
    pub triangles: Vec<[Vector3; 3]>,
    pub area: Real,
}

/// Parches planos de al menos `min_area` (mm²). `tol` es la distancia máxima al
/// plano para considerar dos triángulos coplanares
pub fn planar_patches(mesh: &Mesh, min_area: Real, tol: Real) -> Vec<PlanarPatch> {
    let (pos, tris) = triangles(mesh);
    let mut groups: HashMap<(i32, i32, i32, i64), PlanarPatch> = HashMap::new();
    for t in &tris {
        let (a, b, c) = (pos[t[0]], pos[t[1]], pos[t[2]]);
        let Some((n, area)) = normal_area(a, b, c) else { continue };
        let off = n.dot(&a);
        // Solo si los tres vértices están en el plano (normal exacta del triángulo)
        let key = ((n.x() * 1e4).round() as i32, (n.y() * 1e4).round() as i32, (n.z() * 1e4).round() as i32, (off / tol.max(1e-9)).round() as i64);
        let g = groups.entry(key).or_insert_with(|| PlanarPatch { normal: n, offset: off, triangles: Vec::new(), area: 0.0 });
        g.triangles.push([a, b, c]);
        g.area += area;
    }
    let mut out: Vec<PlanarPatch> = groups.into_values().filter(|g| g.area >= min_area).collect();
    // Normal y plano promedio por parche (los triángulos del grupo son casi iguales)
    for g in &mut out {
        let n = g.triangles.iter().fold(Vector3::zero(), |acc, t| acc + normal_area(t[0], t[1], t[2]).map_or(Vector3::zero(), |(n, a)| n * a));
        if let Some(n) = n.try_normalize() {
            g.normal = n;
        }
        g.offset = g.triangles.iter().map(|t| g.normal.dot(&t[0])).sum::<Real>() / g.triangles.len() as Real;
    }
    out.sort_by(|a, b| b.area.total_cmp(&a.area));
    out
}

/// Cara de contacto entre dos piezas
#[derive(Debug, Clone)]
pub struct Contact {
    pub piece_a: usize,
    pub piece_b: usize,
    /// Normal exterior de A (apunta hacia B)
    pub normal: Vector3,
    /// `normal · p` del plano de contacto
    pub offset: Real,
    pub area: Real,
    pub center: Vector3,
    /// Región de contacto: polígonos convexos coplanares (solapes triángulo a triángulo)
    pub region: Vec<Vec<Vector3>>,
}

/// Contactos planos entre dos mallas (plano común con normales opuestas y solape)
pub fn contacts_between(a: &Mesh, b: &Mesh, min_area: Real, tol: Real) -> Vec<Contact> {
    let pa = planar_patches(a, min_area, tol);
    let pb = planar_patches(b, min_area, tol);
    let mut out = Vec::new();
    for x in &pa {
        for y in &pb {
            if x.normal.dot(&y.normal) > -0.9999 || (x.offset + y.offset).abs() > tol * 2.0 {
                continue;
            }
            let (n, o) = (x.normal, x.offset);
            let (u, v) = plane_basis(n);
            let to2 = |p: &Vector3| (p.dot(&u), p.dot(&v));
            let mut region = Vec::new();
            let (mut area, mut centroid) = (0.0, Vector3::zero());
            // Prefiltro por caja 2D de cada triángulo de B
            let boxes_b: Vec<([Real; 2], [Real; 2])> = y.triangles.iter().map(|t| bbox2(t, &to2)).collect();
            for ta in &x.triangles {
                let (la, ha) = bbox2(ta, &to2);
                for (tb, (lb, hb)) in y.triangles.iter().zip(&boxes_b) {
                    if ha[0] < lb[0] || hb[0] < la[0] || ha[1] < lb[1] || hb[1] < la[1] {
                        continue;
                    }
                    if let Some(poly) = clip_triangles(ta, tb, n, &to2) {
                        let ar = polygon_area(&poly, n);
                        if ar > 1e-9 {
                            let c = poly.iter().fold(Vector3::zero(), |s, p| s + *p) * (1.0 / poly.len() as Real);
                            area += ar;
                            centroid += c * ar;
                            region.push(poly);
                        }
                    }
                }
            }
            if area >= min_area {
                out.push(Contact { piece_a: 0, piece_b: 0, normal: n, offset: o, area, center: centroid * (1.0 / area), region });
            }
        }
    }
    out
}

/// Contactos entre todas las piezas
pub fn find_contacts(pieces: &[LabeledPiece], min_area: Real, tol: Real) -> Vec<Contact> {
    let mut out = Vec::new();
    // Prefiltro por caja: solo pares que se tocan
    let boxes: Vec<Option<([Real; 3], [Real; 3])>> = pieces
        .iter()
        .map(|p| {
            (!p.mesh.vertices.is_empty()).then(|| {
                let bb = crate::analysis::compute_bounding_box(&p.mesh);
                (bb.min, bb.max)
            })
        })
        .collect();
    for i in 0..pieces.len() {
        for j in i + 1..pieces.len() {
            let (Some(bi), Some(bj)) = (boxes[i], boxes[j]) else { continue };
            if (0..3).any(|k| bi.1[k] + tol < bj.0[k] || bj.1[k] + tol < bi.0[k]) {
                continue;
            }
            for mut c in contacts_between(&pieces[i].mesh, &pieces[j].mesh, min_area, tol) {
                c.piece_a = pieces[i].id;
                c.piece_b = pieces[j].id;
                out.push(c);
            }
        }
    }
    out
}

/// Tolerancia de coplanaridad razonable para una malla
pub fn plane_tolerance(mesh: &Mesh) -> Real {
    let (pos, _) = triangles(mesh);
    (diagonal(&pos) * 1e-5).max(1e-6)
}

/// Base ortonormal (u, v) del plano de normal `n` (u × v = n)
pub fn plane_basis(n: Vector3) -> (Vector3, Vector3) {
    let helper = if n.x().abs() < 0.9 { Vector3::unit_x() } else { Vector3::unit_y() };
    let u = helper.cross(&n).normalize();
    let v = n.cross(&u);
    (u, v)
}

fn bbox2(t: &[Vector3; 3], to2: &impl Fn(&Vector3) -> (Real, Real)) -> ([Real; 2], [Real; 2]) {
    let pts = [to2(&t[0]), to2(&t[1]), to2(&t[2])];
    let lo = [pts.iter().map(|p| p.0).fold(Real::INFINITY, Real::min), pts.iter().map(|p| p.1).fold(Real::INFINITY, Real::min)];
    let hi = [pts.iter().map(|p| p.0).fold(Real::NEG_INFINITY, Real::max), pts.iter().map(|p| p.1).fold(Real::NEG_INFINITY, Real::max)];
    (lo, hi)
}

/// Área de un polígono convexo coplanar respecto a la normal `n`
pub(crate) fn polygon_area(poly: &[Vector3], n: Vector3) -> Real {
    let mut s = Vector3::zero();
    for i in 0..poly.len() {
        s += poly[i].cross(&poly[(i + 1) % poly.len()]);
    }
    (s.dot(&n) * 0.5).abs()
}

/// Recorta un polígono convexo por un semiplano: se queda con `side(p) >= 0`
pub(crate) fn clip_half_plane(poly: &[Vector3], side: impl Fn(&Vector3) -> Real) -> Vec<Vector3> {
    let mut out = Vec::with_capacity(poly.len() + 2);
    for i in 0..poly.len() {
        let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
        let (sp, sq) = (side(&p), side(&q));
        if sp >= 0.0 {
            out.push(p);
        }
        if (sp >= 0.0) != (sq >= 0.0) {
            let t = sp / (sp - sq);
            out.push(p + (q - p) * t);
        }
    }
    out
}

/// Intersección de dos triángulos coplanares (polígono convexo)
fn clip_triangles(a: &[Vector3; 3], b: &[Vector3; 3], n: Vector3, to2: &impl Fn(&Vector3) -> (Real, Real)) -> Option<Vec<Vector3>> {
    let _ = to2;
    let mut poly: Vec<Vector3> = a.to_vec();
    // Orientación de b respecto a n para que "dentro" sea la izquierda
    let orient = (b[1] - b[0]).cross(&(b[2] - b[0])).dot(&n).signum();
    for i in 0..3 {
        let (p, q) = (b[i], b[(i + 1) % 3]);
        let edge = q - p;
        poly = clip_half_plane(&poly, |x| orient * edge.cross(&(*x - p)).dot(&n));
        if poly.len() < 3 {
            return None;
        }
    }
    Some(poly)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::printability::tests::make_box;
    use crate::slicer::{slice_by_plane, Plane};

    #[test]
    fn contacto_de_una_caja_cortada() {
        let b = make_box(40.0, 40.0, 40.0);
        let (x, y) = slice_by_plane(&b, &Plane::xy_at_z(15.0)).unwrap();
        let zmin = |m: &Mesh| m.vertices.iter().map(|v| v.position.z()).fold(Real::INFINITY, Real::min);
        let (lo, hi) = if zmin(&x) < zmin(&y) { (x, y) } else { (y, x) };
        let pieces = vec![
            LabeledPiece { mesh: lo, id: 0, ..Default::default() },
            LabeledPiece { mesh: hi, id: 1, ..Default::default() },
        ];
        let cs = find_contacts(&pieces, 1.0, 1e-4);
        assert_eq!(cs.len(), 1, "{cs:?}");
        let c = &cs[0];
        assert!((c.area - 1600.0).abs() < 1e-3, "área {}", c.area);
        assert!((c.normal.z().abs() - 1.0).abs() < 1e-9);
        // Normal de la pieza de abajo (A) hacia arriba
        assert!(c.normal.z() > 0.0 && (c.offset - 15.0).abs() < 1e-6);
        assert!((c.center.x() - 20.0).abs() < 1e-3 && (c.center.y() - 20.0).abs() < 1e-3);
    }

    #[test]
    fn piezas_lejanas_no_se_tocan() {
        let a = make_box(10.0, 10.0, 10.0);
        let mut b = make_box(10.0, 10.0, 10.0);
        for v in &mut b.vertices {
            v.position += Vector3::new(0.0, 0.0, 50.0);
        }
        let pieces = vec![LabeledPiece { mesh: a, id: 0, ..Default::default() }, LabeledPiece { mesh: b, id: 1, ..Default::default() }];
        assert!(find_contacts(&pieces, 1.0, 1e-4).is_empty());
    }
}

#[cfg(test)]
mod cut_tests {
    use crate::analysis::compute_volume;
    use crate::printability::tests::make_box;
    use crate::slicer::{slice_by_planes, Plane};
    use pinocchio_math::Vector3;
    use pinocchio_mesh::Mesh;

    #[test]
    fn cortes_multiples_dan_piezas_cerradas_que_suman_el_volumen() {
        let b = make_box(60.0, 40.0, 30.0);
        let planes = [Plane::yz_at_x(20.0), Plane::yz_at_x(40.0), Plane::xz_at_y(20.0), Plane::xy_at_z(15.0)];
        let pieces = slice_by_planes(&b, &planes).unwrap();
        assert_eq!(pieces.len(), 12);
        let mut total = 0.0;
        for p in &pieces {
            assert!(p.is_closed(), "pieza abierta");
            total += compute_volume(p);
        }
        assert!((total - 72000.0).abs() < 1e-6, "{total}");
    }

    #[test]
    fn corte_de_un_prisma_en_l_cerrado() {
        // Prisma en L (cóncavo) extruido 10 en Z; el corte atraviesa los dos brazos
        let l = [(0., 0.), (30., 0.), (30., 10.), (10., 10.), (10., 30.), (0., 30.)];
        let n = l.len();
        let mut pos: Vec<Vector3> = l.iter().map(|p| Vector3::new(p.0, p.1, 0.0)).collect();
        pos.extend(l.iter().map(|p| Vector3::new(p.0, p.1, 10.0)));
        let mut tris = Vec::new();
        // lados
        for i in 0..n {
            let j = (i + 1) % n;
            tris.push([i, j, n + j]);
            tris.push([i, n + j, n + i]);
        }
        // tapas por triangulación en abanico no sirve en cóncavo: usar dos rectángulos
        // base: [0,1,2,(3)] y [3,4,5,(0)] — se hace a mano
        for t in [[0, 3, 2], [0, 2, 1], [0, 5, 4], [0, 4, 3]] {
            tris.push(t);
        }
        for t in [[0, 2, 3], [0, 1, 2], [0, 4, 5], [0, 3, 4]] {
            tris.push([t[0] + n, t[1] + n, t[2] + n]);
        }
        let m = Mesh::from_triangles(&pos, &tris);
        assert!(m.is_closed());
        let vol = compute_volume(&m);
        assert!((vol - 500.0 * 10.0).abs() < 1e-6, "{vol}");
        // corte diagonal: x = y... usar plano x = 5 y y = 5, que atraviesan los dos brazos
        let pieces = slice_by_planes(&m, &[Plane::yz_at_x(5.0), Plane::xz_at_y(5.0)]).unwrap();
        let mut total = 0.0;
        for p in &pieces {
            assert!(p.is_closed());
            total += compute_volume(p);
        }
        assert!((total - vol).abs() < 1e-6, "{total} vs {vol}");
    }
}
