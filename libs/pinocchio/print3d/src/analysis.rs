//! Análisis de propiedades geométricas de mallas 3D
//!
//! Proporciona funciones para calcular volumen, área superficial,
//! centro de masa y bounding box de mallas triangulares.

use pinocchio_math::{Real, Vector3};
use pinocchio_mesh::Mesh;
use serde::{Deserialize, Serialize};

use crate::{AnalysisConfig, Print3dError, Result};

/// Bounding box axis-aligned (AABB)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BoundingBox {
    /// Esquina mínima
    pub min: [Real; 3],
    /// Esquina máxima
    pub max: [Real; 3],
}

impl BoundingBox {
    /// Crea un bounding box desde min y max
    pub fn new(min: [Real; 3], max: [Real; 3]) -> Self {
        Self { min, max }
    }

    /// Bounding box vacío (invertido para expansión)
    pub fn empty() -> Self {
        Self {
            min: [Real::INFINITY, Real::INFINITY, Real::INFINITY],
            max: [Real::NEG_INFINITY, Real::NEG_INFINITY, Real::NEG_INFINITY],
        }
    }

    /// Expande el bounding box para incluir un punto
    pub fn expand(&mut self, point: Vector3) {
        self.min[0] = self.min[0].min(point.x());
        self.min[1] = self.min[1].min(point.y());
        self.min[2] = self.min[2].min(point.z());
        self.max[0] = self.max[0].max(point.x());
        self.max[1] = self.max[1].max(point.y());
        self.max[2] = self.max[2].max(point.z());
    }

    /// Dimensiones del bounding box
    pub fn dimensions(&self) -> [Real; 3] {
        [
            self.max[0] - self.min[0],
            self.max[1] - self.min[1],
            self.max[2] - self.min[2],
        ]
    }

    /// Centro del bounding box
    pub fn center(&self) -> Vector3 {
        Vector3::new(
            (self.min[0] + self.max[0]) / 2.0,
            (self.min[1] + self.max[1]) / 2.0,
            (self.min[2] + self.max[2]) / 2.0,
        )
    }

    /// Dimensión máxima (el lado más largo)
    pub fn max_dimension(&self) -> Real {
        let d = self.dimensions();
        d[0].max(d[1]).max(d[2])
    }

    /// Volumen del bounding box
    pub fn volume(&self) -> Real {
        let d = self.dimensions();
        d[0] * d[1] * d[2]
    }

    /// Verifica si un punto está dentro del bounding box
    pub fn contains(&self, point: Vector3) -> bool {
        point.x() >= self.min[0]
            && point.x() <= self.max[0]
            && point.y() >= self.min[1]
            && point.y() <= self.max[1]
            && point.z() >= self.min[2]
            && point.z() <= self.max[2]
    }

    /// Crea desde Vector3 de pinocchio_math
    pub fn from_vectors(min: Vector3, max: Vector3) -> Self {
        Self {
            min: [min.x(), min.y(), min.z()],
            max: [max.x(), max.y(), max.z()],
        }
    }

    /// Obtiene min como Vector3
    pub fn min_vec(&self) -> Vector3 {
        Vector3::new(self.min[0], self.min[1], self.min[2])
    }

    /// Obtiene max como Vector3
    pub fn max_vec(&self) -> Vector3 {
        Vector3::new(self.max[0], self.max[1], self.max[2])
    }
}

/// Resultado del análisis de una malla
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshAnalysis {
    /// Volumen de la malla (mm³)
    /// Solo válido para mallas cerradas
    pub volume: Real,

    /// Área superficial (mm²)
    pub surface_area: Real,

    /// Centro de masa [x, y, z]
    pub center_of_mass: [Real; 3],

    /// Bounding box
    pub bounding_box: BoundingBox,

    /// Número de vértices
    pub vertex_count: usize,

    /// Número de triángulos
    pub triangle_count: usize,

    /// Si la malla está cerrada (watertight)
    pub is_closed: bool,

    /// Peso estimado en gramos (si se especificó densidad)
    pub estimated_weight: Option<Real>,
}

/// Calcula el bounding box de una malla
pub fn compute_bounding_box(mesh: &Mesh) -> BoundingBox {
    let mut bbox = BoundingBox::empty();

    for vertex in &mesh.vertices {
        bbox.expand(vertex.position);
    }

    bbox
}

/// Calcula el área superficial de una malla
///
/// Suma las áreas de todos los triángulos.
pub fn compute_surface_area(mesh: &Mesh) -> Real {
    let mut area = 0.0;

    for &face_edge_idx in &mesh.faces {
        // Obtener los 3 vértices del triángulo
        let e0 = &mesh.edges[face_edge_idx];
        let e1 = &mesh.edges[e0.next];
        let e2 = &mesh.edges[e1.next];

        let v0 = mesh.vertices[e0.vertex].position;
        let v1 = mesh.vertices[e1.vertex].position;
        let v2 = mesh.vertices[e2.vertex].position;

        // Vectores de los lados
        let a = v1 - v0;
        let b = v2 - v0;

        // Área = ||a × b|| / 2
        area += a.cross(&b).length() / 2.0;
    }

    area
}

/// Calcula el volumen de una malla cerrada usando el teorema de la divergencia
///
/// Fórmula: V = Σ (v0 · (v1 × v2)) / 6
///
/// El volumen puede ser negativo si las normales apuntan hacia adentro,
/// por lo que tomamos el valor absoluto.
pub fn compute_volume(mesh: &Mesh) -> Real {
    let mut volume = 0.0;

    for &face_edge_idx in &mesh.faces {
        let e0 = &mesh.edges[face_edge_idx];
        let e1 = &mesh.edges[e0.next];
        let e2 = &mesh.edges[e1.next];

        let v0 = mesh.vertices[e0.vertex].position;
        let v1 = mesh.vertices[e1.vertex].position;
        let v2 = mesh.vertices[e2.vertex].position;

        // Volumen del tetraedro con origen
        volume += v0.dot(&v1.cross(&v2));
    }

    (volume / 6.0).abs()
}

/// Calcula el centro de masa de una malla
///
/// Usa el método volumétrico: suma ponderada de centroides de tetraedros
pub fn compute_center_of_mass(mesh: &Mesh) -> Vector3 {
    let mut com = Vector3::zero();
    let mut total_volume = 0.0;

    for &face_edge_idx in &mesh.faces {
        let e0 = &mesh.edges[face_edge_idx];
        let e1 = &mesh.edges[e0.next];
        let e2 = &mesh.edges[e1.next];

        let v0 = mesh.vertices[e0.vertex].position;
        let v1 = mesh.vertices[e1.vertex].position;
        let v2 = mesh.vertices[e2.vertex].position;

        // Volumen del tetraedro (con signo)
        let vol = v0.dot(&v1.cross(&v2)) / 6.0;

        // Centroide del tetraedro (origen + 3 vértices) / 4
        let centroid = (v0 + v1 + v2) * 0.25;

        com = com + centroid * vol;
        total_volume += vol;
    }

    if total_volume.abs() > 1e-10 {
        com * (1.0 / total_volume)
    } else {
        // Fallback: centro del bounding box
        compute_bounding_box(mesh).center()
    }
}

/// Verifica si una malla está cerrada (watertight)
///
/// Una malla está cerrada si cada arista tiene un gemelo (twin).
pub fn is_mesh_closed(mesh: &Mesh) -> bool {
    // Verificar que cada arista tiene un twin
    for edge in &mesh.edges {
        if edge.twin.is_none() {
            return false;
        }
    }
    true
}

/// Analiza una malla y retorna todas sus propiedades
pub fn analyze(mesh: &Mesh) -> Result<MeshAnalysis> {
    analyze_with_config(mesh, &AnalysisConfig::default())
}

/// Analiza una malla con configuración personalizada
pub fn analyze_with_config(mesh: &Mesh, config: &AnalysisConfig) -> Result<MeshAnalysis> {
    if mesh.vertices.is_empty() {
        return Err(Print3dError::EmptyMesh);
    }

    let bounding_box = compute_bounding_box(mesh);
    let surface_area = compute_surface_area(mesh);
    let is_closed = if config.check_closed {
        is_mesh_closed(mesh)
    } else {
        false
    };

    let volume = if is_closed { compute_volume(mesh) } else { 0.0 };

    let center_of_mass = if config.compute_center_of_mass {
        compute_center_of_mass(mesh)
    } else {
        bounding_box.center()
    };

    // Peso estimado: volumen (mm³) * densidad (g/cm³) / 1000
    let estimated_weight = config
        .material_density
        .map(|density| volume * density / 1000.0);

    Ok(MeshAnalysis {
        volume,
        surface_area,
        center_of_mass: [center_of_mass.x(), center_of_mass.y(), center_of_mass.z()],
        bounding_box,
        vertex_count: mesh.vertices.len(),
        triangle_count: mesh.faces.len(),
        is_closed,
        estimated_weight,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bounding_box_empty() {
        let bbox = BoundingBox::empty();
        assert!(bbox.min[0].is_infinite());
        assert!(bbox.max[0].is_infinite());
    }

    #[test]
    fn test_bounding_box_expand() {
        let mut bbox = BoundingBox::empty();
        bbox.expand(Vector3::new(0.0, 0.0, 0.0));
        bbox.expand(Vector3::new(1.0, 2.0, 3.0));

        assert_eq!(bbox.min, [0.0, 0.0, 0.0]);
        assert_eq!(bbox.max, [1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_bounding_box_dimensions() {
        let bbox = BoundingBox::new([0.0, 0.0, 0.0], [2.0, 3.0, 4.0]);

        assert_eq!(bbox.dimensions(), [2.0, 3.0, 4.0]);
        assert_eq!(bbox.max_dimension(), 4.0);
        assert_eq!(bbox.volume(), 24.0);
    }

    #[test]
    fn test_bounding_box_center() {
        let bbox = BoundingBox::new([0.0, 0.0, 0.0], [2.0, 4.0, 6.0]);
        let center = bbox.center();

        assert!((center.x() - 1.0).abs() < 1e-10);
        assert!((center.y() - 2.0).abs() < 1e-10);
        assert!((center.z() - 3.0).abs() < 1e-10);
    }

    #[test]
    fn test_bounding_box_contains() {
        let bbox = BoundingBox::new([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);

        assert!(bbox.contains(Vector3::new(0.5, 0.5, 0.5)));
        assert!(bbox.contains(Vector3::new(0.0, 0.0, 0.0)));
        assert!(bbox.contains(Vector3::new(1.0, 1.0, 1.0)));
        assert!(!bbox.contains(Vector3::new(1.5, 0.5, 0.5)));
    }
}
