import { Component, createSignal, onMount, onCleanup, Show } from "solid-js";
import { invoke, Channel } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { Header, StatusBar, Viewport, Toolbar } from "./components/layout";
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
import { Viewer3D, MeshData, SkeletonData, WeightsData } from "./lib/Viewer3D";
import { createPipelineStore } from "./lib/pipeline";
import { buildSceneTree } from "./lib/scene-tree";
import type { ToolId } from "./lib/tools";
import { createHistoryStore } from "./lib/history";
import { createShortcutManager } from "./lib/shortcuts";
import type { SkeletonTransform } from "./components/panels/SkeletonTransformPanel";
import type { MeshDiagnostics, RepairResult, RepairAnalysisConfig, RepairOptions } from "./components/panels/RepairPanel";
import type { MeshAnalysis, SubdivideResult, ScaleParams, SubdivideConfig } from "./components/panels/Print3DPanel";

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

interface TauriSkeletonData {
  bones: Array<{
    name: string;
    position: [number, number, number];
    parent: number | null;
    is_leaf: boolean;
  }>;
  edges: Array<[number, number]>;
}

interface TauriWeightsData {
  num_vertices: number;
  num_bones: number;
  bone_names: string[];
  weights: number[];
  max_influences: number;
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
}

interface SupportedFormats {
  import: Array<{ id: string; name: string; extensions: string[]; description: string }>;
  export: Array<{ id: string; name: string; extensions: string[]; description: string }>;
}

// Repair types
interface TauriMeshDiagnosticsInfo {
  boundary_loops: number;
  boundary_edges: number;
  duplicate_vertices: number;
  degenerate_faces: number;
  zero_area_faces: number;
  needle_faces: number;
  cap_faces: number;
  non_manifold_edges: number;
  non_manifold_vertices: number;
  normals_consistent: boolean;
  connected_components: number;
  is_closed: boolean;
  self_intersections: number;
  needs_repair: boolean;
  is_healthy: boolean;
}

interface TauriRepairResult {
  vertices_merged: number;
  faces_removed: number;
  faces_flipped: number;
  holes_filled: number;
  faces_added: number;
  non_manifold_fixed: number;
  new_mesh_info: MeshInfo;
  new_diagnostics: TauriMeshDiagnosticsInfo;
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
}

interface TauriQuadMeshData {
  positions: number[];
  normals: number[];
  indices: number[];
  quad_indices: number[];
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

function tauriWeightsToViewer(data: TauriWeightsData): WeightsData {
  return {
    numVertices: data.num_vertices,
    numBones: data.num_bones,
    boneNames: data.bone_names,
    weights: data.weights,
    maxInfluences: data.max_influences,
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
  const [progress, setProgress] = createSignal<{ value: number; label?: string } | undefined>();
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
  const [canUndoPrintScale, setCanUndoPrintScale] = createSignal(false);

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
  const [boneEditMode, setBoneEditMode] = createSignal(false);

  // Active tool
  const [activeTool, setActiveTool] = createSignal<ToolId>("select");

  // Repair state
  const [repairAnalysisConfig, setRepairAnalysisConfig] = createSignal<RepairAnalysisConfig>({
    checkNonManifold: true,
    checkSelfIntersections: false,
  });
  const [repairOptions, setRepairOptions] = createSignal<RepairOptions>({
    mergeDuplicates: true,
    removeDegenerates: true,
    fixNormals: true,
    orientOutward: true,
    fillHoles: true,
    holeFillMethod: "ear_clipping",
    fixNonManifold: false,
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
      setProgress({ value: 30 });

      const info = await invoke<MeshInfo>("import_model", { path: filePath });
      setProgress({ value: 70 });

      const data = await invoke<MeshData>("get_mesh_data");
      setMeshData(data);
      setMeshLoaded(true);
      setMeshInfo({ vertices: info.num_vertices, faces: info.num_faces, format: info.format });

      setProgress({ value: 100 });
      await new Promise((resolve) => setTimeout(resolve, 200));

      setStatusMessage(
        `Modelo cargado: ${info.num_vertices.toLocaleString()} vertices, ${info.num_faces.toLocaleString()} caras (${info.format})`
      );
      setProgress(undefined);

      // Reset other states
      setAutorigComplete(false);
      setWeightsData(undefined);
      setDiagnostics(undefined);
      setRepairResult(undefined);
      setCanUndoRepair(false);
      setMeshAnalysis(undefined);
      setSubdivideResult(undefined);
      setQuadMeshData(undefined);
      setQuadMeshLoaded(false);
      setQuadMeshInfo({ vertices: 0, quads: 0 });
      setQuadQuality(undefined);
      setShowQuadMesh(false);
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

      const weightsResult = await invoke<TauriWeightsData>("get_weights_data");
      const viewerWeights = tauriWeightsToViewer(weightsResult);
      setWeightsData(viewerWeights);
      setBoneNames(viewerWeights.boneNames);

      const skelData = await invoke<TauriSkeletonData>("get_skeleton_data");
      setSkeletonData(tauriSkeletonToViewer(skelData));

      setIsProcessing(false);
      setAutorigComplete(true);
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

      const selected = await save({
        title: "Exportar modelo",
        filters: formats.export.map((f) => ({
          name: f.name,
          extensions: f.extensions,
        })),
      });

      if (!selected) return;

      setStatusMessage(`Exportando a ${selected.split("/").pop()}...`);
      setProgress({ value: 50 });

      const ext = selected.split(".").pop()?.toLowerCase() || "glb";

      const includeRig = exportIncludeRig() && autorigComplete();
      const result = await invoke<ExportResult>("export_model", {
        config: {
          format: ext,
          path: selected,
          include_skeleton: includeRig,
          include_weights: includeRig,
          use_retopology: exportUseRetopology() && quadMeshLoaded(),
        },
      });

      setProgress({ value: 100 });
      await new Promise((resolve) => setTimeout(resolve, 200));

      setStatusMessage(result.message);
      setProgress(undefined);

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

      const quadData = await invoke<TauriQuadMeshData>("get_quad_mesh_data");

      const meshDataForViewer: MeshData = {
        positions: quadData.positions,
        normals: quadData.normals,
        indices: quadData.indices,
        quadIndices: quadData.quad_indices,
      };
      setQuadMeshData(meshDataForViewer);
      setQuadMeshLoaded(true);
      setShowQuadMesh(true);

      setIsProcessing(false);
      setProgress(undefined);
      setStatusMessage(
        `Retopologia completada: ${info.num_vertices.toLocaleString()} vertices, ${info.num_quads.toLocaleString()} quads`
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
      const result = await invoke<TauriMeshDiagnosticsInfo>("analyze_mesh", {
        config: {
          check_non_manifold: config.checkNonManifold,
          check_self_intersections: config.checkSelfIntersections,
        },
      });
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
      const result = await invoke<TauriRepairResult>("repair_mesh", {
        config: {
          merge_duplicates: opts.mergeDuplicates,
          remove_degenerates: opts.removeDegenerates,
          fix_normals: opts.fixNormals,
          orient_outward: opts.orientOutward,
          fill_holes: opts.fillHoles,
          hole_fill_method: opts.holeFillMethod,
          fix_non_manifold: opts.fixNonManifold,
        },
      });

      setRepairResult(result);
      setDiagnostics(result.new_diagnostics);
      setCanUndoRepair(true);
      // El backend descarta el rig al cambiar la geometría
      setAutorigComplete(false);
      setWeightsData(undefined);

      // Refrescar meshData y meshInfo
      const data = await invoke<MeshData>("get_mesh_data");
      setMeshData(data);
      setMeshInfo({
        vertices: result.new_mesh_info.num_vertices,
        faces: result.new_mesh_info.num_faces,
        format: meshInfo().format,
      });

      setIsProcessing(false);
      setStatusMessage("Reparación completada");
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
      const info = await invoke<MeshInfo>("undo_repair");

      const data = await invoke<MeshData>("get_mesh_data");
      setMeshData(data);
      setMeshInfo({ vertices: info.num_vertices, faces: info.num_faces, format: meshInfo().format });
      setCanUndoRepair(false);
      setDiagnostics(undefined);
      setRepairResult(undefined);
      setAutorigComplete(false);
      setWeightsData(undefined);
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
      const result = await invoke<TauriPrint3dAnalysis>("analyze_print3d");
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
      const result = await invoke<TauriPrint3dAnalysis>("undo_print_scale");
      setMeshAnalysis(result);
      setSubdivideResult(undefined);
      setAutorigComplete(false);
      setWeightsData(undefined);
      const data = await invoke<MeshData>("get_mesh_data");
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
      const result = await invoke<TauriPrint3dAnalysis>("scale_mesh_for_print", { params });
      setMeshAnalysis(result);
      setCanUndoPrintScale(true);
      // El backend descarta el rig al cambiar la geometría
      setAutorigComplete(false);
      setWeightsData(undefined);

      // Refrescar meshData
      const data = await invoke<MeshData>("get_mesh_data");
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
      const result = await invoke<TauriSubdivideResult>("subdivide_mesh", { config });
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
      const result = await invoke<ExportResult>("export_print3d_piece", {
        pieceIndex: index,
        path: selected,
      });
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

  const handleAutoFit = async () => {
    try {
      const data = await invoke<TauriSkeletonData>("auto_fit_skeleton");
      setSkeletonData(tauriSkeletonToViewer(data));
      setSkeletonTransform({ ...defaultTransform });
      setStatusMessage("Esqueleto ajustado al modelo");
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
      });
      setSkeletonData(tauriSkeletonToViewer(data));
    } catch (e) {
      console.error("Move bone error:", e);
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
            onToolChange={(tool) => {
              setActiveTool(tool);
              if (tool === "move" || tool === "rotate" || tool === "scale") {
                setBoneEditMode(true);
              } else {
                setBoneEditMode(false);
              }
            }}
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
              meshData={showQuadMesh() && quadMeshData() ? quadMeshData() : meshData()}
              skeletonData={skeletonData()}
              weightsData={weightsData()}
              settings={viewSettings()}
              vertices={showQuadMesh() ? quadMeshInfo().vertices : (meshLoaded() ? meshInfo().vertices : undefined)}
              faces={showQuadMesh() ? quadMeshInfo().quads : (meshLoaded() ? meshInfo().faces : undefined)}
              groundSelectionMode={isGroundMode()}
              boneEditMode={boneEditMode()}
              activeTool={activeTool()}
            />

            {/* Welcome Screen overlay */}
            <Show when={!meshLoaded()}>
              <WelcomeScreen onImport={handleLoad} />
            </Show>
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
              quality: quadQuality(),
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
              onResetTransform: handleResetTransform,
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
