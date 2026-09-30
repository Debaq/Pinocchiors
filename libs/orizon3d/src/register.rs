//! Varias nubes del mismo objeto: alinearlas (registro), fusionarlas y
//! compararlas.
//!
//! - **Alinear**: una búsqueda global prueba muchas orientaciones de la nube
//!   nueva sobre la de referencia (centroides encimados, ICP punto a punto
//!   corto en cada una) y se queda con la que más superficie comparte; después
//!   un ICP punto a plano fino la ajusta. Así un segundo escaneo con el objeto
//!   dado vuelta (para ver la base) cae sobre el primero sin marcar puntos a mano.
//! - **Fusionar**: suma las nubes y, en el solape, promedia por celda para no
//!   dejar dos capas de puntos.
//! - **Comparar**: distancia de cada punto a la nube de referencia más
//!   cercana, con estadísticas y un mapa de color.

use crate::edit;
use crate::pointcloud::{Point, PointCloud};
use crate::scan::{best_fit_transform, downsample_positions, estimate_normals, icp, icp_point_to_plane, Transform, VoxelIndex};

/// Cómo se alinea la nube nueva antes de sumarla
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignMode {
    /// Tal como está (mismo sistema de coordenadas)
    None,
    /// Solo ajuste fino: la nube ya está cerca de su lugar
    Fine,
    /// Búsqueda global de la orientación y después ajuste fino
    Auto,
}

/// Resultado de una alineación
#[derive(Debug, Clone, Copy)]
pub struct Alignment {
    /// Lleva la nube nueva al sistema de la referencia
    pub transform: Transform,
    /// Error cuadrático medio de las parejas finales (mm)
    pub rmse: f32,
    /// Fracción de la nube nueva que cae sobre la referencia (0 a 1)
    pub overlap: f32,
}

fn centroid(points: &[[f32; 3]]) -> [f32; 3] {
    let mut c = [0.0f64; 3];
    for p in points {
        for d in 0..3 {
            c[d] += p[d] as f64;
        }
    }
    let n = points.len().max(1) as f64;
    [(c[0] / n) as f32, (c[1] / n) as f32, (c[2] / n) as f32]
}

fn diagonal(points: &[[f32; 3]]) -> f32 {
    let (mut min, mut max) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for p in points {
        for d in 0..3 {
            min[d] = min[d].min(p[d]);
            max[d] = max[d].max(p[d]);
        }
    }
    (0..3).map(|d| (max[d] - min[d]).max(0.0).powi(2)).sum::<f32>().sqrt()
}

fn mat_mul(a: &[f32; 9], b: &[f32; 9]) -> [f32; 9] {
    let mut r = [0.0; 9];
    for i in 0..3 {
        for j in 0..3 {
            r[i * 3 + j] = (0..3).map(|k| a[i * 3 + k] * b[k * 3 + j]).sum();
        }
    }
    r
}

/// Rotación de ángulo `angle` alrededor del eje unitario `axis` (Rodrigues)
fn axis_angle(axis: [f32; 3], angle: f32) -> [f32; 9] {
    let (s, c) = angle.sin_cos();
    let t = 1.0 - c;
    let [x, y, z] = axis;
    [
        t * x * x + c,
        t * x * y - s * z,
        t * x * z + s * y,
        t * x * y + s * z,
        t * y * y + c,
        t * y * z - s * x,
        t * x * z - s * y,
        t * y * z + s * x,
        t * z * z + c,
    ]
}

/// Rotación que lleva el eje Y a la dirección unitaria `u`
fn y_to(u: [f32; 3]) -> [f32; 9] {
    // Y × u da el eje; el ángulo es el que forman
    let axis = [u[2], 0.0, -u[0]];
    let len = (axis[0] * axis[0] + axis[2] * axis[2]).sqrt();
    let angle = u[1].clamp(-1.0, 1.0).acos();
    if len < 1e-6 {
        return if u[1] > 0.0 { axis_angle([1.0, 0.0, 0.0], 0.0) } else { axis_angle([1.0, 0.0, 0.0], std::f32::consts::PI) };
    }
    axis_angle([axis[0] / len, 0.0, axis[2] / len], angle)
}

/// Orientaciones de prueba de la búsqueda global: 26 direcciones para el eje
/// "arriba" del objeto (caras, aristas y vértices de un cubo) por 8 giros de
/// 45° alrededor de ese eje. El ICP corto de cada prueba cubre la diferencia
/// que queda (unos 25°)
fn candidate_rotations() -> Vec<[f32; 9]> {
    let mut out = Vec::new();
    for x in -1i32..=1 {
        for y in -1i32..=1 {
            for z in -1i32..=1 {
                if x == 0 && y == 0 && z == 0 {
                    continue;
                }
                let len = ((x * x + y * y + z * z) as f32).sqrt();
                let up = y_to([x as f32 / len, y as f32 / len, z as f32 / len]);
                for k in 0..8 {
                    let yaw = axis_angle([0.0, 1.0, 0.0], k as f32 * std::f32::consts::FRAC_PI_4);
                    out.push(mat_mul(&up, &yaw));
                }
            }
        }
    }
    out
}

/// Rotación `r` alrededor de `from` y traslado de `from` a `to`
fn rotate_about(r: [f32; 9], from: [f32; 3], to: [f32; 3]) -> Transform {
    let rot = Transform { r, t: [0.0; 3] };
    let rf = rot.apply(from);
    Transform { r, t: [to[0] - rf[0], to[1] - rf[1], to[2] - rf[2]] }
}

/// Fracción de `src` que, llevada con `t`, tiene un punto de `target` a menos de `dist`
fn fitness(src: &[[f32; 3]], target: &VoxelIndex, t: &Transform, dist: f32) -> f32 {
    if src.is_empty() {
        return 0.0;
    }
    let hits = src.iter().filter(|&&p| target.nearest(t.apply(p), dist).is_some()).count();
    hits as f32 / src.len() as f32
}

/// Toma a lo sumo `max` puntos repartidos a lo largo del arreglo
fn thin(points: Vec<[f32; 3]>, max: usize) -> Vec<[f32; 3]> {
    if points.len() <= max {
        return points;
    }
    let step = points.len() as f32 / max as f32;
    (0..max).map(|i| points[(i as f32 * step) as usize]).collect()
}

// ---------------------------------------------------------------------------
// Registro global por descriptores (FPFH + RANSAC, Rusu 2009)
// ---------------------------------------------------------------------------

/// Casillas por ángulo del histograma FPFH
const FPFH_BINS: usize = 11;
const FPFH_LEN: usize = 3 * FPFH_BINS;

/// Normales orientadas hacia afuera del centroide (sirve para objetos
/// escaneados alrededor: casi todo punto de la superficie mira hacia afuera)
fn outward_normals(points: &[[f32; 3]], radius: f32) -> Vec<[f32; 3]> {
    let c = centroid(points);
    let mut normals = estimate_normals(points, radius);
    for (n, p) in normals.iter_mut().zip(points) {
        if (0..3).map(|d| n[d] * (p[d] - c[d])).sum::<f32>() < 0.0 {
            *n = [-n[0], -n[1], -n[2]];
        }
    }
    normals
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// Histogramas FPFH de cada punto con sus vecinos a `radius`
fn fpfh(points: &[[f32; 3]], normals: &[[f32; 3]], radius: f32) -> Vec<[f32; FPFH_LEN]> {
    let index = VoxelIndex::build(points.to_vec(), radius);
    let neighbors: Vec<Vec<usize>> = points.iter().enumerate().map(|(i, &p)| index.neighbors(p, radius).into_iter().filter(|&j| j != i).collect()).collect();
    let bin = |v: f32, lo: f32, hi: f32| (((v - lo) / (hi - lo)) * FPFH_BINS as f32).clamp(0.0, FPFH_BINS as f32 - 1.0) as usize;
    // Histograma simple (SPFH): ángulos del marco de Darboux con cada vecino
    let spfh: Vec<[f32; FPFH_LEN]> = points
        .iter()
        .enumerate()
        .map(|(i, &p)| {
            let mut h = [0.0f32; FPFH_LEN];
            for &j in &neighbors[i] {
                let q = points[j];
                let mut d = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
                let len = dot(d, d).sqrt();
                if len < 1e-6 {
                    continue;
                }
                d = d.map(|v| v / len);
                let (mut ns, mut nt, mut dd) = (normals[i], normals[j], d);
                // La fuente es el punto cuya normal forma el menor ángulo con la línea
                if dot(ns, d).abs() < dot(nt, d).abs() {
                    std::mem::swap(&mut ns, &mut nt);
                    dd = d.map(|v| -v);
                }
                let v = cross(ns, dd);
                let vl = dot(v, v).sqrt();
                if vl < 1e-6 {
                    continue;
                }
                let v = v.map(|x| x / vl);
                let w = cross(ns, v);
                let alpha = dot(v, nt);
                let phi = dot(ns, dd);
                let theta = dot(w, nt).atan2(dot(ns, nt));
                h[bin(alpha, -1.0, 1.0)] += 1.0;
                h[FPFH_BINS + bin(phi, -1.0, 1.0)] += 1.0;
                h[2 * FPFH_BINS + bin(theta, -std::f32::consts::PI, std::f32::consts::PI)] += 1.0;
            }
            let k = neighbors[i].len().max(1) as f32;
            h.map(|x| x * 100.0 / k)
        })
        .collect();
    // FPFH: el propio más el de los vecinos, pesados por la inversa de la distancia
    points
        .iter()
        .enumerate()
        .map(|(i, &p)| {
            let mut h = spfh[i];
            let k = neighbors[i].len();
            if k > 0 {
                for &j in &neighbors[i] {
                    let q = points[j];
                    let dist = ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2) + (q[2] - p[2]).powi(2)).sqrt().max(1e-3);
                    let w = 1.0 / (dist * k as f32);
                    for b in 0..FPFH_LEN {
                        h[b] += w * spfh[j][b];
                    }
                }
            }
            // Cada tercio suma 100: comparables entre densidades distintas
            for part in 0..3 {
                let sum: f32 = h[part * FPFH_BINS..(part + 1) * FPFH_BINS].iter().sum();
                if sum > 0.0 {
                    for b in &mut h[part * FPFH_BINS..(part + 1) * FPFH_BINS] {
                        *b *= 100.0 / sum;
                    }
                }
            }
            h
        })
        .collect()
}

fn feature_dist(a: &[f32; FPFH_LEN], b: &[f32; FPFH_LEN]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y).powi(2)).sum()
}

/// Generador pseudoaleatorio chico y determinista (xorshift)
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
}

/// Poses candidatas por descriptores: parejas por FPFH más parecido (en los
/// dos sentidos) y RANSAC de tres parejas con control de longitudes
fn feature_candidates(src: &[[f32; 3]], tgt: &[[f32; 3]], voxel: f32, keep: usize) -> Vec<Transform> {
    if src.len() < 8 || tgt.len() < 8 {
        return Vec::new();
    }
    let (ns, nt) = (outward_normals(src, 2.5 * voxel), outward_normals(tgt, 2.5 * voxel));
    let (fs, ft) = (fpfh(src, &ns, 5.0 * voxel), fpfh(tgt, &nt, 5.0 * voxel));
    let nearest = |f: &[f32; FPFH_LEN], set: &[[f32; FPFH_LEN]]| {
        let mut best = (0, f32::INFINITY);
        for (j, g) in set.iter().enumerate() {
            let d = feature_dist(f, g);
            if d < best.1 {
                best = (j, d);
            }
        }
        best.0
    };
    let back: Vec<usize> = ft.iter().map(|f| nearest(f, &fs)).collect();
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    for (i, f) in fs.iter().enumerate() {
        let j = nearest(f, &ft);
        // Pareja mutua, o la mitad de las demás para no quedarse sin datos
        if back[j] == i || i % 2 == 0 {
            pairs.push((i, j));
        }
    }
    if pairs.len() < 3 {
        return Vec::new();
    }
    let inlier = (2.0 * voxel).powi(2);
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut found: Vec<(Transform, usize)> = Vec::new();
    for _ in 0..20_000 {
        let pick = [rng.below(pairs.len()), rng.below(pairs.len()), rng.below(pairs.len())];
        if pick[0] == pick[1] || pick[1] == pick[2] || pick[0] == pick[2] {
            continue;
        }
        let a: Vec<[f32; 3]> = pick.iter().map(|&k| src[pairs[k].0]).collect();
        let b: Vec<[f32; 3]> = pick.iter().map(|&k| tgt[pairs[k].1]).collect();
        // Un cuerpo rígido conserva las distancias entre los tres puntos
        let len = |p: &[[f32; 3]], x: usize, y: usize| (0..3).map(|d| (p[x][d] - p[y][d]).powi(2)).sum::<f32>().sqrt();
        let ok = [(0, 1), (1, 2), (0, 2)].iter().all(|&(x, y)| {
            let (la, lb) = (len(&a, x, y), len(&b, x, y));
            la > 2.0 * voxel && la.min(lb) > 0.9 * la.max(lb)
        });
        if !ok {
            continue;
        }
        let t = best_fit_transform(&a, &b);
        let count = pairs
            .iter()
            .filter(|&&(i, j)| {
                let p = t.apply(src[i]);
                (0..3).map(|d| (p[d] - tgt[j][d]).powi(2)).sum::<f32>() < inlier
            })
            .count();
        if count >= 4 {
            found.push((t, count));
        }
    }
    found.sort_by(|a, b| b.1.cmp(&a.1));
    found.into_iter().take(keep).map(|(t, _)| t).collect()
}

/// Candidatas de la búsqueda global que pasan al ajuste fino
const FINE_CANDIDATES: usize = 8;

/// Alinea `source` sobre `target`. `None` si no se encontró ninguna
/// orientación en la que compartan superficie
pub fn align(source: &PointCloud, target: &PointCloud, mode: AlignMode) -> Option<Alignment> {
    if source.points.len() < 16 || target.points.len() < 16 {
        return None;
    }
    let spacing = edit::point_spacing(target).max(edit::point_spacing(source)).max(0.3);
    let all: Vec<[f32; 3]> = target.points.iter().map(|p| [p.x, p.y, p.z]).collect();
    let diag = diagonal(&all).max(10.0 * spacing);

    // Nubes finas: sobre ellas se ajusta y se juzga cada candidata
    let fine = (2.0 * spacing).max(diag / 250.0);
    let src = thin(downsample_positions(&source.points, fine), 20_000);
    let tgt = downsample_positions(&target.points, fine);
    let tight = 1.5 * fine;
    let judge = VoxelIndex::build(tgt.clone(), tight);

    if mode == AlignMode::None {
        let t = Transform::identity();
        return Some(Alignment { transform: t, rmse: 0.0, overlap: fitness(&src, &judge, &t, tight) });
    }

    // Ajuste fino punto a plano: gana la candidata que más superficie calza
    // con precisión (una pose equivocada puede solapar mucho en grueso, pero
    // no a la distancia de los puntos)
    let normals = estimate_normals(&tgt, 3.0 * fine);
    let reach = if mode == AlignMode::Auto { (diag / 30.0).max(4.0 * fine) } else { (diag / 10.0).max(6.0 * fine) };
    let index = VoxelIndex::build(tgt, reach);
    let min_corr = (src.len() / 20).max(8);
    let refine = |starts: &[Transform], best: &mut Option<Alignment>| {
        for &start in starts {
            let Some(r) = icp_point_to_plane(&src, &index, &normals, start, 40, reach, min_corr) else { continue };
            let overlap = fitness(&src, &judge, &r.transform, tight);
            let better = best.map_or(true, |b| overlap > b.overlap + 0.02 || (overlap > b.overlap - 0.02 && r.rmse < b.rmse));
            if better {
                *best = Some(Alignment { transform: r.transform, rmse: r.rmse, overlap });
            }
        }
    };

    let mut best = None;
    if mode == AlignMode::Fine {
        refine(&[Transform::identity()], &mut best);
        return best;
    }

    // 1. Descriptores: encuentran la parte común aunque las nubes solo
    //    compartan un pedazo, y son rápidos
    let fvoxel = (diag / 60.0).max(2.0 * spacing);
    let fsrc = thin(downsample_positions(&source.points, fvoxel), 1500);
    let ftgt = thin(downsample_positions(&target.points, fvoxel), 3000);
    refine(&feature_candidates(&fsrc, &ftgt, fvoxel, FINE_CANDIDATES), &mut best);
    if best.is_some_and(|b| b.overlap >= CONFIDENT_OVERLAP) {
        return best;
    }

    // 2. Sin una pose convincente (superficies lisas o repetidas, donde los
    //    descriptores se parecen): se prueban orientaciones en una grilla
    refine(&rotation_candidates(source, target, spacing, diag), &mut best);
    best
}

/// Solape a partir del cual la pose por descriptores se da por buena
const CONFIDENT_OVERLAP: f32 = 0.35;

/// Poses de la grilla de orientaciones: cada una termina en algún mínimo con
/// un ICP grueso corto; pasan al ajuste fino las que más solapan, sin repetir
fn rotation_candidates(source: &PointCloud, target: &PointCloud, spacing: f32, diag: f32) -> Vec<Transform> {
    let coarse = (diag / 45.0).max(2.0 * spacing);
    let csrc = thin(downsample_positions(&source.points, coarse), 500);
    let ctgt = downsample_positions(&target.points, coarse);
    let (cs, ct) = (centroid(&csrc), centroid(&ctgt));
    let wide = VoxelIndex::build(ctgt.clone(), diag / 5.0);
    let near = VoxelIndex::build(ctgt.clone(), diag / 14.0);
    let score = VoxelIndex::build(ctgt, 1.5 * coarse);
    let min_corr = (csrc.len() / 8).max(8);
    let mut found: Vec<(Transform, f32)> = Vec::new();
    for r in candidate_rotations() {
        let init = rotate_about(r, cs, ct);
        // Dos tramos: primero atrapa movimientos grandes, luego ajusta
        let Some(a) = icp(&csrc, &wide, init, 6, diag / 5.0, min_corr) else { continue };
        let t = icp(&csrc, &near, a.transform, 10, diag / 14.0, min_corr).map_or(a.transform, |b| b.transform);
        found.push((t, fitness(&csrc, &score, &t, 1.5 * coarse)));
    }
    found.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    // Varias pruebas caen en el mismo mínimo
    let probe = centroid(&csrc);
    let mut starts: Vec<Transform> = Vec::new();
    for (t, _) in found {
        let same = starts.iter().any(|s| {
            let (a, b) = (t.apply(probe), s.apply(probe));
            let moved = (0..3).map(|d| (a[d] - b[d]).powi(2)).sum::<f32>().sqrt();
            let turned = (0..9).map(|k| (t.r[k] - s.r[k]).powi(2)).sum::<f32>().sqrt();
            moved < 2.0 * coarse && turned < 0.2
        });
        if !same {
            starts.push(t);
        }
        if starts.len() == FINE_CANDIDATES {
            break;
        }
    }
    starts
}

/// La nube llevada con `t` (posición y dirección de vista)
pub fn transform_cloud(cloud: &PointCloud, t: &Transform) -> PointCloud {
    let points = cloud
        .points
        .iter()
        .map(|p| {
            let [x, y, z] = t.apply([p.x, p.y, p.z]);
            Point { x, y, z, rgb: p.rgb, view: t.apply_vec(p.view) }
        })
        .collect();
    PointCloud { points, has_color: cloud.has_color }
}

/// Suma dos nubes. Con `fuse_voxel > 0`, promedia por celda para que el
/// solape no quede con dos capas de puntos
pub fn merge(a: &PointCloud, b: &PointCloud, fuse_voxel: f32) -> PointCloud {
    let mut points = Vec::with_capacity(a.points.len() + b.points.len());
    points.extend_from_slice(&a.points);
    points.extend_from_slice(&b.points);
    let merged = PointCloud { points, has_color: a.has_color || b.has_color };
    if fuse_voxel > 0.0 {
        edit::downsample(&merged, fuse_voxel)
    } else {
        merged
    }
}

/// Estadísticas de una comparación
#[derive(Debug, Clone, Copy, Default)]
pub struct Deviation {
    /// Puntos con una pareja en la referencia a menos del alcance
    pub compared: usize,
    /// Puntos sin pareja (zonas que la referencia no tiene)
    pub unmatched: usize,
    pub mean: f32,
    pub rms: f32,
    pub max: f32,
    /// Percentil 95 (mm)
    pub p95: f32,
    /// Fracción de los comparados dentro de la tolerancia
    pub within: f32,
}

/// Distancia de cada punto de `cloud` al punto más cercano de `reference`;
/// `None` si no hay ninguno a menos de `reach` mm
pub fn distances(cloud: &PointCloud, reference: &PointCloud, reach: f32) -> Vec<Option<f32>> {
    let reach = reach.max(1e-3);
    let index = VoxelIndex::build(reference.points.iter().map(|p| [p.x, p.y, p.z]).collect(), reach);
    cloud.points.iter().map(|p| index.nearest([p.x, p.y, p.z], reach).map(|(_, d2)| d2.sqrt())).collect()
}

/// Resume las distancias con la `tolerance` dada (mm)
pub fn summarize(dist: &[Option<f32>], tolerance: f32) -> Deviation {
    let mut values: Vec<f32> = dist.iter().flatten().copied().collect();
    let unmatched = dist.len() - values.len();
    if values.is_empty() {
        return Deviation { unmatched, ..Default::default() };
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = values.len() as f64;
    let mean = values.iter().map(|&v| v as f64).sum::<f64>() / n;
    let rms = (values.iter().map(|&v| (v as f64).powi(2)).sum::<f64>() / n).sqrt();
    let p95 = values[((values.len() - 1) as f32 * 0.95).round() as usize];
    let within = values.iter().filter(|&&v| v <= tolerance).count() as f32 / values.len() as f32;
    Deviation { compared: values.len(), unmatched, mean: mean as f32, rms: rms as f32, max: *values.last().unwrap(), p95, within }
}

/// Mapa de color de las distancias: azul (0) → verde → amarillo → rojo
/// (`scale` mm o más); gris para los puntos sin pareja
pub fn heat_colors(dist: &[Option<f32>], scale: f32) -> Vec<[u8; 3]> {
    const STOPS: [[f32; 3]; 5] = [[40.0, 90.0, 220.0], [30.0, 190.0, 210.0], [60.0, 200.0, 80.0], [240.0, 210.0, 40.0], [225.0, 50.0, 40.0]];
    let scale = scale.max(1e-3);
    dist.iter()
        .map(|d| match d {
            None => [110, 110, 115],
            Some(v) => {
                let t = (v / scale).clamp(0.0, 1.0) * (STOPS.len() - 1) as f32;
                let i = (t.floor() as usize).min(STOPS.len() - 2);
                let f = t - i as f32;
                let (a, b) = (STOPS[i], STOPS[i + 1]);
                [0, 1, 2].map(|k| (a[k] + (b[k] - a[k]) * f).round() as u8)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Caja hueca de 80×50×30 mm muestreada cada 2 mm, sin la tapa de abajo:
    /// asimétrica en los tres ejes para que la orientación quede definida
    fn box_cloud() -> PointCloud {
        let (sx, sy, sz) = (80.0f32, 50.0f32, 30.0f32);
        let step = 2.0;
        let mut points = Vec::new();
        let mut push = |x: f32, y: f32, z: f32| points.push(Point { x, y, z, rgb: [200, 200, 200], view: [0.0; 3] });
        let n = |s: f32| (s / step) as i32;
        for i in 0..=n(sx) {
            for j in 0..=n(sy) {
                let (x, y) = (i as f32 * step, j as f32 * step);
                push(x, y, sz);
            }
        }
        for i in 0..=n(sx) {
            for k in 0..=n(sz) {
                let (x, z) = (i as f32 * step, k as f32 * step);
                push(x, 0.0, z);
                push(x, sy, z);
            }
        }
        for j in 0..=n(sy) {
            for k in 0..=n(sz) {
                let (y, z) = (j as f32 * step, k as f32 * step);
                push(0.0, y, z);
                push(sx, y, z);
            }
        }
        // Un saliente en una esquina rompe las simetrías de la caja
        for i in 0..8 {
            for j in 0..8 {
                push(i as f32 * step, j as f32 * step, sz + 12.0);
            }
        }
        PointCloud { points, has_color: true }
    }

    fn max_error(a: &PointCloud, t: &Transform, original: &PointCloud) -> f32 {
        a.points
            .iter()
            .zip(&original.points)
            .map(|(p, q)| {
                let r = t.apply([p.x, p.y, p.z]);
                ((r[0] - q.x).powi(2) + (r[1] - q.y).powi(2) + (r[2] - q.z).powi(2)).sqrt()
            })
            .fold(0.0, f32::max)
    }

    #[test]
    fn auto_alignment_recovers_a_flipped_scan() {
        let target = box_cloud();
        // Dado vuelta (180° en X), girado 70° en Y y corrido
        let r = mat_mul(&axis_angle([0.0, 1.0, 0.0], 70f32.to_radians()), &axis_angle([1.0, 0.0, 0.0], std::f32::consts::PI));
        let moved = transform_cloud(&target, &Transform { r, t: [120.0, -40.0, 300.0] });
        let a = align(&moved, &target, AlignMode::Auto).expect("alineación");
        assert!(a.overlap > 0.9, "solape {}", a.overlap);
        let err = max_error(&moved, &a.transform, &target);
        assert!(err < 2.0, "error máximo {err} mm");
    }

    /// Elipsoide con bultos (como una cabeza o una figura), muestreado denso
    fn lumpy_cloud() -> PointCloud {
        let mut points = Vec::new();
        let (nu, nv) = (140, 90);
        for i in 0..nu {
            for j in 1..nv {
                let th = i as f32 / nu as f32 * std::f32::consts::TAU;
                let ph = j as f32 / nv as f32 * std::f32::consts::PI;
                let bump = 1.0 + 0.12 * (3.0 * th).sin() * (2.0 * ph).cos() + 0.08 * (5.0 * ph + th).sin();
                let (x, y, z) = (40.0 * ph.sin() * th.cos(), 55.0 * ph.cos(), 30.0 * ph.sin() * th.sin());
                points.push(Point { x: x * bump, y: y * bump, z: z * bump, rgb: [200; 3], view: [0.0; 3] });
            }
        }
        PointCloud { points, has_color: true }
    }

    #[test]
    fn auto_alignment_works_with_partial_overlap() {
        let full = lumpy_cloud();
        let part = |keep: &dyn Fn(&Point) -> bool| PointCloud { points: full.points.iter().copied().filter(|p| keep(p)).collect(), has_color: true };
        // Cada escaneo ve una parte distinta del objeto y comparten el resto
        let target = part(&|p| p.x <= 15.0);
        let second = part(&|p| p.y >= -20.0);
        let r = mat_mul(&axis_angle([0.0, 0.0, 1.0], 2.4), &axis_angle([1.0, 0.0, 0.0], 1.1));
        let mut moved = transform_cloud(&second, &Transform { r, t: [-60.0, 25.0, 200.0] });
        // Ruido del sensor (±0.3 mm)
        let mut rng = Rng(7);
        for p in &mut moved.points {
            p.x += (rng.below(1000) as f32 / 1000.0 - 0.5) * 0.6;
            p.y += (rng.below(1000) as f32 / 1000.0 - 0.5) * 0.6;
            p.z += (rng.below(1000) as f32 / 1000.0 - 0.5) * 0.6;
        }
        let a = align(&moved, &target, AlignMode::Auto).expect("alineación");
        let err = max_error(&moved, &a.transform, &second);
        assert!(err < 2.5, "error máximo {err} mm, solape {}", a.overlap);
    }

    #[test]
    fn fine_alignment_fixes_a_small_offset() {
        let target = box_cloud();
        let r = axis_angle([0.3, 0.9, 0.1f32].map(|v| v / 0.9539), 6f32.to_radians());
        let moved = transform_cloud(&target, &Transform { r, t: [3.0, -2.0, 4.0] });
        let a = align(&moved, &target, AlignMode::Fine).expect("alineación");
        assert!(max_error(&moved, &a.transform, &target) < 1.0);
    }

    #[test]
    fn merge_fuses_the_overlap() {
        let a = box_cloud();
        let merged = merge(&a, &a, 2.0);
        assert!(merged.points.len() <= a.points.len() + a.points.len() / 10, "{} puntos", merged.points.len());
        assert_eq!(merge(&a, &a, 0.0).points.len(), 2 * a.points.len());
    }

    #[test]
    fn deviation_measures_the_offset() {
        let a = box_cloud();
        let shifted = transform_cloud(&a, &Transform { r: axis_angle([1.0, 0.0, 0.0], 0.0), t: [0.0, 0.0, 0.5] });
        let d = distances(&shifted, &a, 5.0);
        let s = summarize(&d, 1.0);
        assert_eq!(s.unmatched, 0);
        assert!(s.max <= 0.51 && s.within == 1.0, "{s:?}");
        let far = transform_cloud(&a, &Transform { r: axis_angle([1.0, 0.0, 0.0], 0.0), t: [500.0, 0.0, 0.0] });
        assert_eq!(summarize(&distances(&far, &a, 5.0), 1.0).compared, 0);
        assert_eq!(heat_colors(&[None, Some(0.0), Some(9.0)], 1.0).len(), 3);
    }
}
