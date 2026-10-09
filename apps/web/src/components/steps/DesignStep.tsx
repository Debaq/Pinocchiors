import { Component, For, Match, Show, Switch, createEffect, createMemo, createSignal, on, onCleanup, untrack, type JSX } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { clsx } from "clsx";
import {
  BASE_PLANES,
  CONSTRAINT_LABELS,
  FEATURE_LABELS,
  MATERIALS,
  MATE_LABELS,
  METRIC_HOLES,
  BOLT_HEADS,
  type BoltHead,
  designMass,
  partColor,
  partHidden,
  partMass,
  partMaterial,
  samePart,
  OP_LABELS,
  PLANE_LABELS,
  constraintIds,
  editsAsSketch,
  constraintValue,
  isReference,
  filletCorner,
  circularPattern,
  curvePattern,
  fillPattern,
  linearPattern,
  mirrorEntities,
  tablePattern,
  toggleSplineHandles,
  linesAt,
  offsetEntities,
  offsetPlane,
  type AxisSpec,
  type BendRelief,
  type BodyOp,
  type CornerRelief,
  type CadStore,
  type ChamferSecond,
  type Configuration,
  nextVersionName,
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
  type AxisDef,
  type Connector,
  type Mate,
  type MateKind,
  type PartId,
  type PlaneDef,
  type PointSpec,
  type Plane,
  type Sketch3d,
  type Constraint3d,
  type PartView,
  type PatternKind,
  type PrimitiveShape,
  type RegionSelection,
  type PlaneSpec,
  type ScanPick,
  type Sketch,
  type SketchEntity,
  type Geometry,
  type SketchConstraint,
  type SketchSuggestion,
  type ThreadAxis,
  type ThreadLink,
  type ThreadWant,
  coaxialThreads,
  threadCandidates,
  threadLabel,
  threadName,
  threadOfFace,
  boltLengthFor,
  BOLT_LENGTHS,
  NUT_HEIGHT,
} from "../../lib/cad";
import type { CadUi, Hit3d } from "../../lib/cadUi";
import type { DesignActions } from "../../lib/designActions";
import { regionContains } from "../../lib/CadViewer";
import { measureConstraint, rotation, scaling, selectionCenter, translation } from "../../lib/sketchTransform";
import { convertToBSpline, fitSplineToPoints, removeSplinePoint, simplifySpline, splineCurvature, toggleSplineHandle } from "../../lib/sketchSplines";
import { selectByKind, type SelectKind } from "../../lib/sketchCheck";
import { GLYPHS, isDimension } from "../../lib/sketchGlyphs";
import { partKey } from "../../lib/objects";
import { Button, Checkbox, IconButton, NumberInput, Select, Slider, Tooltip } from "../ui";
import * as Icons from "../icons";

export interface DesignStepProps {
  store: CadStore;
  ui: CadUi;
  /** Acciones de Diseñar (las comparte con la barra del visor) */
  actions: DesignActions;
  /** Pestaña del panel */
  section: "design" | "design_part" | "design_inspect" | "design_scan";
  /** Hay un modelo cargado que se puede usar como escaneo de referencia */
  hasModel: boolean;
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
const RELIEF_OPTIONS = [
  { value: "rectangle", label: "Ranura recta" },
  { value: "obround", label: "Ranura con fondo redondo" },
  { value: "none", label: "Sin alivio" },
];
const CORNER_OPTIONS = [
  { value: "none", label: "Muesca del doblez" },
  { value: "round", label: "Redonda" },
  { value: "square", label: "Cuadrada" },
];
const AXIS_DIRS: Record<string, P3> = { x: [1, 0, 0], y: [0, 1, 0], z: [0, 0, 1] };

/** Número con `d` decimales; lo que redondea a cero se muestra sin signo (no "-0,00") */
const fmt = (v: number, d = 2) => (Math.abs(v) < 0.5 * 10 ** -d ? 0 : v).toLocaleString("es", { maximumFractionDigits: d, minimumFractionDigits: d });

export function pickSummary(picks: { kind: string }[]): string {
  const count = (k: string) => picks.filter((p) => p.kind === k).length;
  const parts: [number, string, string][] = [
    [count("region"), "región", "regiones"],
    [count("face"), "cara", "caras"],
    [count("edge"), "arista", "aristas"],
    [count("vertex"), "vértice", "vértices"],
    [count("plane") + count("refplane"), "plano", "planos"],
  ];
  return parts.filter(([n]) => n > 0).map(([n, one, many]) => `${n} ${n === 1 ? one : many}`).join(", ");
}

function planeLabel(p: PlaneSpec): string {
  if (p.type === "face") return "Cara del sólido";
  if (p.type === "custom") return "Plano propio";
  if (p.type === "reference") return "Plano de referencia";
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

  const actions = props.actions;
  const { say } = actions;

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
            <p class="text-xs text-text-dim">Preparando el diseño…</p>
          }
        >
          <Switch>
            <Match when={props.section === "design"}>
              <Show when={ui.session()} fallback={
                <Show when={!ui.assemblyMode()} fallback={<AssemblyPanel store={store} ui={ui} />}>
                  <ParametersSection store={store} />
                  <ConfigurationsSection store={store} />
                  <VersionsSection store={store} />
                </Show>
              }>
                <SketchPanel ui={ui} store={store} />
              </Show>
            </Match>
            <Match when={props.section === "design_part"}>
              <PartsSection store={store} ui={ui} />
              <Show when={!store.result()?.parts.length}>
                <p class="text-xs text-text-dim">Todavía no hay piezas: crea un sólido con la barra de arriba del visor.</p>
              </Show>
            </Match>
            <Match when={props.section === "design_inspect"}>
              <BodySection store={store} ui={ui} />
              <Show when={!store.result()?.body}>
                <p class="text-xs text-text-dim">Todavía no hay sólido para inspeccionar.</p>
              </Show>
            </Match>
            <Match when={props.section === "design_scan"}>
              <Show
                when={props.hasModel}
                fallback={<p class="text-xs text-text-dim">Importa un modelo (un escaneo) para tomar de él planos, cilindros y contornos.</p>}
              >
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
                  <DeviationCard store={store} ui={ui} />
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

            </Match>
          </Switch>
        </Show>
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

/**
 * Configuraciones: variantes de la misma pieza (M3, M4, M5…). Cada una
 * reemplaza parámetros y suprime operaciones; la elegida es la que se ve y
 * se exporta.
 */
/** Fecha corta: "8 oct 2026, 14:05" */
const versionDate = (iso: string) => {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleString("es", { day: "numeric", month: "short", year: "numeric", hour: "2-digit", minute: "2-digit" });
};

/** Versiones con nombre: instantáneas del diseño para volver a verlas, compararlas o restaurarlas */
const VersionsSection: Component<{ store: CadStore }> = (props) => {
  const store = props.store;
  const versions = () => store.committed()?.versions ?? [];
  const [name, setName] = createSignal("");
  const [note, setNote] = createSignal("");
  const save = () => {
    void store.saveVersion(name() || nextVersionName(versions()), note());
    setName("");
    setNote("");
  };
  const input = "min-w-0 px-1.5 py-0.5 rounded bg-surface/40 border border-border text-xs text-text outline-none focus:border-accent";
  return (
    <Section title="Versiones">
      <div class="space-y-1.5" data-versions>
        <div class="flex gap-1">
          <input
            aria-label="Nombre de la versión"
            placeholder={nextVersionName(versions())}
            value={name()}
            class={clsx(input, "flex-1")}
            onInput={(e) => setName(e.currentTarget.value)}
            onKeyDown={(e) => e.key === "Enter" && save()}
          />
          <Button size="sm" onClick={save} disabled={store.viewing() !== undefined}>
            Guardar versión
          </Button>
        </div>
        <input
          aria-label="Nota de la versión"
          placeholder="Nota (opcional)"
          value={note()}
          class={clsx(input, "w-full")}
          onInput={(e) => setNote(e.currentTarget.value)}
          onKeyDown={(e) => e.key === "Enter" && save()}
        />
        <Show
          when={versions().length > 0}
          fallback={
            <p class="text-[11px] text-text-dim leading-relaxed">
              Guarda cómo está el diseño ("v1 enviada a imprimir") para verlo después, compararlo con el de ahora o volver a él.
            </p>
          }
        >
          <For each={versions().map((v, i) => ({ v, i })).reverse()}>
            {(row) => (
              <div
                data-version={row.v.name}
                class={clsx(
                  "rounded border px-1.5 py-1 space-y-0.5",
                  store.viewing() === row.i ? "border-warning/70 bg-warning/5" : "border-border",
                )}
              >
                <div class="flex items-center gap-1">
                  <input
                    aria-label={`Nombre de ${row.v.name}`}
                    value={row.v.name}
                    class={clsx(input, "flex-1 border-transparent bg-transparent font-medium")}
                    onChange={(e) => void store.renameVersion(row.i, e.currentTarget.value)}
                    onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
                  />
                  <span class="text-[10px] text-text-dim shrink-0">{versionDate(row.v.created)}</span>
                </div>
                <Show when={row.v.note}>
                  <p class="text-[11px] text-text-muted px-1.5">{row.v.note}</p>
                </Show>
                <div class="flex items-center gap-0.5 -mx-1">
                  <Button
                    size="sm"
                    variant="ghost"
                    aria-label={`Ver ${row.v.name}`}
                    onClick={() => void store.viewVersion(store.viewing() === row.i ? undefined : row.i)}
                  >
                    {store.viewing() === row.i ? "Volver" : "Ver"}
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    aria-label={`Comparar con ${row.v.name}`}
                    onClick={() => void store.compareVersion(store.comparison()?.version === row.i ? undefined : row.i)}
                  >
                    {store.comparison()?.version === row.i ? "Ocultar" : "Comparar"}
                  </Button>
                  <Button size="sm" variant="ghost" aria-label={`Restaurar ${row.v.name}`} onClick={() => void store.restoreVersion(row.i)}>
                    Restaurar
                  </Button>
                  <span class="flex-1" />
                  <IconButton aria-label={`Quitar ${row.v.name}`} size="sm" variant="ghost" onClick={() => void store.deleteVersion(row.i)}>
                    <Icons.X size={10} />
                  </IconButton>
                </div>
              </div>
            )}
          </For>
        </Show>
      </div>
    </Section>
  );
};

const ConfigurationsSection: Component<{ store: CadStore }> = (props) => {
  const store = props.store;
  const configs = () => store.doc()?.configurations ?? [];
  const active = () => store.doc()?.active_configuration ?? null;
  const params = () => store.doc()?.parameters ?? [];
  const features = () => store.doc()?.features ?? [];
  const add = () => {
    let n = configs().length + 1;
    while (configs().some((c) => c.name === `Variante ${n}`)) n++;
    const index = configs().length;
    void store.commit((d) => {
      d.configurations = [...(d.configurations ?? []), { name: `Variante ${n}`, values: {}, suppressed: [] }];
      d.active_configuration = index;
    });
  };
  const edit = (index: number, change: (c: Configuration) => void) =>
    void store.commit((d) => {
      const c = d.configurations?.[index];
      if (c) change(c);
    });
  return (
    <Section
      title="Configuraciones"
      right={
        <IconButton aria-label="Agregar configuración" size="sm" variant="ghost" onClick={add}>
          <Icons.Plus size={12} />
        </IconButton>
      }
    >
      <Show
        when={configs().length > 0}
        fallback={
          <p class="text-[11px] text-text-dim leading-relaxed">
            Variantes de la misma pieza (M3, M4, M5; con o sin un agujero): cada una cambia parámetros y apaga operaciones. Se pueden exportar
            todas juntas.
          </p>
        }
      >
        <div class="space-y-1" data-configs>
          <For each={[{ name: "Base", index: null as number | null }, ...configs().map((c, i) => ({ name: c.name, index: i as number | null }))]}>
            {(row) => (
              <div class="flex items-center gap-1.5">
                <button
                  class={clsx(
                    "w-3 h-3 shrink-0 rounded-full border",
                    active() === row.index ? "bg-accent border-accent" : "border-border hover:border-accent",
                  )}
                  aria-label={`Usar ${row.name}`}
                  onClick={() => void store.commit((d) => (d.active_configuration = row.index))}
                />
                <Show when={row.index !== null} fallback={<span class="flex-1 text-xs text-text-muted">Base</span>}>
                  <input
                    value={row.name}
                    class="flex-1 min-w-0 px-1.5 py-0.5 rounded bg-surface/40 border border-border text-xs text-text outline-none focus:border-accent"
                    onChange={(e) => {
                      const name = e.currentTarget.value.trim();
                      if (name && name !== row.name) edit(row.index!, (c) => (c.name = name));
                    }}
                    onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
                  />
                  <IconButton
                    aria-label={`Quitar ${row.name}`}
                    size="sm"
                    variant="ghost"
                    onClick={() =>
                      void store.commit((d) => {
                        d.configurations = (d.configurations ?? []).filter((_, i) => i !== row.index);
                        const a = d.active_configuration;
                        if (a === row.index) d.active_configuration = null;
                        else if (a != null && a > row.index!) d.active_configuration = a - 1;
                      })
                    }
                  >
                    <Icons.X size={10} />
                  </IconButton>
                </Show>
              </div>
            )}
          </For>
        </div>
        {/* La elegida: qué cambia respecto de lo de base */}
        <Show when={active() !== null && configs()[active()!]}>
          {(c) => (
            <div class="space-y-1.5 rounded-md border border-border p-2" data-config-editor>
              <Show when={params().length} fallback={<p class="text-[11px] text-text-dim">Sin parámetros: crear alguno arriba para variar medidas.</p>}>
                <For each={params()}>
                  {(p) => (
                    <label class="flex items-center gap-1.5 text-xs text-text-muted">
                      <span class="w-20 shrink-0 font-mono truncate">{p.name}</span>
                      <input
                        value={c().values?.[p.name] ?? ""}
                        placeholder={p.expr}
                        aria-label={`${p.name} en ${c().name}`}
                        class="flex-1 min-w-0 px-1.5 py-0.5 rounded bg-surface/40 border border-border text-xs text-text font-mono outline-none focus:border-accent placeholder:text-text-dim"
                        onChange={(e) => {
                          const v = e.currentTarget.value.trim();
                          edit(active()!, (x) => {
                            x.values = { ...(x.values ?? {}) };
                            if (v) x.values[p.name] = v;
                            else delete x.values[p.name];
                          });
                        }}
                        onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
                      />
                    </label>
                  )}
                </For>
              </Show>
              <p class="text-[11px] text-text-dim">Vacío: el valor de base. Operaciones apagadas en esta variante:</p>
              <div class="max-h-40 overflow-y-auto space-y-0.5">
                <For each={features()}>
                  {(f) => (
                    <Checkbox
                      small
                      label={f.name}
                      checked={(c().suppressed ?? []).includes(f.id)}
                      onChange={(on) =>
                        edit(active()!, (x) => {
                          const s = new Set(x.suppressed ?? []);
                          if (on) s.add(f.id);
                          else s.delete(f.id);
                          x.suppressed = [...s];
                        })
                      }
                    />
                  )}
                </For>
              </div>
            </div>
          )}
        </Show>
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

/** Árbol de operaciones (va en el grupo Operaciones del Outliner) */
export const FeatureTree: Component<{ store: CadStore; ui: CadUi }> = (props) => {
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
    <div data-feature-tree>
      <Show when={count() > 0} fallback={<p class="px-1 text-xs text-text-dim">Todavía no hay operaciones</p>}>
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
                    onDblClick={() => editsAsSketch(f().kind) && void store.settled().then(() => props.ui.editSketch(f().id))}
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
                        when={["sketch", "plane", "axis", "point"].includes(f().kind.type)}
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
    </div>
  );
};

// ─── Edición de una operación ─────────────────────────────────────────────

export const FeatureEditor: Component<{
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
  const planeSpecSelect = (value: PlaneSpec, set: (p: PlaneSpec) => void, lostFace = false) => {
    const current = value.type === "reference" ? `ref:${value.feature}` : value.type;
    return (
      <Select
        options={[
          ...(["xy", "xz", "yz"] as const).map((k) => ({ value: k, label: PLANE_LABELS[k] })),
          ...refOptions(props.store, "plane", f().id),
          ...(value.type === "face" ? [{ value: "face", label: lostFace ? "Cara del sólido · no encontrada" : "Cara del sólido" }] : []),
          ...(value.type === "custom" ? [{ value: "custom", label: "Plano propio" }] : []),
        ]}
        value={current}
        onChange={(v) => {
          if (v === current) return;
          if (v === "xy" || v === "xz" || v === "yz") set({ type: v });
          else if (v.startsWith("ref:")) set({ type: "reference", feature: +v.slice(4) });
        }}
      />
    );
  };
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
                        { value: "two_sides", label: "Dos direcciones" },
                        { value: "through_all", label: "Atravesar todo" },
                        { value: "up_to_next", label: "Hasta la siguiente" },
                        { value: "up_to_face", label: "Hasta una cara" },
                      ]}
                      value={extentType()}
                      onChange={(v) => {
                        if (v === extentType()) return;
                        const d = "distance" in k().extent ? (k().extent as { distance: number }).distance : 10;
                        if (v === "blind" || v === "symmetric") setExtent({ type: v, distance: d });
                        else if (v === "two_sides") setExtent({ type: "two_sides", distance: d, second: d / 2 });
                        else if (v === "through_all" || v === "up_to_next") setExtent({ type: v });
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
                  <Show when={k().extent.type === "blind" || k().extent.type === "symmetric" || k().extent.type === "two_sides"}>
                    {field(
                      "Distancia",
                      "kind.extent.distance",
                      (k().extent as { distance: number }).distance,
                      (x, v) => x.type === "extrude" && "distance" in x.extent && (x.extent.distance = v),
                      "mm",
                    )}
                  </Show>
                  <Show when={k().extent.type === "two_sides"}>
                    {field(
                      "Hacia atrás",
                      "kind.extent.second",
                      (k().extent as { second: number }).second,
                      (x, v) => x.type === "extrude" && x.extent.type === "two_sides" && (x.extent.second = v),
                      "mm",
                    )}
                  </Show>
                  {field("Desmolde", "kind.draft", k().draft ?? 0, (x, v) => x.type === "extrude" && (x.draft = v), "°")}
                  <Checkbox
                    small
                    label="Delgada (solo una pared)"
                    checked={k().thin != null}
                    onChange={(c) => update((x) => x.type === "extrude" && (x.thin = c ? 1 : null))}
                  />
                  <Show when={k().thin != null}>
                    {field("Espesor", "kind.thin", k().thin ?? 1, (x, v) => x.type === "extrude" && (x.thin = v), "mm")}
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
              const axisValue = () => {
                const a = k().axis;
                return a.type === "sketch_line" ? `line:${a.line}` : a.type === "reference" ? `ref:${a.feature}` : a.type;
              };
              return (
                <>
                  <Row label="Sketch">
                    <Select options={sketchOptions()} value={String(k().sketch)} onChange={(v) => update((x) => x.type === "revolve" && (x.sketch = +v))} />
                  </Row>
                  <Row label="Eje">
                    <Select
                      options={[
                        ...(k().axis.type === "edge" ? [{ value: "edge", label: "Arista del sólido" }] : []),
                        { value: "x", label: "X" },
                        { value: "y", label: "Y" },
                        { value: "z", label: "Z" },
                        ...lines().map((l) => ({ value: `line:${l.id}`, label: `Línea ${l.id}${l.construction ? " (construcción)" : ""}` })),
                        ...refOptions(props.store, "axis"),
                      ]}
                      value={axisValue()}
                      onChange={(v) =>
                        update((x) => {
                          if (x.type !== "revolve") return;
                          x.axis = v.startsWith("line:")
                            ? { type: "sketch_line", sketch: x.sketch, line: +v.slice(5) }
                            : v.startsWith("ref:")
                              ? { type: "reference", feature: +v.slice(4) }
                              : { type: v as "x" | "y" | "z" };
                        })
                      }
                    />
                  </Row>
                  <Button
                    size="sm"
                    fullWidth
                    variant={(props.ui.pick() as { owner?: string }).owner === `${f().id}:eje` ? "primary" : "default"}
                    onClick={() =>
                      props.ui.setPick({
                        kind: "axis",
                        owner: `${f().id}:eje`,
                        sketch: k().sketch,
                        prompt: "Clic en la línea del sketch (o la arista del sólido) que hace de eje",
                        done: (axis) => update((x) => x.type === "revolve" && (x.axis = axis)),
                      })
                    }
                  >
                    Elegir el eje en el visor
                  </Button>
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
                bolt: [["length", "Largo"]],
                nut: [],
                washer: [],
              };
              type Standard = Extract<PrimitiveShape, { type: "bolt" | "nut" | "washer" }>;
              const standard = () => (["bolt", "nut", "washer"].includes(k().shape.type) ? (k().shape as Standard) : undefined);
              const threaded = () => k().shape.type === "bolt" || k().shape.type === "nut";
              const linked = () => threaded() && !!k().link;
              /** Asociar a otra rosca: la medida y un largo (o una posición) que calzan; soltar deja la pieza donde está */
              const setLink = (link: ThreadLink | null, src: ThreadAxis | null) => {
                const placed = props.store.result()?.threads?.find((t) => t.feature === f().id)?.placed;
                update((x) => {
                  if (x.type !== "primitive" || (x.shape.type !== "bolt" && x.shape.type !== "nut")) return;
                  if (!link || !src) {
                    if (placed) [x.origin, x.z, x.x] = placed;
                    x.link = null;
                    return;
                  }
                  const m = METRIC_HOLES.find((h) => Math.abs(h.nominal - src.spec.nominal) < 1e-6);
                  if (m) x.shape.size = m.size;
                  const size = x.shape.size;
                  let offset = 0;
                  if (x.shape.type === "bolt") {
                    // Ciego: el más largo que entra; pasante o tuerca: el que llega al otro lado
                    x.shape.length = src.blind
                      ? ([...BOLT_LENGTHS].reverse().find((l) => l <= src.length + 1e-6) ?? BOLT_LENGTHS[0])
                      : boltLengthFor(size, src.length, false);
                  } else if (src.placed) {
                    // Tuerca en un tornillo: cerca de la punta
                    offset = Math.max(0, +(src.length - (NUT_HEIGHT[size] ?? src.spec.nominal) - 2 * src.spec.pitch).toFixed(2));
                  }
                  x.shape.modeled = src.modeled || undefined;
                  // Por la boca más cercana a donde estaba la pieza (p. ej. el borde elegido al crearla)
                  let flip: boolean | undefined;
                  if (placed) {
                    const d = (a: P3, b: P3) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);
                    const far: P3 = [0, 1, 2].map((i) => src.mouth[i] - src.out[i] * src.length) as P3;
                    // (solo si ya estaba apoyada en la otra boca, mirando hacia afuera de ella)
                    const back = placed[1][0] * src.out[0] + placed[1][1] * src.out[1] + placed[1][2] * src.out[2] < -0.9;
                    flip = (back && d(placed[0], far) < Math.min(d(placed[0], src.mouth), src.spec.nominal)) || undefined;
                  }
                  x.link = { ...link, offset, flip };
                });
              };
              return (
                <>
                  {/* Pieza estándar: medida, cabeza y rosca */}
                  <Show when={threaded()}>
                    <ThreadLinkBox
                      store={props.store}
                      ui={props.ui}
                      feature={f().id}
                      value={k().link}
                      want={k().shape.type === "bolt" ? "internal" : "external"}
                      what={k().shape.type === "bolt" ? "el tornillo" : "la tuerca"}
                      askAlways
                      onChange={setLink}
                    />
                  </Show>
                  <Show when={standard()}>
                    {(st) => (
                      <>
                        <Show when={!linked()}>
                          <Row label="Medida">
                            <Select
                              options={METRIC_HOLES.map((m) => ({ value: m.size, label: m.size }))}
                              value={st().size}
                              onChange={(v) => update((x) => x.type === "primitive" && "size" in x.shape && (x.shape.size = v))}
                            />
                          </Row>
                        </Show>
                        <Show when={st().type === "bolt" && (st() as Extract<PrimitiveShape, { type: "bolt" }>)}>
                          {(b) => (
                            <Row label="Cabeza">
                              <Select
                                options={BOLT_HEADS}
                                value={b().head ?? "socket"}
                                onChange={(v) => update((x) => x.type === "primitive" && x.shape.type === "bolt" && (x.shape.head = v as BoltHead))}
                              />
                            </Row>
                          )}
                        </Show>
                      </>
                    )}
                  </Show>
                  <For each={fields[k().shape.type]}>
                    {([key, label]) => (
                      field(label, `kind.shape.${key}`, (k().shape as unknown as Record<string, number>)[key], (x, v) => {
                        if (x.type === "primitive") (x.shape as unknown as Record<string, number>)[key] = v;
                      }, "mm")
                    )}
                  </For>
                  <Show when={k().shape.type === "bolt" || k().shape.type === "nut"}>
                    <Tooltip content="El filete de verdad (para imprimir); sin marcar, un cilindro liso, más rápido">
                      <Checkbox
                        small
                        label="Rosca modelada"
                        checked={!!(standard() as { modeled?: boolean } | undefined)?.modeled}
                        onChange={(c) =>
                          update((x) => {
                            if (x.type === "primitive" && (x.shape.type === "bolt" || x.shape.type === "nut")) x.shape.modeled = c || undefined;
                          })
                        }
                      />
                    </Tooltip>
                  </Show>
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
                  <Show when={linked() && k().link}>
                    {(l) => (
                      <>
                        {field(
                          k().shape.type === "bolt" ? "Hundido" : "Hasta",
                          "kind.link.offset",
                          l().offset ?? 0,
                          (x, v) => x.type === "primitive" && x.link && (x.link.offset = v),
                          "mm",
                        )}
                        <Checkbox small label="Por la otra boca" checked={!!l().flip} onChange={(c) => update((x) => x.type === "primitive" && !!x.link && (x.link.flip = c || undefined))} />
                        <p class="text-[11px] text-text-dim">
                          {k().shape.type === "bolt"
                            ? "Va en el eje de la rosca, con la cabeza en la boca; «Hundido» lo mete más (negativo: queda afuera)."
                            : "Va en el eje de la rosca; «Hasta» es cuánto avanza desde la boca."}
                        </p>
                      </>
                    )}
                  </Show>
                  <Show when={!linked()}>
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
                  </Show>
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
                  <Show when={kind.type === "chamfer"}>
                    <Row label="Tipo">
                      <Select
                        options={[
                          { value: "equal", label: "Igual a los dos lados" },
                          { value: "distance", label: "Dos distancias" },
                          { value: "angle", label: "Distancia y ángulo" },
                        ]}
                        value={(k() as { second?: ChamferSecond | null }).second?.type ?? "equal"}
                        onChange={(v) =>
                          update((x) => {
                            if (x.type !== "chamfer" || v === (x.second?.type ?? "equal")) return;
                            const flip = x.second?.flip ?? false;
                            x.second = v === "distance" ? { type: "distance", distance: x.distance * 2, flip } : v === "angle" ? { type: "angle", degrees: 45, flip } : null;
                          })
                        }
                      />
                    </Row>
                  </Show>
                  {field(kind.type === "fillet" ? ((k() as { radius2?: number | null }).radius2 != null ? "Radio al comienzo" : "Radio") : "Distancia", kind.type === "fillet" ? "kind.radius" : "kind.distance", value(), (x, v) => {
                    if (x.type === "fillet") x.radius = v;
                    else if (x.type === "chamfer") x.distance = v;
                  }, "mm")}
                  <Show when={kind.type === "fillet"}>
                    <Checkbox
                      small
                      label="Radio variable"
                      checked={(k() as { radius2?: number | null }).radius2 != null}
                      onChange={(c) => update((x) => x.type === "fillet" && (x.radius2 = c ? x.radius * 2 : null))}
                    />
                    <Show when={(k() as { radius2?: number | null }).radius2 != null}>
                      {field("Radio al final", "kind.radius2", (k() as { radius2: number }).radius2, (x, v) => x.type === "fillet" && (x.radius2 = v), "mm")}
                    </Show>
                  </Show>
                  <Show when={kind.type === "chamfer" && (k() as { second?: ChamferSecond | null }).second}>
                    {(sec) => (
                      <>
                        <Show
                          when={sec().type === "angle"}
                          fallback={field("Otra distancia", "kind.second.distance", (sec() as { distance: number }).distance, (x, v) => x.type === "chamfer" && x.second?.type === "distance" && (x.second.distance = v), "mm")}
                        >
                          {field("Ángulo", "kind.second.degrees", (sec() as { degrees: number }).degrees, (x, v) => x.type === "chamfer" && x.second?.type === "angle" && (x.second.degrees = v), "°")}
                        </Show>
                        <Checkbox small label="Medir desde la otra cara" checked={sec().flip} onChange={(c) => update((x) => x.type === "chamfer" && !!x.second && (x.second.flip = c))} />
                      </>
                    )}
                  </Show>
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
                <Show when={k().pattern.type === "table" && (k().pattern as Extract<PatternKind, { type: "table" }>)}>
                  {(t) => (
                    <div class="space-y-1" aria-label="Tabla de copias">
                      <span class="text-xs text-text-muted">Desplazamiento de cada copia (mm)</span>
                      <For each={t().offsets}>
                        {(row, i) => (
                          <div class="flex items-center gap-1" data-table-row={i()}>
                            <For each={[0, 1, 2]}>
                              {(c) => (
                                <NumberInput
                                  value={row[c]}
                                  step={1}
                                  label={["X", "Y", "Z"][c]}
                                  onChange={(v) => update((x) => x.type === "pattern" && x.pattern.type === "table" && (x.pattern.offsets[i()][c] = v))}
                                />
                              )}
                            </For>
                            <IconButton
                              aria-label={`Quitar la fila ${i() + 1}`}
                              size="sm"
                              variant="ghost"
                              onClick={() => update((x) => x.type === "pattern" && x.pattern.type === "table" && x.pattern.offsets.splice(i(), 1))}
                            >
                              <Icons.Trash size={11} />
                            </IconButton>
                          </div>
                        )}
                      </For>
                      <Button
                        size="sm"
                        fullWidth
                        onClick={() =>
                          update((x) => {
                            if (x.type !== "pattern" || x.pattern.type !== "table") return;
                            const last = x.pattern.offsets[x.pattern.offsets.length - 1] ?? [0, 0, 0];
                            x.pattern.offsets.push([last[0] + 10, last[1], last[2]]);
                          })
                        }
                      >
                        Agregar fila
                      </Button>
                    </div>
                  )}
                </Show>
                <Show when={k().pattern.type === "fill" && (k().pattern as Extract<PatternKind, { type: "fill" }>)}>
                  {(fl) => (
                    <>
                      <Row label="Región">
                        <Select
                          options={sketchOptions()}
                          value={String(fl().sketch)}
                          onChange={(v) => update((x) => x.type === "pattern" && x.pattern.type === "fill" && ((x.pattern.sketch = +v), (x.pattern.regions = { type: "all" })))}
                        />
                      </Row>
                      <RegionBox
                        ui={props.ui}
                        store={props.store}
                        owner={`${f().id}:relleno`}
                        lost={lost("regions")}
                        sketch={fl().sketch}
                        value={fl().regions}
                        onChange={(r) => update((x) => x.type === "pattern" && x.pattern.type === "fill" && (x.pattern.regions = r))}
                      />
                      {field("Separación", "kind.pattern.spacing", fl().spacing, (x, v) => x.type === "pattern" && x.pattern.type === "fill" && (x.pattern.spacing = v), "mm")}
                      {field("Margen al borde", "kind.pattern.margin", fl().margin, (x, v) => x.type === "pattern" && x.pattern.type === "fill" && (x.pattern.margin = v), "mm")}
                      <Checkbox small label="Grilla hexagonal" checked={fl().hex} onChange={(c) => update((x) => x.type === "pattern" && x.pattern.type === "fill" && (x.pattern.hex = c))} />
                      <p class="text-[11px] text-text-dim">
                        Copias a la separación elegida dentro de la región, con el centro a no menos del margen del borde. La grilla pasa por la operación
                        original.
                      </p>
                    </>
                  )}
                </Show>
                <Show when={k().pattern.type !== "table" && k().pattern.type !== "fill"}>
                <Show
                  when={k().pattern.type !== "curve"}
                  fallback={
                    <>
                      <Row label="A lo largo de">
                        <Select
                          options={[...sketchOptions(), ...refOptions(props.store, "curve").map((o) => ({ value: o.value.replace("ref:", "curve:"), label: o.label }))]}
                          value={(() => {
                            const p = (k().pattern as Extract<PatternKind, { type: "curve" }>).path;
                            return p.type === "sketch" ? String(p.sketch) : `curve:${p.feature}`;
                          })()}
                          onChange={(v) =>
                            update(
                              (x) =>
                                x.type === "pattern" &&
                                x.pattern.type === "curve" &&
                                (x.pattern.path = v.startsWith("curve:") ? { type: "curve", feature: +v.slice(6) } : { type: "sketch", sketch: +v, entities: [] }),
                            )
                          }
                        />
                      </Row>
                      {field("Cantidad", "kind.pattern.count", (k().pattern as { count: number }).count, (x, v) => x.type === "pattern" && "count" in x.pattern && (x.pattern.count = Math.max(2, Math.round(v))))}
                      <p class="text-[11px] text-text-dim">Las copias se reparten de punta a punta del camino, sin girar.</p>
                    </>
                  }
                >
                <Show
                  when={k().pattern.type === "linear" && (k().pattern as Extract<PatternKind, { type: "linear" }>)}
                  fallback={
                    <>
                      <Row label="Eje">
                        <Select
                          options={[...["x", "y", "z"].map((a) => ({ value: a, label: a.toUpperCase() })), ...refOptions(props.store, "axis")]}
                          value={(() => {
                            const a = (k().pattern as { axis: AxisSpec }).axis;
                            return a.type === "reference" ? `ref:${a.feature}` : a.type;
                          })()}
                          onChange={(v) =>
                            update(
                              (x) =>
                                x.type === "pattern" &&
                                x.pattern.type === "circular" &&
                                (x.pattern.axis = v.startsWith("ref:") ? { type: "reference", feature: +v.slice(4) } : { type: v as "x" | "y" | "z" }),
                            )
                          }
                        />
                      </Row>
                      {field("Cantidad", "kind.pattern.count", (k().pattern as { count: number }).count, (x, v) => x.type === "pattern" && "count" in x.pattern && (x.pattern.count = Math.max(2, Math.round(v))))}
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
                      {field("Cantidad", "kind.pattern.count", lin().count, (x, v) => x.type === "pattern" && "count" in x.pattern && (x.pattern.count = Math.max(2, Math.round(v))))}
                      {field("Separación", "kind.pattern.spacing", lin().spacing, (x, v) => x.type === "pattern" && x.pattern.type === "linear" && (x.pattern.spacing = v), "mm")}
                    </>
                  )}
                </Show>
                </Show>
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
                return p.type === "face" || p.type === "reference" ? "xy" : p.type;
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
          <Match when={f().kind.type === "boolean" && (f().kind as Extract<FeatureKind, { type: "boolean" }>)}>
            {(k) => (
              <>
                <Row label="Operación">
                  <Select
                    options={[
                      { value: "union", label: "Unir" },
                      { value: "subtract", label: "Restar" },
                      { value: "intersect", label: "Intersecar" },
                    ]}
                    value={k().op}
                    onChange={(v) => update((x) => x.type === "boolean" && (x.op = v as "union" | "subtract" | "intersect"))}
                  />
                </Row>
                <PartChecklist
                  store={props.store}
                  featureId={f().id}
                  label={k().op === "subtract" ? "Piezas que quedan" : "Piezas"}
                  empty="Elegir las piezas"
                  value={k().targets}
                  missing={lost("targets")}
                  onChange={(v) => update((x) => x.type === "boolean" && (x.targets = v))}
                />
                <Show when={k().op !== "union" || k().tools.length > 0}>
                  <PartChecklist
                    store={props.store}
                    featureId={f().id}
                    label={k().op === "subtract" ? "Piezas que restan" : "Con"}
                    empty={k().op === "subtract" ? "Elegir con qué restar" : "Ninguna más"}
                    value={k().tools}
                    missing={lost("tools")}
                    onChange={(v) => update((x) => x.type === "boolean" && (x.tools = v))}
                  />
                </Show>
                <Show when={k().op === "subtract"}>
                  <Checkbox small label="Conservar las que restan" checked={k().keep_tools} onChange={(c) => update((x) => x.type === "boolean" && (x.keep_tools = c))} />
                </Show>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "split_by" && (f().kind as Extract<FeatureKind, { type: "split_by" }>)}>
            {(k) => (
              <>
                <PartChecklist
                  store={props.store}
                  featureId={f().id}
                  label="Piezas a partir"
                  empty="Todos los sólidos"
                  value={k().parts}
                  missing={lost("parts")}
                  onChange={(v) => update((x) => x.type === "split_by" && (x.parts = v))}
                />
                <Row label="Con">
                  <Select
                    options={[
                      { value: "plane", label: "Un plano" },
                      ...(props.store.result()?.parts ?? []).map((p) => ({ value: `part:${partKey(p.id)}`, label: `${p.name}${p.surface ? " (superficie)" : ""}` })),
                    ]}
                    value={k().tool.type === "plane" ? "plane" : `part:${partKey((k().tool as { part: PartId }).part)}`}
                    onChange={(v) =>
                      update((x) => {
                        if (x.type !== "split_by") return;
                        if (v === "plane") x.tool = { type: "plane", plane: { type: "custom", plane: offsetPlane("xy", 0) } };
                        else {
                          const part = (props.store.result()?.parts ?? []).find((p) => `part:${partKey(p.id)}` === v);
                          if (part) x.tool = { type: "part", part: part.id };
                        }
                      })
                    }
                  />
                </Row>
                <Show when={k().tool.type === "plane" && (k().tool as { plane: PlaneSpec })}>
                  {(t) => <Row label="Plano">{planeSpecSelect(t().plane, (p) => update((x) => x.type === "split_by" && x.tool.type === "plane" && (x.tool.plane = p)), lost("plane").length > 0)}</Row>}
                </Show>
                <p class="text-[11px] text-text-dim">
                  Cada pedazo queda como una pieza. Con otra pieza (una superficie curva, por ejemplo) se parte por su forma; la herramienta queda como
                  estaba. Las superficies se recortan con Superficies → Recortar.
                </p>
              </>
            )}
          </Match>
          <Match when={(f().kind.type === "split_parts" || f().kind.type === "delete_parts") && f().kind}>
            {(k) => {
              const kind = k() as Extract<FeatureKind, { type: "split_parts" | "delete_parts" }>;
              return (
                <PartChecklist
                  store={props.store}
                  featureId={f().id}
                  label={kind.type === "split_parts" ? "Piezas a separar" : "Piezas a borrar"}
                  empty={kind.type === "split_parts" ? "Todas" : "Elegir las piezas"}
                  value={kind.parts}
                  missing={lost("parts")}
                  onChange={(v) => update((x) => (x.type === "split_parts" || x.type === "delete_parts") && (x.parts = v))}
                />
              );
            }}
          </Match>
          <Match when={f().kind.type === "hole" && (f().kind as Extract<FeatureKind, { type: "hole" }>)}>
            {(k) => {
              // Tamaño estándar que corresponde a los diámetros actuales (si alguno)
              const fit = (): { size: string; mode: "clearance" | "tap" | "modeled" } | null => {
                const mod = k().modeled;
                if (mod) return { size: METRIC_HOLES.find((m) => Math.abs(m.nominal - mod.nominal) < 1e-6)?.size ?? "", mode: "modeled" };
                for (const m of METRIC_HOLES) {
                  if (Math.abs(m.clearance - k().diameter) < 1e-6) return { size: m.size, mode: "clearance" };
                  if (Math.abs(m.tap - k().diameter) < 1e-6) return { size: m.size, mode: "tap" };
                }
                return null;
              };
              const applySize = (size: string, mode: "clearance" | "tap" | "modeled") => {
                const m = METRIC_HOLES.find((x) => x.size === size);
                if (!m) return;
                update((x) => {
                  if (x.type !== "hole") return;
                  x.diameter = mode === "clearance" ? m.clearance : m.tap;
                  x.thread = mode === "clearance" ? null : m.size;
                  // Al cambiar de medida la rosca modelada toma el paso grueso y conserva la holgura
                  x.modeled = mode === "modeled" ? { nominal: m.nominal, pitch: m.pitch, clearance: x.modeled?.clearance ?? 0, left: x.modeled?.left ?? false } : null;
                  if (x.style.type === "counterbore") x.style = { type: "counterbore", diameter: m.cbore, depth: m.cboreDepth };
                  if (x.style.type === "countersink") x.style = { type: "countersink", diameter: m.csink, angle: 90 };
                });
              };
              const view = () => props.store.sketchView(k().sketch);
              const sketchPoints = () => {
                const s = props.sketches.find((x) => x.id === k().sketch);
                return s?.kind.type === "sketch" ? s.kind.sketch.entities.filter((e) => e.geometry.type === "point" && !e.construction) : [];
              };
              return (
                <>
                  <Row label="Centros en">
                    <Select options={sketchOptions()} value={String(k().sketch)} onChange={(v) => update((x) => x.type === "hole" && ((x.sketch = +v), (x.points = [])))} />
                  </Row>
                  <p class="text-[11px] text-text-dim">
                    {sketchPoints().length ? `${sketchPoints().length} puntos del sketch` : `${view()?.sketch.entities.filter((e) => e.geometry.type === "circle").length ?? 0} centros de círculos`}
                  </p>
                  <Row label="Tipo">
                    <Select
                      options={[
                        { value: "simple", label: "Simple" },
                        { value: "counterbore", label: "Con caja" },
                        { value: "countersink", label: "Avellanado" },
                      ]}
                      value={k().style.type}
                      onChange={(v) => {
                        if (v === k().style.type) return;
                        const m = METRIC_HOLES.find((x) => x.size === fit()?.size) ?? METRIC_HOLES.find((x) => x.size === "M6")!;
                        update((x) => {
                          if (x.type !== "hole") return;
                          x.style =
                            v === "counterbore"
                              ? { type: "counterbore", diameter: m.cbore, depth: m.cboreDepth }
                              : v === "countersink"
                                ? { type: "countersink", diameter: m.csink, angle: 90 }
                                : { type: "simple" };
                        });
                      }}
                    />
                  </Row>
                  <Show when={k().thread || k().modeled}>
                    <ThreadLinkBox
                      store={props.store}
                      ui={props.ui}
                      feature={f().id}
                      value={k().link}
                      want="any"
                      what="el agujero"
                      onChange={(link, src) =>
                        update((x) => {
                          if (x.type !== "hole") return;
                          x.link = link;
                          if (!src) return;
                          if (x.modeled) x.modeled = { ...src.spec, clearance: x.modeled.clearance };
                          else {
                            x.thread = threadName(src.spec);
                            x.diameter = src.spec.nominal - src.spec.pitch;
                          }
                        })
                      }
                    />
                  </Show>
                  <Show when={!k().link}>
                  <Row label="Tamaño">
                    <Select
                      options={[{ value: "", label: "A medida" }, ...METRIC_HOLES.map((m) => ({ value: m.size, label: m.size }))]}
                      value={fit()?.size ?? ""}
                      onChange={(v) => v && applySize(v, fit()?.mode ?? "clearance")}
                    />
                  </Row>
                  </Show>
                  <Show when={fit()}>
                    {(fi) => (
                      <Row label="Para">
                        <Select
                          options={[
                            { value: "clearance", label: "Que pase el tornillo" },
                            { value: "tap", label: "Roscar (rosca cosmética)" },
                            { value: "modeled", label: "Roscar (rosca modelada)" },
                          ]}
                          value={fi().mode}
                          onChange={(v) => applySize(fi().size || "M6", v as "clearance" | "tap" | "modeled")}
                        />
                      </Row>
                    )}
                  </Show>
                  <Show
                    when={k().modeled}
                    fallback={field("Diámetro", "kind.diameter", k().diameter, (x, v) => x.type === "hole" && (x.diameter = v), "mm")}
                  >
                    {(t) => (
                      <>
                        <Show when={!k().link}>
                          {field("Paso", "kind.modeled.pitch", t().pitch, (x, v) => x.type === "hole" && x.modeled && (x.modeled.pitch = v), "mm")}
                        </Show>
                        {field("Holgura", "kind.modeled.clearance", t().clearance, (x, v) => x.type === "hole" && x.modeled && (x.modeled.clearance = v), "mm")}
                        <Show when={!k().link}>
                          <Checkbox small label="A izquierdas" checked={t().left} onChange={(c) => update((x) => x.type === "hole" && !!x.modeled && (x.modeled.left = c))} />
                        </Show>
                        <p class="text-[11px] text-text-dim">
                          M{t().nominal} × {t().pitch}: el agujero sale con el filete (diámetro menor {fmt(t().nominal - 1.082532 * t().pitch + t().clearance)} mm). Para imprimir en FDM conviene
                          una holgura de 0,2–0,4 mm. Cada vuelta del filete tarda en calcularse.
                        </p>
                      </>
                    )}
                  </Show>
                  <Row label="Profundidad">
                    <Select
                      options={[
                        { value: "through_all", label: "Pasante" },
                        { value: "blind", label: "Ciega" },
                      ]}
                      value={k().depth.type}
                      onChange={(v) => v !== k().depth.type && update((x) => x.type === "hole" && (x.depth = v === "blind" ? { type: "blind", depth: 10 } : { type: "through_all" }))}
                    />
                  </Row>
                  {/* Sin elegir, hacia el lado donde hay material; la flecha del visor también lo da vuelta */}
                  <div class="flex items-center justify-between gap-2">
                    <Checkbox
                      small
                      label="Hacia el otro lado"
                      checked={k().reverse ?? state()?.handle?.reversed ?? false}
                      onChange={(c) => update((x) => x.type === "hole" && (x.reverse = c))}
                    />
                    <Show when={k().reverse != null} fallback={<span class="text-[11px] text-text-dim">Automático</span>}>
                      <button
                        class="text-[11px] text-text-muted hover:text-text"
                        title="Que vuelva a ir hacia el lado donde hay material"
                        onClick={() => update((x) => x.type === "hole" && (x.reverse = null))}
                      >
                        Automático
                      </button>
                    </Show>
                  </div>
                  <Show when={k().depth.type === "blind"}>
                    {field("Hasta", "kind.depth.depth", (k().depth as { depth: number }).depth, (x, v) => x.type === "hole" && x.depth.type === "blind" && (x.depth.depth = v), "mm")}
                    {field("Punta", "kind.tip_angle", k().tip_angle, (x, v) => x.type === "hole" && (x.tip_angle = v), "°")}
                  </Show>
                  <Show when={k().style.type === "counterbore" && (k().style as { diameter: number; depth: number })}>
                    {(st) => (
                      <>
                        {field("Caja: diámetro", "kind.style.diameter", st().diameter, (x, v) => x.type === "hole" && x.style.type === "counterbore" && (x.style.diameter = v), "mm")}
                        {field("Caja: profundidad", "kind.style.depth", st().depth, (x, v) => x.type === "hole" && x.style.type === "counterbore" && (x.style.depth = v), "mm")}
                      </>
                    )}
                  </Show>
                  <Show when={k().style.type === "countersink" && (k().style as { diameter: number; angle: number })}>
                    {(st) => (
                      <>
                        {field("Avellanado: diámetro", "kind.style.diameter", st().diameter, (x, v) => x.type === "hole" && x.style.type === "countersink" && (x.style.diameter = v), "mm")}
                        {field("Avellanado: ángulo", "kind.style.angle", st().angle, (x, v) => x.type === "hole" && x.style.type === "countersink" && (x.style.angle = v), "°")}
                      </>
                    )}
                  </Show>
                  <Show when={k().thread && !k().modeled}>
                    <p class="text-[11px] text-text-dim">Rosca {k().thread} (cosmética: no se modela)</p>
                  </Show>
                </>
              );
            }}
          </Match>
          <Match when={f().kind.type === "sketch3d" && f()}>{(ff) => <Sketch3dEditor store={props.store} ui={props.ui} feature={ff()} />}</Match>
          <Match when={f().kind.type === "surface_sketch" && (f().kind as Extract<FeatureKind, { type: "surface_sketch" }>)}>
            {(k) => (
              <>
                <SelectionBox
                  store={props.store}
                  ui={props.ui}
                  owner={`${f().id}:cara`}
                  kind="face"
                  label="Cara"
                  lost={lost("face")}
                  refs={[k().face]}
                  onChange={(refs) => refs[0] && update((x) => x.type === "surface_sketch" && (x.face = refs[0] as FaceRef))}
                />
                <Button size="sm" fullWidth onClick={() => void props.store.acceptDraft().then(() => props.ui.editSketch(f().id))}>
                  Editar el sketch
                </Button>
                <p class="text-[11px] text-text-dim">
                  Se dibuja en el desarrollo de la cara (el plano tangente en su centro: X a lo largo de la cara, Y a lo ancho) y las curvas se llevan sobre ella midiendo sobre la superficie. Sirven de camino para un Barrido.
                </p>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "helix" && (f().kind as Extract<FeatureKind, { type: "helix" }>)}>
            {(k) => (
              <>
                <AxisField store={props.store} ui={props.ui} owner={`${f().id}:eje`} label="Eje" value={k().axis} except={f().id} onChange={(a) => update((x) => x.type === "helix" && (x.axis = a))} />
                {field("Radio", "kind.radius", k().radius, (x, v) => x.type === "helix" && (x.radius = v), "mm")}
                {field("Paso", "kind.pitch", k().pitch, (x, v) => x.type === "helix" && (x.pitch = v), "mm")}
                {field("Vueltas", "kind.turns", k().turns, (x, v) => x.type === "helix" && (x.turns = v))}
                <Checkbox small label="A izquierdas" checked={k().left} onChange={(c) => update((x) => x.type === "helix" && (x.left = c))} />
                <p class="text-[11px] text-text-dim">Para un resorte: un círculo en un plano que pase por el eje, en el arranque de la hélice, y un Barrido con la hélice como camino.</p>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "move_face" && (f().kind as Extract<FeatureKind, { type: "move_face" }>)}>
            {(k) => (
              <>
                <SelectionBox
                  store={props.store}
                  ui={props.ui}
                  owner={`${f().id}:caras`}
                  kind="faces"
                  label="Caras planas"
                  refs={k().faces}
                  lost={lost("faces")}
                  onChange={(refs) => update((x) => x.type === "move_face" && (x.faces = refs as FaceRef[]))}
                />
                {field("Distancia", "kind.distance", k().distance, (x, v) => x.type === "move_face" && (x.distance = v), "mm")}
                <p class="text-[11px] text-text-dim">Positiva: hacia afuera (suma material); negativa: hacia adentro.</p>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "sheet_metal" && (f().kind as Extract<FeatureKind, { type: "sheet_metal" }>)}>
            {(k) => (
              <>
                <Row label="Sketch">
                  <Select options={sketchOptions()} value={String(k().sketch)} onChange={(v) => update((x) => x.type === "sheet_metal" && (x.sketch = +v))} />
                </Row>
                <RegionBox
                  ui={props.ui}
                  store={props.store}
                  owner={`${f().id}:regiones`}
                  lost={lost("regions")}
                  sketch={k().sketch}
                  value={k().regions}
                  onChange={(r) => update((x) => x.type === "sheet_metal" && (x.regions = r))}
                />
                {field("Espesor", "kind.thickness", k().thickness, (x, v) => x.type === "sheet_metal" && (x.thickness = v), "mm")}
                {field("Radio de doblez", "kind.radius", k().radius, (x, v) => x.type === "sheet_metal" && (x.radius = v), "mm")}
                {field("Factor K", "kind.k_factor", k().k_factor, (x, v) => x.type === "sheet_metal" && (x.k_factor = v))}
                <Checkbox small label="Hacia el otro lado" checked={k().flip} onChange={(c) => update((x) => x.type === "sheet_metal" && (x.flip = c))} />
                <Row label="Con el sólido">{opSelect(k().op, (o) => update((x) => x.type === "sheet_metal" && (x.op = o)))}</Row>
                <p class="text-[11px] text-text-dim">
                  El radio es el interior de los dobleces de sus pestañas. El factor K dice dónde queda la fibra neutra (0,44 para acero y aluminio
                  doblados en plegadora; 0,5 si el radio es grande): con él se calcula el largo del desarrollo.
                </p>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "flange" && (f().kind as Extract<FeatureKind, { type: "flange" }>)}>
            {(k) => {
              // El radio de la chapa de la que sale (la última antes)
              const sheet = () => {
                const list = props.store.doc()?.features ?? [];
                const before = list.slice(0, list.findIndex((x) => x.id === f().id));
                const sm = [...before].reverse().find((x) => x.kind.type === "sheet_metal");
                return sm?.kind.type === "sheet_metal" ? sm.kind : undefined;
              };
              const sheetRadius = () => sheet()?.radius;
              const sheetThickness = () => sheet()?.thickness;
              return (
                <>
                  <SelectionBox
                    store={props.store}
                    ui={props.ui}
                    owner={`${f().id}:arista`}
                    kind="edges"
                    label="Arista del borde"
                    refs={k().edge ? [k().edge!] : []}
                    lost={lost("edge")}
                    onChange={(refs) => update((x) => x.type === "flange" && (x.edge = (refs[refs.length - 1] as EdgeRef | undefined) ?? null))}
                  />
                  {field("Largo", "kind.length", k().length, (x, v) => x.type === "flange" && (x.length = v), "mm")}
                  {field("Ángulo", "kind.angle", k().angle, (x, v) => x.type === "flange" && (x.angle = v), "°")}
                  {field("Radio", "kind.radius", k().radius ?? sheetRadius() ?? 0, (x, v) => x.type === "flange" && (x.radius = v), "mm")}
                  <Checkbox small label="Doblar hacia el otro lado" checked={k().flip} onChange={(c) => update((x) => x.type === "flange" && (x.flip = c))} />
                  <p class="text-[11px] text-text-dim">
                    Dobla hacia el lado de la cara de la arista elegida. El largo es la pared después del doblez; el radio, el interior (el de la chapa si
                    no se cambia).
                  </p>
                  <Row label="Alivio">
                    <Select
                      options={RELIEF_OPTIONS}
                      value={k().relief ?? "rectangle"}
                      onChange={(v) => update((x) => x.type === "flange" && (x.relief = v as BendRelief))}
                    />
                  </Row>
                  <Show when={(k().relief ?? "rectangle") !== "none"}>
                    {field("Ancho del alivio", "kind.relief_width", k().relief_width ?? sheetThickness() ?? 0, (x, v) => x.type === "flange" && (x.relief_width = v), "mm")}
                  </Show>
                  <Row label="Esquina">
                    <Select
                      options={CORNER_OPTIONS}
                      value={k().corner_relief ?? "none"}
                      onChange={(v) => update((x) => x.type === "flange" && (x.corner_relief = v as CornerRelief))}
                    />
                  </Row>
                  <Checkbox
                    small
                    label="Esquina cerrada con otra pestaña"
                    checked={!!k().closed_corner}
                    onChange={(c) => update((x) => x.type === "flange" && (x.closed_corner = c))}
                  />
                  <Show when={k().closed_corner}>
                    {field("Holgura de la esquina", "kind.corner_gap", k().corner_gap ?? 0.2, (x, v) => x.type === "flange" && (x.corner_gap = v), "mm")}
                  </Show>
                  <Show when={(k().corner_relief ?? "none") !== "none"}>
                    {field(
                      k().corner_relief === "square" ? "Medio lado" : "Radio del alivio",
                      "kind.corner_size",
                      k().corner_size ?? sheetThickness() ?? 0,
                      (x, v) => x.type === "flange" && (x.corner_size = v),
                      "mm",
                    )}
                  </Show>
                  <p class="text-[11px] text-text-dim">
                    El alivio es una ranura en el extremo donde la chapa sigue al lado del doblez (una esquina hacia adentro): sin ella el doblez quedaría
                    pegado y la chapa se rasgaría. Donde se junta con otra pestaña queda una muesca cuadrada del largo del doblez; también puede ser
                    redonda o cuadrada más grande. El ancho, si no se cambia, es el espesor.
                  </p>
                </>
              );
            }}
          </Match>
          <Match when={f().kind.type === "thread" && (f().kind as Extract<FeatureKind, { type: "thread" }>)}>
            {(k) => (
              <>
                <SelectionBox
                  store={props.store}
                  ui={props.ui}
                  owner={`${f().id}:cara`}
                  kind="face"
                  label="Cara cilíndrica"
                  refs={[k().face]}
                  lost={lost("face")}
                  onChange={(refs) => refs.length && update((x) => x.type === "thread" && (x.face = refs[refs.length - 1] as FaceRef))}
                />
                <ThreadLinkBox
                  store={props.store}
                  ui={props.ui}
                  feature={f().id}
                  value={k().link}
                  want={(() => {
                    // Un agujero se coordina con un macho y un eje con una hembra
                    const own = props.store.result()?.threads?.find((t) => t.feature === f().id);
                    return own ? (own.internal ? "external" : "internal") : "any";
                  })()}
                  what="la rosca"
                  onChange={(link, src) =>
                    update((x) => {
                      if (x.type !== "thread") return;
                      x.link = link;
                      if (src) {
                        x.pitch = src.spec.pitch;
                        x.left = src.spec.left;
                      }
                    })
                  }
                />
                <Show when={!k().link}>
                <Row label="Medida">
                  <Select
                    options={[{ value: "", label: "Paso a medida" }, ...METRIC_HOLES.map((m) => ({ value: String(m.pitch), label: `${m.size} × ${String(m.pitch).replace(".", ",")}` }))]}
                    value={METRIC_HOLES.some((m) => m.pitch === k().pitch) ? String(k().pitch) : ""}
                    onChange={(v) => v && update((x) => x.type === "thread" && (x.pitch = +v))}
                  />
                </Row>
                {field("Paso", "kind.pitch", k().pitch, (x, v) => x.type === "thread" && (x.pitch = v), "mm")}
                </Show>
                {field("Largo", "kind.length", k().length, (x, v) => x.type === "thread" && (x.length = v), "mm")}
                {field("Holgura", "kind.clearance", k().clearance, (x, v) => x.type === "thread" && (x.clearance = v), "mm")}
                <Checkbox small label="Desde el otro extremo" checked={k().flip} onChange={(c) => update((x) => x.type === "thread" && (x.flip = c))} />
                <Show when={!k().link}>
                  <Checkbox small label="A izquierdas" checked={k().left} onChange={(c) => update((x) => x.type === "thread" && (x.left = c))} />
                </Show>
                <p class="text-[11px] text-text-dim">
                  En un eje, el cilindro es el diámetro nominal (Ø6 para M6); en un agujero, el diámetro menor (5 mm para M6). Largo 0: toda la cara. La
                  holgura achica la rosca de un eje y agranda la de un agujero (0,2–0,4 mm para imprimir en FDM).
                </p>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "rib" && (f().kind as Extract<FeatureKind, { type: "rib" }>)}>
            {(k) => (
              <>
                <Row label="Sketch">
                  <Select options={sketchOptions()} value={String(k().sketch)} onChange={(v) => update((x) => x.type === "rib" && (x.sketch = +v))} />
                </Row>
                {field("Espesor", "kind.thickness", k().thickness, (x, v) => x.type === "rib" && (x.thickness = v), "mm")}
                <Checkbox small label="Hacia el otro lado" checked={k().flip} onChange={(c) => update((x) => x.type === "rib" && (x.flip = c))} />
                <p class="text-[11px] text-text-dim">
                  Cada línea del sketch se lleva hasta el sólido, en su plano, con el espesor repartido a los dos lados. El sketch tiene que estar en un
                  plano que corte el sólido.
                </p>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "replace_face" && (f().kind as Extract<FeatureKind, { type: "replace_face" }>)}>
            {(k) => (
              <>
                <SelectionBox
                  store={props.store}
                  ui={props.ui}
                  owner={`${f().id}:caras`}
                  kind="faces"
                  label="Caras planas"
                  refs={k().faces}
                  lost={lost("faces")}
                  onChange={(refs) => update((x) => x.type === "replace_face" && (x.faces = refs as FaceRef[]))}
                />
                <PlaneField
                  store={props.store}
                  ui={props.ui}
                  owner={`${f().id}:hasta`}
                  label="Hasta"
                  value={k().target}
                  except={f().id}
                  lost={lost("target").length > 0}
                  onChange={(p) => update((x) => x.type === "replace_face" && (x.target = p))}
                />
                <p class="text-[11px] text-text-dim">Las caras llegan hasta el plano: suma material si queda afuera, resta si queda adentro.</p>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "scale" && (f().kind as Extract<FeatureKind, { type: "scale" }>)}>
            {(k) => {
              const uniform = () => k().factor[0] === k().factor[1] && k().factor[1] === k().factor[2];
              const [perAxis, setPerAxis] = createSignal(!uniform());
              return (
                <>
                  <Checkbox small label="Distinta en cada eje" checked={perAxis()} onChange={(c) => {
                    setPerAxis(c);
                    if (!c) update((x) => x.type === "scale" && (x.factor = [x.factor[0], x.factor[0], x.factor[0]]));
                  }} />
                  <Show
                    when={perAxis()}
                    fallback={field("Factor", "kind.factor.0", k().factor[0], (x, v) => x.type === "scale" && (x.factor = [v, v, v]))}
                  >
                    <div class="grid grid-cols-3 gap-1">
                      <For each={["X", "Y", "Z"]}>
                        {(axis, i) => field(axis, `kind.factor.${i()}`, k().factor[i()], (x, v) => x.type === "scale" && (x.factor[i()] = v))}
                      </For>
                    </div>
                  </Show>
                  <PointField store={props.store} ui={props.ui} owner={`${f().id}:centro`} label="Centro" value={k().center} except={f().id} onChange={(p) => update((x) => x.type === "scale" && (x.center = p))} />
                </>
              );
            }}
          </Match>
          <Match when={(f().kind.type === "surface_extrude" || f().kind.type === "surface_revolve") && f().kind}>
            {(kk) => {
              type SurfaceKind = Extract<FeatureKind, { type: "surface_extrude" | "surface_revolve" }>;
              const k = () => kk() as SurfaceKind;
              const sketch = () => props.sketches.find((s) => s.id === k().sketch);
              const curves = () => {
                const s = sketch();
                return s?.kind.type === "sketch"
                  ? s.kind.sketch.entities.filter((e) => !e.construction && ["line", "arc", "spline", "circle", "ellipse"].includes(e.geometry.type))
                  : [];
              };
              const NAMES: Record<string, string> = { line: "Línea", arc: "Arco", spline: "Spline", circle: "Círculo", ellipse: "Elipse" };
              const upd = (m: (x: SurfaceKind) => void) => update((x) => (x.type === "surface_extrude" || x.type === "surface_revolve") && m(x));
              const ext = () => (k().type === "surface_extrude" ? (k() as Extract<FeatureKind, { type: "surface_extrude" }>) : undefined);
              const rev = () => (k().type === "surface_revolve" ? (k() as Extract<FeatureKind, { type: "surface_revolve" }>) : undefined);
              const allLines = () => {
                const s = sketch();
                return s?.kind.type === "sketch" ? s.kind.sketch.entities.filter((e) => e.geometry.type === "line") : [];
              };
              return (
                <>
                  <Row label="Sketch">
                    <Select options={sketchOptions()} value={String(k().sketch)} onChange={(v) => upd((x) => ((x.sketch = +v), (x.entities = [])))} />
                  </Row>
                  <div class="space-y-1" aria-label="Curvas">
                    <span class="text-xs text-text-muted">Curvas (sin elegir: todas)</span>
                    <For each={curves()} fallback={<p class="text-[11px] text-text-dim">Ese sketch no tiene curvas</p>}>
                      {(e) => (
                        <Checkbox
                          small
                          label={`${NAMES[e.geometry.type]} ${e.id}`}
                          checked={k().entities.includes(e.id)}
                          onChange={(c) => upd((x) => (x.entities = c ? [...x.entities, e.id] : x.entities.filter((i) => i !== e.id)))}
                        />
                      )}
                    </For>
                  </div>
                  <Show when={ext()}>
                    {(e) => (
                      <>
                        <Row label="Hasta">
                          <Select
                            options={[
                              { value: "blind", label: "Distancia" },
                              { value: "symmetric", label: "Simétrica" },
                              { value: "two_sides", label: "Dos direcciones" },
                            ]}
                            value={e().extent.type}
                            onChange={(v) =>
                              update((x) => {
                                if (x.type !== "surface_extrude" || v === x.extent.type) return;
                                const d = "distance" in x.extent ? x.extent.distance : 10;
                                x.extent = v === "two_sides" ? { type: "two_sides", distance: d, second: d / 2 } : { type: v as "blind" | "symmetric", distance: d };
                              })
                            }
                          />
                        </Row>
                        {field(
                          "Distancia",
                          "kind.extent.distance",
                          (e().extent as { distance: number }).distance,
                          (x, v) => x.type === "surface_extrude" && "distance" in x.extent && (x.extent.distance = v),
                          "mm",
                        )}
                        <Show when={e().extent.type === "two_sides"}>
                          {field(
                            "Hacia atrás",
                            "kind.extent.second",
                            (e().extent as { second: number }).second,
                            (x, v) => x.type === "surface_extrude" && x.extent.type === "two_sides" && (x.extent.second = v),
                            "mm",
                          )}
                        </Show>
                        <Checkbox small label="Hacia el otro lado" checked={e().reverse} onChange={(c) => update((x) => x.type === "surface_extrude" && (x.reverse = c))} />
                      </>
                    )}
                  </Show>
                  <Show when={rev()}>
                    {(r) => (
                      <>
                        <Row label="Eje">
                          <Select
                            options={[
                              ...(r().axis.type === "edge" ? [{ value: "edge", label: "Arista del sólido" }] : []),
                              { value: "x", label: "X" },
                              { value: "y", label: "Y" },
                              { value: "z", label: "Z" },
                              ...allLines().map((l) => ({ value: `line:${l.id}`, label: `Línea ${l.id}${l.construction ? " (construcción)" : ""}` })),
                              ...refOptions(props.store, "axis"),
                            ]}
                            value={(() => {
                              const a = r().axis;
                              return a.type === "sketch_line" ? `line:${a.line}` : a.type === "reference" ? `ref:${a.feature}` : a.type;
                            })()}
                            onChange={(v) =>
                              update((x) => {
                                if (x.type !== "surface_revolve") return;
                                x.axis = v.startsWith("line:")
                                  ? { type: "sketch_line", sketch: x.sketch, line: +v.slice(5) }
                                  : v.startsWith("ref:")
                                    ? { type: "reference", feature: +v.slice(4) }
                                    : { type: v as "x" | "y" | "z" };
                              })
                            }
                          />
                        </Row>
                        {field("Ángulo", "kind.angle", r().angle, (x, v) => x.type === "surface_revolve" && (x.angle = v), "°")}
                      </>
                    )}
                  </Show>
                  <p class="text-[11px] text-text-dim">
                    Una superficie no tiene espesor: no se une a los sólidos ni la cortan sin elegirla. Se cierra con Relleno y Coser, o se le da espesor
                    con Engrosar.
                  </p>
                </>
              );
            }}
          </Match>
          <Match when={f().kind.type === "fill" && (f().kind as Extract<FeatureKind, { type: "fill" }>)}>
            {(k) => (
              <>
                <SelectionBox
                  store={props.store}
                  ui={props.ui}
                  owner={`${f().id}:borde`}
                  kind="edges"
                  label="Aristas del borde"
                  refs={k().edges}
                  lost={lost("edges")}
                  onChange={(refs) => update((x) => x.type === "fill" && (x.edges = refs as EdgeRef[]))}
                />
                <Checkbox
                  small
                  label="Tangente a las caras vecinas"
                  checked={k().tangent}
                  onChange={(c) => update((x) => x.type === "fill" && (x.tangent = c))}
                />
                <p class="text-[11px] text-text-dim">
                  El borde tiene que cerrarse. Plano y sin tangencia queda una cara plana; tangente, sigue la curvatura del borde de cada superficie
                  (no sirve contra una pared perpendicular al relleno).
                </p>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "sew" && (f().kind as Extract<FeatureKind, { type: "sew" }>)}>
            {(k) => (
              <>
                <PartChecklist
                  store={props.store}
                  featureId={f().id}
                  label="Superficies"
                  empty="Elegir las superficies"
                  value={k().parts}
                  missing={lost("parts")}
                  onChange={(v) => update((x) => x.type === "sew" && (x.parts = v))}
                />
                <Checkbox small label="Hacer sólido si cierran" checked={k().solid} onChange={(c) => update((x) => x.type === "sew" && (x.solid = c))} />
                {field("Separación máxima", "kind.tolerance", k().tolerance, (x, v) => x.type === "sew" && (x.tolerance = v), "mm")}
                <p class="text-[11px] text-text-dim">Une los bordes que están a menos de la separación máxima. Las piezas cosidas pasan a ser una.</p>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "thicken" && (f().kind as Extract<FeatureKind, { type: "thicken" }>)}>
            {(k) => (
              <>
                <SelectionBox
                  store={props.store}
                  ui={props.ui}
                  owner={`${f().id}:caras`}
                  kind="faces"
                  label="Caras"
                  refs={k().faces}
                  lost={lost("faces")}
                  onChange={(refs) => update((x) => x.type === "thicken" && (x.faces = refs as FaceRef[]))}
                />
                {field("Espesor", "kind.thickness", k().thickness, (x, v) => x.type === "thicken" && (x.thickness = v), "mm")}
                <Row label="Con el sólido">{opSelect(k().op, (o) => update((x) => x.type === "thicken" && (x.op = o)))}</Row>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "sweep" && (f().kind as Extract<FeatureKind, { type: "sweep" }>)}>
            {(k) => {
              const pathSketch = () => (k().path.type === "sketch" ? props.sketches.find((s) => s.id === (k().path as { sketch: number }).sketch) : undefined);
              const curves = () => {
                const s = pathSketch();
                return s?.kind.type === "sketch" ? s.kind.sketch.entities.filter((e) => !e.construction && ["line", "arc", "spline"].includes(e.geometry.type)) : [];
              };
              const NAMES: Record<string, string> = { line: "Línea", arc: "Arco", spline: "Spline" };
              return (
                <>
                  <Row label="Perfil">
                    <Select options={sketchOptions()} value={String(k().sketch)} onChange={(v) => update((x) => x.type === "sweep" && (x.sketch = +v))} />
                  </Row>
                  <RegionBox
                    ui={props.ui}
                    store={props.store}
                    owner={`${f().id}:regiones`}
                    lost={lost("regions")}
                    sketch={k().sketch}
                    value={k().regions}
                    onChange={(r) => update((x) => x.type === "sweep" && (x.regions = r))}
                  />
                  <Row label="Camino">
                    <Select
                      options={[...sketchOptions(), ...refOptions(props.store, "curve").map((o) => ({ value: o.value.replace("ref:", "curve:"), label: o.label }))]}
                      value={(() => {
                        const p = k().path;
                        return p.type === "sketch" ? String(p.sketch) : `curve:${p.feature}`;
                      })()}
                      onChange={(v) =>
                        update(
                          (x) =>
                            x.type === "sweep" &&
                            (x.path = v.startsWith("curve:") ? { type: "curve", feature: +v.slice(6) } : { type: "sketch", sketch: +v, entities: [] }),
                        )
                      }
                    />
                  </Row>
                  <Show when={k().path.type === "sketch"}>
                  <div class="space-y-1" aria-label="Tramos del camino">
                    <span class="text-xs text-text-muted">Tramos (sin elegir: todos)</span>
                    <For each={curves()} fallback={<p class="text-[11px] text-text-dim">Ese sketch no tiene líneas, arcos ni splines</p>}>
                      {(e) => (
                        <Checkbox
                          small
                          label={`${NAMES[e.geometry.type]} ${e.id}`}
                          checked={k().path.type === "sketch" && (k().path as { entities: number[] }).entities.includes(e.id)}
                          onChange={(c) =>
                            update((x) => {
                              if (x.type !== "sweep" || x.path.type !== "sketch") return;
                              x.path.entities = c ? [...x.path.entities, e.id] : x.path.entities.filter((i) => i !== e.id);
                            })
                          }
                        />
                      )}
                    </For>
                  </div>
                  </Show>
                  <Row label="Con el sólido">{opSelect(k().op, (o) => update((x) => x.type === "sweep" && (x.op = o)))}</Row>
                </>
              );
            }}
          </Match>
          <Match when={f().kind.type === "loft" && (f().kind as Extract<FeatureKind, { type: "loft" }>)}>
            {(k) => (
              <>
                <For each={k().sections}>
                  {(sec, i) => (
                    <div class="flex items-end gap-1">
                      <div class="flex-1">
                        <Row label={`Sección ${i() + 1}`}>
                          <Select
                            options={sketchOptions()}
                            value={String(sec.sketch)}
                            onChange={(v) => update((x) => x.type === "loft" && (x.sections[i()] = { sketch: +v, regions: { type: "all" } }))}
                          />
                        </Row>
                      </div>
                      <IconButton
                        aria-label={`Quitar la sección ${i() + 1}`}
                        size="sm"
                        variant="ghost"
                        disabled={k().sections.length <= 2}
                        onClick={() => update((x) => x.type === "loft" && x.sections.splice(i(), 1))}
                      >
                        <Icons.X size={10} />
                      </IconButton>
                    </div>
                  )}
                </For>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() =>
                    update((x) => {
                      if (x.type !== "loft") return;
                      const used = new Set(x.sections.map((s) => s.sketch));
                      const next = props.sketches.find((s) => !used.has(s.id));
                      if (next) x.sections.push({ sketch: next.id, regions: { type: "all" } });
                    })
                  }
                >
                  Agregar sección
                </Button>
                <Checkbox small label="Caras planas entre secciones" checked={k().ruled} onChange={(c) => update((x) => x.type === "loft" && (x.ruled = c))} />
                <Row label="Con el sólido">{opSelect(k().op, (o) => update((x) => x.type === "loft" && (x.op = o)))}</Row>
              </>
            )}
          </Match>
          <Match when={f().kind.type === "plane" && (f().kind as Extract<FeatureKind, { type: "plane" }>)}>
            {(k) => {
              const def = () => k().def;
              const setDef = (d: PlaneDef) => update((x) => x.type === "plane" && (x.def = d));
              const base = (): PlaneSpec => (def().type === "offset" || def().type === "angle" ? (def() as { base: PlaneSpec }).base : { type: "xy" });
              return (
                <>
                  <Row label="Tipo">
                    <Select
                      options={[
                        { value: "offset", label: "Desplazado" },
                        { value: "angle", label: "En ángulo" },
                        { value: "midplane", label: "Plano medio" },
                        { value: "three_points", label: "Por tres puntos" },
                      ]}
                      value={def().type}
                      onChange={(v) => {
                        if (v === def().type) return;
                        if (v === "offset") setDef({ type: "offset", base: base(), distance: 10 });
                        else if (v === "angle") setDef({ type: "angle", base: base(), axis: { type: "x" }, angle: 45 });
                        else if (v === "midplane") setDef({ type: "midplane", a: base(), b: { type: "xy" } });
                        else
                          setDef({
                            type: "three_points",
                            points: [
                              { type: "at", point: [0, 0, 0] },
                              { type: "at", point: [10, 0, 0] },
                              { type: "at", point: [0, 10, 0] },
                            ],
                          });
                      }}
                    />
                  </Row>
                  <Switch>
                    <Match when={def().type === "offset" && (def() as Extract<PlaneDef, { type: "offset" }>)}>
                      {(d) => (
                        <>
                          <PlaneField store={props.store} ui={props.ui} owner={`${f().id}:base`} label="Desde" value={d().base} except={f().id} lost={lost("base").length > 0} onChange={(p) => setDef({ ...d(), base: p })} />
                          {field("Distancia", "kind.def.distance", d().distance, (x, v) => x.type === "plane" && x.def.type === "offset" && (x.def.distance = v), "mm")}
                        </>
                      )}
                    </Match>
                    <Match when={def().type === "angle" && (def() as Extract<PlaneDef, { type: "angle" }>)}>
                      {(d) => (
                        <>
                          <PlaneField store={props.store} ui={props.ui} owner={`${f().id}:base`} label="Desde" value={d().base} except={f().id} lost={lost("base").length > 0} onChange={(p) => setDef({ ...d(), base: p })} />
                          <AxisField store={props.store} ui={props.ui} owner={`${f().id}:eje`} label="Alrededor de" value={d().axis} except={f().id} onChange={(a) => setDef({ ...d(), axis: a })} />
                          {field("Ángulo", "kind.def.angle", d().angle, (x, v) => x.type === "plane" && x.def.type === "angle" && (x.def.angle = v), "°")}
                        </>
                      )}
                    </Match>
                    <Match when={def().type === "midplane" && (def() as Extract<PlaneDef, { type: "midplane" }>)}>
                      {(d) => (
                        <>
                          <PlaneField store={props.store} ui={props.ui} owner={`${f().id}:a`} label="Entre" value={d().a} except={f().id} lost={lost("a").length > 0} onChange={(p) => setDef({ ...d(), a: p })} />
                          <PlaneField store={props.store} ui={props.ui} owner={`${f().id}:b`} label="Y" value={d().b} except={f().id} lost={lost("b").length > 0} onChange={(p) => setDef({ ...d(), b: p })} />
                        </>
                      )}
                    </Match>
                    <Match when={def().type === "three_points" && (def() as Extract<PlaneDef, { type: "three_points" }>)}>
                      {(d) => (
                        <For each={[0, 1, 2] as const}>
                          {(k) => (
                            <PointField
                              store={props.store}
                              ui={props.ui}
                              owner={`${f().id}:p${k}`}
                              label={`Punto ${k + 1}`}
                              value={d().points[k]}
                              except={f().id}
                              onChange={(p) => setDef({ ...d(), points: d().points.map((q, i) => (i === k ? p : q)) as [PointSpec, PointSpec, PointSpec] })}
                            />
                          )}
                        </For>
                      )}
                    </Match>
                  </Switch>
                </>
              );
            }}
          </Match>
          <Match when={f().kind.type === "axis" && (f().kind as Extract<FeatureKind, { type: "axis" }>)}>
            {(k) => {
              const def = () => k().def;
              const setDef = (d: AxisDef) => update((x) => x.type === "axis" && (x.def = d));
              return (
                <>
                  <Row label="Tipo">
                    <Select
                      options={[
                        { value: "two_points", label: "Por dos puntos" },
                        { value: "edge", label: "Arista" },
                        { value: "face", label: "Eje de un cilindro" },
                        { value: "planes", label: "Cruce de dos planos" },
                      ]}
                      value={def().type}
                      onChange={(v) => {
                        if (v === def().type) return;
                        if (v === "two_points") setDef({ type: "two_points", a: { type: "at", point: [0, 0, 0] }, b: { type: "at", point: [0, 0, 10] } });
                        else if (v === "planes") setDef({ type: "planes", a: { type: "xz" }, b: { type: "yz" } });
                        else if (v === "edge") setDef({ type: "edge", edge: undefined as unknown as EdgeRef });
                        else setDef({ type: "face", face: undefined as unknown as FaceRef });
                      }}
                    />
                  </Row>
                  <Switch>
                    <Match when={def().type === "two_points" && (def() as Extract<AxisDef, { type: "two_points" }>)}>
                      {(d) => (
                        <>
                          <PointField store={props.store} ui={props.ui} owner={`${f().id}:a`} label="Desde" value={d().a} except={f().id} onChange={(p) => setDef({ ...d(), a: p })} />
                          <PointField store={props.store} ui={props.ui} owner={`${f().id}:b`} label="Hasta" value={d().b} except={f().id} onChange={(p) => setDef({ ...d(), b: p })} />
                        </>
                      )}
                    </Match>
                    <Match when={def().type === "edge" && (def() as Extract<AxisDef, { type: "edge" }>)}>
                      {(d) => (
                        <SelectionBox
                          store={props.store}
                          ui={props.ui}
                          owner={`${f().id}:arista`}
                          kind="edges"
                          label="Arista"
                          refs={d().edge ? [d().edge] : []}
                          lost={lost("edge")}
                          onChange={(refs) => refs.length && setDef({ type: "edge", edge: refs[refs.length - 1] as EdgeRef })}
                        />
                      )}
                    </Match>
                    <Match when={def().type === "face" && (def() as Extract<AxisDef, { type: "face" }>)}>
                      {(d) => (
                        <SelectionBox
                          store={props.store}
                          ui={props.ui}
                          owner={`${f().id}:cara`}
                          kind="face"
                          label="Cara cilíndrica o cónica"
                          refs={d().face ? [d().face] : []}
                          lost={lost("face")}
                          onChange={(refs) => refs[0] && setDef({ type: "face", face: refs[0] as FaceRef })}
                        />
                      )}
                    </Match>
                    <Match when={def().type === "planes" && (def() as Extract<AxisDef, { type: "planes" }>)}>
                      {(d) => (
                        <>
                          <PlaneField store={props.store} ui={props.ui} owner={`${f().id}:a`} label="Plano" value={d().a} except={f().id} lost={lost("a").length > 0} onChange={(p) => setDef({ ...d(), a: p })} />
                          <PlaneField store={props.store} ui={props.ui} owner={`${f().id}:b`} label="Con" value={d().b} except={f().id} lost={lost("b").length > 0} onChange={(p) => setDef({ ...d(), b: p })} />
                        </>
                      )}
                    </Match>
                  </Switch>
                </>
              );
            }}
          </Match>
          <Match when={f().kind.type === "point" && (f().kind as Extract<FeatureKind, { type: "point" }>)}>
            {(k) => (
              <PointField
                store={props.store}
                ui={props.ui}
                owner={`${f().id}:punto`}
                label="Punto"
                value={k().def}
                except={f().id}
                onChange={(p) => update((x) => x.type === "point" && (x.def = p))}
              />
            )}
          </Match>
        </Switch>
        {/* Unir, restar o intersecar: con qué piezas (sin elegir, las que toca; el agujero, la más cercana) */}
        <Show when={f().kind.type === "hole" || ("op" in f().kind && f().kind.type !== "boolean" && (f().kind as { op: BodyOp }).op !== "new")}>
          <PartChecklist
            store={props.store}
            featureId={f().id}
            label={f().kind.type === "hole" ? "En las piezas" : "Con las piezas"}
            empty={f().kind.type === "hole" ? "La más cercana" : "Las que toca"}
            value={f().scope ?? []}
            auto={state()?.auto_scope}
            missing={lost("scope")}
            onChange={(v) => void props.store.updateFeature(f().id, (x) => (x.scope = v.length ? v : undefined))}
          />
        </Show>
        <p class="text-[11px] text-text-dim">{planeLabelFor(f())}</p>
      </div>
    </Section>
  );
};

function planeLabelFor(f: Feature): string {
  return f.kind.type === "sketch" ? `Sobre: ${planeLabel(f.kind.plane)}` : "";
}

// ─── Geometría de referencia en los diálogos ──────────────────────────────

/** Referencias calculadas de un tipo, con el nombre de su operación (sin `except`) */
function refOptions(store: CadStore, kind: "plane" | "axis" | "point" | "curve", except?: number): { value: string; label: string }[] {
  const names = new Map((store.doc()?.features ?? []).map((f) => [f.id, f.name]));
  return (store.result()?.references ?? [])
    .filter((r) => (r.kind === kind || (kind === "curve" && r.kind === "curves")) && r.id !== except)
    .map((r) => ({ value: `ref:${r.id}`, label: names.get(r.id) ?? `Referencia ${r.id}` }));
}

/** Plano: base, de referencia o una cara plana elegida en el visor */
const PlaneField: Component<{ store: CadStore; ui: CadUi; owner: string; label: string; value: PlaneSpec; except?: number; lost?: boolean; onChange: (p: PlaneSpec) => void }> = (props) => {
  const [wantFace, setWantFace] = createSignal(false);
  const current = () => (wantFace() ? "face" : props.value.type === "reference" ? `ref:${props.value.feature}` : props.value.type);
  return (
    <>
      <Row label={props.label}>
        <Select
          options={[
            ...(["xy", "xz", "yz"] as const).map((k) => ({ value: k, label: PLANE_LABELS[k] })),
            ...refOptions(props.store, "plane", props.except),
            { value: "face", label: props.lost ? "Cara del sólido · no encontrada" : "Cara del sólido" },
            ...(props.value.type === "custom" ? [{ value: "custom", label: "Plano propio" }] : []),
          ]}
          value={current()}
          onChange={(v) => {
            if (v === current()) return;
            setWantFace(v === "face");
            if (v === "xy" || v === "xz" || v === "yz") props.onChange({ type: v });
            else if (v.startsWith("ref:")) props.onChange({ type: "reference", feature: +v.slice(4) });
          }}
        />
      </Row>
      <Show when={wantFace() || props.value.type === "face"}>
        <SelectionBox
          store={props.store}
          ui={props.ui}
          owner={props.owner}
          kind="face"
          label={`${props.label}: cara`}
          refs={props.value.type === "face" ? [props.value.face] : []}
          lost={props.lost ? [0] : []}
          onChange={(refs) => {
            if (refs[0]) props.onChange({ type: "face", face: refs[0] as FaceRef });
            setWantFace(false);
          }}
        />
      </Show>
    </>
  );
};

/** Eje: X/Y/Z, de referencia o una arista elegida */
const AxisField: Component<{ store: CadStore; ui: CadUi; owner: string; label: string; value: AxisSpec; except?: number; onChange: (a: AxisSpec) => void }> = (props) => {
  const [wantEdge, setWantEdge] = createSignal(false);
  const current = () => (wantEdge() ? "edge" : props.value.type === "reference" ? `ref:${props.value.feature}` : props.value.type);
  return (
    <>
      <Row label={props.label}>
        <Select
          options={[
            ...(["x", "y", "z"] as const).map((k) => ({ value: k, label: k.toUpperCase() })),
            ...refOptions(props.store, "axis", props.except),
            { value: "edge", label: "Arista del sólido" },
            ...(props.value.type === "sketch_line" ? [{ value: "sketch_line", label: "Línea del sketch" }] : []),
            ...(props.value.type === "custom" ? [{ value: "custom", label: "Eje propio" }] : []),
          ]}
          value={current()}
          onChange={(v) => {
            if (v === current()) return;
            setWantEdge(v === "edge");
            if (v === "x" || v === "y" || v === "z") props.onChange({ type: v });
            else if (v.startsWith("ref:")) props.onChange({ type: "reference", feature: +v.slice(4) });
          }}
        />
      </Row>
      <Show when={wantEdge() || props.value.type === "edge"}>
        <SelectionBox
          store={props.store}
          ui={props.ui}
          owner={props.owner}
          kind="edges"
          label={`${props.label}: arista`}
          refs={props.value.type === "edge" ? [props.value.edge] : []}
          onChange={(refs) => {
            const last = refs[refs.length - 1];
            if (last) props.onChange({ type: "edge", edge: last as EdgeRef });
            setWantEdge(false);
          }}
        />
      </Show>
    </>
  );
};

/** Punto: coordenadas, centro o punto de una arista, o de referencia */
const PointField: Component<{ store: CadStore; ui: CadUi; owner: string; label: string; value: PointSpec; except?: number; onChange: (p: PointSpec) => void }> = (props) => {
  const current = () => (props.value.type === "reference" ? `ref:${props.value.feature}` : props.value.type);
  const edgeOf = () => (props.value.type === "center" || props.value.type === "on_edge" ? [props.value.edge] : []);
  return (
    <div class="space-y-1">
      <Row label={props.label}>
        <Select
          options={[
            { value: "at", label: "Coordenadas" },
            { value: "center", label: "Centro de una arista" },
            { value: "on_edge", label: "Sobre una arista recta" },
            ...refOptions(props.store, "point", props.except),
          ]}
          value={current()}
          onChange={(v) => {
            if (v === current()) return;
            const edge = edgeOf()[0];
            if (v === "at") props.onChange({ type: "at", point: [0, 0, 0] });
            else if (v.startsWith("ref:")) props.onChange({ type: "reference", feature: +v.slice(4) });
            else if (edge) props.onChange(v === "center" ? { type: "center", edge } : { type: "on_edge", edge, at: 0.5 });
            else props.onChange(v === "center" ? { type: "center", edge: undefined as unknown as EdgeRef } : { type: "on_edge", edge: undefined as unknown as EdgeRef, at: 0.5 });
          }}
        />
      </Row>
      <Show when={props.value.type === "at" && props.value}>
        {(p) => (
          <div class="grid grid-cols-3 gap-1">
            <For each={["X", "Y", "Z"]}>
              {(axis, i) => (
                <Num
                  label={axis}
                  value={p().point[i()]}
                  onCommit={(v) => props.onChange({ type: "at", point: p().point.map((c, k) => (k === i() ? v : c)) as P3 })}
                />
              )}
            </For>
          </div>
        )}
      </Show>
      <Show when={props.value.type === "center" || props.value.type === "on_edge"}>
        <SelectionBox
          store={props.store}
          ui={props.ui}
          owner={props.owner}
          kind="edges"
          label={`${props.label}: arista`}
          refs={edgeOf().filter(Boolean)}
          onChange={(refs) => {
            const edge = refs[refs.length - 1] as EdgeRef | undefined;
            if (!edge) return;
            props.onChange(props.value.type === "on_edge" ? { ...props.value, edge } : { type: "center", edge });
          }}
        />
      </Show>
      <Show when={props.value.type === "on_edge" && props.value}>
        {(p) => <Num label="Fracción del largo" step={0.05} value={p().at} onCommit={(v) => props.onChange({ ...p(), at: Math.min(1, Math.max(0, v)) })} />}
      </Show>
    </div>
  );
};

// ─── Piezas en los diálogos ───────────────────────────────────────────────

/** Lista de piezas que hay antes de la operación para tildar; las perdidas en rojo */
const PartChecklist: Component<{
  store: CadStore;
  featureId: number;
  label: string;
  /** Qué significa no elegir ninguna */
  empty: string;
  value: PartId[];
  /** Las que eligió sola sin `value`: se ven marcadas y desde ahí se cambia */
  auto?: PartId[];
  /** Posiciones de `value` que el último cálculo no encontró */
  missing?: number[];
  onChange: (v: PartId[]) => void;
}> = (props) => {
  const [parts, setParts] = createSignal<PartView[]>([]);
  let seq = 0;
  createEffect(() => {
    props.store.doc();
    const n = ++seq;
    void props.store.partsBefore(props.featureId).then((p) => n === seq && setParts(p));
  });
  const shown = () => (props.value.length > 0 ? props.value : (props.auto ?? []));
  const has = (id: PartId) => shown().some((x) => samePart(x, id));
  const gone = () => props.value.filter((v, i) => (props.missing ?? []).includes(i) || (parts().length > 0 && !parts().some((p) => samePart(p.id, v))));
  return (
    <div class="space-y-1" aria-label={props.label}>
      <span class="text-xs text-text-muted">{props.label}</span>
      <Show when={parts().length > 0} fallback={<p class="text-[11px] text-text-dim">Todavía no hay piezas</p>}>
        <For each={parts()}>
          {(p) => (
            <Checkbox
              small
              label={p.name}
              checked={has(p.id)}
              onChange={(c) => props.onChange(c ? [...shown(), p.id] : shown().filter((x) => !samePart(x, p.id)))}
            />
          )}
        </For>
      </Show>
      <For each={gone()}>
        {(g) => (
          <div class="flex items-center justify-between text-[11px] text-error">
            <span>Pieza no encontrada</span>
            <button aria-label="Quitar la pieza no encontrada" onClick={() => props.onChange(props.value.filter((x) => !samePart(x, g)))}>
              <Icons.X size={10} />
            </button>
          </div>
        )}
      </For>
      <Show when={props.value.length === 0}>
        <p class="text-[11px] text-text-dim">{props.empty}</p>
      </Show>
    </div>
  );
};

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
/** Operaciones a las que ya se les contestó "no" a coordinar la rosca (no se vuelve a preguntar) */
const [threadDeclined, setThreadDeclined] = createSignal<number[]>([]);

/**
 * Rosca asociada: con qué otra rosca se coordina la operación (medida, eje y
 * fase del filete). Sin asociar y con roscas que calzan, pregunta; la que está
 * en el mismo eje va primero. También se elige con un clic en el visor.
 */
const ThreadLinkBox: Component<{
  store: CadStore;
  ui: CadUi;
  feature: number;
  value: ThreadLink | null | undefined;
  want: ThreadWant;
  /** Qué se coordina ("el tornillo", "la rosca"…), para la pregunta */
  what: string;
  /** Preguntar con cualquier rosca que calce (si no, solo con una en el mismo eje) */
  askAlways?: boolean;
  onChange: (link: ThreadLink | null, source: ThreadAxis | null) => void;
}> = (props) => {
  const owner = () => `${props.feature}:rosca`;
  const threads = () => props.store.result()?.threads ?? [];
  const doc = () => props.store.doc();
  const own = () => threads().filter((t) => t.feature === props.feature);
  const candidates = createMemo(() => {
    const list = threadCandidates(doc(), threads(), props.feature, props.want);
    // Las que están en el eje propio, primero
    const onAxis = (t: ThreadAxis) => own().some((o) => coaxialThreads(t, o));
    return [...list.filter(onAxis), ...list.filter((t) => !onAxis(t))];
  });
  const suggested = () => candidates().find((t) => own().some((o) => coaxialThreads(t, o)));
  const key = (l: { feature: number; index: number }) => `${l.feature}:${l.index}`;
  const source = () => (props.value ? threads().find((t) => key(t) === key(props.value!)) : undefined);
  const choose = (t: ThreadAxis | undefined) => props.onChange(t ? { feature: t.feature, index: t.index } : null, t ?? null);
  const label = (t: ThreadAxis) => `${threadLabel(doc(), threads(), t)}${own().some((o) => coaxialThreads(t, o)) ? " (en el eje)" : ""}`;
  const asking = () => !props.value && candidates().length > 0 && !threadDeclined().includes(props.feature) && (props.askAlways || !!suggested());
  const picking = () => (props.ui.pick() as { owner?: string }).owner === owner();
  const pick = () => {
    if (picking()) return props.ui.cancelPick();
    void props.store.setSelecting(true);
    props.ui.setPick({
      kind: "face",
      owner: owner(),
      prompt: "Clic en la rosca con la que se coordina: el agujero, el tornillo, la tuerca o el eje",
      done: (face) => {
        void props.store.setSelecting(false);
        const t = threadOfFace(face, candidates());
        if (t) choose(t);
        else props.ui.setMessage(`Ahí no hay una rosca ${props.want === "internal" ? "interior " : props.want === "external" ? "exterior " : ""}anterior a esta operación`);
      },
    });
  };
  onCleanup(() => {
    if (picking()) props.ui.cancelPick();
  });
  const pickButton = (text: string) => (
    <Button size="sm" variant={picking() ? "primary" : "default"} onClick={pick}>
      {text}
    </Button>
  );
  return (
    <Show
      when={!asking()}
      fallback={
        <div class="space-y-2 rounded border border-accent/60 bg-accent/10 p-2 text-xs" data-thread-question>
          <p class="text-text">
            {suggested() ? `¿Coordinar ${props.what} con la rosca de ${threadLabel(doc(), threads(), suggested()!)}?` : `¿Coordinar ${props.what} con una rosca existente?`}
          </p>
          <p class="text-[11px] text-text-dim">Toma su medida y su eje y alinea el filete para que calcen. No las une: cada una sigue siendo lo que es.</p>
          <Show when={!suggested()}>
            <Select options={candidates().map((t) => ({ value: key(t), label: label(t) }))} value="" placeholder="Elegir la rosca" onChange={(v) => choose(candidates().find((t) => key(t) === v))} />
          </Show>
          <div class="flex flex-wrap gap-1">
            <Show when={suggested()}>
              {(t) => (
                <Button size="sm" variant="primary" onClick={() => choose(t())}>
                  Coordinar
                </Button>
              )}
            </Show>
            {pickButton("Elegir en el visor")}
            <Button size="sm" variant="ghost" onClick={() => setThreadDeclined((d) => [...d, props.feature])}>
              No
            </Button>
          </div>
        </div>
      }
    >
      <div class="space-y-1">
        <Row label="Rosca asociada">
          <Select
            options={[{ value: "", label: "Ninguna" }, ...candidates().map((t) => ({ value: key(t), label: label(t) })), ...(props.value && !source() ? [{ value: key(props.value), label: "(ya no está)" }] : [])]}
            value={props.value ? key(props.value) : ""}
            onChange={(v) => choose(v ? candidates().find((t) => key(t) === v) : undefined)}
          />
        </Row>
        <Show when={props.value || candidates().length > 0}>
          <div class="flex justify-end">{pickButton(props.value ? "Elegir otra en el visor" : "Elegir en el visor")}</div>
        </Show>
        <Show when={source()}>
          {(t) => (
            <p class="text-[11px] text-text-dim">
              {threadName(t().spec)} × {String(t().spec.pitch).replace(".", ",")}: medida, eje y fase del filete de {threadLabel(doc(), threads(), t()).split(" · ")[0]}. Si cambia, esta lo sigue.
            </p>
          )}
        </Show>
      </div>
    </Show>
  );
};

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

// ─── Ensamble ─────────────────────────────────────────────────────────────

const deg = (r: number) => (r * 180) / Math.PI;

/** Instancias de las piezas, relaciones entre ellas, grados libres y choques */
const AssemblyPanel: Component<{ store: CadStore; ui: CadUi }> = (props) => {
  const store = props.store;
  const asm = () => store.doc()?.assembly ?? null;
  const parts = () => store.result()?.parts ?? [];
  const sol = () => store.result()?.assembly?.solution;
  const [part, setPart] = createSignal<string>("");
  const [kind, setKind] = createSignal<MateKind>("revolute");
  const [picking, setPicking] = createSignal<"" | "a" | "b">("");
  const [hits, setHits] = createSignal<[number, number, number][] | null>(null);
  const partKey = (p: PartView) => `${p.id.feature}:${p.id.index}`;
  const instName = (id: number) => asm()?.instances.find((i) => i.id === id)?.name ?? `#${id}`;

  const insert = () => {
    const p = parts().find((x) => partKey(x) === (part() || (parts()[0] && partKey(parts()[0]))));
    if (!p) return props.ui.setMessage("El diseño todavía no tiene piezas");
    void store.editAssembly((a) => {
      const id = a.next_id || 1;
      const copies = a.instances.filter((i) => samePart(i.part, p.id)).length;
      // Las nuevas, corridas a un costado para que no se pisen
      const size = Math.cbrt(Math.max(p.volume, 1)) * 1.5;
      a.instances.push({
        id,
        part: p.id,
        name: copies ? `${p.name} (${copies + 1})` : p.name,
        position: [a.instances.length * size, 0, 0],
        rotation: [0, 0, 0],
        fixed: a.instances.length === 0,
      });
      a.next_id = id + 1;
    });
  };

  /** Dos clics: la cara de una instancia y la de otra; con eso, la relación */
  const newMate = () => {
    let first: Connector | null = null;
    const pick = (step: "a" | "b") => {
      setPicking(step);
      props.ui.setPick({
        kind: "asm_face",
        prompt: step === "a" ? "Clic en la cara de la primera pieza (plana o cilíndrica)" : "Clic en la cara de la segunda pieza",
        done: async (face) => {
          try {
            const c = await store.assemblyConnector(face);
            if (step === "a") {
              first = c;
              return pick("b");
            }
            setPicking("");
            if (!first || first.instance === c.instance) return props.ui.setMessage("Las dos caras tienen que ser de instancias distintas");
            const a = first;
            void store.editAssembly((x) => {
              const id = x.next_id || 1;
              // Caras planas enfrentadas: normales opuestas
              const facing = a.z[0] * c.z[0] + a.z[1] * c.z[1] + a.z[2] * c.z[2] < 0;
              x.mates.push({ id, name: `${MATE_LABELS[kind()]} ${x.mates.length + 1}`, kind: kind(), a, b: c, flip: kind() === "planar" || kind() === "fastened" ? facing : false });
              x.next_id = id + 1;
            });
          } catch (e) {
            setPicking("");
            props.ui.setMessage(String(e));
          }
        },
      });
    };
    pick("a");
  };

  const removeInstance = (id: number) =>
    void store.editAssembly((a) => {
      a.instances = a.instances.filter((i) => i.id !== id);
      a.mates = a.mates.filter((m) => m.a.instance !== id && m.b.instance !== id);
    });

  return (
    <div class="space-y-3">
      <Section title="Instancias">
        <div class="flex gap-1.5">
          <div class="flex-1">
            <Select
              options={parts().map((p) => ({ value: partKey(p), label: p.name }))}
              value={part() || (parts()[0] ? partKey(parts()[0]) : "")}
              onChange={setPart}
            />
          </div>
          <Button size="sm" onClick={insert} disabled={!parts().length}>
            Insertar
          </Button>
        </div>
        <For each={asm()?.instances ?? []} fallback={<p class="text-[11px] text-text-dim">Insertar piezas del diseño para ensamblarlas</p>}>
          {(inst) => (
            <div class="space-y-1 rounded border border-border p-1.5" data-instance={inst.id}>
              <div class="flex items-center gap-1.5 text-xs">
                <span class="flex-1 truncate text-text">{inst.name}</span>
                <Checkbox
                  small
                  label="Fija"
                  checked={inst.fixed}
                  onChange={(c) => void store.editAssembly((a) => (a.instances.find((i) => i.id === inst.id)!.fixed = c))}
                />
                <IconButton aria-label={`Quitar ${inst.name}`} size="sm" variant="ghost" onClick={() => removeInstance(inst.id)}>
                  <Icons.Trash size={11} />
                </IconButton>
              </div>
              <span class="text-[10px] text-text-dim">Posición (mm)</span>
              <div class="grid grid-cols-3 gap-1">
                <For each={["X", "Y", "Z"]}>
                  {(axis, k) => (
                    <Num
                      label={axis}
                      value={inst.position[k()]}
                      onCommit={(v) => void store.editAssembly((a) => (a.instances.find((i) => i.id === inst.id)!.position[k()] = v))}
                    />
                  )}
                </For>
              </div>
              <span class="text-[10px] text-text-dim">Giro (°)</span>
              <div class="grid grid-cols-3 gap-1">
                <For each={["X", "Y", "Z"]}>
                  {(axis, k) => (
                    <Num
                      label={axis}
                      value={Math.round(deg(inst.rotation[k()]) * 1000) / 1000}
                      onCommit={(v) => void store.editAssembly((a) => (a.instances.find((i) => i.id === inst.id)!.rotation[k()] = (v * Math.PI) / 180))}
                    />
                  )}
                </For>
              </div>
            </div>
          )}
        </For>
      </Section>
      <Section title="Relaciones">
        <div class="flex gap-1.5">
          <div class="flex-1">
            <Select options={(Object.keys(MATE_LABELS) as MateKind[]).map((k) => ({ value: k, label: MATE_LABELS[k] }))} value={kind()} onChange={(v) => setKind(v as MateKind)} />
          </div>
          <Button size="sm" disabled={(asm()?.instances.length ?? 0) < 2 || !!picking()} onClick={newMate}>
            {picking() === "a" ? "Cara 1…" : picking() === "b" ? "Cara 2…" : "Agregar"}
          </Button>
        </div>
        <For each={asm()?.mates ?? []} fallback={<p class="text-[11px] text-text-dim">Agregar: elegir el tipo y hacer clic en una cara de cada pieza</p>}>
          {(m) => {
            const set = (f: (x: Mate) => void) => void store.editAssembly((a) => f(a.mates.find((x) => x.id === m.id)!));
            return (
              <div class="space-y-1 rounded border border-border p-1.5 text-xs" data-mate={m.id}>
                <div class="flex items-center gap-1.5">
                  <span class="flex-1 truncate text-text" title={`${instName(m.a.instance)} ↔ ${instName(m.b.instance)}`}>
                    {m.name}
                  </span>
                  <Checkbox small label="Invertir" checked={m.flip} onChange={(c) => set((x) => (x.flip = c))} />
                  <IconButton aria-label={`Quitar ${m.name}`} size="sm" variant="ghost" onClick={() => void store.editAssembly((a) => (a.mates = a.mates.filter((x) => x.id !== m.id)))}>
                    <Icons.Trash size={11} />
                  </IconButton>
                </div>
                <p class="text-[11px] text-text-dim">
                  {instName(m.a.instance)} ↔ {instName(m.b.instance)}
                </p>
                <Show when={m.kind === "revolute" || m.kind === "cylindrical"}>
                  <div class="flex items-center gap-1.5">
                    <Checkbox small label="Ángulo" checked={m.angle != null} onChange={(c) => set((x) => (x.angle = c ? 0 : null))} />
                    <Show when={m.angle != null}>
                      <div class="flex-1">
                        <Slider value={m.angle ?? 0} min={-180} max={180} step={1} onChange={(v) => set((x) => (x.angle = v))} />
                      </div>
                      <span class="w-10 text-right font-mono">{Math.round(m.angle ?? 0)}°</span>
                    </Show>
                  </div>
                </Show>
                <Show when={m.kind === "slider" || m.kind === "cylindrical"}>
                  <div class="flex items-center gap-1.5">
                    <Checkbox small label="Distancia" checked={m.distance != null} onChange={(c) => set((x) => (x.distance = c ? 0 : null))} />
                    <Show when={m.distance != null}>
                      <div class="flex-1">
                        <Num value={m.distance ?? 0} suffix="mm" onCommit={(v) => set((x) => (x.distance = v))} />
                      </div>
                    </Show>
                  </div>
                </Show>
              </div>
            );
          }}
        </For>
        <Show when={sol()}>
          {(s) => (
            <p class={clsx("text-[11px]", s().converged ? "text-text-dim" : "text-warning")} aria-label="Estado del ensamble">
              {s().converged
                ? s().dof === 0
                  ? "Todo definido: no queda nada que se mueva"
                  : s().dof === 1
                    ? "Queda 1 grado libre"
                    : `Quedan ${s().dof} grados libres`
                : `No se pueden cumplir todas las relaciones (desvío ${fmt(s().residual, 3)})`}
            </p>
          )}
        </Show>
      </Section>
      <Show when={(asm()?.instances.length ?? 0) > 0}>
        <BomSection store={store} />
      </Show>
      <Section title="Choques">
        <Button size="sm" fullWidth disabled={(asm()?.instances.length ?? 0) < 2} onClick={() => void store.assemblyInterference().then(setHits)}>
          Revisar choques
        </Button>
        <Show when={hits()}>
          {(list) => (
            <For each={list()} fallback={<p class="text-[11px] text-success">Ninguna pieza se pisa con otra</p>}>
              {([a, b, v]) => (
                <p class="text-[11px] text-warning" data-hit>
                  {instName(a)} y {instName(b)}: {fmt(v / 1000, 3)} cm³ en común
                </p>
              )}
            </For>
          )}
        </Show>
      </Section>
    </div>
  );
};

/** Lista de materiales del ensamble: piezas, cantidades, material y masa; exportable a CSV */
const BomSection: Component<{ store: CadStore }> = (props) => {
  const rows = createMemo(() => {
    const doc = props.store.doc();
    const parts = props.store.result()?.parts ?? [];
    const out: { name: string; qty: number; material: string; mass: number | null; volume: number }[] = [];
    for (const inst of doc?.assembly?.instances ?? []) {
      const p = parts.find((x) => samePart(x.id, inst.part));
      if (!p) continue;
      const row = out.find((r) => r.name === p.name);
      if (row) row.qty += 1;
      else out.push({ name: p.name, qty: 1, material: partMaterial(doc, p)?.name ?? "", mass: partMass(doc, p), volume: p.volume });
    }
    return out;
  });
  const total = () => (rows().every((r) => r.mass !== null) ? rows().reduce((a, r) => a + r.mass! * r.qty, 0) : null);
  const exportCsv = async () => {
    const path = await save({ filters: [{ name: "CSV", extensions: ["csv"] }], defaultPath: "lista_de_materiales.csv" });
    if (!path) return;
    const q = (s: string) => `"${s.replace(/"/g, '""')}"`;
    const lines = [
      "Elemento;Pieza;Cantidad;Material;Masa unitaria (g);Volumen unitario (cm³)",
      ...rows().map((r, i) => [i + 1, q(r.name), r.qty, q(r.material), r.mass === null ? "" : r.mass.toFixed(2).replace(".", ","), (r.volume / 1000).toFixed(3).replace(".", ",")].join(";")),
    ];
    await invoke("cad_write_text", { path, content: lines.join("\n") + "\n" });
  };
  return (
    <Section title="Lista de materiales">
      <table class="w-full text-[11px]" aria-label="Lista de materiales">
        <thead>
          <tr class="text-text-dim">
            <th class="text-left font-normal">Pieza</th>
            <th class="text-right font-normal">Cant.</th>
            <th class="text-left font-normal pl-2">Material</th>
            <th class="text-right font-normal">Masa</th>
          </tr>
        </thead>
        <tbody>
          <For each={rows()}>
            {(r) => (
              <tr class="text-text" data-bom-row>
                <td class="truncate max-w-[7rem]">{r.name}</td>
                <td class="text-right font-mono">{r.qty}</td>
                <td class="pl-2 truncate max-w-[5rem]">{r.material || "—"}</td>
                <td class="text-right font-mono">{r.mass === null ? "—" : fmtMass(r.mass * r.qty)}</td>
              </tr>
            )}
          </For>
        </tbody>
      </table>
      <Show when={total() !== null}>
        <p class="text-xs text-text">Masa total: {fmtMass(total()!)}</p>
      </Show>
      <Button size="sm" variant="ghost" fullWidth onClick={() => void exportCsv()}>
        Exportar CSV
      </Button>
    </Section>
  );
};

// ─── Escaneo ──────────────────────────────────────────────────────────────

/** Comparar el escaneo con el diseño: colores en el visor y resumen */
const DeviationCard: Component<{ store: CadStore; ui: CadUi }> = (props) => {
  const [tolerance, setTolerance] = createSignal(0.2);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string>();
  const run = async () => {
    setBusy(true);
    setError(undefined);
    try {
      props.ui.setDeviation(await props.store.deviation(tolerance()));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  // Si cambia el diseño, los colores viejos ya no valen
  createEffect(on(() => props.store.result()?.version, () => props.ui.setDeviation(null), { defer: true }));
  const d = () => props.ui.deviation();
  return (
    <div class="space-y-1.5 rounded-md border border-border p-2" aria-label="Comparar con el escaneo">
      <span class="text-xs text-text-muted">Comparar el diseño con el escaneo</span>
      <Num label="Tolerancia" value={tolerance()} step={0.05} suffix="mm" onCommit={(v) => setTolerance(Math.max(0.001, v))} />
      <Button size="sm" fullWidth disabled={busy() || !props.store.result()?.body} onClick={() => void run()}>
        {busy() ? "Midiendo…" : "Comparar"}
      </Button>
      <Show when={error()}>
        <p class="text-[11px] text-error">{error()}</p>
      </Show>
      <Show when={d()}>
        {(dev) => (
          <div class="space-y-0.5 text-xs">
            <div class="h-2 rounded" style={{ background: "linear-gradient(to right, rgb(51,89,242), rgb(77,191,230), rgb(64,191,89), rgb(242,204,51), rgb(230,51,38))" }} />
            <div class="flex justify-between text-[10px] text-text-dim">
              <span>adentro</span>
              <span>± {fmt(dev().stats.tolerance)} mm</span>
              <span>afuera</span>
            </div>
            <Line label="Dentro de tolerancia">{fmt(dev().stats.within * 100, 1)} %</Line>
            <Line label="Media (sesgo)">{fmt(dev().stats.mean, 3)} mm</Line>
            <Line label="Media absoluta">{fmt(dev().stats.meanAbs, 3)} mm</Line>
            <Line label="P95">{fmt(dev().stats.p95, 3)} mm</Line>
            <Line label="Máxima">{fmt(dev().stats.maxAbs, 3)} mm</Line>
            <Button size="sm" variant="ghost" fullWidth onClick={() => props.ui.setDeviation(null)}>
              Quitar los colores
            </Button>
          </div>
        )}
      </Show>
    </div>
  );
};

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

/** Piezas del diseño: color, nombre, visibilidad y exportar cada una */
const PartsSection: Component<{ store: CadStore; ui: CadUi }> = (props) => {
  const parts = () => props.store.result()?.parts ?? [];
  const [renaming, setRenaming] = createSignal<string>();
  const key = (p: PartView) => `${p.id.feature}:${p.id.index}`;
  // Pieza abierta (sus datos y material abajo de la lista; se resalta en el visor)
  const open = props.ui.openPart;
  const setOpen = props.ui.setOpenPart;
  const opened = () => parts().find((p) => key(p) === open());
  createEffect(
    on(opened, (p, prev) => {
      if (p) props.ui.setHighlight({ faces: Array.from({ length: p.faces[1] - p.faces[0] }, (_, k) => p.faces[0] + k), edges: [] });
      else if (prev) props.ui.setHighlight({ faces: [], edges: [] });
    }),
  );
  return (
    <Show when={parts().length > 0}>
      <Section title={`Piezas (${parts().length})`}>
        <div class="space-y-0.5">
          <For each={parts()}>
            {(p, i) => {
              const hidden = () => partHidden(props.store.doc(), p);
              return (
                <div data-part={key(p)} class={clsx("group flex items-center gap-1.5 px-1 py-0.5 rounded text-xs hover:bg-surface", hidden() && "opacity-50")}>
                  <input
                    type="color"
                    aria-label={`Color de ${p.name}`}
                    class="w-4 h-4 shrink-0 rounded cursor-pointer bg-transparent border-0 p-0"
                    value={partColor(props.store.doc(), p, i())}
                    onChange={(e) => void props.store.setPartProps(p.id, { color: e.currentTarget.value })}
                  />
                  <Show
                    when={renaming() === key(p)}
                    fallback={
                      <span
                        class={clsx("flex-1 truncate cursor-pointer", open() === key(p) ? "text-accent" : "text-text")}
                        title="Clic: ver y elegir material · doble clic: renombrar"
                        onClick={() => setOpen(open() === key(p) ? undefined : key(p))}
                        onDblClick={() => setRenaming(key(p))}
                      >
                        {p.name}
                      </span>
                    }
                  >
                    <input
                      aria-label="Nombre de la pieza"
                      class="flex-1 min-w-0 px-1 rounded bg-surface/40 border border-accent text-xs text-text outline-none"
                      value={p.name}
                      ref={(el) => setTimeout(() => el.select())}
                      onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
                      onBlur={(e) => {
                        const name = e.currentTarget.value.trim();
                        setRenaming(undefined);
                        if (name && name !== p.name) void props.store.setPartProps(p.id, { name });
                      }}
                    />
                  </Show>
                  <span class="font-mono text-[10px] text-text-dim group-hover:hidden">
                    {p.surface
                      ? `superficie · ${fmt(p.area / 100)} cm²`
                      : partMass(props.store.doc(), p) !== null
                        ? fmtMass(partMass(props.store.doc(), p)!)
                        : `${fmt(p.volume / 1000)} cm³`}
                  </span>
                  <span class="hidden group-hover:flex">
                    <IconButton
                      aria-label={`Borrar ${p.name}`}
                      size="sm"
                      variant="ghost"
                      onClick={() => void props.store.addFeature({ type: "delete_parts", parts: [p.id] })}
                    >
                      <Icons.Trash size={11} />
                    </IconButton>
                  </span>
                  <IconButton
                    aria-label={hidden() ? `Mostrar ${p.name}` : `Ocultar ${p.name}`}
                    size="sm"
                    variant="ghost"
                    onClick={() => void props.store.setPartProps(p.id, { hidden: !hidden() })}
                  >
                    {hidden() ? <Icons.EyeSlash size={12} /> : <Icons.Eye size={12} />}
                  </IconButton>
                </div>
              );
            }}
          </For>
        </div>
        <Show when={opened()}>
          {(p) => {
            const own = () => props.store.doc()?.parts?.find((x) => samePart(x.part, p().id))?.material ?? null;
            return (
              <div class="space-y-1.5 rounded border border-border p-2 text-xs" aria-label={`Datos de ${p().name}`}>
                <MaterialPicker value={own()} none="El del diseño" onChange={(m) => void props.store.setPartProps(p().id, { material: m })} />
                <Show when={!p().surface} fallback={<Line label="Tipo">Superficie (sin espesor)</Line>}>
                  <Line label="Volumen">{fmt(p().volume / 1000)} cm³</Line>
                </Show>
                <Line label="Área">{fmt(p().area / 100)} cm²</Line>
                <Show when={!p().surface && partMass(props.store.doc(), p()) !== null}>
                  <Line label="Masa">
                    {fmtMass(partMass(props.store.doc(), p())!)} ({partMaterial(props.store.doc(), p())!.name})
                  </Line>
                </Show>
                <Line label="Centro">{p().center.map((v) => fmt(v, 2)).join(", ")} mm</Line>
              </div>
            );
          }}
        </Show>
      </Section>
    </Show>
  );
};

/** Material: ninguno (o el heredado), uno de la lista o una densidad propia */
const MaterialPicker: Component<{ value: Material | null; none: string; onChange: (m: Material | null) => void }> = (props) => {
  const choice = () => {
    const m = props.value;
    if (!m) return "none";
    return MATERIALS.some((x) => x.name === m.name && x.density === m.density) ? m.name : "custom";
  };
  return (
    <>
      <Row label="Material">
        <Select
          options={[
            { value: "none", label: props.none },
            ...MATERIALS.map((m) => ({ value: m.name, label: `${m.name} (${m.density} kg/m³)` })),
            { value: "custom", label: "Densidad propia" },
          ]}
          value={choice()}
          onChange={(v) => {
            if (v === choice()) return;
            if (v === "none") props.onChange(null);
            else if (v === "custom") props.onChange({ name: "Otro", density: props.value?.density ?? 1000 });
            else props.onChange(MATERIALS.find((m) => m.name === v)!);
          }}
        />
      </Row>
      <Show when={choice() === "custom"}>
        <Num label="Densidad" suffix="kg/m³" step={10} value={props.value!.density} onCommit={(v) => v > 0 && props.onChange({ name: props.value!.name, density: v })} />
      </Show>
    </>
  );
};

/** "12,30 g" o "1,234 kg" */
const fmtMass = (g: number) => (g >= 1000 ? `${fmt(g / 1000, 3)} kg` : `${fmt(g, 2)} g`);

const Line = (p: { label: string; children: JSX.Element }) => (
  <div class="flex justify-between gap-2">
    <span class="text-text-muted whitespace-nowrap">{p.label}</span>
    <span class="font-mono text-text text-right">{p.children}</span>
  </div>
);

const BodySection: Component<{ store: CadStore; ui: CadUi }> = (props) => (
  <Show when={props.store.result()?.body}>
    {(b) => {
      const size = () => b().bbox_max.map((v, i) => v - b().bbox_min[i]);
      const material = () => props.store.doc()?.material ?? null;
      const setMaterial = (m: Material | null) => void props.store.commit((d) => (d.material = m));
      const total = () => designMass(props.store.doc(), props.store.result());
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
          <MaterialPicker value={material()} none="Sin material" onChange={setMaterial} />
          <div class="space-y-1 text-xs">
            <Show when={total().mass !== null}>
              <Line label="Masa">{fmtMass(total().mass!)}</Line>
            </Show>
            <Line label="Centro de masa">{(total().center ?? b().center).map((v) => fmt(v, 2)).join(", ")} mm</Line>
            {/* Con una sola densidad: kg·mm² = mm⁵ × kg/m³ × 1e−9 */}
            <Show when={total().mass !== null && total().uniform && material()}>
              <Line label="Inercia">
                {b()
                  .inertia.map((v) => fmt(v * material()!.density * 1e-9, 3))
                  .join(" · ")}{" "}
                kg·mm²
              </Line>
            </Show>
          </div>
          <Checkbox small label="Ver el centro de masa" checked={props.ui.showCenterOfMass()} onChange={(c) => props.ui.setShowCenterOfMass(c)} />
          <Button size="sm" fullWidth onClick={() => props.ui.setDrawingOpen(true)}>
            Plano 2D (vistas, ocultas, cajetín)
          </Button>
        </Section>
      );
    }}
  </Show>
);

// ─── Sketch en edición ────────────────────────────────────────────────────

type SketchPatternKind = "linear" | "circular" | "curve" | "table" | "fill";

const SKETCH_PATTERNS: { value: SketchPatternKind; label: string }[] = [
  { value: "linear", label: "Lineal" },
  { value: "circular", label: "Circular" },
  { value: "curve", label: "En curva" },
  { value: "table", label: "Por tabla" },
  { value: "fill", label: "De relleno" },
];

// ─── Sketch 3D ───────────────────────────────────────────────────────────────

type Tool3d = "select" | "line" | "arc" | "spline" | "point";

const TOOL3D: { id: Tool3d; label: string; prompt: string }[] = [
  { id: "line", label: "Línea", prompt: "Clic en el plano activo, en un punto o en un vértice del sólido; cada clic sigue la línea" },
  { id: "arc", label: "Arco", prompt: "Arco por tres puntos: comienzo, uno del medio y fin" },
  { id: "spline", label: "Spline", prompt: "Clic en cada punto de paso; «Terminar trazo» la crea" },
  { id: "point", label: "Punto", prompt: "Clic donde va el punto" },
  { id: "select", label: "Elegir", prompt: "Clic en puntos y curvas para restringirlos (Mayús suma)" },
];

const C3D_LABELS: Record<Constraint3d["type"], string> = {
  coincident: "Coincidentes",
  fixed: "Fijo",
  attach: "Sobre el modelo",
  on_plane: "Sobre el plano",
  along_axis: "Paralela al eje",
  parallel: "Paralelas",
  perpendicular: "Perpendiculares",
  equal: "Mismo largo",
  tangent: "Tangentes",
  midpoint: "Punto medio",
  length: "Largo",
  distance: "Distancia",
  angle: "Ángulo",
};

/** Ids de puntos y entidades que nombra una restricción del sketch 3D */
function ids3d(c: Constraint3d): number[] {
  return (["a", "b", "point", "line"] as const).flatMap((k) => (k in c ? [(c as Record<string, unknown>)[k] as number] : []));
}

const sub3 = (a: P3, b: P3): P3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const len3 = (a: P3) => Math.hypot(a[0], a[1], a[2]);

/** Editor del sketch 3D: dibuja con clics en el visor (sobre el plano activo o los vértices del sólido) y restringe lo elegido */
const Sketch3dEditor: Component<{ store: CadStore; ui: CadUi; feature: Feature }> = (props) => {
  const ui = props.ui;
  const id = () => props.feature.id;
  const [tool, setTool] = createSignal<Tool3d>("line");
  const [planeKey, setPlaneKey] = createSignal<"xy" | "xz" | "yz">("xy");
  const [offset, setOffset] = createSignal(0);
  // Puntos del trazo en curso (línea: el último; arco: comienzo y medio; spline: todos)
  const [trace, setTrace] = createSignal<number[]>([]);
  const view = () => props.store.result()?.sketches3d?.find((v) => v.id === id());
  const draftSketch = (): Sketch3d | undefined => {
    const f = props.store.doc()?.features.find((x) => x.id === id());
    return f?.kind.type === "sketch3d" ? f.kind.sketch : undefined;
  };
  const sketch = (): Sketch3d => view()?.sketch ?? draftSketch() ?? { points: [], entities: [], constraints: [], next_id: 1 };
  const report = () => view()?.report;
  const plane = (): Plane => {
    const b = BASE_PLANES[planeKey()];
    return { ...b, origin: b.normal.map((v) => v * offset()) as P3 };
  };
  const coords = (s: Sketch3d, pid: number): P3 | undefined => {
    const p = s.points.find((q) => q.id === pid);
    return p ? [p.x, p.y, p.z] : undefined;
  };
  /**
   * Cambia el sketch del borrador con los puntos donde los dejó el solver (lo
   * que sigue arranca de la geometría que cumple todo; si la vista previa
   * todavía no llegó, el borrador manda)
   */
  const change = (mutate: (s: Sketch3d) => void) =>
    props.store.updateFeature(id(), (f) => {
      if (f.kind.type !== "sketch3d") return;
      const s = structuredClone(f.kind.sketch);
      const solved = new Map((view()?.sketch.points ?? []).map((p) => [p.id, p]));
      for (const p of s.points) {
        const q = solved.get(p.id);
        if (q) [p.x, p.y, p.z] = [q.x, q.y, q.z];
      }
      mutate(s);
      f.kind.sketch = s;
    });
  const fresh = (s: Sketch3d) => {
    const used = Math.max(0, ...s.points.map((p) => p.id + 1), ...s.entities.map((e) => e.id + 1));
    const n = Math.max(s.next_id, used);
    s.next_id = n + 1;
    return n;
  };
  /** El punto del clic: uno que ya está o uno nuevo (atado al vértice del sólido si cayó en uno) */
  const place = (s: Sketch3d, hit: Hit3d, spec: PointSpec | undefined): number => {
    if (hit.point !== undefined && s.points.some((p) => p.id === hit.point)) return hit.point;
    const pid = fresh(s);
    s.points.push({ id: pid, x: hit.at[0], y: hit.at[1], z: hit.at[2] });
    if (spec) s.constraints.push({ type: "attach", point: pid, target: spec });
    return pid;
  };
  /** Línea nueva; si va casi por un eje, queda paralela a él (como al dibujar en un plano) */
  const addLine = (s: Sketch3d, a: number, b: number) => {
    const lid = fresh(s);
    s.entities.push({ id: lid, geometry: { type: "line", start: a, end: b } });
    const [pa, pb] = [coords(s, a), coords(s, b)];
    if (!pa || !pb) return;
    const d = sub3(pb, pa);
    const l = len3(d);
    const axis = (["x", "y", "z"] as const).find((_, k) => l > 1e-9 && Math.abs(d[k]) / l > Math.cos((3 * Math.PI) / 180));
    if (axis) s.constraints.push({ type: "along_axis", line: lid, axis });
  };
  const onClick = async (hit: Hit3d, e: PointerEvent) => {
    const t = tool();
    if (t === "select") {
      const target = hit.point ?? hit.entity;
      if (target === undefined) return ui.setSel3d([]);
      ui.setSel3d((cur) => (e.shiftKey ? (cur.includes(target) ? cur.filter((x) => x !== target) : [...cur, target]) : cur.length === 1 && cur[0] === target ? [] : [target]));
      return;
    }
    const spec = hit.vertex && hit.point === undefined ? await props.store.vertexSpec(hit.at).catch(() => undefined) : undefined;
    const tr = trace();
    let made = -1;
    // El cambio al borrador es inmediato (la vista previa llega después): el trazo sigue ya
    const pending = change((s) => {
      made = place(s, hit, spec);
      if (t === "line" && tr.length && tr[0] !== made) addLine(s, tr[0], made);
      if (t === "arc" && tr.length === 2 && !tr.includes(made)) s.entities.push({ id: fresh(s), geometry: { type: "arc", start: tr[0], mid: tr[1], end: made } });
      if (t === "point" && hit.point === undefined) s.entities.push({ id: fresh(s), geometry: { type: "point", point: made } });
    });
    if (t === "line") setTrace([made]);
    else if (t === "arc") setTrace(tr.length === 2 ? [] : [...tr, made]);
    else if (t === "spline") setTrace([...tr, made]);
    await pending;
  };
  /** Corta el trazo; una spline en curso se crea */
  const endTrace = () => {
    const tr = trace();
    if (tool() === "spline" && tr.length >= 2) void change((s) => s.entities.push({ id: fresh(s), geometry: { type: "spline", points: tr } }));
    setTrace([]);
  };
  createEffect(
    on([tool, planeKey, offset], () => {
      setTrace([]);
      if (tool() !== "select") ui.setSel3d([]);
    }),
  );
  // El visor dibuja con el plano activo; el comienzo del tramo, para la línea guía
  createEffect(() => {
    const tr = trace();
    const from = tr.length && tool() !== "select" ? coords(sketch(), tr[tr.length - 1]) : undefined;
    const t = TOOL3D.find((x) => x.id === tool())!;
    ui.setPick({ kind: "sketch3d", prompt: t.prompt, owner: `${id()}:sketch3d`, feature: id(), plane: plane(), from, click: (h, e) => void onClick(h, e) });
  });
  onCleanup(() => {
    if (ui.pick().kind === "sketch3d") ui.setPick({ kind: "none" });
    ui.setSel3d([]);
  });

  const selPoints = () => ui.sel3d().filter((x) => sketch().points.some((p) => p.id === x));
  const selEntities = () => ui.sel3d().flatMap((x) => sketch().entities.filter((e) => e.id === x));
  const selLines = () => selEntities().filter((e) => e.geometry.type === "line");
  const lineVec = (s: Sketch3d, lid: number): P3 | undefined => {
    const g = s.entities.find((e) => e.id === lid)?.geometry;
    if (g?.type !== "line") return undefined;
    const [a, b] = [coords(s, g.start), coords(s, g.end)];
    return a && b ? sub3(b, a) : undefined;
  };
  const round = (v: number) => Math.round(v * 1e4) / 1e4;
  const add = (c: Constraint3d | Constraint3d[]) => {
    void change((s) => s.constraints.push(...[c].flat()));
  };
  /** Restricciones posibles para lo elegido */
  const options = (): { label: string; make: () => Constraint3d[] }[] => {
    const s = sketch();
    const P = selPoints();
    const L = selLines().map((e) => e.id);
    const E = selEntities();
    const out: { label: string; make: () => Constraint3d[] }[] = [];
    if (P.length === 2 && E.length === 0) {
      out.push({ label: "Coincidentes", make: () => [{ type: "coincident", a: P[0], b: P[1] }] });
      out.push({ label: "Distancia", make: () => [{ type: "distance", a: P[0], b: P[1], value: round(len3(sub3(coords(s, P[1])!, coords(s, P[0])!))) }] });
    }
    if (P.length >= 1 && E.length === 0) {
      out.push({ label: "Sobre el plano activo", make: () => P.map((p) => ({ type: "on_plane", point: p, plane: { type: planeKey() }, offset: offset() }) as Constraint3d) });
      out.push({ label: "Fijo", make: () => P.map((p) => ({ type: "fixed", point: p, at: coords(s, p)! }) as Constraint3d) });
    }
    if (L.length === 1 && P.length === 0 && E.length === 1) {
      for (const axis of ["x", "y", "z"] as const) out.push({ label: `Paralela a ${axis.toUpperCase()}`, make: () => [{ type: "along_axis", line: L[0], axis }] });
      out.push({ label: "Largo", make: () => [{ type: "length", line: L[0], value: round(len3(lineVec(s, L[0])!)) }] });
    }
    if (L.length === 2 && E.length === 2 && P.length === 0) {
      out.push({ label: "Paralelas", make: () => [{ type: "parallel", a: L[0], b: L[1] }] });
      out.push({ label: "Perpendiculares", make: () => [{ type: "perpendicular", a: L[0], b: L[1] }] });
      out.push({ label: "Mismo largo", make: () => [{ type: "equal", a: L[0], b: L[1] }] });
      out.push({
        label: "Ángulo",
        make: () => {
          const [u, v] = [lineVec(s, L[0])!, lineVec(s, L[1])!];
          const cos = (u[0] * v[0] + u[1] * v[1] + u[2] * v[2]) / (len3(u) * len3(v) || 1);
          return [{ type: "angle", a: L[0], b: L[1], degrees: round((Math.acos(Math.max(-1, Math.min(1, cos))) * 180) / Math.PI) }];
        },
      });
    }
    if (E.length === 2 && P.length === 0 && E.every((e) => e.geometry.type === "line" || e.geometry.type === "arc") && E.some((e) => e.geometry.type === "arc")) {
      out.push({ label: "Tangentes", make: () => [{ type: "tangent", a: E[0].id, b: E[1].id }] });
    }
    if (P.length === 1 && L.length === 1 && E.length === 1) out.push({ label: "Punto medio", make: () => [{ type: "midpoint", point: P[0], line: L[0] }] });
    return out;
  };
  /** Borra lo elegido: entidades, puntos sueltos y lo que los nombra */
  const removeSelected = () => {
    const gone = new Set(ui.sel3d());
    void change((s) => {
      s.entities = s.entities.filter((e) => !gone.has(e.id));
      const used = new Set(s.entities.flatMap((e) => (e.geometry.type === "spline" ? e.geometry.points : e.geometry.type === "point" ? [e.geometry.point] : e.geometry.type === "arc" ? [e.geometry.start, e.geometry.mid, e.geometry.end] : [e.geometry.start, e.geometry.end])));
      s.points = s.points.filter((p) => used.has(p.id) || !gone.has(p.id));
      const left = new Set([...s.points.map((p) => p.id), ...s.entities.map((e) => e.id)]);
      s.constraints = s.constraints.filter((c) => ids3d(c).every((x) => left.has(x)));
    });
    ui.setSel3d([]);
  };
  const statusText = () => {
    const r = report();
    if (!r) return "";
    if (r.status === "well_constrained") return "Totalmente definido";
    if (r.status === "under_constrained") return `${r.dof} grados de libertad`;
    return "Hay restricciones en conflicto (en rojo); lo demás se cumple";
  };
  const onePoint = () => (selPoints().length === 1 && selEntities().length === 0 ? sketch().points.find((p) => p.id === selPoints()[0]) : undefined);

  return (
    <div class="space-y-2" data-sketch3d>
      <div class="flex flex-wrap gap-1" role="radiogroup" aria-label="Herramienta del sketch 3D">
        <For each={TOOL3D}>
          {(t) => (
            <button
              role="radio"
              aria-checked={tool() === t.id}
              class={clsx("px-2 py-0.5 rounded text-xs border", tool() === t.id ? "bg-accent/20 border-accent text-text" : "border-border text-text-muted hover:text-text")}
              title={t.prompt}
              onClick={() => setTool(t.id)}
            >
              {t.label}
            </button>
          )}
        </For>
      </div>
      <Row label="Plano activo">
        <Select
          options={(["xy", "xz", "yz"] as const).map((k) => ({ value: k, label: PLANE_LABELS[k] }))}
          value={planeKey()}
          onChange={(v) => setPlaneKey(v as "xy" | "xz" | "yz")}
        />
      </Row>
      <Num label="Desplazado" suffix="mm" step={1} value={offset()} onCommit={setOffset} />
      <div class="flex flex-wrap gap-1.5">
        <Button size="sm" variant="ghost" disabled={trace().length === 0} onClick={endTrace}>
          Terminar trazo
        </Button>
        <Show when={ui.pick().kind !== "sketch3d"}>
          <Button size="sm" onClick={() => setTool(tool())}>
            Dibujar en el visor
          </Button>
        </Show>
      </div>
      <p class={clsx("text-xs", report()?.status === "well_constrained" ? "text-success" : report()?.status === "over_constrained" ? "text-error" : "text-text-muted")} data-sketch3d-status>
        {sketch().points.length} puntos · {sketch().entities.length} curvas
        <Show when={statusText()}> · {statusText()}</Show>
      </p>

      <Show when={ui.sel3d().length > 0}>
        <div class="space-y-1.5 rounded border border-border p-1.5">
          <p class="text-[11px] text-text-dim">Elegido: {ui.sel3d().length}</p>
          <Show when={onePoint()}>
            {(p) => (
              <div class="grid grid-cols-3 gap-1">
                <For each={["x", "y", "z"] as const}>
                  {(k, i) => (
                    <Num
                      label={k.toUpperCase()}
                      step={1}
                      value={p()[k]}
                      onCommit={(v) =>
                        void change((s) => {
                          const q = s.points.find((x) => x.id === p().id);
                          if (q) q[(["x", "y", "z"] as const)[i()]] = v;
                        })
                      }
                    />
                  )}
                </For>
              </div>
            )}
          </Show>
          <div class="flex flex-wrap gap-1">
            <For each={options()}>
              {(o) => (
                <Button size="sm" onClick={() => add(o.make())}>
                  {o.label}
                </Button>
              )}
            </For>
            <Show when={selEntities().length > 0}>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => {
                  const ids = new Set(selEntities().map((e) => e.id));
                  const on = !selEntities().every((e) => e.construction);
                  void change((s) => s.entities.forEach((e) => ids.has(e.id) && (e.construction = on)));
                }}
              >
                Construcción
              </Button>
            </Show>
            <Button size="sm" variant="danger" icon={<Icons.Trash size={12} />} onClick={removeSelected}>
              Borrar
            </Button>
          </div>
        </div>
      </Show>

      <Show when={sketch().constraints.length > 0}>
        <div class="space-y-0.5 max-h-56 overflow-y-auto" aria-label="Restricciones del sketch 3D">
          <For each={sketch().constraints}>
            {(c, i) => {
              const conflict = () => report()?.conflicting.includes(i()) ?? false;
              const value = () => ("value" in c ? c.value : "degrees" in c ? c.degrees : undefined);
              return (
                <div
                  class={clsx("flex items-center gap-2 px-1.5 py-0.5 rounded text-xs hover:bg-surface", conflict() ? "bg-error/15 text-error" : "text-text-muted")}
                  data-constraint3d={c.type}
                >
                  <button class="flex-1 truncate text-left hover:text-text" title="Elegir lo que restringe" onClick={() => ui.setSel3d(ids3d(c))}>
                    {c.type === "along_axis" ? `Paralela a ${c.axis.toUpperCase()}` : C3D_LABELS[c.type]}
                  </button>
                  <Show when={value() !== undefined}>
                    <div class="w-20">
                      <Num
                        step={1}
                        value={value()!}
                        onCommit={(v) =>
                          void change((s) => {
                            const x = s.constraints[i()];
                            if (x && "value" in x) x.value = v;
                            else if (x && "degrees" in x) x.degrees = v;
                          })
                        }
                      />
                    </div>
                  </Show>
                  <IconButton size="sm" aria-label="Quitar" onClick={() => void change((s) => s.constraints.splice(i(), 1))}>
                    <Icons.X size={11} />
                  </IconButton>
                </div>
              );
            }}
          </For>
        </div>
      </Show>
      <p class="text-[11px] text-text-dim leading-relaxed">
        Clic en un vértice del sólido: el punto queda atado a él. Las líneas casi paralelas a un eje quedan paralelas. Sirve de camino para un Barrido (elegirlo como curva); un perfil en un plano que la cruce se
        ubica con «Perforar» en su sketch.
      </p>
    </div>
  );
};

const SketchPanel: Component<{ ui: CadUi; store: CadStore }> = (props) => {
  const ui = props.ui;
  const [cornerRadius, setCornerRadius] = createSignal(5);
  const [offsetDist, setOffsetDist] = createSignal(2);
  const [moveBy, setMoveBy] = createSignal<P2>([10, 0]);
  const [turnBy, setTurnBy] = createSignal(90);
  const [scaleBy, setScaleBy] = createSignal(2);
  const [patternCount, setPatternCount] = createSignal(3);
  const [patternKind, setPatternKind] = createSignal<SketchPatternKind>("linear");
  const [patternStep, setPatternStep] = createSignal<P2>([20, 0]);
  const [patternRows, setPatternRows] = createSignal(1);
  const [patternStep2, setPatternStep2] = createSignal<P2>([0, 20]);
  const [patternAngle, setPatternAngle] = createSignal(360);
  const [patternRotate, setPatternRotate] = createSignal(true);
  const [curveRotate, setCurveRotate] = createSignal(false);
  const [tableRows, setTableRows] = createSignal<P2[]>([
    [10, 0],
    [20, 5],
  ]);
  const [fillSpacing, setFillSpacing] = createSignal(10);
  const [fillMargin, setFillMargin] = createSignal(1);
  const [fillHex, setFillHex] = createSignal(false);
  // Restricciones que faltan: se piden con el botón; cambiar el sketch deja la lista vieja
  const [missing, setMissing] = createSignal<SketchSuggestion[]>();
  const [defineRelations, setDefineRelations] = createSignal(true);
  const [assistBusy, setAssistBusy] = createSignal(false);
  createEffect(on(() => ui.sketchHistory(), () => setMissing(undefined), { defer: true }));
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
  const add = (c: SketchConstraint | SketchConstraint[]) => ui.change((s) => s.constraints.push(...[c].flat()));
  const askMissing = async () => {
    setAssistBusy(true);
    try {
      setMissing(await props.store.suggestSketch(sketch()));
    } catch (e) {
      ui.setMessage(String(e));
    } finally {
      setAssistBusy(false);
    }
  };
  /** Agrega sugeridas y vuelve a preguntar qué falta (con lo nuevo puesto) */
  const applyMissing = (list: SketchSuggestion[]) => {
    ui.setHoverIds([]);
    add(list.map((m) => m.constraint));
    void askMissing();
  };
  const defineAll = async () => {
    setAssistBusy(true);
    try {
      const more = await props.store.defineSketch(sketch(), defineRelations());
      if (more.length === 0) ui.setMessage("No encontré qué agregar: lo que queda libre no se define con cotas simples");
      else {
        add(more);
        setMissing(undefined);
      }
    } catch (e) {
      ui.setMessage(String(e));
    } finally {
      setAssistBusy(false);
    }
  };
  /** Curvas de operaciones anteriores al sketch (hélices, sketches 3D o envueltos) que pueden perforarlo */
  const pierceCurves = () => {
    const doc = props.store.doc();
    const at = doc?.features.findIndex((f) => f.id === s().feature) ?? -1;
    const before = new Set((doc?.features ?? []).slice(0, Math.max(0, at)).map((f) => f.id));
    const names = new Map((doc?.features ?? []).map((f) => [f.id, f.name]));
    return (props.store.result()?.references ?? [])
      .filter((r) => (r.kind === "curve" || r.kind === "curves") && before.has(r.id))
      .map((r) => ({ value: String(r.id), label: names.get(r.id) ?? `Curva ${r.id}` }));
  };
  /** Ata el punto a donde la curva cruza el plano del sketch (el cruce más cercano) */
  const pierce = (pid: number, curve: number): string | undefined => {
    const ref = props.store.result()?.references.find((r) => r.id === curve);
    if (!ref || (ref.kind !== "curve" && ref.kind !== "curves")) return "Esa curva no está calculada";
    const plane = s().plane;
    const n = plane.normal;
    const side = (p: P3) => (p[0] - plane.origin[0]) * n[0] + (p[1] - plane.origin[1]) * n[1] + (p[2] - plane.origin[2]) * n[2];
    const hits: P2[] = [];
    for (const line of ref.kind === "curve" ? [ref.points] : ref.lines) {
      for (let i = 1; i < line.length; i++) {
        const [a, b] = [line[i - 1], line[i]];
        const [da, db] = [side(a), side(b)];
        if ((da > 0) === (db > 0) && Math.abs(da) > 1e-9 && Math.abs(db) > 1e-9) continue;
        const t = Math.abs(da - db) < 1e-15 ? 0 : da / (da - db);
        const w: P3 = [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1]), a[2] + t * (b[2] - a[2])];
        const d: P3 = [w[0] - plane.origin[0], w[1] - plane.origin[1], w[2] - plane.origin[2]];
        const x = plane.x_dir;
        const y: P3 = [n[1] * x[2] - n[2] * x[1], n[2] * x[0] - n[0] * x[2], n[0] * x[1] - n[1] * x[0]];
        hits.push([d[0] * x[0] + d[1] * x[1] + d[2] * x[2], d[0] * y[0] + d[1] * y[1] + d[2] * y[2]]);
      }
    }
    const p = point(pid);
    if (!p || hits.length === 0) return "Esa curva no cruza el plano del sketch";
    const at = hits.reduce((best, h) => (Math.hypot(h[0] - p.x, h[1] - p.y) < Math.hypot(best[0] - p.x, best[1] - p.y) ? h : best));
    const rounded: P2 = [Math.round(at[0] * 1e6) / 1e6, Math.round(at[1] * 1e6) / 1e6];
    ui.change((sk) => {
      sk.constraints = sk.constraints.filter((c) => !(c.type === "pierce" && c.point === pid));
      sk.constraints.push({ type: "pierce", point: pid, curve, at: rounded });
    });
    return undefined;
  };
  /** Grados libres de una entidad (undefined: ya definida) */
  const entityDof = (id: number) => s().report?.entity_dof?.find((d) => d[0] === id)?.[1];
  const measure = (c: SketchConstraint) => +(measureConstraint(sketch(), c) ?? 0).toFixed(3);
  const isArc = (id: number) => entity(id)?.geometry.type === "arc";

  const suggestions = (): { label: string; make: () => SketchConstraint | SketchConstraint[] }[] => {
    const out: { label: string; make: () => SketchConstraint | SketchConstraint[] }[] = [];
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
      out.push({ label: "Colineales", make: () => ({ type: "collinear", a: L[0], b: L[1] }) });
      out.push({ label: "Mismo largo", make: () => ({ type: "equal", a: L[0], b: L[1] }) });
      out.push({ label: "Ángulo", make: () => ({ type: "angle", a: L[0], b: L[1], degrees: 90 }) });
      // Entre paralelas: un extremo de la segunda hasta la recta de la primera (y paralelas si no lo eran)
      out.push({
        label: "Distancia entre paralelas",
        make: () => {
          const g = entity(L[1])!.geometry as { start: number };
          const c: SketchConstraint = { type: "point_line_distance", point: g.start, line: L[0], value: 0 };
          c.value = measure(c);
          const already = sketch().constraints.some((k) => k.type === "parallel" && [k.a, k.b].includes(L[0]) && [k.a, k.b].includes(L[1]));
          return already ? c : [{ type: "parallel", a: L[0], b: L[1] }, c];
        },
      });
    }
    if (C.length === 1 && L.length === 0) {
      out.push({ label: "Radio", make: () => ({ type: "radius", entity: C[0], value: +radius(C[0]).toFixed(3) }) });
      out.push({ label: "Diámetro", make: () => ({ type: "diameter", entity: C[0], value: +(2 * radius(C[0])).toFixed(3) }) });
      if (isArc(C[0])) {
        const c: SketchConstraint = { type: "arc_length", arc: C[0], value: 0 };
        out.push({ label: "Largo del arco", make: () => ({ ...c, value: measure(c) }) });
      }
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
      const d: SketchConstraint = { type: "point_line_distance", point: P[0], line: L[0], value: 0 };
      out.push({ label: "Distancia a la línea", make: () => ({ ...d, value: measure(d) }) });
      // Perfiles de revolución: el diámetro que da el punto al girar alrededor de la línea
      const ax: SketchConstraint = { type: "axis_diameter", point: P[0], line: L[0], value: 0 };
      out.push({ label: "Diámetro respecto del eje", make: () => ({ ...ax, value: measure(ax) }) });
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
    if (L.length === 2) {
      const c: SketchConstraint = { type: "angle", a: L[0], b: L[1], degrees: 0, supplementary: true };
      out.push({ label: "Ángulo suplementario", make: () => ({ ...c, degrees: measure(c) }) });
    }
    // Cadena de líneas, arcos y círculos: su largo total
    const E = selEntities().map((e) => e!);
    const chain = E.filter((e) => ["line", "arc", "circle"].includes(e.geometry.type));
    if (chain.length >= 2 && chain.length === E.length && P.length === 0) {
      const c: SketchConstraint = { type: "curve_length", entities: chain.map((e) => e.id), value: 0 };
      out.push({ label: "Largo total", make: () => ({ ...c, value: measure(c) }) });
    }
    // Distancia mínima y máxima: un círculo o arco con otro, un punto o una línea
    const pair: [number, number] | undefined =
      C.length === 2 && L.length === 0 && P.length === 0 ? [C[0], C[1]] : C.length === 1 && L.length + P.length === 1 && E.length === L.length + 1 ? [C[0], L[0] ?? P[0]] : undefined;
    if (pair) {
      for (const max of [false, true]) {
        const c: SketchConstraint = { type: "circle_distance", a: pair[0], b: pair[1], ...(max ? { max } : {}), value: 0 };
        out.push({ label: max ? "Distancia máxima" : "Distancia mínima", make: () => ({ ...c, value: measure(c) }) });
      }
    }
    if (C.length === 2 && L.length === 0) out.push({ label: "Coradiales", make: () => ({ type: "coradial", a: C[0], b: C[1] }) });
    // Dos entidades del mismo tipo y una línea (la última elegida) como eje
    if (E.length === 3 && P.length === 0) {
      const axis = [...sel()].reverse().find((id) => entity(id)?.geometry.type === "line");
      const pairE = E.filter((e) => e.id !== axis);
      if (axis !== undefined && pairE.length === 2 && pairE[0].geometry.type === pairE[1].geometry.type)
        out.push({ label: "Simétricas (eje: la última línea)", make: () => ({ type: "symmetric_entities", a: pairE[0].id, b: pairE[1].id, line: axis }) });
    }
    // Con una spline por polos: tangencia y curvatura igual (G2) en el extremo común
    if (E.length === 2 && P.length === 0 && E.some((e) => e.geometry.type === "bspline") && E.every((e) => ["line", "arc", "bspline"].includes(e.geometry.type))) {
      out.push({ label: "Tangentes en la unión", make: () => ({ type: "tangent", a: E[0].id, b: E[1].id }) });
      out.push({ label: "Curvatura igual (G2)", make: () => ({ type: "curvature", a: E[0].id, b: E[1].id }) });
    }
    const curvesAny = E.filter((e) => e.geometry.type !== "point");
    if (P.length === 1 && curvesAny.length === 1 && E.length === 1 && ["ellipse", "spline", "bspline", "ellipse_arc"].includes(curvesAny[0].geometry.type))
      out.push({ label: "Punto en la curva", make: () => ({ type: "point_on_curve", point: P[0], curve: curvesAny[0].id }) });
    if (P.length === 1 && curvesAny.length === 2 && E.length === 2)
      out.push({ label: "Punto en la intersección", make: () => ({ type: "intersection", point: P[0], a: curvesAny[0].id, b: curvesAny[1].id }) });
    if (E.length > 0) {
      const locked = new Set(sketch().constraints.flatMap((c) => (c.type === "lock" ? [c.entity] : [])));
      const fresh = E.filter((e) => !locked.has(e.id));
      if (fresh.length) out.push({ label: fresh.length > 1 ? "Bloquear las entidades" : "Bloquear la entidad", make: () => fresh.map((e) => ({ type: "lock", entity: e.id })) });
    }
    // Varios puntos: cotas de ordenadas desde el primero o en cadena
    if (P.length >= 3 && E.length === 0) {
      for (const [axis, name] of [
        [0, "horizontales"],
        [1, "verticales"],
      ] as const) {
        out.push({ label: `Ordenadas ${name}`, make: () => ordinateDims(P, axis) });
        out.push({ label: `Cadena ${name}`, make: () => chainDims(P, axis) });
      }
    }
    return out;
  };

  /** Fila donde van los textos: un poco más allá de los puntos, del lado de afuera */
  const dimRow = (ids: number[], axis: 0 | 1) => {
    const other = ids.map((id) => (axis === 0 ? point(id)!.y : point(id)!.x));
    const span = Math.max(...ids.map((id) => (axis === 0 ? point(id)!.x : point(id)!.y))) - Math.min(...ids.map((id) => (axis === 0 ? point(id)!.x : point(id)!.y)));
    return Math.max(...other) + Math.max(2, span * 0.08);
  };
  /** Distancias desde el primer punto elegido a cada uno de los otros, con los textos en fila junto a cada punto */
  const ordinateDims = (ids: number[], axis: 0 | 1): SketchConstraint[] => {
    const row = dimRow(ids, axis);
    const type = axis === 0 ? "horizontal_distance" : "vertical_distance";
    return ids.slice(1).map((b) => {
      const p = point(b)!;
      const offset: [number, number] = axis === 0 ? [0, +(row - p.y).toFixed(4)] : [+(row - p.x).toFixed(4), 0];
      const c: SketchConstraint = { type, a: ids[0], b, value: 0, opts: { ordinate: true, offset } };
      return { ...c, value: measure(c) } as SketchConstraint;
    });
  };
  /** Cada punto desde el anterior (ordenados a lo largo del eje), con los textos en fila */
  const chainDims = (ids: number[], axis: 0 | 1): SketchConstraint[] => {
    const coord = (id: number) => (axis === 0 ? point(id)!.x : point(id)!.y);
    const sorted = [...ids].sort((a, b) => coord(a) - coord(b));
    const row = dimRow(ids, axis);
    const type = axis === 0 ? "horizontal_distance" : "vertical_distance";
    return sorted.slice(1).map((b, i) => {
      const a = sorted[i];
      const [pa, pb] = [point(a)!, point(b)!];
      const offset: [number, number] = axis === 0 ? [0, +(row - (pa.y + pb.y) / 2).toFixed(4)] : [+(row - (pa.x + pb.x) / 2).toFixed(4), 0];
      const c: SketchConstraint = { type, a, b, value: 0, opts: { offset } };
      return { ...c, value: measure(c) } as SketchConstraint;
    });
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
                    ? r().partial
                      ? "Hay restricciones en conflicto (marcadas en rojo); lo demás se cumple"
                      : "Hay restricciones en conflicto (marcadas en rojo)"
                    : "No se pudo resolver"}
              {" · "}
              {s().regions.length} regiones cerradas
            </p>
          )}
        </Show>
      </Section>

      <Section title="Definir">
        <Show when={selEntities().length === 1 && entityDof(selEntities()[0]!.id)}>
          {(n) => (
            <p class="text-xs text-text-muted" data-entity-dof={n()}>
              Lo elegido tiene {n()} {n() === 1 ? "grado libre" : "grados libres"} (las flechas amarillas muestran hacia dónde se mueve)
            </p>
          )}
        </Show>
        <div class="flex flex-wrap gap-1.5">
          <Button size="sm" disabled={assistBusy()} title="Relaciones casi cumplidas (casi horizontal, casi coincidentes…) y cotas que faltan" onClick={() => void askMissing()}>
            Sugerir restricciones
          </Button>
          <Button
            size="sm"
            disabled={assistBusy() || s().report?.status === "well_constrained"}
            title="Agrega lo que falta para que quede totalmente definido: cotas con lo que mide ahora (nada se mueve) desde el origen"
            onClick={() => void defineAll()}
          >
            Definir todo
          </Button>
        </div>
        <Checkbox small label="Con las relaciones casi cumplidas" checked={defineRelations()} onChange={setDefineRelations} />
        <Show when={missing()}>
          {(list) => (
            <Show when={list().length > 0} fallback={<p class="text-xs text-text-dim">No falta nada evidente</p>}>
              <div class="space-y-0.5 max-h-56 overflow-y-auto" aria-label="Restricciones sugeridas">
                <For each={list()}>
                  {(m) => (
                    <div
                      class="flex items-center gap-2 px-1.5 py-0.5 rounded text-xs text-text-muted hover:bg-surface"
                      data-suggestion={m.constraint.type}
                      onMouseEnter={() => ui.setHoverIds(constraintIds(m.constraint))}
                      onMouseLeave={() => ui.setHoverIds([])}
                    >
                      <span class="w-4 shrink-0 text-center text-[10px]">{GLYPHS[m.constraint.type] ?? (isDimension(m.constraint) ? "↔" : "·")}</span>
                      <span class="flex-1 truncate" title={m.why}>
                        {CONSTRAINT_LABELS[m.constraint.type]}
                        <span class="text-text-dim"> · {m.why}</span>
                      </span>
                      <Button size="sm" variant="ghost" onClick={() => applyMissing([m])}>
                        Agregar
                      </Button>
                    </div>
                  )}
                </For>
              </div>
              <Button size="sm" onClick={() => applyMissing(list())}>
                Agregar todas ({list().length})
              </Button>
            </Show>
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
          <Show when={selPoints().length === 1 && selEntities().length === 0 && pierceCurves().length > 0}>
            <div class="flex flex-wrap items-center gap-1.5" aria-label="Perforar">
              <span class="text-xs text-text-muted">Perforar:</span>
              <For each={pierceCurves()}>
                {(c) => (
                  <Button
                    size="sm"
                    title="El punto va donde la curva cruza el plano del sketch (y la sigue si cambia)"
                    onClick={() => {
                      const msg = pierce(selPoints()[0], +c.value);
                      if (msg) ui.setMessage(msg);
                    }}
                  >
                    {c.label}
                  </Button>
                )}
              </For>
            </div>
          </Show>
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
          <SplineTools ui={ui} />
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
                <Show when={patternKind() !== "table" && patternKind() !== "fill"}>
                  <div class="flex-1">
                    <Num label="Cantidad" min={2} value={patternCount()} onCommit={(v) => setPatternCount(Math.max(2, Math.round(v)))} />
                  </div>
                </Show>
                <div class={patternKind() === "table" || patternKind() === "fill" ? "flex-1" : undefined}>
                  <Select options={SKETCH_PATTERNS} value={patternKind()} onChange={(v) => setPatternKind(v as SketchPatternKind)} />
                </div>
              </div>
              <Switch>
                <Match when={patternKind() === "linear"}>
                  <div class="flex gap-1.5">
                    <Num label="X" suffix="mm" step={1} value={patternStep()[0]} onCommit={(v) => setPatternStep([v, patternStep()[1]])} />
                    <Num label="Y" suffix="mm" step={1} value={patternStep()[1]} onCommit={(v) => setPatternStep([patternStep()[0], v])} />
                  </div>
                  <Num label="Filas" min={1} value={patternRows()} onCommit={(v) => setPatternRows(Math.max(1, Math.round(v)))} />
                  <Show when={patternRows() > 1}>
                    <div class="flex gap-1.5" aria-label="Paso entre filas">
                      <Num label="X" suffix="mm" step={1} value={patternStep2()[0]} onCommit={(v) => setPatternStep2([v, patternStep2()[1]])} />
                      <Num label="Y" suffix="mm" step={1} value={patternStep2()[1]} onCommit={(v) => setPatternStep2([patternStep2()[0], v])} />
                    </div>
                  </Show>
                </Match>
                <Match when={patternKind() === "circular"}>
                  <Num label="Ángulo" suffix="°" step={15} value={patternAngle()} onCommit={setPatternAngle} />
                  <Checkbox small label="Girar las copias" checked={patternRotate()} onChange={setPatternRotate} />
                  <p class="text-[11px] text-text-dim">
                    {selPoints().length === 1 ? "Alrededor del punto elegido" : "Alrededor del origen (o elegir también un punto)"};{" "}
                    {Math.abs(patternAngle()) >= 360 ? "repartidos en la vuelta" : "de punta a punta del ángulo (negativo: horario)"}
                  </p>
                </Match>
                <Match when={patternKind() === "curve"}>
                  <Checkbox small label="Girar con la curva" checked={curveRotate()} onChange={setCurveRotate} />
                  <p class="text-[11px] text-text-dim">
                    El camino es la primera entidad elegida (línea, arco, círculo, spline o elipse); las copias se reparten de punta a punta. En líneas,
                    arcos y círculos siguen al camino si cambia.
                  </p>
                </Match>
                <Match when={patternKind() === "table"}>
                  <div class="space-y-1" aria-label="Tabla de copias">
                    <span class="text-xs text-text-muted">Desplazamiento de cada copia (mm)</span>
                    <For each={tableRows()}>
                      {(row, i) => (
                        <div class="flex items-center gap-1" data-table-row={i()}>
                          <Num label="X" step={1} value={row[0]} onCommit={(v) => setTableRows(tableRows().map((r, j) => (j === i() ? [v, r[1]] : r)))} />
                          <Num label="Y" step={1} value={row[1]} onCommit={(v) => setTableRows(tableRows().map((r, j) => (j === i() ? [r[0], v] : r)))} />
                          <IconButton aria-label={`Quitar la fila ${i() + 1}`} size="sm" variant="ghost" onClick={() => setTableRows(tableRows().filter((_, j) => j !== i()))}>
                            <Icons.Trash size={11} />
                          </IconButton>
                        </div>
                      )}
                    </For>
                    <Button
                      size="sm"
                      fullWidth
                      onClick={() => {
                        const last = tableRows()[tableRows().length - 1] ?? [0, 0];
                        setTableRows([...tableRows(), [last[0] + 10, last[1]]]);
                      }}
                    >
                      Agregar fila
                    </Button>
                  </div>
                </Match>
                <Match when={patternKind() === "fill"}>
                  <Num label="Separación" suffix="mm" step={1} min={0} value={fillSpacing()} onCommit={(v) => setFillSpacing(Math.max(0.01, v))} />
                  <Num label="Margen al borde" suffix="mm" step={0.5} min={0} value={fillMargin()} onCommit={(v) => setFillMargin(Math.max(0, v))} />
                  <Checkbox small label="Grilla hexagonal" checked={fillHex()} onChange={setFillHex} />
                  <p class="text-[11px] text-text-dim">
                    La región es el lazo cerrado de la primera entidad elegida; entran las copias enteras a no menos del margen del borde. La grilla pasa
                    por el original.
                  </p>
                </Match>
              </Switch>
              <Button
                size="sm"
                onClick={() => {
                  const ids = selEntities().map((e) => e!.id);
                  const n = patternCount();
                  let msg: string | undefined;
                  const kind = patternKind();
                  if (kind === "linear") ui.change((sk) => (msg = linearPattern(sk, ids, n, patternStep(), patternRows(), patternStep2())));
                  else if (kind === "circular") {
                    const center = selPoints().length === 1 ? selPoints()[0] : sketch().origin;
                    if (center === undefined) msg = "Elegir el punto alrededor del cual repetir";
                    else ui.change((sk) => (msg = circularPattern(sk, ids, n, center, patternAngle(), patternRotate())));
                  } else if (kind === "curve") ui.change((sk) => (msg = curvePattern(sk, ids, n, ids[0], curveRotate())));
                  else if (kind === "table") ui.change((sk) => (msg = tablePattern(sk, ids, tableRows())));
                  else {
                    const first = ids[0];
                    const region = s().regions.find((r) => r.outer.pieces.some((p) => p.entity === first));
                    const border = new Set(region?.outer.pieces.map((p) => p.entity));
                    if (!region) msg = "La primera entidad elegida tiene que ser el borde de una región cerrada";
                    else
                      ui.change((sk) => (msg = fillPattern(sk, ids.filter((id) => !border.has(id)), region, fillSpacing(), fillHex(), fillMargin())));
                  }
                  ui.setMessage(msg);
                  if (!msg) ui.setSelection([]);
                }}
              >
                Repetir en patrón
              </Button>
            </div>
          </Show>
          {/* Con números (con el mouse: herramientas Mover, Copiar, Girar y Escalar de la barra) */}
          <div class="space-y-1.5 border-t border-border pt-1.5" aria-label="Transformar">
            <span class="text-xs text-text-muted">Transformar lo elegido (girar y escalar desde su centro)</span>
            <div class="flex items-end gap-1.5">
              <Num label="X" suffix="mm" step={1} value={moveBy()[0]} onCommit={(v) => setMoveBy([v, moveBy()[1]])} />
              <Num label="Y" suffix="mm" step={1} value={moveBy()[1]} onCommit={(v) => setMoveBy([moveBy()[0], v])} />
            </div>
            <div class="flex gap-1.5">
              <Button size="sm" onClick={() => ui.setMessage(ui.transformSelected(translation(moveBy())))}>
                Mover
              </Button>
              <Button size="sm" onClick={() => ui.setMessage(ui.transformSelected(translation(moveBy()), { copy: true }))}>
                Copiar corrido
              </Button>
            </div>
            <div class="flex items-end gap-1.5">
              <div class="flex-1">
                <Num label="Ángulo" suffix="°" step={15} value={turnBy()} onCommit={setTurnBy} />
              </div>
              <Button
                size="sm"
                title="Alrededor del centro de lo elegido; positivo: antihorario"
                onClick={() => {
                  const c = selectionCenter(sketch(), sel());
                  ui.setMessage(c ? ui.transformSelected(rotation(c, turnBy())) : "Elegir primero lo que se gira");
                }}
              >
                Girar
              </Button>
            </div>
            <div class="flex items-end gap-1.5">
              <div class="flex-1">
                <Num label="Factor" step={0.1} min={0.001} value={scaleBy()} onCommit={(v) => v > 0 && setScaleBy(v)} />
              </div>
              <Button
                size="sm"
                title="Desde el centro de lo elegido; las cotas toman la medida nueva"
                onClick={() => {
                  const c = selectionCenter(sketch(), sel());
                  ui.setMessage(c ? ui.transformSelected(scaling(c, scaleBy()), { scale: scaleBy() }) : "Elegir primero lo que se escala");
                }}
              >
                Escalar
              </Button>
            </div>
          </div>
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

      <SketchModelRefs ui={ui} store={props.store} />

      <SketchEntities ui={ui} />

      <Section title="Restricciones">
        <Show when={sketch().constraints.length > 0} fallback={<p class="text-xs text-text-dim">Sin restricciones</p>}>
          <div class="flex flex-wrap gap-1" aria-label="Elegir restricciones">
            <Button size="sm" variant="ghost" onClick={() => ui.setSelectedConstraints(sketch().constraints.flatMap((c, i) => (isDimension(c) ? [] : [i])))}>
              Todas las restricciones
            </Button>
            <Button size="sm" variant="ghost" onClick={() => ui.setSelectedConstraints(sketch().constraints.flatMap((c, i) => (isDimension(c) ? [i] : [])))}>
              Todas las cotas
            </Button>
            <Show when={ui.selectedConstraints().length > 0}>
              <Button size="sm" variant="danger" icon={<Icons.Trash size={12} />} onClick={() => ui.removeConstraints(ui.selectedConstraints())}>
                Quitar elegidas ({ui.selectedConstraints().length})
              </Button>
            </Show>
          </div>
          <div class="space-y-0.5 max-h-72 overflow-y-auto">
            <For each={sketch().constraints}>
              {(c, i) => {
                const conflict = () => s().report?.conflicting.includes(i()) ?? false;
                const value = () => constraintValue(c);
                return (
                  <div
                    class={clsx(
                      "flex items-center gap-2 px-1.5 py-0.5 rounded text-xs hover:bg-surface",
                      conflict() ? "bg-error/15 text-error" : ui.selectedConstraints().includes(i()) ? "bg-cyan/15 text-text" : "text-text-muted",
                    )}
                    data-constraint-row={i()}
                    onMouseEnter={() => ui.setHoverIds(constraintIds(c))}
                    onMouseLeave={() => ui.setHoverIds([])}
                  >
                    <button
                      class={clsx(
                        "w-4 h-4 shrink-0 rounded border text-[10px] leading-none",
                        ui.selectedConstraints().includes(i()) ? "bg-cyan text-bg border-cyan" : "border-border text-text-dim hover:text-text",
                      )}
                      title="Elegir la restricción (Mayús suma; Supr la quita)"
                      onClick={(e) =>
                        ui.setSelectedConstraints((cur) =>
                          e.shiftKey ? (cur.includes(i()) ? cur.filter((x) => x !== i()) : [...cur, i()]) : cur.length === 1 && cur[0] === i() ? [] : [i()],
                        )
                      }
                    >
                      {GLYPHS[c.type] ?? (isDimension(c) ? "↔" : "·")}
                    </button>
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

const SELECT_KINDS: { kind: SelectKind; label: string }[] = [
  { kind: "lines", label: "Líneas" },
  { kind: "round", label: "Círculos y arcos" },
  { kind: "construction", label: "Construcción" },
  { kind: "points", label: "Puntos" },
  { kind: "free", label: "Sin definir" },
  { kind: "all", label: "Todo" },
];

const ENTITY_NAMES: Record<Geometry["type"], string> = {
  line: "Línea",
  circle: "Círculo",
  arc: "Arco",
  spline: "Spline",
  point: "Punto",
  ellipse: "Elipse",
  ellipse_arc: "Arco elíptico",
  bspline: "B-spline",
};

/** Usar del modelo: silueta, intersección, otro sketch y romper el vínculo */
const SketchModelRefs: Component<{ ui: CadUi; store: CadStore }> = (props) => {
  const ui = props.ui;
  const [busy, setBusy] = createSignal(false);
  const s = () => ui.session()!;
  const linked = () => s().sketch.uses ?? [];
  const selectedLinked = () => ui.selection().filter((id) => linked().some((u) => u.entity === id)).length;
  // Sketches de antes en el historial (los de después no están calculados todavía)
  const earlier = () => {
    const fs = props.store.doc()?.features ?? [];
    const at = fs.findIndex((f) => f.id === s().feature);
    return fs.slice(0, Math.max(0, at)).filter((f) => f.kind.type === "sketch" && !f.suppressed);
  };
  const run = async (job: () => Promise<string>) => {
    setBusy(true);
    try {
      ui.setMessage(await job());
    } finally {
      setBusy(false);
    }
  };
  return (
    <Section title="Usar del modelo">
      <p class="text-[11px] text-text-dim leading-relaxed">
        Con Usar (J), clic en una arista o en una cara del sólido (la cara trae todo su contorno). Lo usado queda ligado: si el modelo cambia, se mueve con él.
      </p>
      <div class="flex flex-wrap gap-1.5">
        <Button size="sm" disabled={busy()} title="El contorno del sólido visto desde la normal del plano, con los agujeros pasantes" onClick={() => void run(() => ui.useModel("silhouette"))}>
          Silueta
        </Button>
        <Button size="sm" disabled={busy()} title="Las curvas donde el plano del sketch corta el sólido" onClick={() => void run(() => ui.useModel("section"))}>
          Intersección
        </Button>
      </div>
      <Show when={earlier().length > 0}>
        <span class="text-xs text-text-muted">De otro sketch</span>
        <div class="flex flex-wrap gap-1.5">
          <For each={earlier()}>
            {(f) => (
              <Button size="sm" variant="ghost" disabled={busy()} title="Todas sus curvas y puntos, proyectados a este plano" onClick={() => void run(() => ui.useSketch(f.id))}>
                {f.name}
              </Button>
            )}
          </For>
        </div>
      </Show>
      <div class="flex items-center justify-between gap-2">
        <span class="text-xs text-text-muted">{linked().length === 1 ? "1 curva ligada" : `${linked().length} curvas ligadas`}</span>
        <Button size="sm" variant="ghost" disabled={selectedLinked() === 0} title="Lo elegido deja de seguir al modelo y queda editable" onClick={() => ui.setMessage(ui.breakLinks())}>
          Romper vínculo
        </Button>
      </div>
    </Section>
  );
};

/** Entidades del sketch: elegir por tipo, lista y propiedades de lo elegido */
const SketchEntities: Component<{ ui: CadUi }> = (props) => {
  const ui = props.ui;
  const sketch = (): Sketch => ui.session()!.sketch;
  const pos = (id: number): P2 => {
    const p = sketch().points.find((q) => q.id === id);
    return p ? [p.x, p.y] : [0, 0];
  };
  const fmtN = (v: number) => String(+v.toFixed(3));
  /** Una fila por entidad (un texto es una sola fila) */
  const rows = createMemo(() => {
    const s = sketch();
    const inText = new Map<number, number>();
    for (const t of s.texts ?? []) for (const e of t.entities) inText.set(e, t.id);
    const out: { key: string; label: string; measure: string; ids: number[]; construction: boolean }[] = [];
    for (const t of s.texts ?? []) out.push({ key: `t${t.id}`, label: `Texto «${t.text}»`, measure: `${fmtN(t.size)} mm`, ids: [...t.entities], construction: false });
    for (const e of s.entities) {
      if (inText.has(e.id)) continue;
      const g = e.geometry;
      let measure = "";
      if (g.type === "line") measure = fmtN(Math.hypot(pos(g.end)[0] - pos(g.start)[0], pos(g.end)[1] - pos(g.start)[1]));
      else if (g.type === "circle") measure = `Ø ${fmtN(2 * g.radius)}`;
      else if (g.type === "arc") measure = `R ${fmtN(Math.hypot(pos(g.start)[0] - pos(g.center)[0], pos(g.start)[1] - pos(g.center)[1]))}`;
      else if (g.type === "point") measure = pos(g.point).map(fmtN).join(", ");
      out.push({ key: `e${e.id}`, label: ENTITY_NAMES[g.type], measure, ids: [g.type === "point" ? g.point : e.id], construction: !!e.construction });
    }
    return out;
  });
  const MAX_ROWS = 300;
  const sel = () => ui.selection();
  const selected = (ids: number[]) => ids.every((id) => sel().includes(id));
  /** Lo elegido, si es una sola entidad (o un solo punto) */
  const single = () => {
    const ids = sel();
    if (ids.length !== 1) return undefined;
    const e = sketch().entities.find((x) => x.id === ids[0]);
    if (e) return e.geometry.type === "point" ? ({ kind: "point", point: e.geometry.point } as const) : ({ kind: "entity", e } as const);
    return sketch().points.some((p) => p.id === ids[0]) ? ({ kind: "point", point: ids[0] } as const) : undefined;
  };
  const PointFields = (p: { point: number; label?: string }) => (
    <div class="flex gap-1.5">
      <Num label={`${p.label ?? ""}X`} suffix="mm" step={1} value={pos(p.point)[0]} onCommit={(v) => ui.movePoint(p.point, [v, pos(p.point)[1]])} />
      <Num label={`${p.label ?? ""}Y`} suffix="mm" step={1} value={pos(p.point)[1]} onCommit={(v) => ui.movePoint(p.point, [pos(p.point)[0], v])} />
    </div>
  );
  return (
    <Section title={`Entidades (${rows().length})`}>
      <div class="flex flex-wrap gap-1" aria-label="Elegir por tipo">
        <For each={SELECT_KINDS}>
          {(k) => (
            <Button size="sm" variant="ghost" onClick={() => ui.setSelection(selectByKind(sketch(), k.kind, ui.session()?.report?.free_entities ?? []))}>
              {k.label}
            </Button>
          )}
        </For>
      </div>
      <p class="text-[11px] text-text-dim">Doble clic en el visor elige la cadena unida; Ctrl+A, todo.</p>
      <div class="max-h-48 overflow-y-auto rounded border border-border" aria-label="Lista de entidades">
        <For each={rows().slice(0, MAX_ROWS)}>
          {(r) => (
            <button
              class={clsx(
                "w-full flex items-center justify-between gap-2 px-2 py-0.5 text-[11px] text-left",
                selected(r.ids) ? "bg-accent/20 text-text" : "text-text-muted hover:bg-surface hover:text-text",
              )}
              onClick={(e) => ui.setSelection((cur) => (e.shiftKey ? (selected(r.ids) ? cur.filter((x) => !r.ids.includes(x)) : [...cur, ...r.ids]) : r.ids))}
              onMouseEnter={() => ui.setHoverIds(r.ids)}
              onMouseLeave={() => ui.setHoverIds([])}
            >
              <span class={r.construction ? "italic text-text-dim" : undefined}>
                {r.label}
                {r.construction ? " (constr.)" : ""}
              </span>
              <span class="font-mono text-text-dim">{r.measure}</span>
            </button>
          )}
        </For>
        <Show when={rows().length > MAX_ROWS}>
          <p class="px-2 py-0.5 text-[11px] text-text-dim">… y {rows().length - MAX_ROWS} más</p>
        </Show>
      </div>
      {/* Propiedades: escribir un valor mueve la geometría (las restricciones mandan) */}
      <Show when={single()}>
        {(it) => (
          <div class="space-y-1.5 border-t border-border pt-1.5" aria-label="Propiedades">
            <Switch>
              <Match when={it().kind === "point" && it()}>{(p) => <PointFields point={(p() as { point: number }).point} />}</Match>
              <Match when={it().kind === "entity" && (it() as { e: SketchEntity }).e}>
                {(e) => {
                  const g = () => e().geometry;
                  return (
                    <Switch>
                      <Match when={g().type === "line" && (g() as Extract<Geometry, { type: "line" }>)}>
                        {(l) => {
                          const len = () => Math.hypot(pos(l().end)[0] - pos(l().start)[0], pos(l().end)[1] - pos(l().start)[1]);
                          return (
                            <>
                              <Num
                                label="Largo"
                                suffix="mm"
                                step={1}
                                value={len()}
                                onCommit={(v) => {
                                  const [a, b] = [pos(l().start), pos(l().end)];
                                  const k = len() > 0 ? v / len() : 0;
                                  if (v > 0) ui.movePoint(l().end, [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k]);
                                }}
                              />
                              <PointFields point={l().start} label="Inicio " />
                              <PointFields point={l().end} label="Fin " />
                            </>
                          );
                        }}
                      </Match>
                      <Match when={g().type === "circle" && (g() as Extract<Geometry, { type: "circle" }>)}>
                        {(c) => (
                          <>
                            <PointFields point={c().center} label="Centro " />
                            <Num label="Diámetro" suffix="mm" step={1} value={2 * c().radius} onCommit={(v) => ui.setCircleRadius(e().id, v / 2)} />
                          </>
                        )}
                      </Match>
                      <Match when={g().type === "arc" && (g() as Extract<Geometry, { type: "arc" }>)}>
                        {(a) => {
                          const r = () => Math.hypot(pos(a().start)[0] - pos(a().center)[0], pos(a().start)[1] - pos(a().center)[1]);
                          return (
                            <>
                              <PointFields point={a().center} label="Centro " />
                              <Num
                                label="Radio"
                                suffix="mm"
                                step={1}
                                value={r()}
                                onCommit={(v) => {
                                  const [c, st] = [pos(a().center), pos(a().start)];
                                  const k = r() > 0 ? v / r() : 0;
                                  if (v > 0) ui.movePoint(a().start, [c[0] + (st[0] - c[0]) * k, c[1] + (st[1] - c[1]) * k]);
                                }}
                              />
                            </>
                          );
                        }}
                      </Match>
                    </Switch>
                  );
                }}
              </Match>
            </Switch>
          </div>
        )}
      </Show>
    </Section>
  );
};

/**
 * Herramientas de splines del panel: manijas y quitar puntos (punto de una
 * spline elegido), ajuste a puntos, simplificar, convertir a polos y el peine
 * de curvatura.
 */
const SplineTools: Component<{ ui: CadUi }> = (props) => {
  const ui = props.ui;
  const [tol, setTol] = createSignal(0.05);
  const sketch = () => ui.session()!.sketch;
  const sel = () => ui.selection();
  const ents = () => sel().map((id) => sketch().entities.find((e) => e.id === id)).filter((e) => e !== undefined);
  const pts = () => sel().filter((id) => sketch().points.some((p) => p.id === id));
  const splines = () => ents().filter((e) => e.geometry.type === "spline" || e.geometry.type === "bspline");
  /** Spline (por puntos o por polos) a la que pertenece un punto elegido */
  const owner = () => {
    if (pts().length !== 1 || ents().length) return undefined;
    const p = pts()[0];
    const e = sketch().entities.find((x) => (x.geometry.type === "spline" && x.geometry.points.includes(p)) || (x.geometry.type === "bspline" && x.geometry.poles.includes(p)));
    return e && { entity: e.id, point: p, kind: e.geometry.type };
  };
  const convertible = () => ents().length > 0 && ents().every((e) => ["line", "arc", "spline"].includes(e.geometry.type)) && !ents().every((e) => e.geometry.type === "line");
  const run = (f: (sk: Sketch) => string | number | undefined, ok?: (r: string | number | undefined) => string | undefined) => {
    let r: string | number | undefined;
    ui.change((sk) => void (r = f(sk)));
    ui.setMessage(typeof r === "string" ? r : ok?.(r));
  };
  const comb = () => {
    const id = ui.combEntity();
    return id !== undefined ? splineCurvature(sketch(), id, 1) : undefined;
  };
  return (
    <Show when={owner() || splines().length || pts().length >= 3 || convertible() || ui.combEntity() !== undefined}>
      <div class="space-y-1.5 border-t border-border pt-1.5" data-spline-tools>
        <div class="text-[11px] text-text-dim">Splines</div>
        <div class="flex flex-wrap gap-1.5">
          <Show when={owner()}>
            {(o) => (
              <>
                <Show when={o().kind === "spline"}>
                  <Button size="sm" title="Un punto que da la dirección de la curva en ese punto (se arrastra)" onClick={() => run((sk) => toggleSplineHandle(sk, o().entity, o().point))}>
                    Manija en el punto sí/no
                  </Button>
                </Show>
                <Button size="sm" onClick={() => run((sk) => removeSplinePoint(sk, o().entity, o().point), () => void ui.setSelection([]))}>
                  {o().kind === "spline" ? "Quitar el punto de la spline" : "Quitar el polo"}
                </Button>
              </>
            )}
          </Show>
          <Show when={pts().length >= 3 && !ents().length}>
            <Button
              size="sm"
              title="Spline por polos que pasa por el primero y el último y se acerca a los demás (en el orden en que se eligieron)"
              onClick={() =>
                run(
                  (sk) => fitSplineToPoints(sk, pts(), tol()),
                  (id) => (typeof id === "number" ? void ui.setSelection([id]) : undefined),
                )
              }
            >
              Spline de ajuste
            </Button>
          </Show>
          <Show when={splines().length === 1}>
            <Button
              size="sm"
              title="Saca puntos (o polos) mientras la curva no se aparte más que la tolerancia"
              onClick={() => run((sk) => simplifySpline(sk, splines()[0].id, tol()), (n) => (typeof n === "number" ? (n ? `Se sacaron ${n}` : "No hay nada que sacar con esa tolerancia") : undefined))}
            >
              Simplificar
            </Button>
            <Button size="sm" aria-pressed={ui.combEntity() === splines()[0].id} onClick={() => ui.setCombEntity(ui.combEntity() === splines()[0].id ? undefined : splines()[0].id)}>
              Peine de curvatura
            </Button>
          </Show>
          <Show when={convertible()}>
            <Button
              size="sm"
              title="Líneas, arcos y splines por puntos unidos por los extremos pasan a una spline por polos (los arcos, con un error de ~0,03 % del radio)"
              onClick={() =>
                run(
                  (sk) => convertToBSpline(sk, ents().map((e) => e.id)),
                  (id) => (typeof id === "number" ? void ui.setSelection([id]) : undefined),
                )
              }
            >
              Convertir a spline por polos
            </Button>
          </Show>
        </div>
        <label class="flex items-center gap-1 text-[11px] text-text-muted">
          Tolerancia
          <input
            type="number"
            min="0.0001"
            step="0.01"
            value={tol()}
            aria-label="Tolerancia de las splines"
            class="w-16 px-1 py-0.5 rounded bg-surface/40 border border-border text-xs text-text font-mono outline-none focus:border-accent"
            onChange={(e) => setTol(Math.max(1e-4, parseFloat(e.currentTarget.value) || 0.05))}
          />
          mm
        </label>
        <Show when={comb()}>
          {(c) => (
            <p class="text-[11px] text-text-muted" data-curvature>
              Radio mínimo {Number.isFinite(c().minRadius) ? `${+c().minRadius.toFixed(3)} mm` : "— (recta)"} · {c().inflections.length} inflexiones
              <button class="ml-1 underline hover:text-text" onClick={() => ui.setCombEntity(undefined)}>
                ocultar
              </button>
            </p>
          )}
        </Show>
      </div>
    </Show>
  );
};
