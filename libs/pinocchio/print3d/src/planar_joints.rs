//! Uniones reales sobre caras de corte planas: pernos en una pieza y huecos en la vecina
//!
//! No hace falta CSG general: la unión solo toca la tapa plana del corte. Se
//! recortan de la tapa los triángulos bajo la huella de la unión (polígono
//! convexo) y se extruye el borde resultante hacia afuera (perno) o hacia
//! adentro (hueco).

use std::collections::HashMap;

use pinocchio_math::{Real, Vector3};
use pinocchio_mesh::Mesh;
use pinocchio_spatial::{Bvh, Triangle};
use serde::{Deserialize, Serialize};

use crate::contact::{clip_half_plane, find_contacts, plane_basis, plane_tolerance, Contact};
use crate::error::{Print3dError, Result};
use crate::geom::{diagonal, normal_area, triangles, weld_ids};
use crate::labeling::LabeledPiece;

/// Forma de la huella de la unión
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlanarShape {
    /// Clavija cilíndrica
    Round { diameter: Real },
    /// Perno cuadrado (evita el giro)
    Square { size: Real },
    /// Trapecio: ancho en la base, ancho en la punta y largo
    Dovetail { width_base: Real, width_top: Real, length: Real },
}

impl PlanarShape {
    /// Mayor extensión de la huella (mm)
    pub fn extent(&self) -> Real {
        match *self {
            PlanarShape::Round { diameter } => diameter,
            PlanarShape::Square { size } => size * std::f64::consts::SQRT_2,
            PlanarShape::Dovetail { width_base, width_top, length } => width_base.max(width_top).hypot(length),
        }
    }

    /// La huella con cada medida sumada `delta` (negativo: más pequeña)
    fn resized(&self, delta: Real) -> PlanarShape {
        match *self {
            PlanarShape::Round { diameter } => PlanarShape::Round { diameter: (diameter + delta).max(0.1) },
            PlanarShape::Square { size } => PlanarShape::Square { size: (size + delta).max(0.1) },
            PlanarShape::Dovetail { width_base, width_top, length } => PlanarShape::Dovetail {
                width_base: (width_base + delta).max(0.1),
                width_top: (width_top + delta).max(0.1),
                length: (length + delta).max(0.1),
            },
        }
    }
}

/// Parámetros de las uniones
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanarJointConfig {
    pub shape: PlanarShape,
    /// Profundidad del hueco (mm); el perno mide lo mismo menos la holgura
    pub depth: Real,
    /// Holgura: negativa = el perno es más pequeño que el hueco (mm)
    pub tolerance: Real,
    /// Uniones por cara de contacto
    pub count: usize,
    /// Distancia mínima de la huella al borde de la cara y a la pared (mm)
    pub edge_margin: Real,
    /// Profundidad mínima útil; si el hueco no llega se omite la unión (mm)
    pub min_depth: Real,
    /// Área mínima de contacto para poner uniones (mm²)
    pub min_contact_area: Real,
    /// Giro de la huella dentro del plano (grados)
    pub rotation_deg: Real,
}

impl Default for PlanarJointConfig {
    fn default() -> Self {
        Self {
            shape: PlanarShape::Round { diameter: 6.0 },
            depth: 10.0,
            tolerance: -0.4,
            count: 2,
            edge_margin: 2.0,
            min_depth: 3.0,
            min_contact_area: 50.0,
            rotation_deg: 0.0,
        }
    }
}

impl PlanarJointConfig {
    fn gap(&self) -> Real {
        self.tolerance.abs().max(0.2)
    }
}

/// Qué pasó con las uniones de un contacto
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectionJoints {
    pub piece_a: usize,
    pub piece_b: usize,
    pub contact_area: Real,
    pub requested: usize,
    pub placed: usize,
    /// Motivos por los que no se pusieron todas
    pub notes: Vec<String>,
}

/// Resultado de poner uniones en un conjunto de piezas
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct JointsReport {
    pub joints_placed: usize,
    pub connections: Vec<ConnectionJoints>,
}

/// Polígono convexo (antihorario mirando contra `normal`) de la huella
pub fn footprint_polygon(shape: &PlanarShape, center: Vector3, normal: Vector3, rotation_deg: Real) -> Vec<Vector3> {
    let (u, v) = plane_basis(normal);
    let (s, c) = rotation_deg.to_radians().sin_cos();
    let pts2: Vec<(Real, Real)> = match *shape {
        PlanarShape::Round { diameter } => {
            let n = 24;
            (0..n)
                .map(|i| {
                    let a = std::f64::consts::TAU * i as Real / n as Real;
                    (diameter * 0.5 * a.cos(), diameter * 0.5 * a.sin())
                })
                .collect()
        }
        PlanarShape::Square { size } => {
            let h = size * 0.5;
            vec![(-h, -h), (h, -h), (h, h), (-h, h)]
        }
        PlanarShape::Dovetail { width_base, width_top, length } => {
            let l = length * 0.5;
            vec![(-width_base * 0.5, -l), (width_base * 0.5, -l), (width_top * 0.5, l), (-width_top * 0.5, l)]
        }
    };
    pts2.into_iter()
        .map(|(x, y)| {
            let (rx, ry) = (x * c - y * s, x * s + y * c);
            center + u * rx + v * ry
        })
        .collect()
}

/// Cabecera común: triángulos de la tapa (coplanares con la huella y de cara a `n`)
struct Cap {
    pos: Vec<Vector3>,
    tris: Vec<[usize; 3]>,
    is_cap: Vec<bool>,
    diag: Real,
    off: Real,
}

fn find_cap(mesh: &Mesh, polygon: &[Vector3], n: Vector3) -> Result<Cap> {
    let (pos, tris) = triangles(mesh);
    if tris.is_empty() {
        return Err(Print3dError::EmptyMesh);
    }
    let diag = diagonal(&pos);
    let ptol = (diag * 1e-6).max(1e-7);
    let off = n.dot(&polygon[0]);
    let is_cap = tris
        .iter()
        .map(|t| {
            let ps = [pos[t[0]], pos[t[1]], pos[t[2]]];
            ps.iter().all(|p| (n.dot(p) - off).abs() <= ptol) && normal_area(ps[0], ps[1], ps[2]).is_some_and(|(tn, _)| tn.dot(&n) > 0.999)
        })
        .collect();
    Ok(Cap { pos, tris, is_cap, diag, off })
}

/// ¿Está el punto `p` (del plano) dentro de algún triángulo de la tapa?
fn in_cap(cap: &Cap, p: Vector3, n: Vector3) -> bool {
    let eps = (cap.diag * 1e-9).max(1e-9);
    cap.tris.iter().zip(&cap.is_cap).filter(|(_, c)| **c).any(|(t, _)| {
        let (a, b, c) = (cap.pos[t[0]], cap.pos[t[1]], cap.pos[t[2]]);
        [(a, b), (b, c), (c, a)].iter().all(|(p0, p1)| (*p1 - *p0).cross(&(p - *p0)).dot(&n) >= -eps * (*p1 - *p0).length().max(1.0))
    })
}

/// Puntos de muestra: borde y relleno de la huella
fn sample_polygon(polygon: &[Vector3]) -> Vec<Vector3> {
    let c = polygon.iter().fold(Vector3::zero(), |s, p| s + *p) * (1.0 / polygon.len() as Real);
    let mut pts = vec![c];
    for i in 0..polygon.len() {
        let (a, b) = (polygon[i], polygon[(i + 1) % polygon.len()]);
        for k in 0..6 {
            let p = a.lerp(&b, k as Real / 6.0);
            pts.push(p);
            pts.push(c.lerp(&p, 0.5));
        }
    }
    pts
}

/// Recorta la tapa bajo la huella y extruye su borde `dir` (perno: hacia afuera; hueco: hacia adentro)
fn carve(mesh: &Mesh, polygon: &[Vector3], n: Vector3, dir: Vector3) -> Result<Mesh> {
    if polygon.len() < 3 {
        return Err(Print3dError::JointError("huella degenerada".into()));
    }
    let cap = find_cap(mesh, polygon, n)?;
    if !cap.is_cap.iter().any(|&c| c) {
        return Err(Print3dError::JointError("la pieza no tiene una cara plana en ese plano".into()));
    }
    if !sample_polygon(polygon).iter().all(|&p| in_cap(&cap, p, n)) {
        return Err(Print3dError::JointError("la unión no cabe dentro de la cara de corte".into()));
    }

    // Triángulos de la tapa que solapan la huella: se sustituye su unión (región S)
    // por S menos la huella, así no aparecen vértices nuevos en aristas ajenas
    let sides: Vec<(Vector3, Vector3)> = (0..polygon.len()).map(|i| (polygon[i], polygon[(i + 1) % polygon.len()] - polygon[i])).collect();
    let overlap_area = |tri: [Vector3; 3]| -> Real {
        let mut inside = tri.to_vec();
        for (p0, e) in &sides {
            inside = clip_half_plane(&inside, |p| e.cross(&(*p - *p0)).dot(&n));
            if inside.len() < 3 {
                return 0.0;
            }
        }
        crate::contact::polygon_area(&inside, n)
    };
    let area_tol = (cap.diag * cap.diag * 1e-12).max(1e-12);
    let in_s: Vec<bool> = cap
        .tris
        .iter()
        .zip(&cap.is_cap)
        .map(|(t, &c)| c && overlap_area([cap.pos[t[0]], cap.pos[t[1]], cap.pos[t[2]]]) > area_tol)
        .collect();
    if !in_s.iter().any(|&x| x) {
        return Err(Print3dError::JointError("la huella no toca la tapa".into()));
    }
    let tol = (cap.diag * 1e-9).max(1e-9);
    let wid = weld_ids(&cap.pos, tol);
    let mut edge_count: HashMap<(usize, usize), (u32, usize, usize)> = HashMap::new();
    for (t, _) in cap.tris.iter().zip(&in_s).filter(|(_, s)| **s) {
        for i in 0..3 {
            let (a, b) = (t[i], t[(i + 1) % 3]);
            let key = (wid[a].min(wid[b]), wid[a].max(wid[b]));
            let e = edge_count.entry(key).or_insert((0, a, b));
            e.0 += 1;
        }
    }
    let mut segments: Vec<(Vector3, Vector3)> = edge_count.values().filter(|e| e.0 == 1).map(|e| (cap.pos[e.1], cap.pos[e.2])).collect();
    // La huella debe quedar bien dentro de la región: lejos de su borde
    let near = |p: &Vector3| segments.iter().any(|(a, b)| point_segment_distance(p, a, b) < tol * 100.0);
    if polygon.iter().any(near) {
        return Err(Print3dError::JointError("la unión toca el borde de la cara de corte".into()));
    }
    for (p0, e) in &sides {
        segments.push((*p0, *p0 + *e));
    }
    let filled = crate::capping::cap_triangles(&segments, n, polygon[0], tol);
    if filled.is_empty() {
        return Err(Print3dError::JointError("no se pudo rehacer la tapa".into()));
    }
    let mut out_pos: Vec<Vector3> = cap.pos.clone();
    let mut out_tris: Vec<[usize; 3]> = cap.tris.iter().zip(&in_s).filter(|(_, s)| !**s).map(|(t, _)| *t).collect();
    for t in filled {
        let base = out_pos.len();
        out_pos.extend(t);
        out_tris.push([base, base + 1, base + 2]);
    }

    // Soldar y descartar triángulos degenerados
    let weld = weld_ids(&out_pos, (cap.diag * 1e-9).max(1e-9));
    let mut rep: HashMap<usize, Vector3> = HashMap::new();
    for (i, &w) in weld.iter().enumerate() {
        rep.entry(w).or_insert(out_pos[i]);
    }
    let mut tris: Vec<[usize; 3]> = out_tris.iter().map(|t| [weld[t[0]], weld[t[1]], weld[t[2]]]).filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2]).collect();
    let mut positions: Vec<Vector3> = (0..rep.len()).map(|w| rep[&w]).collect();

    // Borde abierto = contorno de la huella (aristas dirigidas sin su opuesta)
    let mut directed: HashMap<(usize, usize), u32> = HashMap::new();
    for t in &tris {
        for i in 0..3 {
            *directed.entry((t[i], t[(i + 1) % 3])).or_default() += 1;
        }
    }
    let boundary: Vec<(usize, usize)> = directed.keys().filter(|&&(a, b)| !directed.contains_key(&(b, a))).copied().collect();
    if boundary.is_empty() {
        return Err(Print3dError::JointError("la huella no abrió la tapa".into()));
    }
    let next: HashMap<usize, usize> = boundary.iter().copied().collect();
    if next.len() != boundary.len() {
        return Err(Print3dError::JointError("el contorno de la unión no es simple".into()));
    }
    // Recorrer los contornos de a→b
    let mut seen = std::collections::HashSet::new();
    let mut rings: Vec<Vec<usize>> = Vec::new();
    for &(start, _) in &boundary {
        if seen.contains(&start) {
            continue;
        }
        let mut ring = vec![start];
        seen.insert(start);
        let mut cur = next[&start];
        while cur != start {
            if !seen.insert(cur) || ring.len() > boundary.len() {
                return Err(Print3dError::JointError("el contorno de la unión no se cierra".into()));
            }
            ring.push(cur);
            cur = *next.get(&cur).ok_or_else(|| Print3dError::JointError("el contorno de la unión está roto".into()))?;
        }
        rings.push(ring);
    }
    if rings.len() != 1 {
        return Err(Print3dError::JointError("la unión abrió más de un contorno".into()));
    }
    let ring = &rings[0];

    // Paredes: por cada arista a→b del borde, el cuadrilátero (b, a, a+dir, b+dir)
    let top_start = positions.len();
    let lifted: Vec<Vector3> = ring.iter().map(|&i| positions[i] + dir).collect();
    positions.extend(lifted);
    let m = ring.len();
    for i in 0..m {
        let (a, b) = (ring[i], ring[(i + 1) % m]);
        let (ta, tb) = (top_start + i, top_start + (i + 1) % m);
        tris.push([b, a, ta]);
        tris.push([b, ta, tb]);
    }
    // Fondo (hueco) o punta (perno): abanico desde el centro, de cara a +n
    let center = ring.iter().fold(Vector3::zero(), |s, &i| s + positions[i]) * (1.0 / m as Real) + dir;
    let cidx = positions.len();
    positions.push(center);
    for i in 0..m {
        let (ta, tb) = (top_start + i, top_start + (i + 1) % m);
        tris.push([tb, ta, cidx]);
    }
    // El fondo del hueco mira hacia +n (hacia el vacío) y la punta del perno también: si
    // dir va contra n (hueco) el abanico anterior ya queda hacia +n; si va a favor (perno)
    // también. Se comprueba el sentido real por el área firmada
    let last = &tris[tris.len() - m..];
    let signed: Real = last
        .iter()
        .map(|t| {
            let (a, b, c) = (positions[t[0]], positions[t[1]], positions[t[2]]);
            (b - a).cross(&(c - a)).dot(&n)
        })
        .sum();
    if signed < 0.0 {
        let l = tris.len();
        for t in &mut tris[l - m..] {
            t.swap(1, 2);
        }
    }

    // Quitar vértices sin usar
    let mut used = vec![false; positions.len()];
    for t in &tris {
        for &i in t {
            used[i] = true;
        }
    }
    let mut remap = vec![usize::MAX; positions.len()];
    let mut compact = Vec::new();
    for (i, p) in positions.iter().enumerate() {
        if used[i] {
            remap[i] = compact.len();
            compact.push(*p);
        }
    }
    let tris: Vec<[usize; 3]> = tris.iter().map(|t| [remap[t[0]], remap[t[1]], remap[t[2]]]).collect();
    let result = Mesh::from_triangles(&compact, &tris);
    if !result.is_closed() {
        return Err(Print3dError::JointError("la malla resultante no quedó cerrada".into()));
    }
    let _ = cap.off;
    Ok(result)
}

/// Añade un perno de `height` mm saliendo de la tapa plana de normal `normal`
pub fn add_pin(mesh: &Mesh, polygon: &[Vector3], normal: Vector3, height: Real) -> Result<Mesh> {
    let n = normal.try_normalize().ok_or_else(|| Print3dError::JointError("normal nula".into()))?;
    if height <= 0.0 {
        return Err(Print3dError::JointError("altura del perno no positiva".into()));
    }
    carve(mesh, polygon, n, n * height)
}

/// Abre un hueco de `depth` mm hacia dentro de la tapa plana de normal `normal`
///
/// Falla si el hueco no cabe en la cara o si la pieza no tiene fondo suficiente.
pub fn add_hole(mesh: &Mesh, polygon: &[Vector3], normal: Vector3, depth: Real, min_wall: Real) -> Result<Mesh> {
    let n = normal.try_normalize().ok_or_else(|| Print3dError::JointError("normal nula".into()))?;
    if depth <= 0.0 {
        return Err(Print3dError::JointError("profundidad del hueco no positiva".into()));
    }
    let free = free_depth(mesh, polygon, n);
    if free < depth + min_wall {
        return Err(Print3dError::JointError(format!("la pieza solo tiene {free:.1} mm de fondo bajo la unión")));
    }
    carve(mesh, polygon, n, -n * depth)
}

/// Espacio libre bajo la huella: distancia mínima hacia dentro hasta otra superficie
pub fn free_depth(mesh: &Mesh, polygon: &[Vector3], normal: Vector3) -> Real {
    let (pos, tris) = triangles(mesh);
    let diag = diagonal(&pos);
    let bvh = Bvh::build(tris.iter().map(|t| Triangle::new(pos[t[0]], pos[t[1]], pos[t[2]])).collect());
    let dir = -normal;
    let eps = (diag * 1e-5).max(1e-6);
    sample_polygon(polygon)
        .iter()
        .filter_map(|p| bvh.ray_hit(&(*p + dir * eps), &dir, 0.0, Real::INFINITY).map(|(t, _)| t + eps))
        .fold(Real::INFINITY, Real::min)
}

/// Pone uniones (perno en la pieza de menor id, hueco en la otra) en todas las caras de contacto
pub fn apply_joints(pieces: &mut [LabeledPiece], config: &PlanarJointConfig) -> Result<JointsReport> {
    let tol = pieces.iter().find(|p| !p.mesh.vertices.is_empty()).map_or(1e-4, |p| plane_tolerance(&p.mesh));
    let contacts = find_contacts(pieces, config.min_contact_area, tol);
    let mut report = JointsReport::default();
    for contact in contacts {
        let mut conn = ConnectionJoints { piece_a: contact.piece_a, piece_b: contact.piece_b, contact_area: contact.area, requested: config.count, placed: 0, notes: Vec::new() };
        let Some(ia) = pieces.iter().position(|p| p.id == contact.piece_a) else { continue };
        let Some(ib) = pieces.iter().position(|p| p.id == contact.piece_b) else { continue };
        let spots = choose_spots(&contact, config);
        if spots.len() < config.count {
            conn.notes.push(format!("solo caben {} de {} uniones en la cara", spots.len(), config.count));
        }
        for center in spots {
            // Pin en A saliendo hacia B; hueco en B hacia dentro (normal exterior de B = −n)
            let n = contact.normal;
            let hole_poly = footprint_polygon(&config.shape, center, -n, config.rotation_deg);
            let free = free_depth(&pieces[ib].mesh, &hole_poly, -n);
            let wall = config.edge_margin.max(0.8);
            let depth = config.depth.min(free - wall);
            if depth < config.min_depth {
                conn.notes.push(format!("fondo insuficiente ({free:.1} mm) para una unión en ({:.1}, {:.1}, {:.1})", center.x(), center.y(), center.z()));
                continue;
            }
            // Pin: huella más pequeña por la holgura, en el sentido de A (normal n)
            let pin_shape = config.shape.resized(config.tolerance);
            let pin_poly = footprint_polygon(&pin_shape, center, n, -config.rotation_deg);
            let pin_height = (depth - config.gap()).max(0.5);
            let hole = add_hole(&pieces[ib].mesh, &hole_poly, -n, depth, wall);
            let pin = add_pin(&pieces[ia].mesh, &pin_poly, n, pin_height);
            match (hole, pin) {
                (Ok(h), Ok(p)) => {
                    pieces[ib].mesh = h;
                    pieces[ia].mesh = p;
                    conn.placed += 1;
                }
                (h, p) => {
                    let why = h.err().or(p.err()).map_or_else(String::new, |e| e.to_string());
                    conn.notes.push(format!("unión omitida: {why}"));
                }
            }
        }
        report.joints_placed += conn.placed;
        report.connections.push(conn);
    }
    Ok(report)
}

/// Elige hasta `count` posiciones dentro de la región de contacto, separadas entre sí
fn choose_spots(contact: &Contact, config: &PlanarJointConfig) -> Vec<Vector3> {
    let n = contact.normal;
    let (u, v) = plane_basis(n);
    let to2 = |p: &Vector3| (p.dot(&u), p.dot(&v));
    let region2: Vec<Vec<(Real, Real)>> = contact.region.iter().map(|poly| poly.iter().map(to2).collect()).collect();
    let (mut lo, mut hi) = ((Real::INFINITY, Real::INFINITY), (Real::NEG_INFINITY, Real::NEG_INFINITY));
    for poly in &region2 {
        for p in poly {
            lo = (lo.0.min(p.0), lo.1.min(p.1));
            hi = (hi.0.max(p.0), hi.1.max(p.1));
        }
    }
    if !lo.0.is_finite() {
        return Vec::new();
    }
    let extent = config.shape.extent();
    let reach = extent * 0.5 + config.edge_margin;
    let inside_region = |p: (Real, Real)| region2.iter().any(|poly| point_in_convex(p, poly));
    let mut cands: Vec<(Real, Real)> = Vec::new();
    let steps = 40;
    let step = ((hi.0 - lo.0).max(hi.1 - lo.1) / steps as Real).max(extent * 0.1).max(1e-3);
    let mut x = lo.0 + reach;
    while x <= hi.0 - reach + 1e-9 {
        let mut y = lo.1 + reach;
        while y <= hi.1 - reach + 1e-9 {
            // Anillo de comprobación alrededor del centro
            let ok = (0..12).all(|k| {
                let a = std::f64::consts::TAU * k as Real / 12.0;
                inside_region((x + reach * a.cos(), y + reach * a.sin()))
            }) && inside_region((x, y));
            if ok {
                cands.push((x, y));
            }
            y += step;
        }
        x += step;
    }
    if cands.is_empty() {
        return Vec::new();
    }
    let area: Real = contact.area.max(1e-9);
    let centroid2 = to2(&contact.center);
    let d2 = |a: (Real, Real), b: (Real, Real)| ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
    let mut chosen: Vec<(Real, Real)> = Vec::new();
    let min_sep = extent + config.edge_margin;
    for k in 0..config.count {
        let pick = if k == 0 {
            if config.count == 1 {
                cands.iter().copied().min_by(|a, b| d2(*a, centroid2).total_cmp(&d2(*b, centroid2)))
            } else {
                cands.iter().copied().max_by(|a, b| d2(*a, centroid2).total_cmp(&d2(*b, centroid2)))
            }
        } else {
            cands
                .iter()
                .copied()
                .filter(|c| chosen.iter().all(|s| d2(*c, *s) >= min_sep))
                .max_by(|a, b| chosen.iter().map(|s| d2(*a, *s)).fold(Real::INFINITY, Real::min).total_cmp(&chosen.iter().map(|s| d2(*b, *s)).fold(Real::INFINITY, Real::min)))
        };
        match pick {
            Some(p) => chosen.push(p),
            None => break,
        }
    }
    let _ = area;
    // Volver al 3D: el punto en el plano de contacto
    chosen.into_iter().map(|(a, b)| u * a + v * b + n * contact.offset).collect()
}

fn point_segment_distance(p: &Vector3, a: &Vector3, b: &Vector3) -> Real {
    let ab = *b - *a;
    let t = ((*p - *a).dot(&ab) / ab.length_squared().max(1e-30)).clamp(0.0, 1.0);
    (*p - (*a + ab * t)).length()
}

fn point_in_convex(p: (Real, Real), poly: &[(Real, Real)]) -> bool {
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let mut sign = 0.0;
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let c = (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0);
        if c.abs() < 1e-12 {
            continue;
        }
        if sign == 0.0 {
            sign = c.signum();
        } else if c.signum() != sign {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::compute_volume;
    use crate::printability::tests::make_box;
    use crate::slicer::{slice_by_plane, Plane};

    fn two_halves() -> (Mesh, Mesh) {
        let b = make_box(60.0, 60.0, 40.0);
        let (x, y) = slice_by_plane(&b, &Plane::xy_at_z(20.0)).unwrap();
        let zmin = |m: &Mesh| m.vertices.iter().map(|v| v.position.z()).fold(Real::INFINITY, Real::min);
        if zmin(&x) < zmin(&y) { (x, y) } else { (y, x) }
    }

    #[test]
    fn perno_suma_volumen_y_queda_cerrado() {
        let (lo, _) = two_halves();
        let v0 = compute_volume(&lo);
        let poly = footprint_polygon(&PlanarShape::Square { size: 10.0 }, Vector3::new(30.0, 30.0, 20.0), Vector3::unit_z(), 0.0);
        let m = add_pin(&lo, &poly, Vector3::unit_z(), 8.0).unwrap();
        assert!(m.is_closed());
        assert!((compute_volume(&m) - (v0 + 100.0 * 8.0)).abs() < 1e-6, "{} vs {}", compute_volume(&m), v0 + 800.0);
    }

    #[test]
    fn hueco_resta_volumen_y_queda_cerrado() {
        let (_, hi) = two_halves();
        let v0 = compute_volume(&hi);
        let poly = footprint_polygon(&PlanarShape::Square { size: 10.0 }, Vector3::new(30.0, 30.0, 20.0), -Vector3::unit_z(), 0.0);
        let m = add_hole(&hi, &poly, -Vector3::unit_z(), 8.0, 1.0).unwrap();
        assert!(m.is_closed());
        assert!((compute_volume(&m) - (v0 - 800.0)).abs() < 1e-6, "{} vs {}", compute_volume(&m), v0 - 800.0);
    }

    #[test]
    fn hueco_redondo_cerrado() {
        let (_, hi) = two_halves();
        let poly = footprint_polygon(&PlanarShape::Round { diameter: 8.0 }, Vector3::new(25.0, 31.0, 20.0), -Vector3::unit_z(), 17.0);
        let m = add_hole(&hi, &poly, -Vector3::unit_z(), 6.0, 1.0).unwrap();
        assert!(m.is_closed());
        let expected = 24.0 * 0.5 * 4.0 * 4.0 * (std::f64::consts::TAU / 24.0).sin() * 6.0;
        assert!((compute_volume(&hi) - compute_volume(&m) - expected).abs() < 1e-6);
    }

    #[test]
    fn hueco_demasiado_hondo_falla() {
        let (_, hi) = two_halves();
        let poly = footprint_polygon(&PlanarShape::Round { diameter: 6.0 }, Vector3::new(30.0, 30.0, 20.0), -Vector3::unit_z(), 0.0);
        assert!(add_hole(&hi, &poly, -Vector3::unit_z(), 25.0, 1.0).is_err());
    }

    #[test]
    fn huella_fuera_de_la_cara_falla() {
        let (lo, _) = two_halves();
        let poly = footprint_polygon(&PlanarShape::Round { diameter: 10.0 }, Vector3::new(2.0, 30.0, 20.0), Vector3::unit_z(), 0.0);
        assert!(add_pin(&lo, &poly, Vector3::unit_z(), 5.0).is_err());
    }

    #[test]
    fn uniones_entre_piezas_de_un_corte() {
        let b = make_box(80.0, 60.0, 50.0);
        let meshes = crate::slicer::slice_by_planes(&b, &[Plane::xy_at_z(25.0)]).unwrap();
        let mut pieces: Vec<LabeledPiece> = meshes.into_iter().enumerate().map(|(id, mesh)| LabeledPiece { mesh, id, ..Default::default() }).collect();
        let total0: Real = pieces.iter().map(|p| compute_volume(&p.mesh)).sum();
        let report = apply_joints(&mut pieces, &PlanarJointConfig::default()).unwrap();
        assert_eq!(report.connections.len(), 1, "{report:?}");
        assert_eq!(report.joints_placed, 2, "{report:?}");
        for p in &pieces {
            assert!(p.mesh.is_closed());
        }
        // el volumen total baja la holgura de los pernos respecto al hueco
        let total1: Real = pieces.iter().map(|p| compute_volume(&p.mesh)).sum();
        let poly_area = |r: Real| 0.5 * 24.0 * r * r * (std::f64::consts::TAU / 24.0).sin();
        let per_joint = poly_area(3.0) * 10.0 - poly_area(2.8) * 9.6;
        assert!((total0 - total1 - 2.0 * per_joint).abs() < 1e-6, "{total0} -> {total1}, esperado {}", 2.0 * per_joint);
    }

    #[test]
    fn cara_pequena_no_recibe_union() {
        let b = make_box(8.0, 8.0, 40.0);
        let meshes = crate::slicer::slice_by_planes(&b, &[Plane::xy_at_z(20.0)]).unwrap();
        let mut pieces: Vec<LabeledPiece> = meshes.into_iter().enumerate().map(|(id, mesh)| LabeledPiece { mesh, id, ..Default::default() }).collect();
        let report = apply_joints(&mut pieces, &PlanarJointConfig::default()).unwrap();
        assert_eq!(report.joints_placed, 0);
    }

    /// Esfera UV de radio `r` centrada en el origen, normales hacia afuera
    fn uv_sphere(r: Real, rings: usize, segs: usize) -> Mesh {
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
        Mesh::from_triangles(&pos, &tris)
    }

    #[test]
    fn uniones_en_una_esfera_cortada() {
        let s = uv_sphere(40.0, 48, 64);
        assert!(s.is_closed());
        let meshes = crate::slicer::slice_by_planes(&s, &[Plane::xy_at_z(3.0)]).unwrap();
        let mut pieces: Vec<LabeledPiece> = meshes.into_iter().enumerate().map(|(id, mesh)| LabeledPiece { mesh, id, ..Default::default() }).collect();
        for p in &pieces {
            assert!(p.mesh.is_closed(), "corte abierto");
        }
        let cfg = PlanarJointConfig { count: 3, depth: 12.0, ..Default::default() };
        let report = apply_joints(&mut pieces, &cfg).unwrap();
        assert_eq!(report.joints_placed, 3, "{report:?}");
        for p in &pieces {
            assert!(p.mesh.is_closed());
            assert!(compute_volume(&p.mesh) > 0.0);
        }
    }
}
