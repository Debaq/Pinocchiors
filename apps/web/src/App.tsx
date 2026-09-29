import { Component, createEffect, createMemo, createSignal, onMount, onCleanup, Show, untrack } from "solid-js";
import { invoke, Channel } from "@tauri-apps/api/core";
import { ask, open, save } from "@tauri-apps/plugin-dialog";
import { Header, StatusBar, Viewport, Toolbar, ProgressOverlay, Timeline, type TimelineRow } from "./components/layout";
import { WelcomeScreen } from "./components/layout/WelcomeScreen";
import { ContextPanel } from "./components/layout/ContextPanel";
import * as Icons from "./components/icons";
import {
  type SkeletonPreset,
  type AutorigConfig,
  type ViewSettings,
  type RetopologyConfig,
  type QuadQuality,
} from "./components/panels";
import {
  Viewer3D,
  MeshData,
  MeshTextures,
  PaintSettings,
  PaintStroke,
  SceneMaterial,
  LightSettings,
  defaultLights,
  SkeletonData,
  WeightsData,
  type GridUnits,
  type PlacementMode,
  type PlacementPick,
} from "./lib/Viewer3D";
import * as THREE from "three";
import {
  dropMatrix,
  floorMatrix,
  frontMatrix,
  mirrorMatrix,
  originMatrix,
  rotationMatrix,
  scaleMatrix,
  translationMatrix,
  zoneNormal,
  boundsAfter,
  type Axis,
  type FloorCandidate,
  type OriginMode,
  type PlacementInfo,
} from "./lib/placement";
import type { SceneStructure, MaterialInfo } from "./components/steps/StructureStep";
import { createPipelineStore } from "./lib/pipeline";
import { buildSceneTree } from "./lib/scene-tree";
import type { ToolId } from "./lib/tools";
import { createHistoryStore } from "./lib/history";
import { createShortcutManager, type ShortcutDef } from "./lib/shortcuts";
import { decodeMesh, decodeWeights } from "./lib/buffers";
import { createPersisted } from "./lib/ui-state";
import { applyTheme, followSystemTheme, type ThemeSetting } from "./lib/theme";
import type { MenuEntry } from "./components/ui/ContextMenu";
import {
  SettingsDialog,
  UNIT_METERS,
  type AutosaveSettings,
  type GridSettings,
  type LengthUnit,
} from "./components/layout/SettingsDialog";

interface ProjectSaved {
  written: boolean;
  bytes: number;
}

interface ProjectOpened {
  ui: string;
  source_name: string | null;
  saved_at: number;
}

interface RecoveryInfo {
  path: string;
  saved_at: number;
  source_name: string | null;
}
import {
  clipsForExport,
  createClip,
  deleteKeys,
  duplicateClip,
  insertKeys,
  keyId,
  moveKeys,
  parseKeyId,
  samplePose,
  setKeysInterpolation,
  type AnimationClip,
  type KeyInterpolation,
} from "./lib/animation";
import type { SkeletonTransform } from "./components/panels/SkeletonTransformPanel";
import type { MeshDiagnostics, RepairResult, RepairAnalysisConfig, RepairOptions } from "./components/panels/RepairPanel";
import type { MeshAnalysis, SubdivideResult, ScaleParams, SubdivideConfig } from "./components/panels/Print3DPanel";
import { defaultExportOptions, formatBytes, type ExportOptions } from "./components/steps/ExportStep";
import { defaultUvConfig, type UvConfig, type UvInfo, type UvPreview } from "./components/steps/UvStep";
import type { SkeletonFitInfo } from "./components/steps/SkeletonStep";
import type { BodyPlan } from "./components/panels/BodyPlanPanel";

// ═══════════════════════════════════════════════════════════════════════════
// TAURI TYPES
// ═══════════════════════════════════════════════════════════════════════════

interface MeshInfo {
  num_vertices: number;
  num_faces: number;
  num_meshes: number;
  has_normals: boolean;
  has_uvs: boolean;
  has_materials: boolean;
  bounding_box: { min: [number, number, number]; max: [number, number, number] };
  format: string;
}

interface TauriSkeletonPreset {
  id: string;
  name: string;
  description: string;
  num_bones: number;
}

interface TauriAutoFitResult {
  skeleton: TauriSkeletonData;
  quality: number;
  extremities: number;
  unused_extremities: [number, number, number][];
}

interface TauriSkeletonData {
  bones: Array<{
    name: string;
    position: [number, number, number];
    parent: number | null;
    is_leaf: boolean;
  }>;
  edges: Array<[number, number]>;
  /** Centro de escala y rotación, si el esqueleto está transformado */
  pivot?: [number, number, number];
}

interface Progress {
  stage: string;
  percent: number;
  message: string;
}

interface ExportResult {
  success: boolean;
  path: string;
  message: string;
  files_created: string[];
  total_bytes: number;
}

interface SupportedFormats {
  import: Array<{ id: string; name: string; extensions: string[]; description: string }>;
  export: Array<{ id: string; name: string; extensions: string[]; description: string }>;
}

// Repair types
interface TauriRepairResult extends RepairResult {
  new_mesh_info: MeshInfo;
  new_diagnostics: MeshDiagnostics;
}

// Print3D types
interface TauriPrint3dAnalysis {
  volume: number;
  surface_area: number;
  center_of_mass: [number, number, number];
  dimensions: [number, number, number];
  is_closed: boolean;
  vertex_count: number;
  triangle_count: number;
  estimated_weight: number | null;
}

interface TauriSubdivideResult {
  piece_count: number;
  pieces: Array<{
    index: number;
    label: string;
    vertex_count: number;
    face_count: number;
    dimensions: [number, number, number];
  }>;
}

// Retopology types
interface TauriQuadMeshInfo {
  num_vertices: number;
  num_quads: number;
  bounding_box: { min: [number, number, number]; max: [number, number, number] };
  quality: QuadQuality;
  /** Caras que cruzan una costura del mapa UV original; null si no había UV */
  uv_seam_faces: number | null;
}

// ═══════════════════════════════════════════════════════════════════════════
// HELPERS
// ═══════════════════════════════════════════════════════════════════════════

function tauriSkeletonToViewer(data: TauriSkeletonData): SkeletonData {
  return {
    bones: data.bones.map((b) => ({
      name: b.name,
      position: b.position,
      parent: b.parent,
      isLeaf: b.is_leaf,
    })),
    edges: data.edges,
    pivot: data.pivot,
  };
}

// ═══════════════════════════════════════════════════════════════════════════
// APP COMPONENT
// ═══════════════════════════════════════════════════════════════════════════

export const App: Component = () => {
  // Pipeline
  const pipeline = createPipelineStore();

  // History (undo/redo)
  // Pasos del historial como datos (se guardan en el proyecto); cada tipo
  // sabe aplicarse y deshacerse. Las funciones se definen más abajo
  type Vec3 = [number, number, number];
  const sendWeights = (vertices: number[], rows: number[]) =>
    invoke("set_vertex_weights", { vertices, influences: rows });
  const history = createHistoryStore({
    clip: {
      apply: (d: { before: AnimationClip; after: AnimationClip }) => replaceClip(d.after),
      revert: (d: { before: AnimationClip; after: AnimationClip }) => replaceClip(d.before),
    },
    clips: {
      apply: (d: { after: AnimationClip[]; activeAfter?: string }) => {
        setClips(d.after);
        setActiveClipId(d.activeAfter);
      },
      revert: (d: { before: AnimationClip[]; activeBefore?: string }) => {
        setClips(d.before);
        setActiveClipId(d.activeBefore);
      },
    },
    placement: {
      apply: (d: { matrix: number[] }) => sendPlacement(new THREE.Matrix4().fromArray(d.matrix)),
      revert: (d: { matrix: number[] }) => sendPlacement(new THREE.Matrix4().fromArray(d.matrix).invert()),
    },
    skeletonTransform: {
      apply: (d: { before: SkeletonTransform; after: SkeletonTransform }) => applyTransform(d.after),
      revert: (d: { before: SkeletonTransform; after: SkeletonTransform }) => applyTransform(d.before),
    },
    moveJoint: {
      apply: (d: { index: number; from: Vec3; to: Vec3 }) => handleBoneMoved(d.index, d.to),
      revert: (d: { index: number; from: Vec3; to: Vec3 }) => handleBoneMoved(d.index, d.from),
    },
    paintWeights: {
      apply: async (d: { vertices: number[]; before: number[]; after: number[] }) => {
        viewerRef?.applyWeightRows(new Uint32Array(d.vertices), new Float32Array(d.after));
        await sendWeights(d.vertices, d.after);
      },
      revert: async (d: { vertices: number[]; before: number[]; after: number[] }) => {
        viewerRef?.applyWeightRows(new Uint32Array(d.vertices), new Float32Array(d.before));
        await sendWeights(d.vertices, d.before);
      },
    },
  });

  // State
  const [fps, setFps] = createSignal(0);
  const [statusMessage, setStatusMessage] = createSignal("Listo - Importa un modelo para comenzar");
  // value ausente = progreso indeterminado (solo etiqueta)
  const [progress, setProgress] = createSignal<{ value?: number; label?: string } | undefined>();

  /** Canal de progreso del backend que alimenta la barra */
  const progressChannel = () => {
    const channel = new Channel<Progress>();
    channel.onmessage = (msg) => setProgress({ value: msg.percent, label: msg.message });
    return channel;
  };

  /** Muestra la barra mientras dura `task` (indeterminada salvo que un canal informe avance) */
  const busy = async <T,>(label: string, task: () => Promise<T>): Promise<T> => {
    setProgress({ label });
    try {
      return await task();
    } finally {
      setProgress(undefined);
    }
  };

  /** Malla actual del backend, en binario (sin JSON), con la estructura y
   * los materiales del archivo de origen */
  const fetchMeshData = () =>
    busy("Cargando en el visor...", async () => {
      const mesh = decodeMesh(await invoke<ArrayBuffer>("get_mesh_data"));
      await loadSceneAppearance();
      return mesh;
    });

  // Archivo de origen: estructura, materiales y texturas
  const [sceneStructure, setSceneStructure] = createSignal<SceneStructure | undefined>();
  const [sceneMaterials, setSceneMaterials] = createSignal<SceneMaterial[]>([]);
  const [textureUrls, setTextureUrls] = createSignal<string[]>([]);
  const [lights, setLights] = createSignal<LightSettings>({ ...defaultLights });

  /** Estructura, materiales y texturas de la escena (miniaturas e imágenes para el visor) */
  const loadSceneAppearance = async () => {
    try {
      const [structure, materials] = await Promise.all([
        invoke<SceneStructure>("get_scene_structure"),
        invoke<MaterialInfo[]>("get_scene_materials"),
      ]);
      const buffers = await Promise.all(
        structure.textures.map((_, index) => invoke<ArrayBuffer>("get_scene_texture", { index }))
      );
      const mime = (format: string) => (format === "JPEG" ? "image/jpeg" : format === "WebP" ? "image/webp" : "image/png");
      const blobs = buffers.map((b, i) => new Blob([b], { type: mime(structure.textures[i].format) }));
      const images = await Promise.all(blobs.map((b) => createImageBitmap(b).catch(() => undefined)));
      textureUrls().forEach((url) => URL.revokeObjectURL(url));
      setTextureUrls(blobs.map((b) => URL.createObjectURL(b)));
      const image = (i: number | null) => (i === null ? undefined : images[i]);
      setSceneMaterials(
        materials.map((m) => ({
          name: m.name,
          baseColor: m.base_color,
          metallic: m.metallic,
          roughness: m.roughness,
          emissive: m.emissive,
          normalScale: m.normal_scale,
          occlusionStrength: m.occlusion_strength,
          alphaMode: m.alpha_mode,
          alphaCutoff: m.alpha_cutoff,
          unlit: m.unlit,
          maps: {
            base: image(m.base_color_texture),
            metallicRoughness: image(m.metallic_roughness_texture),
            normal: image(m.normal_texture),
            occlusion: image(m.occlusion_texture),
            emissive: image(m.emissive_texture),
          },
        }))
      );
      setSceneStructure(structure);
    } catch (e) {
      console.error("Scene structure error:", e);
    }
  };
  const [fileName, setFileName] = createSignal<string | undefined>();

  // Skeleton presets (loaded from Tauri)
  const [skeletonPresets, setSkeletonPresets] = createSignal<SkeletonPreset[]>([]);
  const [supportedFormats, setSupportedFormats] = createSignal<SupportedFormats | null>(null);

  // Mesh state
  const [meshLoaded, setMeshLoaded] = createSignal(false);
  const [meshInfo, setMeshInfo] = createSignal({ vertices: 0, faces: 0, format: "" });
  const [meshData, setMeshData] = createSignal<MeshData | undefined>();

  // Skeleton state
  const [selectedSkeleton, setSelectedSkeleton] = createSignal<string | undefined>();
  const [skeletonLoaded, setSkeletonLoaded] = createSignal(false);
  const [skeletonData, setSkeletonData] = createSignal<SkeletonData | undefined>();

  // Autorig state
  const [autorigComplete, setAutorigComplete] = createSignal(false);
  const [isProcessing, setIsProcessing] = createSignal(false);
  const [boneNames, setBoneNames] = createSignal<string[]>([]);
  const [weightsData, setWeightsData] = createSignal<WeightsData | undefined>();

  // Orientación: qué se elige con el clic y los planos candidatos a piso
  const [placementMode, setPlacementMode] = createSignal<PlacementMode | undefined>();
  const [floorCandidates, setFloorCandidates] = createSignal<FloorCandidate[]>([]);
  const [centerOfMass, setCenterOfMass] = createSignal<THREE.Vector3 | undefined>();

  // Retopology state
  const [retopologyConfig, setRetopologyConfig] = createSignal<RetopologyConfig>({
    targetQuads: 5000,
    preserveSharp: false,
    sharpAngle: 45,
    smoothIterations: 10,
    rebuild: "auto",
    curvatureAlignment: 1,
    adaptiveDensity: false,
    symmetry: "none",
    followSeams: true,
  });
  const [quadQuality, setQuadQuality] = createSignal<QuadQuality | undefined>();
  const [quadMeshData, setQuadMeshData] = createSignal<MeshData | undefined>();
  const [quadMeshLoaded, setQuadMeshLoaded] = createSignal(false);
  const [quadMeshInfo, setQuadMeshInfo] = createSignal({ vertices: 0, quads: 0 });
  const [showQuadMesh, setShowQuadMesh] = createSignal(false);
  const [exportIncludeRig, setExportIncludeRig] = createSignal(true);
  const [exportUseRetopology, setExportUseRetopology] = createSignal(false);
  const [exportOptions, setExportOptions] = createSignal<ExportOptions>(defaultExportOptions);
  const [lastExport, setLastExport] = createSignal<{ bytes: number; files: string[] } | undefined>();
  const [canUndoPrintScale, setCanUndoPrintScale] = createSignal(false);

  // Animación: clips con keys por articulación, cuadro actual y reproducción
  const [clips, setClips] = createSignal<AnimationClip[]>([]);
  const [activeClipId, setActiveClipId] = createSignal<string | undefined>();
  const [frame, setFrame] = createSignal(0);
  const [playing, setPlaying] = createSignal(false);
  const [keySelection, setKeySelection] = createSignal<Set<string>>(new Set());
  const [autoKey, setAutoKey] = createSignal(true);
  const [keyInterpolation, setKeyInterpolation] = createSignal<KeyInterpolation>("linear");
  /** El visor, como señal: los efectos de animación lo necesitan listo */
  const [viewer, setViewer] = createSignal<Viewer3D | undefined>();

  // UV / Piel de la malla retopologizada
  const [uvConfig, setUvConfig] = createSignal<UvConfig>(defaultUvConfig);
  const [uvInfo, setUvInfo] = createSignal<UvInfo | undefined>();
  const [uvPreview, setUvPreview] = createSignal<UvPreview>("texture");
  const [uvLayout, setUvLayout] = createSignal<Float32Array | undefined>();
  const [skinTextures, setSkinTextures] = createSignal<MeshTextures>({});
  const [checkerTexture, setCheckerTexture] = createSignal<ImageBitmap | undefined>();

  // Malla activa: tras retopologizar, UV, esqueleto y pesos trabajan sobre los
  // quads (lo que se exporta), salvo que se elija volver a la original
  const [activeQuad, setActiveQuad] = createSignal(false);
  const usesQuad = () => activeQuad() && quadMeshLoaded();
  /** El visor muestra los quads: a pedido, o en las etapas que los usan */
  const displayQuad = () =>
    (showQuadMesh() || (usesQuad() && ["uv", "skeleton", "animate"].includes(pipeline.activeStep()))) && !!quadMeshData();

  /** Los pesos ya no corresponden a la malla activa */
  const dropWeights = () => {
    setAutorigComplete(false);
    setWeightsData(undefined);
    setPaintMirrorLoaded(false);
    setViewSettings((prev) => ({ ...prev, showWeights: false }));
  };

  /** Relee los pesos en el orden de vértices del visor (cambia al desplegar UV) */
  const reloadWeights = async () => {
    const weights = decodeWeights(await invoke<ArrayBuffer>("get_weights_data"));
    setWeightsData(weights);
    setPaintMirrorLoaded(false);
  };

  const decodeImage = async (buffer: ArrayBuffer): Promise<ImageBitmap | undefined> =>
    buffer.byteLength > 0 ? createImageBitmap(new Blob([buffer])) : undefined;

  /** Lee del backend la piel actual: estado, atlas, texturas y la malla con UV */
  const refreshSkin = async () => {
    const info = await invoke<UvInfo | null>("get_uv_info");
    setUvInfo(info ?? undefined);
    if (!info) {
      setUvLayout(undefined);
      setSkinTextures({});
      return;
    }
    const [layout, base, normal, mesh] = await Promise.all([
      invoke<ArrayBuffer>("get_uv_layout"),
      invoke<ArrayBuffer>("get_uv_texture", { kind: "base" }).then(decodeImage),
      invoke<ArrayBuffer>("get_uv_texture", { kind: "normal" }).then(decodeImage),
      invoke<ArrayBuffer>("get_quad_mesh_data"),
    ]);
    setUvLayout(new Float32Array(layout));
    setSkinTextures({ base, normal });
    setQuadMeshData(decodeMesh(mesh));
    if (autorigComplete() && usesQuad()) await reloadWeights();
  };

  /** Texturas que ve el visor sobre la malla de quads según la vista elegida */
  const viewerTextures = (): MeshTextures | undefined => {
    if (!displayQuad() || !uvInfo()) return undefined;
    switch (uvPreview()) {
      case "texture":
        return skinTextures();
      case "checker":
        return { base: checkerTexture() };
      default:
        return {};
    }
  };

  const handleUvPreview = async (preview: UvPreview) => {
    if (preview === "checker" && !checkerTexture()) {
      setCheckerTexture(await decodeImage(await invoke<ArrayBuffer>("get_uv_texture", { kind: "checker" })));
    }
    setUvPreview(preview);
    setShowQuadMesh(true);
  };

  /** Descarta la retopología: ya no corresponde a la geometría actual */
  const clearQuadMesh = () => {
    setUvInfo(undefined);
    setUvLayout(undefined);
    setSkinTextures({});
    setQuadMeshData(undefined);
    setQuadMeshLoaded(false);
    setQuadMeshInfo({ vertices: 0, quads: 0 });
    setQuadQuality(undefined);
    setShowQuadMesh(false);
    setExportUseRetopology(false);
    setActiveQuad(false);
  };

  // Config
  const [autorigConfig, setAutorigConfig] = createSignal<AutorigConfig>({
    quality: "default",
    diffusionWeight: 1.0,
    maxInfluences: 4,
  });

  // View settings (la navegación de notebook se recuerda entre sesiones)
  const [trackpadNavigation, setTrackpadNavigation] = createPersisted("view.trackpadNavigation", false);
  const [viewSettings, setViewSettings] = createSignal<ViewSettings>({
    trackpadNavigation: trackpadNavigation(),
    showMesh: true,
    showWireframe: false,
    showSkeleton: true,
    showWeights: false,
    selectedBone: -1,
    showTextures: true,
  });

  // Skeleton transform
  const defaultTransform: SkeletonTransform = { scale: 1, translation: [0, 0, 0], rotation: [0, 0, 0] };
  const [skeletonTransform, setSkeletonTransform] = createSignal<SkeletonTransform>({ ...defaultTransform });
  // Edición del esqueleto
  const [symmetricEdit, setSymmetricEdit] = createSignal(true);
  // Pincel de pesos (el hueso es el seleccionado)
  const [paintConfig, setPaintConfig] = createSignal<Omit<PaintSettings, "bone">>({
    mode: "add",
    radius: 0.04,
    strength: 0.3,
    mirror: true,
  });
  const [paintMirrorLoaded, setPaintMirrorLoaded] = createSignal(false);
  const [fitInfo, setFitInfo] = createSignal<SkeletonFitInfo | undefined>();
  const [bodyPlan, setBodyPlan] = createSignal<BodyPlan | undefined>();
  const [boneEditMode, setBoneEditMode] = createSignal(false);

  // Active tool
  const [activeTool, setActiveTool] = createSignal<ToolId>("select");

  // Repair state
  const [repairAnalysisConfig, setRepairAnalysisConfig] = createSignal<RepairAnalysisConfig>({
    checkSelfIntersections: false,
  });
  const [repairOptions, setRepairOptions] = createSignal<RepairOptions>({
    mergeDuplicates: true,
    removeDegenerates: true,
    fixNormals: true,
    fixNonManifold: true,
    orientOutward: true,
    removeSmallComponents: false,
    fillHoles: true,
    maxHoleEdges: 0,
    refineFill: true,
  });
  const [diagnostics, setDiagnostics] = createSignal<MeshDiagnostics | undefined>();
  const [repairResult, setRepairResult] = createSignal<RepairResult | undefined>();
  const [canUndoRepair, setCanUndoRepair] = createSignal(false);

  // Print3D state
  const [meshAnalysis, setMeshAnalysis] = createSignal<MeshAnalysis | undefined>();
  const [subdivideResult, setSubdivideResult] = createSignal<SubdivideResult | undefined>();

  // Viewer ref
  let viewerRef: Viewer3D | undefined;

  // Context panel visibility
  const [showContextPanel, setShowContextPanel] = createSignal(true);

  // Grid visibility
  const [showGrid, setShowGrid] = createSignal(true);

  // Derived scene tree
  const sceneTree = () => buildSceneTree({
    hasMesh: meshLoaded(),
    hasWireframe: meshLoaded(),
    hasSkeleton: skeletonLoaded(),
    hasWeights: autorigComplete(),
    hasQuadMesh: quadMeshLoaded(),
    showMesh: viewSettings().showMesh,
    showWireframe: viewSettings().showWireframe,
    showSkeleton: viewSettings().showSkeleton,
    showWeights: viewSettings().showWeights,
    showGrid: showGrid(),
    showQuadMesh: showQuadMesh(),
    skeletonData: skeletonData(),
    selectedBone: viewSettings().selectedBone,
  });

  // Shortcuts
  const shortcuts = createShortcutManager();

  const shortcutDefs: ShortcutDef[] = [
    { key: "s", ctrl: true, action: () => handleSaveProject(), description: "Guardar proyecto" },
    { key: "s", ctrl: true, shift: true, action: () => handleSaveProject(true), description: "Guardar proyecto como" },
    { key: "o", ctrl: true, action: () => handleOpenProject(), description: "Abrir proyecto" },
    { key: "i", ctrl: true, action: () => handleLoad(), description: "Importar modelo" },
    { key: "z", ctrl: true, action: () => history.undo(), description: "Deshacer" },
    { key: "z", ctrl: true, shift: true, action: () => history.redo(), description: "Rehacer" },
    { key: "q", action: () => { setActiveTool("select"); setBoneEditMode(false); }, description: "Seleccionar" },
    // Como en Blender: G y R son operaciones modales sobre la articulación
    // seleccionada (el mouse mueve, clic confirma, clic derecho/Esc cancela)
    { key: "g", action: () => viewerRef?.startModal("grab"), description: "Mover articulación" },
    { key: "r", action: () => viewerRef?.startModal("rotate"), description: "Rotar (pose de prueba)" },
    { key: "s", action: () => useTool("scale"), description: "Escalar el esqueleto entero" },
    { key: "Home", action: () => viewerRef?.resetView(), description: "Ver todo" },
    {
      key: " ",
      action: () => (animating() ? setPlaying(!playing()) : viewerRef?.resetView()),
      description: "Reproducir (Animar) / ver todo",
    },
    { key: "i", action: () => animating() && handleInsertKey(), description: "Insertar key" },
    { key: "x", action: () => animating() && handleDeleteKeys(), description: "Borrar keys" },
    { key: "Delete", action: () => animating() && handleDeleteKeys(), description: "Borrar keys" },
    { key: "ArrowLeft", action: () => animating() && stepFrame(-1), description: "Cuadro anterior" },
    { key: "ArrowRight", action: () => animating() && stepFrame(1), description: "Cuadro siguiente" },
    { key: "ArrowLeft", shift: true, action: () => animating() && stepFrame(-Infinity), description: "Al inicio" },
    { key: "ArrowRight", shift: true, action: () => animating() && stepFrame(Infinity), description: "Al final" },
    { key: ".", action: () => viewerRef?.focusSelection(), description: "Centrar en la selección" },
    {
      key: "f",
      action: () => (paintSettings() ? viewerRef?.startModal("radius") : viewerRef?.focusSelection()),
      description: "Radio del pincel / centrar",
    },
    { key: "f", shift: true, action: () => viewerRef?.startModal("strength"), description: "Intensidad del pincel" },
    { key: "n", action: () => setShowContextPanel(!showContextPanel()), description: "Mostrar/ocultar panel" },
    { key: "1", action: () => viewerRef?.setView("front"), description: "Vista frontal" },
    { key: "3", action: () => viewerRef?.setView("right"), description: "Vista derecha" },
    { key: "7", action: () => viewerRef?.setView("top"), description: "Vista superior" },
    { key: "1", ctrl: true, action: () => viewerRef?.setView("back"), description: "Vista trasera" },
    { key: "3", ctrl: true, action: () => viewerRef?.setView("left"), description: "Vista izquierda" },
    { key: "7", ctrl: true, action: () => viewerRef?.setView("bottom"), description: "Vista inferior" },
    { key: "b", action: () => useTool("paint"), description: "Pintar pesos" },
  ];

  // Para la pestaña de atajos del panel derecho
  const keyName = (key: string) => (key === " " ? "Espacio" : key.length === 1 ? key.toUpperCase() : key);
  const shortcutHints = shortcutDefs.map((d) => ({
    keys: [d.ctrl && "Ctrl", d.shift && "Shift", d.alt && "Alt", keyName(d.key)].filter(Boolean).join("+"),
    description: d.description,
  }));

  // Load presets on mount
  onMount(async () => {
    // Register keyboard shortcuts
    shortcuts.register(shortcutDefs);
    shortcuts.attach();

    try {
      const presets = await invoke<TauriSkeletonPreset[]>("list_skeleton_presets");
      setSkeletonPresets(
        presets.map((p) => ({
          id: p.id,
          name: p.name,
          description: p.description,
          numBones: p.num_bones,
        }))
      );

      const formats = await invoke<SupportedFormats>("get_supported_formats");
      setSupportedFormats(formats);
    } catch (e) {
      console.error("Error loading presets:", e);
      setStatusMessage("Error conectando con el backend");
    }
  });

  onCleanup(() => {
    shortcuts.detach();
  });

  // ═══════════════════════════════════════════════════════════════════════════
  // HANDLERS
  // ═══════════════════════════════════════════════════════════════════════════

  const handleViewerReady = (v: Viewer3D) => {
    viewerRef = v;
    setViewer(v);
  };

  const handleLoad = async () => {
    try {
      const formats = supportedFormats();
      if (!formats) return;

      const extensions = formats.import.flatMap((f) => f.extensions);
      const selected = await open({
        title: "Importar modelo 3D",
        filters: [
          {
            name: "Modelos 3D",
            extensions,
          },
          ...formats.import.map((f) => ({
            name: f.name,
            extensions: f.extensions,
          })),
        ],
      });

      if (!selected) return;

      const filePath = typeof selected === "string" ? selected : selected[0];
      const name = filePath.split("/").pop() ?? filePath;
      setFileName(name);
      setStatusMessage(`Importando ${name}...`);

      const info = await busy(`Importando ${name}...`, () =>
        invoke<MeshInfo>("import_model", { path: filePath, onProgress: progressChannel() })
      );
      // Modelo nuevo: historial nuevo
      history.clear();
      history.milestone(`Importar ${name}`);

      const data = await fetchMeshData();
      setMeshData(data);
      setMeshLoaded(true);
      setMeshInfo({ vertices: info.num_vertices, faces: info.num_faces, format: info.format });

      setStatusMessage(
        `Modelo cargado: ${info.num_vertices.toLocaleString()} vertices, ${info.num_faces.toLocaleString()} caras (${info.format})`
      );

      // Reset other states
      setAutorigComplete(false);
      setWeightsData(undefined);
      setDiagnostics(undefined);
      setRepairResult(undefined);
      setCanUndoRepair(false);
      setMeshAnalysis(undefined);
      setSubdivideResult(undefined);
      setClips([]);
      setActiveClipId(undefined);
      setFrame(0);
      clearQuadMesh();
      setLastExport(undefined);
      setCanUndoPrintScale(false);

      // Modelo nuevo: el proyecto abierto ya no corresponde
      setProjectPath(undefined);

      // Pipeline: importado; se muestra lo que trae el archivo
      pipeline.markCompleted("import");
      pipeline.setActiveStep("structure");
    } catch (e) {
      console.error("Import error:", e);
      setStatusMessage(`Error: ${e}`);
      setProgress(undefined);
    }
  };

  const handleSkeletonChange = async (presetId: string) => {
    try {
      setSelectedSkeleton(presetId);

      const data = await invoke<TauriSkeletonData>("select_skeleton", { presetId });
      history.milestone(`Plantilla: ${skeletonPresets().find((p) => p.id === presetId)?.name ?? presetId}`);
      setSkeletonData(tauriSkeletonToViewer(data));
      setSkeletonLoaded(true);
      setFitInfo(undefined);
      setAutorigComplete(false);
      setBodyPlan((await invoke<BodyPlan | null>("get_body_plan", { presetId })) ?? undefined);

      const preset = skeletonPresets().find((p) => p.id === presetId);
      if (preset) {
        setStatusMessage(`Esqueleto seleccionado: ${preset.name} (${preset.numBones} huesos)`);
      }

      setAutorigComplete(false);
      setWeightsData(undefined);
      setSkeletonTransform({ ...defaultTransform });
      setBoneEditMode(false);
    } catch (e) {
      console.error("Skeleton error:", e);
      setStatusMessage(`Error: ${e}`);
    }
  };

  const handleAutorig = async () => {
    try {
      setIsProcessing(true);
      setProgress({ value: 0, label: "Preparando..." });
      setStatusMessage("Ejecutando autorig...");

      const config = autorigConfig();

      const onProgress = new Channel<Progress>();
      onProgress.onmessage = (msg) => {
        setProgress({ value: msg.percent, label: msg.message });
      };

      await invoke("run_autorig", {
        config: {
          quality: config.quality,
          diffusion_weight: config.diffusionWeight,
          max_influences: config.maxInfluences,
        },
        onProgress,
      });
      history.milestone("Calcular pesos");

      setProgress({ value: 100, label: "Cargando pesos en el visor..." });
      const viewerWeights = decodeWeights(await invoke<ArrayBuffer>("get_weights_data"));
      setWeightsData(viewerWeights);
      setBoneNames(viewerWeights.boneNames);

      const skelData = await invoke<TauriSkeletonData>("get_skeleton_data");
      setSkeletonData(tauriSkeletonToViewer(skelData));

      setIsProcessing(false);
      setAutorigComplete(true);
      setPaintMirrorLoaded(false);
      setProgress(undefined);
      setStatusMessage(`Autorig completado - ${viewerWeights.numBones} huesos procesados`);

      // Pipeline: mark skeleton as completed
      pipeline.markCompleted("skeleton");
    } catch (e) {
      console.error("Autorig error:", e);
      setStatusMessage(`Error: ${e}`);
      setIsProcessing(false);
      setProgress(undefined);
    }
  };

  // ═══════════════════════════════════════════════════════════════════════════
  // ANIMACIÓN
  // ═══════════════════════════════════════════════════════════════════════════

  const activeClip = () => clips().find((c) => c.id === activeClipId());
  const boneIndex = createMemo(() => new Map((skeletonData()?.bones ?? []).map((b, i) => [b.name, i])));
  /** Hay rig para animar: esqueleto con pesos sobre la malla que se ve */
  const animationReady = () =>
    autorigComplete() && !!weightsData() && !!skeletonData() && weightsData()!.numBones === skeletonData()!.bones.length;
  const animating = () => pipeline.activeStep() === "animate" && animationReady();

  // El visor entra en modo animación en el paso Animar (y rehace el rig solo
  // cuando cambian malla, pesos o esqueleto)
  createEffect(() => {
    const v = viewer();
    if (!v) return;
    const on = animating();
    untrack(() => {
      if (!on) setPlaying(false);
      v.setAnimationMode(on);
      // La primera vez ya hay una animación donde poner keys
      if (on && clips().length === 0) handleNewClip();
    });
  });

  // Pose del cuadro actual
  createEffect(() => {
    const v = viewer();
    if (!v || !animating()) return;
    weightsData();
    v.setPose(samplePose(activeClip(), frame(), boneIndex()));
  });

  // Reproducción en bucle dentro del rango del clip
  createEffect(() => {
    if (!playing()) return;
    let last = performance.now();
    let handle = requestAnimationFrame(function tick(now: number) {
      const clip = untrack(activeClip);
      if (!clip) {
        setPlaying(false);
        return;
      }
      let next = untrack(frame) + ((now - last) / 1000) * clip.fps;
      last = now;
      if (next >= clip.end + 1 || next < clip.start) next = clip.start + Math.max(0, next - clip.end - 1);
      setFrame(next);
      handle = requestAnimationFrame(tick);
    });
    onCleanup(() => {
      cancelAnimationFrame(handle);
      setFrame((f) => Math.round(f));
    });
  });

  /** Filas de la línea de tiempo: articulaciones con algo que girar, en orden de árbol */
  const timelineRows = createMemo<TimelineRow[]>(() => {
    const bones = skeletonData()?.bones ?? [];
    const children = bones.map((_, j) => bones.flatMap((b, i) => (b.parent === j ? [i] : [])));
    const rows: TimelineRow[] = [];
    const visit = (j: number, depth: number) => {
      if (bones[j].parent === null || children[j].length > 0) rows.push({ joint: j, bone: bones[j].name, depth });
      children[j].forEach((c) => visit(c, depth + 1));
    };
    bones.forEach((b, j) => b.parent === null && visit(j, 0));
    return rows;
  });

  const replaceClip = (clip: AnimationClip) => {
    setClips((list) => list.map((c) => (c.id === clip.id ? clip : c)));
  };

  /** Cambio de las keys del clip activo, con deshacer */
  const editClip = async (description: string, change: (clip: AnimationClip) => AnimationClip) => {
    const clip = activeClip();
    if (!clip) return;
    const next = change(clip);
    if (next === clip) return;
    await history.execute(description, { kind: "clip", data: { before: clip, after: next } });
  };

  const handleNewClip = () => {
    const clip = createClip(`Acción ${clips().length + 1}`, activeClip()?.fps ?? 24);
    setClips([...clips(), clip]);
    setActiveClipId(clip.id);
    setKeySelection(new Set<string>());
    setFrame(clip.start);
  };

  const handleDuplicateClip = () => {
    const clip = activeClip();
    if (!clip) return;
    const copy = duplicateClip(clip, `${clip.name} (copia)`);
    setClips([...clips(), copy]);
    setActiveClipId(copy.id);
  };

  const handleDeleteClip = async () => {
    const before = clips();
    const clip = activeClip();
    if (!clip) return;
    const after = before.filter((c) => c.id !== clip.id);
    await history.execute("Borrar animación", {
      kind: "clips",
      data: { before, after, activeBefore: clip.id, activeAfter: after[after.length - 1]?.id },
    });
  };

  /** Keys en el cuadro actual con la pose que muestra el visor */
  const insertKeysFor = (joints: number[]) => {
    const v = viewer();
    const bones = skeletonData()?.bones;
    if (!v || !bones || !animating()) return;
    if (!activeClip()) handleNewClip();
    const entries = joints.flatMap((j) => {
      const pose = v.getJointPose(j);
      return pose && bones[j] ? [{ bone: bones[j].name, ...pose }] : [];
    });
    if (entries.length === 0) return;
    const f = Math.round(frame());
    void editClip(entries.length === 1 ? "Insertar key" : "Insertar keys", (c) =>
      insertKeys(c, f, entries, keyInterpolation())
    );
    setStatusMessage(
      entries.length === 1 ? `Key de ${entries[0].bone} en el cuadro ${f}` : `Keys de ${entries.length} articulaciones en el cuadro ${f}`
    );
  };

  /** I: la articulación seleccionada, o todas si no hay selección */
  const handleInsertKey = () => {
    const selected = viewSettings().selectedBone;
    insertKeysFor(selected >= 0 ? [selected] : timelineRows().map((r) => r.joint));
  };

  const handlePoseEdited = (joint: number) => {
    if (autoKey()) insertKeysFor([joint]);
  };

  const handleDeleteKeys = () => {
    const selection = keySelection();
    if (selection.size === 0) return;
    void editClip("Borrar keys", (c) => deleteKeys(c, selection));
    setKeySelection(new Set<string>());
  };

  const handleMoveKeys = (delta: number) => {
    const selection = keySelection();
    void editClip("Mover keys", (c) => moveKeys(c, selection, delta));
    setKeySelection(new Set([...selection].map((id) => {
      const { bone, frame: f } = parseKeyId(id);
      return keyId(bone, f + delta);
    })));
  };

  const handleKeyInterpolation = (interpolation: KeyInterpolation) => {
    setKeyInterpolation(interpolation);
    const selection = keySelection();
    if (selection.size > 0) void editClip("Interpolación", (c) => setKeysInterpolation(c, selection, interpolation));
  };

  const handleClipRange = (range: { start?: number; end?: number; fps?: number }) => {
    const clip = activeClip();
    if (!clip) return;
    const start = Math.max(0, range.start ?? clip.start);
    const end = Math.max(start, range.end ?? clip.end);
    replaceClip({ ...clip, start, end, fps: range.fps ?? clip.fps });
    setFrame((f) => Math.min(end, Math.max(start, f)));
  };

  const stepFrame = (delta: number) => {
    const clip = activeClip();
    if (!clip) return;
    setPlaying(false);
    setFrame((f) => Math.min(clip.end, Math.max(clip.start, Math.round(f) + delta)));
  };

  const handleExport = async () => {
    try {
      const formats = supportedFormats();
      if (!formats) return;
      const opts = exportOptions();
      const format = formats.export.find((f) => f.id === opts.format);
      if (!format) return;

      const baseName = (fileName() ?? "modelo").replace(/\.[^.]+$/, "");
      const selected = await save({
        title: `Exportar ${format.name}`,
        defaultPath: `${baseName}.${format.extensions[0]}`,
        filters: [{ name: format.name, extensions: format.extensions }],
      });

      if (!selected) return;
      // El diálogo no siempre agrega la extensión
      const path = /\.[^./]+$/.test(selected) ? selected : `${selected}.${format.extensions[0]}`;
      const name = path.split("/").pop();

      const includeRig = exportIncludeRig() && autorigComplete();
      const result = await busy(`Exportando a ${name}...`, () => invoke<ExportResult>("export_model", {
        config: {
          format: opts.format,
          path,
          include_skeleton: includeRig,
          include_weights: includeRig,
          use_retopology: exportUseRetopology() && quadMeshLoaded(),
          draco: opts.draco,
          draco_level: opts.dracoLevel,
          draco_position_bits: opts.highPrecision ? 16 : 14,
          simplify_ratio: opts.simplifyPercent < 100 ? opts.simplifyPercent / 100 : null,
          max_texture_size: opts.maxTextureSize > 0 ? opts.maxTextureSize : null,
          texture_quality: opts.textureQuality > 0 ? opts.textureQuality : null,
          optimize_geometry: opts.cleanGeometry,
          strip_unused: opts.cleanGeometry,
          animations: includeRig ? clipsForExport(clips(), boneIndex()) : null,
          fps: activeClip()?.fps ?? null,
        },
      }));

      setLastExport({ bytes: result.total_bytes, files: result.files_created });
      setStatusMessage(`${result.message}: ${name} (${formatBytes(result.total_bytes)})`);

      // Pipeline: mark export as completed
      pipeline.markCompleted("export");
    } catch (e) {
      console.error("Export error:", e);
      setStatusMessage(`Error: ${e}`);
      setProgress(undefined);
    }
  };

  // ── Orientación ──
  // El backend aplica la transformación a malla, quads, esqueleto y rig;
  // deshacer es aplicar la inversa

  /** Planos candidatos y centro de masa de la malla actual */
  const loadPlacementInfo = async () => {
    const info = await busy("Buscando planos de apoyo...", () => invoke<PlacementInfo>("get_placement_info"));
    setFloorCandidates(info.candidates);
    setCenterOfMass(new THREE.Vector3(...info.center_of_mass));
    return info;
  };

  /** Relee del backend todo lo que se movió */
  const refreshAfterPlacement = async (skeletonReset: boolean) => {
    setMeshData(await fetchMeshData());
    if (quadMeshLoaded()) setQuadMeshData(decodeMesh(await invoke<ArrayBuffer>("get_quad_mesh_data")));
    // Mismos pesos por vértice, pero el visor rehace su malla de colores sobre la nueva
    const weights = weightsData();
    if (weights) setWeightsData({ ...weights });
    if (skeletonReset) {
      setSkeletonData(tauriSkeletonToViewer(await invoke<TauriSkeletonData>("get_skeleton_data")));
      setSkeletonTransform({ ...defaultTransform });
    }
    setCenterOfMass(undefined);
    setFloorCandidates([]);
  };

  const sendPlacement = async (matrix: THREE.Matrix4) => {
    const result = await invoke<{ skeleton_reset: boolean }>("apply_placement", { matrix: matrix.elements });
    await refreshAfterPlacement(result.skeleton_reset);
  };

  const applyPlacement = async (matrix: THREE.Matrix4 | null, description: string) => {
    if (!matrix) return;
    try {
      await history.execute(description, { kind: "placement", data: { matrix: matrix.toArray() } });
      setStatusMessage(description);
    } catch (e) {
      console.error("Placement error:", e);
      setStatusMessage(`Error: ${e}`);
    }
  };

  const handlePlacementMode = async (mode: PlacementMode | undefined) => {
    if (mode === "floor") {
      try {
        await loadPlacementInfo();
      } catch (e) {
        setStatusMessage(`Error: ${e}`);
        return;
      }
    }
    setPlacementMode(mode);
  };

  /** Normal de la zona bajo el clic (radio: 3 % de la diagonal) */
  const surfaceNormal = (pick: Extract<PlacementPick, { kind: "surface" }>, positions: Float32Array) => {
    const data = meshData()!;
    const radius = 0.03 * boundsAfter(positions).getSize(new THREE.Vector3()).length();
    return zoneNormal(positions, data.indices, new THREE.Vector3(...pick.point), radius) ?? new THREE.Vector3(...pick.normal);
  };

  const handlePlacementPick = async (pick: PlacementPick) => {
    const data = meshData();
    const mode = placementMode();
    if (!data || !mode) return;
    setPlacementMode(undefined);
    const positions = data.positions;
    if (mode === "floor") {
      const normal =
        pick.kind === "candidate"
          ? new THREE.Vector3(...floorCandidates()[pick.index].normal)
          : surfaceNormal(pick, positions);
      await applyPlacement(floorMatrix(positions, normal), "Piso elegido: modelo apoyado en la grilla");
    } else if (pick.kind === "surface" && mode === "front") {
      const matrix = frontMatrix(positions, surfaceNormal(pick, positions));
      if (!matrix) {
        setStatusMessage("Esa cara mira hacia arriba o abajo: elige una lateral");
        return;
      }
      await applyPlacement(matrix, "Frente elegido: el modelo mira hacia +Z");
    } else if (pick.kind === "surface" && mode === "point") {
      await applyPlacement(originMatrix(positions, "point", undefined, new THREE.Vector3(...pick.point)), "Origen en el punto elegido");
    }
  };

  const handleRotate = (axis: Axis, degrees: number) => {
    const data = meshData();
    if (data) applyPlacement(rotationMatrix(data.positions, axis, degrees), `Giro de ${degrees}° en ${axis.toUpperCase()}`);
  };

  const handleMirror = (axis: Axis) => {
    const data = meshData();
    if (data) applyPlacement(mirrorMatrix(data.positions, axis), `Espejado en ${axis.toUpperCase()}`);
  };

  const handleScaleModel = (factor: number, description: string) => {
    const data = meshData();
    if (data && factor > 0 && factor !== 1) applyPlacement(scaleMatrix(data.positions, factor), description);
  };

  const handleMoveModel = (offset: [number, number, number]) => {
    const u = gridUnits();
    const shown = offset.map((v) => Number((v * (u.metersPerUnit / u.unitMeters)).toPrecision(5)));
    applyPlacement(translationMatrix(offset), `Mover ${shown.join(", ")} ${u.unitLabel}`);
  };

  const handleDrop = () => {
    const data = meshData();
    if (data) applyPlacement(dropMatrix(data.positions), "Modelo apoyado en el piso");
  };

  const ORIGIN_LABELS: Record<OriginMode, string> = {
    base: "centro de la base",
    box: "centro de la caja",
    mass: "centro de masa",
    point: "punto elegido",
  };

  const handleOrigin = async (mode: OriginMode) => {
    const data = meshData();
    if (!data) return;
    if (mode === "point") {
      setPlacementMode("point");
      return;
    }
    let com: THREE.Vector3 | undefined;
    if (mode === "mass") com = centerOfMass() ?? new THREE.Vector3(...(await loadPlacementInfo()).center_of_mass);
    await applyPlacement(originMatrix(data.positions, mode, com), `Origen en el ${ORIGIN_LABELS[mode]}`);
  };

  const handleUvUnwrap = async () => {
    try {
      setIsProcessing(true);
      setProgress({ value: 0, label: "Desplegando..." });
      setStatusMessage("Desplegando UV y horneando texturas...");
      const onProgress = new Channel<Progress>();
      onProgress.onmessage = (msg) => setProgress({ value: msg.percent, label: msg.message });
      const config = uvConfig();
      const info = await invoke<UvInfo>("run_uv_unwrap", {
        config: { texture_size: config.textureSize, padding: config.padding, max_angle: config.maxAngle },
        onProgress,
      });
      history.milestone("Desplegar UV");
      setProgress({ value: 100, label: "Cargando en el visor..." });
      await refreshSkin();
      setUvPreview("texture");
      setShowQuadMesh(true);
      setExportUseRetopology(true);
      pipeline.markCompleted("uv");
      setStatusMessage(
        `UV desplegadas: ${info.num_charts ?? 0} islas, estiramiento ${info.stretch?.toFixed(3) ?? "--"}` +
          (info.texture_size > 0 ? `, texturas de ${info.texture_size} px` : "")
      );
    } catch (e) {
      console.error("UV unwrap error:", e);
      setStatusMessage(`Error: ${e}`);
    } finally {
      setIsProcessing(false);
      setProgress(undefined);
    }
  };

  const handleUvRestore = async () => {
    try {
      setIsProcessing(true);
      await invoke<UvInfo>("restore_transferred_uvs");
      history.milestone("Volver a las UV trasladadas");
      await refreshSkin();
      setStatusMessage("UV trasladadas del modelo original");
    } catch (e) {
      setStatusMessage(`Error: ${e}`);
    } finally {
      setIsProcessing(false);
    }
  };

  const handleRetopology = async () => {
    try {
      setIsProcessing(true);
      setProgress({ value: 0, label: "Preparando retopologia..." });
      setStatusMessage("Ejecutando QuadriFlow...");

      const config = retopologyConfig();

      const onProgress = new Channel<Progress>();
      onProgress.onmessage = (msg) => {
        setProgress({ value: msg.percent, label: msg.message });
      };

      const info = await invoke<TauriQuadMeshInfo>("run_retopology", {
        config: {
          target_quads: config.targetQuads,
          preserve_sharp: config.preserveSharp,
          sharp_angle: config.sharpAngle,
          smooth_iterations: config.smoothIterations,
          rebuild: config.rebuild,
          curvature_alignment: config.curvatureAlignment,
          adaptive_density: config.adaptiveDensity,
          symmetry: config.symmetry,
          follow_seams: config.followSeams,
        },
        onProgress,
      });
      history.milestone("Retopología");

      setQuadMeshInfo({ vertices: info.num_vertices, quads: info.num_quads });
      setQuadQuality(info.quality);

      setProgress({ value: 100, label: "Cargando en el visor..." });
      const quadData = decodeMesh(await invoke<ArrayBuffer>("get_quad_mesh_data"));
      setQuadMeshData(quadData);
      // Las etapas siguientes usan la malla nueva: el rig anterior se descartó
      dropWeights();
      setActiveQuad(true);
      await refreshSkin();
      setQuadMeshLoaded(true);
      setShowQuadMesh(true);
      // Lo último que se ve en el visor es lo que se exporta
      setExportUseRetopology(true);

      setIsProcessing(false);
      setProgress(undefined);
      const uvNote =
        info.uv_seam_faces === null
          ? ""
          : ` · UV trasladadas (${info.uv_seam_faces.toLocaleString()} caras cruzan costuras)`;
      setStatusMessage(
        `Retopologia completada: ${info.num_vertices.toLocaleString()} vertices, ${info.num_quads.toLocaleString()} quads${uvNote}`
      );

      // Pipeline: mark retopology as completed
      pipeline.markCompleted("retopology");
    } catch (e) {
      console.error("Retopology error:", e);
      setStatusMessage(`Error: ${e}`);
      setIsProcessing(false);
      setProgress(undefined);
    }
  };

  // ═══════════════════════════════════════════════════════════════════════════
  // REPAIR HANDLERS
  // ═══════════════════════════════════════════════════════════════════════════

  const handleAnalyzeMesh = async () => {
    try {
      setIsProcessing(true);
      setStatusMessage("Analizando malla...");
      const config = repairAnalysisConfig();
      const result = await busy("Analizando malla...", () =>
        invoke<MeshDiagnostics>("analyze_mesh", {
          config: {
            check_self_intersections: config.checkSelfIntersections,
          },
        })
      );
      setDiagnostics(result);
      setIsProcessing(false);
      setStatusMessage(
        result.is_healthy
          ? "Malla saludable - no requiere reparación"
          : "Análisis completado - se encontraron problemas"
      );
    } catch (e) {
      console.error("Analyze error:", e);
      setStatusMessage(`Error: ${e}`);
      setIsProcessing(false);
    }
  };

  const handleRepairMesh = async () => {
    try {
      setIsProcessing(true);
      setStatusMessage("Reparando malla...");
      const opts = repairOptions();
      const result = await busy("Reparando malla...", () => invoke<TauriRepairResult>("repair_mesh", {
        onProgress: progressChannel(),
        config: {
          merge_duplicates: opts.mergeDuplicates,
          remove_degenerates: opts.removeDegenerates,
          fix_normals: opts.fixNormals,
          fix_non_manifold: opts.fixNonManifold,
          orient_outward: opts.orientOutward,
          remove_small_components: opts.removeSmallComponents,
          fill_holes: opts.fillHoles,
          max_hole_edges: opts.maxHoleEdges,
          refine_fill: opts.refineFill,
        },
      }));
      history.milestone("Reparar malla");

      setRepairResult(result);
      setDiagnostics(result.new_diagnostics);
      setCanUndoRepair(true);
      // El backend descarta el rig al cambiar la geometría
      setAutorigComplete(false);
      setWeightsData(undefined);
      clearQuadMesh();

      // Refrescar meshData y meshInfo
      const data = await fetchMeshData();
      setMeshData(data);
      setMeshInfo({
        vertices: result.new_mesh_info.num_vertices,
        faces: result.new_mesh_info.num_faces,
        format: meshInfo().format,
      });

      setIsProcessing(false);
      setStatusMessage(
        result.new_diagnostics.is_healthy
          ? "Reparación completada: malla sana"
          : "Reparación completada (revisa el diagnóstico)"
      );
      pipeline.markCompleted("repair");
    } catch (e) {
      console.error("Repair error:", e);
      setStatusMessage(`Error: ${e}`);
      setIsProcessing(false);
    }
  };

  const handleUndoRepair = async () => {
    try {
      setStatusMessage("Deshaciendo reparación...");
      const info = await busy("Deshaciendo reparación...", () => invoke<MeshInfo>("undo_repair"));
      history.milestone("Deshacer reparación");

      const data = await fetchMeshData();
      setMeshData(data);
      setMeshInfo({ vertices: info.num_vertices, faces: info.num_faces, format: meshInfo().format });
      setCanUndoRepair(false);
      setDiagnostics(undefined);
      setRepairResult(undefined);
      setAutorigComplete(false);
      setWeightsData(undefined);
      clearQuadMesh();
      setStatusMessage("Reparación deshecha");
    } catch (e) {
      console.error("Undo repair error:", e);
      setStatusMessage(`Error: ${e}`);
    }
  };

  // ═══════════════════════════════════════════════════════════════════════════
  // PRINT3D HANDLERS
  // ═══════════════════════════════════════════════════════════════════════════

  const handleAnalyzePrint3d = async () => {
    try {
      setIsProcessing(true);
      setStatusMessage("Analizando para impresión 3D...");
      const result = await busy("Analizando para impresión 3D...", () => invoke<TauriPrint3dAnalysis>("analyze_print3d"));
      setMeshAnalysis(result);
      setIsProcessing(false);
      setStatusMessage(
        `Análisis 3D: ${result.dimensions[0].toFixed(1)} x ${result.dimensions[1].toFixed(1)} x ${result.dimensions[2].toFixed(1)} mm`
      );
    } catch (e) {
      console.error("Print3D analyze error:", e);
      setStatusMessage(`Error: ${e}`);
      setIsProcessing(false);
    }
  };

  const handleUndoPrintScale = async () => {
    try {
      setIsProcessing(true);
      const result = await busy("Deshaciendo escala...", () => invoke<TauriPrint3dAnalysis>("undo_print_scale"));
      setMeshAnalysis(result);
      setSubdivideResult(undefined);
      setAutorigComplete(false);
      setWeightsData(undefined);
      clearQuadMesh();
      const data = await fetchMeshData();
      setMeshData(data);
      setCanUndoPrintScale(false);
      setStatusMessage("Escala deshecha");
    } catch (e) {
      console.error("Undo scale error:", e);
      setStatusMessage(`Error: ${e}`);
    } finally {
      setIsProcessing(false);
    }
  };

  const handleScaleForPrint = async (params: ScaleParams) => {
    try {
      setIsProcessing(true);
      setStatusMessage("Escalando malla...");
      const result = await busy("Escalando malla...", () => invoke<TauriPrint3dAnalysis>("scale_mesh_for_print", { params }));
      setMeshAnalysis(result);
      setCanUndoPrintScale(true);
      // El backend descarta el rig al cambiar la geometría
      setAutorigComplete(false);
      setWeightsData(undefined);
      clearQuadMesh();

      // Refrescar meshData
      const data = await fetchMeshData();
      setMeshData(data);

      setIsProcessing(false);
      setStatusMessage(
        `Escalado completado: ${result.dimensions[0].toFixed(1)} x ${result.dimensions[1].toFixed(1)} x ${result.dimensions[2].toFixed(1)} mm`
      );
    } catch (e) {
      console.error("Scale error:", e);
      setStatusMessage(`Error: ${e}`);
      setIsProcessing(false);
    }
  };

  const handleSubdivide = async (config: SubdivideConfig) => {
    try {
      setIsProcessing(true);
      setStatusMessage("Subdividiendo malla...");
      const result = await busy("Subdividiendo malla...", () => invoke<TauriSubdivideResult>("subdivide_mesh", { config }));
      history.milestone("Dividir en piezas");
      setSubdivideResult(result);
      setIsProcessing(false);
      setStatusMessage(`Subdivisión completada: ${result.piece_count} piezas`);
      pipeline.markCompleted("print3d");
    } catch (e) {
      console.error("Subdivide error:", e);
      setStatusMessage(`Error: ${e}`);
      setIsProcessing(false);
    }
  };

  const handleExportPiece = async (index: number) => {
    try {
      const selected = await save({
        title: "Exportar pieza STL",
        filters: [{ name: "STL", extensions: ["stl"] }],
      });
      if (!selected) return;

      setStatusMessage(`Exportando pieza ${index + 1}...`);
      const result = await busy(`Exportando pieza ${index + 1}...`, () =>
        invoke<ExportResult>("export_print3d_piece", {
          pieceIndex: index,
          path: selected,
        })
      );
      setStatusMessage(result.message);
    } catch (e) {
      console.error("Export piece error:", e);
      setStatusMessage(`Error: ${e}`);
    }
  };

  // ═══════════════════════════════════════════════════════════════════════════
  // SKELETON TRANSFORM HANDLERS
  // ═══════════════════════════════════════════════════════════════════════════

  const applyTransform = async (transform: SkeletonTransform) => {
    setSkeletonTransform(transform);
    const data = await invoke<TauriSkeletonData>("transform_skeleton", {
      scale: transform.scale,
      translation: transform.translation,
      rotation: transform.rotation,
    });
    setSkeletonData(tauriSkeletonToViewer(data));
  };

  /** Texto del paso en el historial: qué cambió y a cuánto */
  const describeTransform = (before: SkeletonTransform, after: SkeletonTransform) => {
    const same = (a: number[], b: number[]) => a.every((v, i) => v === b[i]);
    const parts: string[] = [];
    if (after.scale !== before.scale) parts.push(`escala ${after.scale.toFixed(2)}x`);
    if (!same(after.translation, before.translation)) parts.push("posición");
    if (!same(after.rotation, before.rotation)) parts.push(`giro ${after.rotation.map((r) => `${Math.round(r)}°`).join(" ")}`);
    return parts.length > 0 ? `Esqueleto: ${parts.join(" · ")}` : "Transformar esqueleto";
  };

  const handleTransformChange = async (transform: SkeletonTransform) => {
    const prevTransform = { ...skeletonTransform() };
    try {
      await history.execute(describeTransform(prevTransform, transform), {
        kind: "skeletonTransform",
        data: { before: prevTransform, after: transform },
      });
    } catch (e) {
      console.error("Transform error:", e);
      setStatusMessage(`Error: ${e}`);
    }
  };

  /**
   * Se soltó el gizmo del esqueleto entero: se compone con la transformación
   * del panel (la escala multiplica, el giro se aplica antes del que había)
   */
  const handleSkeletonGizmo = (change: { scale: number } | { rotation: [number, number, number, number] }) => {
    const current = skeletonTransform();
    if ("scale" in change) {
      const scale = Math.round(THREE.MathUtils.clamp(current.scale * change.scale, 0.1, 5) * 1000) / 1000;
      void handleTransformChange({ ...current, scale });
      return;
    }
    // El backend gira X, luego Y, luego Z: la matriz es Rz·Ry·Rx (orden "ZYX" de three)
    const toRad = THREE.MathUtils.degToRad;
    const before = new THREE.Quaternion().setFromEuler(
      new THREE.Euler(toRad(current.rotation[0]), toRad(current.rotation[1]), toRad(current.rotation[2]), "ZYX")
    );
    const after = new THREE.Quaternion(...change.rotation).multiply(before);
    const euler = new THREE.Euler().setFromQuaternion(after, "ZYX");
    // Cada giro tiene dos juegos de ángulos: el más parecido a los de antes
    // (girar 100° en Y da 0, 100, 0 y no 180, 80, 180)
    const wrap = (a: number) => Math.atan2(Math.sin(a), Math.cos(a));
    const alt = [wrap(euler.x + Math.PI), wrap(Math.PI - euler.y), wrap(euler.z + Math.PI)];
    const prev = current.rotation.map(toRad);
    const distance = (angles: number[]) => angles.reduce((sum, a, k) => sum + Math.abs(wrap(a - prev[k])), 0);
    const best = distance(alt) < distance([euler.x, euler.y, euler.z]) ? alt : [euler.x, euler.y, euler.z];
    const deg = (r: number) => Math.round(THREE.MathUtils.radToDeg(r) * 100) / 100;
    void handleTransformChange({ ...current, rotation: [deg(best[0]), deg(best[1]), deg(best[2])] });
  };

  /** Cambió un apéndice: se rehace la plantilla */
  const handleBodyPlanChange = async (plan: BodyPlan) => {
    setBodyPlan(plan);
    try {
      const data = await invoke<TauriSkeletonData>("select_body_plan", { plan });
      history.milestone("Esqueleto por forma de cuerpo");
      setSkeletonData(tauriSkeletonToViewer(data));
      setSkeletonTransform({ ...defaultTransform });
      setFitInfo(undefined);
      setAutorigComplete(false);
      setStatusMessage(`Plantilla con ${data.bones.length} huesos: ajústala al modelo`);
    } catch (e) {
      console.error("Body plan error:", e);
      setStatusMessage(`Error: ${e}`);
    }
  };

  const handleAutoFit = async () => {
    try {
      const fit = await busy("Detectando extremidades y ajustando el esqueleto...", () =>
        invoke<TauriAutoFitResult>("auto_fit_skeleton")
      );
      history.milestone("Ajustar esqueleto al modelo");
      setSkeletonData(tauriSkeletonToViewer(fit.skeleton));
      setSkeletonTransform({ ...defaultTransform });
      setAutorigComplete(false);
      setFitInfo({
        quality: fit.quality,
        extremities: fit.extremities,
        unusedExtremities: fit.unused_extremities.length,
      });
      setStatusMessage(
        `Esqueleto ajustado: ${fit.extremities} extremidades, proporciones ${Math.round(fit.quality * 100)} %`
      );
    } catch (e) {
      console.error("Auto-fit error:", e);
      setStatusMessage(`Error: ${e}`);
    }
  };

  const handleResetTransform = async () => {
    const presetId = selectedSkeleton();
    if (presetId) {
      try {
        const data = await invoke<TauriSkeletonData>("select_skeleton", { presetId });
        history.milestone("Resetear esqueleto");
        setSkeletonData(tauriSkeletonToViewer(data));
        setSkeletonTransform({ ...defaultTransform });
        setStatusMessage("Esqueleto reseteado");
      } catch (e) {
        console.error("Reset error:", e);
      }
    }
  };

  // Movimientos de articulaciones en orden, uno a la vez: mientras uno va al
  // backend solo se guarda el último pedido (el arrastre manda muchos)
  let pendingBoneMove: { index: number; position: [number, number, number] } | null = null;
  let boneMoveRunning: Promise<void> | null = null;

  const handleBoneMoved = (index: number, position: [number, number, number]): Promise<void> => {
    pendingBoneMove = { index, position };
    if (!boneMoveRunning) {
      boneMoveRunning = (async () => {
        while (pendingBoneMove) {
          const move = pendingBoneMove;
          pendingBoneMove = null;
          try {
            const data = await invoke<TauriSkeletonData>("move_bone", {
              boneIndex: move.index,
              position: move.position,
              mirror: symmetricEdit(),
            });
            // Si llegó otro pedido, esta respuesta ya está vieja
            if (!pendingBoneMove) setSkeletonData(tauriSkeletonToViewer(data));
          } catch (e) {
            console.error("Move bone error:", e);
          }
        }
        boneMoveRunning = null;
      })();
    }
    return boneMoveRunning;
  };

  /** Fin de un movimiento con G: queda en el historial */
  const handleBoneMoveCommitted = async (
    index: number,
    from: [number, number, number],
    to: [number, number, number]
  ) => {
    setAutorigComplete(false);
    // Ya está aplicado: el visor lo fue mandando durante el arrastre
    await history.execute("Mover articulación", { kind: "moveJoint", data: { index, from, to } }, { applied: true });
  };

  /** Centra en el volumen la articulación seleccionada, o todas */
  const handleCenterBones = async (onlySelected: boolean) => {
    const selected = viewSettings().selectedBone;
    try {
      const data = await busy("Centrando articulaciones...", () =>
        invoke<TauriSkeletonData>("center_bones", { bones: onlySelected && selected >= 0 ? [selected] : null })
      );
      history.milestone(onlySelected ? "Centrar articulación" : "Centrar articulaciones");
      setSkeletonData(tauriSkeletonToViewer(data));
      setSkeletonTransform({ ...defaultTransform });
      setAutorigComplete(false);
      setStatusMessage(onlySelected ? "Articulación centrada en el miembro" : "Articulaciones centradas");
    } catch (e) {
      console.error("Center bones error:", e);
      setStatusMessage(`Error: ${e}`);
    }
  };

  const useTool = (tool: ToolId) => {
    setActiveTool(tool);
    setBoneEditMode(tool === "move" || tool === "rotate" || tool === "scale");
    if (tool === "paint") void startPainting();
  };

  /** Elige la malla de las etapas siguientes (quads u original): el rig se rehace */
  const handleActiveMesh = async (useRetopology: boolean) => {
    try {
      const active = await invoke<boolean>("set_active_mesh", { retopology: useRetopology });
      history.milestone(useRetopology ? "Usar la malla de quads" : "Usar la malla original");
      setActiveQuad(active);
      dropWeights();
      setStatusMessage(
        active
          ? "UV, esqueleto y pesos usan la malla retopologizada"
          : "UV y piel siguen en la retopología; esqueleto y pesos usan la malla original"
      );
    } catch (e) {
      setStatusMessage(`Error: ${e}`);
    }
  };

  /** Pincel: mapa de calor del hueso activo y simetría de vértices */
  const startPainting = async () => {
    if (!autorigComplete()) {
      setStatusMessage("Primero calcula los pesos");
      return;
    }
    setViewSettings((prev) => ({
      ...prev,
      showWeights: true,
      selectedBone: prev.selectedBone >= 0 ? prev.selectedBone : 1,
    }));
    if (!paintMirrorLoaded()) {
      try {
        const buffer = await invoke<ArrayBuffer>("get_weight_mirror");
        const [numVertices, numBones] = new Uint32Array(buffer, 0, 2);
        const vertex = new Uint32Array(buffer, 8, numVertices);
        const bone = new Uint32Array(buffer, 8 + numVertices * 4, numBones);
        viewerRef?.setPaintMirror(vertex, bone);
        setPaintMirrorLoaded(true);
      } catch (e) {
        console.error("Mirror map error:", e);
      }
    }
  };

  /** Pincel activo con el hueso seleccionado (o `undefined`) */
  const paintSettings = (): PaintSettings | undefined => {
    const bone = viewSettings().selectedBone;
    if (activeTool() !== "paint" || !autorigComplete() || bone < 0) return undefined;
    return { ...paintConfig(), bone };
  };

  /** Guarda un trazo del pincel (ya aplicado en el visor) con deshacer */
  const handleWeightsPainted = async (stroke: PaintStroke) => {
    const data = { vertices: Array.from(stroke.vertices), before: Array.from(stroke.before), after: Array.from(stroke.after) };
    try {
      // El visor ya lo tiene aplicado; falta mandarlo al backend
      await sendWeights(data.vertices, data.after);
      await history.execute("Pintar pesos", { kind: "paintWeights", data }, { applied: true });
    } catch (e) {
      console.error("Paint error:", e);
      setStatusMessage(`Error al guardar los pesos: ${e}`);
    }
  };

  const handleBoneSelected = (index: number) => {
    setViewSettings((prev) => ({ ...prev, selectedBone: index }));
  };

  // ═══════════════════════════════════════════════════════════════════════════
  // OUTLINER HANDLERS
  // ═══════════════════════════════════════════════════════════════════════════

  const handleToggleVisibility = (nodeId: string) => {
    switch (nodeId) {
      case "mesh":
        setViewSettings((prev) => ({ ...prev, showMesh: !prev.showMesh }));
        break;
      case "wireframe":
        setViewSettings((prev) => ({ ...prev, showWireframe: !prev.showWireframe }));
        break;
      case "skeleton":
        setViewSettings((prev) => ({ ...prev, showSkeleton: !prev.showSkeleton }));
        break;
      case "weights":
        setViewSettings((prev) => ({ ...prev, showWeights: !prev.showWeights }));
        break;
      case "grid": {
        const newVal = !showGrid();
        setShowGrid(newVal);
        viewerRef?.setGridVisible(newVal);
        break;
      }
      case "quadmesh":
        setShowQuadMesh(!showQuadMesh());
        break;
    }
  };

  /** Sin esqueleto: ni pesos, ni ajuste, ni animaciones */
  const clearSkeletonUi = () => {
    dropWeights();
    setSkeletonData(undefined);
    setSkeletonLoaded(false);
    setSelectedSkeleton(undefined);
    setFitInfo(undefined);
    setBodyPlan(undefined);
    setSkeletonTransform({ ...defaultTransform });
    setBoneEditMode(false);
    setClips([]);
    setActiveClipId(undefined);
    setFrame(0);
    setViewSettings((prev) => ({ ...prev, selectedBone: -1 }));
  };

  /**
   * Borra un objeto desde el Outliner (o el menú del visor). No se puede
   * deshacer: queda como hito en el historial.
   */
  const handleDeleteNode = async (nodeId: string) => {
    const kind = nodeId.startsWith("bone-") ? "skeleton" : nodeId;
    if (!["skeleton", "weights", "quadmesh"].includes(kind)) return;
    try {
      await invoke("remove_object", { kind });
    } catch (e) {
      setStatusMessage(`Error: ${e}`);
      return;
    }
    if (kind === "skeleton") {
      clearSkeletonUi();
      setStatusMessage("Esqueleto borrado");
    } else if (kind === "weights") {
      dropWeights();
      setStatusMessage("Pesos borrados");
    } else {
      // Un rig calculado sobre los quads se queda sin malla
      if (usesQuad()) dropWeights();
      clearQuadMesh();
      setStatusMessage("Retopología borrada");
    }
    // Lo anterior apuntaba a lo borrado: deshacer no pasa de acá
    history.milestone({ skeleton: "Borrar esqueleto", weights: "Borrar pesos", quadmesh: "Borrar retopología" }[kind]!);
  };

  // ═══════════════════════════════════════════════════════════════════════════
  // PROYECTO (.pinocchio)
  // El backend guarda su estado (modelo original, malla, esqueleto, pesos,
  // quads, UV, respaldos para deshacer reparación y escala); acá se suma el de
  // la interfaz como JSON, con el árbol del historial (sus pasos son datos)
  // ═══════════════════════════════════════════════════════════════════════════

  const PROJECT_UI_VERSION = 1;
  const [projectPath, setProjectPath] = createSignal<string | undefined>();
  const [autosave, setAutosave] = createPersisted<AutosaveSettings>("settings.autosave", { enabled: false, minutes: 5 });
  const [settingsOpen, setSettingsOpen] = createSignal(false);
  const [gridSettings, setGridSettings] = createPersisted<GridSettings>("settings.grid", { unit: "auto", modelUnit: "auto" });
  const [themeSetting, setThemeSetting] = createPersisted<ThemeSetting>("settings.theme", "dark");
  createEffect(() => applyTheme(themeSetting()));
  onCleanup(followSystemTheme(themeSetting));

  // La grilla (y las medidas del panel Orientación) se miden en unidades
  // reales: las del archivo, o las que elija el usuario
  const gridUnits = createMemo((): GridUnits => {
    const grid = gridSettings();
    const metersPerUnit =
      grid.modelUnit === "auto" ? (sceneStructure()?.meters_per_unit ?? 1) : UNIT_METERS[grid.modelUnit];
    // Automática: la unidad conocida más cercana a la del modelo
    const unit: LengthUnit =
      grid.unit !== "auto"
        ? grid.unit
        : (Object.keys(UNIT_METERS) as LengthUnit[]).reduce((best, u) =>
            Math.abs(Math.log(UNIT_METERS[u] / metersPerUnit)) < Math.abs(Math.log(UNIT_METERS[best] / metersPerUnit)) ? u : best
          );
    return { metersPerUnit, unitMeters: UNIT_METERS[unit], unitLabel: unit === "in" ? "pulg" : unit };
  });
  createEffect(() => viewer()?.setGridUnits(gridUnits()));

  /** Medidas de la caja del modelo (unidades de la escena) */
  const modelSize = createMemo((): [number, number, number] | undefined => {
    const data = meshData();
    if (!data) return undefined;
    const s = boundsAfter(data.positions).getSize(new THREE.Vector3());
    return [s.x, s.y, s.z];
  });
  const [recovery, setRecovery] = createSignal<RecoveryInfo | undefined>();

  const baseName = (path: string) => path.split(/[\\/]/).pop() ?? path;

  /** Estado de la interfaz que va al proyecto */
  const projectUi = (): string => {
    const { trackpadNavigation: _, ...view } = viewSettings();
    return JSON.stringify({
      version: PROJECT_UI_VERSION,
      fileName: fileName(),
      meshInfo: meshInfo(),
      pipeline: { active: pipeline.activeStep(), completed: [...pipeline.completedSteps()] },
      lights: lights(),
      view,
      showGrid: showGrid(),
      showQuadMesh: showQuadMesh(),
      activeQuad: activeQuad(),
      skeleton: {
        loaded: skeletonLoaded(),
        preset: selectedSkeleton(),
        transform: skeletonTransform(),
        symmetricEdit: symmetricEdit(),
        fitInfo: fitInfo(),
        bodyPlan: bodyPlan(),
      },
      autorig: { config: autorigConfig(), complete: autorigComplete() },
      paintConfig: paintConfig(),
      retopology: { config: retopologyConfig(), loaded: quadMeshLoaded(), info: quadMeshInfo(), quality: quadQuality() },
      uv: { config: uvConfig(), preview: uvPreview() },
      repair: {
        analysisConfig: repairAnalysisConfig(),
        options: repairOptions(),
        diagnostics: diagnostics(),
        result: repairResult(),
        canUndo: canUndoRepair(),
      },
      print3d: { analysis: meshAnalysis(), subdivide: subdivideResult(), canUndoScale: canUndoPrintScale() },
      export: { includeRig: exportIncludeRig(), useRetopology: exportUseRetopology(), options: exportOptions() },
      animation: { clips: clips(), activeClipId: activeClipId(), frame: frame(), autoKey: autoKey(), interpolation: keyInterpolation() },
      history: history.save(),
    });
  };

  /** Pone la interfaz como estaba y relee del backend lo que ve el visor */
  const restoreProjectUi = async (json: string) => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const ui: any = JSON.parse(json || "{}");
    history.load(ui.history);
    setPlacementMode(undefined);
    setBoneEditMode(false);
    setPlaying(false);
    setKeySelection(new Set<string>());
    setLastExport(undefined);
    setPaintMirrorLoaded(false);
    setUvLayout(undefined);
    setSkinTextures({});
    setUvInfo(undefined);

    setFileName(ui.fileName);
    if (ui.meshInfo) setMeshInfo(ui.meshInfo);
    if (ui.lights) setLights(ui.lights);
    setViewSettings((prev) => ({ ...prev, ...ui.view, trackpadNavigation: prev.trackpadNavigation }));
    if (typeof ui.showGrid === "boolean") {
      setShowGrid(ui.showGrid);
      viewerRef?.setGridVisible(ui.showGrid);
    }
    setActiveQuad(ui.activeQuad === true);
    setSelectedSkeleton(ui.skeleton?.preset);
    setSkeletonTransform(ui.skeleton?.transform ?? { ...defaultTransform });
    if (typeof ui.skeleton?.symmetricEdit === "boolean") setSymmetricEdit(ui.skeleton.symmetricEdit);
    setFitInfo(ui.skeleton?.fitInfo);
    setBodyPlan(ui.skeleton?.bodyPlan);
    if (ui.autorig?.config) setAutorigConfig(ui.autorig.config);
    if (ui.paintConfig) setPaintConfig(ui.paintConfig);
    if (ui.retopology?.config) setRetopologyConfig(ui.retopology.config);
    setQuadMeshInfo(ui.retopology?.info ?? { vertices: 0, quads: 0 });
    setQuadQuality(ui.retopology?.quality);
    if (ui.uv?.config) setUvConfig(ui.uv.config);
    if (ui.uv?.preview) setUvPreview(ui.uv.preview);
    if (ui.repair?.analysisConfig) setRepairAnalysisConfig(ui.repair.analysisConfig);
    if (ui.repair?.options) setRepairOptions(ui.repair.options);
    setDiagnostics(ui.repair?.diagnostics);
    setRepairResult(ui.repair?.result);
    setCanUndoRepair(ui.repair?.canUndo === true);
    setMeshAnalysis(ui.print3d?.analysis);
    setSubdivideResult(ui.print3d?.subdivide);
    setCanUndoPrintScale(ui.print3d?.canUndoScale === true);
    if (ui.export) {
      setExportIncludeRig(ui.export.includeRig !== false);
      setExportUseRetopology(ui.export.useRetopology === true);
      if (ui.export.options) setExportOptions(ui.export.options);
    }
    setClips(ui.animation?.clips ?? []);
    setActiveClipId(ui.animation?.activeClipId);
    setFrame(ui.animation?.frame ?? 0);
    if (typeof ui.animation?.autoKey === "boolean") setAutoKey(ui.animation.autoKey);
    if (ui.animation?.interpolation) setKeyInterpolation(ui.animation.interpolation);

    // Lo que dibuja el visor sale del backend
    setMeshData(await fetchMeshData());
    setMeshLoaded(true);
    const hasSkeleton = ui.skeleton?.loaded === true;
    setSkeletonData(hasSkeleton ? tauriSkeletonToViewer(await invoke<TauriSkeletonData>("get_skeleton_data")) : undefined);
    setSkeletonLoaded(hasSkeleton);
    if (ui.retopology?.loaded) {
      setQuadMeshData(decodeMesh(await invoke<ArrayBuffer>("get_quad_mesh_data")));
      setQuadMeshLoaded(true);
      setShowQuadMesh(ui.showQuadMesh === true);
      await refreshSkin();
    } else {
      clearQuadMesh();
    }
    if (ui.autorig?.complete) {
      const weights = decodeWeights(await invoke<ArrayBuffer>("get_weights_data"));
      setWeightsData(weights);
      setBoneNames(weights.boneNames);
      setAutorigComplete(true);
    } else {
      dropWeights();
    }

    pipeline.setCompleted(ui.pipeline?.completed ?? ["import"]);
    pipeline.setActiveStep(ui.pipeline?.active ?? "structure");
  };

  /** Guarda en el archivo del proyecto; sin archivo (o con `as`) lo pregunta */
  const handleSaveProject = async (as = false) => {
    if (!meshLoaded()) {
      setStatusMessage("No hay nada que guardar: importa un modelo primero");
      return;
    }
    let path = as ? undefined : projectPath();
    if (!path) {
      const stem = (fileName() ?? "proyecto").replace(/\.[^.]+$/, "");
      const chosen = await save({
        title: "Guardar proyecto",
        defaultPath: projectPath() ?? `${stem}.pinocchio`,
        filters: [{ name: "Proyecto de Pinocchio", extensions: ["pinocchio"] }],
      });
      if (!chosen) return;
      path = chosen.endsWith(".pinocchio") ? chosen : `${chosen}.pinocchio`;
    }
    try {
      const saved = await busy("Guardando proyecto...", () =>
        invoke<ProjectSaved>("save_project", { path, ui: projectUi(), onlyIfChanged: false })
      );
      setProjectPath(path);
      setStatusMessage(`Proyecto guardado: ${baseName(path!)} (${(saved.bytes / 1e6).toFixed(1)} MB)`);
    } catch (e) {
      console.error("Save project error:", e);
      setStatusMessage(`Error al guardar: ${e}`);
    }
  };

  /** Abre un proyecto (`path`) o pregunta cuál; `recovered` = viene del archivo de recuperación */
  const handleOpenProject = async (path?: string, recovered = false) => {
    if (meshLoaded()) {
      const ok = await ask("Lo que no esté guardado del trabajo actual se pierde. ¿Abrir otro proyecto?", {
        title: "Abrir proyecto",
        kind: "warning",
      });
      if (!ok) return;
    }
    if (!path) {
      const chosen = await open({
        title: "Abrir proyecto",
        filters: [{ name: "Proyecto de Pinocchio", extensions: ["pinocchio"] }],
      });
      if (!chosen) return;
      path = typeof chosen === "string" ? chosen : chosen[0];
    }
    try {
      const opened = await busy("Abriendo proyecto...", () => invoke<ProjectOpened>("open_project", { path }));
      await restoreProjectUi(opened.ui);
      // La recuperación no es el archivo del usuario: el próximo Guardar pregunta dónde
      setProjectPath(recovered ? undefined : path);
      setRecovery(undefined);
      setStatusMessage(recovered ? "Sesión recuperada" : `Proyecto abierto: ${baseName(path)}`);
    } catch (e) {
      console.error("Open project error:", e);
      setStatusMessage(`Error al abrir: ${e}`);
    }
  };

  /** Descarta todo lo hecho sobre el modelo y vuelve al archivo importado */
  const handleRevertToOriginal = async () => {
    const ok = await ask("Se descarta todo lo hecho sobre el modelo: reparación, esqueleto, pesos, retopología, UV y animaciones.", {
      title: "Volver al modelo original",
      kind: "warning",
    });
    if (!ok) return;
    try {
      await invoke("revert_to_original");
      clearSkeletonUi();
      clearQuadMesh();
      setDiagnostics(undefined);
      setRepairResult(undefined);
      setCanUndoRepair(false);
      setMeshAnalysis(undefined);
      setSubdivideResult(undefined);
      setCanUndoPrintScale(false);
      setLastExport(undefined);
      history.milestone("Volver al modelo original");
      setMeshData(await fetchMeshData());
      setStatusMessage("Modelo original restaurado");
    } catch (e) {
      setStatusMessage(`Error: ${e}`);
    }
  };

  const fileMenuItems = (): MenuEntry[] => [
    { label: "Abrir proyecto…", shortcut: "Ctrl+O", onSelect: () => handleOpenProject() },
    { label: "Guardar", shortcut: "Ctrl+S", disabled: !meshLoaded(), onSelect: () => handleSaveProject() },
    { label: "Guardar como…", shortcut: "Ctrl+Shift+S", disabled: !meshLoaded(), onSelect: () => handleSaveProject(true) },
    { separator: true },
    { label: "Importar modelo…", shortcut: "Ctrl+I", onSelect: () => handleLoad() },
    { label: "Exportar…", disabled: !meshLoaded(), onSelect: () => pipeline.setActiveStep("export") },
    { label: "Volver al modelo original…", disabled: !meshLoaded(), onSelect: () => handleRevertToOriginal() },
    { separator: true },
    { label: "Configuración…", onSelect: () => setSettingsOpen(true) },
  ];

  // Guardado automático: al archivo del proyecto, o al de recuperación si aún no tiene
  let lastAutosave = Date.now();
  let autosaving = false;
  const autosaveTimer = setInterval(async () => {
    const settings = autosave();
    if (!settings.enabled || autosaving || !meshLoaded() || isProcessing() || progress()) return;
    if (Date.now() - lastAutosave < settings.minutes * 60_000) return;
    autosaving = true;
    lastAutosave = Date.now();
    try {
      const path = projectPath() ?? (await invoke<string>("recovery_project_path"));
      const saved = await invoke<ProjectSaved>("save_project", { path, ui: projectUi(), onlyIfChanged: true });
      if (saved.written) {
        const time = new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
        setStatusMessage(`Guardado automático ${projectPath() ? "" : "(recuperación) "}a las ${time}`);
      }
    } catch (e) {
      console.error("Autosave error:", e);
    } finally {
      autosaving = false;
    }
  }, 15_000);
  onCleanup(() => clearInterval(autosaveTimer));

  // Al abrir la app sin modelo: ofrecer la última recuperación
  onMount(async () => {
    try {
      setRecovery((await invoke<RecoveryInfo | null>("recovery_info")) ?? undefined);
    } catch {
      // Sin backend (vista previa) no hay recuperación
    }
  });

  /** Ítems propios del menú del clic derecho en el visor */
  const viewportMenuItems = (): MenuEntry[] => {
    const view = viewSettings();
    const toggle = (key: "showMesh" | "showWireframe" | "showSkeleton" | "showWeights") =>
      setViewSettings((prev) => ({ ...prev, [key]: !prev[key] }));
    const items: MenuEntry[] = [
      { header: "Mostrar" },
      { label: "Malla", checked: view.showMesh, onSelect: () => toggle("showMesh") },
      { label: "Wireframe", checked: view.showWireframe, onSelect: () => toggle("showWireframe") },
      { label: "Grilla", checked: showGrid(), onSelect: () => handleToggleVisibility("grid") },
    ];
    if (skeletonLoaded()) items.push({ label: "Esqueleto", checked: view.showSkeleton, onSelect: () => toggle("showSkeleton") });
    if (autorigComplete()) items.push({ label: "Heatmap de pesos", checked: view.showWeights, onSelect: () => toggle("showWeights") });
    const removable: MenuEntry[] = [];
    if (skeletonLoaded()) removable.push({ label: "Borrar esqueleto", danger: true, onSelect: () => handleDeleteNode("skeleton") });
    if (quadMeshLoaded()) removable.push({ label: "Borrar retopología", danger: true, onSelect: () => handleDeleteNode("quadmesh") });
    if (removable.length > 0) items.push({ separator: true }, ...removable);
    return items;
  };

  const handleSelectNode = (nodeId: string) => {
    if (nodeId.startsWith("bone-")) {
      const index = parseInt(nodeId.replace("bone-", ""));
      setViewSettings((prev) => ({ ...prev, selectedBone: index }));
      viewerRef?.selectBone(index);
    }
  };

  // ═══════════════════════════════════════════════════════════════════════════
  // RENDER
  // ═══════════════════════════════════════════════════════════════════════════

  return (
    <div class="flex flex-col h-full w-full overflow-hidden bg-bg-darker p-2.5">
      <div class="flex flex-col flex-1 min-h-0 overflow-hidden rounded-lg border border-border bg-bg">
        {/* Header */}
        <Header
          title="Pinocchio"
          fps={fps()}
          fileName={projectPath() ? baseName(projectPath()!) : fileName()}
          fileMenu={fileMenuItems}
          onOpenSettings={() => setSettingsOpen(true)}
          workspace={pipeline.workspace()?.id}
          onWorkspace={pipeline.openWorkspace}
          exporting={pipeline.activeStep() === "export"}
          onExport={() => pipeline.setActiveStep("export")}
          hasModel={meshLoaded()}
        />

        {/* Main Content */}
        <div class="flex flex-1 min-h-0 overflow-hidden">
          {/* Toolbar */}
          <Toolbar
            activeTool={activeTool()}
            onToolChange={(tool) => useTool(tool)}
            onResetView={() => viewerRef?.resetView()}
            canUndo={history.canUndo()}
            canRedo={history.canRedo()}
            onUndo={() => history.undo()}
            onRedo={() => history.redo()}
          />

          {/* Viewport y, al animar, la línea de tiempo debajo */}
          <div class="flex flex-col flex-1 min-w-0 min-h-0">
          <div class="relative flex-1 min-w-0 min-h-0">
            <Viewport
              onViewerReady={handleViewerReady}
              onFpsUpdate={setFps}
              onPlacementPick={handlePlacementPick}
              onBoneSelected={handleBoneSelected}
              onBoneMoved={handleBoneMoved}
              onBoneMoveCommitted={handleBoneMoveCommitted}
              onSkeletonTransformed={handleSkeletonGizmo}
              onPoseEdited={handlePoseEdited}
              onPaintSettingsChanged={(change) => setPaintConfig((prev) => ({ ...prev, ...change }))}
              onWeightsPainted={handleWeightsPainted}
              paintSettings={paintSettings()}
              meshData={displayQuad() ? quadMeshData() : meshData()}
              sceneMaterials={sceneMaterials()}
              lights={lights()}
              onLightsChanged={setLights}
              textures={viewerTextures()}
              skeletonData={skeletonData()}
              weightsData={weightsData()}
              settings={viewSettings()}
              vertices={displayQuad() ? quadMeshInfo().vertices : (meshLoaded() ? meshInfo().vertices : undefined)}
              faces={displayQuad() ? quadMeshInfo().quads : (meshLoaded() ? meshInfo().faces : undefined)}
              placementMode={placementMode()}
              floorCandidates={floorCandidates()}
              boneEditMode={boneEditMode()}
              activeTool={activeTool()}
              contextMenuItems={viewportMenuItems}
            />

            {/* Welcome Screen overlay */}
            <Show when={!meshLoaded()}>
              <WelcomeScreen
                onImport={handleLoad}
                onOpenProject={() => handleOpenProject()}
                recovery={recovery()}
                onRecover={() => handleOpenProject(recovery()!.path, true)}
              />
            </Show>

            <ProgressOverlay progress={progress()} />

            {/* Con el panel derecho oculto, una pestaña para traerlo de vuelta */}
            <Show when={!showContextPanel()}>
              <button
                class="absolute top-1/2 -translate-y-1/2 right-0 z-10 w-5 h-10 flex items-center justify-center rounded-l-md bg-bg-lighter border border-r-0 border-border text-text-muted hover:text-text"
                onClick={() => setShowContextPanel(true)}
                title="Mostrar panel (N)"
                aria-label="Mostrar panel"
              >
                <Icons.CaretRight size={12} class="rotate-180" />
              </button>
            </Show>
          </div>
          <Show when={animating() && activeClip()}>
            {(clip) => (
              <Timeline
                clip={clip()}
                frame={frame()}
                playing={playing()}
                rows={timelineRows()}
                selectedJoint={viewSettings().selectedBone}
                selection={keySelection()}
                autoKey={autoKey()}
                interpolation={keyInterpolation()}
                onFrame={(f) => {
                  setPlaying(false);
                  setFrame(f);
                }}
                onTogglePlay={() => setPlaying(!playing())}
                onRangeChange={handleClipRange}
                onSelectJoint={(joint) => {
                  setViewSettings((prev) => ({ ...prev, selectedBone: joint }));
                  viewerRef?.selectBone(joint);
                }}
                onSelection={setKeySelection}
                onMoveKeys={handleMoveKeys}
                onDeleteKeys={handleDeleteKeys}
                onInsertKey={handleInsertKey}
                onAutoKey={setAutoKey}
                onInterpolation={handleKeyInterpolation}
              />
            )}
          </Show>
          </div>

          {/* Context Panel (right sidebar) */}
          <Show when={showContextPanel()}>
          <ContextPanel
            activeStep={pipeline.activeStep()}
            structureProps={{
              structure: sceneStructure(),
              fileName: fileName(),
              format: meshInfo().format,
              textureUrls: textureUrls(),
            }}
            lights={lights()}
            onLightsChange={setLights}
            onSection={pipeline.setActiveStep}
            objectProps={{
              meshInfo: meshLoaded() ? meshInfo() : undefined,
              placement: {
                mode: placementMode(),
                onPickMode: handlePlacementMode,
                onRotate: handleRotate,
                onMirror: handleMirror,
                onDrop: handleDrop,
                onOrigin: handleOrigin,
                onScale: handleScaleModel,
                onMove: handleMoveModel,
                size: modelSize(),
                unit: { label: gridUnits().unitLabel, perSceneUnit: gridUnits().metersPerUnit / gridUnits().unitMeters },
                disabled: isProcessing(),
              },
            }}
            repairProps={{
              onAnalyze: handleAnalyzeMesh,
              onRepair: handleRepairMesh,
              onUndo: handleUndoRepair,
              diagnostics: diagnostics(),
              repairResult: repairResult(),
              canAnalyze: meshLoaded(),
              canRepair: meshLoaded() && !!diagnostics()?.needs_repair,
              canUndo: canUndoRepair(),
              isProcessing: isProcessing(),
              analysisConfig: repairAnalysisConfig(),
              onAnalysisConfigChange: setRepairAnalysisConfig,
              repairOptions: repairOptions(),
              onRepairOptionsChange: setRepairOptions,
            }}
            retopologyProps={{
              config: retopologyConfig(),
              onChange: setRetopologyConfig,
              onExecute: handleRetopology,
              canExecute: meshLoaded(),
              isProcessing: isProcessing(),
              hasResult: quadMeshLoaded(),
              showQuadMesh: showQuadMesh(),
              onShowQuadMeshChange: setShowQuadMesh,
              useForNextSteps: activeQuad(),
              onUseForNextStepsChange: handleActiveMesh,
              quality: quadQuality(),
            }}
            uvProps={{
              config: uvConfig(),
              onChange: setUvConfig,
              hasRetopology: quadMeshLoaded(),
              info: uvInfo(),
              isProcessing: isProcessing(),
              onUnwrap: handleUvUnwrap,
              onRestore: handleUvRestore,
              preview: uvPreview(),
              onPreviewChange: handleUvPreview,
              layout: uvLayout(),
              atlasImage: skinTextures().base,
            }}
            skeletonProps={{
              presets: skeletonPresets(),
              selectedPreset: selectedSkeleton(),
              onPresetChange: handleSkeletonChange,
              autorigConfig: autorigConfig(),
              onAutorigConfigChange: setAutorigConfig,
              onAutorig: handleAutorig,
              canAutorig: meshLoaded() && skeletonLoaded(),
              isProcessing: isProcessing(),
              autorigComplete: autorigComplete(),
              numBones: boneNames().length || undefined,
              skeletonTransform: skeletonTransform(),
              onTransformChange: handleTransformChange,
              onAutoFit: handleAutoFit,
              fitInfo: fitInfo(),
              bodyPlan: bodyPlan(),
              onBodyPlanChange: handleBodyPlanChange,
              onResetTransform: handleResetTransform,
              editing: boneEditMode() && activeTool() === "move",
              onEdit: () => useTool("move"),
              symmetric: symmetricEdit(),
              onSymmetricChange: setSymmetricEdit,
              selectedBoneName: skeletonData()?.bones[viewSettings().selectedBone]?.name,
              onCenterSelected: () => handleCenterBones(true),
              onCenterAll: () => handleCenterBones(false),
              painting: activeTool() === "paint",
              onPaint: () => useTool("paint"),
              paintConfig: paintConfig(),
              onPaintConfigChange: setPaintConfig,
              boneNames: skeletonData()?.bones.map((b) => b.name) ?? [],
              selectedBone: viewSettings().selectedBone,
              onSelectBone: (index: number) => {
                setViewSettings((prev) => ({ ...prev, selectedBone: index, showWeights: true }));
                viewerRef?.selectBone(index);
              },
              posing: boneEditMode() && activeTool() === "rotate",
              onPose: () => useTool("rotate"),
              onResetPose: () => viewerRef?.resetPose(),
            }}
            print3dProps={{
              onAnalyze: handleAnalyzePrint3d,
              onScale: handleScaleForPrint,
              onUndoScale: handleUndoPrintScale,
              canUndoScale: canUndoPrintScale(),
              onSubdivide: handleSubdivide,
              onExportPiece: handleExportPiece,
              analysis: meshAnalysis(),
              subdivideResult: subdivideResult(),
              canAnalyze: meshLoaded(),
              isProcessing: isProcessing(),
            }}
            animateProps={{
              ready: animationReady(),
              clips: clips(),
              activeClipId: activeClipId(),
              onSelectClip: (id: string) => {
                setActiveClipId(id);
                setKeySelection(new Set<string>());
              },
              onNewClip: handleNewClip,
              onDuplicateClip: handleDuplicateClip,
              onDeleteClip: handleDeleteClip,
              onRenameClip: (name: string) => {
                const clip = activeClip();
                if (clip) replaceClip({ ...clip, name });
              },
              selectedBoneName: skeletonData()?.bones[viewSettings().selectedBone]?.name,
            }}
            exportProps={{
              onExport: handleExport,
              canExport: meshLoaded(),
              autorigComplete: autorigComplete(),
              hasQuadMesh: quadMeshLoaded(),
              includeRig: exportIncludeRig(),
              onIncludeRigChange: setExportIncludeRig,
              useRetopology: exportUseRetopology(),
              onUseRetopologyChange: setExportUseRetopology,
              options: exportOptions(),
              onOptionsChange: setExportOptions,
              lastExport: lastExport(),
            }}
            viewSettings={viewSettings()}
            onViewSettingsChange={(settings) => {
              setViewSettings(settings);
              setTrackpadNavigation(settings.trackpadNavigation === true);
            }}
            boneNames={boneNames()}
            hasWeights={autorigComplete()}
            sceneTree={meshLoaded() ? sceneTree() : undefined}
            onToggleVisibility={handleToggleVisibility}
            onSelectNode={handleSelectNode}
            onDeleteNode={handleDeleteNode}
            history={history}
            stats={{
              fileName: fileName(),
              format: meshInfo().format,
              vertices: meshLoaded() ? meshInfo().vertices : undefined,
              faces: meshLoaded() ? meshInfo().faces : undefined,
              quads: quadMeshLoaded() ? quadMeshInfo().quads : undefined,
              bones: skeletonData()?.bones.length,
            }}
            shortcuts={shortcutHints}
            onHide={() => setShowContextPanel(false)}
          />
          </Show>
        </div>

        {/* Status Bar */}
        <StatusBar
          message={statusMessage()}
          progress={progress()}
        />
      </div>

      <Show when={settingsOpen()}>
        <SettingsDialog
          autosave={autosave()}
          onAutosaveChange={setAutosave}
          grid={gridSettings()}
          onGridChange={setGridSettings}
          theme={themeSetting()}
          onThemeChange={setThemeSetting}
          fileMetersPerUnit={sceneStructure()?.meters_per_unit}
          projectPath={projectPath()}
          onClose={() => setSettingsOpen(false)}
        />
      </Show>
    </div>
  );
};
