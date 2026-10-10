// Edición II del sketch: recortar con un trazo, unir, chaflán, equidistante de
// cadena (a los dos lados y con extremos cerrados), eliminar duplicados,
// reparar y cerrar contornos.
// Funciones puras: se prueban con node (e2e/sketchEdit.test.mjs).

import {
  addEntity,
  addPoint,
  constraintIds,
  constraintMentions,
  geometryPoints,
  linesAt,
  mapConstraintIds,
  mapGeometry,
  removeEntity,
  trimAt,
  type Geometry,
  type P2,
  type Sketch,
  type SketchEntity,
} from "./cad.ts";
import { checkSketch, polylineOf } from "./sketchCheck.ts";
import { splitEntityAt } from "./sketchTransform.ts";

const sub = (a: P2, b: P2): P2 => [a[0] - b[0], a[1] - b[1]];
const add = (a: P2, b: P2): P2 => [a[0] + b[0], a[1] + b[1]];
const mul = (a: P2, k: number): P2 => [a[0] * k, a[1] * k];
const dot = (a: P2, b: P2) => a[0] * b[0] + a[1] * b[1];
const cross = (a: P2, b: P2) => a[0] * b[1] - a[1] * b[0];
const len = (a: P2) => Math.hypot(a[0], a[1]);
const dist = (a: P2, b: P2) => len(sub(a, b));
const unit = (a: P2): P2 => mul(a, 1 / (len(a) || 1));
/** Normal izquierda (90° antihorario) */
const left = (a: P2): P2 => [-a[1], a[0]];

function pos(s: Sketch, id: number): P2 {
  const p = s.points.find((q) => q.id === id)!;
  return [p.x, p.y];
}

/** Tamaño del sketch para las tolerancias */
function span(s: Sketch): number {
  return s.points.reduce((m, p) => Math.max(m, Math.abs(p.x), Math.abs(p.y)), 1);
}

/** Ángulo antihorario de `a` a `b` alrededor de `c`, en (0, 2π] */
function ccwSweep(c: P2, a: P2, b: P2): number {
  let t = Math.atan2(b[1] - c[1], b[0] - c[0]) - Math.atan2(a[1] - c[1], a[0] - c[0]);
  while (t <= 1e-12) t += 2 * Math.PI;
  while (t > 2 * Math.PI) t -= 2 * Math.PI;
  return t;
}

/**
 * Junta el punto `from` con `into`: todo lo que usaba `from` pasa a usar
 * `into`. Se quitan las entidades que quedan sin largo (una línea con los dos
 * extremos iguales) y las restricciones que nombran dos veces lo mismo.
 */
export function mergePoint(s: Sketch, from: number, into: number): void {
  if (from === into) return;
  // El origen no se va nunca
  if (from === s.origin) [from, into] = [into, from];
  const f = (id: number) => (id === from ? into : id);
  for (const e of s.entities) e.geometry = mapGeometry(e.geometry, f);
  s.constraints = s.constraints.map((c) => mapConstraintIds(c, f));
  for (const t of s.texts ?? []) {
    t.anchor = f(t.anchor);
    t.points = [...new Set(t.points.map(f))];
  }
  s.points = s.points.filter((p) => p.id !== from);
  const degenerate = s.entities.filter((e) => e.geometry.type === "line" && e.geometry.start === e.geometry.end).map((e) => e.id);
  for (const id of degenerate) removeEntity(s, id);
  dropDegenerate(s);
}

/** Restricciones que nombran dos veces el mismo punto o entidad (una distancia de un punto a sí mismo) */
function dropDegenerate(s: Sketch): void {
  s.constraints = s.constraints.filter((c) => {
    const ids = constraintIds(c);
    return new Set(ids).size === ids.length;
  });
}

/** Saca la entidad `from` y lo que la nombraba pasa a `into` */
function replaceEntity(s: Sketch, from: number, into: number): void {
  const f = (id: number) => (id === from ? into : id);
  s.constraints = s.constraints.map((c) => mapConstraintIds(c, f));
  dropDegenerate(s);
  for (const t of s.texts ?? []) t.entities = [...new Set(t.entities.map(f))];
  if (s.uses) s.uses = s.uses.filter((u) => u.entity !== from);
  s.entities = s.entities.filter((e) => e.id !== from);
}

/** Puntos que nadie usa (salvo el origen y las anclas de textos) */
function dropLoose(s: Sketch): void {
  const used = new Set(s.entities.flatMap((e) => geometryPoints(e.geometry)));
  for (const t of s.texts ?? []) used.add(t.anchor);
  if (s.origin !== undefined) used.add(s.origin);
  const loose = new Set(s.points.filter((p) => !used.has(p.id)).map((p) => p.id));
  if (!loose.size) return;
  s.points = s.points.filter((p) => !loose.has(p.id));
  s.constraints = s.constraints.filter((c) => !constraintMentions(c, loose));
}

// ─── Recortar con un trazo ────────────────────────────────────────────────

/** Cruces de un segmento del trazo con una línea, círculo o arco: parámetro sobre el segmento */
function strokeHits(s: Sketch, e: SketchEntity, a: P2, b: P2): { t: number; p: P2 }[] {
  const g = e.geometry;
  const d = sub(b, a);
  if (g.type === "line") {
    const [c, q] = [pos(s, g.start), pos(s, g.end)];
    const f = sub(q, c);
    const den = cross(d, f);
    if (Math.abs(den) < 1e-15) return [];
    const t = cross(sub(c, a), f) / den;
    const u = cross(sub(c, a), d) / den;
    return t >= 0 && t <= 1 && u > 0 && u < 1 ? [{ t, p: add(a, mul(d, t)) }] : [];
  }
  if (g.type !== "circle" && g.type !== "arc") return [];
  const c = pos(s, g.center);
  const r = g.type === "circle" ? g.radius : dist(pos(s, g.start), c);
  const m = sub(a, c);
  const qa = dot(d, d);
  const qb = 2 * dot(m, d);
  const qc = dot(m, m) - r * r;
  const disc = qb * qb - 4 * qa * qc;
  if (disc < 0 || qa === 0) return [];
  const out: { t: number; p: P2 }[] = [];
  for (const t of [(-qb - Math.sqrt(disc)) / (2 * qa), (-qb + Math.sqrt(disc)) / (2 * qa)]) {
    if (t < 0 || t > 1) continue;
    const p = add(a, mul(d, t));
    if (g.type === "arc" && ccwSweep(c, pos(s, g.start), p) >= ccwSweep(c, pos(s, g.start), pos(s, g.end))) continue;
    out.push({ t, p });
  }
  return out;
}

/**
 * Recorta todo lo que cruza el trazo `stroke` (puntos del plano, en orden):
 * cada tramo tocado se quita como con un clic de Recortar, en el orden en que
 * el trazo los cruza. Devuelve cuántos se quitaron y un mensaje si algo no se
 * pudo (las splines no se recortan).
 */
export function trimByStroke(s: Sketch, stroke: P2[]): { trimmed: number; message?: string } {
  const failed = new Set<number>();
  let message: string | undefined;
  let trimmed = 0;
  // Avance sobre el trazo: lo ya pasado no se vuelve a mirar (así termina siempre)
  let at = -1;
  for (let guard = 0; guard < 500; guard++) {
    let best: { u: number; id: number; p: P2 } | undefined;
    for (let k = 0; k + 1 < stroke.length; k++) {
      if (k + 1 <= at) continue;
      for (const e of s.entities) {
        if (failed.has(e.id)) continue;
        for (const h of strokeHits(s, e, stroke[k], stroke[k + 1])) {
          const u = k + h.t;
          if (u > at + 1e-9 && (!best || u < best.u)) best = { u, id: e.id, p: h.p };
        }
      }
      if (best) break;
    }
    if (!best) break;
    at = best.u;
    const msg = trimAt(s, best.id, best.p);
    if (msg) {
      failed.add(best.id);
      message = msg;
    } else trimmed++;
  }
  // Las splines tocadas por el trazo también se avisan
  for (const e of s.entities)
    if ((e.geometry.type === "spline" || e.geometry.type === "bspline") && crossesPolyline(polylineOf(s, e.geometry), stroke)) message = "Las splines no se recortan";
  return { trimmed, message };
}

function crossesPolyline(a: P2[], b: P2[]): boolean {
  for (let i = 0; i + 1 < a.length; i++)
    for (let j = 0; j + 1 < b.length; j++) {
      const d = sub(a[i + 1], a[i]);
      const f = sub(b[j + 1], b[j]);
      const den = cross(d, f);
      if (Math.abs(den) < 1e-15) continue;
      const t = cross(sub(b[j], a[i]), f) / den;
      const u = cross(sub(b[j], a[i]), d) / den;
      if (t >= 0 && t <= 1 && u >= 0 && u <= 1) return true;
    }
  return false;
}

// ─── Unir ─────────────────────────────────────────────────────────────────

/**
 * Une dos entidades con un extremo común en una sola: dos líneas alineadas
 * pasan a ser una; dos arcos del mismo círculo, un arco (o el círculo si se
 * cierra). Lo que nombraba a la segunda pasa a la primera. Si el punto del
 * medio lo usa otra entidad, queda sobre la línea (o el arco).
 */
export function joinEntities(s: Sketch, a: number, b: number): string | undefined {
  const ea = s.entities.find((e) => e.id === a);
  const eb = s.entities.find((e) => e.id === b);
  if (!ea || !eb || a === b) return "Elegir dos líneas o dos arcos";
  const tol = span(s) * 1e-7;
  const ga = ea.geometry;
  const gb = eb.geometry;
  if (ga.type === "line" && gb.type === "line") {
    const common = [ga.start, ga.end].find((p) => p === gb.start || p === gb.end);
    if (common === undefined) return "Las líneas tienen que compartir un extremo";
    const farA = ga.start === common ? ga.end : ga.start;
    const farB = gb.start === common ? gb.end : gb.start;
    const [A, C, B] = [pos(s, farA), pos(s, common), pos(s, farB)];
    const dir = unit(sub(B, A));
    if (Math.abs(cross(dir, sub(C, A))) > tol || dot(sub(C, A), dir) <= 0 || dot(sub(B, C), dir) <= 0) return "Las líneas tienen que estar alineadas y seguidas";
    // La unida conserva el sentido de la primera
    if (ga.start === common) ga.start = farB;
    else ga.end = farB;
    // Los largos ya no valen; lo demás (horizontal, paralela, tangente...) pasa a la unida
    s.constraints = s.constraints.filter((k) => !(k.type === "length" && (k.line === a || k.line === b)));
    if (ea.construction !== eb.construction) ea.construction = ea.construction && eb.construction;
    replaceEntity(s, b, a);
    keepMiddle(s, common, a, "line");
    return undefined;
  }
  if (ga.type === "arc" && gb.type === "arc") {
    const [ca, cb] = [pos(s, ga.center), pos(s, gb.center)];
    const ra = dist(pos(s, ga.start), ca);
    const rb = dist(pos(s, gb.start), cb);
    if (dist(ca, cb) > tol || Math.abs(ra - rb) > tol) return "Los arcos tienen que ser del mismo círculo";
    let common: number;
    if (ga.end === gb.start) {
      common = ga.end;
      ga.end = gb.end;
    } else if (gb.end === ga.start) {
      common = ga.start;
      ga.start = gb.start;
    } else return "Los arcos tienen que seguirse (el fin de uno es el comienzo del otro)";
    if (gb.center !== ga.center) mergePoint(s, gb.center, ga.center);
    s.constraints = s.constraints.filter((k) => !(k.type === "arc_length" && (k.arc === a || k.arc === b)));
    replaceEntity(s, b, a);
    // Vuelta entera: pasa a ser un círculo
    const g = ea.geometry as Extract<Geometry, { type: "arc" }>;
    if (g.start === g.end) {
      ea.geometry = { type: "circle", center: g.center, radius: ra };
      if (g.start !== common) keepMiddle(s, g.start, a, "circle");
    }
    keepMiddle(s, common, a, "circle");
    return undefined;
  }
  return "Se unen dos líneas o dos arcos";
}

function usedElsewhere(s: Sketch, point: number): boolean {
  return s.entities.some((e) => geometryPoints(e.geometry).includes(point));
}

/** El punto del medio: si lo usa otra entidad queda sobre la unida; si no, se va */
function keepMiddle(s: Sketch, point: number, entity: number, kind: "line" | "circle"): void {
  if (usedElsewhere(s, point)) {
    s.constraints.push(kind === "line" ? { type: "point_on_line", point, line: entity } : { type: "point_on_circle", point, circle: entity });
    return;
  }
  s.points = s.points.filter((p) => p.id !== point);
  s.constraints = s.constraints.filter((k) => !constraintMentions(k, [point]));
}

// ─── Chaflán ──────────────────────────────────────────────────────────────

export type CornerChamferSecond = { distance: number } | { angle: number };

/**
 * Chaflán en la esquina entre las dos líneas que comparten `point`: las corta
 * a `d1` (sobre la primera) y a la segunda distancia o al ángulo dado (en
 * grados, entre el chaflán y la primera línea), y las une con una línea.
 */
export function chamferCorner(s: Sketch, point: number, d1: number, second: CornerChamferSecond = { distance: d1 }): string | undefined {
  const lines = linesAt(s, point);
  if (lines.length !== 2) return "La esquina debe unir exactamente dos líneas";
  if (!(d1 > 0)) return "La distancia tiene que ser positiva";
  const P = pos(s, point);
  const other = (e: SketchEntity) => {
    const g = e.geometry as { start: number; end: number };
    return g.start === point ? g.end : g.start;
  };
  const [A, B] = [pos(s, other(lines[0])), pos(s, other(lines[1]))];
  const u = unit(sub(A, P));
  const v = unit(sub(B, P));
  const corner = Math.acos(Math.max(-1, Math.min(1, dot(u, v))));
  if (corner < 1e-3 || Math.PI - corner < 1e-3) return "Las líneas están alineadas";
  let d2: number;
  if ("distance" in second) d2 = second.distance;
  else {
    // Ley de los senos en el triángulo esquina-T1-T2
    const a = (second.angle * Math.PI) / 180;
    const opposite = Math.PI - corner - a;
    if (!(a > 0) || opposite <= 1e-6) return "Con ese ángulo el chaflán no corta la segunda línea";
    d2 = (d1 * Math.sin(a)) / Math.sin(opposite);
  }
  if (!(d2 > 0)) return "La distancia tiene que ser positiva";
  if (d1 >= dist(A, P) || d2 >= dist(B, P)) return "El chaflán no entra en esas líneas";
  const t1 = addPoint(s, add(P, mul(u, d1)));
  const t2 = addPoint(s, add(P, mul(v, d2)));
  for (const [line, tp] of [[lines[0], t1], [lines[1], t2]] as const) {
    const g = line.geometry as { start: number; end: number };
    if (g.start === point) g.start = tp;
    else g.end = tp;
  }
  addEntity(s, { type: "line", start: t1, end: t2 });
  s.constraints = s.constraints.filter((k) => !constraintMentions(k, [point]));
  if (!usedElsewhere(s, point)) s.points = s.points.filter((q) => q.id !== point);
  s.constraints = s.constraints.filter((k) => !(k.type === "length" && (k.line === lines[0].id || k.line === lines[1].id)));
  return undefined;
}

// ─── Equidistante de cadena ───────────────────────────────────────────────

/** Tramo de una cadena recorrido en orden */
interface Piece {
  entity: number;
  reversed: boolean;
}

/**
 * Ordena las entidades (líneas y arcos) en una cadena de extremo a extremo.
 * Devuelve los tramos en orden y si cierra, o un mensaje.
 */
export function orderChain(s: Sketch, ids: number[]): { pieces: Piece[]; closed: boolean } | string {
  const ents = ids.map((id) => s.entities.find((e) => e.id === id)).filter((e): e is SketchEntity => !!e);
  if (!ents.length) return "Elegir líneas o arcos";
  if (ents.some((e) => e.geometry.type !== "line" && e.geometry.type !== "arc")) return "La cadena es de líneas y arcos";
  const tol = span(s) * 1e-9;
  const ends = (e: SketchEntity, rev: boolean): [P2, P2] => {
    const g = e.geometry as { start: number; end: number };
    const [a, b] = [pos(s, g.start), pos(s, g.end)];
    return rev ? [b, a] : [a, b];
  };
  const left = new Set(ents.map((e) => e.id));
  const byId = new Map(ents.map((e) => [e.id, e]));
  const first = ents[0];
  left.delete(first.id);
  const pieces: Piece[] = [{ entity: first.id, reversed: false }];
  // Hacia adelante y hacia atrás desde la primera
  for (const forward of [true, false]) {
    for (;;) {
      const tip = forward ? ends(byId.get(pieces[pieces.length - 1].entity)!, pieces[pieces.length - 1].reversed)[1] : ends(byId.get(pieces[0].entity)!, pieces[0].reversed)[0];
      let found: Piece | undefined;
      for (const id of left) {
        const [a, b] = ends(byId.get(id)!, false);
        if (dist(forward ? a : b, tip) <= tol) found = { entity: id, reversed: false };
        else if (dist(forward ? b : a, tip) <= tol) found = { entity: id, reversed: true };
        if (found) break;
      }
      if (!found) break;
      left.delete(found.entity);
      if (forward) pieces.push(found);
      else pieces.unshift(found);
    }
  }
  if (left.size) return "Lo elegido no forma una sola cadena";
  const head = ends(byId.get(pieces[0].entity)!, pieces[0].reversed)[0];
  const tail = ends(byId.get(pieces[pieces.length - 1].entity)!, pieces[pieces.length - 1].reversed)[1];
  return { pieces, closed: pieces.length > 1 && dist(head, tail) <= tol };
}

/** Tramo desplazado: recta o arco (con su sentido de recorrido) */
type OffsetPiece = { kind: "line"; a: P2; b: P2; from: number } | { kind: "arc"; c: P2; center: number; r: number; a: P2; b: P2; ccw: boolean; from: number };

function offsetPiece(s: Sketch, piece: Piece, d: number): OffsetPiece | string {
  const e = s.entities.find((x) => x.id === piece.entity)!;
  const g = e.geometry;
  if (g.type === "line") {
    const [a, b] = piece.reversed ? [pos(s, g.end), pos(s, g.start)] : [pos(s, g.start), pos(s, g.end)];
    const n = mul(left(unit(sub(b, a))), d);
    return { kind: "line", a: add(a, n), b: add(b, n), from: e.id };
  }
  const arc = g as Extract<Geometry, { type: "arc" }>;
  const c = pos(s, arc.center);
  const r = dist(pos(s, arc.start), c);
  // Antihorario, la izquierda es hacia el centro
  const ccw = !piece.reversed;
  const r2 = ccw ? r - d : r + d;
  if (r2 <= 1e-9 * span(s)) return "La distancia es mayor que el radio de un arco";
  const [a, b] = ccw ? [pos(s, arc.start), pos(s, arc.end)] : [pos(s, arc.end), pos(s, arc.start)];
  const on = (p: P2) => add(c, mul(unit(sub(p, c)), r2));
  return { kind: "arc", c, center: arc.center, r: r2, a: on(a), b: on(b), ccw, from: e.id };
}

/** Cruces de las curvas completas (recta infinita o círculo) de dos tramos */
function curveCrossings(p: OffsetPiece, q: OffsetPiece): P2[] {
  const lineCircle = (a: P2, b: P2, c: P2, r: number): P2[] => {
    const d = unit(sub(b, a));
    const m = sub(a, c);
    const B = dot(m, d);
    const disc = B * B - (dot(m, m) - r * r);
    if (disc < 0) return [];
    return [-B - Math.sqrt(disc), -B + Math.sqrt(disc)].map((t) => add(a, mul(d, t)));
  };
  if (p.kind === "line" && q.kind === "line") {
    const d = sub(p.b, p.a);
    const f = sub(q.b, q.a);
    const den = cross(d, f);
    if (Math.abs(den) < 1e-12 * len(d) * len(f)) return [];
    return [add(p.a, mul(d, cross(sub(q.a, p.a), f) / den))];
  }
  if (p.kind === "line" && q.kind === "arc") return lineCircle(p.a, p.b, q.c, q.r);
  if (p.kind === "arc" && q.kind === "line") return lineCircle(q.a, q.b, p.c, p.r);
  const [a, b] = [p as Extract<OffsetPiece, { kind: "arc" }>, q as Extract<OffsetPiece, { kind: "arc" }>];
  const dd = dist(a.c, b.c);
  if (dd < 1e-12 || dd > a.r + b.r || dd < Math.abs(a.r - b.r)) return [];
  const x = (a.r * a.r - b.r * b.r + dd * dd) / (2 * dd);
  const h = Math.sqrt(Math.max(0, a.r * a.r - x * x));
  const ex = unit(sub(b.c, a.c));
  const base = add(a.c, mul(ex, x));
  return [add(base, mul(left(ex), h)), add(base, mul(left(ex), -h))];
}

/**
 * Lleva los tramos desplazados a sus esquinas: donde se cruzan sus curvas
 * (lo más cerca de donde estaban) o, si no se tocan, con un arco alrededor
 * del vértice original (`at` = tramo que empieza en ese vértice).
 */
function joinOffsets(pieces: OffsetPiece[], closed: boolean, tol: number): { pieces: OffsetPiece[]; rounds: { at: number; a: P2; b: P2 }[] } {
  const rounds: { at: number; a: P2; b: P2 }[] = [];
  const n = pieces.length;
  for (let i = 0; i < (closed ? n : n - 1); i++) {
    const p = pieces[i];
    const q = pieces[(i + 1) % n];
    if (dist(p.b, q.a) <= tol) {
      q.a = p.b;
      continue;
    }
    const guess: P2 = mul(add(p.b, q.a), 0.5);
    const near = curveCrossings(p, q).sort((x, y) => dist(x, guess) - dist(y, guess))[0];
    // Un cruce lejos (rectas casi paralelas) no sirve: mejor redondear
    if (near && dist(near, guess) <= 4 * dist(p.b, q.a) + tol) {
      p.b = near;
      q.a = near;
    } else rounds.push({ at: (i + 1) % n, a: p.b, b: q.a });
  }
  return { pieces, rounds };
}

export type OffsetCaps = "open" | "round" | "line";

/**
 * Equidistante de una cadena de líneas y arcos a distancia `d` (positiva = a
 * la izquierda del recorrido; en un lazo cerrado antihorario, hacia adentro).
 * `both`: a los dos lados. `caps` cierra una cadena abierta: con arcos
 * alrededor de las puntas (a los dos lados) o con líneas. Los arcos
 * desplazados comparten el centro con el original; las líneas quedan
 * paralelas (o horizontales/verticales como la original).
 */
export function offsetChain(s: Sketch, ids: number[], d: number, opts: { both?: boolean; caps?: OffsetCaps } = {}): { entities: number[] } | string {
  const order = orderChain(s, ids);
  if (typeof order === "string") return order;
  if (!(Math.abs(d) > 0)) return "La distancia no puede ser cero";
  const { pieces, closed } = order;
  const tol = span(s) * 1e-9;
  const vertexIds = pieces.map((pc) => {
    const g = s.entities.find((e) => e.id === pc.entity)!.geometry as { start: number; end: number };
    return pc.reversed ? g.end : g.start;
  });
  const lastPiece = s.entities.find((e) => e.id === pieces[pieces.length - 1].entity)!.geometry as { start: number; end: number };
  const tailId = pieces[pieces.length - 1].reversed ? lastPiece.start : lastPiece.end;
  const sides = opts.both ? [Math.abs(d), -Math.abs(d)] : [d];
  const made: number[] = [];
  const sideEnds: { head: number; tail: number }[] = [];
  for (const side of sides) {
    const off: OffsetPiece[] = [];
    for (const pc of pieces) {
      const o = offsetPiece(s, pc, side);
      if (typeof o === "string") return o;
      off.push(o);
    }
    const { rounds } = joinOffsets(off, closed, tol);
    // Puntos: uno por esquina compartida
    const pointAt = new Map<string, number>();
    const pid = (p: P2) => {
      const key = `${p[0].toFixed(9)},${p[1].toFixed(9)}`;
      let id = pointAt.get(key);
      if (id === undefined) {
        id = addPoint(s, p);
        pointAt.set(key, id);
      }
      return id;
    };
    const roundsAt = new Map(rounds.map((r) => [r.at, r]));
    off.forEach((o, i) => {
      const r = roundsAt.get(i);
      if (r && (closed || i > 0)) {
        // Arco alrededor del vértice original: antihorario según de qué lado va
        const v = vertexIds[i];
        const [a, b] = side > 0 ? [r.b, r.a] : [r.a, r.b];
        made.push(addEntity(s, { type: "arc", center: v, start: pid(a), end: pid(b) }));
      }
      const [a, b] = [pid(o.a), pid(o.b)];
      if (o.kind === "line") {
        const id = addEntity(s, { type: "line", start: a, end: b });
        let axis = false;
        for (const k of [...s.constraints])
          if ((k.type === "horizontal" || k.type === "vertical") && k.line === o.from) {
            s.constraints.push({ type: k.type, line: id });
            axis = true;
          }
        if (!axis) s.constraints.push({ type: "parallel", a: o.from, b: id });
        made.push(id);
      } else made.push(addEntity(s, o.ccw ? { type: "arc", center: o.center, start: a, end: b } : { type: "arc", center: o.center, start: b, end: a }));
    });
    sideEnds.push({ head: pid(off[0].a), tail: pid(off[off.length - 1].b) });
  }
  // Extremos cerrados (solo en cadenas abiertas)
  const caps = closed ? "open" : (opts.caps ?? "open");
  if (caps !== "open") {
    const headId = vertexIds[0];
    if (sides.length === 2) {
      const [L, R] = sideEnds;
      if (caps === "round") {
        // Al final: de la derecha a la izquierda por delante; al comienzo, al revés
        made.push(addEntity(s, { type: "arc", center: tailId, start: R.tail, end: L.tail }));
        made.push(addEntity(s, { type: "arc", center: headId, start: L.head, end: R.head }));
      } else {
        made.push(addEntity(s, { type: "line", start: R.tail, end: L.tail }));
        made.push(addEntity(s, { type: "line", start: L.head, end: R.head }));
      }
    } else {
      made.push(addEntity(s, { type: "line", start: tailId, end: sideEnds[0].tail }));
      made.push(addEntity(s, { type: "line", start: sideEnds[0].head, end: headId }));
    }
  }
  return { entities: made };
}

// ─── Duplicados, reparar, cerrar ──────────────────────────────────────────

/** Si dos entidades son la misma curva (los mismos extremos y forma) */
function sameCurve(s: Sketch, a: Geometry, b: Geometry, tol: number): boolean {
  const near = (x: number, y: number) => dist(pos(s, x), pos(s, y)) <= tol;
  if (a.type === "line" && b.type === "line") return (near(a.start, b.start) && near(a.end, b.end)) || (near(a.start, b.end) && near(a.end, b.start));
  if (a.type === "circle" && b.type === "circle") return near(a.center, b.center) && Math.abs(a.radius - b.radius) <= tol;
  if (a.type === "arc" && b.type === "arc") return near(a.center, b.center) && near(a.start, b.start) && near(a.end, b.end);
  if (a.type === "point" && b.type === "point") return near(a.point, b.point);
  return false;
}

/**
 * Quita las entidades repetidas (la misma línea, círculo o arco dos veces).
 * Lo que nombraba a la repetida pasa a la que queda. Devuelve cuántas quitó.
 */
export function removeDuplicates(s: Sketch): number {
  const tol = span(s) * 1e-7;
  let removed = 0;
  for (let i = 0; i < s.entities.length; i++)
    for (let j = i + 1; j < s.entities.length; j++) {
      const [a, b] = [s.entities[i], s.entities[j]];
      if (!!a.construction !== !!b.construction || !sameCurve(s, a.geometry, b.geometry, tol)) continue;
      const pa = geometryPoints(a.geometry);
      const pb = geometryPoints(b.geometry);
      replaceEntity(s, b.id, a.id);
      // Sus puntos sueltos se juntan con los de la que queda
      for (const p of pb) {
        if (pa.includes(p) || !s.points.some((q) => q.id === p)) continue;
        const twin = pa.find((q) => dist(pos(s, q), pos(s, p)) <= tol);
        if (twin !== undefined) mergePoint(s, p, twin);
      }
      removed++;
      j--;
    }
  dropLoose(s);
  return removed;
}

/** Extremos de las curvas abiertas (sus puntos) */
function endPoints(s: Sketch): number[] {
  const out = new Set<number>();
  for (const e of s.entities) {
    const g = e.geometry;
    if (g.type === "line" || g.type === "arc" || g.type === "ellipse_arc") (out.add(g.start), out.add(g.end));
    else if (g.type === "spline" && !g.closed) (out.add(g.points[0]), out.add(g.points[g.points.length - 1]));
    else if (g.type === "bspline" && !g.closed) (out.add(g.poles[0]), out.add(g.poles[g.poles.length - 1]));
  }
  return [...out];
}

/** Largo de una línea o arco */
function curveSize(s: Sketch, g: Geometry): number | undefined {
  if (g.type === "line") return dist(pos(s, g.start), pos(s, g.end));
  if (g.type === "arc") {
    const c = pos(s, g.center);
    return dist(pos(s, g.start), c) * ccwSweep(c, pos(s, g.start), pos(s, g.end));
  }
  return undefined;
}

export interface RepairReport {
  /** Extremos juntados */
  joined: number;
  /** Entidades cortísimas quitadas */
  removed: number;
  /** Cruces y T partidos */
  split: number;
}

/**
 * Repara el contorno: junta los extremos a menos de `tol`, quita las líneas y
 * arcos más cortos que `tol` y parte las curvas en los cruces y las T que no
 * tienen un punto común (lo que marca la revisión).
 */
export function repairSketch(s: Sketch, tol: number): RepairReport {
  const report: RepairReport = { joined: 0, removed: 0, split: 0 };
  // Cortísimas: sus dos extremos pasan a ser uno
  for (const e of [...s.entities]) {
    if (!s.entities.includes(e)) continue;
    const size = curveSize(s, e.geometry);
    if (size === undefined || size >= tol) continue;
    const g = e.geometry as { start: number; end: number };
    const [a, b] = [g.start, g.end];
    removeEntity(s, e.id);
    if (s.points.some((p) => p.id === a) && s.points.some((p) => p.id === b)) mergePoint(s, b, a);
    report.removed++;
  }
  // Extremos cercanos
  const ends = endPoints(s);
  const gone = new Set<number>();
  for (let i = 0; i < ends.length; i++) {
    if (gone.has(ends[i])) continue;
    for (let j = i + 1; j < ends.length; j++) {
      if (gone.has(ends[j]) || !s.points.some((p) => p.id === ends[j])) continue;
      if (dist(pos(s, ends[i]), pos(s, ends[j])) > tol) continue;
      mergePoint(s, ends[j], ends[i]);
      gone.add(ends[j]);
      report.joined++;
    }
  }
  // Cruces y T: cada curva se parte ahí (o usa su extremo) y los puntos se juntan
  for (let guard = 0; guard < 200; guard++) {
    const x = checkSketch(s).crossings.find((c) => {
      const t = (id: number) => s.entities.find((e) => e.id === id)?.geometry.type;
      return [t(c.a), t(c.b)].every((k) => k === "line" || k === "arc" || k === "circle");
    });
    if (!x) break;
    const at = (id: number): number | undefined => {
      const e = s.entities.find((q) => q.id === id)!;
      const own = geometryPoints(e.geometry).find((p) => p !== (e.geometry as { center?: number }).center && dist(pos(s, p), x.p) <= Math.max(tol, span(s) * 1e-7));
      if (own !== undefined) return own;
      const r = splitEntityAt(s, id, x.p);
      return typeof r === "number" ? r : undefined;
    };
    const pa = at(x.a);
    const pb = at(x.b);
    if (pa === undefined || pb === undefined) break;
    // Al mismo lugar exacto y juntos
    const pp = s.points.find((p) => p.id === pa)!;
    [pp.x, pp.y] = x.p;
    mergePoint(s, pb, pa);
    report.split++;
  }
  dropLoose(s);
  return report;
}

/**
 * Cierra el contorno: une con una línea los extremos sueltos de a pares,
 * primero los más cercanos. Devuelve las líneas nuevas.
 */
export function closeContour(s: Sketch): number[] {
  const loose = checkSketch(s).looseEnds;
  const tol = span(s) * 1e-7;
  const ids = loose
    .map((l) => {
      const e = s.entities.find((x) => x.id === l.entity);
      const pts = e ? geometryPoints(e.geometry) : [];
      return pts.sort((a, b) => dist(pos(s, a), l.p) - dist(pos(s, b), l.p)).find((p) => dist(pos(s, p), l.p) <= tol);
    })
    .filter((x): x is number => x !== undefined);
  const free = [...new Set(ids)];
  const pairs: [number, number, number][] = [];
  for (let i = 0; i < free.length; i++) for (let j = i + 1; j < free.length; j++) pairs.push([free[i], free[j], dist(pos(s, free[i]), pos(s, free[j]))]);
  pairs.sort((a, b) => a[2] - b[2]);
  const used = new Set<number>();
  const made: number[] = [];
  for (const [a, b] of pairs) {
    if (used.has(a) || used.has(b)) continue;
    // Los dos extremos de una misma línea no se unen entre sí
    if (s.entities.some((e) => e.geometry.type === "line" && geometryPoints(e.geometry).includes(a) && geometryPoints(e.geometry).includes(b))) continue;
    used.add(a);
    used.add(b);
    made.push(addEntity(s, { type: "line", start: a, end: b }));
  }
  return made;
}
