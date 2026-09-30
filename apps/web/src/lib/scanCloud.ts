import { createSignal } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import type { CloudSelectTool, PointCloudData, Viewer3D } from "./Viewer3D";

/** Nube en edición (`CloudInfo` en apps/desktop/src/scan_cloud.rs) */
export interface CloudInfo {
  points: number;
  has_color: boolean;
  version: number;
  spacing_mm: number;
  source: string;
  undo: string | null;
  redo: string | null;
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

/** Lee la nube binaria de `scan_cloud_data` */
function parseCloud(buf: ArrayBuffer): PointCloudData {
  const [count, version, hasColor] = new Uint32Array(buf, 0, 4);
  const positions = new Float32Array(buf, 16, count * 3);
  const colors = hasColor ? new Uint8Array(buf, 16 + count * 12, count * 3) : null;
  return { count, version, positions, colors };
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
    await refresh(fit);
  };

  const take = () =>
    run("Tomar la nube", async () => {
      await adopt(await invoke<CloudInfo>("scan_cloud_take"), true);
      onMessage(`Nube lista para editar: ${info()!.points.toLocaleString()} puntos`);
    });

  const importPly = (path: string) =>
    run("Abrir la nube", async () => {
      await adopt(await invoke<CloudInfo>("scan_cloud_import", { path }), true);
      onMessage(`Nube abierta: ${info()!.points.toLocaleString()} puntos`);
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
    select,
  };
}

export type ScanCloud = ReturnType<typeof createScanCloud>;
