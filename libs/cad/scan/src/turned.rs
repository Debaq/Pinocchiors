//! Piezas torneadas y recubrimiento desde cortes del escaneo.
//!
//! - [`add_turned`]: el corte por un plano que contiene el eje, la mitad del
//!   perfil a un lado del eje y su revolución (el espéculo, un jarrón, una polea);
//! - [`add_skin`]: cortes paralelos perpendiculares a una dirección y el
//!   recubrimiento que pasa por todos (cuerpos que no son de revolución).

use cad_model::geom::{P2, P3, Plane, cross, dot, normalize, scale};
use cad_model::{AxisSpec, BodyOp, Document, FeatureId, FeatureKind, Loft, LoftSection, PlaneSpec, RegionSelection, Revolve};

use crate::outline::{OutlineOptions, outline_sketch, slice};
use crate::{ScanMesh, perpendicular, regions_area};

/// Recorta el polígono a x ≥ 0 (Sutherland–Hodgman contra el semiplano)
fn clip_to_positive_x(poly: &[P2]) -> Vec<P2> {
    let mut out = Vec::new();
    for k in 0..poly.len() {
        let (a, b) = (poly[k], poly[(k + 1) % poly.len()]);
        let (ina, inb) = (a[0] >= 0.0, b[0] >= 0.0);
        if ina {
            out.push(a);
        }
        if ina != inb {
            let t = a[0] / (a[0] - b[0]);
            out.push([0.0, a[1] + (b[1] - a[1]) * t]);
        }
    }
    out.dedup();
    out
}

/// Sketch sobre `plane` con los lazos (sin reconocer círculos: son perfiles) y la operación que lo usa
fn add_sketch(doc: &mut Document, plane: Plane, loops: &[Vec<P2>], opts: &OutlineOptions, name: &str) -> FeatureId {
    let opts = OutlineOptions { detect_circles: false, ..*opts };
    let id = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Custom { plane }, offset: 0.0, sketch: outline_sketch(loops, &opts) });
    doc.get_mut(id).unwrap().name = name.to_string();
    id
}

/// Revolución del perfil que muestra el escaneo cortado por un plano que
/// contiene el eje (`origin`, `direction`): crea el sketch con la mitad del
/// perfil y la revolución de 360°. Devuelve la revolución.
pub fn add_turned(doc: &mut Document, mesh: &ScanMesh, origin: P3, direction: P3, opts: &OutlineOptions, op: BodyOp) -> Result<FeatureId, String> {
    let dir = normalize(direction);
    let radial = perpendicular(dir);
    // y del plano = el eje; x = hacia afuera
    let plane = Plane { origin, normal: cross(radial, dir), x_dir: radial };
    let loops: Vec<Vec<P2>> = slice(mesh, &plane)
        .into_iter()
        .filter(|s| s.closed && s.points.len() >= 3)
        .map(|s| clip_to_positive_x(&s.points))
        .filter(|l| l.len() >= 3 && regions_area(l) > mesh.diagonal() * mesh.diagonal() * 1e-7)
        .collect();
    if loops.is_empty() {
        return Err("El plano que pasa por el eje no corta el escaneo en un perfil cerrado: revisar el eje".into());
    }
    let sketch = add_sketch(doc, plane, &loops, opts, "Perfil torneado");
    let id = doc.add(FeatureKind::Revolve(Revolve {
        sketch,
        regions: RegionSelection::All,
        axis: AxisSpec::Custom { origin, direction: dir },
        angle: 360.0,
        op,
        extent: Default::default(),
        thin: None,
    }));
    doc.get_mut(id).unwrap().name = "Revolución del escaneo".to_string();
    Ok(id)
}

/// `count` cortes paralelos, perpendiculares a `direction`, repartidos a lo
/// largo del escaneo, y el recubrimiento que pasa por todos (el contorno
/// exterior de cada corte). Devuelve el recubrimiento.
pub fn add_skin(doc: &mut Document, mesh: &ScanMesh, direction: P3, count: usize, opts: &OutlineOptions, ruled: bool, op: BodyOp) -> Result<FeatureId, String> {
    if count < 2 {
        return Err("Hacen falta al menos dos cortes".into());
    }
    let dir = normalize(direction);
    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
    for v in &mesh.vertices {
        let t = dot(*v, dir);
        lo = lo.min(t);
        hi = hi.max(t);
    }
    // Sin tocar las puntas: ahí el corte es un punto
    let (from, to) = (lo + (hi - lo) * 0.02, hi - (hi - lo) * 0.02);
    let x = perpendicular(dir);
    let mut sections = Vec::new();
    for k in 0..count {
        let t = from + (to - from) * k as f64 / (count - 1) as f64;
        let plane = Plane { origin: scale(dir, t), normal: dir, x_dir: x };
        let best = slice(mesh, &plane)
            .into_iter()
            .filter(|s| s.closed && s.points.len() >= 3)
            .max_by(|a, b| regions_area(&a.points).total_cmp(&regions_area(&b.points)));
        let Some(best) = best else { continue };
        let sk = add_sketch(doc, plane, &[best.points], opts, &format!("Corte {}", k + 1));
        sections.push(LoftSection { sketch: sk, regions: RegionSelection::All });
    }
    if sections.len() < 2 {
        return Err("El escaneo no se corta en contornos cerrados en esa dirección".into());
    }
    let id = doc.add(FeatureKind::Loft(Loft {
        sections,
        ruled,
        op,
        start_point: None,
        end_point: None,
        closed: false,
        thin: None,
        path: None,
        guide: None,
        start_tangent: None,
        end_tangent: None,
    }));
    doc.get_mut(id).unwrap().name = "Recubrimiento de cortes".to_string();
    Ok(id)
}
