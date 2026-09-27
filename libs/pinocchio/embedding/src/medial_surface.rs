//! Muestreo de la superficie medial

use pinocchio_math::{Real, Rect, Vector3};
use pinocchio_mesh::Mesh;
use pinocchio_spatial::DistanceField;

/// Esfera del eje medial
#[derive(Debug, Clone)]
pub struct MedialSphere {
    /// Centro de la esfera
    pub center: Vector3,
    /// Radio de la esfera
    pub radius: Real,
}

impl MedialSphere {
    /// Crea una nueva esfera medial
    pub fn new(center: Vector3, radius: Real) -> Self {
        Self { center, radius }
    }
}

/// Muestrea la superficie medial de una malla
///
/// La superficie medial es el conjunto de centros de esferas máximas
/// inscritas en la malla.
pub fn sample_medial_surface(
    mesh: &Mesh,
    num_samples: usize,
    distance_field: Option<&DistanceField>,
) -> Vec<MedialSphere> {
    let bbox = mesh.bounding_box();
    let mut spheres = Vec::new();

    // Crear campo de distancias si no se proporciona
    let default_resolution = [32, 32, 32];
    let owned_field;
    let field = match distance_field {
        Some(f) => f,
        None => {
            owned_field = DistanceField::from_mesh(mesh, default_resolution, 0.1);
            &owned_field
        }
    };

    // Muestrear puntos dentro del bounding box
    let samples_per_dim = (num_samples as f64).cbrt().ceil() as usize;

    for iz in 0..samples_per_dim {
        for iy in 0..samples_per_dim {
            for ix in 0..samples_per_dim {
                let t = Vector3::new(
                    (ix as Real + 0.5) / samples_per_dim as Real,
                    (iy as Real + 0.5) / samples_per_dim as Real,
                    (iz as Real + 0.5) / samples_per_dim as Real,
                );

                let pos = bbox.min + Vector3::new(
                    t.x() * bbox.size().x(),
                    t.y() * bbox.size().y(),
                    t.z() * bbox.size().z(),
                );

                let dist = field.sample(&pos);

                // Solo considerar puntos dentro de la malla (distancia positiva o pequeña)
                if dist > 0.0 && dist < bbox.diagonal() * 0.5 {
                    // Verificar si es un máximo local (candidato a eje medial)
                    if is_local_maximum(&pos, dist, field) {
                        spheres.push(MedialSphere::new(pos, dist));
                    }
                }
            }
        }
    }

    // Filtrar esferas redundantes
    filter_spheres(&mut spheres, 0.01);

    spheres
}

/// Extrae esferas mediales directamente de un campo de distancias **con signo**.
///
/// Una celda interior (valor > 0) se considera parte del eje medial si como
/// mucho `MAX_GREATER_NEIGHBORS` de sus 26 vecinas tienen una distancia mayor:
/// en el interior de un tubo, las celdas del eje solo tienen vecinas mayores a
/// lo largo del propio eje (cuando el tubo se ensancha), mientras que las
/// celdas desplazadas del eje tienen toda una cara de vecinas mayores.
pub fn medial_spheres_from_field(field: &DistanceField) -> Vec<MedialSphere> {
    const MAX_GREATER_NEIGHBORS: usize = 3;
    let [rx, ry, rz] = field.resolution();
    let mut spheres = Vec::new();

    for z in 0..rz {
        for y in 0..ry {
            for x in 0..rx {
                let value = field.get(x, y, z);
                if !(value > 0.0 && value.is_finite()) {
                    continue;
                }
                let mut greater = 0;
                for dz in -1i64..=1 {
                    for dy in -1i64..=1 {
                        for dx in -1i64..=1 {
                            if dx == 0 && dy == 0 && dz == 0 {
                                continue;
                            }
                            let (nx, ny, nz) = (x as i64 + dx, y as i64 + dy, z as i64 + dz);
                            if nx < 0 || ny < 0 || nz < 0 || nx >= rx as i64 || ny >= ry as i64 || nz >= rz as i64 {
                                continue;
                            }
                            if field.get(nx as usize, ny as usize, nz as usize) > value {
                                greater += 1;
                            }
                        }
                    }
                }
                if greater <= MAX_GREATER_NEIGHBORS {
                    spheres.push(MedialSphere::new(field.cell_center(x, y, z), value));
                }
            }
        }
    }

    spheres
}

/// Verifica si un punto es un máximo local en el campo de distancias (vecindario de 6)
fn is_local_maximum(pos: &Vector3, dist: Real, field: &DistanceField) -> bool {
    is_local_maximum_with_neighborhood(pos, dist, field, 0.01, false)
}

/// Verifica si un punto es un máximo local usando vecindario expandido de 26
fn is_local_maximum_expanded(pos: &Vector3, dist: Real, field: &DistanceField, step: Real) -> bool {
    is_local_maximum_with_neighborhood(pos, dist, field, step, true)
}

/// Verifica si un punto es un máximo local en el campo de distancias
///
/// # Arguments
/// * `pos` - Posición a verificar
/// * `dist` - Distancia en esa posición
/// * `field` - Campo de distancias
/// * `h` - Paso para verificar vecindario
/// * `expanded` - Si true, usa 26 vecinos; si false, usa 6 vecinos
fn is_local_maximum_with_neighborhood(
    pos: &Vector3,
    dist: Real,
    field: &DistanceField,
    h: Real,
    expanded: bool,
) -> bool {
    if expanded {
        // Vecindario 3x3x3 = 26 vecinos (excluyendo el centro)
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if dx == 0 && dy == 0 && dz == 0 {
                        continue;
                    }
                    let offset = Vector3::new(dx as Real * h, dy as Real * h, dz as Real * h);
                    let neighbor_pos = *pos + offset;
                    let neighbor_dist = field.sample(&neighbor_pos);

                    if neighbor_dist > dist + 1e-6 {
                        return false;
                    }
                }
            }
        }
    } else {
        // Vecindario de 6 (caras del cubo)
        let offsets = [
            Vector3::new(h, 0.0, 0.0),
            Vector3::new(-h, 0.0, 0.0),
            Vector3::new(0.0, h, 0.0),
            Vector3::new(0.0, -h, 0.0),
            Vector3::new(0.0, 0.0, h),
            Vector3::new(0.0, 0.0, -h),
        ];

        for offset in &offsets {
            let neighbor_pos = *pos + *offset;
            let neighbor_dist = field.sample(&neighbor_pos);

            if neighbor_dist > dist + 1e-6 {
                return false;
            }
        }
    }

    true
}

/// Filtra esferas redundantes (demasiado cercanas)
fn filter_spheres(spheres: &mut Vec<MedialSphere>, min_distance_ratio: Real) {
    if spheres.is_empty() {
        return;
    }

    // Ordenar por radio (mayor primero)
    spheres.sort_by(|a, b| b.radius.partial_cmp(&a.radius).unwrap_or(std::cmp::Ordering::Equal));

    let mut keep = vec![true; spheres.len()];

    for i in 0..spheres.len() {
        if !keep[i] {
            continue;
        }

        for j in (i + 1)..spheres.len() {
            if !keep[j] {
                continue;
            }

            let dist = spheres[i].center.distance(&spheres[j].center);
            let min_dist = (spheres[i].radius + spheres[j].radius) * min_distance_ratio;

            if dist < min_dist {
                keep[j] = false;
            }
        }
    }

    let mut idx = 0;
    spheres.retain(|_| {
        let k = keep[idx];
        idx += 1;
        k
    });
}

/// Refina las posiciones de las esferas usando descenso de gradiente
pub fn refine_spheres(
    spheres: &mut [MedialSphere],
    field: &DistanceField,
    num_iterations: usize,
    step_size: Real,
) {
    for sphere in spheres.iter_mut() {
        for _ in 0..num_iterations {
            let gradient = field.gradient(&sphere.center);

            // Mover hacia donde aumenta la distancia
            let new_center = sphere.center + gradient * step_size;
            let new_dist = field.sample(&new_center);

            if new_dist > sphere.radius {
                sphere.center = new_center;
                sphere.radius = new_dist;
            } else {
                break; // Convergió
            }
        }
    }
}

/// Configuración para muestreo adaptativo
#[derive(Debug, Clone)]
pub struct AdaptiveSamplingConfig {
    /// Número base de muestras
    pub base_samples: usize,
    /// Peso de curvatura (0.0 = uniforme, 1.0 = muy adaptativo)
    pub curvature_weight: Real,
    /// Factor de refinamiento en regiones de alta curvatura
    pub refinement_factor: usize,
    /// Umbral de gradiente para considerar alta curvatura
    pub gradient_threshold: Real,
}

impl Default for AdaptiveSamplingConfig {
    fn default() -> Self {
        Self {
            base_samples: 1000,
            curvature_weight: 0.5,
            refinement_factor: 2,
            gradient_threshold: 0.3,
        }
    }
}

/// Muestrea la superficie medial con densidad adaptativa
///
/// Este método realiza:
/// 1. Muestreo inicial uniforme
/// 2. Detección de regiones de alta curvatura (gradiente del campo de distancias)
/// 3. Subdivisión en regiones de alta curvatura
/// 4. Detección de máximos locales con vecindario expandido (26 vecinos)
pub fn sample_medial_surface_adaptive(
    mesh: &Mesh,
    distance_field: &DistanceField,
    config: &AdaptiveSamplingConfig,
) -> Vec<MedialSphere> {
    let bbox = mesh.bounding_box();
    let mut spheres = Vec::new();

    // Fase 1: Muestreo inicial uniforme
    let samples_per_dim = (config.base_samples as f64).cbrt().ceil() as usize;
    let base_step = bbox.diagonal() / samples_per_dim as Real;

    for iz in 0..samples_per_dim {
        for iy in 0..samples_per_dim {
            for ix in 0..samples_per_dim {
                let t = Vector3::new(
                    (ix as Real + 0.5) / samples_per_dim as Real,
                    (iy as Real + 0.5) / samples_per_dim as Real,
                    (iz as Real + 0.5) / samples_per_dim as Real,
                );

                let pos = bbox.min + Vector3::new(
                    t.x() * bbox.size().x(),
                    t.y() * bbox.size().y(),
                    t.z() * bbox.size().z(),
                );

                let dist = distance_field.sample(&pos);

                // Solo considerar puntos dentro de la malla
                if dist > 0.0 && dist < bbox.diagonal() * 0.5 {
                    // Calcular gradiente para determinar curvatura local
                    let gradient = distance_field.gradient(&pos);
                    let gradient_magnitude = gradient.length();

                    // Paso adaptativo: más pequeño donde hay alta curvatura
                    let adaptive_step = if gradient_magnitude > config.gradient_threshold {
                        base_step / config.refinement_factor as Real
                    } else {
                        base_step
                    };

                    // Usar vecindario expandido (26 vecinos) para detección más robusta
                    if is_local_maximum_expanded(&pos, dist, distance_field, adaptive_step * 0.5) {
                        spheres.push(MedialSphere::new(pos, dist));
                    }

                    // Fase 2: Subdivisión en regiones de alta curvatura
                    if gradient_magnitude > config.gradient_threshold && config.curvature_weight > 0.0 {
                        sample_high_curvature_region(
                            &pos,
                            adaptive_step,
                            distance_field,
                            &bbox,
                            config.refinement_factor,
                            &mut spheres,
                        );
                    }
                }
            }
        }
    }

    // Fase 3: Refinar esferas hacia máximos locales
    refine_spheres(&mut spheres, distance_field, 10, base_step * 0.1);

    // Filtrar esferas redundantes con umbral adaptativo
    let avg_radius = if spheres.is_empty() {
        0.01
    } else {
        spheres.iter().map(|s| s.radius).sum::<Real>() / spheres.len() as Real
    };
    filter_spheres(&mut spheres, 0.1 * avg_radius / base_step);

    spheres
}

/// Muestrea una región de alta curvatura con mayor densidad
fn sample_high_curvature_region(
    center: &Vector3,
    step: Real,
    field: &DistanceField,
    bbox: &Rect,
    refinement: usize,
    spheres: &mut Vec<MedialSphere>,
) {
    let sub_step = step / refinement as Real;

    for dx in 0..refinement {
        for dy in 0..refinement {
            for dz in 0..refinement {
                let offset = Vector3::new(
                    (dx as Real - refinement as Real * 0.5) * sub_step,
                    (dy as Real - refinement as Real * 0.5) * sub_step,
                    (dz as Real - refinement as Real * 0.5) * sub_step,
                );

                let pos = *center + offset;

                // Verificar que está dentro del bbox
                if pos.x() < bbox.min.x() || pos.x() > bbox.max.x() ||
                   pos.y() < bbox.min.y() || pos.y() > bbox.max.y() ||
                   pos.z() < bbox.min.z() || pos.z() > bbox.max.z() {
                    continue;
                }

                let dist = field.sample(&pos);

                if dist > 0.0 && dist < bbox.diagonal() * 0.5
                    && is_local_maximum_expanded(&pos, dist, field, sub_step * 0.5) {
                        spheres.push(MedialSphere::new(pos, dist));
                    }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cube_mesh() -> Mesh {
        let positions = vec![
            // Cara frontal
            Vector3::new(-0.5, -0.5, 0.5),
            Vector3::new(0.5, -0.5, 0.5),
            Vector3::new(0.5, 0.5, 0.5),
            Vector3::new(-0.5, 0.5, 0.5),
            // Cara trasera
            Vector3::new(-0.5, -0.5, -0.5),
            Vector3::new(0.5, -0.5, -0.5),
            Vector3::new(0.5, 0.5, -0.5),
            Vector3::new(-0.5, 0.5, -0.5),
        ];

        let indices = vec![
            // Frontal
            [0, 1, 2], [0, 2, 3],
            // Trasera
            [5, 4, 7], [5, 7, 6],
            // Derecha
            [1, 5, 6], [1, 6, 2],
            // Izquierda
            [4, 0, 3], [4, 3, 7],
            // Arriba
            [3, 2, 6], [3, 6, 7],
            // Abajo
            [4, 5, 1], [4, 1, 0],
        ];

        Mesh::from_triangles(&positions, &indices)
    }

    #[test]
    fn test_sample_medial_surface() {
        let mesh = make_cube_mesh();
        let spheres = sample_medial_surface(&mesh, 100, None);

        // Debería encontrar al menos algunas esferas
        // Para un cubo, el eje medial es una cruz 3D
        assert!(!spheres.is_empty());
    }

    #[test]
    fn test_medial_sphere() {
        let sphere = MedialSphere::new(Vector3::zero(), 1.0);
        assert!((sphere.radius - 1.0).abs() < 1e-10);
    }
}
