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
} from "../../lib/Viewer3D";
import type { FloorCandidate } from "../../lib/placement";

export interface ViewportProps {
  // Callbacks
  onViewerReady?: (viewer: Viewer3D) => void;
  onFpsUpdate?: (fps: number) => void;
  /** Clic al orientar (plano candidato o punto de la superficie) */
  onPlacementPick?: (pick: PlacementPick) => void;
  onBoneSelected?: (index: number) => void;
  onBoneMoved?: (index: number, position: [number, number, number]) => void;
  /** Fin de un movimiento con G (para deshacer) */
  onBoneMoveCommitted?: (index: number, from: [number, number, number], to: [number, number, number]) => void;
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

  /** Pincel de pesos activo (`undefined` = apagado) */
  paintSettings?: PaintSettings;
}

const PLACEMENT_HINTS: Record<PlacementMode, string> = {
  floor: "Clic en un plano: pasa a ser el piso (verde = estable) · Shift+clic: la superficie bajo el cursor",
  front: "Clic en la cara que debe mirar al frente (+Z)",
  point: "Clic en el punto que será el origen",
};

export const Viewport: Component<ViewportProps> = (props) => {
  let canvasRef: HTMLCanvasElement | undefined;
  let viewer: Viewer3D | undefined;
  /** Ayuda de la operación modal en curso (G, R, F…) */
  const [hint, setHint] = createSignal<string | null>(null);

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
    if (viewer && props.skeletonData) {
      viewer.loadSkeleton(props.skeletonData);
    }
  });

  // React to weights data changes
  createEffect(() => {
    if (viewer && props.weightsData) {
      viewer.loadWeights(props.weightsData);
    }
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
        {/* Axis Gizmo */}
        <div class="absolute top-3 right-3 w-16 h-16 flex items-center justify-center">
          <div class="relative w-12 h-12">
            {/* X axis */}
            <div class="absolute left-1/2 top-1/2 w-5 h-0.5 bg-red origin-left" style={{ transform: "rotate(-30deg)" }}>
              <span class="absolute -right-3 -top-2 text-[10px] font-bold text-red">X</span>
            </div>
            {/* Y axis */}
            <div class="absolute left-1/2 top-1/2 w-0.5 h-5 bg-green origin-top" style={{ transform: "translateX(-50%) rotate(0deg)" }}>
              <span class="absolute -top-4 left-1/2 -translate-x-1/2 text-[10px] font-bold text-green">Y</span>
            </div>
            {/* Z axis */}
            <div class="absolute left-1/2 top-1/2 w-5 h-0.5 bg-cyan origin-left" style={{ transform: "rotate(30deg)" }}>
              <span class="absolute -right-3 top-0 text-[10px] font-bold text-cyan">Z</span>
            </div>
          </div>
        </div>

        {/* Ayuda de la orientación */}
        <Show when={props.placementMode}>
          <div class="absolute top-3 left-1/2 -translate-x-1/2 px-3 py-1.5 rounded-md bg-purple/90 text-bg text-xs font-medium">
            {PLACEMENT_HINTS[props.placementMode!]}
          </div>
        </Show>

        {/* Ayuda de la operación en curso, o de los gestos del esqueleto */}
        <Show
          when={hint()}
          fallback={
            <Show when={props.skeletonData && !props.placementMode && !props.paintSettings}>
              <div class="absolute top-3 left-1/2 -translate-x-1/2 px-3 py-1.5 rounded-md bg-bg-darker/80 text-text-muted text-xs">
                Clic: seleccionar articulación · G mover · R rotar · rueda/botón central: vista
              </div>
            </Show>
          }
        >
          <div class="absolute top-3 left-1/2 -translate-x-1/2 px-3 py-1.5 rounded-md bg-orange/90 text-bg text-xs font-medium">
            {hint()}
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
          </div>
        </Show>
      </div>
    </main>
  );
};
