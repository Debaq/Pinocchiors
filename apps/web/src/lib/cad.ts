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
import type { Snap } from "./sketchSnap";
import type { Contour } from "./sketchText";

export type P2 = [number, number];
export type P3 = [number, number, number];

// ─── Documento (mismo JSON que cad_model) ─────────────────────────────────

export interface Plane {
  origin: P3;
  normal: P3;
  x_dir: P3;
}

/** Origen de una cara: operación que la creó y qué parte ("fin", "lado:7", "+z"…) */
export interface FaceTag {
  feature: number;
  name: string;
}

export interface FaceRef {
  point: P3;
  normal: P3;
  /** Orígenes de la cara: se busca por ellos antes que por posición */
  tags?: FaceTag[];
}

export interface EdgeRef {
  point: P3;
  direction: P3;
  /** Orígenes de las dos caras que separa la arista */
  sides?: FaceTag[][];
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
  /** Manijas opcionales (abierta): dirección de salida y de llegada, en el sentido de avance */
  | { type: "spline"; points: number[]; closed: boolean; start_handle?: number; end_handle?: number }
  /** Punto suelto: no forma perfiles */
  | { type: "point"; point: number }
  /** Elipse: extremos de los semiejes (a 90°) */
  | { type: "ellipse"; center: number; major: number; minor: number };

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
  | { type: "horizontal_points"; a: number; b: number }
  | { type: "vertical_points"; a: number; b: number }
  | { type: "parallel"; a: number; b: number }
  | { type: "perpendicular"; a: number; b: number }
  | { type: "equal"; a: number; b: number }
  | { type: "tangent"; a: number; b: number }
  | { type: "concentric"; a: number; b: number }
  | { type: "point_on_line"; point: number; line: number }
  | { type: "point_on_circle"; point: number; circle: number }
  | { type: "midpoint"; point: number; line: number }
  | { type: "symmetric"; a: number; b: number; line: number }
  | { type: "equal_offset"; a1: number; a2: number; b1: number; b2: number }
  | { type: "equal_rotation"; center: number; a1: number; a2: number; b1: number; b2: number }
  | { type: "distance"; a: number; b: number; value: number; reference?: boolean }
  | { type: "horizontal_distance"; a: number; b: number; value: number; reference?: boolean }
  | { type: "vertical_distance"; a: number; b: number; value: number; reference?: boolean }
  | { type: "length"; line: number; value: number; reference?: boolean }
  | { type: "radius"; entity: number; value: number; reference?: boolean }
  | { type: "diameter"; entity: number; value: number; reference?: boolean }
  | { type: "angle"; a: number; b: number; degrees: number; reference?: boolean };

export interface Sketch {
  points: SketchPoint[];
  entities: SketchEntity[];
  constraints: SketchConstraint[];
  next_id?: number;
  /** Punto origen, fijo en (0, 0): no se borra */
  origin?: number;
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

/** Parámetro con nombre: `ancho = 40`, `alto = ancho / 2` */
export interface Parameter {
  name: string;
  expr: string;
}

export interface CadDocument {
  features: Feature[];
  rollback?: number | null;
  next_id?: number;
  parameters?: Parameter[];
  /** Campos calculados por fórmula: ruta (`<id>.kind.…`) → expresión */
  bindings?: Record<string, string>;
}

/** Valor calculado de un parámetro (key = nombre) o de un campo vinculado (key = ruta) */
export interface ResolvedValue {
  key: string;
  value: number | null;
  error: string | null;
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
  /** Entidades a las que les falta definir (algún punto o el radio libre) */
  free_entities?: number[];
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
  parameters: ResolvedValue[];
  bindings: ResolvedValue[];
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
  return { points: [{ id: 0, x: 0, y: 0 }], entities: [], constraints: [], next_id: 1, origin: 0 };
}

/** Agrega el punto origen a un sketch que no lo tiene (los de antes) */
export function ensureOrigin(s: Sketch): number {
  if (s.origin !== undefined && s.points.some((p) => p.id === s.origin)) return s.origin;
  s.origin = addPoint(s, [0, 0]);
  return s.origin;
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

/** Contorno de una elipse de centro `c` y semiejes hasta `a` y `b` */
export function ellipsePolyline(c: P2, a: P2, b: P2, n = 64): P2[] {
  const u: P2 = [a[0] - c[0], a[1] - c[1]];
  const v: P2 = [b[0] - c[0], b[1] - c[1]];
  return Array.from({ length: n + 1 }, (_, i) => {
    const t = (i / n) * 2 * Math.PI;
    return [c[0] + u[0] * Math.cos(t) + v[0] * Math.sin(t), c[1] + u[1] * Math.cos(t) + v[1] * Math.sin(t)] as P2;
  });
}

/**
 * Curva suave por los puntos (Catmull-Rom, como se ve antes de recalcular).
 * `t0`/`t1`: dirección de salida y de llegada impuestas por las manijas.
 */
export function splinePolyline(p: P2[], closed: boolean, t0?: P2, t1?: P2, steps = 16): P2[] {
  const n = p.length;
  if (n < 2) return p.slice();
  const sub = (a: P2, b: P2): P2 => [a[0] - b[0], a[1] - b[1]];
  const len = (v: P2) => Math.hypot(v[0], v[1]) || 1;
  const along = (dir: P2, ref: P2): P2 => [(dir[0] / len(dir)) * len(ref), (dir[1] / len(dir)) * len(ref)];
  const at = (i: number) => p[((i % n) + n) % n];
  const tangent = (i: number): P2 => {
    if (closed) return sub(at(i + 1), at(i - 1)).map((v) => v / 2) as P2;
    if (i === 0) return t0 ? along(t0, sub(p[1], p[0])) : sub(p[1], p[0]);
    if (i === n - 1) return t1 ? along(t1, sub(p[n - 1], p[n - 2])) : sub(p[n - 1], p[n - 2]);
    return sub(p[i + 1], p[i - 1]).map((v) => v / 2) as P2;
  };
  const out: P2[] = [];
  const segs = closed ? n : n - 1;
  for (let i = 0; i < segs; i++) {
    const [a, b, ma, mb] = [at(i), at(i + 1), tangent(i), tangent((i + 1) % (closed ? n : n + 1))];
    for (let k = 0; k < steps; k++) {
      const t = k / steps;
      const [h00, h10, h01, h11] = [2 * t ** 3 - 3 * t ** 2 + 1, t ** 3 - 2 * t ** 2 + t, -2 * t ** 3 + 3 * t ** 2, t ** 3 - t ** 2];
      out.push([h00 * a[0] + h10 * ma[0] + h01 * b[0] + h11 * mb[0], h00 * a[1] + h10 * ma[1] + h01 * b[1] + h11 * mb[1]]);
    }
  }
  out.push(closed ? p[0] : p[n - 1]);
  return out;
}

/** Contorno de una spline del sketch (con sus manijas) */
export function splineOf(g: Extract<Geometry, { type: "spline" }>, point: (id: number) => P2 | undefined): P2[] | undefined {
  const pts = g.points.map(point);
  if (pts.some((p) => !p)) return undefined;
  const p = pts as P2[];
  const h0 = g.start_handle !== undefined ? point(g.start_handle) : undefined;
  const h1 = g.end_handle !== undefined ? point(g.end_handle) : undefined;
  const t0: P2 | undefined = h0 && [h0[0] - p[0][0], h0[1] - p[0][1]];
  const t1: P2 | undefined = h1 && [h1[0] - p[p.length - 1][0], h1[1] - p[p.length - 1][1]];
  return splinePolyline(p, g.closed, t0, t1);
}

/** Manijas en los extremos de una spline abierta (o las quita si ya tiene) */
export function toggleSplineHandles(s: Sketch, id: number): string | undefined {
  const e = s.entities.find((x) => x.id === id);
  if (e?.geometry.type !== "spline") return "Elegir una spline";
  const g = e.geometry;
  if (g.closed) return "Las manijas son para splines abiertas";
  if (g.start_handle !== undefined || g.end_handle !== undefined) {
    const hs = [g.start_handle, g.end_handle].filter((x): x is number => x !== undefined);
    delete g.start_handle;
    delete g.end_handle;
    s.points = s.points.filter((q) => !hs.includes(q.id) || s.entities.some((x) => geometryPoints(x.geometry).includes(q.id)));
    s.constraints = s.constraints.filter((k) => !constraintIds(k).some((v) => hs.includes(v)));
    return undefined;
  }
  const p = g.points.map((q) => pointOf(s, q));
  const n = p.length;
  // A un tercio del tramo vecino, en el sentido de avance
  g.start_handle = addPoint(s, [p[0][0] + (p[1][0] - p[0][0]) / 3, p[0][1] + (p[1][1] - p[0][1]) / 3]);
  g.end_handle = addPoint(s, [p[n - 1][0] + (p[n - 1][0] - p[n - 2][0]) / 3, p[n - 1][1] + (p[n - 1][1] - p[n - 2][1]) / 3]);
  return undefined;
}

/**
 * Agrega al sketch los contornos de un texto (ver `sketchText.outlineContours`):
 * los tramos de un mismo contorno comparten sus extremos. Devuelve las
 * entidades creadas.
 */
export function addTextContours(s: Sketch, contours: Contour[]): number[] {
  const out: number[] = [];
  for (const c of contours) {
    if ("closed" in c) {
      if (c.closed.length < 3) continue;
      out.push(addEntity(s, { type: "spline", points: c.closed.map((p) => addPoint(s, p)), closed: true }));
      continue;
    }
    const ids = new Map<string, number>();
    const at = (p: P2) => {
      const k = `${p[0].toFixed(9)},${p[1].toFixed(9)}`;
      if (!ids.has(k)) ids.set(k, addPoint(s, p));
      return ids.get(k)!;
    };
    for (const piece of c.pieces) {
      if (piece.kind === "line") out.push(addEntity(s, { type: "line", start: at(piece.a), end: at(piece.b) }));
      else {
        const pts = piece.points;
        const ids2 = pts.map((p, i) => (i === 0 || i === pts.length - 1 ? at(p) : addPoint(s, p)));
        out.push(addEntity(s, { type: "spline", points: ids2, closed: false }));
      }
    }
  }
  return out;
}

/** Puntos que usa una geometría */
export function geometryPoints(g: Geometry): number[] {
  switch (g.type) {
    case "line":
      return [g.start, g.end];
    case "circle":
      return [g.center];
    case "arc":
      return [g.center, g.start, g.end];
    case "spline":
      return [...g.points, ...(g.start_handle !== undefined ? [g.start_handle] : []), ...(g.end_handle !== undefined ? [g.end_handle] : [])];
    case "point":
      return [g.point];
    case "ellipse":
      return [g.center, g.major, g.minor];
  }
}

/**
 * Copia de las entidades con sus puntos pasados por `map` (los que devuelve
 * igual se comparten). Los círculos copiados quedan con el mismo radio;
 * `flip` invierte los arcos (la simetría cambia el sentido).
 */
function copyEntities(s: Sketch, ids: number[], map: (p: number) => number, flip = false): number[] {
  const out: number[] = [];
  for (const id of ids) {
    const e = s.entities.find((x) => x.id === id);
    if (!e) continue;
    const g = e.geometry;
    let copy: Geometry;
    if (g.type === "line") copy = { type: "line", start: map(g.start), end: map(g.end) };
    else if (g.type === "circle") copy = { type: "circle", center: map(g.center), radius: g.radius };
    else if (g.type === "arc") copy = flip ? { type: "arc", center: map(g.center), start: map(g.end), end: map(g.start) } : { type: "arc", center: map(g.center), start: map(g.start), end: map(g.end) };
    else if (g.type === "spline")
      copy = {
        type: "spline",
        points: g.points.map(map),
        closed: g.closed,
        ...(g.start_handle !== undefined ? { start_handle: map(g.start_handle) } : {}),
        ...(g.end_handle !== undefined ? { end_handle: map(g.end_handle) } : {}),
      };
    else if (g.type === "ellipse") copy = { type: "ellipse", center: map(g.center), major: map(g.major), minor: map(g.minor) };
    else copy = { type: "point", point: map(g.point) };
    const c = addEntity(s, copy);
    if (e.construction) s.entities.find((x) => x.id === c)!.construction = true;
    if (g.type === "circle") s.constraints.push({ type: "equal", a: id, b: c });
    out.push(c);
  }
  return out;
}

/** Puntos de las entidades, sin repetir, en orden */
function pointsOfEntities(s: Sketch, ids: number[]): number[] {
  const out: number[] = [];
  for (const id of ids) {
    const e = s.entities.find((x) => x.id === id);
    for (const p of e ? geometryPoints(e.geometry) : []) if (!out.includes(p)) out.push(p);
  }
  return out;
}

/**
 * Simetría: copia las entidades reflejadas respecto de la línea `axis`. Cada
 * punto copiado queda simétrico de su original; los que están sobre el eje
 * se comparten. Devuelve un mensaje si no se puede.
 */
export function mirrorEntities(s: Sketch, ids: number[], axis: number): string | undefined {
  const ax = s.entities.find((e) => e.id === axis)?.geometry;
  if (ax?.type !== "line") return "El eje de la simetría tiene que ser una línea";
  const [A, B] = [pointOf(s, ax.start), pointOf(s, ax.end)];
  const d: P2 = [B[0] - A[0], B[1] - A[1]];
  const l2 = d[0] * d[0] + d[1] * d[1];
  if (l2 === 0) return "El eje no tiene largo";
  const items = ids.filter((id) => id !== axis);
  if (!items.length) return "Elegir también lo que se refleja";
  const tol = 1e-9 * Math.max(1, Math.sqrt(l2));
  const made = new Map<number, number>();
  const map = (p: number): number => {
    if (made.has(p)) return made.get(p)!;
    const P = pointOf(s, p);
    const t = ((P[0] - A[0]) * d[0] + (P[1] - A[1]) * d[1]) / l2;
    const foot: P2 = [A[0] + t * d[0], A[1] + t * d[1]];
    if (Math.hypot(P[0] - foot[0], P[1] - foot[1]) <= tol) {
      made.set(p, p);
      return p;
    }
    const q = addPoint(s, [2 * foot[0] - P[0], 2 * foot[1] - P[1]]);
    s.constraints.push({ type: "symmetric", a: p, b: q, line: axis });
    made.set(p, q);
    return q;
  };
  copyEntities(s, items, map, true);
  return undefined;
}

/**
 * Patrón lineal: `count` en total (el original y count − 1 copias) corridas de
 * a `offset`. Solo el primer par lleva cotas (distancia horizontal y
 * vertical); el resto sigue con "mismo desplazamiento".
 */
export function linearPattern(s: Sketch, ids: number[], count: number, offset: P2): string | undefined {
  const pts = pointsOfEntities(s, ids);
  if (!pts.length || count < 2) return "Elegir qué repetir y al menos 2 en total";
  let prev = new Map(pts.map((p) => [p, p]));
  let master: [number, number] | undefined;
  for (let k = 1; k < count; k++) {
    const next = new Map<number, number>();
    for (const p of pts) {
      const P = pointOf(s, p);
      const q = addPoint(s, [P[0] + k * offset[0], P[1] + k * offset[1]]);
      next.set(p, q);
      if (!master) {
        master = [p, q];
        s.constraints.push({ type: "horizontal_distance", a: p, b: q, value: offset[0] }, { type: "vertical_distance", a: p, b: q, value: offset[1] });
      } else s.constraints.push({ type: "equal_offset", a1: master[0], a2: master[1], b1: prev.get(p)!, b2: q });
    }
    copyEntities(s, ids, (p) => next.get(p) ?? p);
    prev = next;
  }
  return undefined;
}

/**
 * Patrón circular alrededor del punto `center`: `count` en total repartidos
 * en la vuelta. El primer par lleva radios iguales y una cota de ángulo
 * (entre dos líneas de construcción); el resto sigue con "mismo giro".
 */
export function circularPattern(s: Sketch, ids: number[], count: number, center: number): string | undefined {
  const pts = pointsOfEntities(s, ids);
  const lead = pts.find((p) => p !== center);
  if (lead === undefined || count < 2) return "Elegir qué repetir y al menos 2 en total";
  const C = pointOf(s, center);
  const step = (2 * Math.PI) / count;
  const rot = (P: P2, a: number): P2 => {
    const [x, y] = [P[0] - C[0], P[1] - C[1]];
    return [C[0] + x * Math.cos(a) - y * Math.sin(a), C[1] + x * Math.sin(a) + y * Math.cos(a)];
  };
  let prev = new Map(pts.map((p) => [p, p]));
  let master: [number, number] | undefined;
  for (let k = 1; k < count; k++) {
    const next = new Map<number, number>();
    for (const p of pts) {
      if (p === center) {
        next.set(p, p);
        continue;
      }
      const q = addPoint(s, rot(pointOf(s, p), k * step));
      next.set(p, q);
      if (!master && p === lead) {
        master = [p, q];
        if (count === 2) {
          // Media vuelta: el centro es el punto medio (un ángulo de 180° es inestable en el solver)
          const d = addEntity(s, { type: "line", start: p, end: q });
          s.entities.find((x) => x.id === d)!.construction = true;
          s.constraints.push({ type: "midpoint", point: center, line: d });
        } else {
          const r0 = addEntity(s, { type: "line", start: center, end: p });
          const r1 = addEntity(s, { type: "line", start: center, end: q });
          for (const r of [r0, r1]) s.entities.find((x) => x.id === r)!.construction = true;
          s.constraints.push({ type: "equal", a: r0, b: r1 }, { type: "angle", a: r0, b: r1, degrees: +((step * 180) / Math.PI).toFixed(6) });
        }
      }
    }
    for (const p of pts) {
      if (p === center || (k === 1 && p === lead)) continue;
      s.constraints.push({ type: "equal_rotation", center, a1: master![0], a2: master![1], b1: prev.get(p)!, b2: next.get(p)! });
    }
    copyEntities(s, ids, (p) => next.get(p) ?? p);
    prev = next;
  }
  return undefined;
}

/** Borra una entidad, sus restricciones y los puntos que quedan sueltos */
export function removeEntity(s: Sketch, id: number): void {
  const e = s.entities.find((x) => x.id === id);
  if (!e) return;
  s.entities = s.entities.filter((x) => x.id !== id);
  const pointsOf = geometryPoints;
  const mentions = (c: SketchConstraint, ids: Set<number>) => Object.entries(c).some(([k, v]) => k !== "type" && typeof v === "number" && k !== "value" && k !== "degrees" && k !== "x" && k !== "y" && ids.has(v));
  s.constraints = s.constraints.filter((c) => !mentions(c, new Set([id])));
  const loose = pointsOf(e.geometry).filter((p) => p !== s.origin && !s.entities.some((x) => pointsOf(x.geometry).includes(p)));
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
  if (!s.entities.some((e) => geometryPoints(e.geometry).includes(point))) s.points = s.points.filter((q) => q.id !== point);
  // Las líneas quedaron más cortas: sus cotas de largo ya no valen
  s.constraints = s.constraints.filter((k) => !(k.type === "length" && (k.line === lines[0].id || k.line === lines[1].id)));
  s.constraints.push({ type: "tangent", a: lines[0].id, b: arc }, { type: "tangent", a: lines[1].id, b: arc }, { type: "radius", entity: arc, value: r });
  return undefined;
}

/**
 * Parte la línea `lineId` en el punto `point` (que debe estar sobre ella): dos
 * líneas que lo comparten, así las regiones se cierran en ese cruce. Copia
 * horizontal/vertical (o deja las dos mitades paralelas: alineadas, porque
 * comparten el punto); la cota de largo de la línea entera pasa a ser la
 * distancia entre sus extremos. Devuelve la mitad nueva.
 */
export function splitLineAt(s: Sketch, lineId: number, point: number): number | undefined {
  const line = s.entities.find((e) => e.id === lineId);
  if (!line || line.geometry.type !== "line") return;
  const g = line.geometry;
  if (g.start === point || g.end === point) return;
  const [start, oldEnd] = [g.start, g.end];
  g.end = point;
  const rest = addEntity(s, { type: "line", start: point, end: oldEnd });
  if (line.construction) s.entities.find((e) => e.id === rest)!.construction = true;
  let axis = false;
  for (const k of [...s.constraints]) {
    if ((k.type === "horizontal" || k.type === "vertical") && k.line === lineId) {
      s.constraints.push({ type: k.type, line: rest });
      axis = true;
    }
  }
  if (!axis) s.constraints.push({ type: "parallel", a: lineId, b: rest });
  // En el lugar (mismo índice): la fórmula vinculada sigue valiendo
  s.constraints.forEach((k, i) => {
    if (k.type === "length" && k.line === lineId) {
      const expr = (k as { expr?: string }).expr;
      s.constraints[i] = { type: "distance", a: start, b: oldEnd, value: k.value, ...(k.reference ? { reference: true } : {}), ...(expr ? { expr } : {}) } as SketchConstraint;
    }
  });
  // El punto ya es extremo compartido: no necesita "punto en línea"
  s.constraints = s.constraints.filter((k) => !(k.type === "point_on_line" && k.point === point && k.line === lineId));
  return rest;
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
  const stick = (point: number, c: { entity: number; kind: "line" | "circle" }) => stickTo(s, point, c);
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
  const used = new Set(s.entities.flatMap((e) => geometryPoints(e.geometry)));
  const loose = new Set(s.points.filter((q) => !used.has(q.id) && q.id !== s.origin).map((q) => q.id));
  s.points = s.points.filter((q) => !loose.has(q.id));
  s.constraints = s.constraints.filter((k) => !Object.entries(k).some(([key, v]) => key !== "type" && key !== "value" && key !== "degrees" && key !== "x" && key !== "y" && typeof v === "number" && loose.has(v)));
  return undefined;
}

/**
 * Equidistante de lo elegido a distancia `d` (positiva = hacia afuera):
 * círculos concéntricos (mismo centro) y lazos cerrados de líneas como
 * polígono paralelo (horizontal/vertical se copian). Devuelve un mensaje si
 * algo no se pudo.
 */
export function offsetEntities(s: Sketch, regions: Region[], ids: number[], d: number): string | undefined {
  const pos = (id: number): P2 => {
    const q = s.points.find((x) => x.id === id)!;
    return [q.x, q.y];
  };
  const done = new Set<number>();
  let skipped = 0;
  for (const id of ids) {
    if (done.has(id)) continue;
    const e = s.entities.find((x) => x.id === id);
    if (!e) continue;
    if (e.geometry.type === "circle") {
      const r = e.geometry.radius + d;
      if (r <= 0) {
        skipped++;
        continue;
      }
      addEntity(s, { type: "circle", center: e.geometry.center, radius: r });
      done.add(id);
      continue;
    }
    // El lazo cerrado que contiene la entidad, solo de líneas
    const loop = regions.flatMap((r) => [r.outer, ...r.holes]).find((l) => l.pieces.some((p) => p.entity === id));
    if (!loop || loop.pieces.some((p) => s.entities.find((x) => x.id === p.entity)?.geometry.type !== "line")) {
      skipped++;
      continue;
    }
    // Vértices en el orden del lazo (antihorario) y aristas desplazadas
    const verts: P2[] = loop.pieces.map((p) => {
      const g = s.entities.find((x) => x.id === p.entity)!.geometry as { start: number; end: number };
      return pos(p.reversed ? g.end : g.start);
    });
    const n = verts.length;
    const lines = verts.map((a, i) => {
      const b = verts[(i + 1) % n];
      const len = Math.hypot(b[0] - a[0], b[1] - a[1]);
      const nx = (b[1] - a[1]) / len;
      const ny = -(b[0] - a[0]) / len;
      // Antihorario: la normal saliente es la derecha del recorrido
      return { a: [a[0] + nx * d, a[1] + ny * d] as P2, dir: [(b[0] - a[0]) / len, (b[1] - a[1]) / len] as P2 };
    });
    const corner = (l1: (typeof lines)[0], l2: (typeof lines)[0]): P2 => {
      const den = l1.dir[0] * l2.dir[1] - l1.dir[1] * l2.dir[0];
      if (Math.abs(den) < 1e-12) return l2.a;
      const t = ((l2.a[0] - l1.a[0]) * l2.dir[1] - (l2.a[1] - l1.a[1]) * l2.dir[0]) / den;
      return [l1.a[0] + l1.dir[0] * t, l1.a[1] + l1.dir[1] * t];
    };
    const pts = lines.map((l, i) => addPoint(s, corner(lines[(i + n - 1) % n], l)));
    loop.pieces.forEach((piece, i) => {
      const line = addEntity(s, { type: "line", start: pts[i], end: pts[(i + 1) % n] });
      for (const k of [...s.constraints]) {
        if ((k.type === "horizontal" || k.type === "vertical") && k.line === piece.entity) s.constraints.push({ type: k.type, line });
      }
      done.add(piece.entity);
    });
  }
  return skipped ? "Algunas entidades no se pudieron desplazar (solo círculos y lazos cerrados de líneas)" : undefined;
}

// ─── Geometría de sketch: cruces, recortes, extensiones ───────────────────

type Hit = { entity: number; kind: "line" | "circle" };

function pointOf(s: Sketch, id: number): P2 {
  const q = s.points.find((x) => x.id === id)!;
  return [q.x, q.y];
}

/** Centro y radio de un círculo o arco; barrido y ángulo inicial si es arco */
function circleOf(s: Sketch, g: Geometry): { c: P2; r: number; a0?: number; sweep?: number } | undefined {
  if (g.type === "circle") return { c: pointOf(s, g.center), r: g.radius };
  if (g.type === "arc") {
    const [c, a, b] = [pointOf(s, g.center), pointOf(s, g.start), pointOf(s, g.end)];
    const a0 = Math.atan2(a[1] - c[1], a[0] - c[0]);
    let sweep = Math.atan2(b[1] - c[1], b[0] - c[0]) - a0;
    while (sweep <= 1e-12) sweep += 2 * Math.PI;
    return { c, r: Math.hypot(a[0] - c[0], a[1] - c[1]), a0, sweep };
  }
  return undefined;
}

/** Ángulo relativo al inicio del arco, en [0, 2π) */
function relAngle(p: P2, c: P2, a0: number): number {
  let t = Math.atan2(p[1] - c[1], p[0] - c[0]) - a0;
  while (t < 0) t += 2 * Math.PI;
  while (t >= 2 * Math.PI) t -= 2 * Math.PI;
  return t;
}

/** Puntos donde las demás entidades (no de construcción) cruzan un círculo o arco */
function crossingsOnCircle(s: Sketch, id: number, c: P2, r: number): { p: P2; hit: Hit }[] {
  const out: { p: P2; hit: Hit }[] = [];
  for (const e of s.entities) {
    if (e.id === id || e.construction) continue;
    const g = e.geometry;
    if (g.type === "line") {
      const [A, B] = [pointOf(s, g.start), pointOf(s, g.end)];
      const d: P2 = [B[0] - A[0], B[1] - A[1]];
      const m: P2 = [A[0] - c[0], A[1] - c[1]];
      const qa = d[0] * d[0] + d[1] * d[1];
      const qb = 2 * (m[0] * d[0] + m[1] * d[1]);
      const qc = m[0] * m[0] + m[1] * m[1] - r * r;
      const disc = qb * qb - 4 * qa * qc;
      if (disc < 0 || qa === 0) continue;
      for (const u of [(-qb - Math.sqrt(disc)) / (2 * qa), (-qb + Math.sqrt(disc)) / (2 * qa)]) {
        if (u >= -1e-9 && u <= 1 + 1e-9) out.push({ p: [A[0] + u * d[0], A[1] + u * d[1]], hit: { entity: e.id, kind: "line" } });
      }
    } else {
      const o = circleOf(s, g);
      if (!o) continue;
      const dx = o.c[0] - c[0];
      const dy = o.c[1] - c[1];
      const dd = Math.hypot(dx, dy);
      if (dd < 1e-12 || dd > r + o.r || dd < Math.abs(r - o.r)) continue;
      const a = (r * r - o.r * o.r + dd * dd) / (2 * dd);
      const h = Math.sqrt(Math.max(0, r * r - a * a));
      const base: P2 = [c[0] + (a * dx) / dd, c[1] + (a * dy) / dd];
      for (const sgn of h > 1e-12 ? [1, -1] : [1]) {
        const p: P2 = [base[0] - (sgn * h * dy) / dd, base[1] + (sgn * h * dx) / dd];
        if (o.sweep !== undefined && relAngle(p, o.c, o.a0!) > o.sweep + 1e-9) continue;
        out.push({ p, hit: { entity: e.id, kind: "circle" } });
      }
    }
  }
  return out;
}

/**
 * Punto para un anclaje (ver `sketchSnap.infer`): el existente, o uno nuevo
 * con la restricción que corresponde. Punto medio: parte la línea y deja las
 * dos mitades iguales. Cuadrante: sobre la curva y alineado con el centro.
 * Intersección: sobre las dos curvas. Alineado: a la misma altura o en la
 * misma vertical que esos puntos. La dirección (paralela, perpendicular,
 * tangente) la pone la herramienta Línea, que crea la línea.
 * Sobre una línea: la parte (así cierra regiones). Sobre una curva: punto en
 * círculo.
 */
export function placeSnap(s: Sketch, snap: Snap): number {
  if (snap.id !== undefined && s.points.some((q) => q.id === snap.id)) return snap.id;
  const id = addPoint(s, snap.p);
  if (snap.align?.h !== undefined) s.constraints.push({ type: "horizontal_points", a: snap.align.h, b: id });
  if (snap.align?.v !== undefined) s.constraints.push({ type: "vertical_points", a: snap.align.v, b: id });
  const e = snap.entity;
  if (e === undefined) return id;
  if (snap.kind === "midpoint") {
    const g = s.entities.find((x) => x.id === e)?.geometry;
    // La línea pudo partirse con otro punto de la misma forma: entonces solo "sobre"
    const mid = g?.type === "line" ? [(pointOf(s, g.start)[0] + pointOf(s, g.end)[0]) / 2, (pointOf(s, g.start)[1] + pointOf(s, g.end)[1]) / 2] : undefined;
    if (mid && Math.hypot(mid[0] - snap.p[0], mid[1] - snap.p[1]) <= 1e-9 * Math.max(1, Math.hypot(snap.p[0], snap.p[1]))) {
      const rest = splitLineAt(s, e, id);
      if (rest !== undefined) s.constraints.push({ type: "equal", a: e, b: rest });
      return id;
    }
    stickTo(s, id, { entity: e, kind: "line" });
    return id;
  }
  if (snap.kind === "quadrant") {
    const g = s.entities.find((x) => x.id === e)?.geometry;
    s.constraints.push({ type: "point_on_circle", point: id, circle: e });
    if (g?.type === "circle" || g?.type === "arc") {
      const q = snap.quadrant ?? 0;
      s.constraints.push({ type: q % 2 === 0 ? "horizontal_points" : "vertical_points", a: g.center, b: id });
    }
    return id;
  }
  if (snap.kind === "intersection") {
    // Sobre las dos curvas: las líneas se parten ahí (cierra regiones)
    for (const c of [e, snap.other]) {
      const g = s.entities.find((x) => x.id === c)?.geometry;
      if (c !== undefined && g) stickTo(s, id, { entity: c, kind: g.type === "line" ? "line" : "circle" });
    }
    return id;
  }
  if (snap.kind === "on_line") stickTo(s, id, { entity: e, kind: "line" });
  else if (snap.kind === "on_circle") s.constraints.push({ type: "point_on_circle", point: id, circle: e });
  return id;
}

/**
 * Deja el punto nuevo sobre lo que cruza: parte la línea o lo pega a la curva.
 * Si la línea ya se partió antes (dos cruces sobre el mismo lado), se parte el
 * tramo que de verdad contiene el punto.
 */
function stickTo(s: Sketch, point: number, hit: Hit) {
  if (hit.kind !== "line") {
    s.constraints.push({ type: "point_on_circle", point, circle: hit.entity });
    return;
  }
  const p = pointOf(s, point);
  const onSegment = (id: number) => {
    const g = s.entities.find((e) => e.id === id)?.geometry;
    if (g?.type !== "line") return Infinity;
    const [a, b] = [pointOf(s, g.start), pointOf(s, g.end)];
    const d: P2 = [b[0] - a[0], b[1] - a[1]];
    const l2 = d[0] * d[0] + d[1] * d[1] || 1;
    const t = ((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / l2;
    if (t < -1e-9 || t > 1 + 1e-9) return Infinity;
    return Math.hypot(p[0] - a[0] - t * d[0], p[1] - a[1] - t * d[1]);
  };
  const tol = 1e-6 * Math.max(1, Math.hypot(p[0], p[1]));
  if (onSegment(hit.entity) <= tol) return splitLineAt(s, hit.entity, point);
  // La línea cruzada ya se partió: el tramo que contiene el punto (sin contar
  // las que ya lo tienen como extremo)
  const lines = s.entities.filter(
    (e) => e.geometry.type === "line" && e.geometry.start !== point && e.geometry.end !== point,
  );
  const best = lines.reduce<{ id: number; d: number } | undefined>((acc, e) => {
    const d = onSegment(e.id);
    return d <= tol && (!acc || d < acc.d) ? { id: e.id, d } : acc;
  }, undefined);
  if (best) splitLineAt(s, best.id, point);
  else s.constraints.push({ type: "point_on_line", point, line: hit.entity });
}

/**
 * Recorta un círculo o un arco en el tramo entre cruces donde cayó `p`: el
 * círculo pasa a ser arco; el arco se acorta o se parte en dos. Sin cruces se
 * borra entero.
 */
export function trimCurve(s: Sketch, id: number, p: P2): string | undefined {
  const e = s.entities.find((x) => x.id === id);
  if (!e) return undefined;
  const o = circleOf(s, e.geometry);
  if (!o) return "Solo círculos y arcos";
  const g = e.geometry as { center: number };
  const a0 = o.a0 ?? 0;
  const sweep = o.sweep ?? 2 * Math.PI;
  const cuts = crossingsOnCircle(s, id, o.c, o.r)
    .map((x) => ({ ...x, t: relAngle(x.p, o.c, a0) }))
    .filter((x) => (o.sweep === undefined ? true : x.t > 1e-9 && x.t < sweep - 1e-9))
    .sort((x, y) => x.t - y.t);
  if (cuts.length === 0) {
    removeEntity(s, id);
    return undefined;
  }
  const tc = relAngle(p, o.c, a0);
  if (e.geometry.type === "circle") {
    if (cuts.length < 2) return "El círculo necesita al menos dos cruces para recortarlo";
    // Tramo quitado: entre el último cruce antes del clic y el primero después
    const after = cuts.find((x) => x.t > tc) ?? cuts[0];
    const before = [...cuts].reverse().find((x) => x.t < tc) ?? cuts[cuts.length - 1];
    const ps = addPoint(s, after.p);
    const pe = addPoint(s, before.p);
    e.geometry = { type: "arc", center: g.center, start: ps, end: pe };
    stickTo(s, ps, after.hit);
    stickTo(s, pe, before.hit);
    return undefined;
  }
  const arc = e.geometry as { center: number; start: number; end: number };
  const before = [...cuts].reverse().find((x) => x.t < tc);
  const after = cuts.find((x) => x.t > tc);
  if (before && after) {
    const pb = addPoint(s, before.p);
    const pa = addPoint(s, after.p);
    const oldEnd = arc.end;
    arc.end = pb;
    addEntity(s, { type: "arc", center: arc.center, start: pa, end: oldEnd });
    stickTo(s, pb, before.hit);
    stickTo(s, pa, after.hit);
  } else if (after) {
    const pa = addPoint(s, after.p);
    arc.start = pa;
    stickTo(s, pa, after.hit);
  } else if (before) {
    const pb = addPoint(s, before.p);
    arc.end = pb;
    stickTo(s, pb, before.hit);
  }
  dropLoosePoints(s);
  return undefined;
}

function dropLoosePoints(s: Sketch) {
  const used = new Set(
    s.entities.flatMap((e) => geometryPoints(e.geometry)),
  );
  const loose = new Set(s.points.filter((q) => !used.has(q.id)).map((q) => q.id));
  if (loose.size === 0) return;
  s.points = s.points.filter((q) => !loose.has(q.id));
  s.constraints = s.constraints.filter((k) => !constraintIds(k).some((v) => loose.has(v)));
}

/**
 * Alarga una línea por el extremo más cercano a `p` hasta lo primero que
 * cruce (otra línea, círculo o arco). El extremo nuevo queda sobre eso.
 */
export function extendLine(s: Sketch, lineId: number, p: P2): string | undefined {
  const line = s.entities.find((e) => e.id === lineId);
  if (!line || line.geometry.type !== "line") return "Solo se extienden líneas";
  const g = line.geometry;
  const [A, B] = [pointOf(s, g.start), pointOf(s, g.end)];
  const atEnd = Math.hypot(p[0] - B[0], p[1] - B[1]) <= Math.hypot(p[0] - A[0], p[1] - A[1]);
  const from = atEnd ? B : A;
  const other = atEnd ? A : B;
  const len = Math.hypot(from[0] - other[0], from[1] - other[1]);
  const d: P2 = [(from[0] - other[0]) / len, (from[1] - other[1]) / len];
  let best: { t: number; hit: Hit } | undefined;
  const consider = (t: number, hit: Hit) => {
    if (t > 1e-9 && (!best || t < best.t)) best = { t, hit };
  };
  for (const e of s.entities) {
    if (e.id === lineId || e.construction) continue;
    const og = e.geometry;
    if (og.type === "line") {
      const [C, D] = [pointOf(s, og.start), pointOf(s, og.end)];
      const f: P2 = [D[0] - C[0], D[1] - C[1]];
      const den = d[0] * f[1] - d[1] * f[0];
      if (Math.abs(den) < 1e-12) continue;
      const t = ((C[0] - from[0]) * f[1] - (C[1] - from[1]) * f[0]) / den;
      const u = ((C[0] - from[0]) * d[1] - (C[1] - from[1]) * d[0]) / den;
      if (u >= -1e-9 && u <= 1 + 1e-9) consider(t, { entity: e.id, kind: "line" });
    } else {
      const o = circleOf(s, og);
      if (!o) continue;
      const m: P2 = [from[0] - o.c[0], from[1] - o.c[1]];
      const b = 2 * (m[0] * d[0] + m[1] * d[1]);
      const c = m[0] * m[0] + m[1] * m[1] - o.r * o.r;
      const disc = b * b - 4 * c;
      if (disc < 0) continue;
      for (const t of [(-b - Math.sqrt(disc)) / 2, (-b + Math.sqrt(disc)) / 2]) {
        const q: P2 = [from[0] + t * d[0], from[1] + t * d[1]];
        if (o.sweep !== undefined && relAngle(q, o.c, o.a0!) > o.sweep + 1e-9) continue;
        consider(t, { entity: e.id, kind: "circle" });
      }
    }
  }
  if (!best) return "No hay nada en esa dirección hasta donde extender";
  const target: P2 = [from[0] + best.t * d[0], from[1] + best.t * d[1]];
  // Punto nuevo para el extremo (si el viejo lo comparte otra entidad, queda allí)
  const np = addPoint(s, target);
  if (atEnd) g.end = np;
  else g.start = np;
  s.constraints = s.constraints.filter((k) => !(k.type === "length" && k.line === lineId));
  stickTo(s, np, best.hit);
  dropLoosePoints(s);
  return undefined;
}

/** Recorta lo que haya bajo el clic: líneas, círculos o arcos */
export function trimAt(s: Sketch, id: number, p: P2): string | undefined {
  const e = s.entities.find((x) => x.id === id);
  if (!e) return undefined;
  if (e.geometry.type === "line") return trimLine(s, id, p);
  if (e.geometry.type === "circle" || e.geometry.type === "arc") return trimCurve(s, id, p);
  return "Las splines no se recortan";
}

/** Puntos y entidades que nombra una restricción (para resaltarla) */
export function constraintIds(c: SketchConstraint): number[] {
  return Object.entries(c)
    .filter(([k, v]) => typeof v === "number" && !["value", "degrees", "x", "y"].includes(k))
    .map(([, v]) => v as number);
}

/**
 * Arco tangente que sale de `start` en la dirección `t` (unitaria) y llega a
 * `end`. Devuelve centro, si va antihorario y la dirección de salida en `end`.
 */
export function tangentArc(start: P2, t: P2, end: P2): { center: P2; ccw: boolean; outDir: P2 } | undefined {
  const n: P2 = [-t[1], t[0]];
  const se: P2 = [start[0] - end[0], start[1] - end[1]];
  const den = 2 * (n[0] * se[0] + n[1] * se[1]);
  if (Math.abs(den) < 1e-12) return undefined; // en línea recta: no hay arco
  const k = -(se[0] * se[0] + se[1] * se[1]) / den;
  const center: P2 = [start[0] + n[0] * k, start[1] + n[1] * k];
  const ccw = k > 0;
  const r: P2 = [end[0] - center[0], end[1] - center[1]];
  const rl = Math.hypot(r[0], r[1]);
  const outDir: P2 = ccw ? [-r[1] / rl, r[0] / rl] : [r[1] / rl, -r[0] / rl];
  return { center, ccw, outDir };
}

/** Dirección en que se sale de `point` siguiendo la entidad hacia afuera de ella */
export function leavingDirection(s: Sketch, entity: number, point: number): P2 | undefined {
  const e = s.entities.find((x) => x.id === entity);
  if (!e) return undefined;
  const g = e.geometry;
  if (g.type === "line") {
    const [p, q] = g.end === point ? [pointOf(s, g.end), pointOf(s, g.start)] : [pointOf(s, g.start), pointOf(s, g.end)];
    const l = Math.hypot(p[0] - q[0], p[1] - q[1]);
    return [(p[0] - q[0]) / l, (p[1] - q[1]) / l];
  }
  if (g.type === "arc") {
    const c = pointOf(s, g.center);
    const r = pointOf(s, point);
    const v: P2 = [r[0] - c[0], r[1] - c[1]];
    const l = Math.hypot(v[0], v[1]);
    // Al final de un arco antihorario se sigue antihorario; al inicio, al revés
    return g.end === point ? [-v[1] / l, v[0] / l] : [v[1] / l, -v[0] / l];
  }
  return undefined;
}

/** Texto que es solo un número (con punto o coma decimal) */
export function plainNumber(text: string): number | undefined {
  const t = text.trim().replace(",", ".");
  return /^[-+]?(\d+\.?\d*|\.\d+)(e[-+]?\d+)?$/i.test(t) ? parseFloat(t) : undefined;
}

/** Ruta de vínculo del valor de una restricción de un sketch */
export function constraintPath(feature: number, index: number, c: SketchConstraint): string {
  return `${feature}.kind.sketch.constraints.${index}.${c.type === "angle" ? "degrees" : "value"}`;
}

/** Cambia el nombre de un parámetro dentro de una fórmula (solo palabras completas) */
export function renameInExpr(expr: string, from: string, to: string): string {
  const esc = from.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return expr.replace(new RegExp(`(?<![\\p{L}\\p{N}_])${esc}(?![\\p{L}\\p{N}_])`, "gu"), to);
}

/** Valor editable de una restricción (cota), si tiene */
/** Cota de referencia: muestra la medida, no restringe */
export function isReference(c: SketchConstraint): boolean {
  return "reference" in c && !!c.reference;
}

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
  horizontal_points: "Alineados horizontal",
  vertical_points: "Alineados vertical",
  parallel: "Paralelas",
  perpendicular: "Perpendiculares",
  equal: "Iguales",
  tangent: "Tangente",
  concentric: "Concéntricos",
  point_on_line: "Punto en línea",
  point_on_circle: "Punto en círculo",
  midpoint: "Punto medio",
  symmetric: "Simétricos",
  equal_offset: "Patrón lineal",
  equal_rotation: "Patrón circular",
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
        // Sus campos vinculados se van con ella
        for (const k of Object.keys(n.bindings ?? {})) if (k.startsWith(`${id}.`)) delete n.bindings![k];
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

    // ─── Parámetros y fórmulas ─────────────────────────────────────────
    /** Fórmula vinculada a un campo, si tiene */
    bindingOf: (path: string): string | undefined => doc()?.bindings?.[path],
    /** Valor calculado de un campo vinculado (o su error) */
    bindingResult: (path: string): ResolvedValue | undefined => result()?.bindings.find((b) => b.key === path),
    parameterResult: (name: string): ResolvedValue | undefined => result()?.parameters.find((p) => p.key === name),

    /** Calcula una fórmula con los parámetros actuales (lanza el error si no se puede) */
    evalExpr: (expr: string) => invoke<number>("cad_eval_expr", { expr, parameters: doc()?.parameters ?? [] }),

    /**
     * Cambia un campo numérico de una operación: con un número lo desvincula,
     * con una fórmula la guarda (y el número calculado) en un solo paso deshacible.
     */
    async setField(feature: number, path: string, text: string, write: (f: Feature, v: number) => void): Promise<string | undefined> {
      const n = plainNumber(text);
      let value = n;
      if (value === undefined) {
        try {
          value = await store.evalExpr(text);
        } catch (e) {
          return String(e);
        }
      }
      await commit((d) => {
        const f = d.features.find((x) => x.id === feature);
        if (f) write(f, value!);
        d.bindings = { ...(d.bindings ?? {}) };
        if (n === undefined) d.bindings[path] = text.trim();
        else delete d.bindings[path];
      });
      return undefined;
    },

    addParameter(name: string, expr: string) {
      return commit((d) => {
        d.parameters = [...(d.parameters ?? []), { name, expr }];
      });
    },

    /** Edita un parámetro; al renombrarlo se actualizan las fórmulas que lo usan */
    updateParameter(index: number, change: Partial<Parameter>) {
      return commit((d) => {
        const params = [...(d.parameters ?? [])];
        const old = params[index];
        if (!old) return;
        params[index] = { ...old, ...change };
        if (change.name && change.name !== old.name) {
          for (const p of params) p.expr = renameInExpr(p.expr, old.name, change.name);
          for (const k of Object.keys(d.bindings ?? {})) d.bindings![k] = renameInExpr(d.bindings![k], old.name, change.name);
        }
        d.parameters = params;
      });
    },

    removeParameter(index: number) {
      return commit((d) => {
        d.parameters = (d.parameters ?? []).filter((_, i) => i !== index);
      });
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
