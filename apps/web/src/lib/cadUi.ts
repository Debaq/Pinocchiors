// Estado de interacción del espacio Diseñar, compartido entre el visor
// (CadView) y el panel (DesignStep): qué se está eligiendo con el mouse y el
// sketch en edición.

import { batch, createSignal } from "solid-js";
import {
  addEntity,
  addPoint,
  addRectangle,
  removeEntity,
  type CadStore,
  type EdgeRef,
  type FaceRef,
  type P2,
  type Plane,
  type Region,
  type ScanPick,
  type Sketch,
  type SketchConstraint,
  type SolveReport,
} from "./cad";

export type SketchTool = "select" | "line" | "rect" | "circle" | "arc" | "polygon" | "slot" | "trim";

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
  | { kind: "region"; prompt: string; sketch: number; chosen: () => P2[]; toggle: (p: P2) => void };

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
  const [message, setMessage] = createSignal<string>();
  // Resolver de a uno: mientras se arrastra no se encolan pedidos
  let solving = false;
  let pendingDrag: [number, P2] | null = null;

  const solve = async (sketch: Sketch, drag?: [number, P2]) => {
    const s = session();
    if (!s) return;
    try {
      const r = await store.solveSketch(sketch, drag);
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
      setSession({ feature, plane: view.plane, sketch: clone(f.kind.sketch), report: view.report, regions: view.regions });
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

    setConstraintValue(index: number, value: number) {
      ui.change((s) => {
        const c = s.constraints[index];
        if (!c) return;
        if ("value" in c) c.value = value;
        else if (c.type === "angle") c.degrees = value;
      });
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
      await store.updateFeature(s.feature, (f) => {
        if (f.kind.type === "sketch") f.kind.sketch = s.sketch;
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
