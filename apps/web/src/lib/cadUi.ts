// Estado de interacción del espacio Diseñar, compartido entre el visor
// (CadView) y el panel (DesignStep): qué se está eligiendo con el mouse y el
// sketch en edición.

import { batch, createEffect, createSignal, on } from "solid-js";
import { clipCenter, extractClip, insertClip, transformSelection, translation, type SketchClip, type Xform } from "./sketchTransform";
import type { BasePlane } from "./CadViewer";
import {
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
  removeEntity,
  removeText,
  type AxisSpec,
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
} from "./cad";

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
  | { kind: "plane"; plane: BasePlane }
  /** Plano de referencia del historial */
  | { kind: "refplane"; feature: number };

const samePick = (a: Pick3d, b: Pick3d) => JSON.stringify(a) === JSON.stringify(b);

export type SketchTool =
  | "select"
  | "line"
  | "line_mid"
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
  | "tangent"
  | "extend"
  | "use"
  | "split"
  | "move"
  | "copy"
  | "rotate"
  | "scale"
  /** Pegar lo copiado (Ctrl+V): no está en la barra */
  | "paste";

/** Qué hace un clic en el visor */
export type PickMode =
  | { kind: "none" }
  /** Elegir una cara del sólido: se avisa a `done` */
  | { kind: "face"; prompt: string; owner?: string; done: (face: FaceRef, index: number) => void }
  /** Marcar/desmarcar aristas: `toggle` con cada clic */
  | { kind: "edges"; prompt: string; owner?: string; toggle: (edge: EdgeRef, index: number) => void }
  /** Marcar/desmarcar caras: `toggle` con cada clic */
  | { kind: "faces"; prompt: string; owner?: string; toggle: (face: FaceRef, index: number) => void }
  /** Elegir una zona del escaneo */
  | { kind: "scan"; prompt: string; shape: "plane" | "cylinder"; done: (pick: ScanPick, triangle: number) => void }
  /** Marcar/desmarcar regiones de un sketch con un clic dentro */
  | { kind: "region"; prompt: string; owner?: string; sketch: number; chosen: () => P2[]; toggle: (p: P2) => void }
  /** Un eje: una línea del sketch `sketch` o una arista del sólido */
  | { kind: "axis"; prompt: string; owner?: string; sketch: number; done: (axis: AxisSpec) => void }
  /** Dónde va un sketch nuevo: un plano base o una cara plana */
  | { kind: "place"; prompt: string; done: (spec: PlaneSpec) => void }
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
}

const clone = <T>(v: T): T => structuredClone(v);
/** Herramientas que no dibujan: el modo construcción no marca lo que crean */
const NOT_DRAWING = new Set<SketchTool>(["select", "trim", "extend", "use", "split", "move", "copy", "rotate", "scale", "paste", "spline_point"]);

/** Qué se ve dentro del sketch */
export interface SketchShow {
  dims: boolean;
  constraints: boolean;
  construction: boolean;
  points: boolean;
  /** Flechas de lo que todavía se puede mover */
  freedom: boolean;
}
/** Qué dice la etiqueta de una cota: el valor, su nombre (d1, d2… o el parámetro) o la fórmula con el valor */
export type DimLabel = "value" | "name" | "expr";

/** Pasos que se pueden deshacer dentro de un sketch */
const SKETCH_HISTORY_LIMIT = 200;

export function createCadUi(store: CadStore) {
  const [pick, setPick] = createSignal<PickMode>({ kind: "none" });
  const [highlight, setHighlight] = createSignal<{ faces: number[]; edges: number[] }>({ faces: [], edges: [] });
  const [scanHighlight, setScanHighlight] = createSignal<number[]>([]);
  const [session, setSession] = createSignal<SketchSession>();
  const [tool, setTool] = createSignal<SketchTool>("line");
  const [selection, setSelection] = createSignal<number[]>([]);
  // Restricciones elegidas (por índice): con un ícono o desde el panel; Supr las borra
  const [selectedConstraints, setSelectedConstraints] = createSignal<number[]>([]);
  // Lo que nombra la restricción bajo el mouse en el panel (resaltado en el visor)
  const [hoverIds, setHoverIds] = createSignal<number[]>([]);
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
  // Plano 2D abierto sobre el visor
  const [drawingOpen, setDrawingOpen] = createSignal(false);
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
  const [sketchShow, setSketchShow] = createSignal<SketchShow>({ dims: true, constraints: true, construction: true, points: true, freedom: true });
  const [dimLabel, setDimLabel] = createSignal<DimLabel>("expr");
  // Sketch 3D: lo elegido (puntos y entidades) y dónde está el cursor
  const [sel3d, setSel3d] = createSignal<number[]>([]);
  const [cursor3d, setCursor3d] = createSignal<P3>();
  /** Spline con el peine de curvatura a la vista */
  const [combEntity, setCombEntity] = createSignal<number>();
  // Lo copiado con Ctrl+C: sobrevive a cerrar el sketch (para pegar en otro)
  const [clipboard, setClipboard] = createSignal<SketchClip>();
  // Resolver de a uno: mientras se arrastra no se encolan pedidos
  let solving = false;
  let pendingDrag: [number, P2] | null = null;
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
    message,
    setMessage,

    // ─── Sketch ───────────────────────────────────────────────────────────
    session,
    tool,
    setTool,
    selection,
    setSelection,
    hoverIds,
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
      resetHistory();
      return true;
    },

    /** Cambia el sketch en edición y lo resuelve */
    change(mutate: (s: Sketch) => void) {
      const s = session();
      if (!s) return;
      const next = clone(s.sketch);
      // Junto con lo que la herramienta actualice dentro de `mutate`: la vista
      // previa nunca ve el sketch viejo con el estado nuevo de la herramienta
      batch(() => {
        const before = new Set(s.sketch.entities.map((e) => e.id));
        mutate(next);
        if (constructionMode() && !NOT_DRAWING.has(tool())) for (const e of next.entities) if (!before.has(e.id)) e.construction = true;
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
    /**
     * Mueve, gira o escala lo elegido (`scale`: el factor, para radios y
     * textos); con `copy` deja lo elegido y pone una copia llevada por `f`
     * (solo traslaciones). Devuelve el aviso si no se pudo.
     */
    transformSelected(f: Xform, opts: { scale?: number; copy?: boolean } = {}): string | undefined {
      const sel = selection();
      let msg: string | undefined;
      let made: number[] | undefined;
      ui.change((sk) => {
        if (opts.copy) {
          const clip = extractClip(sk, sel);
          if (!clip) msg = "Elegir primero lo que se copia";
          else made = insertClip(sk, clip, f);
        } else msg = transformSelection(sk, sel, f, opts.scale ?? 1);
      });
      if (made) setSelection(made);
      return msg;
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
        else if (c.type === "angle") c.degrees = value;
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
        // Un texto se borra entero (alcanza con elegir una de sus curvas o su ancla)
        for (const t of [...(s.texts ?? [])]) if ([t.anchor, ...t.entities].some((x) => ids.has(x))) removeText(s, t);
        // Un punto suelto se elige por su punto
        for (const e of [...s.entities]) if (ids.has(e.id) || (e.geometry.type === "point" && ids.has(e.geometry.point))) removeEntity(s, e.id);
      });
      setSelection([]);
    },

    constructionMode,
    setConstructionMode,
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

    toggleConstruction() {
      const ids = new Set(selection());
      ui.change((s) => {
        for (const e of s.entities) if (ids.has(e.id)) e.construction = !e.construction;
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
      const s = session();
      if (!s) return;
      setSession(undefined);
      setSelection([]);
      resetHistory();
      // Las fórmulas de las cotas pasan a ser vínculos del documento (por
      // índice: se rehacen todos los de este sketch)
      const sketch = clone(s.sketch);
      const exprs = sketch.constraints.map((c) => {
        const e = (c as { expr?: string }).expr;
        delete (c as { expr?: string }).expr;
        return e;
      });
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

    cancelSketch() {
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
