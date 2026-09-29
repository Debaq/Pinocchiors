import { Component, onMount, onCleanup, Show, createEffect, createSignal } from "solid-js";
import { clsx } from "clsx";
import {
  Viewer3D,
  ViewerSettings,
  MeshData,
  MeshTextures,
  PaintSettings,
  PaintStroke,
  SceneMaterial,
  LightSettings,
  SkeletonData,
  WeightsData,
  PlacementMode,
  PlacementPick,
  ViewerCallbacks,
} from "../../lib/Viewer3D";
import type { FloorCandidate } from "../../lib/placement";
import { ContextMenu, type MenuEntry } from "../ui/ContextMenu";

export interface ViewportProps {
  // Callbacks
  onViewerReady?: (viewer: Viewer3D) => void;
  onFpsUpdate?: (fps: number) => void;
  /** Clic al orientar (plano candidato o punto de la superficie) */
  onPlacementPick?: (pick: PlacementPick) => void;
  onBoneSelected?: (index: number) => void;
  onBoneMoved?: (index: number, position: [number, number, number]) => void;
  /** Modo animación: se confirmó un giro o desplazamiento de la articulación */
  onPoseEdited?: (joint: number) => void;
  /** Fin de un movimiento con G (para deshacer) */
  onBoneMoveCommitted?: (index: number, from: [number, number, number], to: [number, number, number]) => void;
  /** Se soltó el gizmo del modelo (herramientas Mover, Rotar y Escalar del modelo) */
  onObjectTransformed?: ViewerCallbacks["onObjectTransformed"];
  /** Se soltó el gizmo del esqueleto entero (herramienta Escalar, o Rotar sin pesos) */
  onSkeletonTransformed?: ViewerCallbacks["onSkeletonTransformed"];
  /** Radio o intensidad del pincel cambiados con F / Shift+F */
  onPaintSettingsChanged?: (change: Partial<PaintSettings>) => void;
  /** La luz principal se movió arrastrando con L */
  onLightsChanged?: (lights: LightSettings) => void;

  /** Materiales del archivo de origen */
  sceneMaterials?: SceneMaterial[];
  /** Luces del visor */
  lights?: LightSettings;
  onWeightsPainted?: (stroke: PaintStroke) => void;

  // Data
  meshData?: MeshData;
  /** Texturas de la piel de `meshData` (vista previa del paso UV) */
  textures?: MeshTextures;
  skeletonData?: SkeletonData;
  weightsData?: WeightsData;

  // Settings
  settings?: ViewerSettings;

  // Stats
  vertices?: number;
  faces?: number;
  showStats?: boolean;

  /** Orientación en curso: qué se elige con el clic */
  placementMode?: PlacementMode;
  /** Planos candidatos a piso (modo "floor") */
  floorCandidates?: FloorCandidate[];

  // Bone editing
  boneEditMode?: boolean;

  // Active tool
  activeTool?: string;
  /** Las herramientas actúan sobre el modelo entero en vez del esqueleto */
  objectTools?: boolean;
  /** Ayuda de la herramienta activa (cuando no hay otra operación en curso) */
  toolHint?: string;

  /** Pincel de pesos activo (`undefined` = apagado) */
  paintSettings?: PaintSettings;

  /** Ítems del menú del clic derecho que agrega la app (después de los de vista) */
  contextMenuItems?: () => MenuEntry[];
}

const PLACEMENT_HINTS: Record<PlacementMode, string> = {
  floor: "Clic en un plano: pasa a ser el piso (verde = estable) · Shift+clic: la superficie bajo el cursor",
  front: "Clic en la cara que debe mirar al frente (hacia −Y, la vista frontal)",
  point: "Clic en el punto que será el origen",
};

export const Viewport: Component<ViewportProps> = (props) => {
  let canvasRef: HTMLCanvasElement | undefined;
  let viewer: Viewer3D | undefined;
  /** Ayuda de la operación modal en curso (G, R, F…) */
  const [hint, setHint] = createSignal<string | null>(null);
  /** Resultado de la herramienta Medir */
  const [measure, setMeasure] = createSignal<string | null>(null);
  /** Tamaño de celda de la grilla */
  const [gridLabel, setGridLabel] = createSignal<string | null>(null);
  /** Menú del clic derecho abierto, con sus ítems */
  const [menu, setMenu] = createSignal<{ x: number; y: number; items: MenuEntry[] } | null>(null);

  const openMenu = (x: number, y: number) => {
    if (!viewer) return;
    const v = viewer;
    const items: MenuEntry[] = [
      { header: "Vista" },
      { label: "Ver todo", shortcut: "Inicio", onSelect: () => v.resetView(), disabled: !props.meshData },
      { label: "Frente", shortcut: "1", onSelect: () => v.setView("front") },
      { label: "Derecha", shortcut: "3", onSelect: () => v.setView("right") },
      { label: "Arriba", shortcut: "7", onSelect: () => v.setView("top") },
      { label: "Atrás", shortcut: "Ctrl+1", onSelect: () => v.setView("back") },
    ];
    const extra = props.contextMenuItems?.() ?? [];
    if (extra.length > 0) items.push({ separator: true }, ...extra);
    setMenu({ x, y, items });
  };

  onMount(() => {
    if (canvasRef) {
      viewer = new Viewer3D(canvasRef);

      viewer.setCallbacks({
        onFpsUpdate: props.onFpsUpdate,
        onPlacementPick: (pick) => props.onPlacementPick?.(pick),
        onBoneSelected: props.onBoneSelected,
        onBoneMoved: props.onBoneMoved,
        onBoneMoveCommitted: (index, from, to) => props.onBoneMoveCommitted?.(index, from, to),
        onPaintSettingsChanged: (change) => props.onPaintSettingsChanged?.(change),
        onHint: setHint,
        onWeightsPainted: (stroke) => props.onWeightsPainted?.(stroke),
        onLightsChanged: (lights) => props.onLightsChanged?.(lights),
        onPoseEdited: (joint) => props.onPoseEdited?.(joint),
        onSkeletonTransformed: (change) => props.onSkeletonTransformed?.(change),
        onObjectTransformed: (matrix, mode) => props.onObjectTransformed?.(matrix, mode),
        onMeasure: setMeasure,
        onContextMenu: openMenu,
        onGridChanged: setGridLabel,
      });

      props.onViewerReady?.(viewer);
    }
  });

  onCleanup(() => {
    viewer?.dispose();
  });

  // React to mesh data changes
  createEffect(() => {
    if (viewer && props.meshData) {
      viewer.loadMesh(props.meshData);
    }
  });

  // React to skeleton data changes
  // Después de cargar la malla: sus texturas dependen de que tenga UV
  createEffect(() => {
    props.meshData;
    if (viewer) {
      viewer.setTextures(props.textures ?? {});
    }
  });

  createEffect(() => {
    const materials = props.sceneMaterials ?? [];
    viewer?.setSceneMaterials(materials);
  });

  createEffect(() => {
    if (viewer && props.lights) viewer.setLights(props.lights);
  });

  createEffect(() => {
    if (!viewer) return;
    if (props.skeletonData) viewer.loadSkeleton(props.skeletonData);
    else viewer.unloadSkeleton();
  });

  // React to weights data changes
  createEffect(() => {
    if (!viewer) return;
    if (props.weightsData) viewer.loadWeights(props.weightsData);
    else viewer.unloadWeights();
  });

  // React to settings changes
  createEffect(() => {
    if (viewer && props.settings) {
      viewer.updateSettings(props.settings);
    }
  });

  // Orientación: planos candidatos o clic en la superficie
  createEffect(() => {
    viewer?.setPlacementMode(props.placementMode ?? null, props.floorCandidates ?? []);
  });

  // React to bone edit mode
  createEffect(() => {
    if (viewer) {
      viewer.setBoneEditMode(props.boneEditMode ?? false);
    }
  });

  // React to active tool
  createEffect(() => {
    viewer?.setPaintMode(props.paintSettings ?? null);
  });

  createEffect(() => {
    viewer?.setObjectTools(props.objectTools ?? false);
  });

  createEffect(() => {
    if (viewer && props.activeTool) {
      viewer.setActiveTool(props.activeTool);
    }
  });

  return (
    <main class="relative w-full h-full bg-bg overflow-hidden">
      {/* Canvas */}
      <canvas
        ref={canvasRef}
        class="w-full h-full block"
      />

      {/* Overlay */}
      <div class="absolute inset-0 pointer-events-none">
        {/* Ayuda de la orientación */}
        <Show when={props.placementMode}>
          <div class="absolute top-3 left-1/2 -translate-x-1/2 px-3 py-1.5 rounded-md bg-purple/90 text-bg text-xs font-medium">
            {PLACEMENT_HINTS[props.placementMode!]}
          </div>
        </Show>

        {/* Ayuda de la operación en curso, o de la herramienta activa */}
        <Show
          when={hint()}
          fallback={
            <Show when={props.meshData && props.toolHint && !props.placementMode && !props.paintSettings}>
              <div class="absolute top-3 left-1/2 -translate-x-1/2 px-3 py-1.5 rounded-md bg-bg-darker/80 text-text-muted text-xs">
                {props.toolHint}
              </div>
            </Show>
          }
        >
          <div class="absolute top-3 left-1/2 -translate-x-1/2 px-3 py-1.5 rounded-md bg-orange/90 text-bg text-xs font-medium">
            {hint()}
          </div>
        </Show>

        {/* Medida de la herramienta Medir */}
        <Show when={measure()}>
          <div class="absolute top-12 left-1/2 -translate-x-1/2 px-3 py-1.5 rounded-md bg-bg-darker/90 border border-yellow/60 text-text text-xs font-mono">
            {measure()}
          </div>
        </Show>

        {/* Stats */}
        <Show when={props.showStats !== false}>
          <div
            class={clsx(
              "absolute bottom-3 left-3",
              "flex gap-4 px-2.5 py-1.5 rounded-md",
              "bg-bg-darker/90 backdrop-blur-sm border border-border/50"
            )}
          >
            <span class="text-xs text-text-muted">
              Vértices: <span class="text-text font-mono">{props.vertices?.toLocaleString() ?? "--"}</span>
            </span>
            <span class="text-xs text-text-muted">
              Caras: <span class="text-text font-mono">{props.faces?.toLocaleString() ?? "--"}</span>
            </span>
            <Show when={props.meshData && gridLabel()}>
              <span class="text-xs text-text-muted">{gridLabel()}</span>
            </Show>
          </div>
        </Show>
      </div>
      <Show when={menu()}>
        {(m) => <ContextMenu x={m().x} y={m().y} items={m().items} onClose={() => setMenu(null)} />}
      </Show>
    </main>
  );
};
