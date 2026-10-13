//! Piel imprimible: desplaza la malla con un mapa de altura en una zona de caras
//!
//! 1. Se unen los vértices que están en el mismo lugar (las costuras de UV los parten) y se parten las
//!    aristas largas de la zona hasta que midan lo que el detalle del mapa pide; las caras vecinas se
//!    parten con ellas, así la malla sigue cerrada.
//! 2. Cada vértice toma su altura del mapa por UV (la baldosa mide `tile_mm`) y se mueve a lo largo de su
//!    normal. El borde de la zona queda fijo y la altura se funde hacia él, para que no se abran grietas.

use std::collections::{BinaryHeap, HashMap};

use pinocchio_math::{Real, Vector3};
use pinocchio_mesh::Mesh;
use serde::{Deserialize, Serialize};

use crate::error::{Print3dError, Result};
use crate::geom::{diagonal, triangles, weld_ids};
use crate::texture::HeightMap;

/// Hacia dónde crece el relieve respecto de la superficie original
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SkinOffset {
    /// Solo hacia afuera: la superficie original es el fondo (el blanco sobresale `amplitude`)
    #[default]
    Outward,
    /// Mitad y mitad: la superficie original es el gris medio
    Centered,
    /// Solo hacia adentro: la superficie original es la cima (el negro se hunde `amplitude`)
    Inward,
}

/// Parámetros de la piel (todo en milímetros; `unit_mm` es cuántos mm mide una unidad de la malla)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkinConfig {
    /// Lado de la baldosa de la imagen en la superficie (mm)
    pub tile_mm: Real,
    /// Altura entre el negro y el blanco (mm)
    pub amplitude: Real,
    pub offset: SkinOffset,
    /// Giro de la imagen sobre la superficie (grados)
    pub rotation_deg: Real,
    /// Lado objetivo de los triángulos de la zona (mm); `None`: lo da el detalle del mapa y la boquilla
    pub edge_mm: Option<Real>,
    /// Ancho del tramo donde el relieve nace desde el borde de la zona (mm); `None`: tres amplitudes
    pub feather_mm: Option<Real>,
    /// Tope de triángulos de la malla resultante
    pub max_triangles: usize,
    /// Mm por unidad de la malla
    pub unit_mm: Real,
}

impl Default for SkinConfig {
    fn default() -> Self {
        Self { tile_mm: 20.0, amplitude: 0.6, offset: SkinOffset::Outward, rotation_deg: 0.0, edge_mm: None, feather_mm: None, max_triangles: 2_000_000, unit_mm: 1.0 }
    }
}

/// Resultado de aplicar la piel
#[derive(Debug, Clone)]
pub struct SkinResult {
    pub mesh: Mesh,
    pub triangles_before: usize,
    pub triangles_after: usize,
    /// Lado de triángulo con que se partió la zona (mm)
    pub edge_mm: Real,
    /// Mayor desplazamiento (mm, en valor absoluto)
    pub max_displacement_mm: Real,
    /// Caras que quedaron en la zona (después de partir)
    pub zone_faces: usize,
    pub warnings: Vec<String>,
}

#[derive(Clone)]
struct Face {
    v: [usize; 3],
    uv: [[Real; 2]; 3],
    sel: bool,
}

fn face_normal_area(p: &[Vector3], v: [usize; 3]) -> (Vector3, Real) {
    let n = (p[v[1]] - p[v[0]]).cross(&(p[v[2]] - p[v[0]]));
    let len = n.length();
    if len > 0.0 { (n * (1.0 / len), len * 0.5) } else { (Vector3::zero(), 0.0) }
}

fn smooth(t: Real) -> Real {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Aplica el mapa `map` a las caras `zone` de `mesh`. `uvs` lleva un UV por vértice de la malla
pub fn apply_skin(mesh: &Mesh, uvs: &[[f32; 2]], zone: &[usize], map: &HeightMap, cfg: &SkinConfig) -> Result<SkinResult> {
    let (pos, tris) = triangles(mesh);
    if tris.is_empty() {
        return Err(Print3dError::EmptyMesh);
    }
    if uvs.len() != pos.len() {
        return Err(Print3dError::InvalidConfig("la malla no tiene UV: despliégala primero en UV / Piel".into()));
    }
    if !(cfg.tile_mm > 0.0 && cfg.amplitude >= 0.0 && cfg.unit_mm > 0.0) {
        return Err(Print3dError::InvalidConfig("la baldosa y la unidad tienen que ser positivas y la amplitud no negativa".into()));
    }
    let mut in_zone = vec![false; tris.len()];
    for &f in zone.iter().filter(|&&f| f < tris.len()) {
        in_zone[f] = true;
    }
    if !in_zone.iter().any(|&s| s) {
        return Err(Print3dError::InvalidConfig("no hay una zona elegida".into()));
    }

    // Vértices unidos por posición
    let diag = diagonal(&pos);
    let weld = weld_ids(&pos, (diag * 1e-7).max(1e-9));
    let mut wpos: Vec<Vector3> = Vec::new();
    let mut remap: HashMap<usize, usize> = HashMap::new();
    for (i, &w) in weld.iter().enumerate() {
        remap.entry(w).or_insert_with(|| {
            wpos.push(pos[i]);
            wpos.len() - 1
        });
    }
    let wid = |i: usize| remap[&weld[i]];
    let mut faces: Vec<Face> = tris
        .iter()
        .enumerate()
        .filter_map(|(f, t)| {
            let v = [wid(t[0]), wid(t[1]), wid(t[2])];
            let uv = [0, 1, 2].map(|k| [Real::from(uvs[t[k]][0]), Real::from(uvs[t[k]][1])]);
            (v[0] != v[1] && v[1] != v[2] && v[0] != v[2]).then_some(Face { v, uv, sel: in_zone[f] })
        })
        .collect();
    let triangles_before = faces.len();

    // Cuántos mm mide una unidad de UV en la zona (promedio por área)
    let (mut area3, mut area_uv) = (0.0, 0.0);
    for f in faces.iter().filter(|f| f.sel) {
        area3 += face_normal_area(&wpos, f.v).1;
        area_uv += ((f.uv[1][0] - f.uv[0][0]) * (f.uv[2][1] - f.uv[0][1]) - (f.uv[2][0] - f.uv[0][0]) * (f.uv[1][1] - f.uv[0][1])).abs() * 0.5;
    }
    if !(area_uv > 1e-12 && area3 > 0.0) {
        return Err(Print3dError::InvalidConfig("la zona no tiene UV (todas sus caras caen en un punto): despliégala primero en UV / Piel".into()));
    }
    let mm_per_uv = (area3 / area_uv).sqrt() * cfg.unit_mm;
    let zone_area_mm2 = area3 * cfg.unit_mm * cfg.unit_mm;

    // Lado de los triángulos: el detalle del mapa, sin bajar de media boquilla (≈ 0,2 mm)
    let pixel_mm = cfg.tile_mm / map.width.max(map.height) as Real;
    let edge_mm = cfg.edge_mm.unwrap_or_else(|| pixel_mm.max(0.2));
    if edge_mm.is_nan() || edge_mm <= 0.0 {
        return Err(Print3dError::InvalidConfig("el lado de los triángulos tiene que ser positivo".into()));
    }
    let estimate = (zone_area_mm2 / (0.19 * edge_mm * edge_mm)) as usize;
    if estimate + faces.iter().filter(|f| !f.sel).count() > cfg.max_triangles {
        return Err(Print3dError::InvalidConfig(format!(
            "la piel pediría unos {estimate} triángulos (tope {}): sube el tamaño del detalle, achica la zona o la baldosa",
            cfg.max_triangles
        )));
    }

    // Partir las aristas largas de la zona (y las caras vecinas con ellas)
    let max_len = edge_mm / cfg.unit_mm;
    for _pass in 0..64 {
        let mut edges: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
        for (fi, f) in faces.iter().enumerate() {
            for k in 0..3 {
                let (a, b) = (f.v[k], f.v[(k + 1) % 3]);
                edges.entry((a.min(b), a.max(b))).or_default().push(fi);
            }
        }
        let mut long: Vec<((usize, usize), Real)> = edges
            .iter()
            .filter(|(_, fs)| fs.len() <= 2 && fs.iter().any(|&f| faces[f].sel))
            .map(|(&e, _)| (e, (wpos[e.0] - wpos[e.1]).length()))
            .filter(|(_, l)| *l > max_len)
            .collect();
        if long.is_empty() {
            break;
        }
        long.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        let mut touched = vec![false; faces.len()];
        let mut added: Vec<Face> = Vec::new();
        for ((a, b), _) in long {
            let adj = &edges[&(a, b)];
            if adj.iter().any(|&f| touched[f]) {
                continue;
            }
            let m = wpos.len();
            wpos.push((wpos[a] + wpos[b]) * 0.5);
            for &fi in adj {
                touched[fi] = true;
                let f = faces[fi].clone();
                let k = (0..3).find(|&k| (f.v[k] == a && f.v[(k + 1) % 3] == b) || (f.v[k] == b && f.v[(k + 1) % 3] == a)).expect("la arista es de la cara");
                let (p, q, r) = (f.v[k], f.v[(k + 1) % 3], f.v[(k + 2) % 3]);
                let (up, uq, ur) = (f.uv[k], f.uv[(k + 1) % 3], f.uv[(k + 2) % 3]);
                let um = [(up[0] + uq[0]) * 0.5, (up[1] + uq[1]) * 0.5];
                faces[fi] = Face { v: [p, m, r], uv: [up, um, ur], sel: f.sel };
                added.push(Face { v: [m, q, r], uv: [um, uq, ur], sel: f.sel });
            }
        }
        faces.extend(added);
        if faces.len() > cfg.max_triangles {
            return Err(Print3dError::InvalidConfig(format!("la piel pasó el tope de {} triángulos al partir la malla", cfg.max_triangles)));
        }
    }

    // Normales suaves de cada vértice y a qué lado del borde de la zona está
    let nv = wpos.len();
    let mut normal = vec![Vector3::zero(); nv];
    let (mut in_sel, mut in_other) = (vec![false; nv], vec![false; nv]);
    for f in &faces {
        let (n, a) = face_normal_area(&wpos, f.v);
        for &v in &f.v {
            normal[v] += n * a;
            if f.sel {
                in_sel[v] = true;
            } else {
                in_other[v] = true;
            }
        }
    }
    // Distancia (por las aristas de la zona) hasta el borde, para fundir el relieve
    let mut adj: Vec<Vec<(usize, Real)>> = vec![Vec::new(); nv];
    for f in faces.iter().filter(|f| f.sel) {
        for k in 0..3 {
            let (a, b) = (f.v[k], f.v[(k + 1) % 3]);
            let l = (wpos[a] - wpos[b]).length() * cfg.unit_mm;
            adj[a].push((b, l));
            adj[b].push((a, l));
        }
    }
    let mut dist = vec![Real::INFINITY; nv];
    let mut heap: BinaryHeap<(std::cmp::Reverse<u64>, usize)> = BinaryHeap::new();
    for v in 0..nv {
        if in_sel[v] && in_other[v] {
            dist[v] = 0.0;
            heap.push((std::cmp::Reverse(0), v));
        }
    }
    // Una zona sin vecinos (toda la malla): no hay borde y el relieve no se funde
    let closed_zone = !heap.is_empty();
    while let Some((std::cmp::Reverse(bits), v)) = heap.pop() {
        let d = f64::from_bits(bits);
        if d > dist[v] {
            continue;
        }
        for &(w, l) in &adj[v] {
            if d + l < dist[w] {
                dist[w] = d + l;
                heap.push((std::cmp::Reverse((d + l).to_bits()), w));
            }
        }
    }
    let feather = cfg.feather_mm.unwrap_or(cfg.amplitude * 3.0).max(1e-9);

    // UV de cada vértice (de una cara de la zona si la hay) y su altura
    let mut vuv: Vec<Option<[Real; 2]>> = vec![None; nv];
    for sel_pass in [true, false] {
        for f in faces.iter().filter(|f| f.sel == sel_pass) {
            for k in 0..3 {
                vuv[f.v[k]].get_or_insert(f.uv[k]);
            }
        }
    }
    let (sr, cr) = cfg.rotation_deg.to_radians().sin_cos();
    let center = match cfg.offset {
        SkinOffset::Outward => 0.0,
        SkinOffset::Centered => 0.5,
        SkinOffset::Inward => 1.0,
    };
    let mut out = wpos.clone();
    let mut max_d: Real = 0.0;
    for v in 0..nv {
        if !in_sel[v] || (in_other[v] && closed_zone) {
            continue;
        }
        let Some(uv) = vuv[v] else { continue };
        let w = if closed_zone { smooth(dist[v] / feather) } else { 1.0 };
        if w <= 0.0 {
            continue;
        }
        let (x, y) = (uv[0] * cr - uv[1] * sr, uv[0] * sr + uv[1] * cr);
        let h = map.sample(x * mm_per_uv / cfg.tile_mm, y * mm_per_uv / cfg.tile_mm);
        let d_mm = cfg.amplitude * w * (h - center);
        let n = normal[v];
        if n.length() > 0.0 {
            out[v] = wpos[v] + n.normalize() * (d_mm / cfg.unit_mm);
            max_d = max_d.max(d_mm.abs());
        }
    }

    let tris_out: Vec<[usize; 3]> = faces.iter().map(|f| f.v).collect();
    let mut warnings = Vec::new();
    if edge_mm < 0.2 {
        warnings.push(format!("el detalle es de {edge_mm:.2} mm, más fino que media boquilla: la impresora no lo va a reproducir"));
    }
    if cfg.amplitude > 0.0 && cfg.amplitude < 0.1 {
        warnings.push(format!("un relieve de {:.2} mm es menos que una capa: casi no se va a notar", cfg.amplitude));
    }
    Ok(SkinResult {
        mesh: Mesh::from_triangles(&out, &tris_out),
        triangles_before,
        triangles_after: tris_out.len(),
        edge_mm,
        max_displacement_mm: max_d,
        zone_faces: faces.iter().filter(|f| f.sel).count(),
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::texture::Pattern;

    /// Caja [0,40]x[0,40]x[0,10] en "sopa" (cada triángulo con sus vértices) y UV planos (x/40, y/40) en la
    /// tapa; las demás caras reciben UV propios
    fn box_soup() -> (Mesh, Vec<[f32; 2]>, Vec<usize>) {
        let c = |x: f64, y: f64, z: f64| Vector3::new(x * 40.0, y * 40.0, z * 10.0);
        let v = [c(0., 0., 0.), c(1., 0., 0.), c(1., 1., 0.), c(0., 1., 0.), c(0., 0., 1.), c(1., 0., 1.), c(1., 1., 1.), c(0., 1., 1.)];
        let f: [[usize; 3]; 12] = [[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]];
        let mut pos = Vec::new();
        let mut uvs: Vec<[f32; 2]> = Vec::new();
        let mut tris = Vec::new();
        for (i, t) in f.iter().enumerate() {
            for &k in t {
                let p = v[k];
                pos.push(p);
                // La tapa (caras 2 y 3) usa (x, y)/40; el resto, otro mapeo cualquiera
                uvs.push(if i == 2 || i == 3 { [(p.x() / 40.0) as f32, (p.y() / 40.0) as f32] } else { [(p.x() / 40.0) as f32 * 0.5, (p.z() / 10.0) as f32 * 0.5] });
            }
            tris.push([3 * i, 3 * i + 1, 3 * i + 2]);
        }
        (Mesh::from_triangles(&pos, &tris), uvs, vec![2, 3])
    }

    fn cfg(amplitude: f64) -> SkinConfig {
        SkinConfig { tile_mm: 10.0, amplitude, edge_mm: Some(1.0), feather_mm: Some(2.0), ..Default::default() }
    }

    #[test]
    fn el_relieve_sigue_al_mapa_y_la_malla_queda_cerrada() {
        let (mesh, uvs, zone) = box_soup();
        let map = Pattern::Dots.generate(64);
        let r = apply_skin(&mesh, &uvs, &zone, &map, &cfg(0.8)).unwrap();
        assert!(r.mesh.is_closed(), "la piel abrió la malla");
        assert!(r.triangles_after > 1000 && r.triangles_after > r.triangles_before, "{} triángulos", r.triangles_after);
        // Hacia afuera: nada baja del plano de la tapa (z = 10) y nada pasa de 10 + 0,8
        let zs: Vec<f64> = r.mesh.vertices.iter().map(|v| v.position.z()).collect();
        let top: Vec<f64> = zs.iter().copied().filter(|z| *z > 9.0).collect();
        assert!(top.iter().all(|z| *z >= 10.0 - 1e-9 && *z <= 10.8 + 1e-9), "{:?}", top.iter().cloned().fold(0.0, f64::max));
        assert!(r.max_displacement_mm > 0.7 && r.max_displacement_mm <= 0.8 + 1e-9, "{}", r.max_displacement_mm);
        // El borde de la zona y el resto de la caja no se mueven
        let rim_z: Vec<f64> = r.mesh.vertices.iter().filter(|v| (v.position.x() < 1e-9 || v.position.x() > 40.0 - 1e-9 || v.position.y() < 1e-9 || v.position.y() > 40.0 - 1e-9) && v.position.z() > 9.0).map(|v| v.position.z()).collect();
        assert!(!rim_z.is_empty() && rim_z.iter().all(|z| (z - 10.0).abs() < 1e-9), "el borde se movió");
        // El volumen crece lo que sube el relieve
        let gained = crate::analysis::compute_volume(&r.mesh) - crate::analysis::compute_volume(&mesh);
        let mean_h = map.data.iter().sum::<f32>() as f64 / map.data.len() as f64;
        assert!(gained > 0.3 * 0.8 * mean_h * 1600.0 && gained < 0.8 * 0.8 * 1600.0, "volumen +{gained}");
    }

    #[test]
    fn cada_vertice_toma_la_altura_del_mapa_en_su_lugar() {
        let (mesh, uvs, zone) = box_soup();
        let map = Pattern::Lines.generate(64);
        let r = apply_skin(&mesh, &uvs, &zone, &map, &cfg(1.0)).unwrap();
        // Lejos del borde (peso 1), z = 10 + h(x / tile, y / tile) con la baldosa de 10 mm
        let mut checked = 0;
        for v in r.mesh.vertices.iter() {
            let (x, y) = (v.position.x(), v.position.y());
            if x > 4.0 && x < 36.0 && y > 4.0 && y < 36.0 && v.position.z() > 9.0 {
                let expected = 10.0 + map.sample(x / 10.0, y / 10.0);
                // El vértice se movió por su normal (casi +z): la x e y no cambian en la tapa plana
                assert!((v.position.z() - expected).abs() < 1e-6, "({x:.2},{y:.2}) {} vs {expected}", v.position.z());
                checked += 1;
            }
        }
        assert!(checked > 500, "{checked}");
    }

    #[test]
    fn la_rotacion_gira_la_imagen() {
        let (mesh, uvs, zone) = box_soup();
        let map = Pattern::Lines.generate(64);
        let r = apply_skin(&mesh, &uvs, &zone, &map, &SkinConfig { rotation_deg: 90.0, ..cfg(1.0) }).unwrap();
        for v in r.mesh.vertices.iter() {
            let (x, y) = (v.position.x(), v.position.y());
            if x > 4.0 && x < 36.0 && y > 4.0 && y < 36.0 && v.position.z() > 9.0 {
                // Girada 90°: (u, v) → (−v, u)
                let expected = 10.0 + map.sample(-y / 10.0, x / 10.0);
                assert!((v.position.z() - expected).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn desfase_hacia_adentro_y_centrado() {
        let (mesh, uvs, zone) = box_soup();
        let map = Pattern::Dots.generate(64);
        let inward = apply_skin(&mesh, &uvs, &zone, &map, &SkinConfig { offset: SkinOffset::Inward, ..cfg(0.8) }).unwrap();
        assert!(inward.mesh.is_closed());
        assert!(inward.mesh.vertices.iter().all(|v| v.position.z() <= 10.0 + 1e-9), "algo sobresale de la superficie");
        assert!(inward.mesh.vertices.iter().any(|v| v.position.z() < 9.5));
        let centered = apply_skin(&mesh, &uvs, &zone, &map, &SkinConfig { offset: SkinOffset::Centered, ..cfg(0.8) }).unwrap();
        assert!(centered.mesh.is_closed());
        let (hi, lo) = centered.mesh.vertices.iter().filter(|v| v.position.z() > 9.0).fold((f64::MIN, f64::MAX), |(h, l), v| (h.max(v.position.z()), l.min(v.position.z())));
        assert!(hi > 10.2 && lo < 9.8, "{lo}..{hi}");
    }

    #[test]
    fn una_zona_que_cruza_aristas_vivas_queda_cerrada() {
        // La tapa y una cara lateral, con UV de cartas distintas: el relieve cruza la arista dura
        let (mesh, uvs, _) = box_soup();
        let map = Pattern::Hex.generate(64);
        let r = apply_skin(&mesh, &uvs, &[2, 3, 4, 5], &map, &cfg(0.6)).unwrap();
        assert!(r.mesh.is_closed(), "se abrió la costura entre cartas");
        assert!(r.triangles_after > 500);
    }

    #[test]
    fn errores_claros() {
        let (mesh, uvs, zone) = box_soup();
        let map = Pattern::Dots.generate(64);
        assert!(apply_skin(&mesh, &uvs, &[], &map, &cfg(0.5)).is_err(), "sin zona");
        assert!(apply_skin(&mesh, &uvs[..5], &zone, &map, &cfg(0.5)).is_err(), "sin UV");
        assert!(apply_skin(&mesh, &vec![[0.0, 0.0]; uvs.len()], &zone, &map, &cfg(0.5)).is_err(), "UV colapsados");
        assert!(apply_skin(&mesh, &uvs, &zone, &map, &SkinConfig { max_triangles: 100, ..cfg(0.5) }).is_err(), "tope de triángulos");
        assert!(apply_skin(&mesh, &uvs, &zone, &map, &SkinConfig { tile_mm: 0.0, ..cfg(0.5) }).is_err());
    }

    #[test]
    fn avisa_si_el_detalle_es_mas_fino_que_la_boquilla() {
        let (mesh, uvs, zone) = box_soup();
        let map = Pattern::Dots.generate(64);
        let r = apply_skin(&mesh, &uvs, &zone, &map, &SkinConfig { edge_mm: Some(0.15), max_triangles: 5_000_000, amplitude: 0.05, ..cfg(0.05) }).unwrap();
        assert_eq!(r.warnings.len(), 2, "{:?}", r.warnings);
    }

    /// Cortar después de texturizar: las piezas salen cerradas y las tapas planas, aunque la malla sea densa
    #[test]
    fn se_puede_cortar_una_malla_con_piel() {
        use crate::slicer::{slice_by_plane, Plane};
        let (mesh, uvs, zone) = box_soup();
        let map = Pattern::Leather.generate(128);
        let skinned = apply_skin(&mesh, &uvs, &zone, &map, &SkinConfig { tile_mm: 12.0, amplitude: 0.7, edge_mm: Some(0.5), feather_mm: Some(2.0), ..Default::default() }).unwrap();
        assert!(skinned.triangles_after > 20_000, "{}", skinned.triangles_after);
        let t0 = std::time::Instant::now();
        // Un corte vertical por el medio de la tapa texturizada
        let (a, b) = slice_by_plane(&skinned.mesh, &Plane::yz_at_x(20.0)).unwrap();
        let elapsed = t0.elapsed();
        assert!(a.is_closed() && b.is_closed(), "las piezas cortadas no quedaron cerradas");
        let va = crate::analysis::compute_volume(&a);
        let vb = crate::analysis::compute_volume(&b);
        assert!((va + vb - crate::analysis::compute_volume(&skinned.mesh)).abs() < 1e-6 * (va + vb));
        // La tapa del corte es plana: todos sus vértices en x = 20
        let cap: Vec<_> = a.vertices.iter().filter(|v| (v.position.x() - 20.0).abs() < 1e-9).collect();
        assert!(cap.len() > 20, "{}", cap.len());
        eprintln!("corte de {} triángulos en {:?}", skinned.triangles_after, elapsed);
    }
}
