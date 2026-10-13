//! Dónde cortar: planos elegidos a mano y planos óptimos
//!
//! "Óptimo" quiere decir pocas piezas y cortes por donde el modelo es más angosto: las piezas pueden girarse
//! al imprimir, así que el eje más largo del modelo se reparte contra el espacio más largo de la impresora; y
//! dentro de lo que cabe, cada plano se coloca donde la sección es más chica (menos costura que unir y lijar).

use pinocchio_math::{Real, Vector3};
use pinocchio_mesh::Mesh;

use crate::analysis::BoundingBox;
use crate::config::CutPlane;
use crate::error::{Print3dError, Result};
use crate::geom::triangles;
use crate::slicer::Plane;

fn axis_plane(axis: usize, c: Real) -> Plane {
    match axis {
        0 => Plane::yz_at_x(c),
        1 => Plane::xz_at_y(c),
        _ => Plane::xy_at_z(c),
    }
}

/// Los planos de los cortes elegidos a mano. El plano pasa por la posición pedida sobre su eje y por el centro
/// del modelo en los otros dos; la inclinación gira su normal hacia el eje siguiente
pub fn manual_planes(bbox: &BoundingBox, cuts: &[CutPlane]) -> Result<Vec<Plane>> {
    if cuts.is_empty() {
        return Err(Print3dError::InvalidConfig("no hay ningún plano de corte".into()));
    }
    let dims = bbox.dimensions();
    let center = bbox.center();
    cuts.iter()
        .enumerate()
        .map(|(i, c)| {
            if c.axis > 2 {
                return Err(Print3dError::InvalidConfig(format!("el plano {} tiene un eje que no existe ({})", i + 1, c.axis)));
            }
            if !(c.position > 0.0 && c.position < dims[c.axis]) {
                return Err(Print3dError::InvalidConfig(format!("el plano {} queda fuera del modelo (a {:.1} de un largo de {:.1})", i + 1, c.position, dims[c.axis])));
            }
            if !(-80.0..=80.0).contains(&c.tilt_deg) {
                return Err(Print3dError::InvalidConfig(format!("la inclinación del plano {} va entre −80° y 80°", i + 1)));
            }
            let mut origin = [center.x(), center.y(), center.z()];
            origin[c.axis] = bbox.min[c.axis] + c.position;
            let t = c.tilt_deg.to_radians();
            let mut n = [0.0; 3];
            n[c.axis] = t.cos();
            n[(c.axis + 1) % 3] = t.sin();
            Ok(Plane::new(Vector3::new(origin[0], origin[1], origin[2]), Vector3::new(n[0], n[1], n[2])))
        })
        .collect()
}

/// Área de la sección de la malla cerrada por el plano `eje = c` (los huecos restan)
pub fn section_area(mesh: &Mesh, axis: usize, c: Real) -> Real {
    let (pos, tris) = triangles(mesh);
    section_area_of(&pos, &tris, axis, c)
}

fn section_area_of(pos: &[Vector3], tris: &[[usize; 3]], axis: usize, c: Real) -> Real {
    let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
    let coord = |p: &Vector3, k: usize| match k {
        0 => p.x(),
        1 => p.y(),
        _ => p.z(),
    };
    let mut total = 0.0;
    for t in tris {
        let q = [0, 1, 2].map(|k| coord(&pos[t[k]], axis) - c);
        if q.iter().all(|&x| x >= 0.0) || q.iter().all(|&x| x < 0.0) {
            continue;
        }
        let (mut entry, mut exit) = (None, None);
        for k in 0..3 {
            let (a, b) = (k, (k + 1) % 3);
            let cross = |a: usize, b: usize| {
                let s = q[a] / (q[a] - q[b]);
                let (pa, pb) = (&pos[t[a]], &pos[t[b]]);
                [coord(pa, u) + (coord(pb, u) - coord(pa, u)) * s, coord(pa, v) + (coord(pb, v) - coord(pa, v)) * s]
            };
            if q[a] < 0.0 && q[b] >= 0.0 {
                entry = Some(cross(a, b));
            } else if q[a] >= 0.0 && q[b] < 0.0 {
                exit = Some(cross(a, b));
            }
        }
        if let (Some(p), Some(r)) = (entry, exit) {
            total += (p[0] * r[1] - r[0] * p[1]) * 0.5;
        }
    }
    total.abs()
}

/// ¿Cabe `dims` en `volume` girando la pieza como se quiera (ejes permutados)?
pub fn fits_any_orientation(dims: [Real; 3], volume: [Real; 3]) -> bool {
    let mut d = dims;
    let mut v = volume;
    d.sort_by(|a, b| b.total_cmp(a));
    v.sort_by(|a, b| b.total_cmp(a));
    (0..3).all(|i| d[i] <= v[i] + 1e-9)
}

/// Cuántos cortes lleva cada eje del modelo con el reparto óptimo de ejes: el eje más largo del modelo
/// contra el espacio más largo de la impresora, y así. Devuelve también el largo máximo de pieza de cada eje
pub fn optimal_cut_counts(dims: [Real; 3], volume: [Real; 3]) -> ([usize; 3], [Real; 3]) {
    let rank = |a: &[Real; 3]| {
        let mut idx = [0, 1, 2];
        idx.sort_by(|&i, &j| a[j].total_cmp(&a[i]));
        idx
    };
    let (od, ov) = (rank(&dims), rank(&volume));
    let mut limit = [0.0; 3];
    for k in 0..3 {
        limit[od[k]] = volume[ov[k]];
    }
    let counts = [0, 1, 2].map(|i| ((dims[i] / limit[i] - 1e-9).ceil().max(1.0) as usize) - 1);
    (counts, limit)
}

/// Planos que dan el menor número de piezas, cada uno donde la sección es más chica. Vacío si ya cabe
pub fn optimal_planes(mesh: &Mesh, bbox: &BoundingBox, volume: [Real; 3]) -> Vec<Plane> {
    let dims = bbox.dimensions();
    let (counts, limit) = optimal_cut_counts(dims, volume);
    let (pos, tris) = triangles(mesh);
    let mut planes = Vec::new();
    for axis in 0..3 {
        let n = counts[axis];
        if n == 0 {
            continue;
        }
        let (lo, len, l) = (bbox.min[axis], dims[axis], limit[axis]);
        let even = |k: usize| lo + len * k as Real / (n + 1) as Real;
        let positions = best_positions(n, lo, len, l, |c| section_area_of(&pos, &tris, axis, c)).unwrap_or_else(|| (1..=n).map(even).collect());
        planes.extend(positions.into_iter().map(|c| axis_plane(axis, c)));
    }
    planes
}

/// `n` posiciones en `[lo, lo + len]` con piezas de a lo más `l` de largo (y no menos de un décimo de `l`) que
/// minimizan la suma de `area` más una pequeña preferencia por el reparto parejo. `None` si no hay solución
fn best_positions(n: usize, lo: Real, len: Real, l: Real, area: impl Fn(Real) -> Real) -> Option<Vec<Real>> {
    let m = (24 * (n + 1)).clamp(48, 400);
    let cand: Vec<Real> = (0..m).map(|j| lo + len * (j as Real + 0.5) / m as Real).collect();
    let areas: Vec<Real> = cand.iter().map(|&c| area(c)).collect();
    let mean = (areas.iter().sum::<Real>() / m as Real).max(1e-12);
    let min_piece = 0.1 * l;
    let cost = |k: usize, j: usize| {
        let target = lo + len * (k + 1) as Real / (n + 1) as Real;
        areas[j] / mean + 0.3 * ((cand[j] - target) / l).powi(2)
    };
    let mut best = vec![vec![Real::INFINITY; m]; n];
    let mut from = vec![vec![usize::MAX; m]; n];
    for j in 0..m {
        if cand[j] - lo <= l && cand[j] - lo >= min_piece {
            best[0][j] = cost(0, j);
        }
    }
    for k in 1..n {
        for j in 0..m {
            for i in 0..m {
                let gap = cand[j] - cand[i];
                if best[k - 1][i].is_finite() && gap <= l && gap >= min_piece {
                    let c = best[k - 1][i] + cost(k, j);
                    if c < best[k][j] {
                        best[k][j] = c;
                        from[k][j] = i;
                    }
                }
            }
        }
    }
    let end = lo + len;
    let last = (0..m).filter(|&j| best[n - 1][j].is_finite() && end - cand[j] <= l && end - cand[j] >= min_piece).min_by(|&a, &b| best[n - 1][a].total_cmp(&best[n - 1][b]))?;
    let mut out = vec![0.0; n];
    let mut j = last;
    for k in (0..n).rev() {
        out[k] = cand[j];
        if k > 0 {
            j = from[k][j];
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::compute_bounding_box;
    use crate::printability::tests::make_box;

    /// Superficie de revolución alrededor de X con radio `r(x)` (cerrada con tapas)
    fn revolve(length: Real, steps: usize, r: impl Fn(Real) -> Real) -> Mesh {
        let segs = 32;
        let mut pos = Vec::new();
        for i in 0..=steps {
            let x = length * i as Real / steps as Real;
            for j in 0..segs {
                let a = std::f64::consts::TAU * j as Real / segs as Real;
                pos.push(Vector3::new(x, r(x) * a.cos(), r(x) * a.sin()));
            }
        }
        let c0 = pos.len();
        pos.push(Vector3::new(0.0, 0.0, 0.0));
        pos.push(Vector3::new(length, 0.0, 0.0));
        let at = |i: usize, j: usize| i * segs + j % segs;
        let mut tris = Vec::new();
        for i in 0..steps {
            for j in 0..segs {
                tris.push([at(i, j), at(i, j + 1), at(i + 1, j + 1)]);
                tris.push([at(i, j), at(i + 1, j + 1), at(i + 1, j)]);
            }
        }
        for j in 0..segs {
            tris.push([c0, at(0, j + 1), at(0, j)]);
            tris.push([c0 + 1, at(steps, j), at(steps, j + 1)]);
        }
        Mesh::from_triangles(&pos, &tris)
    }

    #[test]
    fn el_area_de_la_seccion_de_una_caja() {
        let b = make_box(20.0, 10.0, 5.0);
        assert!((section_area(&b, 0, 7.0) - 50.0).abs() < 1e-9);
        assert!((section_area(&b, 1, 3.0) - 100.0).abs() < 1e-9);
        assert!((section_area(&b, 2, 1.0) - 200.0).abs() < 1e-9);
        assert_eq!(section_area(&b, 0, 25.0), 0.0);
    }

    #[test]
    fn la_seccion_de_una_revolucion_es_un_circulo() {
        let m = revolve(100.0, 40, |_| 10.0);
        let poly = 0.5 * 32.0 * 100.0 * (std::f64::consts::TAU / 32.0).sin();
        assert!((section_area(&m, 0, 33.3) - poly).abs() < 1e-6);
    }

    #[test]
    fn el_corte_optimo_cae_en_el_cuello() {
        // Mancuerna de 300 mm con un cuello fino entre 110 y 130
        let m = revolve(300.0, 300, |x| if (110.0..=130.0).contains(&x) { 3.0 } else { 20.0 });
        let bb = compute_bounding_box(&m);
        // Cama de 250: una pieza no puede pasar de 250; el reparto parejo cortaría en 150
        let planes = optimal_planes(&m, &bb, [250.0, 250.0, 250.0]);
        assert_eq!(planes.len(), 1);
        let x = planes[0].origin[0];
        assert!((110.0..=130.0).contains(&x), "el plano cayó en {x}");
        let grid_x = 150.0;
        assert!(section_area(&m, 0, x) < 0.1 * section_area(&m, 0, grid_x));
    }

    #[test]
    fn las_piezas_pueden_girarse_para_caber() {
        // 300 × 150 × 40 en una cama de 160 × 320 × 100: la rejilla corta en X, pero girada cabe entera
        assert!(fits_any_orientation([300.0, 150.0, 40.0], [160.0, 320.0, 100.0]));
        assert!(!fits_any_orientation([300.0, 150.0, 40.0], [160.0, 160.0, 100.0]));
        let (counts, limit) = optimal_cut_counts([300.0, 150.0, 40.0], [160.0, 320.0, 100.0]);
        assert_eq!(counts, [0, 0, 0]);
        assert_eq!(limit, [320.0, 160.0, 100.0]);
        // Mismo modelo en una cama de 160 × 160 × 100: solo el largo necesita un corte
        let (counts, _) = optimal_cut_counts([300.0, 150.0, 40.0], [160.0, 160.0, 100.0]);
        assert_eq!(counts, [1, 0, 0]);
    }

    #[test]
    fn cortes_a_mano() {
        let b = make_box(20.0, 10.0, 10.0);
        let bb = compute_bounding_box(&b);
        let planes = manual_planes(&bb, &[CutPlane { axis: 0, position: 5.0, tilt_deg: 0.0 }, CutPlane { axis: 2, position: 4.0, tilt_deg: 20.0 }]).unwrap();
        assert_eq!(planes.len(), 2);
        assert!((planes[0].origin[0] - 5.0).abs() < 1e-12 && (planes[0].normal[0] - 1.0).abs() < 1e-12);
        // Inclinado: la normal gira hacia X (el eje que sigue a Z) y pasa por z = 4
        assert!((planes[1].origin[2] - 4.0).abs() < 1e-12 && planes[1].normal[0] > 0.3 && planes[1].normal[2] > 0.9);
        let pieces = crate::slicer::slice_by_planes(&b, &planes).unwrap();
        assert_eq!(pieces.len(), 4);
        assert!(pieces.iter().all(|p| p.is_closed()));
        let total: Real = pieces.iter().map(crate::compute_volume).sum();
        assert!((total - 2000.0).abs() < 1e-6);
        // Fuera del modelo, sin planos o con un eje que no existe
        assert!(manual_planes(&bb, &[CutPlane { axis: 0, position: 25.0, tilt_deg: 0.0 }]).is_err());
        assert!(manual_planes(&bb, &[]).is_err());
        assert!(manual_planes(&bb, &[CutPlane { axis: 3, position: 1.0, tilt_deg: 0.0 }]).is_err());
        assert!(manual_planes(&bb, &[CutPlane { axis: 0, position: 5.0, tilt_deg: 85.0 }]).is_err());
    }

    #[test]
    fn subdividir_con_corte_optimo_y_manual() {
        use crate::config::{SubdivideConfig, SubdivideStrategy};
        let m = revolve(300.0, 300, |x| if (110.0..=130.0).contains(&x) { 3.0 } else { 20.0 });
        let base = SubdivideConfig { build_volume: [250.0, 250.0, 250.0], margin: 0.0, ..Default::default() };
        let optimal = crate::slicer::subdivide(&m, &SubdivideConfig { strategy: SubdivideStrategy::Optimal, ..base.clone() }).unwrap();
        assert_eq!(optimal.len(), 2);
        assert!(optimal.iter().all(|p| p.mesh.is_closed()));
        let grid = crate::slicer::subdivide(&m, &SubdivideConfig { strategy: SubdivideStrategy::Grid, ..base.clone() }).unwrap();
        // La rejilla corta por el medio grueso; el óptimo, por el cuello: el área de la tapa es mucho menor
        let cap = |pieces: &[crate::LabeledPiece]| crate::find_contacts(pieces, 1e-3, 1e-4).iter().map(|c| c.area).fold(0.0, f64::max);
        assert!(cap(&optimal) < 0.1 * cap(&grid), "{} vs {}", cap(&optimal), cap(&grid));
        // Una caja de 300 × 100 × 100 en una cama de 100 × 100 × 320 cabe girada: «óptimo» no la parte, «rejilla» sí
        let long = make_box(300.0, 100.0, 100.0);
        let bed = SubdivideConfig { build_volume: [100.0, 100.0, 320.0], margin: 0.0, ..Default::default() };
        assert_eq!(crate::slicer::subdivide(&long, &SubdivideConfig { strategy: SubdivideStrategy::Optimal, ..bed.clone() }).unwrap().len(), 1);
        assert!(crate::slicer::subdivide(&long, &SubdivideConfig { strategy: SubdivideStrategy::Grid, ..bed.clone() }).unwrap().len() > 1);
        // A mano: dos cortes en X → tres piezas; sin planos, error
        let manual = SubdivideConfig {
            strategy: SubdivideStrategy::Manual,
            manual: vec![CutPlane { axis: 0, position: 100.0, tilt_deg: 0.0 }, CutPlane { axis: 0, position: 200.0, tilt_deg: 0.0 }],
            ..bed.clone()
        };
        let pieces = crate::slicer::subdivide(&long, &manual).unwrap();
        assert_eq!(pieces.len(), 3);
        assert!(pieces.iter().all(|p| p.mesh.is_closed()));
        assert!(crate::slicer::subdivide(&long, &SubdivideConfig { manual: vec![], ..manual }).is_err());
        // Un corte a mano parte aunque el modelo ya quepa
        let small = SubdivideConfig { build_volume: [1000.0; 3], strategy: SubdivideStrategy::Manual, manual: vec![CutPlane { axis: 2, position: 50.0, tilt_deg: 0.0 }], margin: 0.0, ..Default::default() };
        assert_eq!(crate::slicer::subdivide(&long, &small).unwrap().len(), 2);
    }
}
