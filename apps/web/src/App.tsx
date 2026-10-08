import { Component, createEffect, createMemo, createSignal, on, onMount, onCleanup, Show, untrack, type JSX } from "solid-js";
import { invoke as rawInvoke, Channel, type InvokeArgs, type InvokeOptions } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open, save } from "@tauri-apps/plugin-dialog";
import { Header, StatusBar, Viewport, ViewportHeader, Toolbar, ProgressOverlay, Timeline, type TimelineRow } from "./components/layout";
import type { OnionSettings, TimelineMode, TimelineTool } from "./components/layout/Timeline";
import type { CurveChannelRow } from "./components/layout/CurveEditor";
import { parseCurveKeyId } from "./components/layout/CurveEditor";
import {
  GROUP_CHANNELS,
  allRefs,
  closeCycle,
  deleteKeyRefs,
  dopeToRefs,
  eulerFilter,
  keyRefId,
  noiseKeys,
  parseKeyRef,
  reduceSelectedKeys,
  retime,
  scaleKeys,
  setEase,
  setHandleMode,
  setInterpolation,
  smoothKeys,
  withMarker,
  withoutMarker,
  type ChannelGroup,
} from "./lib/curves";
import { bakeMixer, emptyMixer, loadMixer, mixKeys, mixerRange, type AnimationLayer, type Mixer } from "./lib/layers";
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
  type TransformSpace,
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
  type FloorCandidate,
  type OriginMode,
  type PlacementInfo,
} from "./lib/placement";
import type { SceneStructure, MaterialInfo } from "./components/steps/StructureStep";
import { createPipelineStore, type PipelineStepId } from "./lib/pipeline";
import { VIEW_AXES, fromView, toView, viewSize, type Axis } from "./lib/axes";
import { buildSceneTree, type CadOutline, type CadOutlineItem, type ObjectRow, type SceneNode } from "./lib/scene-tree";
import { isCadObject, isGeometryStep, materialized, nextObjectId, partKey, savedTrail, syncCadObjects, type SceneObject } from "./lib/objects";
import { TOOLSETS, toolContext, type ToolId } from "./lib/tools";
import { createHistoryStore, type SavedHistory } from "./lib/history";
import { createShortcutManager, type ShortcutDef } from "./lib/shortcuts";
import { decodeMesh, decodeWeights } from "./lib/buffers";
import { createPersisted, startDrag } from "./lib/ui-state";
import { PlaybackBar, type PlaybackSegment } from "./components/layout/PlaybackBar";
import { TextureEditor, TEXTURE_SLOTS, type TextureExportKind, type TextureSelection } from "./components/layout/TextureEditor";
import { applyTheme, followSystemTheme, type ThemeSetting } from "./lib/theme";
import type { MenuEntry } from "./components/ui/ContextMenu";
import {
  SettingsDialog,
  UNIT_METERS,
  type AutosaveSettings,
  type GridSettings,
  type LengthUnit,
} from "./components/layout/SettingsDialog";
import { RetargetDialog, type RetargetRequest } from "./components/layout/RetargetDialog";
import { CaptureDialog } from "./components/layout/CaptureDialog";
import { LivePuppet } from "./components/layout/LivePuppet";
import type { CaptureMotion } from "./lib/capture";
import { ragdollClip } from "./lib/ragdoll";
import type { NodeTransform } from "./components/panels/ObjectTab";

/** Edición de un nodo del archivo desde el Outliner (el backend guarda lo anterior para deshacer) */
type SceneEdit =
  | { op: "remove"; node: number; name: string }
  | { op: "transform"; node: number; name: string; before: NodeTransform; after: NodeTransform };
import { bvhMotion, parseBvh } from "./lib/bvh";
import { autoMap, retargetClip, type RetargetMap, type SourceMotion } from "./lib/retarget";
import { ConfirmDialog, type ConfirmRequest } from "./components/layout/ConfirmDialog";

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
  clonePose,
  emptyPose,
  insertKeys,
  insertScalarKey,
  samplePose,
  sampleScalar,
  setTracksMuted,
  type ScalarChannel,
  keyId,
  moveKeys,
  parseKeyId,
  setKeysInterpolation,
  type AnimationClip,
  type KeyInterpolation,
  type Pose,
  type Quat,
  type TrackRef,
} from "./lib/animation";
import {
  applyRotationLocks,
  bakeClip,
  constrainer,
  bodyAxes,
  controlWorld,
  runPoseStack,
  chainBlend,
  tidyClips,
  type IkChain,
  type IkSolver,
  boneColor as rigBoneColor,
  boneProps,
  createRigContext,
  emptyRigSettings,
  eulerDegrees,
  fromJointSpace,
  toJointSpace,
  axisAngle,
  evaluatePose,
  fromEulerDegrees,
  isBoneHidden,
  loadRigSettings,
  newRigId,
  reflectVector,
  rollToward,
  vec,
  withAutoGroups,
  withBoneProps,
  type BoneProps,
  type RigControl,
  type RigSettings,
} from "./lib/rig";
import { RigPanel, type RollMode } from "./components/panels/RigPanel";
import { PosePanel, type PoseSource, type SelectCommand } from "./components/panels/PosePanel";
import { LibraryPanel } from "./components/panels/LibraryPanel";
import { IkPanel } from "./components/panels/IkPanel";
import { ConstraintPanel } from "./components/panels/ConstraintPanel";
import { SkeletonEditPanel } from "./components/panels/SkeletonEditPanel";
import {
  addChildBone,
  copySide,
  moveJoints,
  removeBone,
  rotateLimb,
  sideOf,
  skeletonFromJson,
  skeletonToJson,
  stretchBone,
  type EditBone,
  type JointEditContext,
} from "./lib/skeletonEdit";
import { SPRING_PRESETS } from "./lib/secondary";
import {
  CONSTRAINT_PREFIX,
  CONSTRAINT_TYPES,
  constraintInfluence,
  driverValue,
  isConstraintId,
  type ConstraintType,
  type RigConstraint,
} from "./lib/constraints";
import { RigEditor, type RigEditorTab } from "./components/layout/RigEditor";
import { JointPanel, type ChainView, type LimitsAuto, type RelationEdge, type RelationNode, type TrajectorySample } from "./components/panels/JointPanel";
import {
  anatomicalLimits,
  boundaryRadius,
  compose,
  decompose,
  defaultLimits,
  hingeAngle,
  hingeRotation,
  limitExcess,
  meshLimits,
  mirrorLimits,
  observedLimits,
  rangePath,
  scaleLimits,
  type DiskPoint,
  type JointLimits,
} from "./lib/jointLimits";
import { autoRig } from "./lib/autoRig";
import { Fk } from "./lib/ik";
import {
  animatableJoints,
  blendPose,
  breakdownPose,
  copyPose,
  loadPoseLibrary,
  mirrorEntries,
  mirrorPose,
  namesToPose,
  poseToNames,
  pushRelax,
  readPoseClipboard,
  resetPose,
  type StoredPose,
} from "./lib/poseTools";
import {
  analyzeBody,
  availableAnimations,
  availablePoses,
  generateAnimation,
  generatePose,
  type PresetAnimationId,
  type SkeletonBone,
} from "./lib/presetAnimations";
import type { SkeletonTransform } from "./components/panels/SkeletonTransformPanel";
import type { MeshDiagnostics, RepairResult, RepairAnalysisConfig, RepairOptions } from "./components/panels/RepairPanel";
import type { MeshAnalysis, SubdivideResult, ScaleParams, SubdivideConfig } from "./components/panels/Print3DPanel";
import { DESIGN_FORMATS, SKELETON_FORMATS, defaultExportOptions, formatBytes, type ExportOptions } from "./components/steps/ExportStep";
import { defaultUvConfig, type UvConfig, type UvInfo, type UvPreview } from "./components/steps/UvStep";
import type { SkeletonFitInfo } from "./components/steps/SkeletonStep";
import { REMESH_MODES, type RemeshMode, type RemeshStats } from "./components/steps/RemeshStep";
import { DEFAULT_SIMPLIFY, simplifyParams, type SimplifyConfig } from "./components/panels/SimplifyPanel";
import { DEFAULT_SMOOTH, smoothParams, type SmoothConfig } from "./components/panels/SmoothPanel";
import type { PreviewView } from "./components/layout/ViewportHeader";
import { ScanEditor, type ScanEditorTab, type ScanMeshSettings } from "./components/layout/ScanEditor";
import { CadView } from "./components/layout/CadView";
import { DesignStep, FeatureTree } from "./components/steps/DesignStep";
import { createCadStore, partColor, partHidden, PLANE_LABELS, type PartId } from "./lib/cad";
import { createCadUi } from "./lib/cadUi";
import { createDesignActions } from "./lib/designActions";
import { designHints } from "./lib/designHints";
import { createScanCloud } from "./lib/scanCloud";
import type { BodyPlan, BodyShape } from "./lib/bodyPlan";
import { BodyBuilder } from "./components/layout/BodyBuilder";
import { recordTemplateUse } from "./lib/templateUsage";
import { TemplateGallery } from "./components/layout/TemplateGallery";
import type { TemplateShape } from "./lib/skeletonThumb";
import { SkeletonEditor, type SkeletonEditorTab } from "./components/layout/SkeletonEditor";
import { SkeletonFitTab, SkeletonWeightsTab, type SkeletonStepProps } from "./components/steps/SkeletonStep";
import { JointTunePanel, type TuneDirection, type TuneRotation } from "./components/panels/JointTunePanel";

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
  /** Esqueleto con pesos y animaciones que trae el archivo (glTF con skin) */
  rig: { num_bones: number; clips: Omit<AnimationClip, "id">[] } | null;
  /** Texturas sueltas puestas en los materiales (`normales: x.png → Casco`) */
  textures?: string[];
  /** Texturas elegidas que no se usaron y por qué */
  textures_skipped?: string[];
}

/** Archivos que acompañan a un modelo al importarlo: mapas de textura y el MTL de un OBJ */
const TEXTURE_FILE_EXTENSIONS = ["png", "jpg", "jpeg", "webp", "tga", "bmp", "mtl"];

interface TauriSkeletonPreset {
  id: string;
  name: string;
  description: string;
  num_bones: number;
  category: string;
  shape: string | null;
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
  /** Había rig y pasó a la malla reparada */
  rig_kept: boolean;
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
  /** Había rig y pasó a la malla nueva (mismo esqueleto, pesos trasladados) */
  rig_kept: boolean;
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

/** Comando del backend que cambia la geometría, grabado para rehacerlo sobre una malla nueva */
interface RecordedCall {
  cmd: string;
  args?: unknown;
}

/**
 * Comandos que cambian la malla del objeto activo. Si la pieza del diseño de
 * la que sale cambia, se vuelven a correr en orden sobre la malla nueva (el
 * esqueleto se conserva y los pesos se recalculan aparte).
 */
const REPLAYABLE = new Set([
  "repair_mesh",
  "undo_repair",
  "run_retopology",
  "set_active_mesh",
  "run_uv_unwrap",
  "unwrap_original_mesh",
  "undo_unwrap_original",
  "restore_transferred_uvs",
  "scale_mesh_for_print",
  "undo_print_scale",
  "subdivide_mesh",
  "remesh_apply",
]);

/** Pasos deshacibles en curso que graban sus comandos (el último es el que graba) */
const recorders: RecordedCall[][] = [];

/** Argumentos que se pueden guardar: los canales de avance se marcan y se rehacen al repetir */
const recordableArgs = (args?: InvokeArgs) =>
  args === undefined ? undefined : JSON.parse(JSON.stringify(args, (_, v) => (v instanceof Channel ? { __channel: true } : v)));

/** Se rehace: los de `REPLAYABLE` y borrar la retopología (borrar el esqueleto no: se conserva aparte) */
const replayable = (cmd: string, args?: InvokeArgs) =>
  REPLAYABLE.has(cmd) || (cmd === "remove_object" && (args as { kind?: string } | undefined)?.kind === "quadmesh");

/** `invoke` que además graba los comandos de geometría del paso deshacible en curso */
const invoke = <T,>(cmd: string, args?: InvokeArgs, options?: InvokeOptions): Promise<T> => {
  const recorder = recorders[recorders.length - 1];
  if (recorder && replayable(cmd, args)) recorder.push({ cmd, args: recordableArgs(args) });
  return rawInvoke<T>(cmd, args, options);
};

export const App: Component = () => {
  // Pipeline
  const pipeline = createPipelineStore();

  // Diseño CAD: documento propio con su historial (copias del documento)
  const cad = createCadStore();
  const cadUi = createCadUi(cad);
  const designActions = createDesignActions(cad, cadUi);
  // Objetos de la escena (ver lib/objects.ts): el activo es el modelo de siempre
  const [objects, setObjects] = createSignal<SceneObject[]>([]);
  const [activeObjectId, setActiveObjectId] = createSignal<number>();
  const activeObject = () => objects().find((o) => o.id === activeObjectId());
  if (import.meta.env.DEV) Object.assign(window, { __objects: objects, __activeObject: activeObjectId, __viewer: () => viewer() });
  const inDesign = () => pipeline.workspace()?.id === "design";
  onMount(() => void cad.init().catch(() => {}));
  // Dibujando un sketch, el panel muestra lo elegido y sus restricciones
  createEffect(
    on(
      () => !!cadUi.session(),
      (editing) => editing && inDesign() && pipeline.activeStep() !== "design" && pipeline.setActiveStep("design"),
    )
  );
  // Entrar a Diseñar sin diseño crea uno vacío: no hay que "empezar" nada
  let creatingDesign = false;
  createEffect(() => {
    if (!inDesign() || !cad.status()?.available || cad.doc() || creatingDesign) return;
    creatingDesign = true;
    void cad
      .newDesign()
      .catch((e) => cad.setError(String(e)))
      .finally(() => (creatingDesign = false));
  });
  // Salir de Diseñar con el diálogo de una operación abierto lo acepta (si no, quedaría oculto)
  createEffect(() => {
    if (!inDesign() && cad.draft()) void cad.acceptDraft();
  });
  /** El sólido del diseño, si hay */
  const hasDesignBody = () => !!cad.committed() && !!cad.result()?.body;
  // Exportar desde Diseñar (o sin modelo) exporta el sólido del diseño
  const [exportDesign, setExportDesign] = createSignal(false);
  /** Pieza del diseño que se exporta ("operación:índice"); sin ella, todo */
  const [exportPart, setExportPart] = createSignal<string>();
  const designParts = () => {
    const parts: { key: string; name: string; id?: PartId }[] = (cad.result()?.parts ?? []).map((p) => ({ key: `${p.id.feature}:${p.id.index}`, name: p.name, id: p.id }));
    // El ensamble: cada instancia en su lugar (STEP con nombres, 3MF con un objeto por instancia)
    const n = cad.committed()?.assembly?.instances.length ?? 0;
    if (n > 0) parts.push({ key: "assembly", name: `Ensamble (${n} ${n === 1 ? "instancia" : "instancias"})` });
    // Un archivo por variante (nombre-variante.ext)
    const c = cad.committed()?.configurations?.length ?? 0;
    if (c > 0) parts.push({ key: "configs", name: `Todas las configuraciones (${c + 1} archivos)` });
    return parts;
  };
  const openExport = () => {
    const design = hasDesignBody() && (inDesign() || !hasWork());
    setExportDesign(design);
    setExportPart(undefined);
    if (design && !DESIGN_FORMATS.includes(exportOptions().format)) setExportOptions({ ...exportOptions(), format: "step" });
    pipeline.setActiveStep("export");
  };
  /** Visor del diseño: en Diseñar y mientras se exporta el diseño */
  const showCad = () => inDesign() || (exportDesign() && pipeline.activeStep() === "export");

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
    rig: {
      apply: (d: { after: RigSettings }) => void setRigSettings(d.after),
      revert: (d: { before: RigSettings }) => void setRigSettings(d.before),
    },
    mixer: {
      apply: (d: { after: Mixer }) => void setMixer(d.after),
      revert: (d: { before: Mixer }) => void setMixer(d.before),
    },
    poseLibrary: {
      apply: (d: { after: StoredPose[] }) => void setPoseLibrary(d.after),
      revert: (d: { before: StoredPose[] }) => void setPoseLibrary(d.before),
    },
    sceneEdit: {
      apply: (d: SceneEdit) => sendSceneEdit(d),
      revert: (d: SceneEdit) => undoSceneEdit(d),
    },
    skeletonBones: {
      apply: (d: { before: EditBone[]; after: EditBone[] }) => sendSkeletonBones(d.after),
      revert: (d: { before: EditBone[]; after: EditBone[] }) => sendSkeletonBones(d.before),
    },
    restPose: {
      apply: (d: { to: Vec3[] }) => sendRestPose(d.to),
      revert: (d: { from: Vec3[] }) => sendRestPose(d.from),
    },
    placement: {
      apply: (d: { matrix: number[] }) => sendPlacement(new THREE.Matrix4().fromArray(d.matrix)),
      revert: (d: { matrix: number[] }) => sendPlacement(new THREE.Matrix4().fromArray(d.matrix).invert()),
    },
    skeletonTransform: {
      apply: (d: { before: SkeletonTransform; after: SkeletonTransform }) => applyTransform(d.after),
      revert: (d: { before: SkeletonTransform; after: SkeletonTransform }) => applyTransform(d.before),
    },
    jointPositions: {
      apply: (d: { before: Vec3[]; after: Vec3[] }) => sendBonePositions(d.after),
      revert: (d: { before: Vec3[]; after: Vec3[] }) => sendBonePositions(d.before),
    },
    moveJoint: {
      apply: (d: { index: number; from: Vec3; to: Vec3 }) => handleBoneMoved(d.index, d.to),
      revert: (d: { index: number; from: Vec3; to: Vec3 }) => handleBoneMoved(d.index, d.from),
    },
    // Operaciones del backend (reparar, retopología, pesos, UV…): se
    // deshacen intercambiando el estado con una copia completa
    snapshot: {
      apply: (d: { id: number }) => swapSnapshot(d.id, "after"),
      revert: (d: { id: number }) => swapSnapshot(d.id, "before"),
    },
    paintWeights: {
      // Primero el backend: si falla, el visor no muestra pesos que no quedaron
      apply: async (d: { vertices: number[]; before: number[]; after: number[] }) => {
        await sendWeights(d.vertices, d.after);
        viewerRef?.applyWeightRows(new Uint32Array(d.vertices), new Float32Array(d.after));
      },
      revert: async (d: { vertices: number[]; before: number[]; after: number[] }) => {
        await sendWeights(d.vertices, d.before);
        viewerRef?.applyWeightRows(new Uint32Array(d.vertices), new Float32Array(d.before));
      },
    },
  });

  // State
  const [fps, setFps] = createSignal(0);
  const [statusMessage, setStatusMessage] = createSignal("Listo - Importa un modelo para comenzar");
  // En Diseñar la barra muestra atajos; los mensajes nuevos se ven unos segundos a la derecha
  const [freshStatus, setFreshStatus] = createSignal(false);
  let freshTimer: ReturnType<typeof setTimeout> | undefined;
  createEffect(
    on(
      statusMessage,
      () => {
        clearTimeout(freshTimer);
        setFreshStatus(true);
        freshTimer = setTimeout(() => setFreshStatus(false), 6000);
      },
      { defer: true },
    ),
  );
  // value ausente = progreso indeterminado (solo etiqueta)
  const [progress, setProgress] = createSignal<{ value?: number; label?: string } | undefined>();

  /** Canal de progreso del backend que alimenta la barra */
  const progressChannel = () => {
    const channel = new Channel<Progress>();
    const generation = progressGeneration;
    // Un aviso que llega después de terminar la tarea no vuelve a mostrar la barra
    channel.onmessage = (msg) => {
      if (generation === progressGeneration) setProgress({ value: msg.percent, label: msg.message });
    };
    return channel;
  };
  let progressGeneration = 0;

  /** Muestra la barra mientras dura `task` (indeterminada salvo que un canal informe avance) */
  const busy = async <T,>(label: string, task: () => Promise<T>): Promise<T> => {
    setProgress({ label });
    try {
      return await task();
    } finally {
      progressGeneration++;
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

  /** Materiales para el visor con sus texturas (por índice) ya decodificadas */
  const toSceneMaterials = (materials: MaterialInfo[], images: (ImageBitmap | undefined)[]): SceneMaterial[] => {
    const image = (i: number | null) => (i === null ? undefined : images[i]);
    return materials.map((m) => ({
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
    }));
  };

  /** Estructura, materiales y texturas de la escena (miniaturas e imágenes para el visor) */
  let appearanceRequest = 0;
  const loadSceneAppearance = async () => {
    const request = ++appearanceRequest;
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
      // Otra recarga empezó después: la suya es la que vale
      if (request !== appearanceRequest) {
        images.forEach((image) => image?.close());
        return;
      }
      textureUrls().forEach((url) => URL.revokeObjectURL(url));
      setTextureUrls(blobs.map((b) => URL.createObjectURL(b)));
      setSceneMaterials(toSceneMaterials(materials, images));
      setSceneStructure(structure);
    } catch (e) {
      console.error("Scene structure error:", e);
    }
  };
  const [fileName, setFileName] = createSignal<string | undefined>();

  // Skeleton presets (loaded from Tauri)
  const [skeletonPresets, setSkeletonPresets] = createSignal<SkeletonPreset[]>([]);
  const [templateShapes, setTemplateShapes] = createSignal(new Map<string, TemplateShape>());
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
  // Tareas largas en curso: un contador, así la que termina primero no
  // libera los botones mientras otra sigue corriendo
  const [processingCount, setProcessingCount] = createSignal(0);
  const isProcessing = () => processingCount() > 0;
  const setIsProcessing = (on: boolean) => setProcessingCount((n) => Math.max(0, n + (on ? 1 : -1)));
  /** Abriendo, importando o empezando de cero: el proyecto cambia debajo */
  const [switching, setSwitching] = createSignal(false);
  /** Avisa y devuelve `true` si otra tarea larga está en curso */
  const blockedByTask = () => {
    if (!isProcessing() && !progress() && !switching()) return false;
    setStatusMessage("Hay un proceso en curso: espera a que termine");
    return true;
  };
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
  // Remallar: el modo elegido (la retopología es uno de ellos)
  const [remeshMode, setRemeshMode] = createSignal<RemeshMode>("retopology");
  const [simplifyConfig, setSimplifyConfig] = createSignal<SimplifyConfig>(DEFAULT_SIMPLIFY);
  const [smoothConfig, setSmoothConfig] = createSignal<SmoothConfig>(DEFAULT_SMOOTH);
  /** Vista previa del remallado (el modelo no cambió) o lo último aplicado */
  const [remeshStats, setRemeshStats] = createSignal<{ mode: RemeshMode; kind: "preview" | "applied"; stats: RemeshStats }>();
  const [remeshPreviewData, setRemeshPreviewData] = createSignal<MeshData>();
  /** Con vista previa, qué muestra el visor */
  const [previewView, setPreviewView] = createSignal<PreviewView>("both");
  const [exportIncludeRig, setExportIncludeRig] = createSignal(true);
  /** Esqueleto solo: un octaedro por hueso para verlo en cualquier visor */
  const [exportBoneShapes, setExportBoneShapes] = createSignal(false);
  /** Hay algo con qué trabajar: un modelo o, sin modelo, un esqueleto */
  const hasWork = () => meshLoaded() || !!skeletonData();
  /** Algo que guardar en el proyecto: también un diseño CAD sin modelo */
  const hasProjectWork = () => hasWork() || (cad.doc()?.features.length ?? 0) > 0;
  const [exportUseRetopology, setExportUseRetopology] = createSignal(false);
  /** Compactar el mapa UV al exportar; sin elegir, sí cuando el mapa es para pintar */
  const [exportCompactUv, setExportCompactUv] = createSignal<boolean | undefined>();
  const compactUv = () =>
    exportCompactUv() ?? ((uvInfo()?.mode === "unwrapped" || canUndoUnwrap()) && (uvConfig().layout ?? "paintable") === "paintable");
  const [exportOptions, setExportOptions] = createSignal<ExportOptions>(defaultExportOptions);
  // Sin modelo solo sirven los formatos que llevan el esqueleto solo
  createEffect(() => {
    const opts = exportOptions();
    if (!meshLoaded() && skeletonData() && !SKELETON_FORMATS.includes(opts.format)) {
      untrack(() => setExportOptions({ ...opts, format: "glb" }));
    }
  });
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
  // Línea de tiempo: vista, filtros, papel cebolla, trayectorias, curvas y mezclador
  const [timelineMode, setTimelineMode] = createPersisted<TimelineMode>("timeline.mode", "keys");
  const [onlySelection, setOnlySelection] = createPersisted("timeline.onlySelection", false);
  const [collapsedGroups, setCollapsedGroups] = createSignal<Set<string>>(new Set());
  const [onion, setOnion] = createPersisted<OnionSettings>("timeline.onion", { enabled: false, before: 2, after: 2, step: 2, mesh: false });
  const [showPaths, setShowPaths] = createPersisted("timeline.paths", false);
  const [curveSelection, setCurveSelection] = createSignal<Set<string>>(new Set());
  /** Clip mientras se arrastra en el editor de curvas (sin pasar por el historial) */
  const [clipPreview, setClipPreview] = createSignal<AnimationClip | undefined>();
  const [mixer, setMixer] = createSignal<Mixer>(emptyMixer());
  const [mixerPreview, setMixerPreview] = createSignal<Mixer | undefined>();
  const [selectedLayer, setSelectedLayer] = createSignal<string | undefined>();
  const displayMixer = () => mixerPreview() ?? mixer();
  /** El visor muestra la mezcla de capas en vez del clip activo */
  const mixing = () => displayMixer().enabled && displayMixer().layers.length > 0;
  /** Lo que el rig agrega al esqueleto (ver lib/rig.ts): se guarda en el proyecto */
  const [rigSettings, setRigSettings] = createSignal<RigSettings>(emptyRigSettings());
  /** Articulaciones elegidas en el visor (la activa es viewSettings().selectedBone) */
  const [selectedJoints, setSelectedJoints] = createSignal<number[]>([]);
  const [selectedControl, setSelectedControl] = createSignal<string | undefined>();
  /** Cambia cada vez que el visor muestra otra pose (el panel del hueso la relee) */
  const [poseTick, setPoseTick] = createSignal(0);
  /** Poses guardadas del proyecto */
  const [poseLibrary, setPoseLibrary] = createSignal<StoredPose[]>([]);
  const [transformSpace, setTransformSpace] = createPersisted<TransformSpace>("pose.space", "global");
  const [canPaste, setCanPaste] = createSignal(readPoseClipboard() !== null);
  const [selectedChain, setSelectedChain] = createSignal<string | undefined>();
  /** Restricción elegida en el panel (o en su fila de la línea de tiempo) */
  const [selectedConstraint, setSelectedConstraint] = createSignal<string | undefined>();
  const [autoIk, setAutoIk] = createPersisted("pose.autoIk", { enabled: false, toRoot: false });
  /** El visor, como señal: los efectos de animación lo necesitan listo */
  const [viewer, setViewer] = createSignal<Viewer3D | undefined>();
  /** Nube de puntos del escáner en edición (Orizon3D) */
  const scanCloud = createScanCloud(viewer, (text) => setStatusMessage(text));

  // UV / Piel de la malla retopologizada
  const [uvConfig, setUvConfig] = createSignal<UvConfig>(defaultUvConfig);
  const [uvInfo, setUvInfo] = createSignal<UvInfo | undefined>();
  const [uvPreview, setUvPreview] = createSignal<UvPreview>("texture");
  /** Materiales de la piel (color, metal/rugosidad, normal, oclusión, emisión) */
  const [skinMaterials, setSkinMaterials] = createSignal<SceneMaterial[]>([]);
  const [checkerTexture, setCheckerTexture] = createSignal<ImageBitmap | undefined>();

  // Malla activa: tras retopologizar, UV, esqueleto y pesos trabajan sobre los
  // quads (lo que se exporta), salvo que se elija volver a la original
  const [activeQuad, setActiveQuad] = createSignal(false);
  const usesQuad = () => activeQuad() && quadMeshLoaded();
  /** El visor muestra los quads: a pedido, o en las etapas que los usan */
  const displayQuad = () =>
    (showQuadMesh() || (usesQuad() && ["uv", "skeleton", "animate"].includes(pipeline.activeStep()))) && !!quadMeshData();

  /** Los pesos ya no corresponden a la malla activa */
  /** Por qué se descartaron los pesos (para explicarlo en Animar) y el error del último cálculo */
  const [weightsNotice, setWeightsNotice] = createSignal<string | undefined>();
  const [autorigError, setAutorigError] = createSignal<string | undefined>();
  const dropWeights = (reason?: string) => {
    if (reason && autorigComplete()) setWeightsNotice(reason);
    setAutorigComplete(false);
    setWeightsData(undefined);
    setPaintMirrorLoaded(false);
    setViewSettings((prev) => ({ ...prev, showWeights: false }));
  };

  /** El backend trasladó el rig a la malla nueva: pesos listos otra vez */
  const keepRig = async () => {
    await reloadWeights();
    setAutorigComplete(true);
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
  let skinRequest = 0;
  const refreshSkin = async () => {
    const request = ++skinRequest;
    const info = await invoke<UvInfo | null>("get_uv_info");
    if (request !== skinRequest) return;
    setUvInfo(info ?? undefined);
    if (!info) {
      setSkinMaterials([]);
      return;
    }
    const [materials, mesh] = await Promise.all([
      invoke<MaterialInfo[]>("get_skin_materials"),
      invoke<ArrayBuffer>("get_quad_mesh_data"),
    ]);
    // Solo las texturas que usa algún material
    const used = new Set(
      materials.flatMap((m) => [
        m.base_color_texture,
        m.metallic_roughness_texture,
        m.normal_texture,
        m.occlusion_texture,
        m.emissive_texture,
      ])
    );
    const count = Math.max(0, ...[...used].map((i) => (i ?? -1) + 1));
    const images = await Promise.all(
      Array.from({ length: count }, (_, index) =>
        used.has(index) ? invoke<ArrayBuffer>("get_skin_texture", { index }).then(decodeImage) : undefined
      )
    );
    // Otra lectura empezó después: la suya es la que vale
    if (request !== skinRequest) {
      images.forEach((image) => image?.close());
      return;
    }
    setSkinMaterials(toSceneMaterials(materials, images));
    setQuadMeshData(decodeMesh(mesh));
    if (autorigComplete() && usesQuad()) await reloadWeights();
  };

  // Editor de texturas: la textura abierta (sobre la malla que muestra el
  // visor) y qué fracción del ancho ocupa
  const [textureEditor, setTextureEditor] = createSignal<TextureSelection | undefined>();
  const [editorFraction, setEditorFraction] = createPersisted("textureEditor.fraction", 0.5);
  let splitRef: HTMLDivElement | undefined;
  const resizeEditor = (e: PointerEvent) => {
    const start = editorFraction();
    const width = splitRef?.clientWidth ?? 1;
    startDrag(e, "col-resize", (dx) => setEditorFraction(Math.min(0.8, Math.max(0.2, start + dx / width))));
  };
  /** Los quads se muestran siempre en esta etapa (no se puede elegir la original) */
  const quadForced = () => usesQuad() && ["uv", "skeleton", "animate"].includes(pipeline.activeStep());
  const editorMaterials = () => (displayQuad() ? skinMaterials() : sceneMaterials());
  /** Abre una textura en el editor, sobre la malla original o la retopología */
  const openTextureEditor = (target: "original" | "quad", selection: TextureSelection) => {
    setShowQuadMesh(target === "quad");
    setTextureEditor(selection);
  };

  // Editar la textura en otras aplicaciones (ver apps/desktop/src/textures.rs)
  type TextureKey = TextureSelection & { target: "original" | "quad" };
  const textureKey = (): TextureKey | undefined => {
    const selection = textureEditor();
    return selection && { target: displayQuad() ? "quad" : "original", ...selection };
  };
  const sameKey = (a?: TextureKey, b?: TextureKey) =>
    !!a && !!b && a.target === b.target && a.material === b.material && a.slot === b.slot;
  /** Textura abierta en GIMP y hora de la última recarga */
  const [externalEdit, setExternalEdit] = createSignal<{ key: TextureKey; lastUpdate: string | null }>();
  const stopExternalEdit = () => {
    if (!externalEdit()) return;
    setExternalEdit(undefined);
    invoke("stop_texture_watch").catch(() => {});
  };
  // Otra textura, otra malla o editor cerrado: se deja de seguir el archivo
  createEffect(() => {
    const edit = externalEdit();
    if (edit && !sameKey(edit.key, textureKey())) stopExternalEdit();
  });

  /** Relee las imágenes después de cambiar una textura (las dos pieles si comparten) */
  const reloadTextures = async () => {
    await loadSceneAppearance();
    if (uvInfo()) await refreshSkin();
  };

  /** Nombre de archivo sugerido para una textura */
  const textureFileName = (key: TextureKey, ext: string) => {
    const materials = key.target === "quad" ? skinMaterials() : sceneMaterials();
    const material = materials[key.material]?.name || `material${key.material + 1}`;
    const slot = TEXTURE_SLOTS.find((s) => s.slot === key.slot)?.label ?? key.slot;
    const base = (fileName() ?? "modelo").replace(/\.[^.]+$/, "");
    return `${base} - ${material} - ${slot}${ext === "layout" ? " - malla UV" : ""}.${ext === "layout" ? "png" : ext}`
      .replace(/[\\/:*?"<>|]/g, "_");
  };

  const handleTextureExport = async (kind: TextureExportKind) => {
    const key = textureKey();
    if (!key) return;
    const ext = kind === "layout" ? "png" : kind;
    const names: Record<TextureExportKind, string> = {
      xcf: "GIMP",
      psd: "Photoshop / Krita",
      png: "Imagen PNG",
      layout: "Malla UV (PNG)",
    };
    const path = await save({ defaultPath: textureFileName(key, kind), filters: [{ name: names[kind], extensions: [ext] }] });
    if (!path) return;
    try {
      setIsProcessing(true);
      setStatusMessage("Exportando textura...");
      await invoke("export_texture", { key, path, layoutOnly: kind === "layout" });
      setStatusMessage(`Textura exportada: ${path}`);
    } catch (e) {
      setStatusMessage(`Error: ${e}`);
    } finally {
      setIsProcessing(false);
    }
  };

  const handleTextureImport = () =>
    undoable(async (done) => {
      const key = textureKey();
      if (!key) return;
      const path = await open({
        multiple: false,
        filters: [{ name: "Imágenes y capas", extensions: ["xcf", "psd", "png", "jpg", "jpeg", "webp"] }],
      });
      if (!path || Array.isArray(path)) return;
      try {
        setIsProcessing(true);
        setStatusMessage("Importando textura...");
        await invoke("import_texture", { key, path });
        done("Importar textura");
        await reloadTextures();
        setStatusMessage("Textura reemplazada (las capas de la malla UV se ignoran)");
      } catch (e) {
        setStatusMessage(`Error: ${e}`);
      } finally {
        setIsProcessing(false);
      }
    });

  const handleTextureEditExternally = async () => {
    const key = textureKey();
    if (!key) return;
    try {
      setStatusMessage("Abriendo en GIMP...");
      const path = await invoke<string>("edit_texture_externally", { key, format: "xcf" });
      setExternalEdit({ key, lastUpdate: null });
      setStatusMessage(`Editando ${path}: guarda en GIMP (Ctrl+S) y la textura se recarga sola`);
    } catch (e) {
      setStatusMessage(`Error: ${e}`);
    }
  };

  // Avisos del seguimiento del archivo editado afuera
  const unlistenTexture = [
    listen("texture-updated", async () => {
      try {
        await reloadTextures();
      } catch (e) {
        console.error("Texture reload error:", e);
        setStatusMessage(`No se pudo recargar la textura desde GIMP: ${e}`);
        return;
      }
      const time = new Date().toLocaleTimeString();
      setExternalEdit((prev) => prev && { ...prev, lastUpdate: time });
      setStatusMessage(`Textura recargada desde GIMP (${time})`);
    }),
    listen<{ message: string }>("texture-update-failed", (e) => {
      setStatusMessage(`No se pudo recargar la textura: ${e.payload.message}`);
    }),
  ];
  onCleanup(() => unlistenTexture.forEach((p) => p.then((unlisten) => unlisten())));

  /** Materiales del visor: los de la piel sobre los quads (con la vista
   * "textura"), si no los del archivo de origen */
  const noMaterials: SceneMaterial[] = [];
  const viewerMaterials = createMemo(() => {
    if (!displayQuad()) return sceneMaterials();
    return uvInfo() && uvPreview() === "texture" ? skinMaterials() : noMaterials;
  });

  /** Tablero sobre los quads para ver la distorsión de las UV */
  const viewerTextures = (): MeshTextures | undefined =>
    displayQuad() && uvInfo() && uvPreview() === "checker" ? { base: checkerTexture() } : undefined;

  const handleUvPreview = async (preview: UvPreview) => {
    if (preview === "checker" && !checkerTexture()) {
      try {
        setCheckerTexture(await decodeImage(await invoke<ArrayBuffer>("get_checker_texture")));
      } catch (e) {
        console.error("Checker texture error:", e);
        setStatusMessage(`No se pudo generar el tablero de UV: ${e}`);
        return;
      }
    }
    setUvPreview(preview);
    setShowQuadMesh(true);
  };

  /** Descarta la retopología: ya no corresponde a la geometría actual */
  const clearQuadMesh = () => {
    setUvInfo(undefined);
    setSkinMaterials([]);
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
    xray: false,
    xrayAlpha: 0.35,
    boneDisplay: "octahedral",
  });

  // Sombreado por sección, como cada espacio de trabajo con su propio visor:
  // el esqueleto se ve dentro del modelo translúcido, sin piel y con pesos
  type Shading = Pick<ViewSettings, "showMesh" | "showWireframe" | "showTextures" | "xray" | "showWeights">;
  const DEFAULT_SHADING: Shading = { showMesh: true, showWireframe: false, showTextures: true, xray: false, showWeights: false };
  const SECTION_SHADING: Partial<Record<PipelineStepId, Shading>> = {
    skeleton: { ...DEFAULT_SHADING, showTextures: false, xray: true, showWeights: true },
  };
  const [shadingBySection, setShadingBySection] = createPersisted<Partial<Record<PipelineStepId, Shading>>>(
    "view.shadingBySection",
    {}
  );
  const shadingOf = (v: ViewSettings): Shading => ({
    showMesh: v.showMesh,
    showWireframe: v.showWireframe,
    showTextures: v.showTextures,
    xray: v.xray,
    showWeights: v.showWeights,
  });
  createEffect(
    on(
      () => pipeline.activeStep(),
      (step, prev) => {
        untrack(() => {
          if (prev !== undefined) setShadingBySection({ ...shadingBySection(), [prev]: shadingOf(viewSettings()) });
          const next = shadingBySection()[step] ?? SECTION_SHADING[step] ?? DEFAULT_SHADING;
          setViewSettings((v) => ({ ...v, ...next }));
        });
      }
    )
  );

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
    resolveIntersections: false,
  });
  const [diagnostics, setDiagnostics] = createSignal<MeshDiagnostics | undefined>();
  const [repairResult, setRepairResult] = createSignal<RepairResult | undefined>();
  const [canUndoRepair, setCanUndoRepair] = createSignal(false);
  /** Se desplegó la malla original (sin retopología) y se puede volver atrás */
  const [canUndoUnwrap, setCanUndoUnwrap] = createSignal(false);

  // Print3D state
  const [meshAnalysis, setMeshAnalysis] = createSignal<MeshAnalysis | undefined>();
  const [subdivideResult, setSubdivideResult] = createSignal<SubdivideResult | undefined>();

  // Viewer ref
  let viewerRef: Viewer3D | undefined;

  // Context panel visibility
  const [showContextPanel, setShowContextPanel] = createSignal(true);

  // Grid visibility
  const [showGrid, setShowGrid] = createSignal(true);

  // ─── Nodos del archivo (Outliner) ─────────────────────────────────────────

  /** Nodos del archivo ocultos en el visor y el elegido (índices de `sceneStructure().nodes`) */
  const [hiddenFileNodes, setHiddenFileNodes] = createSignal<number[]>([]);
  const [selectedFileNode, setSelectedFileNode] = createSignal<number | undefined>();
  /** El nodo y todo lo que cuelga de él */
  const nodeSubtree = (index: number): number[] => {
    const nodes = sceneStructure()?.nodes ?? [];
    const out: number[] = [];
    const stack = [index];
    while (stack.length > 0) {
      const n = stack.pop()!;
      if (out.includes(n) || !nodes[n]) continue;
      out.push(n);
      stack.push(...nodes[n].children);
    }
    return out;
  };
  createEffect(() => viewer()?.setHiddenNodes(hiddenFileNodes().flatMap(nodeSubtree)));
  // En Animar la malla se mueve con el rig y el resaltado quedaría quieto
  createEffect(() => {
    const node = selectedFileNode();
    viewer()?.setHighlightedNodes(node === undefined || animating() ? [] : nodeSubtree(node));
  });
  // Otro archivo: los índices de nodo ya no son los mismos
  createEffect(
    on(fileName, () => {
      setHiddenFileNodes([]);
      setSelectedFileNode(undefined);
    }, { defer: true })
  );

  const toggleFileNode = (index: number) =>
    setHiddenFileNodes((list) => (list.includes(index) ? list.filter((n) => n !== index) : [...list, index]));

  /** Relee la malla después de editar nodos: el rig y la retopología ya no le corresponden */
  const afterSceneEdit = async (reason: string) => {
    setMeshData(await fetchMeshData());
    if (quadMeshLoaded()) clearQuadMesh();
    dropWeights(reason);
  };

  const sendSceneEdit = async (d: SceneEdit) => {
    if (d.op === "remove") {
      await busy(`Borrando ${d.name}...`, () => invoke("remove_scene_node", { node: d.node }));
    } else {
      await busy(`Moviendo ${d.name}...`, () =>
        invoke("set_scene_node_transform", { node: d.node, translation: d.after.translation, rotation: d.after.rotation, scale: d.after.scale })
      );
    }
    await afterSceneEdit(d.op === "remove" ? `Borraste ${d.name} del modelo` : `Moviste ${d.name}`);
  };

  const undoSceneEdit = async (d: SceneEdit) => {
    await busy("Deshaciendo...", () => invoke("undo_scene_edit"));
    await afterSceneEdit(d.op === "remove" ? `Volvió ${d.name}` : `${d.name} volvió a su lugar`);
  };

  /** Borra la geometría del nodo (y de sus hijos): sale de la malla y de lo exportado */
  const deleteFileNode = async (index: number) => {
    const name = sceneStructure()?.nodes[index]?.name || `nodo ${index}`;
    try {
      await history.execute(`Borrar ${name}`, { kind: "sceneEdit", data: { op: "remove", node: index, name } });
    } catch (e) {
      setStatusMessage(`Error: ${e}`);
      return;
    }
    const removed = new Set(nodeSubtree(index));
    setHiddenFileNodes((list) => list.filter((n) => !removed.has(n)));
    if (selectedFileNode() !== undefined && removed.has(selectedFileNode()!)) setSelectedFileNode(undefined);
    setStatusMessage(`${name} borrado del modelo (también de lo que se exporte). Ctrl+Z lo trae de vuelta`);
  };

  /** Transformación local de un nodo del archivo (pestaña Objeto) */
  const transformFileNode = async (index: number, after: NodeTransform) => {
    const node = sceneStructure()?.nodes[index];
    if (!node) return;
    const name = node.name || `nodo ${index}`;
    const before: NodeTransform = { translation: node.translation, rotation: node.rotation, scale: node.scale };
    try {
      await history.execute(`Mover ${name}`, { kind: "sceneEdit", data: { op: "transform", node: index, name, before, after } });
      setStatusMessage(`${name}: transformación cambiada (el rig y la retopología se descartan)`);
    } catch (e) {
      setStatusMessage(`Error: ${e}`);
    }
  };

  // ─── El diseño en el Outliner ─────────────────────────────────────────────
  // El árbol de operaciones de Diseñar va dentro del Outliner (se crea una vez)
  const featureTree = <FeatureTree store={cad} ui={cadUi} />;
  /** Planos, operaciones y piezas del diseño paramétrico para el Outliner */
  const cadOutline = (): CadOutline | undefined => {
    const doc = cad.doc();
    if (!doc) return undefined;
    const picks = cadUi.picks();
    const hiddenRefs = cadUi.hiddenSketches();
    const rollback = doc.rollback ?? null;
    const planes: CadOutlineItem[] = (["xy", "xz", "yz"] as const).map((id) => ({
      id: `plane-${id}`,
      type: "plane",
      label: PLANE_LABELS[id],
      visible: cadUi.showPlanes() && !cadUi.hiddenPlanes().includes(id),
      selected: picks.some((p) => p.kind === "plane" && p.plane === id),
      hint: "Plano base del diseño",
    }));
    doc.features.forEach((f, i) => {
      const kind = f.kind.type;
      if (kind !== "plane" && kind !== "axis" && kind !== "point") return;
      const state = cad.stateOf(f.id);
      const muted = !!f.suppressed || (rollback !== null && i >= rollback) || state?.state === "rolled_back";
      planes.push({
        id: `cad-${f.id}`,
        type: kind,
        label: f.name,
        visible: !hiddenRefs.includes(f.id),
        selected: cad.selected() === f.id || picks.some((p) => p.kind === "refplane" && p.feature === f.id),
        deletable: true,
        muted,
        error: state?.state === "error",
        hint: state?.state === "error" ? state.message : muted ? (f.suppressed ? "Suprimida" : "Después de la barra de retroceso") : undefined,
      });
    });
    return { planes, featureTree, featureCount: doc.features.length };
  };

  /** Filas de los objetos para el Outliner (las piezas que ya no están y nunca tuvieron malla no salen) */
  const objectRows = (): ObjectRow[] => {
    const doc = cad.committed();
    const parts = cad.result()?.parts ?? [];
    const active = activeObjectId();
    return objects().flatMap((o): ObjectRow[] => {
      const isActive = o.id === active;
      // Lo hecho sobre la malla: del historial vivo (activo) o del guardado (los demás)
      const trail = isActive ? (history.current(), history.trail()) : savedTrail(o.history);
      const modifications: SceneNode[] = trail.filter(isGeometryStep).map((node) => ({
        id: `mod-${o.id}-${node.id}`,
        type: "feature",
        label: node.description,
        visible: true,
        expanded: false,
        selected: false,
        readonly: true,
        children: [],
        hint: isCadObject(o) ? "Se rehace sola si la pieza cambia en el diseño" : undefined,
      }));
      if (isCadObject(o)) {
        const i = parts.findIndex((p) => partKey(p.id) === partKey(o.source.part));
        const view = parts[i];
        if (!view && !materialized(o, active)) return [];
        return [{
          id: `obj-${o.id}`,
          type: "part",
          label: o.name,
          active: isActive,
          modifications,
          meshLabel: "Malla generada",
          selected: isActive,
          visible: inDesign() ? !view || !partHidden(doc, view) : isActive ? viewSettings().showMesh : !hiddenGhosts().includes(o.id),
          readonly: inDesign() ? !view : !isActive && !o.ui,
          deletable: true,
          color: view ? partColor(doc, view, i) : undefined,
          muted: !view,
          hint: view
            ? `Pieza del diseño · ${(view.volume / 1000).toLocaleString("es", { maximumFractionDigits: 2 })} cm³`
            : "Ya no está en el diseño: queda la última malla",
        }];
      }
      return [{
        id: `obj-${o.id}`,
        type: "mesh",
        label: o.name,
        active: isActive,
        modifications,
        selected: isActive,
        visible: isActive ? viewSettings().showMesh : !hiddenGhosts().includes(o.id),
        readonly: !isActive && !o.ui,
        deletable: true,
        hint: isActive ? undefined : "Clic para trabajar con este objeto",
      }];
    });
  };

  /** `obj-<n>` → objeto */
  const objectOfNode = (nodeId: string): SceneObject | undefined => {
    const m = /^obj-(\d+)$/.exec(nodeId);
    return m ? objects().find((o) => o.id === Number(m[1])) : undefined;
  };
  const cadFeatureOfNode = (nodeId: string): number | undefined => {
    const m = /^cad-(\d+)$/.exec(nodeId);
    return m ? Number(m[1]) : undefined;
  };

  // ─── Objetos ──────────────────────────────────────────────────────────────

  /** Espacios que trabajan sobre la malla del objeto activo */
  const meshWorkspace = () => ["prepare", "rig", "print"].includes(pipeline.workspace()?.id ?? "");

  /** Interfaz vacía: un objeto que todavía no tiene modelo */
  const EMPTY_OBJECT_UI = JSON.stringify({ model: false });

  /**
   * Deja activo el objeto `id`: el actual guarda su interfaz e historial, el
   * backend intercambia los modelos y la interfaz se pone como estaba la del
   * otro. Fuera de Diseñar, una pieza del diseño se asegura su malla.
   */
  let switchingObject: Promise<void> = Promise.resolve();
  const activateObject = (id: number, ensureMesh = !inDesign()) =>
    (switchingObject = switchingObject.then(async () => {
      if (id !== activeObjectId()) {
        if (operations > 0) return void setStatusMessage("Espera a que termine la operación para cambiar de objeto");
        const current = activeObjectId();
        const ui = projectUi(false);
        const saved = history.save();
        try {
          await busy("Cambiando de objeto...", () => invoke("object_activate", { id }));
        } catch (e) {
          return void setStatusMessage(`Error: ${e}`);
        }
        const target = objects().find((o) => o.id === id);
        setObjects((list) =>
          list.map((o) =>
            o.id === current ? { ...o, ui, history: saved, rev: (o.rev ?? 0) + 1 } : o.id === id ? { ...o, ui: undefined, history: undefined } : o
          )
        );
        setActiveObjectId(id);
        setSwitching(true);
        try {
          await restoreProjectUi(target?.ui ?? EMPTY_OBJECT_UI, true);
          if (!target?.ui) {
            setSceneStructure(undefined);
            setSceneMaterials([]);
            setMeshInfo({ vertices: 0, faces: 0, format: "" });
          }
          loadObjectHistory(target?.history);
          setFileName(target?.name);
        } finally {
          setSwitching(false);
        }
      }
      if (ensureMesh) await ensureCadMesh();
    }));

  /** Historial del objeto que pasa a estar activo: lo que ya no se puede deshacer queda como hito */
  const loadObjectHistory = (saved: SceneObject["history"]) => {
    history.load(saved, ["sceneEdit"]);
    history.seal((step) => step.kind === "snapshot" && !snapshotUi.has((step.data as { id: number }).id));
  };

  /** Objeto nuevo y activo (vacío): para importar o escanear un modelo */
  const newObject = async (name: string, source: SceneObject["source"]) => {
    const id = nextObjectId(objects());
    setObjects((list) => [...list, { id, name, source }]);
    await activateObject(id, false);
    return id;
  };

  /**
   * Dónde va un modelo que se importa o se escanea: un objeto nuevo, salvo que
   * el activo sea importado y todavía no tenga malla (un esqueleto solo: sus
   * animaciones pasan al modelo, como siempre)
   */
  const objectForNewModel = async (name: string): Promise<{ id: number; created: boolean }> => {
    await adoptCurrentModel();
    const active = activeObject();
    if (active && active.source.kind === "import" && !meshLoaded()) {
      setObjects((list) => list.map((o) => (o.id === active.id ? { ...o, name } : o)));
      return { id: active.id, created: false };
    }
    return { id: await newObject(name, { kind: "import" }), created: true };
  };

  /** Borra un objeto; si era el activo, queda sin modelo */
  const removeObject = async (id: number) => {
    await switchingObject;
    try {
      await invoke("object_remove", { id });
    } catch (e) {
      return setStatusMessage(`Error: ${e}`);
    }
    const was = objects().find((o) => o.id === id);
    if (was?.history) dropSnapshotsOf(was.history);
    setObjects((list) => list.filter((o) => o.id !== id));
    if (activeObjectId() === id) {
      setActiveObjectId(undefined);
      resetHistory();
      await restoreProjectUi(EMPTY_OBJECT_UI, true);
      setSceneStructure(undefined);
      setSceneMaterials([]);
      setMeshInfo({ vertices: 0, faces: 0, format: "" });
      setFileName(undefined);
    }
  };

  /** El modelo cargado antes de que hubiera objetos pasa a ser uno (proyectos viejos, recuperación) */
  const adoptCurrentModel = async () => {
    if (activeObjectId() !== undefined || (!meshLoaded() && !skeletonLoaded())) return;
    const id = nextObjectId(objects());
    await invoke("object_adopt", { id }).catch(() => {});
    setObjects((list) => [...list, { id, name: fileName() ?? "Modelo", source: { kind: "import" } }]);
    setActiveObjectId(id);
  };

  /**
   * La pieza del diseño activa tiene su malla al día: la primera vez se
   * genera; si el diseño cambió, se regenera y se rehacen sus modificaciones.
   */
  const ensureCadMesh = async () => {
    const object = activeObject();
    if (!isCadObject(object)) return;
    const parts = cad.result()?.parts ?? [];
    if (!parts.some((p) => partKey(p.id) === partKey(object.source.part))) return;
    let r: { changed: boolean; hash: string; info: MeshInfo | null };
    try {
      r = await busy(`Preparando la malla de ${object.name}...`, () =>
        invoke("cad_part_to_model", { part: object.source.part, known: object.source.hash ?? null, onProgress: progressChannel() })
      );
    } catch (e) {
      return setStatusMessage(`Error: ${e}`);
    }
    if (!r.changed || !r.info) return;
    const first = !object.source.hash;
    setObjects((list) => list.map((o) => (o.id === object.id && isCadObject(o) ? { ...o, source: { ...o.source, hash: r.hash } } : o)));
    setSwitching(true);
    try {
      if (first) await showNewModel(r.info, `Malla de ${object.name}`);
      else await replayModifications(r.info);
      setFileName(object.name);
    } finally {
      setSwitching(false);
    }
  };

  /**
   * La pieza cambió en el diseño y el backend ya cargó su malla nueva
   * (conservando el esqueleto): se rehacen en orden las modificaciones del
   * historial (orientar, reparar, escalar, dividir, retopología, UV…) y, si
   * había pesos, se recalculan. Las copias para deshacer de esos pasos tenían
   * la malla vieja: quedan como hitos.
   */
  const replayModifications = async (info: MeshInfo) => {
    const hadWeights = autorigComplete() && skeletonLoaded();
    const revive = (args: unknown) =>
      JSON.parse(JSON.stringify(args ?? {}), (_, v) => (v && typeof v === "object" && v.__channel === true ? progressChannel() : v));
    let count = 0;
    // Lo último que dijeron los pasos de Fabricar (el panel muestra eso)
    let subdivided: TauriSubdivideResult | undefined;
    try {
      await busy("Rehaciendo las modificaciones...", async () => {
        for (const node of history.trail()) {
          const step = node.step;
          if (step?.kind === "placement") {
            const matrix = new THREE.Matrix4().fromArray((step.data as { matrix: number[] }).matrix);
            await rawInvoke("apply_placement", { matrix: matrix.elements });
            count++;
          } else if (step?.kind === "snapshot") {
            for (const op of (step.data as { ops?: RecordedCall[] }).ops ?? []) {
              const result = await rawInvoke(op.cmd, revive(op.args));
              if (op.cmd === "subdivide_mesh") subdivided = result as TauriSubdivideResult;
              count++;
            }
          }
        }
        if (hadWeights) {
          const config = autorigConfig();
          await rawInvoke("run_autorig", {
            config: { quality: config.quality, diffusion_weight: config.diffusionWeight, max_influences: config.maxInfluences },
            onProgress: progressChannel(),
          });
        }
      });
    } catch (e) {
      setStatusMessage(`El diseño cambió y no se pudo rehacer todo: ${e}`);
    }
    for (const node of history.nodes()) {
      if (node.step?.kind !== "snapshot" || node.milestone) continue;
      const id = (node.step.data as { id?: number }).id;
      if (id !== undefined) {
        snapshotUi.delete(id);
        invoke("drop_snapshot", { id }).catch(() => {});
      }
    }
    history.seal((step) => step.kind === "snapshot");
    history.milestone("Malla regenerada del diseño");
    // La interfaz relee del backend lo que quedó (malla, quads, esqueleto, pesos)
    await restoreProjectUi(projectUi(false), true);
    setMeshInfo((prev) => ({ ...prev, format: info.format }));
    if (meshAnalysis()) setMeshAnalysis(await invoke<TauriPrint3dAnalysis>("analyze_print3d").catch(() => undefined));
    if (subdivideResult()) setSubdivideResult(subdivided);
    if (count > 0 || hadWeights) setStatusMessage(`El diseño cambió: malla nueva con ${count} modificación${count === 1 ? "" : "es"} rehecha${count === 1 ? "" : "s"}`);
  };

  // Los demás objetos se ven en gris en el visor (los que ya tienen malla y no se ocultaron)
  const [hiddenGhosts, setHiddenGhosts] = createSignal<number[]>([]);
  const ghostCache = new Map<string, MeshData>();
  let ghostRequest = 0;
  createEffect(() => {
    const v = viewer();
    if (!v) return;
    const active = activeObjectId();
    const hidden = hiddenGhosts();
    // `"model":false` en su interfaz: todavía no tiene malla
    const list = objects().filter((o) => o.id !== active && o.ui && !o.ui.includes('"model":false') && !hidden.includes(o.id));
    const request = ++ghostRequest;
    void (async () => {
      await switchingObject;
      const out: MeshData[] = [];
      for (const o of list) {
        // En las unidades del activo: cambia con el activo y con lo hecho sobre el objeto
        const key = `${o.id}:${o.rev ?? 0}:${active}`;
        let data = ghostCache.get(key);
        if (!data) {
          try {
            data = decodeMesh(await rawInvoke<ArrayBuffer>("object_mesh_data", { id: o.id }));
          } catch {
            continue;
          }
          for (const k of [...ghostCache.keys()]) if (k.startsWith(`${o.id}:`)) ghostCache.delete(k);
          ghostCache.set(key, data);
        }
        out.push(data);
      }
      if (request === ghostRequest) v.setGhosts(out);
    })();
  });

  // Cada pieza del diseño es un objeto (solo con el diseño guardado, no con la vista previa de un diálogo)
  createEffect(() => {
    const r = cad.result();
    if (!r || cad.draft()) return;
    setObjects((list) => syncCadObjects(list, r.parts.map((p) => ({ id: p.id, name: p.name }))));
  });

  // Salir de Diseñar a un espacio que trabaja sobre la malla: la pieza elegida
  // (la activa, la abierta en la lista de piezas o la primera) tiene su malla al día
  createEffect(
    on(meshWorkspace, (needsMesh) => {
      if (!needsMesh) return;
      untrack(() => {
        const active = activeObject();
        if (active) return void activateObject(active.id, true);
        const parts = cad.result()?.parts ?? [];
        const wanted = parts.find((p) => partKey(p.id) === cadUi.openPart()) ?? parts[0];
        const target = wanted && objects().find((o) => isCadObject(o) && partKey(o.source.part) === partKey(wanted.id));
        if (target) void activateObject(target.id, true);
      });
    })
  );

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
    structure: sceneStructure(),
    hiddenNodes: new Set(hiddenFileNodes()),
    selectedNode: selectedFileNode(),
    cad: cadOutline(),
    objects: objectRows(),
  });

  // ─── Deshacer operaciones del backend ────────────────────────────────────
  // Antes de una operación que cambia el backend (reparar, retopología,
  // pesos, UV…) se guarda una copia completa de su estado; deshacer la
  // intercambia con el actual y la interfaz vuelve a como estaba antes
  /** Interfaz antes y después de cada copia (en memoria, como las copias) */
  const snapshotUi = new Map<number, { before: string; after: string }>();
  /** Operaciones deshacibles en curso: deshacer espera a que terminen */
  let operations = 0;

  /** Corre `run` como paso deshacible: `done(descripción)` lo registra (si no
   *  se llama, la operación no se hizo y la copia se descarta) */
  const undoable = async <T,>(run: (done: (description: string) => void) => Promise<T>): Promise<T> => {
    let id: number | undefined;
    try {
      const taken = await invoke<{ id: number; evicted: number[] }>("take_snapshot");
      id = taken.id;
      forgetSnapshots(taken.evicted);
    } catch (e) {
      // Sin copia la operación igual se hace, pero no se puede deshacer
      console.warn("Sin copia para deshacer:", e);
    }
    const before = projectUi(false);
    let description: string | undefined;
    // Los comandos de geometría que corra quedan en el paso (para rehacerlos si cambia el diseño)
    const ops: RecordedCall[] = [];
    recorders.push(ops);
    operations++;
    try {
      return await run((d) => void (description = d));
    } finally {
      operations--;
      recorders.splice(recorders.lastIndexOf(ops), 1);
      if (description === undefined) {
        if (id !== undefined) invoke("drop_snapshot", { id }).catch(() => {});
      } else if (id === undefined) {
        history.milestone(description, { kind: "snapshot", data: { ops } });
      } else {
        snapshotUi.set(id, { before, after: projectUi(false) });
        await history.execute(description, { kind: "snapshot", data: { id, ops } }, { applied: true });
      }
    }
  };

  /** Copias que el backend descartó: sus pasos pasan a ser hitos */
  const forgetSnapshots = (ids: number[]) => {
    if (ids.length === 0) return;
    ids.forEach((id) => snapshotUi.delete(id));
    history.seal((step) => step.kind === "snapshot" && ids.includes((step.data as { id: number }).id));
  };

  const swapSnapshot = async (id: number, side: "before" | "after") => {
    const ui = snapshotUi.get(id);
    if (!ui) throw new Error("la copia para deshacer ya no está");
    await busy(side === "before" ? "Deshaciendo..." : "Rehaciendo...", () => invoke("swap_snapshot", { id }));
    await restoreProjectUi(ui[side], true);
  };

  /** Modelo nuevo: historial nuevo (y sin copias del anterior) */
  const resetHistory = () => {
    dropSnapshotsOf(history.save());
    history.clear();
  };
  /** Descarta las copias del backend de los pasos de un historial (los demás objetos guardan las suyas) */
  const dropSnapshotsOf = (saved: SavedHistory) => {
    for (const node of saved.nodes) {
      if (node.step?.kind !== "snapshot") continue;
      const id = (node.step.data as { id: number }).id;
      snapshotUi.delete(id);
      invoke("drop_snapshot", { id }).catch(() => {});
    }
  };

  /** Deshacer y rehacer con el error a la vista si el backend falla */
  const undo = () => {
    if (inDesign()) return cad.undo();
    if (cloudEditing()) return void scanCloud.history(false);
    if (operations > 0) return setStatusMessage("Espera a que termine la operación para deshacer");
    void history.undo().catch((e) => setStatusMessage(`No se pudo deshacer: ${e}`));
  };
  const redo = () => {
    if (inDesign()) return cad.redo();
    if (cloudEditing()) return void scanCloud.history(true);
    if (operations > 0) return setStatusMessage("Espera a que termine la operación para rehacer");
    void history.redo().catch((e) => setStatusMessage(`No se pudo rehacer: ${e}`));
  };

  // Shortcuts
  const shortcuts = createShortcutManager();

  const shortcutDefs: ShortcutDef[] = [
    { key: "s", ctrl: true, action: () => handleSaveProject(), description: "Guardar proyecto" },
    { key: "s", ctrl: true, shift: true, action: () => handleSaveProject(true), description: "Guardar proyecto como" },
    { key: "n", ctrl: true, action: () => handleNewProject(), description: "Proyecto nuevo" },
    { key: "o", ctrl: true, action: () => handleOpenProject(), description: "Abrir proyecto" },
    { key: "i", ctrl: true, action: () => handleLoad(), description: "Importar modelo" },
    { key: "z", ctrl: true, action: () => undo(), description: "Deshacer" },
    { key: "z", ctrl: true, shift: true, action: () => redo(), description: "Rehacer" },
    { key: "q", action: () => useTool("select"), description: "Seleccionar" },
    // Sobre el modelo, G/R/S eligen el gizmo. Sobre el esqueleto, como en
    // Blender, G y R son operaciones modales sobre la articulación
    // seleccionada (el mouse mueve, clic confirma, clic derecho/Esc cancela)
    {
      key: "g",
      action: () => (toolCtx() === "object" ? useTool("move") : viewerRef?.startModal("grab")),
      description: "Mover (modelo o articulación)",
    },
    {
      key: "r",
      action: () => (toolCtx() === "object" ? useTool("rotate") : viewerRef?.startModal("rotate")),
      description: "Rotar (modelo, o pose de prueba)",
    },
    { key: "s", action: () => useTool("scale"), description: "Escalar (modelo o esqueleto entero)" },
    { key: "m", action: () => useTool("measure"), description: "Medir" },
    { key: "Home", action: () => viewerRef?.resetView(), description: "Ver todo" },
    {
      key: " ",
      action: () =>
        animating() ? setPlaying(!playing()) : showPlayback() ? togglePreview() : viewerRef?.resetView(),
      description: "Reproducir / ver todo",
    },
    { key: "i", action: () => animating() && handleInsertKey(), description: "Insertar key" },
    {
      key: "x",
      action: () => (cloudEditing() ? scanCloud.deleteSelection() : animating() && handleDeleteKeys()),
      description: "Borrar keys / puntos seleccionados",
    },
    {
      key: "Delete",
      action: () => (cloudEditing() ? scanCloud.deleteSelection() : animating() && handleDeleteKeys()),
      description: "Borrar keys / puntos seleccionados",
    },
    { key: "i", ctrl: true, action: () => cloudEditing() && scanCloud.select("invert"), description: "Invertir la selección de la nube" },
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
    {
      key: "z",
      alt: true,
      action: () => setViewSettings((prev) => ({ ...prev, xray: !prev.xray })),
      description: "Rayos X",
    },
    { key: "1", action: () => viewerRef?.setView("front"), description: "Vista frontal" },
    { key: "3", action: () => viewerRef?.setView("right"), description: "Vista derecha" },
    { key: "7", action: () => viewerRef?.setView("top"), description: "Vista superior" },
    { key: "1", ctrl: true, action: () => viewerRef?.setView("back"), description: "Vista trasera" },
    { key: "3", ctrl: true, action: () => viewerRef?.setView("left"), description: "Vista izquierda" },
    { key: "7", ctrl: true, action: () => viewerRef?.setView("bottom"), description: "Vista inferior" },
    { key: "b", action: () => useTool("paint"), description: "Pintar pesos" },
    // Pose (Animar): selección, reiniciar, copiar y pegar
    {
      key: "a",
      action: () => (cloudEditing() ? scanCloud.select("all") : rigging() && handleSelectCommand("all")),
      description: "Seleccionar todo",
    },
    {
      key: "a",
      alt: true,
      action: () => (cloudEditing() ? scanCloud.select("none") : rigging() && handleSelectCommand("none")),
      description: "No seleccionar nada",
    },
    { key: "[", action: () => rigging() && handleSelectCommand("parent"), description: "Seleccionar el padre" },
    { key: "{", shift: true, action: () => rigging() && handleSelectCommand("parent", true), description: "Sumar el padre" },
    { key: "]", action: () => rigging() && handleSelectCommand("children"), description: "Seleccionar los hijos" },
    { key: "}", shift: true, action: () => rigging() && handleSelectCommand("children", true), description: "Sumar los descendientes" },
    { key: "m", ctrl: true, shift: true, action: () => rigging() && handleSelectCommand("mirror"), description: "Seleccionar el espejo" },
    { key: "g", shift: true, action: () => rigging() && handleSelectCommand("group"), description: "Seleccionar el grupo" },
    { key: "r", alt: true, action: () => animating() && handleResetPose("rotation"), description: "Giro en reposo" },
    { key: "g", alt: true, action: () => animating() && handleResetPose("translation"), description: "Posición de reposo" },
    {
      key: "e",
      ctrl: true,
      action: () => (animating() ? toggleRigEditor() : inSkeletonStep() && toggleSkeletonEditor()),
      description: "Editor de pose y rig, o constructor de cuerpo",
    },
    { key: "c", ctrl: true, action: () => animating() && handleCopyPose(), description: "Copiar pose" },
    { key: "v", ctrl: true, action: () => animating() && handlePastePose(false), description: "Pegar pose" },
    { key: "v", ctrl: true, shift: true, action: () => animating() && handlePastePose(true), description: "Pegar pose espejada" },
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
          category: p.category,
          shape: p.shape ?? undefined,
        }))
      );

      // Huesos de cada plantilla, para las miniaturas de la galería
      invoke<TemplateShape[]>("list_template_shapes")
        .then((shapes) => setTemplateShapes(new Map(shapes.map((s) => [s.id, s]))))
        .catch((e) => console.error("Template shapes error:", e));

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
    void scanCloud.attach(v);
  };

  /**
   * Importa un modelo. Con `projects` el mismo diálogo abre también proyectos
   * .pinocchio (pantalla de inicio); `then` es la sección a la que se pasa
   */
  const handleLoad = async (opts: { projects?: boolean; then?: PipelineStepId } = {}) => {
    if (blockedByTask()) return;
    try {
      const formats = supportedFormats();
      if (!formats) return;

      const extensions = formats.import.flatMap((f) => f.extensions);
      const selected = await open({
        title: opts.projects ? "Abrir un proyecto o importar un modelo 3D" : "Importar modelo 3D (con sus texturas, si vienen aparte)",
        multiple: true,
        filters: [
          {
            name: opts.projects ? "Proyectos, modelos 3D y texturas" : "Modelos 3D y texturas",
            extensions: [...(opts.projects ? ["pinocchio"] : []), ...extensions, ...TEXTURE_FILE_EXTENSIONS],
          },
          ...(opts.projects ? [{ name: "Proyecto de Pinocchio", extensions: ["pinocchio"] }] : []),
          ...formats.import.map((f) => ({
            name: f.name,
            extensions: f.extensions,
          })),
        ],
      });

      if (!selected) return;
      const files = typeof selected === "string" ? [selected] : selected;
      const extensionOf = (f: string) => f.split(".").pop()?.toLowerCase() ?? "";
      const project = files.find((f) => extensionOf(f) === "pinocchio");
      if (project) {
        await handleOpenProject(project);
        return;
      }
      const models = files.filter((f) => extensions.includes(extensionOf(f)));
      if (models.length !== 1) {
        setStatusMessage(models.length === 0 ? "Elige también el modelo 3D, no solo las texturas" : "Elige un solo modelo 3D (las texturas pueden ser varias)");
        return;
      }
      const filePath = models[0];
      const textures = files.filter((f) => f !== filePath && TEXTURE_FILE_EXTENSIONS.includes(extensionOf(f)));
      const name = filePath.split(/[\\/]/).pop() ?? filePath;
      setStatusMessage(`Importando ${name}...`);

      // El modelo importado es un objeto nuevo de la escena (los demás quedan)
      const target = await objectForNewModel(name);
      setSwitching(true);
      let info: MeshInfo;
      try {
        info = await busy(`Importando ${name}...`, () =>
          invoke<MeshInfo>("import_model", { path: filePath, textures, onProgress: progressChannel() })
        );
      } catch (e) {
        if (target.created) await removeObject(target.id);
        throw e;
      }
      await showNewModel(info, `Importar ${name}`);
      // El nombre cambia solo si la importación salió bien
      setFileName(name);
      const attached = info.textures ?? [];
      const skipped = info.textures_skipped ?? [];
      if (attached.length > 0 || skipped.length > 0) {
        const parts = [];
        if (attached.length > 0) parts.push(`${attached.length === 1 ? "1 textura puesta" : `${attached.length} texturas puestas`} (${attached.join("; ")})`);
        if (skipped.length > 0) parts.push(`sin usar: ${skipped.join("; ")}`);
        setStatusMessage(`Modelo cargado: ${parts.join(" · ")}`);
      }
      pipeline.setActiveStep(opts.then ?? "structure");
    } catch (e) {
      console.error("Import error:", e);
      setStatusMessage(`Error: ${e}`);
      setProgress(undefined);
    } finally {
      setSwitching(false);
    }
  };

  /** Modelo del escáner (Orizon3D), de la nube editada o del escáner: reemplaza al abierto */
  const handleScanModel = async (settings: ScanMeshSettings, fromCloud: boolean) => {
    if (blockedByTask()) return;
    const target = await objectForNewModel("Escaneo");
    setIsProcessing(true);
    try {
      let info: MeshInfo;
      try {
        info = await busy("Creando el modelo del escáner...", () =>
          invoke<MeshInfo>(fromCloud ? "scan_cloud_create_model" : "scanner_create_model", { settings, onProgress: progressChannel() })
        );
      } catch (e) {
        if (target.created) await removeObject(target.id);
        throw e;
      }
      setFileName("Escaneo");
      // Se ve el modelo nuevo; la nube sigue en edición por si hay que retocarla
      scanCloud.setShown(false);
      await showNewModel(info, "Escanear");
    } catch (e) {
      console.error("Scan model error:", e);
      setStatusMessage(`Error: ${e}`);
    } finally {
      setIsProcessing(false);
    }
  };

  /** Muestra el modelo recién cargado en el backend y limpia lo del anterior */
  const showNewModel = async (info: MeshInfo, milestone: string) => {
    // Se venía animando el esqueleto solo: sus animaciones pasan al modelo
    const keepClips = !info.rig && !meshLoaded() && !!skeletonData() && clips().some((c) => c.tracks.length > 0);
    // Modelo nuevo: historial nuevo
    resetHistory();
    history.milestone(milestone);

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
    setCanUndoUnwrap(false);
    setMeshAnalysis(undefined);
    setSubdivideResult(undefined);
    if (!keepClips) {
      setClips([]);
      setActiveClipId(undefined);
      setRigSettings(emptyRigSettings());
      setMixer(emptyMixer());
    }
    setFrame(0);
    clearQuadMesh();
    setLastExport(undefined);
    setCanUndoPrintScale(false);

    if (keepClips) {
      setStatusMessage("Modelo cargado: calcula los pesos en Esqueleto para ver las animaciones sobre él");
    }

    // El archivo trae esqueleto y pesos: queda listo para animar sus clips
    if (info.rig) {
      clearSkeletonUi();
      setSkeletonData(tauriSkeletonToViewer(await invoke<TauriSkeletonData>("get_skeleton_data")));
      setSkeletonLoaded(true);
      const weights = decodeWeights(await invoke<ArrayBuffer>("get_weights_data"));
      setWeightsData(weights);
      setBoneNames(weights.boneNames);
      setAutorigComplete(true);
      const imported = info.rig.clips.map((c) => ({ ...c, id: createClip(c.name, c.fps).id }));
      setClips(imported);
      setActiveClipId(imported[0]?.id);
      pipeline.markCompleted("skeleton");
      const animations = imported.length === 1 ? "1 animación" : `${imported.length} animaciones`;
      setStatusMessage(
        `Modelo cargado: ${info.num_vertices.toLocaleString()} vertices, esqueleto de ${info.rig.num_bones} huesos y ${animations} (${info.format})`
      );
    }

    // Pipeline: importado; se muestra lo que trae el archivo
    pipeline.markCompleted("import");
  };

  /** Cuenta las elecciones de plantilla y de forma de cuerpo: una respuesta
   * vieja no pisa a la última */
  let skeletonRequest = 0;
  const handleSkeletonChange = (presetId: string) =>
    undoable(async (done) => {
      const request = ++skeletonRequest;
      const previous = selectedSkeleton();
      try {
        setSelectedSkeleton(presetId);

        const data = await invoke<TauriSkeletonData>("select_skeleton", { presetId });
        if (request !== skeletonRequest) return;
        done(`Plantilla: ${skeletonPresets().find((p) => p.id === presetId)?.name ?? presetId}`);
        recordTemplateUse(presetId);
        setSkeletonData(tauriSkeletonToViewer(data));
        setSkeletonLoaded(true);
        setFitInfo(undefined);
        setAutorigComplete(false);
        const plan = (await invoke<BodyPlan | null>("get_body_plan", { presetId })) ?? undefined;
        if (request !== skeletonRequest) return;
        setBodyPlan(plan);

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
        // La lista vuelve a mostrar la plantilla que sigue puesta
        if (request === skeletonRequest) setSelectedSkeleton(previous);
      }
    });

  const handleAutorig = () =>
    undoable(async (done) => {
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
        done("Calcular pesos");

        setProgress({ value: 100, label: "Cargando pesos en el visor..." });
        const viewerWeights = decodeWeights(await invoke<ArrayBuffer>("get_weights_data"));
        setWeightsData(viewerWeights);
        setBoneNames(viewerWeights.boneNames);

        const skelData = await invoke<TauriSkeletonData>("get_skeleton_data");
        setSkeletonData(tauriSkeletonToViewer(skelData));

        setIsProcessing(false);
        setAutorigComplete(true);
        setWeightsNotice(undefined);
        setAutorigError(undefined);
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
        // Que no pase desapercibido: sin pesos no se puede animar ni pintar
        const message = String(e).replace(/^Error:\s*/, "");
        setAutorigError(message);
        void confirmAction({
          title: "No se pudieron calcular los pesos",
          message: `${message}\n\nPrueba con calidad "rápida", revisa que el esqueleto quede dentro de la malla o repara la malla (Preparar → Reparar) y vuelve a calcularlos.`,
          confirmLabel: "Entendido",
        });
      }
    });

  // ═══════════════════════════════════════════════════════════════════════════
  // ANIMACIÓN
  // ═══════════════════════════════════════════════════════════════════════════

  const activeClip = () => clips().find((c) => c.id === activeClipId());
  const boneIndex = createMemo(() => new Map((skeletonData()?.bones ?? []).map((b, i) => [b.name, i])));
  const rigBones = createMemo(() =>
    (skeletonData()?.bones ?? []).map((b) => ({ name: b.name, position: b.position as Vec3, parent: b.parent }))
  );
  /** Esqueleto + rig: orientación de las articulaciones, simetría y la pila de evaluación */
  const rigCtx = createMemo(() => createRigContext(rigBones(), rigSettings()));

  // Grupos automáticos la primera vez que aparece un esqueleto (no pasa por el historial)
  createEffect(
    on(
      () => rigBones().map((b) => b.name).join("|"),
      (names) => {
        if (!names) return;
        const s = untrack(rigSettings);
        if (s.groups.length === 0) setRigSettings(withAutoGroups(s, untrack(rigBones)));
      }
    )
  );

  // Lo que el visor dibuja y permite del rig
  createEffect(() => {
    const v = viewer();
    if (!v) return;
    const ctx = rigCtx();
    if (ctx.bones.length === 0) {
      v.setRigDisplay(null);
      return;
    }
    const s = ctx.settings;
    const props = ctx.bones.map((b) => boneProps(s, b.name));
    v.setRigDisplay({
      colors: ctx.bones.map((b) => rigBoneColor(s, b.name)),
      hidden: ctx.bones.map((b) => isBoneHidden(s, b.name)),
      locked: props.map((p) => p.locked),
      shapes: props.map((p) => p.shape),
      lockTranslation: props.map((p) => p.lockTranslation),
      deform: props.map((p) => p.deform),
      frames: ctx.frames,
      constrain: constrainer(ctx),
      controls: s.controls,
    });
  });

  // La selección múltiple la lleva el visor: al cambiar la activa, se relee
  createEffect(
    on(
      () => viewSettings().selectedBone,
      () => queueMicrotask(() => setSelectedJoints(viewerRef?.getSelection() ?? []))
    )
  );
  /** Hay rig para animar: esqueleto con pesos sobre la malla que se ve */
  const animationReady = () =>
    !!skeletonData() &&
    (!meshLoaded() ||
      (autorigComplete() && !!weightsData() && weightsData()!.numBones === skeletonData()!.bones.length));
  const animating = () => pipeline.activeStep() === "animate" && animationReady();
  /** Qué falta para animar, dicho para el usuario (`undefined` si nada) */
  const animationBlocker = createMemo(() => {
    if (!skeletonData()) return "Falta el esqueleto: elige una plantilla en la sección Esqueleto.";
    if (!meshLoaded()) return undefined;
    const w = weightsData();
    if (autorigComplete() && w && w.numBones !== skeletonData()!.bones.length) {
      return `Los pesos son de otro esqueleto (${w.numBones} huesos; el esqueleto tiene ${skeletonData()!.bones.length}): vuelve a calcularlos en Esqueleto → Pesos.`;
    }
    if (autorigComplete() && w) return undefined;
    if (autorigError()) return `El último cálculo de pesos falló: ${autorigError()}`;
    if (weightsNotice()) return `${weightsNotice()} y los pesos se descartaron: vuelve a calcularlos en Esqueleto → Pesos.`;
    return "Faltan los pesos: calcúlalos en Esqueleto → 4. Pesos (después se pueden pintar).";
  });

  // Fuera del paso Animar, las animaciones del modelo se reproducen una tras
  // otra con la barra de reproducción (sin editar keys)
  const [previewTime, setPreviewTime] = createSignal<number | null>(null);
  const [previewPlaying, setPreviewPlaying] = createSignal(false);
  const [previewLoop, setPreviewLoop] = createPersisted("playback.loop", true);
  /** Clips con keys en fila, cada uno desde donde termina el anterior (segundos) */
  const sequence = createMemo(() => {
    let start = 0;
    return clips()
      .filter((c) => c.tracks.length > 0 && c.fps > 0)
      .map((clip) => {
        const duration = (clip.end - clip.start + 1) / clip.fps;
        const segment = { clip, start, duration };
        start += duration;
        return segment;
      });
  });
  const sequenceLength = () => sequence().reduce((sum, s) => sum + s.duration, 0);
  const playbackSegments = createMemo<PlaybackSegment[]>(() =>
    sequence().map((s) => ({ name: s.clip.name, start: s.start, duration: s.duration }))
  );
  const showPlayback = () => pipeline.activeStep() !== "animate" && animationReady() && sequence().length > 0;
  /** La barra muestra una pose (reproduciendo o en pausa fuera del reposo) */
  const previewing = () => showPlayback() && previewTime() !== null;
  const togglePreview = () => {
    if (previewPlaying()) return setPreviewPlaying(false);
    if (previewTime() === null || previewTime()! >= sequenceLength()) setPreviewTime(0);
    setPreviewPlaying(true);
  };
  const stopPreview = () => {
    setPreviewPlaying(false);
    setPreviewTime(null);
  };
  // Sin barra (paso Animar, sin rig): vuelve al reposo
  createEffect(() => {
    if (!showPlayback()) untrack(stopPreview);
  });

  // El visor entra en modo animación en el paso Animar o al reproducir con la
  // barra (y rehace el rig solo cuando cambian malla, pesos o esqueleto)
  createEffect(() => {
    const v = viewer();
    if (!v) return;
    const on = animating();
    const preview = previewing();
    untrack(() => {
      if (!on) setPlaying(false);
      v.setAnimationMode(on || preview);
      // La primera vez ya hay una animación donde poner keys
      if (on && clips().length === 0) handleNewClip();
    });
  });

  // Pose del cuadro actual
  createEffect(() => {
    const v = viewer();
    if (!v) return;
    weightsData();
    const ctx = rigCtx();
    if (animating()) {
      if (puppeteering()) return;
      // La pose de las keys (FK) queda a mano para rehacer el IK mientras se arrastra un control
      if (mixing()) {
        const { pose, top } = mixKeys(displayMixer(), clips(), frame(), ctx);
        lastFkPose = pose;
        v.setPose(runPoseStack(clonePose(pose), ctx, top ? { clip: top.clip, frame: top.frame } : {}));
      } else {
        const clip = clipPreview() ?? activeClip();
        lastFkPose = samplePose(clip, frame(), ctx.boneIndex, ctx.rotation);
        v.setPose(runPoseStack(clonePose(lastFkPose), ctx, { clip, frame: frame() }));
      }
      setPoseTick((t) => t + 1);
      return;
    }
    const t = previewTime();
    if (!previewing() || t === null) return;
    const segments = sequence();
    const segment = segments.find((s) => t < s.start + s.duration) ?? segments[segments.length - 1];
    const local = Math.min(t - segment.start, segment.duration);
    v.setPose(evaluatePose(segment.clip, segment.clip.start + local * segment.clip.fps, ctx));
  });

  /** Pose de las keys en el cuadro actual, antes del IK */
  let lastFkPose: Pose = emptyPose();

  /** IK en vivo: un control se mueve y las cadenas que lo usan lo siguen */
  const handleControlsDragged = () => {
    const v = viewer();
    if (!v || !animating() || (rigSettings().ikChains ?? []).length === 0) return;
    const pose = clonePose(lastFkPose);
    pose.controls = v.getPose().controls;
    v.setJointsPose(runPoseStack(pose, rigCtx(), { clip: activeClip(), frame: frame() }));
  };

  // Reproducción de la barra: al terminar vuelve a empezar (repetir) o se detiene
  createEffect(() => {
    if (!previewPlaying()) return;
    let last = performance.now();
    let handle = requestAnimationFrame(function tick(now: number) {
      const total = untrack(sequenceLength);
      let t = (untrack(previewTime) ?? 0) + (now - last) / 1000;
      last = now;
      if (t >= total) {
        if (!untrack(previewLoop)) {
          setPreviewTime(total);
          setPreviewPlaying(false);
          return;
        }
        t = total > 0 ? t % total : 0;
      }
      setPreviewTime(t);
      handle = requestAnimationFrame(tick);
    });
    onCleanup(() => cancelAnimationFrame(handle));
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
      // Con la mezcla, se recorren todas las capas
      const [start, end] = untrack(mixing) ? untrack(mixerView) : [clip.start, clip.end];
      if (next >= end + 1 || next < start) next = start + Math.max(0, next - end - 1);
      setFrame(next);
      handle = requestAnimationFrame(tick);
    });
    onCleanup(() => {
      cancelAnimationFrame(handle);
      setFrame((f) => Math.round(f));
    });
  });

  /**
   * Filas de la línea de tiempo: articulaciones con algo que girar, en orden
   * de árbol (sin las ocultas), y después los controles
   */
  const timelineRows = createMemo<TimelineRow[]>(() => {
    const bones = skeletonData()?.bones ?? [];
    const s = rigSettings();
    const children = bones.map((_, j) => bones.flatMap((b, i) => (b.parent === j ? [i] : [])));
    const rows: TimelineRow[] = [];
    const visit = (j: number, depth: number) => {
      const animatable = bones[j].parent === null || children[j].length > 0;
      if (animatable && !isBoneHidden(s, bones[j].name)) rows.push({ joint: j, bone: bones[j].name, depth });
      children[j].forEach((c) => visit(c, depth + 1));
    };
    bones.forEach((b, j) => b.parent === null && visit(j, 0));
    for (const c of s.controls) rows.push({ joint: -1, bone: c.id, label: c.name, depth: 0, control: true });
    for (const c of s.ikChains ?? []) rows.push({ joint: -1, bone: c.id, label: c.name, depth: 0, ik: true });
    for (const c of s.constraints ?? []) rows.push({ joint: -1, bone: c.id, label: c.name, depth: 0, ik: true, constraint: true });
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

  /** Silencia o reactiva pistas desde la línea de tiempo; `branch` suma los huesos que cuelgan de cada fila */
  const handleMuteRows = (rows: TimelineRow[], muted: boolean, branch: boolean) => {
    const bones = skeletonData()?.bones ?? [];
    const refs: TrackRef[] = [];
    const addBranch = (j: number) => {
      refs.push({ bone: bones[j].name });
      bones.forEach((b, i) => b.parent === j && addBranch(i));
    };
    for (const row of rows) {
      if (row.group) refs.push(...row.group.members.map((bone) => ({ bone })));
      else if (row.control) refs.push({ bone: row.bone, kind: "control" });
      else if (row.ik) refs.push({ bone: row.bone, kind: "ik" });
      else if (branch && bones[row.joint]) addBranch(row.joint);
      else refs.push({ bone: row.bone });
    }
    void editClip(muted ? "Silenciar pista" : "Activar pista", (clip) => setTracksMuted(clip, refs, muted));
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

  // Animaciones básicas que admite el esqueleto (ver lib/presetAnimations.ts)
  const body = createMemo(() => analyzeBody(skeletonData()?.bones ?? []));
  const presetAnimations = createMemo(() => {
    const b = body();
    return b ? availableAnimations(b) : [];
  });

  const handleAddPresetClip = async (id: PresetAnimationId) => {
    const b = body();
    const preset = presetAnimations().find((p) => p.id === id);
    if (!b || !preset) return;
    const before = clips();
    const active = activeClip();
    // La acción vacía que se crea al entrar al paso sobra
    const kept = active && active.tracks.length === 0 ? before.filter((c) => c.id !== active.id) : before;
    const names = new Set(kept.map((c) => c.name));
    let name = preset.name;
    for (let i = 2; names.has(name); i++) name = `${preset.name} ${i}`;
    // Son keys FK: las cadenas de IK las siguen solas mientras no se muevan sus controles
    const clip = generateAnimation(b, id, { verticalSwim: selectedSkeleton() === "plan:dolphin" || bodyPlan()?.flukes === true }, name);
    await history.execute(`Animación: ${name}`, {
      kind: "clips",
      data: { before, after: [...kept, clip], activeBefore: active?.id, activeAfter: clip.id },
    });
    setKeySelection(new Set<string>());
    setFrame(clip.start);
  };

  /**
   * Keys en el cuadro actual con la pose que muestra el visor. El
   * desplazamiento solo se guarda en la raíz, en las articulaciones corridas
   * y en las que ya tienen keys de desplazamiento.
   */
  const insertKeysFor = (joints: number[], controls: string[] = []) => {
    const v = viewer();
    const bones = skeletonData()?.bones;
    if (!v || !bones || !animating()) return;
    if (!activeClip()) handleNewClip();
    const clip = activeClip();
    const s = rigSettings();
    const entries = [
      ...joints.flatMap((j) => {
        const pose = v.getJointPose(j);
        if (!pose || !bones[j]) return [];
        const name = bones[j].name;
        const track = clip?.tracks.find((t) => t.bone === name && !t.kind);
        const moved = Math.hypot(...pose.translation) > 1e-9;
        const withOffset =
          !boneProps(s, name).lockTranslation && (bones[j].parent === null || moved || (track?.translation.length ?? 0) > 0);
        return [{ bone: name, rotation: pose.rotation, translation: withOffset ? pose.translation : undefined }];
      }),
      ...controls.map((id) => ({ bone: id, kind: "control" as const, ...v.getControlPose(id) })),
    ];
    if (entries.length === 0) return;
    const f = Math.round(frame());
    void editClip(entries.length === 1 ? "Insertar key" : "Insertar keys", (c) =>
      insertKeys(c, f, entries, keyInterpolation())
    );
    const label = (e: (typeof entries)[number]) => ("kind" in e ? s.controls.find((c) => c.id === e.bone)?.name ?? e.bone : e.bone);
    setStatusMessage(
      entries.length === 1 ? `Key de ${label(entries[0])} en el cuadro ${f}` : `Keys de ${entries.length} articulaciones en el cuadro ${f}`
    );
  };

  /** I: el control o las articulaciones elegidas, o todas si no hay selección */
  const handleInsertKey = () => {
    const control = selectedControl();
    if (control) return insertKeysFor([], [control]);
    const selected = selectedJoints().length > 0 ? selectedJoints() : viewSettings().selectedBone >= 0 ? [viewSettings().selectedBone] : [];
    insertKeysFor(selected.length > 0 ? selected : timelineRows().flatMap((r) => (r.control ? [] : [r.joint])));
  };

  const handlePoseEdited = (joints: number[]) => {
    liveEdit = null;
    setPoseTick((t) => t + 1);
    if (autoKey()) insertKeysFor(joints);
  };

  const handleControlPoseEdited = (id: string) => {
    if (autoKey()) insertKeysFor([], [id]);
  };

  /**
   * Cambio de pose hecho por una herramienta (reiniciar, espejo, pegar,
   * biblioteca…): se ve en el visor y, con auto-key, queda en keys
   */
  const applyPoseEdit = (pose: Pose, status?: string) => {
    const v = viewer();
    if (!v || !animating()) return;
    v.applyPartialPose(pose);
    setPoseTick((t) => t + 1);
    const joints = [...new Set([...pose.rotations.keys(), ...pose.translations.keys()])];
    if (autoKey()) insertKeysFor(joints, [...pose.controls.keys()]);
    if (status) setStatusMessage(status);
  };

  // ─── Rig: propiedades de los huesos, grupos, controles y reposo ───────────

  const changeRig = (description: string, next: RigSettings) =>
    history.execute(description, { kind: "rig", data: { before: rigSettings(), after: next } });

  /** Articulaciones elegidas (la activa incluida), o ninguna */
  const jointSelection = () => {
    const active = viewSettings().selectedBone;
    const set = new Set(selectedJoints());
    if (active >= 0) set.add(active);
    return [...set].filter((j) => j < rigBones().length);
  };

  /** Resortes (F8) en los apéndices que cuelgan: orejas, colas, antenas, trompas y tentáculos sin IK */
  const handleAutoSprings = () => {
    const ctx = rigCtx();
    const body = ctx.body;
    if (!body) {
      setStatusMessage("No se reconoció el cuerpo: pon los resortes a mano en cada hueso");
      return;
    }
    const withIk = new Set((ctx.settings.ikChains ?? []).filter((c) => !c.disabled).flatMap((c) => c.joints));
    const presets = { ear: "jiggle", antenna: "jiggle", tail: "follow", trunk: "follow", tentacle: "follow" } as const;
    let settings = rigSettings();
    let count = 0;
    for (const chain of body.chains) {
      const preset = presets[chain.kind as keyof typeof presets];
      if (!preset) continue;
      const names = chain.rotating.map((j) => ctx.bones[j].name).filter((n) => !withIk.has(n));
      if (names.length === 0) continue;
      const spring = SPRING_PRESETS.find((x) => x.id === preset)!.settings;
      settings = withBoneProps(settings, names, { spring: { ...spring } });
      count += names.length;
    }
    if (count === 0) {
      setStatusMessage("No hay orejas, colas, antenas ni trompas sin IK para poner resortes");
      return;
    }
    void changeRig("Resortes automáticos", settings);
    setStatusMessage(`Resortes en ${count} articulaciones: se ven al reproducir (se hornean al exportar)`);
  };

  const changeBoneProps = (description: string, change: Partial<BoneProps>) => {
    const names = jointSelection().map((j) => rigBones()[j].name);
    if (names.length > 0) void changeRig(description, withBoneProps(rigSettings(), names, change));
  };

  /** Roll de los huesos elegidos: automático, hacia la cámara, la superficie o el espejo */
  const handleRoll = (mode: RollMode) => {
    const ctx = rigCtx();
    const v = viewer();
    let next = rigSettings();
    for (const j of jointSelection()) {
      const frame = ctx.frames[j];
      let roll: number | null = 0;
      if (mode === "view" && v) roll = rollToward(frame, vec.cross(frame.y, v.cameraDirection()));
      else if (mode === "normal" && v) {
        const n = v.nearestNormal(ctx.bones[j].position);
        roll = n ? rollToward(frame, vec.cross(frame.y, n)) : null;
      } else if (mode === "mirror") {
        const m = ctx.mirror[j];
        roll = m === null ? null : rollToward(frame, vec.scale(reflectVector(ctx.frames[m].x, ctx.symmetry.normal), -1));
      }
      if (roll !== null) next = withBoneProps(next, [ctx.bones[j].name], { roll: Math.abs(roll) < 1e-9 ? undefined : roll });
    }
    if (next !== rigSettings()) void changeRig("Roll", next);
  };

  /** Giro (ejes locales, grados) y desplazamiento de la articulación activa, como los muestra el visor */
  const activeJointPose = createMemo(() => {
    poseTick();
    const v = viewer();
    const j = viewSettings().selectedBone;
    const ctx = rigCtx();
    if (!v || !animating() || j < 0 || !ctx.bones[j]) return undefined;
    const pose = v.getJointPose(j);
    if (!pose) return undefined;
    const mode = boneProps(ctx.settings, ctx.bones[j].name).rotationMode;
    return { rotation: eulerDegrees(pose.rotation, ctx.frames[j], mode), translation: pose.translation };
  });

  const setActiveJointPose = (change: { rotation?: Vec3; translation?: Vec3 }) => {
    const j = viewSettings().selectedBone;
    const ctx = rigCtx();
    if (j < 0 || !ctx.bones[j]) return;
    const pose = emptyPose();
    if (change.rotation) {
      const p = boneProps(ctx.settings, ctx.bones[j].name);
      const q = fromEulerDegrees(change.rotation, ctx.frames[j], p.rotationMode);
      pose.rotations.set(j, applyRotationLocks(q, ctx.frames[j], p.lockRotation, p.rotationMode));
    }
    if (change.translation) pose.translations.set(j, change.translation);
    applyPoseEdit(pose);
  };

  const handleAutoGroups = () => void changeRig("Grupos automáticos", withAutoGroups(rigSettings(), rigBones()));

  const handleNewGroup = () => {
    const names = jointSelection().map((j) => rigBones()[j].name);
    if (names.length === 0) return;
    const s = rigSettings();
    const id = newRigId("group");
    const hue = (s.groups.length * 67) % 360;
    const color = `#${new THREE.Color().setHSL(hue / 360, 0.65, 0.58).getHexString()}`;
    const next = withBoneProps({ ...s, groups: [...s.groups, { id, name: `Grupo ${s.groups.length + 1}`, color }] }, names, { group: id });
    void changeRig("Grupo nuevo", next);
  };

  const handleSelectGroup = (groupId: string) => {
    const s = rigSettings();
    const joints = rigBones().flatMap((b, i) => (s.bones[b.name]?.group === groupId && !isBoneHidden(s, b.name) ? [i] : []));
    if (joints.length > 0) selectJoints(joints, joints[0]);
  };

  /** Selección desde la app (panel, línea de tiempo, atajos): el visor la muestra */
  const selectJoints = (joints: number[], active: number) => {
    setSelectedControl(undefined);
    viewerRef?.setSelection(joints, active);
    setSelectedJoints(viewerRef?.getSelection() ?? joints);
    setViewSettings((prev) => ({ ...prev, selectedBone: active }));
  };

  const handleSelectControl = (id: string | null) => {
    setSelectedControl(id ?? undefined);
    viewerRef?.selectControl(id);
    if (id) {
      setSelectedJoints([]);
      setViewSettings((prev) => ({ ...prev, selectedBone: -1 }));
    }
  };

  const handleAddControl = async () => {
    const ctx = rigCtx();
    const s = rigSettings();
    const j = viewSettings().selectedBone;
    const bone = ctx.bones[j];
    // En la articulación activa, o en el centro del esqueleto
    const center = ([0, 1, 2] as const).map(
      (k) => ctx.bones.reduce((sum, b) => sum + b.position[k], 0) / Math.max(ctx.bones.length, 1)
    ) as Vec3;
    const position: Vec3 = bone ? [...bone.position] : center;

    const control: RigControl = {
      id: newRigId("ctl"),
      name: bone ? `${bone.name}_ctl` : `Control ${s.controls.length + 1}`,
      parent: bone?.name ?? null,
      shape: "circle",
      position,
      size: (ctx.body?.size ?? 1) * 0.05,
    };
    await changeRig("Agregar control", { ...s, controls: [...s.controls, control] });
    handleSelectControl(control.id);
  };

  // ─── Estructura del esqueleto ─────────────────────────────────────────────

  /** Esqueletos guardados como plantilla propia (para otros modelos) */
  const [customSkeletons, setCustomSkeletons] = createPersisted<{ id: string; name: string; bones: EditBone[] }[]>("skeleton.custom", []);

  const editBones = (): EditBone[] =>
    (skeletonData()?.bones ?? []).map((b) => ({ name: b.name, position: [...b.position] as Vec3, parent: b.parent }));

  /** Manda la estructura nueva al backend; los pesos ya no sirven */
  const sendSkeletonBones = async (bones: EditBone[]) => {
    const data = await invoke<TauriSkeletonData>("set_skeleton_bones", { bones });
    setSkeletonData(tauriSkeletonToViewer(data));
    setSkeletonLoaded(true);
    setSelectedSkeleton(undefined);
    setBodyPlan(undefined);
    setFitInfo(undefined);
    dropWeights("Cambiaste la estructura del esqueleto");
  };

  const changeSkeletonBones = async (description: string, after: EditBone[], select?: number) => {
    try {
      await history.execute(description, { kind: "skeletonBones", data: { before: editBones(), after } });
      if (select !== undefined) selectJoints([select], select);
      setStatusMessage(`${description}: ${after.length} huesos`);
      return true;
    } catch (e) {
      console.error("Skeleton edit error:", e);
      setStatusMessage(`Error: ${e}`);
      return false;
    }
  };

  const handleAddChildBone = () => {
    const j = viewSettings().selectedBone;
    const bones = editBones();
    if (!bones[j]) return;
    const { bones: after, index } = addChildBone(bones, j, bodyAxes(rigCtx().body).up);
    void changeSkeletonBones(`Hueso nuevo en ${bones[j].name}`, after, index);
  };

  const handleDeleteBone = () => {
    const j = viewSettings().selectedBone;
    const bones = editBones();
    if (!bones[j]) return;
    const after = removeBone(bones, j);
    if (!after) {
      setStatusMessage("El esqueleto necesita al menos un hueso");
      return;
    }
    void changeSkeletonBones(`Borrar ${bones[j].name}`, after);
  };

  /** Renombra un hueso y lo que lo nombra: propiedades del rig, cadenas, restricciones, controles y pistas */
  const handleRenameBone = async (name: string) => {
    const j = viewSettings().selectedBone;
    const bones = editBones();
    const from = bones[j]?.name;
    if (!from || name === from) return;
    if (bones.some((b) => b.name === name)) {
      setStatusMessage(`Ya hay un hueso «${name}»`);
      return;
    }
    const after = bones.map((b, i) => (i === j ? { ...b, name } : b));
    if (!(await changeSkeletonBones(`Renombrar ${from} a ${name}`, after, j))) return;
    const s = rigSettings();
    const rename = (n: string) => (n === from ? name : n);
    const renamedBones = { ...s.bones };
    if (renamedBones[from]) {
      renamedBones[name] = renamedBones[from];
      delete renamedBones[from];
    }
    await changeRig("Renombrar en el rig", {
      ...s,
      bones: renamedBones,
      controls: s.controls.map((c) => (c.parent === from ? { ...c, parent: name } : c)),
      ikChains: (s.ikChains ?? []).map((c) => ({ ...c, joints: c.joints.map(rename) })),
      constraints: s.constraints?.map((c) => ({
        ...c,
        owner: rename(c.owner),
        target: c.target === from ? name : c.target,
        joints: c.joints?.map(rename),
      })),
    });
    const before = clips();
    const renamed = before.map((c) => ({ ...c, tracks: c.tracks.map((t) => (!t.kind && t.bone === from ? { ...t, bone: name } : t)) }));
    await history.execute("Renombrar en las animaciones", {
      kind: "clips",
      data: { before, after: renamed, activeBefore: activeClipId(), activeAfter: activeClipId() },
    });
  };

  const handleExportSkeletonJson = async () => {
    const bones = editBones();
    if (bones.length === 0) return;
    const stem = (fileName() ?? "esqueleto").replace(/\.[^.]+$/, "");
    const path = await save({ title: "Exportar esqueleto", defaultPath: `${stem} - esqueleto.json`, filters: [{ name: "Esqueleto JSON", extensions: ["json"] }] });
    if (!path) return;
    try {
      await invoke("write_text_file", { path, contents: skeletonToJson(stem, bones) });
      setStatusMessage(`Esqueleto exportado: ${path}`);
    } catch (e) {
      setStatusMessage(`Error: ${e}`);
    }
  };

  const handleImportSkeletonJson = async () => {
    const picked = await pickTextFile(".json");
    if (!picked) return;
    try {
      const { bones } = skeletonFromJson(picked.text);
      if (await changeSkeletonBones(`Esqueleto de ${picked.name}`, bones)) {
        setStatusMessage(`Esqueleto de ${picked.name}: ${bones.length} huesos. Ajústalo al modelo o muévelo a mano`);
      }
    } catch (e) {
      setStatusMessage(`No se pudo leer ${picked.name}: ${e instanceof Error ? e.message : e}`);
    }
  };

  const skeletonEditPanel = (
    <SkeletonEditPanel
      selectedName={skeletonData()?.bones[viewSettings().selectedBone]?.name}
      disabled={isProcessing()}
      onAddChild={handleAddChildBone}
      onDelete={handleDeleteBone}
      onRename={(name) => void handleRenameBone(name)}
      onExportJson={() => void handleExportSkeletonJson()}
      onImportJson={() => void handleImportSkeletonJson()}
      custom={customSkeletons().map((s) => ({ id: s.id, name: s.name, bones: s.bones.length }))}
      onSaveCustom={(name) => {
        setCustomSkeletons([...customSkeletons(), { id: `skel-${Date.now().toString(36)}`, name, bones: editBones() }]);
        setStatusMessage(`Esqueleto guardado: ${name}`);
      }}
      onApplyCustom={(id) => {
        const s = customSkeletons().find((x) => x.id === id);
        if (s) void changeSkeletonBones(`Esqueleto: ${s.name}`, s.bones.map((b) => ({ ...b, position: [...b.position] as Vec3 })));
      }}
      onDeleteCustom={(id) => setCustomSkeletons(customSkeletons().filter((x) => x.id !== id))}
    />
  );

  const sendRestPose = async (positions: Vec3[]) => {
    const data = await invoke<TauriSkeletonData>("apply_rest_pose", { positions });
    setSkeletonData(tauriSkeletonToViewer(data));
  };

  const handleApplyRest = async () => {
    const v = viewer();
    const skeleton = skeletonData();
    if (!v || !skeleton || !animating()) return;
    const from = skeleton.bones.map((b) => [...b.position] as Vec3);
    const to = v.getPosedJointPositions();
    try {
      await history.execute("Aplicar pose como reposo", { kind: "restPose", data: { from, to } });
      setStatusMessage(
        clips().some((c) => c.tracks.length > 0)
          ? "Pose aplicada como reposo: las keys ahora giran desde el reposo nuevo"
          : "Pose aplicada como reposo"
      );
    } catch (e) {
      setStatusMessage(`Error: ${e}`);
    }
  };

  /** Todas las articulaciones y controles al reposo */
  const handleClearPose = () => {
    const v = viewer();
    if (!v) return;
    const current = v.getPose();
    const reset = emptyPose();
    for (const j of current.rotations.keys()) reset.rotations.set(j, [0, 0, 0, 1]);
    for (const j of current.translations.keys()) reset.translations.set(j, [0, 0, 0]);
    for (const id of current.controls.keys()) reset.controls.set(id, { rotation: [0, 0, 0, 1], translation: [0, 0, 0] });
    applyPoseEdit(reset, "Pose de reposo");
  };

  const rigPanel = () => (
    <RigPanel
      boneNames={rigBones().map((b) => b.name)}
      settings={rigSettings()}
      selection={jointSelection()}
      active={viewSettings().selectedBone}
      hasMirror={(rigCtx().mirror[viewSettings().selectedBone] ?? null) !== null}
      hasMesh={meshLoaded()}
      rotation={activeJointPose()?.rotation}
      translation={activeJointPose()?.translation}
      posing={animating()}
      selectedControl={selectedControl()}
      onChange={(description, next) => void changeRig(description, next)}
      onBoneProps={changeBoneProps}
      onRoll={handleRoll}
      onRotation={(rotation) => setActiveJointPose({ rotation })}
      onTranslation={(translation) => setActiveJointPose({ translation })}
      onAutoGroups={handleAutoGroups}
      onSelectGroup={handleSelectGroup}
      onNewGroup={handleNewGroup}
      onAddControl={() => void handleAddControl()}
      onSelectControl={handleSelectControl}
      onApplyRest={() => void handleApplyRest()}
      onAutoSprings={handleAutoSprings}
      onClearPose={handleClearPose}
    />
  );

  const handleDeleteKeys = () => {
    if (timelineMode() === "curves") {
      const refs = curveRefs();
      if (refs.size === 0) return;
      void editClip("Borrar keys", (c) => deleteKeyRefs(c, refs, rigCtx().rotation));
      setCurveSelection(new Set<string>());
      return;
    }
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
    if (timelineMode() === "curves") {
      const refs = curveRefs();
      if (refs.size > 0) void editClip("Interpolación", (c) => setInterpolation(c, refs, interpolation, rigCtx().rotation));
      return;
    }
    const selection = keySelection();
    if (selection.size > 0) void editClip("Interpolación", (c) => setKeysInterpolation(c, selection, interpolation));
  };

  // ─── Edición de animación (F5): curvas, herramientas, marcadores ──────────

  /** Referencias (key de un grupo en un cuadro) de la selección del editor de curvas */
  const curveRefs = () => new Set([...curveSelection()].map((id) => keyRefId(parseCurveKeyId(id).ref)));
  /** Referencias de lo elegido en la vista activa */
  const toolRefs = () => {
    const clip = activeClip();
    if (!clip) return new Set<string>();
    return timelineMode() === "curves" ? curveRefs() : dopeToRefs(clip, keySelection());
  };

  /** Pistas que se ven en el editor de curvas: las elegidas o, sin selección, todas las que tienen keys */
  const curveTracks = createMemo(() => {
    const clip = clipPreview() ?? activeClip();
    if (!clip) return [];
    const names = new Set(jointSelection().map((j) => rigBones()[j]?.name).filter(Boolean));
    const control = selectedControl();
    const chain = selectedChain();
    const picked = clip.tracks.filter((t) =>
      t.kind === "control" ? t.bone === control : t.kind === "ik" ? t.bone === chain || t.bone === selectedConstraint() : names.has(t.bone)
    );
    // Sin selección, o si lo elegido no tiene keys, se ven todas las pistas
    return picked.length > 0 ? picked : clip.tracks;
  });

  const CHANNEL_COLORS = ["var(--color-red)", "var(--color-green)", "var(--color-cyan)"];
  const curveRows = createMemo<CurveChannelRow[]>(() => {
    const s = rigSettings();
    const rows: CurveChannelRow[] = [];
    for (const t of curveTracks()) {
      const owner =
        t.kind === "control"
          ? (s.controls.find((c) => c.id === t.bone)?.name ?? t.bone)
          : t.kind === "ik"
            ? (s.ikChains?.find((c) => c.id === t.bone)?.name ?? s.constraints?.find((c) => c.id === t.bone)?.name ?? t.bone)
            : t.bone;
      for (const group of ["rotation", "translation", "blend", "pin", "roll"] as ChannelGroup[]) {
        const keys = group === "rotation" ? t.rotation : group === "translation" ? t.translation : t[group];
        if (!keys || keys.length === 0) continue;
        GROUP_CHANNELS[group].forEach((label, channel) =>
          rows.push({
            id: `${t.kind ?? "b"}|${t.bone}|${group}|${channel}`,
            track: { bone: t.bone, kind: t.kind },
            group,
            channel,
            label,
            owner,
            color: group === "rotation" || group === "translation" ? CHANNEL_COLORS[channel] : "var(--color-yellow)",
          })
        );
      }
    }
    return rows;
  });

  const handleCurveEdit = (next: AnimationClip, commit: boolean, description: string) => {
    if (!commit) return void setClipPreview(next);
    setClipPreview(undefined);
    void editClip(description, () => next);
  };

  const handleTimelineTool = (tool: TimelineTool) => {
    const clip = activeClip();
    if (!clip) return;
    const rot = rigCtx().rotation;
    const refs = toolRefs();
    /** Sin selección, los filtros van sobre todo lo que se ve */
    const target = () => (refs.size > 0 ? refs : allRefs(clip, new Set(curveTracks().map((t) => `${t.kind ?? "b"}|${t.bone}`))));
    const needs = () => {
      if (refs.size > 0) return true;
      setStatusMessage("Elige keys primero");
      return false;
    };
    const clearSelections = () => {
      setKeySelection(new Set<string>());
      setCurveSelection(new Set<string>());
    };
    switch (tool.op) {
      case "handles":
        if (needs()) void editClip("Manijas", (c) => setHandleMode(c, refs, tool.value, rot));
        break;
      case "ease":
        if (needs()) void editClip({ in: "Acelerar", out: "Frenar", both: "Acelerar y frenar", none: "Sin aceleración" }[tool.value], (c) => setEase(c, refs, tool.value, rot));
        break;
      case "scale":
        if (!(tool.factor > 0)) {
          setStatusMessage("Escalar: el factor tiene que ser mayor que 0");
          break;
        }
        if (needs()) {
          void editClip("Escalar keys", (c) => scaleKeys(c, refs, Math.round(frame()), tool.factor));
          clearSelections();
        }
        break;
      case "retime": {
        if (!(tool.factor > 0)) {
          setStatusMessage("Retiempo: el factor tiene que ser mayor que 0");
          break;
        }
        if (!needs()) break;
        const frames = [...refs].map((id) => parseKeyRef(id).frame);
        const [a, b] = [Math.min(...frames), Math.max(...frames)];
        if (b <= a) {
          setStatusMessage("Retiempo: elige keys de al menos dos cuadros");
          break;
        }
        void editClip("Retiempo", (c) => retime(c, a, b, tool.factor));
        clearSelections();
        break;
      }
      case "closeCycle": {
        const tracks = new Set(curveTracks().map((t) => `${t.kind ?? "b"}|${t.bone}`));
        void editClip("Cerrar ciclo", (c) => closeCycle(c, jointSelection().length > 0 || selectedControl() ? tracks : null, rot));
        break;
      }
      case "eulerFilter":
        void editClip("Filtro Euler", (c) => eulerFilter(c, target(), rot));
        break;
      case "smooth":
        void editClip("Suavizar", (c) => smoothKeys(c, target(), 0.6, rot));
        break;
      case "noise":
        if (needs()) void editClip("Ruido", (c) => noiseKeys(c, refs, tool.amount, rot, Date.now() % 100000));
        break;
      case "reduce":
        void editClip("Reducir keys", (c) => reduceSelectedKeys(c, target(), tool.tolerance, rot));
        clearSelections();
        break;
      case "marker": {
        const f = Math.round(frame());
        void editClip("Marcador", (c) => withMarker(c, f, `M${(c.markers?.length ?? 0) + 1}`));
        break;
      }
      case "interpolation":
        handleKeyInterpolation(tool.value);
        break;
    }
  };

  const handleMarker = (change: { rename?: { frame: number; name: string }; move?: { from: number; to: number }; remove?: number }) => {
    if (change.rename) void editClip("Renombrar marcador", (c) => withMarker(c, change.rename!.frame, change.rename!.name));
    if (change.remove !== undefined) void editClip("Borrar marcador", (c) => withoutMarker(c, change.remove!));
    if (change.move) {
      const { from, to } = change.move;
      void editClip("Mover marcador", (c) => {
        const m = c.markers?.find((x) => x.frame === from);
        return m ? withMarker(withoutMarker(c, from), to, m.name) : c;
      });
    }
  };

  /**
   * Filas de la hoja de claves: por grupo (resumen plegable) o como el árbol,
   * y con "solo elegidas" únicamente las articulaciones de la selección
   */
  const timelineDisplayRows = createMemo<TimelineRow[]>(() => {
    const s = rigSettings();
    const selected = new Set(jointSelection());
    let rows = timelineRows().filter((r) => !onlySelection() || r.control || r.ik || selected.has(r.joint));
    if (onlySelection()) {
      rows = rows.filter((r) => (r.control ? r.bone === selectedControl() : r.ik ? r.bone === selectedChain() : true));
    }
    if (s.groups.length === 0) return rows;
    const out: TimelineRow[] = [];
    const used = new Set<TimelineRow>();
    for (const g of s.groups) {
      const members = rows.filter((r) => !r.control && !r.ik && s.bones[r.bone]?.group === g.id);
      if (members.length === 0) continue;
      const collapsed = collapsedGroups().has(g.id);
      out.push({ joint: -1, bone: `grupo:${g.id}`, label: g.name, depth: 0, group: { id: g.id, color: g.color, collapsed, members: members.map((m) => m.bone) } });
      members.forEach((m) => used.add(m));
      if (!collapsed) out.push(...members.map((m) => ({ ...m, depth: 1 })));
    }
    out.push(...rows.filter((r) => !used.has(r)).map((r) => ({ ...r, depth: r.control || r.ik ? 0 : 1 })));
    return out;
  });

  // ─── Mezclador ────────────────────────────────────────────────────────────

  /** Cuadros que muestra la línea de tiempo con el mezclador */
  const mixerView = createMemo<[number, number]>(() => {
    const [lo, hi] = mixerRange(displayMixer(), clips());
    const start = Math.min(0, lo);
    return [start, Math.max(hi, start + 48)];
  });

  const changeMixer = (description: string, next: Mixer, commit: boolean) => {
    if (!commit) return void setMixerPreview(next);
    setMixerPreview(undefined);
    void history.execute(description, { kind: "mixer", data: { before: mixer(), after: next } });
  };

  const handleAddLayer = (clipId: string) => {
    const m = mixer();
    const clip = clips().find((c) => c.id === clipId);
    if (!clip) return;
    const [, hi] = mixerRange(m, clips());
    const layer: AnimationLayer = {
      id: newRigId("layer"),
      clipId,
      mode: "replace",
      weight: 1,
      start: m.layers.length === 0 ? 0 : hi + 1,
      repeat: 1,
      blendIn: 0,
      blendOut: 0,
    };
    changeMixer("Agregar capa", { enabled: true, layers: [...m.layers, layer] }, true);
    setSelectedLayer(layer.id);
  };

  const handleBakeMixer = async () => {
    const baked = bakeMixer(mixer(), clips(), rigCtx(), "Mezcla", activeClip()?.fps ?? 24);
    const before = clips();
    await history.execute("Hornear la mezcla", { kind: "clips", data: { before, after: [...before, baked], activeBefore: activeClipId(), activeAfter: baked.id } });
    setMixer({ ...mixer(), enabled: false });
    setTimelineMode("keys");
    setStatusMessage("La mezcla quedó horneada en la animación «Mezcla»");
  };

  // ─── Papel cebolla y trayectorias ─────────────────────────────────────────

  /** Pose final en el cuadro `f`: de la mezcla o del clip */
  const poseAt = (f: number) => {
    const ctx = rigCtx();
    if (mixing()) {
      const { pose, top } = mixKeys(displayMixer(), clips(), f, ctx);
      return runPoseStack(clonePose(pose), ctx, top ? { clip: top.clip, frame: top.frame } : {});
    }
    return evaluatePose(clipPreview() ?? activeClip(), f, ctx);
  };

  createEffect(() => {
    const v = viewer();
    if (!v) return;
    const o = onion();
    const clip = clipPreview() ?? activeClip();
    const f = Math.round(frame());
    if (!animating() || !o.enabled || !clip || playing()) return v.setOnionSkin([], false);
    const [start, end] = mixing() ? mixerView() : [clip.start, clip.end];
    const ghosts: { pose: Pose; color: string; opacity: number }[] = [];
    for (let k = 1; k <= o.before; k++) {
      const g = f - k * o.step;
      if (g >= start) ghosts.push({ pose: poseAt(g), color: "#ff6e6e", opacity: 0.6 * (1 - (k - 1) / (o.before + 1)) });
    }
    for (let k = 1; k <= o.after; k++) {
      const g = f + k * o.step;
      if (g <= end) ghosts.push({ pose: poseAt(g), color: "#69ff94", opacity: 0.6 * (1 - (k - 1) / (o.after + 1)) });
    }
    v.setOnionSkin(ghosts, o.mesh);
  });

  const PATH_COLORS = ["#ffb86c", "#8be9fd", "#ff79c6", "#50fa7b", "#bd93f9", "#f1fa8c"];

  /** Trayectorias de las articulaciones elegidas (y del control): una posición por cuadro */
  createEffect(() => {
    const v = viewer();
    if (!v) return;
    const clip = clipPreview() ?? activeClip();
    if (!animating() || !showPaths() || !clip) return v.setMotionPaths([]);
    const ctx = rigCtx();
    const joints = jointSelection().slice(0, 6);
    const control = selectedControl();
    const [start, end] = mixing() ? mixerView() : [Math.round(clip.start), Math.round(clip.end)];
    const frames: number[] = [];
    for (let f = start; f <= Math.min(end, start + 400); f++) frames.push(f);
    const poses = frames.map((f) => poseAt(f));
    const fks = poses.map((p) => new Fk(ctx.bones, p));
    const keysOf = (names: string[], kind?: "control") =>
      [...new Set(clip.tracks.filter((t) => t.kind === kind && names.includes(t.bone)).flatMap((t) => [...t.rotation, ...t.translation].map((k) => k.frame)))];
    const paths = joints.map((j, i) => {
      // Las keys de la articulación y de las que la mueven (hasta la ramificación)
      const chain = [j];
      for (let p = ctx.bones[j].parent; p !== null && ctx.children[p].length === 1; p = ctx.bones[p].parent) chain.push(p);
      const parent = ctx.bones[chain[chain.length - 1]].parent;
      if (parent !== null) chain.push(parent);
      return {
        id: `j:${j}`,
        points: fks.map((fk) => fk.position(j)),
        frames,
        keys: keysOf(chain.map((c) => ctx.bones[c].name)),
        color: PATH_COLORS[i % PATH_COLORS.length],
      };
    });
    const c = control ? ctx.settings.controls.find((x) => x.id === control) : undefined;
    if (c) {
      paths.push({
        id: `c:${c.id}`,
        points: fks.map((fk, i) => controlWorld(c, ctx, fk, poses[i]).position),
        frames,
        keys: keysOf([c.id], "control"),
        color: "#f8f8f2",
      });
    }
    v.setMotionPaths(paths);
  });

  /** Clic en la key de una trayectoria: a ese cuadro, y a arrastrarla (IK automático o el control) */
  const handlePathKey = (id: string, f: number) => {
    const v = viewer();
    if (!v) return;
    setPlaying(false);
    setFrame(f);
    if (id.startsWith("c:")) {
      setSelectedControl(id.slice(2));
      v.startPathDrag({ control: id.slice(2) });
    } else {
      v.startPathDrag({ joint: Number(id.slice(2)) });
    }
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

  // ─── Herramientas de pose (F1) ────────────────────────────────────────────

  createEffect(() => viewer()?.setTransformSpace(transformSpace()));

  /**
   * Arrastre de un deslizador (intermedia, empujar, mezcla de la biblioteca):
   * la pose de partida se toma al empezar y cada movimiento es una vista
   * previa; al soltar queda en keys. Cualquier otra edición lo corta.
   */
  let liveEdit: { kind: string; base: Pose } | null = null;
  createEffect(on([frame, activeClipId], () => (liveEdit = null), { defer: true }));

  const liveBase = (kind: string): Pose | null => {
    const v = viewer();
    if (!v) return null;
    if (!liveEdit || liveEdit.kind !== kind) liveEdit = { kind, base: v.getPose() };
    return liveEdit.base;
  };

  const livePreview = (pose: Pose, commit: boolean, status?: string) => {
    if (commit) {
      liveEdit = null;
      applyPoseEdit(pose, status);
    } else {
      viewer()?.applyPartialPose(pose);
      setPoseTick((t) => t + 1);
    }
  };

  /** Selección o, sin selección, todas las articulaciones animables */
  const jointsOrAll = () => (jointSelection().length > 0 ? jointSelection() : animatableJoints(rigCtx()));

  const handleSelectCommand = (command: SelectCommand, extend = false) => {
    const ctx = rigCtx();
    const s = ctx.settings;
    const selectable = (j: number) => !isBoneHidden(s, ctx.bones[j].name) && !boneProps(s, ctx.bones[j].name).locked;
    const current = jointSelection();
    const active = viewSettings().selectedBone;
    let next: number[] = [];
    let nextActive = active;
    switch (command) {
      case "all":
        next = ctx.bones.map((_, j) => j).filter(selectable);
        nextActive = active >= 0 ? active : (next[0] ?? -1);
        break;
      case "none":
        nextActive = -1;
        break;
      case "mirror":
        next = current.map((j) => ctx.mirror[j] ?? j);
        nextActive = active >= 0 ? (ctx.mirror[active] ?? active) : -1;
        break;
      case "parent":
        next = current.flatMap((j) => (ctx.bones[j].parent === null ? [j] : [ctx.bones[j].parent!]));
        nextActive = active >= 0 ? (ctx.bones[active].parent ?? active) : -1;
        break;
      case "children": {
        const all = (j: number): number[] => ctx.children[j].flatMap((c) => [c, ...all(c)]);
        next = current.flatMap((j) => (extend ? all(j) : ctx.children[j]));
        nextActive = active >= 0 ? (ctx.children[active][0] ?? active) : -1;
        break;
      }
      case "group": {
        const groups = new Set(current.map((j) => s.bones[ctx.bones[j].name]?.group).filter(Boolean));
        next = ctx.bones.flatMap((b, j) => (groups.has(s.bones[b.name]?.group) ? [j] : []));
        break;
      }
    }
    if (extend && command !== "none") next = [...current, ...next];
    next = [...new Set(next)].filter(selectable);
    if (nextActive >= 0 && !next.includes(nextActive)) nextActive = next[0] ?? -1;
    selectJoints(next, nextActive);
  };

  const handleResetPose = (what: "rotation" | "translation" | "all") => {
    const joints = jointsOrAll();
    const control = selectedControl();
    const controls = control ? [control] : jointSelection().length === 0 ? rigSettings().controls.map((c) => c.id) : [];
    const labels = { rotation: "Giro en reposo", translation: "Posición de reposo", all: "Pose de reposo" };
    applyPoseEdit(resetPose(control ? [] : joints, what, controls), labels[what]);
  };

  const handleMirrorPose = (mode: "flip" | "leftToRight" | "rightToLeft") => {
    const v = viewer();
    if (!v) return;
    const selection = jointSelection();
    const pose = mirrorPose(v.getPose(), rigCtx(), mode, selection.length > 0 ? selection : undefined);
    if (pose.rotations.size === 0 && pose.controls.size === 0) {
      setStatusMessage("Nada que reflejar: el esqueleto no tiene pares izquierda/derecha");
      return;
    }
    const labels = { flip: "Pose volteada", leftToRight: "Izquierda copiada a la derecha", rightToLeft: "Derecha copiada a la izquierda" };
    applyPoseEdit(pose, labels[mode]);
  };

  const handleCopyPose = () => {
    const v = viewer();
    if (!v || !animating()) return;
    const entries = poseToNames(v.getPose(), rigCtx(), jointsOrAll());
    copyPose(entries);
    setCanPaste(true);
    setStatusMessage(`Pose copiada (${Object.keys(entries).length} huesos)`);
  };

  const handlePastePose = (mirrored: boolean) => {
    const entries = readPoseClipboard();
    if (!entries || !animating()) return;
    const ctx = rigCtx();
    const pose = namesToPose(mirrored ? mirrorEntries(entries, ctx) : entries, ctx);
    if (pose.rotations.size === 0) {
      setStatusMessage("La pose copiada no tiene huesos con estos nombres");
      return;
    }
    applyPoseEdit(pose, mirrored ? "Pose pegada del otro lado" : "Pose pegada");
  };

  const handleBreakdown = (t: number, commit: boolean) => {
    const clip = activeClip();
    if (!clip) return;
    const pose = breakdownPose(clip, frame(), jointsOrAll(), rigCtx(), t);
    if (pose.rotations.size === 0) {
      if (commit) setStatusMessage("Intermedia: hace falta una key antes y otra después de este cuadro");
      return;
    }
    livePreview(pose, commit, `Intermedia al ${Math.round(t * 100)} %`);
  };

  const handlePushRelax = (factor: number, commit: boolean) => {
    const base = liveBase("push");
    if (!base) return;
    livePreview(pushRelax(base, jointsOrAll(), factor), commit, factor >= 1 ? "Pose empujada" : "Pose relajada");
  };

  /** Pose de la biblioteca o de fábrica, lista para aplicar */
  const sourcePose = (source: PoseSource): Pose | null => {
    const ctx = rigCtx();
    if (source.kind === "stored") {
      const stored = poseLibrary().find((p) => p.id === source.id);
      return stored ? namesToPose(stored.bones, ctx) : null;
    }
    const b = body();
    if (!b) return null;
    const generated = generatePose(b, source.id);
    const pose = emptyPose();
    generated.rotations.forEach((q, j) => pose.rotations.set(j, q));
    if (generated.offset) pose.translations.set(b.root, generated.offset);
    return pose;
  };

  const handleApplyLibraryPose = (source: PoseSource, factor: number, commit: boolean) => {
    const target = sourcePose(source);
    const base = liveBase("library");
    if (!target || !base) return;
    livePreview(blendPose(base, target, factor), commit, "Pose de la biblioteca aplicada");
  };

  const handleSavePose = async (selectionOnly: boolean) => {
    const v = viewer();
    if (!v || !animating()) return;
    const joints = selectionOnly ? jointSelection() : animatableJoints(rigCtx());
    const pose: StoredPose = {
      id: newRigId("pose"),
      name: `Pose ${poseLibrary().length + 1}`,
      thumbnail: v.snapshot(96),
      bones: poseToNames(v.getPose(), rigCtx(), joints),
    };
    await history.execute("Guardar pose", { kind: "poseLibrary", data: { before: poseLibrary(), after: [...poseLibrary(), pose] } });
    setStatusMessage(`${pose.name} guardada en la biblioteca`);
  };

  const changePoseLibrary = (description: string, after: StoredPose[]) =>
    void history.execute(description, { kind: "poseLibrary", data: { before: poseLibrary(), after } });

  /** Resumen de la articulación activa para la barra de estado */
  // ─── IK (F2) ──────────────────────────────────────────────────────────────

  createEffect(() => viewer()?.setAutoIk(autoIk()));
  createEffect(() => viewer()?.setDualQuaternion(!!rigSettings().dualQuaternion));

  const chainById = (id?: string) => (rigSettings().ikChains ?? []).find((c) => c.id === id);

  /** Mezcla, fijado y balanceo de la cadena elegida en el cuadro actual */
  const chainValues = createMemo(() => {
    const chain = chainById(selectedChain());
    const clip = activeClip();
    const ctx = rigCtx();
    const f = Math.round(frame());
    const keyed = (channel: ScalarChannel) =>
      !!clip?.tracks.find((t) => t.kind === "ik" && t.bone === chain?.id)?.[channel]?.length;
    return {
      blend: chain ? chainBlend(chain, clip, f, samplePose(clip, f, ctx.boneIndex, ctx.rotation), chain.joints.flatMap((n) => ctx.boneIndex.get(n) ?? [])) : 1,
      pinned: chain ? (sampleScalar(clip, chain.id, "pin", f) ?? 0) >= 0.5 : false,
      roll: chain ? (sampleScalar(clip, chain.id, "roll", f) ?? chain.roll ?? 0) : 0,
      keyed: { blend: keyed("blend"), pin: keyed("pin"), roll: keyed("roll") },
    };
  });

  const changeChain = (description: string, chain: IkChain) =>
    void changeRig(description, {
      ...rigSettings(),
      ikChains: (rigSettings().ikChains ?? []).map((c) => (c.id === chain.id ? chain : c)),
    });

  // ─── Restricciones (F3) ───────────────────────────────────────────────────

  const constraintById = (id?: string) => (rigSettings().constraints ?? []).find((c) => c.id === id);

  /** Influencia y valor del driver de la restricción elegida en el cuadro actual */
  const constraintValues = createMemo(() => {
    const c = constraintById(selectedConstraint());
    const clip = activeClip();
    const f = Math.round(frame());
    const track = c && clip?.tracks.find((t) => t.kind === "ik" && t.bone === c.id);
    return {
      influence: c ? constraintInfluence(c, clip, f) : 1,
      value: c ? driverValue(c, clip, f) : 0,
      keyed: { influence: !!track?.blend?.length, value: !!track?.roll?.length },
    };
  });

  const setConstraints = (description: string, constraints: RigConstraint[]) =>
    changeRig(description, { ...rigSettings(), constraints: constraints.length > 0 ? constraints : undefined });

  const changeConstraint = (description: string, constraint: RigConstraint) =>
    void setConstraints(description, (rigSettings().constraints ?? []).map((c) => (c.id === constraint.id ? constraint : c)));

  /** Restricción nueva en la articulación activa; la otra elegida es el objetivo (o la cadena) */
  const handleNewConstraint = async (type: ConstraintType) => {
    const bones = rigBones();
    const active = viewSettings().selectedBone;
    if (!bones[active]) return;
    const others = jointSelection().filter((j) => j !== active).map((j) => bones[j].name);
    const label = CONSTRAINT_TYPES.find((t) => t.value === type)!.label;
    const control = selectedControl();
    const constraint: RigConstraint = {
      id: newRigId(CONSTRAINT_PREFIX.slice(0, -1)),
      name: `${label}: ${bones[active].name}`,
      type,
      owner: bones[active].name,
      ...(type === "driver" || type === "distribute"
        ? { joints: others }
        : { target: control ? `control:${control}` : others[0] }),
      ...(type === "driver" ? { axis: 0 as const, angle: 90, value: 0 } : {}),
    };
    await setConstraints(`Restricción: ${label}`, [...(rigSettings().constraints ?? []), constraint]);
    setSelectedConstraint(constraint.id);
    if (type !== "driver" && type !== "distribute" && !constraint.target) {
      setStatusMessage("Elige el objetivo en el panel (o crea la restricción con el objetivo elegido con Mayús)");
    }
  };

  const handleDeleteConstraint = async (id: string) => {
    await setConstraints("Borrar restricción", (rigSettings().constraints ?? []).filter((c) => c.id !== id));
    // Sus keys de influencia ya no sirven
    const before = clips();
    const after = tidyClips(before, rigSettings());
    if (after.some((c, i) => c !== before[i])) {
      await history.execute("Ordenar pistas del rig", { kind: "clips", data: { before, after, activeBefore: activeClipId(), activeAfter: activeClipId() } });
    }
    setSelectedConstraint(undefined);
  };

  const handleMoveConstraint = (id: string, direction: -1 | 1) => {
    const list = [...(rigSettings().constraints ?? [])];
    const i = list.findIndex((c) => c.id === id);
    const k = i + direction;
    if (i < 0 || k < 0 || k >= list.length) return;
    [list[i], list[k]] = [list[k], list[i]];
    void setConstraints("Orden de las restricciones", list);
  };

  /** Influencia o valor del driver: provisorio al arrastrar, key (o valor fijo) al soltar */
  const handleConstraintScalar = (channel: "blend" | "roll", value: number, commit: boolean) => {
    const c = constraintById(selectedConstraint());
    const v = viewer();
    if (!c || !v) return;
    const f = Math.round(frame());
    const clip = activeClip();
    const keyed = channel === "blend" ? constraintValues().keyed.influence : constraintValues().keyed.value;
    const asKey = autoKey() || keyed;
    const field = channel === "blend" ? "influence" : "value";
    if (!commit) {
      const ctx = asKey
        ? rigCtx()
        : createRigContext(rigBones(), {
            ...rigSettings(),
            constraints: (rigSettings().constraints ?? []).map((x) => (x.id === c.id ? { ...x, [field]: value } : x)),
          });
      const preview = asKey && clip ? insertScalarKey(clip, c.id, channel, f, value, keyInterpolation()) : clip;
      v.setPose(evaluatePose(preview, frame(), ctx));
      setPoseTick((t) => t + 1);
      return;
    }
    const label = channel === "blend" ? "Influencia de la restricción" : "Valor del driver";
    if (asKey) {
      if (!activeClip()) handleNewClip();
      void editClip(label, (x) => insertScalarKey(x, c.id, channel, f, value, keyInterpolation()));
    } else {
      changeConstraint(label, { ...c, [field]: value });
    }
  };

  /**
   * Cambio de mezcla, fijado o balanceo. Mientras se arrastra se ve con una
   * key provisoria; al soltar queda como key (con auto-key, si ya tiene
   * keys, o siempre en el fijado) o como valor fijo de la cadena.
   */
  const handleIkScalar = (channel: ScalarChannel, value: number, commit: boolean) => {
    const chain = chainById(selectedChain());
    const v = viewer();
    if (!chain || !v) return;
    const f = Math.round(frame());
    const clip = activeClip();
    // La mezcla va siempre como key de esta animación: un valor fijo de la
    // cadena valdría para todas y pisaría las que siguen sus keys FK
    const asKey = channel === "pin" || channel === "blend" || autoKey() || chainValues().keyed[channel];
    if (!commit) {
      const ctx = asKey
        ? rigCtx()
        : createRigContext(rigBones(), {
            ...rigSettings(),
            ikChains: (rigSettings().ikChains ?? []).map((c) => (c.id === chain.id ? { ...c, [channel]: value } : c)),
          });
      const preview = asKey && clip ? insertScalarKey(clip, chain.id, channel, f, value, keyInterpolation()) : clip;
      v.setPose(evaluatePose(preview, frame(), ctx));
      setPoseTick((t) => t + 1);
      return;
    }
    const labels = { blend: "Mezcla IK/FK", pin: value >= 0.5 ? "Fijar" : "Soltar", roll: "Balanceo del pie" };
    if (asKey) {
      if (!activeClip()) handleNewClip();
      void editClip(labels[channel], (c) => insertScalarKey(c, chain.id, channel, f, value, keyInterpolation()));
    } else {
      changeChain(labels[channel], { ...chain, [channel]: value });
    }
  };

  /**
   * Rig automático. No hace falta tocar las animaciones que ya existen: una
   * cadena sin key de mezcla sigue las keys FK de sus articulaciones mientras
   * la animación no mueva sus controles (ver `chainBlend`)
   */
  const handleAutoRig = async () => {
    const next = autoRig(rigCtx());
    const chains = (next.ikChains ?? []).filter((c) => c.auto);
    if (chains.length === 0) {
      setStatusMessage("No se reconoció el cuerpo: crea las cadenas a mano con la selección");
      return;
    }
    await changeRig("Rig automático", next);
    // Rehacer el rig puede dejar pistas de cadenas o controles que ya no existen
    const before = clips();
    const after = tidyClips(before, next);
    if (after.some((c, i) => c !== before[i])) {
      await history.execute("Ordenar pistas del rig", { kind: "clips", data: { before, after, activeBefore: activeClipId(), activeAfter: activeClipId() } });
    }
    setSelectedChain(chains[0].id);
    setStatusMessage(
      `Rig automático: ${chains.length} cadenas de IK. Lo animado con keys FK sigue igual; mueve un control o sube la mezcla para pasarlo a IK`
    );
  };

  /** Cadena nueva: la selección si es una cadena, o de la activa hacia arriba */
  const handleNewChain = async (solver: IkSolver) => {
    const ctx = rigCtx();
    const s = rigSettings();
    const active = viewSettings().selectedBone;
    const depth = (j: number) => {
      let d = 0;
      for (let p = ctx.bones[j].parent; p !== null; p = ctx.bones[p].parent) d++;
      return d;
    };
    let joints = [...jointSelection()].sort((a, b) => depth(a) - depth(b));
    const isPath = joints.every((j, i) => i === 0 || ctx.bones[j].parent === joints[i - 1]);
    if (!isPath || joints.length < 2) {
      const want = solver === "twoBone" ? 3 : solver === "root" ? 1 : solver === "lookAt" ? 2 : 4;
      joints = [];
      for (let j: number | null = active; j !== null && joints.length < want; j = ctx.bones[j].parent) joints.unshift(j);
    }
    if (solver === "root") joints = [ctx.body?.root ?? ctx.bones.findIndex((b) => b.parent === null)];
    if (solver === "twoBone" && joints.length < 3) {
      setStatusMessage("Dos huesos: elige el tobillo o la muñeca (con dos articulaciones arriba)");
      return;
    }
    const names = joints.map((j) => ctx.bones[j].name);
    const effector = joints[joints.length - 1];
    const size = (ctx.body?.size ?? 1) * 0.03;
    const at = (j: number) => [...ctx.bones[j].position] as Vec3;
    const newControl = (name: string, position: Vec3, shape: RigControl["shape"], parent: string | null = null): RigControl => ({
      id: newRigId("ctl"),
      name,
      parent,
      shape,
      position,
      size,
    });
    const controls: RigControl[] = [];
    const chain: IkChain = { id: newRigId("ik"), name: `${names[names.length - 1]} ${solver === "twoBone" ? "IK" : solver}`, solver, joints: names };
    if (solver === "spline") {
      const count = Math.min(4, joints.length - 1);
      const parent = ctx.bones[joints[0]].parent;
      for (let i = 1; i <= count; i++) {
        const j = joints[Math.round((i * (joints.length - 1)) / count)];
        controls.push(newControl(`${ctx.bones[j].name}_curva`, at(j), "sphere", parent === null ? null : ctx.bones[parent].name));
      }
      chain.curve = controls.map((c) => c.id);
    } else if (solver === "lookAt") {
      const p = ctx.bones[effector].position;
      const f = bodyAxes(ctx.body).forward;
      const d = (ctx.body?.size ?? 1) * 0.5;
      controls.push(newControl("mirar", [p[0] + f[0] * d, p[1] + f[1] * d, p[2] + f[2] * d], "circle"));
      chain.target = controls[0].id;
    } else {
      controls.push(newControl(`${names[names.length - 1]}_ik`, at(effector), solver === "root" ? "circle" : "cube"));
      chain.target = controls[0].id;
      if (solver === "twoBone") {
        const [a, b] = joints;
        const f = bodyAxes(ctx.body).forward;
        const mid = ctx.bones[b].position;
        const len = vec.length(vec.sub(ctx.bones[b].position, ctx.bones[a].position));
        controls.push(newControl(`${names[1]}_pole`, [mid[0] + f[0] * len, mid[1] + f[1] * len, mid[2] + f[2] * len], "sphere"));
        chain.pole = controls[1].id;
      }
    }
    await changeRig("Cadena de IK", { ...s, controls: [...s.controls, ...controls], ikChains: [...(s.ikChains ?? []), chain] });
    setSelectedChain(chain.id);
  };

  /**
   * Cambiar de modo sin salto. A FK: keys de las articulaciones de la cadena
   * con la pose que se ve y mezcla 0. A IK: los controles van adonde la pose
   * FK deja el efector (y el pole, la curva o la mirada) y mezcla 1.
   */
  const handleMatch = async (to: "fk" | "ik") => {
    const chain = chainById(selectedChain());
    const v = viewer();
    if (!chain || !v || !animating()) return;
    if (!activeClip()) handleNewClip();
    const ctx = rigCtx();
    const f = Math.round(frame());
    const joints = chain.joints.flatMap((n) => ctx.boneIndex.get(n) ?? []);
    if (to === "fk") {
      const entries = joints.flatMap((j) => {
        const p = v.getJointPose(j);
        return p ? [{ bone: ctx.bones[j].name, rotation: p.rotation, translation: Math.hypot(...p.translation) > 1e-9 ? p.translation : undefined }] : [];
      });
      await editClip("IK → FK", (c) => insertScalarKey(insertKeys(c, f, entries, keyInterpolation()), chain.id, "blend", f, 0, keyInterpolation()));
      setStatusMessage(`${chain.name}: en FK`);
      return;
    }
    // La pose que se ve (en FK para esta cadena, con el resto de la pila)
    const fkPose = v.getPose();
    const fk = new Fk(ctx.bones, fkPose);
    const controls = new Map(ctx.settings.controls.map((c) => [c.id, c]));
    const entries: { bone: string; kind: "control"; rotation: Quat; translation: Vec3 }[] = [];
    /** Pose del control para que quede en `position` (y con giro `rotation`) en el mundo */
    const place = (id: string | undefined, position: Vec3, rotation?: Quat) => {
      const control = id ? controls.get(id) : undefined;
      if (!control) return;
      const current = controlWorld(control, ctx, fk, fkPose);
      const inverse = current.follow.clone().invert();
      const local = new THREE.Vector3(...position).applyMatrix4(inverse);
      const followRot = new THREE.Quaternion().setFromRotationMatrix(current.follow);
      const r = rotation ? followRot.clone().invert().multiply(new THREE.Quaternion(...rotation)) : new THREE.Quaternion();
      entries.push({
        bone: control.id,
        kind: "control",
        rotation: [r.x, r.y, r.z, r.w],
        translation: [local.x - control.position[0], local.y - control.position[1], local.z - control.position[2]],
      });
    };
    const effector = joints[joints.length - 1];
    if (chain.solver === "twoBone" && joints.length >= 3) {
      const [a, b, c] = joints;
      const q = fk.world(c);
      place(chain.target, fk.position(c), chain.footRoll || chain.alignEffector ? [q.x, q.y, q.z, q.w] : undefined);
      const A = fk.joint[a], B = fk.joint[b], C = fk.joint[c];
      const axis = C.clone().sub(A).normalize();
      const out = B.clone().sub(A.clone().add(C).multiplyScalar(0.5));
      out.sub(axis.clone().multiplyScalar(out.dot(axis)));
      if (out.lengthSq() > 1e-12) place(chain.pole, [B.x + out.normalize().x * A.distanceTo(B), B.y + out.y * A.distanceTo(B), B.z + out.z * A.distanceTo(B)]);
    } else if (chain.solver === "spline") {
      for (const id of chain.curve ?? []) {
        const control = controls.get(id);
        if (!control) continue;
        // El control va a la articulación que tenía más cerca en reposo
        const nearest = joints.reduce((best, j) =>
          vec.length(vec.sub(ctx.bones[j].position, control.position)) < vec.length(vec.sub(ctx.bones[best].position, control.position)) ? j : best
        );
        place(id, fk.position(nearest));
      }
    } else if (chain.solver === "lookAt") {
      const q = fk.world(effector);
      const facing = new THREE.Vector3(...bodyAxes(ctx.body).forward).applyQuaternion(q);
      const d = (ctx.body?.size ?? 1) * 0.5;
      const p = fk.joint[effector];
      place(chain.target, [p.x + facing.x * d, p.y + facing.y * d, p.z + facing.z * d]);
    } else {
      const q = fk.world(effector);
      place(chain.target, fk.position(effector), [q.x, q.y, q.z, q.w]);
    }
    await editClip("FK → IK", (c) => {
      let next = insertScalarKey(insertKeys(c, f, entries, keyInterpolation()), chain.id, "blend", f, 1, keyInterpolation());
      // El control ya lleva el giro del pie: sin balanceo, el tobillo queda donde está
      if (chain.footRoll) next = insertScalarKey(next, chain.id, "roll", f, 0, keyInterpolation());
      return next;
    });
    setStatusMessage(`${chain.name}: en IK`);
  };

  // ─── Panel de articulación (F4) ───────────────────────────────────────────

  const activeJoint = () => viewSettings().selectedBone;
  const activeName = () => rigBones()[activeJoint()]?.name;
  const limitsOf = (j: number) => (rigBones()[j] ? boneProps(rigSettings(), rigBones()[j].name).limits : undefined);
  /** Límite que se está arrastrando (se dibuja antes de guardarlo) */
  const [limitsPreview, setLimitsPreview] = createSignal<JointLimits | undefined>();

  /** Contexto sin límites: la trayectoria y las violaciones se miden antes de recortar */
  const unlimitedCtx = createMemo(() => ({ ...rigCtx(), settings: { ...rigCtx().settings, limitsOff: true } }));

  /** Giro de una articulación en sus ejes: swing, twist y ángulo de bisagra */
  const jointParts = (j: number, q: Quat) => {
    const r = toJointSpace(q, rigCtx().frames[j]);
    return { ...decompose(r), hinge: hingeAngle(r) };
  };

  const activeCurrent = createMemo(() => {
    poseTick();
    const v = viewer();
    const j = activeJoint();
    if (!v || j < 0 || !animating()) return undefined;
    const p = v.getJointPose(j);
    return p ? jointParts(j, p.rotation) : undefined;
  });

  /** La animación de la articulación activa, cuadro a cuadro (antes de los límites) */
  const trajectory = createMemo<TrajectorySample[]>(() => {
    const clip = activeClip();
    const j = activeJoint();
    if (!clip || j < 0 || !animating() || !rigBones()[j]) return [];
    const ctx = unlimitedCtx();
    const limits = limitsOf(j);
    const out: TrajectorySample[] = [];
    const end = Math.min(Math.round(clip.end), Math.round(clip.start) + 600);
    for (let f = Math.round(clip.start); f <= end; f++) {
      const q = evaluatePose(clip, f, ctx).rotations.get(j) ?? ([0, 0, 0, 1] as Quat);
      const parts = jointParts(j, q);
      out.push({ frame: f, ...parts, excess: limits ? limitExcess(q, ctx.frames[j], limits) : 0 });
    }
    return out;
  });

  const activeKeyFrames = createMemo(() => {
    const name = activeName();
    const track = activeClip()?.tracks.find((t) => !t.kind && t.bone === name);
    return track ? [...new Set(track.rotation.map((k) => k.frame))] : [];
  });

  /** Cuadros fuera del límite de cada hueso (marcas rojas en la línea de tiempo) */
  const violations = createMemo(() => {
    const clip = activeClip();
    const out = new Map<string, number[]>();
    if (!clip || !animating()) return out;
    const ctx = unlimitedCtx();
    const limited = rigBones().flatMap((b, j) => {
      const l = boneProps(rigSettings(), b.name).limits;
      return l ? [{ j, l, name: b.name }] : [];
    });
    if (limited.length === 0) return out;
    const end = Math.min(Math.round(clip.end), Math.round(clip.start) + 600);
    for (let f = Math.round(clip.start); f <= end; f++) {
      const pose = evaluatePose(clip, f, ctx);
      for (const { j, l, name } of limited) {
        const q = pose.rotations.get(j);
        if (q && limitExcess(q, ctx.frames[j], l) > 0.5) {
          if (!out.has(name)) out.set(name, []);
          out.get(name)!.push(f);
        }
      }
    }
    return out;
  });

  /** Articulación activa en la barra de estado: nombre, giro, bloqueos y límite */
  const poseInfo = createMemo(() => {
    const pose = activeJointPose();
    const j = viewSettings().selectedBone;
    const name = rigBones()[j]?.name;
    if (!pose || !name) return undefined;
    const p = boneProps(rigSettings(), name);
    const locked = ["X", "Y", "Z"].filter((_, i) => p.lockRotation[i]);
    const moved = Math.hypot(...pose.translation) > 1e-9;
    const [x, y, z] = pose.rotation.map((a) => a.toFixed(1));
    // El visor muestra la pose ya recortada: se mide la animación antes de los límites
    let excess = 0;
    const clip = activeClip();
    if (p.limits && clip) {
      const ctx = unlimitedCtx();
      const q = evaluatePose(clip, frame(), ctx).rotations.get(j);
      if (q) excess = limitExcess(q, ctx.frames[j], p.limits);
    }
    return [
      name,
      `X ${x}° Y ${y}° Z ${z}°`,
      locked.length > 0 ? `bloqueado ${locked.join("")}` : "",
      moved ? "desplazado" : "",
      excess > 0.5 ? `fuera del límite ${excess.toFixed(0)}° (se recorta)` : "",
      jointSelection().length > 1 ? `${jointSelection().length} elegidas` : "",
    ]
      .filter(Boolean)
      .join(" · ");
  });

  /** Guardar los límites de la articulación activa */
  const setActiveLimits = (description: string, limits: JointLimits | undefined) => {
    const name = activeName();
    setLimitsPreview(undefined);
    if (name) void changeRig(description, withBoneProps(rigSettings(), [name], { limits }));
  };

  const handleLimitKind = (kind: "none" | "hinge" | "ball") => {
    const j = activeJoint();
    if (j < 0) return;
    if (kind === "none") return setActiveLimits("Sin límite", undefined);
    const anatomical = anatomicalLimits(rigCtx()).get(j);
    setActiveLimits(kind === "hinge" ? "Bisagra" : "Rótula", anatomical?.kind === kind ? anatomical : defaultLimits(kind));
  };

  /** Cambiar la key de `frame` de la activa por swing, twist o ángulo de bisagra */
  const handleLimitKeyEdit = (keyFrame: number, change: { swing?: DiskPoint; twist?: number; hinge?: number }, commit: boolean) => {
    const clip = activeClip();
    const j = activeJoint();
    const name = activeName();
    if (!clip || !name) return;
    const ctx = rigCtx();
    const q0 = samplePose(clip, keyFrame, ctx.boneIndex, ctx.rotation).rotations.get(j) ?? ([0, 0, 0, 1] as Quat);
    const parts = jointParts(j, q0);
    const r =
      change.hinge !== undefined ? hingeRotation(change.hinge) : compose(change.swing ?? parts.swing, change.twist ?? parts.twist);
    const q = fromJointSpace(r, ctx.frames[j]);
    const interpolation = clip.tracks.find((t) => t.bone === name)?.rotation.find((k) => k.frame === keyFrame)?.interpolation;
    const next = insertKeys(clip, keyFrame, [{ bone: name, rotation: q }], interpolation ?? keyInterpolation());
    if (commit) void editClip("Editar key", () => next);
    else {
      viewer()?.setPose(evaluatePose(next, frame(), ctx));
      setPoseTick((t) => t + 1);
    }
  };

  /** Recorre todo el rango de la activa en el visor y vuelve a la pose que había */
  /** Barrido de "probar rango" en curso y la pose a la que vuelve */
  let probe: { handle: number; saved: Pose; frame: number; clip?: string } | undefined;
  const stopProbe = (restore: boolean) => {
    if (!probe) return;
    cancelAnimationFrame(probe.handle);
    if (restore) viewer()?.setPose(probe.saved);
    probe = undefined;
    setPoseTick((t) => t + 1);
  };
  const handleProbe = () => {
    const v = viewer();
    const j = activeJoint();
    const limits = limitsOf(j);
    if (!v || !limits || !animating()) return;
    // Otro clic mientras barre: vuelve a la pose de antes y empieza de nuevo
    stopProbe(true);
    const frameAxes = rigCtx().frames[j];
    const path = rangePath(limits);
    const current = { handle: 0, saved: v.getPose(), frame: frame(), clip: activeClipId() };
    probe = current;
    let i = 0;
    const step = () => {
      if (probe !== current) return;
      // Cambió el cuadro, la animación o se salió de Animar: la pose de antes ya no vale
      if (!animating() || frame() !== current.frame || activeClipId() !== current.clip) {
        probe = undefined;
        return;
      }
      if (i >= path.length) {
        stopProbe(true);
        return;
      }
      const pose = emptyPose();
      pose.rotations.set(j, fromJointSpace(path[i++], frameAxes));
      v.applyPartialPose(pose);
      current.handle = requestAnimationFrame(step);
    };
    current.handle = requestAnimationFrame(step);
  };
  onCleanup(() => stopProbe(false));

  /** Límites automáticos para la selección (o todo el esqueleto) */
  const handleAutoLimits = (mode: LimitsAuto) => {
    const ctx = rigCtx();
    const joints = (jointSelection().length > 0 ? jointSelection() : ctx.bones.map((_, j) => j)).filter((j) => ctx.children[j].length > 0);
    const anatomical = anatomicalLimits(ctx);
    let next = rigSettings();
    const set = (j: number, limits: JointLimits | undefined) => (next = withBoneProps(next, [ctx.bones[j].name], { limits }));
    const kindOf = (j: number) => limitsOf(j)?.kind ?? anatomical.get(j)?.kind ?? "ball";
    let count = 0;
    if (mode === "clear") joints.forEach((j) => set(j, undefined));
    if (mode === "anatomical") {
      for (const j of joints) {
        const l = anatomical.get(j);
        if (l) {
          set(j, l);
          count++;
        }
      }
    }
    if (mode === "observed") {
      const found = observedLimits(clips(), ctx, joints, new Map(joints.map((j) => [j, kindOf(j)])));
      found.forEach((l, j) => (set(j, l), count++));
      if (found.size === 0) return setStatusMessage("No hay animaciones con keys de donde sacar los límites");
    }
    if (mode === "mesh") {
      const mesh = meshData();
      const weights = weightsData();
      if (!mesh || !weights || weights.numVertices * 3 !== mesh.positions.length) return;
      const skin = { positions: mesh.positions, weights: weights.weights, maxInfluences: weights.maxInfluences };
      for (const j of joints) {
        const found = meshLimits(ctx, skin, j, kindOf(j));
        if (!found) continue;
        const current = limitsOf(j) ?? anatomical.get(j);
        // La malla dice hasta dónde; el sentido de la bisagra y el twist se conservan
        if (found.kind === "hinge" && current?.kind === "hinge") {
          found.min = Math.max(found.min!, current.min ?? -180);
          found.max = Math.min(found.max!, current.max ?? 180);
          if (found.min > found.max) found.min = found.max = 0;
        }
        if (found.kind === "ball" && current?.kind === "ball" && current.twist) found.twist = current.twist;
        set(j, found);
        count++;
      }
    }
    const labels = { anatomical: "Límites anatómicos", mesh: "Límites por la malla", observed: "Límites por la animación", clear: "Quitar límites" };
    void changeRig(labels[mode], next);
    if (mode !== "clear") setStatusMessage(`${labels[mode]}: ${count} articulaciones`);
  };

  /** La selección ordenada de la raíz hacia la punta, si forma una cadena */
  const selectionPath = createMemo(() => {
    const ctx = rigCtx();
    const depth = (j: number) => {
      let d = 0;
      for (let p = ctx.bones[j]?.parent ?? null; p !== null; p = ctx.bones[p].parent) d++;
      return d;
    };
    const joints = [...jointSelection()].sort((a, b) => depth(a) - depth(b));
    const path = joints.length >= 2 && joints.every((j, i) => i === 0 || ctx.bones[j].parent === joints[i - 1]);
    return path ? joints : null;
  });

  /** Plano de doblez de la cadena: normal de la primera bisagra, o el plano que forman sus puntos */
  const chainPlane = (joints: number[]) => {
    const ctx = rigCtx();
    const pos = joints.map((j) => ctx.bones[j].position);
    const hinge = joints.slice(0, -1).find((j) => limitsOf(j)?.kind === "hinge");
    let n: Vec3 = hinge !== undefined ? ctx.frames[hinge].x : vec.cross(vec.sub(pos[1], pos[0]), vec.sub(pos[pos.length - 1], pos[0]));
    if (vec.length(n) < 1e-6) n = bodyAxes(ctx.body).right;
    n = vec.unit(n, [1, 0, 0]);
    // Arriba en el dibujo = arriba en el mundo (si el plano no es horizontal)
    const up = bodyAxes(ctx.body).up;
    const e2 = vec.sub(up, vec.scale(n, vec.dot(up, n)));
    if (vec.length(e2) > 0.3) {
      const v = vec.unit(e2, [0, 1, 0]);
      return { n, e1: vec.cross(v, n), e2: v };
    }
    let e1 = vec.sub(pos[pos.length - 1], pos[0]);
    e1 = vec.unit(vec.sub(e1, vec.scale(n, vec.dot(e1, n))), [0, -1, 0]);
    return { n, e1, e2: vec.cross(n, e1) };
  };

  const chainView = createMemo<ChainView | undefined>(() => {
    poseTick();
    const joints = selectionPath();
    const v = viewer();
    if (!joints || !v || !animating()) return undefined;
    const ctx = rigCtx();
    const { n, e1, e2 } = chainPlane(joints);
    // Posada: el plano gira con el marco de la base de la cadena
    const fk = new Fk(ctx.bones, v.getPose());
    const base = fk.frame(joints[0]);
    const turn = (d: Vec3) => {
      const r = new THREE.Vector3(...d).applyQuaternion(base);
      return [r.x, r.y, r.z] as Vec3;
    };
    const [pe1, pe2] = [turn(e1), turn(e2)];
    const origin = ctx.bones[joints[0]].position;
    const posedOrigin = fk.position(joints[0]);
    const project = (p: Vec3, o: Vec3, a: Vec3, b: Vec3): [number, number] => [vec.dot(vec.sub(p, o), a), vec.dot(vec.sub(p, o), b)];
    const posed = joints.map((j) => project(fk.position(j), posedOrigin, pe1, pe2));
    const rest = joints.map((j) => project(ctx.bones[j].position, origin, e1, e2));
    // Rango de cada articulación en el plano (bisagras alineadas, o el swing a lo largo del plano)
    const ranges = joints.slice(0, -1).map((j) => {
      const l = limitsOf(j);
      const f = ctx.frames[j];
      if (l?.kind === "hinge" && Math.abs(vec.dot(f.x, n)) > 0.8) {
        const s = Math.sign(vec.dot(f.x, n));
        const a = (l.min ?? -180) * s;
        const b = (l.max ?? 180) * s;
        return { lo: Math.min(a, b), hi: Math.max(a, b), hinge: true, s };
      }
      if (l?.kind === "ball" && l.swing) {
        const w = vec.cross(n, f.y);
        const u = vec.unit([vec.dot(w, f.x), vec.dot(w, f.z), 0], [1, 0, 0]);
        return { lo: -boundaryRadius(l.swing, -u[0], -u[1]), hi: boundaryRadius(l.swing, u[0], u[1]), hinge: false, s: 1 };
      }
      return { lo: -120, hi: 120, hinge: false, s: 1 };
    });
    const arcs = joints.slice(0, -1).flatMap((j, k) => {
      const r = ranges[k];
      if (!r.hinge) return [];
      const d = [posed[k + 1][0] - posed[k][0], posed[k + 1][1] - posed[k][1]];
      const child = (Math.atan2(d[1], d[0]) * 180) / Math.PI;
      const q = v.getJointPose(j)?.rotation ?? ([0, 0, 0, 1] as Quat);
      const current = jointParts(j, q).hinge * r.s;
      return [{ index: k, from: child - current + r.lo, to: child - current + r.hi }];
    });
    // Alcance: configuraciones al azar dentro de los rangos, sobre la cadena en reposo
    const segments = rest.slice(1).map((p, k) => [p[0] - rest[k][0], p[1] - rest[k][1]]);
    const reach: [number, number][] = [];
    let seed = 1;
    const random = () => ((seed = (seed * 16807) % 2147483647) / 2147483647);
    for (let s = 0; s < 500; s++) {
      let angle = 0;
      let x = 0;
      let y = 0;
      segments.forEach((seg, k) => {
        angle += ((ranges[k].lo + (ranges[k].hi - ranges[k].lo) * random()) * Math.PI) / 180;
        const [c, sn] = [Math.cos(angle), Math.sin(angle)];
        x += seg[0] * c - seg[1] * sn;
        y += seg[0] * sn + seg[1] * c;
      });
      reach.push([x, y]);
    }
    return { names: joints.map((j) => ctx.bones[j].name), posed, rest, arcs, reach };
  });

  /** Arrastre en la vista de cadena: gira la articulación `index` alrededor de la normal del plano */
  const handleChainDrag = (index: number, delta: number, commit: boolean) => {
    const joints = selectionPath();
    const base = liveBase("chain");
    if (!joints || !base) return;
    const ctx = rigCtx();
    const { n } = chainPlane(joints);
    const fk = new Fk(ctx.bones, clonePose(base));
    const q = fk.frame(joints[0]);
    const axis = new THREE.Vector3(...n).applyQuaternion(q);
    const j = joints[index];
    fk.rotate(j, axisAngle([axis.x, axis.y, axis.z], (delta * Math.PI) / 180), constrainer(ctx));
    const pose = emptyPose();
    pose.rotations.set(j, fk.pose.rotations.get(j)!);
    livePreview(pose, commit);
  };

  const disks = createMemo(() => {
    poseTick();
    const v = viewer();
    return jointSelection()
      .filter((j) => rigCtx().children[j]?.length > 0)
      .map((j) => {
        const q = v?.getJointPose(j)?.rotation ?? ([0, 0, 0, 1] as Quat);
        const parts = jointParts(j, q);
        return { name: rigBones()[j].name, limits: limitsOf(j), swing: parts.swing, hinge: parts.hinge };
      });
  });

  const handleLimitsBatch = (op: "copyActive" | "mirror" | "scale", factor = 1) => {
    const ctx = rigCtx();
    const active = limitsOf(activeJoint());
    const joints = jointSelection().length > 0 ? jointSelection() : activeJoint() >= 0 ? [activeJoint()] : [];
    let next = rigSettings();
    for (const j of joints) {
      const l = limitsOf(j);
      if (op === "copyActive" && active && j !== activeJoint()) next = withBoneProps(next, [ctx.bones[j].name], { limits: active });
      if (op === "scale" && l) next = withBoneProps(next, [ctx.bones[j].name], { limits: scaleLimits(l, factor) });
      const pair = ctx.mirror[j];
      if (op === "mirror" && l && pair !== null) next = withBoneProps(next, [ctx.bones[pair].name], { limits: mirrorLimits(l) });
    }
    const labels = { copyActive: "Límites iguales a la activa", mirror: "Límites al lado espejo", scale: "Escalar límites" };
    void changeRig(labels[op], next);
  };

  /** Vecindario de la activa: padre, hijos, controles que la siguen y cadenas de IK que la usan */
  const relations = createMemo(() => {
    const ctx = rigCtx();
    const j = activeJoint();
    const nodes: RelationNode[] = [];
    const edges: RelationEdge[] = [];
    const bone = ctx.bones[j];
    if (!bone) return { nodes, edges };
    const id = (k: number) => `b:${k}`;
    nodes.push({ id: id(j), label: bone.name, kind: "bone", role: "active" });
    if (bone.parent !== null) {
      nodes.push({ id: id(bone.parent), label: ctx.bones[bone.parent].name, kind: "bone", role: "parent" });
      edges.push({ from: id(bone.parent), to: id(j), kind: "hierarchy" });
    }
    for (const c of ctx.children[j].slice(0, 5)) {
      nodes.push({ id: id(c), label: ctx.bones[c].name, kind: "bone", role: "child" });
      edges.push({ from: id(j), to: id(c), kind: "hierarchy" });
    }
    const controls = new Map(ctx.settings.controls.map((c) => [c.id, c]));
    const shown = new Set<string>();
    const addControl = (cid: string) => {
      const c = controls.get(cid);
      if (!c || shown.has(cid)) return;
      shown.add(cid);
      nodes.push({ id: `c:${cid}`, label: c.name, kind: "control", role: "control" });
    };
    for (const c of ctx.settings.controls.filter((c) => c.parent === bone.name)) {
      addControl(c.id);
      edges.push({ from: `c:${c.id}`, to: id(j), kind: "follows" });
    }
    for (const chain of (ctx.settings.ikChains ?? []).filter((c) => c.joints.includes(bone.name)).slice(0, 3)) {
      nodes.push({ id: `k:${chain.id}`, label: chain.name, kind: "chain", role: "chain" });
      edges.push({ from: `k:${chain.id}`, to: id(j), kind: "ik" });
      for (const cid of [chain.target, chain.pole, ...(chain.curve ?? [])].filter(Boolean) as string[]) {
        if (shown.size >= 5) break;
        addControl(cid);
        edges.push({ from: `k:${chain.id}`, to: `c:${cid}`, kind: "ik" });
      }
    }
    // Restricciones que mueven la activa o que la usan de objetivo
    const bonesShown = new Set(nodes.filter((n) => n.kind === "bone").map((n) => n.label));
    const boneNode = (name: string) => {
      const k = ctx.boneIndex.get(name);
      if (k === undefined) return undefined;
      if (!bonesShown.has(name)) {
        bonesShown.add(name);
        nodes.push({ id: id(k), label: name, kind: "bone", role: "control" });
      }
      return id(k);
    };
    const related = (ctx.settings.constraints ?? []).filter(
      (c) => c.owner === bone.name || c.target === bone.name || (c.joints ?? []).includes(bone.name)
    );
    for (const c of related.slice(0, 3)) {
      nodes.push({ id: `r:${c.id}`, label: c.name, kind: "constraint", role: "chain" });
      const owner = boneNode(c.owner);
      if (owner) edges.push({ from: `r:${c.id}`, to: owner, kind: "constraint" });
      if (c.target?.startsWith("control:")) {
        addControl(c.target.slice(8));
        edges.push({ from: `c:${c.target.slice(8)}`, to: `r:${c.id}`, kind: "constraint" });
      } else if (c.target) {
        const target = boneNode(c.target);
        if (target) edges.push({ from: target, to: `r:${c.id}`, kind: "constraint" });
      }
    }
    return { nodes, edges };
  });

  const handleRelationSelect = (node: RelationNode) => {
    const [kind, rest] = [node.id.slice(0, 1), node.id.slice(2)];
    if (kind === "b") selectJoints([Number(rest)], Number(rest));
    else if (kind === "c") handleSelectControl(rest);
    else if (kind === "r") {
      setSelectedConstraint(rest);
      setRigEditorTab("ik");
    } else setSelectedChain(rest);
  };

  /** Conectar dos huesos en el grafo: el soltado copia el giro del arrastrado (se cambia en el panel) */
  const handleConnectConstraint = async (target: string, owner: string) => {
    const constraint: RigConstraint = {
      id: newRigId(CONSTRAINT_PREFIX.slice(0, -1)),
      name: `Copiar giro: ${owner}`,
      type: "copyRotation",
      owner,
      target,
      space: "local",
    };
    await setConstraints("Restricción: Copiar giro", [...(rigSettings().constraints ?? []), constraint]);
    setSelectedConstraint(constraint.id);
    setStatusMessage(`${owner} copia el giro de ${target}: cambia el tipo en Restricciones (pestaña IK)`);
  };

  const handleReparent = (controlNode: string, boneName: string | null) => {
    const cid = controlNode.slice(2);
    const s = rigSettings();
    void changeRig(boneName ? `El control sigue a ${boneName}` : "Control suelto", {
      ...s,
      controls: s.controls.map((c) => (c.id === cid ? { ...c, parent: boneName } : c)),
    });
  };

  /** Huesos que comparten piel con lo que mueve la activa, por cuánto peso */
  const sharedSkin = createMemo(() => {
    const weights = weightsData();
    const ctx = rigCtx();
    const j = activeJoint();
    if (!weights || j < 0 || !ctx.bones[j]) return [];
    const own = new Set(ctx.bones[j].parent === null ? [j] : ctx.children[j]);
    const k = weights.maxInfluences;
    const totals = new Map<number, number>();
    let sum = 0;
    for (let v = 0; v < weights.numVertices; v++) {
      let mine = 0;
      for (let s = 0; s < k; s++) if (own.has(weights.weights[(v * k + s) * 2])) mine += weights.weights[(v * k + s) * 2 + 1];
      if (mine < 0.05) continue;
      for (let s = 0; s < k; s++) {
        const b = weights.weights[(v * k + s) * 2];
        const w = weights.weights[(v * k + s) * 2 + 1];
        if (own.has(b) || w <= 0) continue;
        totals.set(b, (totals.get(b) ?? 0) + w * mine);
        sum += w * mine;
      }
    }
    return [...totals.entries()]
      .sort((a, b) => b[1] - a[1])
      .slice(0, 5)
      .map(([b, w]) => ({ name: ctx.bones[b]?.name ?? `hueso ${b}`, fraction: sum > 0 ? w / sum : 0 }));
  });

  // El rango de la activa en 3D
  createEffect(() => {
    const v = viewer();
    if (!v) return;
    const j = activeJoint();
    const limits = limitsPreview() ?? limitsOf(j);
    const ctx = rigCtx();
    const child = ctx.children[j]?.[0];
    if (!limits || child === undefined || pipeline.activeStep() !== "animate") {
      v.setLimitGizmo(null);
      return;
    }
    v.setLimitGizmo({ joint: j, limits, length: vec.length(vec.sub(ctx.bones[child].position, ctx.bones[j].position)) });
  });

  const jointPanel = (
    <JointPanel
      boneName={activeName()}
      limits={limitsPreview() ?? limitsOf(activeJoint())}
      current={activeCurrent()}
      trajectory={trajectory()}
      keyFrames={activeKeyFrames()}
      limitsOff={rigSettings().limitsOff === true}
      posing={animating()}
      canMesh={!!weightsData() && !!meshData()}
      hasSelection={jointSelection().length > 0}
      onLimitsOff={(off) => void changeRig(off ? "No aplicar límites" : "Aplicar límites", { ...rigSettings(), limitsOff: off || undefined })}
      onLimits={setActiveLimits}
      onLimitsPreview={setLimitsPreview}
      onKind={handleLimitKind}
      onKeyEdit={handleLimitKeyEdit}
      onProbe={handleProbe}
      onAuto={handleAutoLimits}
      chain={chainView()}
      onChainDrag={handleChainDrag}
      disks={disks()}
      onBatch={handleLimitsBatch}
      relations={relations()}
      onSelectNode={handleRelationSelect}
      onReparent={handleReparent}
      onConnect={(target, owner) => void handleConnectConstraint(target, owner)}
      shared={sharedSkin()}
    />
  );

  const ikPanel = () => (
    <IkPanel
      chains={rigSettings().ikChains ?? []}
      controls={rigSettings().controls}
      selectedChain={selectedChain()}
      posing={animating()}
      hasSelection={jointSelection().length > 0}
      blend={chainValues().blend}
      pinned={chainValues().pinned}
      roll={chainValues().roll}
      keyed={chainValues().keyed}
      autoIk={autoIk()}
      onAutoIk={setAutoIk}
      onAutoRig={() => void handleAutoRig()}
      onNewChain={(solver) => void handleNewChain(solver)}
      onSelectChain={setSelectedChain}
      onChangeChain={changeChain}
      onDeleteChain={(id) =>
        void changeRig("Borrar cadena", { ...rigSettings(), ikChains: (rigSettings().ikChains ?? []).filter((c) => c.id !== id) })
      }
      onScalar={handleIkScalar}
      onMatch={(to) => void handleMatch(to)}
    />
  );

  const constraintPanel = () => (
    <ConstraintPanel
      constraints={rigSettings().constraints ?? []}
      joints={rigBones().flatMap((b, j) => (b.parent === null || rigBones().some((c) => c.parent === j) ? [b.name] : []))}
      controls={rigSettings().controls}
      selected={selectedConstraint()}
      posing={animating()}
      activeJoint={rigBones()[viewSettings().selectedBone]?.name}
      otherJoints={jointSelection()
        .filter((j) => j !== viewSettings().selectedBone)
        .map((j) => rigBones()[j].name)}
      influence={constraintValues().influence}
      value={constraintValues().value}
      keyed={constraintValues().keyed}
      onNew={(type) => void handleNewConstraint(type)}
      onSelect={setSelectedConstraint}
      onChange={changeConstraint}
      onDelete={(id) => void handleDeleteConstraint(id)}
      onMove={handleMoveConstraint}
      onScalar={handleConstraintScalar}
    />
  );

  /** Ragdoll desde la pose del cuadro actual: un clip nuevo con la caída */
  const handleRagdoll = async (seconds: number) => {
    const ctx = rigCtx();
    if (ctx.bones.length < 2) return;
    const clip = activeClip();
    const f = Math.round(frame());
    const pose = clip ? evaluatePose(clip, f, ctx) : (viewer()?.getPose() ?? evaluatePose(undefined, 0, ctx));
    const previous = clip && f > clip.start ? evaluatePose(clip, f - 1, ctx) : null;
    const baked = ragdollClip(ctx, pose, previous, { seconds, fps: clip?.fps ?? 24, name: `Ragdoll ${clips().length + 1}` });
    const before = clips();
    await history.execute("Ragdoll", { kind: "clips", data: { before, after: [...before, baked], activeBefore: activeClipId(), activeAfter: baked.id } });
    setFrame(baked.start);
    setStatusMessage(`Caída horneada en «${baked.name}» (${seconds} s)`);
  };

  const posePanel = (
    <PosePanel
      posing={animating()}
      hasSelection={jointSelection().length > 0}
      space={transformSpace()}
      onSpace={setTransformSpace}
      onSelect={(command) => handleSelectCommand(command)}
      onReset={handleResetPose}
      onMirror={handleMirrorPose}
      onCopy={handleCopyPose}
      onPaste={handlePastePose}
      canPaste={canPaste()}
      onBreakdown={handleBreakdown}
      onPushRelax={handlePushRelax}
      onRagdoll={(seconds) => void handleRagdoll(seconds)}
    />
  );

  const libraryPanel = (
    <LibraryPanel
      hasSelection={jointSelection().length > 0}
      animations={presetAnimations()}
      onAddAnimation={(id) => void handleAddPresetClip(id)}
      factory={body() ? availablePoses(body()!) : []}
      library={poseLibrary()}
      onSavePose={(selectionOnly) => void handleSavePose(selectionOnly)}
      onDeletePose={(id) => changePoseLibrary("Borrar pose", poseLibrary().filter((p) => p.id !== id))}
      onRenamePose={(id, name) => changePoseLibrary("Renombrar pose", poseLibrary().map((p) => (p.id === id ? { ...p, name } : p)))}
      onApplyPose={handleApplyLibraryPose}
    />
  );

  // Editor de pose y rig al costado del visor (como el de texturas)
  const [rigEditorOpen, setRigEditorOpen] = createPersisted("rigEditor.open", false);
  const [rigEditorTab, setRigEditorTab] = createPersisted<RigEditorTab>("rigEditor.tab", "joint");
  const [rigEditorFraction, setRigEditorFraction] = createPersisted("rigEditor.fraction", 0.45);
  const rigEditorVisible = () => rigEditorOpen() && animating() && !textureEditor();
  const toggleRigEditor = () => {
    if (!rigEditorOpen()) setTextureEditor(undefined);
    setRigEditorOpen(!rigEditorOpen());
  };
  const resizeRigEditor = (e: PointerEvent) => {
    const start = rigEditorFraction();
    const width = splitRef?.clientWidth ?? 1;
    startDrag(e, "col-resize", (dx) => setRigEditorFraction(Math.min(0.75, Math.max(0.2, start + dx / width))));
  };
  const openLibrary = () => {
    setTextureEditor(undefined);
    setRigEditorTab("library");
    setRigEditorOpen(true);
  };
  // Editor de esqueleto al costado del visor, en la sección Esqueleto:
  // crear → ajustar → pesos, y los ayudantes del rig
  const [skeletonEditorOpen, setSkeletonEditorOpen] = createPersisted("skeletonEditor.open", false);
  const [skeletonEditorTab, setSkeletonEditorTab] = createPersisted<SkeletonEditorTab>("skeletonEditor.tab", "templates");
  const [skeletonEditorFraction, setSkeletonEditorFraction] = createPersisted("skeletonEditor.fraction", 0.42);
  const inSkeletonStep = () => pipeline.activeStep() === "skeleton";
  const skeletonEditorVisible = () => skeletonEditorOpen() && inSkeletonStep() && !textureEditor();
  const toggleSkeletonEditor = () => {
    if (!skeletonEditorOpen()) setTextureEditor(undefined);
    setSkeletonEditorOpen(!skeletonEditorOpen());
  };
  const resizeSkeletonEditor = (e: PointerEvent) => {
    const start = skeletonEditorFraction();
    const width = splitRef?.clientWidth ?? 1;
    startDrag(e, "col-resize", (dx) => setSkeletonEditorFraction(Math.min(0.75, Math.max(0.2, start + dx / width))));
  };
  // Panel lateral de Orizon3D (como los editores de pieles y de rig)
  const [scanEditorOpen, setScanEditorOpen] = createPersisted("scanEditor.open", true);
  const [scanEditorTab, setScanEditorTab] = createPersisted<ScanEditorTab>("scanEditor.tab", "capture");
  const [scanEditorFraction, setScanEditorFraction] = createPersisted("scanEditor.fraction", 0.4);
  const inScanWorkspace = () => pipeline.workspace()?.id === "scan";
  const scanEditorVisible = () => inScanWorkspace() && scanEditorOpen() && !textureEditor();
  const resizeScanEditor = (e: PointerEvent) => {
    const start = scanEditorFraction();
    const width = splitRef?.clientWidth ?? 1;
    startDrag(e, "col-resize", (dx) => setScanEditorFraction(Math.min(0.75, Math.max(0.2, start + dx / width))));
  };
  /** Abre Orizon3D en una pestaña (desde la pantalla de inicio) */
  const openScanEditor = (tab: ScanEditorTab) => {
    setTextureEditor(undefined);
    pipeline.setActiveStep("scan");
    setScanEditorTab(tab);
    setScanEditorOpen(true);
  };
  /** La nube se ve y se edita en el visor (en Orizon3D, con una nube abierta) */
  const cloudEditing = () => inScanWorkspace() && !!scanCloud.info() && scanCloud.shown();
  createEffect(() => {
    const v = viewer();
    if (!v) return;
    const on = cloudEditing();
    v.setCloudFocus(on);
    v.setCloudTool(on ? scanCloud.tool() : null);
  });

  /** Props del paso Esqueleto: la barra lateral y las pestañas del editor */
  const skeletonStepProps = (): SkeletonStepProps => ({
    presets: skeletonPresets(),
    selectedPreset: selectedSkeleton(),
    skeletonLoaded: skeletonLoaded(),
    skeletonBones: skeletonData()?.bones.length,
    onPresetChange: handleSkeletonChange,
    autorigConfig: autorigConfig(),
    onAutorigConfigChange: setAutorigConfig,
    onAutorig: handleAutorig,
    canAutorig: meshLoaded() && skeletonLoaded(),
    hasModel: meshLoaded(),
    isProcessing: isProcessing(),
    autorigComplete: autorigComplete(),
    numBones: boneNames().length || undefined,
    skeletonTransform: skeletonTransform(),
    onTransformChange: handleTransformChange,
    onAutoFit: handleAutoFit,
    fitInfo: fitInfo(),
    editorOpen: skeletonEditorVisible(),
    onToggleEditor: toggleSkeletonEditor,
    onResetTransform: handleResetTransform,
    editing: boneEditMode() && activeTool() === "move",
    onEdit: () => useTool("move"),
    symmetric: symmetricEdit(),
    onSymmetricChange: setSymmetricEdit,
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
  });

  // ─── Ajuste fino de articulaciones (pestaña Ajustar) ──────────────────────

  const sendBonePositions = async (positions: Vec3[]) => {
    const data = await invoke<TauriSkeletonData>("set_bone_positions", { positions });
    setSkeletonData(tauriSkeletonToViewer(data));
  };
  const jointEditCtx = (): JointEditContext => {
    const ctx = rigCtx();
    return { parents: ctx.bones.map((b) => b.parent), mirror: ctx.mirror, symmetry: ctx.symmetry };
  };
  /** Mayor medida de la caja del esqueleto (los pasos son una fracción de ella) */
  const skeletonSize = () => {
    const ps = rigBones().map((b) => b.position);
    if (ps.length === 0) return 1;
    const extent = [0, 1, 2].map((k) => Math.max(...ps.map((p) => p[k])) - Math.min(...ps.map((p) => p[k])));
    return Math.max(...extent) || 1;
  };
  /** Aplica un ajuste a todas las posiciones; queda en el historial y descarta los pesos */
  const tuneJoints = async (description: string, edit: (positions: Vec3[], ctx: JointEditContext) => Vec3[]) => {
    const before = rigBones().map((b) => [...b.position] as Vec3);
    if (before.length === 0) return;
    const after = edit(before, jointEditCtx());
    if (after.every((p, i) => p.every((x, k) => Math.abs(x - before[i][k]) < 1e-12))) return;
    try {
      await history.execute(description, { kind: "jointPositions", data: { before, after } });
      dropWeights("Ajustaste el esqueleto");
    } catch (e) {
      console.error("Joint tune error:", e);
      setStatusMessage(`Error: ${e}`);
    }
  };
  const tunedJoint = () => {
    const j = viewSettings().selectedBone;
    return j >= 0 && j < rigBones().length ? j : -1;
  };
  const DIRECTION_NAMES: Record<TuneDirection, string> = {
    forward: "adelante",
    back: "atrás",
    up: "arriba",
    down: "abajo",
    left: "a la izquierda",
    right: "a la derecha",
  };
  const handleTuneMove = (direction: TuneDirection, fraction: number, withChildren: boolean) => {
    const j = tunedJoint();
    if (j < 0) return;
    const { forward, up, right } = bodyAxes(rigCtx().body);
    const axis = { forward, back: forward, up, down: up, right, left: right }[direction];
    const sign = direction === "back" || direction === "down" || direction === "left" ? -1 : 1;
    const d = sign * fraction * skeletonSize();
    void tuneJoints(`Mover ${rigBones()[j].name} ${DIRECTION_NAMES[direction]}`, (ps, ctx) =>
      moveJoints(ps, ctx, j, [axis[0] * d, axis[1] * d, axis[2] * d], withChildren, symmetricEdit())
    );
  };
  const handleTuneRotate = (rotation: TuneRotation, degrees: number) => {
    const j = tunedJoint();
    if (j < 0) return;
    const { forward, up, right } = bodyAxes(rigCtx().body);
    const rad = (degrees * Math.PI) / 180;
    const p = rigBones()[j].position;
    // Ángulos con el signo de la interfaz: + adelante, + abrir, + a la izquierda
    const side = sideOf(p, jointEditCtx(), skeletonSize()) || 1;
    const [axis, angle] =
      rotation === "pitch" ? [right, -rad] : rotation === "spread" ? [forward, side * rad] : [up, -rad];
    void tuneJoints(`Girar desde ${rigBones()[j].name}`, (ps, ctx) => rotateLimb(ps, ctx, j, axis, angle, symmetricEdit()));
  };
  const handleTuneStretch = (fraction: number) => {
    const j = tunedJoint();
    if (j < 0) return;
    void tuneJoints(`${fraction > 0 ? "Alargar" : "Acortar"} hasta ${rigBones()[j].name}`, (ps, ctx) =>
      stretchBone(ps, ctx, j, fraction * skeletonSize(), symmetricEdit())
    );
  };
  const handleTunePosition = (view: Vec3, withChildren: boolean) => {
    const j = tunedJoint();
    if (j < 0) return;
    const target = fromView(view);
    const p = rigBones()[j].position;
    void tuneJoints(`Posición de ${rigBones()[j].name}`, (ps, ctx) =>
      moveJoints(ps, ctx, j, [target[0] - p[0], target[1] - p[1], target[2] - p[2]], withChildren, symmetricEdit())
    );
  };
  const jointTunePanel = () => (
    <JointTunePanel
      boneNames={rigBones().map((b) => b.name)}
      selected={tunedJoint()}
      onSelect={(i) => selectJoints([i], i)}
      position={tunedJoint() >= 0 ? toView(rigBones()[tunedJoint()].position) : undefined}
      hasParent={tunedJoint() >= 0 && rigBones()[tunedJoint()].parent !== null}
      sided={
        tunedJoint() >= 0 &&
        rigCtx().mirror[tunedJoint()] != null &&
        sideOf(rigBones()[tunedJoint()].position, jointEditCtx(), skeletonSize()) !== 0
      }
      symmetric={symmetricEdit()}
      onSymmetricChange={setSymmetricEdit}
      onSetPosition={handleTunePosition}
      onMove={handleTuneMove}
      onRotate={handleTuneRotate}
      onStretch={handleTuneStretch}
      onCopySide={() => {
        const j = tunedJoint();
        if (j >= 0) void tuneJoints("Copiar el lado al otro", (ps, ctx) => copySide(ps, ctx, j, skeletonSize()));
      }}
      onCenter={meshLoaded() ? () => handleCenterBones(true) : undefined}
      disabled={isProcessing()}
    />
  );

  /** Pestañas del editor de esqueleto */
  const skeletonEditorPanels: Record<SkeletonEditorTab, () => JSX.Element> = {
    templates: () => (
      <TemplateGallery
        presets={skeletonPresets()}
        shapes={templateShapes()}
        selected={selectedSkeleton()}
        onPick={(id) => void handleSkeletonChange(id)}
        onCustomize={() => setSkeletonEditorTab("create")}
        disabled={isProcessing()}
      />
    ),
    create: () => (
      <>
        <BodyBuilder plan={bodyPlan()} onChange={handleBodyPlanChange} onShape={handleBodyShape} disabled={isProcessing()} />
        <div>{skeletonEditPanel}</div>
      </>
    ),
    fit: () => <SkeletonFitTab {...skeletonStepProps()} tunePanel={jointTunePanel()} />,
    weights: () => <SkeletonWeightsTab {...skeletonStepProps()} />,
    controls: () => (
      <>
        <div>
          {ikPanel()}
          {constraintPanel()}
        </div>
        {rigPanel()}
      </>
    ),
  };

  const rigEditorPanels = { library: libraryPanel, joint: jointPanel, pose: posePanel, ik: <div>{ikPanel()}{constraintPanel()}</div>, rig: rigPanel() };

  /** Hay esqueleto y las herramientas actúan sobre él (atajos de selección de pose) */
  const rigging = () => !!skeletonData() && toolCtx() !== "object";


  /**
   * `extras` de glTF para otras herramientas: los límites de giro en los
   * nodos que giran con cada articulación (sus hijos; la raíz, el suyo), con
   * los ejes de la articulación para leerlos, y en la raíz los datos del rig
   * (controles, cadenas IK, restricciones y grupos)
   */
  const rigExtras = (): Record<string, { pinocchio: Record<string, unknown> }> => {
    const ctx = rigCtx();
    const s = rigSettings();
    const out: Record<string, { pinocchio: Record<string, unknown> }> = {};
    const put = (name: string, key: string, value: unknown) => {
      out[name] ??= { pinocchio: {} };
      out[name].pinocchio[key] = value;
    };
    ctx.bones.forEach((b, j) => {
      const limits = boneProps(s, b.name).limits;
      if (!limits) return;
      const nodes = b.parent === null ? [j] : ctx.children[j];
      for (const c of nodes) put(ctx.bones[c].name, "limits", { joint: b.name, axes: ctx.frames[j].q, ...limits });
    });
    const root = ctx.bones.find((b) => b.parent === null);
    if (root && (s.controls.length > 0 || (s.ikChains ?? []).length > 0 || (s.constraints ?? []).length > 0)) {
      put(root.name, "rig", { controls: s.controls, ikChains: s.ikChains ?? [], constraints: s.constraints ?? [], groups: s.groups });
    }
    return out;
  };

  /** Exporta el sólido del diseño (STEP exacto o malla) */
  const handleExportDesign = async () => {
    try {
      const format = exportOptions().format;
      const extensions = format === "step" ? ["step", "stp"] : [format];
      const name = format.toUpperCase();
      const part = designParts().find((p) => p.key === exportPart());
      const assembly = part?.key === "assembly";
      const base = assembly ? "ensamble" : part ? part.name.replace(/[^\p{L}\p{N}_-]+/gu, "_") : (fileName() ?? "diseño").replace(/\.[^.]+$/, "");
      const selected = await save({
        title: `Exportar ${name}`,
        defaultPath: `${base}.${extensions[0]}`,
        filters: [{ name, extensions }],
      });
      if (!selected) return;
      const path = /\.[^./]+$/.test(selected) ? selected : `${selected}.${extensions[0]}`;
      const file = path.split("/").pop();
      if (part?.key === "configs") {
        // Cada variante (y la base) calculada y exportada; al final vuelve la que estaba
        const doc = cad.committed();
        const before = doc?.active_configuration ?? null;
        const variants: { index: number | null; name: string }[] = [
          { index: null, name: "base" },
          ...(doc?.configurations ?? []).map((c, i) => ({ index: i, name: c.name })),
        ];
        const files: string[] = [];
        let total = 0;
        try {
          for (const v of variants) {
            const p = path.replace(/(\.[^./]+)$/, `-${v.name.replace(/[^\p{L}\p{N}_-]+/gu, "_")}$1`);
            await cad.commit((d) => (d.active_configuration = v.index));
            await cad.settled();
            total += await busy(`Exportando ${v.name}...`, () => cad.exportDesign(p, format));
            files.push(p);
          }
        } finally {
          await cad.commit((d) => (d.active_configuration = before));
        }
        setLastExport({ bytes: total, files });
        setStatusMessage(`Exportadas ${files.length} variantes (${formatBytes(total)})`);
        pipeline.markCompleted("export");
        return;
      }
      const bytes = await busy(`Exportando a ${file}...`, () => cad.exportDesign(path, format, part?.id, assembly));
      setLastExport({ bytes, files: [path] });
      setStatusMessage(`Diseño exportado: ${file} (${formatBytes(bytes)})`);
      pipeline.markCompleted("export");
    } catch (e) {
      console.error("Export error:", e);
      setStatusMessage(`Error: ${e}`);
      setProgress(undefined);
    }
  };

  const handleExport = async () => {
    try {
      const formats = supportedFormats();
      if (!formats) return;
      const opts = exportOptions();
      const format = formats.export.find((f) => f.id === opts.format);
      if (!format) return;

      const baseName = (fileName() ?? (meshLoaded() ? "modelo" : "esqueleto")).replace(/\.[^.]+$/, "");
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
      // Sin modelo va el esqueleto solo; BVH lleva el esqueleto y la animación activa
      const skeletonOnly = !meshLoaded();
      const bvh = opts.format === "bvh";
      // La mezcla de capas activa va horneada como una animación más (en BVH, en lugar de la activa)
      const mix = mixer().enabled && mixer().layers.length > 0 ? bakeMixer(mixer(), clips(), rigCtx(), "Mezcla", activeClip()?.fps ?? 24) : undefined;
      const exported = bvh ? (mix ? [mix] : clips().filter((c) => c.id === activeClipId())) : mix ? [...clips(), mix] : clips();
      const withAnimations = bvh || skeletonOnly || includeRig;
      const result = await busy(`Exportando a ${name}...`, () => invoke<ExportResult>("export_model", {
        config: {
          format: opts.format,
          path,
          include_skeleton: includeRig,
          include_weights: includeRig,
          use_retopology: exportUseRetopology() && quadMeshLoaded(),
          compact_uv: compactUv(),
          draco: opts.draco,
          draco_level: opts.dracoLevel,
          draco_position_bits: opts.highPrecision ? 16 : 14,
          simplify_ratio: opts.simplifyPercent < 100 ? opts.simplifyPercent / 100 : null,
          max_texture_size: opts.maxTextureSize > 0 ? opts.maxTextureSize : null,
          texture_quality: opts.textureQuality > 0 ? opts.textureQuality : null,
          optimize_geometry: opts.cleanGeometry,
          strip_unused: opts.cleanGeometry,
          // La pila de evaluación horneada a giros por cuadro: lo único que entienden glTF, USD y BVH
          animations: withAnimations ? clipsForExport(exported.map((c) => (c.baked ? c : bakeClip(c, rigCtx()))), boneIndex()) : null,
          non_deforming: rigBones().flatMap((b, i) => (boneProps(rigSettings(), b.name).deform ? [] : [i])),
          skeleton_only: skeletonOnly,
          bone_shapes: exportBoneShapes(),
          node_extras: (includeRig || skeletonOnly) && (opts.format === "glb" || opts.format === "gltf") ? rigExtras() : null,
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
    if (!matrix) return false;
    try {
      await history.execute(description, { kind: "placement", data: { matrix: matrix.toArray() } });
      setStatusMessage(description);
      return true;
    } catch (e) {
      console.error("Placement error:", e);
      setStatusMessage(`Error: ${e}`);
      return false;
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
      await applyPlacement(matrix, "Frente elegido: el modelo mira hacia −Y (vista frontal)");
    } else if (pick.kind === "surface" && mode === "point") {
      await applyPlacement(originMatrix(positions, "point", undefined, new THREE.Vector3(...pick.point)), "Origen en el punto elegido");
    }
  };

  /** Giro en torno a un eje como se muestra (Z arriba) */
  const handleRotate = (axis: Axis, degrees: number) => {
    const data = meshData();
    const { axis: inner, sign } = VIEW_AXES[axis];
    if (data) applyPlacement(rotationMatrix(data.positions, inner, sign * degrees), `Giro de ${degrees}° en ${axis.toUpperCase()}`);
  };

  const handleMirror = (axis: Axis) => {
    const data = meshData();
    if (data) applyPlacement(mirrorMatrix(data.positions, VIEW_AXES[axis].axis), `Espejado en ${axis.toUpperCase()}`);
  };

  const handleScaleModel = (factor: number, description: string) => {
    const data = meshData();
    if (data && factor > 0 && factor !== 1) applyPlacement(scaleMatrix(data.positions, factor), description);
  };

  /** Traslación en los ejes que se muestran (Z arriba) */
  const handleMoveModel = (offset: [number, number, number]) => {
    const u = gridUnits();
    const shown = offset.map((v) => Number((v * (u.metersPerUnit / u.unitMeters)).toPrecision(5)));
    applyPlacement(translationMatrix(fromView(offset)), `Mover ${shown.join(", ")} ${u.unitLabel}`);
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
    if (mode === "mass") {
      try {
        com = centerOfMass() ?? new THREE.Vector3(...(await loadPlacementInfo()).center_of_mass);
      } catch (e) {
        console.error("Center of mass error:", e);
        setStatusMessage(`No se pudo calcular el centro de masa: ${e}`);
        return;
      }
    }
    await applyPlacement(originMatrix(data.positions, mode, com), `Origen en el ${ORIGIN_LABELS[mode]}`);
  };

  const handleUvUnwrap = () =>
    undoable(async (done) => {
      try {
        setIsProcessing(true);
        setProgress({ value: 0, label: "Desplegando..." });
        setStatusMessage("Desplegando UV y horneando texturas...");
        const onProgress = new Channel<Progress>();
        onProgress.onmessage = (msg) => setProgress({ value: msg.percent, label: msg.message });
        const config = uvConfig();
        const info = await invoke<UvInfo>("run_uv_unwrap", {
          config: {
            texture_size: config.textureSize,
            padding: config.padding,
            max_angle: config.maxAngle,
            // Proyectos viejos no guardaban la distribución
            layout: config.layout ?? "paintable",
            by_parts: config.byParts === true && autorigComplete(),
          },
          onProgress,
        });
        done("Desplegar UV");
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
    });

  /** Despliega la malla original (sin retopología): la escena pasa a tener
   *  UV nuevas y texturas horneadas; el rig la sigue, como al reparar */
  const handleUvUnwrapOriginal = () =>
    undoable(async (done) => {
      try {
        setIsProcessing(true);
        setProgress({ value: 0, label: "Desplegando..." });
        setStatusMessage("Desplegando UV de la malla original...");
        const onProgress = new Channel<Progress>();
        onProgress.onmessage = (msg) => setProgress({ value: msg.percent, label: msg.message });
        const config = uvConfig();
        const result = await invoke<{ uv: UvInfo; mesh_info: MeshInfo; rig_kept: boolean }>("unwrap_original_mesh", {
          config: {
            texture_size: config.textureSize,
            padding: config.padding,
            max_angle: config.maxAngle,
            // Proyectos viejos no guardaban la distribución
            layout: config.layout ?? "paintable",
            by_parts: config.byParts === true && autorigComplete(),
          },
          onProgress,
        });
        done("Desplegar UV del original");
        setCanUndoUnwrap(true);
        // Deshacer la reparación de antes descartaría el desplegado
        setCanUndoRepair(false);
        setDiagnostics(undefined);
        setRepairResult(undefined);
        dropWeights();
        clearQuadMesh();
        setProgress({ value: 100, label: "Cargando en el visor..." });
        setMeshData(await fetchMeshData());
        setMeshInfo({ vertices: result.mesh_info.num_vertices, faces: result.mesh_info.num_faces, format: meshInfo().format });
        if (result.rig_kept) await keepRig();
        pipeline.markCompleted("uv");
        setStatusMessage(
          `UV desplegadas: ${result.uv.num_charts ?? 0} islas, estiramiento ${result.uv.stretch?.toFixed(3) ?? "--"}` +
            (result.uv.texture_size > 0 ? `, texturas de ${result.uv.texture_size} px` : "")
        );
      } catch (e) {
        console.error("UV unwrap original error:", e);
        setStatusMessage(`Error: ${e}`);
      } finally {
        setIsProcessing(false);
        setProgress(undefined);
      }
    });

  const handleUvUndoOriginal = () =>
    undoable(async (done) => {
      try {
        setIsProcessing(true);
        const info = await busy("Volviendo a la malla anterior...", () =>
          invoke<MeshInfo & { rig_kept: boolean }>("undo_unwrap_original")
        );
        done("Deshacer desplegado del original");
        setCanUndoUnwrap(false);
        dropWeights();
        clearQuadMesh();
        setMeshData(await fetchMeshData());
        setMeshInfo({ vertices: info.num_vertices, faces: info.num_faces, format: meshInfo().format });
        if (info.rig_kept) await keepRig();
        setStatusMessage("Desplegado deshecho");
      } catch (e) {
        setStatusMessage(`Error: ${e}`);
      } finally {
        setIsProcessing(false);
      }
    });

  const handleUvRestore = () =>
    undoable(async (done) => {
      try {
        setIsProcessing(true);
        await invoke<UvInfo>("restore_transferred_uvs");
        done("Volver a las UV trasladadas");
        await refreshSkin();
        setStatusMessage("UV trasladadas del modelo original");
      } catch (e) {
        setStatusMessage(`Error: ${e}`);
      } finally {
        setIsProcessing(false);
      }
    });

  const handleRetopology = () =>
    undoable(async (done) => {
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
        done("Retopología");

        setQuadMeshInfo({ vertices: info.num_vertices, quads: info.num_quads });
        setQuadQuality(info.quality);

        setProgress({ value: 100, label: "Cargando en el visor..." });
        const quadData = decodeMesh(await invoke<ArrayBuffer>("get_quad_mesh_data"));
        setQuadMeshData(quadData);
        // Las etapas siguientes usan la malla nueva; el rig la sigue con los
        // pesos trasladados (el esqueleto y las animaciones no cambian)
        if (info.rig_kept) await reloadWeights();
        else dropWeights();
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
        const rigNote = info.rig_kept ? " · esqueleto y pesos trasladados" : "";
        setStatusMessage(
          `Retopologia completada: ${info.num_vertices.toLocaleString()} vertices, ${info.num_quads.toLocaleString()} quads${uvNote}${rigNote}`
        );

        // Pipeline: mark retopology as completed
        pipeline.markCompleted("remesh");
      } catch (e) {
        console.error("Retopology error:", e);
        setStatusMessage(`Error: ${e}`);
        setIsProcessing(false);
        setProgress(undefined);
      }
    });

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

  const handleRepairMesh = () =>
    undoable(async (done) => {
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
            resolve_intersections: opts.resolveIntersections,
          },
        }));
        done("Reparar malla");

        setRepairResult(result);
        setDiagnostics(result.new_diagnostics);
        setCanUndoRepair(true);
        // Deshacer el desplegado de antes descartaría la reparación
        setCanUndoUnwrap(false);
        // La retopología se descarta; el rig pasa a la malla reparada
        dropWeights();
        clearQuadMesh();

        // Refrescar meshData y meshInfo
        const data = await fetchMeshData();
        setMeshData(data);
        setMeshInfo({
          vertices: result.new_mesh_info.num_vertices,
          faces: result.new_mesh_info.num_faces,
          format: meshInfo().format,
        });
        if (result.rig_kept) await keepRig();

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
    });

  const handleUndoRepair = () =>
    undoable(async (done) => {
      try {
        setStatusMessage("Deshaciendo reparación...");
        const info = await busy("Deshaciendo reparación...", () =>
          invoke<MeshInfo & { rig_kept: boolean }>("undo_repair")
        );
        done("Deshacer reparación");

        const data = await fetchMeshData();
        setMeshData(data);
        setMeshInfo({ vertices: info.num_vertices, faces: info.num_faces, format: meshInfo().format });
        setCanUndoRepair(false);
        setDiagnostics(undefined);
        setRepairResult(undefined);
        dropWeights();
        clearQuadMesh();
        if (info.rig_kept) await keepRig();
        setStatusMessage("Reparación deshecha");
      } catch (e) {
        console.error("Undo repair error:", e);
        setStatusMessage(`Error: ${e}`);
      }
    });

  // ═══════════════════════════════════════════════════════════════════════════
  // REMALLAR (los modos que no son la retopología)
  // ═══════════════════════════════════════════════════════════════════════════

  /** Descarta la vista previa (lo aplicado se sigue mostrando) */
  const discardRemeshPreview = () => {
    if (!remeshPreviewData() && remeshStats()?.kind !== "preview") return;
    setRemeshPreviewData(undefined);
    setRemeshStats((s) => (s?.kind === "preview" ? undefined : s));
    rawInvoke("remesh_discard").catch(() => {});
  };
  // La vista previa deja de valer si cambia la malla, las opciones o el modo
  createEffect(
    on(
      meshData,
      () => {
        discardRemeshPreview();
        setRemeshStats(undefined);
      },
      { defer: true }
    )
  );
  createEffect(on([simplifyConfig, smoothConfig, remeshMode], discardRemeshPreview, { defer: true }));
  createEffect(
    on(
      () => pipeline.activeStep(),
      (step) => {
        if (step !== "remesh") discardRemeshPreview();
      }
    )
  );
  // "Los dos": el resultado en alambre sobre el modelo
  createEffect(() => viewer()?.setPreviewOverlay(previewView() === "both" ? remeshPreviewData() : undefined));
  /** El visor muestra el resultado en lugar del modelo */
  const showsPreview = () => previewView() === "result" && !!remeshPreviewData();

  /** Parámetros, mensaje y paso del historial del modo elegido */
  const remeshJob = () => {
    const mode = remeshMode();
    if (mode === "smooth") {
      const config = smoothConfig();
      const passes = Math.round(config.iterations);
      return {
        mode,
        params: smoothParams(config),
        busy: "Suavizando...",
        step: `Suavizar malla (${passes} ${passes === 1 ? "pasada" : "pasadas"})`,
        done: (stats: RemeshStats) => `Malla suavizada: se movió hasta ${stats.deviation.max_percent.toLocaleString("es", { maximumSignificantDigits: 2 })} % del tamaño`,
      };
    }
    const config = simplifyConfig();
    return {
      mode,
      params: simplifyParams(config),
      busy: "Simplificando...",
      step: `Simplificar malla (${config.percent.toLocaleString("es")} %)`,
      done: (stats: RemeshStats) => `Malla simplificada: ${stats.after.triangles.toLocaleString("es")} triángulos`,
    };
  };

  const handleRemeshPreview = async () => {
    try {
      setIsProcessing(true);
      const { mode, params } = remeshJob();
      const stats = await busy("Calculando la vista previa...", async () => {
        const stats = await invoke<RemeshStats>("remesh_preview", { params, onProgress: progressChannel() });
        setRemeshPreviewData(decodeMesh(await invoke<ArrayBuffer>("get_remesh_preview_data")));
        return stats;
      });
      setRemeshStats({ mode, kind: "preview", stats });
      setStatusMessage(
        `Vista previa: ${stats.after.triangles.toLocaleString("es")} triángulos (antes ${stats.before.triangles.toLocaleString("es")}), se aleja hasta ${stats.deviation.max_percent.toLocaleString("es", { maximumSignificantDigits: 2 })} %`
      );
    } catch (e) {
      console.error("Remesh preview error:", e);
      setStatusMessage(`Error: ${e}`);
    } finally {
      setIsProcessing(false);
    }
  };

  const handleRemeshApply = () =>
    undoable(async (done) => {
      try {
        setIsProcessing(true);
        const job = remeshJob();
        const result = await busy(job.busy, () =>
          invoke<{ mesh_info: MeshInfo; stats: RemeshStats; rig_kept: boolean }>("remesh_apply", {
            params: job.params,
            onProgress: progressChannel(),
          })
        );
        done(job.step);

        // La retopología, los respaldos de reparar/desplegar/escalar y el
        // diagnóstico eran de la malla anterior; el rig pasa a la nueva
        dropWeights();
        clearQuadMesh();
        setCanUndoRepair(false);
        setCanUndoUnwrap(false);
        setCanUndoPrintScale(false);
        setDiagnostics(undefined);
        setRepairResult(undefined);
        setMeshData(await fetchMeshData());
        setMeshInfo({
          vertices: result.mesh_info.num_vertices,
          faces: result.mesh_info.num_faces,
          format: meshInfo().format,
        });
        if (result.rig_kept) await keepRig();
        setRemeshStats({ mode: job.mode, kind: "applied", stats: result.stats });
        pipeline.markCompleted("remesh");
        setStatusMessage(`${job.done(result.stats)}${result.rig_kept ? " (los pesos pasaron a la malla nueva)" : ""}`);
      } catch (e) {
        console.error("Remesh apply error:", e);
        setStatusMessage(`Error: ${e}`);
      } finally {
        setIsProcessing(false);
      }
    });

  // ═══════════════════════════════════════════════════════════════════════════
  // PRINT3D HANDLERS
  // ═══════════════════════════════════════════════════════════════════════════

  const handleAnalyzePrint3d = async () => {
    try {
      setIsProcessing(true);
      setStatusMessage("Analizando el modelo para fabricar...");
      const result = await busy("Analizando el modelo para fabricar...", () => invoke<TauriPrint3dAnalysis>("analyze_print3d"));
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

  const handleUndoPrintScale = () =>
    undoable(async (done) => {
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
        done("Deshacer escala");
        setStatusMessage("Escala deshecha");
      } catch (e) {
        console.error("Undo scale error:", e);
        setStatusMessage(`Error: ${e}`);
      } finally {
        setIsProcessing(false);
      }
    });

  const handleScaleForPrint = (params: ScaleParams) =>
    undoable(async (done) => {
      try {
        setIsProcessing(true);
        setStatusMessage("Escalando malla...");
        const result = await busy("Escalando malla...", () => invoke<TauriPrint3dAnalysis>("scale_mesh_for_print", { params }));
        setMeshAnalysis(result);
        setCanUndoPrintScale(true);
        done("Escalar para fabricar");
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
    });

  const handleSubdivide = (config: SubdivideConfig) =>
    undoable(async (done) => {
      try {
        setIsProcessing(true);
        setStatusMessage("Subdividiendo malla...");
        const result = await busy("Subdividiendo malla...", () => invoke<TauriSubdivideResult>("subdivide_mesh", { config }));
        done("Dividir en piezas");
        setSubdivideResult(result);
        setIsProcessing(false);
        setStatusMessage(`Subdivisión completada: ${result.piece_count} piezas`);
        pipeline.markCompleted("print3d");
      } catch (e) {
        console.error("Subdivide error:", e);
        setStatusMessage(`Error: ${e}`);
        setIsProcessing(false);
      }
    });

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

  /** Cuenta las transformaciones: la respuesta de una vieja no pisa a la última */
  let transformRequest = 0;
  const applyTransform = async (transform: SkeletonTransform) => {
    const request = ++transformRequest;
    setSkeletonTransform(transform);
    const data = await invoke<TauriSkeletonData>("transform_skeleton", {
      scale: transform.scale,
      translation: transform.translation,
      rotation: transform.rotation,
    });
    if (request !== transformRequest) return;
    setSkeletonData(tauriSkeletonToViewer(data));
    // El backend descartó los pesos (eran del esqueleto anterior): la interfaz también
    if (autorigComplete()) dropWeights("Transformaste el esqueleto entero");
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

  /** Cambió un apéndice: se rehace la plantilla. Con `presetId`, además
   * cambia la plantilla elegida (cuerpo armado desde una forma base) */
  const handleBodyPlanChange = (plan: BodyPlan, presetId?: string) =>
    undoable(async (done) => {
      const request = ++skeletonRequest;
      const previous = bodyPlan();
      const previousPreset = selectedSkeleton();
      setBodyPlan(plan);
      if (presetId) setSelectedSkeleton(presetId);
      try {
        const data = await invoke<TauriSkeletonData>("select_body_plan", { plan });
        if (request !== skeletonRequest) return;
        done(presetId ? "Cuerpo desde cero" : "Esqueleto por forma de cuerpo");
        if (presetId) {
          setSkeletonLoaded(true);
          setAutorigComplete(false);
          setBoneEditMode(false);
        }
        setSkeletonData(tauriSkeletonToViewer(data));
        setSkeletonTransform({ ...defaultTransform });
        setFitInfo(undefined);
        dropWeights("Cambiaste la forma del cuerpo");
        setStatusMessage(`Plantilla con ${data.bones.length} huesos: ajústala al modelo`);
      } catch (e) {
        console.error("Body plan error:", e);
        setStatusMessage(`Error: ${e}`);
        // El panel vuelve a mostrar la forma que sigue puesta
        if (request === skeletonRequest) {
          setBodyPlan(previous);
          setSelectedSkeleton(previousPreset);
        }
      }
    });

  /** Cuerpo desde cero: la forma base sin apéndices */
  const handleBodyShape = async (shape: BodyShape) => {
    try {
      const plan = await invoke<BodyPlan>("new_body_plan", { shape });
      // Sin plantilla de la lista: el id dice de qué forma salió
      await handleBodyPlanChange(plan, `plan:${shape}`);
    } catch (e) {
      console.error("Body shape error:", e);
      setStatusMessage(`Error: ${e}`);
    }
  };

  const handleAutoFit = () =>
    undoable(async (done) => {
      try {
        const fit = await busy("Detectando extremidades y ajustando el esqueleto...", () =>
          invoke<TauriAutoFitResult>("auto_fit_skeleton")
        );
        done("Ajustar esqueleto al modelo");
        setSkeletonData(tauriSkeletonToViewer(fit.skeleton));
        setSkeletonTransform({ ...defaultTransform });
        dropWeights("Volviste a ajustar el esqueleto al modelo");
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
    });

  const handleResetTransform = () =>
    undoable(async (done) => {
      const presetId = selectedSkeleton();
      if (presetId) {
        try {
          // Un cuerpo armado vuelve a su plan, no a la variante de la que partió
          const plan = bodyPlan();
          const data = plan
            ? await invoke<TauriSkeletonData>("select_body_plan", { plan })
            : await invoke<TauriSkeletonData>("select_skeleton", { presetId });
          done("Resetear esqueleto");
          setSkeletonData(tauriSkeletonToViewer(data));
          setSkeletonTransform({ ...defaultTransform });
          setStatusMessage("Esqueleto reseteado");
        } catch (e) {
          console.error("Reset error:", e);
        }
      }
    });

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
    dropWeights("Moviste una articulación");
    // Ya está aplicado: el visor lo fue mandando durante el arrastre
    await history.execute("Mover articulación", { kind: "moveJoint", data: { index, from, to } }, { applied: true });
  };

  /** Centra en el volumen la articulación seleccionada, o todas */
  const handleCenterBones = (onlySelected: boolean) =>
    undoable(async (done) => {
      const selected = viewSettings().selectedBone;
      try {
        const data = await busy("Centrando articulaciones...", () =>
          invoke<TauriSkeletonData>("center_bones", { bones: onlySelected && selected >= 0 ? [selected] : null })
        );
        done(onlySelected ? "Centrar articulación" : "Centrar articulaciones");
        setSkeletonData(tauriSkeletonToViewer(data));
        setSkeletonTransform({ ...defaultTransform });
        dropWeights("Centraste articulaciones");
        setStatusMessage(onlySelected ? "Articulación centrada en el miembro" : "Articulaciones centradas");
      } catch (e) {
        console.error("Center bones error:", e);
        setStatusMessage(`Error: ${e}`);
      }
    });

  /** Herramientas de la sección abierta: modelo, esqueleto o animación */
  const toolCtx = createMemo(() => toolContext(pipeline.activeStep(), animating()));
  const tools = () => TOOLSETS[toolCtx()];

  const useTool = (tool: ToolId) => {
    if (!tools().some((t) => t.id === tool)) return;
    setActiveTool(tool);
    setBoneEditMode(toolCtx() !== "object" && (tool === "move" || tool === "rotate" || tool === "scale"));
    if (tool === "paint") void startPainting();
  };

  // Al cambiar de contexto, la herramienta sigue si existe en el nuevo
  createEffect(() => {
    const ctx = toolCtx();
    untrack(() => useTool(TOOLSETS[ctx].some((t) => t.id === activeTool()) ? activeTool() : "select"));
  });

  const toolDisabledReason = (tool: ToolId) => {
    // Sin modelo se trabaja sobre el esqueleto solo
    if (!meshLoaded() && (toolCtx() === "object" || tool === "paint")) return "importa un modelo";
    if (toolCtx() !== "object" && tool !== "select" && tool !== "measure" && !skeletonData()) return "primero elige un esqueleto";
    if (tool === "paint" && !autorigComplete()) return "primero calcula los pesos";
    return undefined;
  };

  /** Se soltó el gizmo del modelo: se aplica como las demás transformaciones del modelo */
  const handleObjectGizmo = async (matrix: number[], mode: "translate" | "rotate" | "scale") => {
    const m = new THREE.Matrix4().fromArray(matrix);
    const factor = Number(new THREE.Vector3().setFromMatrixScale(m).x.toPrecision(4));
    const description =
      mode === "translate" ? "Mover el modelo" : mode === "rotate" ? "Rotar el modelo" : `Escalar el modelo ×${factor.toLocaleString()}`;
    if (!(await applyPlacement(m, description))) viewerRef?.clearObjectPreview();
  };

  /** Elige la malla de las etapas siguientes (quads u original): el rig la sigue */
  const handleActiveMesh = (useRetopology: boolean) =>
    undoable(async (done) => {
      try {
        const { retopology: active, rig_kept } = await invoke<{ retopology: boolean; rig_kept: boolean }>(
          "set_active_mesh",
          { retopology: useRetopology }
        );
        done(useRetopology ? "Usar la malla de quads" : "Usar la malla original");
        setActiveQuad(active);
        // El rig pasa a la malla elegida con los pesos trasladados
        if (rig_kept) await reloadWeights();
        else dropWeights();
        setStatusMessage(
          active
            ? "UV, esqueleto y pesos usan la malla retopologizada"
            : "UV y piel siguen en la retopología; esqueleto y pesos usan la malla original"
        );
      } catch (e) {
        setStatusMessage(`Error: ${e}`);
      }
    });

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

  const skeletonBoneNames = createMemo(() => skeletonData()?.bones.map((b) => b.name) ?? []);

  /** Hueso cuyos pesos se ven (−1: todos), elegido desde la barra del visor */
  const selectWeightsBone = (index: number) => {
    setViewSettings((prev) => ({ ...prev, selectedBone: index }));
    viewerRef?.selectBone(index);
  };

  // ═══════════════════════════════════════════════════════════════════════════
  // OUTLINER HANDLERS
  // ═══════════════════════════════════════════════════════════════════════════

  const handleToggleVisibility = (nodeId: string) => {
    if (nodeId.startsWith("plane-")) {
      cadUi.togglePlane(nodeId.slice(6) as "xy" | "xz" | "yz");
      return;
    }
    const feature = cadFeatureOfNode(nodeId);
    if (feature !== undefined) {
      cadUi.toggleSketchVisible(feature);
      return;
    }
    const object = objectOfNode(nodeId);
    if (object) {
      const view = isCadObject(object) ? cad.result()?.parts.find((p) => partKey(p.id) === partKey(object.source.part)) : undefined;
      // En Diseñar el ojo es el de la pieza en el diseño; fuera, el del objeto en el visor
      if (view && inDesign()) void cad.setPartProps(view.id, { hidden: !partHidden(cad.doc(), view) });
      else if (object.id === activeObjectId()) setViewSettings((prev) => ({ ...prev, showMesh: !prev.showMesh }));
      else setHiddenGhosts((list) => (list.includes(object.id) ? list.filter((x) => x !== object.id) : [...list, object.id]));
      return;
    }
    if (nodeId.startsWith("node-")) {
      toggleFileNode(Number(nodeId.slice(5)));
      return;
    }
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
    setRigSettings(emptyRigSettings());
    setPoseLibrary([]);
    setMixer(emptyMixer());
    setSelectedJoints([]);
    setSelectedControl(undefined);
    setViewSettings((prev) => ({ ...prev, selectedBone: -1 }));
  };

  /**
   * Borra un objeto desde el Outliner (o el menú del visor). No se puede
   * deshacer: queda como hito en el historial.
   */
  const handleDeleteNode = (nodeId: string) => {
    // Lo del diseño se deshace con el historial del propio diseño
    const feature = cadFeatureOfNode(nodeId);
    if (feature !== undefined) {
      const name = cad.doc()?.features.find((f) => f.id === feature)?.name ?? "La operación";
      if (!cad.removeFeature(feature)) setStatusMessage(`${name}: otras operaciones dependen de ella`);
      else setStatusMessage(`${name} borrada`);
      return;
    }
    const object = objectOfNode(nodeId);
    if (object) {
      void (async () => {
        const view = isCadObject(object) ? cad.result()?.parts.find((p) => partKey(p.id) === partKey(object.source.part)) : undefined;
        if (view) {
          await cad.addFeature({ type: "delete_parts", parts: [view.id] });
          await cad.acceptDraft();
        }
        if (!isCadObject(object) || materialized(object, activeObjectId()) || !view) await removeObject(object.id);
        setStatusMessage(`${object.name} borrado`);
      })();
      return;
    }
    return deleteSceneNode(nodeId);
  };
  const deleteSceneNode = (nodeId: string) =>
    undoable(async (done) => {
      if (nodeId.startsWith("node-")) {
        await deleteFileNode(Number(nodeId.slice(5)));
        return;
      }
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
      done({ skeleton: "Borrar esqueleto", weights: "Borrar pesos", quadmesh: "Borrar retopología" }[kind]!);
    });

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
  /** Pregunta pendiente del diálogo de confirmación */
  const [confirmation, setConfirmation] = createSignal<(ConfirmRequest & { resolve: (ok: boolean) => void }) | undefined>();
  /** Pregunta con el diálogo de la app; `true` si se confirma */
  const confirmAction = (request: ConfirmRequest) =>
    new Promise<boolean>((resolve) => {
      // Una pregunta nueva reemplaza a la que estaba abierta: esa queda como "no"
      confirmation()?.resolve(false);
      setConfirmation({ ...request, resolve });
    });
  const answerConfirmation = (ok: boolean) => {
    const pending = confirmation();
    setConfirmation(undefined);
    pending?.resolve(ok);
  };

  /** Hay trabajo que se perdería: sin archivo propio, o con cambios desde que se guardó o abrió */
  const hasUnsavedWork = async () => {
    if (!hasProjectWork()) return false;
    if (!projectPath()) return true;
    try {
      return await invoke<boolean>("project_changed", { ui: projectUi() });
    } catch {
      return true;
    }
  };

  // Indicador de "cambios sin guardar" en la cabecera. La comparación
  // serializa todo el proyecto, así que no corre en cada cambio: después de
  // cada paso del historial (con un respiro), al guardar o abrir, y cada
  // minuto por los ajustes que no pasan por el historial
  const [unsaved, setUnsaved] = createSignal(false);
  let unsavedTimer: number | undefined;
  let unsavedRequest = 0;
  const checkUnsaved = async () => {
    unsavedTimer = undefined;
    // Con una tarea larga el estado está a medias: se vuelve a mirar al terminar
    if (isProcessing() || switching()) return scheduleUnsavedCheck(2000);
    const request = ++unsavedRequest;
    const result = await hasUnsavedWork();
    if (request === unsavedRequest) setUnsaved(result);
  };
  const scheduleUnsavedCheck = (delay = 800) => {
    if (unsavedTimer !== undefined) clearTimeout(unsavedTimer);
    unsavedTimer = window.setTimeout(checkUnsaved, delay);
  };
  createEffect(on([history.current, history.nodes, projectPath, hasWork, isProcessing], () => scheduleUnsavedCheck()));
  const unsavedInterval = setInterval(() => unsavedTimer === undefined && checkUnsaved(), 60_000);
  onCleanup(() => {
    clearInterval(unsavedInterval);
    if (unsavedTimer !== undefined) clearTimeout(unsavedTimer);
  });

  /** Pregunta antes de descartar el trabajo actual, solo si hay algo sin guardar */
  const confirmDiscard = async (title: string, message: string, confirmLabel: string) =>
    !(await hasUnsavedWork()) || confirmAction({ title, message: `${message} Hay cambios sin guardar: se pierden.`, confirmLabel, danger: true });
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
  // Título de la ventana: archivo y un punto si hay cambios sin guardar
  const baseTitle = document.title;
  createEffect(() => {
    const name = projectPath() ? baseName(projectPath()!) : fileName();
    document.title = name ? `${unsaved() ? "• " : ""}${name} — Pinocchio` : baseTitle;
  });

  /** Estado de la interfaz que va al proyecto (`withHistory` = false: para
   *  las copias de deshacer, que no llevan el historial dentro) */
  const projectUi = (withHistory = true): string => {
    const { trackpadNavigation: _, ...view } = viewSettings();
    return JSON.stringify({
      version: PROJECT_UI_VERSION,
      fileName: fileName(),
      // false: proyecto de solo esqueleto
      model: meshLoaded(),
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
      remesh: { mode: remeshMode(), simplify: simplifyConfig(), smooth: smoothConfig() },
      uv: { config: uvConfig(), preview: uvPreview(), canUndoOriginal: canUndoUnwrap() },
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
      rig: rigSettings(),
      poseLibrary: poseLibrary(),
      mixer: mixer(),
      history: withHistory ? history.save() : undefined,
      // Los objetos van solo al proyecto (no a las copias de deshacer ni al casillero de cada uno)
      objects: withHistory ? objects() : undefined,
      activeObject: withHistory ? activeObjectId() : undefined,
    });
  };

  // Valores por defecto de las configuraciones (los del arranque): un proyecto
  // viejo o editado a mano puede no traer todos los campos
  const configDefaults = {
    lights: lights(),
    autorig: autorigConfig(),
    paint: paintConfig(),
    retopology: retopologyConfig(),
    simplify: simplifyConfig(),
    smooth: smoothConfig(),
    uv: uvConfig(),
    repairAnalysis: repairAnalysisConfig(),
    repair: repairOptions(),
    export: exportOptions(),
  };
  /** Lo guardado encima de los valores por defecto (solo si es un objeto) */
  const withDefaults = <T extends object>(defaults: T, saved: unknown): T =>
    saved && typeof saved === "object" && !Array.isArray(saved) ? { ...defaults, ...saved } : defaults;
  /** Clips con la forma esperada (los demás se descartan) */
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const validClips = (raw: any): AnimationClip[] =>
    Array.isArray(raw)
      ? raw.filter((c) => c && typeof c.id === "string" && Array.isArray(c.tracks) && Number.isFinite(c.start) && Number.isFinite(c.end))
      : [];

  /** Pone la interfaz como estaba y relee del backend lo que ve el visor.
   *  `undoing`: viene de deshacer o rehacer; quedan el historial, la etapa,
   *  la vista y las luces de ahora */
  const restoreProjectUi = async (json: string, undoing = false) => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const ui: any = JSON.parse(json || "{}");
    if (!undoing) {
      // Las copias del backend para deshacer operaciones y ediciones de nodos
      // no se guardan en el proyecto: esos pasos quedan como hitos
      snapshotUi.clear();
      history.load(ui.history, ["snapshot", "sceneEdit"]);
      setObjects(Array.isArray(ui.objects) ? ui.objects : []);
      setActiveObjectId(typeof ui.activeObject === "number" ? ui.activeObject : undefined);
    }
    setPlacementMode(undefined);
    setBoneEditMode(false);
    setPlaying(false);
    setKeySelection(new Set<string>());
    setLastExport(undefined);
    setPaintMirrorLoaded(false);
    setSkinMaterials([]);
    setUvInfo(undefined);

    setFileName(ui.fileName);
    if (ui.meshInfo) setMeshInfo(ui.meshInfo);
    if (undoing) {
      setViewSettings((prev) => ({ ...prev, selectedBone: ui.view?.selectedBone ?? -1 }));
    } else {
      if (ui.lights) setLights(withDefaults(configDefaults.lights, ui.lights));
      setViewSettings((prev) => ({ ...prev, ...ui.view, trackpadNavigation: prev.trackpadNavigation }));
      if (typeof ui.showGrid === "boolean") {
        setShowGrid(ui.showGrid);
        viewerRef?.setGridVisible(ui.showGrid);
      }
    }
    setActiveQuad(ui.activeQuad === true);
    setSelectedSkeleton(ui.skeleton?.preset);
    setSkeletonTransform(ui.skeleton?.transform ?? { ...defaultTransform });
    if (typeof ui.skeleton?.symmetricEdit === "boolean") setSymmetricEdit(ui.skeleton.symmetricEdit);
    setFitInfo(ui.skeleton?.fitInfo);
    setBodyPlan(ui.skeleton?.bodyPlan);
    if (ui.autorig?.config) setAutorigConfig(withDefaults(configDefaults.autorig, ui.autorig.config));
    if (ui.paintConfig) setPaintConfig(withDefaults(configDefaults.paint, ui.paintConfig));
    if (ui.retopology?.config) setRetopologyConfig(withDefaults(configDefaults.retopology, ui.retopology.config));
    setQuadMeshInfo(ui.retopology?.info ?? { vertices: 0, quads: 0 });
    setQuadQuality(ui.retopology?.quality);
    if (REMESH_MODES.some((m) => m.id === ui.remesh?.mode)) setRemeshMode(ui.remesh.mode);
    if (ui.remesh?.simplify) setSimplifyConfig(withDefaults(configDefaults.simplify, ui.remesh.simplify));
    if (ui.remesh?.smooth) setSmoothConfig(withDefaults(configDefaults.smooth, ui.remesh.smooth));
    if (ui.uv?.config) setUvConfig(withDefaults(configDefaults.uv, ui.uv.config));
    if (ui.uv?.preview) setUvPreview(ui.uv.preview);
    if (ui.repair?.analysisConfig) setRepairAnalysisConfig(withDefaults(configDefaults.repairAnalysis, ui.repair.analysisConfig));
    if (ui.repair?.options) setRepairOptions(withDefaults(configDefaults.repair, ui.repair.options));
    setDiagnostics(ui.repair?.diagnostics);
    setRepairResult(ui.repair?.result);
    setCanUndoRepair(ui.repair?.canUndo === true);
    setCanUndoUnwrap(ui.uv?.canUndoOriginal === true);
    setMeshAnalysis(ui.print3d?.analysis);
    setSubdivideResult(ui.print3d?.subdivide);
    setCanUndoPrintScale(ui.print3d?.canUndoScale === true);
    if (ui.export) {
      setExportIncludeRig(ui.export.includeRig !== false);
      setExportUseRetopology(ui.export.useRetopology === true);
      if (ui.export.options) setExportOptions(withDefaults(configDefaults.export, ui.export.options));
    }
    const rig = loadRigSettings(ui.rig);
    setRigSettings(rig);
    setPoseLibrary(loadPoseLibrary(ui.poseLibrary));
    setMixer(loadMixer(ui.mixer));
    setSelectedJoints([]);
    setSelectedControl(undefined);
    setClips(tidyClips(validClips(ui.animation?.clips), rig));
    setActiveClipId(ui.animation?.activeClipId);
    setFrame(ui.animation?.frame ?? 0);
    if (typeof ui.animation?.autoKey === "boolean") setAutoKey(ui.animation.autoKey);
    if (ui.animation?.interpolation) setKeyInterpolation(ui.animation.interpolation);

    // Lo que dibuja el visor sale del backend
    if (ui.model === false) {
      viewerRef?.unloadMesh();
      setMeshData(undefined);
      setMeshLoaded(false);
    } else {
      setMeshData(await fetchMeshData());
      setMeshLoaded(true);
    }
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
    if (!undoing) pipeline.setActiveStep(ui.pipeline?.active ?? "structure");
  };

  /** Guarda en el archivo del proyecto; sin archivo (o con `as`) lo pregunta */
  const handleSaveProject = async (as = false) => {
    if (!hasProjectWork()) {
      setStatusMessage("No hay nada que guardar: importa un modelo, elige un esqueleto o empieza un diseño");
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
      scheduleUnsavedCheck(0);
      setStatusMessage(`Proyecto guardado: ${baseName(path!)} (${(saved.bytes / 1e6).toFixed(1)} MB)`);
      // Lo guardado ya está a salvo: la recuperación vieja no debe ofrecerse al abrir la app
      invoke("clear_recovery").catch(() => {});
    } catch (e) {
      console.error("Save project error:", e);
      setStatusMessage(`Error al guardar: ${e}`);
    }
  };

  /** Abre un proyecto (`path`) o pregunta cuál; `recovered` = viene del archivo de recuperación */
  const handleOpenProject = async (path?: string, recovered = false) => {
    if (blockedByTask()) return;
    if (!(await confirmDiscard("Abrir proyecto", "Se cierra el trabajo actual.", "Abrir otro proyecto"))) return;
    if (!path) {
      const chosen = await open({
        title: "Abrir proyecto",
        filters: [{ name: "Proyecto de Pinocchio", extensions: ["pinocchio"] }],
      });
      if (!chosen) return;
      path = typeof chosen === "string" ? chosen : chosen[0];
    }
    let opened: ProjectOpened | undefined;
    setSwitching(true);
    try {
      opened = await busy("Abriendo proyecto...", () => invoke<ProjectOpened>("open_project", { path }));
      await restoreProjectUi(opened.ui);
      cadUi.cancelSketch();
      await cad.reload();
      // Proyecto de antes de los objetos: su modelo pasa a ser uno
      await adoptCurrentModel();
      // La recuperación no es el archivo del usuario: el próximo Guardar pregunta dónde
      setProjectPath(recovered ? undefined : path);
      setRecovery(undefined);
      setStatusMessage(recovered ? "Sesión recuperada" : `Proyecto abierto: ${baseName(path)}`);
    } catch (e) {
      console.error("Open project error:", e);
      if (opened) {
        // El backend ya tiene el proyecto nuevo pero la interfaz quedó a medias:
        // que el próximo Guardar pregunte dónde, así no pisa el archivo anterior
        setProjectPath(undefined);
        setStatusMessage(`El proyecto se abrió con errores (guárdalo con otro nombre): ${e}`);
      } else {
        setStatusMessage(`Error al abrir: ${e}`);
      }
    } finally {
      setSwitching(false);
    }
  };

  /** Empieza de cero: sin modelo, esqueleto, animaciones ni historial */
  const handleNewProject = async () => {
    if (blockedByTask()) return;
    const discard = await confirmDiscard(
      "Proyecto nuevo",
      "Se cierran el modelo, el esqueleto, las animaciones y el historial.",
      "Empezar de cero"
    );
    if (!discard) return;
    setSwitching(true);
    try {
      await invoke("new_project");
      cadUi.cancelSketch();
      await cad.reload();
      clearSkeletonUi();
      // Un proyecto vacío y sin modelo deja la interfaz como al abrir la app
      await restoreProjectUi(JSON.stringify({ model: false }));
      setSceneStructure(undefined);
      setSceneMaterials([]);
      setMeshInfo({ vertices: 0, faces: 0, format: "" });
      setUvPreview("texture");
      setTextureEditor(undefined);
      setProjectPath(undefined);
      setStatusMessage("Proyecto nuevo");
    } catch (e) {
      setStatusMessage(`Error: ${e}`);
    } finally {
      setSwitching(false);
    }
  };

  /** Descarta todo lo hecho sobre el modelo y vuelve al archivo importado */
  const handleRevertToOriginal = () =>
    undoable(async (done) => {
      const ok = await confirmAction({
        title: "Volver al modelo original",
        message: "Se descarta todo lo hecho sobre el modelo: reparación, esqueleto, pesos, retopología, UV y animaciones.",
        confirmLabel: "Descartar",
        danger: true,
      });
      if (!ok) return;
      try {
        await invoke("revert_to_original");
        clearSkeletonUi();
        clearQuadMesh();
        setDiagnostics(undefined);
        setRepairResult(undefined);
        setCanUndoRepair(false);
        setCanUndoUnwrap(false);
        setMeshAnalysis(undefined);
        setSubdivideResult(undefined);
        setCanUndoPrintScale(false);
        setLastExport(undefined);
        done("Volver al modelo original");
        setMeshData(await fetchMeshData());
        setStatusMessage("Modelo original restaurado");
      } catch (e) {
        setStatusMessage(`Error: ${e}`);
      }
    });

  // ─── Importar animación (BVH) y retargeting ──────────────────────────────

  /** Mapeos guardados por esqueleto de origen: nombre del modelo → nombre del origen */
  const [retargetTemplates, setRetargetTemplates] = createPersisted<Record<string, Record<string, string>>>("retarget.templates", {});
  const [retargeting, setRetargeting] = createSignal<(RetargetRequest & { motion: SourceMotion }) | undefined>();
  const sourceSignature = (bones: { name: string }[]) => bones.map((b) => b.name).join("|");

  /** Elige un archivo con el diálogo del webview y devuelve su texto */
  const pickTextFile = (accept: string) =>
    new Promise<{ name: string; text: string } | null>((resolve) => {
      const input = document.createElement("input");
      input.type = "file";
      input.accept = accept;
      input.onchange = async () => {
        const file = input.files?.[0];
        resolve(file ? { name: file.name, text: await file.text() } : null);
      };
      input.oncancel = () => resolve(null);
      input.click();
    });

  /** Abre el mapeo del retargeting para un movimiento de origen (BVH o captura) */
  const openRetarget = (motion: SourceMotion, sourceName: string) => {
    const bones = rigBones();
    const { map, fromTemplate } = initialRetargetMap(motion.bones);
    setRetargeting({
      sourceName,
      source: motion.bones,
      target: bones,
      map,
      frames: motion.positions.length,
      fps: motion.fps,
      fromTemplate,
      motion,
    });
  };

  /** Mapeo del origen al esqueleto: la plantilla guardada para ese origen manda sobre el automático */
  const initialRetargetMap = (source: SkeletonBone[]): { map: RetargetMap; fromTemplate: boolean } => {
    const bones = rigBones();
    const saved = retargetTemplates()[sourceSignature(source)];
    if (!saved) return { map: autoMap(bones, source), fromTemplate: false };
    const sourceIndex = new Map(source.map((b, i) => [b.name, i]));
    const map: RetargetMap = new Map(
      bones.flatMap((b, j) => {
        const k = saved[b.name] === undefined ? undefined : sourceIndex.get(saved[b.name]);
        return k === undefined ? [] : [[j, k] as [number, number]];
      })
    );
    return { map, fromTemplate: true };
  };

  /** Cámara en vivo: el esqueleto sigue al actor (manda sobre la pose de las keys) */
  const [puppeteering, setPuppeteering] = createSignal(false);
  const togglePuppet = () => {
    if (puppeteering()) return setPuppeteering(false);
    if (rigBones().length === 0) {
      setStatusMessage("Primero elige o ajusta un esqueleto: la cámara mueve sus articulaciones");
      return;
    }
    setPuppeteering(true);
  };
  createEffect(() => {
    if (!animating()) setPuppeteering(false);
  });

  const [capturing, setCapturing] = createSignal(false);
  const handleCapture = () => {
    if (rigBones().length === 0) {
      setStatusMessage("Primero elige o ajusta un esqueleto: la captura se pasa a sus articulaciones");
      return;
    }
    setCapturing(true);
  };

  /**
   * Contactos de la captura → keys de fijado de las cadenas IK de las patas:
   * mientras el pie está apoyado queda quieto en el mundo (sin patinar)
   */
  const withContactPins = (clip: AnimationClip, motion: CaptureMotion, map: RetargetMap, target: SkeletonBone[], offset = 0) => {
    const chains = (rigSettings().ikChains ?? []).filter((c) => !c.disabled && ["twoBone", "fabrik", "ccd"].includes(c.solver));
    let out = clip;
    let pinned = 0;
    for (const [source, flags] of motion.contacts) {
      const joint = [...map].find(([, s]) => s === source)?.[0];
      const chain = joint === undefined ? undefined : chains.find((c) => c.joints.includes(target[joint].name));
      if (!chain) continue;
      pinned++;
      // Solo el tramo usado, desde el cuadro 0 del clip
      const used = flags.slice(offset, offset + clip.end - clip.start + 1);
      used.forEach((on, f) => {
        if (f === 0 || on !== used[f - 1]) out = insertScalarKey(out, chain.id, "pin", f, on ? 1 : 0, "step");
      });
    }
    return { clip: out, pinned };
  };

  const handleImportBvh = async () => {
    const bones = rigBones();
    if (bones.length === 0) {
      setStatusMessage("Primero elige o ajusta un esqueleto: la animación se pasa a sus articulaciones");
      return;
    }
    const picked = await pickTextFile(".bvh");
    if (!picked) return;
    try {
      openRetarget(bvhMotion(parseBvh(picked.text)), picked.name);
    } catch (e) {
      setStatusMessage(`No se pudo leer ${picked.name}: ${e instanceof Error ? e.message : e}`);
    }
  };

  const confirmRetarget = async (result: { map: RetargetMap; rootMotion: boolean; name: string; saveTemplate: boolean; range: [number, number] }) => {
    const request = retargeting();
    setRetargeting(undefined);
    if (!request) return;
    if (result.saveTemplate) {
      const byName: Record<string, string> = {};
      for (const [j, k] of result.map) byName[request.target[j].name] = request.source[k].name;
      setRetargetTemplates({ ...retargetTemplates(), [sourceSignature(request.source)]: byName });
    }
    try {
      let clip = retargetClip(request.target, request.motion, result.map, { rootMotion: result.rootMotion, name: result.name, range: result.range });
      let note = "";
      if ("contacts" in request.motion) {
        const pins = withContactPins(clip, request.motion as CaptureMotion, result.map, request.target, result.range[0]);
        clip = pins.clip;
        note = pins.pinned > 0 ? `, pies fijados en ${pins.pinned} patas` : ". Crea el rig automático (IK) para que los pies no patinen";
      }
      const before = clips();
      await history.execute(`Importar animación: ${clip.name}`, {
        kind: "clips",
        data: { before, after: [...before, clip], activeBefore: activeClipId(), activeAfter: clip.id },
      });
      setFrame(clip.start);
      pipeline.setActiveStep("animate");
      setStatusMessage(`Animación importada: ${clip.name} (${clip.end - clip.start + 1} cuadros, ${clip.tracks.length} articulaciones)${note}`);
    } catch (e) {
      console.error("Retarget error:", e);
      setStatusMessage(`No se pudo pasar la animación al esqueleto: ${e instanceof Error ? e.message : e}`);
    }
  };

  const fileMenuItems = (): MenuEntry[] => [
    { label: "Nuevo proyecto", shortcut: "Ctrl+N", onSelect: () => handleNewProject() },
    { label: "Abrir proyecto…", shortcut: "Ctrl+O", onSelect: () => handleOpenProject() },
    { label: "Guardar", shortcut: "Ctrl+S", disabled: !hasProjectWork(), onSelect: () => handleSaveProject() },
    { label: "Guardar como…", shortcut: "Ctrl+Shift+S", disabled: !hasProjectWork(), onSelect: () => handleSaveProject(true) },
    { separator: true },
    { label: "Importar modelo…", shortcut: "Ctrl+I", onSelect: () => handleLoad() },
    { label: "Importar animación (BVH)…", disabled: !skeletonData(), onSelect: () => void handleImportBvh() },
    { label: "Capturar movimiento de un video…", disabled: !skeletonData(), onSelect: handleCapture },
    { label: "Exportar…", disabled: !hasWork() && !hasDesignBody(), onSelect: openExport },
    { label: "Volver al modelo original…", disabled: !meshLoaded(), onSelect: () => handleRevertToOriginal() },
    { separator: true },
    { label: "Configuración…", onSelect: () => setSettingsOpen(true) },
  ];

  // Guardado automático: al archivo del proyecto, o al de recuperación si aún
  // no tiene. Apagado, igual se deja una copia de recuperación cada 5 minutos
  // por si la app se cierra mal (se borra al guardar o al cerrar sin cambios)
  const RECOVERY_MINUTES = 5;
  let lastAutosave = Date.now();
  let autosaving = false;
  const autosaveTimer = setInterval(async () => {
    const settings = autosave();
    if (autosaving || !hasProjectWork() || isProcessing() || progress() || switching()) return;
    const minutes = settings.enabled ? settings.minutes : RECOVERY_MINUTES;
    if (Date.now() - lastAutosave < minutes * 60_000) return;
    autosaving = true;
    lastAutosave = Date.now();
    try {
      const toProject = settings.enabled && projectPath();
      const path = toProject ? projectPath()! : await invoke<string>("recovery_project_path");
      const saved = await invoke<ProjectSaved>("save_project", { path, ui: projectUi(), onlyIfChanged: true });
      if (saved.written && settings.enabled) {
        const time = new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
        setStatusMessage(`Guardado automático ${toProject ? "" : "(recuperación) "}a las ${time}`);
      }
    } catch (e) {
      console.error("Autosave error:", e);
    } finally {
      autosaving = false;
    }
  }, 15_000);
  onCleanup(() => clearInterval(autosaveTimer));

  // Cerrar la ventana con cambios sin guardar pregunta antes. Sin nada
  // pendiente, la recuperación ya no hace falta
  let unlistenClose: (() => void) | undefined;
  onCleanup(() => unlistenClose?.());
  onMount(async () => {
    try {
      unlistenClose = await getCurrentWindow().onCloseRequested(async (event) => {
        if (await hasUnsavedWork()) {
          const ok = await confirmAction({
            title: "Cerrar Pinocchio",
            message: "Hay cambios sin guardar: se pierden (queda la última copia de recuperación).",
            confirmLabel: "Cerrar sin guardar",
            danger: true,
          });
          if (!ok) event.preventDefault();
          return;
        }
        await invoke("clear_recovery").catch(() => {});
      });
    } catch {
      // Sin backend (vista previa) no hay ventana de Tauri
    }
  });

  // Ningún error asíncrono queda mudo: si nadie lo atrapó, va a la barra de estado
  const onUnhandled = (e: PromiseRejectionEvent) => {
    console.error("Unhandled rejection:", e.reason);
    setStatusMessage(`Error: ${e.reason instanceof Error ? e.reason.message : String(e.reason)}`);
  };
  window.addEventListener("unhandledrejection", onUnhandled);
  onCleanup(() => window.removeEventListener("unhandledrejection", onUnhandled));

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
    if (skeletonLoaded()) {
      items.push({ label: autorigComplete() ? "Pesos" : "Influencia estimada", checked: view.showWeights, onSelect: () => toggle("showWeights") });
    }
    items.push({ label: "Rayos X", shortcut: "Alt+Z", checked: view.xray === true, onSelect: () => setViewSettings((prev) => ({ ...prev, xray: !prev.xray })) });
    const removable: MenuEntry[] = [];
    if (skeletonLoaded()) removable.push({ label: "Borrar esqueleto", danger: true, onSelect: () => handleDeleteNode("skeleton") });
    if (quadMeshLoaded()) removable.push({ label: "Borrar retopología", danger: true, onSelect: () => handleDeleteNode("quadmesh") });
    if (removable.length > 0) items.push({ separator: true }, ...removable);
    return items;
  };

  const handleSelectNode = (nodeId: string) => {
    // Lo del diseño se elige en Diseñar
    const plane = nodeId.startsWith("plane-") ? (nodeId.slice(6) as "xy" | "xz" | "yz") : undefined;
    const feature = cadFeatureOfNode(nodeId);
    if (plane || feature !== undefined) {
      if (!inDesign()) pipeline.openWorkspace("design");
      if (plane) cadUi.pickToggle({ kind: "plane", plane }, false);
      else if (feature !== undefined) cad.select(cad.selected() === feature ? undefined : feature);
      return;
    }
    // Un objeto pasa a ser el activo: las herramientas del espacio actúan sobre él
    const object = objectOfNode(nodeId);
    if (object) {
      if (isCadObject(object) && inDesign()) {
        cadUi.setOpenPart(partKey(object.source.part));
        pipeline.setActiveStep("design_part");
      }
      void activateObject(object.id);
      return;
    }
    if (nodeId.startsWith("node-")) {
      const index = Number(nodeId.slice(5));
      setSelectedFileNode(selectedFileNode() === index ? undefined : index);
      return;
    }
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
          unsaved={unsaved()}
          fileMenu={fileMenuItems}
          onOpenSettings={() => setSettingsOpen(true)}
          workspace={pipeline.workspace()?.id}
          onWorkspace={pipeline.openWorkspace}
          exporting={pipeline.activeStep() === "export"}
          onExport={openExport}
          hasModel={meshLoaded() || hasDesignBody()}
          canExport={hasWork() || hasDesignBody()}
        />

        {/* Main Content */}
        <div class="flex flex-1 min-h-0 overflow-hidden">
          {/* Toolbar */}
          <Toolbar
            tools={tools()}
            disabledReason={toolDisabledReason}
            activeTool={activeTool()}
            onToolChange={(tool) => useTool(tool)}
            onResetView={() => viewerRef?.resetView()}
            canUndo={inDesign() ? cad.canUndo() : history.canUndo()}
            canRedo={inDesign() ? cad.canRedo() : history.canRedo()}
            onUndo={undo}
            onRedo={redo}
          />

          {/* Viewport y, al animar, la línea de tiempo debajo */}
          <div class="flex flex-col flex-1 min-w-0 min-h-0">
          <div ref={splitRef} class="flex flex-1 min-w-0 min-h-0">
          {/* Editor de pose y rig a la izquierda del visor */}
          <Show when={rigEditorVisible()}>
            <div class="shrink-0 min-w-0 border-r border-border" style={{ width: `${100 * rigEditorFraction()}%` }}>
              <RigEditor tab={rigEditorTab()} onTab={setRigEditorTab} onClose={() => setRigEditorOpen(false)} panels={rigEditorPanels} />
            </div>
            <div class="shrink-0 w-1 cursor-col-resize bg-border hover:bg-accent/50 transition-colors" onPointerDown={resizeRigEditor} />
          </Show>
          {/* Editor de esqueleto a la izquierda del visor */}
          <Show when={skeletonEditorVisible()}>
            <div class="shrink-0 min-w-0 border-r border-border" style={{ width: `${100 * skeletonEditorFraction()}%` }}>
              <SkeletonEditor
                tab={skeletonEditorTab()}
                onTab={setSkeletonEditorTab}
                onClose={() => setSkeletonEditorOpen(false)}
                panels={skeletonEditorPanels}
                bones={skeletonData()?.bones.length}
              />
            </div>
            <div class="shrink-0 w-1 cursor-col-resize bg-border hover:bg-accent/50 transition-colors" onPointerDown={resizeSkeletonEditor} />
          </Show>
          {/* Orizon3D a la izquierda del visor */}
          <Show when={scanEditorVisible()}>
            <div class="shrink-0 min-w-0 border-r border-border" style={{ width: `${100 * scanEditorFraction()}%` }}>
              <ScanEditor
                tab={scanEditorTab()}
                onTab={setScanEditorTab}
                onClose={() => setScanEditorOpen(false)}
                cloud={scanCloud}
                onCreateModel={handleScanModel}
                onRepair={() => pipeline.setActiveStep("repair")}
                hasModel={meshLoaded()}
                isProcessing={isProcessing()}
              />
            </div>
            <div class="shrink-0 w-1 cursor-col-resize bg-border hover:bg-accent/50 transition-colors" onPointerDown={resizeScanEditor} />
          </Show>
          {/* Editor de texturas a la izquierda del visor */}
          <Show when={textureEditor()}>
            {(selection) => (
              <>
                <div class="shrink-0 min-w-0" style={{ width: `${100 * editorFraction()}%` }}>
                  <TextureEditor
                    materials={editorMaterials()}
                    mesh={displayQuad() ? quadMeshData() : meshData()}
                    selection={selection()}
                    onSelect={setTextureEditor}
                    target={quadMeshData() && !quadForced() ? (displayQuad() ? "quad" : "original") : undefined}
                    onTargetChange={(t) => setShowQuadMesh(t === "quad")}
                    onClose={() => setTextureEditor(undefined)}
                    onExport={handleTextureExport}
                    onImport={handleTextureImport}
                    onEditExternally={handleTextureEditExternally}
                    external={externalEdit()}
                    onStopExternal={stopExternalEdit}
                  />
                </div>
                <div
                  class="shrink-0 w-1 cursor-col-resize bg-border hover:bg-accent/50 transition-colors"
                  onPointerDown={resizeEditor}
                />
              </>
            )}
          </Show>
          <div class="relative flex-1 min-w-0 min-h-0">
            <Viewport
              onViewerReady={handleViewerReady}
              onFpsUpdate={setFps}
              onPlacementPick={handlePlacementPick}
              onNodePicked={(node) => {
                setSelectedFileNode(node);
                setStatusMessage(`Nodo del archivo: ${sceneStructure()?.nodes[node]?.name || `nodo ${node}`}`);
              }}
              onBoneSelected={handleBoneSelected}
              onBoneMoved={handleBoneMoved}
              onBoneMoveCommitted={handleBoneMoveCommitted}
              onSkeletonTransformed={handleSkeletonGizmo}
              onPoseEdited={handlePoseEdited}
              onSelectionChanged={(joints) => {
                setSelectedJoints(joints);
                if (joints.length > 0) setSelectedControl(undefined);
              }}
              onControlSelected={(id) => setSelectedControl(id ?? undefined)}
              onControlPoseEdited={handleControlPoseEdited}
              onControlsDragged={handleControlsDragged}
              onPathKeyPressed={handlePathKey}
              onPaintSettingsChanged={(change) => setPaintConfig((prev) => ({ ...prev, ...change }))}
              onWeightsPainted={handleWeightsPainted}
              paintSettings={paintSettings()}
              meshData={showsPreview() ? remeshPreviewData() : displayQuad() ? quadMeshData() : meshData()}
              sceneMaterials={viewerMaterials()}
              lights={lights()}
              onLightsChanged={setLights}
              textures={viewerTextures()}
              skeletonData={skeletonData()}
              weightsData={weightsData()}
              settings={viewSettings()}
              vertices={
                showsPreview()
                  ? remeshStats()?.stats.after.vertices
                  : displayQuad()
                    ? quadMeshInfo().vertices
                    : meshLoaded()
                      ? meshInfo().vertices
                      : undefined
              }
              faces={
                showsPreview()
                  ? remeshStats()?.stats.after.triangles
                  : displayQuad()
                    ? quadMeshInfo().quads
                    : meshLoaded()
                      ? meshInfo().faces
                      : undefined
              }
              placementMode={placementMode()}
              floorCandidates={floorCandidates()}
              boneEditMode={boneEditMode()}
              activeTool={activeTool()}
              objectTools={toolCtx() === "object"}
              toolHint={toolDisabledReason(activeTool()) ? undefined : tools().find((t) => t.id === activeTool())?.hint}
              onObjectTransformed={handleObjectGizmo}
              contextMenuItems={viewportMenuItems}
              header={
                <Show when={meshLoaded()}>
                  <ViewportHeader
                    settings={viewSettings()}
                    onChange={(change) => setViewSettings((prev) => ({ ...prev, ...change }))}
                    showGrid={showGrid()}
                    onToggleGrid={() => handleToggleVisibility("grid")}
                    hasSkeleton={skeletonLoaded()}
                    hasWeights={autorigComplete()}
                    boneNames={skeletonBoneNames()}
                    onSelectBone={selectWeightsBone}
                    preview={remeshPreviewData() ? { view: previewView(), onChange: setPreviewView } : undefined}
                  />
                </Show>
              }
            />

            {/* Espacio Diseñar: su propio visor encima del principal */}
            <Show when={showCad()}>
              <CadView store={cad} ui={cadUi} scanMesh={meshLoaded() && !isCadObject(activeObject()) ? meshData() : null} showGrid={showGrid()} actions={designActions} />
            </Show>

            {/* Welcome Screen overlay */}
            <Show when={!hasWork() && pipeline.activeStep() !== "scan" && pipeline.workspace()?.id !== "rig" && !showCad()}>
              <WelcomeScreen
                onOpen={() => handleLoad({ projects: true })}
                onScan={() => openScanEditor("capture")}
                onAnimate={() => pipeline.openWorkspace("rig")}
                onDesign={() => pipeline.openWorkspace("design")}
                onFabricate={() => handleLoad({ then: "print3d" })}
                formats={supportedFormats()?.import.flatMap((f) => f.extensions)}
                recovery={recovery()}
                onRecover={() => handleOpenProject(recovery()!.path, true)}
              />
            </Show>

            <Show when={puppeteering()}>
              <LivePuppet
                target={rigBones()}
                mapFor={(source) => initialRetargetMap(source).map}
                onPose={(pose) => viewer()?.setPose(pose)}
                onRecorded={openRetarget}
                onClose={() => setPuppeteering(false)}
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
          </div>
          <Show when={showPlayback()}>
            <PlaybackBar
              segments={playbackSegments()}
              time={previewTime()}
              playing={previewPlaying()}
              loop={previewLoop()}
              onTogglePlay={togglePreview}
              onStop={stopPreview}
              onLoop={setPreviewLoop}
              onSeek={(t) => {
                setPreviewPlaying(false);
                setPreviewTime(t);
              }}
            />
          </Show>
          <Show when={animating() && activeClip()}>
            {(clip) => (
              <Timeline
                clip={clipPreview() ?? clip()}
                frame={frame()}
                playing={playing()}
                rows={timelineDisplayRows()}
                mode={timelineMode()}
                onMode={setTimelineMode}
                onTool={handleTimelineTool}
                onToggleGroup={(id) => {
                  const next = new Set(collapsedGroups());
                  if (next.has(id)) next.delete(id);
                  else next.add(id);
                  setCollapsedGroups(next);
                }}
                onMute={handleMuteRows}
                onlySelection={onlySelection()}
                onOnlySelection={setOnlySelection}
                onMarker={handleMarker}
                onion={onion()}
                onOnion={setOnion}
                paths={showPaths()}
                onPaths={setShowPaths}
                curves={{
                  rows: curveRows(),
                  rotation: rigCtx().rotation,
                  selection: curveSelection(),
                  onSelection: setCurveSelection,
                  onEdit: handleCurveEdit,
                }}
                mixer={{
                  mixer: displayMixer(),
                  clips: clips(),
                  view: mixerView(),
                  selectedLayer: selectedLayer(),
                  onSelectLayer: setSelectedLayer,
                  onChange: changeMixer,
                  onAddLayer: handleAddLayer,
                  onBake: () => void handleBakeMixer(),
                  onEnabled: (enabled) => setMixer({ ...mixer(), enabled }),
                }}

                selectedJoint={viewSettings().selectedBone}
                selection={keySelection()}
                autoKey={autoKey()}
                interpolation={keyInterpolation()}
                violations={violations()}

                onFrame={(f) => {
                  setPlaying(false);
                  setFrame(f);
                }}
                onTogglePlay={() => setPlaying(!playing())}
                onRangeChange={handleClipRange}
                onSelectJoint={(joint) => selectJoints([joint], joint)}
                selectedControl={selectedControl() ?? selectedChain() ?? selectedConstraint()}
                onSelectControl={(id) =>
                  isConstraintId(id) ? setSelectedConstraint(id) : chainById(id) ? setSelectedChain(id) : handleSelectControl(id)
                }
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
            designPanel={(section) => <DesignStep store={cad} ui={cadUi} actions={designActions} section={section()} hasModel={meshLoaded()} />}
            structureProps={{
              structure: sceneStructure(),
              fileName: fileName(),
              format: meshInfo().format,
              textureUrls: textureUrls(),
              onOpenTexture: (selection) => openTextureEditor("original", selection),
            }}
            lights={lights()}
            onLightsChange={setLights}
            onSection={pipeline.setActiveStep}
            objectProps={{
              meshInfo: meshLoaded() ? meshInfo() : undefined,
              node: (() => {
                const index = selectedFileNode();
                const node = index === undefined ? undefined : sceneStructure()?.nodes[index];
                if (index === undefined || !node) return undefined;
                const mesh = node.mesh !== null ? sceneStructure()?.meshes[node.mesh] : undefined;
                return {
                  name: node.name || `nodo ${index}`,
                  hint: mesh ? `malla ${mesh.name || node.mesh}` : node.skin !== null ? "skin" : "nodo",
                  translation: node.translation,
                  rotation: node.rotation,
                  scale: node.scale,
                  hidden: hiddenFileNodes().includes(index),
                };
              })(),
              onToggleNode: () => {
                const index = selectedFileNode();
                if (index !== undefined) toggleFileNode(index);
              },
              onDeselectNode: () => setSelectedFileNode(undefined),
              onNodeTransform: (t) => {
                const index = selectedFileNode();
                if (index !== undefined) void transformFileNode(index, t);
              },
              placement: {
                mode: placementMode(),
                onPickMode: handlePlacementMode,
                onRotate: handleRotate,
                onMirror: handleMirror,
                onDrop: handleDrop,
                onOrigin: handleOrigin,
                onScale: handleScaleModel,
                onMove: handleMoveModel,
                size: modelSize() && viewSize(modelSize()!),
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
            remeshProps={{
              mode: remeshMode(),
              onModeChange: setRemeshMode,
              mesh: { vertices: meshInfo().vertices, faces: meshInfo().faces },
              retopology: {
              result: quadMeshInfo(),
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
              },
              remeshStats: remeshStats(),
              simplify: {
                config: simplifyConfig(),
                onChange: setSimplifyConfig,
                onPreview: handleRemeshPreview,
                onApply: handleRemeshApply,
                onDiscard: discardRemeshPreview,
                canExecute: meshLoaded(),
                isProcessing: isProcessing(),
              },
              smooth: {
                config: smoothConfig(),
                onChange: setSmoothConfig,
                onPreview: handleRemeshPreview,
                onApply: handleRemeshApply,
                onDiscard: discardRemeshPreview,
                canExecute: meshLoaded(),
                isProcessing: isProcessing(),
              },
            }}
            uvProps={{
              config: uvConfig(),
              onChange: setUvConfig,
              hasRetopology: quadMeshLoaded(),
              info: uvInfo(),
              isProcessing: isProcessing(),
              onUnwrap: handleUvUnwrap,
              onRestore: handleUvRestore,
              hasModel: !!meshData(),
              modelHasUvs: meshData()?.uvs !== undefined,
              onUnwrapOriginal: handleUvUnwrapOriginal,
              canUndoOriginal: canUndoUnwrap(),
              onUndoOriginal: handleUvUndoOriginal,
              hasRig: autorigComplete(),
              preview: uvPreview(),
              onPreviewChange: handleUvPreview,
              onOpenEditor: () =>
                openTextureEditor("quad", { material: Math.max(0, skinMaterials().findIndex((m) => m.maps.base)), slot: "base" }),
            }}
            skeletonProps={skeletonStepProps()}
            scanProps={{
              editorOpen: scanEditorVisible(),
              onOpenEditor: () => {
                setTextureEditor(undefined);
                setScanEditorOpen(true);
              },
              cloud: scanCloud.info(),
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
              blocker: animationBlocker(),
              hasWeights: meshLoaded() && autorigComplete(),
              onGoSkeleton: () => pipeline.setActiveStep("skeleton"),
              // Con malla y esqueleto, los pesos se calculan desde Animar mismo
              onComputeWeights: meshLoaded() && skeletonLoaded() && !autorigComplete() ? () => void handleAutorig() : undefined,
              computing: isProcessing(),
              onPaintWeights: () => {
                pipeline.setActiveStep("skeleton");
                useTool("paint");
              },
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
              onOpenLibrary: openLibrary,
              editorOpen: rigEditorOpen(),
              onToggleEditor: toggleRigEditor,
              puppeteering: puppeteering(),
              onTogglePuppet: togglePuppet,
              onCaptureVideo: handleCapture,
              onImportBvh: () => void handleImportBvh(),
            }}
            exportProps={{

              onExport: () => void (exportDesign() ? handleExportDesign() : handleExport()),
              canExport: exportDesign() ? hasDesignBody() : hasWork(),
              design: exportDesign(),
              designParts: designParts(),
              designPart: exportPart(),
              onDesignPartChange: setExportPart,
              skeletonOnly: !meshLoaded(),
              hasSkeleton: !!skeletonData(),
              boneShapes: exportBoneShapes(),
              onBoneShapesChange: setExportBoneShapes,
              autorigComplete: autorigComplete(),
              hasQuadMesh: quadMeshLoaded(),
              includeRig: exportIncludeRig(),
              onIncludeRigChange: setExportIncludeRig,
              useRetopology: exportUseRetopology(),
              onUseRetopologyChange: setExportUseRetopology,
              compactUv: compactUv(),
              onCompactUvChange: setExportCompactUv,
              options: exportOptions(),
              onOptionsChange: setExportOptions,
              lastExport: lastExport(),
            }}
            viewSettings={viewSettings()}
            onViewSettingsChange={(settings) => {
              setViewSettings(settings);
              setTrackpadNavigation(settings.trackpadNavigation === true);
            }}
            boneNames={skeletonBoneNames()}
            hasWeights={skeletonLoaded()}
            sceneTree={meshLoaded() || cad.doc() ? sceneTree() : undefined}
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
          message={showCad() && !freshStatus() ? undefined : statusMessage()}
          progress={progress()}
          info={animating() ? poseInfo() : undefined}
          hints={showCad() ? designHints(cad, cadUi) : undefined}
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

      <Show when={capturing()}>
        <CaptureDialog
          onMotion={(motion, name) => {
            setCapturing(false);
            openRetarget(motion, name);
          }}
          onCancel={() => setCapturing(false)}
        />
      </Show>

      <Show when={retargeting()}>
        {(request) => <RetargetDialog {...request()} onConfirm={(r) => void confirmRetarget(r)} onCancel={() => setRetargeting(undefined)} />}
      </Show>

      <Show when={confirmation()}>
        {(request) => (
          <ConfirmDialog
            {...request()}
            onConfirm={() => answerConfirmation(true)}
            onCancel={() => answerConfirmation(false)}
          />
        )}
      </Show>
    </div>
  );
};
