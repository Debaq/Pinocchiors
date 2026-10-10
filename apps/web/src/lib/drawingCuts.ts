// Cortes del plano 2D: la línea de corte dibujada sobre una vista (recta o
// escalonada) y lo que quita (un prisma), la vista del corte que se mira en el
// sentido de las flechas, el medio corte de una vista, la rotura (corte local
// hasta una profundidad) y el corte de una vista 3D. La geometría va en mm
// del modelo (Z arriba); los puntos dibujados, en coordenadas de la vista.
// Funciones puras: se prueban con node (e2e/drawingCuts.test.mjs).

import { viewUp, type P2, type P3, type ViewDir } from "./drawing.ts";

/** Lo que quita material: un polígono plano extruido por `dir` (el largo va en el vector) */
export interface CutPrism {
  points: P3[];
  dir: P3;
}

/** Rayado: ángulo (grados), separación (mm de hoja) y cruzado */
export interface HatchStyle {
  angle: number;
  spacing: number;
  cross?: boolean;
}
export const DEFAULT_HATCH: HatchStyle = { angle: 45, spacing: 2 };

/**
 * Un corte dibujado sobre una vista: la línea (dos puntos o más, escalonada)
 * en coordenadas de esa vista. Se mira hacia la izquierda del recorrido del
 * primer tramo (`flip` al revés).
 */
export interface SectionDef {
  label: string;
  view: string;
  points: P2[];
  flip?: boolean;
  hidden?: boolean;
  hatch?: HatchStyle;
}

/** Rotura: un contorno sobre una vista (coordenadas de la vista) cortado hasta `depth` mm desde lo más cercano */
export interface BreakDef {
  points: P2[];
  depth: number;
}

/** La vista de un corte en la hoja (`Placed.name`) */
export const sectionViewName = (label: string) => `corte-${label}`;

const add = (a: P3, b: P3): P3 => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
const mul = (a: P3, k: number): P3 => [a[0] * k, a[1] * k, a[2] * k];
const dot = (a: P3, b: P3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a: P3, b: P3): P3 => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const unit = (a: P3): P3 => {
  const l = Math.hypot(...a) || 1;
  return [a[0] / l, a[1] / l, a[2] / l];
};
const clean = (a: P3): P3 => a.map((x) => (Math.abs(x) < 1e-12 ? 0 : x)) as P3;

/** Punto de la vista (y una profundidad hacia quien mira) → modelo */
export function viewToWorld(d: ViewDir, p: P2, depth = 0): P3 {
  const up = viewUp(d);
  return add(add(mul(d.xdir, p[0]), mul(up, p[1])), mul(d.eye, depth));
}

/** Hacia dónde miran las flechas (unitario, en la vista): a la izquierda del primer tramo */
export function sectionArrow(def: SectionDef): P2 {
  const [a, b] = def.points;
  const t: P2 = [b[0] - a[0], b[1] - a[1]];
  const l = Math.hypot(...t) || 1;
  const n: P2 = [-t[1] / l, t[0] / l];
  const r: P2 = def.flip ? [-n[0], -n[1]] : n;
  return [r[0] + 0, r[1] + 0];
}

/**
 * Lo que quita un corte: el lado de atrás de las flechas, como un polígono en
 * el plano de la vista (la línea alargada `reach` mm por las puntas y
 * cerrada lejos) que atraviesa toda la pieza en profundidad
 */
export function sectionPrism(def: SectionDef, parent: ViewDir, reach: number): CutPrism {
  const pts = def.points;
  const a = sectionArrow(def);
  const dir2 = (p: P2, q: P2): P2 => {
    const l = Math.hypot(q[0] - p[0], q[1] - p[1]) || 1;
    return [(q[0] - p[0]) / l, (q[1] - p[1]) / l];
  };
  const t0 = dir2(pts[0], pts[1]);
  const tn = dir2(pts[pts.length - 2], pts[pts.length - 1]);
  const first: P2 = [pts[0][0] - t0[0] * reach, pts[0][1] - t0[1] * reach];
  const last: P2 = [pts[pts.length - 1][0] + tn[0] * reach, pts[pts.length - 1][1] + tn[1] * reach];
  const back = (p: P2): P2 => [p[0] - a[0] * reach, p[1] - a[1] * reach];
  const poly: P2[] = [first, ...pts.slice(1, -1), last, back(last), back(first)];
  return { points: poly.map((p) => viewToWorld(parent, p, -reach)), dir: mul(parent.eye, 2 * reach) };
}

/**
 * Desde dónde se mira la vista del corte: en el sentido de las flechas (el ojo
 * del lado de lo quitado). Arriba, el de la vista madre si no es la dirección
 * de las flechas (si lo es, hacia quien miraba la vista madre).
 */
export function sectionViewDir(def: SectionDef, parent: ViewDir): ViewDir {
  const a2 = sectionArrow(def);
  const up = viewUp(parent);
  const a = unit(add(mul(parent.xdir, a2[0]), mul(up, a2[1])));
  const eye = mul(a, -1);
  let u = Math.abs(dot(up, a)) < 0.5 ? up : parent.eye;
  // Arriba perpendicular a la dirección
  u = unit(add(u, mul(a, -dot(u, a))));
  return { eye: clean(eye), xdir: clean(unit(cross(u, eye))) };
}

/** Caja de la pieza (mm) */
export interface Box3 {
  min: P3;
  max: P3;
}
const corners = (b: Box3): P3[] => [0, 1, 2, 3, 4, 5, 6, 7].map((i) => [i & 1 ? b.max[0] : b.min[0], i & 2 ? b.max[1] : b.min[1], i & 4 ? b.max[2] : b.min[2]]);
/** Cuánto alcanza un prisma para atravesar toda la pieza desde cualquier lado */
export const reachOf = (b: Box3) => 2 * Math.max(1, ...corners(b).map((c) => Math.hypot(...c)));

/**
 * Medio corte de una vista: se quita el cuarto de la pieza que está hacia
 * quien mira y del lado `side` (derecha o izquierda de la hoja) del centro
 */
export function halfPrism(d: ViewDir, box: Box3, side: "right" | "left"): CutPrism {
  const c: P3 = [(box.min[0] + box.max[0]) / 2, (box.min[1] + box.max[1]) / 2, (box.min[2] + box.max[2]) / 2];
  const r = reachOf(box);
  const up = viewUp(d);
  const x = mul(d.xdir, side === "right" ? r : -r);
  const e = mul(d.eye, r);
  const base = add(c, mul(up, -r));
  return { points: [base, add(base, x), add(add(base, x), e), add(base, e)], dir: mul(up, 2 * r) };
}

/** El centro de la pieza en la vista (para la línea que separa el medio corte) */
export function viewCenter(d: ViewDir, box: Box3): P2 {
  const c: P3 = [(box.min[0] + box.max[0]) / 2, (box.min[1] + box.max[1]) / 2, (box.min[2] + box.max[2]) / 2];
  return [dot(c, d.xdir), dot(c, viewUp(d))];
}

/** Rotura: el contorno, desde delante de la pieza hasta `depth` mm adentro de lo más cercano */
export function breakPrism(d: ViewDir, def: BreakDef, box: Box3): CutPrism {
  const front = Math.max(...corners(box).map((p) => dot(p, d.eye)));
  const m = 1 + 0.05 * reachOf(box);
  return { points: def.points.map((p) => viewToWorld(d, p, front + m)), dir: mul(d.eye, -(m + def.depth)) };
}

/** Corte de una vista 3D: un cuarto de la pieza (adelante a la derecha, de arriba abajo) */
export function quarterPrism(box: Box3): CutPrism {
  return halfPrism({ eye: [0, -1, 0], xdir: [1, 0, 0] }, box, "right");
}
