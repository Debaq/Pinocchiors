// Curvas del sketch que no son líneas ni círculos: B-splines por polos
// (racionales: también cónicas y parábolas), arcos de elipse, ajuste de una
// B-spline a puntos, conversión exacta de la spline por puntos a polos,
// inserción de nudos y curvatura (peine, inflexiones, radio mínimo).
// Funciones puras: se prueban con node (e2e/sketchCurves.test.mjs). La misma
// cuenta que `cad_solver::bspline` en Rust.

import type { P2 } from "./cad.ts";

export interface BSplineData {
  poles: P2[];
  /** Vacío = no racional */
  weights?: number[];
  degree: number;
  closed?: boolean;
  /** Vector completo; vacío = uniforme (sujeta si es abierta) */
  knots?: number[];
}

/** B-spline lista para evaluar (las cerradas con los polos repetidos al final) */
export interface BSpline {
  poles: P2[];
  weights: number[];
  knots: number[];
  degree: number;
}

const sub = (a: P2, b: P2): P2 => [a[0] - b[0], a[1] - b[1]];
const add = (a: P2, b: P2): P2 => [a[0] + b[0], a[1] + b[1]];
const mul = (a: P2, k: number): P2 => [a[0] * k, a[1] * k];
const len = (v: P2) => Math.hypot(v[0], v[1]);
const cross = (a: P2, b: P2) => a[0] * b[1] - a[1] * b[0];
const dot = (a: P2, b: P2) => a[0] * b[0] + a[1] * b[1];

/** Nudos uniformes sujetos para `n` polos de grado `p` */
export function clampedUniform(n: number, p: number): number[] {
  const spans = n - p;
  const k: number[] = Array(p + 1).fill(0);
  for (let i = 1; i < spans; i++) k.push(i);
  for (let i = 0; i <= p; i++) k.push(spans);
  return k;
}

export function makeBSpline(d: BSplineData): BSpline | undefined {
  const n = d.poles.length;
  if (n < 2) return undefined;
  let w = d.weights && d.weights.length === n ? [...d.weights] : Array(n).fill(1);
  if (d.closed) {
    const p = Math.min(Math.max(d.degree, 1), Math.max(n, 2) - 1);
    const poles = [...d.poles];
    for (let i = 0; i < p; i++) {
      poles.push(poles[i]);
      w.push(w[i]);
    }
    const m = poles.length + p + 1;
    return { poles, weights: w, knots: Array.from({ length: m }, (_, i) => i), degree: p };
  }
  const p = Math.min(Math.max(d.degree, 1), n - 1);
  let knots = d.knots ?? [];
  if (!knots.length) knots = clampedUniform(n, p);
  else if (knots.length !== n + p + 1) return undefined;
  w = w.slice(0, n);
  return { poles: [...d.poles], weights: w, knots: [...knots], degree: p };
}

export function domain(s: BSpline): [number, number] {
  return [s.knots[s.degree], s.knots[s.poles.length]];
}

function span(s: BSpline, u: number): number {
  const [n, p] = [s.poles.length - 1, s.degree];
  if (u >= s.knots[n + 1]) return n;
  if (u <= s.knots[p]) return p;
  let [lo, hi] = [p, n + 1];
  let mid = (lo + hi) >> 1;
  while (u < s.knots[mid] || u >= s.knots[mid + 1]) {
    if (u < s.knots[mid]) hi = mid;
    else lo = mid;
    mid = (lo + hi) >> 1;
  }
  return mid;
}

/** Funciones base y sus dos primeras derivadas (A2.3 de The NURBS Book) */
function basisDers(s: BSpline, i: number, u: number): number[][] {
  const p = s.degree;
  const k = s.knots;
  const ndu = Array.from({ length: p + 1 }, () => Array(p + 1).fill(0));
  const left = Array(p + 1).fill(0);
  const right = Array(p + 1).fill(0);
  ndu[0][0] = 1;
  for (let j = 1; j <= p; j++) {
    left[j] = u - k[i + 1 - j];
    right[j] = k[i + j] - u;
    let saved = 0;
    for (let r = 0; r < j; r++) {
      ndu[j][r] = right[r + 1] + left[j - r];
      const temp = ndu[j][r] === 0 ? 0 : ndu[r][j - 1] / ndu[j][r];
      ndu[r][j] = saved + right[r + 1] * temp;
      saved = left[j - r] * temp;
    }
    ndu[j][j] = saved;
  }
  const nd = Math.min(2, p);
  const ders = Array.from({ length: 3 }, () => Array(p + 1).fill(0));
  for (let j = 0; j <= p; j++) ders[0][j] = ndu[j][p];
  const a = [Array(p + 1).fill(0), Array(p + 1).fill(0)];
  for (let r = 0; r <= p; r++) {
    let [s1, s2] = [0, 1];
    a[0][0] = 1;
    for (let kk = 1; kk <= nd; kk++) {
      let d = 0;
      const rk = r - kk;
      const pk = p - kk;
      if (r >= kk) {
        a[s2][0] = ndu[pk + 1][rk] === 0 ? 0 : a[s1][0] / ndu[pk + 1][rk];
        d = a[s2][0] * ndu[rk][pk];
      }
      const j1 = rk >= -1 ? 1 : -rk;
      const j2 = r - 1 <= pk ? kk - 1 : p - r;
      for (let j = j1; j <= j2; j++) {
        a[s2][j] = ndu[pk + 1][rk + j] === 0 ? 0 : (a[s1][j] - a[s1][j - 1]) / ndu[pk + 1][rk + j];
        d += a[s2][j] * ndu[rk + j][pk];
      }
      if (r <= pk) {
        a[s2][kk] = ndu[pk + 1][r] === 0 ? 0 : -a[s1][kk - 1] / ndu[pk + 1][r];
        d += a[s2][kk] * ndu[r][pk];
      }
      ders[kk][r] = d;
      [s1, s2] = [s2, s1];
    }
  }
  let f = p;
  for (let kk = 1; kk <= nd; kk++) {
    for (let j = 0; j <= p; j++) ders[kk][j] *= f;
    f *= p - kk;
  }
  return ders;
}

/** Punto, primera y segunda derivada en `u` */
export function derivs(s: BSpline, u: number): [P2, P2, P2] {
  const [a, b] = domain(s);
  u = Math.min(Math.max(u, a), b);
  const i = span(s, u);
  const n = basisDers(s, i, u);
  const p = s.degree;
  let [aw, a1, a2]: P2[] = [
    [0, 0],
    [0, 0],
    [0, 0],
  ];
  let [w0, w1, w2] = [0, 0, 0];
  for (let j = 0; j <= p; j++) {
    const k = i - p + j;
    const [pt, w] = [s.poles[k], s.weights[k]];
    aw = add(aw, mul(pt, n[0][j] * w));
    a1 = add(a1, mul(pt, n[1][j] * w));
    a2 = add(a2, mul(pt, n[2][j] * w));
    w0 += n[0][j] * w;
    w1 += n[1][j] * w;
    w2 += n[2][j] * w;
  }
  const c = mul(aw, 1 / w0);
  const d1 = mul(sub(a1, mul(c, w1)), 1 / w0);
  const d2 = mul(sub(sub(a2, mul(d1, 2 * w1)), mul(c, w2)), 1 / w0);
  return [c, d1, d2];
}

export const evalAt = (s: BSpline, u: number): P2 => derivs(s, u)[0];

/** Curvatura con signo (positiva si dobla a la izquierda del avance) */
export function curvature(s: BSpline, u: number): number {
  const [, d1, d2] = derivs(s, u);
  const l = len(d1);
  return l < 1e-15 ? 0 : cross(d1, d2) / (l * l * l);
}

/** Bordes de los tramos dentro del dominio */
export function breaks(s: BSpline): number[] {
  const [a, b] = domain(s);
  const out = [a];
  for (const k of s.knots) if (k > a && k < b && k > out[out.length - 1]) out.push(k);
  out.push(b);
  return out;
}

/** Parámetros de muestreo: `perSpan` por tramo, con los extremos */
export function sampleParams(s: BSpline, perSpan = 16): number[] {
  const br = breaks(s);
  const out: number[] = [];
  for (let i = 0; i + 1 < br.length; i++) for (let k = 0; k < perSpan; k++) out.push(br[i] + ((br[i + 1] - br[i]) * k) / perSpan);
  out.push(br[br.length - 1]);
  return out;
}

export const sample = (s: BSpline, perSpan = 16): P2[] => sampleParams(s, perSpan).map((u) => evalAt(s, u));

/** Parámetro del punto de la curva más cercano a `p` */
export function closestParam(s: BSpline, p: P2): number {
  const us = sampleParams(s, 24);
  let best = 0;
  let bestD = Infinity;
  us.forEach((u, i) => {
    const d = len(sub(evalAt(s, u), p));
    if (d < bestD) [best, bestD] = [i, d];
  });
  let lo = us[Math.max(0, best - 1)];
  let hi = us[Math.min(us.length - 1, best + 1)];
  const g = 0.5 * (Math.sqrt(5) - 1);
  for (let k = 0; k < 50; k++) {
    const [m1, m2] = [hi - g * (hi - lo), lo + g * (hi - lo)];
    if (len(sub(evalAt(s, m1), p)) < len(sub(evalAt(s, m2), p))) hi = m2;
    else lo = m1;
  }
  return (lo + hi) / 2;
}

// ─── Arco de elipse ───────────────────────────────────────────────────────

/** Ángulo paramétrico de `p` en la elipse de centro `c` y semiejes `u`, `v` (vectores) */
export function ellipseParam(c: P2, u: P2, v: P2, p: P2): number {
  const q = sub(p, c);
  return Math.atan2(dot(q, v) / dot(v, v), dot(q, u) / dot(u, u));
}

/** Arco de elipse antihorario de `start` a `end` (los dos proyectados sobre la elipse) */
export function ellipseArcPolyline(c: P2, major: P2, minor: P2, start: P2, end: P2, n = 48): P2[] {
  const [u, v] = [sub(major, c), sub(minor, c)];
  // El sentido antihorario del plano, sea cual sea la orientación de u y v
  const sense = cross(u, v) >= 0 ? 1 : -1;
  const t0 = ellipseParam(c, u, v, start);
  let sweep = (ellipseParam(c, u, v, end) - t0) * sense;
  while (sweep <= 1e-12) sweep += 2 * Math.PI;
  return Array.from({ length: n + 1 }, (_, i) => {
    const t = t0 + sense * ((sweep * i) / n);
    return add(c, add(mul(u, Math.cos(t)), mul(v, Math.sin(t))));
  });
}

// ─── Cónicas ──────────────────────────────────────────────────────────────

/** Peso del polo del medio de una cónica por su factor rho (0,5 = parábola) */
export const rhoWeight = (rho: number) => rho / (1 - rho);
export const weightRho = (w: number) => w / (1 + w);

// ─── Ajuste y conversión ──────────────────────────────────────────────────

/** Resuelve A x = b (A cuadrada) por eliminación con pivote; devuelve undefined si es singular */
function solveLinear(A: number[][], B: number[][]): number[][] | undefined {
  const n = A.length;
  const m = A.map((r, i) => [...r, ...B[i]]);
  const cols = B[0].length;
  for (let c = 0; c < n; c++) {
    let piv = c;
    for (let r = c + 1; r < n; r++) if (Math.abs(m[r][c]) > Math.abs(m[piv][c])) piv = r;
    if (Math.abs(m[piv][c]) < 1e-14) return undefined;
    [m[c], m[piv]] = [m[piv], m[c]];
    for (let r = 0; r < n; r++) {
      if (r === c) continue;
      const f = m[r][c] / m[c][c];
      for (let k = c; k < n + cols; k++) m[r][k] -= f * m[c][k];
    }
  }
  return m.map((r, i) => r.slice(n).map((x) => x / m[i][i]));
}

/** Parámetros por largo de cuerda, de 0 al último nudo */
function chordParams(pts: P2[], total: number): number[] {
  const d = [0];
  for (let i = 1; i < pts.length; i++) d.push(d[i - 1] + len(sub(pts[i], pts[i - 1])));
  const L = d[d.length - 1] || 1;
  return d.map((x) => (x / L) * total);
}

/**
 * B-spline abierta (grado `degree`, nudos uniformes sujetos) que pasa por el
 * primer y el último punto y se acerca a los demás por mínimos cuadrados; con
 * la menor cantidad de polos que deja todo a menos de `tol`.
 */
export function fitBSpline(pts: P2[], tol: number, degree = 3): { poles: P2[]; error: number } | undefined {
  if (pts.length < 2) return undefined;
  const p = Math.min(degree, pts.length - 1);
  let last: { poles: P2[]; error: number } | undefined;
  for (let n = p + 1; n <= Math.max(p + 1, pts.length); n++) {
    const fit = leastSquares(pts, n, p);
    if (!fit) continue;
    last = fit;
    if (fit.error <= tol) return fit;
  }
  return last;
}

function leastSquares(pts: P2[], n: number, p: number): { poles: P2[]; error: number } | undefined {
  const knots = clampedUniform(n, p);
  const us = chordParams(pts, knots[knots.length - 1]);
  const probe: BSpline = { poles: Array(n).fill([0, 0]), weights: Array(n).fill(1), knots, degree: p };
  // Matriz de funciones base en cada parámetro
  const N = us.map((u) => {
    const row = Array(n).fill(0);
    const i = span(probe, u);
    const b = basisDers(probe, i, Math.min(u, knots[knots.length - 1]))[0];
    for (let j = 0; j <= p; j++) row[i - p + j] = b[j];
    return row;
  });
  const [P0, Pn] = [pts[0], pts[pts.length - 1]];
  let poles: P2[];
  if (n === 2) poles = [P0, Pn];
  else {
    // Incógnitas: los polos del medio; los extremos quedan en el primer y el último punto
    const inner = n - 2;
    const A = Array.from({ length: inner }, () => Array(inner).fill(0));
    const B = Array.from({ length: inner }, () => [0, 0]);
    for (let k = 1; k < pts.length - 1; k++) {
      const r = N[k];
      const rhs = sub(sub(pts[k], mul(P0, r[0])), mul(Pn, r[n - 1]));
      for (let a = 0; a < inner; a++) {
        if (!r[a + 1]) continue;
        B[a][0] += r[a + 1] * rhs[0];
        B[a][1] += r[a + 1] * rhs[1];
        for (let b = 0; b < inner; b++) A[a][b] += r[a + 1] * r[b + 1];
      }
    }
    const x = solveLinear(A, B);
    if (!x) return undefined;
    poles = [P0, ...x.map((q) => [q[0], q[1]] as P2), Pn];
  }
  const s: BSpline = { poles, weights: Array(n).fill(1), knots, degree: p };
  // Error: distancia de cada punto a la curva
  const error = Math.max(0, ...pts.map((q) => len(sub(evalAt(s, closestParam(s, q)), q))));
  return { poles, error };
}

/**
 * Polos y nudos de la curva de Hermite por `pts` con tangentes `m` (la spline
 * por puntos del sketch): cada tramo es una Bézier cúbica (P, P + m/3,
 * Q − m'/3, Q); los nudos interiores van triples (bordes de las Bézier: la
 * curva sigue siendo C¹ por cómo se arman las tangentes).
 */
export function hermiteToBSpline(pts: P2[], m: P2[], closed: boolean): { poles: P2[]; knots: number[] } {
  const n = pts.length;
  const segs = closed ? n : n - 1;
  const poles: P2[] = [pts[0]];
  for (let i = 0; i < segs; i++) {
    const [a, b] = [pts[i], pts[(i + 1) % n]];
    const [ma, mb] = [m[i], m[(i + 1) % n]];
    poles.push(add(a, mul(ma, 1 / 3)), sub(b, mul(mb, 1 / 3)), b);
  }
  const knots = [0, 0, 0, 0];
  for (let i = 1; i < segs; i++) knots.push(i, i, i);
  knots.push(segs, segs, segs, segs);
  return { poles, knots };
}

/**
 * Inserta el nudo `u` una vez (algoritmo de Boehm, en coordenadas
 * homogéneas): la curva no cambia y queda un polo más. Cambian los polos
 * `span − grado + 1 … span`; los de antes siguen iguales y los de después se
 * corren uno.
 */
export function insertKnot(d: { poles: P2[]; weights?: number[]; degree: number; knots: number[] }, u: number): { poles: P2[]; weights?: number[]; knots: number[]; span: number } {
  const s = makeBSpline({ ...d, closed: false })!;
  const p = s.degree;
  const k = span(s, u);
  const H = s.poles.map((q, i) => [q[0] * s.weights[i], q[1] * s.weights[i], s.weights[i]]);
  const out: number[][] = [];
  for (let i = 0; i <= s.poles.length; i++) {
    if (i <= k - p) out.push(H[i]);
    else if (i > k) out.push(H[i - 1]);
    else {
      const a = (u - s.knots[i]) / (s.knots[i + p] - s.knots[i]);
      out.push(H[i].map((x, j) => (1 - a) * H[i - 1][j] + a * x));
    }
  }
  const knots = [...s.knots.slice(0, k + 1), u, ...s.knots.slice(k + 1)];
  const poles = out.map((h) => [h[0] / h[2], h[1] / h[2]] as P2);
  const rational = s.weights.some((w) => w !== 1);
  // `span`: los polos k − p + 1 … k son nuevos (antes había p − 1 en su lugar)
  return { poles, ...(rational ? { weights: out.map((h) => h[2]) } : {}), knots, span: k };
}

// ─── Curvatura ────────────────────────────────────────────────────────────

export interface CurvatureInfo {
  /** Dientes del peine: desde la curva hacia el lado cóncavo opuesto, largo ∝ curvatura */
  comb: [P2, P2][];
  /** Puntos donde la curvatura cambia de signo */
  inflections: P2[];
  /** Radio mínimo (Infinity si es recta) y dónde */
  minRadius: number;
  minAt?: P2;
}

/**
 * Peine de curvatura de una B-spline: `scale` = largo en mm de un diente con
 * curvatura máxima (el peine se ve igual a cualquier tamaño).
 */
export function curvatureInfo(s: BSpline, scale: number, perSpan = 24): CurvatureInfo {
  const us = sampleParams(s, perSpan);
  const ks = us.map((u) => curvature(s, u));
  const kmax = Math.max(1e-12, ...ks.map(Math.abs));
  const comb: [P2, P2][] = [];
  const inflections: P2[] = [];
  let minRadius = Infinity;
  let minAt: P2 | undefined;
  /** Último parámetro con curvatura no nula (para ver los cambios de signo) */
  let lastNonZero = -1;
  us.forEach((u, i) => {
    const [p, d1] = derivs(s, u);
    const l = len(d1) || 1;
    const nrm: P2 = [-d1[1] / l, d1[0] / l];
    // Hacia afuera de la curva (opuesto al centro de curvatura), como en los CAD
    comb.push([p, sub(p, mul(nrm, (ks[i] / kmax) * scale))]);
    if (Math.abs(ks[i]) > 1e-12 && 1 / Math.abs(ks[i]) < minRadius) [minRadius, minAt] = [1 / Math.abs(ks[i]), p];
    const eps = 1e-9 * kmax;
    if (Math.abs(ks[i]) <= eps) return;
    if (lastNonZero >= 0 && ks[lastNonZero] * ks[i] < 0) {
      // Afinar por bisección
      let [a, b] = [us[lastNonZero], u];
      for (let k = 0; k < 40; k++) {
        const m = (a + b) / 2;
        if (curvature(s, a) * curvature(s, m) <= 0) b = m;
        else a = m;
      }
      inflections.push(evalAt(s, (a + b) / 2));
    }
    lastNonZero = i;
  });
  return { comb, inflections, minRadius, minAt };
}
