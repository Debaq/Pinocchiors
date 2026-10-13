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
    /// Cierre de cuarto de vuelta: un perno con cabeza de barra entra por una ranura de la
    /// misma forma, y al girarlo 90° la cabeza queda atrapada bajo el labio de la ranura
    Bayonet { length: Real, width: Real },
    /// Cola de milano deslizante: riel en una pieza y ranura en la otra, de pared a pared de la
    /// cara de corte. `width` es el ancho de la boca; la ranura se ensancha hacia el fondo
    Rail { width: Real },
    /// Terraza: perno y hueco escalonados y concéntricos (ancho `diameter` abajo y 0,6 de eso arriba)
    Terrace { diameter: Real },
    /// Rompecabezas: cabeza redonda de diámetro `size` sobre un cuello angosto; no es convexa y no
    /// tiene simetría, así que perno y hueco se arman con la misma huella en el mundo
    Puzzle { size: Real },
}

impl PlanarShape {
    /// Mayor extensión de la huella (mm)
    pub fn extent(&self) -> Real {
        match *self {
            PlanarShape::Round { diameter } => diameter,
            PlanarShape::Square { size } => size * std::f64::consts::SQRT_2,
            PlanarShape::Dovetail { width_base, width_top, length } => width_base.max(width_top).hypot(length),
            // La cámara redonda de abajo, un poco más ancha que la diagonal de la barra
            PlanarShape::Bayonet { length, width } => length.hypot(width) + 2.0,
            PlanarShape::Rail { width } => width,
            PlanarShape::Terrace { diameter } => diameter,
            PlanarShape::Puzzle { size } => size * 1.6,
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
            PlanarShape::Bayonet { length, width } => PlanarShape::Bayonet { length: (length + delta).max(0.1), width: (width + delta).max(0.1) },
            PlanarShape::Rail { width } => PlanarShape::Rail { width: (width + delta).max(0.1) },
            PlanarShape::Terrace { diameter } => PlanarShape::Terrace { diameter: (diameter + delta).max(0.1) },
            PlanarShape::Puzzle { size } => PlanarShape::Puzzle { size: (size + delta).max(0.1) },
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
    /// Riel con tope: la ranura entra por una pared y termina ciega 2 mm antes de la otra, así el riel no se pasa
    #[serde(default)]
    pub rail_stop: bool,
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
            rail_stop: false,
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

/// Contorno 2D (antihorario) del rompecabezas: cuello de ancho 0,8·r abajo y cabeza redonda de radio `r = size/2`
/// arriba, centrado en la mitad de su caja (alto total 3,2·r)
fn puzzle_outline(size: Real) -> Vec<(Real, Real)> {
    let r = size * 0.5;
    let w = 0.4 * r;
    let c = 0.6 * r;
    let y0 = -1.6 * r;
    let x = (w / r).acos();
    let mut pts = vec![(-w, y0), (w, y0)];
    let n = 20;
    for i in 0..=n {
        let phi = -x + (std::f64::consts::PI + 2.0 * x) * i as Real / n as Real;
        pts.push((r * phi.cos(), c + r * phi.sin()));
    }
    pts
}

/// Polígono (antihorario mirando contra `normal`) de la huella; solo el del rompecabezas no es convexo
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
        PlanarShape::Bayonet { length, width } => {
            let (l, w) = (length * 0.5, width * 0.5);
            vec![(-l, -w), (l, -w), (l, w), (-l, w)]
        }
        // El riel no tiene una huella fija (corre de pared a pared): se da su boca como cuadrado
        PlanarShape::Rail { width } => {
            let h = width * 0.5;
            vec![(-h, -h), (h, -h), (h, h), (-h, h)]
        }
        // La base de la terraza (la parte ancha)
        PlanarShape::Terrace { diameter } => return footprint_polygon(&PlanarShape::Round { diameter }, center, normal, rotation_deg),
        PlanarShape::Puzzle { size } => puzzle_outline(size),
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
    Ok(Cap { pos, tris, is_cap, diag })
}

/// ¿Está el punto `p` (del plano) dentro de algún triángulo de la tapa?
fn in_cap(cap: &Cap, p: Vector3, n: Vector3) -> bool {
    let eps = (cap.diag * 1e-9).max(1e-9);
    cap.tris.iter().zip(&cap.is_cap).filter(|(_, c)| **c).any(|(t, _)| {
        let (a, b, c) = (cap.pos[t[0]], cap.pos[t[1]], cap.pos[t[2]]);
        [(a, b), (b, c), (c, a)].iter().all(|(p0, p1)| (*p1 - *p0).cross(&(p - *p0)).dot(&n) >= -eps * (*p1 - *p0).length().max(1.0))
    })
}

/// Normal del polígono (regla de Newell), sin normalizar
fn polygon_normal(polygon: &[Vector3]) -> Vector3 {
    (0..polygon.len()).fold(Vector3::zero(), |s, i| s + polygon[i].cross(&polygon[(i + 1) % polygon.len()]))
}

/// Triángulos que cubren la huella (sirve también si no es convexa)
fn footprint_triangles(polygon: &[Vector3], n: Vector3) -> Vec<[Vector3; 3]> {
    let segments: Vec<(Vector3, Vector3)> = (0..polygon.len()).map(|i| (polygon[i], polygon[(i + 1) % polygon.len()])).collect();
    let tol = (diagonal(polygon) * 1e-9).max(1e-9);
    crate::capping::cap_triangles(&segments, n, polygon[0], tol)
}

/// Puntos de muestra: borde y relleno de la huella
fn sample_polygon(polygon: &[Vector3]) -> Vec<Vector3> {
    let mut pts = Vec::new();
    for i in 0..polygon.len() {
        let (a, b) = (polygon[i], polygon[(i + 1) % polygon.len()]);
        for k in 0..6 {
            pts.push(a.lerp(&b, k as Real / 6.0));
        }
    }
    let normal = polygon_normal(polygon).try_normalize().unwrap_or_else(Vector3::unit_z);
    for t in footprint_triangles(polygon, normal) {
        let c = (t[0] + t[1] + t[2]) * (1.0 / 3.0);
        pts.push(c);
        pts.extend(t.iter().map(|v| c.lerp(v, 0.5)));
    }
    pts
}

/// Malla con la tapa abierta bajo la huella: posiciones, triángulos y el contorno del
/// agujero (índices, recorrido con el agujero a la derecha, es decir horario mirando contra la normal)
pub(crate) type OpenedCap = (Vec<Vector3>, Vec<[usize; 3]>, Vec<usize>);

/// Como `OpenedCap` con un contorno por huella
pub(crate) type OpenedCaps = (Vec<Vector3>, Vec<[usize; 3]>, Vec<Vec<usize>>);

/// Quita de la tapa plana (normal exterior `n`) lo que queda bajo la huella
pub(crate) fn open_cap(mesh: &Mesh, polygon: &[Vector3], n: Vector3) -> Result<OpenedCap> {
    let (positions, tris, mut rings) = open_cap_multi(mesh, &[polygon.to_vec()], n)?;
    Ok((positions, tris, rings.remove(0)))
}

/// Como `open_cap` con varias huellas separadas: devuelve un contorno por huella, en el mismo orden
pub(crate) fn open_cap_multi(mesh: &Mesh, polygons: &[Vec<Vector3>], n: Vector3) -> Result<OpenedCaps> {
    if polygons.is_empty() || polygons.iter().any(|p| p.len() < 3) {
        return Err(Print3dError::JointError("huella degenerada".into()));
    }
    let cap = find_cap(mesh, &polygons[0], n)?;
    if !cap.is_cap.iter().any(|&c| c) {
        return Err(Print3dError::JointError("la pieza no tiene una cara plana en ese plano".into()));
    }
    if !polygons.iter().flat_map(|p| sample_polygon(p)).all(|p| in_cap(&cap, p, n)) {
        return Err(Print3dError::JointError("la unión no cabe dentro de la cara de corte".into()));
    }

    // Triángulos de la tapa que solapan la huella: se sustituye su unión (región S)
    // por S menos la huella, así no aparecen vértices nuevos en aristas ajenas
    let sides: Vec<(Vector3, Vector3)> = polygons.iter().flat_map(|polygon| (0..polygon.len()).map(move |i| (polygon[i], polygon[(i + 1) % polygon.len()] - polygon[i]))).collect();
    let fp_tris: Vec<[Vector3; 3]> = polygons.iter().flat_map(|p| footprint_triangles(p, n)).collect();
    let overlap_area = |tri: [Vector3; 3]| -> Real {
        fp_tris
            .iter()
            .map(|ft| {
                let mut inside = tri.to_vec();
                for k in 0..3 {
                    let (p0, e) = (ft[k], ft[(k + 1) % 3] - ft[k]);
                    inside = clip_half_plane(&inside, |p| e.cross(&(*p - p0)).dot(&n));
                    if inside.len() < 3 {
                        return 0.0;
                    }
                }
                crate::contact::polygon_area(&inside, n)
            })
            .sum()
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
    if polygons.iter().flatten().any(near) {
        return Err(Print3dError::JointError("la unión toca el borde de la cara de corte".into()));
    }
    for (p0, e) in &sides {
        segments.push((*p0, *p0 + *e));
    }
    let filled = crate::capping::cap_triangles(&segments, n, polygons[0][0], tol);
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
    let tris: Vec<[usize; 3]> = out_tris.iter().map(|t| [weld[t[0]], weld[t[1]], weld[t[2]]]).filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2]).collect();
    let positions: Vec<Vector3> = (0..rep.len()).map(|w| rep[&w]).collect();

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
    if rings.len() != polygons.len() {
        return Err(Print3dError::JointError("la unión abrió más de un contorno".into()));
    }
    // Un contorno por huella: el que tiene más cerca su centro
    let center_of = |poly: &[Vector3]| poly.iter().fold(Vector3::zero(), |s, p| s + *p) * (1.0 / poly.len() as Real);
    let ring_center = |r: &[usize]| r.iter().fold(Vector3::zero(), |s, &i| s + positions[i]) * (1.0 / r.len() as Real);
    let mut ordered: Vec<Vec<usize>> = Vec::with_capacity(polygons.len());
    for poly in polygons {
        let c = center_of(poly);
        let (k, _) = rings
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| (ring_center(a) - c).length().total_cmp(&(ring_center(b) - c).length()))
            .ok_or_else(|| Print3dError::JointError("falta un contorno".into()))?;
        ordered.push(rings.swap_remove(k));
    }
    Ok((positions, tris, ordered))
}

/// Recorta la tapa bajo la huella y extruye su borde `dir` (perno: hacia afuera; hueco: hacia adentro)
fn carve(mesh: &Mesh, polygon: &[Vector3], n: Vector3, dir: Vector3) -> Result<Mesh> {
    let (mut positions, mut tris, ring) = open_cap(mesh, polygon, n)?;
    let ring = &ring;

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
    // Fondo (hueco) o punta (perno): la huella a esa altura, de cara a +n
    let diag = diagonal(&positions);
    let top: Vec<Vector3> = (top_start..top_start + m).map(|i| positions[i]).collect();
    region_tris(&[top], n, diag, &mut positions, &mut tris);

    weld_build(&positions, &tris)
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

/// Espacio libre bajo la huella como `free_depth`, pero 0 si algún punto cae fuera del sólido
/// (el primer corte hacia adentro tiene que ser una cara que se sale, no una que se entra)
fn free_depth_inside(mesh: &Mesh, polygon: &[Vector3], normal: Vector3) -> Real {
    let (pos, tris) = triangles(mesh);
    let diag = diagonal(&pos);
    let bvh = Bvh::build(tris.iter().map(|t| Triangle::new(pos[t[0]], pos[t[1]], pos[t[2]])).collect());
    let dir = -normal;
    let eps = (diag * 1e-5).max(1e-6);
    let mut free = Real::INFINITY;
    for p in sample_polygon(polygon) {
        match bvh.ray_hit(&(p + dir * eps), &dir, 0.0, Real::INFINITY) {
            Some((t, hit)) if bvh.triangle(hit).normal().dot(&dir) > 0.0 => free = free.min(t + eps),
            _ => return 0.0,
        }
    }
    free
}

/// Soldar por posición y cerrar la malla armada con triángulos sueltos
pub(crate) fn weld_build(positions: &[Vector3], tris: &[[usize; 3]]) -> Result<Mesh> {
    let diag = diagonal(positions);
    let weld = weld_ids(positions, (diag * 1e-9).max(1e-9));
    let mut rep: HashMap<usize, Vector3> = HashMap::new();
    for (i, &w) in weld.iter().enumerate() {
        rep.entry(w).or_insert(positions[i]);
    }
    let merged: Vec<[usize; 3]> = tris.iter().map(|t| [weld[t[0]], weld[t[1]], weld[t[2]]]).filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2]).collect();
    let mut used = vec![false; rep.len()];
    for t in &merged {
        for &i in t {
            used[i] = true;
        }
    }
    let mut remap = vec![usize::MAX; rep.len()];
    let mut compact = Vec::new();
    for w in 0..rep.len() {
        if used[w] {
            remap[w] = compact.len();
            compact.push(rep[&w]);
        }
    }
    let tris: Vec<[usize; 3]> = merged.iter().map(|t| [remap[t[0]], remap[t[1]], remap[t[2]]]).collect();
    let tris = flip_slivers(&compact, tris);
    let mesh = Mesh::from_triangles(&compact, &tris);
    if !mesh.is_closed() {
        return Err(Print3dError::JointError("la malla resultante no quedó cerrada".into()));
    }
    Ok(mesh)
}

/// Quita los triángulos de área cero (tres puntos en línea, que deja la triangulación de un borde con puntos
/// intermedios): el vecino de su lado largo se parte en dos por el punto del medio, y la malla sigue cerrada
pub(crate) fn flip_slivers(pos: &[Vector3], mut tris: Vec<[usize; 3]>) -> Vec<[usize; 3]> {
    let diag = diagonal(pos);
    let eps = 1e-12 * diag * diag + 1e-14;
    for _ in 0..tris.len().max(1) {
        let Some(ti) = tris.iter().position(|t| (pos[t[1]] - pos[t[0]]).cross(&(pos[t[2]] - pos[t[0]])).length() <= eps) else { break };
        let t = tris[ti];
        // Girar para que t = [p, q, m] con p→q el lado más largo
        let lens = [(pos[t[1]] - pos[t[0]]).length(), (pos[t[2]] - pos[t[1]]).length(), (pos[t[0]] - pos[t[2]]).length()];
        let long = (0..3).max_by(|&a, &b| lens[a].total_cmp(&lens[b])).unwrap_or(0);
        let (p, q, m) = (t[long], t[(long + 1) % 3], t[(long + 2) % 3]);
        let Some(ui) = tris.iter().position(|u| (0..3).any(|k| u[k] == q && u[(k + 1) % 3] == p)) else { break };
        let u = tris[ui];
        let k = (0..3).find(|&k| u[k] == q && u[(k + 1) % 3] == p).unwrap_or(0);
        let x = u[(k + 2) % 3];
        let (hi, lo) = (ti.max(ui), ti.min(ui));
        tris.swap_remove(hi);
        tris.swap_remove(lo);
        tris.push([q, m, x]);
        tris.push([m, p, x]);
    }
    tris
}

/// Triángulos de una región plana con huecos, sacados de sus contornos (cada uno cerrado); salen
/// antihorarios mirando contra `normal`
fn region_tris(rings: &[Vec<Vector3>], normal: Vector3, diag: Real, out_pos: &mut Vec<Vector3>, out_tris: &mut Vec<[usize; 3]>) {
    let segments: Vec<(Vector3, Vector3)> = rings.iter().flat_map(|r| (0..r.len()).map(move |i| (r[i], r[(i + 1) % r.len()]))).collect();
    for t in crate::capping::cap_triangles(&segments, normal, rings[0][0], (diag * 1e-9).max(1e-9)) {
        let base = out_pos.len();
        out_pos.extend(t);
        out_tris.push([base, base + 1, base + 2]);
    }
}

/// Paredes entre `ring` (antihorario mirando contra `n`) y `ring + dir`. Sus normales son
/// `(q − p) × dir`: hacia afuera con `dir = +n·h` (cuerpo sólido) y hacia el eje con `dir = −n·h` (vacío)
fn prism_walls(ring: &[Vector3], dir: Vector3, out_pos: &mut Vec<Vector3>, out_tris: &mut Vec<[usize; 3]>) {
    let m = ring.len();
    let base = out_pos.len();
    out_pos.extend(ring.iter().copied());
    out_pos.extend(ring.iter().map(|p| *p + dir));
    for i in 0..m {
        let j = (i + 1) % m;
        let (p, q, pt, qt) = (base + i, base + j, base + m + i, base + m + j);
        out_tris.push([p, q, qt]);
        out_tris.push([p, qt, pt]);
    }
}

/// Perno de cierre de cuarto de vuelta: tallo redondo de `stem_r` y `h1` de largo, y encima una
/// cabeza de barra `head` (largo, ancho) de `head_t` de grosor
#[allow(clippy::too_many_arguments)]
pub fn add_bayonet_pin(mesh: &Mesh, center: Vector3, normal: Vector3, rotation_deg: Real, head: (Real, Real), stem_r: Real, h1: Real, head_t: Real) -> Result<Mesh> {
    let n = normal.try_normalize().ok_or_else(|| Print3dError::JointError("normal nula".into()))?;
    if stem_r <= 0.0 || h1 <= 0.0 || head_t <= 0.0 {
        return Err(Print3dError::JointError("medidas del perno no positivas".into()));
    }
    let stem = footprint_polygon(&PlanarShape::Round { diameter: stem_r * 2.0 }, center, n, 0.0);
    let (mut pos, mut tris, ring) = open_cap(mesh, &stem, n)?;
    let diag = diagonal(&pos);
    // Tallo: de la tapa a h1 (mismas paredes que un perno redondo)
    let ring_pts: Vec<Vector3> = ring.iter().map(|&i| pos[i]).collect();
    let stem_top: Vec<Vector3> = ring_pts.iter().map(|p| *p + n * h1).collect();
    let m = ring.len();
    let base = pos.len();
    pos.extend(stem_top.iter().copied());
    for i in 0..m {
        let (a, b) = (ring[i], ring[(i + 1) % m]);
        tris.push([b, a, base + i]);
        tris.push([b, base + i, base + (i + 1) % m]);
    }
    // Cabeza: barra rectangular sobre el tallo; su cara de abajo lleva el agujero del tallo
    let bar = footprint_polygon(&PlanarShape::Bayonet { length: head.0, width: head.1 }, center, n, rotation_deg);
    let bar_low: Vec<Vector3> = bar.iter().map(|p| *p + n * h1).collect();
    // El tallo (horario mirando contra n, como sale de la tapa) es el hueco de la cara de abajo
    region_tris(&[bar_low.clone(), stem_top.clone()], -n, diag, &mut pos, &mut tris);
    prism_walls(&bar_low, n * head_t, &mut pos, &mut tris);
    let bar_top: Vec<Vector3> = bar_low.iter().map(|p| *p + n * head_t).collect();
    region_tris(&[bar_top], n, diag, &mut pos, &mut tris);
    weld_build(&pos, &tris)
}

/// Ranura de cierre de cuarto de vuelta: boca rectangular `(len, wid)` de `d1` de hondo, y debajo
/// una cámara redonda de `chamber_r` hasta `d2` donde la barra puede girar
#[allow(clippy::too_many_arguments)]
pub fn add_bayonet_hole(mesh: &Mesh, center: Vector3, normal: Vector3, rotation_deg: Real, mouth: (Real, Real), chamber_r: Real, d1: Real, d2: Real, min_wall: Real) -> Result<Mesh> {
    let n = normal.try_normalize().ok_or_else(|| Print3dError::JointError("normal nula".into()))?;
    if !(d2 > d1 && d1 > 0.0) || chamber_r <= mouth.0.hypot(mouth.1) * 0.5 {
        return Err(Print3dError::JointError("medidas de la ranura no válidas".into()));
    }
    let rect = footprint_polygon(&PlanarShape::Bayonet { length: mouth.0, width: mouth.1 }, center, n, rotation_deg);
    let chamber = footprint_polygon(&PlanarShape::Round { diameter: chamber_r * 2.0 }, center, n, 0.0);
    let free = free_depth_inside(mesh, &chamber, n);
    if free < d2 + min_wall {
        return Err(Print3dError::JointError(format!("la pieza solo tiene {free:.1} mm de fondo bajo la unión")));
    }
    let (mut pos, mut tris, ring) = open_cap(mesh, &rect, n)?;
    let diag = diagonal(&pos);
    // Boca: de la tapa a d1 hacia adentro
    let down1 = -n * d1;
    let ring_pts: Vec<Vector3> = ring.iter().map(|&i| pos[i]).collect();
    let mouth_low: Vec<Vector3> = ring_pts.iter().map(|p| *p + down1).collect();
    let m = ring.len();
    let base = pos.len();
    pos.extend(mouth_low.iter().copied());
    for i in 0..m {
        let (a, b) = (ring[i], ring[(i + 1) % m]);
        tris.push([b, a, base + i]);
        tris.push([b, base + i, base + (i + 1) % m]);
    }
    // Labio: entre la cámara y la boca, mirando hacia abajo (al vacío)
    let chamber_top: Vec<Vector3> = chamber.iter().map(|p| *p + down1).collect();
    region_tris(&[chamber_top.clone(), mouth_low], -n, diag, &mut pos, &mut tris);
    // Cámara: paredes hacia el eje y suelo hacia arriba
    prism_walls(&chamber_top, -n * (d2 - d1), &mut pos, &mut tris);
    let chamber_floor: Vec<Vector3> = chamber_top.iter().map(|p| *p - n * (d2 - d1)).collect();
    region_tris(&[chamber_floor], n, diag, &mut pos, &mut tris);
    weld_build(&pos, &tris)
}

/// Riel de cola de milano sobre la tapa plana de normal `normal`: corre a lo largo de `along`,
/// mide `length`, tiene `root_half` de semiancho en la tapa y `tip_half` (mayor) en la punta,
/// a `height` de la tapa
#[allow(clippy::too_many_arguments)]
pub fn add_rail(mesh: &Mesh, center: Vector3, normal: Vector3, along: Vector3, root_half: Real, tip_half: Real, height: Real, length: Real) -> Result<Mesh> {
    let n = normal.try_normalize().ok_or_else(|| Print3dError::JointError("normal nula".into()))?;
    let s = along.try_normalize().ok_or_else(|| Print3dError::JointError("dirección nula".into()))?;
    if !(root_half > 0.0 && tip_half >= root_half && height > 0.0 && length > 0.0) {
        return Err(Print3dError::JointError("medidas del riel no válidas".into()));
    }
    let w = n.cross(&s).normalize();
    let half = length * 0.5;
    let at = |sl: Real, wl: Real, h: Real| center + s * sl + w * wl + n * h;
    let root = [at(-half, -root_half, 0.0), at(half, -root_half, 0.0), at(half, root_half, 0.0), at(-half, root_half, 0.0)];
    let (mut pos, mut tris, ring) = open_cap(mesh, &root, n)?;
    let diag = diagonal(&pos);
    // Cada vértice del contorno sube a su esquina de la punta (más ancha)
    let top_of = |p: &Vector3| {
        let d = *p - center;
        let (sl, wl) = (d.dot(&s), d.dot(&w));
        at(sl, wl.signum() * tip_half, height)
    };
    let m = ring.len();
    let base = pos.len();
    let tops: Vec<Vector3> = ring.iter().map(|&i| top_of(&pos[i])).collect();
    pos.extend(tops);
    for i in 0..m {
        let (a, b) = (ring[i], ring[(i + 1) % m]);
        tris.push([b, a, base + i]);
        tris.push([b, base + i, base + (i + 1) % m]);
    }
    let top_ring = [at(-half, -tip_half, height), at(half, -tip_half, height), at(half, tip_half, height), at(-half, tip_half, height)];
    region_tris(&[top_ring.to_vec()], n, diag, &mut pos, &mut tris);
    weld_build(&pos, &tris)
}

/// Un riel entre dos piezas: la línea de la cara por la que corre y su cuerda
struct RailLine {
    /// Punto de la línea en el plano de contacto, a la mitad de la cuerda
    mid: Vector3,
    along: Vector3,
    /// Largo de la cuerda dentro de la cara (mm)
    chord: Real,
}

/// Líneas de riel dentro de la región de contacto: `count` paralelas, centradas en el contacto
fn rail_lines(contact: &Contact, config: &PlanarJointConfig, width: Real) -> (Vec<RailLine>, Vec<String>) {
    let n = contact.normal;
    let (u, v) = plane_basis(n);
    let (sr, cr) = config.rotation_deg.to_radians().sin_cos();
    let along = u * cr + v * sr;
    let lateral = n.cross(&along).normalize();
    let to2 = |p: &Vector3| (p.dot(&u), p.dot(&v));
    let region2: Vec<Vec<(Real, Real)>> = contact.region.iter().map(|poly| poly.iter().map(to2).collect()).collect();
    let inside = |p: &Vector3| {
        let q = to2(p);
        region2.iter().any(|poly| point_in_convex(q, poly))
    };
    let spacing = width * 4.0;
    let mut lines = Vec::new();
    let mut notes = Vec::new();
    let step = 0.25;
    let reach = contact.area.sqrt() * 4.0 + 10.0;
    for k in 0..config.count.max(1) {
        let off = (k as Real - (config.count.max(1) as Real - 1.0) * 0.5) * spacing;
        let origin = contact.center + lateral * off;
        if !inside(&origin) {
            notes.push("el centro de la cara no está dentro de ella (hueco al medio): no hay riel".to_string());
            continue;
        }
        let walk = |dir: Real| {
            let mut t = 0.0;
            while t < reach && inside(&(origin + along * ((t + step) * dir))) && {
                // el borde de la riel también tiene que estar dentro
                let e = origin + along * ((t + step) * dir);
                inside(&(e + lateral * width)) && inside(&(e - lateral * width))
            } {
                t += step;
            }
            t
        };
        let (fwd, back) = (walk(1.0), walk(-1.0));
        lines.push(RailLine { mid: origin + along * ((fwd - back) * 0.5), along, chord: fwd + back });
    }
    (lines, notes)
}

/// Un riel entre `ia` (riel) e `ib` (ranura) sobre la línea dada
fn place_rail(pieces: &mut [LabeledPiece], ia: usize, ib: usize, line: &RailLine, n: Vector3, width: Real, config: &PlanarJointConfig) -> std::result::Result<(), String> {
    use crate::groove::{dovetail_trench, subtract_trench};
    let gap = config.gap();
    let wall = config.edge_margin.max(0.8);
    let margin = 1.5;
    let rail_len = line.chord - 2.0 * margin;
    if rail_len < width * 3.0 {
        return Err(format!("la cara es muy corta para un riel ({:.1} mm)", line.chord));
    }
    // Ranura: boca de `width` y fondo más ancho (15° de cada lado); no puede pasar del fondo libre
    let probe = {
        let w = n.cross(&line.along).normalize();
        let b = width * 0.5 + config.depth * 0.27;
        // Un poco adentro de los extremos: ahí la ranura sale por la pared y el rayo de prueba no tiene sólido
        let h = line.chord * 0.5 - margin;
        [line.mid - line.along * h - w * b, line.mid + line.along * h - w * b, line.mid + line.along * h + w * b, line.mid - line.along * h + w * b]
    };
    let free = free_depth_inside(&pieces[ib].mesh, &probe, -n);
    let depth = config.depth.min(free - wall);
    if depth < config.min_depth {
        return Err(format!("fondo insuficiente ({free:.1} mm) para un riel"));
    }
    let undercut = depth * (15f64.to_radians().tan());
    let (a, b) = (width * 0.5, width * 0.5 + undercut);
    let trench = dovetail_trench(line.mid, -n, line.along, a, b, depth);
    // Con tope, la ranura termina a `end` de la mitad de la cuerda (2 mm antes de la pared del fondo)
    let end = line.chord * 0.5 - 2.0;
    let mut planes = trench.to_vec();
    if config.rail_stop {
        planes.push((line.mid + line.along * end, -line.along));
    }
    // Riel: más delgado por la holgura, un poco más corto que la ranura, y de `depth − gap` de alto
    // Cada lado del riel sigue al de la ranura con la misma inclinación, a `holgura/2` de distancia
    let half_clear = config.tolerance * 0.5;
    let rail_h = (depth - gap).max(0.5);
    let (root_half, tip_half) = (a + half_clear, a + rail_h * 15f64.to_radians().tan() + half_clear);
    if root_half <= 0.3 {
        return Err("la holgura deja el riel sin grosor".to_string());
    }
    let rail = if config.rail_stop {
        // Ocupa [−c/2 + margen, end − holgura] a lo largo de la cuerda
        let (from, to) = (-line.chord * 0.5 + margin, end - gap);
        if to - from < width * 3.0 {
            return Err(format!("la cara es muy corta para un riel con tope ({:.1} mm)", line.chord));
        }
        add_rail(&pieces[ia].mesh, line.mid + line.along * ((from + to) * 0.5), n, line.along, root_half, tip_half, rail_h, to - from)
    } else {
        add_rail(&pieces[ia].mesh, line.mid, n, line.along, root_half, tip_half, rail_h, rail_len)
    };
    let groove = subtract_trench(&pieces[ib].mesh, &planes);
    match (groove, rail) {
        (Ok(g), Ok(r)) => {
            pieces[ib].mesh = g;
            pieces[ia].mesh = r;
            Ok(())
        }
        (g, r) => Err(format!("unión omitida: {}", g.err().or(r.err()).map_or_else(String::new, |e| e.to_string()))),
    }
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
        if let PlanarShape::Rail { width } = config.shape {
            let (lines, notes) = rail_lines(&contact, config, width);
            conn.notes.extend(notes);
            for line in &lines {
                match place_rail(pieces, ia, ib, line, contact.normal, width, config) {
                    Ok(()) => conn.placed += 1,
                    Err(why) => conn.notes.push(why),
                }
            }
            report.joints_placed += conn.placed;
            report.connections.push(conn);
            continue;
        }
        let spots = choose_spots(&contact, config);
        if spots.len() < config.count {
            conn.notes.push(format!("solo caben {} de {} uniones en la cara", spots.len(), config.count));
        }
        for center in spots {
            let n = contact.normal;
            if let PlanarShape::Bayonet { length, width } = config.shape {
                match place_bayonet(pieces, ia, ib, center, n, (length, width), config) {
                    Ok(()) => conn.placed += 1,
                    Err(why) => conn.notes.push(why),
                }
                continue;
            }
            if let PlanarShape::Terrace { diameter } = config.shape {
                match place_terrace(pieces, ia, ib, center, n, diameter, config) {
                    Ok(()) => conn.placed += 1,
                    Err(why) => conn.notes.push(why),
                }
                continue;
            }
            // Pin en A saliendo hacia B; hueco en B hacia dentro (normal exterior de B = −n)
            // El rompecabezas no es simétrico: perno y hueco usan la misma huella en el mundo
            let puzzle = matches!(config.shape, PlanarShape::Puzzle { .. });
            let hole_poly = if puzzle {
                let mut w = footprint_polygon(&config.shape, center, n, config.rotation_deg);
                w.reverse();
                w
            } else {
                footprint_polygon(&config.shape, center, -n, config.rotation_deg)
            };
            let free = free_depth(&pieces[ib].mesh, &hole_poly, -n);
            let wall = config.edge_margin.max(0.8);
            let depth = config.depth.min(free - wall);
            if depth < config.min_depth {
                conn.notes.push(format!("fondo insuficiente ({free:.1} mm) para una unión en ({:.1}, {:.1}, {:.1})", center.x(), center.y(), center.z()));
                continue;
            }
            // Pin: huella más pequeña por la holgura, en el sentido de A (normal n)
            let pin_shape = config.shape.resized(config.tolerance);
            let pin_poly = footprint_polygon(&pin_shape, center, n, if puzzle { config.rotation_deg } else { -config.rotation_deg });
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

/// Perno de terraza: base redonda de radio `r1` y `h1` de alto, y encima un escalón de radio `r2` y `h2`
#[allow(clippy::too_many_arguments)]
pub fn add_terrace_pin(mesh: &Mesh, center: Vector3, normal: Vector3, r1: Real, r2: Real, h1: Real, h2: Real) -> Result<Mesh> {
    let n = normal.try_normalize().ok_or_else(|| Print3dError::JointError("normal nula".into()))?;
    if !(r1 > r2 && r2 > 0.0 && h1 > 0.0 && h2 > 0.0) {
        return Err(Print3dError::JointError("medidas del perno de terraza no válidas".into()));
    }
    let base = footprint_polygon(&PlanarShape::Round { diameter: r1 * 2.0 }, center, n, 0.0);
    let (mut pos, mut tris, ring) = open_cap(mesh, &base, n)?;
    let diag = diagonal(&pos);
    let outer: Vec<Vector3> = ring.iter().map(|&i| pos[i]).collect();
    let step_outer: Vec<Vector3> = outer.iter().map(|p| *p + n * h1).collect();
    let m = ring.len();
    let first = pos.len();
    pos.extend(step_outer.iter().copied());
    for i in 0..m {
        let (a, b) = (ring[i], ring[(i + 1) % m]);
        tris.push([b, a, first + i]);
        tris.push([b, first + i, first + (i + 1) % m]);
    }
    let inner: Vec<Vector3> = footprint_polygon(&PlanarShape::Round { diameter: r2 * 2.0 }, center, n, 0.0).iter().map(|p| *p + n * h1).collect();
    region_tris(&[step_outer, inner.clone()], n, diag, &mut pos, &mut tris);
    prism_walls(&inner, n * h2, &mut pos, &mut tris);
    let top: Vec<Vector3> = inner.iter().map(|p| *p + n * h2).collect();
    region_tris(&[top], n, diag, &mut pos, &mut tris);
    weld_build(&pos, &tris)
}

/// Hueco de terraza: boca de radio `r1` hasta `d1` y debajo un pozo de radio `r2` hasta `d2`
#[allow(clippy::too_many_arguments)]
pub fn add_terrace_hole(mesh: &Mesh, center: Vector3, normal: Vector3, r1: Real, r2: Real, d1: Real, d2: Real, min_wall: Real) -> Result<Mesh> {
    let n = normal.try_normalize().ok_or_else(|| Print3dError::JointError("normal nula".into()))?;
    if !(r1 > r2 && r2 > 0.0 && d2 > d1 && d1 > 0.0) {
        return Err(Print3dError::JointError("medidas del hueco de terraza no válidas".into()));
    }
    let mouth = footprint_polygon(&PlanarShape::Round { diameter: r1 * 2.0 }, center, n, 0.0);
    let free = free_depth_inside(mesh, &mouth, n);
    if free < d2 + min_wall {
        return Err(Print3dError::JointError(format!("la pieza solo tiene {free:.1} mm de fondo bajo la unión")));
    }
    let (mut pos, mut tris, ring) = open_cap(mesh, &mouth, n)?;
    let diag = diagonal(&pos);
    let outer: Vec<Vector3> = ring.iter().map(|&i| pos[i]).collect();
    let step_outer: Vec<Vector3> = outer.iter().map(|p| *p - n * d1).collect();
    let m = ring.len();
    let first = pos.len();
    pos.extend(step_outer.iter().copied());
    for i in 0..m {
        let (a, b) = (ring[i], ring[(i + 1) % m]);
        tris.push([b, a, first + i]);
        tris.push([b, first + i, first + (i + 1) % m]);
    }
    // Escalón: mira hacia la boca (al vacío), entre la pared ancha y el pozo
    let inner: Vec<Vector3> = footprint_polygon(&PlanarShape::Round { diameter: r2 * 2.0 }, center, n, 0.0).iter().map(|p| *p - n * d1).collect();
    region_tris(&[step_outer, inner.clone()], n, diag, &mut pos, &mut tris);
    prism_walls(&inner, -n * (d2 - d1), &mut pos, &mut tris);
    let floor: Vec<Vector3> = inner.iter().map(|p| *p - n * (d2 - d1)).collect();
    region_tris(&[floor], n, diag, &mut pos, &mut tris);
    weld_build(&pos, &tris)
}

/// Una terraza entre dos piezas (perno en `ia`, hueco en `ib`)
fn place_terrace(pieces: &mut [LabeledPiece], ia: usize, ib: usize, center: Vector3, n: Vector3, diameter: Real, config: &PlanarJointConfig) -> std::result::Result<(), String> {
    let gap = config.gap();
    let wall = config.edge_margin.max(0.8);
    let total = config.depth;
    let (r1, r2) = (diameter * 0.5, diameter * 0.3);
    let (d1, d2) = (total * 0.45, total);
    let (pr1, pr2) = (r1 + config.tolerance * 0.5, r2 + config.tolerance * 0.5);
    let h1 = d1 - gap * 0.5;
    let h2 = (d2 - gap) - h1;
    if pr2 <= 0.3 || pr1 <= pr2 || h2 <= 0.3 || h1 <= 0.3 {
        return Err("la holgura o la profundidad no dejan terraza".into());
    }
    let hole = add_terrace_hole(&pieces[ib].mesh, center, -n, r1, r2, d1, d2, wall);
    let pin = add_terrace_pin(&pieces[ia].mesh, center, n, pr1, pr2, h1, h2);
    match (hole, pin) {
        (Ok(h), Ok(p)) => {
            pieces[ib].mesh = h;
            pieces[ia].mesh = p;
            Ok(())
        }
        (h, p) => Err(format!("unión omitida: {}", h.err().or(p.err()).map_or_else(String::new, |e| e.to_string()))),
    }
}

/// Una unión de cuarto de vuelta entre dos piezas (perno en `ia`, ranura en `ib`)
fn place_bayonet(pieces: &mut [LabeledPiece], ia: usize, ib: usize, center: Vector3, n: Vector3, bar: (Real, Real), config: &PlanarJointConfig) -> std::result::Result<(), String> {
    let gap = config.gap();
    let wall = config.edge_margin.max(0.8);
    // Espesor del labio y de la cabeza: lo que sobra de la profundidad, repartido por igual
    let total = config.depth;
    let (d1, head_t) = {
        let t = ((total - 2.0 * gap) / 2.0).max(1.0);
        (t, t)
    };
    let h1 = d1 + gap * 0.5;
    let d2 = h1 + head_t + gap;
    let chamber_r = bar.0.hypot(bar.1) * 0.5 + gap + 0.5;
    let pin_bar = (bar.0 + config.tolerance, bar.1 + config.tolerance);
    if pin_bar.0 <= 0.5 || pin_bar.1 <= 0.5 {
        return Err("la holgura deja la barra sin grosor".into());
    }
    // Pin en A saliendo hacia B (frame de n); la ranura en B mira hacia −n
    let stem_r = pin_bar.1 * 0.3;
    let hole = add_bayonet_hole(&pieces[ib].mesh, center, -n, config.rotation_deg, bar, chamber_r, d1, d2, wall);
    let pin = add_bayonet_pin(&pieces[ia].mesh, center, n, -config.rotation_deg, pin_bar, stem_r, h1, head_t);
    match (hole, pin) {
        (Ok(h), Ok(p)) => {
            pieces[ib].mesh = h;
            pieces[ia].mesh = p;
            Ok(())
        }
        (h, p) => Err(format!("unión omitida: {}", h.err().or(p.err()).map_or_else(String::new, |e| e.to_string()))),
    }
}

/// Elige hasta `count` posiciones dentro de la región de contacto, separadas entre sí
pub(crate) fn choose_spots(contact: &Contact, config: &PlanarJointConfig) -> Vec<Vector3> {
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

    /// Toro con el eje en Z: radio mayor `big`, radio del tubo `small`
    fn torus(big: Real, small: Real, around: usize, tube: usize) -> Mesh {
        let mut pos = Vec::new();
        for i in 0..around {
            let a = std::f64::consts::TAU * i as Real / around as Real;
            for j in 0..tube {
                let b = std::f64::consts::TAU * (j as Real + 0.5) / tube as Real;
                let r = big + small * b.cos();
                pos.push(Vector3::new(r * a.cos(), r * a.sin(), small * b.sin()));
            }
        }
        let at = |i: usize, j: usize| (i % around) * tube + j % tube;
        let mut tris = Vec::new();
        for i in 0..around {
            for j in 0..tube {
                tris.push([at(i, j), at(i + 1, j), at(i + 1, j + 1)]);
                tris.push([at(i, j), at(i + 1, j + 1), at(i, j + 1)]);
            }
        }
        Mesh::from_triangles(&pos, &tris)
    }

    #[test]
    fn toro_cortado_por_el_plano_medio_tiene_tapa_anular() {
        let t = torus(30.0, 10.0, 96, 48);
        assert!(t.is_closed());
        assert!(compute_volume(&t) > 0.0);
        let meshes = crate::slicer::slice_by_planes(&t, &[Plane::xy_at_z(0.5)]).unwrap();
        let mut pieces: Vec<LabeledPiece> = meshes.into_iter().enumerate().map(|(id, mesh)| LabeledPiece { mesh, id, ..Default::default() }).collect();
        for p in &pieces {
            assert!(p.mesh.is_closed(), "tapa anular abierta");
        }
        let vol: Real = pieces.iter().map(|p| compute_volume(&p.mesh)).sum();
        assert!((vol - compute_volume(&t)).abs() < 1e-6 * vol);
        let report = apply_joints(&mut pieces, &PlanarJointConfig { count: 2, depth: 6.0, min_depth: 2.0, ..Default::default() }).unwrap();
        assert_eq!(report.joints_placed, 2, "{report:?}");
        for p in &pieces {
            assert!(p.mesh.is_closed());
        }
    }

    fn split_box_in_two(z: Real) -> (Vec<LabeledPiece>, Vec<Real>) {
        let b = make_box(80.0, 80.0, 60.0);
        let meshes = crate::slicer::slice_by_planes(&b, &[Plane::xy_at_z(z)]).unwrap();
        let pieces: Vec<LabeledPiece> = meshes.into_iter().enumerate().map(|(id, mesh)| LabeledPiece { mesh, id, ..Default::default() }).collect();
        let before = pieces.iter().map(|p| compute_volume(&p.mesh)).collect();
        (pieces, before)
    }

    /// (aumento del que gana volumen, disminución del que pierde)
    fn deltas_of(pieces: &[LabeledPiece], before: &[Real]) -> (Real, Real) {
        let d: Vec<Real> = pieces.iter().zip(before).map(|(p, v)| compute_volume(&p.mesh) - v).collect();
        (d.iter().cloned().fold(Real::NEG_INFINITY, Real::max), -d.iter().cloned().fold(Real::INFINITY, Real::min))
    }

    #[test]
    fn terraza_en_dos_mitades_de_una_caja() {
        let (mut pieces, before) = split_box_in_two(30.0);
        let cfg = PlanarJointConfig { shape: PlanarShape::Terrace { diameter: 12.0 }, depth: 14.0, count: 1, ..Default::default() };
        let report = apply_joints(&mut pieces, &cfg).unwrap();
        assert_eq!(report.joints_placed, 1, "{report:?}");
        assert!(pieces.iter().all(|p| p.mesh.is_closed()));
        // Holgura 0,4: boca de radio 6 hasta 6,3 y pozo de radio 3,6 hasta 14; perno de radios 5,8 y 3,4, alturas 6,1 y 7,5
        let poly24 = |r: Real| 0.5 * 24.0 * r * r * (std::f64::consts::TAU / 24.0).sin();
        let removed = poly24(6.0) * 6.3 + poly24(3.6) * (14.0 - 6.3);
        let added = poly24(5.8) * 6.1 + poly24(3.4) * 7.5;
        let (pin, hole) = deltas_of(&pieces, &before);
        assert!((pin - added).abs() < 1e-6 * added, "perno {pin} esperado {added}");
        assert!((hole - removed).abs() < 1e-6 * removed, "hueco {hole} esperado {removed}");
        assert!(pin < hole, "el perno tiene que ser más chico que el hueco");
    }

    #[test]
    fn rompecabezas_en_dos_mitades_de_una_caja() {
        let (mut pieces, before) = split_box_in_two(30.0);
        let cfg = PlanarJointConfig { shape: PlanarShape::Puzzle { size: 10.0 }, depth: 10.0, count: 1, rotation_deg: 30.0, ..Default::default() };
        let report = apply_joints(&mut pieces, &cfg).unwrap();
        assert_eq!(report.joints_placed, 1, "{report:?}");
        assert!(pieces.iter().all(|p| p.mesh.is_closed()));
        let area = |size: Real| {
            let o = puzzle_outline(size);
            (0..o.len()).map(|i| o[i].0 * o[(i + 1) % o.len()].1 - o[(i + 1) % o.len()].0 * o[i].1).sum::<Real>() * 0.5
        };
        let (removed, added) = (area(10.0) * 10.0, area(9.6) * 9.6);
        let (pin, hole) = deltas_of(&pieces, &before);
        assert!((pin - added).abs() < 1e-6 * added, "perno {pin} esperado {added}");
        assert!((hole - removed).abs() < 1e-6 * removed, "hueco {hole} esperado {removed}");
        // La huella no es convexa: una cabeza ancha sobre un cuello angosto
        let o = puzzle_outline(10.0);
        let xs = o.iter().map(|p| p.0);
        assert!(xs.clone().fold(Real::NEG_INFINITY, Real::max) > 4.9 && o[0].0.abs() < 2.1);
    }

    #[test]
    fn el_rompecabezas_entra_en_su_hueco() {
        // Mismo marco en el mundo: la huella chica del perno cae dentro de la del hueco aunque se gire
        for rot in [0.0, 37.0, 90.0, 200.0] {
            let n = Vector3::unit_z();
            let c = Vector3::new(10.0, 20.0, 30.0);
            let hole = footprint_polygon(&PlanarShape::Puzzle { size: 10.0 }, c, n, rot);
            let pin = footprint_polygon(&PlanarShape::Puzzle { size: 9.6 }, c, n, rot);
            let (u, v) = plane_basis(n);
            let to2 = |p: &Vector3| ((*p - c).dot(&u), (*p - c).dot(&v));
            let hole2: Vec<(Real, Real)> = hole.iter().map(to2).collect();
            // Dentro de un polígono no convexo: regla par-impar
            let inside = |q: (Real, Real)| {
                let mut odd = false;
                for i in 0..hole2.len() {
                    let (a, b) = (hole2[i], hole2[(i + 1) % hole2.len()]);
                    if (a.1 > q.1) != (b.1 > q.1) && q.0 < (b.0 - a.0) * (q.1 - a.1) / (b.1 - a.1) + a.0 {
                        odd = !odd;
                    }
                }
                odd
            };
            assert!(pin.iter().all(|p| inside(to2(p))), "el perno se sale del hueco con giro {rot}");
        }
    }

    #[test]
    fn bayoneta_en_dos_mitades_de_una_caja() {
        let b = make_box(80.0, 80.0, 60.0);
        let meshes = crate::slicer::slice_by_planes(&b, &[Plane::xy_at_z(30.0)]).unwrap();
        let mut pieces: Vec<LabeledPiece> = meshes.into_iter().enumerate().map(|(id, mesh)| LabeledPiece { mesh, id, ..Default::default() }).collect();
        let before: Vec<Real> = pieces.iter().map(|p| compute_volume(&p.mesh)).collect();
        let cfg = PlanarJointConfig { shape: PlanarShape::Bayonet { length: 14.0, width: 6.0 }, depth: 12.0, count: 1, ..Default::default() };
        let report = apply_joints(&mut pieces, &cfg).unwrap();
        assert_eq!(report.joints_placed, 1, "{report:?}");
        for p in &pieces {
            assert!(p.mesh.is_closed());
        }
        // Medidas de la unión (holgura 0,4): labio y cabeza de 5,6; tallo de 5,8; cámara hasta 11,8
        let poly24 = |r: Real| 0.5 * 24.0 * r * r * (std::f64::consts::TAU / 24.0).sin();
        let added = poly24(0.3 * 5.6) * 5.8 + 13.6 * 5.6 * 5.6;
        let removed = 14.0 * 6.0 * 5.6 + poly24(14f64.hypot(6.0) * 0.5 + 0.9) * (11.8 - 5.6);
        let deltas: Vec<Real> = pieces.iter().zip(&before).map(|(p, v)| compute_volume(&p.mesh) - v).collect();
        let pin_delta = deltas.iter().cloned().fold(Real::NEG_INFINITY, Real::max);
        let hole_delta = deltas.iter().cloned().fold(Real::INFINITY, Real::min);
        assert!((pin_delta - added).abs() < 1e-6 * added.max(1.0) + 1e-3, "perno {pin_delta} esperado {added}");
        assert!((hole_delta + removed).abs() < 1e-6 * removed.max(1.0) + 1e-3, "ranura {hole_delta} esperado {}", -removed);
        // La pieza con el perno sobresale de su tapa
        let z_extent = |m: &Mesh| {
            let bb = crate::analysis::compute_bounding_box(m);
            bb.max[2] - bb.min[2]
        };
        assert!(pieces.iter().any(|p| z_extent(&p.mesh) > 30.0 + 3.0), "ningún perno sobresale");
    }

    #[test]
    fn bayoneta_en_cara_chica_se_omite() {
        let b = make_box(14.0, 14.0, 40.0);
        let meshes = crate::slicer::slice_by_planes(&b, &[Plane::xy_at_z(20.0)]).unwrap();
        let mut pieces: Vec<LabeledPiece> = meshes.into_iter().enumerate().map(|(id, mesh)| LabeledPiece { mesh, id, ..Default::default() }).collect();
        let cfg = PlanarJointConfig { shape: PlanarShape::Bayonet { length: 14.0, width: 6.0 }, count: 1, ..Default::default() };
        assert_eq!(apply_joints(&mut pieces, &cfg).unwrap().joints_placed, 0);
    }

    #[test]
    fn riel_deslizante_en_dos_mitades_de_una_caja() {
        let b = make_box(80.0, 60.0, 50.0);
        let meshes = crate::slicer::slice_by_planes(&b, &[Plane::xy_at_z(25.0)]).unwrap();
        let mut pieces: Vec<LabeledPiece> = meshes.into_iter().enumerate().map(|(id, mesh)| LabeledPiece { mesh, id, ..Default::default() }).collect();
        let before: Vec<Real> = pieces.iter().map(|p| compute_volume(&p.mesh)).collect();
        let cfg = PlanarJointConfig { shape: PlanarShape::Rail { width: 6.0 }, depth: 8.0, count: 1, ..Default::default() };
        let report = apply_joints(&mut pieces, &cfg).unwrap();
        assert_eq!(report.joints_placed, 1, "{report:?}");
        for p in &pieces {
            assert!(p.mesh.is_closed());
        }
        let deltas: Vec<Real> = pieces.iter().zip(&before).map(|(p, v)| compute_volume(&p.mesh) - v).collect();
        let (rail, groove) = (deltas.iter().cloned().fold(Real::NEG_INFINITY, Real::max), deltas.iter().cloned().fold(Real::INFINITY, Real::min));
        // Ranura: sección ((6 + 6 + 2·8·tan15°)/2)·8 por el largo de la cara en la dirección del riel (60 mm: la dirección es −Y)
        let undercut = 8.0 * 15f64.to_radians().tan();
        let section = (6.0 + (6.0 + 2.0 * undercut)) * 0.5 * 8.0;
        assert!((groove + section * 60.0).abs() < 1e-6 * section * 60.0, "ranura {groove} esperada {}", -section * 60.0);
        // El riel es más fino y más corto que la ranura, pero no mucho
        assert!(rail > 0.6 * section * 60.0 && rail < section * 60.0, "riel {rail}");
    }

    #[test]
    fn riel_con_tope_termina_ciego_a_2_mm_de_la_pared() {
        let b = make_box(80.0, 60.0, 50.0);
        let meshes = crate::slicer::slice_by_planes(&b, &[Plane::xy_at_z(25.0)]).unwrap();
        let mut pieces: Vec<LabeledPiece> = meshes.into_iter().enumerate().map(|(id, mesh)| LabeledPiece { mesh, id, ..Default::default() }).collect();
        let before: Vec<Real> = pieces.iter().map(|p| compute_volume(&p.mesh)).collect();
        let cfg = PlanarJointConfig { shape: PlanarShape::Rail { width: 6.0 }, depth: 8.0, count: 1, rail_stop: true, ..Default::default() };
        let report = apply_joints(&mut pieces, &cfg).unwrap();
        assert_eq!(report.joints_placed, 1, "{report:?}");
        for p in &pieces {
            assert!(p.mesh.is_closed());
        }
        let deltas: Vec<Real> = pieces.iter().zip(&before).map(|(p, v)| compute_volume(&p.mesh) - v).collect();
        let (rail, groove) = (deltas.iter().cloned().fold(Real::NEG_INFINITY, Real::max), deltas.iter().cloned().fold(Real::INFINITY, Real::min));
        let undercut = 8.0 * 15f64.to_radians().tan();
        let section = (6.0 + (6.0 + 2.0 * undercut)) * 0.5 * 8.0;
        // La ranura entra por una pared (60 mm de cuerda) y termina 2 mm antes de la otra
        assert!((groove + section * 58.0).abs() < 1e-6 * section * 58.0, "ranura {groove} esperada {}", -section * 58.0);
        assert!(rail > 0.5 * section * 58.0 && rail < section * 58.0, "riel {rail}");
    }

    #[test]
    fn el_riel_cabe_en_la_ranura_con_holgura() {
        use crate::groove::dovetail_trench;
        let b = make_box(80.0, 60.0, 50.0);
        let meshes = crate::slicer::slice_by_planes(&b, &[Plane::xy_at_z(25.0)]).unwrap();
        let mut pieces: Vec<LabeledPiece> = meshes.into_iter().enumerate().map(|(id, mesh)| LabeledPiece { mesh, id, ..Default::default() }).collect();
        let before: Vec<Real> = pieces.iter().map(|p| compute_volume(&p.mesh)).collect();
        let cfg = PlanarJointConfig { shape: PlanarShape::Rail { width: 6.0 }, depth: 12.0, tolerance: -0.1, count: 1, ..Default::default() };
        assert_eq!(apply_joints(&mut pieces, &cfg).unwrap().joints_placed, 1);
        let after: Vec<Real> = pieces.iter().map(|p| compute_volume(&p.mesh)).collect();
        let rail_piece = if after[0] - before[0] > after[1] - before[1] { 0 } else { 1 };
        // Cara de la pieza del riel: su centro de z (25) y hacia dónde sobresale
        let zs: Vec<Real> = pieces[rail_piece].mesh.vertices.iter().map(|v| v.position.z()).collect();
        // El cuerpo está abajo de la cara (z < 25) o arriba; el riel sale hacia el otro lado
        let mean_z = zs.iter().sum::<Real>() / zs.len() as Real;
        let body_below = mean_z < 25.0;
        let n = if body_below { Vector3::unit_z() } else { -Vector3::unit_z() };
        let depth = 12.0;
        let undercut = depth * 15f64.to_radians().tan();
        let planes = dovetail_trench(Vector3::new(40.0, 30.0, 25.0), -n, Vector3::unit_y(), 3.0, 3.0 + undercut, depth);
        let mut checked = 0;
        for v in &pieces[rail_piece].mesh.vertices {
            let beyond = (v.position.z() - 25.0) * n.z();
            if beyond > 1e-6 {
                checked += 1;
                for (origin, n_in) in &planes {
                    let clearance = n_in.dot(&(v.position - *origin));
                    assert!(clearance > 0.02, "el riel invade la ranura: {clearance} en {:?}", v.position);
                }
            }
        }
        assert!(checked >= 4, "pocos vértices del riel ({checked})");
    }

    #[test]
    fn riel_en_una_esfera_cortada() {
        let s = uv_sphere(40.0, 48, 64);
        let meshes = crate::slicer::slice_by_planes(&s, &[Plane::xy_at_z(5.0)]).unwrap();
        let mut pieces: Vec<LabeledPiece> = meshes.into_iter().enumerate().map(|(id, mesh)| LabeledPiece { mesh, id, ..Default::default() }).collect();
        let cfg = PlanarJointConfig { shape: PlanarShape::Rail { width: 6.0 }, depth: 8.0, count: 1, ..Default::default() };
        let report = apply_joints(&mut pieces, &cfg).unwrap();
        assert_eq!(report.joints_placed, 1, "{report:?}");
        for p in &pieces {
            assert!(p.mesh.is_closed());
            assert!(compute_volume(&p.mesh) > 0.0);
        }
    }
}
