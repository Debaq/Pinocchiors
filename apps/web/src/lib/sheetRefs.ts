// Las vistas del plano como referencia del dibujo sobre la hoja: sus líneas en
// mm de hoja (Y hacia arriba, como el sketch de la hoja) para anclar, recortar
// y extender; «Usar» una línea de vista (queda ligada: sigue al modelo); y las
// cotas del dibujo puestas en la hoja.
// Funciones puras: se prueban con node (e2e/sheetRefs.test.mjs).

import { constraintIds, geometryPoints, type Geometry, type Projected, type Sketch, type SketchConstraint } from "./cad.ts";
import { angleView } from "./sketchTransform.ts";
import { isPictorial, asArc, asCircle, asSegment, bounds, lineNear, sheetBox, type AnyDimension, type DrawingLine, type P2, type Placed } from "./drawing.ts";

/** Una entidad del dibujo ligada a una línea de una vista (`at`: un punto de la línea, en mm de la pieza) */
export interface SheetLink {
  entity: number;
  view: string;
  at: P2;
}

/** Punto de la vista (mm de la pieza) → mm de hoja con Y hacia arriba */
export const viewToSheet = (v: Placed, p: P2, height: number): P2 => [v.x + p[0] * v.scale, height - v.y + p[1] * v.scale];
/** Al revés */
export const sheetToView = (v: Placed, q: P2, height: number): P2 => [(q[0] - v.x) / v.scale, (q[1] - height + v.y) / v.scale];

/** Qué líneas de una vista cuentan (las ocultas solo si se dibujan; la isométrica sin ocultas) */
function counts(v: Placed, l: DrawingLine, hidden: boolean) {
  if (l.kind === "smooth") return false;
  if (l.kind === "hidden" || l.kind === "hidden_outline") return hidden && !isPictorial(v.name);
  return true;
}

/** Las líneas de todas las vistas en mm de hoja (para anclar, recortar y extender) */
export function viewPolylines(placed: Placed[], height: number, hidden = true): P2[][] {
  return placed.flatMap((v) => v.lines.filter((l) => counts(v, l, hidden)).map((l) => l.points.map((p) => viewToSheet(v, p, height))));
}

/** La línea de vista más cercana a `q` (mm de hoja), a menos de `tol` mm de hoja */
export function nearestViewLine(placed: Placed[], height: number, q: P2, tol: number, hidden = true): { view: Placed; line: DrawingLine; at: P2 } | undefined {
  let best: { view: Placed; line: DrawingLine; at: P2; d: number } | undefined;
  // Primero los detalles (están encima)
  for (const v of [...placed].sort((a, b) => Number(!!b.detail) - Number(!!a.detail))) {
    const p = sheetToView(v, q, height);
    const line = lineNear(v, p, tol / v.scale, (l) => counts(v, l, hidden));
    if (!line) continue;
    const d = distTo(p, line.points) * v.scale;
    if (!best || d < best.d - 1e-9) best = { view: v, line, at: closestOn(p, line.points), d };
    if (v.detail) break;
  }
  return best && { view: best.view, line: best.line, at: best.at };
}

function closestOn(p: P2, pts: P2[]): P2 {
  let best: P2 = pts[0];
  let bd = Infinity;
  for (let i = 0; i + 1 < pts.length; i++) {
    const [a, b] = [pts[i], pts[i + 1]];
    const d: P2 = [b[0] - a[0], b[1] - a[1]];
    const l2 = d[0] * d[0] + d[1] * d[1] || 1;
    const t = Math.max(0, Math.min(1, ((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / l2));
    const q: P2 = [a[0] + t * d[0], a[1] + t * d[1]];
    const dq = Math.hypot(q[0] - p[0], q[1] - p[1]);
    if (dq < bd) [bd, best] = [dq, q];
  }
  return best;
}
const distTo = (p: P2, pts: P2[]) => {
  const q = closestOn(p, pts);
  return Math.hypot(q[0] - p[0], q[1] - p[1]);
};

/**
 * Una línea de vista como curva del sketch de la hoja: recta, círculo, arco
 * (antihorario, como los del sketch) o spline por hasta 24 de sus puntos
 */
export function projectViewLine(v: Placed, line: DrawingLine, height: number): Projected {
  const S = (p: P2) => viewToSheet(v, p, height);
  const seg = asSegment(line);
  if (seg) return { kind: "line", start: S(seg[0]), end: S(seg[1]) };
  const circ = asCircle(line);
  if (circ) return { kind: "circle", center: S(circ.c), radius: circ.r * v.scale };
  const arc = asArc(line);
  if (arc) {
    const [a, b] = [line.points[0], line.points[line.points.length - 1]];
    const ccw = (a[0] - arc.c[0]) * (arc.mid[1] - arc.c[1]) - (a[1] - arc.c[1]) * (arc.mid[0] - arc.c[0]) > 0;
    return { kind: "arc", center: S(arc.c), start: S(ccw ? a : b), end: S(ccw ? b : a) };
  }
  const pts = line.points;
  const [f, z] = [pts[0], pts[pts.length - 1]];
  const closed = pts.length > 3 && Math.hypot(f[0] - z[0], f[1] - z[1]) < 1e-6 * Math.max(1, Math.hypot(...f));
  const ring = closed ? pts.slice(0, -1) : pts;
  const n = Math.min(ring.length, 24);
  const pick = Array.from({ length: n }, (_, i) => ring[Math.round((i * (ring.length - 1)) / Math.max(1, n - 1))]);
  return { kind: "spline", points: pick.map(S), closed };
}

/** Lleva la geometría de una entidad a la de una curva proyectada (si son del mismo tipo) */
function placeEntity(s: Sketch, g: Geometry, c: Projected): boolean {
  const set = (id: number, p: P2) => {
    const q = s.points.find((x) => x.id === id);
    if (q) [q.x, q.y] = p;
  };
  if (g.type === "line" && c.kind === "line") {
    set(g.start, c.start);
    set(g.end, c.end);
  } else if (g.type === "circle" && c.kind === "circle") {
    set(g.center, c.center);
    g.radius = c.radius;
  } else if (g.type === "arc" && c.kind === "arc") {
    set(g.center, c.center);
    set(g.start, c.start);
    set(g.end, c.end);
  } else if (g.type === "spline" && c.kind === "spline" && g.points.length === c.points.length) {
    g.points.forEach((id, i) => set(id, c.points[i]));
  } else return false;
  return true;
}

/**
 * Las entidades ligadas, a donde están hoy sus líneas (otra escala, otro
 * lugar, el modelo cambió). Devuelve el sketch nuevo, los vínculos con su
 * punto al día y cuántos se perdieron (la línea ya no está o cambió de tipo:
 * la entidad queda donde estaba).
 */
export function applySheetLinks(sketch: Sketch, links: SheetLink[], placed: Placed[], height: number): { sketch: Sketch; links: SheetLink[]; lost: number } {
  const s: Sketch = structuredClone(sketch);
  const used = new Set((s.uses ?? []).map((u) => u.entity));
  let lost = 0;
  const out: SheetLink[] = [];
  for (const link of links) {
    const e = s.entities.find((x) => x.id === link.entity);
    if (!e || !used.has(link.entity)) continue;
    const v = placed.find((x) => x.name === link.view);
    const b = v && bounds(v.lines);
    const far = b ? Math.hypot(b.max[0] - b.min[0], b.max[1] - b.min[1]) * 0.15 : 0;
    const kind = e.geometry.type;
    const same = (l: DrawingLine) => {
      const k = asSegment(l) ? "line" : asCircle(l) ? "circle" : asArc(l) ? "arc" : "spline";
      return k === kind;
    };
    const line = v && (lineNear(v, link.at, 3 / v.scale, same) ?? lineNear(v, link.at, far, same));
    if (!v || !line || !placeEntity(s, e.geometry, projectViewLine(v, line, height))) {
      lost++;
      out.push(link);
      continue;
    }
    out.push({ ...link, at: closestOn(link.at, line.points) });
  }
  return { sketch: s, links: out, lost };
}

// ─── Lo dibujado sobre una vista se mueve con ella ──────────────────────────

/**
 * Una entidad dibujada encima de una vista: se mueve (y se escala) con ella.
 * `frame` es dónde estaba el origen de la vista (mm de hoja, Y hacia arriba)
 * y su escala cuando se dibujó.
 */
export interface SheetOwner {
  entity: number;
  view: string;
  frame: [number, number, number];
}

const frameOf = (v: Placed, height: number): [number, number, number] => [...viewToSheet(v, [0, 0], height), v.scale];

/**
 * A qué vista pertenece cada entidad que no está ligada: la que la contiene
 * entera (con 3 mm de margen; los detalles primero). Las que no caen en
 * ninguna quedan de la hoja.
 */
export function sheetOwners(s: Sketch, placed: Placed[], height: number): SheetOwner[] {
  const used = new Set((s.uses ?? []).map((u) => u.entity));
  const pt = new Map(s.points.map((p) => [p.id, [p.x, p.y] as P2]));
  const boxes = [...placed]
    .filter((v) => v.lines.length)
    .sort((a, b) => Number(!!b.detail) - Number(!!a.detail))
    .map((v) => {
      const b = sheetBox(v);
      // A mm de hoja con Y hacia arriba, con margen
      return { v, b: [b[0] - 3, height - b[3] - 3, b[2] + 3, height - b[1] + 3] };
    });
  const out: SheetOwner[] = [];
  for (const e of s.entities) {
    if (used.has(e.id)) continue;
    const pts = geometryPoints(e.geometry).flatMap((id) => (pt.has(id) ? [pt.get(id)!] : []));
    if (e.geometry.type === "circle") {
      const c = pts[0];
      const r = e.geometry.radius;
      if (c) pts.push([c[0] - r, c[1] - r], [c[0] + r, c[1] + r]);
    }
    if (!pts.length) continue;
    const box = boxes.find(({ b }) => pts.every((p) => p[0] >= b[0] && p[0] <= b[2] && p[1] >= b[1] && p[1] <= b[3]));
    if (box) out.push({ entity: e.id, view: box.v.name, frame: frameOf(box.v, height) });
  }
  return out;
}

/**
 * Lleva lo dibujado sobre cada vista a donde está hoy (corrida o con otra
 * escala): mueve sus puntos, los fijos y escala sus cotas. Devuelve el
 * sketch nuevo y los dueños con el lugar al día.
 */
export function applySheetOwners(sketch: Sketch, owners: SheetOwner[], placed: Placed[], height: number): { sketch: Sketch; owners: SheetOwner[] } {
  const s: Sketch = structuredClone(sketch);
  const moved = new Set<number>();
  const ratios = new Map<number, number>();
  const out: SheetOwner[] = [];
  for (const o of owners) {
    const e = s.entities.find((x) => x.id === o.entity);
    const v = placed.find((x) => x.name === o.view);
    if (!e) continue;
    if (!v) {
      out.push(o);
      continue;
    }
    const f = frameOf(v, height);
    out.push({ ...o, frame: f });
    const k = f[2] / o.frame[2];
    if (Math.abs(f[0] - o.frame[0]) < 1e-9 && Math.abs(f[1] - o.frame[1]) < 1e-9 && Math.abs(k - 1) < 1e-12) continue;
    const map = (p: P2): P2 => [f[0] + (p[0] - o.frame[0]) * k, f[1] + (p[1] - o.frame[1]) * k];
    for (const id of geometryPoints(e.geometry)) {
      if (moved.has(id)) continue;
      const q = s.points.find((x) => x.id === id);
      if (!q) continue;
      [q.x, q.y] = map([q.x, q.y]);
      moved.add(id);
      ratios.set(id, k);
    }
    if (e.geometry.type === "circle") e.geometry.radius *= k;
    moved.add(e.id);
    ratios.set(e.id, k);
  }
  if (moved.size) {
    for (const c of s.constraints) {
      if (c.type === "fixed" && moved.has(c.point)) {
        const q = s.points.find((x) => x.id === c.point);
        if (q) [c.x, c.y] = [q.x, q.y];
        continue;
      }
      // Cotas de lo que se escaló entero: el valor nuevo (los ángulos no cambian)
      const ids = constraintIds(c);
      const v = c as { value?: number };
      if (typeof v.value === "number" && ids.length && ids.every((id) => moved.has(id))) v.value *= ratios.get(ids[0]) ?? 1;
    }
  }
  return { sketch: s, owners: out };
}

// ─── Cotas del dibujo en la hoja ────────────────────────────────────────────

/**
 * Las cotas del sketch de la hoja como cotas del plano (en coordenadas del
 * SVG: Y hacia abajo): largos, distancias, horizontales y verticales, radios,
 * diámetros y ángulos entre rectas. El valor es lo que mide la geometría.
 * Si el texto se dejó en un lugar (`opts.offset`, mm de la hoja respecto de
 * donde le toca, como en el editor) la cota sale ahí y no se corre sola
 * (`placed`); si no, 7 mm hacia afuera del dibujo.
 */
export function sheetSketchDims(s: Sketch | null | undefined, height: number): AnyDimension[] {
  if (!s) return [];
  const pt = new Map(s.points.map((p) => [p.id, [p.x, p.y] as P2]));
  const ent = new Map(s.entities.map((e) => [e.id, e.geometry]));
  const svg = (p: P2): P2 => [p[0], height - p[1]];
  const out: AnyDimension[] = [];
  // Centro de lo dibujado: las cotas sin lugar elegido van hacia afuera de él
  const centroid: P2 = s.points.length ? [s.points.reduce((a, p) => a + p.x, 0) / s.points.length, s.points.reduce((a, p) => a + p.y, 0) / s.points.length] : [0, 0];
  const dot = (a: P2, b: P2) => a[0] * b[0] + a[1] * b[1];
  const sub = (a: P2, b: P2): P2 => [a[0] - b[0], a[1] - b[1]];
  /** Dónde dejó el usuario el texto (mm de la hoja, Y arriba) */
  const textAt = (k: SketchConstraint, anchor: P2): P2 | undefined => {
    const o = (k as { opts?: { offset?: [number, number] } }).opts?.offset;
    return o ? [anchor[0] + o[0], anchor[1] + o[1]] : undefined;
  };
  /** Cota lineal de `a` a `b`; `side`: hacia dónde se corre si no hay lugar elegido */
  const linear = (a: P2, b: P2, value: number, at: P2 | undefined, side?: P2, prefix?: string) => {
    const d: P2 = [b[0] - a[0], b[1] - a[1]];
    const l = Math.hypot(...d) || 1;
    const u: P2 = [d[0] / l, d[1] / l];
    let nrm: P2 = [-u[1], u[0]];
    const mid: P2 = [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2];
    if (at) {
      // La línea de cota pasa por el texto: el corrimiento perpendicular y lo que se corrió a lo largo
      const r = sub(at, a);
      const k = dot(r, nrm);
      out.push({ a: svg(a), b: svg(b), offset: [nrm[0] * k, -nrm[1] * k], along: dot(sub(at, mid), u), value, prefix, placed: true });
      return;
    }
    if (side) nrm = side;
    else if (dot(sub(mid, centroid), nrm) < 0) nrm = [-nrm[0], -nrm[1]];
    const off: P2 = prefix ? [0, 0] : [nrm[0] * 7, -nrm[1] * 7];
    out.push({ a: svg(a), b: svg(b), offset: off, value, prefix });
  };
  const ends = (id: number): [P2, P2] | undefined => {
    const g = ent.get(id);
    return g?.type === "line" ? [pt.get(g.start)!, pt.get(g.end)!] : undefined;
  };
  const circle = (id: number): { c: P2; r: number } | undefined => {
    const g = ent.get(id);
    if (g?.type === "circle") return { c: pt.get(g.center)!, r: g.radius };
    if (g?.type === "arc") {
      const c = pt.get(g.center)!;
      const a = pt.get(g.start)!;
      return { c, r: Math.hypot(a[0] - c[0], a[1] - c[1]) };
    }
    return undefined;
  };
  for (const k of s.constraints as SketchConstraint[]) {
    if (k.type === "length") {
      const e = ends(k.line);
      if (e) linear(e[0], e[1], Math.hypot(e[1][0] - e[0][0], e[1][1] - e[0][1]), textAt(k, [(e[0][0] + e[1][0]) / 2, (e[0][1] + e[1][1]) / 2]));
    } else if (k.type === "distance") {
      const [a, b] = [pt.get(k.a), pt.get(k.b)];
      if (a && b) linear(a, b, Math.hypot(b[0] - a[0], b[1] - a[1]), textAt(k, [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2]));
    } else if (k.type === "horizontal_distance" || k.type === "vertical_distance") {
      const [a, b] = [pt.get(k.a), pt.get(k.b)];
      if (!a || !b) continue;
      // Medida sobre el eje, a la altura (o en la columna) del primero
      const h = k.type === "horizontal_distance";
      const b2: P2 = h ? [b[0], a[1]] : [a[0], b[1]];
      const away: P2 = h ? [0, a[1] >= b[1] ? 1 : -1] : [a[0] >= b[0] ? 1 : -1, 0];
      linear(a, b2, Math.abs(h ? b[0] - a[0] : b[1] - a[1]), textAt(k, [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2]), away);
    } else if (k.type === "radius") {
      const c = circle(k.entity);
      if (!c) continue;
      const at = textAt(k, [c.c[0] + c.r * Math.SQRT1_2, c.c[1] + c.r * Math.SQRT1_2]);
      const far = at ? Math.hypot(...sub(at, c.c)) : 0;
      // Flecha hacia el texto, que queda donde se dejó (afuera del arco)
      const u: P2 = at && far > 1e-9 ? [(at[0] - c.c[0]) / far, (at[1] - c.c[1]) / far] : [Math.SQRT1_2, Math.SQRT1_2];
      const p: P2 = [c.c[0] + c.r * u[0], c.c[1] + c.r * u[1]];
      out.push({ kind: "radius", c: svg(c.c), p: svg(p), value: c.r, ...(at ? { tail: Math.max(2, far - c.r), placed: true } : {}) });
    } else if (k.type === "diameter") {
      const c = circle(k.entity);
      if (!c) continue;
      const at = textAt(k, [c.c[0] + c.r * Math.SQRT1_2, c.c[1] + c.r * Math.SQRT1_2]);
      const far = at ? Math.hypot(...sub(at, c.c)) : 0;
      // Con lugar elegido: el diámetro en la dirección del texto (la línea llega hasta él)
      const u: P2 = at && far > 1e-9 ? [(at[0] - c.c[0]) / far, (at[1] - c.c[1]) / far] : [1, 0];
      linear([c.c[0] - c.r * u[0], c.c[1] - c.r * u[1]], [c.c[0] + c.r * u[0], c.c[1] + c.r * u[1]], 2 * c.r, at, [0, 0], "Ø");
    } else if (k.type === "angle") {
      // La misma cuenta que el editor y el solver (líneas, arcos, elipses)
      const v = angleView(s, k);
      if (!v) continue;
      const a = { x: v.x, t0: Math.atan2(v.u1[1], v.u1[0]), t1: Math.atan2(v.u2[1], v.u2[0]), reach: v.reach };
      let r = Math.max(8, a.reach * 0.6);
      // El editor pone el texto sobre la bisectriz, un poco afuera del arco
      const bis: P2 = [Math.cos(a.t0) + Math.cos(a.t1), Math.sin(a.t0) + Math.sin(a.t1)];
      const bl = Math.hypot(...bis) || 1;
      const arcR = Math.min(12, 0.8 * a.reach);
      const at = textAt(k, [a.x[0] + (bis[0] / bl) * arcR * 1.35, a.x[1] + (bis[1] / bl) * arcR * 1.35]);
      if (at) r = Math.max(3, Math.hypot(...sub(at, a.x)) - 3);
      const far = (u: P2): P2 => [a.x[0] + u[0] * a.reach, a.x[1] + u[1] * a.reach];
      out.push({ kind: "angle", c: svg(a.x), r, a0: -a.t0, a1: -a.t1, value: Math.abs(v.degrees), ends: [svg(far(v.u1)), svg(far(v.u2))], ...(at ? { placed: true } : {}) });
    }
  }
  return out;
}
