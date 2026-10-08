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
  designMass,
  partColor,
  partHidden,
  partMass,
  partMaterial,
  samePart,
  OP_LABELS,
  PLANE_LABELS,
  constraintIds,
  constraintValue,
  isReference,
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
  type Configuration,
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
  type PartView,
  type PatternKind,
  type PrimitiveShape,
  type RegionSelection,
  type PlaneSpec,
  type ScanPick,
  type Sketch,
  type SketchConstraint,
} from "../../lib/cad";
import type { CadUi } from "../../lib/cadUi";
import type { DesignActions } from "../../lib/designActions";
import { regionContains } from "../../lib/CadViewer";
import { Button, Checkbox, IconButton, Select, Slider, Tooltip } from "../ui";
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
                </Show>
              }>
                <SketchPanel ui={ui} />
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
                      {field("Cantidad", "kind.pattern.count", (k().pattern as { count: number }).count, (x, v) => x.type === "pattern" && (x.pattern.count = Math.max(2, Math.round(v))))}
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
                  <Row label="Tamaño">
                    <Select
                      options={[{ value: "", label: "A medida" }, ...METRIC_HOLES.map((m) => ({ value: m.size, label: m.size }))]}
                      value={fit()?.size ?? ""}
                      onChange={(v) => v && applySize(v, fit()?.mode ?? "clearance")}
                    />
                  </Row>
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
                        {field("Paso", "kind.modeled.pitch", t().pitch, (x, v) => x.type === "hole" && x.modeled && (x.modeled.pitch = v), "mm")}
                        {field("Holgura", "kind.modeled.clearance", t().clearance, (x, v) => x.type === "hole" && x.modeled && (x.modeled.clearance = v), "mm")}
                        <Checkbox small label="A izquierdas" checked={t().left} onChange={(c) => update((x) => x.type === "hole" && !!x.modeled && (x.modeled.left = c))} />
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
                <Row label="Medida">
                  <Select
                    options={[{ value: "", label: "Paso a medida" }, ...METRIC_HOLES.map((m) => ({ value: String(m.pitch), label: `${m.size} × ${String(m.pitch).replace(".", ",")}` }))]}
                    value={METRIC_HOLES.some((m) => m.pitch === k().pitch) ? String(k().pitch) : ""}
                    onChange={(v) => v && update((x) => x.type === "thread" && (x.pitch = +v))}
                  />
                </Row>
                {field("Paso", "kind.pitch", k().pitch, (x, v) => x.type === "thread" && (x.pitch = v), "mm")}
                {field("Largo", "kind.length", k().length, (x, v) => x.type === "thread" && (x.length = v), "mm")}
                {field("Holgura", "kind.clearance", k().clearance, (x, v) => x.type === "thread" && (x.clearance = v), "mm")}
                <Checkbox small label="Desde el otro extremo" checked={k().flip} onChange={(c) => update((x) => x.type === "thread" && (x.flip = c))} />
                <Checkbox small label="A izquierdas" checked={k().left} onChange={(c) => update((x) => x.type === "thread" && (x.left = c))} />
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
        {/* Unir, restar o intersecar: con qué piezas (sin elegir, las que toca) */}
        <Show when={"op" in f().kind && f().kind.type !== "boolean" && (f().kind as { op: BodyOp }).op !== "new"}>
          <PartChecklist
            store={props.store}
            featureId={f().id}
            label="Con las piezas"
            empty="Las que toca"
            value={f().scope ?? []}
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
    .filter((r) => r.kind === kind && r.id !== except)
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
  const has = (id: PartId) => props.value.some((x) => samePart(x, id));
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
              onChange={(c) => props.onChange(c ? [...props.value, p.id] : props.value.filter((x) => !samePart(x, p.id)))}
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
                    {partMass(props.store.doc(), p) !== null ? fmtMass(partMass(props.store.doc(), p)!) : `${fmt(p.volume / 1000)} cm³`}
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
                <Line label="Volumen">{fmt(p().volume / 1000)} cm³</Line>
                <Line label="Área">{fmt(p().area / 100)} cm²</Line>
                <Show when={partMass(props.store.doc(), p()) !== null}>
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
