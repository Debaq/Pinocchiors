//! Heat diffusion para cálculo de skinning weights ("bone heat")
//!
//! Implementa el método de Baran & Popović (2007): para cada hueso `b` se
//! resuelve
//!
//! ```text
//! (L + A·H) · w_b = A·H · p_b
//! ```
//!
//! donde `L` es el Laplaciano cotangente, `A` el área asociada a cada vértice,
//! `H_i = k / (c · d_i²)` con `d_i` la distancia del vértice `i` a su hueso
//! visible más cercano, y `p_b(i) = 1/k` si `b` es uno de los `k` huesos
//! visibles más cercanos a `i` (0 si no). Como `L·1 = 0`, los pesos de todos
//! los huesos suman 1 en cada vértice. El sistema es invariante a la escala de
//! la malla.
//!
//! La matriz `L + A·H` es la misma para todos los huesos (solo cambia el lado
//! derecho), así que se factoriza una vez (Cholesky disperso, ver
//! `pinocchio_sparse::SpdFactor`) y cada hueso cuesta dos sustituciones
//! triangulares. Antes se resolvía un CG por hueso (cientos de iteraciones cada
//! uno): 85 s contra 1,4 s con 20 huesos y 82k triángulos en 1 hilo, y los pesos
//! coinciden con los de CG hasta ~1e-11. Si la factorización falla se vuelve a CG.

use pinocchio_math::{Real, Vector3};
use pinocchio_mesh::Mesh;
use pinocchio_skeleton::Skeleton;
use pinocchio_sparse::{SPDMatrix, SpdFactor};
use pinocchio_spatial::{Bvh, Triangle};
use rayon::prelude::*;
use thiserror::Error;

/// Error en heat diffusion
#[derive(Debug, Error)]
pub enum HeatDiffusionError {
    #[error("Error en el solver del sistema lineal")]
    SolverError,
    #[error("Malla vacía")]
    EmptyMesh,
    #[error("Esqueleto vacío")]
    EmptySkeleton,
}

/// Tolerancia relativa para considerar dos huesos igual de cercanos
const TIE_TOLERANCE: Real = 1e-4;

/// Peso de lo que se pasa un vértice de los extremos de un hueso: un hueso
/// que solo toca el tubo con una punta (el arranque del cuello en el tronco)
/// no es el de ese tubo.
const OVERSHOOT_WEIGHT: Real = 1.0;
/// Margen (fracción del segmento) ignorado en los extremos del test de visibilidad
const VISIBILITY_MARGIN: Real = 1e-3;

/// Cómo se resuelve el sistema de cada hueso
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Solver {
    /// Cholesky disperso: se factoriza una vez y cada hueso son dos sustituciones.
    /// Si la factorización falla, cae a CG.
    Direct,
    /// Gradiente conjugado por hueso (el método original); hoy lo usan las pruebas de comparación
    #[cfg_attr(not(test), allow(dead_code))]
    Cg,
}

/// Calculador de heat diffusion para skinning weights
pub struct HeatDiffusion<'a> {
    mesh: &'a Mesh,
    /// Peso de difusión: valores mayores dan pesos más suaves (1.0 = Pinocchio original)
    diffusion_weight: Real,
}

/// Hueso visible más cercano a un vértice
struct NearestBones {
    /// Huesos igual de cercanos (empates)
    bones: Vec<usize>,
    /// Distancia efectiva al más cercano: su distancia relativa al grosor
    /// local, por el grosor mediano del esqueleto (para conservar la escala)
    distance: Real,
}

impl<'a> HeatDiffusion<'a> {
    /// Crea un nuevo calculador de heat diffusion
    pub fn new(mesh: &'a Mesh) -> Self {
        Self {
            mesh,
            diffusion_weight: 1.0,
        }
    }

    /// Establece el peso de difusión (> 0). Valores mayores suavizan los pesos.
    pub fn with_diffusion_weight(mut self, weight: Real) -> Self {
        self.diffusion_weight = weight;
        self
    }

    /// Calcula los skinning weights `[vértice][hueso]` para el esqueleto dado.
    ///
    /// Las posiciones del esqueleto deben estar en el mismo espacio que la malla
    /// (normalmente, el esqueleto ya embebido). El peso del hueso `b` corresponde
    /// al segmento `padre(b) → b`; la raíz no tiene segmento y recibe peso 0.
    pub fn compute_weights<S: Skeleton + Sync>(
        &self,
        skeleton: &S,
    ) -> Result<Vec<Vec<Real>>, HeatDiffusionError> {
        self.compute_weights_with(skeleton, Solver::Direct)
    }

    pub(crate) fn compute_weights_with<S: Skeleton + Sync>(
        &self,
        skeleton: &S,
        solver: Solver,
    ) -> Result<Vec<Vec<Real>>, HeatDiffusionError> {
        let num_vertices = self.mesh.num_vertices();
        let num_bones = skeleton.num_bones();

        if num_vertices == 0 {
            return Err(HeatDiffusionError::EmptyMesh);
        }
        if num_bones == 0 {
            return Err(HeatDiffusionError::EmptySkeleton);
        }

        // Segmentos padre → hijo
        let segments: Vec<(usize, Vector3, Vector3)> = (0..num_bones)
            .filter_map(|b| {
                let bone = skeleton.get_bone(b)?;
                let parent = skeleton.get_bone(bone.parent?)?;
                Some((b, parent.position, bone.position))
            })
            .collect();

        if segments.is_empty() {
            // Esqueleto de un solo hueso: todo va a la raíz
            let mut weights = vec![vec![0.0; num_bones]; num_vertices];
            for w in &mut weights {
                w[0] = 1.0;
            }
            return Ok(weights);
        }

        let nearest = self.find_nearest_visible_bones(&segments);

        // H_i ponderado por el área de cada vértice
        let areas = self.vertex_areas();
        let bbox_diag = self.mesh.bounding_box().diagonal().max(Real::MIN_POSITIVE);
        let min_dist = 1e-6 * bbox_diag;
        let diffusion = self.diffusion_weight.max(1e-12);
        let heat_diag: Vec<Real> = nearest
            .iter()
            .zip(&areas)
            .map(|(n, &area)| {
                let d = n.distance.max(min_dist);
                area * n.bones.len() as Real / (diffusion * d * d)
            })
            .collect();

        let system = self.build_system(&heat_diag)?;

        // Lado derecho de cada hueso (`None` si no toca a ningún vértice)
        let rhs_list: Vec<Option<Vec<Real>>> = (0..num_bones)
            .into_par_iter()
            .map(|bone_idx| {
                let rhs: Vec<Real> = nearest
                    .iter()
                    .zip(&heat_diag)
                    .map(|(n, &h)| {
                        if n.bones.contains(&bone_idx) {
                            h / n.bones.len() as Real
                        } else {
                            0.0
                        }
                    })
                    .collect();
                (!rhs.iter().all(|&v| v == 0.0)).then_some(rhs)
            })
            .collect();

        let bone_weights = match solver {
            Solver::Direct => match system.cholesky_factor() {
                Ok(factor) => Self::solve_direct(&factor, &rhs_list)?,
                Err(_) => Self::solve_cg(&system, &rhs_list)?,
            },
            Solver::Cg => Self::solve_cg(&system, &rhs_list)?,
        };
        // Con muchos huesos son cientos de MB que ya no hacen falta
        drop((system, rhs_list));

        // Transponer a [vértice][hueso], recortar negativos y normalizar
        let mut vertex_weights = vec![vec![0.0; num_bones]; num_vertices];
        for (bone_idx, weights) in bone_weights.iter().enumerate() {
            if let Some(weights) = weights {
                for (vert_idx, &w) in weights.iter().enumerate() {
                    vertex_weights[vert_idx][bone_idx] = w.max(0.0);
                }
            }
        }
        for (weights, n) in vertex_weights.iter_mut().zip(&nearest) {
            let sum: Real = weights.iter().sum();
            if sum > 1e-12 {
                for w in weights.iter_mut() {
                    *w /= sum;
                }
            } else {
                for &b in &n.bones {
                    weights[b] = 1.0 / n.bones.len() as Real;
                }
            }
        }

        Ok(vertex_weights)
    }

    /// Resuelve todos los huesos con una sola factorización
    fn solve_direct(
        factor: &SpdFactor,
        rhs_list: &[Option<Vec<Real>>],
    ) -> Result<Vec<Option<Vec<Real>>>, HeatDiffusionError> {
        let present: Vec<usize> = (0..rhs_list.len()).filter(|&b| rhs_list[b].is_some()).collect();
        let cols: Vec<&[Real]> = present.iter().filter_map(|&b| rhs_list[b].as_deref()).collect();
        let solved = factor.solve_columns(&cols).map_err(|_| HeatDiffusionError::SolverError)?;
        let mut out: Vec<Option<Vec<Real>>> = vec![None; rhs_list.len()];
        for (&b, w) in present.iter().zip(solved) {
            out[b] = Some(w);
        }
        Ok(out)
    }

    /// Resuelve cada hueso con CG, en paralelo (el método original)
    fn solve_cg(
        system: &SPDMatrix,
        rhs_list: &[Option<Vec<Real>>],
    ) -> Result<Vec<Option<Vec<Real>>>, HeatDiffusionError> {
        rhs_list
            .par_iter()
            .map(|rhs| match rhs {
                None => Ok(None),
                Some(rhs) => system.solve_cg(rhs, 5000, 1e-9).map(Some).map_err(|_| HeatDiffusionError::SolverError),
            })
            .collect()
    }

    /// Para cada vértice, el/los hueso(s) visible(s) que pasan por el centro
    /// de su tubo.
    ///
    /// Un hueso es visible si el segmento entre el vértice y el punto más
    /// cercano del hueso no atraviesa la malla. Si ningún hueso es visible, se
    /// usa el más cercano.
    ///
    /// Cada vértice tiene un radio de tubo: la mitad del espesor del cuerpo
    /// medido con un rayo hacia adentro por su normal. Le corresponde el hueso
    /// cuya distancia más se parece a ese radio (el que va por el centro de su
    /// tubo), no el más cercano: en un tronco gordo, la panza tiene tubo de
    /// 0,45 y queda con la columna (a 0,45) aunque la pata esté a 0,2. En un
    /// miembro delgado el hueso más cercano es también el del centro, y en una
    /// articulación los dos huesos están a la misma distancia y empatan. Sin
    /// radio (el rayo no sale), se usa el más cercano.
    fn find_nearest_visible_bones(&self, segments: &[(usize, Vector3, Vector3)]) -> Vec<NearestBones> {
        let triangles: Vec<Triangle> = (0..self.mesh.num_faces())
            .map(|i| {
                let [v0, v1, v2] = self.mesh.get_face_positions(i);
                Triangle::new(v0, v1, v2)
            })
            .collect();
        let bvh = Bvh::build(triangles);
        let tubes = self.tube_radii(&bvh);
        // Radio de tubo mediano: escala de la distancia efectiva para el calor
        let typical = {
            let mut r: Vec<Real> = tubes.iter().flatten().copied().collect();
            r.sort_by(Real::total_cmp);
            r.get(r.len() / 2).copied().unwrap_or(1.0).max(Real::MIN_POSITIVE)
        };

        let result = (0..self.mesh.num_vertices())
            .into_par_iter()
            .map(|vert_idx| {
                let pos = self.mesh.vertices[vert_idx].position;
                let tube = tubes[vert_idx];

                // (puntaje, hueso, punto más cercano, distancia), ordenado por
                // cuánto difiere la distancia del radio del tubo, más cuánto se
                // pasa el vértice de los extremos del hueso
                let mut candidates: Vec<(Real, usize, Vector3, Real)> = segments
                    .iter()
                    .map(|&(bone, a, b)| {
                        let (t, overshoot) = segment_projection(&pos, &a, &b);
                        let closest = a.lerp(&b, t);
                        let d = pos.distance(&closest);
                        let score = tube.map_or(d, |r| (d - r).abs() + OVERSHOOT_WEIGHT * overshoot);
                        (score, bone, closest, d)
                    })
                    .collect();
                candidates.sort_by(|x, y| x.0.total_cmp(&y.0));

                let visible = |c: &(Real, usize, Vector3, Real)| !bvh.segment_intersects(&pos, &c.2, VISIBILITY_MARGIN);

                // Distancia efectiva para el calor: relativa al tubo, así un
                // vértice del tronco se ancla tan fuerte como uno de la pata
                let effective = |d: Real| tube.map_or(d, |r| d / r * typical);
                let Some(first) = candidates.iter().position(visible) else {
                    return NearestBones {
                        bones: vec![candidates[0].1],
                        distance: effective(candidates[0].3),
                    };
                };
                let scale = tube.unwrap_or(candidates[first].3);
                let limit = candidates[first].0 + TIE_TOLERANCE * scale + Real::MIN_POSITIVE;
                let tied: Vec<&(Real, usize, Vector3, Real)> = std::iter::once(&candidates[first])
                    .chain(candidates[first + 1..].iter().take_while(|c| c.0 <= limit).filter(|c| visible(c)))
                    .collect();
                // En un empate el orden lo decide el redondeo: la distancia es
                // la del más cercano de los empatados, no la del primero
                let distance = tied.iter().map(|c| c.3).fold(Real::INFINITY, Real::min);
                NearestBones { bones: tied.iter().map(|c| c.1).collect(), distance: effective(distance) }
            })
            .collect();
        result
    }

    /// Radio del tubo de cada vértice: mitad de la distancia, hacia adentro
    /// por su normal, hasta el otro lado de la malla. `None` si el rayo no
    /// sale (malla abierta) o el vértice no tiene normal.
    fn tube_radii(&self, bvh: &Bvh) -> Vec<Option<Real>> {
        let mut normals = vec![Vector3::zero(); self.mesh.num_vertices()];
        for f in 0..self.mesh.num_faces() {
            let [a, b, c] = self.mesh.get_face_positions(f);
            let n = (b - a).cross(&(c - a));
            for v in self.mesh.get_face_vertices(f) {
                normals[v] += n;
            }
        }
        let reach = self.mesh.bounding_box().diagonal();
        let eps = reach * 1e-6;
        normals
            .into_par_iter()
            .enumerate()
            .map(|(v, n)| {
                let inward = n.try_normalize()? * -1.0;
                let origin = self.mesh.vertices[v].position;
                let hit = bvh.ray_distance(&origin, &inward, eps, reach)?;
                Some(0.5 * hit).filter(|r| *r > eps)
            })
            .collect()
    }

    /// Área asociada a cada vértice (un tercio del área de sus triángulos).
    /// Los vértices sin caras reciben el área media para mantener el sistema SPD.
    fn vertex_areas(&self) -> Vec<Real> {
        let mut areas = vec![0.0; self.mesh.num_vertices()];
        for face_idx in 0..self.mesh.num_faces() {
            let verts = self.mesh.get_face_vertices(face_idx);
            let [p0, p1, p2] = self.mesh.get_face_positions(face_idx);
            let area = (p1 - p0).cross(&(p2 - p0)).length() * 0.5;
            for v in verts {
                areas[v] += area / 3.0;
            }
        }
        let positive: Vec<Real> = areas.iter().copied().filter(|&a| a > 0.0).collect();
        let mean = if positive.is_empty() {
            1.0
        } else {
            positive.iter().sum::<Real>() / positive.len() as Real
        };
        for a in &mut areas {
            if *a <= 0.0 {
                *a = mean;
            }
        }
        areas
    }

    /// Construye el sistema `L + diag(heat)`, con `L_ij = -½(cot α + cot β)` el Laplaciano
    /// cotangente. Los cotangentes negativos se recortan a 0 para que sea una M-matriz.
    ///
    /// Las caras se reparten en tandas entre hilos y las entradas se ordenan y suman
    /// una sola vez (sin una lista por cara ni copias intermedias).
    fn build_system(&self, heat_diag: &[Real]) -> Result<SPDMatrix, HeatDiffusionError> {
        const FACES_PER_TASK: usize = 4096;
        let n = self.mesh.num_vertices();
        let num_faces = self.mesh.num_faces();

        // Cada cara contribuye 12 entradas (4 por cada una de sus 3 aristas)
        let chunks: Vec<Vec<(usize, usize, Real)>> = (0..num_faces.div_ceil(FACES_PER_TASK))
            .into_par_iter()
            .map(|chunk| {
                let (lo, hi) = (chunk * FACES_PER_TASK, ((chunk + 1) * FACES_PER_TASK).min(num_faces));
                let mut out = Vec::with_capacity((hi - lo) * 12);
                for face_idx in lo..hi {
                    let verts = self.mesh.get_face_vertices(face_idx);
                    let positions = self.mesh.get_face_positions(face_idx);
                    for i in 0..3 {
                        let j = (i + 1) % 3;
                        let k = (i + 2) % 3;
                        let (vi, vj) = (verts[i], verts[j]);
                        let w = 0.5 * Self::cotangent_weight_static(&positions[i], &positions[j], &positions[k]);
                        out.push((vi, vi, w));
                        out.push((vj, vj, w));
                        out.push((vi, vj, -w));
                        out.push((vj, vi, -w));
                    }
                }
                out
            })
            .collect();

        let mut entries = Vec::with_capacity(num_faces * 12 + n);
        for chunk in chunks {
            entries.extend(chunk);
        }
        // La diagonal va al final: se suma después de las caras, igual que antes
        entries.extend(heat_diag.iter().enumerate().map(|(i, &h)| (i, i, h)));
        SPDMatrix::from_entries(n, entries).map_err(|_| HeatDiffusionError::SolverError)
    }

    /// Cotangente del ángulo en `pk` opuesto a la arista `pi-pj` (recortado a ≥ 0)
    fn cotangent_weight_static(pi: &Vector3, pj: &Vector3, pk: &Vector3) -> Real {
        let v1 = *pi - *pk;
        let v2 = *pj - *pk;

        let len1 = v1.length();
        let len2 = v2.length();

        if len1 < 1e-10 || len2 < 1e-10 {
            return 0.0;
        }

        let cos_angle = v1.dot(&v2) / (len1 * len2);
        let sin_angle = v1.cross(&v2).length() / (len1 * len2);

        if sin_angle.abs() < 1e-10 {
            0.0
        } else {
            (cos_angle / sin_angle).max(0.0)
        }
    }
}

/// Punto más cercano a `point` sobre el segmento `a-b`
/// Parámetro recortado del punto más cercano y cuánto se pasa la proyección
/// de `point` de los extremos del segmento (en unidades de longitud).
fn segment_projection(point: &Vector3, a: &Vector3, b: &Vector3) -> (Real, Real) {
    let ab = *b - *a;
    let len2 = ab.dot(&ab);
    if len2 < 1e-20 {
        // Hueso de largo nulo: no tiene tubo propio, todo es exceso
        return (0.0, point.distance(a));
    }
    let t = (*point - *a).dot(&ab) / len2;
    let overshoot = (-t).max(t - 1.0).max(0.0) * len2.sqrt();
    (t.clamp(0.0, 1.0), overshoot)
}


#[cfg(test)]
mod tests {
    use super::*;
    use pinocchio_skeleton::{BasicSkeleton, Bone};

    fn make_simple_mesh() -> Mesh {
        let positions = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.5, 1.0, 0.0),
            Vector3::new(0.5, 0.5, 1.0),
        ];
        let indices = vec![[0, 1, 2], [0, 3, 1], [1, 3, 2], [2, 3, 0]];
        Mesh::from_triangles(&positions, &indices)
    }

    /// Cilindro cerrado vertical, y ∈ [0, h]
    fn make_cylinder(r: Real, h: Real, seg: usize, rings: usize) -> Mesh {
        let mut p = vec![];
        let mut t = vec![];
        for j in 0..=rings {
            let y = h * j as Real / rings as Real;
            for i in 0..seg {
                let a = std::f64::consts::TAU * i as Real / seg as Real;
                p.push(Vector3::new(r * a.cos(), y, r * a.sin()));
            }
        }
        for j in 0..rings {
            for i in 0..seg {
                let a = j * seg + i;
                let b = j * seg + (i + 1) % seg;
                t.push([a, a + seg, b]);
                t.push([b, a + seg, b + seg]);
            }
        }
        let bottom = p.len();
        p.push(Vector3::new(0.0, 0.0, 0.0));
        let top = p.len();
        p.push(Vector3::new(0.0, h, 0.0));
        for i in 0..seg {
            let n = (i + 1) % seg;
            t.push([bottom, i, n]);
            t.push([top, rings * seg + n, rings * seg + i]);
        }
        Mesh::from_triangles(&p, &t)
    }

    fn make_chain(points: &[Vector3]) -> BasicSkeleton {
        let mut skel = BasicSkeleton::new();
        skel.add_bone(Bone::new("b0", points[0]));
        for (i, &p) in points.iter().enumerate().skip(1) {
            skel.add_bone(Bone::with_parent(format!("b{i}"), p, i - 1));
        }
        skel
    }

    /// El solver directo (Cholesky) da los mismos pesos que CG, y suman 1
    #[test]
    fn test_direct_solver_matches_cg() {
        let mesh = make_cylinder(0.5, 2.0, 24, 40);
        let skel = make_chain(&[
            Vector3::new(0.0, 0.1, 0.0),
            Vector3::new(0.0, 0.6, 0.0),
            Vector3::new(0.0, 1.1, 0.0),
            Vector3::new(0.0, 1.5, 0.0),
            Vector3::new(0.0, 1.9, 0.0),
        ]);
        let hd = HeatDiffusion::new(&mesh);
        let direct = hd.compute_weights_with(&skel, Solver::Direct).unwrap();
        let cg = hd.compute_weights_with(&skel, Solver::Cg).unwrap();
        assert_eq!(direct.len(), cg.len());
        let mut worst: Real = 0.0;
        for (d, c) in direct.iter().zip(&cg) {
            assert!((d.iter().sum::<Real>() - 1.0).abs() < 1e-9);
            for (a, b) in d.iter().zip(c) {
                worst = worst.max((a - b).abs());
            }
        }
        assert!(worst < 1e-6, "direct y CG difieren en {worst}");
    }

    #[test]
    fn test_heat_diffusion_creation() {
        let mesh = make_simple_mesh();
        let hd = HeatDiffusion::new(&mesh);
        assert!((hd.diffusion_weight - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_segment_projection() {
        let s1 = Vector3::new(0.0, 0.0, 0.0);
        let s2 = Vector3::new(0.0, 1.0, 0.0);
        let d = |p: Vector3| p.distance(&s1.lerp(&s2, segment_projection(&p, &s1, &s2).0));
        assert!((d(Vector3::new(1.0, 0.5, 0.0)) - 1.0).abs() < 1e-10);
        assert!((d(Vector3::new(0.0, 3.0, 0.0)) - 2.0).abs() < 1e-10);
        assert!((d(Vector3::new(0.0, -1.0, 0.0)) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_compute_weights_normalized() {
        let mesh = make_simple_mesh();
        let skeleton = make_chain(&[Vector3::new(0.5, 0.0, 0.3), Vector3::new(0.5, 0.8, 0.3)]);
        let hd = HeatDiffusion::new(&mesh).with_diffusion_weight(0.5);

        let weights = hd.compute_weights(&skeleton).unwrap();
        assert_eq!(weights.len(), 4);
        for vert_weights in &weights {
            let sum: Real = vert_weights.iter().sum();
            assert!((sum - 1.0).abs() < 1e-6, "Suma de pesos = {sum}, esperado 1.0");
            assert!(vert_weights.iter().all(|&w| w >= 0.0));
        }
    }

    #[test]
    fn test_weights_follow_bones() {
        // Cadena de 3 huesos (4 joints) a lo largo de un cilindro vertical
        let mesh = make_cylinder(0.1, 1.0, 16, 30);
        let skeleton = make_chain(&[
            Vector3::new(0.0, 0.05, 0.0),
            Vector3::new(0.0, 0.35, 0.0),
            Vector3::new(0.0, 0.65, 0.0),
            Vector3::new(0.0, 0.95, 0.0),
        ]);
        let weights = HeatDiffusion::new(&mesh).compute_weights(&skeleton).unwrap();

        let dominant = |v: usize| {
            weights[v]
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .map(|(i, _)| i)
                .unwrap()
        };
        for (v, vertex) in mesh.vertices.iter().enumerate() {
            let y = vertex.position.y();
            // El hueso i cubre el segmento joint(i-1) → joint(i)
            let expected = if y < 0.3 { 1 } else if y > 0.7 { 3 } else if (0.4..0.6).contains(&y) { 2 } else { continue };
            assert_eq!(dominant(v), expected, "vértice en y={y:.2}");
        }
        // La raíz no tiene segmento
        assert!(weights.iter().all(|w| w[0] == 0.0));
    }

    #[test]
    fn test_weights_scale_invariant() {
        let chain = [Vector3::new(0.0, 0.05, 0.0), Vector3::new(0.0, 0.5, 0.0), Vector3::new(0.0, 0.95, 0.0)];
        let small = make_cylinder(0.1, 1.0, 12, 20);
        let w_small = HeatDiffusion::new(&small).compute_weights(&make_chain(&chain)).unwrap();

        let k = 170.0;
        let big = make_cylinder(0.1 * k, k, 12, 20);
        let chain_big: Vec<Vector3> = chain.iter().map(|&p| p * k).collect();
        let w_big = HeatDiffusion::new(&big).compute_weights(&make_chain(&chain_big)).unwrap();

        for (a, b) in w_small.iter().zip(&w_big) {
            for (x, y) in a.iter().zip(b) {
                assert!((x - y).abs() < 1e-5, "{x} vs {y}");
            }
        }
    }
}
