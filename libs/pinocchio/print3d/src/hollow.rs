//! Vaciado (cáscara) y estimación de material
//!
//! El exterior se conserva tal cual; el hueco sale de la superficie desplazada
//! hacia adentro el espesor de pared, reconstruida por vóxeles, e invertida.
//! Las partes más finas que el doble de la pared quedan macizas.

use pinocchio_math::{Real, Vector3};
use pinocchio_mesh::Mesh;
use quadriflow_core::remesh::{voxel, VoxelOptions};
use serde::{Deserialize, Serialize};

use crate::analysis::compute_volume;
use crate::error::{Print3dError, Result};
use crate::geom::triangles;
use crate::profile::MaterialProfile;

/// Parámetros del vaciado
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HollowConfig {
    /// Espesor de pared (mm)
    pub wall: Real,
    /// Lado del vóxel (mm); `None`: la pared entre 2,5 (con un mínimo para limitar la grilla)
    pub voxel_size: Option<Real>,
    /// Agujero de drenaje en la cara plana de abajo, con este diámetro (mm). Pide una cara
    /// plana en `z` mínimo; para resina, donde el hueco cerrado atrapa la resina
    pub drain_diameter: Option<Real>,
    /// Segundo agujero (venteo) del mismo diámetro, lejos del de drenaje, para que el aire entre al sacar la
    /// resina. Solo con `drain_diameter`
    #[serde(default)]
    pub vent: bool,
}

impl Default for HollowConfig {
    fn default() -> Self {
        Self { wall: 2.0, voxel_size: None, drain_diameter: None, vent: false }
    }
}

/// Resultado del vaciado
#[derive(Debug, Clone)]
pub struct HollowResult {
    pub mesh: Mesh,
    /// Volumen del hueco (mm³)
    pub cavity_volume: Real,
    /// Volumen del sólido original (mm³)
    pub original_volume: Real,
    /// Fracción de material ahorrada (0..1)
    pub saved_fraction: Real,
    /// Avisos (por ejemplo, que se aplanó la base para poder poner el agujero)
    pub notes: Vec<String>,
}

/// Vacía una malla cerrada dejando paredes de `config.wall`
pub fn hollow(mesh: &Mesh, config: &HollowConfig) -> Result<HollowResult> {
    if !(config.wall.is_finite() && config.wall > 0.0) {
        return Err(Print3dError::InvalidConfig("el espesor de pared debe ser positivo".into()));
    }
    if mesh.vertices.is_empty() {
        return Err(Print3dError::EmptyMesh);
    }
    if !mesh.is_closed() {
        return Err(Print3dError::MeshNotClosed);
    }
    let mut notes = Vec::new();
    // Con agujero, hace falta una cara plana abajo donde quepa: si no la hay se aplana la base
    let mut count = 0;
    let flattened;
    let mesh = match config.drain_diameter {
        Some(d) => {
            count = if config.vent { 2 } else { 1 };
            flattened = prepare_base(mesh, d, config.wall, count, &mut notes)?;
            &flattened
        }
        None => mesh,
    };
    let (pos, tris) = triangles(mesh);
    let original_volume = compute_volume(mesh);
    let positions: Vec<[f32; 3]> = pos.iter().map(|p| [p.x() as f32, p.y() as f32, p.z() as f32]).collect();
    let indices: Vec<u32> = tris.iter().flat_map(|t| t.map(|i| i as u32)).collect();

    let voxel_size = config.voxel_size.unwrap_or(config.wall / 2.5).max(1e-6);
    let options = VoxelOptions { voxel_size, smooth_iterations: 2, isotropic_edge: Some(voxel_size * 1.5), offset: -config.wall, thickness: 0.0 };
    let cavity = voxel(&positions, &indices, &options).map_err(|e| match e {
        quadriflow_core::RemeshError::EmptyMesh => Print3dError::InvalidConfig("la pared es demasiado gruesa: no queda hueco".into()),
        other => Print3dError::GeometryError(other.to_string()),
    })?;

    // Exterior + hueco con las caras al revés (miran al vacío)
    let cav_pos: Vec<Vector3> = cavity.positions.iter().map(|p| Vector3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]))).collect();
    let cav_tris: Vec<[usize; 3]> = cavity.indices.as_chunks::<3>().0.iter().map(|t| [t[0] as usize, t[2] as usize, t[1] as usize]).collect();
    let (all, all_tris) = match config.drain_diameter {
        Some(d) => {
            let (all, all_tris, placed) = drill_drain(mesh, &pos, &tris, &cav_pos, &cav_tris, d, config.wall, count)?;
            if placed < count {
                notes.push("solo cupo un agujero en la base: no hay venteo".to_string());
            }
            (all, all_tris)
        }
        None => {
            let mut all = pos.clone();
            let mut all_tris = tris.clone();
            let base = all.len();
            all.extend(cav_pos);
            all_tris.extend(cav_tris.iter().map(|t| [base + t[0], base + t[1], base + t[2]]));
            (all, all_tris)
        }
    };
    let all_tris = if config.drain_diameter.is_some() { crate::planar_joints::flip_slivers(&all, all_tris) } else { all_tris };
    let result = Mesh::from_triangles(&all, &all_tris);

    let new_volume = compute_signed_volume(&result);
    let cavity_volume = (original_volume - new_volume).max(0.0);
    Ok(HollowResult { mesh: result, cavity_volume, original_volume, saved_fraction: if original_volume > 0.0 { cavity_volume / original_volume } else { 0.0 }, notes })
}

/// La cara plana de abajo (en `z` mínimo, mirando hacia abajo) como región de contacto
fn bottom_face(pos: &[Vector3], tris: &[[usize; 3]]) -> Option<crate::contact::Contact> {
    use crate::geom::normal_area;
    let n = Vector3::new(0.0, 0.0, -1.0);
    let zmin = pos.iter().map(|p| p.z()).fold(Real::INFINITY, Real::min);
    let ptol = (crate::geom::diagonal(pos) * 1e-6).max(1e-7);
    let mut region: Vec<Vec<Vector3>> = Vec::new();
    let mut area = 0.0;
    let mut centroid = Vector3::zero();
    for t in tris {
        let ps = [pos[t[0]], pos[t[1]], pos[t[2]]];
        if ps.iter().all(|p| (p.z() - zmin).abs() <= ptol)
            && let Some((tn, a)) = normal_area(ps[0], ps[1], ps[2])
            && tn.dot(&n) > 0.999
        {
            area += a;
            centroid += (ps[0] + ps[1] + ps[2]) * (a / 3.0);
            region.push(ps.to_vec());
        }
    }
    if region.is_empty() {
        return None;
    }
    Some(crate::contact::Contact { piece_a: 0, piece_b: 0, normal: n, offset: -zmin, area, center: centroid * (1.0 / area), region })
}

/// Dónde caben `count` agujeros de `diameter` en la cara plana de abajo (vacío si no caben)
fn drain_spots(pos: &[Vector3], tris: &[[usize; 3]], diameter: Real, wall: Real, count: usize) -> Vec<Vector3> {
    use crate::planar_joints::{choose_spots, PlanarJointConfig, PlanarShape};
    let Some(contact) = bottom_face(pos, tris) else { return Vec::new() };
    let config = PlanarJointConfig { shape: PlanarShape::Round { diameter }, edge_margin: (wall * 2.0).max(1.0), count, ..Default::default() };
    choose_spots(&contact, &config)
}

/// Deja una base plana donde quepan los agujeros: si la pieza no tiene cara plana abajo (o es muy chica) le quita
/// la lámina más delgada que lo logre. Avisa en `notes` cuánto se quitó
fn prepare_base(mesh: &Mesh, diameter: Real, wall: Real, count: usize, notes: &mut Vec<String>) -> Result<Mesh> {
    use crate::slicer::{slice_by_plane, Plane};
    let (pos, tris) = triangles(mesh);
    let found = drain_spots(&pos, &tris, diameter, wall, count).len();
    if found >= count {
        return Ok(mesh.clone());
    }
    let zmin = pos.iter().map(|p| p.z()).fold(Real::INFINITY, Real::min);
    let zmax = pos.iter().map(|p| p.z()).fold(Real::NEG_INFINITY, Real::max);
    let limit = (zmax - zmin) * 0.3;
    let top_of = |m: &Mesh| m.vertices.iter().map(|v| v.position.z()).fold(Real::NEG_INFINITY, Real::max);
    // La mejor base con todos los agujeros; si ninguna sirve para todos, la primera que sirva para uno
    let mut fallback: Option<(Mesh, Real, usize)> = None;
    let mut h = (diameter * 0.05).max(0.1);
    while h <= limit {
        if let Ok((a, b)) = slice_by_plane(mesh, &Plane::xy_at_z(zmin + h)) {
            let upper = if top_of(&a) > top_of(&b) { a } else { b };
            let (up_pos, up_tris) = triangles(&upper);
            let got = drain_spots(&up_pos, &up_tris, diameter, wall, count).len();
            if got >= count {
                notes.push(format!("se aplanó la base quitando {h:.2} mm para poder poner el agujero"));
                return Ok(upper);
            }
            if got >= 1 && fallback.is_none() {
                fallback = Some((upper, h, got));
            }
        }
        h *= 1.3;
    }
    match fallback {
        Some((m, h, _)) => {
            notes.push(format!("se aplanó la base quitando {h:.2} mm para poder poner el agujero"));
            Ok(m)
        }
        None => Err(Print3dError::JointError("no hay dónde poner el agujero de drenaje: la pieza no tiene una base donde quepa ni quitando hasta el 30 % de su alto".to_string())),
    }
}

/// Agujeros de drenaje (y venteo): un túnel por cada uno desde la cara plana de abajo hasta el suelo del hueco
///
/// Se abre la tapa plana con las huellas (recorte exacto), se quita el suelo del hueco bajo ellas y
/// el borde de cada par de agujeros se une con una banda de triángulos. Devuelve cuántos agujeros se hicieron
#[allow(clippy::too_many_arguments)]
fn drill_drain(
    outer: &Mesh,
    pos: &[Vector3],
    tris: &[[usize; 3]],
    cav_pos: &[Vector3],
    cav_tris: &[[usize; 3]],
    diameter: Real,
    wall: Real,
    count: usize,
) -> Result<(Vec<Vector3>, Vec<[usize; 3]>, usize)> {
    use crate::geom::normal_area;
    use crate::planar_joints::{footprint_polygon, open_cap_multi, PlanarShape};
    use std::collections::HashMap;

    let err = |m: &str| Print3dError::JointError(m.to_string());
    let n = Vector3::new(0.0, 0.0, -1.0);
    if bottom_face(pos, tris).is_none() {
        return Err(err("para el drenaje hace falta una cara plana abajo: apoya el modelo en una cara plana"));
    }
    let spots = drain_spots(pos, tris, diameter, wall, count);
    if spots.is_empty() {
        return Err(err("la cara de abajo es muy chica para el agujero de drenaje"));
    }
    let polygons: Vec<Vec<Vector3>> = spots.iter().map(|&s| footprint_polygon(&PlanarShape::Round { diameter }, s, n, 0.0)).collect();

    // Tapa de abajo abierta, con un contorno por agujero
    let (mut all, mut all_tris, rings) = open_cap_multi(outer, &polygons, n)?;

    // Suelo del hueco bajo los agujeros: triángulos que miran hacia arriba y cuyo centro cae dentro de alguno
    let radius = diameter * 0.5;
    let near_spot = |c: Vector3| spots.iter().position(|s| ((c.x() - s.x()).powi(2) + (c.y() - s.y()).powi(2)).sqrt() < radius);
    let keep: Vec<[usize; 3]> = cav_tris
        .iter()
        .filter(|t| {
            let (a, b, c) = (cav_pos[t[0]], cav_pos[t[1]], cav_pos[t[2]]);
            let up = normal_area(a, b, c).is_some_and(|(tn, _)| tn.z() > 0.2);
            !(up && near_spot((a + b + c) * (1.0 / 3.0)).is_some())
        })
        .copied()
        .collect();
    if keep.len() == cav_tris.len() {
        return Err(err("no hay suelo de hueco sobre el agujero de drenaje"));
    }
    // Bordes de los agujeros del hueco: un lazo por agujero
    let mut directed: HashMap<(usize, usize), u32> = HashMap::new();
    for t in &keep {
        for i in 0..3 {
            *directed.entry((t[i], t[(i + 1) % 3])).or_default() += 1;
        }
    }
    let boundary: Vec<(usize, usize)> = directed.keys().filter(|&&(a, b)| !directed.contains_key(&(b, a))).copied().collect();
    let next: HashMap<usize, usize> = boundary.iter().copied().collect();
    if boundary.is_empty() || next.len() != boundary.len() {
        return Err(err("el agujero del hueco no es simple"));
    }
    let mut loops: Vec<Vec<usize>> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for &(start, _) in &boundary {
        if seen.contains(&start) {
            continue;
        }
        let mut lp = vec![start];
        seen.insert(start);
        let mut cur = next[&start];
        while cur != start {
            if !seen.insert(cur) {
                return Err(err("el borde del hueco no se cierra"));
            }
            lp.push(cur);
            cur = *next.get(&cur).ok_or_else(|| err("el borde del hueco está roto"))?;
        }
        loops.push(lp);
    }
    if loops.len() != spots.len() {
        return Err(err("el suelo del hueco quedó en varios agujeros"));
    }

    let base = all.len();
    all.extend(cav_pos.iter().copied());
    all_tris.extend(keep.iter().map(|t| [base + t[0], base + t[1], base + t[2]]));
    // Cada lazo del hueco con la tapa de su agujero
    for (k, spot) in spots.iter().enumerate() {
        let lp = loops
            .iter()
            .position(|l| {
                let c = l.iter().fold(Vector3::zero(), |s, &i| s + cav_pos[i]) * (1.0 / l.len() as Real);
                ((c.x() - spot.x()).powi(2) + (c.y() - spot.y()).powi(2)).sqrt() < radius
            })
            .ok_or_else(|| err("falta el borde del hueco bajo un agujero"))?;
        let a_seq: Vec<usize> = rings[k].iter().rev().copied().collect();
        let b_seq: Vec<usize> = loops[lp].iter().map(|&i| base + i).collect();
        stitch_band(&all, &mut all_tris, &a_seq, &b_seq, *spot)?;
    }
    Ok((all, all_tris, spots.len()))
}

/// Banda de triángulos entre el borde de la tapa (`a_seq`: aristas gemelas) y el del hueco (`b_seq`: su orden
/// original), ambos alrededor del eje de `spot`
fn stitch_band(all: &[Vector3], all_tris: &mut Vec<[usize; 3]>, a_seq: &[usize], b_seq: &[usize], spot: Vector3) -> Result<()> {
    let err = |m: &str| Print3dError::JointError(m.to_string());
    let angle = |i: usize| (all[i].y() - spot.y()).atan2(all[i].x() - spot.x());
    let signed_area = |seq: &[usize]| -> Real {
        let mut s = 0.0;
        for k in 0..seq.len() {
            let (p, q) = (all[seq[k]], all[seq[(k + 1) % seq.len()]]);
            s += (p.x() - spot.x()) * (q.y() - spot.y()) - (q.x() - spot.x()) * (p.y() - spot.y());
        }
        s
    };
    // Las dos secuencias deben girar igual alrededor del eje; si no, el hueco o la tapa están al revés
    let sense = signed_area(a_seq).signum();
    if sense != signed_area(b_seq).signum() {
        return Err(err("los bordes del drenaje giran en sentidos distintos"));
    }
    let progress = |seq: &[usize], k: usize, a0: Real| -> Real {
        // ángulo acumulado desde `a0` en el sentido de giro, en [0, 2π)
        let mut d = (angle(seq[k % seq.len()]) - a0) * sense;
        while d < 0.0 {
            d += std::f64::consts::TAU;
        }
        while d >= std::f64::consts::TAU {
            d -= std::f64::consts::TAU;
        }
        d
    };
    // Arranque: el vértice de A con el menor ángulo; B' desde su vértice inmediatamente anterior
    let a_start = 0usize;
    let a0 = angle(a_seq[a_start]);
    let b_start = (0..b_seq.len())
        .max_by(|&x, &y| {
            // el más cercano por detrás de a0 (mayor progreso desde a0 que no pase de la vuelta)
            progress(b_seq, x, a0).total_cmp(&progress(b_seq, y, a0))
        })
        .unwrap_or(0);
    let a_rot: Vec<usize> = (0..a_seq.len()).map(|k| a_seq[(a_start + k) % a_seq.len()]).collect();
    let b_rot: Vec<usize> = (0..b_seq.len()).map(|k| b_seq[(b_start + k) % b_seq.len()]).collect();
    let b0 = angle(b_rot[0]);
    // Progreso relativo a un origen común (el de B' para ambos), creciente a lo largo de cada secuencia
    let unwrap = |seq: &[usize]| -> Vec<Real> {
        let mut out = Vec::with_capacity(seq.len());
        let mut prev = 0.0;
        for (k, &i) in seq.iter().enumerate() {
            let mut d = (angle(i) - b0) * sense;
            while d < prev - std::f64::consts::PI && k > 0 {
                d += std::f64::consts::TAU;
            }
            while d < 0.0 && k == 0 {
                d += std::f64::consts::TAU;
            }
            while d < prev && k > 0 {
                d += std::f64::consts::TAU;
            }
            prev = d;
            out.push(d);
        }
        out
    };
    let ua = unwrap(&a_rot);
    let ub = unwrap(&b_rot);
    let (m, k) = (a_rot.len(), b_rot.len());
    let (mut i, mut j) = (0usize, 0usize);
    // A arranca desfasado: su primer progreso respecto a B' puede ser > 0; se avanza B' hasta alcanzarlo
    while i < m || j < k {
        let a_next = if i < m { ua.get(i + 1).copied().unwrap_or(ua[0] + std::f64::consts::TAU) } else { Real::INFINITY };
        let b_next = if j < k { ub.get(j + 1).copied().unwrap_or(ub[0] + std::f64::consts::TAU) } else { Real::INFINITY };
        let advance_a = j >= k || (i < m && a_next <= b_next);
        if advance_a {
            all_tris.push([a_rot[i % m], a_rot[(i + 1) % m], b_rot[j % k]]);
            i += 1;
        } else {
            all_tris.push([b_rot[(j + 1) % k], b_rot[j % k], a_rot[i % m]]);
            j += 1;
        }
    }
    Ok(())
}

/// Volumen con signo: el exterior suma y las cavidades restan
fn compute_signed_volume(mesh: &Mesh) -> Real {
    let (pos, tris) = triangles(mesh);
    tris.iter().map(|t| pos[t[0]].dot(&pos[t[1]].cross(&pos[t[2]]))).sum::<Real>() / 6.0
}

/// Material que gasta una impresión
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MaterialEstimate {
    /// Volumen de material (mm³)
    pub volume_mm3: Real,
    pub mass_g: Real,
    /// Largo de filamento (m)
    pub filament_m: Real,
}

/// Estima el material con `infill` (1 = macizo) y filamento de `filament_diameter` mm
///
/// Con un hueco interior, `infill` solo se aplica al volumen que queda; no cuenta
/// perímetros ni capas superiores, así que es una cota aproximada.
pub fn estimate_material(mesh: &Mesh, material: &MaterialProfile, infill: Real, filament_diameter: Real) -> MaterialEstimate {
    let volume = compute_signed_volume(mesh).abs() * infill.clamp(0.0, 1.0);
    let area = std::f64::consts::PI * (filament_diameter * 0.5).powi(2);
    MaterialEstimate { volume_mm3: volume, mass_g: volume * material.density / 1000.0, filament_m: volume / area / 1000.0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::printability::{check_printability, tests::make_box};
    use crate::profile::PrinterProfile;

    #[test]
    fn caja_hueca() {
        let b = make_box(40.0, 40.0, 40.0);
        let r = hollow(&b, &HollowConfig { wall: 3.0, voxel_size: Some(1.0), ..Default::default() }).unwrap();
        assert!(r.mesh.is_closed());
        let expected = 34f64.powi(3);
        assert!((r.cavity_volume - expected).abs() / expected < 0.06, "hueco {} esperado {}", r.cavity_volume, expected);
        assert!(r.saved_fraction > 0.5);
        // Paredes de al menos ~la mitad del pedido y sin errores de topología
        let rep = check_printability(&r.mesh, &PrinterProfile::default(), &MaterialProfile::default()).unwrap();
        assert!(rep.is_watertight, "{:?}", rep.issues);
        assert!(rep.min_thickness.unwrap() > 2.4, "grosor {:?}", rep.min_thickness);
        assert_eq!(rep.components, 2);
    }

    #[test]
    fn pared_gruesa_no_deja_hueco() {
        let b = make_box(10.0, 10.0, 10.0);
        assert!(hollow(&b, &HollowConfig { wall: 8.0, voxel_size: Some(1.0), ..Default::default() }).is_err());
    }

    #[test]
    fn malla_abierta_se_rechaza() {
        let b = make_box(20.0, 20.0, 20.0);
        let (pos, mut tris) = triangles(&b);
        tris.pop();
        let m = Mesh::from_triangles(&pos, &tris);
        assert!(matches!(hollow(&m, &HollowConfig::default()), Err(Print3dError::MeshNotClosed)));
    }

    #[test]
    fn material_de_una_caja() {
        let b = make_box(10.0, 10.0, 10.0);
        let e = estimate_material(&b, &MaterialProfile::default(), 1.0, 1.75);
        assert!((e.volume_mm3 - 1000.0).abs() < 1e-6);
        assert!((e.mass_g - 1.24).abs() < 1e-9);
        let half = estimate_material(&b, &MaterialProfile::default(), 0.5, 1.75);
        assert!((half.volume_mm3 - 500.0).abs() < 1e-6);
        assert!(e.filament_m > 0.4 && e.filament_m < 0.5);
    }

    #[test]
    fn esfera_hueca() {
        // Esfera UV de radio 20
        let (rings, segs, r) = (40usize, 64usize, 20.0);
        let mut pos = vec![Vector3::new(0.0, 0.0, r)];
        for i in 1..rings {
            let th = std::f64::consts::PI * i as Real / rings as Real;
            for j in 0..segs {
                let ph = std::f64::consts::TAU * j as Real / segs as Real;
                pos.push(Vector3::new(r * th.sin() * ph.cos(), r * th.sin() * ph.sin(), r * th.cos()));
            }
        }
        pos.push(Vector3::new(0.0, 0.0, -r));
        let bottom = pos.len() - 1;
        let at = |i: usize, j: usize| 1 + (i - 1) * segs + j % segs;
        let mut tris = Vec::new();
        for j in 0..segs {
            tris.push([0, at(1, j), at(1, j + 1)]);
            tris.push([bottom, at(rings - 1, j + 1), at(rings - 1, j)]);
        }
        for i in 1..rings - 1 {
            for j in 0..segs {
                tris.push([at(i, j), at(i + 1, j), at(i + 1, j + 1)]);
                tris.push([at(i, j), at(i + 1, j + 1), at(i, j + 1)]);
            }
        }
        let sphere = Mesh::from_triangles(&pos, &tris);
        let res = hollow(&sphere, &HollowConfig { wall: 2.0, voxel_size: Some(0.8), ..Default::default() }).unwrap();
        let expected = 4.0 / 3.0 * std::f64::consts::PI * 18f64.powi(3);
        assert!((res.cavity_volume - expected).abs() / expected < 0.06, "{} vs {}", res.cavity_volume, expected);
        assert!(res.mesh.is_closed());
    }

    #[test]
    fn caja_hueca_con_drenaje() {
        let b = make_box(40.0, 40.0, 40.0);
        let r = hollow(&b, &HollowConfig { wall: 3.0, voxel_size: Some(1.0), drain_diameter: Some(5.0), ..Default::default() }).unwrap();
        assert!(r.mesh.is_closed(), "la malla con drenaje no quedó cerrada");
        // El túnel de drenaje conecta el hueco con el exterior: un solo cuerpo
        let rep = check_printability(&r.mesh, &PrinterProfile::default(), &MaterialProfile::default()).unwrap();
        assert!(rep.is_watertight, "{:?}", rep.issues);
        assert_eq!(rep.components, 1);
        let plain = hollow(&b, &HollowConfig { wall: 3.0, voxel_size: Some(1.0), ..Default::default() }).unwrap();
        // Con el agujero hay un poco más de volumen vaciado
        let vol = |m: &Mesh| super::compute_signed_volume(m);
        // El túnel (≈ π·2,5²·3 mm³ = 59) resta material; con las paredes al revés restaría de más o sumaría
        let removed = vol(&plain.mesh) - vol(&r.mesh);
        assert!(removed > 20.0 && removed < 140.0, "túnel de {removed} mm³");
    }

    fn uv_sphere(rad: Real, rings: usize, segs: usize) -> Mesh {
        let mut pos = vec![Vector3::new(0.0, 0.0, rad)];
        for i in 1..rings {
            let th = std::f64::consts::PI * i as Real / rings as Real;
            for j in 0..segs {
                let ph = std::f64::consts::TAU * j as Real / segs as Real;
                pos.push(Vector3::new(rad * th.sin() * ph.cos(), rad * th.sin() * ph.sin(), rad * th.cos()));
            }
        }
        pos.push(Vector3::new(0.0, 0.0, -rad));
        let bottom = pos.len() - 1;
        let at = |i: usize, j: usize| 1 + (i - 1) * segs + j % segs;
        let mut tris = Vec::new();
        for j in 0..segs {
            tris.push([0, at(1, j), at(1, j + 1)]);
            tris.push([bottom, at(rings - 1, j + 1), at(rings - 1, j)]);
        }
        for i in 1..rings - 1 {
            for j in 0..segs {
                tris.push([at(i, j), at(i + 1, j), at(i + 1, j + 1)]);
                tris.push([at(i, j), at(i + 1, j + 1), at(i, j + 1)]);
            }
        }
        Mesh::from_triangles(&pos, &tris)
    }

    #[test]
    fn drenaje_sin_cara_plana_aplana_la_base() {
        // Esfera: sin cara plana abajo; se le quita una lámina de la base y se avisa
        let sphere = uv_sphere(15.0, 24, 32);
        let r = hollow(&sphere, &HollowConfig { wall: 2.0, voxel_size: Some(1.0), drain_diameter: Some(4.0), ..Default::default() }).unwrap();
        assert!(r.mesh.is_closed());
        assert!(r.notes.iter().any(|n| n.contains("aplanó la base")), "{:?}", r.notes);
        let rep = check_printability(&r.mesh, &PrinterProfile::default(), &MaterialProfile::default()).unwrap();
        assert!(rep.is_watertight, "{:?}", rep.issues);
        assert_eq!(rep.components, 1, "el túnel tiene que unir el hueco con el exterior");
        // La base quedó plana: el punto más bajo ya no es el polo
        let zmin = r.mesh.vertices.iter().map(|v| v.position.z()).fold(Real::INFINITY, Real::min);
        assert!(zmin > -15.0 + 0.1, "zmin {zmin}");
    }

    #[test]
    fn caja_hueca_con_drenaje_y_venteo() {
        let b = make_box(60.0, 40.0, 40.0);
        let one = hollow(&b, &HollowConfig { wall: 3.0, voxel_size: Some(1.0), drain_diameter: Some(5.0), ..Default::default() }).unwrap();
        let two = hollow(&b, &HollowConfig { wall: 3.0, voxel_size: Some(1.0), drain_diameter: Some(5.0), vent: true }).unwrap();
        assert!(two.mesh.is_closed());
        assert!(two.notes.is_empty(), "{:?}", two.notes);
        let rep = check_printability(&two.mesh, &PrinterProfile::default(), &MaterialProfile::default()).unwrap();
        assert!(rep.is_watertight, "{:?}", rep.issues);
        assert_eq!(rep.components, 1);
        // El venteo suma un segundo túnel (≈ π·2,5²·3 mm³ cada uno)
        let vol = |m: &Mesh| super::compute_signed_volume(m);
        let second = vol(&one.mesh) - vol(&two.mesh);
        assert!(second > 20.0 && second < 140.0, "segundo túnel de {second} mm³");
        // Los dos agujeros quedan lejos entre sí
        let (pos, tris) = triangles(&two.mesh);
        let zmin = pos.iter().map(|p| p.z()).fold(Real::INFINITY, Real::min);
        let mut hole_pts: Vec<Vector3> = Vec::new();
        for t in &tris {
            let ps = [pos[t[0]], pos[t[1]], pos[t[2]]];
            // Vértices de la tapa de abajo que no son del contorno del cuadro exterior: los de los agujeros
            for p in ps {
                if (p.z() - zmin).abs() < 1e-6 && p.x() > 1.0 && p.x() < 59.0 && p.y() > 1.0 && p.y() < 39.0 {
                    hole_pts.push(p);
                }
            }
        }
        let xs: Vec<Real> = hole_pts.iter().map(|p| p.x()).collect();
        let spread = xs.iter().cloned().fold(Real::NEG_INFINITY, Real::max) - xs.iter().cloned().fold(Real::INFINITY, Real::min);
        assert!(spread > 20.0, "los agujeros están muy juntos ({spread} mm)");
    }
}
