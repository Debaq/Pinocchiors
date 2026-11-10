// Inferencias del sketch (anclajes): a qué punto significativo se pega el
// cursor mientras se dibuja. Solo calcula; qué restricción deja cada una lo
// hace `placeSnap` en cad.ts. Sin dependencias de ejecución para probarlo con
// node (e2e/sketchSnap.test.mjs).

import type { P2, P3, Plane, Sketch } from "./cad";
import { t as tr } from "./i18n.ts";

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
  /** Sobre una spline, elipse o B-spline */
  | "on_curve"
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
  /** Vértice del sólido (proyectado al plano) */
  | "solid_vertex"
  /** Centro de una arista circular del sólido */
  | "solid_center"
  /** Punto medio de una arista recta del sólido */
  | "solid_midpoint"
  /** Sobre una arista del sólido que está en el plano del sketch */
  | "on_edge"
  /** La línea que se dibuja desde `from` cae en un ángulo múltiplo del paso elegido */
  | "angle"
  /** Prolongación de una línea más allá de sus puntas */
  | "extension"
  /** Cruce de una guía (horizontal, vertical, prolongación o del arco) con geometría u otra guía */
  | "guide_cross"
  /** Guía tangente al arco en una de sus puntas */
  | "guide_tangent"
  /** Guía radial del arco (por el centro y una de sus puntas) */
  | "guide_radial"
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
  /** Ángulo fijo (°, desde +X) de la línea que se dibuja desde `from` */
  angle?: number;
  /** Sobre una arista recta del sólido: sus extremos (el punto queda sobre esa recta) */
  edgeLine?: [P2, P2];
  /** Anclaje al sólido o a otro sketch: de qué arista o entidad sale */
  src?: AnchorSrc;
  /** Líneas guía punteadas (para dibujar) */
  guides?: [P2, P2][];
  /** Prolongación y cruce de guías: líneas prolongadas (el punto queda sobre ellas) */
  ext?: number[];
}

/** Punto "despierto": por donde pasó el cursor hace poco (puede ser un punto medio, sin id) */
export interface AwakePoint {
  id?: number;
  p: P2;
}

export interface InferOptions {
  /** Puntos que no cuentan para nada (p. ej. el que se arrastra) */
  exclude?: number[];
  /** Punto desde el que se dibuja una línea: habilita paralela, perpendicular y tangente */
  from?: number;
  /** Puntos con los que no alinearse (los de la forma en curso) */
  noAlign?: number[];
  /** Con `from`: la línea salta a ángulos múltiplos de este paso (°; 0 o sin valor, apagado) */
  angleStep?: number;
  /** Geometría del sólido proyectada al plano (`solidRefs`) */
  solid?: SolidRefs;
  /** Splines, elipses y B-splines del sketch como polilíneas: sobre la curva y cruces */
  curves?: { id: number; poly: P2[] }[];
  /**
   * Puntos despiertos (como Onshape): solo ellos generan guías de alineación,
   * y con esta opción se activan las prolongaciones, los cruces de guías y las
   * guías del arco. Sin ella, la alineación usa todos los puntos del sketch.
   */
  awake?: AwakePoint[];
}

/**
 * Aristas del sólido vistas desde el plano del sketch: vértices, centros de
 * aristas circulares y medios de las rectas (proyectados), y las aristas que
 * están en el plano (para quedar sobre ellas). Solo posición: el punto que se
 * crea no queda atado al sólido.
 */
export interface SolidRefs {
  vertices: P2[];
  centers: P2[];
  midpoints: P2[];
  /** Polilíneas de las aristas contenidas en el plano */
  edges: P2[][];
  /** De dónde sale cada uno (mismo orden que las listas), si se puede ligar a eso */
  src?: { vertices: (AnchorSrc | undefined)[]; centers: (AnchorSrc | undefined)[]; midpoints: (AnchorSrc | undefined)[]; edges: (AnchorSrc | undefined)[] };
}

/**
 * De dónde sale un anclaje al sólido, para ligar el punto a eso de verdad
 * (sigue los cambios): una arista del sólido mostrado (su número) o una
 * entidad de otro sketch. `end`: en un vértice, qué punta de esa curva.
 */
export type AnchorSrc = ({ edge: number } | { sketch: number; entity: number }) & { end?: 0 | 1 };

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
  on_curve: 7,
  aligned: 8,
  horizontal: 8,
  vertical: 8,
  parallel: 8,
  perpendicular: 8,
  tangent: 8,
  angle: 8,
  extension: 7.5,
  guide_cross: 6,
  guide_tangent: 7.5,
  guide_radial: 7.5,
  solid_vertex: 4,
  solid_center: 4,
  solid_midpoint: 5,
  on_edge: 7,
  free: 9,
};

export const SNAP_GLYPHS: Record<SnapKind, { glyph: string; label: string }> = {
  point: { glyph: "●", get label() { return tr("Punto"); } },
  origin: { glyph: "◎", get label() { return tr("Origen"); } },
  center: { glyph: "⊙", get label() { return tr("Centro"); } },
  intersection: { glyph: "✕", get label() { return tr("Intersección"); } },
  midpoint: { glyph: "△", get label() { return tr("Punto medio"); } },
  quadrant: { glyph: "◇", get label() { return tr("Cuadrante"); } },
  on_line: { glyph: "∕", get label() { return tr("Sobre la línea"); } },
  on_circle: { glyph: "◠", get label() { return tr("Sobre la curva"); } },
  on_curve: { glyph: "∿", get label() { return tr("Sobre la curva"); } },
  aligned: { glyph: "┆", get label() { return tr("Alineado"); } },
  horizontal: { glyph: "—", get label() { return tr("Horizontal"); } },
  vertical: { glyph: "|", get label() { return tr("Vertical"); } },
  parallel: { glyph: "∥", get label() { return tr("Paralela"); } },
  perpendicular: { glyph: "⊥", get label() { return tr("Perpendicular"); } },
  tangent: { glyph: "◡", get label() { return tr("Tangente"); } },
  angle: { glyph: "∠", get label() { return tr("Ángulo"); } },
  extension: { glyph: "⋯", get label() { return tr("Prolongación"); } },
  guide_cross: { glyph: "✕", get label() { return tr("Cruce de guías"); } },
  guide_tangent: { glyph: "◡", get label() { return tr("Guía tangente"); } },
  guide_radial: { glyph: "⊙", get label() { return tr("Guía radial"); } },
  solid_vertex: { glyph: "●", get label() { return tr("Vértice del sólido"); } },
  solid_center: { glyph: "⊙", get label() { return tr("Centro de la arista"); } },
  solid_midpoint: { glyph: "△", get label() { return tr("Medio de la arista"); } },
  on_edge: { glyph: "∕", get label() { return tr("Sobre la arista"); } },
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
  | { id: number; kind: "line"; a: P2; b: P2; infinite?: boolean }
  | { id: number; kind: "circle"; c: P2; r: number; a0?: number; sweep?: number };

function curveOf(pt: Map<number, P2>, e: Sketch["entities"][number]): Curve | undefined {
  const g = e.geometry;
  if (g.type === "line") {
    const [a, b] = [pt.get(g.start), pt.get(g.end)];
    return a && b ? { id: e.id, kind: "line", a, b, ...(e.infinite ? { infinite: true } : {}) } : undefined;
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
    return !!k.infinite || (t >= -EPS && t <= 1 + EPS);
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
 * Proyecta las aristas del sólido (polilíneas en mm del CAD) al plano del
 * sketch. Las rectas dan su punto medio y las circulares su centro (si el
 * círculo queda paralelo al plano); los extremos son los vértices.
 */
export function solidRefs(edges: P3[][], plane: Plane, sources?: (AnchorSrc | undefined)[]): SolidRefs {
  const [o, n, x] = [plane.origin, plane.normal, plane.x_dir];
  const y: P3 = [n[1] * x[2] - n[2] * x[1], n[2] * x[0] - n[0] * x[2], n[0] * x[1] - n[1] * x[0]];
  const dot = (a: P3, b: P3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
  const out: SolidRefs = { vertices: [], centers: [], midpoints: [], edges: [], src: { vertices: [], centers: [], midpoints: [], edges: [] } };
  const srcOf = (list: P2[]) => (list === out.vertices ? out.src!.vertices : list === out.centers ? out.src!.centers : out.src!.midpoints);
  // La malla viene en float32: tolerancia relativa al tamaño
  let size = 0;
  for (const poly of edges) for (const q of poly) size = Math.max(size, Math.abs(q[0]), Math.abs(q[1]), Math.abs(q[2]));
  const eps = 1e-3 + 2e-5 * size;
  const seen = new Map<string, number>();
  const add = (list: P2[], p: P2, src?: AnchorSrc) => {
    const key = `${list === out.vertices ? "v" : list === out.centers ? "c" : "m"}${Math.round(p[0] / eps)},${Math.round(p[1] / eps)}`;
    const at = seen.get(key);
    // Repetido: se queda con el primero que se pueda ligar
    if (at !== undefined) {
      if (!srcOf(list)[at] && src) srcOf(list)[at] = src;
      return;
    }
    seen.set(key, list.length);
    list.push(p);
    srcOf(list).push(src);
  };
  for (const [k, poly] of edges.entries()) {
    const base = sources?.[k];
    if (poly.length < 2) continue;
    const pts: P2[] = [];
    let off = 0;
    for (const q of poly) {
      const d: P3 = [q[0] - o[0], q[1] - o[1], q[2] - o[2]];
      pts.push([dot(d, x), dot(d, y)]);
      off = Math.max(off, Math.abs(dot(d, n)));
    }
    const [a, b] = [pts[0], pts[pts.length - 1]];
    const closed = dist(a, b) <= eps;
    const len = pts.slice(1).reduce((l, p, i) => l + dist(p, pts[i]), 0);
    // Una arista que se ve de punta no se puede traer como curva: sus puntas quedan fijas
    const linkable = base && len > eps ? base : undefined;
    if (!closed) {
      add(out.vertices, a, linkable && { ...linkable, end: 0 });
      add(out.vertices, b, linkable && { ...linkable, end: 1 });
    }
    if (off <= eps) {
      out.edges.push(pts);
      out.src!.edges.push(linkable);
    }
    if (len <= eps) continue;
    // Recta: todos los puntos sobre el segmento de los extremos
    const lineDev = closed ? Infinity : Math.max(...pts.map((p) => curveDist(p, { id: 0, kind: "line", a, b })));
    if (lineDev <= eps) {
      if (dist(a, b) > eps) add(out.midpoints, [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2], linkable);
      continue;
    }
    // Circular: círculo por tres puntos y todos a la misma distancia del centro
    if (pts.length < 3) continue;
    const c = circumcenter(pts[0], pts[Math.floor(pts.length / 3)], pts[Math.floor((2 * pts.length) / 3)]);
    if (!c) continue;
    const r = dist(c, pts[0]);
    if (r > eps && pts.every((p) => Math.abs(dist(c, p) - r) <= Math.max(eps, 1e-3 * r))) add(out.centers, c, linkable);
  }
  return out;
}

function circumcenter(a: P2, b: P2, c: P2): P2 | undefined {
  const d = 2 * (a[0] * (b[1] - c[1]) + b[0] * (c[1] - a[1]) + c[0] * (a[1] - b[1]));
  if (Math.abs(d) < 1e-12) return undefined;
  const [a2, b2, c2] = [a, b, c].map((p) => p[0] * p[0] + p[1] * p[1]);
  return [(a2 * (b[1] - c[1]) + b2 * (c[1] - a[1]) + c2 * (a[1] - b[1])) / d, (a2 * (c[0] - b[0]) + b2 * (a[0] - c[0]) + c2 * (b[0] - a[0])) / d];
}

/** Una recta o un círculo (o arco) como polilínea, para cruzarla con curvas libres */
function curvePoly(k: Curve): P2[] {
  if (k.kind === "line") {
    if (!k.infinite) return [k.a, k.b];
    const d: P2 = [k.b[0] - k.a[0], k.b[1] - k.a[1]];
    const far = 1e6 / (Math.hypot(d[0], d[1]) || 1);
    return [[k.a[0] - far * d[0], k.a[1] - far * d[1]], [k.a[0] + far * d[0], k.a[1] + far * d[1]]];
  }
  const [a0, sweep] = [k.a0 ?? 0, k.sweep ?? 2 * Math.PI];
  const n = Math.max(16, Math.ceil((256 * sweep) / (2 * Math.PI)));
  return Array.from({ length: n + 1 }, (_, i) => {
    const t = a0 + (sweep * i) / n;
    return [k.c[0] + k.r * Math.cos(t), k.c[1] + k.r * Math.sin(t)] as P2;
  });
}

/** Cruces entre dos polilíneas (tramo contra tramo) */
export function polyCrossings(p: P2[], q: P2[]): P2[] {
  const out: P2[] = [];
  for (let i = 0; i + 1 < p.length; i++) {
    const [a, b] = [p[i], p[i + 1]];
    const r: P2 = [b[0] - a[0], b[1] - a[1]];
    for (let j = 0; j + 1 < q.length; j++) {
      const [c, d] = [q[j], q[j + 1]];
      const s: P2 = [d[0] - c[0], d[1] - c[1]];
      const den = r[0] * s[1] - r[1] * s[0];
      if (Math.abs(den) < 1e-15) continue;
      const w: P2 = [c[0] - a[0], c[1] - a[1]];
      const t = (w[0] * s[1] - w[1] * s[0]) / den;
      const u = (w[0] * r[1] - w[1] * r[0]) / den;
      if (t < -EPS || t > 1 + EPS || u < -EPS || u > 1 + EPS) continue;
      const x: P2 = [a[0] + t * r[0], a[1] + t * r[1]];
      if (!out.some((o) => dist(o, x) < 1e-9)) out.push(x);
    }
  }
  return out;
}

/** Extremos de una polilínea recta (todos sus puntos sobre el segmento de los extremos); nada si es curva */
export function straightEdge(poly: P2[]): [P2, P2] | undefined {
  if (poly.length < 2) return undefined;
  const [a, b] = [poly[0], poly[poly.length - 1]];
  const len = dist(a, b);
  if (len <= 1e-9) return undefined;
  const tol = 1e-3 + 1e-5 * len;
  return poly.every((p) => curveDist(p, { id: 0, kind: "line", a, b }) <= tol) ? [a, b] : undefined;
}

/** Punto más cercano de una polilínea */
function nearestOnPolyline(poly: P2[], p: P2): P2 {
  let best: P2 = poly[0];
  let bd = Infinity;
  for (let i = 0; i + 1 < poly.length; i++) {
    const [a, b] = [poly[i], poly[i + 1]];
    const dx = b[0] - a[0];
    const dy = b[1] - a[1];
    const l2 = dx * dx + dy * dy;
    const t = l2 > 0 ? Math.max(0, Math.min(1, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2)) : 0;
    const q: P2 = [a[0] + t * dx, a[1] + t * dy];
    const d = dist(p, q);
    if (d < bd) [best, bd] = [q, d];
  }
  return best;
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

/** Línea desde `from` a un ángulo múltiplo de `step` (°) si el cursor queda a `tol` de ese rayo */
function angleSnap(pt: Map<number, P2>, cursor: P2, tol: number, from: number, step: number): Snap | undefined {
  const f = pt.get(from);
  if (!f || !(step > 0)) return undefined;
  const v: P2 = [cursor[0] - f[0], cursor[1] - f[1]];
  const len = Math.hypot(v[0], v[1]);
  if (len <= tol) return undefined;
  const deg = Math.round(((Math.atan2(v[1], v[0]) * 180) / Math.PI) / step) * step;
  const u: P2 = [Math.cos((deg * Math.PI) / 180), Math.sin((deg * Math.PI) / 180)];
  const t = v[0] * u[0] + v[1] * u[1];
  const p: P2 = [f[0] + t * u[0], f[1] + t * u[1]];
  if (t <= tol || dist(cursor, p) > tol) return undefined;
  // Los ejes ya los resuelve la restricción horizontal / vertical
  if (Math.abs(u[0]) < 1e-9 || Math.abs(u[1]) < 1e-9) return undefined;
  const norm = ((deg % 360) + 360) % 360;
  return { p, kind: "angle", angle: norm, guides: [[f, p]] };
}

/** Alineación horizontal o vertical con los puntos candidatos (las dos a la vez si hay) */
function alignSnap(pts: { id?: number; q: P2 }[], cursor: P2, tol: number, skip: Set<number>, from?: P2): Snap | undefined {
  let h: { id?: number; q: P2; d: number } | undefined;
  let v: { id?: number; q: P2; d: number } | undefined;
  for (const a of pts) {
    if (a.id !== undefined && skip.has(a.id)) continue;
    const dy = Math.abs(cursor[1] - a.q[1]);
    const dx = Math.abs(cursor[0] - a.q[0]);
    // A la altura del punto de salida ya la da la línea horizontal (y vertical)
    if (dy <= tol && !(from && Math.abs(a.q[1] - from[1]) < 1e-9) && (!h || dy < h.d)) h = { id: a.id, q: a.q, d: dy };
    if (dx <= tol && !(from && Math.abs(a.q[0] - from[0]) < 1e-9) && (!v || dx < v.d)) v = { id: a.id, q: a.q, d: dx };
  }
  if (!h && !v) return undefined;
  const p: P2 = [v ? v.q[0] : cursor[0], h ? h.q[1] : cursor[1]];
  const guides: [P2, P2][] = [];
  if (h) guides.push([h.q, p]);
  if (v) guides.push([v.q, p]);
  return { p, kind: "aligned", align: { h: h?.id, v: v?.id }, guides };
}

/**
 * Guía infinita (o limitada) que sale de `o` en la dirección unitaria `u`:
 * horizontal o vertical de un punto despierto, prolongación de una línea, o
 * tangente / radial de la punta de un arco. Los puntos son `o + t·u` con `t`
 * en [t0, t1].
 */
interface Guide {
  kind: "h" | "v" | "ext" | "tan" | "rad";
  o: P2;
  u: P2;
  t0: number;
  t1: number;
  /** h / v: punto despierto (si es del sketch) */
  id?: number;
  /** ext: la línea que se prolonga */
  line?: number;
  /** tan / rad: el arco */
  arc?: number;
}

/** Posición `t` sobre la guía y distancia del punto a su recta; nada si cae fuera del tramo */
function onGuide(g: Guide, p: P2): { t: number; d: number } | undefined {
  const w: P2 = [p[0] - g.o[0], p[1] - g.o[1]];
  const t = w[0] * g.u[0] + w[1] * g.u[1];
  if (t < g.t0 - 1e-9 || t > g.t1 + 1e-9) return undefined;
  return { t, d: Math.abs(w[0] * g.u[1] - w[1] * g.u[0]) };
}

const guideLine = (g: Guide): Curve => ({ id: -1, kind: "line", a: g.o, b: [g.o[0] + g.u[0], g.o[1] + g.u[1]], infinite: true });
const guidePoly = (g: Guide): P2[] => {
  const [t0, t1] = [Math.max(g.t0, -1e6), Math.min(g.t1, 1e6)];
  return [[g.o[0] + t0 * g.u[0], g.o[1] + t0 * g.u[1]], [g.o[0] + t1 * g.u[0], g.o[1] + t1 * g.u[1]]];
};

/** Los puntos de una línea, círculo o arco (para saber si el punto arrastrado es suyo) */
function ownedPoints(g: Sketch["entities"][number]["geometry"]): number[] {
  const x = g as unknown as Record<string, unknown>;
  return ["start", "end", "center"].flatMap((k) => (typeof x[k] === "number" ? [x[k] as number] : []));
}

/**
 * Guías cercanas al cursor: de los puntos despiertos (horizontal y vertical;
 * tangente y radial si son punta de un arco) y prolongaciones de las líneas.
 */
function nearGuides(s: Sketch, pt: Map<number, P2>, cursor: P2, tol: number, awake: AwakePoint[], from: number | undefined, skip: Set<number>): Guide[] {
  const out: Guide[] = [];
  const pts: AwakePoint[] = [];
  for (const a of awake) {
    if (a.id === undefined) pts.push(a);
    else if (pt.has(a.id) && !skip.has(a.id)) pts.push({ id: a.id, p: pt.get(a.id)! });
  }
  // El punto de salida de la línea en curso siempre está despierto
  if (from !== undefined && pt.has(from) && !pts.some((a) => a.id === from)) pts.push({ id: from, p: pt.get(from)! });
  for (const a of pts) {
    out.push({ kind: "h", o: a.p, u: [1, 0], t0: -Infinity, t1: Infinity, id: a.id }, { kind: "v", o: a.p, u: [0, 1], t0: -Infinity, t1: Infinity, id: a.id });
    if (a.id === undefined) continue;
    for (const e of s.entities) {
      const g = e.geometry;
      if (g.type !== "arc" || (g.start !== a.id && g.end !== a.id)) continue;
      const c = pt.get(g.center);
      const r = c ? dist(c, a.p) : 0;
      if (!c || r < 1e-12) continue;
      const u: P2 = [(a.p[0] - c[0]) / r, (a.p[1] - c[1]) / r];
      out.push({ kind: "rad", o: a.p, u, t0: -Infinity, t1: Infinity, arc: e.id }, { kind: "tan", o: a.p, u: [-u[1], u[0]], t0: -Infinity, t1: Infinity, arc: e.id });
    }
  }
  // Prolongaciones: desde una punta despierta, hacia afuera, con un tope (como Onshape: solo
  // las líneas por cuya punta pasó el cursor; si no, cualquier línea del sketch daría guías)
  const woke = new Set(pts.flatMap((a) => (a.id !== undefined ? [a.id] : [])));
  for (const e of s.entities) {
    const g = e.geometry;
    if (g.type !== "line" || e.infinite || skip.has(g.start) || skip.has(g.end)) continue;
    if (!woke.has(g.start) && !woke.has(g.end)) continue;
    const [a, b] = [pt.get(g.start), pt.get(g.end)];
    if (!a || !b) continue;
    const l = dist(a, b);
    if (l < tol) continue;
    const u: P2 = [(b[0] - a[0]) / l, (b[1] - a[1]) / l];
    const max = Math.min(5 * l, 60 * tol);
    if (woke.has(g.end)) out.push({ kind: "ext", o: b, u, t0: 0, t1: max, line: e.id });
    if (woke.has(g.start)) out.push({ kind: "ext", o: a, u: [-u[0], -u[1]], t0: 0, t1: max, line: e.id });
  }
  return out.filter((g) => {
    const r = onGuide(g, cursor);
    return r !== undefined && r.d <= tol;
  });
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
      // Sobre el segmento (o en cualquier parte de una línea infinita)
      if ((t > 0 && t < 1) || e.infinite) offer({ p: [a[0] + t * dx, a[1] + t * dy], kind: "on_line", entity: e.id });
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
  for (const c of opts.curves ?? []) if (c.poly.length >= 2) offer({ p: nearestOnPolyline(c.poly, cursor), kind: "on_curve", entity: c.id });
  if (opts.solid) {
    const src = opts.solid.src;
    const from = (x: AnchorSrc | undefined) => (x ? { src: x } : {});
    opts.solid.vertices.forEach((p, i) => offer({ p, kind: "solid_vertex", ...from(src?.vertices[i]) }));
    opts.solid.centers.forEach((p, i) => offer({ p, kind: "solid_center", ...from(src?.centers[i]) }));
    opts.solid.midpoints.forEach((p, i) => offer({ p, kind: "solid_midpoint", ...from(src?.midpoints[i]) }));
    opts.solid.edges.forEach((poly, i) => {
      const line = straightEdge(poly);
      offer({ p: nearestOnPolyline(poly, cursor), kind: "on_edge", ...(line ? { edgeLine: line } : {}), ...from(src?.edges[i]) });
    });
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
  // Cruces con splines y elipses: por sus polilíneas (el solver los deja justo encima)
  const polys = (opts.curves ?? []).filter((c) => c.poly.length >= 2 && dist(cursor, nearestOnPolyline(c.poly, cursor)) <= tol);
  if (polys.length) {
    const all = [...near.map((k) => ({ id: k.id, poly: curvePoly(k) })), ...polys];
    for (let i = 0; i < all.length; i++) {
      for (let j = Math.max(i + 1, near.length); j < all.length; j++) {
        for (const p of polyCrossings(all[i].poly, all[j].poly)) offer({ p, kind: "intersection", entity: all[i].id, other: all[j].id });
      }
    }
  }
  // Guías (despiertos, prolongaciones, arcos) y sus cruces con geometría u otras guías
  const skipG = new Set([...exclude, ...(opts.noAlign ?? [])]);
  const guides = opts.awake ? nearGuides(s, pt, cursor, tol, opts.awake, opts.from, skipG) : [];
  const cross = (gs: Guide[], p: P2, entity?: number) => {
    const h = gs.find((g) => g.kind === "h" && g.id !== undefined)?.id;
    const v = gs.find((g) => g.kind === "v" && g.id !== undefined)?.id;
    const ext = gs.flatMap((g) => (g.kind === "ext" ? [g.line!] : []));
    offer({
      p,
      kind: "guide_cross",
      ...(entity !== undefined ? { entity } : {}),
      ...(h !== undefined || v !== undefined ? { align: { ...(h !== undefined ? { h } : {}), ...(v !== undefined ? { v } : {}) } } : {}),
      ...(ext.length ? { ext } : {}),
      guides: gs.map((g) => [g.o, p] as [P2, P2]),
    });
  };
  for (const g of guides) {
    const gl = guideLine(g);
    for (const e of s.entities) {
      if (e.id === g.line || ownedPoints(e.geometry).some((id) => skipG.has(id))) continue;
      const k = curveOf(pt, e);
      if (!k || curveDist(cursor, k) > tol) continue;
      for (const p of crossings(gl, k)) if (onGuide(g, p)) cross([g], p, e.id);
    }
    for (const c of opts.curves ?? []) {
      if (c.poly.length < 2 || dist(cursor, nearestOnPolyline(c.poly, cursor)) > tol) continue;
      for (const p of polyCrossings(guidePoly(g), c.poly)) cross([g], p, c.id);
    }
  }
  // Guía × guía: horizontal con vertical ya es "alineado"; el resto cruza aquí
  for (let i = 0; i < guides.length; i++) {
    for (let j = i + 1; j < guides.length; j++) {
      const [a, b] = [guides[i], guides[j]];
      const plain = (g: Guide) => g.kind === "h" || g.kind === "v";
      if ((plain(a) && plain(b)) || (a.kind === "ext" && b.kind === "ext" && a.line === b.line)) continue;
      for (const p of crossings(guideLine(a), guideLine(b))) if (onGuide(a, p) && onGuide(b, p)) cross([a, b], p);
    }
  }
  if (best) return best.snap;
  // Sin nada cerca: horizontal/vertical o dirección de la línea en curso, o alineación con puntos
  const skip = new Set([...exclude, ...(opts.noAlign ?? []), ...(opts.from !== undefined ? [opts.from] : [])]);
  if (opts.from !== undefined) {
    const d =
      axisSnap(s, pt, cursor, tol, opts.from, skip) ??
      directionSnap(s, pt, cursor, tol, opts.from) ??
      angleSnap(pt, cursor, tol, opts.from, opts.angleStep ?? 0);
    if (d) return d;
  }
  // Prolongación o guía del arco (más fuerte que la alineación)
  let g1: { g: Guide; t: number; d: number } | undefined;
  for (const g of guides) {
    if (g.kind === "h" || g.kind === "v") continue;
    const r = onGuide(g, cursor)!;
    if (!g1 || r.d < g1.d) g1 = { g, ...r };
  }
  if (g1) {
    const { g, t } = g1;
    const p: P2 = [g.o[0] + t * g.u[0], g.o[1] + t * g.u[1]];
    const kind: SnapKind = g.kind === "ext" ? "extension" : g.kind === "tan" ? "guide_tangent" : "guide_radial";
    return { p, kind, entity: g.kind === "ext" ? g.line : g.arc, guides: [[g.o, p]] };
  }
  const candidates = opts.awake
    ? opts.awake.flatMap((a) => (a.id === undefined ? [{ q: a.p }] : pt.has(a.id) ? [{ id: a.id, q: pt.get(a.id)! }] : []))
    : s.points.map((q) => ({ id: q.id, q: [q.x, q.y] as P2 }));
  return alignSnap(candidates, cursor, tol, skip, opts.from !== undefined ? pt.get(opts.from) : undefined) ?? { p: cursor, kind: "free" };
}
