// Inferencias del sketch (anclajes): a qué punto significativo se pega el
// cursor mientras se dibuja. Solo calcula; qué restricción deja cada una lo
// hace `placeSnap` en cad.ts. Sin dependencias de ejecución para probarlo con
// node (e2e/sketchSnap.test.mjs).

import type { P2, Sketch } from "./cad";

export type SnapKind =
  /** Punto existente (extremo, vértice) */
  | "point"
  /** Origen del sketch */
  | "origin"
  /** Centro de un círculo o arco (también es un punto existente) */
  | "center"
  /** Cruce de dos curvas */
  | "intersection"
  /** Punto medio de una línea */
  | "midpoint"
  /** Cuadrante de un círculo o arco (0°, 90°, 180°, 270°) */
  | "quadrant"
  /** Sobre una línea */
  | "on_line"
  /** Sobre un círculo o arco */
  | "on_circle"
  /** Alineado en horizontal o vertical con otros puntos */
  | "aligned"
  /** Línea horizontal o vertical (dibujando desde un punto) */
  | "horizontal"
  | "vertical"
  /** Línea paralela a otra (dibujando desde un punto) */
  | "parallel"
  /** Línea perpendicular a otra */
  | "perpendicular"
  /** Línea tangente al arco del que sale */
  | "tangent"
  /** Nada cerca */
  | "free";

export interface Snap {
  p: P2;
  kind: SnapKind;
  /** Punto existente (point, origin, center) */
  id?: number;
  /** Línea del punto medio, curva del cuadrante o curva sobre la que cae */
  entity?: number;
  /** Intersección: la otra curva */
  other?: number;
  /** Cuadrante: 0 = +x, 1 = +y, 2 = −x, 3 = −y desde el centro */
  quadrant?: 0 | 1 | 2 | 3;
  /** Alineado: `h` = punto a la misma altura, `v` = punto en la misma vertical */
  align?: { h?: number; v?: number };
  /** La línea que se dibuja desde `from` queda horizontal o vertical */
  axis?: "horizontal" | "vertical";
  /** Dirección de la línea que se dibuja desde `from` respecto de otra entidad */
  direction?: { kind: "parallel" | "perpendicular" | "tangent"; entity: number };
  /** Líneas guía punteadas (para dibujar) */
  guides?: [P2, P2][];
}

export interface InferOptions {
  /** Puntos que no cuentan para nada (p. ej. el que se arrastra) */
  exclude?: number[];
  /** Punto desde el que se dibuja una línea: habilita paralela, perpendicular y tangente */
  from?: number;
  /** Puntos con los que no alinearse (los de la forma en curso) */
  noAlign?: number[];
}

/** Prioridad cuando hay varios candidatos cerca (menor gana), como Onshape */
const PRIORITY: Record<SnapKind, number> = {
  point: 0,
  center: 0,
  origin: 1,
  intersection: 2,
  midpoint: 3,
  quadrant: 5,
  on_line: 7,
  on_circle: 7,
  aligned: 8,
  horizontal: 8,
  vertical: 8,
  parallel: 8,
  perpendicular: 8,
  tangent: 8,
  free: 9,
};

export const SNAP_GLYPHS: Record<SnapKind, { glyph: string; label: string }> = {
  point: { glyph: "●", label: "Punto" },
  origin: { glyph: "◎", label: "Origen" },
  center: { glyph: "⊙", label: "Centro" },
  intersection: { glyph: "✕", label: "Intersección" },
  midpoint: { glyph: "△", label: "Punto medio" },
  quadrant: { glyph: "◇", label: "Cuadrante" },
  on_line: { glyph: "∕", label: "Sobre la línea" },
  on_circle: { glyph: "◠", label: "Sobre la curva" },
  aligned: { glyph: "┆", label: "Alineado" },
  horizontal: { glyph: "—", label: "Horizontal" },
  vertical: { glyph: "|", label: "Vertical" },
  parallel: { glyph: "∥", label: "Paralela" },
  perpendicular: { glyph: "⊥", label: "Perpendicular" },
  tangent: { glyph: "◡", label: "Tangente" },
  free: { glyph: "", label: "" },
};

const dist = (a: P2, b: P2) => Math.hypot(a[0] - b[0], a[1] - b[1]);

/** Ángulo de `p` respecto de `c` medido desde `a0`, en [0, 2π) */
function relAngle(p: P2, c: P2, a0: number): number {
  let t = Math.atan2(p[1] - c[1], p[0] - c[0]) - a0;
  while (t < 0) t += 2 * Math.PI;
  while (t >= 2 * Math.PI) t -= 2 * Math.PI;
  return t;
}

/** Curva de un sketch en coordenadas: segmento o círculo (con barrido si es arco) */
export type Curve =
  | { id: number; kind: "line"; a: P2; b: P2 }
  | { id: number; kind: "circle"; c: P2; r: number; a0?: number; sweep?: number };

function curveOf(pt: Map<number, P2>, e: Sketch["entities"][number]): Curve | undefined {
  const g = e.geometry;
  if (g.type === "line") {
    const [a, b] = [pt.get(g.start), pt.get(g.end)];
    return a && b ? { id: e.id, kind: "line", a, b } : undefined;
  }
  if (g.type === "circle") {
    const c = pt.get(g.center);
    return c ? { id: e.id, kind: "circle", c, r: g.radius } : undefined;
  }
  if (g.type === "arc") {
    const [c, a, b] = [pt.get(g.center), pt.get(g.start), pt.get(g.end)];
    if (!c || !a || !b) return undefined;
    const a0 = Math.atan2(a[1] - c[1], a[0] - c[0]);
    let sweep = Math.atan2(b[1] - c[1], b[0] - c[0]) - a0;
    while (sweep <= 1e-12) sweep += 2 * Math.PI;
    return { id: e.id, kind: "circle", c, r: dist(c, a), a0, sweep };
  }
  return undefined;
}

/** Distancia de `p` a la curva (los arcos como círculo entero: es solo un filtro) */
function curveDist(p: P2, k: Curve): number {
  if (k.kind === "circle") return Math.abs(dist(p, k.c) - k.r);
  const dx = k.b[0] - k.a[0];
  const dy = k.b[1] - k.a[1];
  const l2 = dx * dx + dy * dy;
  const t = l2 > 0 ? Math.max(0, Math.min(1, ((p[0] - k.a[0]) * dx + (p[1] - k.a[1]) * dy) / l2)) : 0;
  return dist(p, [k.a[0] + t * dx, k.a[1] + t * dy]);
}

const EPS = 1e-9;

/** El punto (que está sobre la recta o el círculo completo) cae dentro del tramo */
function within(k: Curve, p: P2): boolean {
  if (k.kind === "line") {
    const dx = k.b[0] - k.a[0];
    const dy = k.b[1] - k.a[1];
    const t = ((p[0] - k.a[0]) * dx + (p[1] - k.a[1]) * dy) / (dx * dx + dy * dy || 1);
    return t >= -EPS && t <= 1 + EPS;
  }
  return k.sweep === undefined || relAngle(p, k.c, k.a0!) <= k.sweep + EPS;
}

/** Puntos de cruce de dos curvas (rectas y círculos completos, después se recortan) */
export function crossings(k1: Curve, k2: Curve): P2[] {
  let out: P2[] = [];
  if (k1.kind === "line" && k2.kind === "line") {
    const d1: P2 = [k1.b[0] - k1.a[0], k1.b[1] - k1.a[1]];
    const d2: P2 = [k2.b[0] - k2.a[0], k2.b[1] - k2.a[1]];
    const den = d1[0] * d2[1] - d1[1] * d2[0];
    if (Math.abs(den) < 1e-12 * Math.hypot(...d1) * Math.hypot(...d2)) return []; // paralelas
    const t = ((k2.a[0] - k1.a[0]) * d2[1] - (k2.a[1] - k1.a[1]) * d2[0]) / den;
    out = [[k1.a[0] + t * d1[0], k1.a[1] + t * d1[1]]];
  } else if (k1.kind === "circle" && k2.kind === "circle") {
    const dx = k2.c[0] - k1.c[0];
    const dy = k2.c[1] - k1.c[1];
    const d = Math.hypot(dx, dy);
    if (d < 1e-12 || d > k1.r + k2.r || d < Math.abs(k1.r - k2.r)) return [];
    const a = (k1.r * k1.r - k2.r * k2.r + d * d) / (2 * d);
    const h = Math.sqrt(Math.max(0, k1.r * k1.r - a * a));
    const base: P2 = [k1.c[0] + (a * dx) / d, k1.c[1] + (a * dy) / d];
    out = [
      [base[0] - (h * dy) / d, base[1] + (h * dx) / d],
      [base[0] + (h * dy) / d, base[1] - (h * dx) / d],
    ];
  } else {
    const [l, c] = (k1.kind === "line" ? [k1, k2] : [k2, k1]) as [Extract<Curve, { kind: "line" }>, Extract<Curve, { kind: "circle" }>];
    const d: P2 = [l.b[0] - l.a[0], l.b[1] - l.a[1]];
    const m: P2 = [l.a[0] - c.c[0], l.a[1] - c.c[1]];
    const qa = d[0] * d[0] + d[1] * d[1];
    const qb = 2 * (m[0] * d[0] + m[1] * d[1]);
    const qc = m[0] * m[0] + m[1] * m[1] - c.r * c.r;
    const disc = qb * qb - 4 * qa * qc;
    if (qa === 0 || disc < 0) return [];
    out = [(-qb - Math.sqrt(disc)) / (2 * qa), (-qb + Math.sqrt(disc)) / (2 * qa)].map((u) => [l.a[0] + u * d[0], l.a[1] + u * d[1]] as P2);
  }
  return out.filter((p) => within(k1, p) && within(k2, p));
}

/** Direcciones de salida desde `from`: paralela o perpendicular a una línea, tangente a su arco */
function directionSnap(s: Sketch, pt: Map<number, P2>, cursor: P2, tol: number, from: number): Snap | undefined {
  const f = pt.get(from);
  if (!f) return undefined;
  const v: P2 = [cursor[0] - f[0], cursor[1] - f[1]];
  let best: { snap: Snap; d: number } | undefined;
  const tryDir = (u: P2, kind: "parallel" | "perpendicular" | "tangent", entity: number) => {
    // Horizontal y vertical las pone la restricción de eje de la línea
    if (Math.abs(u[0]) < 1e-9 || Math.abs(u[1]) < 1e-9) return;
    const t = v[0] * u[0] + v[1] * u[1];
    const p: P2 = [f[0] + t * u[0], f[1] + t * u[1]];
    const d = dist(cursor, p);
    if (d <= tol && Math.abs(t) > tol && (!best || d < best.d)) best = { snap: { p, kind, direction: { kind, entity }, guides: [[f, p]] }, d };
  };
  for (const e of s.entities) {
    const g = e.geometry;
    if (g.type === "line") {
      const [a, b] = [pt.get(g.start), pt.get(g.end)];
      if (!a || !b) continue;
      const l = dist(a, b);
      if (l < 1e-12) continue;
      const u: P2 = [(b[0] - a[0]) / l, (b[1] - a[1]) / l];
      tryDir(u, "parallel", e.id);
      tryDir([-u[1], u[0]], "perpendicular", e.id);
    } else if (g.type === "arc" && (g.start === from || g.end === from)) {
      const c = pt.get(g.center);
      if (!c) continue;
      const r = dist(c, f);
      if (r < 1e-12) continue;
      tryDir([-(f[1] - c[1]) / r, (f[0] - c[0]) / r], "tangent", e.id);
    }
  }
  return best?.snap;
}

/**
 * Línea horizontal o vertical desde `from`; se combina con la alineación del
 * otro eje (p. ej. horizontal y a la altura… en la vertical de otro punto).
 */
function axisSnap(s: Sketch, pt: Map<number, P2>, cursor: P2, tol: number, from: number, skip: Set<number>): Snap | undefined {
  const f = pt.get(from);
  if (!f) return undefined;
  const [dx, dy] = [Math.abs(cursor[0] - f[0]), Math.abs(cursor[1] - f[1])];
  if ((dx <= tol && dy <= tol) || Math.min(dx, dy) > tol) return undefined;
  const axis = dy <= dx ? "horizontal" : "vertical";
  // El otro eje: alineado con el punto más cercano en esa dirección
  const k = axis === "horizontal" ? 0 : 1;
  let best: { id: number; q: P2; d: number } | undefined;
  for (const q of s.points) {
    const qq: P2 = [q.x, q.y];
    const d = Math.abs(cursor[k] - qq[k]);
    if (skip.has(q.id) || Math.abs(qq[k] - f[k]) < 1e-9 || d > tol || (best && d >= best.d)) continue;
    best = { id: q.id, q: qq, d };
  }
  const p: P2 = axis === "horizontal" ? [best ? best.q[0] : cursor[0], f[1]] : [f[0], best ? best.q[1] : cursor[1]];
  const guides: [P2, P2][] = [[f, p]];
  if (best) guides.push([best.q, p]);
  return { p, kind: axis, axis, guides, ...(best ? { align: axis === "horizontal" ? { v: best.id } : { h: best.id } } : {}) };
}

/** Alineación horizontal o vertical con los puntos existentes (las dos a la vez si hay) */
function alignSnap(s: Sketch, cursor: P2, tol: number, skip: Set<number>, from?: P2): Snap | undefined {
  let h: { id: number; q: P2; d: number } | undefined;
  let v: { id: number; q: P2; d: number } | undefined;
  for (const q of s.points) {
    if (skip.has(q.id)) continue;
    const dy = Math.abs(cursor[1] - q.y);
    const dx = Math.abs(cursor[0] - q.x);
    // A la altura del punto de salida ya la da la línea horizontal (y vertical)
    if (dy <= tol && !(from && Math.abs(q.y - from[1]) < 1e-9) && (!h || dy < h.d)) h = { id: q.id, q: [q.x, q.y], d: dy };
    if (dx <= tol && !(from && Math.abs(q.x - from[0]) < 1e-9) && (!v || dx < v.d)) v = { id: q.id, q: [q.x, q.y], d: dx };
  }
  if (!h && !v) return undefined;
  const p: P2 = [v ? v.q[0] : cursor[0], h ? h.q[1] : cursor[1]];
  const guides: [P2, P2][] = [];
  if (h) guides.push([h.q, p]);
  if (v) guides.push([v.q, p]);
  return { p, kind: "aligned", align: { h: h?.id, v: v?.id }, guides };
}

/**
 * Anclaje para el cursor `cursor` (coordenadas del sketch) con tolerancia
 * `tol` (mm).
 */
export function infer(s: Sketch, cursor: P2, tol: number, opts: InferOptions = {}): Snap {
  const exclude = opts.exclude ?? [];
  const pt = new Map(s.points.map((q) => [q.id, [q.x, q.y] as P2]));
  const centers = new Set<number>();
  for (const e of s.entities) if (e.geometry.type === "circle" || e.geometry.type === "arc" || e.geometry.type === "ellipse") centers.add(e.geometry.center);

  let best: { snap: Snap; d: number } | undefined;
  const offer = (snap: Snap) => {
    const d = dist(cursor, snap.p);
    if (d > tol) return;
    if (!best || PRIORITY[snap.kind] < PRIORITY[best.snap.kind] || (PRIORITY[snap.kind] === PRIORITY[best.snap.kind] && d < best.d)) {
      best = { snap, d };
    }
  };

  for (const q of s.points) {
    if (exclude.includes(q.id)) continue;
    const kind: SnapKind = q.id === s.origin ? "origin" : centers.has(q.id) ? "center" : "point";
    offer({ p: [q.x, q.y], kind, id: q.id });
  }

  for (const e of s.entities) {
    const g = e.geometry;
    if (g.type === "line") {
      const a = pt.get(g.start);
      const b = pt.get(g.end);
      if (!a || !b) continue;
      offer({ p: [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2], kind: "midpoint", entity: e.id });
      // Sobre el segmento (no la recta entera)
      const dx = b[0] - a[0];
      const dy = b[1] - a[1];
      const l2 = dx * dx + dy * dy;
      if (l2 === 0) continue;
      const t = ((cursor[0] - a[0]) * dx + (cursor[1] - a[1]) * dy) / l2;
      if (t > 0 && t < 1) offer({ p: [a[0] + t * dx, a[1] + t * dy], kind: "on_line", entity: e.id });
    } else if (g.type === "circle" || g.type === "arc") {
      const c = pt.get(g.center);
      if (!c) continue;
      let r: number;
      let inArc = (_p: P2) => true;
      if (g.type === "circle") r = g.radius;
      else {
        const a = pt.get(g.start);
        const b = pt.get(g.end);
        if (!a || !b) continue;
        r = dist(c, a);
        const a0 = Math.atan2(a[1] - c[1], a[0] - c[0]);
        let sweep = Math.atan2(b[1] - c[1], b[0] - c[0]) - a0;
        while (sweep <= 1e-12) sweep += 2 * Math.PI;
        inArc = (p) => relAngle(p, c, a0) <= sweep + 1e-9;
      }
      if (!(r > 0)) continue;
      const quads: P2[] = [
        [c[0] + r, c[1]],
        [c[0], c[1] + r],
        [c[0] - r, c[1]],
        [c[0], c[1] - r],
      ];
      quads.forEach((q, i) => {
        if (inArc(q)) offer({ p: q, kind: "quadrant", entity: e.id, quadrant: i as 0 | 1 | 2 | 3 });
      });
      const d = dist(cursor, c);
      if (d > 0) {
        const p: P2 = [c[0] + ((cursor[0] - c[0]) * r) / d, c[1] + ((cursor[1] - c[1]) * r) / d];
        if (inArc(p)) offer({ p, kind: "on_circle", entity: e.id });
      }
    }
  }
  // Cruces: solo entre curvas que pasan cerca del cursor
  const near = s.entities.flatMap((e) => {
    const k = curveOf(pt, e);
    return k && curveDist(cursor, k) <= tol ? [k] : [];
  });
  for (let i = 0; i < near.length; i++) {
    for (let j = i + 1; j < near.length; j++) {
      for (const p of crossings(near[i], near[j])) offer({ p, kind: "intersection", entity: near[i].id, other: near[j].id });
    }
  }
  if (best) return best.snap;
  // Sin nada cerca: horizontal/vertical o dirección de la línea en curso, o alineación con puntos
  const skip = new Set([...exclude, ...(opts.noAlign ?? []), ...(opts.from !== undefined ? [opts.from] : [])]);
  if (opts.from !== undefined) {
    const d = axisSnap(s, pt, cursor, tol, opts.from, skip) ?? directionSnap(s, pt, cursor, tol, opts.from);
    if (d) return d;
  }
  return alignSnap(s, cursor, tol, skip, opts.from !== undefined ? pt.get(opts.from) : undefined) ?? { p: cursor, kind: "free" };
}
