// Diseño CAD paramétrico (backend: apps/desktop/src/cad.rs y libs/cad).
//
// El documento es una lista de operaciones con sus recetas. Se edita acá, se
// manda entero al backend y vuelve el resultado del recálculo. Deshacer y
// rehacer guardan copias del documento: son chicas (sin sólidos adentro).
//
// Coordenadas del documento: mm, Z arriba. El visor recibe la malla del
// sólido ya en Y arriba y en las unidades de la escena.

import { createSignal } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

export type P2 = [number, number];
export type P3 = [number, number, number];

// ─── Documento (mismo JSON que cad_model) ─────────────────────────────────

export interface Plane {
  origin: P3;
  normal: P3;
  x_dir: P3;
}

export interface FaceRef {
  point: P3;
  normal: P3;
}

export interface EdgeRef {
  point: P3;
  direction: P3;
}

export type PlaneSpec =
  | { type: "xy" }
  | { type: "xz" }
  | { type: "yz" }
  | { type: "face"; face: FaceRef }
  | { type: "custom"; plane: Plane };

export type AxisSpec =
  | { type: "x" }
  | { type: "y" }
  | { type: "z" }
  | { type: "sketch_line"; sketch: number; line: number }
  | { type: "edge"; edge: EdgeRef }
  | { type: "custom"; origin: P3; direction: P3 };

export type BodyOp = "join" | "cut" | "intersect";

export type RegionSelection = { type: "all" } | { type: "points"; points: P2[] };

export type Extent =
  | { type: "blind"; distance: number }
  | { type: "symmetric"; distance: number }
  | { type: "through_all" }
  | { type: "up_to_face"; face: FaceRef };

export interface SketchPoint {
  id: number;
  x: number;
  y: number;
}

export type Geometry =
  | { type: "line"; start: number; end: number }
  | { type: "circle"; center: number; radius: number }
  | { type: "arc"; center: number; start: number; end: number }
  | { type: "spline"; points: number[]; closed: boolean };

export interface SketchEntity {
  id: number;
  construction?: boolean;
  geometry: Geometry;
}

export type SketchConstraint =
  | { type: "coincident"; a: number; b: number }
  | { type: "fixed"; point: number; x: number; y: number }
  | { type: "horizontal"; line: number }
  | { type: "vertical"; line: number }
  | { type: "parallel"; a: number; b: number }
  | { type: "perpendicular"; a: number; b: number }
  | { type: "equal"; a: number; b: number }
  | { type: "tangent"; a: number; b: number }
  | { type: "point_on_line"; point: number; line: number }
  | { type: "point_on_circle"; point: number; circle: number }
  | { type: "midpoint"; point: number; line: number }
  | { type: "symmetric"; a: number; b: number; line: number }
  | { type: "distance"; a: number; b: number; value: number }
  | { type: "horizontal_distance"; a: number; b: number; value: number }
  | { type: "vertical_distance"; a: number; b: number; value: number }
  | { type: "length"; line: number; value: number }
  | { type: "radius"; entity: number; value: number }
  | { type: "diameter"; entity: number; value: number }
  | { type: "angle"; a: number; b: number; degrees: number };

export interface Sketch {
  points: SketchPoint[];
  entities: SketchEntity[];
  constraints: SketchConstraint[];
  next_id?: number;
}

export type PrimitiveShape =
  | { type: "box"; dx: number; dy: number; dz: number }
  | { type: "cylinder"; radius: number; height: number }
  | { type: "cone"; r1: number; r2: number; height: number }
  | { type: "sphere"; radius: number }
  | { type: "torus"; major: number; minor: number };

export type PatternKind =
  | { type: "linear"; direction: P3; count: number; spacing: number }
  | { type: "circular"; axis: AxisSpec; count: number; angle: number };

export type FeatureKind =
  | { type: "sketch"; plane: PlaneSpec; offset: number; sketch: Sketch }
  | { type: "extrude"; sketch: number; regions: RegionSelection; extent: Extent; reverse: boolean; op: BodyOp }
  | { type: "revolve"; sketch: number; regions: RegionSelection; axis: AxisSpec; angle: number; op: BodyOp }
  | { type: "primitive"; shape: PrimitiveShape; origin: P3; z: P3; x: P3; op: BodyOp }
  | { type: "fillet"; edges: EdgeRef[]; radius: number }
  | { type: "chamfer"; edges: EdgeRef[]; distance: number }
  | { type: "shell"; faces: FaceRef[]; thickness: number }
  | { type: "draft"; faces: FaceRef[]; neutral: PlaneSpec; angle: number }
  | { type: "pattern"; features: number[]; pattern: PatternKind }
  | { type: "mirror"; features: number[]; plane: PlaneSpec }
  | { type: "split"; plane: PlaneSpec; flip: boolean }
  | { type: "import"; format: "step" | "brep"; data: number[]; op: BodyOp };

export interface Feature {
  id: number;
  name: string;
  suppressed?: boolean;
  kind: FeatureKind;
}

export interface CadDocument {
  features: Feature[];
  rollback?: number | null;
  next_id?: number;
}

// ─── Resultados ───────────────────────────────────────────────────────────

export type FeatureState =
  | { state: "ok" }
  | { state: "error"; message: string }
  | { state: "suppressed" }
  | { state: "rolled_back" };

export type FeatureStatus = { id: number } & FeatureState;

export interface Loop {
  pieces: { entity: number; reversed: boolean }[];
  polygon: P2[];
  area: number;
}

export interface Region {
  outer: Loop;
  holes: Loop[];
  depth: number;
  sample: P2;
}

export interface SolveReport {
  status: "well_constrained" | "under_constrained" | "over_constrained" | "failed";
  dof: number;
  residual: number;
  conflicting: number[];
  free_points: number[];
}

export interface SketchView {
  id: number;
  plane: Plane;
  sketch: Sketch;
  report: SolveReport;
  regions: Region[];
}

export interface BodyInfo {
  volume: number;
  area: number;
  bbox_min: P3;
  bbox_max: P3;
  faces: number;
  edges: number;
  valid: boolean;
}

export interface CadResult {
  status: FeatureStatus[];
  sketches: SketchView[];
  body: BodyInfo | null;
  version: number;
}

export interface CadStatus {
  available: boolean;
  occt_version: string;
  has_document: boolean;
}

export interface SolvedSketch {
  sketch: Sketch;
  report: SolveReport;
  regions: Region[];
}

/** Malla del sólido para el visor (`cad_mesh`) */
export interface CadMesh {
  positions: Float32Array;
  normals: Float32Array;
  indices: Uint32Array;
  /** Cara B-Rep de cada triángulo */
  triangleFace: Uint32Array;
  /** Fin (acumulado, en puntos) de cada arista */
  edgeEnds: Uint32Array;
  edgePoints: Float32Array;
}

export function decodeCadMesh(buffer: ArrayBuffer): CadMesh | null {
  const [vertices, triangles, edges, points] = new Uint32Array(buffer, 0, 4);
  if (vertices === 0) return null;
  let offset = 16;
  const floats = (n: number) => {
    const v = new Float32Array(buffer, offset, n);
    offset += n * 4;
    return v;
  };
  const uints = (n: number) => {
    const v = new Uint32Array(buffer, offset, n);
    offset += n * 4;
    return v;
  };
  const positions = floats(vertices * 3);
  const normals = floats(vertices * 3);
  const indices = uints(triangles * 3);
  const triangleFace = uints(triangles);
  const edgeEnds = uints(edges);
  const edgePoints = floats(points * 3);
  return { positions, normals, indices, triangleFace, edgeEnds, edgePoints };
}

// ─── Escaneo → CAD ────────────────────────────────────────────────────────

export interface PlanePick {
  kind: "plane";
  plane: Plane;
  rms: number;
  faces: number[];
  area: number;
  boundary: P2[][];
  depth: number | null;
}

export interface CylinderPick {
  kind: "cylinder";
  origin: P3;
  direction: P3;
  radius: number;
  length: number;
  rms: number;
  faces: number[];
  hole: boolean;
  coverage: number;
}

export type ScanPick = PlanePick | CylinderPick;

export interface Detection {
  shape:
    | { type: "plane"; plane: Plane }
    | { type: "cylinder"; origin: P3; direction: P3; radius: number }
    | { type: "sphere"; center: P3; radius: number };
  faces: number[];
  area: number;
  rms: number;
}

export type ScanFeature =
  | { kind: "plane_outline"; triangle: number; extrude: boolean; depth?: number | null; op?: BodyOp }
  | { kind: "cylinder"; triangle: number }
  | { kind: "slice"; plane: Plane }
  | { kind: "work_plane"; triangle: number };

// ─── Utilidades del documento ─────────────────────────────────────────────

const clone = <T>(v: T): T => structuredClone(v);

export const FEATURE_LABELS: Record<FeatureKind["type"], string> = {
  sketch: "Sketch",
  extrude: "Extrusión",
  revolve: "Revolución",
  primitive: "Primitiva",
  fillet: "Redondeo",
  chamfer: "Chaflán",
  shell: "Vaciado",
  draft: "Desmolde",
  pattern: "Patrón",
  mirror: "Simetría",
  split: "Corte",
  import: "Importado",
};

export const OP_LABELS: Record<BodyOp, string> = { join: "Unir", cut: "Restar", intersect: "Intersecar" };

/** Planos base (Z arriba) */
export const BASE_PLANES: Record<"xy" | "xz" | "yz", Plane> = {
  xy: { origin: [0, 0, 0], normal: [0, 0, 1], x_dir: [1, 0, 0] },
  xz: { origin: [0, 0, 0], normal: [0, -1, 0], x_dir: [1, 0, 0] },
  yz: { origin: [0, 0, 0], normal: [1, 0, 0], x_dir: [0, 1, 0] },
};

export const PLANE_LABELS: Record<"xy" | "xz" | "yz", string> = {
  xy: "Planta (XY)",
  xz: "Frente (XZ)",
  yz: "Lateral (YZ)",
};

/** Plano base desplazado a lo largo de su normal */
export function offsetPlane(base: "xy" | "xz" | "yz", offset: number): Plane {
  const p = BASE_PLANES[base];
  return { ...p, origin: [p.normal[0] * offset, p.normal[1] * offset, p.normal[2] * offset] };
}

function nextFeatureId(doc: CadDocument): number {
  const max = doc.features.reduce((m, f) => Math.max(m, f.id + 1), 0);
  return Math.max(max, doc.next_id ?? 0);
}

const PRIMITIVE_LABELS: Record<PrimitiveShape["type"], string> = { box: "Caja", cylinder: "Cilindro", cone: "Cono", sphere: "Esfera", torus: "Toro" };

/** Nombre del tipo de operación (las primitivas, por su forma) */
export function kindLabel(kind: FeatureKind): string {
  return kind.type === "primitive" ? PRIMITIVE_LABELS[kind.shape.type] : FEATURE_LABELS[kind.type];
}

function defaultName(doc: CadDocument, kind: FeatureKind): string {
  const label = kindLabel(kind);
  const n = doc.features.filter((f) => kindLabel(f.kind) === label).length + 1;
  return `${label} ${n}`;
}

/** Operaciones de las que depende `kind` (mismo criterio que el backend) */
export function dependencies(kind: FeatureKind): number[] {
  const axis = (a: AxisSpec) => (a.type === "sketch_line" ? [a.sketch] : []);
  switch (kind.type) {
    case "extrude":
      return [kind.sketch];
    case "revolve":
      return [kind.sketch, ...axis(kind.axis)];
    case "pattern":
      return [...kind.features, ...(kind.pattern.type === "circular" ? axis(kind.pattern.axis) : [])];
    case "mirror":
      return kind.features;
    default:
      return [];
  }
}

export function emptySketch(): Sketch {
  return { points: [], entities: [], constraints: [], next_id: 0 };
}

/** Id nuevo dentro de un sketch (puntos y entidades comparten numeración) */
export function sketchId(s: Sketch): number {
  const max = Math.max(-1, ...s.points.map((p) => p.id), ...s.entities.map((e) => e.id));
  const id = Math.max(max + 1, s.next_id ?? 0);
  s.next_id = id + 1;
  return id;
}

export function addPoint(s: Sketch, p: P2): number {
  const id = sketchId(s);
  s.points.push({ id, x: p[0], y: p[1] });
  return id;
}

export function addEntity(s: Sketch, geometry: Geometry): number {
  const id = sketchId(s);
  s.entities.push({ id, geometry });
  return id;
}

/** Rectángulo con esquinas compartidas y restricciones horizontal/vertical */
export function addRectangle(s: Sketch, a: P2, b: P2): number[] {
  const [x0, x1] = [Math.min(a[0], b[0]), Math.max(a[0], b[0])];
  const [y0, y1] = [Math.min(a[1], b[1]), Math.max(a[1], b[1])];
  const p = [addPoint(s, [x0, y0]), addPoint(s, [x1, y0]), addPoint(s, [x1, y1]), addPoint(s, [x0, y1])];
  const l = [0, 1, 2, 3].map((i) => addEntity(s, { type: "line", start: p[i], end: p[(i + 1) % 4] }));
  s.constraints.push({ type: "horizontal", line: l[0] }, { type: "horizontal", line: l[2] });
  s.constraints.push({ type: "vertical", line: l[1] }, { type: "vertical", line: l[3] });
  return l;
}

/** Borra una entidad, sus restricciones y los puntos que quedan sueltos */
export function removeEntity(s: Sketch, id: number): void {
  const e = s.entities.find((x) => x.id === id);
  if (!e) return;
  s.entities = s.entities.filter((x) => x.id !== id);
  const pointsOf = (g: Geometry): number[] =>
    g.type === "line" ? [g.start, g.end] : g.type === "circle" ? [g.center] : g.type === "arc" ? [g.center, g.start, g.end] : g.points;
  const mentions = (c: SketchConstraint, ids: Set<number>) => Object.entries(c).some(([k, v]) => k !== "type" && typeof v === "number" && k !== "value" && k !== "degrees" && k !== "x" && k !== "y" && ids.has(v));
  s.constraints = s.constraints.filter((c) => !mentions(c, new Set([id])));
  const loose = pointsOf(e.geometry).filter((p) => !s.entities.some((x) => pointsOf(x.geometry).includes(p)));
  s.points = s.points.filter((p) => !loose.includes(p.id));
  s.constraints = s.constraints.filter((c) => !mentions(c, new Set(loose)));
}

/** Líneas que llegan a un punto (para redondear esquinas) */
export function linesAt(s: Sketch, point: number): SketchEntity[] {
  return s.entities.filter((e) => e.geometry.type === "line" && (e.geometry.start === point || e.geometry.end === point));
}

/**
 * Redondea la esquina entre las dos líneas que comparten `point`: las acorta
 * hasta los puntos de tangencia y pone un arco de radio `r` con su cota.
 * Devuelve un mensaje si no se puede.
 */
export function filletCorner(s: Sketch, point: number, r: number): string | undefined {
  const lines = linesAt(s, point);
  if (lines.length !== 2) return "La esquina debe unir exactamente dos líneas";
  const pos = (id: number): P2 => {
    const p = s.points.find((q) => q.id === id)!;
    return [p.x, p.y];
  };
  const P = pos(point);
  const other = (e: SketchEntity) => {
    const g = e.geometry as { start: number; end: number };
    return g.start === point ? g.end : g.start;
  };
  const [A, B] = [pos(other(lines[0])), pos(other(lines[1]))];
  const unit = (v: P2): P2 => {
    const l = Math.hypot(v[0], v[1]);
    return [v[0] / l, v[1] / l];
  };
  const u = unit([A[0] - P[0], A[1] - P[1]]);
  const v = unit([B[0] - P[0], B[1] - P[1]]);
  const cos = Math.max(-1, Math.min(1, u[0] * v[0] + u[1] * v[1]));
  const half = Math.acos(cos) / 2;
  if (half < 1e-3 || Math.PI / 2 - half < 1e-3) return "Las líneas están alineadas";
  const t = r / Math.tan(half);
  if (t >= Math.hypot(A[0] - P[0], A[1] - P[1]) || t >= Math.hypot(B[0] - P[0], B[1] - P[1])) return "El radio no entra en esas líneas";
  const bis = unit([u[0] + v[0], u[1] + v[1]]);
  const d = r / Math.sin(half);
  const T1: P2 = [P[0] + u[0] * t, P[1] + u[1] * t];
  const T2: P2 = [P[0] + v[0] * t, P[1] + v[1] * t];
  const C: P2 = [P[0] + bis[0] * d, P[1] + bis[1] * d];
  const t1 = addPoint(s, T1);
  const t2 = addPoint(s, T2);
  const c = addPoint(s, C);
  for (const [line, tp] of [[lines[0], t1], [lines[1], t2]] as const) {
    const g = line.geometry as { start: number; end: number };
    if (g.start === point) g.start = tp;
    else g.end = tp;
  }
  // Arco antihorario por el lado corto
  const cross = (T1[0] - C[0]) * (T2[1] - C[1]) - (T1[1] - C[1]) * (T2[0] - C[0]);
  const arc = addEntity(s, cross > 0 ? { type: "arc", center: c, start: t1, end: t2 } : { type: "arc", center: c, start: t2, end: t1 });
  // La esquina vieja desaparece con sus restricciones
  s.constraints = s.constraints.filter((k) => !Object.entries(k).some(([key, val]) => key !== "type" && key !== "value" && key !== "degrees" && key !== "x" && key !== "y" && val === point));
  if (!s.entities.some((e) => e.geometry.type !== "spline" && Object.values(e.geometry).includes(point))) s.points = s.points.filter((q) => q.id !== point);
  s.constraints.push({ type: "tangent", a: lines[0].id, b: arc }, { type: "tangent", a: lines[1].id, b: arc }, { type: "radius", entity: arc, value: r });
  return undefined;
}

/**
 * Parte la línea `lineId` en el punto `point` (que debe estar sobre ella): dos
 * líneas que lo comparten, así las regiones se cierran en ese cruce. Copia
 * horizontal/vertical; las cotas de largo de la línea entera se quitan.
 */
export function splitLineAt(s: Sketch, lineId: number, point: number): void {
  const line = s.entities.find((e) => e.id === lineId);
  if (!line || line.geometry.type !== "line") return;
  const g = line.geometry;
  if (g.start === point || g.end === point) return;
  const oldEnd = g.end;
  g.end = point;
  const rest = addEntity(s, { type: "line", start: point, end: oldEnd });
  if (line.construction) s.entities.find((e) => e.id === rest)!.construction = true;
  for (const k of [...s.constraints]) {
    if ((k.type === "horizontal" || k.type === "vertical") && k.line === lineId) s.constraints.push({ type: k.type, line: rest });
  }
  s.constraints = s.constraints.filter((k) => !(k.type === "length" && k.line === lineId));
  // El punto ya es extremo compartido: no necesita "punto en línea"
  s.constraints = s.constraints.filter((k) => !(k.type === "point_on_line" && k.point === point && k.line === lineId));
}

/**
 * Recorta una línea: quita el tramo entre cruces (con otras líneas, círculos
 * o arcos) donde cayó el clic `p`. Los extremos nuevos quedan pegados a la
 * curva que cruzan. Devuelve un mensaje si no hay nada que recortar.
 */
export function trimLine(s: Sketch, lineId: number, p: P2): string | undefined {
  const line = s.entities.find((e) => e.id === lineId);
  if (!line || line.geometry.type !== "line") return "Solo se recortan líneas";
  const g = line.geometry;
  const pos = (id: number): P2 => {
    const q = s.points.find((x) => x.id === id)!;
    return [q.x, q.y];
  };
  const A = pos(g.start);
  const B = pos(g.end);
  const d: P2 = [B[0] - A[0], B[1] - A[1]];
  const len2 = d[0] * d[0] + d[1] * d[1];
  const eps = 1e-9;
  // Cruces: parámetro sobre la línea y con qué entidad
  const cuts: { t: number; entity: number; kind: "line" | "circle" }[] = [];
  for (const e of s.entities) {
    if (e.id === lineId || e.construction) continue;
    const o = e.geometry;
    if (o.type === "line") {
      const [C, D] = [pos(o.start), pos(o.end)];
      const f: P2 = [D[0] - C[0], D[1] - C[1]];
      const den = d[0] * f[1] - d[1] * f[0];
      if (Math.abs(den) < eps) continue;
      const t = ((C[0] - A[0]) * f[1] - (C[1] - A[1]) * f[0]) / den;
      const u = ((C[0] - A[0]) * d[1] - (C[1] - A[1]) * d[0]) / den;
      if (t > eps && t < 1 - eps && u >= -eps && u <= 1 + eps) cuts.push({ t, entity: e.id, kind: "line" });
    } else if (o.type === "circle" || o.type === "arc") {
      const c = pos(o.center);
      const r = o.type === "circle" ? o.radius : Math.hypot(pos(o.start)[0] - c[0], pos(o.start)[1] - c[1]);
      const m: P2 = [A[0] - c[0], A[1] - c[1]];
      const b = 2 * (m[0] * d[0] + m[1] * d[1]);
      const cc = m[0] * m[0] + m[1] * m[1] - r * r;
      const disc = b * b - 4 * len2 * cc;
      if (disc < 0) continue;
      for (const t of [(-b - Math.sqrt(disc)) / (2 * len2), (-b + Math.sqrt(disc)) / (2 * len2)]) {
        if (t <= eps || t >= 1 - eps) continue;
        if (o.type === "arc") {
          // ¿El cruce cae dentro del barrido del arco?
          const q: P2 = [A[0] + t * d[0] - c[0], A[1] + t * d[1] - c[1]];
          const a0 = Math.atan2(pos(o.start)[1] - c[1], pos(o.start)[0] - c[0]);
          let sweep = Math.atan2(pos(o.end)[1] - c[1], pos(o.end)[0] - c[0]) - a0;
          while (sweep <= 0) sweep += 2 * Math.PI;
          let a = Math.atan2(q[1], q[0]) - a0;
          while (a < 0) a += 2 * Math.PI;
          if (a > sweep) continue;
        }
        cuts.push({ t, entity: e.id, kind: "circle" });
      }
    }
  }
  if (cuts.length === 0) {
    // Sin cruces: el tramo entero se va
    removeEntity(s, lineId);
    return undefined;
  }
  cuts.sort((x, y) => x.t - y.t);
  const tc = ((p[0] - A[0]) * d[0] + (p[1] - A[1]) * d[1]) / len2;
  const before = [...cuts].reverse().find((c) => c.t < tc);
  const after = cuts.find((c) => c.t > tc);
  const at = (c: { t: number }): P2 => [A[0] + c.t * d[0], A[1] + c.t * d[1]];
  // Las líneas cruzadas se parten en el punto (cierra regiones); en curvas, pegado
  const stick = (point: number, c: { entity: number; kind: "line" | "circle" }) => {
    if (c.kind === "line") splitLineAt(s, c.entity, point);
    else s.constraints.push({ type: "point_on_circle", point, circle: c.entity });
  };
  if (before && after) {
    // Tramo del medio: la línea queda partida en dos
    const p1 = addPoint(s, at(before));
    const p2 = addPoint(s, at(after));
    const oldEnd = g.end;
    g.end = p1;
    const rest = addEntity(s, { type: "line", start: p2, end: oldEnd });
    stick(p1, before);
    stick(p2, after);
    // Horizontal/vertical valen para las dos partes
    for (const k of [...s.constraints]) {
      if ((k.type === "horizontal" || k.type === "vertical") && k.line === lineId) s.constraints.push({ type: k.type, line: rest });
    }
    // Cotas de largo de la línea entera ya no tienen sentido
    s.constraints = s.constraints.filter((k) => !(k.type === "length" && k.line === lineId));
  } else if (after) {
    const np = addPoint(s, at(after));
    g.start = np;
    stick(np, after);
    s.constraints = s.constraints.filter((k) => !(k.type === "length" && k.line === lineId));
  } else if (before) {
    const np = addPoint(s, at(before));
    g.end = np;
    stick(np, before);
    s.constraints = s.constraints.filter((k) => !(k.type === "length" && k.line === lineId));
  }
  // Puntos que quedaron sueltos
  const used = new Set(s.entities.flatMap((e) => (e.geometry.type === "line" ? [e.geometry.start, e.geometry.end] : e.geometry.type === "spline" ? e.geometry.points : e.geometry.type === "circle" ? [e.geometry.center] : [e.geometry.center, e.geometry.start, e.geometry.end])));
  const loose = new Set(s.points.filter((q) => !used.has(q.id)).map((q) => q.id));
  s.points = s.points.filter((q) => !loose.has(q.id));
  s.constraints = s.constraints.filter((k) => !Object.entries(k).some(([key, v]) => key !== "type" && key !== "value" && key !== "degrees" && key !== "x" && key !== "y" && typeof v === "number" && loose.has(v)));
  return undefined;
}

/** Valor editable de una restricción (cota), si tiene */
export function constraintValue(c: SketchConstraint): number | undefined {
  if ("value" in c) return c.value;
  if (c.type === "angle") return c.degrees;
  return undefined;
}

export const CONSTRAINT_LABELS: Record<SketchConstraint["type"], string> = {
  coincident: "Coincidente",
  fixed: "Fijo",
  horizontal: "Horizontal",
  vertical: "Vertical",
  parallel: "Paralelas",
  perpendicular: "Perpendiculares",
  equal: "Iguales",
  tangent: "Tangente",
  point_on_line: "Punto en línea",
  point_on_circle: "Punto en círculo",
  midpoint: "Punto medio",
  symmetric: "Simétricos",
  distance: "Distancia",
  horizontal_distance: "Distancia horizontal",
  vertical_distance: "Distancia vertical",
  length: "Largo",
  radius: "Radio",
  diameter: "Diámetro",
  angle: "Ángulo",
};

// ─── Store ────────────────────────────────────────────────────────────────

const HISTORY_LIMIT = 100;

export function createCadStore() {
  const [status, setStatus] = createSignal<CadStatus>();
  const [doc, setDoc] = createSignal<CadDocument | null>(null);
  const [result, setResult] = createSignal<CadResult | null>(null);
  const [mesh, setMesh] = createSignal<CadMesh | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string>();
  const [selected, setSelected] = createSignal<number>();
  const undoStack: CadDocument[] = [];
  const redoStack: CadDocument[] = [];
  const [historyVersion, setHistoryVersion] = createSignal(0);
  let meshVersion = -1;
  // Envíos encadenados: el último gana y los intermedios no pisan el resultado
  let sendSeq = 0;

  const refreshMesh = async (version: number) => {
    if (version === meshVersion) return;
    meshVersion = version;
    const buf = await invoke<ArrayBuffer>("cad_mesh");
    if (meshVersion === version) setMesh(decodeCadMesh(buf));
  };

  const apply = async (r: CadResult) => {
    setResult(r);
    await refreshMesh(r.version);
  };

  const send = async (next: CadDocument) => {
    const seq = ++sendSeq;
    setBusy(true);
    try {
      const r = await invoke<CadResult>("cad_set_document", { document: next });
      if (seq === sendSeq) {
        setError(undefined);
        await apply(r);
      }
    } catch (e) {
      if (seq === sendSeq) setError(String(e));
    } finally {
      if (seq === sendSeq) setBusy(false);
    }
  };

  /** Cambia el documento (deshacible) y recalcula */
  const commit = (mutate: (d: CadDocument) => void) => {
    const current = doc() ?? { features: [] };
    const next = clone(current);
    mutate(next);
    undoStack.push(current);
    if (undoStack.length > HISTORY_LIMIT) undoStack.shift();
    redoStack.length = 0;
    setHistoryVersion((v) => v + 1);
    setDoc(next);
    return send(next);
  };

  const store = {
    status,
    doc,
    result,
    mesh,
    busy,
    error,
    setError,
    selected,
    select: setSelected,
    canUndo: () => (historyVersion(), undoStack.length > 0),
    canRedo: () => (historyVersion(), redoStack.length > 0),

    async init() {
      const s = await invoke<CadStatus>("cad_status");
      setStatus(s);
      await store.reload();
    },

    /** Trae el documento del backend (al abrir un proyecto o empezar uno nuevo) */
    async reload() {
      const d = await invoke<CadDocument | null>("cad_get_document");
      undoStack.length = 0;
      redoStack.length = 0;
      setHistoryVersion((v) => v + 1);
      setDoc(d);
      setSelected(undefined);
      if (!d) {
        setResult(null);
        setMesh(null);
        meshVersion = -1;
        return;
      }
      try {
        await apply(await invoke<CadResult>("cad_evaluate"));
      } catch (e) {
        setError(String(e));
      }
    },

    async newDesign() {
      const r = await invoke<CadResult>("cad_new");
      undoStack.length = 0;
      redoStack.length = 0;
      setHistoryVersion((v) => v + 1);
      setDoc({ features: [] });
      setSelected(undefined);
      await apply(r);
    },

    async close() {
      await invoke("cad_close");
      await store.reload();
    },

    commit,

    /** Agrega una operación al final (o donde está la barra de retroceso) y la selecciona */
    async addFeature(kind: FeatureKind, name?: string): Promise<number> {
      const current = doc() ?? { features: [] };
      const id = nextFeatureId(current);
      setSelected(id);
      await commit((d) => {
        const at = Math.min(d.rollback ?? d.features.length, d.features.length);
        d.features.splice(at, 0, { id, name: name ?? defaultName(d, kind), suppressed: false, kind });
        d.next_id = id + 1;
        if (d.rollback != null) d.rollback += 1;
      });
      return id;
    },

    updateFeature(id: number, mutate: (f: Feature) => void) {
      return commit((d) => {
        const f = d.features.find((x) => x.id === id);
        if (f) mutate(f);
      });
    },

    /** Borra la operación; `false` si otras dependen de ella */
    removeFeature(id: number): boolean {
      const d = doc();
      if (!d) return false;
      if (d.features.some((f) => dependencies(f.kind).includes(id))) return false;
      void commit((n) => {
        const i = n.features.findIndex((f) => f.id === id);
        if (i < 0) return;
        n.features.splice(i, 1);
        if (n.rollback != null && i < n.rollback) n.rollback -= 1;
      });
      if (selected() === id) setSelected(undefined);
      return true;
    },

    /** Mueve la operación una posición; `false` si rompe dependencias */
    moveFeature(id: number, delta: -1 | 1): boolean {
      const d = doc();
      if (!d) return false;
      const i = d.features.findIndex((f) => f.id === id);
      const j = i + delta;
      if (i < 0 || j < 0 || j >= d.features.length) return false;
      const order = [...d.features];
      [order[i], order[j]] = [order[j], order[i]];
      const pos = (x: number) => order.findIndex((f) => f.id === x);
      const broken = order.some((f, k) => dependencies(f.kind).some((dep) => pos(dep) < 0 || pos(dep) > k));
      if (broken) return false;
      void commit((n) => {
        n.features = order.map((f) => n.features.find((x) => x.id === f.id)!);
      });
      return true;
    },

    setRollback(index: number | null) {
      return commit((d) => {
        d.rollback = index == null || index >= d.features.length ? null : index;
      });
    },

    undo() {
      const prev = undoStack.pop();
      if (!prev) return;
      const current = doc();
      if (current) redoStack.push(current);
      setHistoryVersion((v) => v + 1);
      setDoc(prev);
      void send(prev);
    },

    redo() {
      const next = redoStack.pop();
      if (!next) return;
      const current = doc();
      if (current) undoStack.push(current);
      setHistoryVersion((v) => v + 1);
      setDoc(next);
      void send(next);
    },

    stateOf(id: number): FeatureState | undefined {
      return result()?.status.find((s) => s.id === id);
    },

    sketchView(id: number): SketchView | undefined {
      return result()?.sketches.find((s) => s.id === id);
    },

    faceRef: (face: number) => invoke<FaceRef>("cad_face_ref", { face }),
    edgeRef: (edge: number) => invoke<EdgeRef>("cad_edge_ref", { edge }),
    solveSketch: (sketch: Sketch, drag?: [number, P2]) => invoke<SolvedSketch>("cad_solve_sketch", { sketch, drag: drag ?? null }),

    exportDesign: (path: string, format: string) => invoke<number>("cad_export", { path, format }),

    async importStep(path: string, op: BodyOp = "join") {
      setBusy(true);
      try {
        await apply(await invoke<CadResult>("cad_import_step", { path, op }));
        await store.syncDocument();
      } finally {
        setBusy(false);
      }
    },

    // Escaneo → CAD
    scanPick: (kind: "plane" | "cylinder", triangle: number) => invoke<ScanPick>("cad_scan_pick", { kind, triangle, options: null }),
    scanDetect: () => invoke<Detection[]>("cad_scan_detect", { options: null }),

    async scanAdd(feature: ScanFeature) {
      const before = doc();
      setBusy(true);
      try {
        await apply(await invoke<CadResult>("cad_scan_add", { feature, options: null }));
        if (before) {
          undoStack.push(before);
          redoStack.length = 0;
          setHistoryVersion((v) => v + 1);
        }
        await store.syncDocument();
      } finally {
        setBusy(false);
      }
    },

    /** El backend agregó operaciones por su cuenta: traer el documento sin perder el historial */
    async syncDocument() {
      const d = await invoke<CadDocument | null>("cad_get_document");
      setDoc(d);
      const last = d?.features[d.features.length - 1];
      if (last) setSelected(last.id);
    },
  };
  return store;
}

export type CadStore = ReturnType<typeof createCadStore>;
