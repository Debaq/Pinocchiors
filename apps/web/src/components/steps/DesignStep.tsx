import { Component, For, Match, Show, Switch, createEffect, createMemo, createSignal, on, onCleanup, untrack, type JSX } from "solid-js";
import { open, save } from "@tauri-apps/plugin-dialog";
import { clsx } from "clsx";
import {
  BASE_PLANES,
  CONSTRAINT_LABELS,
  FEATURE_LABELS,
  MATERIALS,
  OP_LABELS,
  PLANE_LABELS,
  constraintIds,
  constraintValue,
  isReference,
  emptySketch,
  filletCorner,
  circularPattern,
  linearPattern,
  mirrorEntities,
  toggleSplineHandles,
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
  type FeatureState,
  folderRange,
  type Material,
  type MissingRef,
  type P2,
  type P3,
  type PatternKind,
  type PrimitiveShape,
  type RegionSelection,
  type PlaneSpec,
  type ScanPick,
  type Sketch,
  type SketchConstraint,
} from "../../lib/cad";
import type { CadUi } from "../../lib/cadUi";
import { regionContains } from "../../lib/CadViewer";
import { Button, Checkbox, IconButton, Select, Tooltip } from "../ui";
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

const Section: Component<{ title: string; children: JSX.Element; right?: JSX.Element; class?: string }> = (props) => (
  <div class={clsx("space-y-2", props.class)}>
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

function pickSummary(picks: { kind: string }[]): string {
  const count = (k: string) => picks.filter((p) => p.kind === k).length;
  const parts: [number, string, string][] = [
    [count("region"), "región", "regiones"],
    [count("face"), "cara", "caras"],
    [count("edge"), "arista", "aristas"],
    [count("vertex"), "vértice", "vértices"],
    [count("plane"), "plano", "planos"],
  ];
  return parts.filter(([n]) => n > 0).map(([n, one, many]) => `${n} ${n === 1 ? one : many}`).join(", ");
}

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

  /** Nuevo sketch en el plano o la cara elegida; sin elegir, se pide dónde */
  const startSketch = async () => {
    const picks = ui.picks();
    const plane = picks.find((p) => p.kind === "plane");
    if (plane?.kind === "plane") {
      ui.clearPicks();
      return newSketch({ type: plane.plane });
    }
    const face = picks.find((p) => p.kind === "face");
    if (face?.kind === "face") {
      const ref = await store.faceRef(face.face);
      ui.clearPicks();
      return newSketch({ type: "face", face: ref });
    }
    ui.setPick({ kind: "place", prompt: "Elegir dónde va el sketch: un plano base o una cara plana", done: (spec) => void newSketch(spec) });
  };

  /** Regiones elegidas en el visor, del primer sketch que tenga alguna */
  const pickedRegions = (): { sketch: number; points: P2[] } | undefined => {
    const regs = ui.picks().flatMap((p) => (p.kind === "region" ? [p] : []));
    if (!regs.length) return undefined;
    const sketch = regs[0].sketch;
    const view = store.sketchView(sketch);
    if (!view) return undefined;
    const points = regs.filter((r) => r.sketch === sketch).map((r) => view.regions[r.region]?.sample).filter((p): p is P2 => !!p);
    return { sketch, points };
  };

  /** Sketch al que apuntan extrusiones y revoluciones nuevas: el elegido o el último */
  const targetSketch = (): Feature | undefined => {
    const sel = selectedFeature();
    if (sel?.kind.type === "sketch") return sel;
    return [...sketches()].pop();
  };

  const addExtrude = () => {
    // Con regiones elegidas en el visor se extruyen esas; si no, todo el sketch
    const picked = pickedRegions();
    const s = picked ? sketches().find((f) => f.id === picked.sketch) : targetSketch();
    if (!s) return say("Primero hace falta un sketch con una región cerrada");
    ui.clearPicks();
    void store.addFeature({
      type: "extrude",
      sketch: s.id,
      regions: picked ? { type: "points", points: picked.points } : { type: "all" },
      extent: { type: "blind", distance: 10 },
      reverse: false,
      op: "join",
    });
  };

  const addRevolve = () => {
    const picked = pickedRegions();
    const s = picked ? sketches().find((f) => f.id === picked.sketch) : targetSketch();
    if (!s || s.kind.type !== "sketch") return say("Primero hace falta un sketch con una región cerrada");
    ui.clearPicks();
    // Eje: la primera línea de construcción del sketch, o Z
    const axisLine = s.kind.sketch.entities.find((e) => e.construction && e.geometry.type === "line");
    const axis: AxisSpec = axisLine ? { type: "sketch_line", sketch: s.id, line: axisLine.id } : { type: "z" };
    const regions: RegionSelection = picked ? { type: "points", points: picked.points } : { type: "all" };
    void store.addFeature({ type: "revolve", sketch: s.id, regions, axis, angle: 360, op: "join" });
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

  /** Redondeo o chaflán con las aristas ya elegidas; si no hay, se eligen en la caja del diálogo */
  const startEdges = async (kind: "fillet" | "chamfer") => {
    if (!store.result()?.body) return say("Primero hace falta un sólido");
    const picked = ui.picks().flatMap((p) => (p.kind === "edge" ? [p.edge] : []));
    const edges = await Promise.all(picked.map((e) => store.edgeRef(e)));
    ui.clearPicks();
    if (kind === "fillet") void store.addFeature({ type: "fillet", edges, radius: 1 });
    else void store.addFeature({ type: "chamfer", edges, distance: 1 });
  };

  /** Vaciado o desmolde con las caras ya elegidas; si no hay, se eligen en la caja del diálogo */
  const startFaces = async (kind: "shell" | "draft") => {
    if (!store.result()?.body) return say("Primero hace falta un sólido");
    const picked = ui.picks().flatMap((p) => (p.kind === "face" ? [p.face] : []));
    const faces = await Promise.all(picked.map((f) => store.faceRef(f)));
    ui.clearPicks();
    if (kind === "shell") void store.addFeature({ type: "shell", faces, thickness: 1 });
    else void store.addFeature({ type: "draft", faces, neutral: { type: "xy" }, angle: 3 });
  };

  // El menú del visor las usa con lo elegido
  ui.setActions({
    sketch: () => void startSketch(),
    fillet: () => void startEdges("fillet"),
    chamfer: () => void startEdges("chamfer"),
    shell: () => void startFaces("shell"),
    draft: () => void startFaces("draft"),
  });
  onCleanup(() => ui.setActions({}));

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
              <Show when={ui.picks().length > 0}>
                <div class="flex items-center justify-between rounded-md border border-cyan/40 bg-cyan/5 px-2 py-1.5 text-xs">
                  <span class="text-text">Elegido: {pickSummary(ui.picks())}</span>
                  <button class="text-text-muted hover:text-text" onClick={() => ui.clearPicks()}>
                    Limpiar (Esc)
                  </button>
                </div>
              </Show>
              <AddSection
                onSketch={() => void startSketch()}
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

              {/* Por id: el editor no se rehace con cada cambio del documento */}
              <For each={selectedFeature() ? [selectedFeature()!.id] : []}>
                {(id) => (
                  <FeatureEditor
                    featureId={id}
                    store={store}
                    ui={ui}
                    sketches={sketches()}
                    tools={toolFeatures()}
                  />
                )}
              </For>

              <ParametersSection store={store} />

              <FeatureTree store={store} ui={ui} />


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

              <BodySection store={store} ui={ui} onExport={(f) => void exportAs(f)} onUseAsModel={props.onUseAsModel} />

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
  onSketch: () => void;
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
        <div class="grid grid-cols-3 gap-1.5">
          <B label="Sketch" onClick={props.onSketch} title="En el plano o la cara elegida; si no hay nada elegido, se elige en el visor" />
          <B label="Extrusión" onClick={props.onExtrude} title="Las regiones elegidas en el visor; si no hay, todo el último sketch" />
          <B label="Revolución" onClick={props.onRevolve} title="Las regiones elegidas; eje: la primera línea de construcción del sketch, o Z" />
        </div>
        <div class="grid grid-cols-3 gap-1.5">
          <B label="Caja" onClick={() => props.onPrimitive({ type: "box", dx: 20, dy: 20, dz: 20, centered: true, centered_z: true })} />
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

/** Duración corta: "120 ms", "1,4 s" */
const formatMs = (ms: number) => (ms < 1000 ? `${Math.round(ms)} ms` : `${(ms / 1000).toLocaleString("es", { maximumFractionDigits: 1 })} s`);

/** Mensaje de error o advertencia de una operación (nada si está bien) */
const problem = (st: FeatureState | undefined) => (st?.state === "error" || st?.state === "warning" ? st.message : undefined);

/**
 * Arrastre vertical dentro del árbol: devuelve el hueco bajo el puntero (0 =
 * antes de la primera fila, n = después de la última). Un movimiento de menos
 * de 4 px es un clic.
 */
type TreeDrag = { kind: "feature"; id: number; from: number; to: number } | { kind: "rollback"; to: number };

const FeatureTree: Component<{ store: CadStore; ui: CadUi }> = (props) => {
  const store = props.store;
  const doc = () => store.doc();
  const count = () => doc()?.features.length ?? 0;
  const rollback = () => doc()?.rollback ?? null;
  /** Hueco donde está la barra de retroceso (al final si no hay) */
  const barAt = () => rollback() ?? count();
  let list: HTMLDivElement | undefined;
  const [drag, setDrag] = createSignal<TreeDrag>();
  // Tras arrastrar una fila, el click que sigue no la abre
  let swallowClick = false;
  const gapAt = (y: number) => {
    // Las filas de una carpeta plegada no se ven: cuentan como su cabecera
    let ref = -Infinity;
    let n = 0;
    for (const el of list?.querySelectorAll<HTMLElement>("[data-feature-row], [data-folder]") ?? []) {
      const b = el.getBoundingClientRect();
      const visible = !("hidden" in el.dataset);
      if (visible) ref = b.top + b.height / 2;
      if ("featureRow" in el.dataset && ref < y) n++;
    }
    return n;
  };
  /** Sigue el puntero hasta soltar; `start` decide qué se arrastra al pasar el umbral */
  const track = (e: PointerEvent, start: () => TreeDrag, drop: (d: TreeDrag) => void) => {
    if (e.button !== 0) return;
    const y0 = e.clientY;
    let moving = false;
    const move = (ev: PointerEvent) => {
      if (!moving && Math.abs(ev.clientY - y0) < 4) return;
      moving = true;
      setDrag({ ...start(), to: gapAt(ev.clientY) });
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      const d = drag();
      setDrag(undefined);
      if (moving && d) {
        swallowClick = true;
        setTimeout(() => (swallowClick = false), 0);
        drop(d);
      }
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };
  const dropFeature = (d: TreeDrag) => {
    if (d.kind !== "feature") return;
    if (!store.moveFeatureTo(d.id, d.to)) props.ui.setMessage("No se puede: quedaría antes de lo que necesita o después de lo que la usa");
  };
  const dropBar = (d: TreeDrag) => void store.setRollback(d.to >= count() ? null : d.to);
  // Carpetas: tramo [desde, hasta] de cada una y en cuál está cada fila
  const folders = createMemo(() => {
    const d = doc();
    return d ? (d.folders ?? []).map((f, index) => ({ f, index, range: folderRange(d, f) })).filter((x) => x.range) : [];
  });
  const folderAt = (i: number) => folders().find((x) => x.range![0] <= i && i <= x.range![1]);
  // Mayús+clic: tramo de filas desde la elegida (para agrupar)
  const [rangeTo, setRangeTo] = createSignal<number>();
  const selRange = (): [number, number] | null => {
    const d = doc();
    const a = d?.features.findIndex((f) => f.id === store.selected()) ?? -1;
    if (a < 0) return null;
    const b = rangeTo() !== undefined ? d!.features.findIndex((f) => f.id === rangeTo()) : a;
    return b < 0 ? [a, a] : a <= b ? [a, b] : [b, a];
  };
  const inRange = (i: number) => {
    const r = selRange();
    return rangeTo() !== undefined && r !== null && r[0] <= i && i <= r[1];
  };
  const [renaming, setRenaming] = createSignal<number>();
  /** Dónde se dibuja la línea del hueco mientras se arrastra */
  const dropGap = () => {
    const d = drag();
    if (!d) return null;
    if (d.kind === "feature" && (d.to === d.from || d.to === d.from + 1)) return null;
    return d.to;
  };
  // La barra: en su hueco, o donde se la está llevando
  const barGap = () => {
    const d = drag();
    return d?.kind === "rollback" ? d.to : barAt();
  };
  const Gap = (p: { index: number }) => (
    <>
      <Show when={dropGap() === p.index && drag()?.kind === "feature"}>
        <div class="h-0.5 rounded bg-accent mx-1" />
      </Show>
      <Show when={barGap() === p.index}>
        <div
          aria-label="Barra de retroceso"
          data-rollback-bar
          class="group/bar relative py-1 cursor-ns-resize touch-none"
          title={rollback() === null ? "Arrastrar hacia arriba para ver el diseño en un paso anterior" : "Barra de retroceso: lo de abajo no se calcula y lo nuevo se agrega acá"}
          onPointerDown={(e) => {
            e.preventDefault();
            track(e, () => ({ kind: "rollback", to: barAt() }), dropBar);
          }}
        >
          <div
            class={clsx(
              "h-1 rounded mx-1 transition-colors",
              rollback() !== null || drag()?.kind === "rollback" ? "bg-warning" : "bg-border group-hover/bar:bg-warning/60",
            )}
          />
        </div>
      </Show>
    </>
  );
  return (
    <Section title="Operaciones">
      <Show when={count() > 0} fallback={<p class="text-xs text-text-dim">Todavía no hay operaciones</p>}>
        <div class="space-y-0.5" ref={list}>
          {/* Por id: las filas no se rehacen cuando el documento se clona (un
              clic entre mousedown y mouseup se perdería) */}
          <For each={doc()?.features.map((x) => x.id) ?? []}>
            {(id, i) => {
              const f = () => doc()?.features[i()] ?? ({ id, name: "", suppressed: false, kind: { type: "sketch" } } as unknown as Feature);
              const state = () => store.stateOf(f().id);
              const rolled = () => (rollback() !== null && i() >= rollback()!) || state()?.state === "rolled_back";
              const folder = () => folderAt(i());
              const hidden = () => !!folder()?.f.collapsed && folder()!.range![0] !== undefined;
              return (
                <>
                  <Show when={folder() && folder()!.range![0] === i() && folder()}>
                    {(fo) => (
                      <div
                        data-folder
                        class="group flex items-center gap-1.5 px-1 py-1 rounded text-xs text-text hover:bg-surface cursor-pointer select-none"
                        onClick={() => void store.updateFolder(fo().index, { collapsed: !fo().f.collapsed })}
                      >
                        <Icons.CaretDown size={10} class={clsx("shrink-0 transition-transform", fo().f.collapsed && "-rotate-90")} />
                        <Icons.Folder size={13} class="shrink-0 text-text-muted" />
                        <Show
                          when={renaming() === fo().index}
                          fallback={
                            <span class="flex-1 truncate" onDblClick={(e) => (e.stopPropagation(), setRenaming(fo().index))}>
                              {fo().f.name}
                            </span>
                          }
                        >
                          <input
                            aria-label="Nombre de la carpeta"
                            class="flex-1 min-w-0 px-1 rounded bg-surface/40 border border-accent text-xs text-text outline-none"
                            value={fo().f.name}
                            ref={(el) => setTimeout(() => el.select())}
                            onClick={(e) => e.stopPropagation()}
                            onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
                            onBlur={(e) => {
                              const name = e.currentTarget.value.trim();
                              setRenaming(undefined);
                              if (name && name !== fo().f.name) void store.updateFolder(fo().index, { name });
                            }}
                          />
                        </Show>
                        {/* Plegada: avisa si algo de adentro tiene problemas */}
                        <Show
                          when={
                            fo().f.collapsed &&
                            doc()!
                              .features.slice(fo().range![0], fo().range![1] + 1)
                              .some((x) => problem(store.stateOf(x.id)))
                          }
                        >
                          <Icons.Warning size={12} class="text-warning shrink-0" aria-label="Algo de la carpeta tiene problemas" />
                        </Show>
                        <span class="hidden group-hover:flex" onClick={(e) => e.stopPropagation()}>
                          <IconButton aria-label="Desagrupar" size="sm" variant="ghost" onClick={() => void store.updateFolder(fo().index, null)}>
                            <Icons.X size={10} />
                          </IconButton>
                        </span>
                      </div>
                    )}
                  </Show>
                  <Gap index={i()} />
                  <div
                    data-feature-row
                    data-hidden={hidden() ? "" : undefined}
                    class={clsx(
                      "group flex items-center gap-1.5 px-2 py-1 rounded text-xs cursor-pointer select-none",
                      folder() && "ml-3",
                      hidden() && "hidden",
                      store.selected() === f().id || inRange(i()) ? "bg-accent/20 text-text" : "hover:bg-surface text-text-muted",
                      (f().suppressed || rolled()) && "opacity-50",
                      drag()?.kind === "feature" && (drag() as { id: number }).id === f().id && "ring-1 ring-accent",
                    )}
                    onPointerDown={(e) => track(e, () => ({ kind: "feature", id: f().id, from: i(), to: i() }), dropFeature)}
                    onClick={(e) => {
                      if (swallowClick) return;
                      // Mayús: tramo desde la elegida, sin abrir el diálogo
                      if (e.shiftKey && store.selected() !== undefined && store.selected() !== f().id) return setRangeTo(f().id);
                      setRangeTo(undefined);
                      store.select(store.selected() === f().id ? undefined : f().id);
                    }}
                    onDblClick={() => f().kind.type === "sketch" && void store.settled().then(() => props.ui.editSketch(f().id))}
                    title={problem(state())}
                  >
                    <Show
                      when={problem(state())}
                      fallback={<span class="w-3.5 text-center text-text-dim">{FEATURE_LABELS[f().kind.type][0]}</span>}
                    >
                      <Icons.Warning
                        size={14}
                        aria-label={state()?.state === "warning" ? "Advertencia" : "Error"}
                        class={clsx("shrink-0", state()?.state === "warning" ? "text-warning" : "text-error")}
                      />
                    </Show>
                    <span class={clsx("flex-1 truncate", f().suppressed && "line-through")}>{f().name}</span>
                    {/* Las lentas dicen cuánto tardan (para saber qué frena el diseño) */}
                    <Show when={(state()?.ms ?? 0) >= 100}>
                      <span class="text-[10px] font-mono text-text-dim group-hover:hidden" title="Lo que tardó la última vez que se calculó">
                        {formatMs(state()!.ms!)}
                      </span>
                    </Show>
                    <span class="hidden group-hover:flex items-center gap-0.5" onClick={(e) => e.stopPropagation()}>
                      <IconButton aria-label="Subir" size="sm" variant="ghost" onClick={() => !store.moveFeature(f().id, -1) && props.ui.setMessage("No se puede: quedaría antes de lo que necesita")}>
                        <Icons.CaretDown size={10} class="rotate-180" />
                      </IconButton>
                      <IconButton aria-label="Bajar" size="sm" variant="ghost" onClick={() => !store.moveFeature(f().id, 1) && props.ui.setMessage("No se puede: otra operación la necesita antes")}>
                        <Icons.CaretDown size={10} />
                      </IconButton>
                      <Show
                        when={f().kind.type === "sketch"}
                        fallback={
                          <IconButton
                            aria-label={f().suppressed ? "Activar" : "Suprimir"}
                            size="sm"
                            variant="ghost"
                            onClick={() => void store.updateFeature(f().id, (x) => (x.suppressed = !x.suppressed))}
                          >
                            {f().suppressed ? <Icons.EyeSlash size={12} /> : <Icons.Eye size={12} />}
                          </IconButton>
                        }
                      >
                        <IconButton
                          aria-label={props.ui.hiddenSketches().includes(f().id) ? "Mostrar en el visor" : "Ocultar en el visor"}
                          size="sm"
                          variant="ghost"
                          onClick={() => props.ui.toggleSketchVisible(f().id)}
                        >
                          {props.ui.hiddenSketches().includes(f().id) ? <Icons.EyeSlash size={12} /> : <Icons.Eye size={12} />}
                        </IconButton>
                      </Show>
                      <IconButton
                        aria-label="Borrar"
                        size="sm"
                        variant="ghost"
                        onClick={() => !store.removeFeature(f().id) && props.ui.setMessage("Otras operaciones dependen de esta: borrarlas primero")}
                      >
                        <Icons.Trash size={12} />
                      </IconButton>
                    </span>
                  </div>
                </>
              );
            }}
          </For>
          <Gap index={count()} />
        </div>
        <div class="flex flex-wrap gap-1.5">
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
            <Tooltip content="Mayús+clic en otra fila elige un tramo">
              <Button
                size="sm"
                variant="ghost"
                onClick={() => {
                  const r = selRange();
                  if (!r) return;
                  if (!store.addFolder(r[0], r[1])) return props.ui.setMessage("Se pisa con otra carpeta");
                  setRangeTo(undefined);
                }}
              >
                Agrupar en carpeta
              </Button>
            </Tooltip>
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
  featureId: number;
  store: CadStore;
  ui: CadUi;
  sketches: Feature[];
  tools: Feature[];
}> = (props) => {
  // Al cerrarse el diálogo la operación puede desaparecer antes que el editor:
  // se queda con la última
  const f = createMemo<Feature>((prev) => props.store.doc()?.features.find((x) => x.id === props.featureId) ?? prev!);
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
  /** Posiciones de las referencias de un campo que no se encontraron */
  const lost = (field: MissingRef["field"]) => {
    const st = state();
    return st?.state === "error" || st?.state === "warning" ? (st.missing ?? []).filter((m) => m.field === field).map((m) => m.index) : [];
  };
  const sketchOptions = () => props.sketches.map((s) => ({ value: String(s.id), label: s.name }));
  const planeSpecSelect = (value: PlaneSpec, set: (p: PlaneSpec) => void, lostFace = false) => (
    <Select
      options={[
        ...(["xy", "xz", "yz"] as const).map((k) => ({ value: k, label: PLANE_LABELS[k] })),
        ...(value.type === "face" ? [{ value: "face", label: lostFace ? "Cara del sólido · no encontrada" : "Cara del sólido" }] : []),
        ...(value.type === "custom" ? [{ value: "custom", label: "Plano propio" }] : []),
      ]}
      value={value.type}
      onChange={(v) => v !== value.type && (v === "xy" || v === "xz" || v === "yz") && set({ type: v })}
    />
  );
  const opSelect = (value: BodyOp, set: (o: BodyOp) => void) => <Select options={OP_OPTIONS} value={value} onChange={(v) => set(v as BodyOp)} />;
  // Diálogo abierto: los cambios se ven en el visor y quedan al aceptar
  const open = () => props.store.draft()?.feature === f().id;

  return (
    <Section
      title={FEATURE_LABELS[f().kind.type]}
      class={clsx(open() && "rounded-md border border-accent/50 bg-accent/5 p-2")}
      right={
        <Show when={open()}>
          <div class="flex items-center gap-1">
            <Show when={props.store.busy()}>
              <span class="text-[11px] text-text-dim">Calculando…</span>
            </Show>
            <Tooltip content="Aceptar (Enter)">
              <IconButton aria-label="Aceptar" size="sm" variant="primary" onClick={() => void props.store.acceptDraft()}>
                <Icons.Check size={12} />
              </IconButton>
            </Tooltip>
            <Tooltip content="Cancelar (Esc)">
              <IconButton aria-label="Cancelar" size="sm" onClick={() => void props.store.cancelDraft()}>
                <Icons.X size={12} />
              </IconButton>
            </Tooltip>
          </div>
        </Show>
      }
    >
      <div class="space-y-2">
        <input
          class="w-full px-2 py-1 rounded bg-surface/40 border border-border text-xs text-text outline-none focus:border-accent"
          value={f().name}
          onChange={(e) => void props.store.updateFeature(f().id, (x) => (x.name = e.currentTarget.value))}
        />
        <Show when={problem(state())}>
          {(msg) => <p class={clsx("text-xs", state()?.state === "warning" ? "text-warning" : "text-error")}>{msg()}</p>}
        </Show>
        <Switch>
          <Match when={f().kind.type === "sketch" && (f().kind as Extract<FeatureKind, { type: "sketch" }>)}>
            {(k) => {
              const view = () => props.store.sketchView(f().id);
              return (
                <>
                  <Row label="Plano">{planeSpecSelect(k().plane, (p) => update((x) => x.type === "sketch" && (x.plane = p)), lost("plane").length > 0)}</Row>
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
              // "Hasta una cara" elegido y todavía sin cara
              const [upTo, setUpTo] = createSignal(false);
              const extentType = () => (upTo() ? "up_to_face" : k().extent.type);
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
                        else setUpTo(true);
                      }}
                    />
                  </Row>
                  <Show when={k().extent.type === "up_to_face" || upTo()}>
                    <SelectionBox
                      store={props.store}
                      ui={props.ui}
                      owner={`${f().id}:hasta`}
                      kind="face"
                      label="Hasta la cara"
                      lost={lost("extent")}
                      refs={k().extent.type === "up_to_face" ? [(k().extent as { face: FaceRef }).face] : []}
                      onChange={(refs) => {
                        if (refs[0]) setExtent({ type: "up_to_face", face: refs[0] as FaceRef });
                        setUpTo(false);
                      }}
                    />
                  </Show>
                  <Show when={k().extent.type === "blind" || k().extent.type === "symmetric"}>
                    {field(
                      "Distancia",
                      "kind.extent.distance",
                      (k().extent as { distance: number }).distance,
                      (x, v) => x.type === "extrude" && "distance" in x.extent && (x.extent.distance = v),
                      "mm",
                    )}
                  </Show>
                  <RegionBox
                    ui={props.ui}
                    store={props.store}
                    owner={`${f().id}:regiones`}
                    lost={lost("regions")}
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
                  <RegionBox
                    ui={props.ui}
                    store={props.store}
                    owner={`${f().id}:regiones`}
                    lost={lost("regions")}
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
                  <Show when={k().shape.type === "box" && (k().shape as Extract<PrimitiveShape, { type: "box" }>)}>
                    {(bx) => (
                      <Row label="Origen en">
                        <Select
                          options={[
                            { value: "center", label: "El centro" },
                            { value: "base", label: "El centro de la base" },
                            { value: "corner", label: "Una esquina" },
                          ]}
                          value={bx().centered_z ? "center" : bx().centered ? "base" : "corner"}
                          onChange={(v) =>
                            update((x) => {
                              if (x.type !== "primitive" || x.shape.type !== "box") return;
                              x.shape.centered = v !== "corner";
                              x.shape.centered_z = v === "center";
                            })
                          }
                        />
                      </Row>
                    )}
                  </Show>
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
                  <SelectionBox
                    store={props.store}
                    ui={props.ui}
                    owner={`${f().id}:aristas`}
                    kind="edges"
                    label="Aristas"
                    lost={lost("edges")}
                    refs={(k() as { edges: EdgeRef[] }).edges}
                    onChange={(refs) =>
                      update((x) => {
                        if (x.type === "fillet" || x.type === "chamfer") x.edges = refs as EdgeRef[];
                      })
                    }
                  />
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
                        <Row label="Plano neutro">{planeSpecSelect(d().neutral, (p) => update((x) => x.type === "draft" && (x.neutral = p)), lost("neutral").length > 0)}</Row>
                      </>
                    )}
                  </Show>
                  <SelectionBox
                    store={props.store}
                    ui={props.ui}
                    owner={`${f().id}:caras`}
                    kind="faces"
                    label={kind.type === "shell" ? "Caras abiertas" : "Caras"}
                    lost={lost("faces")}
                    refs={(k() as { faces: FaceRef[] }).faces}
                    onChange={(refs) =>
                      update((x) => {
                        if (x.type === "shell" || x.type === "draft") x.faces = refs as FaceRef[];
                      })
                    }
                  />
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
// ─── Cajas de selección ───────────────────────────────────────────────────

const NOUNS = { edges: "Arista", faces: "Cara", face: "Cara" } as const;
const PROMPTS = {
  edges: "Clic en las aristas (otra vez para quitarla)",
  faces: "Clic en las caras (otra vez para quitarla)",
  face: "Clic en la cara",
};

/**
 * Caja de un campo de referencias, como en Onshape: activa, lo que se elige en
 * el visor va ahí (y se ve el sólido de antes de la operación); la lista
 * resalta cada ítem al pasar el mouse y lo quita con ✗.
 */
const SelectionBox: Component<{
  store: CadStore;
  ui: CadUi;
  /** Identifica la caja (operación y campo) */
  owner: string;
  kind: "edges" | "faces" | "face";
  label: string;
  refs: (EdgeRef | FaceRef)[];
  /** Referencias que el último cálculo no encontró (posiciones en `refs`) */
  lost?: number[];
  onChange: (refs: (EdgeRef | FaceRef)[]) => void;
}> = (props) => {
  const ui = props.ui;
  const active = () => (ui.pick() as { owner?: string }).owner === props.owner;
  const [hover, setHover] = createSignal<number>();
  // Índice en el sólido mostrado de cada referencia (solo con la caja activa:
  // es el sólido de antes de la operación)
  const [resolved, setResolved] = createSignal<(number | null)[]>([]);
  let seq = 0;
  createEffect(() => {
    const refs = props.refs;
    const version = props.store.result()?.version;
    if (!active() || version === undefined) return setResolved([]);
    const n = ++seq;
    const edges = props.kind === "edges";
    void props.store.resolveRefs(edges ? [] : (refs as FaceRef[]), edges ? (refs as EdgeRef[]) : []).then((r) => {
      if (n === seq) setResolved(edges ? r.edges : r.faces);
    });
  });
  // Resaltado: todo lo de la caja activa, o el ítem bajo el mouse
  createEffect(() => {
    if (!active()) return;
    const h = hover();
    const list = resolved().filter((x, i): x is number => x !== null && (h === undefined || i === h));
    ui.setHighlight(props.kind === "edges" ? { faces: [], edges: list } : { faces: list, edges: [] });
  });
  createEffect(
    on(active, (now, before) => {
      if (now || !before) return;
      ui.setHighlight({ faces: [], edges: [] });
      if (ui.pick().kind === "none") void props.store.setSelecting(false);
    }),
  );

  // Con la caja activa manda lo que se ve (el sólido de antes de la operación);
  // si no, lo que dijo el último cálculo
  const isLost = (i: number) => (active() ? resolved()[i] === null : (props.lost ?? []).includes(i));
  const toggleRef = (ref: EdgeRef | FaceRef, index: number) => {
    const at = resolved().indexOf(index);
    if (at >= 0) return props.onChange(props.refs.filter((_, i) => i !== at));
    // Lo nuevo reemplaza a la primera referencia perdida
    const gone = props.refs.findIndex((_, i) => resolved()[i] === null);
    props.onChange(gone >= 0 ? props.refs.map((r, i) => (i === gone ? ref : r)) : [...props.refs, ref]);
  };
  const activate = () => {
    void props.store.setSelecting(true);
    const base = { prompt: PROMPTS[props.kind], owner: props.owner };
    if (props.kind === "edges") ui.setPick({ ...base, kind: "edges", toggle: (r, i) => toggleRef(r, i) });
    else if (props.kind === "faces") ui.setPick({ ...base, kind: "faces", toggle: (r, i) => toggleRef(r, i) });
    else ui.setPick({ ...base, kind: "face", done: (r) => props.onChange([r]) });
  };
  // Vacía o con referencias perdidas al abrirse el diálogo: se empieza eligiendo
  // (al editar, el borrador se abre después de montar la caja)
  createEffect(
    on(
      () => !!props.store.draft(),
      (open) => {
        if (open && untrack(() => props.refs.length === 0 || (props.lost ?? []).length > 0)) activate();
      },
    ),
  );
  onCleanup(() => {
    if (active()) ui.cancelPick();
  });

  return (
    <div class="space-y-1">
      <span class="text-xs text-text-muted">{props.label}</span>
      <div
        aria-label={props.label}
        data-selection-box
        class={clsx(
          "min-h-[2rem] rounded border px-1.5 py-1 cursor-pointer text-xs",
          active() ? "border-accent bg-accent/10" : "border-border bg-surface/40 hover:border-border-hover",
        )}
        onClick={() => (active() ? ui.cancelPick() : activate())}
      >
        <Show
          when={props.refs.length > 0}
          fallback={<span class="text-text-dim">{active() ? PROMPTS[props.kind] : "Nada elegido: clic para elegir"}</span>}
        >
          <For each={props.refs}>
            {(_, i) => (
              <div
                class="group flex items-center justify-between gap-1 rounded px-1 hover:bg-surface"
                onMouseEnter={() => setHover(i())}
                onMouseLeave={() => setHover(undefined)}
              >
                <span class={clsx(isLost(i()) ? "text-error" : "text-text")}>
                  {NOUNS[props.kind]} {i() + 1}
                  {isLost(i()) ? " · no encontrada" : ""}
                </span>
                <button
                  class="opacity-0 group-hover:opacity-100 text-text-muted hover:text-text"
                  aria-label={`Quitar ${NOUNS[props.kind].toLowerCase()} ${i() + 1}`}
                  onClick={(e) => {
                    e.stopPropagation();
                    setHover(undefined);
                    props.onChange(props.refs.filter((_, j) => j !== i()));
                  }}
                >
                  <Icons.X size={10} />
                </button>
              </div>
            )}
          </For>
        </Show>
      </div>
    </div>
  );
};

/** Caja de regiones de un sketch: clic dentro de una región la suma o la quita */
const RegionBox: Component<{
  ui: CadUi;
  store: CadStore;
  owner: string;
  sketch: number;
  value: RegionSelection;
  /** Regiones que el último cálculo no encontró */
  lost?: number[];
  onChange: (r: RegionSelection) => void;
}> = (props) => {
  const ui = props.ui;
  const active = () => (ui.pick() as { owner?: string }).owner === props.owner;
  const points = () => (props.value.type === "points" ? props.value.points : []);
  createEffect(
    on(active, (now, before) => {
      if (!now && before && ui.pick().kind === "none") void props.store.setSelecting(false);
    }),
  );
  onCleanup(() => {
    if (active()) ui.cancelPick();
  });
  const activate = () => {
    void props.store.setSelecting(true);
    ui.setPick({
      kind: "region",
      owner: props.owner,
      sketch: props.sketch,
      prompt: "Clic dentro de las regiones a usar (otra vez para quitarla)",
      chosen: points,
      toggle: (p) => {
        const view = props.store.sketchView(props.sketch);
        if (!view) return;
        // Región más interna bajo el clic; si ya había un punto en ella, se quita
        const hit = [...view.regions].filter((r) => regionContains(r, p)).sort((a, b) => b.depth - a.depth)[0];
        if (!hit) return;
        const list = points();
        const inside = list.filter((q) => regionContains(hit, q) && !view.regions.some((o) => o.depth > hit.depth && regionContains(o, q)));
        const next = inside.length ? list.filter((q) => !inside.includes(q)) : [...list, hit.sample];
        props.onChange(next.length ? { type: "points", points: next } : { type: "all" });
      },
    });
  };
  return (
    <div class="space-y-1">
      <span class="text-xs text-text-muted">Regiones</span>
      <div
        aria-label="Regiones"
        data-selection-box
        class={clsx(
          "min-h-[2rem] rounded border px-1.5 py-1 cursor-pointer text-xs",
          active() ? "border-accent bg-accent/10" : "border-border bg-surface/40 hover:border-border-hover",
        )}
        onClick={() => (active() ? ui.cancelPick() : activate())}
      >
        <Show when={points().length > 0} fallback={<span class="text-text-dim">Todas{active() ? " · clic en las regiones a usar" : ""}</span>}>
          <For each={points()}>
            {(_, i) => (
              <div class="group flex items-center justify-between gap-1 rounded px-1 hover:bg-surface">
                <span class={(props.lost ?? []).includes(i()) ? "text-error" : "text-text"}>
                  Región {i() + 1}
                  {(props.lost ?? []).includes(i()) ? " · no encontrada" : ""}
                </span>
                <button
                  class="opacity-0 group-hover:opacity-100 text-text-muted hover:text-text"
                  aria-label={`Quitar región ${i() + 1}`}
                  onClick={(e) => {
                    e.stopPropagation();
                    const next = points().filter((_, j) => j !== i());
                    props.onChange(next.length ? { type: "points", points: next } : { type: "all" });
                  }}
                >
                  <Icons.X size={10} />
                </button>
              </div>
            )}
          </For>
        </Show>
      </div>
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

const BodySection: Component<{ store: CadStore; ui: CadUi; onExport: (f: "step" | "stl" | "3mf") => void; onUseAsModel?: () => void }> = (props) => (
  <Show when={props.store.result()?.body}>
    {(b) => {
      const size = () => b().bbox_max.map((v, i) => v - b().bbox_min[i]);
      const material = () => props.store.doc()?.material ?? null;
      const setMaterial = (m: Material | null) => void props.store.commit((d) => (d.material = m));
      // Gramos: mm³ × kg/m³ × 1e−6
      const mass = () => (material() ? b().volume * material()!.density * 1e-6 : null);
      const materialValue = () => {
        const m = material();
        if (!m) return "none";
        return MATERIALS.some((x) => x.name === m.name && x.density === m.density) ? m.name : "custom";
      };
      const Line = (p: { label: string; children: JSX.Element }) => (
        <div class="flex justify-between gap-2">
          <span class="text-text-muted whitespace-nowrap">{p.label}</span>
          <span class="font-mono text-text text-right">{p.children}</span>
        </div>
      );
      return (
        <Section title="Sólido">
          <div class="space-y-1 text-xs">
            <Line label="Volumen">{fmt(b().volume / 1000)} cm³</Line>
            <Line label="Área">{fmt(b().area / 100)} cm²</Line>
            <Line label="Tamaño X × Y × Z">{size().map((v) => fmt(v, 1)).join(" × ")} mm</Line>
            <Line label="Caras / aristas">
              {b().faces} / {b().edges}
            </Line>
            <Show when={!b().valid}>
              <p class="text-warning">La forma tiene defectos: puede fallar al exportar o imprimir</p>
            </Show>
          </div>
          <Row label="Material">
            <Select
              options={[
                { value: "none", label: "Sin material" },
                ...MATERIALS.map((m) => ({ value: m.name, label: `${m.name} (${m.density} kg/m³)` })),
                { value: "custom", label: "Densidad propia" },
              ]}
              value={materialValue()}
              onChange={(v) => {
                if (v === materialValue()) return;
                if (v === "none") setMaterial(null);
                else if (v === "custom") setMaterial({ name: "Otro", density: material()?.density ?? 1000 });
                else setMaterial(MATERIALS.find((m) => m.name === v)!);
              }}
            />
          </Row>
          <Show when={materialValue() === "custom"}>
            <Num label="Densidad" suffix="kg/m³" step={10} value={material()!.density} onCommit={(v) => v > 0 && setMaterial({ name: material()!.name, density: v })} />
          </Show>
          <div class="space-y-1 text-xs">
            <Show when={mass() !== null}>
              <Line label="Masa">{mass()! >= 1000 ? `${fmt(mass()! / 1000, 3)} kg` : `${fmt(mass()!, 2)} g`}</Line>
            </Show>
            <Line label="Centro de masa">{b().center.map((v) => fmt(v, 2)).join(", ")} mm</Line>
            <Show when={mass() !== null}>
              {/* kg·mm² = mm⁵ × kg/m³ × 1e−9 */}
              <Line label="Inercia">
                {b()
                  .inertia.map((v) => fmt(v * material()!.density * 1e-9, 3))
                  .join(" · ")}{" "}
                kg·mm²
              </Line>
            </Show>
          </div>
          <Checkbox small label="Ver el centro de masa" checked={props.ui.showCenterOfMass()} onChange={(c) => props.ui.setShowCenterOfMass(c)} />
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
  const [patternCount, setPatternCount] = createSignal(3);
  const [patternKind, setPatternKind] = createSignal<"linear" | "circular">("linear");
  const [patternStep, setPatternStep] = createSignal<P2>([20, 0]);
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
    if (C.length === 2) {
      out.push({ label: "Mismo radio", make: () => ({ type: "equal", a: C[0], b: C[1] }) });
      out.push({ label: "Tangentes", make: () => ({ type: "tangent", a: C[0], b: C[1] }) });
      out.push({ label: "Concéntricos", make: () => ({ type: "concentric", a: C[0], b: C[1] }) });
    }
    if (L.length === 1 && C.length === 1) out.push({ label: "Tangente", make: () => ({ type: "tangent", a: L[0], b: C[0] }) });
    if (P.length === 1 && L.length === 1) {
      out.push({ label: "Punto en la línea", make: () => ({ type: "point_on_line", point: P[0], line: L[0] }) });
      out.push({ label: "Punto medio", make: () => ({ type: "midpoint", point: P[0], line: L[0] }) });
    }
    if (P.length === 1 && C.length === 1) out.push({ label: "Punto en el círculo", make: () => ({ type: "point_on_circle", point: P[0], circle: C[0] }) });
    if (P.length === 2 && L.length === 0) {
      const [a, b] = [point(P[0])!, point(P[1])!];
      out.push({ label: "Coincidentes", make: () => ({ type: "coincident", a: P[0], b: P[1] }) });
      out.push({ label: "Alineados horizontal", make: () => ({ type: "horizontal_points", a: P[0], b: P[1] }) });
      out.push({ label: "Alineados vertical", make: () => ({ type: "vertical_points", a: P[0], b: P[1] }) });
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
          <Show when={selEntities().length === 1 && selEntities()[0]!.geometry.type === "spline"}>
            <Button
              size="sm"
              title="Puntos que dan la dirección de salida y de llegada; se arrastran"
              onClick={() => {
                const id = selEntities()[0]!.id;
                let msg: string | undefined;
                ui.change((sk) => (msg = toggleSplineHandles(sk, id)));
                ui.setMessage(msg);
              }}
            >
              Manijas en los extremos sí/no
            </Button>
          </Show>
          <Show when={selEntities().length > 0}>
            <div class="space-y-1.5 border-t border-border pt-1.5">
              <Show when={lines().length > 0 && selEntities().length > 1}>
                <Button
                  size="sm"
                  title="El eje es la primera línea elegida; lo demás se copia reflejado"
                  onClick={() => {
                    const ids = selEntities().map((e) => e!.id);
                    let msg: string | undefined;
                    ui.change((sk) => (msg = mirrorEntities(sk, ids, lines()[0]!.id)));
                    ui.setMessage(msg);
                    if (!msg) ui.setSelection([]);
                  }}
                >
                  Simetría (eje: primera línea elegida)
                </Button>
              </Show>
              <div class="flex items-end gap-1.5">
                <div class="flex-1">
                  <Num label="Cantidad" min={2} value={patternCount()} onCommit={(v) => setPatternCount(Math.max(2, Math.round(v)))} />
                </div>
                <Select
                  options={[
                    { value: "linear", label: "Lineal" },
                    { value: "circular", label: "Circular" },
                  ]}
                  value={patternKind()}
                  onChange={(v) => setPatternKind(v as "linear" | "circular")}
                />
              </div>
              <Show
                when={patternKind() === "linear"}
                fallback={
                  <p class="text-[11px] text-text-dim">
                    {selPoints().length === 1 ? "Alrededor del punto elegido" : "Alrededor del origen (o elegir también un punto)"}, repartidos en la vuelta
                  </p>
                }
              >
                <div class="flex gap-1.5">
                  <Num label="X" suffix="mm" step={1} value={patternStep()[0]} onCommit={(v) => setPatternStep([v, patternStep()[1]])} />
                  <Num label="Y" suffix="mm" step={1} value={patternStep()[1]} onCommit={(v) => setPatternStep([patternStep()[0], v])} />
                </div>
              </Show>
              <Button
                size="sm"
                onClick={() => {
                  const ids = selEntities().map((e) => e!.id);
                  const n = patternCount();
                  let msg: string | undefined;
                  if (patternKind() === "linear") ui.change((sk) => (msg = linearPattern(sk, ids, n, patternStep())));
                  else {
                    const center = selPoints().length === 1 ? selPoints()[0] : sketch().origin;
                    if (center === undefined) msg = "Elegir el punto alrededor del cual repetir";
                    else ui.change((sk) => (msg = circularPattern(sk, ids, n, center)));
                  }
                  ui.setMessage(msg);
                  if (!msg) ui.setSelection([]);
                }}
              >
                Repetir en patrón
              </Button>
            </div>
          </Show>
          <div class="flex gap-1.5">
            <Button size="sm" variant="ghost" onClick={() => ui.toggleConstruction()} disabled={selEntities().length === 0}>
              Construcción sí/no (Q)
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
                      <Show
                        when={!isReference(c)}
                        fallback={<span class="w-28 px-1.5 font-mono text-text-dim" title="Cota de referencia: mide, no restringe">({+value()!.toFixed(3)})</span>}
                      >
                        <div class="w-28">
                          <Formula
                            value={value()!}
                            expr={(c as { expr?: string }).expr}
                            onCommit={(text) => ui.setConstraintText(i(), text)}
                          />
                        </div>
                      </Show>
                      <Tooltip content={isReference(c) ? "Pasar a cota que restringe" : "Dejar como cota de referencia (solo mide)"}>
                        <button
                          class={clsx("px-1 rounded text-[10px] border", isReference(c) ? "border-accent text-accent" : "border-border text-text-dim hover:text-text")}
                          onClick={() => ui.toggleReference(i())}
                        >
                          Ref
                        </button>
                      </Tooltip>
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
