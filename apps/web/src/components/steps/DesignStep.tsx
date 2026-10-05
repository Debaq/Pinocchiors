import { Component, For, Match, Show, Switch, createSignal, type JSX } from "solid-js";
import { open, save } from "@tauri-apps/plugin-dialog";
import { clsx } from "clsx";
import {
  BASE_PLANES,
  CONSTRAINT_LABELS,
  FEATURE_LABELS,
  OP_LABELS,
  PLANE_LABELS,
  constraintIds,
  constraintValue,
  emptySketch,
  filletCorner,
  linesAt,
  offsetEntities,
  offsetPlane,
  type AxisSpec,
  type BodyOp,
  type CadStore,
  type Detection,
  type EdgeRef,
  type Extent,
  type FaceRef,
  type Feature,
  type FeatureKind,
  type P2,
  type P3,
  type PatternKind,
  type RegionSelection,
  type PlaneSpec,
  type ScanPick,
  type Sketch,
  type SketchConstraint,
} from "../../lib/cad";
import type { CadUi } from "../../lib/cadUi";
import { regionContains } from "../../lib/CadViewer";
import { Button, Checkbox, IconButton, Select } from "../ui";
import * as Icons from "../icons";

export interface DesignStepProps {
  store: CadStore;
  ui: CadUi;
  /** Hay un modelo cargado que se puede usar como escaneo de referencia */
  hasModel: boolean;
  /** El sólido pasó a ser el modelo de la app */
  onUseAsModel?: () => void;
}

// ─── Controles chicos ─────────────────────────────────────────────────────

/** Número que se aplica al confirmar (Enter o salir del campo), no con cada tecla */
const Num: Component<{ value: number; onCommit: (v: number) => void; label?: string; suffix?: string; step?: number; min?: number }> = (props) => (
  <label class="flex items-center gap-2 text-xs">
    <Show when={props.label}>
      <span class={clsx("text-text-muted shrink-0", (props.label?.length ?? 0) <= 2 ? "w-3" : "w-24")}>{props.label}</span>
    </Show>
    <input
      type="number"
      value={Number.isFinite(props.value) ? +props.value.toFixed(6) : 0}
      step={props.step ?? 1}
      min={props.min}
      class="w-full min-w-0 px-2 py-1 rounded bg-surface/40 border border-border text-xs text-text font-mono outline-none focus:border-accent [appearance:textfield]"
      onChange={(e) => {
        const v = parseFloat(e.currentTarget.value);
        if (!Number.isNaN(v) && v !== props.value) props.onCommit(v);
      }}
      onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
    />
    <Show when={props.suffix}>
      <span class="text-text-dim shrink-0">{props.suffix}</span>
    </Show>
  </label>
);

/**
 * Campo que acepta un número o una fórmula (`ancho / 2`). Con fórmula muestra
 * "fx" y el valor calculado al pasar el mouse. `onCommit` devuelve un error
 * si la fórmula no se puede calcular (y el texto queda para corregirlo).
 */
export const Formula: Component<{
  value: number;
  expr?: string;
  error?: string | null;
  label?: string;
  suffix?: string;
  onCommit: (text: string) => Promise<string | undefined> | string | undefined;
}> = (props) => {
  const [err, setErr] = createSignal<string>();
  const shown = () => props.expr ?? (Number.isFinite(props.value) ? String(+props.value.toFixed(6)) : "0");
  const problem = () => err() ?? props.error ?? undefined;
  return (
    <label class="flex items-center gap-2 text-xs" title={problem() ?? (props.expr ? `= ${+props.value.toFixed(6)}` : "Acepta fórmulas: ancho / 2, max(a, b)…")}>
      <Show when={props.label}>
        <span class={clsx("text-text-muted shrink-0", (props.label?.length ?? 0) <= 2 ? "w-3" : "w-24")}>{props.label}</span>
      </Show>
      <div class="relative flex-1 min-w-0">
        <input
          type="text"
          value={shown()}
          class={clsx(
            "w-full min-w-0 px-2 py-1 rounded bg-surface/40 border text-xs text-text font-mono outline-none focus:border-accent",
            problem() ? "border-error" : props.expr ? "border-accent/60 pr-6" : "border-border",
          )}
          onChange={async (e) => {
            const text = e.currentTarget.value.trim();
            if (!text || text === shown()) return setErr(undefined);
            setErr((await props.onCommit(text)) ?? undefined);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") e.currentTarget.blur();
            e.stopPropagation();
          }}
        />
        <Show when={props.expr}>
          <span class="absolute right-1.5 top-1/2 -translate-y-1/2 text-[9px] font-semibold italic text-accent pointer-events-none">fx</span>
        </Show>
      </div>
      <Show when={props.suffix}>
        <span class="text-text-dim shrink-0">{props.suffix}</span>
      </Show>
    </label>
  );
};

const Row: Component<{ label: string; children: JSX.Element }> = (props) => (
  <div class="flex items-center gap-2 text-xs">
    <span class="text-text-muted w-24 shrink-0">{props.label}</span>
    <div class="flex-1 min-w-0">{props.children}</div>
  </div>
);

const Section: Component<{ title: string; children: JSX.Element; right?: JSX.Element }> = (props) => (
  <div class="space-y-2">
    <div class="flex items-center justify-between">
      <h4 class="text-[11px] font-semibold uppercase tracking-wide text-text-dim">{props.title}</h4>
      {props.right}
    </div>
    {props.children}
  </div>
);

const OP_OPTIONS = (Object.keys(OP_LABELS) as BodyOp[]).map((v) => ({ value: v, label: OP_LABELS[v] }));
const AXIS_DIRS: Record<string, P3> = { x: [1, 0, 0], y: [0, 1, 0], z: [0, 0, 1] };

const fmt = (v: number, d = 2) => v.toLocaleString("es", { maximumFractionDigits: d, minimumFractionDigits: d });

function planeLabel(p: PlaneSpec): string {
  if (p.type === "face") return "Cara del sólido";
  if (p.type === "custom") return "Plano propio";
  return PLANE_LABELS[p.type];
}

// ─── Panel ────────────────────────────────────────────────────────────────

export const DesignStep: Component<DesignStepProps> = (props) => {
  const store = props.store;
  const ui = props.ui;
  const [scanResult, setScanResult] = createSignal<{ pick: ScanPick; triangle: number }>();
  const [sliceBase, setSliceBase] = createSignal<"xy" | "xz" | "yz">("xy");
  const [sliceOffset, setSliceOffset] = createSignal(0);
  const [detections, setDetections] = createSignal<Detection[]>();
  const [notice, setNotice] = createSignal<string>();

  const features = () => store.doc()?.features ?? [];
  const selectedFeature = () => features().find((f) => f.id === store.selected());
  const sketches = () => features().filter((f) => f.kind.type === "sketch");
  const toolFeatures = () => features().filter((f) => ["extrude", "revolve", "primitive", "import"].includes(f.kind.type));

  const say = (msg: string) => {
    setNotice(msg);
    setTimeout(() => setNotice((m) => (m === msg ? undefined : m)), 4000);
  };

  // ─── Agregar operaciones ─────────────────────────────────────────────

  const newSketch = async (plane: PlaneSpec) => {
    const id = await store.addFeature({ type: "sketch", plane, offset: 0, sketch: emptySketch() });
    ui.editSketch(id);
  };

  const sketchOnFace = () =>
    ui.setPick({ kind: "face", prompt: "Elegir una cara plana del sólido para el sketch", done: (face) => void newSketch({ type: "face", face }) });

  /** Sketch al que apuntan extrusiones y revoluciones nuevas: el elegido o el último */
  const targetSketch = (): Feature | undefined => {
    const sel = selectedFeature();
    if (sel?.kind.type === "sketch") return sel;
    return [...sketches()].pop();
  };

  const addExtrude = () => {
    const s = targetSketch();
    if (!s) return say("Primero hace falta un sketch con una región cerrada");
    void store.addFeature({
      type: "extrude",
      sketch: s.id,
      regions: { type: "all" },
      extent: { type: "blind", distance: 10 },
      reverse: false,
      op: "join",
    });
  };

  const addRevolve = () => {
    const s = targetSketch();
    if (!s || s.kind.type !== "sketch") return say("Primero hace falta un sketch con una región cerrada");
    // Eje: la primera línea de construcción del sketch, o Z
    const axisLine = s.kind.sketch.entities.find((e) => e.construction && e.geometry.type === "line");
    const axis: AxisSpec = axisLine ? { type: "sketch_line", sketch: s.id, line: axisLine.id } : { type: "z" };
    void store.addFeature({ type: "revolve", sketch: s.id, regions: { type: "all" }, axis, angle: 360, op: "join" });
  };

  const addPrimitive = (shape: FeatureKind & { type: "primitive" }) => void store.addFeature(shape);
  const primitive = (s: Extract<FeatureKind, { type: "primitive" }>["shape"]): Extract<FeatureKind, { type: "primitive" }> => ({
    type: "primitive",
    shape: s,
    origin: [0, 0, 0],
    z: [0, 0, 1],
    x: [1, 0, 0],
    op: "join",
  });

  /** Redondeo o chaflán: se eligen aristas y se crea al confirmar */
  const [edgePick, setEdgePick] = createSignal<{ kind: "fillet" | "chamfer"; edges: EdgeRef[]; feature?: number }>();
  const startEdges = (kind: "fillet" | "chamfer", feature?: number) => {
    if (!store.result()?.body) return say("Primero hace falta un sólido");
    setEdgePick({ kind, edges: [], feature });
    ui.setHighlight({ faces: [], edges: [] });
    ui.setPick({
      kind: "edges",
      prompt: "Elegir aristas (clic para marcar o desmarcar) y confirmar en el panel",
      toggle: (ref) =>
        setEdgePick((p) => {
          if (!p) return p;
          const same = (a: EdgeRef) => Math.hypot(a.point[0] - ref.point[0], a.point[1] - ref.point[1], a.point[2] - ref.point[2]) < 1e-6;
          const edges = p.edges.some(same) ? p.edges.filter((a) => !same(a)) : [...p.edges, ref];
          return { ...p, edges };
        }),
    });
  };
  const confirmEdges = () => {
    const p = edgePick();
    ui.cancelPick();
    setEdgePick(undefined);
    if (!p || p.edges.length === 0) return;
    if (p.feature !== undefined) {
      void store.updateFeature(p.feature, (f) => {
        if (f.kind.type === "fillet" || f.kind.type === "chamfer") f.kind.edges = p.edges;
      });
    } else if (p.kind === "fillet") void store.addFeature({ type: "fillet", edges: p.edges, radius: 1 });
    else void store.addFeature({ type: "chamfer", edges: p.edges, distance: 1 });
  };

  /** Elegir varias caras (vaciado, desmolde) */
  const [facePick, setFacePick] = createSignal<{ kind: "shell" | "draft"; faces: FaceRef[]; feature?: number }>();
  const pickNextFace = () => {
    ui.setPick({
      kind: "face",
      prompt: "Elegir una cara (se pueden sumar más desde el panel)",
      done: (face) => setFacePick((p) => (p ? { ...p, faces: [...p.faces, face] } : p)),
    });
  };
  const startFaces = (kind: "shell" | "draft", feature?: number) => {
    if (!store.result()?.body) return say("Primero hace falta un sólido");
    setFacePick({ kind, faces: [], feature });
    pickNextFace();
  };
  const confirmFaces = () => {
    const p = facePick();
    ui.cancelPick();
    setFacePick(undefined);
    if (!p || p.faces.length === 0) return;
    if (p.feature !== undefined) {
      void store.updateFeature(p.feature, (f) => {
        if (f.kind.type === "shell" || f.kind.type === "draft") f.kind.faces = p.faces;
      });
    } else if (p.kind === "shell") void store.addFeature({ type: "shell", faces: p.faces, thickness: 1 });
    else void store.addFeature({ type: "draft", faces: p.faces, neutral: { type: "xy" }, angle: 3 });
  };

  const addPattern = (kind: "linear" | "circular") => {
    const sel = selectedFeature();
    const features = sel && toolFeatures().includes(sel) ? [sel.id] : [];
    void store.addFeature({
      type: "pattern",
      features,
      pattern: kind === "linear" ? { type: "linear", direction: [1, 0, 0], count: 3, spacing: 10 } : { type: "circular", axis: { type: "z" }, count: 6, angle: 360 },
    });
  };

  const addMirror = () => {
    const sel = selectedFeature();
    const features = sel && toolFeatures().includes(sel) ? [sel.id] : [];
    void store.addFeature({ type: "mirror", features, plane: { type: "yz" } });
  };

  // ─── Archivos ─────────────────────────────────────────────────────────

  const exportAs = async (format: "step" | "stl" | "3mf") => {
    const names = { step: "STEP", stl: "STL", "3mf": "3MF" };
    const path = await save({ filters: [{ name: names[format], extensions: [format === "step" ? "step" : format] }], defaultPath: `diseno.${format}` });
    if (!path) return;
    try {
      const bytes = await store.exportDesign(path, format);
      say(`Exportado (${fmt(bytes / 1024, 0)} KB)`);
    } catch (e) {
      say(String(e));
    }
  };

  const importStep = async () => {
    const path = await open({ filters: [{ name: "STEP", extensions: ["step", "stp"] }], multiple: false });
    if (typeof path !== "string") return;
    try {
      await store.importStep(path);
    } catch (e) {
      say(String(e));
    }
  };

  // ─── Escaneo → CAD ────────────────────────────────────────────────────

  const pickScan = (shape: "plane" | "cylinder") => {
    setScanResult(undefined);
    ui.setPick({
      kind: "scan",
      shape,
      prompt: shape === "plane" ? "Clic sobre una zona plana del modelo" : "Clic sobre una pared cilíndrica (agujero o tetón)",
      done: (pick, triangle) => {
        ui.setPick({ kind: "none" });
        setScanResult({ pick, triangle });
      },
    });
  };

  const addScan = async (feature: Parameters<CadStore["scanAdd"]>[0]) => {
    try {
      await store.scanAdd(feature);
      setScanResult(undefined);
      ui.setScanHighlight([]);
    } catch (e) {
      say(String(e));
    }
  };

  const detect = async () => {
    try {
      setDetections(await store.scanDetect());
    } catch (e) {
      say(String(e));
    }
  };

  // ─── Render ───────────────────────────────────────────────────────────

  return (
    <div class="space-y-5">
      <Show when={store.status()?.available !== false} fallback={<NoKernel />}>
        <Show
          when={store.doc()}
          fallback={
            <div class="space-y-3">
              <p class="text-xs text-text-muted leading-relaxed">
                Diseño paramétrico: sketches con medidas, extrusiones, redondeos, agujeros y patrones. Todo queda editable: cambiar una medida
                recalcula la pieza. Con un modelo escaneado cargado, se pueden tomar sus caras planas y cilíndricas como punto de partida.
              </p>
              <Button variant="primary" size="sm" fullWidth icon={<Icons.Plus size={14} />} onClick={() => void store.newDesign()}>
                Nuevo diseño
              </Button>
              <Button size="sm" fullWidth icon={<Icons.FolderOpen size={14} />} onClick={() => void importStep()}>
                Abrir un STEP
              </Button>
            </div>
          }
        >
          <Show when={ui.session()} fallback={
            <>
              <AddSection
                onSketch={(p) => void newSketch(p)}
                onSketchOnFace={sketchOnFace}
                onExtrude={addExtrude}
                onRevolve={addRevolve}
                onPrimitive={(s) => addPrimitive(primitive(s))}
                onFillet={() => startEdges("fillet")}
                onChamfer={() => startEdges("chamfer")}
                onShell={() => startFaces("shell")}
                onDraft={() => startFaces("draft")}
                onPattern={addPattern}
                onMirror={addMirror}
                onSplit={() => void store.addFeature({ type: "split", plane: { type: "custom", plane: offsetPlane("xy", 0) }, flip: false })}
                onImport={() => void importStep()}
                hasBody={!!store.result()?.body}
              />

              <Show when={edgePick()}>
                {(p) => (
                  <div class="rounded-md border border-accent/50 p-2 space-y-2">
                    <p class="text-xs text-text">
                      {p().kind === "fillet" ? "Redondeo" : "Chaflán"}: {p().edges.length} aristas elegidas
                    </p>
                    <div class="flex gap-2">
                      <Button size="sm" variant="primary" disabled={p().edges.length === 0} onClick={confirmEdges}>
                        Aplicar
                      </Button>
                      <Button size="sm" variant="ghost" onClick={() => (ui.cancelPick(), setEdgePick(undefined))}>
                        Cancelar
                      </Button>
                    </div>
                  </div>
                )}
              </Show>
              <Show when={facePick()}>
                {(p) => (
                  <div class="rounded-md border border-accent/50 p-2 space-y-2">
                    <p class="text-xs text-text">
                      {p().kind === "shell" ? "Vaciado (caras que quedan abiertas)" : "Desmolde"}: {p().faces.length} caras
                    </p>
                    <div class="flex gap-2 flex-wrap">
                      <Button size="sm" onClick={pickNextFace}>
                        Sumar cara
                      </Button>
                      <Button size="sm" variant="primary" disabled={p().faces.length === 0} onClick={confirmFaces}>
                        Aplicar
                      </Button>
                      <Button size="sm" variant="ghost" onClick={() => (ui.cancelPick(), setFacePick(undefined))}>
                        Cancelar
                      </Button>
                    </div>
                  </div>
                )}
              </Show>

              <ParametersSection store={store} />

              <FeatureTree store={store} ui={ui} />

              <Show when={selectedFeature()}>
                {(f) => (
                  <FeatureEditor
                    feature={f()}
                    store={store}
                    ui={ui}
                    sketches={sketches()}
                    tools={toolFeatures()}
                    onEdges={(kind) => startEdges(kind, f().id)}
                    onFaces={(kind) => startFaces(kind, f().id)}
                  />
                )}
              </Show>

              <Show when={props.hasModel}>
                <Section title="Desde el escaneo">
                  <p class="text-[11px] text-text-dim leading-relaxed">
                    El modelo cargado se ve translúcido detrás del diseño. Elegir una zona ajusta un plano o un cilindro y lo convierte en operaciones.
                  </p>
                  <div class="grid grid-cols-2 gap-1.5">
                    <Button size="sm" onClick={() => pickScan("plane")}>
                      Cara plana
                    </Button>
                    <Button size="sm" onClick={() => pickScan("cylinder")}>
                      Cilindro / agujero
                    </Button>
                  </div>
                  <Show when={scanResult()}>
                    {(r) => <ScanResultCard result={r()} onAdd={addScan} />}
                  </Show>
                  <div class="space-y-1.5">
                    <span class="text-xs text-text-muted">Corte del modelo por un plano</span>
                    <div class="flex gap-1.5">
                      <Select
                        options={(["xy", "xz", "yz"] as const).map((k) => ({ value: k, label: PLANE_LABELS[k] }))}
                        value={sliceBase()}
                        onChange={(v) => setSliceBase(v as "xy" | "xz" | "yz")}
                      />
                    </div>
                    <Num label="A distancia" value={sliceOffset()} onCommit={setSliceOffset} suffix="mm" />
                    <Button size="sm" fullWidth onClick={() => void addScan({ kind: "slice", plane: offsetPlane(sliceBase(), sliceOffset()) })}>
                      Sketch con el corte
                    </Button>
                  </div>
                  <Button size="sm" variant="ghost" fullWidth onClick={() => void detect()}>
                    Detectar planos y cilindros
                  </Button>
                  <Show when={detections()}>
                    {(list) => (
                      <div class="max-h-48 overflow-y-auto space-y-0.5">
                        <For each={list()} fallback={<p class="text-xs text-text-dim">No se encontraron zonas</p>}>
                          {(d) => (
                            <div class="group flex items-center gap-1 rounded hover:bg-surface">
                              <button class="flex-1 flex justify-between gap-2 px-2 py-1 text-xs text-left" onClick={() => ui.setScanHighlight(d.faces)}>
                                <span class="text-text">
                                  {d.shape.type === "plane" ? "Plano" : d.shape.type === "cylinder" ? `Cilindro r ${fmt(d.shape.radius)}` : `Esfera r ${fmt(d.shape.radius)}`}
                                </span>
                                <span class="text-text-dim font-mono">{fmt(d.area, 0)} mm²</span>
                              </button>
                              <Show when={d.shape.type !== "sphere"}>
                                <button
                                  class="hidden group-hover:block px-1.5 text-[11px] text-accent hover:text-accent-hover"
                                  title={d.shape.type === "plane" ? "Agregar el contorno como sketch" : "Agregar el cilindro"}
                                  onClick={() =>
                                    void addScan(
                                      d.shape.type === "plane"
                                        ? { kind: "plane_outline", triangle: d.faces[0], extrude: false }
                                        : { kind: "cylinder", triangle: d.faces[0] },
                                    )
                                  }
                                >
                                  Agregar
                                </button>
                              </Show>
                            </div>
                          )}
                        </For>
                      </div>
                    )}
                  </Show>
                </Section>
              </Show>

              <BodySection store={store} onExport={(f) => void exportAs(f)} onUseAsModel={props.onUseAsModel} />

              <div class="flex gap-2">
                <Button size="sm" variant="ghost" icon={<Icons.ArrowCounterClockwise size={14} />} disabled={!store.canUndo()} onClick={() => store.undo()}>
                  Deshacer
                </Button>
                <Button size="sm" variant="ghost" icon={<Icons.ArrowClockwise size={14} />} disabled={!store.canRedo()} onClick={() => store.redo()}>
                  Rehacer
                </Button>
              </div>
            </>
          }>
            <SketchPanel ui={ui} />
          </Show>
        </Show>
      </Show>
      <Show when={notice()}>
        <p class="text-xs text-warning">{notice()}</p>
      </Show>
    </div>
  );
};

const NoKernel: Component = () => (
  <div class="space-y-2">
    <p class="text-xs text-warning">Esta compilación no incluye el núcleo CAD.</p>
    <p class="text-[11px] text-text-dim leading-relaxed">
      El diseño paramétrico usa OpenCASCADE. Las versiones publicadas lo traen incluido; al compilar a mano hay que instalarlo (en Arch:
      pacman -S opencascade).
    </p>
  </div>
);

// ─── Agregar ──────────────────────────────────────────────────────────────

const AddSection: Component<{
  onSketch: (p: PlaneSpec) => void;
  onSketchOnFace: () => void;
  onExtrude: () => void;
  onRevolve: () => void;
  onPrimitive: (s: Extract<FeatureKind, { type: "primitive" }>["shape"]) => void;
  onFillet: () => void;
  onChamfer: () => void;
  onShell: () => void;
  onDraft: () => void;
  onPattern: (k: "linear" | "circular") => void;
  onMirror: () => void;
  onSplit: () => void;
  onImport: () => void;
  hasBody: boolean;
}> = (props) => {
  const B = (p: { label: string; onClick: () => void; disabled?: boolean; title?: string }) => (
    <button
      class="px-2 py-1.5 rounded text-xs text-left bg-surface/40 border border-border text-text hover:border-border-hover hover:bg-surface disabled:opacity-40 disabled:pointer-events-none"
      onClick={p.onClick}
      disabled={p.disabled}
      title={p.title}
    >
      {p.label}
    </button>
  );
  return (
    <Section title="Agregar">
      <div class="space-y-2">
        <div class="grid grid-cols-2 gap-1.5">
          <B label="Sketch en planta" onClick={() => props.onSketch({ type: "xy" })} />
          <B label="Sketch de frente" onClick={() => props.onSketch({ type: "xz" })} />
          <B label="Sketch lateral" onClick={() => props.onSketch({ type: "yz" })} />
          <B label="Sketch en una cara" onClick={props.onSketchOnFace} disabled={!props.hasBody} />
        </div>
        <div class="grid grid-cols-2 gap-1.5">
          <B label="Extrusión" onClick={props.onExtrude} title="Del sketch elegido (o el último)" />
          <B label="Revolución" onClick={props.onRevolve} title="Eje: la primera línea de construcción del sketch, o Z" />
        </div>
        <div class="grid grid-cols-3 gap-1.5">
          <B label="Caja" onClick={() => props.onPrimitive({ type: "box", dx: 20, dy: 20, dz: 20 })} />
          <B label="Cilindro" onClick={() => props.onPrimitive({ type: "cylinder", radius: 10, height: 20 })} />
          <B label="Esfera" onClick={() => props.onPrimitive({ type: "sphere", radius: 10 })} />
          <B label="Cono" onClick={() => props.onPrimitive({ type: "cone", r1: 10, r2: 0, height: 20 })} />
          <B label="Toro" onClick={() => props.onPrimitive({ type: "torus", major: 15, minor: 4 })} />
          <B label="STEP..." onClick={props.onImport} />
        </div>
        <div class="grid grid-cols-2 gap-1.5">
          <B label="Redondeo" onClick={props.onFillet} disabled={!props.hasBody} />
          <B label="Chaflán" onClick={props.onChamfer} disabled={!props.hasBody} />
          <B label="Vaciado" onClick={props.onShell} disabled={!props.hasBody} />
          <B label="Desmolde" onClick={props.onDraft} disabled={!props.hasBody} />
          <B label="Patrón lineal" onClick={() => props.onPattern("linear")} disabled={!props.hasBody} />
          <B label="Patrón circular" onClick={() => props.onPattern("circular")} disabled={!props.hasBody} />
          <B label="Simetría" onClick={props.onMirror} disabled={!props.hasBody} />
          <B label="Cortar por plano" onClick={props.onSplit} disabled={!props.hasBody} />
        </div>
      </div>
    </Section>
  );
};

// ─── Parámetros ───────────────────────────────────────────────────────────

const NAME_RE = /^[\p{L}_][\p{L}\p{N}_]*$/u;

const ParametersSection: Component<{ store: CadStore }> = (props) => {
  const store = props.store;
  const params = () => store.doc()?.parameters ?? [];
  const [problem, setProblem] = createSignal<string>();
  const add = () => {
    let n = params().length + 1;
    while (params().some((p) => p.name === `p${n}`)) n++;
    void store.addParameter(`p${n}`, "10");
  };
  return (
    <Section
      title="Parámetros"
      right={
        <IconButton aria-label="Agregar parámetro" size="sm" variant="ghost" onClick={add}>
          <Icons.Plus size={12} />
        </IconButton>
      }
    >
      <Show
        when={params().length > 0}
        fallback={
          <p class="text-[11px] text-text-dim leading-relaxed">
            Medidas con nombre para usar en cualquier campo: escribir <span class="font-mono">ancho / 2</span> en una cota o una distancia
            la deja atada al parámetro.
          </p>
        }
      >
        <div class="space-y-1">
          <For each={params()}>
            {(p, i) => {
              const res = () => store.parameterResult(p.name);
              return (
                <div class="flex items-center gap-1.5">
                  <input
                    value={p.name}
                    class="w-20 shrink-0 px-1.5 py-1 rounded bg-surface/40 border border-border text-xs text-text font-mono outline-none focus:border-accent"
                    onChange={(e) => {
                      const name = e.currentTarget.value.trim();
                      if (name === p.name) return;
                      if (!NAME_RE.test(name)) return setProblem(`«${name}» no sirve como nombre (letras, números y _)`);
                      if (params().some((q, j) => j !== i() && q.name === name)) return setProblem(`ya hay un parámetro «${name}»`);
                      setProblem(undefined);
                      void store.updateParameter(i(), { name });
                    }}
                    onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
                  />
                  <span class="text-text-dim text-xs">=</span>
                  <input
                    value={p.expr}
                    class={clsx(
                      "flex-1 min-w-0 px-1.5 py-1 rounded bg-surface/40 border text-xs text-text font-mono outline-none focus:border-accent",
                      res()?.error ? "border-error" : "border-border",
                    )}
                    title={res()?.error ?? undefined}
                    onChange={(e) => {
                      const expr = e.currentTarget.value.trim();
                      if (expr && expr !== p.expr) void store.updateParameter(i(), { expr });
                    }}
                    onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
                  />
                  <span class={clsx("w-14 text-right text-xs font-mono truncate", res()?.error ? "text-error" : "text-text-muted")} title={res()?.error ?? undefined}>
                    {res()?.error ? "error" : res()?.value != null ? fmt(res()!.value!, 3).replace(/,?0+$/, "") : ""}
                  </span>
                  <IconButton aria-label="Quitar parámetro" size="sm" variant="ghost" onClick={() => void store.removeParameter(i())}>
                    <Icons.X size={10} />
                  </IconButton>
                </div>
              );
            }}
          </For>
        </div>
      </Show>
      <Show when={problem()}>
        <p class="text-[11px] text-error">{problem()}</p>
      </Show>
    </Section>
  );
};

// ─── Árbol ────────────────────────────────────────────────────────────────

const FeatureTree: Component<{ store: CadStore; ui: CadUi }> = (props) => {
  const store = props.store;
  const doc = () => store.doc();
  const rollback = () => doc()?.rollback ?? null;
  return (
    <Section title="Operaciones">
      <Show when={(doc()?.features.length ?? 0) > 0} fallback={<p class="text-xs text-text-dim">Todavía no hay operaciones</p>}>
        <div class="space-y-0.5">
          <For each={doc()?.features ?? []}>
            {(f, i) => {
              const state = () => store.stateOf(f.id);
              const rolled = () => rollback() !== null && i() >= rollback()!;
              return (
                <>
                  <div
                    class={clsx(
                      "group flex items-center gap-1.5 px-2 py-1 rounded text-xs cursor-pointer",
                      store.selected() === f.id ? "bg-accent/20 text-text" : "hover:bg-surface text-text-muted",
                      (f.suppressed || rolled()) && "opacity-50",
                    )}
                    onClick={() => store.select(store.selected() === f.id ? undefined : f.id)}
                    onDblClick={() => f.kind.type === "sketch" && props.ui.editSketch(f.id)}
                    title={state()?.state === "error" ? (state() as { message: string }).message : undefined}
                  >
                    <Show
                      when={state()?.state === "error"}
                      fallback={<span class="w-3.5 text-center text-text-dim">{FEATURE_LABELS[f.kind.type][0]}</span>}
                    >
                      <Icons.Warning size={14} class="text-error shrink-0" />
                    </Show>
                    <span class={clsx("flex-1 truncate", f.suppressed && "line-through")}>{f.name}</span>
                    <span class="hidden group-hover:flex items-center gap-0.5" onClick={(e) => e.stopPropagation()}>
                      <IconButton aria-label="Subir" size="sm" variant="ghost" onClick={() => !store.moveFeature(f.id, -1) && props.ui.setMessage("No se puede: quedaría antes de lo que necesita")}>
                        <Icons.CaretDown size={10} class="rotate-180" />
                      </IconButton>
                      <IconButton aria-label="Bajar" size="sm" variant="ghost" onClick={() => !store.moveFeature(f.id, 1) && props.ui.setMessage("No se puede: otra operación la necesita antes")}>
                        <Icons.CaretDown size={10} />
                      </IconButton>
                      <IconButton
                        aria-label={f.suppressed ? "Activar" : "Suprimir"}
                        size="sm"
                        variant="ghost"
                        onClick={() => void store.updateFeature(f.id, (x) => (x.suppressed = !x.suppressed))}
                      >
                        {f.suppressed ? <Icons.EyeSlash size={12} /> : <Icons.Eye size={12} />}
                      </IconButton>
                      <IconButton
                        aria-label="Borrar"
                        size="sm"
                        variant="ghost"
                        onClick={() => !store.removeFeature(f.id) && props.ui.setMessage("Otras operaciones dependen de esta: borrarlas primero")}
                      >
                        <Icons.Trash size={12} />
                      </IconButton>
                    </span>
                  </div>
                  <Show when={rollback() === i() + 1}>
                    <div class="h-0.5 rounded bg-warning mx-2" title="Barra de retroceso: lo de abajo no se calcula" />
                  </Show>
                </>
              );
            }}
          </For>
        </div>
        <div class="flex gap-1.5">
          <Show when={store.selected() !== undefined}>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => {
                const i = doc()!.features.findIndex((f) => f.id === store.selected());
                void store.setRollback(i + 1);
              }}
            >
              Retroceder hasta acá
            </Button>
          </Show>
          <Show when={rollback() !== null}>
            <Button size="sm" variant="ghost" onClick={() => void store.setRollback(null)}>
              Calcular todo
            </Button>
          </Show>
        </div>
      </Show>
    </Section>
  );
};

// ─── Edición de una operación ─────────────────────────────────────────────

const FeatureEditor: Component<{
  feature: Feature;
  store: CadStore;
  ui: CadUi;
  sketches: Feature[];
  tools: Feature[];
  onEdges: (kind: "fillet" | "chamfer") => void;
  onFaces: (kind: "shell" | "draft") => void;
}> = (props) => {
  const f = () => props.feature;
  const update = (mutate: (k: FeatureKind) => void) => void props.store.updateFeature(f().id, (x) => mutate(x.kind));
  /** Campo numérico vinculable: `path` relativo a la operación (p. ej. "kind.radius") */
  const field = (label: string, rel: string, raw: number, write: (k: FeatureKind, v: number) => void, suffix?: string) => {
    const path = () => `${f().id}.${rel}`;
    const res = () => props.store.bindingResult(path());
    return (
      <Formula
        label={label}
        suffix={suffix}
        value={res()?.value ?? raw}
        expr={props.store.bindingOf(path())}
        error={res()?.error}
        onCommit={(text) => props.store.setField(f().id, path(), text, (x, v) => write(x.kind, v))}
      />
    );
  };
  const state = () => props.store.stateOf(f().id);
  const sketchOptions = () => props.sketches.map((s) => ({ value: String(s.id), label: s.name }));
  const planeSpecSelect = (value: PlaneSpec, set: (p: PlaneSpec) => void) => (
    <Select
      options={[
        ...(["xy", "xz", "yz"] as const).map((k) => ({ value: k, label: PLANE_LABELS[k] })),
        ...(value.type === "face" ? [{ value: "face", label: "Cara del sólido" }] : []),
        ...(value.type === "custom" ? [{ value: "custom", label: "Plano propio" }] : []),
      ]}
      value={value.type}
      onChange={(v) => v !== value.type && (v === "xy" || v === "xz" || v === "yz") && set({ type: v })}
    />
  );
  const opSelect = (value: BodyOp, set: (o: BodyOp) => void) => <Select options={OP_OPTIONS} value={value} onChange={(v) => set(v as BodyOp)} />;

  return (
    <Section title={FEATURE_LABELS[f().kind.type]}>
      <div class="space-y-2">
        <input
          class="w-full px-2 py-1 rounded bg-surface/40 border border-border text-xs text-text outline-none focus:border-accent"
          value={f().name}
          onChange={(e) => void props.store.updateFeature(f().id, (x) => (x.name = e.currentTarget.value))}
        />
        <Show when={state()?.state === "error"}>
          <p class="text-xs text-error">{(state() as { message: string }).message}</p>
        </Show>
        <Switch>
          <Match when={f().kind.type === "sketch" && (f().kind as Extract<FeatureKind, { type: "sketch" }>)}>
            {(k) => {
              const view = () => props.store.sketchView(f().id);
              return (
                <>
                  <Row label="Plano">{planeSpecSelect(k().plane, (p) => update((x) => x.type === "sketch" && (x.plane = p)))}</Row>
                  {field("Desplazamiento", "kind.offset", k().offset, (x, v) => x.type === "sketch" && (x.offset = v), "mm")}
                  <p class="text-[11px] text-text-dim">
                    {k().sketch.entities.length} entidades · {k().sketch.constraints.length} restricciones · {view()?.regions.length ?? 0} regiones
                  </p>
                  <Button size="sm" variant="primary" fullWidth icon={<Icons.PaintBrush size={14} />} onClick={() => props.ui.editSketch(f().id)}>
                    Editar sketch
                  </Button>
                </>
              );
            }}
          </Match>
          <Match when={f().kind.type === "extrude" && (f().kind as Extract<FeatureKind, { type: "extrude" }>)}>
            {(k) => {
              const extentType = () => k().extent.type;
              const setExtent = (e: Extent) => update((x) => x.type === "extrude" && (x.extent = e));
              return (
                <>
                  <Row label="Sketch">
                    <Select options={sketchOptions()} value={String(k().sketch)} onChange={(v) => update((x) => x.type === "extrude" && (x.sketch = +v))} />
                  </Row>
                  <Row label="Hasta">
                    <Select
                      options={[
                        { value: "blind", label: "Distancia" },
                        { value: "symmetric", label: "Simétrica" },
                        { value: "through_all", label: "Atravesar todo" },
                        { value: "up_to_face", label: "Hasta una cara" },
                      ]}
                      value={extentType()}
                      onChange={(v) => {
                        if (v === extentType()) return;
                        const d = "distance" in k().extent ? (k().extent as { distance: number }).distance : 10;
                        if (v === "blind" || v === "symmetric") setExtent({ type: v, distance: d });
                        else if (v === "through_all") setExtent({ type: "through_all" });
                        else
                          props.ui.setPick({
                            kind: "face",
                            prompt: "Elegir la cara hasta donde llega la extrusión",
                            done: (face) => setExtent({ type: "up_to_face", face }),
                          });
                      }}
                    />
                  </Row>
                  <Show when={k().extent.type === "blind" || k().extent.type === "symmetric"}>
                    {field(
                      "Distancia",
                      "kind.extent.distance",
                      (k().extent as { distance: number }).distance,
                      (x, v) => x.type === "extrude" && "distance" in x.extent && (x.extent.distance = v),
                      "mm",
                    )}
                  </Show>
                  <RegionPicker
                    ui={props.ui}
                    store={props.store}
                    sketch={k().sketch}
                    value={k().regions}
                    onChange={(r) => update((x) => x.type === "extrude" && (x.regions = r))}
                  />
                  <Checkbox small label="Hacia el otro lado" checked={k().reverse} onChange={(c) => update((x) => x.type === "extrude" && (x.reverse = c))} />
                  <Row label="Con el sólido">{opSelect(k().op, (o) => update((x) => x.type === "extrude" && (x.op = o)))}</Row>
                </>
              );
            }}
          </Match>
          <Match when={f().kind.type === "revolve" && (f().kind as Extract<FeatureKind, { type: "revolve" }>)}>
            {(k) => {
              const sketch = () => props.sketches.find((s) => s.id === k().sketch);
              const lines = () => {
                const s = sketch();
                return s?.kind.type === "sketch" ? s.kind.sketch.entities.filter((e) => e.geometry.type === "line") : [];
              };
              const axisValue = () => (k().axis.type === "sketch_line" ? `line:${(k().axis as { line: number }).line}` : k().axis.type);
              return (
                <>
                  <Row label="Sketch">
                    <Select options={sketchOptions()} value={String(k().sketch)} onChange={(v) => update((x) => x.type === "revolve" && (x.sketch = +v))} />
                  </Row>
                  <Row label="Eje">
                    <Select
                      options={[
                        { value: "x", label: "X" },
                        { value: "y", label: "Y" },
                        { value: "z", label: "Z" },
                        ...lines().map((l) => ({ value: `line:${l.id}`, label: `Línea ${l.id}${l.construction ? " (construcción)" : ""}` })),
                      ]}
                      value={axisValue()}
                      onChange={(v) =>
                        update((x) => {
                          if (x.type !== "revolve") return;
                          x.axis = v.startsWith("line:") ? { type: "sketch_line", sketch: x.sketch, line: +v.slice(5) } : { type: v as "x" | "y" | "z" };
                        })
                      }
                    />
                  </Row>
                  <RegionPicker
                    ui={props.ui}
                    store={props.store}
                    sketch={k().sketch}
                    value={k().regions}
                    onChange={(r) => update((x) => x.type === "revolve" && (x.regions = r))}
                  />
                  {field("Ángulo", "kind.angle", k().angle, (x, v) => x.type === "revolve" && (x.angle = v), "°")}
                  <Row label="Con el sólido">{opSelect(k().op, (o) => update((x) => x.type === "revolve" && (x.op = o)))}</Row>
                </>
              );
            }}
          </Match>
          <Match when={f().kind.type === "primitive" && (f().kind as Extract<FeatureKind, { type: "primitive" }>)}>
            {(k) => {
              const fields: Record<string, [string, string][]> = {
                box: [["dx", "Ancho (X)"], ["dy", "Fondo (Y)"], ["dz", "Alto (Z)"]],
                cylinder: [["radius", "Radio"], ["height", "Alto"]],
                cone: [["r1", "Radio abajo"], ["r2", "Radio arriba"], ["height", "Alto"]],
                sphere: [["radius", "Radio"]],
                torus: [["major", "Radio mayor"], ["minor", "Radio menor"]],
              };
              return (
                <>
                  <For each={fields[k().shape.type]}>
                    {([key, label]) => (
                      field(label, `kind.shape.${key}`, (k().shape as unknown as Record<string, number>)[key], (x, v) => {
                        if (x.type === "primitive") (x.shape as unknown as Record<string, number>)[key] = v;
                      }, "mm")
                    )}
                  </For>
                  <div class="space-y-1">
                    <span class="text-xs text-text-muted">Posición (mm)</span>
                    <div class="grid grid-cols-3 gap-1">
                      <For each={["X", "Y", "Z"]}>
                        {(axis, i) => field(axis, `kind.origin.${i()}`, k().origin[i()], (x, v) => x.type === "primitive" && (x.origin[i()] = v))}
                      </For>
                    </div>
                  </div>
                  <Row label="Eje">
                    <Select
                      options={["x", "y", "z"].map((a) => ({ value: a, label: a.toUpperCase() }))}
                      value={Object.entries(AXIS_DIRS).find(([, d]) => d.every((c, i) => Math.abs(c - k().z[i]) < 1e-9))?.[0] ?? "z"}
                      onChange={(v) =>
                        update((x) => {
                          if (x.type !== "primitive") return;
                          x.z = AXIS_DIRS[v];
                          x.x = v === "x" ? [0, 1, 0] : [1, 0, 0];
                        })
                      }
                    />
                  </Row>
                  <Row label="Con el sólido">{opSelect(k().op, (o) => update((x) => x.type === "primitive" && (x.op = o)))}</Row>
                </>
              );
            }}
          </Match>
          <Match when={(f().kind.type === "fillet" || f().kind.type === "chamfer") && f().kind}>
            {(k) => {
              const kind = k() as Extract<FeatureKind, { type: "fillet" | "chamfer" }>;
              const value = () => (kind.type === "fillet" ? kind.radius : kind.distance);
              return (
                <>
                  {field(kind.type === "fillet" ? "Radio" : "Distancia", kind.type === "fillet" ? "kind.radius" : "kind.distance", value(), (x, v) => {
                    if (x.type === "fillet") x.radius = v;
                    else if (x.type === "chamfer") x.distance = v;
                  }, "mm")}
                  <p class="text-[11px] text-text-dim">{kind.edges.length} aristas</p>
                  <Button size="sm" fullWidth onClick={() => props.onEdges(kind.type)}>
                    Elegir aristas de nuevo
                  </Button>
                </>
              );
            }}
          </Match>
          <Match when={(f().kind.type === "shell" || f().kind.type === "draft") && f().kind}>
            {(k) => {
              const kind = k() as Extract<FeatureKind, { type: "shell" | "draft" }>;
              return (
                <>
                  <Show
                    when={kind.type === "draft" && kind}
                    fallback={
                      field("Grosor de pared", "kind.thickness", (kind as { thickness: number }).thickness, (x, v) => x.type === "shell" && (x.thickness = v), "mm")
                    }
                  >
                    {(d) => (
                      <>
                        {field("Ángulo", "kind.angle", d().angle, (x, v) => x.type === "draft" && (x.angle = v), "°")}
                        <Row label="Plano neutro">{planeSpecSelect(d().neutral, (p) => update((x) => x.type === "draft" && (x.neutral = p)))}</Row>
                      </>
                    )}
                  </Show>
                  <p class="text-[11px] text-text-dim">{kind.faces.length} caras</p>
                  <Button size="sm" fullWidth onClick={() => props.onFaces(kind.type)}>
                    Elegir caras de nuevo
                  </Button>
                </>
              );
            }}
          </Match>
          <Match when={f().kind.type === "pattern" && (f().kind as Extract<FeatureKind, { type: "pattern" }>)}>
            {(k) => (
              <>
                <ToolChecklist tools={props.tools.filter((t) => t.id !== f().id)} value={k().features} onChange={(ids) => update((x) => x.type === "pattern" && (x.features = ids))} />
                <Show
                  when={k().pattern.type === "linear" && (k().pattern as Extract<PatternKind, { type: "linear" }>)}
                  fallback={
                    <>
                      <Row label="Eje">
                        <Select
                          options={["x", "y", "z"].map((a) => ({ value: a, label: a.toUpperCase() }))}
                          value={(k().pattern as { axis: AxisSpec }).axis.type}
                          onChange={(v) => update((x) => x.type === "pattern" && x.pattern.type === "circular" && (x.pattern.axis = { type: v as "x" | "y" | "z" }))}
                        />
                      </Row>
                      {field("Cantidad", "kind.pattern.count", (k().pattern as { count: number }).count, (x, v) => x.type === "pattern" && (x.pattern.count = Math.max(2, Math.round(v))))}
                      {field("Ángulo total", "kind.pattern.angle", (k().pattern as { angle: number }).angle, (x, v) => x.type === "pattern" && x.pattern.type === "circular" && (x.pattern.angle = v), "°")}
                    </>
                  }
                >
                  {(lin) => (
                    <>
                      <Row label="Dirección">
                        <Select
                          options={["x", "y", "z"].map((a) => ({ value: a, label: a.toUpperCase() }))}
                          value={Object.entries(AXIS_DIRS).find(([, d]) => d.every((c, i) => Math.abs(c - lin().direction[i]) < 1e-9))?.[0] ?? "x"}
                          onChange={(v) => update((x) => x.type === "pattern" && x.pattern.type === "linear" && (x.pattern.direction = AXIS_DIRS[v]))}
                        />
                      </Row>
                      {field("Cantidad", "kind.pattern.count", lin().count, (x, v) => x.type === "pattern" && (x.pattern.count = Math.max(2, Math.round(v))))}
                      {field("Separación", "kind.pattern.spacing", lin().spacing, (x, v) => x.type === "pattern" && x.pattern.type === "linear" && (x.pattern.spacing = v), "mm")}
                    </>
                  )}
                </Show>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "mirror" && (f().kind as Extract<FeatureKind, { type: "mirror" }>)}>
            {(k) => (
              <>
                <ToolChecklist tools={props.tools.filter((t) => t.id !== f().id)} value={k().features} onChange={(ids) => update((x) => x.type === "mirror" && (x.features = ids))} />
                <Row label="Plano">{planeSpecSelect(k().plane, (p) => update((x) => x.type === "mirror" && (x.plane = p)))}</Row>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "split" && (f().kind as Extract<FeatureKind, { type: "split" }>)}>
            {(k) => {
              const base = (): "xy" | "xz" | "yz" => {
                const p = k().plane;
                if (p.type === "custom") {
                  const n = p.plane.normal;
                  return Math.abs(n[2]) > 0.9 ? "xy" : Math.abs(n[1]) > 0.9 ? "xz" : "yz";
                }
                return p.type === "face" ? "xy" : p.type;
              };
              const offset = () => {
                const p = k().plane;
                if (p.type !== "custom") return 0;
                const n = BASE_PLANES[base()].normal;
                return p.plane.origin[0] * n[0] + p.plane.origin[1] * n[1] + p.plane.origin[2] * n[2];
              };
              const set = (b: "xy" | "xz" | "yz", o: number) => update((x) => x.type === "split" && (x.plane = { type: "custom", plane: offsetPlane(b, o) }));
              return (
                <>
                  <Row label="Plano">
                    <Select options={(["xy", "xz", "yz"] as const).map((v) => ({ value: v, label: PLANE_LABELS[v] }))} value={base()} onChange={(v) => set(v as "xy", offset())} />
                  </Row>
                  <Num label="A distancia" suffix="mm" value={offset()} onCommit={(v) => set(base(), v)} />
                  <Checkbox small label="Conservar el otro lado" checked={k().flip} onChange={(c) => update((x) => x.type === "split" && (x.flip = c))} />
                </>
              );
            }}
          </Match>
          <Match when={f().kind.type === "import" && (f().kind as Extract<FeatureKind, { type: "import" }>)}>
            {(k) => (
              <>
                <p class="text-[11px] text-text-dim">{k().format.toUpperCase()} · {(k().data.length / 1024).toFixed(0)} KB</p>
                <Row label="Con el sólido">{opSelect(k().op, (o) => update((x) => x.type === "import" && (x.op = o)))}</Row>
              </>
            )}
          </Match>
        </Switch>
        <p class="text-[11px] text-text-dim">{planeLabelFor(f())}</p>
      </div>
    </Section>
  );
};

function planeLabelFor(f: Feature): string {
  return f.kind.type === "sketch" ? `Sobre: ${planeLabel(f.kind.plane)}` : "";
}

/** Qué regiones del sketch usar: todas o las elegidas con clic en el visor */
const RegionPicker: Component<{ ui: CadUi; store: CadStore; sketch: number; value: RegionSelection; onChange: (r: RegionSelection) => void }> = (props) => {
  const [draft, setDraft] = createSignal<P2[]>();
  const picking = () => draft() !== undefined;
  const start = () => {
    const initial = props.value.type === "points" ? [...props.value.points] : [];
    setDraft(initial);
    props.ui.setPick({
      kind: "region",
      sketch: props.sketch,
      prompt: "Clic dentro de las regiones a usar (otra vez para quitarla); confirmar en el panel",
      chosen: () => draft() ?? [],
      toggle: (p) => {
        const view = props.store.sketchView(props.sketch);
        if (!view) return;
        // Región más interna bajo el clic; si ya había un punto en ella, se quita
        const hit = [...view.regions].filter((r) => regionContains(r, p)).sort((a, b) => b.depth - a.depth)[0];
        if (!hit) return;
        setDraft((d) => {
          const list = d ?? [];
          const inside = list.filter((q) => regionContains(hit, q) && !view.regions.some((o) => o.depth > hit.depth && regionContains(o, q)));
          return inside.length ? list.filter((q) => !inside.includes(q)) : [...list, hit.sample];
        });
      },
    });
  };
  const confirm = () => {
    const d = draft() ?? [];
    props.ui.cancelPick();
    setDraft(undefined);
    props.onChange(d.length ? { type: "points", points: d } : { type: "all" });
  };
  return (
    <div class="space-y-1.5">
      <Row label="Regiones">
        <span class="text-xs text-text">{props.value.type === "all" ? "Todas" : `${props.value.points.length} elegidas`}</span>
      </Row>
      <Show
        when={picking()}
        fallback={
          <div class="flex gap-1.5">
            <Button size="sm" onClick={start}>
              Elegir en el visor
            </Button>
            <Show when={props.value.type === "points"}>
              <Button size="sm" variant="ghost" onClick={() => props.onChange({ type: "all" })}>
                Usar todas
              </Button>
            </Show>
          </div>
        }
      >
        <div class="flex gap-1.5">
          <Button size="sm" variant="primary" onClick={confirm}>
            Listo ({draft()?.length ?? 0})
          </Button>
          <Button size="sm" variant="ghost" onClick={() => (props.ui.cancelPick(), setDraft(undefined))}>
            Cancelar
          </Button>
        </div>
      </Show>
    </div>
  );
};

const ToolChecklist: Component<{ tools: Feature[]; value: number[]; onChange: (ids: number[]) => void }> = (props) => (
  <div class="space-y-1">
    <span class="text-xs text-text-muted">Qué repetir (nada = todo el sólido)</span>
    <For each={props.tools} fallback={<p class="text-[11px] text-text-dim">No hay operaciones con herramienta</p>}>
      {(t) => (
        <Checkbox
          small
          label={t.name}
          checked={props.value.includes(t.id)}
          onChange={(c) => props.onChange(c ? [...props.value, t.id] : props.value.filter((x) => x !== t.id))}
        />
      )}
    </For>
  </div>
);

// ─── Escaneo ──────────────────────────────────────────────────────────────

const ScanResultCard: Component<{ result: { pick: ScanPick; triangle: number }; onAdd: (f: Parameters<CadStore["scanAdd"]>[0]) => void }> = (props) => {
  const [depth, setDepth] = createSignal<number>();
  const [op, setOp] = createSignal<BodyOp>("join");
  return (
    <div class="rounded-md border border-cyan/50 p-2 space-y-2 text-xs">
      <Show
        when={props.result.pick.kind === "plane" && props.result.pick}
        fallback={(() => {
          const c = props.result.pick as Extract<ScanPick, { kind: "cylinder" }>;
          return (
            <>
              <p class="text-text">
                {c.hole ? "Agujero" : "Cilindro"} · radio {fmt(c.radius, 3)} mm · largo {fmt(c.length)} mm
              </p>
              <p class="text-text-dim">
                Error {fmt(c.rms, 3)} mm · cubre {fmt(c.coverage * 100, 0)} % de la vuelta
              </p>
              <Button size="sm" variant="primary" fullWidth onClick={() => props.onAdd({ kind: "cylinder", triangle: props.result.triangle })}>
                {c.hole ? "Agregar el agujero (resta)" : "Agregar el cilindro"}
              </Button>
            </>
          );
        })()}
      >
        {(p) => (
          <>
            <p class="text-text">
              Plano · área {fmt(p().area, 1)} mm² · {p().boundary.length} contornos
            </p>
            <p class="text-text-dim">
              Error {fmt(p().rms, 3)} mm · espesor medido {p().depth != null ? `${fmt(p().depth!)} mm` : "no se pudo medir"}
            </p>
            <Num label="Profundidad" suffix="mm" value={depth() ?? p().depth ?? 10} onCommit={setDepth} />
            <Row label="Con el sólido">
              <Select options={OP_OPTIONS} value={op()} onChange={(v) => setOp(v as BodyOp)} />
            </Row>
            <div class="grid grid-cols-2 gap-1.5">
              <Button size="sm" onClick={() => props.onAdd({ kind: "plane_outline", triangle: props.result.triangle, extrude: false })}>
                Solo el sketch
              </Button>
              <Button
                size="sm"
                variant="primary"
                onClick={() => props.onAdd({ kind: "plane_outline", triangle: props.result.triangle, extrude: true, depth: depth() ?? p().depth ?? 10, op: op() })}
              >
                Sketch y extruir
              </Button>
              <Button size="sm" variant="ghost" onClick={() => props.onAdd({ kind: "work_plane", triangle: props.result.triangle })}>
                Plano de trabajo
              </Button>
            </div>
          </>
        )}
      </Show>
    </div>
  );
};

// ─── Sólido ───────────────────────────────────────────────────────────────

const BodySection: Component<{ store: CadStore; onExport: (f: "step" | "stl" | "3mf") => void; onUseAsModel?: () => void }> = (props) => (
  <Show when={props.store.result()?.body}>
    {(b) => {
      const size = () => b().bbox_max.map((v, i) => v - b().bbox_min[i]);
      return (
        <Section title="Sólido">
          <div class="space-y-1 text-xs">
            <div class="flex justify-between">
              <span class="text-text-muted">Volumen</span>
              <span class="font-mono text-text">{fmt(b().volume / 1000)} cm³</span>
            </div>
            <div class="flex justify-between">
              <span class="text-text-muted">Área</span>
              <span class="font-mono text-text">{fmt(b().area / 100)} cm²</span>
            </div>
            <div class="flex justify-between">
              <span class="text-text-muted">Tamaño (X × Y × Z)</span>
              <span class="font-mono text-text">{size().map((v) => fmt(v, 1)).join(" × ")} mm</span>
            </div>
            <div class="flex justify-between">
              <span class="text-text-muted">Caras / aristas</span>
              <span class="font-mono text-text">
                {b().faces} / {b().edges}
              </span>
            </div>
            <Show when={!b().valid}>
              <p class="text-warning">La forma tiene defectos: puede fallar al exportar o imprimir</p>
            </Show>
          </div>
          <div class="grid grid-cols-3 gap-1.5">
            <Button size="sm" onClick={() => props.onExport("step")}>
              STEP
            </Button>
            <Button size="sm" onClick={() => props.onExport("stl")}>
              STL
            </Button>
            <Button size="sm" onClick={() => props.onExport("3mf")}>
              3MF
            </Button>
          </div>
          <Show when={props.onUseAsModel}>
            <Button size="sm" fullWidth variant="ghost" onClick={() => props.onUseAsModel?.()}>
              Usar como modelo (fabricar, pintar, animar)
            </Button>
          </Show>
        </Section>
      );
    }}
  </Show>
);

// ─── Sketch en edición ────────────────────────────────────────────────────

const SketchPanel: Component<{ ui: CadUi }> = (props) => {
  const ui = props.ui;
  const [cornerRadius, setCornerRadius] = createSignal(5);
  const [offsetDist, setOffsetDist] = createSignal(2);
  const s = () => ui.session()!;
  const sketch = (): Sketch => s().sketch;
  const entity = (id: number) => sketch().entities.find((e) => e.id === id);
  const point = (id: number) => sketch().points.find((p) => p.id === id);
  const sel = () => ui.selection();
  const selEntities = () => sel().map(entity).filter((e) => e !== undefined);
  const selPoints = () => sel().filter((id) => point(id));
  const lines = () => selEntities().filter((e) => e!.geometry.type === "line");
  const curves = () => selEntities().filter((e) => e!.geometry.type === "circle" || e!.geometry.type === "arc");

  const lineLength = (id: number) => {
    const g = entity(id)!.geometry as { start: number; end: number };
    const [a, b] = [point(g.start)!, point(g.end)!];
    return Math.hypot(b.x - a.x, b.y - a.y);
  };
  const radius = (id: number) => {
    const g = entity(id)!.geometry;
    if (g.type === "circle") return g.radius;
    if (g.type === "arc") {
      const [c, a] = [point(g.center)!, point(g.start)!];
      return Math.hypot(a.x - c.x, a.y - c.y);
    }
    return 0;
  };
  const add = (c: SketchConstraint) => ui.addConstraint(c);

  const suggestions = (): { label: string; make: () => SketchConstraint }[] => {
    const out: { label: string; make: () => SketchConstraint }[] = [];
    const L = lines().map((e) => e!.id);
    const C = curves().map((e) => e!.id);
    const P = selPoints();
    if (L.length === 1 && C.length === 0 && P.length === 0) {
      out.push({ label: "Horizontal", make: () => ({ type: "horizontal", line: L[0] }) });
      out.push({ label: "Vertical", make: () => ({ type: "vertical", line: L[0] }) });
      out.push({ label: "Largo", make: () => ({ type: "length", line: L[0], value: +lineLength(L[0]).toFixed(3) }) });
    }
    if (L.length === 2) {
      out.push({ label: "Paralelas", make: () => ({ type: "parallel", a: L[0], b: L[1] }) });
      out.push({ label: "Perpendiculares", make: () => ({ type: "perpendicular", a: L[0], b: L[1] }) });
      out.push({ label: "Mismo largo", make: () => ({ type: "equal", a: L[0], b: L[1] }) });
      out.push({ label: "Ángulo", make: () => ({ type: "angle", a: L[0], b: L[1], degrees: 90 }) });
    }
    if (C.length === 1 && L.length === 0) {
      out.push({ label: "Radio", make: () => ({ type: "radius", entity: C[0], value: +radius(C[0]).toFixed(3) }) });
      out.push({ label: "Diámetro", make: () => ({ type: "diameter", entity: C[0], value: +(2 * radius(C[0])).toFixed(3) }) });
    }
    if (C.length === 2) out.push({ label: "Mismo radio", make: () => ({ type: "equal", a: C[0], b: C[1] }) });
    if (L.length === 1 && C.length === 1) out.push({ label: "Tangente", make: () => ({ type: "tangent", a: L[0], b: C[0] }) });
    if (P.length === 1 && L.length === 1) {
      out.push({ label: "Punto en la línea", make: () => ({ type: "point_on_line", point: P[0], line: L[0] }) });
      out.push({ label: "Punto medio", make: () => ({ type: "midpoint", point: P[0], line: L[0] }) });
    }
    if (P.length === 1 && C.length === 1) out.push({ label: "Punto en el círculo", make: () => ({ type: "point_on_circle", point: P[0], circle: C[0] }) });
    if (P.length === 2 && L.length === 0) {
      const [a, b] = [point(P[0])!, point(P[1])!];
      out.push({ label: "Coincidentes", make: () => ({ type: "coincident", a: P[0], b: P[1] }) });
      out.push({ label: "Distancia", make: () => ({ type: "distance", a: P[0], b: P[1], value: +Math.hypot(b.x - a.x, b.y - a.y).toFixed(3) }) });
      out.push({ label: "Distancia horizontal", make: () => ({ type: "horizontal_distance", a: P[0], b: P[1], value: +(b.x - a.x).toFixed(3) }) });
      out.push({ label: "Distancia vertical", make: () => ({ type: "vertical_distance", a: P[0], b: P[1], value: +(b.y - a.y).toFixed(3) }) });
    }
    if (P.length === 2 && L.length === 1) out.push({ label: "Simétricos", make: () => ({ type: "symmetric", a: P[0], b: P[1], line: L[0] }) });
    if (P.length === 1 && L.length === 0 && C.length === 0) {
      const p = point(P[0])!;
      out.push({ label: "Fijar", make: () => ({ type: "fixed", point: P[0], x: p.x, y: p.y }) });
    }
    return out;
  };

  return (
    <div class="space-y-4">
      <Section title="Sketch">
        <p class="text-[11px] text-text-dim leading-relaxed">
          Dibujar en el visor. Con «Elegir» se marcan puntos y líneas (Mayús suma) para restringirlos; los puntos se arrastran y el sketch se
          recalcula respetando las medidas.
        </p>
        <Show when={s().report}>
          {(r) => (
            <p class={clsx("text-xs", r().status === "well_constrained" ? "text-success" : r().status === "under_constrained" ? "text-text-muted" : "text-error")}>
              {r().status === "well_constrained"
                ? "Totalmente definido"
                : r().status === "under_constrained"
                  ? `${r().dof} grados de libertad`
                  : r().status === "over_constrained"
                    ? "Hay restricciones en conflicto (marcadas en rojo)"
                    : "No se pudo resolver"}
              {" · "}
              {s().regions.length} regiones cerradas
            </p>
          )}
        </Show>
      </Section>

      <Section title={`Elegido (${sel().length})`}>
        <Show when={sel().length > 0} fallback={<p class="text-xs text-text-dim">Nada elegido</p>}>
          <div class="flex flex-wrap gap-1.5">
            <For each={suggestions()}>
              {(sug) => (
                <Button size="sm" onClick={() => add(sug.make())}>
                  {sug.label}
                </Button>
              )}
            </For>
          </div>
          <Show when={selPoints().length === 1 && linesAt(sketch(), selPoints()[0]).length === 2}>
            <div class="flex items-end gap-1.5">
              <div class="flex-1">
                <Num label="Radio" suffix="mm" step={0.5} value={cornerRadius()} onCommit={setCornerRadius} />
              </div>
              <Button
                size="sm"
                onClick={() => {
                  let msg: string | undefined;
                  ui.change((s) => (msg = filletCorner(s, selPoints()[0], cornerRadius())));
                  if (msg) ui.setMessage(msg);
                  else ui.setSelection([]);
                }}
              >
                Redondear esquina
              </Button>
            </div>
          </Show>
          <Show when={selEntities().length > 0}>
            <div class="flex items-end gap-1.5">
              <div class="flex-1">
                <Num label="Distancia" suffix="mm" step={0.5} value={offsetDist()} onCommit={setOffsetDist} />
              </div>
              <Button
                size="sm"
                title="Positivo: hacia afuera del lazo"
                onClick={() => {
                  let msg: string | undefined;
                  const regions = s().regions;
                  const ids = selEntities().map((e) => e!.id);
                  ui.change((sk) => (msg = offsetEntities(sk, regions, ids, offsetDist())));
                  ui.setMessage(msg);
                }}
              >
                Equidistante
              </Button>
            </div>
          </Show>
          <div class="flex gap-1.5">
            <Button size="sm" variant="ghost" onClick={() => ui.toggleConstruction()} disabled={selEntities().length === 0}>
              Construcción sí/no
            </Button>
            <Button size="sm" variant="danger" icon={<Icons.Trash size={12} />} onClick={() => ui.deleteSelection()} disabled={selEntities().length === 0}>
              Borrar
            </Button>
          </div>
        </Show>
      </Section>

      <Section title="Restricciones">
        <Show when={sketch().constraints.length > 0} fallback={<p class="text-xs text-text-dim">Sin restricciones</p>}>
          <div class="space-y-0.5 max-h-72 overflow-y-auto">
            <For each={sketch().constraints}>
              {(c, i) => {
                const conflict = () => s().report?.conflicting.includes(i()) ?? false;
                const value = () => constraintValue(c);
                return (
                  <div
                    class={clsx("flex items-center gap-2 px-1.5 py-0.5 rounded text-xs hover:bg-surface", conflict() ? "bg-error/15 text-error" : "text-text-muted")}
                    onMouseEnter={() => ui.setHoverIds(constraintIds(c))}
                    onMouseLeave={() => ui.setHoverIds([])}
                  >
                    <button
                      class="flex-1 truncate text-left hover:text-text"
                      title="Elegir lo que restringe"
                      onClick={() => ui.setSelection(constraintIds(c))}
                    >
                      {CONSTRAINT_LABELS[c.type]}
                    </button>
                    <Show when={value() !== undefined}>
                      <div class="w-28">
                        <Formula
                          value={value()!}
                          expr={(c as { expr?: string }).expr}
                          onCommit={(text) => ui.setConstraintText(i(), text)}
                        />
                      </div>
                    </Show>
                    <IconButton aria-label="Quitar restricción" size="sm" variant="ghost" onClick={() => ui.removeConstraint(i())}>
                      <Icons.X size={10} />
                    </IconButton>
                  </div>
                );
              }}
            </For>
          </div>
        </Show>
      </Section>

      <div class="flex gap-2">
        <Button size="sm" variant="primary" onClick={() => void ui.finishSketch()}>
          Terminar sketch
        </Button>
        <Button size="sm" variant="ghost" onClick={() => ui.cancelSketch()}>
          Descartar cambios
        </Button>
      </div>
    </div>
  );
};
