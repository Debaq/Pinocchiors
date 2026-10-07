//! Escaneo → CAD.
//!
//! Sobre una malla escaneada ([`ScanMesh`], Z arriba, mm):
//! - [`pick_plane`] / [`pick_cylinder`]: con un clic, crecer la zona y ajustarle
//!   la primitiva (el cilindro sabe si es agujero o tetón);
//! - [`slice`]: cortar con un plano y obtener contornos;
//! - [`outline_sketch`]: contornos → sketch (reconoce círculos, endereza líneas);
//! - [`detect_all`]: todas las zonas planas/cilíndricas/esféricas de una vez;
//! - [`PlanePick::depth`]: profundidad del material bajo una cara (para extruir
//!   "hasta el escaneo");
//! - `*_feature`: convertir lo detectado en operaciones de [`cad_model`].

mod deviation;
mod mesh;
mod outline;
mod pick;

use cad_model::geom::{P2, P3, Plane, add, dot, normalize, scale, sub};
use cad_model::{BodyOp, Extent, Extrude, FeatureId, FeatureKind, PlaneSpec, Primitive, PrimitiveShape, RegionSelection};
use cad_solver::fitting::{self, PrimitiveParams, PrimitiveType};
use serde::{Deserialize, Serialize};

pub use deviation::{Deviation, DeviationStats, deviation};
pub use mesh::ScanMesh;
pub use outline::{OutlineOptions, Section, fit_circle, outline_sketch, simplify_closed, simplify_open, slice};
pub use pick::{CylinderPick, PickError, PickOptions, PlanePick, boundary_loops, pick_cylinder, pick_plane};

pub(crate) fn regions_area(p: &[P2]) -> f64 {
    cad_model::regions::signed_area(p).abs()
}

impl PlanePick {
    /// Espesor del material bajo la zona: mediana de rayos lanzados desde las
    /// caras de la zona hacia adentro. `None` si ningún rayo sale del otro lado.
    pub fn depth(&self, mesh: &ScanMesh) -> Option<f64> {
        let n = self.plane.normal;
        let step = (self.faces.len() / 48).max(1);
        // Partir bajo la banda de ruido de la superficie y contar solo
        // salidas del material
        let back = (self.rms * 4.0).max(mesh.diagonal() * 1e-6);
        let mut hits: Vec<f64> = self
            .faces
            .iter()
            .step_by(step)
            .filter_map(|&f| {
                let c = mesh.face_centroids[f as usize];
                let on = sub(c, scale(n, dot(sub(c, self.plane.origin), n) + back));
                mesh.raycast_exit(on, scale(n, -1.0), 0.0).map(|(t, _)| t + back)
            })
            .collect();
        if hits.is_empty() {
            return None;
        }
        hits.sort_by(f64::total_cmp);
        Some(hits[hits.len() / 2])
    }

    /// Sketch con el contorno de la zona sobre su plano.
    pub fn sketch_feature(&self, opts: &OutlineOptions) -> FeatureKind {
        FeatureKind::Sketch {
            plane: PlaneSpec::Custom { plane: self.plane },
            offset: 0.0,
            sketch: outline_sketch(&self.boundary, opts),
        }
    }
}

/// Extrusión de un sketch de zona hacia adentro del material, `depth` mm.
pub fn extrude_into(sketch: FeatureId, depth: f64, op: BodyOp) -> FeatureKind {
    FeatureKind::Extrude(Extrude {
        sketch,
        regions: RegionSelection::All,
        extent: Extent::Blind { distance: depth },
        reverse: true,
        op,
    draft: 0.0,
    thin: None,
    })
}

impl CylinderPick {
    /// Cilindro equivalente: tetón se une, agujero se resta.
    pub fn feature(&self) -> FeatureKind {
        FeatureKind::Primitive(Primitive {
            shape: PrimitiveShape::Cylinder { radius: self.radius, height: self.length },
            origin: self.origin,
            z: self.direction,
            x: perpendicular(self.direction),
            op: if self.hole { BodyOp::Cut } else { BodyOp::Join },
        })
    }

    pub fn end(&self) -> P3 {
        add(self.origin, scale(self.direction, self.length))
    }
}

fn perpendicular(d: P3) -> P3 {
    let w = if d[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    normalize(sub(w, scale(d, dot(w, d))))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DetectedShape {
    Plane { plane: Plane },
    Cylinder { origin: P3, direction: P3, radius: f64 },
    Sphere { center: P3, radius: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Detection {
    pub shape: DetectedShape,
    /// Triángulos de la zona; el primero es el más grande (semilla para elegirla)
    pub faces: Vec<u32>,
    pub area: f64,
    pub rms: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DetectOptions {
    /// Ángulo entre normales vecinas para agrupar (grados).
    pub angle_threshold: f64,
    /// Zonas más chicas que esta fracción del área total se ignoran.
    pub min_area_fraction: f64,
    pub tolerance: Option<f64>,
}

impl Default for DetectOptions {
    fn default() -> Self {
        Self { angle_threshold: 12.0, min_area_fraction: 0.005, tolerance: None }
    }
}

/// Zonas suaves: crecen de cara a vecina mientras el ángulo entre ambas no
/// pase el umbral (los cilindros quedan enteros; las aristas vivas cortan).
/// Las caras de normal poco confiable se cruzan llevando la normal y un punto
/// de la última cara confiable, y entran si sus vértices caen en esa banda.
fn segment_smooth(mesh: &ScanMesh, angle: f64, tol: f64) -> Vec<Vec<u32>> {
    let cos = angle.to_radians().cos();
    let n = mesh.face_count();
    let bad: Vec<bool> = (0..n).map(|f| pick::unreliable_normal(mesh, f, tol)).collect();
    let mut region = vec![u32::MAX; n];
    let mut order: Vec<usize> = (0..n).filter(|&f| !bad[f]).collect();
    order.sort_by(|&a, &b| mesh.face_areas[b].total_cmp(&mesh.face_areas[a]));
    let mut out: Vec<Vec<u32>> = Vec::new();
    for seed in order {
        if region[seed] != u32::MAX {
            continue;
        }
        let id = out.len() as u32;
        let mut faces = vec![seed as u32];
        region[seed] = id;
        let mut queue = std::collections::VecDeque::from([(seed, mesh.face_normals[seed], mesh.face_centroids[seed])]);
        while let Some((f, rn, rp)) = queue.pop_front() {
            for &g in &mesh.adjacency[f] {
                let gi = g as usize;
                if region[gi] != u32::MAX {
                    continue;
                }
                let next = if bad[gi] {
                    let near = mesh.triangles[gi].iter().all(|&v| dot(sub(mesh.vertices[v as usize], rp), rn).abs() <= tol * 3.0);
                    near.then_some((gi, rn, rp))
                } else if dot(rn, mesh.face_normals[gi]) >= cos {
                    Some((gi, mesh.face_normals[gi], mesh.face_centroids[gi]))
                } else {
                    None
                };
                if let Some(item) = next {
                    region[gi] = id;
                    faces.push(g);
                    queue.push_back(item);
                }
            }
        }
        out.push(faces);
    }
    out
}

fn fit_region(mesh: &ScanMesh, mut faces: Vec<u32>, tol: f64) -> Option<Detection> {
    // La cara más grande primero: es la mejor semilla para volver a elegir la zona
    if let Some(i) = (0..faces.len()).max_by(|&a, &b| mesh.face_areas[faces[a] as usize].total_cmp(&mesh.face_areas[faces[b] as usize])) {
        faces.swap(0, i);
    }
    let area: f64 = faces.iter().map(|&f| mesh.face_areas[f as usize]).sum();
    let (pts, nrm) = mesh.sample_faces(&faces, 6000);
    let fit = fitting::fit_best_with_normals(&pts, Some(&nrm), tol);
    let avg = nrm.iter().fold([0.0; 3], |a, n| add(a, *n));
    let shape = match (fit.primitive_type, fit.parameters) {
        (PrimitiveType::Plane, PrimitiveParams::Plane { point, normal }) => {
            let n = if dot(normal, avg) < 0.0 { scale(normal, -1.0) } else { normal };
            DetectedShape::Plane { plane: Plane::from_normal(point, n) }
        }
        (PrimitiveType::Cylinder, PrimitiveParams::Cylinder { point, axis, radius }) => {
            DetectedShape::Cylinder { origin: point, direction: normalize(axis), radius }
        }
        (PrimitiveType::Sphere, PrimitiveParams::Sphere { center, radius }) => DetectedShape::Sphere { center, radius },
        _ => return None,
    };
    Some(Detection { shape, faces, area, rms: fit.rms_error })
}

/// Segmenta todo el escaneo y ajusta una primitiva a cada zona.
pub fn detect_all(mesh: &ScanMesh, opts: &DetectOptions) -> Vec<Detection> {
    let tol = opts.tolerance.unwrap_or(mesh.diagonal() * 0.002);
    let total: f64 = mesh.face_areas.iter().sum();
    let min_area = total * opts.min_area_fraction;
    let big = |faces: &[u32]| faces.iter().map(|&f| mesh.face_areas[f as usize]).sum::<f64>() >= min_area;
    let mut out = Vec::new();
    for faces in segment_smooth(mesh, opts.angle_threshold, tol) {
        if !big(&faces) {
            continue;
        }
        if let Some(d) = fit_region(mesh, faces.clone(), tol) {
            out.push(d);
            continue;
        }
        // Zona mixta (plano + redondeo + plano…): rescatar sus partes planas
        // agrupando contra la normal semilla.
        let local: std::collections::HashMap<u32, usize> = faces.iter().enumerate().map(|(i, &f)| (f, i)).collect();
        let normals: Vec<P3> = faces.iter().map(|&f| mesh.face_normals[f as usize]).collect();
        let adjacency: Vec<Vec<usize>> = faces
            .iter()
            .map(|&f| mesh.adjacency[f as usize].iter().filter_map(|g| local.get(g).copied()).collect())
            .collect();
        let seg = cad_solver::segmentation::segment_by_normals(&normals, &adjacency, opts.angle_threshold);
        for r in seg.regions {
            let sub: Vec<u32> = r.face_indices.iter().map(|&i| faces[i]).collect();
            if big(&sub)
                && let Some(d) = fit_region(mesh, sub, tol)
            {
                out.push(d);
            }
        }
    }
    out.sort_by(|a, b| b.area.total_cmp(&a.area));
    out
}
