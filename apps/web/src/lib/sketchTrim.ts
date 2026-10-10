// Recortar y extender splines (por puntos y por polos) y elipses, pasar una
// spline por polos a puntos de paso, y recortar o extender solo contra lo
// elegido a mano. Las líneas, los círculos y los arcos los recorta `trimAt`
// de cad.ts (que también cortan contra estas curvas).
// Funciones puras: se prueban con node (e2e/sketchTrim.test.mjs).

import {
  addEntity,
  addPoint,
  bsplineOf,
  splineParts,
  splinePolyline,
  splineTangents,
  dropLoosePoints,
  extendAt,
  removeEntity,
  stickOrFix,
  trimAt,
  type Geometry,
  type Hit,
  type P2,
  type Sketch,
  type TrimLimits,
} from "./cad.ts";
import { polylineOf } from "./sketchCheck.ts";
import { breaks, closestParam, domain, ellipseParam, evalAt, hermiteToBSpline, insertKnot, makeBSpline, sample } from "./sketchCurves.ts";

const sub = (a: P2, b: P2): P2 => [a[0] - b[0], a[1] - b[1]];
const cross = (a: P2, b: P2) => a[0] * b[1] - a[1] * b[0];
const dot = (a: P2, b: P2) => a[0] * b[0] + a[1] * b[1];
const TAU = 2 * Math.PI;

/** Curvas que recorta este módulo (lo demás, `trimAt`) */
export const isFreeCurve = (g: Geometry) => g.type === "spline" || g.type === "bspline" || g.type === "ellipse" || g.type === "ellipse_arc";

const pos = (s: Sketch, id: number): P2 => {
  const q = s.points.find((x) => x.id === id)!;
  return [q.x, q.y];
};

const hitKind = (g: Geometry): Hit["kind"] => (g.type === "line" ? "line" : g.type === "circle" || g.type === "arc" ? "circle" : "curve");

/** Un cruce sobre la polilínea de la curva: `u` = índice del tramo + fracción */
interface Cut {
  u: number;
  p: P2;
  hit: Hit;
}

/** Cruces de la polilínea `pl` con las otras entidades (no de construcción) y con los límites de afuera */
function crossings(s: Sketch, id: number, pl: P2[], limits: TrimLimits): Cut[] {
  const out: Cut[] = [];
  const against = (other: P2[], hit: Hit) => {
    for (let i = 0; i + 1 < pl.length; i++) {
      const a = pl[i];
      const d = sub(pl[i + 1], a);
      for (let j = 0; j + 1 < other.length; j++) {
        const c = other[j];
        const f = sub(other[j + 1], c);
        const den = cross(d, f);
        if (Math.abs(den) < 1e-15) continue;
        const t = cross(sub(c, a), f) / den;
        const w = cross(sub(c, a), d) / den;
        if (t >= 0 && t < 1 && w >= -1e-9 && w <= 1 + 1e-9) out.push({ u: i + t, p: [a[0] + t * d[0], a[1] + t * d[1]], hit });
      }
    }
  };
  for (const e of s.entities) if (e.id !== id && !e.construction) against(polylineOf(s, e.geometry), { entity: e.id, kind: hitKind(e.geometry) });
  for (const poly of limits) against(poly, { entity: -1, kind: "line" });
  out.sort((x, y) => x.u - y.u);
  // Un cruce justo en la unión de dos tramos de la otra curva sale dos veces
  return out.filter((c, i) => i === 0 || c.u - out[i - 1].u > 1e-7);
}

/** Parámetro (tramo + fracción) del punto de la polilínea más cercano a `p` */
function paramOf(pl: P2[], p: P2): number {
  let best = { u: 0, d: Infinity };
  for (let i = 0; i + 1 < pl.length; i++) {
    const d = sub(pl[i + 1], pl[i]);
    const t = Math.max(0, Math.min(1, dot(sub(p, pl[i]), d) / (dot(d, d) || 1)));
    const q: P2 = [pl[i][0] + t * d[0], pl[i][1] + t * d[1]];
    const dd = Math.hypot(p[0] - q[0], p[1] - q[1]);
    if (dd < best.d) best = { u: i + t, d: dd };
  }
  return best.u;
}

function pointAt(pl: P2[], u: number): P2 {
  const i = Math.max(0, Math.min(pl.length - 2, Math.floor(u)));
  const t = u - i;
  return [pl[i][0] + t * (pl[i + 1][0] - pl[i][0]), pl[i][1] + t * (pl[i + 1][1] - pl[i][1])];
}

/** Una punta de un tramo que queda: la del original (con su id) o un cruce */
type End = { id: number; u: number } | Cut;

const isCut = (x: End): x is Cut => "hit" in x;

/**
 * Recorta una spline o una elipse en el tramo entre cruces donde cayó `p`
 * (como `trimAt` con las líneas): sin cruces se borra entera; una cerrada
 * necesita dos. Sin cambiar la forma: la elipse pasa a arco de elipse y la
 * spline queda por polos (la por puntos conserva los suyos como uniones;
 * «A puntos de paso» la vuelve a pasar, aproximada).
 */
export function trimFreeCurve(s: Sketch, id: number, p: P2, limits: TrimLimits = []): string | undefined {
  const e = s.entities.find((x) => x.id === id);
  if (!e || !isFreeCurve(e.geometry)) return "Solo splines y elipses";
  const g = e.geometry;
  const pl = polylineOf(s, g);
  if (pl.length < 2) return "No se pudo leer la curva";
  const closed = g.type === "ellipse" || ((g.type === "spline" || g.type === "bspline") && !!g.closed);
  const last = pl.length - 1;
  // En una abierta, lo que toca sus puntas no corta (son sus uniones)
  const cuts = crossings(s, id, pl, limits).filter((c) => closed || (c.u > 1e-6 && c.u < last - 1e-6));
  if (!cuts.length) {
    removeEntity(s, id);
    return undefined;
  }
  const uc = paramOf(pl, p);
  let before = [...cuts].reverse().find((c) => c.u < uc);
  let after = cuts.find((c) => c.u > uc);
  // Los tramos que quedan, como pares de puntas en el sentido de la curva
  const pieces: [End, End][] = [];
  if (closed) {
    if (cuts.length < 2) return "Una curva cerrada necesita al menos dos cruces para recortarla";
    before ??= cuts[cuts.length - 1];
    after ??= cuts[0];
    pieces.push([after, before]);
  } else {
    const [first, end] = curveEnds(g);
    if (before) pieces.push([{ id: first, u: 0 }, before]);
    if (after) pieces.push([after, { id: end, u: last }]);
  }
  const ids = pieces.map(([a, b]) => buildPiece(s, g, pl, closed, a, b));
  // El primer tramo se queda con la entidad (y lo que la nombra); los otros, nuevas
  e.geometry = ids[0];
  for (const ng of ids.slice(1)) {
    const nid = addEntity(s, ng);
    if (e.construction) s.entities.find((x) => x.id === nid)!.construction = true;
  }
  // Cada corte, pegado a lo que lo cortó
  for (const [a, b] of pieces)
    for (const x of [a, b]) if (isCut(x)) stickOrFix(s, (x as Cut & { point: number }).point, x.hit);
  dropLoosePoints(s);
  return undefined;
}

function curveEnds(g: Geometry): [number, number] {
  if (g.type === "spline") return [g.points[0], g.points[g.points.length - 1]];
  if (g.type === "bspline") return [g.poles[0], g.poles[g.poles.length - 1]];
  if (g.type === "ellipse_arc") return [g.start, g.end];
  return [-1, -1];
}

/** La geometría de un tramo de `g` entre dos puntas (los cortes reciben su punto en `point`) */
function buildPiece(s: Sketch, g: Geometry, pl: P2[], closed: boolean, a: End, b: End): Geometry {
  const idOf = (x: End) => {
    if (!isCut(x)) return x.id;
    const c = x as Cut & { point?: number };
    return (c.point ??= addPoint(s, c.p));
  };
  const last = pl.length - 1;
  // Recorrido de a hasta b en el sentido de la curva (una cerrada puede dar la vuelta)
  const span = b.u - a.u + (closed && b.u <= a.u ? last : 0);
  const wrap = (u: number) => (closed && u > last ? u - last : u);
  if (g.type === "ellipse" || g.type === "ellipse_arc") {
    const [c, u] = [pos(s, g.center), sub(pos(s, g.major), pos(s, g.center))];
    const v = sub(pos(s, g.minor), c);
    // La polilínea de una elipse entera va de u a v: si eso es horario, el arco (antihorario) va al revés
    const ccw = g.type === "ellipse_arc" || cross(u, v) >= 0;
    const [from, to] = ccw ? [idOf(a), idOf(b)] : [idOf(b), idOf(a)];
    return { type: "ellipse_arc", center: g.center, major: g.major, minor: g.minor, start: from, end: to };
  }
  // Spline: la curva exacta como B-spline (la por puntos, con sus tramos de
  // Hermite como Bézier), partida en los cortes insertando nudos
  const curve = asBSpline(s, g);
  if (!curve) return sampledPiece(s, g, pl, a, b, idOf, wrap, span);
  const param = (x: End) => (isCut(x) ? (g.type === "spline" ? x.u / 16 : closestParam(makeBSpline(curve)!, x.p)) : x.u === 0 ? curve.knots[0] : curve.knots[curve.knots.length - 1]);
  const [u0, u1] = [param(a), param(b)];
  let part: Curve;
  if (u1 > u0) part = between(curve, u0, u1);
  else {
    // Una cerrada que da la vuelta: del corte al final y del comienzo al otro corte
    const [k0, k1] = [curve.knots[0], curve.knots[curve.knots.length - 1]];
    part = join(between(curve, u0, k1), between(curve, k0, u1));
  }
  // Los polos que caen en un punto del sketch de la curva vieja lo reusan (uniones de la por puntos)
  const old = g.type === "spline" ? g.points : g.type === "bspline" ? g.poles : [];
  const ids = part.poles.map((q, i) => {
    if (i === 0) return idOf(a);
    if (i === part.poles.length - 1) return idOf(b);
    return old.find((o) => near(pos(s, o), q)) ?? addPoint(s, q);
  });
  return { type: "bspline", poles: ids, degree: part.degree, knots: part.knots.map((k) => k - part.knots[0]), ...(part.weights ? { weights: part.weights } : {}) };
}

/** B-spline sujeta con su vector de nudos completo */
type Curve = { poles: P2[]; degree: number; knots: number[]; weights?: number[] };

const near = (a: P2, b: P2) => Math.hypot(a[0] - b[0], a[1] - b[1]) < 1e-9;

/** La spline (por puntos o por polos abierta) como B-spline sujeta con su vector de nudos completo */
function asBSpline(s: Sketch, g: Geometry): Curve | undefined {
  if (g.type === "spline") {
    const sp = splineParts(g, (q) => pos(s, q));
    if (!sp) return undefined;
    const h = hermiteToBSpline(sp.p, splineTangents(sp.p, g.closed, sp.t0, sp.t1, sp.dirs), g.closed);
    return { poles: h.poles, degree: 3, knots: h.knots };
  }
  if (g.type !== "bspline" || g.closed) return undefined;
  const c = bsplineOf(g, (q) => pos(s, q));
  if (!c) return undefined;
  return { poles: c.poles, degree: c.degree, knots: c.knots, ...(g.weights?.length ? { weights: c.weights } : {}) };
}

/** Inserta `u` hasta que quede con multiplicidad grado (la curva pasa por ese polo) */
function saturate(d: Curve, u: number): { d: Curve; at: number } {
  let cur = { ...d, knots: [...d.knots] };
  const mult = () => cur.knots.filter((k) => Math.abs(k - u) < 1e-12).length;
  while (mult() < d.degree) {
    const r = insertKnot({ ...cur, knots: cur.knots }, u);
    cur = { ...cur, poles: r.poles, knots: r.knots, ...(r.weights ? { weights: r.weights } : {}) };
  }
  // El polo sobre la curva: el primero cuyo nudo siguiente pasa de u
  const first = cur.knots.findIndex((k) => Math.abs(k - u) < 1e-12);
  return { d: cur, at: first - 1 };
}

/** El tramo de `d` entre los parámetros u0 < u1, sujeto en las dos puntas */
function between(d: Curve, u0: number, u1: number): Curve {
  const p = d.degree;
  const ks = d.knots;
  let cur = d;
  let from = 0;
  if (u0 > ks[0] + 1e-12) {
    const r = saturate(cur, u0);
    cur = r.d;
    from = r.at;
  }
  let to = cur.poles.length - 1;
  if (u1 < ks[ks.length - 1] - 1e-12) {
    const r = saturate(cur, u1);
    cur = r.d;
    to = r.at;
  }
  const poles = cur.poles.slice(from, to + 1);
  const weights = cur.weights?.slice(from, to + 1);
  const inner = cur.knots.filter((k) => k > u0 + 1e-12 && k < u1 - 1e-12);
  const knots = [...Array(p + 1).fill(u0), ...inner, ...Array(p + 1).fill(u1)];
  return { poles, degree: p, knots, ...(weights ? { weights } : {}) };
}

/** Une dos B-splines sujetas del mismo grado donde la primera termina y la segunda empieza */
function join(a: Curve, b: Curve): Curve {
  const p = a.degree;
  const end = a.knots[a.knots.length - 1];
  const shift = end - b.knots[0];
  const knots = [...a.knots.slice(0, -1), ...b.knots.slice(p + 1).map((k) => k + shift)];
  const poles = [...a.poles, ...b.poles.slice(1)];
  const weights = a.weights && b.weights ? [...a.weights, ...b.weights.slice(1)] : undefined;
  return { poles, degree: p, knots, ...(weights ? { weights } : {}) };
}

/** Una spline por polos cerrada: el tramo como puntos de paso tomados sobre ella */
function sampledPiece(s: Sketch, g: Geometry, pl: P2[], a: End, b: End, idOf: (x: End) => number, wrap: (u: number) => number, span: number): Geometry {
  const last = pl.length - 1;
  const spans = g.type === "bspline" ? Math.max(1, g.poles.length) : 4;
  const n = Math.max(3, Math.ceil((3 * spans * span) / last));
  const pts = [idOf(a)];
  for (let k = 1; k < n; k++) pts.push(addPoint(s, pointAt(pl, wrap(a.u + (span * k) / n))));
  pts.push(idOf(b));
  return { type: "spline", points: pts, closed: false };
}

/**
 * Alarga una spline por puntos abierta o un arco de elipse por la punta más
 * cercana a `p` hasta lo primero que cruce. La spline sigue la tangente de la
 * punta y suma un punto de paso allí; el arco sigue su elipse.
 */
export function extendFreeCurve(s: Sketch, id: number, p: P2, limits: TrimLimits = []): string | undefined {
  const e = s.entities.find((x) => x.id === id);
  if (!e) return undefined;
  const g = e.geometry;
  if (g.type === "bspline") return "Una spline por polos no se extiende: pasarla antes a puntos de paso";
  if (g.type === "ellipse_arc") return extendEllipseArc(s, g, p, limits, id);
  if (g.type !== "spline") return "Solo líneas, arcos, splines y arcos de elipse";
  if (g.closed) return "Una spline cerrada no tiene puntas";
  const pl = polylineOf(s, g);
  const [A, B] = [pl[0], pl[pl.length - 1]];
  const atEnd = Math.hypot(p[0] - B[0], p[1] - B[1]) <= Math.hypot(p[0] - A[0], p[1] - A[1]);
  const from = atEnd ? B : A;
  const prev = atEnd ? pl[pl.length - 2] : pl[1];
  const len = Math.hypot(from[0] - prev[0], from[1] - prev[1]) || 1;
  const d: P2 = [(from[0] - prev[0]) / len, (from[1] - prev[1]) / len];
  // Lo primero que corta el rayo que sale de la punta
  const far = 1e6;
  const ray: P2[] = [from, [from[0] + d[0] * far, from[1] + d[1] * far]];
  const hits = crossings(s, id, ray, limits).filter((c) => c.u * far > 1e-6);
  if (!hits.length) return "No hay nada en esa dirección hasta donde extender";
  const hit = hits[0];
  const np = addPoint(s, hit.p);
  if (atEnd) {
    g.points.push(np);
    delete g.end_handle;
  } else {
    g.points.unshift(np);
    delete g.start_handle;
    if (g.handles) g.handles = g.handles.map(([i, h]) => [i + 1, h]);
  }
  stickOrFix(s, np, hit.hit);
  return undefined;
}

function extendEllipseArc(s: Sketch, g: Extract<Geometry, { type: "ellipse_arc" }>, p: P2, limits: TrimLimits, id: number): string | undefined {
  const c = pos(s, g.center);
  const [u, v] = [sub(pos(s, g.major), c), sub(pos(s, g.minor), c)];
  const sense = cross(u, v) >= 0 ? 1 : -1;
  const [A, B] = [pos(s, g.start), pos(s, g.end)];
  const atEnd = Math.hypot(p[0] - B[0], p[1] - B[1]) <= Math.hypot(p[0] - A[0], p[1] - A[1]);
  const ang = (q: P2) => ellipseParam(c, u, v, q) * sense;
  const norm = (x: number) => ((x % TAU) + TAU) % TAU;
  const sweep = norm(ang(B) - ang(A)) || TAU;
  // Cuánto se avanza (antihorario desde el fin, horario desde el comienzo) hasta cada cruce de la elipse entera
  const full: P2[] = Array.from({ length: 129 }, (_, i) => {
    const t = (i / 128) * TAU;
    return [c[0] + u[0] * Math.cos(t) + v[0] * Math.sin(t), c[1] + u[1] * Math.cos(t) + v[1] * Math.sin(t)];
  });
  let best: { t: number; cut: Cut } | undefined;
  for (const x of crossings(s, id, full, limits)) {
    const t = atEnd ? norm(ang(x.p) - ang(B)) : norm(ang(A) - ang(x.p));
    if (t > 1e-6 && t < TAU - sweep - 1e-6 && (!best || t < best.t)) best = { t, cut: x };
  }
  if (!best) return "No hay nada por ese lado de la elipse hasta donde extender";
  const np = addPoint(s, best.cut.p);
  if (atEnd) g.end = np;
  else g.start = np;
  stickOrFix(s, np, best.cut.hit);
  dropLoosePoints(s);
  return undefined;
}

/**
 * Pasa una spline por polos a una por puntos de paso tomados sobre ella: uno
 * en cada nudo y más donde la nueva se aparta de la vieja más de 0,5 % de su
 * tamaño. Las puntas abiertas conservan su punto y lo que nombra a la curva
 * sigue valiendo (misma entidad).
 */
export function bsplineToThrough(s: Sketch, id: number): string | undefined {
  const e = s.entities.find((x) => x.id === id);
  if (e?.geometry.type !== "bspline") return "Elegir una spline por polos";
  const g = e.geometry;
  const curve = bsplineOf(g, (q) => pos(s, q));
  if (!curve) return "No se pudo leer la spline";
  const closed = !!g.closed;
  const [u0, u1] = domain(curve);
  // Parámetros de los puntos (una cerrada no repite el final)
  let params = breaks(curve);
  if (closed) params = params.slice(0, -1);
  const m = closed ? 4 : 3;
  if (params.length < (closed ? 3 : 2)) params = Array.from({ length: m }, (_, i) => u0 + ((u1 - u0) * i) / (closed ? m : m - 1));
  const dense = sample(curve, 32);
  const size = Math.max(...dense.map((q) => Math.hypot(q[0] - dense[0][0], q[1] - dense[0][1])), 1e-9);
  const tol = size * 5e-3;
  for (let round = 0; round < 8 && params.length < 200; round++) {
    const pts = params.map((u) => evalAt(curve, u));
    const pl = splinePolyline(pts, closed, undefined, undefined, 16);
    // Cuánto se aparta cada tramo: muestras de la curva vieja contra el tramo nuevo
    const worse: number[] = [];
    params.forEach((u, i) => {
      if (!closed && i === params.length - 1) return;
      const next = i + 1 < params.length ? params[i + 1] : u1;
      const piece = pl.slice(16 * i, 16 * i + 17);
      let d = 0;
      for (let k = 1; k < 8; k++) d = Math.max(d, distTo(piece, evalAt(curve, u + ((next - u) * k) / 8)));
      if (d > tol) worse.push(i);
    });
    if (!worse.length) break;
    const add = new Set(worse.map((i) => (params[i] + (i + 1 < params.length ? params[i + 1] : u1)) / 2));
    params = [...params, ...add].sort((a, b) => a - b);
  }
  const pts = params.map((u, i) => {
    if (!closed && i === 0) return g.poles[0];
    if (!closed && i === params.length - 1) return g.poles[g.poles.length - 1];
    return addPoint(s, evalAt(curve, u));
  });
  e.geometry = { type: "spline", points: pts, closed };
  // Los polos que nadie más usa se van
  dropLoosePoints(s);
  return undefined;
}

/** Distancia de `p` a la polilínea */
function distTo(pl: P2[], p: P2): number {
  let best = Infinity;
  for (let i = 0; i + 1 < pl.length; i++) {
    const d = sub(pl[i + 1], pl[i]);
    const t = Math.max(0, Math.min(1, dot(sub(p, pl[i]), d) / (dot(d, d) || 1)));
    best = Math.min(best, Math.hypot(p[0] - pl[i][0] - t * d[0], p[1] - pl[i][1] - t * d[1]));
  }
  return best;
}

/**
 * Corre `run` con solo `bounds` como lo que corta (además de la curva
 * `target`, que no cuenta). Vacío: corta todo, como siempre. Para eso marca
 * por un momento como construcción (que no corta) lo demás.
 */
function withBounds<T>(s: Sketch, target: number, bounds: number[], run: () => T): T {
  if (!bounds.length) return run();
  const was = new Map(s.entities.map((x) => [x, x.construction]));
  const only = new Set(bounds);
  for (const x of s.entities) if (x.id !== target) x.construction = !only.has(x.id);
  try {
    return run();
  } finally {
    for (const [x, c] of was) {
      if (c) x.construction = c;
      else delete x.construction;
    }
  }
}

/**
 * Recorta lo que haya bajo el clic (cualquier curva). Con `bounds`, solo esas
 * entidades cortan (el límite elegido a mano).
 */
export function trimAny(s: Sketch, id: number, p: P2, limits: TrimLimits = [], bounds: number[] = []): string | undefined {
  const g = s.entities.find((x) => x.id === id)?.geometry;
  if (!g) return undefined;
  return withBounds(s, id, bounds, () => (isFreeCurve(g) ? trimFreeCurve(s, id, p, limits) : trimAt(s, id, p, limits)));
}

/** Extiende lo que haya bajo el clic; con `bounds`, solo hasta esas entidades */
export function extendAny(s: Sketch, id: number, p: P2, limits: TrimLimits = [], bounds: number[] = []): string | undefined {
  const g = s.entities.find((x) => x.id === id)?.geometry;
  if (!g) return undefined;
  if (g.type === "line" || g.type === "arc") return withBounds(s, id, bounds, () => extendAt(s, id, p, limits));
  return withBounds(s, id, bounds, () => extendFreeCurve(s, id, p, limits));
}
