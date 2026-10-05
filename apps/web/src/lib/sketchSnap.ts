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
  /** Cuadrante: 0 = +x, 1 = +y, 2 = −x, 3 = −y desde el centro */
  quadrant?: 0 | 1 | 2 | 3;
}

/** Prioridad cuando hay varios candidatos cerca (menor gana), como Onshape */
const PRIORITY: Record<SnapKind, number> = {
  point: 0,
  center: 0,
  origin: 1,
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
  return best?.snap ?? { p: cursor, kind: "free" };
}
