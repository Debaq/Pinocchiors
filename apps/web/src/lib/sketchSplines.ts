// Edición de splines del sketch: agregar y quitar puntos (o polos), manijas
// en puntos intermedios, ajuste a puntos, simplificar, convertir a spline por
// polos y la curvatura de lo elegido. También el paralelogramo.
// Funciones puras sobre `Sketch`: se prueban con node (e2e/sketchSplines.test.mjs).

import { addEntity, addPoint, bsplineOf, constraintMentions, geometryPoints, removeEntity, splineParts, splinePolyline, splineTangents, type Geometry, type P2, type Sketch } from "./cad.ts";
import { clampedUniform, closestParam, curvatureInfo, fitBSpline, hermiteToBSpline, insertKnot, makeBSpline, sample, type BSpline, type CurvatureInfo } from "./sketchCurves.ts";

const sub = (a: P2, b: P2): P2 => [a[0] - b[0], a[1] - b[1]];
const add = (a: P2, b: P2): P2 => [a[0] + b[0], a[1] + b[1]];
const mul = (a: P2, k: number): P2 => [a[0] * k, a[1] * k];
const len = (v: P2) => Math.hypot(v[0], v[1]);
const pos = (s: Sketch, id: number): P2 => {
  const p = s.points.find((q) => q.id === id)!;
  return [p.x, p.y];
};
const has = (s: Sketch, id: number) => s.points.some((q) => q.id === id);
const entity = (s: Sketch, id: number) => s.entities.find((e) => e.id === id);

/** Paralelogramo por tres esquinas seguidas `a`, `b`, `c` (ids): la cuarta sale sola; lados opuestos paralelos */
export function addParallelogram(s: Sketch, a: number, b: number, c: number): number[] {
  const d = addPoint(s, add(pos(s, a), sub(pos(s, c), pos(s, b))));
  const l = [addEntity(s, { type: "line", start: a, end: b }), addEntity(s, { type: "line", start: b, end: c }), addEntity(s, { type: "line", start: c, end: d }), addEntity(s, { type: "line", start: d, end: a })];
  s.constraints.push({ type: "parallel", a: l[0], b: l[2] }, { type: "parallel", a: l[1], b: l[3] });
  return l;
}

/** Distancia de `p` a una polilínea */
function toPolyline(p: P2, pl: P2[]): number {
  let best = Infinity;
  for (let i = 0; i + 1 < pl.length; i++) {
    const [a, b] = [pl[i], pl[i + 1]];
    const d = sub(b, a);
    const l2 = d[0] * d[0] + d[1] * d[1];
    const t = l2 ? Math.max(0, Math.min(1, ((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / l2)) : 0;
    best = Math.min(best, len(sub(p, add(a, mul(d, t)))));
  }
  return best;
}

/** Spline por puntos o por polos lista para evaluar (la por puntos, pasada a polos sin cambiar la forma) */
export function curveOf(s: Sketch, id: number): BSpline | undefined {
  const g = entity(s, id)?.geometry;
  const at = (q: number) => (has(s, q) ? pos(s, q) : undefined);
  if (g?.type === "bspline") return bsplineOf(g, at);
  if (g?.type === "spline") {
    const sp = splineParts(g, at);
    if (!sp || sp.p.length < 2) return undefined;
    const h = hermiteToBSpline(sp.p, splineTangents(sp.p, g.closed, sp.t0, sp.t1, sp.dirs), g.closed);
    return makeBSpline({ poles: h.poles, degree: 3, knots: h.knots });
  }
  return undefined;
}

/**
 * Agrega un punto en `p` (proyectado sobre la curva): en una spline por
 * puntos, entre los dos vecinos; en una por polos, un nudo más (la forma no
 * cambia). Devuelve un mensaje si no se pudo.
 */
export function insertSplinePoint(s: Sketch, id: number, p: P2): string | undefined {
  const g = entity(s, id)?.geometry;
  if (g?.type === "spline") {
    const sp = splineParts(g, (q) => (has(s, q) ? pos(s, q) : undefined));
    if (!sp) return "La spline tiene puntos que no están";
    const steps = 32;
    const pl = splinePolyline(sp.p, g.closed, sp.t0, sp.t1, steps, sp.dirs);
    // Tramo de la polilínea más cercano → tramo de la spline
    let best = 0;
    let bestD = Infinity;
    for (let i = 0; i + 1 < pl.length; i++) {
      const d = toPolyline(p, [pl[i], pl[i + 1]]);
      if (d < bestD) [best, bestD] = [i, d];
    }
    const seg = Math.floor(best / steps);
    const a = pl[best];
    const b = pl[best + 1];
    const d = sub(b, a);
    const t = Math.max(0, Math.min(1, ((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / (d[0] * d[0] + d[1] * d[1] || 1)));
    const q = addPoint(s, add(a, mul(d, t)));
    g.points.splice(seg + 1, 0, q);
    if (g.handles) g.handles = g.handles.map(([i, h]) => [i > seg ? i + 1 : i, h]);
    return undefined;
  }
  if (g?.type === "bspline") {
    if (g.closed) return "En una spline por polos cerrada todavía no se agregan polos";
    const curve = bsplineOf(g, (q) => (has(s, q) ? pos(s, q) : undefined));
    if (!curve) return "La spline tiene polos que no están";
    const u = closestParam(curve, p);
    const knots = g.knots?.length ? g.knots : clampedUniform(g.poles.length, curve.degree);
    const r = insertKnot({ poles: curve.poles, weights: g.weights, degree: curve.degree, knots }, u);
    const [k, deg, old] = [r.span, curve.degree, g.poles];
    // Antes: los mismos; los p − 1 de en medio se mueven a su lugar nuevo y se agrega uno; después: corridos
    const ids = r.poles.map((q, i) => {
      if (i <= k - deg) return old[i];
      if (i > k) return old[i - 1];
      if (i < k) {
        const pt = s.points.find((x) => x.id === old[i])!;
        [pt.x, pt.y] = q;
        return old[i];
      }
      return addPoint(s, q);
    });
    g.poles = ids;
    g.knots = r.knots;
    if (r.weights) g.weights = r.weights;
    return undefined;
  }
  return "Clic sobre una spline";
}

/** Quita un punto de una spline por puntos o un polo de una por polos (quedan al menos dos) */
export function removeSplinePoint(s: Sketch, id: number, point: number): string | undefined {
  const g = entity(s, id)?.geometry;
  if (g?.type === "spline") {
    const i = g.points.indexOf(point);
    if (i < 0) return "Ese punto no es de la spline";
    if (g.points.length <= (g.closed ? 3 : 2)) return "La spline se quedaría sin puntos";
    g.points.splice(i, 1);
    if (g.handles) {
      g.handles = g.handles.filter(([k]) => k !== i).map(([k, h]) => [k > i ? k - 1 : k, h]);
      if (!g.handles.length) delete g.handles;
    }
  } else if (g?.type === "bspline") {
    const i = g.poles.indexOf(point);
    if (i < 0) return "Ese punto no es un polo de la spline";
    if (g.poles.length <= (g.closed ? 3 : 2)) return "La spline se quedaría sin polos";
    g.poles.splice(i, 1);
    if (g.weights?.length) g.weights.splice(i, 1);
    // Con un polo menos los nudos ya no cuadran: uniformes otra vez
    delete g.knots;
    g.degree = Math.min(g.degree, g.poles.length - 1);
  } else return "Elegir una spline";
  dropLoose(s, [point]);
  return undefined;
}

/** Saca los puntos que ya no usa nadie (y sus restricciones) */
function dropLoose(s: Sketch, pts: number[]) {
  for (const p of pts) {
    if (p === s.origin || s.entities.some((e) => geometryPoints(e.geometry).includes(p))) continue;
    s.points = s.points.filter((q) => q.id !== p);
    s.constraints = s.constraints.filter((c) => !constraintMentions(c, [p]));
  }
}

/** Pone o saca la manija del punto `point` de una spline por puntos (en las puntas, las de salida y llegada) */
export function toggleSplineHandle(s: Sketch, id: number, point: number): string | undefined {
  const g = entity(s, id)?.geometry;
  if (g?.type !== "spline") return "Elegir un punto de una spline por puntos";
  const i = g.points.indexOf(point);
  if (i < 0) return "Ese punto no es de la spline";
  const end = !g.closed && (i === 0 || i === g.points.length - 1);
  const existing = end ? (i === 0 ? g.start_handle : g.end_handle) : g.handles?.find((h) => h[0] === i)?.[1];
  if (existing !== undefined) {
    if (end) {
      if (i === 0) delete g.start_handle;
      else delete g.end_handle;
    } else {
      g.handles = g.handles!.filter((h) => h[0] !== i);
      if (!g.handles.length) delete g.handles;
    }
    dropLoose(s, [existing]);
    return undefined;
  }
  const sp = splineParts(g, (q) => (has(s, q) ? pos(s, q) : undefined))!;
  const tg = splineTangents(sp.p, g.closed, sp.t0, sp.t1, sp.dirs)[i];
  const h = addPoint(s, add(sp.p[i], mul(tg, 1 / 3)));
  if (end && i === 0) g.start_handle = h;
  else if (end) g.end_handle = h;
  else g.handles = [...(g.handles ?? []), [i, h]];
  return undefined;
}

/**
 * Spline por polos (cúbica) que pasa por el primer y el último punto
 * elegidos y se acerca a los demás a menos de `tol`, con los menos polos que
 * alcance. Devuelve la entidad nueva o un mensaje.
 */
export function fitSplineToPoints(s: Sketch, points: number[], tol: number): number | string {
  if (points.length < 3) return "Elegir al menos tres puntos";
  const pts = points.map((p) => pos(s, p));
  const fit = fitBSpline(pts, tol, 3);
  if (!fit) return "No se pudo ajustar";
  const poles = fit.poles.map((p, i) => (i === 0 ? points[0] : i === fit.poles.length - 1 ? points[points.length - 1] : addPoint(s, p)));
  return addEntity(s, { type: "bspline", poles, degree: Math.min(3, poles.length - 1) });
}

/**
 * Menos puntos (o polos) con la curva a menos de `tol` de la de ahora.
 * Devuelve cuántos sacó, o un mensaje.
 */
export function simplifySpline(s: Sketch, id: number, tol: number): number | string {
  const g = entity(s, id)?.geometry;
  const before = curveOf(s, id);
  if (!g || !before) return "Elegir una spline";
  const ref = sample(before, 32);
  if (g.type === "spline") {
    let removed = 0;
    for (let i = g.closed ? 0 : 1; i < g.points.length - (g.closed ? 0 : 1) && g.points.length > (g.closed ? 3 : 2); ) {
      const trial = structuredClone(s);
      const tg = entity(trial, id)!.geometry as Extract<Geometry, { type: "spline" }>;
      if (tg.handles?.some((h) => h[0] === i)) {
        i++;
        continue;
      }
      removeSplinePoint(trial, id, tg.points[i]);
      const c = curveOf(trial, id);
      const pl = c ? sample(c, 32) : [];
      if (pl.length && Math.max(...ref.map((p) => toPolyline(p, pl))) <= tol) {
        removeSplinePoint(s, id, g.points[i]);
        removed++;
      } else i++;
    }
    return removed;
  }
  if (g.type === "bspline") {
    if (g.closed || g.weights?.some((w) => w !== 1)) return "Solo las splines por polos abiertas y no racionales";
    const fit = fitBSpline(ref, tol, g.degree);
    if (!fit || fit.poles.length >= g.poles.length) return 0;
    const removed = g.poles.length - fit.poles.length;
    const [first, last] = [g.poles[0], g.poles[g.poles.length - 1]];
    const old = g.poles.slice(1, -1);
    g.poles = fit.poles.map((p, i) => (i === 0 ? first : i === fit.poles.length - 1 ? last : addPoint(s, p)));
    g.degree = Math.min(g.degree, g.poles.length - 1);
    delete g.knots;
    dropLoose(s, old);
    return removed;
  }
  return "Elegir una spline";
}

/** Polos de Bézier cúbica de una línea o un arco (el arco en tramos de hasta 90°) */
function bezierPieces(s: Sketch, g: Geometry): P2[][] | undefined {
  if (g.type === "line") {
    const [a, b] = [pos(s, g.start), pos(s, g.end)];
    const d = sub(b, a);
    return [[a, add(a, mul(d, 1 / 3)), add(a, mul(d, 2 / 3)), b]];
  }
  if (g.type === "arc") {
    const [c, a, b] = [pos(s, g.center), pos(s, g.start), pos(s, g.end)];
    const r = len(sub(a, c));
    const a0 = Math.atan2(a[1] - c[1], a[0] - c[0]);
    let sweep = Math.atan2(b[1] - c[1], b[0] - c[0]) - a0;
    while (sweep <= 1e-12) sweep += 2 * Math.PI;
    const n = Math.ceil(sweep / (Math.PI / 2) - 1e-9);
    const th = sweep / n;
    const k = (4 / 3) * Math.tan(th / 4);
    const at = (t: number): P2 => [c[0] + r * Math.cos(t), c[1] + r * Math.sin(t)];
    const tan = (t: number): P2 => [-Math.sin(t) * r * k, Math.cos(t) * r * k];
    const out: P2[][] = [];
    for (let i = 0; i < n; i++) {
      const [t0, t1] = [a0 + th * i, a0 + th * (i + 1)];
      out.push([at(t0), add(at(t0), tan(t0)), sub(at(t1), tan(t1)), at(t1)]);
    }
    out[out.length - 1][3] = b;
    out[0][0] = a;
    return out;
  }
  return undefined;
}

/**
 * Pasa a una spline por polos (cúbica) una cadena de líneas, arcos y splines
 * por puntos unidos por los extremos: las líneas y las splines quedan
 * exactas, los arcos con un error de ~0,03 % del radio. Los extremos de la
 * cadena y las uniones conservan sus puntos. Devuelve la entidad nueva o un mensaje.
 */
export function convertToBSpline(s: Sketch, ids: number[]): number | string {
  const ents = ids.map((id) => entity(s, id)).filter((e) => e !== undefined);
  if (!ents.length) return "Elegir líneas, arcos o splines";
  if (ents.some((e) => !["line", "arc", "spline"].includes(e.geometry.type) || (e.geometry.type === "spline" && e.geometry.closed && ents.length > 1)))
    return "Solo líneas, arcos y splines por puntos (una cerrada va sola)";
  // Una spline cerrada sola: exacta, como curva que vuelve a su comienzo
  const ends = (g: Geometry): [number, number] =>
    g.type === "line" ? [g.start, g.end] : g.type === "arc" ? [g.start, g.end] : g.type === "spline" ? [g.points[0], g.points[g.closed ? 0 : g.points.length - 1]] : [-1, -1];
  // Ordenar la cadena: empezar por una punta (un extremo que no comparte nadie)
  const count = new Map<number, number>();
  for (const e of ents) for (const p of ends(e.geometry)) count.set(p, (count.get(p) ?? 0) + 1);
  let cur = ents.find((e) => ends(e.geometry).some((p) => count.get(p) === 1)) ?? ents[0];
  let at = ends(cur.geometry).find((p) => count.get(p) === 1) ?? ends(cur.geometry)[0];
  const order: { e: (typeof ents)[number]; reversed: boolean }[] = [];
  const left = new Set(ents);
  while (cur) {
    left.delete(cur);
    const [a, b] = ends(cur.geometry);
    const reversed = a !== at;
    order.push({ e: cur, reversed });
    at = reversed ? a : b;
    cur = [...left].find((e) => ends(e.geometry).includes(at))!;
  }
  if (left.size) return "Las entidades no forman una sola cadena";
  // Polos: los de cada tramo en el sentido de la cadena; las uniones con su id
  const poles: { p: P2; id?: number }[] = [];
  const knots = [0, 0, 0, 0];
  let spans = 0;
  for (const { e, reversed } of order) {
    const g = e.geometry;
    let pieces: P2[][];
    let joints: number[];
    if (g.type === "spline") {
      const sp = splineParts(g, (q) => pos(s, q))!;
      const h = hermiteToBSpline(sp.p, splineTangents(sp.p, g.closed, sp.t0, sp.t1, sp.dirs), g.closed);
      pieces = [];
      for (let i = 0; i + 3 < h.poles.length; i += 3) pieces.push(h.poles.slice(i, i + 4));
      joints = g.closed ? [...g.points, g.points[0]] : [...g.points];
    } else {
      pieces = bezierPieces(s, g)!;
      const [a, b] = ends(g);
      joints = [a, ...Array(pieces.length - 1).fill(-1), b];
    }
    if (reversed) {
      pieces = pieces.reverse().map((q) => [...q].reverse());
      joints = [...joints].reverse();
    }
    pieces.forEach((q, i) => {
      if (!poles.length) poles.push({ p: q[0], id: joints[0] >= 0 ? joints[0] : undefined });
      poles.push({ p: q[1] }, { p: q[2] }, { p: q[3], id: joints[i + 1] >= 0 ? joints[i + 1] : undefined });
      spans++;
      if (spans > 1) knots.push(spans - 1, spans - 1, spans - 1);
    });
  }
  knots.push(spans, spans, spans, spans);
  const ids2 = poles.map((q) => q.id ?? addPoint(s, q.p));
  // Las splines: sus puntos interiores pasan a ser polos (siguen siendo de la curva)
  const nb = addEntity(s, { type: "bspline", poles: ids2, degree: 3, knots });
  const construction = ents.every((e) => e.construction);
  for (const e of ents) removeEntity(s, e.id);
  if (construction) entity(s, nb)!.construction = true;
  return nb;
}

/** Curvatura de una spline por puntos o por polos: peine (dientes de hasta `scale` mm), inflexiones y radio mínimo */
export function splineCurvature(s: Sketch, id: number, scale: number): CurvatureInfo | undefined {
  const c = curveOf(s, id);
  return c && curvatureInfo(c, scale);
}
