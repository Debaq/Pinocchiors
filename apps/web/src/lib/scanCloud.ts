import { createSignal } from "solid-js";
import { createPersisted } from "./ui-state";
import { invoke } from "@tauri-apps/api/core";
import type { CloudSelectTool, PointCloudData, Viewer3D } from "./Viewer3D";
import { t, fmtNumber } from "./i18n.ts";

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
  /** La selección ignora lo que otra parte de la nube tapa */
  const [visibleOnly, setVisibleOnlyState] = createPersisted("scan.select.visibleOnly", true);
  /** Recorte por caja o plano a la vista */
  const [clip, setClipState] = createSignal<"box" | "plane" | null>(null);
  /** Alineación a mano: puntos marcados en la toma activa y en las demás */
  const [pairMode, setPairModeState] = createSignal(false);
  const [pairs, setPairs] = createSignal<{ from: number; to: number }>({ from: 0, to: 0 });

  /** Corre una tarea con aviso de ocupado y el error a la vista */
  const run = async <T>(what: string, task: () => Promise<T>): Promise<T | undefined> => {
    if (busy()) return;
    setBusy(what);
    try {
      return await task();
    } catch (e) {
      onMessage(`${t(what)}: ${e}`);
    } finally {
      setBusy(undefined);
    }
  };

  /** Trae la nube al visor */
  const refresh = async (fit = false) => {
    const data = parseCloud(await invoke<ArrayBuffer>("scan_cloud_data"));
    viewer()?.setCloudVisibleOnly(visibleOnly());
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
      onMessage(t("{source}: {n} puntos", { source: info()!.source, n: fmtNumber(info()!.points) }));
    });

  const importPly = (path: string) =>
    run("Abrir la nube", async () => {
      await adopt(await invoke<CloudInfo>("scan_cloud_import", { path }), true);
      onMessage(t("{source} abierta: {n} puntos", { source: info()!.source, n: fmtNumber(info()!.points) }));
    });

  const exportPly = (path: string) =>
    run("Guardar la nube", async () => {
      await invoke("scan_cloud_export", { path });
      onMessage(t("Nube guardada en {path}", { path }));
    });

  /** Aplica una edición; con `selectOnly` solo marca en el visor lo que quitaría */
  const edit = (op: CloudOp, label: string, selectOnly = false) =>
    run(label, async () => {
      const result = await invoke<CloudEditResult>("scan_cloud_edit", { op, selectOnly });
      const note = result.note ? ` (${result.note})` : "";
      if (selectOnly) {
        viewer()?.setCloudSelection(result.selected ?? []);
        onMessage(t("{label}: {n} puntos seleccionados{note}", { label: t(label), n: fmtNumber(result.removed), note }));
        return;
      }
      await adopt(result.info);
      const what = op.kind === "downsample" || op.kind === "smooth" ? t("quedan {n} puntos", { n: fmtNumber(result.info.points) }) : t("{n} puntos quitados", { n: fmtNumber(result.removed) });
      onMessage(`${t(label)}: ${what}${note}`);
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
        onMessage(t("{name}: {n} puntos. {note}", { name, n: fmtNumber(result.added), note: result.note }));
      } else if (result.overlap !== null) {
        const warn = result.overlap < 0.15 ? t(". Poco solape: revisa que haya caído en su lugar") : "";
        onMessage(t("{name}: {n} puntos · calza {fit} % (error {err} mm){warn}", { name, n: fmtNumber(result.added), fit: Math.round(result.overlap * 100), err: result.rmse.toFixed(2), warn }));
      } else {
        onMessage(t("{name}: {n} puntos", { name, n: fmtNumber(result.added) }));
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
      const warn = overlap < 0.15 ? t(". Poco solape: revisa que haya caído en su lugar (se deshace con Ctrl+Z)") : "";
      onMessage(t("{source} alineada: calza {fit} % (error {err} mm){warn}", { source: result.info.source, fit: Math.round(overlap * 100), err: result.rmse.toFixed(2), warn }));
    });

  /** Fusiona las tomas a la vista en una toma nueva */
  const fuse = (align: AlignMode, average: boolean) =>
    run("Fusionar", async () => {
      const result = await invoke<FuseResult>("scan_cloud_fuse", { align, fuse: average });
      await adopt(result.info, true);
      const worst =
        result.worst_overlap !== null && result.worst
          ? t(" · el peor encaje: {worst} ({pct} %)", { worst: result.worst, pct: Math.round(result.worst_overlap * 100) })
          : "";
      const warn = result.worst_overlap !== null && result.worst_overlap < 0.15 ? t(". Poco solape: revisa esa toma") : "";
      onMessage(t("{fused} tomas fusionadas: {n} puntos{worst}{warn}. Las originales quedan ocultas en la lista", { fused: result.fused, n: fmtNumber(result.info.points), worst, warn }));
    });

  /** Fija la referencia: la nube actual, un PLY o ninguna */
  const setReference = (action: "current" | "ply" | "clear", path?: string) =>
    run("Referencia", async () => {
      await adopt(await invoke<CloudInfo>("scan_cloud_reference", { action, path: path ?? null }));
      if (action !== "clear") onMessage(t("Referencia fijada: {name}", { name: info()!.reference!.name }));
    });

  /** Mueve la nube para que calce sobre la referencia */
  const alignToReference = (align: AlignMode) =>
    run("Alinear a la referencia", async () => {
      const result = await invoke<AlignResult>("scan_cloud_align_reference", { align });
      await adopt(result.info);
      onMessage(t("Alineada a la referencia: calza {fit} % (error {err} mm)", { fit: Math.round((result.overlap ?? 0) * 100), err: result.rmse.toFixed(2) }));
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
    // Con un gizmo o los puntos de alineación a la vista, el clic no selecciona
    setClipState(null);
    viewer()?.setCloudClip(null);
    setPairModeState(false);
    viewer()?.setCloudPairMode(false);
    setToolSignal(next);
    viewer()?.setCloudTool(next);
  };

  const setVisibleOnly = (on: boolean) => {
    setVisibleOnlyState(on);
    viewer()?.setCloudVisibleOnly(on);
  };

  /** Recorte por caja o plano: se arrastra el gizmo y lo marcado se borra o se conserva */
  const setClip = (shape: "box" | "plane" | null) => {
    const v = viewer();
    if (!v) return;
    setPairModeState(false);
    v.setCloudPairMode(false);
    // Con el gizmo a la vista, el rectángulo y el lazo no deben tomar los clics
    v.setCloudTool(shape ? null : tool());
    v.setCloudClip(shape);
    setClipState(shape);
  };
  const setClipMode = (mode: "translate" | "scale" | "rotate") => viewer()?.setCloudClipMode(mode);
  const flipClip = () => viewer()?.flipCloudClip();

  /** Activa o apaga el marcado de parejas de puntos para alinear a mano */
  const setPairMode = (on: boolean) => {
    const v = viewer();
    if (!v) return;
    if (on) {
      setClip(null);
      v.setCloudTool(null);
    } else {
      v.setCloudTool(tool());
    }
    v.setCloudPairMode(on);
    v.setCloudPairListener(on ? (from, to) => setPairs({ from, to }) : null);
    setPairs({ from: 0, to: 0 });
    setPairModeState(on);
  };
  const undoPair = () => viewer()?.undoCloudPair();
  const clearPairs = () => viewer()?.clearCloudPairs();

  /** Lleva la toma activa sobre las demás con las parejas de puntos marcadas */
  const alignByPoints = () =>
    run("Alinear por puntos", async () => {
      const marked = viewer()?.getCloudPairs();
      if (!marked) return;
      const result = await invoke<AlignResult>("scan_cloud_align_points", { from: marked.from, to: marked.to });
      viewer()?.clearCloudPairs();
      await adopt(result.info);
      onMessage(t("{source} alineada por puntos (error {err} mm)", { source: result.info.source, err: result.rmse.toFixed(2) }));
    });

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
    visibleOnly,
    setVisibleOnly,
    clip,
    setClip,
    setClipMode,
    flipClip,
    pairMode,
    setPairMode,
    pairs,
    undoPair,
    clearPairs,
    alignByPoints,
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
