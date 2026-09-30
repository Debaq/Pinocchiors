//! Registro (ICP) y fusión incremental de nubes para escaneo multi-frame.
//!
//! Cada nube nueva (en coordenadas de cámara) se alinea contra el MODELO
//! acumulado (en coordenadas globales, las del primer cuadro) con ICP
//! punto-a-plano, partiendo de la pose anterior. La transformación resultante
//! es la pose global del cuadro; con ella se acumulan los puntos en una rejilla
//! de vóxeles que fusiona (promedia) las observaciones repetidas.
//!
//! El ICP solo mueve la pose en las direcciones que la geometría restringe: en
//! un objeto con paredes verticales, un cilindro o un plano, alguna dirección
//! queda libre y, sin ese cuidado, el ruido la desliza y el error se acumula
//! hasta deformar el modelo (ver [`solve_constrained`]).
//!
//! Todo es Rust puro y sin dependencias externas:
//! - vecino más cercano por *hash* espacial de vóxeles (celdas 3×3×3),
//! - ajuste rígido óptimo por el método de cuaterniones de Horn, con la
//!   eigen-descomposición de una matriz simétrica vía rotaciones de Jacobi.

use std::collections::{HashMap, HashSet};

use crate::pointcloud::{Point, PointCloud};

/// Transformación rígida 3D (rotación 3×3 fila-mayor + traslación), en mm.
#[derive(Debug, Clone, Copy)]
pub struct Transform {
    pub r: [f32; 9],
    pub t: [f32; 3],
}

impl Transform {
    pub fn identity() -> Self {
        Transform {
            r: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            t: [0.0, 0.0, 0.0],
        }
    }

    /// Aplica la transformación a un punto.
    #[inline]
    pub fn apply(&self, p: [f32; 3]) -> [f32; 3] {
        let r = &self.r;
        [
            r[0] * p[0] + r[1] * p[1] + r[2] * p[2] + self.t[0],
            r[3] * p[0] + r[4] * p[1] + r[5] * p[2] + self.t[1],
            r[6] * p[0] + r[7] * p[1] + r[8] * p[2] + self.t[2],
        ]
    }

    /// Transformación inversa (la rotación es ortonormal: su inversa es la traspuesta)
    pub fn inverse(&self) -> Transform {
        let r = &self.r;
        let rt = [r[0], r[3], r[6], r[1], r[4], r[7], r[2], r[5], r[8]];
        let t = [
            -(rt[0] * self.t[0] + rt[1] * self.t[1] + rt[2] * self.t[2]),
            -(rt[3] * self.t[0] + rt[4] * self.t[1] + rt[5] * self.t[2]),
            -(rt[6] * self.t[0] + rt[7] * self.t[1] + rt[8] * self.t[2]),
        ];
        Transform { r: rt, t }
    }

    /// Aplica solo la rotación (para vectores/normales, sin traslación).
    #[inline]
    pub fn apply_vec(&self, v: [f32; 3]) -> [f32; 3] {
        let r = &self.r;
        [
            r[0] * v[0] + r[1] * v[1] + r[2] * v[2],
            r[3] * v[0] + r[4] * v[1] + r[5] * v[2],
            r[6] * v[0] + r[7] * v[1] + r[8] * v[2],
        ]
    }
}

// ---------------------------------------------------------------------------
// Índice espacial para vecino más cercano (hash de vóxeles).
// ---------------------------------------------------------------------------

type Cell = (i32, i32, i32);

/// Índice de puntos por celdas de vóxel; busca el vecino más cercano dentro de
/// un radio examinando el cubo 3×3×3 de celdas alrededor de la consulta.
pub struct VoxelIndex {
    voxel: f32,
    map: HashMap<Cell, Vec<u32>>,
    points: Vec<[f32; 3]>,
}

impl VoxelIndex {
    pub fn build(points: Vec<[f32; 3]>, voxel: f32) -> Self {
        let voxel = voxel.max(1e-3);
        let mut map: HashMap<Cell, Vec<u32>> = HashMap::new();
        for (i, p) in points.iter().enumerate() {
            map.entry(cell_of(*p, voxel)).or_default().push(i as u32);
        }
        VoxelIndex { voxel, map, points }
    }

    #[inline]
    pub fn point(&self, idx: usize) -> [f32; 3] {
        self.points[idx]
    }

    /// Vecino más cercano a `q` cuya distancia sea ≤ `max_dist`. Para que baste
    /// con mirar el cubo 3×3×3, conviene usar `voxel >= max_dist` al construir.
    pub fn nearest(&self, q: [f32; 3], max_dist: f32) -> Option<(usize, f32)> {
        let (cx, cy, cz) = cell_of(q, self.voxel);
        let max_d2 = max_dist * max_dist;
        let mut best: Option<(usize, f32)> = None;
        for dz in -1..=1 {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let Some(bucket) = self.map.get(&(cx + dx, cy + dy, cz + dz)) else {
                        continue;
                    };
                    for &i in bucket {
                        let p = self.points[i as usize];
                        let d2 = (p[0] - q[0]).powi(2)
                            + (p[1] - q[1]).powi(2)
                            + (p[2] - q[2]).powi(2);
                        if d2 <= max_d2 && best.map_or(true, |(_, bd)| d2 < bd) {
                            best = Some((i as usize, d2));
                        }
                    }
                }
            }
        }
        best
    }

    /// Todos los puntos a distancia ≤ `radius` de `q` (para estimar normales).
    pub fn neighbors(&self, q: [f32; 3], radius: f32) -> Vec<usize> {
        let (cx, cy, cz) = cell_of(q, self.voxel);
        let r = (radius / self.voxel).ceil() as i32;
        let r2 = radius * radius;
        let mut out = Vec::new();
        for dz in -r..=r {
            for dy in -r..=r {
                for dx in -r..=r {
                    let Some(bucket) = self.map.get(&(cx + dx, cy + dy, cz + dz)) else {
                        continue;
                    };
                    for &i in bucket {
                        let p = self.points[i as usize];
                        let d2 = (p[0] - q[0]).powi(2)
                            + (p[1] - q[1]).powi(2)
                            + (p[2] - q[2]).powi(2);
                        if d2 <= r2 {
                            out.push(i as usize);
                        }
                    }
                }
            }
        }
        out
    }
}

#[inline]
fn cell_of(p: [f32; 3], voxel: f32) -> Cell {
    (
        (p[0] / voxel).floor() as i32,
        (p[1] / voxel).floor() as i32,
        (p[2] / voxel).floor() as i32,
    )
}

/// Limpia la nube para quedarse con el objeto: descarta vóxeles poco poblados
/// (ruido / puntos sueltos) y, si `isolate`, conserva solo el grupo conexo más
/// grande (el objeto principal), tirando trozos desconectados (mesa, pared,
/// manos, fondo). Es la "detección de objeto" automática.
pub fn clean_cloud(cloud: &PointCloud, voxel: f32, min_pts: u32, isolate: bool) -> PointCloud {
    let empty = || PointCloud {
        points: Vec::new(),
        has_color: cloud.has_color,
    };
    if cloud.points.is_empty() {
        return empty();
    }
    let voxel = voxel.max(1e-3);

    // 1) Indexar puntos por vóxel.
    let mut cells: HashMap<Cell, Vec<u32>> = HashMap::new();
    for (i, p) in cloud.points.iter().enumerate() {
        cells
            .entry(cell_of([p.x, p.y, p.z], voxel))
            .or_default()
            .push(i as u32);
    }

    // 2) Filtro de densidad: descartar vóxeles con muy pocos puntos (ruido).
    let min_pts = min_pts.max(1);
    cells.retain(|_, v| v.len() as u32 >= min_pts);
    if cells.is_empty() {
        return empty();
    }

    // 3) (Opcional) Mayor componente conexa sobre los vóxeles ocupados (26-vec).
    if isolate {
        let occupied: HashSet<Cell> = cells.keys().copied().collect();
        let mut visited: HashSet<Cell> = HashSet::new();
        let mut best: Vec<Cell> = Vec::new();
        for &start in occupied.iter() {
            if visited.contains(&start) {
                continue;
            }
            let mut comp = Vec::new();
            let mut stack = vec![start];
            visited.insert(start);
            while let Some(c) = stack.pop() {
                comp.push(c);
                for dz in -1..=1 {
                    for dy in -1..=1 {
                        for dx in -1..=1 {
                            let n = (c.0 + dx, c.1 + dy, c.2 + dz);
                            if occupied.contains(&n) && visited.insert(n) {
                                stack.push(n);
                            }
                        }
                    }
                }
            }
            if comp.len() > best.len() {
                best = comp;
            }
        }
        let keep: HashSet<Cell> = best.into_iter().collect();
        cells.retain(|c, _| keep.contains(c));
    }

    // 4) Reunir los puntos conservados.
    let mut points = Vec::with_capacity(cloud.points.len());
    for idxs in cells.values() {
        for &i in idxs {
            points.push(cloud.points[i as usize]);
        }
    }
    PointCloud {
        points,
        has_color: cloud.has_color,
    }
}

/// Submuestrea las posiciones de una nube a una rejilla de vóxeles (un punto
/// representativo por celda, el centroide de los que caen en ella).
pub fn downsample_positions(points: &[Point], voxel: f32) -> Vec<[f32; 3]> {
    let voxel = voxel.max(1e-3);
    let mut acc: HashMap<Cell, ([f64; 3], u32)> = HashMap::new();
    for p in points {
        let e = acc.entry(cell_of([p.x, p.y, p.z], voxel)).or_insert(([0.0; 3], 0));
        e.0[0] += p.x as f64;
        e.0[1] += p.y as f64;
        e.0[2] += p.z as f64;
        e.1 += 1;
    }
    acc.into_values()
        .map(|(s, n)| {
            let n = n as f64;
            [(s[0] / n) as f32, (s[1] / n) as f32, (s[2] / n) as f32]
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Ajuste rígido óptimo (Horn) + eigen-descomposición Jacobi.
// ---------------------------------------------------------------------------

/// Transformación rígida que mejor lleva `src[i]` sobre `dst[i]` (mínimos
/// cuadrados), por el método de cuaterniones de Horn. `src` y `dst` deben tener
/// la misma longitud y ser parejas correspondientes.
fn best_fit_transform(src: &[[f32; 3]], dst: &[[f32; 3]]) -> Transform {
    let n = src.len();
    if n == 0 {
        return Transform::identity();
    }

    // Centroides.
    let mut cs = [0.0f64; 3];
    let mut cd = [0.0f64; 3];
    for i in 0..n {
        for k in 0..3 {
            cs[k] += src[i][k] as f64;
            cd[k] += dst[i][k] as f64;
        }
    }
    for k in 0..3 {
        cs[k] /= n as f64;
        cd[k] /= n as f64;
    }

    // Covarianza cruzada S = Σ (src-cs)(dst-cd)^T.
    let mut s = [[0.0f64; 3]; 3];
    for i in 0..n {
        let a = [
            src[i][0] as f64 - cs[0],
            src[i][1] as f64 - cs[1],
            src[i][2] as f64 - cs[2],
        ];
        let b = [
            dst[i][0] as f64 - cd[0],
            dst[i][1] as f64 - cd[1],
            dst[i][2] as f64 - cd[2],
        ];
        for r in 0..3 {
            for c in 0..3 {
                s[r][c] += a[r] * b[c];
            }
        }
    }

    // Matriz simétrica N de Horn (4×4); su mayor autovector es el cuaternión.
    let (sxx, sxy, sxz) = (s[0][0], s[0][1], s[0][2]);
    let (syx, syy, syz) = (s[1][0], s[1][1], s[1][2]);
    let (szx, szy, szz) = (s[2][0], s[2][1], s[2][2]);
    let nm = [
        [sxx + syy + szz, syz - szy, szx - sxz, sxy - syx],
        [syz - szy, sxx - syy - szz, sxy + syx, szx + sxz],
        [szx - sxz, sxy + syx, -sxx + syy - szz, syz + szy],
        [sxy - syx, szx + sxz, syz + szy, -sxx - syy + szz],
    ];

    let (eig, vec) = jacobi_eigen(nm);
    // Autovector (columna) del mayor autovalor.
    let mut jmax = 0;
    for j in 1..4 {
        if eig[j] > eig[jmax] {
            jmax = j;
        }
    }
    let mut q = [vec[0][jmax], vec[1][jmax], vec[2][jmax], vec[3][jmax]]; // [w,x,y,z]
    let norm = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if norm < 1e-12 {
        return Transform::identity();
    }
    for v in &mut q {
        *v /= norm;
    }

    let r = quat_to_rot(q);
    // t = cd - R·cs.
    let rcs = [
        r[0] as f64 * cs[0] + r[1] as f64 * cs[1] + r[2] as f64 * cs[2],
        r[3] as f64 * cs[0] + r[4] as f64 * cs[1] + r[5] as f64 * cs[2],
        r[6] as f64 * cs[0] + r[7] as f64 * cs[1] + r[8] as f64 * cs[2],
    ];
    Transform {
        r,
        t: [
            (cd[0] - rcs[0]) as f32,
            (cd[1] - rcs[1]) as f32,
            (cd[2] - rcs[2]) as f32,
        ],
    }
}

/// Cuaternión normalizado `[w,x,y,z]` → matriz de rotación fila-mayor.
#[allow(dead_code)]
fn quat_to_rot(q: [f64; 4]) -> [f32; 9] {
    let (w, x, y, z) = (q[0], q[1], q[2], q[3]);
    [
        (1.0 - 2.0 * (y * y + z * z)) as f32,
        (2.0 * (x * y - w * z)) as f32,
        (2.0 * (x * z + w * y)) as f32,
        (2.0 * (x * y + w * z)) as f32,
        (1.0 - 2.0 * (x * x + z * z)) as f32,
        (2.0 * (y * z - w * x)) as f32,
        (2.0 * (x * z - w * y)) as f32,
        (2.0 * (y * z + w * x)) as f32,
        (1.0 - 2.0 * (x * x + y * y)) as f32,
    ]
}

/// Eigen-descomposición de una matriz simétrica N×N por rotaciones de Jacobi
/// cíclicas. Devuelve (autovalores, autovectores en columnas).
#[allow(dead_code)]
fn jacobi_eigen<const N: usize>(mut a: [[f64; N]; N]) -> ([f64; N], [[f64; N]; N]) {
    let mut v = [[0.0f64; N]; N];
    for i in 0..N {
        v[i][i] = 1.0;
    }

    for _ in 0..100 {
        let mut off = 0.0;
        for p in 0..N {
            for q in (p + 1)..N {
                off += a[p][q] * a[p][q];
            }
        }
        if off < 1e-24 {
            break;
        }
        for p in 0..N {
            for q in (p + 1)..N {
                if a[p][q].abs() < 1e-300 {
                    continue;
                }
                let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
                let t = if theta == 0.0 {
                    1.0
                } else {
                    theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt())
                };
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;

                let app = a[p][p];
                let aqq = a[q][q];
                let apq = a[p][q];
                a[p][p] = c * c * app - 2.0 * s * c * apq + s * s * aqq;
                a[q][q] = s * s * app + 2.0 * s * c * apq + c * c * aqq;
                a[p][q] = 0.0;
                a[q][p] = 0.0;
                for i in 0..N {
                    if i != p && i != q {
                        let aip = a[i][p];
                        let aiq = a[i][q];
                        a[i][p] = c * aip - s * aiq;
                        a[p][i] = a[i][p];
                        a[i][q] = s * aip + c * aiq;
                        a[q][i] = a[i][q];
                    }
                }
                for i in 0..N {
                    let vip = v[i][p];
                    let viq = v[i][q];
                    v[i][p] = c * vip - s * viq;
                    v[i][q] = s * vip + c * viq;
                }
            }
        }
    }

    (std::array::from_fn(|i| a[i][i]), v)
}

// ---------------------------------------------------------------------------
// ICP punto-a-punto.
// ---------------------------------------------------------------------------

/// Resultado de una alineación ICP.
#[derive(Debug, Clone, Copy)]
pub struct IcpResult {
    pub transform: Transform,
    pub rmse: f32,
    pub correspondences: usize,
    /// Iteraciones realizadas (informativo / depuración).
    #[allow(dead_code)]
    pub iterations: usize,
}

/// Alinea `src` (posiciones) contra `target` (índice ya construido) partiendo
/// de la pose `init`. Devuelve la pose global del frame nuevo, o `None` si no
/// hay suficientes correspondencias.
///
/// Punto-a-punto (Horn). Se conserva como referencia y para tests; el escaneo
/// usa `icp_point_to_plane`.
#[allow(dead_code)]
pub fn icp(
    src: &[[f32; 3]],
    target: &VoxelIndex,
    init: Transform,
    max_iter: usize,
    max_dist: f32,
    min_corr: usize,
) -> Option<IcpResult> {
    let mut t = init;
    let mut rmse = f32::INFINITY;
    let mut corr = 0;
    let mut iters = 0;

    // Trimmed ICP: en cada iteración descartamos el 30% de correspondencias más
    // lejanas (probables emparejamientos erróneos). Esto reduce mucho el
    // "resbalón" tangencial que emborrona la nube al girar.
    const KEEP_FRAC: f32 = 0.7;

    let mut pairs: Vec<([f32; 3], [f32; 3], f32)> = Vec::with_capacity(src.len());
    let mut sp: Vec<[f32; 3]> = Vec::with_capacity(src.len());
    let mut dp: Vec<[f32; 3]> = Vec::with_capacity(src.len());

    for it in 0..max_iter {
        iters = it + 1;
        pairs.clear();
        for &p in src {
            let tp = t.apply(p);
            if let Some((idx, d2)) = target.nearest(tp, max_dist) {
                pairs.push((p, target.point(idx), d2));
            }
        }
        if pairs.len() < min_corr {
            return None;
        }
        // Conservar las correspondencias más cercanas.
        pairs.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
        let keep = ((pairs.len() as f32 * KEEP_FRAC) as usize).max(min_corr);
        let keep = keep.min(pairs.len());

        sp.clear();
        dp.clear();
        let mut err = 0.0f64;
        for &(s, d, d2) in &pairs[..keep] {
            sp.push(s);
            dp.push(d);
            err += d2 as f64;
        }
        corr = keep;
        let new_rmse = (err / corr as f64).sqrt() as f32;
        t = best_fit_transform(&sp, &dp);

        // Convergencia: mejora de RMSE por debajo de un umbral.
        if (rmse - new_rmse).abs() < 1e-3 {
            rmse = new_rmse;
            break;
        }
        rmse = new_rmse;
    }

    Some(IcpResult {
        transform: t,
        rmse,
        correspondences: corr,
        iterations: iters,
    })
}

// ---------------------------------------------------------------------------
// ICP punto-a-plano (mejor convergencia: no resbala tangencialmente).
// ---------------------------------------------------------------------------

/// Normales por punto estimadas por PCA sobre los vecinos en `radius`. El signo
/// no importa para punto-a-plano (residuo al cuadrado), así que no se orientan.
pub fn estimate_normals(points: &[[f32; 3]], radius: f32) -> Vec<[f32; 3]> {
    if points.is_empty() {
        return Vec::new();
    }
    let idx = VoxelIndex::build(points.to_vec(), radius);
    let mut normals = Vec::with_capacity(points.len());
    for &p in points {
        let nb = idx.neighbors(p, radius);
        if nb.len() < 4 {
            normals.push([0.0, 0.0, 1.0]);
            continue;
        }
        let mut c = [0.0f64; 3];
        for &j in &nb {
            let q = points[j];
            for d in 0..3 {
                c[d] += q[d] as f64;
            }
        }
        let n = nb.len() as f64;
        for d in 0..3 {
            c[d] /= n;
        }
        let mut cov = [[0.0f64; 3]; 3];
        for &j in &nb {
            let q = points[j];
            let v = [q[0] as f64 - c[0], q[1] as f64 - c[1], q[2] as f64 - c[2]];
            for a in 0..3 {
                for b in 0..3 {
                    cov[a][b] += v[a] * v[b];
                }
            }
        }
        normals.push(smallest_eigvec_sym3(cov));
    }
    normals
}

/// Radio mínimo de búsqueda de parejas en las últimas iteraciones (mm)
const MIN_PAIR_DIST: f32 = 3.0;

/// Fracción del peso total por debajo de la cual un autovalor del sistema
/// normalizado se considera sin restricción (ver [`solve_constrained`])
const MIN_CONSTRAINT: f64 = 0.005;

/// Alinea `src` contra `target` (con `normals` por punto del target) minimizando
/// la distancia punto-a-plano (linealización de ángulo pequeño, sistema 6×6).
pub fn icp_point_to_plane(
    src: &[[f32; 3]],
    target: &VoxelIndex,
    normals: &[[f32; 3]],
    init: Transform,
    max_iter: usize,
    max_dist: f32,
    min_corr: usize,
) -> Option<IcpResult> {
    let mut t = init;
    let mut rmse = f32::INFINITY;
    let mut corr = 0;
    let mut iters = 0;
    const KEEP_FRAC: f32 = 0.8;
    // Compatibilidad de normales: rechaza correspondencias entre superficies
    // cuyas normales difieran más de ~60° (|cos| < 0.5). Empareja solo puntos
    // de la misma cara, evitando que el frente case con un lateral —principal
    // fuente de deriva al girar el objeto—. Se usa el valor absoluto porque las
    // normales del modelo no están orientadas (PCA da signo arbitrario).
    const NORMAL_COS_MIN: f32 = 0.5;

    // Normales del src en su propio frame (se rotan con la pose en cada iter).
    let src_normals = estimate_normals(src, max_dist);

    // (p_transformado, q_target, n_target, b_residuo, d2)
    let mut pairs: Vec<([f32; 3], [f32; 3], [f32; 3], f32, f32)> = Vec::with_capacity(src.len());

    for it in 0..max_iter {
        iters = it + 1;
        pairs.clear();
        // Radio de búsqueda de parejas que se achica en cada iteración (de
        // `max_dist` a 3 mm): al principio atrapa movimientos grandes y, ya
        // cerca, descarta parejas falsas que tiran la pose hacia un lado
        let dist = (max_dist * 0.8f32.powi(it as i32)).max(MIN_PAIR_DIST.min(max_dist));
        for (i, &s) in src.iter().enumerate() {
            let p = t.apply(s);
            if let Some((idx, d2)) = target.nearest(p, dist) {
                let n = normals[idx];
                // Normal del src rotada al frame global y comparada con la del target.
                let ns = t.apply_vec(src_normals[i]);
                let dot = ns[0] * n[0] + ns[1] * n[1] + ns[2] * n[2];
                if dot.abs() < NORMAL_COS_MIN {
                    continue;
                }
                let q = target.point(idx);
                let b = (q[0] - p[0]) * n[0] + (q[1] - p[1]) * n[1] + (q[2] - p[2]) * n[2];
                pairs.push((p, q, n, b, d2));
            }
        }
        if pairs.len() < min_corr {
            return None;
        }
        pairs.sort_by(|a, b| a.4.partial_cmp(&b.4).unwrap_or(std::cmp::Ordering::Equal));
        let keep = (((pairs.len() as f32 * KEEP_FRAC) as usize).max(min_corr)).min(pairs.len());

        // Escala robusta: mediana del residuo absoluto |b| sobre las parejas
        // conservadas → umbral de Huber δ = 1.345·σ (acotado para no anularse).
        let mut absb: Vec<f32> = pairs[..keep].iter().map(|p| p.3.abs()).collect();
        absb.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let sigma = absb[absb.len() / 2];
        let delta = (1.345 * sigma).max(0.5); // mm

        // Giro linealizado alrededor del centroide de las parejas, no del origen
        // (la cámara): con el objeto a ~30 cm, un giro pequeño alrededor de la
        // cámara mueve el objeto casi igual que una traslación, las columnas del
        // sistema quedan casi paralelas y el ruido se convierte en saltos de
        // varios mm (sobre todo en vertical, donde las paredes no restringen)
        let mut center = [0.0f32; 3];
        for &(p, ..) in &pairs[..keep] {
            for d in 0..3 {
                center[d] += p[d];
            }
        }
        for c in &mut center {
            *c /= keep as f32;
        }

        // Sistema normal 6×6 ponderado: A^T W A x = A^T W b, con
        // A_i = [(p−c)×n, n], b_i=(q-p)·n, y peso de Huber w_i.
        // Las columnas de giro se escalan por el radio del objeto para que
        // giro y traslación tengan unidades comparables al medir la degeneración
        let radius = (pairs[..keep]
            .iter()
            .map(|&(p, ..)| (0..3).map(|d| (p[d] - center[d]).powi(2)).sum::<f32>())
            .sum::<f32>()
            / keep as f32)
            .sqrt()
            .max(1.0);
        let mut ata = [[0.0f64; 6]; 6];
        let mut atb = [0.0f64; 6];
        let mut wsum = 0.0f64;
        let mut sum_d2 = 0.0f64;
        for &(p, _q, n, b, d2) in &pairs[..keep] {
            let p = [
                (p[0] - center[0]) / radius,
                (p[1] - center[1]) / radius,
                (p[2] - center[2]) / radius,
            ];
            let c = [
                p[1] * n[2] - p[2] * n[1],
                p[2] * n[0] - p[0] * n[2],
                p[0] * n[1] - p[1] * n[0],
            ];
            let row = [
                c[0] as f64,
                c[1] as f64,
                c[2] as f64,
                n[0] as f64,
                n[1] as f64,
                n[2] as f64,
            ];
            // Peso de Huber: 1 para residuos pequeños, δ/|b| para los grandes.
            let ab = b.abs();
            let w = if ab <= delta { 1.0 } else { (delta / ab) as f64 };
            let b = b as f64;
            for r in 0..6 {
                for cc in 0..6 {
                    ata[r][cc] += w * row[r] * row[cc];
                }
                atb[r] += w * row[r] * b;
            }
            wsum += w;
            sum_d2 += d2 as f64;
        }
        corr = keep;
        rmse = (sum_d2 / corr as f64).sqrt() as f32;

        let Some(x) = solve_constrained(ata, atb, MIN_CONSTRAINT * wsum) else {
            // Ninguna dirección restringida (casi sin datos): para.
            break;
        };
        let x = [x[0] / radius as f64, x[1] / radius as f64, x[2] / radius as f64, x[3], x[4], x[5]];
        // Giro alrededor de `center`: x ↦ R(x−c) + c + t
        let mut inc = transform_from_increment(&x);
        let rc = inc.apply_vec(center);
        for d in 0..3 {
            inc.t[d] += center[d] - rc[d];
        }
        t = compose(&inc, &t);

        // Convergencia: incremento pequeño (rad + mm).
        let step =
            (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt() + (x[3].abs() + x[4].abs() + x[5].abs());
        if step < 1e-4 {
            break;
        }
    }

    Some(IcpResult {
        transform: t,
        rmse,
        correspondences: corr,
        iterations: iters,
    })
}

/// Resuelve `ata·x = atb` solo en las direcciones que los datos restringen:
/// descompone `ata` en autovectores y descarta los de autovalor menor que
/// `min_eig` (el movimiento en esas direcciones queda en cero, o sea, se
/// conserva la pose anterior). Sin esto, en un objeto que no fija alguna
/// dirección (paredes verticales, un cilindro, un plano) el ruido desliza la
/// pose por ella y el error se acumula cuadro a cuadro
fn solve_constrained(ata: [[f64; 6]; 6], atb: [f64; 6], min_eig: f64) -> Option<[f64; 6]> {
    let (eig, vec) = jacobi_eigen(ata);
    let mut x = [0.0f64; 6];
    let mut any = false;
    for k in 0..6 {
        if eig[k] <= min_eig {
            continue;
        }
        any = true;
        let proj: f64 = (0..6).map(|i| vec[i][k] * atb[i]).sum::<f64>() / eig[k];
        for i in 0..6 {
            x[i] += proj * vec[i][k];
        }
    }
    any.then_some(x)
}

/// Transformación incremental desde un vector [αx,αy,αz, tx,ty,tz] (Rodrigues).
fn transform_from_increment(x: &[f64; 6]) -> Transform {
    let w = [x[0], x[1], x[2]];
    let theta = (w[0] * w[0] + w[1] * w[1] + w[2] * w[2]).sqrt();
    let r = if theta < 1e-9 {
        [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]
    } else {
        let a = [w[0] / theta, w[1] / theta, w[2] / theta];
        let (s, c) = theta.sin_cos();
        let v = 1.0 - c;
        [
            (c + a[0] * a[0] * v) as f32,
            (a[0] * a[1] * v - a[2] * s) as f32,
            (a[0] * a[2] * v + a[1] * s) as f32,
            (a[1] * a[0] * v + a[2] * s) as f32,
            (c + a[1] * a[1] * v) as f32,
            (a[1] * a[2] * v - a[0] * s) as f32,
            (a[2] * a[0] * v - a[1] * s) as f32,
            (a[2] * a[1] * v + a[0] * s) as f32,
            (c + a[2] * a[2] * v) as f32,
        ]
    };
    Transform {
        r,
        t: [x[3] as f32, x[4] as f32, x[5] as f32],
    }
}

/// Composición `a ∘ b` (aplica `b`, luego `a`).
fn compose(a: &Transform, b: &Transform) -> Transform {
    let m = |x: &[f32; 9], y: &[f32; 9]| {
        let mut o = [0.0f32; 9];
        for i in 0..3 {
            for j in 0..3 {
                o[i * 3 + j] = x[i * 3] * y[j] + x[i * 3 + 1] * y[3 + j] + x[i * 3 + 2] * y[6 + j];
            }
        }
        o
    };
    let r = m(&a.r, &b.r);
    let t = [
        a.r[0] * b.t[0] + a.r[1] * b.t[1] + a.r[2] * b.t[2] + a.t[0],
        a.r[3] * b.t[0] + a.r[4] * b.t[1] + a.r[5] * b.t[2] + a.t[1],
        a.r[6] * b.t[0] + a.r[7] * b.t[1] + a.r[8] * b.t[2] + a.t[2],
    ];
    Transform { r, t }
}

/// Autovector del menor autovalor de una matriz simétrica 3×3 (Jacobi).
pub(crate) fn smallest_eigvec_sym3(mut a: [[f64; 3]; 3]) -> [f32; 3] {
    let mut v = [[0.0f64; 3]; 3];
    for i in 0..3 {
        v[i][i] = 1.0;
    }
    for _ in 0..50 {
        // Mayor off-diagonal.
        let mut p = 0;
        let mut q = 1;
        let mut max = 0.0;
        for i in 0..3 {
            for j in (i + 1)..3 {
                if a[i][j].abs() > max {
                    max = a[i][j].abs();
                    p = i;
                    q = j;
                }
            }
        }
        if max < 1e-12 {
            break;
        }
        let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
        let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
        let t = if theta == 0.0 { 1.0 } else { t };
        let c = 1.0 / (t * t + 1.0).sqrt();
        let s = t * c;
        let app = a[p][p];
        let aqq = a[q][q];
        let apq = a[p][q];
        a[p][p] = c * c * app - 2.0 * s * c * apq + s * s * aqq;
        a[q][q] = s * s * app + 2.0 * s * c * apq + c * c * aqq;
        a[p][q] = 0.0;
        a[q][p] = 0.0;
        for i in 0..3 {
            if i != p && i != q {
                let aip = a[i][p];
                let aiq = a[i][q];
                a[i][p] = c * aip - s * aiq;
                a[p][i] = a[i][p];
                a[i][q] = s * aip + c * aiq;
                a[q][i] = a[i][q];
            }
        }
        for i in 0..3 {
            let vip = v[i][p];
            let viq = v[i][q];
            v[i][p] = c * vip - s * viq;
            v[i][q] = s * vip + c * viq;
        }
    }
    let eig = [a[0][0], a[1][1], a[2][2]];
    let mut jmin = 0;
    for j in 1..3 {
        if eig[j] < eig[jmin] {
            jmin = j;
        }
    }
    let n = [v[0][jmin] as f32, v[1][jmin] as f32, v[2][jmin] as f32];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len > 1e-9 {
        [n[0] / len, n[1] / len, n[2] / len]
    } else {
        [0.0, 0.0, 1.0]
    }
}

// ---------------------------------------------------------------------------
// Fusión incremental por rejilla de vóxeles.
// ---------------------------------------------------------------------------

/// Peso de confianza por punto ≈ 1/z² (z = profundidad en coords de cámara, mm).
/// En un sensor de luz estructurada el ruido crece con la distancia, así que las
/// lecturas cercanas son más fiables y deben pesar más al fusionar. Devuelve 0
/// para profundidades no válidas (descarta el punto en la fusión).
fn confidence_weights(points: &[Point]) -> Vec<f32> {
    points
        .iter()
        .map(|p| if p.z > 1.0 { 1.0 / (p.z * p.z) } else { 0.0 })
        .collect()
}

struct Accum {
    pos: [f64; 3],
    col: [f64; 3],
    /// Suma de las direcciones hacia la cámara de cada observación
    view: [f64; 3],
    w: f64,
    n: u32,
}

/// Acumulador de puntos por vóxel: cada celda guarda el promedio PONDERADO de
/// posición y color de todas las observaciones que han caído en ella. El peso
/// es la "confianza" de cada lectura (≈1/z²): las muestras cercanas, menos
/// ruidosas en un sensor de luz estructurada, pesan más que las lejanas.
pub struct FusionGrid {
    voxel: f32,
    cells: HashMap<Cell, Accum>,
    has_color: bool,
}

impl FusionGrid {
    pub fn new(voxel: f32) -> Self {
        FusionGrid {
            voxel: voxel.max(1e-3),
            cells: HashMap::new(),
            has_color: false,
        }
    }

    /// Integra puntos ya expresados en coordenadas globales, con un peso de
    /// confianza por punto (`weights[i]`). Si `weights` es más corto que
    /// `points`, los puntos sin peso usan 1.0 (sin ponderar).
    pub fn integrate(&mut self, points: &[Point], weights: &[f32], has_color: bool) {
        if has_color {
            self.has_color = true;
        }
        for (i, p) in points.iter().enumerate() {
            let w = weights.get(i).copied().unwrap_or(1.0).max(0.0) as f64;
            if w <= 0.0 {
                continue;
            }
            let e = self
                .cells
                .entry(cell_of([p.x, p.y, p.z], self.voxel))
                .or_insert(Accum {
                    pos: [0.0; 3],
                    col: [0.0; 3],
                    view: [0.0; 3],
                    w: 0.0,
                    n: 0,
                });
            e.pos[0] += w * p.x as f64;
            e.pos[1] += w * p.y as f64;
            e.pos[2] += w * p.z as f64;
            e.col[0] += w * p.rgb[0] as f64;
            e.col[1] += w * p.rgb[1] as f64;
            e.col[2] += w * p.rgb[2] as f64;
            for d in 0..3 {
                e.view[d] += p.view[d] as f64;
            }
            e.w += w;
            e.n += 1;
        }
    }

    pub fn len(&self) -> usize {
        self.cells.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Materializa la nube fusionada (centroide y color medio por vóxel).
    pub fn to_cloud(&self) -> PointCloud {
        let mut points = Vec::with_capacity(self.cells.len());
        for a in self.cells.values() {
            // Promedio ponderado por confianza (peso total acumulado).
            let w = if a.w > 0.0 { a.w } else { a.n.max(1) as f64 };
            points.push(Point {
                x: (a.pos[0] / w) as f32,
                y: (a.pos[1] / w) as f32,
                z: (a.pos[2] / w) as f32,
                rgb: [
                    (a.col[0] / w) as u8,
                    (a.col[1] / w) as u8,
                    (a.col[2] / w) as u8,
                ],
                view: normalized(a.view),
            });
        }
        PointCloud {
            points,
            has_color: self.has_color,
        }
    }
}

/// Vector unitario; cero si no hay dirección
fn normalized(v: [f64; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < 1e-9 {
        return [0.0; 3];
    }
    [(v[0] / len) as f32, (v[1] / len) as f32, (v[2] / len) as f32]
}

// ---------------------------------------------------------------------------
// Sesión de escaneo: orquesta ICP + fusión frame a frame.
// ---------------------------------------------------------------------------

/// Estadísticas de la última integración, para mostrarlas en la UI.
#[derive(Debug, Clone, Copy, Default)]
pub struct ScanStats {
    pub frames: u32,
    pub registered: u32,
    /// Frames descartados por mala alineación (no fusionados).
    pub dropped: u32,
    pub last_rmse: f32,
    pub last_corr: usize,
}

/// Sesión de escaneo multi-frame con registro ICP **frame-a-modelo** y fusión.
///
/// Cada frame nuevo se alinea contra el MODELO acumulado (toda la superficie
/// fusionada hasta ahora), no solo contra el frame anterior. Eso reduce mucho
/// la deriva y es lo que permite girar el objeto y que las vistas encajen, como
/// en los escáneres reales.
pub struct ScanSession {
    fusion: FusionGrid,
    /// Modelo acumulado submuestreado (coords globales): objetivo de ICP.
    model: Vec<[f32; 3]>,
    /// Normales del modelo (para ICP punto-a-plano), alineadas con `model`.
    model_normals: Vec<[f32; 3]>,
    /// Pose global del último frame (cámara → mundo).
    pose: Transform,
    icp_voxel: f32,
    max_dist: f32,
    pub stats: ScanStats,
}

impl Default for ScanSession {
    fn default() -> Self {
        Self::new()
    }
}

impl ScanSession {
    pub fn new() -> Self {
        // Tamaños pensados para la serie POP (profundidad en mm, objetos a
        // ~15-40 cm): fusión fina, ICP algo más grueso por rendimiento.
        ScanSession {
            fusion: FusionGrid::new(1.5),
            model: Vec::new(),
            model_normals: Vec::new(),
            pose: Transform::identity(),
            icp_voxel: 3.0,
            max_dist: 10.0,
            stats: ScanStats::default(),
        }
    }

    /// Reconstruye el modelo de ICP (submuestreo del acumulado) y sus normales.
    fn rebuild_model(&mut self) {
        self.model = downsample_positions(&self.fusion.to_cloud().points, self.icp_voxel);
        self.model_normals = estimate_normals(&self.model, self.max_dist);
    }

    /// Pose del último cuadro sumado (cámara → sistema del primer cuadro)
    pub fn pose(&self) -> Transform {
        self.pose
    }

    pub fn point_count(&self) -> usize {
        self.fusion.len()
    }

    pub fn fused_cloud(&self) -> PointCloud {
        self.fusion.to_cloud()
    }

    /// Integra un frame nuevo. Devuelve `true` si se registró y fusionó.
    pub fn integrate_frame(&mut self, cloud: &PointCloud) -> bool {
        if cloud.points.is_empty() {
            return false;
        }
        self.stats.frames += 1;

        let src = downsample_positions(&cloud.points, self.icp_voxel);
        if src.len() < 10 {
            return false;
        }

        if self.model.is_empty() {
            // Primer frame: define el sistema de coordenadas global.
            self.fuse(cloud, Transform::identity());
            self.stats.last_corr = self.model.len();
            self.stats.last_rmse = 0.0;
            return true;
        }

        // Alinear el frame nuevo contra el MODELO acumulado (frame-a-modelo),
        // por ICP punto-a-plano (no resbala tangencialmente como punto-a-punto).
        let Some(res) = self.align(cloud, self.pose) else {
            // No se pudo alinear: descartamos el frame, mantenemos la pose.
            self.stats.dropped += 1;
            return false;
        };

        // Solo descartar alineaciones ABSURDAS (catástrofe), no las "regulares":
        // con datos reales el RMSE correcto es de varios mm, y ser estricto
        // congelaba el modelo (rechazaba todo). Mejor acumular y avisar en
        // verde/ámbar/rojo que no crecer.
        const CATASTROPHIC_RMSE: f32 = 15.0; // mm
        const MIN_FUSE_CORR: usize = 25;
        if res.correspondences < MIN_FUSE_CORR || res.rmse > CATASTROPHIC_RMSE {
            self.stats.dropped += 1;
            self.stats.last_rmse = res.rmse;
            self.stats.last_corr = res.correspondences;
            return false; // mantiene la última pose buena y reintenta
        }
        self.fuse(cloud, res.transform);
        self.stats.last_rmse = res.rmse;
        self.stats.last_corr = res.correspondences;
        true
    }

    /// Alinea un cuadro (coordenadas de cámara) con el modelo acumulado,
    /// partiendo de la pose `init`. `None` si no hay modelo o no hay parejas
    pub fn align(&self, cloud: &PointCloud, init: Transform) -> Option<IcpResult> {
        if self.model.is_empty() {
            return None;
        }
        let src = downsample_positions(&cloud.points, self.icp_voxel);
        let target = VoxelIndex::build(self.model.clone(), self.max_dist);
        icp_point_to_plane(&src, &target, &self.model_normals, init, 40, self.max_dist, 20)
    }

    /// Suma un cuadro al modelo con la pose dada (cámara → global)
    pub fn fuse(&mut self, cloud: &PointCloud, pose: Transform) {
        self.pose = pose;
        // El peso de confianza se calcula con la profundidad en coords de
        // CÁMARA (antes de transformar), alineado por índice con `global`.
        let weights = confidence_weights(&cloud.points);
        let global: Vec<Point> = cloud
            .points
            .iter()
            .map(|p| {
                let g = pose.apply([p.x, p.y, p.z]);
                Point { x: g[0], y: g[1], z: g[2], rgb: p.rgb, view: pose.apply_vec(p.view) }
            })
            .collect();
        self.fusion.integrate(&global, &weights, cloud.has_color);
        self.rebuild_model();
        self.stats.registered += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pts(coords: &[[f32; 3]]) -> Vec<Point> {
        coords
            .iter()
            .map(|c| Point {
                x: c[0],
                y: c[1],
                z: c[2],
                rgb: [128, 128, 128],
                view: [0.0; 3],
            })
            .collect()
    }

    #[test]
    fn transform_identity_is_noop() {
        let t = Transform::identity();
        let p = [1.0, -2.0, 3.5];
        let q = t.apply(p);
        assert_eq!(p, q);
    }

    #[test]
    fn best_fit_recovers_known_motion() {
        // Rotación de 30° alrededor de Z + traslación.
        let ang: f32 = 30f32.to_radians();
        let (s, c) = ang.sin_cos();
        let rot = |p: [f32; 3]| {
            [
                c * p[0] - s * p[1] + 5.0,
                s * p[0] + c * p[1] - 2.0,
                p[2] + 1.0,
            ]
        };
        let src = [
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [0.0, 8.0, 0.0],
            [0.0, 0.0, 6.0],
            [3.0, 4.0, 5.0],
        ];
        let dst: Vec<[f32; 3]> = src.iter().map(|&p| rot(p)).collect();

        let t = best_fit_transform(&src, &dst);
        for &p in &src {
            let got = t.apply(p);
            let want = rot(p);
            for k in 0..3 {
                assert!((got[k] - want[k]).abs() < 1e-3, "k={k} got={got:?} want={want:?}");
            }
        }
    }

    #[test]
    fn icp_aligns_translated_cloud() {
        // Nube con relieve (no plana) para que el registro sea único.
        let mut base = Vec::new();
        for i in 0..10 {
            for j in 0..10 {
                let x = i as f32 * 2.0;
                let y = j as f32 * 2.0;
                let z = 50.0 + ((i * j) % 7) as f32; // relieve irregular
                base.push([x, y, z]);
            }
        }
        // Movemos la nube fuente una traslación conocida pequeña.
        let shift = [1.5f32, -1.0, 0.8];
        let target_pts: Vec<[f32; 3]> = base.clone();
        let src: Vec<[f32; 3]> = base
            .iter()
            .map(|p| [p[0] - shift[0], p[1] - shift[1], p[2] - shift[2]])
            .collect();

        let target = VoxelIndex::build(target_pts, 10.0);
        let res = icp(&src, &target, Transform::identity(), 50, 10.0, 10).unwrap();

        // La pose debe deshacer el shift: src+shift ≈ target.
        for k in 0..3 {
            assert!(
                (res.transform.t[k] - shift[k]).abs() < 0.2,
                "t[{k}]={} esperado≈{}",
                res.transform.t[k],
                shift[k]
            );
        }
        assert!(res.rmse < 0.5, "rmse={}", res.rmse);
    }

    #[test]
    fn point_to_plane_aligns_curved_surface() {
        // Superficie con relieve curvo (normales variadas) → punto-a-plano
        // bien condicionado.
        let mut base = Vec::new();
        for i in 0..14 {
            for j in 0..14 {
                let x = i as f32 * 3.0;
                let y = j as f32 * 3.0;
                let z = 60.0 + 5.0 * ((i as f32 * 0.4).sin() + (j as f32 * 0.4).cos());
                base.push([x, y, z]);
            }
        }
        let normals = estimate_normals(&base, 10.0);
        let target = VoxelIndex::build(base.clone(), 10.0);
        let shift = [1.5f32, -1.0, 0.7];
        let src: Vec<[f32; 3]> = base
            .iter()
            .map(|p| [p[0] - shift[0], p[1] - shift[1], p[2] - shift[2]])
            .collect();
        let res =
            icp_point_to_plane(&src, &target, &normals, Transform::identity(), 60, 10.0, 10).unwrap();
        for k in 0..3 {
            assert!(
                (res.transform.t[k] - shift[k]).abs() < 0.4,
                "t[{k}]={} esperado≈{}",
                res.transform.t[k],
                shift[k]
            );
        }
    }

    #[test]
    fn point_to_plane_robust_to_outliers() {
        // Superficie curva con un ~16% de puntos atípicos (saltos en Z). El
        // rechazo por normales + la ponderación de Huber deben recuperar el
        // desplazamiento con la misma precisión que sin atípicos.
        let mut base = Vec::new();
        for i in 0..16 {
            for j in 0..16 {
                let x = i as f32 * 3.0;
                let y = j as f32 * 3.0;
                let z = 60.0 + 5.0 * ((i as f32 * 0.4).sin() + (j as f32 * 0.4).cos());
                base.push([x, y, z]);
            }
        }
        let normals = estimate_normals(&base, 10.0);
        let target = VoxelIndex::build(base.clone(), 10.0);
        let shift = [1.2f32, -0.8, 0.6];
        let mut src: Vec<[f32; 3]> = base
            .iter()
            .map(|p| [p[0] - shift[0], p[1] - shift[1], p[2] - shift[2]])
            .collect();
        // Inyectar atípicos: empujar 1 de cada 6 puntos fuertemente en Z.
        for k in (0..src.len()).step_by(6) {
            src[k][2] += 7.0;
        }
        let res =
            icp_point_to_plane(&src, &target, &normals, Transform::identity(), 60, 10.0, 10).unwrap();
        for k in 0..3 {
            assert!(
                (res.transform.t[k] - shift[k]).abs() < 0.5,
                "t[{k}]={} esperado≈{}",
                res.transform.t[k],
                shift[k]
            );
        }
    }

    #[test]
    fn voxel_index_finds_nearest() {
        let idx = VoxelIndex::build(vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]], 5.0);
        let (i, d2) = idx.nearest([0.5, 0.0, 0.0], 5.0).unwrap();
        assert_eq!(i, 0);
        assert!((d2 - 0.25).abs() < 1e-5);
        assert!(idx.nearest([100.0, 100.0, 100.0], 5.0).is_none());
    }

    #[test]
    fn fusion_merges_repeated_observations() {
        let mut grid = FusionGrid::new(2.0);
        // Tres observaciones del mismo punto (misma celda) → un solo vóxel.
        grid.integrate(&pts(&[[0.1, 0.1, 0.1], [0.2, 0.0, 0.3], [0.0, 0.2, 0.1]]), &[], false);
        grid.integrate(&pts(&[[100.0, 100.0, 100.0]]), &[], false);
        assert_eq!(grid.len(), 2);
        let cloud = grid.to_cloud();
        assert_eq!(cloud.points.len(), 2);
    }

    #[test]
    fn fusion_weights_pull_toward_confident() {
        // Dos observaciones en la misma celda con distinta confianza: el
        // promedio ponderado se acerca a la de mayor peso.
        let mut grid = FusionGrid::new(100.0); // celda grande → mismo vóxel
        grid.integrate(&pts(&[[0.0, 0.0, 0.0]]), &[3.0], false);
        grid.integrate(&pts(&[[10.0, 0.0, 0.0]]), &[1.0], false);
        let c = grid.to_cloud();
        assert_eq!(c.points.len(), 1);
        // (3·0 + 1·10) / (3+1) = 2.5
        assert!((c.points[0].x - 2.5).abs() < 1e-4, "x={}", c.points[0].x);
    }

    #[test]
    fn scan_session_first_frame_is_identity() {
        // Rejilla amplia (puntos a 5 mm, > icp_voxel) para sobrevivir al
        // submuestreo y superar el mínimo de puntos del primer frame.
        let mut coords = Vec::new();
        for i in 0..8 {
            for j in 0..8 {
                coords.push([i as f32 * 5.0, j as f32 * 5.0, 50.0 + (i + j) as f32]);
            }
        }
        let mut scan = ScanSession::new();
        let cloud = PointCloud {
            points: pts(&coords),
            has_color: false,
        };
        assert!(scan.integrate_frame(&cloud));
        assert_eq!(scan.stats.frames, 1);
        assert_eq!(scan.stats.registered, 1);
        assert!(scan.point_count() > 0);
    }

    #[test]
    fn scan_session_registers_second_frame() {
        // Dos frames del mismo objeto con un pequeño desplazamiento conocido:
        // el segundo debe registrarse contra el primero.
        let mut coords = Vec::new();
        for i in 0..12 {
            for j in 0..12 {
                let z = 60.0 + ((i * 3 + j * 2) % 11) as f32; // relieve
                coords.push([i as f32 * 4.0, j as f32 * 4.0, z]);
            }
        }
        let shift = [2.0f32, -1.5, 1.0];
        let moved: Vec<[f32; 3]> = coords
            .iter()
            .map(|p| [p[0] - shift[0], p[1] - shift[1], p[2] - shift[2]])
            .collect();

        let mut scan = ScanSession::new();
        assert!(scan.integrate_frame(&PointCloud {
            points: pts(&coords),
            has_color: false,
        }));
        assert!(scan.integrate_frame(&PointCloud {
            points: pts(&moved),
            has_color: false,
        }));
        assert_eq!(scan.stats.registered, 2);
        // El registro debe recuperar aproximadamente el desplazamiento.
        for k in 0..3 {
            assert!(
                (scan.pose.t[k] - shift[k]).abs() < 0.5,
                "t[{k}]={} esperado≈{}",
                scan.pose.t[k],
                shift[k]
            );
        }
    }
}
