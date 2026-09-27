//! Campos de orientación (4-RoSy) y de posición (4-PoSy) extrínsecos.
//!
//! Formulación de Jakob et al., *Instant Field-Aligned Meshes* (2015), en la
//! que se basa QuadriFlow:
//!
//! - La orientación en cada vértice es un vector tangente `q`, definido salvo
//!   rotaciones de 90° alrededor de la normal.
//! - La posición es un punto `o` del plano tangente: el origen de un retículo
//!   cuadrado de lado `scale` con ejes `q` y `n × q`. Dos vértices vecinos son
//!   compatibles cuando sus retículos coinciden cerca de ambos.
//!
//! Ambos campos se suavizan con Gauss-Seidel sobre la jerarquía, del nivel
//! más grueso al más fino.

use crate::hierarchy::{Hierarchy, Level, NONE};
use crate::surface::Constraint;
use crate::V3;
use rayon::prelude::*;

/// Representantes de `q0` y `q1` (entre sus 4 rotaciones) más parecidos entre sí.
#[inline]
pub(crate) fn compat_orientation(q0: &V3, n0: &V3, q1: &V3, n1: &V3) -> (V3, V3) {
    let (a, b, _, _) = compat_orientation_index(q0, n0, q1, n1);
    (a, b)
}

/// Como [`compat_orientation`], devolviendo también cuántos cuartos de vuelta
/// (alrededor de su normal) se rotó cada vector: `a = R^k0 q0`, `b = R^k1 q1`,
/// con `R(v) = n × v`.
#[inline]
pub(crate) fn compat_orientation_index(q0: &V3, n0: &V3, q1: &V3, n1: &V3) -> (V3, V3, u8, u8) {
    let a = [*q0, n0.cross(q0)];
    let b = [*q1, n1.cross(q1)];
    let (mut best, mut bi, mut bj) = (-1.0, 0, 0);
    for (i, ai) in a.iter().enumerate() {
        for (j, bj_) in b.iter().enumerate() {
            let score = ai.dot(bj_).abs();
            if score > best {
                (best, bi, bj) = (score, i, j);
            }
        }
    }
    let sign = a[bi].dot(&b[bj]).signum();
    // Invertir el signo es girar media vuelta
    let k1 = bj as u8 + if sign < 0.0 { 2 } else { 0 };
    (a[bi], b[bj] * sign, bi as u8, k1)
}

/// Punto más cercano a `p0` y `p1` que está en ambos planos tangentes.
#[inline]
fn middle_point(p0: &V3, n0: &V3, p1: &V3, n1: &V3) -> V3 {
    let (n0p0, n0p1) = (n0.dot(p0), n0.dot(p1));
    let (n1p0, n1p1) = (n1.dot(p0), n1.dot(p1));
    let n0n1 = n0.dot(n1);
    let denom = 1.0 / (1.0 - n0n1 * n0n1 + 1e-4);
    let l0 = 2.0 * (n0p1 - n0p0 - n0n1 * (n1p0 - n1p1)) * denom;
    let l1 = 2.0 * (n1p0 - n1p1 - n0n1 * (n0p1 - n0p0)) * denom;
    (p0 + p1) * 0.5 - (n0 * l0 + n1 * l1) * 0.25
}

/// Coordenadas enteras (piso) de `p` en el retículo de origen `o`.
#[inline]
fn lattice_floor(o: &V3, q: &V3, n: &V3, p: &V3, inv_scale: f64) -> [i64; 2] {
    let t = n.cross(q);
    let d = p - o;
    [
        (q.dot(&d) * inv_scale).floor() as i64,
        (t.dot(&d) * inv_scale).floor() as i64,
    ]
}

#[inline]
fn lattice_point(o: &V3, q: &V3, n: &V3, idx: [i64; 2], scale: f64) -> V3 {
    o + (q * idx[0] as f64 + n.cross(q) * idx[1] as f64) * scale
}

/// Punto del retículo de origen `o` más cercano a `p`.
#[inline]
pub(crate) fn lattice_round(o: &V3, q: &V3, n: &V3, p: &V3, scale: f64, inv_scale: f64) -> V3 {
    let t = n.cross(q);
    let d = p - o;
    o + q * ((q.dot(&d) * inv_scale).round() * scale) + t * ((t.dot(&d) * inv_scale).round() * scale)
}

/// Índices (en cada retículo) del par de puntos más cercanos entre sí,
/// buscados alrededor del punto medio de los dos vértices.
#[allow(clippy::too_many_arguments)]
#[inline]
pub(crate) fn compat_position_index(
    p0: &V3, n0: &V3, q0: &V3, o0: &V3,
    p1: &V3, n1: &V3, q1: &V3, o1: &V3,
    scale: f64, inv_scale: f64,
) -> ([i64; 2], [i64; 2]) {
    let middle = middle_point(p0, n0, p1, n1);
    let base0 = lattice_floor(o0, q0, n0, &middle, inv_scale);
    let base1 = lattice_floor(o1, q1, n1, &middle, inv_scale);
    let corner = |base: [i64; 2], k: usize| [base[0] + (k & 1) as i64, base[1] + (k >> 1) as i64];

    let mut best = (f64::INFINITY, [0; 2], [0; 2]);
    for i in 0..4 {
        let i0 = corner(base0, i);
        let x0 = lattice_point(o0, q0, n0, i0, scale);
        for j in 0..4 {
            let i1 = corner(base1, j);
            let cost = (x0 - lattice_point(o1, q1, n1, i1, scale)).norm_squared();
            if cost < best.0 {
                best = (cost, i0, i1);
            }
        }
    }
    (best.1, best.2)
}

#[allow(clippy::too_many_arguments)]
#[inline]
fn compat_position(
    p0: &V3, n0: &V3, q0: &V3, o0: &V3,
    p1: &V3, n1: &V3, q1: &V3, o1: &V3,
    scale: f64, inv_scale: f64,
) -> (V3, V3) {
    let (i0, i1) = compat_position_index(p0, n0, q0, o0, p1, n1, q1, o1, scale, inv_scale);
    (lattice_point(o0, q0, n0, i0, scale), lattice_point(o1, q1, n1, i1, scale))
}

/// Proyecta `v` al plano tangente de `n`, normalizado; `None` si queda nulo.
#[inline]
fn tangent(v: &V3, n: &V3) -> Option<V3> {
    (v - n * n.dot(v)).try_normalize(1e-12)
}

fn any_tangent(n: &V3) -> V3 {
    let helper = if n.x.abs() > 0.9 { V3::y() } else { V3::x() };
    n.cross(&helper).normalize()
}

/// Generador determinista (xorshift64*).
struct Rng(u64);

impl Rng {
    fn next_f64(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Dirección impuesta por una restricción de línea, si la hay.
fn constrained_direction(level: &Level, i: usize) -> Option<V3> {
    match level.constraint[i] {
        Constraint::Line { dir, .. } => tangent(&dir, &level.nrm[i]),
        _ => None,
    }
}

/// Un barrido de Gauss-Seidel: dentro de cada fase del coloreo los vértices
/// no son vecinos y se actualizan en paralelo; el resultado no depende del
/// número de hilos.
fn gauss_seidel<F>(level: &Level, values: &mut [V3], update: F)
where
    F: Fn(usize, &[V3]) -> Option<V3> + Sync,
{
    for phase in &level.phases {
        let current: &[V3] = values;
        let updates: Vec<(u32, V3)> = phase
            .par_iter()
            .filter_map(|&i| update(i as usize, current).map(|v| (i, v)))
            .collect();
        for (i, v) in updates {
            values[i as usize] = v;
        }
    }
}

/// Dirección guía por vértice (dirección principal de curvatura) y su peso,
/// relativo a la suma de los pesos de los vecinos.
pub(crate) type Guide = Vec<(V3, f64)>;

fn smooth_orientation(level: &Level, q: &mut [V3], guide: Option<&Guide>) {
    gauss_seidel(level, q, |i, q| {
        if constrained_direction(level, i).is_some() {
            return None;
        }
        let n = level.nrm[i];
        let mut sum = q[i];
        let mut weight = 0.0;
        for &(j, w) in level.neighbors(i) {
            let j = j as usize;
            let (a, b) = compat_orientation(&sum, &n, &q[j], &level.nrm[j]);
            sum = a * weight + b * w;
            weight += w;
            sum = tangent(&sum, &n).unwrap_or(a);
        }
        if let Some(&(dir, w)) = guide.map(|g| &g[i])
            && w > 0.0
            && let Some(dir) = tangent(&dir, &n)
        {
            let (a, b) = compat_orientation(&sum, &n, &dir, &n);
            sum = tangent(&(a * weight + b * (w * weight)), &n).unwrap_or(a);
        }
        Some(sum)
    });
}

/// La guía de curvatura actúa desde este nivel de la jerarquía hacia los más
/// gruesos: fija la estructura global (sin pares de singularidades sobrantes)
/// y deja que el nivel fino se suavice sin el ruido local de la curvatura.
const GUIDE_MIN_LEVEL: usize = 2;

/// Radio (en vueltas de vecinos) del suavizado del tensor de curvatura.
const CURVATURE_SMOOTHING: usize = 3;

/// Guía de curvatura del nivel más fino: dirección principal de mayor
/// curvatura y peso `strength · min(1, |k1 − k2| · scale)` (nulo en planos y
/// esferas, donde no hay dirección preferida).
pub(crate) fn curvature_guide(level: &Level, scale: f64, strength: f64) -> Guide {
    let n = level.len();
    // Operador de forma por vértice, como tensor 3×3 en coordenadas del mundo
    let mut tensor: Vec<[f64; 6]> = (0..n)
        .into_par_iter()
        .map(|i| {
            let (p, nrm) = (level.pos[i], level.nrm[i]);
            let t1 = any_tangent(&nrm);
            let t2 = nrm.cross(&t1);
            // Mínimos cuadrados de S·e = Δn con S = [[a, b], [b, c]]
            let mut m = pinocchio_math::nalgebra::Matrix3::<f64>::zeros();
            let mut r = V3::zeros();
            for &(j, _) in level.neighbors(i).iter().filter(|&&(j, _)| same_side(level, i, j as usize)) {
                let e = level.pos[j as usize] - p;
                let dn = level.nrm[j as usize] - nrm;
                let (ex, ey) = (e.dot(&t1), e.dot(&t2));
                let (dx, dy) = (dn.dot(&t1), dn.dot(&t2));
                let w = 1.0 / e.norm_squared().max(1e-300);
                // Filas: [ex, ey, 0]·(a, b, c) = dx ; [0, ex, ey]·(a, b, c) = dy
                let rows = [(V3::new(ex, ey, 0.0), dx), (V3::new(0.0, ex, ey), dy)];
                for (row, rhs) in rows {
                    m += row * row.transpose() * w;
                    r += row * rhs * w;
                }
            }
            let Some(abc) = m.try_inverse().map(|inv| inv * r) else { return [0.0; 6] };
            let (a, b, c) = (abc.x, abc.y, abc.z);
            // S3 = a t1t1ᵀ + b (t1t2ᵀ + t2t1ᵀ) + c t2t2ᵀ, simétrico: 6 componentes
            let s = t1 * t1.transpose() * a + (t1 * t2.transpose() + t2 * t1.transpose()) * b + t2 * t2.transpose() * c;
            [s[(0, 0)], s[(1, 1)], s[(2, 2)], s[(0, 1)], s[(0, 2)], s[(1, 2)]]
        })
        .collect();
    for _ in 0..CURVATURE_SMOOTHING {
        tensor = (0..n)
            .into_par_iter()
            .map(|i| {
                let mut acc = tensor[i];
                let mut count = 1.0;
                for &(j, _) in level.neighbors(i).iter().filter(|&&(j, _)| same_side(level, i, j as usize)) {
                    for (x, y) in acc.iter_mut().zip(&tensor[j as usize]) {
                        *x += y;
                    }
                    count += 1.0;
                }
                acc.map(|x| x / count)
            })
            .collect();
    }

    (0..n)
        .into_par_iter()
        .map(|i| {
            let nrm = level.nrm[i];
            let t1 = any_tangent(&nrm);
            let t2 = nrm.cross(&t1);
            let [xx, yy, zz, xy, xz, yz] = tensor[i];
            let s = pinocchio_math::nalgebra::Matrix3::new(xx, xy, xz, xy, yy, yz, xz, yz, zz);
            let (a, b, c) = ((s * t1).dot(&t1), (s * t2).dot(&t1), (s * t2).dot(&t2));
            let anisotropy = ((a - c).powi(2) + 4.0 * b * b).sqrt();
            let theta = 0.5 * (2.0 * b).atan2(a - c);
            let dir = t1 * theta.cos() + t2 * theta.sin();
            (dir, strength * (anisotropy * scale).min(1.0))
        })
        .collect()
}

/// Coseno mínimo entre normales vecinas para que cuenten en la curvatura: una
/// arista viva no es curvatura (tiene su propia restricción) y no debe
/// contagiar su dirección a las caras de al lado.
const SAME_SIDE_COS: f64 = std::f64::consts::FRAC_1_SQRT_2;

fn same_side(level: &Level, i: usize, j: usize) -> bool {
    level.nrm[i].dot(&level.nrm[j]) > SAME_SIDE_COS
}

/// Guía de un nivel grueso a partir del fino: suma de las guías de los hijos
/// (alineadas entre sí) ponderadas por área; si discrepan, el peso baja.
fn restrict_guide(fine: &Level, coarse: &Level, guide: &Guide) -> Guide {
    (0..coarse.len())
        .map(|c| {
            let n = coarse.nrm[c];
            let [a, b] = coarse.children[c];
            let (a, b) = (a as usize, b);
            let (ga, wa) = guide[a];
            if b == NONE {
                return (ga, wa);
            }
            let (gb, wb) = guide[b as usize];
            let (xa, xb) = compat_orientation(&ga, &fine.nrm[a], &gb, &fine.nrm[b as usize]);
            let (aa, ab) = (fine.area[a], fine.area[b as usize]);
            let total = xa * (wa * aa) + xb * (wb * ab);
            let area = (aa + ab).max(1e-300);
            (tangent(&total, &n).unwrap_or(ga), total.norm() / area)
        })
        .collect()
}

/// Resuelve la orientación en todos los niveles. Devuelve `q` por nivel; los
/// niveles gruesos se recalculan al final a partir del más fino, para que el
/// campo de posición use orientaciones coherentes en toda la jerarquía.
pub(crate) fn solve_orientation(h: &Hierarchy, iterations: usize, guide: Option<Guide>) -> Vec<Vec<V3>> {
    let levels = &h.levels;
    let guides: Vec<Guide> = match guide {
        Some(g) => {
            let mut out = vec![g];
            for lvl in 1..levels.len() {
                let next = restrict_guide(&levels[lvl - 1], &levels[lvl], &out[lvl - 1]);
                out.push(next);
            }
            out
        }
        None => Vec::new(),
    };
    let mut qs: Vec<Vec<V3>> = levels.iter().map(|l| vec![V3::zeros(); l.len()]).collect();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);

    for lvl in (0..levels.len()).rev() {
        let level = &levels[lvl];
        for i in 0..level.len() {
            let n = level.nrm[i];
            let init = if lvl + 1 == levels.len() {
                let r = V3::new(rng.next_f64(), rng.next_f64(), rng.next_f64()) - V3::repeat(0.5);
                tangent(&r, &n)
            } else {
                tangent(&qs[lvl + 1][level.parent[i] as usize], &n)
            };
            qs[lvl][i] = constrained_direction(level, i)
                .or(init)
                .unwrap_or_else(|| any_tangent(&n));
        }
        for _ in 0..iterations {
            let guide = guides.get(lvl).filter(|_| lvl >= GUIDE_MIN_LEVEL);
            smooth_orientation(level, &mut qs[lvl], guide);
        }
    }

    // Restricción: cada vértice grueso combina las orientaciones de sus hijos
    for lvl in 1..levels.len() {
        let (fine, coarse) = (&levels[lvl - 1], &levels[lvl]);
        for c in 0..coarse.len() {
            let n = coarse.nrm[c];
            let [a, b] = coarse.children[c];
            let (a, b) = (a as usize, b);
            let q = if b == NONE {
                qs[lvl - 1][a]
            } else {
                let b = b as usize;
                let (qa, qb) =
                    compat_orientation(&qs[lvl - 1][a], &fine.nrm[a], &qs[lvl - 1][b], &fine.nrm[b]);
                qa * fine.area[a] + qb * fine.area[b]
            };
            qs[lvl][c] = constrained_direction(coarse, c)
                .or_else(|| tangent(&q, &n))
                .unwrap_or_else(|| any_tangent(&n));
        }
    }
    qs
}

fn smooth_position(level: &Level, q: &[V3], o: &mut [V3], scale: f64, inv_scale: f64) {
    gauss_seidel(level, o, |i, o| {
        let constraint = level.constraint[i];
        if let Constraint::Corner { point } = constraint {
            return Some(point);
        }
        let (v, n, qi) = (level.pos[i], level.nrm[i], q[i]);
        let mut sum = o[i];
        let mut weight = 0.0;
        for &(j, w) in level.neighbors(i) {
            let j = j as usize;
            let (a, b) = compat_position(
                &v, &n, &qi, &sum,
                &level.pos[j], &level.nrm[j], &q[j], &o[j],
                scale, inv_scale,
            );
            sum = a * weight + b * w;
            weight += w;
            sum /= weight;
            sum -= n * n.dot(&(sum - v));
        }
        (weight > 0.0).then(|| {
            constraint.project_position(lattice_round(&sum, &qi, &n, &v, scale, inv_scale))
        })
    });
}

/// Resuelve el campo de posición y devuelve `o` en el nivel más fino.
pub(crate) fn solve_position(
    h: &Hierarchy,
    qs: &[Vec<V3>],
    scale: f64,
    iterations: usize,
) -> Vec<V3> {
    let levels = &h.levels;
    let inv_scale = 1.0 / scale;
    let mut coarser: Vec<V3> = Vec::new();

    for lvl in (0..levels.len()).rev() {
        let level = &levels[lvl];
        let mut o: Vec<V3> = (0..level.len())
            .map(|i| {
                let (v, n) = (level.pos[i], level.nrm[i]);
                let init = if lvl + 1 == levels.len() {
                    v
                } else {
                    let p = coarser[level.parent[i] as usize];
                    p - n * n.dot(&(p - v))
                };
                level.constraint[i].project_position(init)
            })
            .collect();
        for _ in 0..iterations {
            smooth_position(level, &qs[lvl], &mut o, scale, inv_scale);
        }
        coarser = o;
    }
    coarser
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compat_orientation_picks_closest_rotation() {
        let n = V3::z();
        let q0 = V3::x();
        // q1 rotado 90° + 5°: su representante más cercano a x está a 5°
        let angle = (95.0f64).to_radians();
        let q1 = V3::new(angle.cos(), angle.sin(), 0.0);
        let (a, b) = compat_orientation(&q0, &n, &q1, &n);
        assert!((a.dot(&b) - 5f64.to_radians().cos()).abs() < 1e-12);
    }

    #[test]
    fn middle_point_on_flat_surface_is_midpoint() {
        let n = V3::z();
        let m = middle_point(&V3::new(0.0, 0.0, 1.0), &n, &V3::new(2.0, 4.0, 1.0), &n);
        assert!((m - V3::new(1.0, 2.0, 1.0)).norm() < 1e-9);
    }

    #[test]
    fn compatible_lattices_have_consistent_indices() {
        let n = V3::z();
        let (s, inv) = (0.5, 2.0);
        // Mismo retículo, orígenes desplazados una celda en x
        let (o0, o1) = (V3::new(0.1, 0.2, 0.0), V3::new(0.6, 0.2, 0.0));
        let (i0, i1) = compat_position_index(
            &V3::new(0.0, 0.0, 0.0), &n, &V3::x(), &o0,
            &V3::new(0.6, 0.1, 0.0), &n, &V3::x(), &o1,
            s, inv,
        );
        assert_eq!([i0[0] - i1[0], i0[1] - i1[1]], [1, 0]);
    }
}
