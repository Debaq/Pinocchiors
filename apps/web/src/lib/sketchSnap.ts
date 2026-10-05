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

/**
 * Anclaje para el cursor `cursor` (coordenadas del sketch) con tolerancia
 * `tol` (mm). `exclude`: puntos que no cuentan (p. ej. el que se arrastra).
 */
export function infer(s: Sketch, cursor: P2, tol: number, exclude: number[] = []): Snap {
  const pt = new Map(s.points.map((q) => [q.id, [q.x, q.y] as P2]));
  const centers = new Set<number>();
  for (const e of s.entities) if (e.geometry.type === "circle" || e.geometry.type === "arc") centers.add(e.geometry.center);

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
  return best?.snap ?? { p: cursor, kind: "free" };
}
