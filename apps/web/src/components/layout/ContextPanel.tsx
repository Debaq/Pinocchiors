import { outlinerQuery, setOutlinerQuery } from "../../lib/outlinerFilter";
import { Component, Switch, Match, Show, For, createEffect, createSignal, on, onCleanup, onMount, type JSX } from "solid-js";
import { clsx } from "clsx";
import { stepInfo, workspaceOf, type PipelineStepId } from "../../lib/pipeline";
import { createPersisted, startDrag } from "../../lib/ui-state";
import { Panel, Tooltip } from "../ui";
import * as Icons from "../icons";
import type { ViewSettings } from "../panels/ViewPanel";
import type { SceneNode } from "../../lib/scene-tree";
import { ViewPanel } from "../panels/ViewPanel";
import { LightsPanel } from "../panels/LightsPanel";
import type { LightSettings } from "../../lib/Viewer3D";
import { SceneOutliner } from "../panels/SceneOutliner";
import { HistoryPanel } from "../panels/HistoryPanel";
import type { HistoryStore } from "../../lib/history";
import { ObjectTab, type ObjectTabProps } from "../panels/ObjectTab";
import { StructureStep, type StructureStepProps } from "../steps/StructureStep";
import { RemeshStep, type RemeshStepProps } from "../steps/RemeshStep";
import { SkeletonStep, type SkeletonStepProps } from "../steps/SkeletonStep";
import { UvStep, type UvStepProps } from "../steps/UvStep";
import { ExportStep, type ExportStepProps } from "../steps/ExportStep";
import { RenderStep, type RenderStepProps } from "../steps/RenderStep";
import { RepairStep, type RepairStepProps } from "../steps/RepairStep";
import { AnimateStep, type AnimateStepProps } from "../steps/AnimateStep";
import { Print3DStep, type Print3DStepProps } from "../steps/Print3DStep";
import { MaterialsStep, type MaterialsStepProps } from "../steps/MaterialsStep";
import { ScanStep, type ScanStepProps } from "../steps/ScanStep";
import { fmtNumber, t } from "../../lib/i18n";

export interface ContextPanelProps {
  activeStep: PipelineStepId;
  /** Se eligió una sección del espacio activo (pestaña vertical) */
  onSection?: (step: PipelineStepId) => void;

  /** Pestaña Objeto: el modelo y sus transformaciones */
  objectProps: ObjectTabProps;

  // Estructura del archivo de origen
  structureProps: StructureStepProps;

  // Materiales y texturas
  materialsProps: MaterialsStepProps;

  // Repair
  repairProps: RepairStepProps;

  // Remallar (la retopología es uno de sus modos)
  remeshProps: RemeshStepProps;

  // UV / Piel
  uvProps: UvStepProps;

  // Skeleton
  skeletonProps: SkeletonStepProps;

  // Print3D
  print3dProps: Print3DStepProps;

  // Escáner 3D (Orizon3D)
  scanProps: ScanStepProps;
  /** Panel del espacio Diseñar por pestaña (lo arma App con el store del CAD) */
  designPanel?: (section: () => "design" | "design_part" | "design_inspect" | "design_scan") => JSX.Element;

  // Export
  exportProps: ExportStepProps;
  renderProps: RenderStepProps;
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
  onSelectNode?: (nodeId: string, add?: boolean) => void;
  onDeleteNode?: (nodeId: string) => void;
  onHoverNode?: (nodeId: string | null) => void;
  onAddNode?: (groupId: string) => void;

  // Pestaña de información
  stats?: SceneStats;
  shortcuts?: ShortcutHint[];

  /** Historial en árbol (pestaña Historial) */
  history?: HistoryStore;

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

/** "section" = la sección activa del espacio de trabajo; el resto, pestañas fijas */
type TabId = "section" | "object" | "view" | "lights" | "history" | "info";

/** Gestos del visor que no pasan por el gestor de atajos */
export const MOUSE_HINTS: ShortcutHint[] = [
  { get keys() { return t("Central / Alt+Izq"); }, description: "Orbitar" },
  { get keys() { return t("Shift+Central"); }, description: "Desplazar" },
  { get keys() { return t("Rueda / Ctrl+Central"); }, description: "Zoom" },
  { get keys() { return t("Clic"); }, description: "Seleccionar articulación" },
  { get keys() { return t("L+arrastrar"); }, description: "Girar la luz" },
];

const FIXED_TABS: { id: Exclude<TabId, "section">; label: string; icon: Component<{ size?: number }> }[] = [
  { id: "object", label: "Objeto", icon: Icons.Cube },
  { id: "view", label: "Visualización", icon: Icons.Eye },
  { id: "lights", label: "Luces", icon: Icons.Lightning },
  { id: "history", label: "Historial", icon: Icons.Clock },
  { id: "info", label: "Escena y atajos", icon: Icons.Info },
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
  // Alto del Outliner como parte del panel: al empezar 3 de 5 (3:2 con las propiedades)
  const [outlinerRatio, setOutlinerRatio] = createPersisted("dock.outliner.ratio", 0.6);
  const [dockHeight, setDockHeight] = createSignal(600);
  /** Alto que queda para el Outliner y las propiedades (sin los encabezados) */
  const free = () => Math.max(200, dockHeight() - 8 - 32 - 4);
  const outlinerHeight = () => Math.max(MIN_OUTLINER, Math.min(free() - 160, Math.round(free() * outlinerRatio())));
  const [storedTab, setTab] = createPersisted<string>("dock.tab", "section");
  const tab = (): TabId => (FIXED_TABS.some((x) => x.id === storedTab()) ? (storedTab() as TabId) : "section");

  // Al cambiar de sección (o de espacio), mostrar sus herramientas
  createEffect(on(() => props.activeStep, () => setTab("section"), { defer: true }));

  // Cada pestaña se abre desde arriba
  let scrollRef: HTMLDivElement | undefined;
  createEffect(on([tab, () => props.activeStep], () => scrollRef?.scrollTo({ top: 0 }), { defer: true }));

  /** Secciones del espacio activo (al exportar, solo exportar) */
  const sections = () => workspaceOf(props.activeStep)?.sections ?? [props.activeStep];
  const tabTitle = () =>
    tab() === "section" ? t(stepInfo(props.activeStep).label) : t(FIXED_TABS.find((x) => x.id === tab())!.label);

  const TabButton = (p: { label: string; active: boolean; onClick: () => void; children: JSX.Element }) => (
    <Tooltip content={p.label} placement="left">
      <button
        class={clsx(
          "w-7 h-7 flex items-center justify-center rounded transition-colors",
          p.active ? "bg-accent/20 text-accent" : "text-text-muted hover:text-text hover:bg-surface/50"
        )}
        onClick={() => p.onClick()}
        aria-label={p.label}
        aria-pressed={p.active}
      >
        {p.children}
      </button>
    </Tooltip>
  );

  const resizeWidth = (e: PointerEvent) => {
    const start = width();
    startDrag(e, "col-resize", (dx) => setWidth(Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, start - dx))));
  };

  let dockRef: HTMLElement | undefined;
  onMount(() => {
    if (!dockRef) return;
    const ro = new ResizeObserver(() => setDockHeight(dockRef!.clientHeight));
    ro.observe(dockRef);
    setDockHeight(dockRef.clientHeight);
    onCleanup(() => ro.disconnect());
  });
  const resizeOutliner = (e: PointerEvent) => {
    const start = outlinerHeight();
    const max = free() - 160;
    startDrag(e, "row-resize", (_, dy) => setOutlinerRatio(Math.min(max, Math.max(MIN_OUTLINER, start + dy)) / free()));
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
          title={t("Outliner")}
          icon={<Icons.TreeStructure size={13} />}
          open={outlinerOpen()}
          onToggle={() => setOutlinerOpen(!outlinerOpen())}
        >
          {/* Buscador: filtra por nombre (objetos, planos, operaciones, huesos…) */}
          <div class="relative w-32 shrink min-w-0">
            <Icons.MagnifyingGlass size={11} class="absolute left-1.5 top-1/2 -translate-y-1/2 text-text-dim pointer-events-none" />
            <input
              type="search"
              aria-label={t("Buscar en el Outliner")}
              placeholder={t("Buscar")}
              value={outlinerQuery()}
              class="w-full h-5 pl-5 pr-1 rounded bg-surface/40 border border-border text-[11px] text-text outline-none focus:border-accent placeholder:text-text-dim"
              onInput={(e) => {
                setOutlinerQuery(e.currentTarget.value);
                if (!outlinerOpen()) setOutlinerOpen(true);
              }}
              onKeyDown={(e) => {
                e.stopPropagation();
                if (e.key === "Escape") {
                  setOutlinerQuery("");
                  e.currentTarget.blur();
                }
              }}
            />
          </div>
          <Tooltip content={t("Ocultar panel (N)")} placement="left">
            <button
              class="w-5 h-5 flex items-center justify-center rounded text-text-muted hover:text-text hover:bg-surface/50"
              onClick={() => props.onHide?.()}
              aria-label={t("Ocultar panel")}
            >
              <Icons.CaretRight size={12} />
            </button>
          </Tooltip>
        </AreaHeader>
        <Show when={outlinerOpen()}>
          <div class="overflow-y-auto px-1.5 py-1" style={{ height: `${outlinerHeight()}px` }}>
            <Show
              when={props.sceneTree}
              fallback={<p class="px-2 py-1 text-xs text-text-dim">{t("Sin escena: importa un modelo.")}</p>}
            >
              <SceneOutliner
                tree={props.sceneTree!}
                onToggleVisibility={props.onToggleVisibility}
                onSelectNode={props.onSelectNode}
                onDeleteNode={props.onDeleteNode}
                onHoverNode={props.onHoverNode}
                onAddNode={props.onAddNode}
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
          {/* Secciones del espacio de trabajo */}
          <For each={sections()}>
            {(id) => {
              const info = stepInfo(id);
              return (
                <TabButton
                  label={t(info.label)}
                  active={tab() === "section" && props.activeStep === id}
                  onClick={() => {
                    setTab("section");
                    props.onSection?.(id);
                  }}
                >
                  <info.icon size={16} />
                </TabButton>
              );
            }}
          </For>
          <div class="w-5 h-px my-1 bg-border" />
          {/* Fijas: en cualquier espacio */}
          <For each={FIXED_TABS}>
            {(ft) => (
              <TabButton label={t(ft.label)} active={tab() === ft.id} onClick={() => setTab(ft.id)}>
                <ft.icon size={16} />
              </TabButton>
            )}
          </For>
        </nav>

        <div class="flex flex-col flex-1 min-w-0 bg-bg">
          <div class="h-8 shrink-0 flex items-center px-3 border-b border-border text-xs font-semibold text-text truncate">
            {tabTitle()}
          </div>
          <div ref={scrollRef} class="flex-1 min-h-0 overflow-y-auto px-4 py-4">
            <Switch>
              <Match when={tab() === "section"}>
                <StepContent {...props} />
              </Match>
              <Match when={tab() === "object"}>
                <ObjectTab {...props.objectProps} />
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
              <Match when={tab() === "history" && props.history}>
                {(history) => (
                  <HistoryPanel
                    nodes={history().nodes()}
                    current={history().current()}
                    canReach={history().canReach}
                    onGoTo={(id) => void history().goTo(id)}
                  />
                )}
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
    <Match when={props.activeStep === "structure"}>
      <StructureStep {...props.structureProps} />
    </Match>
    <Match when={props.activeStep === "materials"}>
      <MaterialsStep {...props.materialsProps} />
    </Match>
    <Match when={props.activeStep === "repair"}>
      <RepairStep {...props.repairProps} />
    </Match>
    <Match when={props.activeStep === "remesh"}>
      <RemeshStep {...props.remeshProps} />
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
    <Match when={props.activeStep === "scan"}>
      <ScanStep {...props.scanProps} />
    </Match>
    <Match
      when={
        (["design", "design_part", "design_inspect", "design_scan"] as const).find((d) => d === props.activeStep)
      }
    >
      {(section) => props.designPanel?.(section)}
    </Match>
    <Match when={props.activeStep === "render"}>
      <RenderStep {...props.renderProps} />
    </Match>
    <Match when={props.activeStep === "export"}>
      <ExportStep {...props.exportProps} />
    </Match>
  </Switch>
);

const StatRow: Component<{ label: string; value?: string | number }> = (props) => (
  <Show when={props.value !== undefined && props.value !== ""}>
    <div class="flex justify-between gap-3 text-xs">
      <span class="text-text-muted">{t(props.label)}</span>
      <span class="text-text font-mono truncate">
        {typeof props.value === "number" ? fmtNumber(props.value) : props.value}
      </span>
    </div>
  </Show>
);

const HintList: Component<{ hints: ShortcutHint[] }> = (props) => (
  <div class="space-y-1">
    <For each={props.hints}>
      {(h) => (
        <div class="flex items-center justify-between gap-3 text-xs">
          <span class="text-text-muted truncate">{t(h.description)}</span>
          <kbd class="shrink-0 px-1.5 rounded bg-surface/60 border border-border font-mono text-[11px] text-text">
            {h.keys}
          </kbd>
        </div>
      )}
    </For>
  </div>
);

const InfoTab: Component<{ stats?: SceneStats; shortcuts?: ShortcutHint[] }> = (props) => (
  <div>
    <Panel id="info.stats" title={t("Estadísticas")} icon={<Icons.Ruler size={14} />} defaultOpen>
      <Show
        when={props.stats?.vertices}
        fallback={<p class="text-xs text-text-dim">{t("Sin modelo cargado.")}</p>}
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
    <Panel id="info.keys" title={t("Atajos de teclado")} icon={<Icons.Cursor size={14} />} defaultOpen>
      <HintList hints={props.shortcuts ?? []} />
    </Panel>
    <Panel id="info.mouse" title={t("Mouse en el visor")} icon={<Icons.CubeFocus size={14} />} defaultOpen={false}>
      <HintList hints={MOUSE_HINTS} />
    </Panel>
  </div>
);
