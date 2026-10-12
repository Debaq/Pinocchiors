// Elegir entre lo encimado (como KiCad, «¿cuál?»): qué distingue a cada
// candidata y la vista separada (cada una corrida a un costado, con su color,
// para que hasta dos iguales se vean como dos).
// Funciones puras: se prueban con node (e2e/sketchClarify.test.mjs).

import { CONSTRAINT_LABELS, constraintIds, constraintValue, type P2, type Sketch } from "./cad.ts";
import { ENTITY_NAMES, polylineOf } from "./sketchCheck.ts";

/** Colores de la vista separada (tokens del tema), uno por candidata */
export const SPREAD_COLORS = ["cyan", "purple", "orange", "success", "warning"];

export interface Candidate {
  id: number;
  name: string;
  /** Lo que la distingue: largo o radio, sus puntos, construcción */
  detail: string;
  /** Sus restricciones y cotas */
  constraints: string[];
  color: string;
  /** Corrimiento de la vista separada (mm del sketch) */
  offset: P2;
  /** Dónde va su etiqueta en el visor (ya corrida) */
  label: P2;
}

const num = (v: number) => `${+v.toFixed(2)}`;
const dist = (a: P2, b: P2) => Math.hypot(a[0] - b[0], a[1] - b[1]);

/** Punto a mitad de largo de una polilínea */
function halfWay(pts: P2[]): P2 {
  const total = pts.slice(1).reduce((m, q, i) => m + dist(pts[i], q), 0);
  let left = total / 2;
  for (let i = 0; i + 1 < pts.length; i++) {
    const l = dist(pts[i], pts[i + 1]);
    if (l >= left && l > 0) {
      const t = left / l;
      return [pts[i][0] + t * (pts[i + 1][0] - pts[i][0]), pts[i][1] + t * (pts[i + 1][1] - pts[i][1])];
    }
    left -= l;
  }
  return pts[pts.length - 1] ?? [0, 0];
}

/** Nombre, lo que la distingue y sus restricciones */
export function describeEntity(s: Sketch, id: number): { name: string; detail: string; constraints: string[] } {
  const e = s.entities.find((x) => x.id === id);
  if (!e) return { name: `${id}`, detail: "", constraints: [] };
  const at = (p: number): P2 => {
    const q = s.points.find((x) => x.id === p);
    return q ? [q.x, q.y] : [0, 0];
  };
  const g = e.geometry;
  const parts: string[] = [];
  if (g.type === "line") parts.push(`${num(dist(at(g.start), at(g.end)))} mm`, `P${g.start} → P${g.end}`);
  else if (g.type === "circle") parts.push(`R ${num(g.radius)}`, `centro P${g.center}`);
  else if (g.type === "arc") parts.push(`R ${num(dist(at(g.center), at(g.start)))}`, `P${g.start} → P${g.end}`);
  if (e.construction) parts.push("construcción");
  const constraints = s.constraints
    .filter((c) => constraintIds(c).includes(id))
    .map((c) => {
      const v = constraintValue(c);
      return v === undefined ? CONSTRAINT_LABELS[c.type] : `${CONSTRAINT_LABELS[c.type]} ${num(v)}${c.type === "angle" ? "°" : ""}`;
    });
  return { name: `${ENTITY_NAMES[g.type]} ${id}`, detail: parts.join(" · "), constraints };
}

/**
 * Las candidatas bajo `at` separadas `step` mm entre sí, de costado a la
 * primera (perpendicular a su tramo más cercano al clic), centradas.
 */
export function spreadCandidates(s: Sketch, ids: number[], at: P2, step: number): Candidate[] {
  const poly = (id: number) => {
    const e = s.entities.find((x) => x.id === id);
    return e ? polylineOf(s, e.geometry) : [];
  };
  // De costado al tramo de la primera más cercano al clic
  let n: P2 = [0, 1];
  let best = Infinity;
  const first = poly(ids[0]);
  for (let i = 0; i + 1 < first.length; i++) {
    const [a, b] = [first[i], first[i + 1]];
    const d: P2 = [b[0] - a[0], b[1] - a[1]];
    const l = Math.hypot(...d);
    if (l < 1e-12) continue;
    const t = Math.max(0, Math.min(1, ((at[0] - a[0]) * d[0] + (at[1] - a[1]) * d[1]) / (l * l)));
    const gap = dist(at, [a[0] + t * d[0], a[1] + t * d[1]]);
    if (gap < best) [best, n] = [gap, [-d[1] / l, d[0] / l]];
  }
  return ids.map((id, i) => {
    const k = (i - (ids.length - 1) / 2) * step;
    const offset: P2 = [n[0] * k, n[1] * k];
    const mid = halfWay(poly(id));
    return { id, ...describeEntity(s, id), color: SPREAD_COLORS[i % SPREAD_COLORS.length], offset, label: [mid[0] + offset[0], mid[1] + offset[1]] };
  });
}
