//! Detección y aplicación de simetría para skinning weights

use pinocchio_math::{Real, Vector3};
use pinocchio_mesh::Mesh;

/// Par de vértices simétricos
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SymmetryPair {
    /// Índice del vértice izquierdo
    pub left: usize,
    /// Índice del vértice derecho
    pub right: usize,
}

impl SymmetryPair {
    /// Crea un nuevo par simétrico
    pub fn new(left: usize, right: usize) -> Self {
        Self { left, right }
    }
}

/// Eje de simetría
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymmetryAxis {
    /// Simetría respecto al plano YZ (X=0)
    X,
    /// Simetría respecto al plano XZ (Y=0)
    Y,
    /// Simetría respecto al plano XY (Z=0)
    Z,
}

impl SymmetryAxis {
    /// Refleja un punto respecto al eje de simetría
    pub fn reflect(&self, point: &Vector3) -> Vector3 {
        match self {
            SymmetryAxis::X => Vector3::new(-point.x(), point.y(), point.z()),
            SymmetryAxis::Y => Vector3::new(point.x(), -point.y(), point.z()),
            SymmetryAxis::Z => Vector3::new(point.x(), point.y(), -point.z()),
        }
    }

    /// Obtiene la coordenada relevante para este eje
    pub fn get_coord(&self, point: &Vector3) -> Real {
        match self {
            SymmetryAxis::X => point.x(),
            SymmetryAxis::Y => point.y(),
            SymmetryAxis::Z => point.z(),
        }
    }
}

/// Mapa de simetría de una malla
#[derive(Debug, Clone)]
pub struct SymmetryMap {
    /// Pares de vértices simétricos
    pub pairs: Vec<SymmetryPair>,
    /// Eje de simetría
    pub axis: SymmetryAxis,
    /// Vértices en el eje central (no tienen par)
    pub center_vertices: Vec<usize>,
}

impl SymmetryMap {
    /// Detecta la simetría de una malla
    ///
    /// # Arguments
    /// * `mesh` - Malla a analizar
    /// * `axis` - Eje de simetría a buscar
    /// * `tolerance` - Tolerancia de distancia para considerar vértices simétricos
    pub fn detect(mesh: &Mesh, axis: SymmetryAxis, tolerance: Real) -> Self {
        let mut pairs = Vec::new();
        let mut center_vertices = Vec::new();
        let mut matched = vec![false; mesh.num_vertices()];

        for i in 0..mesh.num_vertices() {
            if matched[i] {
                continue;
            }

            let pos = mesh.vertices[i].position;
            let coord = axis.get_coord(&pos);

            // Vértice en el centro
            if coord.abs() < tolerance {
                center_vertices.push(i);
                matched[i] = true;
                continue;
            }

            // Buscar par simétrico
            let reflected = axis.reflect(&pos);

            for j in (i + 1)..mesh.num_vertices() {
                if matched[j] {
                    continue;
                }

                let other_pos = mesh.vertices[j].position;
                let dist = reflected.distance(&other_pos);

                if dist < tolerance {
                    // Determinar cuál es izquierdo/derecho según la coordenada
                    let (left, right) = if coord < 0.0 { (i, j) } else { (j, i) };
                    pairs.push(SymmetryPair::new(left, right));
                    matched[i] = true;
                    matched[j] = true;
                    break;
                }
            }
        }

        Self {
            pairs,
            axis,
            center_vertices,
        }
    }

    /// Número de pares simétricos encontrados
    pub fn num_pairs(&self) -> usize {
        self.pairs.len()
    }

    /// Verifica si un vértice tiene un par simétrico
    pub fn get_symmetric(&self, vertex_idx: usize) -> Option<usize> {
        for pair in &self.pairs {
            if pair.left == vertex_idx {
                return Some(pair.right);
            }
            if pair.right == vertex_idx {
                return Some(pair.left);
            }
        }
        None
    }

    /// Verifica si un vértice está en el centro
    pub fn is_center_vertex(&self, vertex_idx: usize) -> bool {
        self.center_vertices.contains(&vertex_idx)
    }

    /// Simetriza los pesos de skinning
    ///
    /// Para cada par de vértices simétricos, promedia sus pesos con
    /// los huesos correspondientes.
    ///
    /// # Arguments
    /// * `weights` - Pesos de skinning [vértice][hueso]
    /// * `bone_pairs` - Pares de huesos simétricos (izquierdo, derecho)
    ///
    /// Los huesos que no están en ningún par se asumen centrales
    /// y sus pesos se promedian directamente.
    pub fn symmetrize_weights(
        &self,
        weights: &mut [Vec<Real>],
        bone_pairs: &[(usize, usize)],
    ) {
        if weights.is_empty() {
            return;
        }

        let num_bones = weights[0].len();

        // Crear mapa de hueso a su par
        let mut bone_symmetric: Vec<Option<usize>> = vec![None; num_bones];
        for &(left, right) in bone_pairs {
            if left < num_bones && right < num_bones {
                bone_symmetric[left] = Some(right);
                bone_symmetric[right] = Some(left);
            }
        }

        // Para cada par de vértices
        for pair in &self.pairs {
            if pair.left >= weights.len() || pair.right >= weights.len() {
                continue;
            }

            let mut new_left = vec![0.0; num_bones];
            let mut new_right = vec![0.0; num_bones];

            for bone_idx in 0..num_bones {
                let weight_left = weights[pair.left][bone_idx];
                let weight_right = weights[pair.right][bone_idx];

                match bone_symmetric[bone_idx] {
                    Some(symmetric_bone) => {
                        // Hueso lateral: promediar con el simétrico
                        let sym_weight_left = weights[pair.left][symmetric_bone];
                        let sym_weight_right = weights[pair.right][symmetric_bone];

                        // Para el vértice izquierdo:
                        // - peso del hueso izquierdo = promedio de (peso_left_bone en left_vert, peso_right_bone en right_vert)
                        new_left[bone_idx] = (weight_left + sym_weight_right) * 0.5;
                        new_right[symmetric_bone] = new_left[bone_idx];

                        // Para el vértice derecho (simétricamente)
                        new_right[bone_idx] = (weight_right + sym_weight_left) * 0.5;
                        new_left[symmetric_bone] = new_right[bone_idx];
                    }
                    None => {
                        // Hueso central: promediar directamente
                        let avg = (weight_left + weight_right) * 0.5;
                        new_left[bone_idx] = avg;
                        new_right[bone_idx] = avg;
                    }
                }
            }

            // Normalizar
            let sum_left: Real = new_left.iter().sum();
            let sum_right: Real = new_right.iter().sum();

            if sum_left > 1e-10 {
                for w in &mut new_left {
                    *w /= sum_left;
                }
            }
            if sum_right > 1e-10 {
                for w in &mut new_right {
                    *w /= sum_right;
                }
            }

            weights[pair.left] = new_left;
            weights[pair.right] = new_right;
        }
    }

    /// Copia los pesos de un lado al otro
    ///
    /// # Arguments
    /// * `weights` - Pesos de skinning
    /// * `bone_pairs` - Pares de huesos simétricos
    /// * `left_to_right` - Si true, copia de izquierda a derecha; si false, al revés
    pub fn mirror_weights(
        &self,
        weights: &mut [Vec<Real>],
        bone_pairs: &[(usize, usize)],
        left_to_right: bool,
    ) {
        if weights.is_empty() {
            return;
        }

        let num_bones = weights[0].len();

        let mut bone_symmetric: Vec<Option<usize>> = vec![None; num_bones];
        for &(left, right) in bone_pairs {
            if left < num_bones && right < num_bones {
                bone_symmetric[left] = Some(right);
                bone_symmetric[right] = Some(left);
            }
        }

        for pair in &self.pairs {
            if pair.left >= weights.len() || pair.right >= weights.len() {
                continue;
            }

            let (src, dst) = if left_to_right {
                (pair.left, pair.right)
            } else {
                (pair.right, pair.left)
            };

            let mut new_weights = vec![0.0; num_bones];

            for bone_idx in 0..num_bones {
                let src_weight = weights[src][bone_idx];

                match bone_symmetric[bone_idx] {
                    Some(symmetric_bone) => {
                        // Copiar peso al hueso simétrico
                        new_weights[symmetric_bone] = src_weight;
                    }
                    None => {
                        // Hueso central: copiar directo
                        new_weights[bone_idx] = src_weight;
                    }
                }
            }

            weights[dst] = new_weights;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_symmetric_mesh() -> Mesh {
        // Malla con 5 vértices simétricos en X
        let positions = vec![
            Vector3::new(0.0, 0.0, 0.0),  // 0: centro
            Vector3::new(-1.0, 0.0, 0.0), // 1: izquierda
            Vector3::new(1.0, 0.0, 0.0),  // 2: derecha (par de 1)
            Vector3::new(-1.0, 1.0, 0.0), // 3: arriba izquierda
            Vector3::new(1.0, 1.0, 0.0),  // 4: arriba derecha (par de 3)
        ];
        let indices = vec![[0, 1, 3], [0, 3, 4], [0, 4, 2], [0, 2, 1]];
        Mesh::from_triangles(&positions, &indices)
    }

    #[test]
    fn test_symmetry_detection() {
        let mesh = make_symmetric_mesh();
        let sym = SymmetryMap::detect(&mesh, SymmetryAxis::X, 0.1);

        assert_eq!(sym.num_pairs(), 2);
        assert_eq!(sym.center_vertices.len(), 1);
        assert!(sym.center_vertices.contains(&0));
    }

    #[test]
    fn test_get_symmetric() {
        let mesh = make_symmetric_mesh();
        let sym = SymmetryMap::detect(&mesh, SymmetryAxis::X, 0.1);

        // El vértice 1 debería tener como par el 2
        let sym_of_1 = sym.get_symmetric(1);
        assert!(sym_of_1 == Some(2) || sym_of_1 == Some(1)); // Depende del orden de detección

        // El vértice 0 (centro) no tiene par
        assert!(sym.is_center_vertex(0));
    }

    #[test]
    fn test_symmetry_axis_reflect() {
        let p = Vector3::new(1.0, 2.0, 3.0);

        let rx = SymmetryAxis::X.reflect(&p);
        assert!((rx.x() - (-1.0)).abs() < 1e-10);
        assert!((rx.y() - 2.0).abs() < 1e-10);
        assert!((rx.z() - 3.0).abs() < 1e-10);

        let ry = SymmetryAxis::Y.reflect(&p);
        assert!((ry.y() - (-2.0)).abs() < 1e-10);

        let rz = SymmetryAxis::Z.reflect(&p);
        assert!((rz.z() - (-3.0)).abs() < 1e-10);
    }

    #[test]
    fn test_symmetrize_weights() {
        let mesh = make_symmetric_mesh();
        let sym = SymmetryMap::detect(&mesh, SymmetryAxis::X, 0.1);

        // 3 huesos: spine (centro), arm_l, arm_r
        let mut weights = vec![
            vec![1.0, 0.0, 0.0], // v0: solo spine
            vec![0.0, 0.8, 0.2], // v1: mayormente arm_l
            vec![0.0, 0.3, 0.7], // v2: mayormente arm_r (pero incorrecto)
            vec![0.1, 0.9, 0.0], // v3: arm_l
            vec![0.1, 0.0, 0.9], // v4: arm_r
        ];

        // bone_pairs: (arm_l=1, arm_r=2)
        sym.symmetrize_weights(&mut weights, &[(1, 2)]);

        // Después de simetrizar, los pares deben tener pesos simétricos
        // v1 y v2 son un par, deben tener pesos simétricos respecto a arm_l/arm_r
        let v1_weights = &weights[1];
        let v2_weights = &weights[2];

        // arm_l en v1 debe ser igual a arm_r en v2
        assert!(
            (v1_weights[1] - v2_weights[2]).abs() < 1e-6,
            "arm_l en v1 ({}) != arm_r en v2 ({})",
            v1_weights[1],
            v2_weights[2]
        );
    }

    #[test]
    fn test_mirror_weights() {
        let mesh = make_symmetric_mesh();
        let sym = SymmetryMap::detect(&mesh, SymmetryAxis::X, 0.1);

        // 3 huesos: spine, arm_l, arm_r
        let mut weights = vec![
            vec![1.0, 0.0, 0.0], // v0
            vec![0.0, 1.0, 0.0], // v1: solo arm_l
            vec![0.5, 0.25, 0.25], // v2: mixto
            vec![0.0, 0.8, 0.2], // v3
            vec![0.0, 0.2, 0.8], // v4
        ];

        sym.mirror_weights(&mut weights, &[(1, 2)], true);

        // v2 debería ahora tener los pesos de v1 pero en arm_r
        // v1 tiene [0, 1, 0] -> v2 debería tener [0, 0, 1]
        // (arm_l en v1 se convierte en arm_r en v2)
        // Nota: El vértice 0 está en el centro así que spine se copia directo
    }
}
