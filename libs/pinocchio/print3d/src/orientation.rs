//! Orientación para imprimir: elegir la cara de apoyo y asentar la pieza en la cama
//!
//! Se prueban como cara de apoyo los grupos de caras con la misma normal (los
//! más extensos) y los seis ejes, y se puntúa cada uno por voladizos, área de
//! contacto y altura. La malla se espera con Z arriba.

use std::collections::HashMap;

use pinocchio_math::{Real, Vector3};
use pinocchio_mesh::Mesh;
use pinocchio_spatial::{Bvh, Triangle};
use serde::{Deserialize, Serialize};

use crate::error::{Print3dError, Result};
use crate::geom::{normal_area, triangles};
use crate::printability::PrintabilityConfig;

/// Una orientación posible y su puntuación (menor es mejor)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrientationCandidate {
    /// Normal exterior de la cara que queda abajo, en el sistema de la malla
    pub down: [Real; 3],
    /// Área en voladizo (mm²)
    pub overhang_area: Real,
    /// Área apoyada en la cama (mm²)
    pub contact_area: Real,
    /// Altura total con esta orientación (mm)
    pub height: Real,
    /// Cota superior del volumen de soporte (mm³)
    pub support_volume_max: Real,
    /// Volumen de soporte estimado (mm³): cada columna baja hasta lo primero que hay debajo
    pub support_volume: Real,
    pub score: Real,
    /// Medidas con esta orientación: ancho y fondo de la huella (en el giro mínimo que la deja abajo) y alto
    #[serde(default)]
    pub dims: [Real; 3],
    /// Cabe en el volumen pedido (siempre `true` si no se pidió ninguno)
    #[serde(default)]
    pub fits: bool,
}

struct Face {
    n: Vector3,
    area: Real,
    center: Vector3,
    /// Menor y mayor proyección de los vértices sobre cada eje se calcula al evaluar
    verts: [Vector3; 3],
}

fn faces_of(mesh: &Mesh) -> Vec<Face> {
    let (pos, tris) = triangles(mesh);
    tris.iter()
        .filter_map(|t| {
            let (a, b, c) = (pos[t[0]], pos[t[1]], pos[t[2]]);
            normal_area(a, b, c).map(|(n, area)| Face { n, area, center: (a + b + c) * (1.0 / 3.0), verts: [a, b, c] })
        })
        .collect()
}

fn evaluate(faces: &[Face], d: Vector3, config: &PrintabilityConfig, diag: Real, total_area: Real) -> OrientationCandidate {
    let sin_max = config.max_overhang_deg.to_radians().sin();
    // Altura sobre la cama de un punto p: −d·p + const; el punto más bajo es el de mayor d·p
    let (mut lo, mut hi) = (Real::INFINITY, Real::NEG_INFINITY);
    for f in faces {
        for v in &f.verts {
            let s = d.dot(v);
            lo = lo.min(s);
            hi = hi.max(s);
        }
    }
    let height = hi - lo;
    let (mut overhang_area, mut contact_area, mut support) = (0.0, 0.0, 0.0);
    for f in faces {
        let facing = d.dot(&f.n);
        if facing <= sin_max {
            continue;
        }
        let lowest = f.verts.iter().map(|v| d.dot(v)).fold(Real::INFINITY, Real::min);
        if lowest >= hi - config.bed_tolerance {
            contact_area += f.area;
        } else {
            overhang_area += f.area;
            support += f.area * facing * (hi - d.dot(&f.center));
        }
    }
    let score = overhang_area - 0.5 * contact_area + 0.1 * total_area * (height / diag.max(1e-9));
    OrientationCandidate { down: [d.x(), d.y(), d.z()], overhang_area, contact_area, height, support_volume_max: support, support_volume: support, score, dims: [0.0; 3], fits: true }
}

/// Las mejores orientaciones (hasta `max`), de mejor a peor
pub fn suggest_orientations(mesh: &Mesh, config: &PrintabilityConfig, max: usize) -> Result<Vec<OrientationCandidate>> {
    suggest_orientations_in(mesh, config, max, None)
}

/// Medidas de la malla con la cara de normal `d` abajo: huella (ancho, fondo) y alto
fn oriented_dims(faces: &[Face], d: Vector3) -> [Real; 3] {
    let rot = rotation_to_down(d);
    let (mut lo, mut hi) = ([Real::INFINITY; 3], [Real::NEG_INFINITY; 3]);
    for f in faces {
        for v in &f.verts {
            let p = apply(&rot, *v);
            for (k, c) in [p.x(), p.y(), p.z()].into_iter().enumerate() {
                lo[k] = lo[k].min(c);
                hi[k] = hi[k].max(c);
            }
        }
    }
    [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]]
}

/// Como [`suggest_orientations`], pero con un volumen de impresión (ancho, fondo, alto): las orientaciones en que la
/// pieza cabe (girándola sobre la cama) van primero, y `fits` dice cuáles
pub fn suggest_orientations_in(mesh: &Mesh, config: &PrintabilityConfig, max: usize, volume: Option<[Real; 3]>) -> Result<Vec<OrientationCandidate>> {
    let faces = faces_of(mesh);
    if faces.is_empty() {
        return Err(Print3dError::EmptyMesh);
    }
    let total_area: Real = faces.iter().map(|f| f.area).sum();
    let diag = {
        let (mut lo, mut hi) = (faces[0].verts[0], faces[0].verts[0]);
        for f in &faces {
            for v in &f.verts {
                lo = lo.min(v);
                hi = hi.max(v);
            }
        }
        (hi - lo).length()
    };

    // Grupos de normales parecidas (celdas de ~7°) ponderados por área
    let mut groups: HashMap<(i32, i32, i32), (Vector3, Real)> = HashMap::new();
    for f in &faces {
        let key = ((f.n.x() * 8.0).round() as i32, (f.n.y() * 8.0).round() as i32, (f.n.z() * 8.0).round() as i32);
        let e = groups.entry(key).or_insert((Vector3::zero(), 0.0));
        e.0 += f.n * f.area;
        e.1 += f.area;
    }
    let mut ranked: Vec<(Vector3, Real)> = groups.into_values().collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut dirs: Vec<Vector3> = ranked.iter().take(24).filter_map(|(s, _)| s.try_normalize()).collect();
    for axis in [Vector3::unit_x(), Vector3::unit_y(), Vector3::unit_z()] {
        dirs.push(axis);
        dirs.push(-axis);
    }
    // Sin repetidos (direcciones casi iguales)
    let mut unique: Vec<Vector3> = Vec::new();
    for d in dirs {
        if !unique.iter().any(|u| u.dot(&d) > 0.9995) {
            unique.push(d);
        }
    }
    let mut out: Vec<OrientationCandidate> = unique.into_iter().map(|d| evaluate(&faces, d, config, diag, total_area)).collect();
    out.sort_by(|a, b| a.score.total_cmp(&b.score));
    // Los mejores se afinan con el soporte real (rayos hacia la cama) y se reordenan
    let refine = out.len().min(8);
    let bvh = Bvh::build(faces.iter().map(|f| Triangle::new(f.verts[0], f.verts[1], f.verts[2])).collect());
    let sin_max = config.max_overhang_deg.to_radians().sin();
    for cand in out.iter_mut().take(refine) {
        let d = Vector3::new(cand.down[0], cand.down[1], cand.down[2]);
        let hi = faces.iter().flat_map(|f| f.verts.iter()).map(|v| d.dot(v)).fold(Real::NEG_INFINITY, Real::max);
        let overhangs: Vec<(Vector3, Real, Real)> = faces
            .iter()
            .filter(|f| {
                let lowest = f.verts.iter().map(|v| d.dot(v)).fold(Real::INFINITY, Real::min);
                d.dot(&f.n) > sin_max && lowest < hi - config.bed_tolerance
            })
            .map(|f| (f.center, f.area * d.dot(&f.n), hi - d.dot(&f.center)))
            .collect();
        cand.support_volume = crate::printability::support_estimate(&bvh, &overhangs, d, diag);
        cand.score += cand.support_volume / (0.25 * diag.max(1e-9));
    }
    out[..refine].sort_by(|a, b| a.score.total_cmp(&b.score));
    // Medidas de cada orientación y si caben; las que caben van antes (el orden de puntaje se conserva)
    for cand in &mut out {
        let d = Vector3::new(cand.down[0], cand.down[1], cand.down[2]);
        cand.dims = oriented_dims(&faces, d);
        cand.fits = volume.is_none_or(|v| {
            let [w, depth, h] = cand.dims;
            h <= v[2] + 1e-9 && ((w <= v[0] + 1e-9 && depth <= v[1] + 1e-9) || (depth <= v[0] + 1e-9 && w <= v[1] + 1e-9))
        });
    }
    if volume.is_some() {
        out.sort_by_key(|c| !c.fits);
    }
    out.truncate(max.max(1));
    Ok(out)
}

/// Gira la malla para que la cara de normal exterior `down` quede abajo, y la asienta
/// en la cama (`z_min = 0`, centrada en X e Y sobre el origen)
pub fn orient_with_down(mesh: &mut Mesh, down: [Real; 3]) -> Result<()> {
    let d = Vector3::new(down[0], down[1], down[2]).try_normalize().ok_or_else(|| Print3dError::InvalidConfig("dirección nula".into()))?;
    let rot = rotation_to_down(d);
    for v in &mut mesh.vertices {
        v.position = apply(&rot, v.position);
        v.normal = apply(&rot, v.normal);
    }
    settle_on_bed(mesh);
    Ok(())
}

/// Elige la mejor orientación y la aplica
pub fn orient_for_print(mesh: &mut Mesh, config: &PrintabilityConfig) -> Result<OrientationCandidate> {
    orient_for_print_in(mesh, config, None)
}

/// Como [`orient_for_print`], preferiendo las orientaciones en que la pieza cabe en `volume`
pub fn orient_for_print_in(mesh: &mut Mesh, config: &PrintabilityConfig, volume: Option<[Real; 3]>) -> Result<OrientationCandidate> {
    let best = suggest_orientations_in(mesh, config, 1, volume)?.remove(0);
    orient_with_down(mesh, best.down)?;
    Ok(best)
}

/// Apoya la malla en la cama (`z_min = 0`) y la centra en X e Y sobre el origen
pub fn settle_on_bed(mesh: &mut Mesh) {
    if mesh.vertices.is_empty() {
        return;
    }
    let (mut lo, mut hi) = (mesh.vertices[0].position, mesh.vertices[0].position);
    for v in &mesh.vertices {
        lo = lo.min(&v.position);
        hi = hi.max(&v.position);
    }
    let shift = Vector3::new(-(lo.x() + hi.x()) * 0.5, -(lo.y() + hi.y()) * 0.5, -lo.z());
    for v in &mut mesh.vertices {
        v.position += shift;
    }
}

type Rot = [[Real; 3]; 3];

fn apply(r: &Rot, p: Vector3) -> Vector3 {
    Vector3::new(
        r[0][0] * p.x() + r[0][1] * p.y() + r[0][2] * p.z(),
        r[1][0] * p.x() + r[1][1] * p.y() + r[1][2] * p.z(),
        r[2][0] * p.x() + r[2][1] * p.y() + r[2][2] * p.z(),
    )
}

/// Rotación (Rodrigues) que lleva `d` (unitario) a −Z
fn rotation_to_down(d: Vector3) -> Rot {
    let t = Vector3::new(0.0, 0.0, -1.0);
    let cos = d.dot(&t);
    let axis = d.cross(&t);
    let sin = axis.length();
    if sin < 1e-12 {
        return if cos > 0.0 { [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] } else { [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, -1.0]] };
    }
    let k = axis * (1.0 / sin);
    let (kx, ky, kz) = (k.x(), k.y(), k.z());
    let c1 = 1.0 - cos;
    [
        [cos + kx * kx * c1, kx * ky * c1 - kz * sin, kx * kz * c1 + ky * sin],
        [ky * kx * c1 + kz * sin, cos + ky * ky * c1, ky * kz * c1 - kx * sin],
        [kz * kx * c1 - ky * sin, kz * ky * c1 + kx * sin, cos + kz * kz * c1],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::compute_bounding_box;
    use crate::printability::tests::make_box;

    #[test]
    fn losa_se_apoya_plana() {
        // Losa 10x40x60 de pie sobre su lado de 10x60 (alta en Z)
        let mut m = make_box(10.0, 60.0, 40.0);
        let before = crate::analysis::compute_volume(&m);
        let best = orient_for_print(&mut m, &PrintabilityConfig::default()).unwrap();
        let d = compute_bounding_box(&m).dimensions();
        assert!((d[2] - 10.0).abs() < 1e-6, "altura {d:?}");
        assert!(best.overhang_area < 1e-9);
        assert!((best.contact_area - 2400.0).abs() < 1e-6);
        let bb = compute_bounding_box(&m);
        assert!(bb.min[2].abs() < 1e-9);
        assert!((crate::analysis::compute_volume(&m) - before).abs() < 1e-6, "el giro conserva el volumen");
    }

    #[test]
    fn cono_con_la_punta_arriba() {
        // Pirámide de base cuadrada con la punta hacia −Z (mala): debe voltearse
        let v = vec![
            Vector3::new(-10.0, -10.0, 20.0),
            Vector3::new(10.0, -10.0, 20.0),
            Vector3::new(10.0, 10.0, 20.0),
            Vector3::new(-10.0, 10.0, 20.0),
            Vector3::new(0.0, 0.0, 0.0),
        ];
        let f = vec![[0, 2, 1], [0, 3, 2], [0, 1, 4], [1, 2, 4], [2, 3, 4], [3, 0, 4]];
        // Normales exteriores: base mira a +Z
        let mut m = Mesh::from_triangles(&v, &f);
        // Comprobar que la base mira a +Z antes de orientar; si no, invertir
        let (pos, mut tris) = triangles(&m);
        let n = normal_area(pos[tris[0][0]], pos[tris[0][1]], pos[tris[0][2]]).unwrap().0;
        if n.z() < 0.0 {
            for t in &mut tris {
                t.swap(1, 2);
            }
            m = Mesh::from_triangles(&pos, &tris);
        }
        let best = orient_for_print(&mut m, &PrintabilityConfig::default()).unwrap();
        assert!(best.overhang_area < 1e-6, "{best:?}");
        // La base (400 mm²) queda en la cama
        assert!((best.contact_area - 400.0).abs() < 1e-6, "{best:?}");
        let bb = compute_bounding_box(&m);
        assert!(bb.min[2].abs() < 1e-9 && (bb.max[2] - 20.0).abs() < 1e-6);
    }

    #[test]
    fn rotacion_lleva_d_abajo() {
        for d in [Vector3::unit_x(), Vector3::new(1.0, 2.0, -3.0).normalize(), -Vector3::unit_z(), Vector3::unit_z()] {
            let r = rotation_to_down(d);
            let o = apply(&r, d);
            assert!((o.x()).abs() < 1e-9 && (o.y()).abs() < 1e-9 && (o.z() + 1.0).abs() < 1e-9, "{d:?} -> {o:?}");
        }
    }

    #[test]
    fn prefiere_la_orientacion_que_cabe() {
        // Una barra de 10 × 10 × 120 en una impresora de altura 50: de pie no cabe (120 > 50), acostada sí
        let mut bar = make_box(10.0, 10.0, 120.0);
        let cfg = PrintabilityConfig::default();
        let volume = [200.0, 200.0, 50.0];
        let best = suggest_orientations_in(&bar, &cfg, 6, Some(volume)).unwrap();
        assert!(best[0].fits, "{:?}", best[0]);
        assert!(best[0].dims[2] <= 50.0 + 1e-9, "alto {}", best[0].dims[2]);
        // Sin volumen manda solo el puntaje: todas «caben»
        assert!(suggest_orientations(&bar, &cfg, 6).unwrap().iter().all(|c| c.fits));
        // La orientación elegida deja la barra baja
        let chosen = orient_for_print_in(&mut bar, &cfg, Some(volume)).unwrap();
        assert!(chosen.fits);
        let bb = compute_bounding_box(&bar);
        assert!(bb.dimensions()[2] <= 50.0 + 1e-9);
        // Una pieza que no cabe de ninguna forma lo dice
        let huge = make_box(300.0, 300.0, 300.0);
        assert!(suggest_orientations_in(&huge, &cfg, 6, Some(volume)).unwrap().iter().all(|c| !c.fits));
    }
}
