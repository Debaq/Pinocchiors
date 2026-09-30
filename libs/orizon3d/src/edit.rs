//! Edición de la nube de puntos antes de mallar, como en el software de los
//! escáneres: quitar ruido (estadístico o por radio), borrar fragmentos
//! sueltos, quitar el plano de la mesa, simplificar, suavizar, borrar o
//! conservar una selección, y leer o guardar la nube en PLY.
//!
//! Las operaciones que solo quitan puntos devuelven una máscara (`true` =
//! conservar) para que quien llama pueda contar lo quitado o previsualizarlo;
//! [`retain`] la aplica. Coordenadas en mm, como el resto del crate.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};
use std::path::Path;

use crate::pointcloud::{Point, PointCloud};
use crate::scan::smallest_eigvec_sym3;

type Cell = (i32, i32, i32);

#[inline]
fn pos(p: &Point) -> [f32; 3] {
    [p.x, p.y, p.z]
}

#[inline]
fn cell_of(p: [f32; 3], size: f32) -> Cell {
    ((p[0] / size).floor() as i32, (p[1] / size).floor() as i32, (p[2] / size).floor() as i32)
}

#[inline]
fn dist2(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// Índice espacial por celdas cúbicas
struct Grid {
    size: f32,
    map: HashMap<Cell, Vec<u32>>,
}

impl Grid {
    fn build(points: &[Point], size: f32) -> Grid {
        let size = size.max(1e-3);
        let mut map: HashMap<Cell, Vec<u32>> = HashMap::new();
        for (i, p) in points.iter().enumerate() {
            map.entry(cell_of(pos(p), size)).or_default().push(i as u32);
        }
        Grid { size, map }
    }

    /// Distancias al cuadrado a los `k` vecinos más cercanos del punto `i`
    /// (sin él mismo), de menor a mayor. Recorre anillos de celdas hasta que
    /// el k-ésimo quede dentro de lo ya revisado o se llegue a `max_rings`
    fn knn(&self, points: &[Point], i: usize, k: usize, max_rings: i32, out: &mut Vec<f32>) {
        out.clear();
        let q = pos(&points[i]);
        let c = cell_of(q, self.size);
        for s in 0..=max_rings {
            for dz in -s..=s {
                for dy in -s..=s {
                    for dx in -s..=s {
                        if dx.abs().max(dy.abs()).max(dz.abs()) != s {
                            continue;
                        }
                        let Some(bucket) = self.map.get(&(c.0 + dx, c.1 + dy, c.2 + dz)) else { continue };
                        for &j in bucket {
                            if j as usize != i {
                                out.push(dist2(q, pos(&points[j as usize])));
                            }
                        }
                    }
                }
            }
            if out.len() >= k {
                out.select_nth_unstable_by(k - 1, f32::total_cmp);
                // Lo que está a menos de `s` celdas ya se revisó entero
                let covered = s as f32 * self.size;
                if out[k - 1] <= covered * covered {
                    break;
                }
            }
        }
        out.sort_unstable_by(f32::total_cmp);
        out.truncate(k);
    }

    /// Índices de los puntos a distancia ≤ `r` de `q`
    fn within(&self, points: &[Point], q: [f32; 3], r: f32, out: &mut Vec<u32>) {
        out.clear();
        let c = cell_of(q, self.size);
        let n = (r / self.size).ceil() as i32;
        let r2 = r * r;
        for dz in -n..=n {
            for dy in -n..=n {
                for dx in -n..=n {
                    let Some(bucket) = self.map.get(&(c.0 + dx, c.1 + dy, c.2 + dz)) else { continue };
                    out.extend(bucket.iter().copied().filter(|&j| dist2(q, pos(&points[j as usize])) <= r2));
                }
            }
        }
    }
}

/// `f(0..n)` repartido entre los núcleos del equipo
fn par_map<T: Send, F: Fn(usize) -> T + Sync>(n: usize, f: F) -> Vec<T> {
    let threads = std::thread::available_parallelism().map_or(1, |t| t.get()).min(16);
    if n < 4096 || threads < 2 {
        return (0..n).map(f).collect();
    }
    let chunk = n.div_ceil(threads);
    std::thread::scope(|scope| {
        let f = &f;
        let handles: Vec<_> = (0..threads)
            .map(|t| scope.spawn(move || (t * chunk..((t + 1) * chunk).min(n)).map(f).collect::<Vec<T>>()))
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("hilo de la nube")).collect()
    })
}

/// Caja envolvente (mínimo, máximo)
pub fn bounds(cloud: &PointCloud) -> Option<([f32; 3], [f32; 3])> {
    let first = cloud.points.first()?;
    let (mut mn, mut mx) = (pos(first), pos(first));
    for p in &cloud.points {
        let a = pos(p);
        for d in 0..3 {
            mn[d] = mn[d].min(a[d]);
            mx[d] = mx[d].max(a[d]);
        }
    }
    Some((mn, mx))
}

/// Separación típica entre un punto y su vecino más cercano (mm): la mediana
/// sobre una muestra de hasta 2000 puntos. Sirve de escala para las demás
/// operaciones. `0` con menos de dos puntos
pub fn point_spacing(cloud: &PointCloud) -> f32 {
    let n = cloud.points.len();
    let Some((mn, mx)) = bounds(cloud) else { return 0.0 };
    if n < 2 {
        return 0.0;
    }
    let diag = dist2(mn, mx).sqrt().max(1e-3);
    // Una superficie de tamaño `diag` con `n` puntos los tiene a ~diag/√n
    let grid = Grid::build(&cloud.points, (diag / (n as f32).sqrt()).max(1e-3) * 2.0);
    let step = (n / 2000).max(1);
    let mut scratch = Vec::new();
    let mut nearest: Vec<f32> = (0..n)
        .step_by(step)
        .filter_map(|i| {
            grid.knn(&cloud.points, i, 1, 8, &mut scratch);
            scratch.first().map(|d| d.sqrt())
        })
        .collect();
    if nearest.is_empty() {
        return 0.0;
    }
    let mid = nearest.len() / 2;
    *nearest.select_nth_unstable_by(mid, f32::total_cmp).1
}

/// Nube con los puntos cuya máscara es `true`
pub fn retain(cloud: &PointCloud, keep: &[bool]) -> PointCloud {
    PointCloud {
        points: cloud.points.iter().zip(keep).filter(|&(_, &k)| k).map(|(p, _)| *p).collect(),
        has_color: cloud.has_color,
    }
}

/// Máscara que conserva todo salvo los índices dados (los fuera de rango se ignoran)
pub fn mask_without(len: usize, indices: &[u32]) -> Vec<bool> {
    let mut keep = vec![true; len];
    for &i in indices {
        if let Some(k) = keep.get_mut(i as usize) {
            *k = false;
        }
    }
    keep
}

/// Máscara que conserva solo los índices dados
pub fn mask_only(len: usize, indices: &[u32]) -> Vec<bool> {
    let mut keep = vec![false; len];
    for &i in indices {
        if let Some(k) = keep.get_mut(i as usize) {
            *k = true;
        }
    }
    keep
}

/// Filtro estadístico de puntos atípicos (SOR): para cada punto, la distancia
/// media a sus `k` vecinos; se quitan los que superan la media global más
/// `std_ratio` desvíos. Limpia la bruma alrededor de la superficie
pub fn statistical_outliers(cloud: &PointCloud, k: usize, std_ratio: f32) -> Vec<bool> {
    let n = cloud.points.len();
    let k = k.clamp(2, 64);
    if n <= k {
        return vec![true; n];
    }
    let spacing = point_spacing(cloud).max(1e-3);
    // Celdas de ~2 separaciones: los k vecinos salen del primer o segundo anillo
    let grid = Grid::build(&cloud.points, spacing * 2.0);
    let mean_dist: Vec<f32> = par_map(n, |i| {
        let mut d = Vec::with_capacity(4 * k);
        grid.knn(&cloud.points, i, k, 6, &mut d);
        if d.is_empty() {
            return f32::INFINITY;
        }
        d.iter().map(|x| x.sqrt()).sum::<f32>() / d.len() as f32
    });
    let finite: Vec<f64> = mean_dist.iter().filter(|d| d.is_finite()).map(|&d| d as f64).collect();
    if finite.is_empty() {
        return vec![true; n];
    }
    let mean = finite.iter().sum::<f64>() / finite.len() as f64;
    let var = finite.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / finite.len() as f64;
    let limit = (mean + std_ratio.max(0.0) as f64 * var.sqrt()) as f32;
    mean_dist.iter().map(|&d| d <= limit).collect()
}

/// Filtro por radio: se quitan los puntos con menos de `min_neighbors`
/// vecinos a `radius` mm o menos. Quita puntos sueltos y motas
pub fn radius_outliers(cloud: &PointCloud, radius: f32, min_neighbors: usize) -> Vec<bool> {
    let n = cloud.points.len();
    let radius = radius.max(1e-3);
    let grid = Grid::build(&cloud.points, radius);
    par_map(n, |i| {
        let mut nb = Vec::new();
        grid.within(&cloud.points, pos(&cloud.points[i]), radius, &mut nb);
        // `within` incluye al propio punto
        nb.len() > min_neighbors
    })
}

/// Fragmentos de la nube: grupos de puntos separados por más de `gap` mm.
/// Devuelve la etiqueta de cada punto y el tamaño de cada grupo (de mayor a
/// menor: la etiqueta 0 es el más grande). Conecta celdas de `gap` vecinas,
/// así que dos puntos a menos de `gap` siempre quedan en el mismo grupo
pub fn clusters(cloud: &PointCloud, gap: f32) -> (Vec<u32>, Vec<usize>) {
    let gap = gap.max(1e-3);
    let mut cells: HashMap<Cell, u32> = HashMap::new();
    let cell_idx: Vec<u32> = cloud
        .points
        .iter()
        .map(|p| {
            let next = cells.len() as u32;
            *cells.entry(cell_of(pos(p), gap)).or_insert(next)
        })
        .collect();

    // Unión de celdas vecinas (26-vecindad)
    let mut parent: Vec<u32> = (0..cells.len() as u32).collect();
    fn find(parent: &mut [u32], mut x: u32) -> u32 {
        while parent[x as usize] != x {
            parent[x as usize] = parent[parent[x as usize] as usize];
            x = parent[x as usize];
        }
        x
    }
    for (&c, &a) in &cells {
        for dz in -1..=1 {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if let Some(&b) = cells.get(&(c.0 + dx, c.1 + dy, c.2 + dz)) {
                        let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
                        if ra != rb {
                            parent[ra.max(rb) as usize] = ra.min(rb);
                        }
                    }
                }
            }
        }
    }

    let mut sizes: HashMap<u32, usize> = HashMap::new();
    let roots: Vec<u32> = cell_idx.iter().map(|&c| find(&mut parent, c)).collect();
    for &r in &roots {
        *sizes.entry(r).or_default() += 1;
    }
    let mut order: Vec<(u32, usize)> = sizes.into_iter().collect();
    order.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let rank: HashMap<u32, u32> = order.iter().enumerate().map(|(i, &(r, _))| (r, i as u32)).collect();
    (roots.iter().map(|r| rank[r]).collect(), order.into_iter().map(|(_, s)| s).collect())
}

/// Quita los fragmentos con menos de `min_fraction` (0 a 1) de los puntos del
/// más grande. Con `min_fraction` ≥ 1 queda solo el más grande
pub fn small_clusters(cloud: &PointCloud, gap: f32, min_fraction: f32) -> Vec<bool> {
    let (labels, sizes) = clusters(cloud, gap);
    let Some(&largest) = sizes.first() else { return Vec::new() };
    if min_fraction >= 1.0 {
        return labels.iter().map(|&l| l == 0).collect();
    }
    let min = (largest as f32 * min_fraction.max(0.0)).ceil() as usize;
    labels.iter().map(|&l| sizes[l as usize] >= min).collect()
}

/// Plano encontrado en la nube: `normal · p = offset`
#[derive(Debug, Clone, Copy)]
pub struct Plane {
    pub normal: [f32; 3],
    pub offset: f32,
    pub inliers: usize,
}

impl Plane {
    #[inline]
    pub fn distance(&self, p: [f32; 3]) -> f32 {
        self.normal[0] * p[0] + self.normal[1] * p[1] + self.normal[2] * p[2] - self.offset
    }
}

/// Generador pseudoaleatorio fijo (xorshift): el mismo resultado cada vez
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// Plano que mejor pasa por los puntos (centroide y autovector menor)
fn fit_plane(points: impl Iterator<Item = [f32; 3]>) -> Option<([f32; 3], [f32; 3])> {
    let pts: Vec<[f32; 3]> = points.collect();
    if pts.len() < 3 {
        return None;
    }
    let mut c = [0.0f64; 3];
    for p in &pts {
        for d in 0..3 {
            c[d] += p[d] as f64;
        }
    }
    for v in &mut c {
        *v /= pts.len() as f64;
    }
    let mut cov = [[0.0f64; 3]; 3];
    for p in &pts {
        let v = [p[0] as f64 - c[0], p[1] as f64 - c[1], p[2] as f64 - c[2]];
        for a in 0..3 {
            for b in 0..3 {
                cov[a][b] += v[a] * v[b];
            }
        }
    }
    Some((smallest_eigvec_sym3(cov), [c[0] as f32, c[1] as f32, c[2] as f32]))
}

/// Plano dominante por RANSAC (la mesa o el plato giratorio): el que junta
/// más puntos a `threshold` mm o menos, afinado por mínimos cuadrados.
/// `None` si ningún plano junta al menos el 5 % de los puntos
pub fn dominant_plane(cloud: &PointCloud, threshold: f32) -> Option<Plane> {
    let n = cloud.points.len();
    if n < 30 {
        return None;
    }
    let threshold = threshold.max(0.05);
    // Se puntúa sobre una muestra para no recorrer la nube entera cada vez
    let step = (n / 20_000).max(1);
    let sample: Vec<[f32; 3]> = cloud.points.iter().step_by(step).map(pos).collect();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut best: Option<([f32; 3], f32, usize)> = None;
    for _ in 0..400 {
        let (a, b, c) = (sample[rng.below(sample.len())], sample[rng.below(sample.len())], sample[rng.below(sample.len())]);
        let nrm = cross(sub(b, a), sub(c, a));
        let len = (nrm[0] * nrm[0] + nrm[1] * nrm[1] + nrm[2] * nrm[2]).sqrt();
        if len < 1e-6 {
            continue;
        }
        let nrm = [nrm[0] / len, nrm[1] / len, nrm[2] / len];
        let off = nrm[0] * a[0] + nrm[1] * a[1] + nrm[2] * a[2];
        let count = sample
            .iter()
            .filter(|p| (nrm[0] * p[0] + nrm[1] * p[1] + nrm[2] * p[2] - off).abs() <= threshold)
            .count();
        if best.is_none_or(|(_, _, c)| count > c) {
            best = Some((nrm, off, count));
        }
    }
    let (nrm, off, _) = best?;
    // Afinado: plano de mínimos cuadrados de los puntos cercanos al candidato
    let near = cloud.points.iter().map(pos).filter(|p| (nrm[0] * p[0] + nrm[1] * p[1] + nrm[2] * p[2] - off).abs() <= threshold);
    let (normal, center) = fit_plane(near)?;
    let plane = Plane { normal, offset: normal[0] * center[0] + normal[1] * center[1] + normal[2] * center[2], inliers: 0 };
    let inliers = cloud.points.iter().filter(|p| plane.distance(pos(p)).abs() <= threshold).count();
    (inliers * 20 >= n).then_some(Plane { inliers, ..plane })
}

/// Quita el plano dominante (la mesa). Con `below`, también lo que queda del
/// lado del plano con menos puntos (la cara de abajo de la mesa, el pie)
pub fn remove_plane(cloud: &PointCloud, threshold: f32, below: bool) -> Option<(Vec<bool>, Plane)> {
    let plane = dominant_plane(cloud, threshold)?;
    let threshold = threshold.max(0.05);
    let side: Vec<f32> = cloud.points.iter().map(|p| plane.distance(pos(p))).collect();
    let above = side.iter().filter(|&&d| d > threshold).count();
    let under = side.iter().filter(|&&d| d < -threshold).count();
    // El objeto está del lado con más puntos
    let sign = if above >= under { 1.0 } else { -1.0 };
    let keep = side.iter().map(|&d| if below { d * sign > threshold } else { d.abs() > threshold }).collect();
    Some((keep, plane))
}

/// Simplifica la nube a un punto por celda de `voxel` mm (promedio de
/// posición, color y dirección de vista)
pub fn downsample(cloud: &PointCloud, voxel: f32) -> PointCloud {
    let voxel = voxel.max(1e-3);
    // Orden de llegada de las celdas: el resultado no depende del hash
    let mut index: HashMap<Cell, usize> = HashMap::new();
    let mut acc: Vec<([f64; 3], [f64; 3], [f64; 3], u32)> = Vec::new();
    for p in &cloud.points {
        let next = acc.len();
        let i = *index.entry(cell_of(pos(p), voxel)).or_insert(next);
        if i == next {
            acc.push(([0.0; 3], [0.0; 3], [0.0; 3], 0));
        }
        let a = &mut acc[i];
        for d in 0..3 {
            a.0[d] += pos(p)[d] as f64;
            a.1[d] += p.rgb[d] as f64;
            a.2[d] += p.view[d] as f64;
        }
        a.3 += 1;
    }
    let points = acc
        .into_iter()
        .map(|(s, c, v, n)| {
            let n = n as f64;
            let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            let view = if len < 1e-9 { [0.0; 3] } else { [(v[0] / len) as f32, (v[1] / len) as f32, (v[2] / len) as f32] };
            Point {
                x: (s[0] / n) as f32,
                y: (s[1] / n) as f32,
                z: (s[2] / n) as f32,
                rgb: [(c[0] / n).round() as u8, (c[1] / n).round() as u8, (c[2] / n).round() as u8],
                view,
            }
        })
        .collect();
    PointCloud { points, has_color: cloud.has_color }
}

/// Suaviza la superficie: cada punto se lleva al plano que mejor ajusta a
/// sus vecinos a `radius` mm (sin encoger la forma como un promedio simple).
/// `strength` (0 a 1) es cuánto se mueve hacia ese plano
pub fn smooth(cloud: &PointCloud, radius: f32, strength: f32) -> PointCloud {
    let radius = radius.max(1e-3);
    let strength = strength.clamp(0.0, 1.0);
    let grid = Grid::build(&cloud.points, radius);
    let points = par_map(cloud.points.len(), |i| {
        let p = cloud.points[i];
        let mut nb = Vec::new();
        grid.within(&cloud.points, pos(&p), radius, &mut nb);
        if nb.len() < 5 {
            return p;
        }
        let Some((n, c)) = fit_plane(nb.iter().map(|&j| pos(&cloud.points[j as usize]))) else { return p };
        let d = (p.x - c[0]) * n[0] + (p.y - c[1]) * n[1] + (p.z - c[2]) * n[2];
        let k = d * strength;
        Point { x: p.x - k * n[0], y: p.y - k * n[1], z: p.z - k * n[2], ..p }
    });
    PointCloud { points, has_color: cloud.has_color }
}

// ---------------------------------------------------------------------------
// PLY
// ---------------------------------------------------------------------------

/// Guarda la nube en PLY binario (little endian) con posición, dirección de
/// vista como normal (orienta la malla al reconstruir) y color si lo hay
pub fn save_ply(cloud: &PointCloud, path: &Path) -> io::Result<()> {
    let mut out = BufWriter::new(File::create(path)?);
    writeln!(out, "ply")?;
    writeln!(out, "format binary_little_endian 1.0")?;
    writeln!(out, "comment generado por Pinocchio (Orizon3D), unidades en mm")?;
    writeln!(out, "element vertex {}", cloud.points.len())?;
    for name in ["x", "y", "z", "nx", "ny", "nz"] {
        writeln!(out, "property float {name}")?;
    }
    if cloud.has_color {
        for name in ["red", "green", "blue"] {
            writeln!(out, "property uchar {name}")?;
        }
    }
    writeln!(out, "end_header")?;
    for p in &cloud.points {
        for v in [p.x, p.y, p.z, p.view[0], p.view[1], p.view[2]] {
            out.write_all(&v.to_le_bytes())?;
        }
        if cloud.has_color {
            out.write_all(&p.rgb)?;
        }
    }
    out.flush()
}

#[derive(Clone, Copy, PartialEq)]
enum Format {
    Ascii,
    Little,
    Big,
}

#[derive(Clone, Copy)]
enum Scalar {
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    F32,
    F64,
}

impl Scalar {
    fn parse(s: &str) -> Option<Scalar> {
        Some(match s {
            "char" | "int8" => Scalar::I8,
            "uchar" | "uint8" => Scalar::U8,
            "short" | "int16" => Scalar::I16,
            "ushort" | "uint16" => Scalar::U16,
            "int" | "int32" => Scalar::I32,
            "uint" | "uint32" => Scalar::U32,
            "float" | "float32" => Scalar::F32,
            "double" | "float64" => Scalar::F64,
            _ => return None,
        })
    }

    fn size(self) -> usize {
        match self {
            Scalar::I8 | Scalar::U8 => 1,
            Scalar::I16 | Scalar::U16 => 2,
            Scalar::I32 | Scalar::U32 | Scalar::F32 => 4,
            Scalar::F64 => 8,
        }
    }

    fn is_float(self) -> bool {
        matches!(self, Scalar::F32 | Scalar::F64)
    }

    fn read(self, bytes: &[u8], format: Format) -> f64 {
        macro_rules! num {
            ($t:ty) => {{
                let b = bytes.try_into().unwrap();
                (if format == Format::Big { <$t>::from_be_bytes(b) } else { <$t>::from_le_bytes(b) }) as f64
            }};
        }
        match self {
            Scalar::I8 => bytes[0] as i8 as f64,
            Scalar::U8 => bytes[0] as f64,
            Scalar::I16 => num!(i16),
            Scalar::U16 => num!(u16),
            Scalar::I32 => num!(i32),
            Scalar::U32 => num!(u32),
            Scalar::F32 => num!(f32),
            Scalar::F64 => num!(f64),
        }
    }
}

struct Property {
    name: String,
    /// `Some(tipo del contador)` si es una lista
    list: Option<Scalar>,
    kind: Scalar,
}

struct Element {
    name: String,
    count: usize,
    props: Vec<Property>,
}

fn bad(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

/// Lee una nube (o los vértices de una malla) de un PLY ASCII o binario.
/// Toma x, y, z, el color (red/green/blue o r/g/b) y las normales como
/// dirección de vista si las trae
pub fn load_ply(path: &Path) -> io::Result<PointCloud> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut line = String::new();
    let next_line = |reader: &mut BufReader<File>, line: &mut String| -> io::Result<()> {
        line.clear();
        if reader.read_line(line)? == 0 {
            return Err(bad("El PLY termina antes de la cabecera"));
        }
        Ok(())
    };
    next_line(&mut reader, &mut line)?;
    if line.trim() != "ply" {
        return Err(bad("No es un archivo PLY"));
    }
    let mut format = None;
    let mut elements: Vec<Element> = Vec::new();
    loop {
        next_line(&mut reader, &mut line)?;
        let words: Vec<&str> = line.split_whitespace().collect();
        match words.as_slice() {
            ["format", f, ..] => {
                format = Some(match *f {
                    "ascii" => Format::Ascii,
                    "binary_little_endian" => Format::Little,
                    "binary_big_endian" => Format::Big,
                    _ => return Err(bad(format!("Formato PLY desconocido: {f}"))),
                })
            }
            ["element", name, count] => elements.push(Element {
                name: name.to_string(),
                count: count.parse().map_err(|_| bad("Cantidad de elementos inválida"))?,
                props: Vec::new(),
            }),
            ["property", "list", count, kind, name] => {
                let prop = Property {
                    name: name.to_string(),
                    list: Some(Scalar::parse(count).ok_or_else(|| bad(format!("Tipo desconocido: {count}")))?),
                    kind: Scalar::parse(kind).ok_or_else(|| bad(format!("Tipo desconocido: {kind}")))?,
                };
                elements.last_mut().ok_or_else(|| bad("Propiedad sin elemento"))?.props.push(prop);
            }
            ["property", kind, name] => {
                let prop = Property {
                    name: name.to_string(),
                    list: None,
                    kind: Scalar::parse(kind).ok_or_else(|| bad(format!("Tipo desconocido: {kind}")))?,
                };
                elements.last_mut().ok_or_else(|| bad("Propiedad sin elemento"))?.props.push(prop);
            }
            ["end_header"] => break,
            _ => {}
        }
    }
    let format = format.ok_or_else(|| bad("El PLY no dice su formato"))?;

    let mut cloud = PointCloud::default();
    let mut rest = String::new();
    if format == Format::Ascii {
        reader.read_to_string(&mut rest)?;
    }
    let mut tokens = rest.split_ascii_whitespace();

    for element in &elements {
        let is_vertex = element.name == "vertex";
        let find = |names: &[&str]| element.props.iter().position(|p| p.list.is_none() && names.contains(&p.name.as_str()));
        let xyz = [find(&["x"]), find(&["y"]), find(&["z"])];
        let rgb = [find(&["red", "r", "diffuse_red"]), find(&["green", "g", "diffuse_green"]), find(&["blue", "b", "diffuse_blue"])];
        let nrm = [find(&["nx"]), find(&["ny"]), find(&["nz"])];
        if is_vertex {
            if xyz.iter().any(Option::is_none) {
                return Err(bad("Los vértices del PLY no tienen x, y, z"));
            }
            cloud.has_color = rgb.iter().all(Option::is_some);
            cloud.points.reserve(element.count);
        }
        let mut values = vec![0.0f64; element.props.len()];
        for _ in 0..element.count {
            for (k, prop) in element.props.iter().enumerate() {
                if format == Format::Ascii {
                    let mut take = || -> io::Result<f64> {
                        tokens.next().ok_or_else(|| bad("El PLY está cortado"))?.parse::<f64>().map_err(|_| bad("Número inválido en el PLY"))
                    };
                    match prop.list {
                        Some(_) => {
                            let len = take()? as usize;
                            for _ in 0..len {
                                take()?;
                            }
                        }
                        None => values[k] = take()?,
                    }
                } else {
                    let mut read = |kind: Scalar| -> io::Result<f64> {
                        let mut buf = [0u8; 8];
                        reader.read_exact(&mut buf[..kind.size()]).map_err(|_| bad("El PLY está cortado"))?;
                        Ok(kind.read(&buf[..kind.size()], format))
                    };
                    match prop.list {
                        Some(count) => {
                            let len = read(count)? as usize;
                            for _ in 0..len {
                                read(prop.kind)?;
                            }
                        }
                        None => values[k] = read(prop.kind)?,
                    }
                }
            }
            if !is_vertex {
                continue;
            }
            let get = |i: Option<usize>| i.map_or(0.0, |i| values[i]);
            let color = |i: Option<usize>| {
                let Some(i) = i else { return 0 };
                let v = values[i];
                (if element.props[i].kind.is_float() { v * 255.0 } else { v }).round().clamp(0.0, 255.0) as u8
            };
            let n = [get(nrm[0]), get(nrm[1]), get(nrm[2])];
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            let view = if len > 1e-9 { [(n[0] / len) as f32, (n[1] / len) as f32, (n[2] / len) as f32] } else { [0.0; 3] };
            cloud.points.push(Point {
                x: get(xyz[0]) as f32,
                y: get(xyz[1]) as f32,
                z: get(xyz[2]) as f32,
                rgb: [color(rgb[0]), color(rgb[1]), color(rgb[2])],
                view,
            });
        }
        if is_vertex {
            // Lo que sigue (caras) no hace falta
            break;
        }
    }
    if cloud.points.is_empty() {
        return Err(bad("El PLY no tiene vértices"));
    }
    Ok(cloud)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(x: f32, y: f32, z: f32) -> Point {
        Point { x, y, z, rgb: [10, 20, 30], view: [0.0, 0.0, -1.0] }
    }

    /// Plano z = 0 de 40×40 puntos cada 1 mm
    fn plane_grid() -> Vec<Point> {
        (0..40).flat_map(|i| (0..40).map(move |j| pt(i as f32, j as f32, 0.0))).collect()
    }

    fn cloud(points: Vec<Point>) -> PointCloud {
        PointCloud { points, has_color: true }
    }

    #[test]
    fn spacing_of_a_regular_grid() {
        let s = point_spacing(&cloud(plane_grid()));
        assert!((s - 1.0).abs() < 1e-4, "{s}");
    }

    #[test]
    fn statistical_filter_drops_the_far_points() {
        let mut pts = plane_grid();
        pts.push(pt(20.0, 20.0, 15.0));
        pts.push(pt(-30.0, 5.0, 8.0));
        let keep = statistical_outliers(&cloud(pts), 8, 2.0);
        let n = keep.len();
        assert!(!keep[n - 1] && !keep[n - 2]);
        // Los bordes del plano quedan a juicio del filtro; el interior se conserva
        let interior = |i: usize| (2..38).contains(&(i / 40)) && (2..38).contains(&(i % 40));
        assert!((0..n - 2).filter(|&i| interior(i)).all(|i| keep[i]));
    }

    #[test]
    fn radius_filter_drops_isolated_points() {
        let mut pts = plane_grid();
        pts.push(pt(100.0, 100.0, 100.0));
        let keep = radius_outliers(&cloud(pts), 1.5, 3);
        assert!(!keep[keep.len() - 1]);
        assert!(keep[..keep.len() - 1].iter().all(|k| *k));
    }

    #[test]
    fn small_fragments_are_removed() {
        let mut pts = plane_grid();
        let island: Vec<Point> = (0..5).map(|i| pt(200.0 + i as f32, 0.0, 0.0)).collect();
        pts.extend(island);
        let c = cloud(pts);
        let (labels, sizes) = clusters(&c, 3.0);
        assert_eq!(sizes, vec![1600, 5]);
        assert_eq!(labels[0], 0);
        let keep = small_clusters(&c, 3.0, 0.1);
        assert_eq!(keep.iter().filter(|k| **k).count(), 1600);
        let only = small_clusters(&c, 3.0, 1.0);
        assert_eq!(only.iter().filter(|k| **k).count(), 1600);
    }

    #[test]
    fn table_plane_is_found_and_removed() {
        // Mesa en y = 50 y un "objeto" encima (y < 50, la Y de cámara va hacia abajo)
        let mut pts: Vec<Point> = (0..60).flat_map(|i| (0..60).map(move |j| pt(i as f32, 50.0, j as f32))).collect();
        let object: Vec<Point> = (0..20).flat_map(|i| (0..20).map(move |j| pt(20.0 + i as f32, 49.0 - j as f32, 30.0))).collect();
        let object_len = object.len();
        pts.extend(object);
        // Algo debajo de la mesa
        pts.push(pt(10.0, 80.0, 10.0));
        let c = cloud(pts);
        let (keep, plane) = remove_plane(&c, 0.5, true).expect("plano");
        assert!(plane.normal[1].abs() > 0.99);
        assert_eq!(plane.inliers, 3600);
        // Se va la mesa y lo de abajo; el objeto queda
        let kept = keep.iter().filter(|k| **k).count();
        assert_eq!(kept, object_len);
        assert!(!keep[keep.len() - 1]);
    }

    #[test]
    fn downsample_averages_per_cell() {
        let c = cloud(plane_grid());
        let d = downsample(&c, 4.0);
        assert_eq!(d.points.len(), 100);
        assert!(d.has_color);
        assert_eq!(d.points[0].rgb, [10, 20, 30]);
    }

    #[test]
    fn smoothing_flattens_noise_on_a_plane() {
        let pts: Vec<Point> = plane_grid()
            .into_iter()
            .enumerate()
            .map(|(i, p)| Point { z: if i % 2 == 0 { 0.3 } else { -0.3 }, ..p })
            .collect();
        let s = smooth(&cloud(pts), 2.5, 1.0);
        // Lejos del borde, donde los vecinos rodean al punto
        let worst = s
            .points
            .iter()
            .filter(|p| (3.0..37.0).contains(&p.x) && (3.0..37.0).contains(&p.y))
            .map(|p| p.z.abs())
            .fold(0.0f32, f32::max);
        assert!(worst < 0.15, "{worst}");
    }

    #[test]
    fn masks_by_index() {
        assert_eq!(mask_without(4, &[1, 3, 9]), vec![true, false, true, false]);
        assert_eq!(mask_only(3, &[2]), vec![false, false, true]);
    }

    #[test]
    fn ply_round_trip_binary_and_ascii() {
        let dir = std::env::temp_dir().join(format!("orizon3d-edit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let c = cloud(vec![pt(1.0, 2.0, 3.0), pt(-4.5, 0.25, 9.0)]);

        let bin = dir.join("nube.ply");
        save_ply(&c, &bin).unwrap();
        let back = load_ply(&bin).unwrap();
        assert_eq!(back.points.len(), 2);
        assert!(back.has_color);
        assert_eq!(pos(&back.points[1]), [-4.5, 0.25, 9.0]);
        assert_eq!(back.points[0].rgb, [10, 20, 30]);
        assert_eq!(back.points[0].view, [0.0, 0.0, -1.0]);

        let ascii = dir.join("nube-ascii.ply");
        c.export_ply(&ascii).unwrap();
        let back = load_ply(&ascii).unwrap();
        assert_eq!(pos(&back.points[0]), [1.0, 2.0, 3.0]);
        assert_eq!(back.points[1].rgb, [10, 20, 30]);

        // Malla ASCII con caras: se leen solo los vértices
        let mesh = dir.join("malla.ply");
        std::fs::write(
            &mesh,
            "ply\nformat ascii 1.0\nelement vertex 3\nproperty float x\nproperty float y\nproperty float z\n\
             element face 1\nproperty list uchar int vertex_indices\nend_header\n0 0 0\n1 0 0\n0 1 0\n3 0 1 2\n",
        )
        .unwrap();
        let back = load_ply(&mesh).unwrap();
        assert_eq!(back.points.len(), 3);
        assert!(!back.has_color);
        std::fs::remove_dir_all(&dir).ok();
    }
}
