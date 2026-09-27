//! Corte y subdivisión de mallas para impresión 3D
//!
//! Permite dividir modelos grandes en piezas que quepan en el volumen
//! de construcción de una impresora.

use pinocchio_math::{Real, Vector3};
use pinocchio_mesh::Mesh;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::{
    compute_bounding_box, BoundingBox, LabeledPiece, LabelingScheme, Print3dError, Result,
    SubdivideConfig, SubdivideStrategy,
};

/// Tolerancia para clasificación de puntos
const EPSILON: Real = 1e-6;

/// Plano 3D definido por un punto y una normal
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Plane {
    /// Punto en el plano [x, y, z]
    pub origin: [Real; 3],
    /// Vector normal (debe estar normalizado) [x, y, z]
    pub normal: [Real; 3],
}

impl Plane {
    /// Crea un nuevo plano desde origen y normal
    pub fn new(origin: Vector3, normal: Vector3) -> Self {
        let n = normal.normalize();
        Self {
            origin: [origin.x(), origin.y(), origin.z()],
            normal: [n.x(), n.y(), n.z()],
        }
    }

    /// Crea un plano desde 3 puntos
    pub fn from_points(p0: Vector3, p1: Vector3, p2: Vector3) -> Option<Self> {
        let v1 = p1 - p0;
        let v2 = p2 - p0;
        let normal = v1.cross(&v2);

        if normal.length() < 1e-10 {
            return None; // Puntos colineales
        }

        Some(Self::new(p0, normal))
    }

    /// Plano XY en Z dado
    pub fn xy_at_z(z: Real) -> Self {
        Self {
            origin: [0.0, 0.0, z],
            normal: [0.0, 0.0, 1.0],
        }
    }

    /// Plano XZ en Y dado
    pub fn xz_at_y(y: Real) -> Self {
        Self {
            origin: [0.0, y, 0.0],
            normal: [0.0, 1.0, 0.0],
        }
    }

    /// Plano YZ en X dado
    pub fn yz_at_x(x: Real) -> Self {
        Self {
            origin: [x, 0.0, 0.0],
            normal: [1.0, 0.0, 0.0],
        }
    }

    /// Obtiene el origen como Vector3
    pub fn origin_vec(&self) -> Vector3 {
        Vector3::new(self.origin[0], self.origin[1], self.origin[2])
    }

    /// Obtiene la normal como Vector3
    pub fn normal_vec(&self) -> Vector3 {
        Vector3::new(self.normal[0], self.normal[1], self.normal[2])
    }

    /// Distancia con signo de un punto al plano
    ///
    /// Positivo = mismo lado que la normal
    /// Negativo = lado opuesto
    /// Cero = en el plano
    pub fn signed_distance(&self, point: Vector3) -> Real {
        (point - self.origin_vec()).dot(&self.normal_vec())
    }

    /// Clasifica un punto respecto al plano
    pub fn classify_point(&self, point: Vector3, epsilon: Real) -> PointClassification {
        let dist = self.signed_distance(point);
        if dist > epsilon {
            PointClassification::Front
        } else if dist < -epsilon {
            PointClassification::Back
        } else {
            PointClassification::OnPlane
        }
    }

    /// Calcula la intersección de un segmento con el plano
    ///
    /// Retorna el parámetro t donde: punto = p0 + t * (p1 - p0)
    pub fn intersect_segment(&self, p0: Vector3, p1: Vector3) -> Option<Real> {
        let d = p1 - p0;
        let denom = d.dot(&self.normal_vec());

        if denom.abs() < 1e-10 {
            return None; // Segmento paralelo al plano
        }

        let t = (self.origin_vec() - p0).dot(&self.normal_vec()) / denom;

        if (0.0..=1.0).contains(&t) {
            Some(t)
        } else {
            None
        }
    }

    /// Calcula el punto de intersección de un segmento con el plano
    pub fn intersect_segment_point(&self, p0: Vector3, p1: Vector3) -> Option<Vector3> {
        self.intersect_segment(p0, p1)
            .map(|t| p0 + (p1 - p0) * t)
    }
}

/// Clasificación de un punto respecto a un plano
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointClassification {
    /// Delante del plano (lado de la normal)
    Front,
    /// Detrás del plano
    Back,
    /// En el plano (dentro de epsilon)
    OnPlane,
}

/// Builder para construir una malla desde triángulos
struct MeshBuilder {
    positions: Vec<Vector3>,
    indices: Vec<[usize; 3]>,
    vertex_map: HashMap<VertexKey, usize>,
}

/// Clave para deduplicar vértices (usando coordenadas cuantizadas)
#[derive(Hash, Eq, PartialEq)]
struct VertexKey {
    x: i64,
    y: i64,
    z: i64,
}

impl VertexKey {
    fn from_vec3(v: Vector3, scale: Real) -> Self {
        Self {
            x: (v.x() * scale).round() as i64,
            y: (v.y() * scale).round() as i64,
            z: (v.z() * scale).round() as i64,
        }
    }
}

impl MeshBuilder {
    fn new() -> Self {
        Self {
            positions: Vec::new(),
            indices: Vec::new(),
            vertex_map: HashMap::new(),
        }
    }

    /// Añade un vértice y retorna su índice (deduplica)
    fn add_vertex(&mut self, pos: Vector3) -> usize {
        let key = VertexKey::from_vec3(pos, 1e6); // Precisión de 1 micra

        if let Some(&idx) = self.vertex_map.get(&key) {
            return idx;
        }

        let idx = self.positions.len();
        self.positions.push(pos);
        self.vertex_map.insert(key, idx);
        idx
    }

    /// Añade un triángulo
    fn add_triangle(&mut self, v0: Vector3, v1: Vector3, v2: Vector3) {
        let i0 = self.add_vertex(v0);
        let i1 = self.add_vertex(v1);
        let i2 = self.add_vertex(v2);

        // Evitar triángulos degenerados
        if i0 != i1 && i1 != i2 && i2 != i0 {
            self.indices.push([i0, i1, i2]);
        }
    }

    /// Construye la malla final
    fn build(self) -> Mesh {
        Mesh::from_triangles(&self.positions, &self.indices)
    }

    /// Verifica si tiene geometría
    fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }
}

/// Corta una malla con un plano, generando dos mallas separadas
///
/// # Argumentos
/// * `mesh` - Malla a cortar
/// * `plane` - Plano de corte
///
/// # Retorna
/// Tupla (front, back) con las dos mallas resultantes.
/// - `front`: parte del lado positivo del plano (dirección de la normal)
/// - `back`: parte del lado negativo del plano
///
/// # Algoritmo
/// 1. Clasificar cada vértice (front/back/on)
/// 2. Para cada triángulo, según clasificación de sus vértices:
///    - Todos front → añadir a front_mesh
///    - Todos back → añadir a back_mesh
///    - Mixto → dividir y añadir partes correspondientes
/// 3. Recolectar segmentos de intersección
/// 4. Triangular el polígono de cierre
/// 5. Añadir caras de cierre a ambas mallas
pub fn slice_by_plane(mesh: &Mesh, plane: &Plane) -> Result<(Mesh, Mesh)> {
    if mesh.faces.is_empty() {
        return Err(Print3dError::EmptyMesh);
    }

    // Clasificar todos los vértices
    let classifications: Vec<PointClassification> = mesh
        .vertices
        .iter()
        .map(|v| plane.classify_point(v.position, EPSILON))
        .collect();

    // Verificar si el plano intersecta la malla
    let has_front = classifications.contains(&PointClassification::Front);
    let has_back = classifications.contains(&PointClassification::Back);

    if !has_front {
        // Todo está detrás del plano
        return Err(Print3dError::PlaneNoIntersection);
    }
    if !has_back {
        // Todo está delante del plano
        return Err(Print3dError::PlaneNoIntersection);
    }

    let mut front_builder = MeshBuilder::new();
    let mut back_builder = MeshBuilder::new();
    let mut cut_segments: Vec<(Vector3, Vector3)> = Vec::new();

    // Procesar cada triángulo
    for &face_edge_idx in &mesh.faces {
        let e0 = &mesh.edges[face_edge_idx];
        let e1 = &mesh.edges[e0.next];
        let e2 = &mesh.edges[e1.next];

        let v0 = mesh.vertices[e0.vertex].position;
        let v1 = mesh.vertices[e1.vertex].position;
        let v2 = mesh.vertices[e2.vertex].position;

        let c0 = classifications[e0.vertex];
        let c1 = classifications[e1.vertex];
        let c2 = classifications[e2.vertex];

        // Contar clasificaciones
        let front_count = [c0, c1, c2]
            .iter()
            .filter(|&&c| c == PointClassification::Front)
            .count();
        let back_count = [c0, c1, c2]
            .iter()
            .filter(|&&c| c == PointClassification::Back)
            .count();

        if back_count == 0 {
            // Todo front (o en el plano) → añadir a front
            front_builder.add_triangle(v0, v1, v2);
        } else if front_count == 0 {
            // Todo back (o en el plano) → añadir a back
            back_builder.add_triangle(v0, v1, v2);
        } else {
            // Triángulo cruza el plano → dividir
            split_triangle(
                [v0, v1, v2],
                [c0, c1, c2],
                plane,
                &mut front_builder,
                &mut back_builder,
                &mut cut_segments,
            );
        }
    }

    // Triangular el polígono de cierre si hay segmentos
    if !cut_segments.is_empty() {
        let cap_triangles = triangulate_cut_polygon(&cut_segments, plane);

        // Añadir caras de cierre a ambas mallas (con normales opuestas)
        for (a, b, c) in &cap_triangles {
            // Front: normal apunta hacia atrás (opuesta al plano)
            back_builder.add_triangle(*a, *b, *c);
            // Back: normal apunta hacia adelante
            front_builder.add_triangle(*a, *c, *b);
        }
    }

    if front_builder.is_empty() || back_builder.is_empty() {
        return Err(Print3dError::PlaneNoIntersection);
    }

    Ok((front_builder.build(), back_builder.build()))
}

/// Divide un triángulo que cruza el plano
fn split_triangle(
    verts: [Vector3; 3],
    classes: [PointClassification; 3],
    plane: &Plane,
    front: &mut MeshBuilder,
    back: &mut MeshBuilder,
    cut_segments: &mut Vec<(Vector3, Vector3)>,
) {
    // Encontrar el vértice solitario (el que está solo de su lado)
    let front_count = classes
        .iter()
        .filter(|&&c| c == PointClassification::Front)
        .count();

    // Rotar para que el vértice solitario sea v0
    let (v0, v1, v2, c0, _c1, _c2) = if front_count == 1 {
        // Un vértice front, dos back
        let solo_idx = classes
            .iter()
            .position(|&c| c == PointClassification::Front)
            .unwrap();
        match solo_idx {
            0 => (verts[0], verts[1], verts[2], classes[0], classes[1], classes[2]),
            1 => (verts[1], verts[2], verts[0], classes[1], classes[2], classes[0]),
            _ => (verts[2], verts[0], verts[1], classes[2], classes[0], classes[1]),
        }
    } else {
        // Un vértice back, dos front
        let solo_idx = classes
            .iter()
            .position(|&c| c == PointClassification::Back)
            .unwrap();
        match solo_idx {
            0 => (verts[0], verts[1], verts[2], classes[0], classes[1], classes[2]),
            1 => (verts[1], verts[2], verts[0], classes[1], classes[2], classes[0]),
            _ => (verts[2], verts[0], verts[1], classes[2], classes[0], classes[1]),
        }
    };

    // Calcular puntos de intersección
    let i1 = plane.intersect_segment_point(v0, v1).unwrap_or(v1);
    let i2 = plane.intersect_segment_point(v0, v2).unwrap_or(v2);

    // Guardar segmento de corte
    cut_segments.push((i1, i2));

    // Generar triángulos
    if c0 == PointClassification::Front {
        // v0 está en front, v1 y v2 están en back
        // Front: un triángulo (v0, i1, i2)
        front.add_triangle(v0, i1, i2);
        // Back: dos triángulos (i1, v1, v2) y (i1, v2, i2)
        back.add_triangle(i1, v1, v2);
        back.add_triangle(i1, v2, i2);
    } else {
        // v0 está en back, v1 y v2 están en front
        // Back: un triángulo (v0, i1, i2)
        back.add_triangle(v0, i1, i2);
        // Front: dos triángulos (i1, v1, v2) y (i1, v2, i2)
        front.add_triangle(i1, v1, v2);
        front.add_triangle(i1, v2, i2);
    }
}

/// Triangula el polígono formado por los segmentos de corte
fn triangulate_cut_polygon(segments: &[(Vector3, Vector3)], plane: &Plane) -> Vec<(Vector3, Vector3, Vector3)> {
    if segments.is_empty() {
        return Vec::new();
    }

    // Construir el polígono ordenando los segmentos
    let polygon = build_polygon_from_segments(segments);

    if polygon.len() < 3 {
        return Vec::new();
    }

    // Triangular usando ear clipping simple
    ear_clip_triangulate(&polygon, plane)
}

/// Construye un polígono ordenado desde segmentos desordenados
fn build_polygon_from_segments(segments: &[(Vector3, Vector3)]) -> Vec<Vector3> {
    if segments.is_empty() {
        return Vec::new();
    }

    let mut result = Vec::new();
    let mut remaining: Vec<(Vector3, Vector3)> = segments.to_vec();

    // Empezar con el primer segmento
    let first = remaining.remove(0);
    result.push(first.0);
    result.push(first.1);

    let tolerance = 1e-6;

    // Intentar conectar segmentos
    while !remaining.is_empty() {
        let last = *result.last().unwrap();
        let mut found = false;

        for i in 0..remaining.len() {
            let (a, b) = remaining[i];

            if (a - last).length() < tolerance {
                result.push(b);
                remaining.remove(i);
                found = true;
                break;
            } else if (b - last).length() < tolerance {
                result.push(a);
                remaining.remove(i);
                found = true;
                break;
            }
        }

        if !found {
            // No se pudo conectar, puede haber múltiples loops
            // Por ahora solo manejamos un loop
            break;
        }
    }

    // Remover el último punto si es igual al primero (cerrar el loop)
    if result.len() > 2 && (*result.first().unwrap() - *result.last().unwrap()).length() < tolerance {
        result.pop();
    }

    result
}

/// Triangula un polígono usando ear clipping
fn ear_clip_triangulate(polygon: &[Vector3], plane: &Plane) -> Vec<(Vector3, Vector3, Vector3)> {
    if polygon.len() < 3 {
        return Vec::new();
    }

    if polygon.len() == 3 {
        return vec![(polygon[0], polygon[1], polygon[2])];
    }

    let mut result = Vec::new();
    let mut remaining: Vec<Vector3> = polygon.to_vec();

    // Proyectar a 2D para el test de ear
    let normal = plane.normal_vec();
    let (axis_u, axis_v) = get_perpendicular_axes(normal);

    let project = |p: Vector3| -> (Real, Real) {
        let rel = p - plane.origin_vec();
        (rel.dot(&axis_u), rel.dot(&axis_v))
    };

    let mut iterations = 0;
    let max_iterations = remaining.len() * remaining.len();

    while remaining.len() > 3 && iterations < max_iterations {
        iterations += 1;
        let n = remaining.len();
        let mut ear_found = false;

        for i in 0..n {
            let prev = (i + n - 1) % n;
            let next = (i + 1) % n;

            let p_prev = project(remaining[prev]);
            let p_curr = project(remaining[i]);
            let p_next = project(remaining[next]);

            // Verificar si es convexo
            if !is_convex_2d(p_prev, p_curr, p_next) {
                continue;
            }

            // Verificar que ningún otro punto esté dentro del triángulo
            let mut is_ear = true;
            for (j, &point) in remaining.iter().enumerate().take(n) {
                if j == prev || j == i || j == next {
                    continue;
                }
                let p = project(point);
                if point_in_triangle_2d(p, p_prev, p_curr, p_next) {
                    is_ear = false;
                    break;
                }
            }

            if is_ear {
                result.push((remaining[prev], remaining[i], remaining[next]));
                remaining.remove(i);
                ear_found = true;
                break;
            }
        }

        if !ear_found {
            // No se encontró ear, intentar con cualquier triángulo válido
            break;
        }
    }

    // Añadir el último triángulo
    if remaining.len() == 3 {
        result.push((remaining[0], remaining[1], remaining[2]));
    }

    result
}

/// Obtiene dos ejes perpendiculares a un vector normal
fn get_perpendicular_axes(normal: Vector3) -> (Vector3, Vector3) {
    let up = if normal.y().abs() < 0.9 {
        Vector3::new(0.0, 1.0, 0.0)
    } else {
        Vector3::new(1.0, 0.0, 0.0)
    };

    let u = normal.cross(&up).normalize();
    let v = normal.cross(&u);

    (u, v)
}

/// Verifica si un ángulo es convexo en 2D
fn is_convex_2d(p0: (Real, Real), p1: (Real, Real), p2: (Real, Real)) -> bool {
    let cross = (p1.0 - p0.0) * (p2.1 - p0.1) - (p1.1 - p0.1) * (p2.0 - p0.0);
    cross > 0.0
}

/// Verifica si un punto está dentro de un triángulo en 2D
fn point_in_triangle_2d(
    p: (Real, Real),
    t0: (Real, Real),
    t1: (Real, Real),
    t2: (Real, Real),
) -> bool {
    let sign = |p1: (Real, Real), p2: (Real, Real), p3: (Real, Real)| -> Real {
        (p1.0 - p3.0) * (p2.1 - p3.1) - (p2.0 - p3.0) * (p1.1 - p3.1)
    };

    let d1 = sign(p, t0, t1);
    let d2 = sign(p, t1, t2);
    let d3 = sign(p, t2, t0);

    let has_neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let has_pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;

    !(has_neg && has_pos)
}

/// Corta una malla con múltiples planos secuencialmente
pub fn slice_by_planes(mesh: &Mesh, planes: &[Plane]) -> Result<Vec<Mesh>> {
    if planes.is_empty() {
        return Ok(vec![mesh.clone()]);
    }

    let mut pieces = vec![mesh.clone()];

    for plane in planes {
        let mut new_pieces = Vec::new();

        for piece in pieces {
            match slice_by_plane(&piece, plane) {
                Ok((front, back)) => {
                    new_pieces.push(front);
                    new_pieces.push(back);
                }
                Err(Print3dError::PlaneNoIntersection) => {
                    // El plano no corta esta pieza, mantenerla
                    new_pieces.push(piece);
                }
                Err(e) => return Err(e),
            }
        }

        pieces = new_pieces;
    }

    Ok(pieces)
}

/// Máximo de planos de corte que acepta [`subdivide`]
pub const MAX_CUT_PLANES: usize = 1000;

/// Subdivide una malla en piezas que quepan en el volumen de construcción
///
/// # Argumentos
/// * `mesh` - Malla a subdividir
/// * `config` - Configuración de subdivisión
///
/// # Retorna
/// Vector de piezas etiquetadas
pub fn subdivide(mesh: &Mesh, config: &SubdivideConfig) -> Result<Vec<LabeledPiece>> {
    let bbox = compute_bounding_box(mesh);
    let dims = bbox.dimensions();

    // Verificar si ya cabe
    let margin2 = config.margin * 2.0;
    let effective_volume = [
        config.build_volume[0] - margin2,
        config.build_volume[1] - margin2,
        config.build_volume[2] - margin2,
    ];
    if config.margin < 0.0 || effective_volume.iter().any(|&v| !(v.is_finite() && v > 0.0)) {
        return Err(Print3dError::InvalidConfig(format!(
            "el margen ({}) no deja espacio útil en el volumen de impresión {:?}",
            config.margin, config.build_volume
        )));
    }

    if dims[0] <= effective_volume[0]
        && dims[1] <= effective_volume[1]
        && dims[2] <= effective_volume[2]
    {
        // No necesita subdivisión
        return Ok(vec![LabeledPiece {
            mesh: mesh.clone(),
            id: 0,
            label: "1".to_string(),
            original_position: [bbox.center().x(), bbox.center().y(), bbox.center().z()],
            neighbors: vec![],
            cut_faces: vec![],
        }]);
    }

    // Rechazar antes de generar los planos si serían demasiados
    let cuts = |axis: usize| (dims[axis] / effective_volume[axis]).ceil().max(1.0) - 1.0;
    let needed = match config.strategy {
        SubdivideStrategy::ZLayers => cuts(2),
        _ => cuts(0) + cuts(1) + cuts(2),
    };
    if needed > MAX_CUT_PLANES as Real {
        return Err(Print3dError::InvalidConfig(format!(
            "se necesitarían {needed} planos de corte (máximo {MAX_CUT_PLANES}); revisa las unidades del modelo y del volumen de impresión"
        )));
    }

    // Calcular planos de corte según estrategia
    let planes = match config.strategy {
        SubdivideStrategy::Grid => calculate_grid_planes(&bbox, &effective_volume),
        SubdivideStrategy::ZLayers => calculate_z_planes(&bbox, effective_volume[2]),
        SubdivideStrategy::Optimal => {
            // Por ahora usa grid
            calculate_grid_planes(&bbox, &effective_volume)
        }
    };

    if planes.is_empty() {
        // Algo salió mal, retornar pieza única
        return Ok(vec![LabeledPiece {
            mesh: mesh.clone(),
            id: 0,
            label: "1".to_string(),
            original_position: [bbox.center().x(), bbox.center().y(), bbox.center().z()],
            neighbors: vec![],
            cut_faces: vec![],
        }]);
    }

    // Aplicar cortes
    let meshes = slice_by_planes(mesh, &planes)?;

    // Crear piezas etiquetadas
    let mut pieces: Vec<LabeledPiece> = meshes
        .into_iter()
        .enumerate()
        .map(|(id, m)| {
            let piece_bbox = compute_bounding_box(&m);
            LabeledPiece {
                mesh: m,
                id,
                label: String::new(),
                original_position: [
                    piece_bbox.center().x(),
                    piece_bbox.center().y(),
                    piece_bbox.center().z(),
                ],
                neighbors: vec![],
                cut_faces: vec![],
            }
        })
        .collect();

    // Generar labels
    generate_labels(&mut pieces, config.strategy.into());

    // Detectar vecinos (simplificado: basado en proximidad)
    detect_neighbors(&mut pieces);

    Ok(pieces)
}

/// Detecta piezas vecinas basándose en proximidad de bounding boxes
fn detect_neighbors(pieces: &mut [LabeledPiece]) {
    let bboxes: Vec<BoundingBox> = pieces.iter().map(|p| compute_bounding_box(&p.mesh)).collect();

    for i in 0..pieces.len() {
        for j in (i + 1)..pieces.len() {
            if bboxes_touch(&bboxes[i], &bboxes[j]) {
                pieces[i].neighbors.push(j);
                pieces[j].neighbors.push(i);
            }
        }
    }
}

/// Verifica si dos bounding boxes se tocan (con tolerancia)
fn bboxes_touch(a: &BoundingBox, b: &BoundingBox) -> bool {
    let tolerance = 1.0; // 1mm de tolerancia

    // Verificar si hay overlap o contacto en cada eje
    let x_touch = a.max[0] + tolerance >= b.min[0] && b.max[0] + tolerance >= a.min[0];
    let y_touch = a.max[1] + tolerance >= b.min[1] && b.max[1] + tolerance >= a.min[1];
    let z_touch = a.max[2] + tolerance >= b.min[2] && b.max[2] + tolerance >= a.min[2];

    x_touch && y_touch && z_touch
}

impl From<SubdivideStrategy> for LabelingScheme {
    fn from(strategy: SubdivideStrategy) -> Self {
        match strategy {
            SubdivideStrategy::Grid => LabelingScheme::Alphanumeric,
            SubdivideStrategy::ZLayers => LabelingScheme::Numeric,
            SubdivideStrategy::Optimal => LabelingScheme::Numeric,
        }
    }
}

/// Calcula planos de corte para estrategia Grid
fn calculate_grid_planes(bbox: &BoundingBox, max_size: &[Real; 3]) -> Vec<Plane> {
    let mut planes = Vec::new();
    let dims = bbox.dimensions();

    // Proteger contra dimensiones inválidas
    if max_size[0] <= 0.0 || max_size[1] <= 0.0 || max_size[2] <= 0.0 {
        return planes;
    }

    // Planos en X
    let nx = (dims[0] / max_size[0]).ceil() as usize;
    if nx > 1 {
        let step = dims[0] / nx as Real;
        for i in 1..nx {
            let x = bbox.min[0] + step * i as Real;
            planes.push(Plane::yz_at_x(x));
        }
    }

    // Planos en Y
    let ny = (dims[1] / max_size[1]).ceil() as usize;
    if ny > 1 {
        let step = dims[1] / ny as Real;
        for i in 1..ny {
            let y = bbox.min[1] + step * i as Real;
            planes.push(Plane::xz_at_y(y));
        }
    }

    // Planos en Z
    let nz = (dims[2] / max_size[2]).ceil() as usize;
    if nz > 1 {
        let step = dims[2] / nz as Real;
        for i in 1..nz {
            let z = bbox.min[2] + step * i as Real;
            planes.push(Plane::xy_at_z(z));
        }
    }

    planes
}

/// Calcula planos de corte solo en Z (capas horizontales)
fn calculate_z_planes(bbox: &BoundingBox, max_height: Real) -> Vec<Plane> {
    let mut planes = Vec::new();
    let height = bbox.max[2] - bbox.min[2];
    if !(max_height.is_finite() && max_height > 0.0) {
        return planes;
    }

    let nz = (height / max_height).ceil() as usize;
    if nz > 1 {
        let step = height / nz as Real;
        for i in 1..nz {
            let z = bbox.min[2] + step * i as Real;
            planes.push(Plane::xy_at_z(z));
        }
    }

    planes
}

/// Genera labels para las piezas según el esquema
pub fn generate_labels(pieces: &mut [LabeledPiece], scheme: LabelingScheme) {
    for (i, piece) in pieces.iter_mut().enumerate() {
        piece.id = i;
        piece.label = match scheme {
            LabelingScheme::Numeric => format!("{}", i + 1),
            LabelingScheme::Alphanumeric => {
                let num = i / 26 + 1;
                let letter = (b'A' + (i % 26) as u8) as char;
                format!("{}{}", num, letter)
            }
            LabelingScheme::Coordinate => {
                format!("P{}", i + 1)
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Crea un cubo unitario para tests
    fn create_unit_cube() -> Mesh {
        let positions = vec![
            // Front face
            Vector3::new(0.0, 0.0, 1.0),
            Vector3::new(1.0, 0.0, 1.0),
            Vector3::new(1.0, 1.0, 1.0),
            Vector3::new(0.0, 1.0, 1.0),
            // Back face
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(1.0, 1.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
        ];

        let indices = vec![
            // Front
            [0, 1, 2],
            [0, 2, 3],
            // Back
            [5, 4, 7],
            [5, 7, 6],
            // Top
            [3, 2, 6],
            [3, 6, 7],
            // Bottom
            [4, 5, 1],
            [4, 1, 0],
            // Right
            [1, 5, 6],
            [1, 6, 2],
            // Left
            [4, 0, 3],
            [4, 3, 7],
        ];

        Mesh::from_triangles(&positions, &indices)
    }

    #[test]
    fn test_plane_signed_distance() {
        let plane = Plane::xy_at_z(5.0);

        assert!((plane.signed_distance(Vector3::new(0.0, 0.0, 10.0)) - 5.0).abs() < 1e-10);
        assert!((plane.signed_distance(Vector3::new(0.0, 0.0, 0.0)) - (-5.0)).abs() < 1e-10);
        assert!((plane.signed_distance(Vector3::new(0.0, 0.0, 5.0))).abs() < 1e-10);
    }

    #[test]
    fn test_plane_classify_point() {
        let plane = Plane::xy_at_z(0.0);

        assert_eq!(
            plane.classify_point(Vector3::new(0.0, 0.0, 1.0), 0.01),
            PointClassification::Front
        );
        assert_eq!(
            plane.classify_point(Vector3::new(0.0, 0.0, -1.0), 0.01),
            PointClassification::Back
        );
        assert_eq!(
            plane.classify_point(Vector3::new(0.0, 0.0, 0.0), 0.01),
            PointClassification::OnPlane
        );
    }

    #[test]
    fn test_plane_intersect_segment() {
        let plane = Plane::xy_at_z(5.0);

        // Segmento que cruza el plano
        let t = plane
            .intersect_segment(Vector3::new(0.0, 0.0, 0.0), Vector3::new(0.0, 0.0, 10.0))
            .unwrap();
        assert!((t - 0.5).abs() < 1e-10);

        // Segmento que no cruza
        let t = plane.intersect_segment(Vector3::new(0.0, 0.0, 0.0), Vector3::new(0.0, 0.0, 4.0));
        assert!(t.is_none());

        // Segmento paralelo
        let t = plane.intersect_segment(Vector3::new(0.0, 0.0, 5.0), Vector3::new(1.0, 0.0, 5.0));
        assert!(t.is_none());
    }

    #[test]
    fn test_plane_from_points() {
        let plane = Plane::from_points(
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
        )
        .unwrap();

        // Normal debe ser Z (o -Z)
        assert!((plane.normal[2].abs() - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_slice_cube_horizontal() {
        let cube = create_unit_cube();
        let plane = Plane::xy_at_z(0.5);

        let result = slice_by_plane(&cube, &plane);
        assert!(result.is_ok());

        let (front, back) = result.unwrap();

        // Ambas partes deben tener triángulos
        assert!(!front.faces.is_empty());
        assert!(!back.faces.is_empty());

        // Verificar bounding boxes
        let front_bbox = compute_bounding_box(&front);
        let back_bbox = compute_bounding_box(&back);

        // Front debe estar en Z > 0.5 (aproximadamente)
        assert!(front_bbox.min[2] >= 0.5 - EPSILON);
        // Back debe estar en Z < 0.5 (aproximadamente)
        assert!(back_bbox.max[2] <= 0.5 + EPSILON);
    }

    #[test]
    fn test_slice_cube_vertical() {
        let cube = create_unit_cube();
        let plane = Plane::yz_at_x(0.5);

        let result = slice_by_plane(&cube, &plane);
        assert!(result.is_ok());

        let (front, back) = result.unwrap();
        assert!(!front.faces.is_empty());
        assert!(!back.faces.is_empty());
    }

    #[test]
    fn test_slice_no_intersection() {
        let cube = create_unit_cube();

        // Plano fuera del cubo
        let plane = Plane::xy_at_z(2.0);
        let result = slice_by_plane(&cube, &plane);

        assert!(matches!(result, Err(Print3dError::PlaneNoIntersection)));
    }

    #[test]
    fn test_slice_by_planes_multiple() {
        let cube = create_unit_cube();
        let planes = vec![Plane::xy_at_z(0.5), Plane::yz_at_x(0.5)];

        let result = slice_by_planes(&cube, &planes);
        assert!(result.is_ok());

        let pieces = result.unwrap();
        // 2 planos perpendiculares deberían crear ~4 piezas
        assert!(pieces.len() >= 2);
    }

    #[test]
    fn test_calculate_grid_planes() {
        let bbox = BoundingBox::new([0.0, 0.0, 0.0], [440.0, 220.0, 500.0]);

        let max_size = [220.0, 220.0, 250.0];
        let planes = calculate_grid_planes(&bbox, &max_size);

        // X: 440/220 = 2 -> 1 plano
        // Y: 220/220 = 1 -> 0 planos
        // Z: 500/250 = 2 -> 1 plano
        assert_eq!(planes.len(), 2);
    }

    #[test]
    fn test_generate_labels_numeric() {
        let mut pieces = vec![
            LabeledPiece {
                mesh: Mesh::new(),
                id: 0,
                label: String::new(),
                original_position: [0.0, 0.0, 0.0],
                neighbors: vec![],
                cut_faces: vec![],
            },
            LabeledPiece {
                mesh: Mesh::new(),
                id: 0,
                label: String::new(),
                original_position: [0.0, 0.0, 0.0],
                neighbors: vec![],
                cut_faces: vec![],
            },
        ];

        generate_labels(&mut pieces, LabelingScheme::Numeric);

        assert_eq!(pieces[0].label, "1");
        assert_eq!(pieces[1].label, "2");
    }

    #[test]
    fn test_generate_labels_alphanumeric() {
        let mut pieces: Vec<LabeledPiece> = (0..28)
            .map(|_| LabeledPiece {
                mesh: Mesh::new(),
                id: 0,
                label: String::new(),
                original_position: [0.0, 0.0, 0.0],
                neighbors: vec![],
                cut_faces: vec![],
            })
            .collect();

        generate_labels(&mut pieces, LabelingScheme::Alphanumeric);

        assert_eq!(pieces[0].label, "1A");
        assert_eq!(pieces[25].label, "1Z");
        assert_eq!(pieces[26].label, "2A");
        assert_eq!(pieces[27].label, "2B");
    }

    #[test]
    fn test_is_convex_2d() {
        // Triángulo counter-clockwise
        assert!(is_convex_2d((0.0, 0.0), (1.0, 0.0), (0.5, 1.0)));
        // Triángulo clockwise
        assert!(!is_convex_2d((0.0, 0.0), (0.5, 1.0), (1.0, 0.0)));
    }

    #[test]
    fn test_point_in_triangle_2d() {
        let t0 = (0.0, 0.0);
        let t1 = (1.0, 0.0);
        let t2 = (0.5, 1.0);

        assert!(point_in_triangle_2d((0.5, 0.3), t0, t1, t2));
        assert!(!point_in_triangle_2d((2.0, 0.0), t0, t1, t2));
    }

    #[test]
    fn test_subdivide_small_mesh() {
        let cube = create_unit_cube();
        let config = SubdivideConfig {
            build_volume: [100.0, 100.0, 100.0], // Mucho más grande que el cubo
            margin: 1.0,
            ..Default::default()
        };

        let result = subdivide(&cube, &config);
        assert!(result.is_ok());

        let pieces = result.unwrap();
        // Cubo de 1x1x1 cabe en 98x98x98 (100-2*1), no necesita subdivisión
        assert_eq!(pieces.len(), 1);
        assert_eq!(pieces[0].label, "1");
    }

    #[test]
    fn test_subdivide_rejects_margin_without_room() {
        let cube = create_unit_cube();
        for strategy in [SubdivideStrategy::Grid, SubdivideStrategy::ZLayers] {
            let config = SubdivideConfig {
                build_volume: [4.0, 4.0, 4.0],
                margin: 2.0,
                strategy,
                ..Default::default()
            };
            assert!(matches!(subdivide(&cube, &config), Err(Print3dError::InvalidConfig(_))));
        }
    }

    #[test]
    fn test_subdivide_rejects_too_many_planes() {
        // Cubo unitario con un volumen de impresión diminuto (p. ej. unidades mezcladas)
        let cube = create_unit_cube();
        let config = SubdivideConfig {
            build_volume: [1e-3, 1e-3, 1e-3],
            margin: 0.0,
            ..Default::default()
        };
        assert!(matches!(subdivide(&cube, &config), Err(Print3dError::InvalidConfig(_))));
    }
}
