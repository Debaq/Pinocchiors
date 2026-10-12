// Estado de interacción del espacio Diseñar, compartido entre el visor
// (CadView) y el panel (DesignStep): qué se está eligiendo con el mouse y el
// sketch en edición.

import { detachSelection, freeFromConstraints } from "./sketchFree";
import type { SketchFilter } from "./sketchPick";
import { batch, createEffect, createSignal, on } from "solid-js";
import { blockSketch, layerState, saveBlock } from "./sketchBlocks";
import { clipCenter, extractClip, insertClip, transformSelection, translation, type SketchClip, type Xform } from "./sketchTransform";
import type { BasePlane } from "./CadViewer";
import { updateDrawing, type DrawingElement } from "./drawingDoc";
import { polylineOf } from "./sketchCheck";
import type { Placed } from "./drawing";
import { nearestViewLine, projectViewLine, sheetOwners, type SheetLink } from "./sheetRefs";
import {
  geometryPoints,
  mirrorEntities,
  splitRests,
  constraintPath,
  constraintValue,
  isReference,
  plainNumber,
  addEntity,
  addPoint,
  addProjected,
  breakLinks,
  addRectangle,
  ensureOrigin,
  emptySketch,
  removeEntity,
  removeLoosePoint,
  removeLoosePoints,
  removeText,
  type AxisSpec,
  type RefPick,
  type CadStore,
  type DimOpts,
  type EdgeRef,
  type FaceRef,
  type P2,
  type P3,
  type Plane,
  type PlaneSpec,
  type Projected,
  type SketchUse,
  type Region,
  type ScanDeviation,
  type ScanPick,
  type Sketch,
  type SketchConstraint,
  type SolveReport,
  editsAsSketch,
  type PartId, type ScanOutline } from "./cad";

/** Operaciones que el panel de Diseñar ofrece al menú del visor (usan lo elegido) */
export type CadAction = "sketch" | "fillet" | "chamfer" | "shell" | "draft";

/** Vista de corte: plano base, posición (0..1 a lo largo del sólido) y lado que se ve */
export interface SectionView {
  plane: "xy" | "xz" | "yz";
  at: number;
  flip: boolean;
}

/** Qué se puede elegir en el visor (fuera de los sketches) */
export type PickFilter = "all" | "faces" | "edges" | "vertices" | "sketches";

/** Algo elegido en el visor fuera de la edición de sketches */
export type Pick3d =
  | { kind: "face"; face: number }
  | { kind: "edge"; edge: number }
  /** Vértice del sólido, por posición (mm) */
  | { kind: "vertex"; at: P3 }
  | { kind: "region"; sketch: number; region: number }
  /** Un sketch entero (clic en una de sus curvas) */
  | { kind: "sketch"; sketch: number }
  | { kind: "plane"; plane: BasePlane }
  /** Plano de referencia del historial */
  | { kind: "refplane"; feature: number };

const samePick = (a: Pick3d, b: Pick3d) => JSON.stringify(a) === JSON.stringify(b);

export type SketchTool =
  | "select"
  | "line"
  | "line_mid"
  | "centerline"
  | "rect"
  | "rect_center"
  | "rect3"
  | "circle"
  | "circle2"
  | "circle3"
  | "circle_tan"
  | "arc"
  | "arc3"
  | "ellipse"
  | "point"
  | "spline"
  | "bspline"
  | "conic"
  | "parabola"
  | "spline_point"
  | "line_inf"
  | "parallelogram"
  | "ellipse_arc"
  | "slot_arc3"
  | "text"
  | "polygon"
  | "slot"
  | "slot_center"
  | "slot_arc"
  | "trim"
  | "offset"
  | "tangent"
  | "extend"
  | "use"
  | "split"
  | "stretch"
  | "move"
  | "copy"
  | "rotate"
  | "scale"
  /** Pegar lo copiado (Ctrl+V): no está en la barra */
  | "paste"
  /** Insertar una copia del bloque elegido en el panel (clic donde va su origen) */
  | "block_insert"
  /** Calibrar un calco: dos clics sobre la imagen (después, la medida en el panel) */
  | "calibrate";

/** Qué hace un clic en el visor */
export type PickMode =
  | { kind: "none" }
  /** Elegir una cara del sólido: se avisa a `done` */
  | { kind: "face"; prompt: string; owner?: string; done: (face: FaceRef, index: number) => void }
  /** Marcar/desmarcar aristas: `toggle` con cada clic */
  | {
      kind: "edges";
      prompt: string;
      owner?: string;
      toggle: (edge: EdgeRef, index: number) => void;
      /**
       * Redondeo o chaflán en vivo: se elige sobre la vista previa de la
       * operación `feature`. Clic en lo que ella hizo quita la arista más
       * cercana a `near`; clic en una cara suma (o quita) todas sus aristas
       */
      live?: { feature: number; remove: (near: P3) => void; face: (edges: EdgeRef[]) => void };
    }
  /** Marcar/desmarcar caras: `toggle` con cada clic */
  | { kind: "faces"; prompt: string; owner?: string; toggle: (face: FaceRef, index: number) => void }
  /** Elegir una zona del escaneo */
  | { kind: "scan"; prompt: string; shape: "plane" | "cylinder"; done: (pick: ScanPick, triangle: number, at: P3) => void }
  /** Un punto sobre el escaneo (mm) y su triángulo */
  | { kind: "scan_point"; prompt: string; done: (at: P3, triangle: number) => void }
  /** Marcar/desmarcar regiones de un sketch con un clic dentro */
  | { kind: "region"; prompt: string; owner?: string; sketch: number; chosen: () => P2[]; toggle: (p: P2) => void }
  /** Un eje: una línea del sketch `sketch` o una arista del sólido */
  | { kind: "axis"; prompt: string; owner?: string; sketch: number; done: (axis: AxisSpec) => void }
  /** Dónde va un sketch nuevo: un plano base, una cara plana o el plano de otro sketch (con su corrimiento) */
  | { kind: "place"; prompt: string; done: (spec: PlaneSpec, offset?: number) => void }
  /** Lo que arma una referencia por clics: caras, aristas, vértices y planos (cada clic suma uno) */
  | { kind: "refs"; prompt: string; owner?: string; add: (pick: RefPick) => void }
  /** Piezas con un clic sobre ellas (otra vez la quita) */
  | { kind: "parts"; prompt: string; owner?: string; toggle: (part: PartId) => void }
  /** Dónde va un agujero: una cara plana y el punto del clic (mm) */
  | { kind: "hole"; prompt: string; done: (face: number, at: P3) => void }
  /** Una cara de la malla del ensamble (su índice) */
  | { kind: "asm_face"; prompt: string; done: (face: number) => void }
  /** Sketch 3D: clic en un punto o una curva del sketch, un vértice del sólido o el plano activo; `from` es donde arranca lo que se dibuja */
  | { kind: "sketch3d"; prompt: string; owner?: string; feature: number; plane: Plane; from?: P3; click: (hit: Hit3d, e: PointerEvent) => void };

/** Qué hay bajo un clic del sketch 3D: un punto suyo, una curva suya, un vértice del sólido o el plano activo */
export interface Hit3d {
  at: P3;
  point?: number;
  entity?: number;
  vertex?: boolean;
}

export interface SketchSession {
  feature: number;
  plane: Plane;
  sketch: Sketch;
  report?: SolveReport;
  regions: Region[];
  /** Dibujando sobre la hoja del plano 2D (no es una operación: `feature` es `SHEET_SKETCH`) */
  sheet?: SheetEdit;
}

/** La hoja en la que se dibuja: de qué plano, tamaño (mm) y la hoja de fondo como imagen (URL) */
export interface SheetEdit {
  drawing: number;
  width: number;
  height: number;
  backdrop?: string;
  /** Las vistas en la hoja (para anclar, recortar, extender y «Usar») y si se dibujan las ocultas */
  views: Placed[];
  hiddenLines: boolean;
  /** Entidades ligadas a líneas de las vistas (se cambia en el lugar al «Usar») */
  links: SheetLink[];
}

/** `feature` de la sesión que dibuja sobre la hoja del plano 2D */
export const SHEET_SKETCH = -1;

/** Herramienta del plano 2D: cotas a mano, detalle o la línea de un corte */
export type DrawingTool = "" | "dim" | "distance" | "angle" | "detail" | "section" | "break";

/** Lo que el plano abierto ofrece a la barra izquierda (lo registra DrawingView) */
export interface DrawingActions {
  sectionAA: () => void;
  canSectionAA: () => boolean;
  draw: () => void;
  newView: () => void;
  sheet: () => void;
  arrange: () => void;
  canArrange: () => boolean;
}
/** El plano del visor donde se dibuja la hoja (mm de hoja, Y hacia arriba) */
const SHEET_PLANE: Plane = { origin: [0, 0, 0], normal: [0, 0, 1], x_dir: [1, 0, 0] };

const clone = <T>(v: T): T => structuredClone(v);
/** Herramientas que no dibujan: el modo construcción no marca lo que crean */
const NOT_DRAWING = new Set<SketchTool>(["select", "trim", "offset", "extend", "use", "split", "stretch", "block_insert", "calibrate", "move", "copy", "rotate", "scale", "paste", "spline_point"]);

/** Qué se ve dentro del sketch */
export interface SketchShow {
  dims: boolean;
  constraints: boolean;
  construction: boolean;
  points: boolean;
  /** Flechas de lo que todavía se puede mover */
  freedom: boolean;
  /** Marcas donde dos curvas se cruzan sin un punto común */
  crossings?: boolean;
}
/** Ayudas al dibujar: inferencias y rejilla del sketch (se recuerdan entre sesiones) */
export interface SketchAssist {
  /** Anclajes e inferencias (Mayús los apaga para un clic) */
  infer: boolean;
  grid: boolean;
  /** Los clics libres caen en los nudos de la rejilla */
  gridSnap: boolean;
  /** Paso de la rejilla, en mm */
  spacing: number;
  /** Al dibujar: corte en el plano del sketch (se ve lo de atrás) */
  section: boolean;
  /** Al dibujar: el modelo oculto */
  hideModel: boolean;
}
const ASSIST_KEY = "pinocchio.sketchAssist";
function loadAssist(): SketchAssist {
  const base: SketchAssist = { infer: true, grid: false, gridSnap: false, spacing: 5, section: false, hideModel: false };
  try {
    const v = JSON.parse(localStorage.getItem(ASSIST_KEY) ?? "null");
    return v && typeof v === "object" ? { ...base, ...v } : base;
  } catch {
    return base;
  }
}

/** Si la entidad está entera sobre el eje (reflejarla daría la misma) */
function onAxis(s: Sketch, id: number, axis: number): boolean {
  const ax = s.entities.find((e) => e.id === axis)?.geometry;
  const e = s.entities.find((x) => x.id === id);
  if (ax?.type !== "line" || !e) return false;
  const at = (pid: number): P2 | undefined => {
    const q = s.points.find((x) => x.id === pid);
    return q && [q.x, q.y];
  };
  const [a, b] = [at(ax.start), at(ax.end)];
  if (!a || !b) return false;
  const len = Math.hypot(b[0] - a[0], b[1] - a[1]) || 1;
  return geometryPoints(e.geometry).every((pid) => {
    const p = at(pid);
    return !!p && Math.abs((b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])) / len <= 1e-9 * Math.max(1, len);
  });
}

/** Qué dice la etiqueta de una cota: el valor, su nombre (d1, d2… o el parámetro) o la fórmula con el valor */
export type DimLabel = "value" | "name" | "expr";

/** Pasos que se pueden deshacer dentro de un sketch */
const SKETCH_HISTORY_LIMIT = 200;

/** Curvas de `next` que no están en `prev` o cambiaron (geometría o lugar de sus puntos), como polilíneas */
function changedCurves(prev: Sketch, next: Sketch): P2[][] {
  const pos = new Map(prev.points.map((p) => [p.id, `${p.x.toFixed(6)},${p.y.toFixed(6)}`]));
  const old = new Map(prev.entities.map((e) => [e.id, JSON.stringify(e.geometry)]));
  const moved = (id: number) => {
    const q = next.points.find((p) => p.id === id);
    return !q || pos.get(id) !== `${q.x.toFixed(6)},${q.y.toFixed(6)}`;
  };
  return next.entities
    .filter((e) => e.geometry.type !== "point" && (old.get(e.id) !== JSON.stringify(e.geometry) || geometryPoints(e.geometry).some(moved)))
    .map((e) => polylineOf(next, e.geometry))
    .filter((pts) => pts.length >= 2);
}

export function createCadUi(store: CadStore) {
  const [pick, setPick] = createSignal<PickMode>({ kind: "none" });
  const [highlight, setHighlight] = createSignal<{ faces: number[]; edges: number[] }>({ faces: [], edges: [] });
  const [scanHighlight, setScanHighlight] = createSignal<number[]>([]);
  /** Corte del escaneo por el plano del sketch en edición (guía, anclajes y «Usar el corte») */
  const [scanOutline, setScanOutline] = createSignal<ScanOutline>();
  const [session, setSession] = createSignal<SketchSession>();
  const [tool, setTool] = createSignal<SketchTool>("line");
  const [selection, setSelection] = createSignal<number[]>([]);
  // Restricciones elegidas (por índice): con un ícono o desde el panel; Supr las borra
  const [selectedConstraints, setSelectedConstraints] = createSignal<number[]>([]);
  // Lo que nombra la restricción bajo el mouse en el panel (resaltado en el visor)
  const [hoverIds, setHoverIds] = createSignal<number[]>([]);
  /** Operación bajo el mouse en el árbol (se resalta en el visor) */
  const [hoverFeature, setHoverFeature] = createSignal<number>();
  // Selección en el visor (caras, aristas, regiones, planos), como en Onshape
  const [picks, setPicks] = createSignal<Pick3d[]>([]);
  const [hiddenSketches, setHiddenSketches] = createSignal<number[]>([]);
  const [actions, setActions] = createSignal<Partial<Record<CadAction, () => void>>>({});
  const [pickFilter, setPickFilter] = createSignal<PickFilter>("all");
  const [section, setSection] = createSignal<SectionView | null>(null);
  // Desviación del escaneo mostrada en colores (null = color de siempre)
  const [deviation, setDeviation] = createSignal<ScanDeviation | null>(null);
  // Ensamble: el visor muestra las instancias en vez del diseño
  const [assemblyMode, setAssemblyMode] = createSignal(false);
  // Plano 2D abierto sobre el visor: cuál y lo elegido en él (con su ventana de propiedades)
  const [drawingOpen, setDrawingOpen] = createSignal(false);
  const [activeDrawing, setActiveDrawing] = createSignal<number>();
  // El visor de Diseñar (su cámara y su escala), para el plano: «tomar la vista del visor» y la vista previa al girar
  let viewerInfo: { cameraPose: () => { position: P3; target: P3; up: P3 }; mmPerUnit: () => number; focusSketch?: (p: P2, size: number) => void } | undefined;
  const [drawingSel, setDrawingSel] = createSignal<DrawingElement>();
  // Herramienta del plano (la barra izquierda y la hoja la comparten) y lo que el plano ofrece a la barra
  const [drawingTool, setDrawingTool] = createSignal<DrawingTool>("");
  const [drawingActions, setDrawingActions] = createSignal<DrawingActions>();
  // Desarrollo de chapa abierto sobre el visor
  const [flatOpen, setFlatOpen] = createSignal(false);
  // Centro de masa dibujado en el visor
  const [showCenterOfMass, setShowCenterOfMass] = createSignal(false);
  const [showPlanes, setShowPlanes] = createSignal(true);
  // Planos base ocultos uno por uno (desde el Outliner)
  const [hiddenPlanes, setHiddenPlanes] = createSignal<BasePlane[]>([]);
  // Pieza abierta en la lista de piezas ("<operación>:<n>"), también desde el Outliner
  const [openPart, setOpenPart] = createSignal<string>();
  // Recalcular cambia los índices de caras, aristas y regiones: la selección vieja ya no vale
  createEffect(on(() => store.result()?.version, () => setPicks((p) => p.filter((x) => x.kind === "plane")), { defer: true }));
  const [message, setMessage] = createSignal<string>();
  // Modo construcción (como Onshape): lo que se dibuja sale de construcción
  const [constructionMode, setConstructionMode] = createSignal(false);
  // Qué se puede elegir en el visor (también dentro de recortar, extender…)
  const [sketchFilter, setSketchFilter] = createSignal<SketchFilter>("all");
  // Mover, girar y escalar sueltan lo elegido de lo que no se mueve (en vez de estirarlo)
  const [detachOnMove, setDetachOnMove] = createSignal(false);
  const [sketchShow, setSketchShow] = createSignal<SketchShow>({ dims: true, constraints: true, construction: true, points: true, freedom: true, crossings: true });
  const [dimLabel, setDimLabel] = createSignal<DimLabel>("expr");
  const [assist, setAssistRaw] = createSignal<SketchAssist>(loadAssist());
  const setAssist = (patch: Partial<SketchAssist>) => {
    const next = { ...assist(), ...patch };
    setAssistRaw(next);
    try {
      localStorage.setItem(ASSIST_KEY, JSON.stringify(next));
    } catch {
      // Sin almacenamiento: vale para esta sesión
    }
  };
  // Simetría dinámica: lo que se dibuja sale reflejado en esta línea
  const [symmetryAxis, setSymmetryAxis] = createSignal<number>();
  // Capa donde caen las entidades nuevas
  const [activeLayer, setActiveLayer] = createSignal<number>();
  // Editando un bloque: el sketch de la sesión es el del bloque; `parent` es el sketch de afuera
  const [blockEdit, setBlockEdit] = createSignal<{ block: number; name: string; parent: Sketch }>();
  // Bloque a insertar con la herramienta block_insert
  const [blockToInsert, setBlockToInsert] = createSignal<{ block: number; angle: number; scale: number }>();
  // Calco a calibrar (con la herramienta calibrate) y los dos puntos marcados sobre él
  const [calibration, setCalibration] = createSignal<{ image: number; a?: P2; b?: P2 }>();
  // Sketch entero copiado (para pegarlo como un sketch nuevo en otro plano)
  const [sketchCopy, setSketchCopy] = createSignal<Sketch>();
  // Sketch 3D: lo elegido (puntos y entidades) y dónde está el cursor
  const [sel3d, setSel3d] = createSignal<number[]>([]);
  const [cursor3d, setCursor3d] = createSignal<P3>();
  /** Spline con el peine de curvatura a la vista */
  const [combEntity, setCombEntity] = createSignal<number>();
  // Lo copiado con Ctrl+C: sobrevive a cerrar el sketch (para pegar en otro)
  const [clipboard, setClipboard] = createSignal<SketchClip>();
  // Vista previa de un botón del panel: su acción se prueba sobre una copia (modo de prueba)
  let dry = false;
  let dryResult: P2[][] | undefined;
  const [panelPreview, setPanelPreview] = createSignal<P2[][]>();
  // Resolver de a uno: mientras se arrastra no se encolan pedidos
  let solving = false;
  let pendingDrag: [number, P2] | null = null;
  let pendingMove: Sketch | null = null;
  // Deshacer dentro del sketch: el sketch antes de cada cambio (un arrastre
  // entero es un solo paso)
  let past: Sketch[] = [];
  let future: Sketch[] = [];
  let dragSaved = false;
  // Sube con cada cambio y cada deshacer: una respuesta del solver pedida
  // antes ya no vale (pisaría lo repuesto)
  let generation = 0;
  const [sketchHistory, setSketchHistory] = createSignal({ undo: 0, redo: 0 });
  // Sube al reponer un paso (aparte: la herramienta en curso se corta solo entonces)
  const [sketchRestored, setSketchRestored] = createSignal(0);
  const historyChanged = () => setSketchHistory({ undo: past.length, redo: future.length });
  const resetHistory = () => {
    setSelectedConstraints([]);
    past = [];
    future = [];
    dragSaved = false;
    historyChanged();
  };
  const remember = (before: Sketch) => {
    past.push(before);
    if (past.length > SKETCH_HISTORY_LIMIT) past.shift();
    future = [];
    historyChanged();
  };
  /** Repone un sketch guardado y lo vuelve a resolver */
  const restore = (s: SketchSession, sketch: Sketch) => {
    generation++;
    dragSaved = false;
    pendingDrag = null;
    const ids = new Set([...sketch.points.map((p) => p.id), ...sketch.entities.map((e) => e.id)]);
    batch(() => {
      setSession({ ...s, sketch });
      setSelection((sel) => sel.filter((id) => ids.has(id)));
      setSelectedConstraints([]);
      historyChanged();
      setSketchRestored((n) => n + 1);
    });
    void solve(clone(sketch));
  };

  const solve = async (sketch: Sketch, drag?: [number, P2]) => {
    const s = session();
    if (!s) return;
    const asked = generation;
    try {
      const r = await store.solveSketch(sketch, drag);
      if (asked !== generation) return;
      // El solver no conoce las fórmulas: vuelven a su cota (mismo orden)
      sketch.constraints.forEach((c, i) => {
        const expr = (c as { expr?: string }).expr;
        if (expr && r.sketch.constraints[i]) (r.sketch.constraints[i] as { expr?: string }).expr = expr;
      });
      const now = session();
      if (now && now.feature === s.feature) setSession({ ...now, sketch: r.sketch, report: r.report, regions: r.regions });
    } catch (e) {
      setMessage(String(e));
    }
  };

  const ui = {
    pick,
    setPick,
    cancelPick: () => {
      setPick({ kind: "none" });
      setHighlight({ faces: [], edges: [] });
      setScanHighlight([]);
    },
    highlight,
    setHighlight,
    scanHighlight,
    setScanHighlight,
    scanOutline,
    setScanOutline,
    message,
    // En el modo de prueba (vista previa) no se avisa ni se cambia lo elegido
    setMessage: ((m?: string) => void (dry || setMessage(m))) as typeof setMessage,
    /** ¿Corre una vista previa (modo de prueba)? Lo que no pasa por `change` no debe cambiar */
    previewing: () => dry,

    // ─── Sketch ───────────────────────────────────────────────────────────
    session,
    tool,
    setTool,
    selection,
    setSelection: ((v: number[] | ((prev: number[]) => number[])) => void (dry || setSelection(v))) as typeof setSelection,
    hoverIds,
    hoverFeature,
    setHoverFeature,
    setHoverIds,

    // ─── Selección en el visor ────────────────────────────────────────────
    picks,
    setPicks,
    /** Agrega o quita (con `additive`) o reemplaza la selección */
    pickToggle(p: Pick3d, additive: boolean) {
      setPicks((cur) => {
        const has = cur.some((x) => samePick(x, p));
        if (additive) return has ? cur.filter((x) => !samePick(x, p)) : [...cur, p];
        return has && cur.length === 1 ? [] : [p];
      });
    },
    clearPicks: () => setPicks([]),
    hiddenSketches,
    sel3d,
    setSel3d,
    cursor3d,
    setCursor3d,
    showCenterOfMass,
    setShowCenterOfMass,
    pickFilter,
    setPickFilter,
    /** Vista de corte (solo visual; no cambia el modelo) */
    section,
    setSection,
    drawingOpen,
    setDrawingOpen,
    activeDrawing,
    setActiveDrawing,
    /** Lo registra CadView al crear el visor */
    registerViewer(info: typeof viewerInfo) {
      viewerInfo = info;
    },
    viewer: () => viewerInfo,
    drawingSel,
    setDrawingSel,
    drawingTool,
    setDrawingTool,
    drawingActions,
    setDrawingActions,
    /** Abre el plano `id` con `element` elegido (su ventana de propiedades) */
    openDrawing(id: number, element?: DrawingElement) {
      batch(() => {
        setActiveDrawing(id);
        setDrawingSel(element);
        setDrawingOpen(true);
      });
    },
    flatOpen,
    setFlatOpen,
    assemblyMode,
    setAssemblyMode,
    deviation,
    setDeviation,
    /** Acciones del panel con lo elegido (las registra DesignStep) */
    actions,
    setActions,
    toggleSketchVisible(id: number) {
      setHiddenSketches((h) => (h.includes(id) ? h.filter((x) => x !== id) : [...h, id]));
    },
    showPlanes,
    setShowPlanes,
    hiddenPlanes,
    openPart,
    setOpenPart,
    /** Muestra u oculta un plano base; con todos ocultos se apaga el botón de planos */
    togglePlane(plane: BasePlane) {
      const all: BasePlane[] = ["xy", "xz", "yz"];
      batch(() => {
        if (!showPlanes()) {
          setShowPlanes(true);
          setHiddenPlanes(all.filter((p) => p !== plane));
          return;
        }
        const next = hiddenPlanes().includes(plane) ? hiddenPlanes().filter((p) => p !== plane) : [...hiddenPlanes(), plane];
        if (next.length === all.length) {
          setShowPlanes(false);
          setHiddenPlanes([]);
        } else setHiddenPlanes(next);
      });
    },

    /** Empieza a editar el sketch de la operación `feature` */
    editSketch(feature: number): boolean {
      const f = store.doc()?.features.find((x) => x.id === feature);
      const view = store.sketchView(feature);
      if (!f || !editsAsSketch(f.kind) || !view) {
        setMessage("El sketch todavía no tiene plano (recalcular primero)");
        return false;
      }
      ui.cancelPick();
      setSelection([]);
      // Uno nuevo se empieza a dibujar; uno con dibujo se abre para elegir y editar
      setTool(view.sketch.entities.length ? "select" : "line");
      // El sketch ya resuelto (con las fórmulas aplicadas); cada cota vinculada
      // lleva su fórmula mientras se edita
      const sketch = clone(view.sketch);
      ensureOrigin(sketch);
      sketch.constraints.forEach((c, i) => {
        const expr = store.bindingOf(constraintPath(feature, i, c));
        if (expr) (c as { expr?: string }).expr = expr;
      });
      setSession({ feature, plane: view.plane, sketch, report: view.report, regions: view.regions });
      setSymmetryAxis(undefined);
      setActiveLayer(undefined);
      setBlockEdit(undefined);
      setCalibration(undefined);
      resetHistory();
      return true;
    },

    /**
     * Empieza a dibujar sobre la hoja del plano 2D: el sketch guardado en el
     * documento (o uno nuevo) con la hoja de fondo
     */
    editSheet(saved: Sketch | null | undefined, sheet: SheetEdit) {
      ui.cancelPick();
      setSelection([]);
      setTool(saved?.entities.length ? "select" : "line");
      const sketch = saved ? clone(saved) : emptySketch();
      ensureOrigin(sketch);
      setSession({ feature: SHEET_SKETCH, plane: SHEET_PLANE, sketch, regions: [], sheet });
      setSymmetryAxis(undefined);
      setActiveLayer(undefined);
      setBlockEdit(undefined);
      setCalibration(undefined);
      resetHistory();
      void solve(clone(sketch));
    },

    /** Lo que mostraría un botón del panel: sus curvas nuevas o cambiadas (se dibujan como vista previa) */
    panelPreview,
    /** Corre `run` en modo de prueba: los cambios van a una copia y quedan como vista previa */
    previewOf(run: () => void) {
      if (!session()) return;
      dry = true;
      dryResult = undefined;
      try {
        run();
      } catch {
        dryResult = undefined;
      } finally {
        dry = false;
      }
      // (lo deja `change`, dentro de `run`)
      const found = dryResult as P2[][] | undefined;
      setPanelPreview(found?.length ? found : undefined);
    },
    clearPreview: () => setPanelPreview(undefined),

    /** Cambia el sketch en edición y lo resuelve */
    change(mutate: (s: Sketch) => void) {
      const s = session();
      if (!s) return;
      if (dry) {
        // Modo de prueba: en una copia, y lo nuevo o lo que se movió queda para dibujar
        const next = clone(s.sketch);
        mutate(next);
        dryResult = changedCurves(s.sketch, next);
        return;
      }
      setPanelPreview(undefined);
      const next = clone(s.sketch);
      // Junto con lo que la herramienta actualice dentro de `mutate`: la vista
      // previa nunca ve el sketch viejo con el estado nuevo de la herramienta
      batch(() => {
        const before = new Set(s.sketch.entities.map((e) => e.id));
        mutate(next);
        if (constructionMode() && !NOT_DRAWING.has(tool())) for (const e of next.entities) if (!before.has(e.id)) e.construction = true;
        // Capa activa: lo nuevo cae en ella
        const layer = activeLayer();
        if (layer !== undefined && next.layers?.some((l) => l.id === layer)) for (const e of next.entities) if (!before.has(e.id)) e.layer = layer;
        // Simetría dinámica: lo dibujado (no los tramos de líneas partidas ni lo que cae sobre el eje) se refleja atado
        const axis = symmetryAxis();
        if (axis !== undefined && !NOT_DRAWING.has(tool()) && next.entities.some((e) => e.id === axis)) {
          const rests = splitRests.get(next);
          const drawn = next.entities.filter((e) => !before.has(e.id) && !rests?.has(e.id) && !onAxis(next, e.id, axis)).map((e) => e.id);
          if (drawn.length) mirrorEntities(next, drawn, axis);
        }
        // Lo que no cambia nada (una herramienta que avisa un error) no es un paso
        if (JSON.stringify(next) !== JSON.stringify(s.sketch)) remember(s.sketch);
        // Los índices cambian si se agregan o quitan restricciones
        if (next.constraints.length !== s.sketch.constraints.length) setSelectedConstraints([]);
        setSession({ ...s, sketch: next });
      });
      generation++;
      void solve(next);
    },

    /** Pasos para deshacer y rehacer dentro del sketch */
    sketchHistory,
    /** Sube cada vez que deshacer o rehacer repone un sketch */
    sketchRestored,
    undoSketch() {
      const s = session();
      const prev = past.pop();
      if (!s || !prev) return;
      future.push(s.sketch);
      restore(s, prev);
    },
    redoSketch() {
      const s = session();
      const next = future.pop();
      if (!s || !next) return;
      past.push(s.sketch);
      restore(s, next);
    },
    clipboard,
    /** Copia lo elegido al portapapeles; devuelve el aviso */
    copySelection(): string {
      const s = session();
      const clip = s && extractClip(s.sketch, selection());
      if (!clip) return "Elegir primero lo que se copia";
      setClipboard(clip);
      return `Copiado (${clip.entities.length} ${clip.entities.length === 1 ? "entidad" : "entidades"}): Ctrl+V para pegar`;
    },
    /** Pega lo copiado con su centro en `at`; lo pegado queda elegido */
    paste(at: P2) {
      const clip = clipboard();
      if (!clip || !session()) return;
      const c = clipCenter(clip);
      let made: number[] = [];
      ui.change((sk) => (made = insertClip(sk, clip, translation([at[0] - c[0], at[1] - c[1]]))));
      setSelection(made);
    },
    sketchFilter,
    setSketchFilter,
    detachOnMove,
    setDetachOnMove,
    /**
     * Mueve, gira o escala lo elegido (`scale`: el factor, para radios y
     * textos); con `copy` deja lo elegido y pone una copia llevada por `f`
     * (solo traslaciones). Devuelve el aviso si no se pudo.
     */
    transformSelected(f: Xform, opts: { scale?: number; copy?: boolean; times?: number } = {}): string | undefined {
      const sel = selection();
      let msg: string | undefined;
      let made: number[] | undefined;
      let freed: { points: number; constraints: number } | undefined;
      ui.change((sk) => {
        // Soltar: lo elegido se lleva copias de los puntos compartidos y deja sus relaciones con lo demás
        if (!opts.copy && detachOnMove()) freed = detachSelection(sk, sel);
        if (opts.copy) {
          const clip = extractClip(sk, sel);
          if (!clip) msg = "Elegir primero lo que se copia";
          else {
            // Varias copias: cada una con la transformación una vez más (f, f∘f, …)
            made = [];
            let g: Xform = f;
            for (let k = 0; k < Math.max(1, opts.times ?? 1); k++) {
              made.push(...insertClip(sk, clip, g));
              const prev = g;
              g = (p) => f(prev(p));
            }
          }
        } else msg = transformSelection(sk, sel, f, opts.scale ?? 1);
      });
      if (made && !dry) setSelection(made);
      if (!msg && freed && freed.points + freed.constraints) msg = `Se soltó de lo demás: ${freed.points} uniones y ${freed.constraints} restricciones`;
      return msg;
    },
    /** Arrastre libre: quita lo que sujeta el punto o la entidad (un paso de deshacer) */
    freeDrag(id: number): number {
      let n = 0;
      ui.change((sk) => void (n = freeFromConstraints(sk, id)));
      return n;
    },
    /** Lleva un punto a `to` respetando las restricciones (un paso de deshacer) */
    movePoint(point: number, to: P2) {
      void ui.drag(point, to);
      ui.endDrag();
    },
    /** Cambia el radio de un círculo (si una cota lo fija, gana la cota) */
    setCircleRadius(entity: number, r: number) {
      if (!(r > 0)) return;
      ui.change((sk) => {
        const g = sk.entities.find((e) => e.id === entity)?.geometry;
        if (g?.type === "circle") g.radius = r;
      });
    },
    /**
     * Arrastre de una curva entera: una línea (o arco, spline…) se corre `delta`
     * desde donde estaba al empezar (`from`); un círculo cambia el radio hasta
     * `at`. El solver acomoda lo que está atado (todo el arrastre es un paso).
     */
    async dragEntity(from: Sketch, id: number, delta: P2, at: P2) {
      const s = session();
      if (!s) return;
      if (!dragSaved) {
        dragSaved = true;
        remember(s.sketch);
      }
      const next = clone(from);
      const e = next.entities.find((x) => x.id === id);
      if (!e) return;
      if (e.geometry.type === "circle") {
        const c = next.points.find((p) => p.id === (e.geometry as { center: number }).center)!;
        e.geometry.radius = Math.max(1e-6, Math.hypot(at[0] - c.x, at[1] - c.y));
      } else
        for (const pid of geometryPoints(e.geometry)) {
          const p = next.points.find((q) => q.id === pid);
          if (p) [p.x, p.y] = [p.x + delta[0], p.y + delta[1]];
        }
      pendingMove = next;
      if (solving) return;
      solving = true;
      try {
        while (pendingMove) {
          const sk = pendingMove;
          pendingMove = null;
          const now = session();
          if (!now) break;
          generation++;
          setSession({ ...now, sketch: sk });
          await solve(sk);
        }
      } finally {
        solving = false;
      }
    },
    /** Termina un arrastre: el próximo empieza un paso nuevo */
    endDrag() {
      dragSaved = false;
    },

    /** Arrastre de un punto (se resuelve el último pedido cuando termina el anterior) */
    async drag(point: number, target: P2) {
      const s = session();
      if (s && !dragSaved) {
        dragSaved = true;
        remember(s.sketch);
      }
      pendingDrag = [point, target];
      if (solving) return;
      solving = true;
      try {
        while (pendingDrag) {
          const d = pendingDrag;
          pendingDrag = null;
          const s = session();
          if (!s) break;
          await solve(s.sketch, d);
        }
      } finally {
        solving = false;
      }
    },

    addConstraint(c: SketchConstraint) {
      ui.change((s) => s.constraints.push(c));
    },

    /** Pasa una cota a referencia (solo mide) o de vuelta a restringir */
    toggleReference(index: number) {
      ui.change((s) => {
        const c = s.constraints[index];
        if (!c || !("reference" in c || constraintValue(c) !== undefined)) return;
        const k = c as { reference?: boolean };
        if (k.reference) delete k.reference;
        else k.reference = true;
      });
    },

    /**
     * Con el sketch sobre-definido: la cota en conflicto más nueva (la que se
     * puede dejar de referencia), o nada.
     */
    extraDimension(): number | undefined {
      const s = session();
      if (s?.report?.status !== "over_constrained") return undefined;
      const dims = s.report.conflicting.filter((i) => {
        const c = s.sketch.constraints[i];
        return c && constraintValue(c) !== undefined && !isReference(c);
      });
      return dims.length ? Math.max(...dims) : undefined;
    },

    removeConstraint(index: number) {
      ui.change((s) => s.constraints.splice(index, 1));
    },

    setConstraintValue(index: number, value: number, expr?: string) {
      ui.change((s) => {
        const c = s.constraints[index];
        if (!c) return;
        if ("value" in c) c.value = value;
        // El sentido queda (la flecha lo da vuelta); uno negativo lo invierte
        else if (c.type === "angle") c.degrees = (c.degrees < 0 ? -1 : 1) * value;
        if (expr) (c as { expr?: string }).expr = expr;
        else delete (c as { expr?: string }).expr;
      });
    },

    /** Cota escrita como número o fórmula; devuelve el error si no se calcula */
    async setConstraintText(index: number, text: string): Promise<string | undefined> {
      const n = plainNumber(text);
      if (n !== undefined) {
        ui.setConstraintValue(index, n);
        return undefined;
      }
      try {
        ui.setConstraintValue(index, await store.evalExpr(text), text.trim());
        return undefined;
      } catch (e) {
        return String(e);
      }
    },

    selectedConstraints,
    setSelectedConstraints,
    /** Quita varias restricciones (por índice) en un paso */
    removeConstraints(indices: number[]) {
      const drop = new Set(indices);
      ui.change((s) => (s.constraints = s.constraints.filter((_, i) => !drop.has(i))));
      setSelectedConstraints([]);
    },

    /** Supr: las restricciones elegidas si hay; si no, las entidades */
    deleteSelection() {
      if (selectedConstraints().length) return ui.removeConstraints(selectedConstraints());
      const ids = new Set(selection());
      ui.change((s) => {
        // Lo de capas bloqueadas no se toca
        for (const id of layerState(s).locked) ids.delete(id);
        // Un texto se borra entero (alcanza con elegir una de sus curvas o su ancla)
        for (const t of [...(s.texts ?? [])]) if ([t.anchor, ...t.entities].some((x) => ids.has(x))) removeText(s, t);
        // Un punto suelto se elige por su punto
        for (const e of [...s.entities]) if (ids.has(e.id) || (e.geometry.type === "point" && ids.has(e.geometry.point))) removeEntity(s, e.id);
        // Puntos sin entidad (de una línea empezada y cancelada)
        for (const id of ids) removeLoosePoint(s, id);
      });
      setSelection([]);
    },

    constructionMode,
    setConstructionMode,
    activeLayer,
    setActiveLayer,
    blockEdit,
    blockToInsert,
    setBlockToInsert,
    calibration,
    setCalibration,
    sketchCopy,
    setSketchCopy,
    /** Abre un bloque para editarlo (el sketch de afuera queda guardado hasta terminar) */
    editBlock(id: number) {
      const s = session();
      const b = s?.sketch.blocks?.find((x) => x.id === id);
      if (!s || !b || blockEdit()) return;
      setBlockEdit({ block: id, name: b.name, parent: clone(s.sketch) });
      setSelection([]);
      setSession({ ...s, sketch: blockSketch(b), report: undefined, regions: [] });
      resetHistory();
      ui.change(() => {});
    },
    /** Guarda el bloque editado (todas sus copias se rehacen) y vuelve al sketch */
    finishBlock(): string | undefined {
      const be = blockEdit();
      const s = session();
      if (!be || !s) return;
      const edited = clone(s.sketch);
      setBlockEdit(undefined);
      setSelection([]);
      setSession({ ...s, sketch: be.parent });
      resetHistory();
      let msg: string | undefined;
      ui.change((sk) => (msg = saveBlock(sk, be.block, edited)));
      return msg;
    },
    /** Sale del bloque sin guardar */
    cancelBlock() {
      const be = blockEdit();
      const s = session();
      if (!be || !s) return;
      setBlockEdit(undefined);
      setSelection([]);
      setSession({ ...s, sketch: be.parent });
      resetHistory();
      ui.change(() => {});
    },
    assist,
    setAssist,
    symmetryAxis,
    setSymmetryAxis,
    sketchShow,
    dimLabel,
    setDimLabel,
    combEntity,
    setCombEntity,
    /** Cambia las opciones de una cota (bloqueo, lugar del texto, ordenadas) */
    setDimOpts(index: number, patch: Partial<DimOpts>) {
      ui.change((s) => {
        const c = s.constraints[index];
        if (!c || !("value" in c || c.type === "angle")) return;
        const opts: DimOpts = { ...(c.opts ?? {}), ...patch };
        for (const k of Object.keys(opts) as (keyof DimOpts)[]) if (opts[k] === undefined || opts[k] === false) delete opts[k];
        if (Object.keys(opts).length) c.opts = opts;
        else delete c.opts;
      });
    },
    /** Muestra u oculta cotas, construcción o puntos */
    toggleShow(k: keyof SketchShow) {
      setSketchShow((v) => ({ ...v, [k]: !v[k] }));
    },
    /**
     * Botón Construcción (Q): con algo elegido lo pasa a construcción o de
     * vuelta; sin nada elegido prende o apaga el modo construcción
     */
    construction() {
      const s = session();
      const ids = new Set(selection());
      if (s?.sketch.entities.some((e) => ids.has(e.id) || (e.geometry.type === "point" && ids.has(e.geometry.point)))) ui.toggleConstruction();
      else setConstructionMode(!constructionMode());
    },

    /** Lo elegido a construcción y de vuelta: si hay de las dos, todas pasan a construcción (los puntos sueltos también) */
    toggleConstruction() {
      const ids = new Set(selection());
      ui.change((s) => {
        const picked = s.entities.filter((e) => ids.has(e.id) || (e.geometry.type === "point" && ids.has(e.geometry.point)));
        const to = !picked.every((e) => e.construction);
        for (const e of picked) e.construction = to;
      });
    },

    /**
     * Trae curvas del modelo al sketch, ligadas: la intersección del plano con
     * el sólido o su silueta (con el sólido de antes del sketch). Devuelve el aviso.
     */
    async useModel(what: "section" | "silhouette"): Promise<string> {
      const s = session();
      const d = store.doc();
      const index = d?.features.findIndex((f) => f.id === s?.feature) ?? -1;
      if (!s || !d || index < 0) return "No hay sketch abierto";
      try {
        const curves = await store.projectModel(d, index, s.plane, what);
        return ui.addLinked(curves, () => ({ source: { type: what } }), what === "section" ? "de la intersección" : "de la silueta");
      } catch (e) {
        return String(e);
      }
    },

    /** Trae todas las entidades de un sketch anterior, ligadas a las suyas */
    async useSketch(feature: number): Promise<string> {
      const s = session();
      if (!s) return "No hay sketch abierto";
      try {
        const curves = await store.projectSketch(feature, s.plane);
        if (curves.length === 0) return "Ese sketch está vacío";
        return ui.addLinked(curves, (i) => ({ source: { type: "sketch", feature, entity: curves[i].entity } }), "del otro sketch");
      } catch (e) {
        return String(e);
      }
    },

    /** Agrega curvas ligadas al sketch (si la sesión sigue abierta) y las deja elegidas */
    addLinked(curves: Projected[], link: (i: number) => Omit<SketchUse, "entity">, what: string): string {
      if (!session()) return "El sketch se cerró";
      let ids: number[] = [];
      ui.change((sk) => (ids = addProjected(sk, curves, link)));
      setSelection(ids);
      const n = ids.length;
      return `${n === 1 ? "1 curva" : `${n} curvas`} ${what}, ligada${n === 1 ? "" : "s"} al modelo (si cambia, se mueve${n === 1 ? "" : "n"} con él)`;
    },

    /**
     * Dibujando sobre la hoja: la línea de vista más cercana a `q` (a menos de
     * `tol` mm de hoja) entra al dibujo ligada a su vista (sigue al modelo)
     */
    useViewLine(q: P2, tol: number): string {
      const s = session();
      const sheet = s?.sheet;
      if (!sheet) return "No hay hoja abierta";
      const hit = nearestViewLine(sheet.views, sheet.height, q, tol, sheet.hiddenLines);
      if (!hit) return "Clic sobre una línea de una vista";
      let ids: number[] = [];
      ui.change((sk) => (ids = addProjected(sk, [projectViewLine(hit.view, hit.line, sheet.height)], () => ({}))));
      for (const entity of ids) sheet.links.push({ entity, view: hit.view.name, at: hit.at });
      setSelection(ids);
      return "Línea de la vista ligada: si cambia el modelo, se mueve con él";
    },

    /** Lo elegido deja de seguir al modelo: queda donde está, editable */
    breakLinks(): string {
      const s = session();
      const ids = selection().filter((id) => s?.sketch.uses?.some((u) => u.entity === id));
      if (ids.length === 0) return "Nada de lo elegido está ligado al modelo";
      ui.change((sk) => breakLinks(sk, ids));
      return ids.length === 1 ? "Ya no sigue al modelo" : `${ids.length} curvas ya no siguen al modelo`;
    },

    /** Guarda el sketch en el documento y sale */
    async finishSketch() {
      // Con un bloque abierto, primero se guarda el bloque
      if (blockEdit()) ui.finishBlock();
      const s = session();
      if (!s) return;
      setSession(undefined);
      setSelection([]);
      resetHistory();
      // Las fórmulas de las cotas pasan a ser vínculos del documento (por
      // índice: se rehacen todos los de este sketch)
      const sketch = clone(s.sketch);
      // Sin puntos sueltos (primeros clics de líneas que no se terminaron)
      removeLoosePoints(sketch);
      const exprs = sketch.constraints.map((c) => {
        const e = (c as { expr?: string }).expr;
        delete (c as { expr?: string }).expr;
        return e;
      });
      // La hoja: el sketch va al plano del documento (las fórmulas quedan en sus cotas) y el plano se vuelve a abrir
      if (s.sheet) {
        const id = s.sheet.drawing;
        // Los vínculos de lo que sigue ligado (lo borrado o con el vínculo roto se olvida)
        const used = new Set((s.sketch.uses ?? []).map((u) => u.entity));
        const links = s.sheet.links.filter((l) => used.has(l.entity) && s.sketch.entities.some((e) => e.id === l.entity));
        // Lo que quedó encima de una vista se mueve con ella
        const owners = sheetOwners(s.sketch, s.sheet.views, s.sheet.height);
        await store.commit(
          (d) =>
            void updateDrawing(d, id, (x) => {
              x.sketch = clone(s.sketch);
              x.links = links;
              x.owners = owners;
            }),
        );
        setDrawingOpen(true);
        return;
      }
      const prefix = `${s.feature}.kind.sketch.constraints.`;
      await store.commit((d) => {
        const f = d.features.find((x) => x.id === s.feature);
        if (f && editsAsSketch(f.kind)) f.kind.sketch = sketch;
        const bindings = { ...(d.bindings ?? {}) };
        for (const k of Object.keys(bindings)) if (k.startsWith(prefix)) delete bindings[k];
        sketch.constraints.forEach((c, i) => {
          if (exprs[i]) bindings[constraintPath(s.feature, i, c)] = exprs[i]!;
        });
        d.bindings = bindings;
      });
    },

    /** Sale sin guardar; dibujando sobre la hoja, vuelve al plano (salvo `toDrawing` false: abrir otro proyecto) */
    cancelSketch(toDrawing = true) {
      if (toDrawing && session()?.sheet) setDrawingOpen(true);
      setBlockEdit(undefined);
      setSession(undefined);
      setSelection([]);
      resetHistory();
    },

    // Ayudas de dibujo usadas por el visor
    addPoint: (s: Sketch, p: P2) => addPoint(s, p),
    addEntity,
    addRectangle,
  };
  return ui;
}

export type CadUi = ReturnType<typeof createCadUi>;
