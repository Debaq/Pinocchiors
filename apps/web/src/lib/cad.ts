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

function defaultName(doc: CadDocument, kind: FeatureKind): string {
  const label = FEATURE_LABELS[kind.type];
  const n = doc.features.filter((f) => FEATURE_LABELS[f.kind.type] === label).length + 1;
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
