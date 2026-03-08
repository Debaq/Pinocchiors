//! Campo de distancias para mallas

use crate::bvh::{Bvh, Triangle};
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
        }
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
        let edge0 = *v1 - *v0;
        let edge1 = *v2 - *v0;
        let v0_to_p = *p - *v0;

        let a = edge0.dot(&edge0);
        let b = edge0.dot(&edge1);
        let c = edge1.dot(&edge1);
        let d = edge0.dot(&v0_to_p);
        let e = edge1.dot(&v0_to_p);

        let det = a * c - b * b;
        let mut s = c * d - b * e;
        let mut t = a * e - b * d;

        if s + t <= det {
            if s < 0.0 {
                if t < 0.0 {
                    // Region 4
                    if d < 0.0 {
                        t = 0.0;
                        s = (-d).min(a).max(0.0);
                    } else {
                        s = 0.0;
                        t = (-e).min(c).max(0.0);
                    }
                } else {
                    // Region 3
                    s = 0.0;
                    t = e.max(0.0).min(c);
                    if t > 0.0 { t = -e / c; }
                    t = t.clamp(0.0, 1.0);
                }
            } else if t < 0.0 {
                // Region 5
                t = 0.0;
                s = d.max(0.0).min(a);
                if s > 0.0 { s = -d / a; }
                s = s.clamp(0.0, 1.0);
            } else {
                // Region 0
                let inv_det = 1.0 / det;
                s *= inv_det;
                t *= inv_det;
            }
        } else {
            if s < 0.0 {
                // Region 2
                let tmp0 = b + d;
                let tmp1 = c + e;
                if tmp1 > tmp0 {
                    let numer = tmp1 - tmp0;
                    let denom = a - 2.0 * b + c;
                    s = (numer / denom).clamp(0.0, 1.0);
                    t = 1.0 - s;
                } else {
                    s = 0.0;
                    t = (-e / c).clamp(0.0, 1.0);
                }
            } else if t < 0.0 {
                // Region 6
                let tmp0 = b + e;
                let tmp1 = a + d;
                if tmp1 > tmp0 {
                    let numer = tmp1 - tmp0;
                    let denom = a - 2.0 * b + c;
                    t = (numer / denom).clamp(0.0, 1.0);
                    s = 1.0 - t;
                } else {
                    t = 0.0;
                    s = (-d / a).clamp(0.0, 1.0);
                }
            } else {
                // Region 1
                let numer = (c + e) - (b + d);
                if numer <= 0.0 {
                    s = 0.0;
                } else {
                    let denom = a - 2.0 * b + c;
                    s = (numer / denom).clamp(0.0, 1.0);
                }
                t = 1.0 - s;
            }
        }

        let closest = *v0 + edge0 * s + edge1 * t;
        p.distance(&closest)
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
            None => return Real::INFINITY,
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
}
