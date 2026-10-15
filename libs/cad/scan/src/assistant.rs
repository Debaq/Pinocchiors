//! Asistente de reconstrucción: del escaneo a un árbol de operaciones propuesto.
//!
//! [`propose`] detecta las zonas ([`detect_all`]) y arma pasos —la base (la
//! cara plana más grande extruida hasta el fondo), los agujeros, los tetones,
//! conos y toros— que se aceptan por partes con [`apply`]. Es determinista: con
//! las mismas opciones, los mismos números de paso. Lo que ya está dentro de
//! otro paso (un agujero pasante que es un lazo interior del contorno de la
//! base, el borde redondo de una base circular) no se repite.

use std::collections::HashSet;

use cad_model::geom::{P2, dot, normalize, sub};
use cad_model::{BodyOp, Document, FeatureId, FeatureKind};
use serde::{Deserialize, Serialize};

use crate::outline::fit_circle;
use crate::{DetectOptions, DetectedShape, OutlineOptions, PickOptions, ScanMesh, detect_all, extrude_into, pick_cone, pick_cylinder, pick_plane, pick_torus};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    Base,
    Hole,
    Boss,
    Cone,
    Torus,
    /// Solo informa (un redondeo que se hace con la operación de redondeo)
    Fillet,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    /// Número estable dentro de la propuesta
    pub id: u32,
    pub kind: StepKind,
    pub label: String,
    pub detail: String,
    /// Triángulos del escaneo que explica (para resaltarlos)
    pub faces: Vec<u32>,
    pub area: f64,
    /// Marcado por defecto
    pub on: bool,
    /// Tiene operaciones que crear (los redondeos solo informan)
    pub creates: bool,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Proposal {
    pub steps: Vec<Step>,
    /// Fracción del área del escaneo que explican los pasos que crean algo
    pub covered: f64,
    /// Avisos (no se pudo medir el espesor, no hay cara plana grande…)
    pub notes: Vec<String>,
}

/// Lo que crea un paso (sin el id del sketch de la extrusión, que se completa al agregar)
struct Built {
    step: Step,
    features: Vec<(FeatureKind, String)>,
}

fn build(mesh: &ScanMesh, opts: &PickOptions, outline: &OutlineOptions) -> (Vec<Built>, Vec<String>, HashSet<u32>) {
    let detect = DetectOptions { tolerance: opts.tolerance, ..Default::default() };
    let mut dets = detect_all(mesh, &detect);
    dets.sort_by(|a, b| b.area.total_cmp(&a.area));
    let mut notes = Vec::new();
    let mut out: Vec<Built> = Vec::new();
    let mut taken: HashSet<u32> = HashSet::new();
    let tol = opts.tol(mesh);

    // Base: la cara plana más grande, extruida hacia adentro hasta el fondo
    let mut base_normal = None;
    let mut base_outline: Vec<(P2, f64)> = Vec::new(); // lazos interiores como círculos (centro, radio) en coordenadas del plano
    let mut base_dims = (0.0, 0.0);
    let mut base_plane = None;
    let mut base_depth = 0.0;
    // Lo que la extrusión de la base ya explica: sus paredes y la cara de enfrente
    let mut extruded: HashSet<u32> = HashSet::new();
    if let Some(d) = dets.iter().find(|d| matches!(d.shape, DetectedShape::Plane { .. })) {
        match pick_plane(mesh, d.faces[0], opts) {
            Ok(pick) => match pick.depth(mesh) {
                Some(depth) if depth > tol => {
                    for l in pick.boundary.iter().skip(1) {
                        if let Some((c, r, rms)) = fit_circle(l)
                            && rms <= r * 0.05 + tol
                        {
                            base_outline.push((c, r));
                        }
                    }
                    if let Some(outer) = pick.boundary.first() {
                        let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
                        for p in outer {
                            for k in 0..2 {
                                lo[k] = lo[k].min(p[k]);
                                hi[k] = hi[k].max(p[k]);
                            }
                        }
                        base_dims = (hi[0] - lo[0], hi[1] - lo[1]);
                    }
                    base_normal = Some(pick.plane.normal);
                    base_plane = Some(pick.plane);
                    base_depth = depth;
                    let (n, o) = (pick.plane.normal, pick.plane.origin);
                    for f in 0..mesh.face_count() {
                        let h = -dot(sub(mesh.face_centroids[f], o), n);
                        let facing = dot(mesh.face_normals[f], n);
                        let wall = facing.abs() < 0.05 && h >= -tol && h <= depth + tol;
                        let front = facing < -0.999 && (h - depth).abs() <= tol * 3.0;
                        if wall || front {
                            extruded.insert(f as u32);
                        }
                    }
                    let holes = base_outline.len();
                    taken.extend(pick.faces.iter().copied());
                    out.push(Built {
                        step: Step {
                            id: 0,
                            kind: StepKind::Base,
                            label: format!("Base: contorno extruido {depth:.2} mm"),
                            detail: format!("Cara plana de {:.0} mm² con {} contorno(s); {} agujero(s) redondo(s) ya quedan en el contorno", pick.area, pick.boundary.len(), holes),
                            faces: pick.faces.clone(),
                            area: pick.area,
                            on: true,
                            creates: true,
                        },
                        features: vec![
                            (pick.sketch_feature(outline), "Contorno de la base".into()),
                            // El id del sketch se completa al agregar
                            (extrude_into(FeatureId(u32::MAX), depth, BodyOp::New), "Base".into()),
                        ],
                    });
                }
                _ => notes.push("No se pudo medir el espesor bajo la cara plana más grande: la base se hace a mano (Cara plana → Sketch y extruir)".into()),
            },
            Err(e) => notes.push(format!("No se pudo ajustar la cara plana más grande: {e}")),
        }
    } else {
        notes.push("No hay una cara plana grande para empezar la base".into());
    }

    for d in &dets {
        if d.faces.iter().all(|f| taken.contains(f)) {
            continue;
        }
        match &d.shape {
            DetectedShape::Cylinder { .. } => {
                let Ok(c) = pick_cylinder(mesh, d.faces[0], opts) else { continue };
                if c.faces.iter().filter(|f| taken.contains(f)).count() * 2 > c.faces.len() || c.coverage < 0.3 {
                    continue;
                }
                let along_base = base_normal.is_some_and(|n| dot(normalize(c.direction), n).abs() > 0.999);
                if let (Some(plane), true) = (base_plane, along_base) {
                    let centre = plane.to_local(c.origin);
                    // Un agujero pasante que ya es un lazo interior de la base
                    let through = c.hole && c.length >= base_depth * 0.9 && base_outline.iter().any(|(bc, br)| (bc[0] - centre[0]).hypot(bc[1] - centre[1]) < tol * 4.0 + br * 0.05 && (br - c.radius).abs() < tol * 4.0 + br * 0.05);
                    // El borde redondo de una base circular
                    let rim = !c.hole && c.length >= base_depth * 0.9 && ((2.0 * c.radius - base_dims.0).abs() < base_dims.0 * 0.03 || (2.0 * c.radius - base_dims.1).abs() < base_dims.1 * 0.03);
                    if through || rim {
                        taken.extend(c.faces.iter().copied());
                        continue;
                    }
                }
                taken.extend(c.faces.iter().copied());
                let area: f64 = c.faces.iter().map(|&f| mesh.face_areas[f as usize]).sum();
                let (kind, word) = if c.hole { (StepKind::Hole, "Agujero") } else { (StepKind::Boss, "Tetón") };
                out.push(Built {
                    step: Step {
                        id: 0,
                        kind,
                        label: format!("{word} Ø{:.2} × {:.2} mm", c.radius * 2.0, c.length),
                        detail: format!("Cilindro de radio {:.3} mm, cubre {:.0} % de la vuelta, error {:.3} mm", c.radius, c.coverage * 100.0, c.rms),
                        faces: c.faces.clone(),
                        area,
                        on: true,
                        creates: true,
                    },
                    features: vec![(c.feature(), format!("{word} escaneado"))],
                });
            }
            DetectedShape::Cone { .. } => {
                let Ok(c) = pick_cone(mesh, d.faces[0], opts) else { continue };
                if c.faces.iter().filter(|f| taken.contains(f)).count() * 2 > c.faces.len() {
                    continue;
                }
                taken.extend(c.faces.iter().copied());
                let area: f64 = c.faces.iter().map(|&f| mesh.face_areas[f as usize]).sum();
                out.push(Built {
                    step: Step {
                        id: 0,
                        kind: StepKind::Cone,
                        label: format!("{} semiángulo {:.1}° × {:.2} mm", if c.hole { "Cono hueco" } else { "Cono" }, c.half_angle.to_degrees(), c.end - c.start),
                        detail: format!("Error {:.3} mm", c.rms),
                        faces: c.faces.clone(),
                        area,
                        on: true,
                        creates: true,
                    },
                    features: vec![(c.feature(), "Cono escaneado".into())],
                });
            }
            DetectedShape::Torus { .. } => {
                let Ok(t) = pick_torus(mesh, d.faces[0], opts) else { continue };
                if t.faces.iter().filter(|f| taken.contains(f)).count() * 2 > t.faces.len() {
                    continue;
                }
                taken.extend(t.faces.iter().copied());
                let area: f64 = t.faces.iter().map(|&f| mesh.face_areas[f as usize]).sum();
                out.push(Built {
                    step: Step {
                        id: 0,
                        kind: StepKind::Torus,
                        label: format!("Toro R {:.2} r {:.2} mm", t.major, t.minor),
                        detail: format!("Error {:.3} mm", t.rms),
                        faces: t.faces.clone(),
                        area,
                        on: false,
                        creates: true,
                    },
                    features: vec![(t.feature(), "Toro escaneado".into())],
                });
            }
            DetectedShape::Fillet { radius, angle, direction, .. } => {
                // Un redondeo paralelo a la normal de la base ya viene en el contorno
                let in_outline = base_normal.is_some_and(|n| dot(normalize(*direction), n).abs() > 0.999);
                out.push(Built {
                    step: Step {
                        id: 0,
                        kind: StepKind::Fillet,
                        label: format!("Redondeo r {radius:.2} mm · {angle:.0}°"),
                        detail: if in_outline { "Está en el contorno de la base".into() } else { "Hacerlo con Redondeo sobre las aristas del diseño".into() },
                        faces: d.faces.clone(),
                        area: d.area,
                        on: false,
                        creates: false,
                    },
                    features: vec![],
                });
            }
            _ => {}
        }
    }
    for (i, b) in out.iter_mut().enumerate() {
        b.step.id = i as u32;
    }
    (out, notes, extruded)
}

/// El árbol propuesto para el escaneo.
pub fn propose(mesh: &ScanMesh, opts: &PickOptions, outline: &OutlineOptions) -> Proposal {
    let (built, notes, extruded) = build(mesh, opts, outline);
    let total: f64 = mesh.face_areas.iter().sum();
    let mut covered = 0.0;
    let mut seen: HashSet<u32> = HashSet::new();
    for b in built.iter().filter(|b| b.step.creates) {
        if b.step.kind == StepKind::Base {
            for &f in &extruded {
                if seen.insert(f) {
                    covered += mesh.face_areas[f as usize];
                }
            }
        }
        for &f in &b.step.faces {
            if seen.insert(f) {
                covered += mesh.face_areas[f as usize];
            }
        }
    }
    Proposal { steps: built.into_iter().map(|b| b.step).collect(), covered: if total > 0.0 { covered / total } else { 0.0 }, notes }
}

/// Agrega al diseño las operaciones de los pasos `ids` (en el orden de la
/// propuesta: la base primero). Devuelve cuántas operaciones agregó.
pub fn apply(doc: &mut Document, mesh: &ScanMesh, opts: &PickOptions, outline: &OutlineOptions, ids: &[u32]) -> Result<usize, String> {
    let (built, _, _) = build(mesh, opts, outline);
    let mut added = 0;
    for b in built.into_iter().filter(|b| ids.contains(&b.step.id) && b.step.creates) {
        let mut sketch: Option<FeatureId> = None;
        for (mut kind, name) in b.features {
            let is_sketch = matches!(kind, FeatureKind::Sketch { .. });
            if let (FeatureKind::Extrude(e), Some(s)) = (&mut kind, sketch) {
                e.sketch = s;
            }
            let id = doc.add(kind);
            doc.get_mut(id).unwrap().name = name;
            if is_sketch {
                sketch = Some(id);
            }
            added += 1;
        }
    }
    if added == 0 {
        return Err("Ningún paso elegido crea operaciones".into());
    }
    Ok(added)
}
