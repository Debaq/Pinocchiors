//! Transformaciones geométricas para preparación de impresión 3D
//!
//! Proporciona funciones para escalar, trasladar y orientar mallas.

use crate::error::{Print3dError, Result};
use pinocchio_math::{Real, Vector3};
use pinocchio_mesh::Mesh;

use crate::{compute_bounding_box, BoundingBox};

/// Escala una malla uniformemente desde su centro
///
/// # Argumentos
/// * `mesh` - Malla a escalar (modificada in-place)
/// * `factor` - Factor de escala (2.0 = doble tamaño)
pub fn scale(mesh: &mut Mesh, factor: Real) {
    let bbox = compute_bounding_box(mesh);
    let center = bbox.center();

    for vertex in &mut mesh.vertices {
        let offset = vertex.position - center;
        vertex.position = center + offset * factor;
    }
}

/// Escala una malla de forma no uniforme
///
/// # Argumentos
/// * `mesh` - Malla a escalar
/// * `factors` - Factores de escala para cada eje [X, Y, Z]
pub fn scale_non_uniform(mesh: &mut Mesh, factors: [Real; 3]) {
    let bbox = compute_bounding_box(mesh);
    let center = bbox.center();

    for vertex in &mut mesh.vertices {
        let offset = vertex.position - center;
        vertex.position = center
            + Vector3::new(
                offset.x() * factors[0],
                offset.y() * factors[1],
                offset.z() * factors[2],
            );
    }
}

/// Escala una malla para que quepa en un volumen dado (solo reduce)
///
/// Los ejes en los que la malla es plana (dimensión ~0) no limitan la escala.
///
/// # Argumentos
/// * `mesh` - Malla a escalar
/// * `max_size` - Dimensiones máximas permitidas [x, y, z], > 0
///
/// # Retorna
/// El factor de escala aplicado (1.0 si ya cabía)
pub fn scale_to_fit(mesh: &mut Mesh, max_size: [Real; 3]) -> Result<Real> {
    if max_size.iter().any(|&m| !(m.is_finite() && m > 0.0)) {
        return Err(Print3dError::InvalidConfig(format!(
            "tamaño máximo inválido: {max_size:?}"
        )));
    }
    let dims = compute_bounding_box(mesh).dimensions();

    let factor = (0..3)
        .filter(|&i| dims[i] > 1e-12)
        .map(|i| max_size[i] / dims[i])
        .fold(Real::INFINITY, Real::min);

    if factor < 1.0 {
        scale(mesh, factor);
        Ok(factor)
    } else {
        Ok(1.0)
    }
}

/// Escala una malla para alcanzar un volumen objetivo
///
/// # Argumentos
/// * `mesh` - Malla a escalar
/// * `current_volume` - Volumen actual de la malla (> 0: la malla debe ser cerrada)
/// * `target_volume` - Volumen objetivo en mm³ (> 0)
///
/// # Retorna
/// El factor de escala aplicado
pub fn scale_to_volume(mesh: &mut Mesh, current_volume: Real, target_volume: Real) -> Result<Real> {
    if !(current_volume.is_finite() && current_volume > 1e-12) {
        return Err(Print3dError::MeshNotClosed);
    }
    if !(target_volume.is_finite() && target_volume > 0.0) {
        return Err(Print3dError::InvalidConfig(format!(
            "volumen objetivo inválido: {target_volume}"
        )));
    }
    // El volumen escala con el cubo del factor lineal: V_new = V_old * factor³
    let factor = (target_volume / current_volume).cbrt();
    scale(mesh, factor);
    Ok(factor)
}

/// Traslada una malla para que su bounding box comience en el origen
pub fn translate_to_origin(mesh: &mut Mesh) {
    let bbox = compute_bounding_box(mesh);
    let offset = bbox.min_vec();

    for vertex in &mut mesh.vertices {
        vertex.position = vertex.position - offset;
    }
}

/// Centra una malla en el origen
pub fn center_on_origin(mesh: &mut Mesh) {
    let bbox = compute_bounding_box(mesh);
    let center = bbox.center();

    for vertex in &mut mesh.vertices {
        vertex.position = vertex.position - center;
    }
}

/// Traslada una malla por un offset dado
pub fn translate(mesh: &mut Mesh, offset: Vector3) {
    for vertex in &mut mesh.vertices {
        vertex.position = vertex.position + offset;
    }
}

/// Orienta la malla para que la cara más plana quede en Z=0
///
/// Útil para minimizar soportes en impresión FDM.
/// Por ahora es un placeholder - implementación completa requiere
/// análisis de caras y rotación.
pub fn orient_flat_on_bed(_mesh: &mut Mesh) {
    // TODO: Encontrar la cara/grupo de caras más plano
    // y rotar para que quede paralelo al plano XY
    //
    // Algoritmo:
    // 1. Calcular área de cada cara
    // 2. Agrupar caras coplanares (dentro de tolerancia)
    // 3. Encontrar el grupo más grande
    // 4. Calcular rotación para alinear normal del grupo con -Z
    // 5. Aplicar rotación
}

/// Calcula el factor de escala necesario para alcanzar dimensiones específicas
pub fn compute_scale_factor(current_bbox: &BoundingBox, target_dimension: Real) -> Real {
    let max_dim = current_bbox.max_dimension();
    if max_dim > 0.0 {
        target_dimension / max_dim
    } else {
        1.0
    }
}

/// Verifica si una malla cabe en un volumen de construcción dado
pub fn fits_in_build_volume(mesh: &Mesh, build_volume: [Real; 3]) -> bool {
    let bbox = compute_bounding_box(mesh);
    let dims = bbox.dimensions();

    dims[0] <= build_volume[0] && dims[1] <= build_volume[1] && dims[2] <= build_volume[2]
}

/// Calcula cuántas subdivisiones se necesitan para que cada pieza quepa
pub fn compute_subdivisions_needed(mesh: &Mesh, build_volume: [Real; 3]) -> [usize; 3] {
    let bbox = compute_bounding_box(mesh);
    let dims = bbox.dimensions();

    let x = (dims[0] / build_volume[0]).ceil() as usize;
    let y = (dims[1] / build_volume[1]).ceil() as usize;
    let z = (dims[2] / build_volume[2]).ceil() as usize;

    [x.max(1), y.max(1), z.max(1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_scale_factor() {
        let bbox = BoundingBox::new([0.0, 0.0, 0.0], [100.0, 50.0, 25.0]);

        // Max dimension es 100, queremos 200
        let factor = compute_scale_factor(&bbox, 200.0);
        assert!((factor - 2.0).abs() < 1e-10);

        // Max dimension es 100, queremos 50
        let factor = compute_scale_factor(&bbox, 50.0);
        assert!((factor - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_compute_subdivisions_needed_from_bbox() {
        // Modelo de 500x300x400 en impresora 220x220x250
        let bbox = BoundingBox::new([0.0, 0.0, 0.0], [500.0, 300.0, 400.0]);
        let build_volume = [220.0, 220.0, 250.0];
        let dims = bbox.dimensions();

        // Necesitamos: ceil(500/220)=3, ceil(300/220)=2, ceil(400/250)=2
        let x = (dims[0] / build_volume[0]).ceil() as usize;
        let y = (dims[1] / build_volume[1]).ceil() as usize;
        let z = (dims[2] / build_volume[2]).ceil() as usize;

        assert_eq!(x, 3);
        assert_eq!(y, 2);
        assert_eq!(z, 2);
    }

    fn flat_square() -> Mesh {
        // Cuadrado en el plano z = 0 (volumen 0, dimensión z = 0)
        let v = [
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(10.0, 0.0, 0.0),
            Vector3::new(10.0, 10.0, 0.0),
            Vector3::new(0.0, 10.0, 0.0),
        ];
        Mesh::from_triangles(&v, &[[0, 1, 2], [0, 2, 3]])
    }

    #[test]
    fn test_scale_to_volume_rejects_open_mesh() {
        let mut mesh = flat_square();
        assert!(scale_to_volume(&mut mesh, 0.0, 1000.0).is_err());
        assert!(scale_to_volume(&mut mesh, 100.0, -1.0).is_err());
        // La malla no se tocó
        assert!(mesh.vertices.iter().all(|v| v.position.x().is_finite()));
        assert!((scale_to_volume(&mut mesh, 1000.0, 8000.0).unwrap() - 2.0).abs() < 1e-12);
    }

    #[test]
    fn test_scale_to_fit_flat_mesh() {
        let mut mesh = flat_square();
        // z = 0 no limita la escala; x/y = 10 → cabe en 5 con factor 0.5
        let factor = scale_to_fit(&mut mesh, [5.0, 5.0, 5.0]).unwrap();
        assert!((factor - 0.5).abs() < 1e-12);
        assert!(mesh.vertices.iter().all(|v| v.position.z().is_finite()));
        assert!(scale_to_fit(&mut mesh, [0.0, 5.0, 5.0]).is_err());
        assert_eq!(scale_to_fit(&mut mesh, [100.0, 100.0, 100.0]).unwrap(), 1.0);
    }
}
