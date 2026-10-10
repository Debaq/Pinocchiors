// Mover, copiar, girar, escalar y partir entidades del sketch, y el
// portapapeles para copiar y pegar (dentro de un sketch o entre sketches).
// Funciones puras sobre `Sketch`: se prueban con node (e2e/sketchTransform.test.mjs).

import { addEntity, addPoint, constraintIds, geometryPoints, mapConstraintIds, mapGeometry, splitLineAt, type Geometry, type P2, type Sketch, type SketchConstraint, type SketchEntity, type SketchText } from "./cad.ts";

/** Lleva un punto del plano del sketch a otro lugar */
export type Xform = (p: P2) => P2;

export const translation =
  (d: P2): Xform =>
  (p) => [p[0] + d[0], p[1] + d[1]];

/** Giro de `degrees` (antihorario) alrededor de `c` */
export function rotation(c: P2, degrees: number): Xform {
  const a = (degrees * Math.PI) / 180;
  const [cos, sin] = [Math.cos(a), Math.sin(a)];
  return (p) => {
    const [x, y] = [p[0] - c[0], p[1] - c[1]];
    return [c[0] + x * cos - y * sin, c[1] + x * sin + y * cos];
  };
}

export const scaling =
  (c: P2, k: number): Xform =>
  (p) => [c[0] + (p[0] - c[0]) * k, c[1] + (p[1] - c[1]) * k];

const pos = (s: Sketch, id: number): P2 | undefined => {
  const p = s.points.find((q) => q.id === id);
  return p ? [p.x, p.y] : undefined;
};
const sub = (a: P2, b: P2): P2 => [a[0] - b[0], a[1] - b[1]];
const len = (v: P2) => Math.hypot(v[0], v[1]);
const cross = (a: P2, b: P2) => a[0] * b[1] - a[1] * b[0];
const dot = (a: P2, b: P2) => a[0] * b[0] + a[1] * b[1];

/** Matriz 2×2 por filas: x' = a·x + b·y, y' = c·x + d·y */
export type Frame = [number, number, number, number];

/** Parte lineal de una transformación, sin la escala (la lleva el tamaño del texto) */
export function linearPart(f: Xform): Frame {
  const o = f([0, 0]);
  const [ex, ey] = [sub(f([1, 0]), o), sub(f([0, 1]), o)];
  const m: Frame = [ex[0], ey[0], ex[1], ey[1]];
  const k = Math.sqrt(Math.abs(m[0] * m[3] - m[1] * m[2])) || 1;
  return m.map((v) => v / k) as Frame;
}

/** `m · n` */
export function composeFrame(m: Frame, n: Frame = [1, 0, 0, 1]): Frame {
  return [m[0] * n[0] + m[1] * n[2], m[0] * n[1] + m[1] * n[3], m[2] * n[0] + m[3] * n[2], m[2] * n[1] + m[3] * n[3]];
}

/** Aplica `m` a `p` alrededor de `at` */
export function applyFrame(m: Frame, at: P2, p: P2): P2 {
  const [x, y] = sub(p, at);
  return [at[0] + m[0] * x + m[1] * y, at[1] + m[2] * x + m[3] * y];
}

/** Inversa de una matriz de giro o reflejo (determinante ±1) */
export function invertFrame(m: Frame): Frame {
  const det = m[0] * m[3] - m[1] * m[2] || 1;
  return [m[3] / det, -m[1] / det, -m[2] / det, m[0] / det];
}

/** Le suma al marco del texto un giro o reflejo (sin guardar la identidad) */
function turnText(t: SketchText, m: Frame) {
  const f = composeFrame(m, t.style?.frame);
  const identity = Math.abs(f[0] - 1) < 1e-9 && Math.abs(f[1]) < 1e-9 && Math.abs(f[2]) < 1e-9 && Math.abs(f[3] - 1) < 1e-9;
  const style = { ...(t.style ?? {}) };
  if (identity) delete style.frame;
  else style.frame = f.map((v) => +v.toFixed(12)) as Frame;
  if (Object.keys(style).length) t.style = style;
  else delete t.style;
}

/** Textos que toca la selección (por una de sus curvas o su ancla) */
function textsOf(s: Sketch, ids: Set<number>): SketchText[] {
  return (s.texts ?? []).filter((t) => [t.anchor, ...t.entities].some((x) => ids.has(x)));
}

/**
 * Entidades elegidas: las nombradas, las de un punto suelto elegido por su
 * punto y las de los textos tocados (un texto va entero)
 */
export function selectedEntities(s: Sketch, ids: number[]): SketchEntity[] {
  const set = new Set(ids);
  for (const t of textsOf(s, set)) for (const e of t.entities) set.add(e);
  return s.entities.filter((e) => set.has(e.id) || (e.geometry.type === "point" && set.has(e.geometry.point)));
}

/** Entidades que no se mueven: ligadas al sólido o bloqueadas */
export function heldEntities(s: Sketch): Set<number> {
  const out = new Set((s.uses ?? []).map((u) => u.entity));
  for (const c of s.constraints) if (c.type === "lock") out.add(c.entity);
  // Las de capas bloqueadas tampoco
  const locked = new Set((s.layers ?? []).filter((l) => l.locked).map((l) => l.id));
  for (const e of s.entities) if (e.layer != null && locked.has(e.layer)) out.add(e.id);
  return out;
}

/** Puntos que mueve la selección (sin el origen, lo ligado al sólido ni lo bloqueado) */
export function selectionPoints(s: Sketch, ids: number[]): number[] {
  const used = heldEntities(s);
  const out = new Set<number>();
  for (const e of selectedEntities(s, ids)) if (!used.has(e.id)) for (const p of geometryPoints(e.geometry)) out.add(p);
  for (const t of textsOf(s, new Set(ids))) for (const p of [t.anchor, ...t.points]) out.add(p);
  // Puntos elegidos sueltos (el extremo de una línea, por ejemplo)
  for (const id of ids) if (s.points.some((p) => p.id === id)) out.add(id);
  if (s.origin !== undefined) out.delete(s.origin);
  // Lo ligado al sólido y lo bloqueado se quedan donde están (también un extremo compartido)
  for (const e of s.entities) if (used.has(e.id)) for (const p of geometryPoints(e.geometry)) out.delete(p);
  return [...out].filter((id) => s.points.some((p) => p.id === id));
}

/** Centro de la caja de lo elegido (para girar y escalar desde el panel) */
export function selectionCenter(s: Sketch, ids: number[]): P2 | undefined {
  const pts = selectionPoints(s, ids).map((id) => pos(s, id)!);
  // Un círculo elegido cuenta con su borde, no solo su centro
  for (const e of selectedEntities(s, ids))
    if (e.geometry.type === "circle") {
      const c = pos(s, e.geometry.center)!;
      const r = e.geometry.radius;
      pts.push([c[0] - r, c[1] - r], [c[0] + r, c[1] + r]);
    }
  if (!pts.length) return undefined;
  const xs = pts.map((p) => p[0]);
  const ys = pts.map((p) => p[1]);
  return [(Math.min(...xs) + Math.max(...xs)) / 2, (Math.min(...ys) + Math.max(...ys)) / 2];
}

/** Radio de un círculo o arco */
function radiusOf(s: Sketch, id: number): number | undefined {
  const g = s.entities.find((e) => e.id === id)?.geometry;
  if (g?.type === "circle") return g.radius;
  if (g?.type === "arc") {
    const [c, a] = [pos(s, g.center), pos(s, g.start)];
    return c && a ? len(sub(a, c)) : undefined;
  }
  return undefined;
}

function lineOf(s: Sketch, id: number): [P2, P2] | undefined {
  const g = s.entities.find((e) => e.id === id)?.geometry;
  if (g?.type !== "line") return undefined;
  const [a, b] = [pos(s, g.start), pos(s, g.end)];
  return a && b ? [a, b] : undefined;
}

function centerOf(s: Sketch, id: number): P2 | undefined {
  const g = s.entities.find((e) => e.id === id)?.geometry;
  if (g?.type === "circle" || g?.type === "arc" || g?.type === "ellipse") return pos(s, g.center);
  return undefined;
}

/** Distancia de `p` a la recta que pasa por la línea */
function toLine(p: P2, [a, b]: [P2, P2]): number {
  const d = sub(b, a);
  const l = len(d);
  return l ? Math.abs(cross(d, sub(p, a))) / l : len(sub(p, a));
}

type Target = { kind: "point"; p: P2 } | { kind: "line"; a: P2; b: P2 } | { kind: "round"; c: P2; r: number };

/** Qué es un id en una cota con círculos: punto, línea o círculo/arco */
function dimTarget(s: Sketch, id: number): Target | undefined {
  const e = s.entities.find((x) => x.id === id);
  if (!e) {
    const p = pos(s, id);
    return p && { kind: "point", p };
  }
  const g = e.geometry;
  if (g.type === "line") {
    const l = lineOf(s, id);
    return l && { kind: "line", a: l[0], b: l[1] };
  }
  if (g.type === "circle" || g.type === "arc") {
    const [c, r] = [pos(s, g.center), radiusOf(s, id)];
    return c && r !== undefined ? { kind: "round", c, r } : undefined;
  }
  if (g.type === "point") {
    const p = pos(s, g.point);
    return p && { kind: "point", p };
  }
  return undefined;
}

/**
 * Distancia mínima o máxima entre un círculo o arco y otro, un punto o una
 * línea (la misma cuenta que `Sketch::circle_gap` en Rust): la mínima entre
 * círculos es por fuera, o por dentro si uno contiene al otro.
 */
export function circleGap(s: Sketch, a: number, b: number, max: boolean): number | undefined {
  let [ta, tb] = [dimTarget(s, a), dimTarget(s, b)];
  if (!ta || !tb) return undefined;
  if (ta.kind === "line") [ta, tb] = [tb, ta];
  if (ta.kind === "line" || (ta.kind !== "round" && tb.kind !== "round")) return undefined;
  const pa = ta.kind === "round" ? ta.c : ta.p;
  const base = tb.kind === "line" ? toLine(pa, [tb.a, tb.b]) : len(sub(pa, tb.kind === "round" ? tb.c : tb.p));
  const [ra, rb] = [ta.kind === "round" ? ta.r : 0, tb.kind === "round" ? tb.r : 0];
  if (max) return base + ra + rb;
  if (ta.kind === "round" && tb.kind === "round") return base < Math.abs(ra - rb) ? Math.abs(ra - rb) - base : base - ra - rb;
  return Math.abs(base - ra - rb);
}

/** Largo de una línea, un arco o un círculo */
export function curveLength(s: Sketch, id: number): number | undefined {
  const g = s.entities.find((e) => e.id === id)?.geometry;
  if (g?.type === "line") {
    const l = lineOf(s, id);
    return l && len(sub(l[1], l[0]));
  }
  if (g?.type === "circle") return 2 * Math.PI * g.radius;
  if (g?.type === "arc") {
    const [k, a, b] = [pos(s, g.center), pos(s, g.start), pos(s, g.end)];
    if (!k || !a || !b) return undefined;
    const [u, v] = [sub(a, k), sub(b, k)];
    let sweep = Math.atan2(cross(u, v), dot(u, v));
    if (sweep <= 0) sweep += 2 * Math.PI;
    return len(u) * sweep;
  }
  return undefined;
}

/**
 * Lo que mide una cota en la geometría actual (la misma cuenta que
 * `Sketch::measure` en Rust). `undefined` si no es una cota.
 */
export function measureConstraint(s: Sketch, c: SketchConstraint): number | undefined {
  switch (c.type) {
    case "distance": {
      const [a, b] = [pos(s, c.a), pos(s, c.b)];
      return a && b ? len(sub(b, a)) : undefined;
    }
    case "horizontal_distance": {
      const [a, b] = [pos(s, c.a), pos(s, c.b)];
      return a && b ? b[0] - a[0] : undefined;
    }
    case "vertical_distance": {
      const [a, b] = [pos(s, c.a), pos(s, c.b)];
      return a && b ? b[1] - a[1] : undefined;
    }
    case "length": {
      const l = lineOf(s, c.line);
      return l && len(sub(l[1], l[0]));
    }
    case "radius":
      return radiusOf(s, c.entity);
    case "diameter": {
      const r = radiusOf(s, c.entity);
      return r === undefined ? undefined : 2 * r;
    }
    case "point_line_distance":
    case "axis_diameter": {
      const [p, l] = [pos(s, c.point), lineOf(s, c.line)];
      if (!p || !l) return undefined;
      return (c.type === "axis_diameter" ? 2 : 1) * toLine(p, l);
    }
    case "arc_length":
      return s.entities.find((e) => e.id === c.arc)?.geometry.type === "arc" ? curveLength(s, c.arc) : undefined;
    case "curve_length": {
      let total = 0;
      for (const id of c.entities) {
        const l = curveLength(s, id);
        if (l === undefined) return undefined;
        total += l;
      }
      return total;
    }
    case "circle_distance":
      return circleGap(s, c.a, c.b, !!c.max);
    case "angle": {
      const [l1, l2] = [lineOf(s, c.a), lineOf(s, c.b)];
      if (!l1 || !l2) return undefined;
      const d1 = sub(l1[1], l1[0]);
      // Suplementario: hasta la segunda invertida
      const d2 = c.supplementary ? sub(l2[0], l2[1]) : sub(l2[1], l2[0]);
      return (Math.atan2(cross(d1, d2), dot(d1, d2)) * 180) / Math.PI;
    }
    default:
      return undefined;
  }
}

/**
 * Si una restricción geométrica se cumple en la geometría actual (`tol` en
 * mm para distancias, en seno del ángulo para direcciones). Las que no se
 * saben evaluar se dan por cumplidas.
 */
export function constraintHolds(s: Sketch, c: SketchConstraint, tol: number): boolean {
  const angTol = 1e-6;
  const P = (id: number) => pos(s, id);
  switch (c.type) {
    case "coincident": {
      const [a, b] = [P(c.a), P(c.b)];
      return !a || !b || len(sub(a, b)) <= tol;
    }
    case "horizontal":
    case "vertical": {
      const l = lineOf(s, c.line);
      if (!l) return true;
      const d = sub(l[1], l[0]);
      return Math.abs(c.type === "horizontal" ? d[1] : d[0]) <= Math.max(tol, angTol * len(d));
    }
    case "horizontal_points":
    case "vertical_points": {
      const [a, b] = [P(c.a), P(c.b)];
      return !a || !b || Math.abs(c.type === "horizontal_points" ? a[1] - b[1] : a[0] - b[0]) <= tol;
    }
    case "parallel":
    case "perpendicular": {
      const [l1, l2] = [lineOf(s, c.a), lineOf(s, c.b)];
      if (!l1 || !l2) return true;
      const [d1, d2] = [sub(l1[1], l1[0]), sub(l2[1], l2[0])];
      const n = len(d1) * len(d2);
      return !n || Math.abs(c.type === "parallel" ? cross(d1, d2) : dot(d1, d2)) / n <= angTol;
    }
    case "collinear": {
      const [l1, l2] = [lineOf(s, c.a), lineOf(s, c.b)];
      if (!l1 || !l2) return true;
      return constraintHolds(s, { type: "parallel", a: c.a, b: c.b }, tol) && toLine(l2[0], l1) <= tol;
    }
    case "equal": {
      const [l1, l2] = [lineOf(s, c.a), lineOf(s, c.b)];
      if (l1 && l2) return Math.abs(len(sub(l1[1], l1[0])) - len(sub(l2[1], l2[0]))) <= tol;
      const [r1, r2] = [radiusOf(s, c.a), radiusOf(s, c.b)];
      return r1 === undefined || r2 === undefined || Math.abs(r1 - r2) <= tol;
    }
    case "tangent": {
      const [la, lb] = [lineOf(s, c.a), lineOf(s, c.b)];
      const [ca, cb] = [centerOf(s, c.a), centerOf(s, c.b)];
      const [ra, rb] = [radiusOf(s, c.a), radiusOf(s, c.b)];
      if (la && cb && rb !== undefined) return Math.abs(toLine(cb, la) - rb) <= tol;
      if (lb && ca && ra !== undefined) return Math.abs(toLine(ca, lb) - ra) <= tol;
      if (ca && cb && ra !== undefined && rb !== undefined) {
        const d = len(sub(ca, cb));
        return Math.abs(d - (ra + rb)) <= tol || Math.abs(d - Math.abs(ra - rb)) <= tol;
      }
      return true;
    }
    case "concentric": {
      const [a, b] = [centerOf(s, c.a), centerOf(s, c.b)];
      return !a || !b || len(sub(a, b)) <= tol;
    }
    case "point_on_line": {
      const [p, l] = [P(c.point), lineOf(s, c.line)];
      return !p || !l || toLine(p, l) <= tol;
    }
    case "point_on_circle": {
      const [p, k, r] = [P(c.point), centerOf(s, c.circle), radiusOf(s, c.circle)];
      return !p || !k || r === undefined || Math.abs(len(sub(p, k)) - r) <= tol;
    }
    case "point_on_curve":
      return onCurve(s, c.point, c.curve, tol);
    case "intersection":
      return onCurve(s, c.point, c.a, tol) && onCurve(s, c.point, c.b, tol);
    case "coradial": {
      const [a, b, ra, rb] = [centerOf(s, c.a), centerOf(s, c.b), radiusOf(s, c.a), radiusOf(s, c.b)];
      return !a || !b || ra === undefined || rb === undefined || (len(sub(a, b)) <= tol && Math.abs(ra - rb) <= tol);
    }
    case "midpoint": {
      const [p, l] = [P(c.point), lineOf(s, c.line)];
      return !p || !l || len(sub(p, [(l[0][0] + l[1][0]) / 2, (l[0][1] + l[1][1]) / 2])) <= tol;
    }
    case "symmetric": {
      const [a, b, l] = [P(c.a), P(c.b), lineOf(s, c.line)];
      if (!a || !b || !l) return true;
      const d = sub(l[1], l[0]);
      const t = dot(sub(a, l[0]), d) / dot(d, d);
      const foot: P2 = [l[0][0] + t * d[0], l[0][1] + t * d[1]];
      return len(sub([2 * foot[0] - a[0], 2 * foot[1] - a[1]], b)) <= tol;
    }
    case "equal_offset": {
      const [a1, a2, b1, b2] = [P(c.a1), P(c.a2), P(c.b1), P(c.b2)];
      return !a1 || !a2 || !b1 || !b2 || len(sub(sub(a2, a1), sub(b2, b1))) <= tol;
    }
    case "equal_rotation": {
      // Mismo giro alrededor del centro para los dos pares: mismo ángulo y mismas distancias al centro
      const [o, a1, a2, b1, b2] = [P(c.center), P(c.a1), P(c.a2), P(c.b1), P(c.b2)];
      if (!o || !a1 || !a2 || !b1 || !b2) return true;
      const ang = (u: P2, v: P2) => Math.atan2(cross(sub(u, o), sub(v, o)), dot(sub(u, o), sub(v, o)));
      const da = ang(a1, a2) - ang(b1, b2);
      return (
        Math.abs(Math.atan2(Math.sin(da), Math.cos(da))) <= 1e-6 &&
        Math.abs(len(sub(a1, o)) - len(sub(a2, o))) <= tol &&
        Math.abs(len(sub(b1, o)) - len(sub(b2, o))) <= tol
      );
    }
    default:
      return true;
  }
}

/** Si un punto está sobre una curva (las splines se dan por cumplidas) */
function onCurve(s: Sketch, point: number, curve: number, tol: number): boolean {
  const p = pos(s, point);
  const g = s.entities.find((e) => e.id === curve)?.geometry;
  if (!p || !g) return true;
  if (g.type === "line") {
    const l = lineOf(s, curve);
    return !l || toLine(p, l) <= tol;
  }
  if (g.type === "circle" || g.type === "arc") {
    const [k, r] = [centerOf(s, curve), radiusOf(s, curve)];
    return !k || r === undefined || Math.abs(len(sub(p, k)) - r) <= tol;
  }
  if (g.type === "ellipse") {
    const [k, a, b] = [pos(s, g.center), pos(s, g.major), pos(s, g.minor)];
    if (!k || !a || !b) return true;
    const u = sub(a, k);
    const [ra, rb] = [len(u), len(sub(b, k))];
    if (!ra || !rb) return true;
    const q = sub(p, k);
    const [x, y] = [dot(q, u) / ra, cross(u, q) / ra];
    return Math.abs(Math.hypot(x / ra, y / rb) - 1) * Math.sqrt(ra * rb) <= tol;
  }
  return true;
}

/** Puntos que mira una restricción (las entidades, por sus puntos) */
function constraintPoints(s: Sketch, c: SketchConstraint): number[] {
  const out: number[] = [];
  for (const id of constraintIds(c)) {
    const e = s.entities.find((x) => x.id === id);
    if (e) out.push(...geometryPoints(e.geometry));
    else out.push(id);
  }
  return out;
}

/** Tolerancia de "se cumple" según el tamaño del sketch */
function sketchTol(s: Sketch): number {
  let m = 1;
  for (const p of s.points) m = Math.max(m, Math.abs(p.x), Math.abs(p.y));
  return 1e-6 * m;
}

/**
 * Lleva lo elegido por `f` (mover, girar o escalar con factor `scale`). Los
 * puntos compartidos con lo no elegido se mueven igual (lo de al lado se
 * estira). Después, para que el sketch quede donde se dejó: los fijos pasan a
 * su lugar nuevo, las cotas toman lo que miden ahora (salvo las que tienen
 * fórmula) y las restricciones que dejaron de cumplirse se quitan (una
 * horizontal girada 90° pasa a vertical). Devuelve un mensaje si no hay qué mover.
 */
export function transformSelection(s: Sketch, ids: number[], f: Xform, scale = 1, all = false): string | undefined {
  // `all`: el sketch entero, también lo ligado y lo bloqueado (girar el dibujo con su plano)
  const pts = all ? s.points.filter((p) => p.id !== s.origin).map((p) => p.id) : selectionPoints(s, ids);
  if (!pts.length) return "Elegir primero lo que se transforma (lo ligado al sólido y lo bloqueado no se mueven)";
  const moved = new Set(pts);
  for (const p of s.points)
    if (moved.has(p.id)) {
      const [x, y] = f([p.x, p.y]);
      p.x = x;
      p.y = y;
    }
  if (scale !== 1) {
    const k = Math.abs(scale);
    for (const e of selectedEntities(s, ids)) if (e.geometry.type === "circle") e.geometry.radius *= k;
    for (const t of textsOf(s, new Set(ids))) t.size *= k;
  }
  // Los textos recuerdan el giro o reflejo para rehacerse igual
  const lin = linearPart(f);
  if (Math.abs(lin[0] - 1) > 1e-12 || Math.abs(lin[1]) > 1e-12 || Math.abs(lin[2]) > 1e-12 || Math.abs(lin[3] - 1) > 1e-12)
    for (const t of all ? (s.texts ?? []) : textsOf(s, new Set(ids))) turnText(t, lin);
  const tol = sketchTol(s);
  const scaled = new Set(scale !== 1 ? selectedEntities(s, ids).map((e) => e.id) : []);
  const keep: SketchConstraint[] = [];
  for (const c of s.constraints) {
    const touched = constraintPoints(s, c).some((p) => moved.has(p)) || constraintIds(c).some((id) => scaled.has(id));
    if (!touched) {
      keep.push(c);
      continue;
    }
    if (c.type === "fixed") {
      const p = pos(s, c.point)!;
      keep.push({ ...c, x: p[0], y: p[1] });
      continue;
    }
    const v = measureConstraint(s, c);
    if (v !== undefined) {
      // Las que tienen fórmula o están bloqueadas no cambian
      if (!(c as { expr?: string }).expr && !("opts" in c && c.opts?.locked)) {
        if (c.type === "angle") c.degrees = +v.toFixed(9);
        else (c as { value: number }).value = +v.toFixed(9);
      }
      keep.push(c);
      continue;
    }
    if (constraintHolds(s, c, tol)) keep.push(c);
    else if (c.type === "horizontal" || c.type === "vertical") {
      const swapped: SketchConstraint = { type: c.type === "horizontal" ? "vertical" : "horizontal", line: c.line };
      if (constraintHolds(s, swapped, tol)) keep.push(swapped);
    } else if (c.type === "horizontal_points" || c.type === "vertical_points") {
      const swapped: SketchConstraint = { type: c.type === "horizontal_points" ? "vertical_points" : "horizontal_points", a: c.a, b: c.b };
      if (constraintHolds(s, swapped, tol)) keep.push(swapped);
    }
  }
  s.constraints = keep;
  return undefined;
}

// ─── Portapapeles ─────────────────────────────────────────────────────────

/** Parte de un sketch copiada: entidades, sus puntos y las restricciones entre ellas */
export interface SketchClip {
  points: { id: number; x: number; y: number }[];
  entities: SketchEntity[];
  constraints: SketchConstraint[];
  texts: SketchText[];
}

/**
 * Copia lo elegido: entidades (un texto va entero), sus puntos y las
 * restricciones que solo nombran cosas de adentro. Los fijos no se copian.
 */
export function extractClip(s: Sketch, ids: number[]): SketchClip | undefined {
  const ents = selectedEntities(s, ids);
  if (!ents.length) return undefined;
  const pts = new Set<number>();
  for (const e of ents) for (const p of geometryPoints(e.geometry)) pts.add(p);
  const texts = textsOf(s, new Set(ids)).map((t) => structuredClone(t));
  for (const t of texts) for (const p of [t.anchor, ...t.points]) pts.add(p);
  const inside = new Set([...pts, ...ents.map((e) => e.id)]);
  return {
    points: s.points.filter((p) => pts.has(p.id)).map((p) => ({ id: p.id, x: p.x, y: p.y })),
    entities: ents.map((e) => structuredClone(e)),
    constraints: s.constraints.filter((c) => c.type !== "fixed" && c.type !== "lock" && constraintIds(c).every((id) => inside.has(id))).map((c) => structuredClone(c)),
    texts,
  };
}

/** Centro de la caja de un portapapeles (de donde se toma al pegar) */
export function clipCenter(clip: SketchClip): P2 {
  const xs = clip.points.map((p) => p.x);
  const ys = clip.points.map((p) => p.y);
  return [(Math.min(...xs) + Math.max(...xs)) / 2, (Math.min(...ys) + Math.max(...ys)) / 2];
}

/**
 * Pega un portapapeles con sus puntos llevados por `f`, con ids nuevos.
 * Devuelve las entidades nuevas.
 */
export function insertClip(s: Sketch, clip: SketchClip, f: Xform): number[] {
  const map = new Map<number, number>();
  for (const p of clip.points) map.set(p.id, addPoint(s, f([p.x, p.y])));
  const id = (x: number) => map.get(x) ?? x;
  const out: number[] = [];
  for (const e of clip.entities) {
    const n = addEntity(s, mapGeometry(e.geometry, id));
    const added = s.entities.find((x) => x.id === n)!;
    if (e.construction) added.construction = true;
    if (e.infinite) added.infinite = true;
    map.set(e.id, n);
    out.push(n);
  }
  for (const c of clip.constraints) s.constraints.push(mapConstraintIds(c, id));
  for (const t of clip.texts) {
    const tid = (Math.max(0, ...(s.texts ?? []).map((x) => x.id)) || 0) + 1;
    s.texts = [...(s.texts ?? []), { ...t, id: tid, anchor: id(t.anchor), entities: t.entities.map(id), points: t.points.map(id) }];
  }
  return out;
}

// ─── Partir ───────────────────────────────────────────────────────────────

/**
 * Parte una línea, arco o círculo en el punto de la curva más cercano a `p`.
 * La línea queda en dos que comparten el punto; el arco, en dos con el mismo
 * centro; el círculo, en dos medias vueltas (en `p` y en el opuesto). Las
 * restricciones del original quedan en el primer tramo.
 * Devuelve el punto nuevo o un mensaje.
 */
export function splitEntityAt(s: Sketch, entity: number, p: P2): number | string {
  const e = s.entities.find((x) => x.id === entity);
  if (!e) return "No hay nada ahí para partir";
  const g = e.geometry;
  const minLen = 1e-6 * Math.max(1, len(p));
  if (g.type === "line") {
    const [a, b] = [pos(s, g.start)!, pos(s, g.end)!];
    const d = sub(b, a);
    const t = dot(sub(p, a), d) / dot(d, d);
    if (!(t > 1e-6 && t < 1 - 1e-6)) return "Partir en un punto entre los extremos";
    const q = addPoint(s, [a[0] + t * d[0], a[1] + t * d[1]]);
    splitLineAt(s, entity, q);
    return q;
  }
  if (g.type === "arc" || g.type === "circle") {
    const c = pos(s, g.center)!;
    const r = g.type === "circle" ? g.radius : len(sub(pos(s, g.start)!, c));
    const v = sub(p, c);
    if (len(v) < minLen) return "Partir en un punto de la curva";
    const at = (ang: number): P2 => [c[0] + r * Math.cos(ang), c[1] + r * Math.sin(ang)];
    const ang = Math.atan2(v[1], v[0]);
    if (g.type === "arc") {
      const a0 = Math.atan2(pos(s, g.start)![1] - c[1], pos(s, g.start)![0] - c[0]);
      const a1 = Math.atan2(pos(s, g.end)![1] - c[1], pos(s, g.end)![0] - c[0]);
      const ccw = (x: number) => ((x % (2 * Math.PI)) + 2 * Math.PI) % (2 * Math.PI);
      const sweep = ccw(a1 - a0) || 2 * Math.PI;
      const t = ccw(ang - a0);
      if (!(t > 1e-6 && t < sweep - 1e-6)) return "Partir en un punto entre los extremos del arco";
      const q = addPoint(s, at(ang));
      const end = g.end;
      g.end = q;
      const rest = addEntity(s, { type: "arc", center: g.center, start: q, end });
      if (e.construction) s.entities.find((x) => x.id === rest)!.construction = true;
      // Lo tangente en el extremo que pasó al tramo nuevo sigue tangente a ese tramo
      for (const k of s.constraints) {
        if (k.type !== "tangent" || (k.a !== entity && k.b !== entity)) continue;
        const other = s.entities.find((x) => x.id === (k.a === entity ? k.b : k.a));
        if (other && geometryPoints(other.geometry).includes(end) && !geometryPoints(other.geometry).includes(g.start)) {
          if (k.a === entity) k.a = rest;
          else k.b = rest;
        }
      }
      return q;
    }
    // Círculo: dos arcos antihorarios que se juntan en p y en el opuesto
    const q1 = addPoint(s, at(ang));
    const q2 = addPoint(s, at(ang + Math.PI));
    (e as { geometry: Geometry }).geometry = { type: "arc", center: g.center, start: q1, end: q2 };
    const rest = addEntity(s, { type: "arc", center: g.center, start: q2, end: q1 });
    if (e.construction) s.entities.find((x) => x.id === rest)!.construction = true;
    return q1;
  }
  return "Solo se parten líneas, arcos y círculos";
}

// ─── Reflejo para invertir la normal ──────────────────────────────────────

/**
 * Refleja el sketch en su eje x (y → −y) para que, con la normal invertida,
 * el dibujo quede en el mismo lugar del espacio. Los arcos cambian de sentido
 * y las cotas con signo (vertical, ángulo, fijos, textos corridos) también.
 */
export function flipSketchY(s: Sketch): void {
  for (const p of s.points) p.y = -p.y || 0;
  for (const t of s.texts ?? []) turnText(t, [1, 0, 0, -1]);
  for (const e of s.entities) {
    const g = e.geometry;
    if (g.type === "arc" || g.type === "ellipse_arc") [g.start, g.end] = [g.end, g.start];
  }
  for (const c of s.constraints) {
    if (c.type === "fixed") c.y = -c.y || 0;
    else if (c.type === "vertical_distance") c.value = -c.value || 0;
    else if (c.type === "angle") c.degrees = -c.degrees || 0;
    const o = (c as { opts?: { offset?: [number, number] } }).opts;
    if (o?.offset) o.offset = [o.offset[0], -o.offset[1] || 0];
  }
}

/**
 * Gira el sketch entero `degrees` alrededor de su origen: al cambiar la
 * horizontal del plano el dibujo se gira al revés para quedar en su lugar.
 * Las horizontales pasan a verticales (o se quitan si ya no lo son) y las
 * cotas toman lo que miden ahora.
 */
export function rotateSketch(s: Sketch, degrees: number): void {
  if (Math.abs(degrees) < 1e-9) return;
  transformSelection(s, [], rotation([0, 0], degrees), 1, true);
}
