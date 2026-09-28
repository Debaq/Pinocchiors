import { Component, Switch, Match, Show, For, createEffect, on, type JSX } from "solid-js";
import { clsx } from "clsx";
import { PIPELINE_STEPS, type PipelineStepId } from "../../lib/pipeline";
import { createPersisted, startDrag } from "../../lib/ui-state";
import { Panel, Tooltip } from "../ui";
import * as Icons from "../icons";
import type { ViewSettings } from "../panels/ViewPanel";
import type { SceneNode } from "../../lib/scene-tree";
import { ViewPanel } from "../panels/ViewPanel";
import { LightsPanel } from "../panels/LightsPanel";
import type { LightSettings } from "../../lib/Viewer3D";
import { SceneOutliner } from "../panels/SceneOutliner";
import { ImportStep, type ImportStepProps } from "../steps/ImportStep";
import { StructureStep, type StructureStepProps } from "../steps/StructureStep";
import { RetopologyStep, type RetopologyStepProps } from "../steps/RetopologyStep";
import { SkeletonStep, type SkeletonStepProps } from "../steps/SkeletonStep";
import { UvStep, type UvStepProps } from "../steps/UvStep";
import { ExportStep, type ExportStepProps } from "../steps/ExportStep";
import { RepairStep, type RepairStepProps } from "../steps/RepairStep";
import { AnimateStep, type AnimateStepProps } from "../steps/AnimateStep";
import { Print3DStep, type Print3DStepProps } from "../steps/Print3DStep";

export interface ContextPanelProps {
  activeStep: PipelineStepId;

  // Import
  importProps: ImportStepProps;

  // Estructura del archivo de origen
  structureProps: StructureStepProps;

  // Repair
  repairProps: RepairStepProps;

  // Retopology
  retopologyProps: RetopologyStepProps;

  // UV / Piel
  uvProps: UvStepProps;

  // Skeleton
  skeletonProps: SkeletonStepProps;

  // Print3D
  print3dProps: Print3DStepProps;

  // Export
  exportProps: ExportStepProps;
  animateProps: AnimateStepProps;

  // View
  viewSettings: ViewSettings;
  onViewSettingsChange?: (settings: ViewSettings) => void;
  boneNames?: string[];
  hasWeights?: boolean;

  // Luces
  lights: LightSettings;
  onLightsChange?: (lights: LightSettings) => void;

  // Outliner
  sceneTree?: SceneNode;
  onToggleVisibility?: (nodeId: string) => void;
  onSelectNode?: (nodeId: string) => void;

  // Pestaña de información
  stats?: SceneStats;
  shortcuts?: ShortcutHint[];

  /** Oculta el panel entero (como N en Blender) */
  onHide?: () => void;
}

export interface SceneStats {
  fileName?: string;
  format?: string;
  vertices?: number;
  faces?: number;
  quads?: number;
  bones?: number;
}

export interface ShortcutHint {
  keys: string;
  description: string;
}

type TabId = "tool" | "view" | "lights" | "info";

/** Gestos del visor que no pasan por el gestor de atajos */
const MOUSE_HINTS: ShortcutHint[] = [
  { keys: "Central / Alt+Izq", description: "Orbitar" },
  { keys: "Shift+Central", description: "Desplazar" },
  { keys: "Rueda / Ctrl+Central", description: "Zoom" },
  { keys: "Clic", description: "Seleccionar articulación" },
  { keys: "L+arrastrar", description: "Girar la luz" },
];

const MIN_WIDTH = 260;
const MAX_WIDTH = 640;
const MIN_OUTLINER = 60;

/**
 * Panel derecho al estilo Blender: arriba el Outliner (plegable y con alto
 * ajustable), abajo el editor de propiedades con pestañas verticales. El
 * ancho se arrastra desde el borde izquierdo y todo se recuerda entre sesiones.
 */
export const ContextPanel: Component<ContextPanelProps> = (props) => {
  const [width, setWidth] = createPersisted("dock.width", 320);
  const [outlinerOpen, setOutlinerOpen] = createPersisted("dock.outliner.open", true);
  const [outlinerHeight, setOutlinerHeight] = createPersisted("dock.outliner.height", 180);
  const [tab, setTab] = createPersisted<TabId>("dock.tab", "tool");

  // Al cambiar de paso en el pipeline, mostrar sus herramientas
  createEffect(on(() => props.activeStep, () => setTab("tool"), { defer: true }));

  const step = () => PIPELINE_STEPS.find((s) => s.id === props.activeStep) ?? PIPELINE_STEPS[0];

  const tabs = (): { id: TabId; label: string; icon: JSX.Element }[] => {
    const StepIcon = step().icon;
    return [
      { id: "tool", label: `Herramientas: ${step().label}`, icon: <StepIcon size={16} /> },
      { id: "view", label: "Visualización", icon: <Icons.Eye size={16} /> },
      { id: "lights", label: "Luces", icon: <Icons.Lightning size={16} /> },
      { id: "info", label: "Escena y atajos", icon: <Icons.Info size={16} /> },
    ];
  };
  const tabTitle = () => (tab() === "tool" ? step().label : tabs().find((t) => t.id === tab())?.label);

  const resizeWidth = (e: PointerEvent) => {
    const start = width();
    startDrag(e, "col-resize", (dx) => setWidth(Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, start - dx))));
  };

  let dockRef: HTMLElement | undefined;
  const resizeOutliner = (e: PointerEvent) => {
    const start = outlinerHeight();
    const max = (dockRef?.clientHeight ?? 600) - 160;
    startDrag(e, "row-resize", (_, dy) => setOutlinerHeight(Math.min(max, Math.max(MIN_OUTLINER, start + dy))));
  };

  return (
    <aside
      ref={dockRef}
      class="relative flex flex-col shrink-0 bg-bg-darker border-l border-border overflow-hidden"
      style={{ width: `${width()}px` }}
    >
      {/* Borde para cambiar el ancho */}
      <div
        class="absolute inset-y-0 left-0 w-1 z-10 cursor-col-resize hover:bg-accent/50 transition-colors"
        onPointerDown={resizeWidth}
      />

      {/* ── Outliner ─────────────────────────────────────────────── */}
      <div class="shrink-0 flex flex-col">
        <AreaHeader
          title="Outliner"
          icon={<Icons.TreeStructure size={13} />}
          open={outlinerOpen()}
          onToggle={() => setOutlinerOpen(!outlinerOpen())}
        >
          <Tooltip content="Ocultar panel (N)" placement="left">
            <button
              class="w-5 h-5 flex items-center justify-center rounded text-text-muted hover:text-text hover:bg-current/50"
              onClick={() => props.onHide?.()}
              aria-label="Ocultar panel"
            >
              <Icons.CaretRight size={12} />
            </button>
          </Tooltip>
        </AreaHeader>
        <Show when={outlinerOpen()}>
          <div class="overflow-y-auto px-1.5 py-1" style={{ height: `${outlinerHeight()}px` }}>
            <Show
              when={props.sceneTree}
              fallback={<p class="px-2 py-1 text-xs text-text-dim">Sin escena: importa un modelo.</p>}
            >
              <SceneOutliner
                tree={props.sceneTree!}
                onToggleVisibility={props.onToggleVisibility}
                onSelectNode={props.onSelectNode}
              />
            </Show>
          </div>
          <div
            class="h-1 shrink-0 cursor-row-resize bg-border hover:bg-accent/50 transition-colors"
            onPointerDown={resizeOutliner}
          />
        </Show>
      </div>

      {/* ── Propiedades ──────────────────────────────────────────── */}
      <div class="flex flex-1 min-h-0 border-t border-border">
        {/* Pestañas verticales */}
        <nav class="flex flex-col items-center gap-1 w-9 shrink-0 py-2 bg-bg-darker border-r border-border">
          <For each={tabs()}>
            {(t) => (
              <Tooltip content={t.label} placement="left">
                <button
                  class={clsx(
                    "w-7 h-7 flex items-center justify-center rounded transition-colors",
                    tab() === t.id
                      ? "bg-accent/20 text-accent"
                      : "text-text-muted hover:text-text hover:bg-current/50"
                  )}
                  onClick={() => setTab(t.id)}
                  aria-label={t.label}
                  aria-pressed={tab() === t.id}
                >
                  {t.icon}
                </button>
              </Tooltip>
            )}
          </For>
        </nav>

        <div class="flex flex-col flex-1 min-w-0 bg-bg">
          <div class="h-8 shrink-0 flex items-center px-3 border-b border-border text-xs font-semibold text-text truncate">
            {tabTitle()}
          </div>
          <div class="flex-1 min-h-0 overflow-y-auto px-4 py-4">
            <Switch>
              <Match when={tab() === "tool"}>
                <StepContent {...props} />
              </Match>
              <Match when={tab() === "view"}>
                <ViewPanel
                  settings={props.viewSettings}
                  onChange={props.onViewSettingsChange}
                  boneNames={props.boneNames}
                  hasWeights={props.hasWeights}
                />
              </Match>
              <Match when={tab() === "lights"}>
                <LightsPanel lights={props.lights} onChange={props.onLightsChange} />
              </Match>
              <Match when={tab() === "info"}>
                <InfoTab stats={props.stats} shortcuts={props.shortcuts} />
              </Match>
            </Switch>
          </div>
        </div>
      </div>
    </aside>
  );
};

const AreaHeader: Component<{
  title: string;
  icon: JSX.Element;
  open: boolean;
  onToggle: () => void;
  children?: JSX.Element;
}> = (props) => (
  <div class="h-8 shrink-0 flex items-center gap-1.5 pl-2 pr-1.5 bg-bg-lighter/40 border-b border-border">
    <button
      class="flex flex-1 items-center gap-1.5 min-w-0 h-full text-xs font-semibold uppercase tracking-wider text-text-muted hover:text-text"
      onClick={props.onToggle}
    >
      <Icons.CaretRight size={10} class={clsx("transition-transform duration-100", props.open && "rotate-90")} />
      {props.icon}
      <span class="truncate">{props.title}</span>
    </button>
    {props.children}
  </div>
);

const StepContent: Component<ContextPanelProps> = (props) => (
  <Switch>
    <Match when={props.activeStep === "import"}>
      <ImportStep {...props.importProps} />
    </Match>
    <Match when={props.activeStep === "structure"}>
      <StructureStep {...props.structureProps} />
    </Match>
    <Match when={props.activeStep === "repair"}>
      <RepairStep {...props.repairProps} />
    </Match>
    <Match when={props.activeStep === "retopology"}>
      <RetopologyStep {...props.retopologyProps} />
    </Match>
    <Match when={props.activeStep === "uv"}>
      <UvStep {...props.uvProps} />
    </Match>
    <Match when={props.activeStep === "skeleton"}>
      <SkeletonStep {...props.skeletonProps} />
    </Match>
    <Match when={props.activeStep === "animate"}>
      <AnimateStep {...props.animateProps} />
    </Match>
    <Match when={props.activeStep === "print3d"}>
      <Print3DStep {...props.print3dProps} />
    </Match>
    <Match when={props.activeStep === "export"}>
      <ExportStep {...props.exportProps} />
    </Match>
  </Switch>
);

const StatRow: Component<{ label: string; value?: string | number }> = (props) => (
  <Show when={props.value !== undefined && props.value !== ""}>
    <div class="flex justify-between gap-3 text-xs">
      <span class="text-text-muted">{props.label}</span>
      <span class="text-text font-mono truncate">
        {typeof props.value === "number" ? props.value.toLocaleString() : props.value}
      </span>
    </div>
  </Show>
);

const HintList: Component<{ hints: ShortcutHint[] }> = (props) => (
  <div class="space-y-1">
    <For each={props.hints}>
      {(h) => (
        <div class="flex items-center justify-between gap-3 text-xs">
          <span class="text-text-muted truncate">{h.description}</span>
          <kbd class="shrink-0 px-1.5 rounded bg-current/60 border border-border font-mono text-[11px] text-text">
            {h.keys}
          </kbd>
        </div>
      )}
    </For>
  </div>
);

const InfoTab: Component<{ stats?: SceneStats; shortcuts?: ShortcutHint[] }> = (props) => (
  <div>
    <Panel id="info.stats" title="Estadísticas" icon={<Icons.Ruler size={14} />} defaultOpen>
      <Show
        when={props.stats?.vertices}
        fallback={<p class="text-xs text-text-dim">Sin modelo cargado.</p>}
      >
        <div class="space-y-1">
          <StatRow label="Archivo" value={props.stats!.fileName} />
          <StatRow label="Formato" value={props.stats!.format?.toUpperCase()} />
          <StatRow label="Vértices" value={props.stats!.vertices} />
          <StatRow label="Caras" value={props.stats!.faces} />
          <StatRow label="Quads (retopología)" value={props.stats!.quads || undefined} />
          <StatRow label="Huesos" value={props.stats!.bones || undefined} />
        </div>
      </Show>
    </Panel>
    <Panel id="info.keys" title="Atajos de teclado" icon={<Icons.Cursor size={14} />} defaultOpen>
      <HintList hints={props.shortcuts ?? []} />
    </Panel>
    <Panel id="info.mouse" title="Mouse en el visor" icon={<Icons.CubeFocus size={14} />} defaultOpen={false}>
      <HintList hints={MOUSE_HINTS} />
    </Panel>
  </div>
);
