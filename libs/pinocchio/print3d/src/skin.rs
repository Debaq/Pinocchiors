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

/// De dónde sale el UV con que se lee el mapa de altura
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SkinMapping {
    /// El UV del modelo (hay que desplegarlo antes)
    #[default]
    Uv,
    /// Proyección en caja, sin UV: cada vértice proyecta sobre el plano del eje al que más mira su normal, en mm,
    /// así que la baldosa mide lo mismo en todas partes. Se nota una costura donde cambia de eje
    Box,
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
    /// De dónde sale el UV (con `Box` no hace falta que el modelo lo tenga)
    #[serde(default)]
    pub mapping: SkinMapping,
    /// Distancia que el relieve se mantiene alejada de las caras protegidas (mm); luego nace con el mismo tramo
    /// que en el borde de la zona
    #[serde(default)]
    pub margin_mm: Real,
    /// Simplificar el resultado sin pasarse de esta desviación (mm): las zonas lisas pierden triángulos y el relieve
    /// se queda. `None`: no se simplifica
    #[serde(default)]
    pub decimate_mm: Option<Real>,
}

impl Default for SkinConfig {
    fn default() -> Self {
        Self {
            tile_mm: 20.0,
            amplitude: 0.6,
            offset: SkinOffset::Outward,
            rotation_deg: 0.0,
            edge_mm: None,
            feather_mm: None,
            max_triangles: 2_000_000,
            unit_mm: 1.0,
            mapping: SkinMapping::Uv,
            margin_mm: 0.0,
            decimate_mm: None,
        }
    }
}

/// Resultado de aplicar la piel
#[derive(Debug, Clone)]
pub struct SkinResult {
    pub mesh: Mesh,
    pub triangles_before: usize,
    pub triangles_after: usize,
    /// Triángulos antes de simplificar (igual a `triangles_after` si no se simplificó)
    pub triangles_skinned: usize,
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
    /// Cara protegida: no se texturiza ni el relieve se le acerca
    prot: bool,
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

fn feather_of(cfg: &SkinConfig) -> Real {
    cfg.feather_mm.unwrap_or(cfg.amplitude * 3.0).max(1e-9)
}

/// Distancia (mm) de cada vértice al más cercano de `seeds`; infinita si pasa de `reach` (unidades de la malla)
fn nearest_distance(pos: &[Vector3], seeds: &[usize], reach: Real, unit_mm: Real) -> Vec<Real> {
    let mut out = vec![Real::INFINITY; pos.len()];
    if seeds.is_empty() || reach.is_nan() || reach <= 0.0 {
        return out;
    }
    let key = |p: &Vector3| ((p.x() / reach).floor() as i64, (p.y() / reach).floor() as i64, (p.z() / reach).floor() as i64);
    let mut grid: HashMap<(i64, i64, i64), Vec<usize>> = HashMap::new();
    for &s in seeds {
        grid.entry(key(&pos[s])).or_default().push(s);
    }
    for (v, p) in pos.iter().enumerate() {
        let (cx, cy, cz) = key(p);
        let mut best = Real::INFINITY;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(list) = grid.get(&(cx + dx, cy + dy, cz + dz)) {
                        for &s in list {
                            best = best.min((pos[s] - *p).length());
                        }
                    }
                }
            }
        }
        if best.is_finite() {
            out[v] = best * unit_mm;
        }
    }
    out
}

/// UV de proyección en caja de un punto con normal `n` (en las unidades de la malla)
fn box_uv(p: Vector3, n: Vector3) -> [Real; 2] {
    let (ax, ay, az) = (n.x().abs(), n.y().abs(), n.z().abs());
    if ax >= ay && ax >= az {
        let s = n.x().signum();
        [-s * p.z(), p.y()]
    } else if ay >= az {
        let s = n.y().signum();
        [s * p.x(), -p.z()]
    } else {
        let s = n.z().signum();
        [s * p.x(), p.y()]
    }
}

/// Aplica el mapa `map` a las caras `zone` de `mesh`. `uvs` lleva un UV por vértice de la malla (con
/// `SkinMapping::Box` no se usa y puede ir vacío)
pub fn apply_skin(mesh: &Mesh, uvs: &[[f32; 2]], zone: &[usize], map: &HeightMap, cfg: &SkinConfig) -> Result<SkinResult> {
    apply_skin_with(mesh, uvs, zone, &[], map, cfg)
}

/// Como [`apply_skin`] con caras `protected` (un agujero, una unión): quedan fuera de la zona aunque estén en ella y el
/// relieve se aleja de ellas `cfg.margin_mm`, naciendo desde ahí
pub fn apply_skin_with(mesh: &Mesh, uvs: &[[f32; 2]], zone: &[usize], protected: &[usize], map: &HeightMap, cfg: &SkinConfig) -> Result<SkinResult> {
    let (pos, tris) = triangles(mesh);
    if tris.is_empty() {
        return Err(Print3dError::EmptyMesh);
    }
    let boxed = cfg.mapping == SkinMapping::Box;
    if !boxed && uvs.len() != pos.len() {
        return Err(Print3dError::InvalidConfig("la malla no tiene UV: despliégala primero en UV / Piel".into()));
    }
    if !(cfg.tile_mm > 0.0 && cfg.amplitude >= 0.0 && cfg.unit_mm > 0.0) {
        return Err(Print3dError::InvalidConfig("la baldosa y la unidad tienen que ser positivas y la amplitud no negativa".into()));
    }
    let mut in_zone = vec![false; tris.len()];
    for &f in zone.iter().filter(|&&f| f < tris.len()) {
        in_zone[f] = true;
    }
    let mut is_prot = vec![false; tris.len()];
    for &f in protected.iter().filter(|&&f| f < tris.len()) {
        is_prot[f] = true;
        in_zone[f] = false;
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
            let uv = if boxed { [[0.0; 2]; 3] } else { [0, 1, 2].map(|k| [Real::from(uvs[t[k]][0]), Real::from(uvs[t[k]][1])]) };
            (v[0] != v[1] && v[1] != v[2] && v[0] != v[2]).then_some(Face { v, uv, sel: in_zone[f], prot: is_prot[f] })
        })
        .collect();
    let triangles_before = faces.len();

    // Cuántos mm mide una unidad de UV en la zona (promedio por área)
    let (mut area3, mut area_uv) = (0.0, 0.0);
    for f in faces.iter().filter(|f| f.sel) {
        area3 += face_normal_area(&wpos, f.v).1;
        area_uv += ((f.uv[1][0] - f.uv[0][0]) * (f.uv[2][1] - f.uv[0][1]) - (f.uv[2][0] - f.uv[0][0]) * (f.uv[1][1] - f.uv[0][1])).abs() * 0.5;
    }
    if !boxed && !(area_uv > 1e-12 && area3 > 0.0) {
        return Err(Print3dError::InvalidConfig("la zona no tiene UV (todas sus caras caen en un punto): despliégala primero en UV / Piel".into()));
    }
    // En caja el UV ya está en unidades de la malla
    let mm_per_uv = if boxed { cfg.unit_mm } else { (area3 / area_uv).sqrt() * cfg.unit_mm };
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
                faces[fi] = Face { v: [p, m, r], uv: [up, um, ur], sel: f.sel, prot: f.prot };
                added.push(Face { v: [m, q, r], uv: [um, uq, ur], sel: f.sel, prot: f.prot });
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
    let (mut in_sel, mut in_other, mut in_prot) = (vec![false; nv], vec![false; nv], vec![false; nv]);
    for f in &faces {
        let (n, a) = face_normal_area(&wpos, f.v);
        for &v in &f.v {
            normal[v] += n * a;
            if f.sel {
                in_sel[v] = true;
            } else {
                in_other[v] = true;
            }
            if f.prot {
                in_prot[v] = true;
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
    // Por las aristas de la zona: distancia desde `seeds` (vértices) a cada vértice
    let spread = |seeds: &[usize]| -> Vec<Real> {
        let mut dist = vec![Real::INFINITY; nv];
        let mut heap: BinaryHeap<(std::cmp::Reverse<u64>, usize)> = BinaryHeap::new();
        for &v in seeds {
            dist[v] = 0.0;
            heap.push((std::cmp::Reverse(0), v));
        }
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
        dist
    };
    let rim: Vec<usize> = (0..nv).filter(|&v| in_sel[v] && in_other[v]).collect();
    // Una zona sin vecinos (toda la malla): no hay borde y el relieve no se funde
    let closed_zone = !rim.is_empty();
    let dist = spread(&rim);
    let near_prot: Vec<usize> = (0..nv).filter(|&v| in_sel[v] && in_prot[v]).collect();
    // Lejos de lo protegido: distancia en línea recta al borde protegido (por las aristas sobrestimaría hasta 40 %)
    let dist_prot = nearest_distance(&wpos, &near_prot, (cfg.margin_mm + feather_of(cfg)) / cfg.unit_mm, cfg.unit_mm);
    let feather = feather_of(cfg);

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
        let uv = if boxed {
            box_uv(wpos[v], normal[v].try_normalize().unwrap_or_else(Vector3::unit_z))
        } else {
            let Some(uv) = vuv[v] else { continue };
            uv
        };
        let w = if closed_zone { smooth(dist[v] / feather) } else { 1.0 } * smooth((dist_prot[v] - cfg.margin_mm) / feather);
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

    let mut tris_out: Vec<[usize; 3]> = faces.iter().map(|f| f.v).collect();
    let mut warnings = Vec::new();
    let triangles_skinned = tris_out.len();
    if let Some(tolerance) = cfg.decimate_mm {
        match decimate(&out, &tris_out, tolerance / cfg.unit_mm) {
            Some((positions, tris)) => {
                out = positions;
                tris_out = tris;
            }
            None => warnings.push("no se pudo simplificar sin abrir la malla: queda con todo el detalle".to_string()),
        }
    }
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
        triangles_skinned,
        edge_mm,
        max_displacement_mm: max_d,
        zone_faces: faces.iter().filter(|f| f.sel).count(),
        warnings,
    })
}

/// Simplifica sin pasar de `max_error` (unidades de la malla). `None` si el resultado no queda cerrado
fn decimate(pos: &[Vector3], tris: &[[usize; 3]], max_error: Real) -> Option<(Vec<Vector3>, Vec<[usize; 3]>)> {
    use quadriflow_core::remesh::{simplify, SimplifyInput, SimplifyOptions};
    let positions: Vec<[f32; 3]> = pos.iter().map(|p| [p.x() as f32, p.y() as f32, p.z() as f32]).collect();
    let indices: Vec<u32> = tris.iter().flat_map(|t| t.map(|i| i as u32)).collect();
    let input = SimplifyInput { positions: &positions, indices: &indices, attributes: &[], attribute_weights: &[], groups: &[] };
    let options = SimplifyOptions { target_triangles: 1, max_error: Some(max_error.max(0.0) as f32), lock_borders: true, keep_seams: true, aggressive: false };
    let result = simplify(&input, &options);
    let kept: Vec<[usize; 3]> = result.indices.as_chunks::<3>().0.iter().map(|t| [t[0] as usize, t[1] as usize, t[2] as usize]).collect();
    // Compactar los vértices que quedan
    let mut remap = vec![usize::MAX; pos.len()];
    let mut out_pos = Vec::new();
    let mut out_tris = Vec::with_capacity(kept.len());
    for t in &kept {
        let mut nt = [0; 3];
        for (k, &i) in t.iter().enumerate() {
            if remap[i] == usize::MAX {
                remap[i] = out_pos.len();
                out_pos.push(pos[i]);
            }
            nt[k] = remap[i];
        }
        out_tris.push(nt);
    }
    let mesh = Mesh::from_triangles(&out_pos, &out_tris);
    (mesh.is_closed() && out_tris.len() < tris.len()).then_some((out_pos, out_tris))
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
    fn la_proyeccion_en_caja_no_necesita_uv() {
        let (mesh, _, _) = box_soup();
        let map = Pattern::Lines.generate(64);
        // Sin UV: la tapa (mira a +z) toma (x, y) y la cara de y = 0 (mira a −y) toma (−x, −z)
        let r = apply_skin(&mesh, &[], &[2, 3, 4, 5], &map, &SkinConfig { mapping: SkinMapping::Box, ..cfg(1.0) }).unwrap();
        assert!(r.mesh.is_closed(), "la caja proyectada abrió la malla");
        let (mut top, mut front) = (0, 0);
        for v in r.mesh.vertices.iter() {
            let p = v.position;
            if p.z() > 9.0 && p.x() > 4.0 && p.x() < 36.0 && p.y() > 4.0 && p.y() < 36.0 {
                assert!((p.z() - (10.0 + map.sample(p.x() / 10.0, p.y() / 10.0))).abs() < 1e-6, "tapa en ({:.2}, {:.2})", p.x(), p.y());
                top += 1;
            }
            if p.y() < 0.0 + 1.0 && p.x() > 4.0 && p.x() < 36.0 && p.z() > 3.0 && p.z() < 7.0 {
                assert!((p.y() + map.sample(-p.x() / 10.0, -p.z() / 10.0)).abs() < 1e-6, "frente en ({:.2}, {:.2})", p.x(), p.z());
                front += 1;
            }
        }
        assert!(top > 300 && front > 100, "{top} {front}");
        // Pedir UV con el modo UV y sin UV sigue fallando
        assert!(apply_skin(&mesh, &[], &[2, 3], &map, &cfg(0.5)).is_err());
    }

    #[test]
    fn simplificar_despues_deja_menos_triangulos_y_el_relieve() {
        let (mesh, uvs, zone) = box_soup();
        let map = Pattern::Dots.generate(64);
        let full = apply_skin(&mesh, &uvs, &zone, &map, &cfg(0.8)).unwrap();
        let slim = apply_skin(&mesh, &uvs, &zone, &map, &SkinConfig { decimate_mm: Some(0.05), ..cfg(0.8) }).unwrap();
        assert!(slim.mesh.is_closed());
        assert_eq!(slim.triangles_skinned, full.triangles_after);
        assert!(slim.triangles_after < full.triangles_after, "{} de {}", slim.triangles_after, full.triangles_after);
        assert!(slim.warnings.is_empty(), "{:?}", slim.warnings);
        // El relieve sigue ahí (dentro de la tolerancia) y el volumen casi no cambia
        let top = |m: &Mesh| m.vertices.iter().map(|v| v.position.z()).fold(f64::MIN, f64::max);
        assert!(top(&slim.mesh) > top(&full.mesh) - 0.1, "{} vs {}", top(&slim.mesh), top(&full.mesh));
        let (vf, vs) = (crate::analysis::compute_volume(&full.mesh), crate::analysis::compute_volume(&slim.mesh));
        assert!((vf - vs).abs() < 0.02 * vf, "{vf} vs {vs}");
    }

    #[test]
    fn el_relieve_se_aleja_de_un_agujero_protegido() {
        use crate::planar_joints::{add_hole, footprint_polygon, PlanarShape};
        let b = crate::printability::tests::make_box(40.0, 40.0, 20.0);
        let poly = footprint_polygon(&PlanarShape::Round { diameter: 8.0 }, Vector3::new(20.0, 20.0, 20.0), Vector3::unit_z(), 0.0);
        let m = add_hole(&b, &poly, Vector3::unit_z(), 10.0, 1.0).unwrap();
        let holes = crate::zones::detect_cylindrical_holes(&m, 3.0, 20.0);
        assert_eq!(holes.len(), 1);
        let all: Vec<usize> = (0..m.num_faces()).collect();
        let map = Pattern::Dots.generate(64);
        // Toda la malla como zona: la pared del agujero y su suelo quedan protegidos con 2 mm de margen
        let protect: Vec<usize> = holes[0].faces.clone();
        let c = SkinConfig { tile_mm: 10.0, amplitude: 0.5, edge_mm: Some(0.8), feather_mm: Some(1.0), margin_mm: 2.0, ..Default::default() };
        let r = apply_skin_with(&m, &[], &all, &protect, &map, &SkinConfig { mapping: SkinMapping::Box, ..c.clone() }).unwrap();
        assert!(r.mesh.is_closed(), "la malla con el agujero protegido se abrió");
        assert!(r.max_displacement_mm > 0.3, "no hay relieve");
        // Ningún vértice de la tapa que se movió está a menos de radio + margen del eje
        let (mut moved, mut checked) = (0, 0);
        for v in r.mesh.vertices.iter() {
            let p = v.position;
            if p.z() > 20.0 + 1e-6 {
                moved += 1;
            }
            let d = ((p.x() - 20.0).powi(2) + (p.y() - 20.0).powi(2)).sqrt();
            if d < 4.0 + 2.0 - 0.3 && p.z() > 19.5 && p.z() < 20.0 + 0.6 {
                checked += 1;
                assert!((p.z() - 20.0).abs() < 1e-6 || d < 4.2, "vértice movido a {d} mm del eje: z = {}", p.z());
            }
        }
        assert!(moved > 100 && checked > 20, "{moved} {checked}");
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
