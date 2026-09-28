import { Component, createSignal, onMount, onCleanup, Show } from "solid-js";
import { invoke, Channel } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { Header, StatusBar, Viewport, Toolbar, ProgressOverlay } from "./components/layout";
import { WelcomeScreen } from "./components/layout/WelcomeScreen";
import { ContextPanel } from "./components/layout/ContextPanel";
import { PipelineBar } from "./components/pipeline";
import {
  type SkeletonPreset,
  type AutorigConfig,
  type ViewSettings,
  type RetopologyConfig,
  type QuadQuality,
} from "./components/panels";
import { Viewer3D, MeshData, MeshTextures, PaintSettings, PaintStroke, SkeletonData, WeightsData } from "./lib/Viewer3D";
import { createPipelineStore } from "./lib/pipeline";
import { buildSceneTree } from "./lib/scene-tree";
import type { ToolId } from "./lib/tools";
import { createHistoryStore } from "./lib/history";
import { createShortcutManager } from "./lib/shortcuts";
import { decodeMesh, decodeWeights } from "./lib/buffers";
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
  };
}

// ═══════════════════════════════════════════════════════════════════════════
// APP COMPONENT
// ═══════════════════════════════════════════════════════════════════════════

export const App: Component = () => {
  // Pipeline
  const pipeline = createPipelineStore();

  // History (undo/redo)
  const history = createHistoryStore();

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

  /** Malla actual del backend, en binario (sin JSON) */
  const fetchMeshData = () =>
    busy("Cargando en el visor...", async () => decodeMesh(await invoke<ArrayBuffer>("get_mesh_data")));
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

  // Ground mode
  const [isGroundMode, setIsGroundMode] = createSignal(false);

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
    (showQuadMesh() || (usesQuad() && ["uv", "skeleton"].includes(pipeline.activeStep()))) && !!quadMeshData();

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

  // View settings
  const [viewSettings, setViewSettings] = createSignal<ViewSettings>({
    showMesh: true,
    showWireframe: false,
    showSkeleton: true,
    showWeights: false,
    selectedBone: -1,
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

  // Load presets on mount
  onMount(async () => {
    // Register keyboard shortcuts
    shortcuts.register([
      { key: "z", ctrl: true, action: () => history.undo(), description: "Deshacer" },
      { key: "z", ctrl: true, shift: true, action: () => history.redo(), description: "Rehacer" },
      { key: "q", action: () => { setActiveTool("select"); setBoneEditMode(false); }, description: "Seleccionar" },
      { key: "g", action: () => { setActiveTool("move"); setBoneEditMode(true); }, description: "Mover" },
      { key: "r", action: () => { setActiveTool("rotate"); setBoneEditMode(true); }, description: "Rotar" },
      { key: "s", action: () => { setActiveTool("scale"); setBoneEditMode(true); }, description: "Escalar" },
      { key: "Home", action: () => viewerRef?.resetView(), description: "Reset vista" },
      { key: " ", action: () => viewerRef?.resetView(), description: "Reset vista" },
      { key: "f", action: () => viewerRef?.focusSelection(), description: "Foco en selección" },
      { key: "n", action: () => setShowContextPanel(!showContextPanel()), description: "Toggle panel" },
      { key: "1", action: () => viewerRef?.setView("front"), description: "Vista frontal" },
      { key: "3", action: () => viewerRef?.setView("right"), description: "Vista derecha" },
      { key: "7", action: () => viewerRef?.setView("top"), description: "Vista superior" },
      { key: "b", action: () => useTool("paint"), description: "Pintar pesos" },
    ]);
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
      clearQuadMesh();
      setLastExport(undefined);
      setCanUndoPrintScale(false);

      // Pipeline: mark import as completed, navigate to next
      pipeline.markCompleted("import");
      pipeline.setActiveStep("import");
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

  const handleSetGround = () => {
    const newMode = !isGroundMode();
    setIsGroundMode(newMode);
    if (newMode) {
      setStatusMessage("Haz click en una cara para establecerla como suelo");
    } else {
      setStatusMessage("Modo de seleccion de suelo cancelado");
    }
  };

  const handleGroundSelected = () => {
    setIsGroundMode(false);
    setStatusMessage("Suelo establecido - modelo reorientado");
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
        },
        onProgress,
      });

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

  const handleTransformChange = async (transform: SkeletonTransform) => {
    const prevTransform = { ...skeletonTransform() };
    try {
      await history.execute({
        description: "Transformar esqueleto",
        execute: () => applyTransform(transform),
        undo: () => applyTransform(prevTransform),
      });
    } catch (e) {
      console.error("Transform error:", e);
      setStatusMessage(`Error: ${e}`);
    }
  };

  /** Cambió un apéndice: se rehace la plantilla */
  const handleBodyPlanChange = async (plan: BodyPlan) => {
    setBodyPlan(plan);
    try {
      const data = await invoke<TauriSkeletonData>("select_body_plan", { plan });
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
        setSkeletonData(tauriSkeletonToViewer(data));
        setSkeletonTransform({ ...defaultTransform });
        setStatusMessage("Esqueleto reseteado");
      } catch (e) {
        console.error("Reset error:", e);
      }
    }
  };

  const handleBoneMoved = async (index: number, position: [number, number, number]) => {
    try {
      const data = await invoke<TauriSkeletonData>("move_bone", {
        boneIndex: index,
        position,
        mirror: symmetricEdit(),
      });
      setSkeletonData(tauriSkeletonToViewer(data));
    } catch (e) {
      console.error("Move bone error:", e);
    }
  };

  /** Centra en el volumen la articulación seleccionada, o todas */
  const handleCenterBones = async (onlySelected: boolean) => {
    const selected = viewSettings().selectedBone;
    try {
      const data = await busy("Centrando articulaciones...", () =>
        invoke<TauriSkeletonData>("center_bones", { bones: onlySelected && selected >= 0 ? [selected] : null })
      );
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
    const send = (rows: Float32Array) =>
      invoke("set_vertex_weights", { vertices: Array.from(stroke.vertices), influences: Array.from(rows) });
    let first = true;
    try {
      await history.execute({
        description: "Pintar pesos",
        execute: async () => {
          // La primera vez el visor ya lo tiene aplicado
          if (!first) viewerRef?.applyWeightRows(stroke.vertices, stroke.after);
          first = false;
          await send(stroke.after);
        },
        undo: async () => {
          viewerRef?.applyWeightRows(stroke.vertices, stroke.before);
          await send(stroke.before);
        },
      });
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
          fileName={fileName()}
        />

        {/* Pipeline Bar */}
        <PipelineBar
          getStepStatus={(stepId) => pipeline.getStepStatus(stepId, meshLoaded())}
          onStepClick={(stepId) => pipeline.navigateTo(stepId, meshLoaded())}
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

          {/* Viewport */}
          <div class="relative flex-1 min-w-0 min-h-0">
            <Viewport
              onViewerReady={handleViewerReady}
              onFpsUpdate={setFps}
              onGroundSelected={handleGroundSelected}
              onBoneSelected={handleBoneSelected}
              onBoneMoved={handleBoneMoved}
              onWeightsPainted={handleWeightsPainted}
              paintSettings={paintSettings()}
              meshData={displayQuad() ? quadMeshData() : meshData()}
              textures={viewerTextures()}
              skeletonData={skeletonData()}
              weightsData={weightsData()}
              settings={viewSettings()}
              vertices={displayQuad() ? quadMeshInfo().vertices : (meshLoaded() ? meshInfo().vertices : undefined)}
              faces={displayQuad() ? quadMeshInfo().quads : (meshLoaded() ? meshInfo().faces : undefined)}
              groundSelectionMode={isGroundMode()}
              boneEditMode={boneEditMode()}
              activeTool={activeTool()}
            />

            {/* Welcome Screen overlay */}
            <Show when={!meshLoaded()}>
              <WelcomeScreen onImport={handleLoad} />
            </Show>

            <ProgressOverlay progress={progress()} />
          </div>

          {/* Context Panel (right sidebar) */}
          <Show when={showContextPanel()}>
          <ContextPanel
            activeStep={pipeline.activeStep()}
            importProps={{
              meshInfo: meshLoaded() ? meshInfo() : undefined,
              onImport: handleLoad,
              onSetGround: handleSetGround,
              isGroundMode: isGroundMode(),
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
            onViewSettingsChange={setViewSettings}
            boneNames={boneNames()}
            hasWeights={autorigComplete()}
            sceneTree={meshLoaded() ? sceneTree() : undefined}
            onToggleVisibility={handleToggleVisibility}
            onSelectNode={handleSelectNode}
          />
          </Show>
        </div>

        {/* Status Bar */}
        <StatusBar
          message={statusMessage()}
          progress={progress()}
        />
      </div>
    </div>
  );
};
