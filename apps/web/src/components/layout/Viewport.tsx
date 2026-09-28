import { Component, onMount, onCleanup, Show, createEffect } from "solid-js";
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
} from "../../lib/Viewer3D";

export interface ViewportProps {
  // Callbacks
  onViewerReady?: (viewer: Viewer3D) => void;
  onFpsUpdate?: (fps: number) => void;
  onGroundSelected?: () => void;
  onBoneSelected?: (index: number) => void;
  onBoneMoved?: (index: number, position: [number, number, number]) => void;
  /** La luz principal se movió arrastrando con Alt */
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

  // Ground mode
  groundSelectionMode?: boolean;

  // Bone editing
  boneEditMode?: boolean;

  // Active tool
  activeTool?: string;

  /** Pincel de pesos activo (`undefined` = apagado) */
  paintSettings?: PaintSettings;
}

export const Viewport: Component<ViewportProps> = (props) => {
  let canvasRef: HTMLCanvasElement | undefined;
  let viewer: Viewer3D | undefined;

  onMount(() => {
    if (canvasRef) {
      viewer = new Viewer3D(canvasRef);

      viewer.setCallbacks({
        onFpsUpdate: props.onFpsUpdate,
        onGroundSelected: props.onGroundSelected,
        onBoneSelected: props.onBoneSelected,
        onBoneMoved: props.onBoneMoved,
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

  // React to ground selection mode
  createEffect(() => {
    if (viewer) {
      viewer.setGroundSelectionMode(props.groundSelectionMode ?? false);
    }
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

        {/* Help text for ground selection */}
        <Show when={props.groundSelectionMode}>
          <div class="absolute top-3 left-1/2 -translate-x-1/2 px-3 py-1.5 rounded-md bg-purple/90 text-bg text-xs font-medium">
            Click en una cara para establecerla como suelo
          </div>
        </Show>

        {/* Help text for bone editing */}
        <Show when={props.boneEditMode}>
          <div class="absolute top-3 left-1/2 -translate-x-1/2 px-3 py-1.5 rounded-md bg-orange/90 text-bg text-xs font-medium">
            Click en un hueso para seleccionarlo y arrastrarlo
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
