// Estado de interacción del espacio Diseñar, compartido entre el visor
// (CadView) y el panel (DesignStep): qué se está eligiendo con el mouse y el
// sketch en edición.

import { batch, createEffect, createSignal, on } from "solid-js";
import type { BasePlane } from "./CadViewer";
import {
  constraintPath,
  plainNumber,
  addEntity,
  addPoint,
  addRectangle,
  removeEntity,
  type CadStore,
  type EdgeRef,
  type FaceRef,
  type P2,
  type Plane,
  type PlaneSpec,
  type Region,
  type ScanPick,
  type Sketch,
  type SketchConstraint,
  type SolveReport,
} from "./cad";

/** Algo elegido en el visor fuera de la edición de sketches */
export type Pick3d =
  | { kind: "face"; face: number }
  | { kind: "edge"; edge: number }
  | { kind: "region"; sketch: number; region: number }
  | { kind: "plane"; plane: BasePlane };

const samePick = (a: Pick3d, b: Pick3d) => JSON.stringify(a) === JSON.stringify(b);

export type SketchTool = "select" | "line" | "rect" | "circle" | "arc" | "polygon" | "slot" | "trim" | "tangent" | "extend";

/** Qué hace un clic en el visor */
export type PickMode =
  | { kind: "none" }
  /** Elegir una cara del sólido: se avisa a `done` */
  | { kind: "face"; prompt: string; done: (face: FaceRef, index: number) => void }
  /** Marcar/desmarcar aristas: `toggle` con cada clic */
  | { kind: "edges"; prompt: string; toggle: (edge: EdgeRef, index: number) => void }
  /** Elegir una zona del escaneo */
  | { kind: "scan"; prompt: string; shape: "plane" | "cylinder"; done: (pick: ScanPick, triangle: number) => void }
  /** Marcar/desmarcar regiones de un sketch con un clic dentro */
  | { kind: "region"; prompt: string; sketch: number; chosen: () => P2[]; toggle: (p: P2) => void }
  /** Dónde va un sketch nuevo: un plano base o una cara plana */
  | { kind: "place"; prompt: string; done: (spec: PlaneSpec) => void };

export interface SketchSession {
  feature: number;
  plane: Plane;
  sketch: Sketch;
  report?: SolveReport;
  regions: Region[];
}

const clone = <T>(v: T): T => structuredClone(v);

export function createCadUi(store: CadStore) {
  const [pick, setPick] = createSignal<PickMode>({ kind: "none" });
  const [highlight, setHighlight] = createSignal<{ faces: number[]; edges: number[] }>({ faces: [], edges: [] });
  const [scanHighlight, setScanHighlight] = createSignal<number[]>([]);
  const [session, setSession] = createSignal<SketchSession>();
  const [tool, setTool] = createSignal<SketchTool>("line");
  const [selection, setSelection] = createSignal<number[]>([]);
  // Lo que nombra la restricción bajo el mouse en el panel (resaltado en el visor)
  const [hoverIds, setHoverIds] = createSignal<number[]>([]);
  // Selección en el visor (caras, aristas, regiones, planos), como en Onshape
  const [picks, setPicks] = createSignal<Pick3d[]>([]);
  const [hiddenSketches, setHiddenSketches] = createSignal<number[]>([]);
  const [showPlanes, setShowPlanes] = createSignal(true);
  // Recalcular cambia los índices de caras, aristas y regiones: la selección vieja ya no vale
  createEffect(on(() => store.result()?.version, () => setPicks((p) => p.filter((x) => x.kind === "plane")), { defer: true }));
  const [message, setMessage] = createSignal<string>();
  // Resolver de a uno: mientras se arrastra no se encolan pedidos
  let solving = false;
  let pendingDrag: [number, P2] | null = null;

  const solve = async (sketch: Sketch, drag?: [number, P2]) => {
    const s = session();
    if (!s) return;
    try {
      const r = await store.solveSketch(sketch, drag);
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
    toggleSketchVisible(id: number) {
      setHiddenSketches((h) => (h.includes(id) ? h.filter((x) => x !== id) : [...h, id]));
    },
    showPlanes,
    setShowPlanes,

    /** Empieza a editar el sketch de la operación `feature` */
    editSketch(feature: number): boolean {
      const f = store.doc()?.features.find((x) => x.id === feature);
      const view = store.sketchView(feature);
      if (!f || f.kind.type !== "sketch" || !view) {
        setMessage("El sketch todavía no tiene plano (recalcular primero)");
        return false;
      }
      ui.cancelPick();
      setSelection([]);
      setTool("line");
      // El sketch ya resuelto (con las fórmulas aplicadas); cada cota vinculada
      // lleva su fórmula mientras se edita
      const sketch = clone(view.sketch);
      sketch.constraints.forEach((c, i) => {
        const expr = store.bindingOf(constraintPath(feature, i, c));
        if (expr) (c as { expr?: string }).expr = expr;
      });
      setSession({ feature, plane: view.plane, sketch, report: view.report, regions: view.regions });
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
        mutate(next);
        setSession({ ...s, sketch: next });
      });
      void solve(next);
    },

    /** Arrastre de un punto (se resuelve el último pedido cuando termina el anterior) */
    async drag(point: number, target: P2) {
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

    deleteSelection() {
      const ids = new Set(selection());
      ui.change((s) => {
        for (const e of [...s.entities]) if (ids.has(e.id)) removeEntity(s, e.id);
      });
      setSelection([]);
    },

    toggleConstruction() {
      const ids = new Set(selection());
      ui.change((s) => {
        for (const e of s.entities) if (ids.has(e.id)) e.construction = !e.construction;
      });
    },

    /** Guarda el sketch en el documento y sale */
    async finishSketch() {
      const s = session();
      if (!s) return;
      setSession(undefined);
      setSelection([]);
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
        if (f?.kind.type === "sketch") f.kind.sketch = sketch;
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
    },

    // Ayudas de dibujo usadas por el visor
    addPoint: (s: Sketch, p: P2) => addPoint(s, p),
    addEntity,
    addRectangle,
  };
  return ui;
}

export type CadUi = ReturnType<typeof createCadUi>;
