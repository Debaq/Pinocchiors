//! B-splines del sketch (racionales o no): nudos por defecto, evaluación con
//! primera y segunda derivada, curvatura con signo y punto más cercano.
//!
//! Una abierta sin nudos dados es uniforme y sujeta: pasa por el primer y el
//! último polo, con la tangente hacia el polo vecino. Una cerrada es periódica
//! uniforme: se arma repitiendo los primeros `grado` polos al final.
//! Algoritmos de «The NURBS Book» (Piegl y Tiller): A2.1 y A2.3.

use nalgebra::Vector2;

type V = Vector2<f64>;

#[derive(Debug, Clone)]
pub struct BSpline {
    pub poles: Vec<V>,
    pub weights: Vec<f64>,
    pub knots: Vec<f64>,
    pub degree: usize,
}

impl BSpline {
    /// Curva por sus polos. `weights` vacío = no racional; `knots` vacío =
    /// uniforme (sujeta si es abierta). `None` si faltan polos o los nudos
    /// no cuadran con la cantidad de polos.
    pub fn new(poles: &[V], weights: &[f64], degree: usize, closed: bool, knots: &[f64]) -> Option<Self> {
        let n = poles.len();
        if n < 2 {
            return None;
        }
        let w: Vec<f64> = if weights.len() == n { weights.to_vec() } else { vec![1.0; n] };
        if closed {
            let p = degree.clamp(1, n.max(2) - 1);
            let mut poles = poles.to_vec();
            let mut w = w;
            for i in 0..p {
                poles.push(poles[i]);
                w.push(w[i]);
            }
            let m = poles.len() + p + 1;
            return Some(Self { poles, weights: w, knots: (0..m).map(|i| i as f64).collect(), degree: p });
        }
        let p = degree.clamp(1, n - 1);
        let knots = if knots.is_empty() {
            clamped_uniform(n, p)
        } else if knots.len() == n + p + 1 && knots.windows(2).all(|k| k[1] >= k[0]) {
            knots.to_vec()
        } else {
            return None;
        };
        Some(Self { poles: poles.to_vec(), weights: w, knots, degree: p })
    }

    /// Intervalo del parámetro
    pub fn domain(&self) -> (f64, f64) {
        let p = self.degree;
        (self.knots[p], self.knots[self.poles.len()])
    }

    /// Tramo de nudos que contiene `u` (A2.1)
    fn span(&self, u: f64) -> usize {
        let (n, p) = (self.poles.len() - 1, self.degree);
        if u >= self.knots[n + 1] {
            return n;
        }
        if u <= self.knots[p] {
            return p;
        }
        let (mut lo, mut hi) = (p, n + 1);
        let mut mid = (lo + hi) / 2;
        while u < self.knots[mid] || u >= self.knots[mid + 1] {
            if u < self.knots[mid] {
                hi = mid;
            } else {
                lo = mid;
            }
            mid = (lo + hi) / 2;
        }
        mid
    }

    /// Funciones base no nulas y sus dos primeras derivadas en `u` (A2.3)
    fn basis_ders(&self, span: usize, u: f64) -> [Vec<f64>; 3] {
        let p = self.degree;
        let k = &self.knots;
        let mut ndu = vec![vec![0.0; p + 1]; p + 1];
        let (mut left, mut right) = (vec![0.0; p + 1], vec![0.0; p + 1]);
        ndu[0][0] = 1.0;
        for j in 1..=p {
            left[j] = u - k[span + 1 - j];
            right[j] = k[span + j] - u;
            let mut saved = 0.0;
            for r in 0..j {
                ndu[j][r] = right[r + 1] + left[j - r];
                let temp = if ndu[j][r] == 0.0 { 0.0 } else { ndu[r][j - 1] / ndu[j][r] };
                ndu[r][j] = saved + right[r + 1] * temp;
                saved = left[j - r] * temp;
            }
            ndu[j][j] = saved;
        }
        let nd = 2.min(p);
        let mut ders = vec![vec![0.0; p + 1]; 3];
        for j in 0..=p {
            ders[0][j] = ndu[j][p];
        }
        let mut a = vec![vec![0.0; p + 1]; 2];
        for r in 0..=p {
            let (mut s1, mut s2) = (0usize, 1usize);
            a[0][0] = 1.0;
            for kk in 1..=nd {
                let mut d = 0.0;
                let (rk, pk) = (r as isize - kk as isize, p - kk);
                if r >= kk {
                    let rk = rk as usize;
                    a[s2][0] = if ndu[pk + 1][rk] == 0.0 { 0.0 } else { a[s1][0] / ndu[pk + 1][rk] };
                    d = a[s2][0] * ndu[rk][pk];
                }
                let j1 = if rk >= -1 { 1 } else { (-rk) as usize };
                let j2 = if (r as isize - 1) <= pk as isize { kk - 1 } else { p - r };
                for j in j1..=j2 {
                    let idx = (rk + j as isize) as usize;
                    a[s2][j] = if ndu[pk + 1][idx] == 0.0 { 0.0 } else { (a[s1][j] - a[s1][j - 1]) / ndu[pk + 1][idx] };
                    d += a[s2][j] * ndu[idx][pk];
                }
                if r as isize <= pk as isize {
                    a[s2][kk] = if ndu[pk + 1][r] == 0.0 { 0.0 } else { -a[s1][kk - 1] / ndu[pk + 1][r] };
                    d += a[s2][kk] * ndu[r][pk];
                }
                ders[kk][r] = d;
                std::mem::swap(&mut s1, &mut s2);
            }
        }
        let mut f = p as f64;
        for kk in 1..=nd {
            for j in 0..=p {
                ders[kk][j] *= f;
            }
            f *= (p - kk) as f64;
        }
        [ders[0].clone(), ders[1].clone(), ders[2].clone()]
    }

    /// Punto, primera y segunda derivada en `u`
    pub fn derivs(&self, u: f64) -> (V, V, V) {
        let (a, b) = self.domain();
        let u = u.clamp(a, b);
        let span = self.span(u);
        let n = self.basis_ders(span, u);
        let p = self.degree;
        let (mut aw, mut a1, mut a2) = (V::zeros(), V::zeros(), V::zeros());
        let (mut w0, mut w1, mut w2) = (0.0, 0.0, 0.0);
        for j in 0..=p {
            let i = span - p + j;
            let (pt, w) = (self.poles[i], self.weights[i]);
            aw += pt * (n[0][j] * w);
            a1 += pt * (n[1][j] * w);
            a2 += pt * (n[2][j] * w);
            w0 += n[0][j] * w;
            w1 += n[1][j] * w;
            w2 += n[2][j] * w;
        }
        let c = aw / w0;
        let d1 = (a1 - c * w1) / w0;
        let d2 = (a2 - d1 * (2.0 * w1) - c * w2) / w0;
        (c, d1, d2)
    }

    pub fn eval(&self, u: f64) -> V {
        self.derivs(u).0
    }

    /// Curvatura con signo (positiva si dobla a la izquierda del avance)
    pub fn curvature(&self, u: f64) -> f64 {
        let (_, d1, d2) = self.derivs(u);
        let l = d1.norm();
        if l < 1e-15 {
            return 0.0;
        }
        (d1.x * d2.y - d1.y * d2.x) / (l * l * l)
    }

    /// Parámetros de los nudos distintos dentro del dominio (bordes de tramo)
    pub fn breaks(&self) -> Vec<f64> {
        let (a, b) = self.domain();
        let mut out: Vec<f64> = vec![a];
        for &k in &self.knots {
            if k > a && k < b && out.last().is_some_and(|&l| k > l) {
                out.push(k);
            }
        }
        out.push(b);
        out
    }

    /// Muestras con `per_span` puntos por tramo (incluye los extremos)
    pub fn sample(&self, per_span: usize) -> Vec<V> {
        let br = self.breaks();
        let mut out = Vec::new();
        for w in br.windows(2) {
            for i in 0..per_span {
                out.push(self.eval(w[0] + (w[1] - w[0]) * i as f64 / per_span as f64));
            }
        }
        out.push(self.eval(*br.last().unwrap()));
        out
    }

    /// Parámetro del punto de la curva más cercano a `p`
    pub fn closest_param(&self, p: V) -> f64 {
        let br = self.breaks();
        const STEPS: usize = 24;
        let (mut best, mut best_d) = (br[0], f64::INFINITY);
        let mut step = 0.0;
        for w in br.windows(2) {
            let h = (w[1] - w[0]) / STEPS as f64;
            for i in 0..=STEPS {
                let u = w[0] + h * i as f64;
                let d = (self.eval(u) - p).norm_squared();
                if d < best_d {
                    (best, best_d, step) = (u, d, h);
                }
            }
        }
        let (a, b) = self.domain();
        let (mut lo, mut hi) = ((best - step).max(a), (best + step).min(b));
        let g = 0.5 * (5f64.sqrt() - 1.0);
        for _ in 0..50 {
            let (m1, m2) = (hi - g * (hi - lo), lo + g * (hi - lo));
            if (self.eval(m1) - p).norm_squared() < (self.eval(m2) - p).norm_squared() {
                hi = m2;
            } else {
                lo = m1;
            }
        }
        0.5 * (lo + hi)
    }
}

/// Nudos uniformes sujetos: grado+1 ceros, interiores 1, 2, … y grado+1 al final
pub fn clamped_uniform(n: usize, p: usize) -> Vec<f64> {
    let inner = n - p; // tramos
    let mut k = vec![0.0; p + 1];
    for i in 1..inner {
        k.push(i as f64);
    }
    k.extend(std::iter::repeat_n(inner as f64, p + 1));
    k
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f64, y: f64) -> V {
        V::new(x, y)
    }

    #[test]
    fn clamped_cubic_hits_end_poles_and_tangents() {
        let poles = [v(0.0, 0.0), v(1.0, 2.0), v(3.0, 2.0), v(4.0, 0.0)];
        let s = BSpline::new(&poles, &[], 3, false, &[]).unwrap();
        let (a, b) = s.domain();
        assert!((s.eval(a) - poles[0]).norm() < 1e-12);
        assert!((s.eval(b) - poles[3]).norm() < 1e-12);
        // Bézier cúbica: C'(0) = 3 (P1 − P0)
        let (_, d1, d2) = s.derivs(a);
        assert!((d1 - (poles[1] - poles[0]) * 3.0).norm() < 1e-9);
        // C''(0) = 6 (P0 − 2 P1 + P2)
        assert!((d2 - (poles[0] - poles[1] * 2.0 + poles[2]) * 6.0).norm() < 1e-9);
        // Simétrica: el medio en x = 2
        assert!((s.eval(0.5 * (a + b)).x - 2.0).abs() < 1e-12);
    }

    #[test]
    fn rational_quadratic_is_a_circle() {
        // Cuarto de círculo de radio 1: polos (1,0), (1,1), (0,1), peso del medio √2/2
        let w = std::f64::consts::FRAC_1_SQRT_2;
        let s = BSpline::new(&[v(1.0, 0.0), v(1.0, 1.0), v(0.0, 1.0)], &[1.0, w, 1.0], 2, false, &[]).unwrap();
        let (a, b) = s.domain();
        for i in 0..=10 {
            let u = a + (b - a) * i as f64 / 10.0;
            assert!((s.eval(u).norm() - 1.0).abs() < 1e-12);
            assert!((s.curvature(u) - 1.0).abs() < 1e-9, "curvatura {}", s.curvature(u));
        }
    }

    #[test]
    fn closed_periodic_is_smooth_at_the_seam() {
        let poles = [v(1.0, 0.0), v(0.0, 1.0), v(-1.0, 0.0), v(0.0, -1.0)];
        let s = BSpline::new(&poles, &[], 3, true, &[]).unwrap();
        let (a, b) = s.domain();
        assert!((s.eval(a) - s.eval(b)).norm() < 1e-12);
        let (_, da, dda) = s.derivs(a);
        let (_, db, ddb) = s.derivs(b - 1e-12);
        assert!((da - db).norm() < 1e-6);
        assert!((dda - ddb).norm() < 1e-4);
    }

    #[test]
    fn many_spans_and_closest_point() {
        let poles: Vec<V> = (0..8).map(|i| v(i as f64, if i % 2 == 0 { 0.0 } else { 1.0 })).collect();
        let s = BSpline::new(&poles, &[], 3, false, &[]).unwrap();
        assert_eq!(s.breaks().len(), 6);
        let q = s.eval(2.3);
        let u = s.closest_param(q);
        assert!((s.eval(u) - q).norm() < 1e-7);
        // Nudos dados con la cantidad equivocada: no
        assert!(BSpline::new(&poles, &[], 3, false, &[0.0, 1.0]).is_none());
    }
}
