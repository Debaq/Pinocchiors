//! Generación de joints (insertos) para ensamblaje de piezas impresas
//!
//! Soporta varios tipos de joints:
//! - Dowel (clavija cilíndrica)
//! - Dovetail (cola de milano)
//! - Puzzle (forma de puzzle)
//! - Terrace (escalonado)

use pinocchio_math::{Real, Vector3};
use pinocchio_mesh::Mesh;
use serde::{Deserialize, Serialize};

use crate::{FdmTolerances, Print3dError, Result};

/// Tipo de joint
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JointType {
    /// Clavija cilíndrica
    Dowel,
    /// Cola de milano (trapezoidal)
    Dovetail,
    /// Forma de puzzle
    Puzzle,
    /// Escalonado
    Terrace,
    /// Pirámide
    Pyramid,
}

/// Configuración para joint tipo Dowel (clavija)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DowelConfig {
    /// Diámetro de la clavija (mm)
    pub diameter: Real,
    /// Profundidad del agujero/clavija (mm)
    pub depth: Real,
    /// Número de clavijas por cara
    pub count: usize,
    /// Tolerancia (negativo = macho más pequeño)
    pub tolerance: Real,
}

impl Default for DowelConfig {
    fn default() -> Self {
        Self {
            diameter: 6.0,
            depth: 10.0,
            count: 2,
            tolerance: -0.4, // FDM típico
        }
    }
}

impl DowelConfig {
    /// Configuración para impresión de alta calidad
    pub fn high_quality() -> Self {
        Self {
            tolerance: -0.3,
            ..Default::default()
        }
    }

    /// Configuración con tolerancias de FDM
    pub fn with_tolerances(tolerances: &FdmTolerances) -> Self {
        Self {
            tolerance: tolerances.joint_clearance,
            ..Default::default()
        }
    }

    /// Diámetro del agujero (hembra)
    pub fn hole_diameter(&self) -> Real {
        self.diameter
    }

    /// Diámetro de la clavija (macho)
    pub fn pin_diameter(&self) -> Real {
        self.diameter + self.tolerance
    }
}

/// Configuración para joint tipo Dovetail (cola de milano)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DovetailConfig {
    /// Ancho en la base (mm)
    pub width_base: Real,
    /// Ancho en la punta (mm) - menor que base para trapecio
    pub width_top: Real,
    /// Altura del dovetail (mm)
    pub height: Real,
    /// Profundidad (mm)
    pub depth: Real,
    /// Ángulo del trapecio (grados)
    pub angle: Real,
    /// Tolerancia
    pub tolerance: Real,
}

impl Default for DovetailConfig {
    fn default() -> Self {
        Self {
            width_base: 15.0,
            width_top: 10.0,
            height: 8.0,
            depth: 12.0,
            angle: 10.0,
            tolerance: -0.4,
        }
    }
}

/// Configuración para joint tipo Puzzle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PuzzleConfig {
    /// Tamaño del nub (mm)
    pub nub_size: Real,
    /// Radio del nub (mm)
    pub nub_radius: Real,
    /// Profundidad (mm)
    pub depth: Real,
    /// Tolerancia
    pub tolerance: Real,
}

impl Default for PuzzleConfig {
    fn default() -> Self {
        Self {
            nub_size: 10.0,
            nub_radius: 5.0,
            depth: 8.0,
            tolerance: -0.4,
        }
    }
}

/// Configuración para joint tipo Terrace (escalonado)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerraceConfig {
    /// Número de escalones
    pub steps: usize,
    /// Altura por escalón (mm)
    pub step_height: Real,
    /// Ancho por escalón (mm)
    pub step_width: Real,
    /// Tolerancia
    pub tolerance: Real,
}

impl Default for TerraceConfig {
    fn default() -> Self {
        Self {
            steps: 3,
            step_height: 3.0,
            step_width: 5.0,
            tolerance: -0.4,
        }
    }
}

/// Posición de un joint en una cara
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JointPlacement {
    /// Posición en la cara [x, y, z]
    pub position: [Real; 3],
    /// Normal de la cara [x, y, z]
    pub normal: [Real; 3],
    /// Si es macho (true) o hembra (false)
    pub is_male: bool,
}

impl JointPlacement {
    /// Obtiene posición como Vector3
    pub fn position_vec(&self) -> Vector3 {
        Vector3::new(self.position[0], self.position[1], self.position[2])
    }

    /// Obtiene normal como Vector3
    pub fn normal_vec(&self) -> Vector3 {
        Vector3::new(self.normal[0], self.normal[1], self.normal[2])
    }
}

/// Genera la geometría de un cilindro (para dowel)
///
/// # Argumentos
/// * `center` - Centro de la base del cilindro
/// * `direction` - Dirección del eje del cilindro (normalizada)
/// * `radius` - Radio del cilindro
/// * `height` - Altura del cilindro
/// * `segments` - Número de segmentos para aproximar el círculo
pub fn generate_cylinder_geometry(
    center: Vector3,
    direction: Vector3,
    radius: Real,
    height: Real,
    segments: usize,
) -> Result<Mesh> {
    if segments < 3 {
        return Err(Print3dError::GeometryError(
            "El cilindro necesita al menos 3 segmentos".to_string(),
        ));
    }

    if radius <= 0.0 || height <= 0.0 {
        return Err(Print3dError::GeometryError(
            "Radio y altura deben ser positivos".to_string(),
        ));
    }

    // Normalizar la dirección
    let dir = direction.normalize();

    // Crear un sistema de coordenadas ortogonal
    // Encontrar un vector que no sea paralelo a dir
    let up = if dir.x().abs() < 0.9 {
        Vector3::unit_x()
    } else {
        Vector3::unit_y()
    };

    // Ejes locales perpendiculares a dir
    let u = dir.cross(&up).normalize();
    let v = dir.cross(&u);

    let mut positions = Vec::with_capacity(2 * segments + 2);
    let mut indices = Vec::new();

    // Centro de la base inferior
    let base_center_idx = 0;
    positions.push(center);

    // Centro de la base superior
    let top_center_idx = 1;
    positions.push(center + dir * height);

    // Vértices del círculo inferior (índices 2 a 2+segments-1)
    for i in 0..segments {
        let angle = 2.0 * std::f64::consts::PI * (i as Real) / (segments as Real);
        let x = angle.cos();
        let y = angle.sin();
        let point = center + u * (radius * x) + v * (radius * y);
        positions.push(point);
    }

    // Vértices del círculo superior (índices 2+segments a 2+2*segments-1)
    for i in 0..segments {
        let angle = 2.0 * std::f64::consts::PI * (i as Real) / (segments as Real);
        let x = angle.cos();
        let y = angle.sin();
        let point = center + dir * height + u * (radius * x) + v * (radius * y);
        positions.push(point);
    }

    // Triángulos de la base inferior (fan desde el centro)
    // Las normales deben apuntar hacia abajo (-dir)
    for i in 0..segments {
        let i0 = base_center_idx;
        let i1 = 2 + (i + 1) % segments; // Orden invertido para normal hacia abajo
        let i2 = 2 + i;
        indices.push([i0, i1, i2]);
    }

    // Triángulos de la base superior (fan desde el centro)
    // Las normales deben apuntar hacia arriba (+dir)
    for i in 0..segments {
        let i0 = top_center_idx;
        let i1 = 2 + segments + i;
        let i2 = 2 + segments + (i + 1) % segments;
        indices.push([i0, i1, i2]);
    }

    // Triángulos laterales (conectar círculos inferior y superior)
    for i in 0..segments {
        let b0 = 2 + i;
        let b1 = 2 + (i + 1) % segments;
        let t0 = 2 + segments + i;
        let t1 = 2 + segments + (i + 1) % segments;

        // Dos triángulos por segmento
        indices.push([b0, b1, t1]);
        indices.push([b0, t1, t0]);
    }

    Ok(Mesh::from_triangles(&positions, &indices))
}

/// Genera un dowel (clavija) como malla
pub fn generate_dowel(config: &DowelConfig, placement: &JointPlacement) -> Result<Mesh> {
    let radius = if placement.is_male {
        config.pin_diameter() / 2.0
    } else {
        config.hole_diameter() / 2.0
    };

    generate_cylinder_geometry(
        placement.position_vec(),
        placement.normal_vec(),
        radius,
        config.depth,
        16, // Segmentos
    )
}

/// Genera la geometría de un trapecio 3D (para dovetail)
///
/// Crea un prisma trapezoidal (cola de milano) orientado según el placement.
pub fn generate_dovetail_geometry(config: &DovetailConfig, placement: &JointPlacement) -> Result<Mesh> {
    let position = placement.position_vec();
    let normal = placement.normal_vec().normalize();

    // Sistema de coordenadas local
    let up = if normal.y().abs() < 0.9 {
        Vector3::unit_y()
    } else {
        Vector3::unit_z()
    };
    let u = normal.cross(&up).normalize(); // Ancho
    let v = normal.cross(&u);              // Alto

    // Dimensiones con tolerancia aplicada
    let tolerance = if placement.is_male {
        config.tolerance
    } else {
        0.0
    };

    let half_base = (config.width_base + tolerance) / 2.0;
    let half_top = (config.width_top + tolerance) / 2.0;
    let half_height = (config.height + tolerance) / 2.0;
    let depth = config.depth;

    // 8 vértices del prisma trapezoidal
    // Base (z=0): trapecio con ancho mayor abajo
    // Top (z=depth): mismo trapecio trasladado
    let mut positions = Vec::with_capacity(8);

    // Cara frontal (base del prisma, z=0)
    positions.push(position - u * half_base - v * half_height);         // 0: inferior izq
    positions.push(position + u * half_base - v * half_height);         // 1: inferior der
    positions.push(position + u * half_top + v * half_height);          // 2: superior der
    positions.push(position - u * half_top + v * half_height);          // 3: superior izq

    // Cara posterior (top del prisma, z=depth)
    let offset = normal * depth;
    positions.push(position - u * half_base - v * half_height + offset); // 4: inferior izq
    positions.push(position + u * half_base - v * half_height + offset); // 5: inferior der
    positions.push(position + u * half_top + v * half_height + offset);  // 6: superior der
    positions.push(position - u * half_top + v * half_height + offset);  // 7: superior izq

    let mut indices = Vec::new();

    // Cara frontal (2 triángulos)
    indices.push([0, 2, 1]);
    indices.push([0, 3, 2]);

    // Cara posterior (2 triángulos)
    indices.push([4, 5, 6]);
    indices.push([4, 6, 7]);

    // Cara inferior (2 triángulos)
    indices.push([0, 1, 5]);
    indices.push([0, 5, 4]);

    // Cara superior (2 triángulos)
    indices.push([3, 6, 2]);
    indices.push([3, 7, 6]);

    // Cara izquierda (2 triángulos)
    indices.push([0, 4, 7]);
    indices.push([0, 7, 3]);

    // Cara derecha (2 triángulos)
    indices.push([1, 2, 6]);
    indices.push([1, 6, 5]);

    Ok(Mesh::from_triangles(&positions, &indices))
}

/// Calcula posiciones óptimas para joints en una cara rectangular
///
/// Distribuye joints uniformemente en la cara según el número solicitado.
/// - 1 joint: centro
/// - 2 joints: 1/3 y 2/3 del ancho
/// - 3 joints: centro y extremos
/// - 4 joints: esquinas con margen
/// - n joints: distribución en grilla
pub fn calculate_joint_positions(
    face_center: Vector3,
    face_normal: Vector3,
    face_width: Real,
    face_height: Real,
    count: usize,
) -> Vec<JointPlacement> {
    if count == 0 {
        return Vec::new();
    }

    let normal = face_normal.normalize();

    // Sistema de coordenadas local de la cara
    let up = if normal.y().abs() < 0.9 {
        Vector3::unit_y()
    } else {
        Vector3::unit_z()
    };
    let u = normal.cross(&up).normalize(); // Eje del ancho
    let v = normal.cross(&u);               // Eje del alto

    // Margen desde los bordes (15% de la dimensión menor)
    let margin = (face_width.min(face_height) * 0.15).max(5.0);
    let usable_width = (face_width - 2.0 * margin).max(0.0);
    let usable_height = (face_height - 2.0 * margin).max(0.0);

    let mut placements = Vec::with_capacity(count);

    match count {
        1 => {
            // Centro
            placements.push(JointPlacement {
                position: [face_center.x(), face_center.y(), face_center.z()],
                normal: [normal.x(), normal.y(), normal.z()],
                is_male: true,
            });
        }
        2 => {
            // Distribuidos horizontalmente a 1/3 y 2/3
            for i in 0..2 {
                let t = (i as Real + 0.5) / 2.0 - 0.5; // -0.25 y 0.25
                let pos = face_center + u * (t * usable_width * 2.0);
                placements.push(JointPlacement {
                    position: [pos.x(), pos.y(), pos.z()],
                    normal: [normal.x(), normal.y(), normal.z()],
                    is_male: i == 0,
                });
            }
        }
        3 => {
            // Centro y dos extremos
            let offsets = [-0.4, 0.0, 0.4];
            for (i, &t) in offsets.iter().enumerate() {
                let pos = face_center + u * (t * usable_width);
                placements.push(JointPlacement {
                    position: [pos.x(), pos.y(), pos.z()],
                    normal: [normal.x(), normal.y(), normal.z()],
                    is_male: i % 2 == 0,
                });
            }
        }
        4 => {
            // Esquinas con margen
            let half_w = usable_width / 2.0;
            let half_h = usable_height / 2.0;
            let corners = [
                (-half_w, -half_h),
                (half_w, -half_h),
                (-half_w, half_h),
                (half_w, half_h),
            ];
            for (i, &(dx, dy)) in corners.iter().enumerate() {
                let pos = face_center + u * dx + v * dy;
                placements.push(JointPlacement {
                    position: [pos.x(), pos.y(), pos.z()],
                    normal: [normal.x(), normal.y(), normal.z()],
                    is_male: i % 2 == 0,
                });
            }
        }
        _ => {
            // Distribución en grilla
            let cols = ((count as Real).sqrt().ceil() as usize).max(1);
            let rows = (count + cols - 1) / cols;

            let step_x = if cols > 1 {
                usable_width / (cols - 1) as Real
            } else {
                0.0
            };
            let step_y = if rows > 1 {
                usable_height / (rows - 1) as Real
            } else {
                0.0
            };

            let start_x = -usable_width / 2.0;
            let start_y = -usable_height / 2.0;

            for i in 0..count {
                let col = i % cols;
                let row = i / cols;
                let dx = start_x + col as Real * step_x;
                let dy = start_y + row as Real * step_y;
                let pos = face_center + u * dx + v * dy;
                placements.push(JointPlacement {
                    position: [pos.x(), pos.y(), pos.z()],
                    normal: [normal.x(), normal.y(), normal.z()],
                    is_male: i % 2 == 0,
                });
            }
        }
    }

    placements
}

/// Genera una pirámide truncada (para alineación visual)
///
/// La pirámide tiene base cuadrada y se estrecha hacia la punta.
pub fn generate_pyramid_geometry(
    center: Vector3,
    direction: Vector3,
    base_size: Real,
    top_size: Real,
    height: Real,
) -> Result<Mesh> {
    if base_size <= 0.0 || height <= 0.0 {
        return Err(Print3dError::GeometryError(
            "Tamaño de base y altura deben ser positivos".to_string(),
        ));
    }

    let dir = direction.normalize();

    // Sistema de coordenadas local
    let up = if dir.x().abs() < 0.9 {
        Vector3::unit_x()
    } else {
        Vector3::unit_y()
    };
    let u = dir.cross(&up).normalize();
    let v = dir.cross(&u);

    let half_base = base_size / 2.0;
    let half_top = top_size / 2.0;

    let mut positions = Vec::with_capacity(8);

    // Base cuadrada
    positions.push(center - u * half_base - v * half_base);  // 0
    positions.push(center + u * half_base - v * half_base);  // 1
    positions.push(center + u * half_base + v * half_base);  // 2
    positions.push(center - u * half_base + v * half_base);  // 3

    // Top (puede ser punto si top_size == 0)
    let top_center = center + dir * height;
    if top_size > 0.0 {
        positions.push(top_center - u * half_top - v * half_top);  // 4
        positions.push(top_center + u * half_top - v * half_top);  // 5
        positions.push(top_center + u * half_top + v * half_top);  // 6
        positions.push(top_center - u * half_top + v * half_top);  // 7
    } else {
        // Pirámide con punta
        positions.push(top_center); // 4 = punta
    }

    let mut indices = Vec::new();

    // Base (normal hacia abajo)
    indices.push([0, 2, 1]);
    indices.push([0, 3, 2]);

    if top_size > 0.0 {
        // Pirámide truncada (8 vértices)
        // Top
        indices.push([4, 5, 6]);
        indices.push([4, 6, 7]);

        // Lados
        indices.push([0, 1, 5]);
        indices.push([0, 5, 4]);

        indices.push([1, 2, 6]);
        indices.push([1, 6, 5]);

        indices.push([2, 3, 7]);
        indices.push([2, 7, 6]);

        indices.push([3, 0, 4]);
        indices.push([3, 4, 7]);
    } else {
        // Pirámide con punta (5 vértices)
        indices.push([0, 1, 4]);
        indices.push([1, 2, 4]);
        indices.push([2, 3, 4]);
        indices.push([3, 0, 4]);
    }

    Ok(Mesh::from_triangles(&positions, &indices))
}

/// Configuración para joint tipo pirámide
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PyramidConfig {
    /// Tamaño de la base (mm)
    pub base_size: Real,
    /// Tamaño del top (0 = punta, >0 = truncada)
    pub top_size: Real,
    /// Altura (mm)
    pub height: Real,
    /// Tolerancia
    pub tolerance: Real,
}

impl Default for PyramidConfig {
    fn default() -> Self {
        Self {
            base_size: 10.0,
            top_size: 0.0, // Punta por defecto
            height: 8.0,
            tolerance: -0.4,
        }
    }
}

/// Genera una pirámide como joint
pub fn generate_pyramid(config: &PyramidConfig, placement: &JointPlacement) -> Result<Mesh> {
    let tolerance = if placement.is_male {
        config.tolerance
    } else {
        0.0
    };

    generate_pyramid_geometry(
        placement.position_vec(),
        placement.normal_vec(),
        config.base_size + tolerance,
        config.top_size + tolerance,
        config.height,
    )
}

/// Aplica joints entre dos piezas
///
/// Modifica las mallas in-place, añadiendo/restando geometría de joints.
pub fn apply_joints_between_pieces(
    _piece_a: &mut Mesh,
    _piece_b: &mut Mesh,
    _config: &DowelConfig,
) -> Result<()> {
    // Requiere operaciones booleanas CSG que no están implementadas aún.
    // Se necesitaría integrar una librería como `csgrs` o implementar
    // operaciones booleanas propias.
    //
    // Algoritmo:
    // 1. Detectar la cara de contacto entre las piezas
    // 2. Calcular posiciones de joints
    // 3. Generar geometría de joints
    // 4. Boolean subtract (hembra) o add (macho) a cada pieza

    Err(Print3dError::JointError(
        "apply_joints_between_pieces requiere operaciones CSG (pendiente)".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dowel_config_default() {
        let config = DowelConfig::default();
        assert_eq!(config.diameter, 6.0);
        assert_eq!(config.depth, 10.0);
        assert_eq!(config.tolerance, -0.4);
    }

    #[test]
    fn test_dowel_diameters() {
        let config = DowelConfig {
            diameter: 6.0,
            tolerance: -0.4,
            ..Default::default()
        };

        assert_eq!(config.hole_diameter(), 6.0);
        assert_eq!(config.pin_diameter(), 5.6);
    }

    #[test]
    fn test_dowel_with_tolerances() {
        let tolerances = FdmTolerances::industrial();
        let config = DowelConfig::with_tolerances(&tolerances);

        assert_eq!(config.tolerance, -0.2);
    }

    #[test]
    fn test_dovetail_config_default() {
        let config = DovetailConfig::default();
        assert!(config.width_base > config.width_top); // Trapecio correcto
        assert_eq!(config.angle, 10.0);
    }

    #[test]
    fn test_calculate_joint_positions() {
        let positions = calculate_joint_positions(
            Vector3::zero(),
            Vector3::unit_z(),
            100.0,
            50.0,
            2, // 2 joints
        );

        assert_eq!(positions.len(), 2);
        // Uno macho, uno hembra
        assert!(positions[0].is_male);
        assert!(!positions[1].is_male);
    }

    #[test]
    fn test_generate_cylinder_geometry() {
        let mesh = generate_cylinder_geometry(
            Vector3::zero(),
            Vector3::unit_z(),
            5.0,  // radio
            10.0, // altura
            16,   // segmentos
        )
        .unwrap();

        // 2 centros + 2 * 16 vértices del círculo
        assert_eq!(mesh.num_vertices(), 34);
        // 16 triángulos base + 16 triángulos top + 32 triángulos laterales
        assert_eq!(mesh.num_faces(), 64);
        // Verificar integridad
        assert!(mesh.integrity_check().is_ok());
    }

    #[test]
    fn test_generate_cylinder_min_segments() {
        // Mínimo 3 segmentos
        let result = generate_cylinder_geometry(
            Vector3::zero(),
            Vector3::unit_z(),
            5.0,
            10.0,
            2, // Muy pocos
        );
        assert!(result.is_err());

        // 3 segmentos debería funcionar
        let mesh = generate_cylinder_geometry(
            Vector3::zero(),
            Vector3::unit_z(),
            5.0,
            10.0,
            3,
        )
        .unwrap();
        assert_eq!(mesh.num_vertices(), 8); // 2 centros + 6 vértices
    }

    #[test]
    fn test_generate_dovetail_geometry() {
        let config = DovetailConfig::default();
        let placement = JointPlacement {
            position: [0.0, 0.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            is_male: true,
        };

        let mesh = generate_dovetail_geometry(&config, &placement).unwrap();

        // Prisma trapezoidal: 8 vértices
        assert_eq!(mesh.num_vertices(), 8);
        // 6 caras * 2 triángulos = 12 triángulos
        assert_eq!(mesh.num_faces(), 12);
        assert!(mesh.integrity_check().is_ok());
    }

    #[test]
    fn test_generate_pyramid_geometry() {
        // Pirámide con punta
        let mesh = generate_pyramid_geometry(
            Vector3::zero(),
            Vector3::unit_z(),
            10.0, // base
            0.0,  // punta
            8.0,  // altura
        )
        .unwrap();

        // 4 vértices de base + 1 punta = 5
        assert_eq!(mesh.num_vertices(), 5);
        // 2 triángulos base + 4 triángulos lados = 6
        assert_eq!(mesh.num_faces(), 6);
        assert!(mesh.integrity_check().is_ok());
    }

    #[test]
    fn test_generate_pyramid_truncated() {
        // Pirámide truncada
        let mesh = generate_pyramid_geometry(
            Vector3::zero(),
            Vector3::unit_z(),
            10.0, // base
            5.0,  // top
            8.0,  // altura
        )
        .unwrap();

        // 8 vértices (4 base + 4 top)
        assert_eq!(mesh.num_vertices(), 8);
        // 2 base + 2 top + 8 lados = 12
        assert_eq!(mesh.num_faces(), 12);
        assert!(mesh.integrity_check().is_ok());
    }

    #[test]
    fn test_generate_dowel() {
        let config = DowelConfig::default();
        let placement = JointPlacement {
            position: [10.0, 20.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            is_male: true,
        };

        let mesh = generate_dowel(&config, &placement).unwrap();
        assert!(mesh.num_vertices() > 0);
        assert!(mesh.num_faces() > 0);
        assert!(mesh.integrity_check().is_ok());
    }

    #[test]
    fn test_calculate_joint_positions_single() {
        let positions = calculate_joint_positions(
            Vector3::new(50.0, 50.0, 0.0),
            Vector3::unit_z(),
            100.0,
            100.0,
            1,
        );

        assert_eq!(positions.len(), 1);
        // Debería estar en el centro
        assert!((positions[0].position[0] - 50.0).abs() < 0.01);
        assert!((positions[0].position[1] - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_calculate_joint_positions_four() {
        let positions = calculate_joint_positions(
            Vector3::zero(),
            Vector3::unit_z(),
            100.0,
            100.0,
            4,
        );

        assert_eq!(positions.len(), 4);
        // Todos deberían tener normal Z
        for p in &positions {
            assert!((p.normal[2] - 1.0).abs() < 0.01);
        }
    }

    #[test]
    fn test_pyramid_config_default() {
        let config = PyramidConfig::default();
        assert_eq!(config.base_size, 10.0);
        assert_eq!(config.top_size, 0.0); // Punta
        assert_eq!(config.height, 8.0);
    }
}
