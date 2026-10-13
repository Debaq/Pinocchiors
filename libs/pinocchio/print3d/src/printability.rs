//! Verificación de imprimibilidad: topología, voladizos y paredes finas
//!
//! La malla se espera con Z arriba (la plataforma está en `z = min`).

use std::collections::{HashMap, HashSet};

use pinocchio_math::{Real, Vector3};
use pinocchio_mesh::Mesh;
use pinocchio_spatial::{Bvh, Triangle};
use serde::{Deserialize, Serialize};

use crate::error::{Print3dError, Result};
use crate::geom::{diagonal, normal_area, triangles, weld_ids};
use crate::profile::{check_fit, FitReport, MaterialProfile, PrinterProfile};

/// Gravedad del problema
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Impide imprimir bien
    Error,
    /// Imprime, pero con riesgo o soportes
    Warning,
    /// Dato a tener en cuenta
    Info,
}

/// Un hallazgo de la verificación
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Issue {
    pub severity: Severity,
    /// Identificador estable (`open_edges`, `overhang`, ...)
    pub code: String,
    pub message: String,
}

/// Parámetros de la verificación
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrintabilityConfig {
    /// Voladizo máximo sin soporte desde la vertical (grados)
    pub max_overhang_deg: Real,
    /// Pared mínima imprimible (mm)
    pub min_wall: Real,
    /// Holgura para considerar una cara apoyada en la cama (mm)
    pub bed_tolerance: Real,
    /// Triángulos muestreados para medir el grosor de pared
    pub wall_samples: usize,
    /// Fracción de área en voladizo a partir de la cual se avisa
    pub overhang_warn_fraction: Real,
}

impl Default for PrintabilityConfig {
    fn default() -> Self {
        Self {
            max_overhang_deg: 50.0,
            min_wall: 0.8,
            bed_tolerance: 0.1,
            wall_samples: 1500,
            overhang_warn_fraction: 0.05,
        }
    }
}

impl PrintabilityConfig {
    pub fn from_profiles(printer: &PrinterProfile, material: &MaterialProfile) -> Self {
        Self {
            max_overhang_deg: material.max_overhang_deg,
            min_wall: printer.min_wall(),
            bed_tolerance: printer.layer_height * 0.5,
            ..Default::default()
        }
    }
}

/// Resultado de la verificación
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrintabilityReport {
    /// Aristas con una sola cara (agujeros)
    pub open_edges: usize,
    /// Aristas compartidas por más de dos caras
    pub non_manifold_edges: usize,
    /// Aristas con las dos caras en el mismo sentido (normales inconsistentes)
    pub inconsistent_edges: usize,
    pub degenerate_faces: usize,
    pub duplicate_faces: usize,
    /// Cuerpos sueltos
    pub components: usize,
    /// Cerrada y con volumen negativo: las normales apuntan hacia adentro
    pub inverted: bool,
    pub is_watertight: bool,
    /// Área en voladizo sobre el máximo (mm²)
    pub overhang_area: Real,
    pub overhang_fraction: Real,
    /// Cota superior del volumen de soporte (mm³): columnas hasta la cama
    pub support_volume_max: Real,
    /// Área apoyada en la cama (mm²)
    pub bed_contact_area: Real,
    /// Grosor mínimo medido (mm); `None` si no se pudo medir
    pub min_thickness: Option<Real>,
    pub thin_fraction: Real,
    /// Puntos con pared fina (hasta 64)
    pub thin_points: Vec<[Real; 3]>,
    pub fit: FitReport,
    pub issues: Vec<Issue>,
    /// Sin errores graves
    pub printable: bool,
}

/// Verifica la malla para la impresora y el material dados
pub fn check_printability(mesh: &Mesh, printer: &PrinterProfile, material: &MaterialProfile) -> Result<PrintabilityReport> {
    let mut config = PrintabilityConfig::from_profiles(printer, material);
    config.min_wall = printer.min_wall();
    check_printability_with(mesh, printer, &config)
}

/// Como [`check_printability`] con parámetros explícitos
pub fn check_printability_with(mesh: &Mesh, printer: &PrinterProfile, config: &PrintabilityConfig) -> Result<PrintabilityReport> {
    let (positions, tris) = triangles(mesh);
    if tris.is_empty() {
        return Err(Print3dError::EmptyMesh);
    }
    let diag = diagonal(&positions);
    let welded = weld_ids(&positions, (diag * 1e-7).max(1e-9));

    // ── Topología ──
    let mut degenerate_faces = 0;
    let mut seen_faces: HashSet<[usize; 3]> = HashSet::new();
    let mut duplicate_faces = 0;
    // arista no dirigida → (a→b, b→a)
    let mut edges: HashMap<(usize, usize), (u32, u32)> = HashMap::new();
    let mut parent: Vec<usize> = (0..welded.iter().max().map_or(0, |m| m + 1)).collect();
    fn find(p: &mut [usize], mut i: usize) -> usize {
        while p[i] != i {
            p[i] = p[p[i]];
            i = p[i];
        }
        i
    }
    let mut live: Vec<[usize; 3]> = Vec::with_capacity(tris.len());
    for t in &tris {
        let w = [welded[t[0]], welded[t[1]], welded[t[2]]];
        let area = normal_area(positions[t[0]], positions[t[1]], positions[t[2]]);
        if w[0] == w[1] || w[1] == w[2] || w[0] == w[2] || area.is_none() {
            degenerate_faces += 1;
            continue;
        }
        let mut key = w;
        key.sort_unstable();
        if !seen_faces.insert(key) {
            duplicate_faces += 1;
            continue;
        }
        live.push(*t);
        for i in 0..3 {
            let (a, b) = (w[i], w[(i + 1) % 3]);
            let e = edges.entry((a.min(b), a.max(b))).or_default();
            if a < b {
                e.0 += 1;
            } else {
                e.1 += 1;
            }
        }
        let (ra, rb) = (find(&mut parent, w[0]), find(&mut parent, w[1]));
        parent[ra] = rb;
        let (rb, rc) = (find(&mut parent, w[1]), find(&mut parent, w[2]));
        parent[rb] = rc;
    }
    let (mut open_edges, mut non_manifold_edges, mut inconsistent_edges) = (0, 0, 0);
    for &(f, b) in edges.values() {
        match f + b {
            1 => open_edges += 1,
            2 if f == 2 || b == 2 => inconsistent_edges += 1,
            2 => {}
            _ => non_manifold_edges += 1,
        }
    }
    let components = {
        let mut roots = HashSet::new();
        for t in &live {
            let r = find(&mut parent, welded[t[0]]);
            roots.insert(r);
        }
        roots.len()
    };
    let is_watertight = open_edges == 0 && non_manifold_edges == 0 && inconsistent_edges == 0;

    let mut signed = 0.0;
    for t in &live {
        let (a, b, c) = (positions[t[0]], positions[t[1]], positions[t[2]]);
        signed += a.dot(&b.cross(&c));
    }
    let inverted = is_watertight && signed < 0.0;

    // ── Voladizos ──
    let zmin = positions.iter().map(|p| p.z()).fold(Real::INFINITY, Real::min);
    let sin_max = config.max_overhang_deg.to_radians().sin();
    let (mut total_area, mut overhang_area, mut support_volume_max, mut bed_contact_area) = (0.0, 0.0, 0.0, 0.0);
    for t in &live {
        let (a, b, c) = (positions[t[0]], positions[t[1]], positions[t[2]]);
        let Some((n, area)) = normal_area(a, b, c) else { continue };
        total_area += area;
        let zhi = a.z().max(b.z()).max(c.z());
        let on_bed = zhi <= zmin + config.bed_tolerance;
        if n.z() < -sin_max {
            if on_bed {
                bed_contact_area += area;
            } else {
                overhang_area += area;
                let zc = (a.z() + b.z() + c.z()) / 3.0;
                support_volume_max += area * -n.z() * (zc - zmin);
            }
        }
    }
    let overhang_fraction = if total_area > 0.0 { overhang_area / total_area } else { 0.0 };

    // ── Paredes finas ──
    let (min_thickness, thin_fraction, thin_points) = if is_watertight && !inverted {
        measure_walls(&positions, &live, config, diag)
    } else {
        (None, 0.0, Vec::new())
    };

    // ── Volumen de impresión ──
    let bb = crate::analysis::compute_bounding_box(mesh).dimensions();
    let fit = check_fit(bb, printer.build_volume, 0.0);

    // ── Hallazgos ──
    let mut issues = Vec::new();
    let mut add = |severity, code: &str, message: String| issues.push(Issue { severity, code: code.to_string(), message });
    if open_edges > 0 {
        add(Severity::Error, "open_edges", format!("La malla tiene {open_edges} aristas abiertas (agujeros): no es estanca"));
    }
    if non_manifold_edges > 0 {
        add(Severity::Error, "non_manifold", format!("{non_manifold_edges} aristas compartidas por más de dos caras (non-manifold)"));
    }
    if inconsistent_edges > 0 {
        add(Severity::Error, "inconsistent_normals", format!("{inconsistent_edges} aristas con normales inconsistentes"));
    }
    if inverted {
        add(Severity::Error, "inverted", "Las normales apuntan hacia adentro (volumen negativo)".to_string());
    }
    if degenerate_faces > 0 {
        add(Severity::Warning, "degenerate", format!("{degenerate_faces} triángulos degenerados"));
    }
    if duplicate_faces > 0 {
        add(Severity::Warning, "duplicate", format!("{duplicate_faces} triángulos duplicados"));
    }
    if components > 1 {
        add(Severity::Info, "components", format!("{components} cuerpos separados"));
    }
    if !fit.fits {
        let [ex, ey, ez] = fit.excess;
        add(Severity::Warning, "build_volume", format!("No cabe en la cama de {}: sobra {ex:.1} x {ey:.1} x {ez:.1} mm", printer.name));
    }
    if overhang_fraction > config.overhang_warn_fraction {
        add(
            Severity::Warning,
            "overhang",
            format!("{:.0}% del área está en voladizo (más de {:.0}°): hará falta soporte", overhang_fraction * 100.0, config.max_overhang_deg),
        );
    }
    if thin_fraction > 0.02 {
        add(
            Severity::Warning,
            "thin_walls",
            format!(
                "Paredes más finas que {:.2} mm en el {:.0}% de las muestras (mínimo {:.2} mm)",
                config.min_wall,
                thin_fraction * 100.0,
                min_thickness.unwrap_or(0.0)
            ),
        );
    }
    let printable = !issues.iter().any(|i| i.severity == Severity::Error);

    Ok(PrintabilityReport {
        open_edges,
        non_manifold_edges,
        inconsistent_edges,
        degenerate_faces,
        duplicate_faces,
        components,
        inverted,
        is_watertight,
        overhang_area,
        overhang_fraction,
        support_volume_max,
        bed_contact_area,
        min_thickness,
        thin_fraction,
        thin_points,
        fit,
        issues,
        printable,
    })
}

/// Mide el grosor lanzando rayos hacia adentro desde una muestra de caras;
/// la fracción de pared fina se pondera por área
fn measure_walls(positions: &[Vector3], tris: &[[usize; 3]], config: &PrintabilityConfig, diag: Real) -> (Option<Real>, Real, Vec<[Real; 3]>) {
    let bvh = Bvh::build(tris.iter().map(|t| Triangle::new(positions[t[0]], positions[t[1]], positions[t[2]])).collect());
    let stride = (tris.len() / config.wall_samples.max(1)).max(1);
    let eps = (diag * 1e-5).max(1e-6);
    let (mut samples, mut thin, mut min_t) = (0.0, 0.0, Real::INFINITY);
    let mut points = Vec::new();
    for t in tris.iter().step_by(stride) {
        let (a, b, c) = (positions[t[0]], positions[t[1]], positions[t[2]]);
        let Some((n, area)) = normal_area(a, b, c) else { continue };
        let center = (a + b + c) * (1.0 / 3.0);
        let dir = -n;
        let Some((dist, hit)) = bvh.ray_hit(&(center + dir * eps), &dir, 0.0, Real::INFINITY) else { continue };
        // Desde adentro se sale por una cara cuya normal mira en el sentido del
        // rayo; si no, es un cruce raro (cavidad interna o malla autointersecada)
        if bvh.triangle(hit).normal().dot(&dir) <= 0.0 {
            continue;
        }
        let d = dist + eps;
        samples += area;
        min_t = min_t.min(d);
        if d < config.min_wall {
            thin += area;
            if points.len() < 64 {
                points.push([center.x(), center.y(), center.z()]);
            }
        }
    }
    if samples <= 0.0 {
        return (None, 0.0, points);
    }
    (Some(min_t), thin / samples, points)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Caja [0,dx]x[0,dy]x[0,dz] cerrada, normales hacia afuera
    pub(crate) fn make_box(dx: Real, dy: Real, dz: Real) -> Mesh {
        let p = |x, y, z| Vector3::new(x * dx, y * dy, z * dz);
        let v = vec![p(0., 0., 0.), p(1., 0., 0.), p(1., 1., 0.), p(0., 1., 0.), p(0., 0., 1.), p(1., 0., 1.), p(1., 1., 1.), p(0., 1., 1.)];
        let f = vec![
            [0, 2, 1], [0, 3, 2], // abajo
            [4, 5, 6], [4, 6, 7], // arriba
            [0, 1, 5], [0, 5, 4], // frente
            [1, 2, 6], [1, 6, 5], // derecha
            [2, 3, 7], [2, 7, 6], // fondo
            [3, 0, 4], [3, 4, 7], // izquierda
        ];
        Mesh::from_triangles(&v, &f)
    }

    fn report(mesh: &Mesh) -> PrintabilityReport {
        check_printability(mesh, &PrinterProfile::default(), &MaterialProfile::default()).unwrap()
    }

    #[test]
    fn caja_cerrada_es_imprimible() {
        let r = report(&make_box(20.0, 20.0, 20.0));
        assert!(r.is_watertight && r.printable, "{:?}", r.issues);
        assert_eq!(r.components, 1);
        assert!(!r.inverted);
        assert!(r.overhang_area < 1e-9);
        assert!((r.bed_contact_area - 400.0).abs() < 1e-6);
        assert!(r.fit.fits);
        let t = r.min_thickness.unwrap();
        assert!((t - 20.0).abs() < 0.01, "grosor {t}");
    }

    #[test]
    fn malla_abierta() {
        let b = make_box(10.0, 10.0, 10.0);
        let (pos, mut tris) = triangles(&b);
        tris.pop();
        let m = Mesh::from_triangles(&pos, &tris);
        let r = report(&m);
        assert!(!r.is_watertight && !r.printable);
        assert!(r.open_edges > 0);
        assert!(r.issues.iter().any(|i| i.code == "open_edges"));
    }

    #[test]
    fn normales_invertidas() {
        let b = make_box(10.0, 10.0, 10.0);
        let (pos, tris) = triangles(&b);
        let flipped: Vec<[usize; 3]> = tris.iter().map(|t| [t[0], t[2], t[1]]).collect();
        let r = report(&Mesh::from_triangles(&pos, &flipped));
        assert!(r.inverted && !r.printable);
    }

    #[test]
    fn pared_fina() {
        let r = report(&make_box(50.0, 50.0, 0.3));
        let t = r.min_thickness.unwrap();
        assert!((t - 0.3).abs() < 0.01, "grosor {t}");
        assert!(r.thin_fraction > 0.5);
        assert!(r.issues.iter().any(|i| i.code == "thin_walls"));
    }

    #[test]
    fn no_cabe_en_la_cama() {
        let r = report(&make_box(300.0, 50.0, 50.0));
        assert!(!r.fit.fits);
        assert!(r.issues.iter().any(|i| i.code == "build_volume"));
        assert!(r.printable, "no caber es aviso, no error");
    }

    #[test]
    fn voladizo_de_un_techo() {
        // Una T invertida sobre la cama: la ceja inferior de la T (techo) vuela
        let b = make_box(10.0, 10.0, 10.0);
        let (pos, tris) = triangles(&b);
        // Elevar la mitad superior: la base queda en z=0 con un hueco, simulado
        // elevando toda la caja y añadiendo una pata fina
        let mut pos2: Vec<Vector3> = pos.iter().map(|p| *p + Vector3::new(0.0, 0.0, 10.0)).collect();
        let mut tris2 = tris.clone();
        let leg = make_box(2.0, 2.0, 10.0);
        let (lp, lt) = triangles(&leg);
        let base = pos2.len();
        pos2.extend(lp.iter().map(|p| *p + Vector3::new(4.0, 4.0, 0.0)));
        tris2.extend(lt.iter().map(|t| [t[0] + base, t[1] + base, t[2] + base]));
        let r = report(&Mesh::from_triangles(&pos2, &tris2));
        assert!(r.overhang_area > 90.0, "voladizo {}", r.overhang_area);
        assert!(r.overhang_fraction > 0.05);
        assert!(r.issues.iter().any(|i| i.code == "overhang"));
        assert!(r.bed_contact_area > 0.0);
    }
}
