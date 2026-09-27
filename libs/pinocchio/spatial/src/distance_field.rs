//! Campo de distancias para mallas

use crate::bvh::{Bvh, ClosestHit, Triangle};
use std::collections::VecDeque;
use pinocchio_math::{Real, Rect, Vector3};
use pinocchio_mesh::Mesh;
use rayon::prelude::*;

/// Campo de distancias discretizado en una grilla 3D
pub struct DistanceField {
    /// Valores de distancia en cada celda
    values: Vec<Real>,
    /// Límites del campo
    bounds: Rect,
    /// Resolución en cada dimensión
    resolution: [usize; 3],
    /// Tamaño de celda
    cell_size: Vector3,
    /// `true` si los valores tienen signo (positivo dentro, negativo fuera)
    signed: bool,
}

impl DistanceField {
    /// Crea un campo de distancias vacío
    pub fn new(bounds: Rect, resolution: [usize; 3]) -> Self {
        let size = bounds.size();
        let cell_size = Vector3::new(
            size.x() / resolution[0] as Real,
            size.y() / resolution[1] as Real,
            size.z() / resolution[2] as Real,
        );

        let total_cells = resolution[0] * resolution[1] * resolution[2];
        let values = vec![Real::INFINITY; total_cells];

        Self {
            values,
            bounds,
            resolution,
            cell_size,
            signed: false,
        }
    }

    /// Calcula el campo de distancias desde una malla
    /// Usa BVH + paralelismo para mallas grandes (>1000 triángulos)
    pub fn from_mesh(mesh: &Mesh, resolution: [usize; 3], padding: Real) -> Self {
        if mesh.num_faces() > 1000 {
            Self::from_mesh_bvh(mesh, resolution, padding)
        } else {
            Self::from_mesh_parallel(mesh, resolution, padding)
        }
    }

    /// Calcula el campo usando BVH para aceleración (O(log n) por consulta)
    pub fn from_mesh_bvh(mesh: &Mesh, resolution: [usize; 3], padding: Real) -> Self {
        let bounds = mesh.bounding_box().expand(padding);
        let size = bounds.size();
        let cell_size = Vector3::new(
            size.x() / resolution[0] as Real,
            size.y() / resolution[1] as Real,
            size.z() / resolution[2] as Real,
        );

        // Construir BVH desde los triángulos de la malla
        let triangles: Vec<Triangle> = (0..mesh.num_faces())
            .map(|i| {
                let [v0, v1, v2] = mesh.get_face_positions(i);
                Triangle::new(v0, v1, v2)
            })
            .collect();
        let bvh = Bvh::build(triangles);

        let total_cells = resolution[0] * resolution[1] * resolution[2];
        let res_xy = resolution[0] * resolution[1];
        let res_x = resolution[0];

        // Calcular distancias en paralelo usando el BVH
        let values: Vec<Real> = (0..total_cells)
            .into_par_iter()
            .map(|idx| {
                let z = idx / res_xy;
                let rem = idx % res_xy;
                let y = rem / res_x;
                let x = rem % res_x;

                let pos = Vector3::new(
                    bounds.min.x() + (x as Real + 0.5) * cell_size.x(),
                    bounds.min.y() + (y as Real + 0.5) * cell_size.y(),
                    bounds.min.z() + (z as Real + 0.5) * cell_size.z(),
                );

                bvh.query_distance(&pos)
            })
            .collect();

        Self {
            values,
            bounds,
            resolution,
            cell_size,
            signed: false,
        }
    }

    /// Calcula el campo de distancias desde una malla (paralelo con rayon, sin BVH)
    pub fn from_mesh_parallel(mesh: &Mesh, resolution: [usize; 3], padding: Real) -> Self {
        let bounds = mesh.bounding_box().expand(padding);
        let size = bounds.size();
        let cell_size = Vector3::new(
            size.x() / resolution[0] as Real,
            size.y() / resolution[1] as Real,
            size.z() / resolution[2] as Real,
        );

        let total_cells = resolution[0] * resolution[1] * resolution[2];
        let res_xy = resolution[0] * resolution[1];
        let res_x = resolution[0];

        // Calcular distancias en paralelo
        let values: Vec<Real> = (0..total_cells)
            .into_par_iter()
            .map(|idx| {
                let z = idx / res_xy;
                let rem = idx % res_xy;
                let y = rem / res_x;
                let x = rem % res_x;

                let pos = Vector3::new(
                    bounds.min.x() + (x as Real + 0.5) * cell_size.x(),
                    bounds.min.y() + (y as Real + 0.5) * cell_size.y(),
                    bounds.min.z() + (z as Real + 0.5) * cell_size.z(),
                );

                Self::distance_to_mesh(&pos, mesh)
            })
            .collect();

        Self {
            values,
            bounds,
            resolution,
            cell_size,
            signed: false,
        }
    }

    /// Calcula un campo de distancias **con signo**: positivo dentro de la malla,
    /// negativo fuera.
    ///
    /// El signo se decide en dos pasos:
    /// 1. Las celdas lejos de la superficie se clasifican con un flood fill desde
    ///    el borde de la grilla (el `padding` garantiza que el borde es exterior).
    ///    Es robusto frente a costuras sin soldar, cáscaras superpuestas y
    ///    agujeros más pequeños que una celda.
    /// 2. Las celdas de la banda de superficie usan la normal del triángulo más
    ///    cercano. Si la malla tiene las normales invertidas, se detecta
    ///    comparando con el resultado del flood fill y se corrige.
    ///
    /// Fuera de los límites de la grilla, [`sample`](Self::sample) devuelve
    /// `-inf` (exterior).
    pub fn from_mesh_signed(mesh: &Mesh, resolution: [usize; 3], padding: Real) -> Self {
        let bounds = mesh.bounding_box().expand(padding);
        let mut field = Self::new(bounds, resolution);
        field.signed = true;

        let triangles: Vec<Triangle> = (0..mesh.num_faces())
            .map(|i| {
                let [v0, v1, v2] = mesh.get_face_positions(i);
                Triangle::new(v0, v1, v2)
            })
            .collect();
        let bvh = Bvh::build(triangles);

        let [rx, ry, rz] = resolution;
        let total = rx * ry * rz;
        let hits: Vec<Option<ClosestHit>> = (0..total)
            .into_par_iter()
            .map(|idx| {
                let (x, y, z) = (idx % rx, (idx / rx) % ry, idx / (rx * ry));
                bvh.query_closest(&field.cell_center(x, y, z))
            })
            .collect();

        // Banda de superficie: cualquier triángulo que atraviese una celda queda
        // a menos de media diagonal de su centro, así que la banda es estanca.
        let band_width = field.cell_size.length() * 0.5;
        let in_band: Vec<bool> = hits
            .iter()
            .map(|h| h.is_some_and(|h| h.distance <= band_width))
            .collect();

        let neighbors6 = |idx: usize| {
            let (x, y, z) = (idx % rx, (idx / rx) % ry, idx / (rx * ry));
            let mut out = [None; 6];
            if x > 0 { out[0] = Some(idx - 1); }
            if x + 1 < rx { out[1] = Some(idx + 1); }
            if y > 0 { out[2] = Some(idx - rx); }
            if y + 1 < ry { out[3] = Some(idx + rx); }
            if z > 0 { out[4] = Some(idx - rx * ry); }
            if z + 1 < rz { out[5] = Some(idx + rx * ry); }
            out
        };

        // 1. Flood fill del exterior desde el borde de la grilla
        let mut outside = vec![false; total];
        let mut queue = VecDeque::new();
        for idx in 0..total {
            let (x, y, z) = (idx % rx, (idx / rx) % ry, idx / (rx * ry));
            let on_border = x == 0 || y == 0 || z == 0 || x + 1 == rx || y + 1 == ry || z + 1 == rz;
            if on_border && !in_band[idx] {
                outside[idx] = true;
                queue.push_back(idx);
            }
        }
        while let Some(idx) = queue.pop_front() {
            for n in neighbors6(idx).into_iter().flatten() {
                if !outside[n] && !in_band[n] {
                    outside[n] = true;
                    queue.push_back(n);
                }
            }
        }
        let has_interior = (0..total).any(|i| !in_band[i] && !outside[i]);

        // 2. Banda: signo por la normal del triángulo más cercano
        let normal_says_inside = |idx: usize| -> bool {
            let hit = hits[idx].expect("celda de banda sin triángulo");
            let (x, y, z) = (idx % rx, (idx / rx) % ry, idx / (rx * ry));
            let to_cell = field.cell_center(x, y, z) - hit.point;
            to_cell.dot(&bvh.triangle(hit.triangle).normal()) < 0.0
        };

        // Calibrar la orientación con las celdas de banda vecinas a regiones ya
        // clasificadas por el flood fill
        let mut agree = 0usize;
        let mut disagree = 0usize;
        if has_interior {
            for idx in (0..total).filter(|&i| in_band[i]) {
                for n in neighbors6(idx).into_iter().flatten() {
                    if in_band[n] {
                        continue;
                    }
                    let expected_inside = !outside[n];
                    if normal_says_inside(idx) == expected_inside {
                        agree += 1;
                    } else {
                        disagree += 1;
                    }
                    break;
                }
            }
        }
        let flip = disagree > agree;

        let values: Vec<Real> = (0..total)
            .map(|idx| {
                let Some(hit) = hits[idx] else {
                    return Real::NEG_INFINITY;
                };
                let inside = if in_band[idx] || !has_interior {
                    // Sin interior (malla muy abierta): solo queda la normal
                    normal_says_inside(idx) != flip
                } else {
                    !outside[idx]
                };
                if inside { hit.distance } else { -hit.distance }
            })
            .collect();
        field.values = values;

        field
    }

    /// Indica si el campo tiene signo (ver [`from_mesh_signed`](Self::from_mesh_signed))
    pub fn is_signed(&self) -> bool {
        self.signed
    }

    /// Versión secuencial para benchmarks/debugging
    pub fn from_mesh_sequential(mesh: &Mesh, resolution: [usize; 3], padding: Real) -> Self {
        let bounds = mesh.bounding_box().expand(padding);
        let mut field = Self::new(bounds, resolution);

        for z in 0..resolution[2] {
            for y in 0..resolution[1] {
                for x in 0..resolution[0] {
                    let pos = field.cell_center(x, y, z);
                    let dist = Self::distance_to_mesh(&pos, mesh);
                    field.set(x, y, z, dist);
                }
            }
        }

        field
    }

    /// Calcula la distancia de un punto a la malla
    fn distance_to_mesh(point: &Vector3, mesh: &Mesh) -> Real {
        let mut min_dist = Real::INFINITY;

        for face_idx in 0..mesh.num_faces() {
            let [p0, p1, p2] = mesh.get_face_positions(face_idx);
            let dist = Self::point_triangle_distance(point, &p0, &p1, &p2);
            min_dist = min_dist.min(dist);
        }

        // Determinar signo (dentro/fuera)
        // Simplificado: usar la normal del triángulo más cercano
        min_dist
    }

    /// Distancia de un punto a un triángulo
    fn point_triangle_distance(p: &Vector3, v0: &Vector3, v1: &Vector3, v2: &Vector3) -> Real {
        crate::bvh::point_triangle_distance(p, v0, v1, v2)
    }

    /// Obtiene el centro de una celda
    pub fn cell_center(&self, x: usize, y: usize, z: usize) -> Vector3 {
        Vector3::new(
            self.bounds.min.x() + (x as Real + 0.5) * self.cell_size.x(),
            self.bounds.min.y() + (y as Real + 0.5) * self.cell_size.y(),
            self.bounds.min.z() + (z as Real + 0.5) * self.cell_size.z(),
        )
    }

    /// Convierte coordenadas del mundo a índices de celda
    pub fn world_to_cell(&self, pos: &Vector3) -> Option<(usize, usize, usize)> {
        if !self.bounds.contains_point(pos) {
            return None;
        }

        let local = *pos - self.bounds.min;
        let x = (local.x() / self.cell_size.x()) as usize;
        let y = (local.y() / self.cell_size.y()) as usize;
        let z = (local.z() / self.cell_size.z()) as usize;

        Some((
            x.min(self.resolution[0] - 1),
            y.min(self.resolution[1] - 1),
            z.min(self.resolution[2] - 1),
        ))
    }

    /// Índice lineal de una celda
    fn cell_index(&self, x: usize, y: usize, z: usize) -> usize {
        z * self.resolution[1] * self.resolution[0] + y * self.resolution[0] + x
    }

    /// Obtiene el valor de distancia en una celda
    pub fn get(&self, x: usize, y: usize, z: usize) -> Real {
        self.values[self.cell_index(x, y, z)]
    }

    /// Establece el valor de distancia en una celda
    pub fn set(&mut self, x: usize, y: usize, z: usize, value: Real) {
        let idx = self.cell_index(x, y, z);
        self.values[idx] = value;
    }

    /// Interpola la distancia en una posición arbitraria (trilineal)
    pub fn sample(&self, pos: &Vector3) -> Real {
        let (x, y, z) = match self.world_to_cell(pos) {
            Some(coords) => coords,
            None => {
                return if self.signed { Real::NEG_INFINITY } else { Real::INFINITY };
            }
        };

        // Interpolación trilineal simplificada
        self.get(x, y, z)
    }

    /// Calcula el gradiente en una posición
    pub fn gradient(&self, pos: &Vector3) -> Vector3 {
        let h = self.cell_size.min_component() * 0.5;

        let dx = self.sample(&(*pos + Vector3::unit_x() * h))
            - self.sample(&(*pos - Vector3::unit_x() * h));
        let dy = self.sample(&(*pos + Vector3::unit_y() * h))
            - self.sample(&(*pos - Vector3::unit_y() * h));
        let dz = self.sample(&(*pos + Vector3::unit_z() * h))
            - self.sample(&(*pos - Vector3::unit_z() * h));

        Vector3::new(dx, dy, dz).normalize()
    }

    /// Resolución del campo
    pub fn resolution(&self) -> [usize; 3] {
        self.resolution
    }

    /// Límites del campo
    pub fn bounds(&self) -> &Rect {
        &self.bounds
    }

    /// Acceso a los valores internos (para testing/comparación)
    pub fn values(&self) -> &[Real] {
        &self.values
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_distance_field_creation() {
        let bounds = Rect::new(Vector3::zero(), Vector3::new(1.0, 1.0, 1.0));
        let field = DistanceField::new(bounds, [10, 10, 10]);

        assert_eq!(field.resolution(), [10, 10, 10]);
    }

    #[test]
    fn test_cell_center() {
        let bounds = Rect::new(Vector3::zero(), Vector3::new(1.0, 1.0, 1.0));
        let field = DistanceField::new(bounds, [10, 10, 10]);

        let center = field.cell_center(0, 0, 0);
        assert!((center.x() - 0.05).abs() < 1e-10);
    }

    #[test]
    fn test_point_triangle_distance() {
        let v0 = Vector3::zero();
        let v1 = Vector3::new(1.0, 0.0, 0.0);
        let v2 = Vector3::new(0.0, 1.0, 0.0);

        // Punto en un vértice del triángulo
        let p = Vector3::zero();
        let dist = DistanceField::point_triangle_distance(&p, &v0, &v1, &v2);
        assert!(dist.abs() < 0.01);

        // Punto alejado del triángulo
        let p2 = Vector3::new(0.0, 0.0, 5.0);
        let dist2 = DistanceField::point_triangle_distance(&p2, &v0, &v1, &v2);
        assert!(dist2 > 4.0);
    }

    #[test]
    fn test_parallel_equals_sequential() {
        // Crear una malla simple (tetraedro)
        let positions = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.5, 1.0, 0.0),
            Vector3::new(0.5, 0.5, 1.0),
        ];
        let indices = vec![
            [0, 1, 2],
            [0, 1, 3],
            [1, 2, 3],
            [0, 2, 3],
        ];
        let mesh = Mesh::from_triangles(&positions, &indices);

        let resolution = [8, 8, 8];
        let padding = 0.1;

        let parallel = DistanceField::from_mesh_parallel(&mesh, resolution, padding);
        let sequential = DistanceField::from_mesh_sequential(&mesh, resolution, padding);

        // Verificar que producen los mismos resultados
        assert_eq!(parallel.values().len(), sequential.values().len());
        for (i, (p, s)) in parallel.values().iter().zip(sequential.values().iter()).enumerate() {
            assert!(
                (p - s).abs() < 1e-10,
                "Diferencia en índice {}: parallel={}, sequential={}", i, p, s
            );
        }
    }

    #[test]
    fn test_bvh_equals_direct() {
        // Crear una malla simple (tetraedro)
        let positions = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.5, 1.0, 0.0),
            Vector3::new(0.5, 0.5, 1.0),
        ];
        let indices = vec![
            [0, 1, 2],
            [0, 1, 3],
            [1, 2, 3],
            [0, 2, 3],
        ];
        let mesh = Mesh::from_triangles(&positions, &indices);

        let resolution = [8, 8, 8];
        let padding = 0.1;

        let bvh_result = DistanceField::from_mesh_bvh(&mesh, resolution, padding);
        let direct = DistanceField::from_mesh_parallel(&mesh, resolution, padding);

        // Verificar que producen los mismos resultados
        assert_eq!(bvh_result.values().len(), direct.values().len());
        for (i, (b, d)) in bvh_result.values().iter().zip(direct.values().iter()).enumerate() {
            assert!(
                (b - d).abs() < 1e-10,
                "Diferencia en índice {}: bvh={}, direct={}", i, b, d
            );
        }
    }

    /// Cubo cerrado centrado en `c` con lado `2h` (normales hacia fuera, o hacia dentro si `invert`)
    fn cube_triangles(c: Vector3, h: Real, invert: bool) -> (Vec<Vector3>, Vec<[usize; 3]>) {
        let p = |x: Real, y: Real, z: Real| c + Vector3::new(x * h, y * h, z * h);
        let v = vec![
            p(-1.0, -1.0, -1.0), p(1.0, -1.0, -1.0), p(1.0, 1.0, -1.0), p(-1.0, 1.0, -1.0),
            p(-1.0, -1.0, 1.0), p(1.0, -1.0, 1.0), p(1.0, 1.0, 1.0), p(-1.0, 1.0, 1.0),
        ];
        let mut f = vec![
            [0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7],
            [0, 1, 5], [0, 5, 4], [3, 7, 6], [3, 6, 2],
            [0, 4, 7], [0, 7, 3], [1, 2, 6], [1, 6, 5],
        ];
        if invert {
            for t in &mut f {
                t.swap(1, 2);
            }
        }
        (v, f)
    }

    fn mesh_from(parts: &[(Vec<Vector3>, Vec<[usize; 3]>)]) -> Mesh {
        let mut verts = Vec::new();
        let mut faces = Vec::new();
        for (v, f) in parts {
            let off = verts.len();
            verts.extend_from_slice(v);
            faces.extend(f.iter().map(|t| [t[0] + off, t[1] + off, t[2] + off]));
        }
        Mesh::from_triangles(&verts, &faces)
    }

    #[test]
    fn test_signed_field_cube() {
        for invert in [false, true] {
            let mesh = mesh_from(&[cube_triangles(Vector3::zero(), 1.0, invert)]);
            let field = DistanceField::from_mesh_signed(&mesh, [24, 24, 24], 0.5);
            assert!(field.is_signed());

            let center = field.sample(&Vector3::zero());
            assert!(center > 0.8, "invert={invert}: centro = {center}");
            let outside = field.sample(&Vector3::new(1.4, 0.0, 0.0));
            assert!(outside < 0.0, "invert={invert}: exterior = {outside}");
            // Fuera de la grilla: exterior
            assert_eq!(field.sample(&Vector3::new(100.0, 0.0, 0.0)), Real::NEG_INFINITY);
        }
    }

    #[test]
    fn test_signed_field_overlapping_shells() {
        // Dos cubos que se superponen: la zona común debe ser interior
        let mesh = mesh_from(&[
            cube_triangles(Vector3::new(-0.5, 0.0, 0.0), 1.0, false),
            cube_triangles(Vector3::new(0.5, 0.0, 0.0), 1.0, false),
        ]);
        let field = DistanceField::from_mesh_signed(&mesh, [32, 24, 24], 0.5);
        assert!(field.sample(&Vector3::new(0.0, 0.0, 0.0)) > 0.0);
        assert!(field.sample(&Vector3::new(-1.2, 0.0, 0.0)) > 0.0);
        assert!(field.sample(&Vector3::new(1.2, 0.0, 0.0)) > 0.0);
        assert!(field.sample(&Vector3::new(0.0, 1.4, 0.0)) < 0.0);
    }
}
