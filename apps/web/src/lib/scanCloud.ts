import { createSignal } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import type { CloudSelectTool, PointCloudData, Viewer3D } from "./Viewer3D";

/** Una toma: un escaneo o una nube abierta (`TakeInfo`) */
export interface TakeInfo {
  name: string;
  points: number;
  visible: boolean;
  /** Escaneos o nubes que junta (más de 1 en una fusión) */
  parts: number;
}

/** Tomas en edición (`CloudInfo` en apps/desktop/src/scan_cloud.rs); los
 *  campos sueltos son de la toma activa */
export interface CloudInfo {
  points: number;
  has_color: boolean;
  version: number;
  spacing_mm: number;
  source: string;
  undo: string | null;
  redo: string | null;
  /** Escaneos o nubes que junta la toma activa */
  parts: number;
  /** Todas las tomas, en orden */
  takes: TakeInfo[];
  /** Índice de la toma activa */
  active: number;
  /** Nube de referencia para comparar */
  reference: { name: string; points: number } | null;
  /** Se muestra el mapa de desviaciones en lugar de los colores */
  comparing: boolean;
}

/** Cómo se alinea una toma (`scan_cloud_add`, `scan_cloud_fuse`) */
export type AlignMode = "auto" | "fine" | "none";

/** Resultado de sumar o alinear una toma (`AlignResult`) */
interface AlignResult {
  added: number;
  /** `null` si no hubo con qué alinear */
  overlap: number | null;
  rmse: number;
  note: string | null;
  info: CloudInfo;
}

/** Resultado de fusionar las tomas a la vista (`FuseResult`) */
interface FuseResult {
  fused: number;
  worst_overlap: number | null;
  worst: string | null;
  info: CloudInfo;
}

/** Comparación con la referencia (`CompareResult`), en mm */
export interface CompareResult {
  compared: number;
  unmatched: number;
  mean_mm: number;
  rms_mm: number;
  max_mm: number;
  p95_mm: number;
  within_percent: number;
}

/** Parámetros de la comparación (mm) */
export interface CompareSettings {
  tolerance_mm: number;
  scale_mm: number;
  reach_mm: number;
}

/** Una edición de la nube (`CloudOp`) */
export type CloudOp =
  | { kind: "delete"; indices: number[]; version: number }
  | { kind: "keep"; indices: number[]; version: number }
  | { kind: "statistical"; neighbors: number; std_ratio: number }
  | { kind: "radius"; radius_mm: number; min_neighbors: number }
  | { kind: "fragments"; gap_mm: number; min_percent: number }
  | { kind: "plane"; threshold_mm: number; below: boolean }
  | { kind: "downsample"; voxel_mm: number }
  | { kind: "smooth"; radius_mm: number; strength: number };

interface CloudEditResult {
  removed: number;
  selected: number[] | null;
  note: string | null;
  info: CloudInfo;
}

/** Lee las nubes binarias de `scan_cloud_data`: la toma activa y, de
 *  fondo, las demás a la vista */
function parseCloud(buf: ArrayBuffer): PointCloudData {
  const [count, version, hasColor, ghostCount] = new Uint32Array(buf, 0, 4);
  const positions = new Float32Array(buf, 16, count * 3);
  const colors = hasColor ? new Uint8Array(buf, 16 + count * 12, count * 3) : null;
  const ghostAt = 16 + count * 12 + Math.ceil((count * 3) / 4) * 4;
  const ghost = ghostCount > 0 ? new Float32Array(buf, ghostAt, ghostCount * 3) : null;
  return { count, version, positions, colors, ghost };
}

/**
 * Nube de puntos del escáner en edición: la toma del escaneo o de un PLY, la
 * muestra en el visor y aplica las herramientas de limpieza y selección
 */
export function createScanCloud(viewer: () => Viewer3D | undefined, onMessage: (text: string) => void) {
  const [info, setInfo] = createSignal<CloudInfo>();
  const [busy, setBusy] = createSignal<string>();
  const [selected, setSelected] = createSignal(0);
  const [tool, setToolSignal] = createSignal<CloudSelectTool>("rect");
  /** La nube se ve en el visor en lugar del modelo */
  const [shown, setShown] = createSignal(true);
  /** Última comparación con la referencia, mientras el mapa está a la vista */
  const [comparison, setComparison] = createSignal<CompareResult>();

  /** Corre una tarea con aviso de ocupado y el error a la vista */
  const run = async <T>(what: string, task: () => Promise<T>): Promise<T | undefined> => {
    if (busy()) return;
    setBusy(what);
    try {
      return await task();
    } catch (e) {
      onMessage(`${what}: ${e}`);
    } finally {
      setBusy(undefined);
    }
  };

  /** Trae la nube al visor */
  const refresh = async (fit = false) => {
    const data = parseCloud(await invoke<ArrayBuffer>("scan_cloud_data"));
    viewer()?.setPointCloud(data.count > 0 ? data : null, fit);
  };

  const adopt = async (next: CloudInfo | undefined, fit = false) => {
    // Una nube nueva se muestra en lugar del modelo
    if (fit) setShown(true);
    setInfo(next);
    // Cualquier cambio de la nube apaga el mapa de desviaciones
    if (!next?.comparing) setComparison(undefined);
    await refresh(fit);
  };

  const take = () =>
    run("Tomar la nube", async () => {
      await adopt(await invoke<CloudInfo>("scan_cloud_take"), true);
      onMessage(`${info()!.source}: ${info()!.points.toLocaleString()} puntos`);
    });

  const importPly = (path: string) =>
    run("Abrir la nube", async () => {
      await adopt(await invoke<CloudInfo>("scan_cloud_import", { path }), true);
      onMessage(`${info()!.source} abierta: ${info()!.points.toLocaleString()} puntos`);
    });

  const exportPly = (path: string) =>
    run("Guardar la nube", async () => {
      await invoke("scan_cloud_export", { path });
      onMessage(`Nube guardada en ${path}`);
    });

  /** Aplica una edición; con `selectOnly` solo marca en el visor lo que quitaría */
  const edit = (op: CloudOp, label: string, selectOnly = false) =>
    run(label, async () => {
      const result = await invoke<CloudEditResult>("scan_cloud_edit", { op, selectOnly });
      const note = result.note ? ` (${result.note})` : "";
      if (selectOnly) {
        viewer()?.setCloudSelection(result.selected ?? []);
        onMessage(`${label}: ${result.removed.toLocaleString()} puntos seleccionados${note}`);
        return;
      }
      await adopt(result.info);
      const what = op.kind === "downsample" || op.kind === "smooth" ? `quedan ${result.info.points.toLocaleString()} puntos` : `${result.removed.toLocaleString()} puntos quitados`;
      onMessage(`${label}: ${what}${note}`);
    });

  const selection = () => Array.from(viewer()?.getCloudSelection() ?? []);

  const deleteSelection = () => {
    const current = info();
    if (!current || selected() === 0) return;
    return edit({ kind: "delete", indices: selection(), version: current.version }, "Borrar selección");
  };

  const keepSelection = () => {
    const current = info();
    if (!current || selected() === 0) return;
    return edit({ kind: "keep", indices: selection(), version: current.version }, "Recortar a la selección");
  };

  const history = (redo: boolean) =>
    run(redo ? "Rehacer" : "Deshacer", async () => {
      await adopt(await invoke<CloudInfo>("scan_cloud_history", { redo }));
    });

  /** Suma una toma nueva (del escáner o un PLY); con `align` distinto de
   *  "none" se encaja sobre las tomas a la vista */
  const addTake = (from: "scan" | "ply", align: AlignMode, path?: string) =>
    run(from === "scan" ? "Sumar el escaneo" : "Sumar la nube", async () => {
      const first = !info();
      const result = await invoke<AlignResult>("scan_cloud_add", { from, path: path ?? null, align });
      await adopt(result.info, first);
      const name = result.info.source;
      if (result.note) {
        onMessage(`${name}: ${result.added.toLocaleString()} puntos. ${result.note}`);
      } else if (result.overlap !== null) {
        const warn = result.overlap < 0.15 ? ". Poco solape: revisa que haya caído en su lugar" : "";
        onMessage(`${name}: ${result.added.toLocaleString()} puntos · calza ${Math.round(result.overlap * 100)} % (error ${result.rmse.toFixed(2)} mm)${warn}`);
      } else {
        onMessage(`${name}: ${result.added.toLocaleString()} puntos`);
      }
      return true;
    });

  /** Elige, muestra, oculta, quita o renombra una toma */
  const setTake = (index: number, action: "select" | "show" | "hide" | "remove" | "rename", name?: string) =>
    run("Tomas", async () => {
      const next = await invoke<CloudInfo | null>("scan_cloud_take_set", { index, action, name: name ?? null });
      await adopt(next ?? undefined);
    });

  /** Lleva la toma activa sobre las demás a la vista (se deshace) */
  const alignTake = (align: AlignMode) =>
    run("Alinear la toma", async () => {
      const result = await invoke<AlignResult>("scan_cloud_take_align", { align });
      await adopt(result.info);
      const overlap = result.overlap ?? 0;
      const warn = overlap < 0.15 ? ". Poco solape: revisa que haya caído en su lugar (se deshace con Ctrl+Z)" : "";
      onMessage(`${result.info.source} alineada: calza ${Math.round(overlap * 100)} % (error ${result.rmse.toFixed(2)} mm)${warn}`);
    });

  /** Fusiona las tomas a la vista en una toma nueva */
  const fuse = (align: AlignMode, average: boolean) =>
    run("Fusionar", async () => {
      const result = await invoke<FuseResult>("scan_cloud_fuse", { align, fuse: average });
      await adopt(result.info, true);
      const worst =
        result.worst_overlap !== null && result.worst
          ? ` · el peor encaje: ${result.worst} (${Math.round(result.worst_overlap * 100)} %)`
          : "";
      const warn = result.worst_overlap !== null && result.worst_overlap < 0.15 ? ". Poco solape: revisa esa toma" : "";
      onMessage(`${result.fused} tomas fusionadas: ${result.info.points.toLocaleString()} puntos${worst}${warn}. Las originales quedan ocultas en la lista`);
    });

  /** Fija la referencia: la nube actual, un PLY o ninguna */
  const setReference = (action: "current" | "ply" | "clear", path?: string) =>
    run("Referencia", async () => {
      await adopt(await invoke<CloudInfo>("scan_cloud_reference", { action, path: path ?? null }));
      if (action !== "clear") onMessage(`Referencia fijada: ${info()!.reference!.name}`);
    });

  /** Mueve la nube para que calce sobre la referencia */
  const alignToReference = (align: AlignMode) =>
    run("Alinear a la referencia", async () => {
      const result = await invoke<AlignResult>("scan_cloud_align_reference", { align });
      await adopt(result.info);
      onMessage(`Alineada a la referencia: calza ${Math.round((result.overlap ?? 0) * 100)} % (error ${result.rmse.toFixed(2)} mm)`);
    });

  /** Compara con la referencia y pinta el mapa; `show = false` lo apaga */
  const compare = (settings: CompareSettings, show = true) =>
    run("Comparar", async () => {
      const result = await invoke<(CompareResult & { info: CloudInfo }) | null>("scan_cloud_compare", { ...settings, show });
      const current = result?.info ?? (await invoke<CloudInfo | null>("scan_cloud_info")) ?? undefined;
      setInfo(current);
      setComparison(result ?? undefined);
      if (result) setShown(true);
      await refresh();
    });

  const discard = async () => {
    await invoke("scan_cloud_discard");
    await adopt(undefined);
  };

  const select = (what: "all" | "none" | "invert") => viewer()?.setCloudSelection(what);

  const setPointSize = (px: number) => viewer()?.setCloudPointSize(px);
  const setBrushSize = (px: number) => viewer()?.setCloudBrushSize(px);

  const setTool = (next: CloudSelectTool) => {
    setToolSignal(next);
    viewer()?.setCloudTool(next);
  };

  /** Engancha el visor (al crearse) y recupera la nube si ya había una */
  const attach = async (v: Viewer3D) => {
    v.setCloudListener(setSelected);
    try {
      const current = await invoke<CloudInfo | null>("scan_cloud_info");
      if (current) await adopt(current, true);
    } catch (e) {
      console.error("Nube:", e);
    }
  };

  return {
    info,
    busy,
    selected,
    tool,
    setTool,
    setPointSize,
    setBrushSize,
    shown,
    setShown,
    attach,
    refresh,
    take,
    importPly,
    exportPly,
    edit,
    deleteSelection,
    keepSelection,
    history,
    discard,
    addTake,
    setTake,
    alignTake,
    fuse,
    setReference,
    alignToReference,
    compare,
    comparison,
    select,
  };
}

export type ScanCloud = ReturnType<typeof createScanCloud>;
