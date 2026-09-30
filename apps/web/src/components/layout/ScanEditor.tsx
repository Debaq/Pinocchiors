import { Component, For, Match, Show, Switch, createEffect, createSignal, onCleanup, onMount, type JSX } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { clsx } from "clsx";
import { Panel, Slider, Checkbox, Button, IconButton, NumberInput, Tooltip } from "../ui";
import { createPersisted } from "../../lib/ui-state";
import type { AlignMode, CompareSettings, ScanCloud } from "../../lib/scanCloud";
import type { CloudSelectTool } from "../../lib/Viewer3D";
import * as Icons from "../icons";

/** Estado del escáner (`ScannerStatusDto` en apps/desktop/src/scanner.rs) */
interface ScannerStatus {
  state: "off" | "connecting" | "streaming" | "error" | "stopped";
  message: string | null;
  device: string | null;
  serial: string | null;
  depth_stream: string | null;
  color_stream: string | null;
  fps: number;
  distance_cm: number;
  coverage: number;
  scanning: boolean;
  frames: number;
  registered: number;
  dropped: number;
  points: number;
  tracking_ok: boolean;
  /** Cuadros guardados si hay una grabación en curso */
  recorded: number | null;
}

/** Grabación de cuadros crudos (`RecordingDto`) */
interface Recording {
  path: string;
  frames: number;
}

/** Volumen de escaneo y limpieza (`ScanSettingsDto`) */
interface ScanSettings {
  clip_min_mm: number;
  clip_max_mm: number;
  box_mm: number;
  clean_noise: boolean;
  isolate_object: boolean;
  edge_filter: boolean;
  temporal_frames: number;
  /** Calibración: escala de la focal (más = objeto más angosto) */
  fx_scale: number;
  /** Calibración: milímetros por unidad del mapa de profundidad */
  depth_scale: number;
}

/** Tamaño del objeto en el cuadro actual (`MeasurementDto`) */
interface Measurement {
  width_mm: number;
  height_mm: number;
  distance_mm: number;
  points: number;
}

/** Reconstrucción de la malla (`MeshSettingsDto`) */
export interface ScanMeshSettings {
  voxel_mm: number;
  fill: number;
  smooth: number;
  /** Tapar los agujeros (también los grandes) al crear el modelo */
  close_holes: boolean;
  /** Perímetro máximo de un agujero a tapar (mm); 0 = todos */
  max_hole_mm: number;
  /** Parche plano en vez de seguir la curvatura del borde */
  flat_patch: boolean;
  /** Quitar las piezas sueltas chicas antes de tapar */
  remove_pieces: boolean;
}

/** Parámetros de las herramientas de limpieza de la nube */
interface CleanSettings {
  sor_neighbors: number;
  sor_ratio: number;
  radius_mm: number;
  radius_neighbors: number;
  gap_mm: number;
  min_percent: number;
  plane_mm: number;
  plane_below: boolean;
  voxel_mm: number;
  smooth_mm: number;
  smooth_strength: number;
  brush_px: number;
  point_px: number;
}

const DEFAULT_SETTINGS: ScanSettings = {
  clip_min_mm: 120,
  clip_max_mm: 450,
  box_mm: 300,
  clean_noise: true,
  isolate_object: true,
  edge_filter: true,
  temporal_frames: 3,
  fx_scale: 1,
  depth_scale: 0.1,
};

const DEFAULT_CLEAN: CleanSettings = {
  sor_neighbors: 16,
  sor_ratio: 2,
  radius_mm: 3,
  radius_neighbors: 6,
  gap_mm: 5,
  min_percent: 10,
  plane_mm: 2,
  plane_below: true,
  voxel_mm: 2,
  smooth_mm: 3,
  smooth_strength: 0.5,
  brush_px: 30,
  point_px: 2,
};

/** Ancho de una tarjeta bancaria o de identidad (ISO/IEC 7810 ID-1), en mm */
const CARD_WIDTH_MM = 85.6;

const DEFAULT_MESH: ScanMeshSettings = {
  voxel_mm: 2,
  fill: 1,
  smooth: 2,
  close_holes: true,
  max_hole_mm: 0,
  flat_patch: false,
  remove_pieces: true,
};

const DEFAULT_COMPARE: CompareSettings = { tolerance_mm: 0.5, scale_mm: 3, reach_mm: 10 };

const ALIGN_MODES: { id: AlignMode; label: string; title: string }[] = [
  { id: "auto", label: "Automática", title: "Busca sola cómo encaja, aunque el objeto se haya dado vuelta entre escaneos" },
  { id: "fine", label: "Fina", title: "Solo ajusta: la nube nueva ya está casi en su lugar" },
  { id: "none", label: "Sin alinear", title: "Tal cual: las nubes ya comparten el mismo sistema de coordenadas" },
];

const STATE_LABEL: Record<ScannerStatus["state"], string> = {
  off: "Desconectado",
  connecting: "Buscando el escáner…",
  streaming: "Transmitiendo",
  error: "Error",
  stopped: "Detenido",
};

/** Cada cuánto se pide estado y vista previa mientras hay conexión (ms) */
const POLL_MS = 120;

export type ScanEditorTab = "capture" | "cloud" | "mesh" | "setup";

const TABS: { id: ScanEditorTab; label: string; title: string }[] = [
  { id: "capture", label: "Captura", title: "Conectar el escáner, vista en vivo, volumen y escaneo" },
  { id: "cloud", label: "Nube", title: "Selección y limpieza de la nube de puntos" },
  { id: "mesh", label: "Malla", title: "Crear el modelo desde la nube" },
  { id: "setup", label: "Calibración", title: "Calibración del escáner y grabación para diagnóstico" },
];

const SELECT_TOOLS: { id: CloudSelectTool; label: string; icon: Component<{ size?: number }> }[] = [
  { id: "rect", label: "Rectángulo", icon: Icons.SelectionRect },
  { id: "lasso", label: "Lazo", icon: Icons.Lasso },
  { id: "polygon", label: "Cuerda", icon: Icons.SelectionPolygon },
  { id: "brush", label: "Pincel", icon: Icons.SelectBrush },
];

export interface ScanEditorProps {
  tab: ScanEditorTab;
  onTab: (tab: ScanEditorTab) => void;
  onClose: () => void;
  cloud: ScanCloud;
  /** Crea el modelo de trabajo con la malla de la nube editada o del escáner */
  onCreateModel?: (mesh: ScanMeshSettings, fromCloud: boolean) => Promise<void> | void;
  /** Lleva a Reparar (Preparar) con el modelo creado */
  onRepair?: () => void;
  hasModel?: boolean;
  isProcessing?: boolean;
}

/** Fila de valor: etiqueta a la izquierda y dato a la derecha */
const Row = (props: { label: string; children: JSX.Element }) => (
  <div class="flex justify-between gap-3 text-xs">
    <span class="text-text-muted">{props.label}</span>
    <span class="text-text font-mono truncate">{props.children}</span>
  </div>
);

const Hint = (props: { children: JSX.Element }) => (
  <p class="text-[11px] text-text-dim leading-relaxed">{props.children}</p>
);

/** Icono de ayuda: la explicación larga queda en el tooltip y no ocupa el panel */
const Help = (props: { text: string }) => (
  <Tooltip content={props.text} placement="left">
    <span class="text-text-dim hover:text-text-muted cursor-help" aria-label={props.text}>
      <Icons.Info size={13} />
    </span>
  </Tooltip>
);

/** Botón de barra de herramientas: solo icono, con su nombre en el tooltip */
const ToolButton = (props: {
  label: string;
  icon: JSX.Element;
  onClick: () => void;
  active?: boolean;
  disabled?: boolean;
  danger?: boolean;
}) => (
  <Tooltip content={props.label} placement="bottom">
    <button
      type="button"
      aria-label={props.label}
      disabled={props.disabled}
      class={clsx(
        "w-7 h-7 flex items-center justify-center rounded transition-colors",
        "disabled:opacity-40 disabled:pointer-events-none",
        props.active
          ? "bg-accent/20 text-accent"
          : props.danger
            ? "text-text-muted hover:text-red hover:bg-red/15"
            : "text-text-muted hover:text-text hover:bg-surface/60"
      )}
      onClick={() => props.onClick()}
    >
      {props.icon}
    </button>
  </Tooltip>
);

/** Grupo de botones de herramienta enmarcado */
const ToolGroup = (props: { children: JSX.Element }) => (
  <div class="flex items-center gap-0.5 p-0.5 rounded-md bg-bg border border-border/60">{props.children}</div>
);

/**
 * Una herramienta dentro de un panel: nombre, ayuda y acciones en una fila,
 * y sus parámetros debajo en filas compactas
 */
const Tool = (props: { title: string; help?: string; actions?: JSX.Element; children?: JSX.Element; first?: boolean }) => (
  <div class={clsx("space-y-1.5", !props.first && "pt-2.5 border-t border-border/40")}>
    <div class="flex items-center gap-1.5 min-h-6">
      <span class="text-xs font-medium text-text">{props.title}</span>
      <Show when={props.help}>
        <Help text={props.help!} />
      </Show>
      <div class="flex-1" />
      {props.actions}
    </div>
    {props.children}
  </div>
);

/** Dato con su nombre encima, para las fichas de resumen */
const Stat = (props: { label: string; children: JSX.Element }) => (
  <div class="min-w-0 px-2 py-1.5 rounded bg-bg border border-border/50">
    <div class="text-[10px] uppercase tracking-wider text-text-dim truncate">{props.label}</div>
    <div class="text-xs font-mono text-text truncate">{props.children}</div>
  </div>
);

/** Selector de opciones en una barra (modo de alineación, forma del parche) */
function Segmented<T extends string>(props: { value: T; options: { id: T; label: string; title?: string }[]; onChange: (v: T) => void }) {
  return (
    <div class="flex p-0.5 rounded-md bg-bg border border-border/60">
      <For each={props.options}>
        {(o) => (
          <button
            type="button"
            title={o.title}
            class={clsx(
              "flex-1 h-6 px-2 rounded text-[11px] truncate transition-colors",
              props.value === o.id ? "bg-accent/20 text-accent font-medium" : "text-text-muted hover:text-text hover:bg-surface/50"
            )}
            onClick={() => props.onChange(o.id)}
          >
            {o.label}
          </button>
        )}
      </For>
    </div>
  );
}

/** Botón chico para las acciones de una herramienta */
const MINI = "h-6 px-2 text-[11px] gap-1";

/**
 * Orizon3D al costado del visor (como los editores de pieles y de rig): una
 * pestaña por etapa del escaneo, con sus secciones repartidas en columnas
 * según el ancho. Captura → Nube (limpiar y seleccionar) → Malla.
 */
export const ScanEditor: Component<ScanEditorProps> = (props) => {
  const cloud = () => props.cloud;
  const [status, setStatus] = createSignal<ScannerStatus>();
  const [stored, setSettings] = createPersisted<ScanSettings>("scan.settings", DEFAULT_SETTINGS);
  // Los ajustes guardados antes de la calibración no traen sus campos
  const settings = (): ScanSettings => ({ ...DEFAULT_SETTINGS, ...stored() });
  const [storedClean, setClean] = createPersisted<CleanSettings>("scan.clean", DEFAULT_CLEAN);
  const clean = (): CleanSettings => ({ ...DEFAULT_CLEAN, ...storedClean() });
  const updateClean = (partial: Partial<CleanSettings>) => setClean({ ...clean(), ...partial });
  const [storedMesh, setMesh] = createPersisted<ScanMeshSettings>("scan.mesh", DEFAULT_MESH);
  // Los ajustes guardados antes del relleno de agujeros no traen sus campos
  const mesh = (): ScanMeshSettings => ({ ...DEFAULT_MESH, ...storedMesh() });
  const updateMesh = (partial: Partial<ScanMeshSettings>) => setMesh({ ...mesh(), ...partial });
  const [mergeAlign, setMergeAlign] = createPersisted<AlignMode>("scan.merge.align", "auto");
  const [mergeFuse, setMergeFuse] = createPersisted("scan.merge.fuse", true);
  /** Al empezar otro escaneo, el anterior se suma a la nube en vez de perderse */
  const [keepScans, setKeepScans] = createPersisted("scan.keep", true);
  /** El escaneo del escáner ya pasó a la nube (no hace falta sumarlo de nuevo) */
  const [scanKept, setScanKept] = createSignal(false);
  const [storedCompare, setCompareSettings] = createPersisted<CompareSettings>("scan.compare", DEFAULT_COMPARE);
  const compareSettings = (): CompareSettings => ({ ...DEFAULT_COMPARE, ...storedCompare() });
  const updateCompare = (partial: Partial<CompareSettings>) => setCompareSettings({ ...compareSettings(), ...partial });
  const [gain, setGain] = createPersisted("scan.gain", 1);
  const [view, setView] = createPersisted<"depth" | "color">("scan.view", "depth");
  const [hasFrame, setHasFrame] = createSignal(false);
  const [realDistance, setRealDistance] = createPersisted("scan.calib.distance", 300);
  const [realWidth, setRealWidth] = createPersisted("scan.calib.width", CARD_WIDTH_MM);
  const [measured, setMeasured] = createSignal<Measurement>();
  const [measureError, setMeasureError] = createSignal<string>();
  const [lastRecording, setLastRecording] = createSignal<Recording>();
  const [recordError, setRecordError] = createSignal<string>();
  /** Último fallo de un comando del escáner (conectar, escanear, ajustes) */
  const [deviceError, setDeviceError] = createSignal<string>();
  let canvas: HTMLCanvasElement | undefined;
  let timer: number | undefined;
  let alive = true;

  const connected = () => {
    const s = status()?.state;
    return s === "connecting" || s === "streaming";
  };
  const streaming = () => status()?.state === "streaming";

  const drawPreview = async () => {
    const buf = await invoke<ArrayBuffer>("scanner_preview", { kind: view() });
    const [w, h] = new Uint32Array(buf, 0, 2);
    if (!canvas || !w || !h) return;
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
    }
    canvas.getContext("2d")?.putImageData(new ImageData(new Uint8ClampedArray(buf, 8, w * h * 4), w, h), 0, 0);
    setHasFrame(true);
  };

  // Una petición a la vez: la siguiente sale cuando termina la anterior. La
  // vista previa solo se pide con la pestaña Captura a la vista
  const poll = async () => {
    timer = undefined;
    try {
      const s = await invoke<ScannerStatus>("scanner_status");
      if (!alive) return;
      setStatus(s);
      if (s.state === "streaming" && props.tab === "capture") await drawPreview();
    } catch (e) {
      console.error("Escáner:", e);
    }
    if (alive && connected()) timer = window.setTimeout(poll, POLL_MS);
  };

  onMount(poll);
  createEffect(() => cloud().setPointSize(clean().point_px));
  createEffect(() => cloud().setBrushSize(clean().brush_px));
  onCleanup(() => {
    alive = false;
    if (timer !== undefined) window.clearTimeout(timer);
  });

  /** Corre un comando del escáner y deja el error a la vista en vez de perderlo */
  const attempt = async (what: string, run: () => Promise<unknown>) => {
    try {
      await run();
      setDeviceError(undefined);
    } catch (e) {
      console.error(`Escáner (${what}):`, e);
      setDeviceError(`No se pudo ${what}: ${e}`);
    }
  };

  const connect = () =>
    attempt("conectar", async () => {
      setHasFrame(false);
      setStatus(await invoke<ScannerStatus>("scanner_connect", { settings: settings() }));
      await invoke("scanner_set_gain", { gain: gain() });
      if (timer === undefined) poll();
    });

  const disconnect = () =>
    attempt("desconectar", async () => {
      await invoke("scanner_disconnect");
      setHasFrame(false);
      await poll();
    });

  const updateSettings = (partial: Partial<ScanSettings>) => {
    const next = { ...settings(), ...partial };
    if (next.clip_max_mm < next.clip_min_mm + 10) next.clip_max_mm = next.clip_min_mm + 10;
    setSettings(next);
    attempt("aplicar los ajustes", () => invoke("scanner_set_settings", { settings: next }));
  };

  const scan = (action: "start" | "stop" | "reset") =>
    attempt(action === "start" ? "iniciar el escaneo" : action === "stop" ? "detener el escaneo" : "reiniciar el escaneo", async () => {
      await invoke("scanner_scan", { action });
      if (action !== "stop") setScanKept(false);
      setStatus(await invoke<ScannerStatus>("scanner_status"));
    });

  /** Escaneo sin pasar a la nube que se perdería al empezar otro */
  const scanPending = () => !!status()?.points && !scanKept();

  /** Suma el escaneo actual a la nube, alineado sobre lo que ya hay */
  const mergeScan = async () => {
    const ok = await cloud().merge("scan", mergeAlign(), mergeFuse());
    if (ok) setScanKept(true);
    return !!ok;
  };

  /** Empieza otro escaneo; si se conservan, el anterior pasa antes a la nube */
  const startScan = async () => {
    if (keepScans() && scanPending() && !(await mergeScan())) return;
    await scan("start");
  };

  const measure = async () => {
    try {
      setMeasured(await invoke<Measurement>("scanner_measure"));
      setMeasureError(undefined);
    } catch (e) {
      setMeasured(undefined);
      setMeasureError(String(e));
    }
  };

  /** La distancia escala con los mm por unidad de profundidad */
  const calibrateDepth = () => {
    const now = status()!.distance_cm * 10;
    if (now <= 0 || realDistance() <= 0) return;
    updateSettings({ depth_scale: (settings().depth_scale * realDistance()) / now });
    setMeasured(undefined);
  };

  /** El ancho escala con 1 / focal: se corrige la focal en la proporción medida */
  const calibrateWidth = async () => {
    const m = measured();
    if (!m || m.width_mm <= 0 || realWidth() <= 0) return;
    updateSettings({ fx_scale: (settings().fx_scale * m.width_mm) / realWidth() });
    // Deja que llegue un cuadro con la focal nueva antes de volver a medir
    await new Promise((r) => setTimeout(r, 300));
    await measure();
  };

  const record = async (action: "start" | "stop") => {
    try {
      const rec = await invoke<Recording>("scanner_record", { action });
      setLastRecording(action === "stop" ? rec : undefined);
      setRecordError(undefined);
    } catch (e) {
      setRecordError(String(e));
    }
    await attempt("leer el estado", async () => setStatus(await invoke<ScannerStatus>("scanner_status")));
  };

  const resetCalibration = () => {
    updateSettings({ fx_scale: DEFAULT_SETTINGS.fx_scale, depth_scale: DEFAULT_SETTINGS.depth_scale });
    setMeasured(undefined);
  };

  /** Toma la nube del escáner y pasa a limpiarla */
  const takeCloud = async () => {
    await cloud().take();
    if (cloud().info()) {
      setScanKept(true);
      props.onTab("cloud");
    }
  };

  const mergePly = async () => {
    const path = await open({ title: "Sumar nube de puntos", multiple: false, filters: [{ name: "Nube de puntos PLY", extensions: ["ply"] }] });
    if (!path || Array.isArray(path)) return;
    await cloud().merge("ply", mergeAlign(), mergeFuse(), path);
  };

  const referencePly = async () => {
    const path = await open({ title: "Nube de referencia", multiple: false, filters: [{ name: "Nube de puntos PLY", extensions: ["ply"] }] });
    if (!path || Array.isArray(path)) return;
    await cloud().setReference("ply", path);
  };

  const importCloud = async () => {
    const path = await open({ title: "Abrir nube de puntos", multiple: false, filters: [{ name: "Nube de puntos PLY", extensions: ["ply"] }] });
    if (!path || Array.isArray(path)) return;
    await cloud().importPly(path);
  };

  const exportCloud = async () => {
    const path = await save({ title: "Guardar nube de puntos", defaultPath: "nube.ply", filters: [{ name: "Nube de puntos PLY", extensions: ["ply"] }] });
    if (!path) return;
    await cloud().exportPly(path);
  };

  const info = () => cloud().info();
  const busy = () => !!cloud().busy();
  const fromCloud = () => !!info();

  /** "Ver" marca en el visor lo que se quitaría; "Quitar" lo quita */
  const PreviewActions = (p: { label: string; op: () => Parameters<ScanCloud["edit"]>[0] }) => (
    <div class="flex gap-1">
      <Button
        size="sm"
        variant="ghost"
        class={MINI}
        disabled={busy()}
        title="Marcar en el visor lo que se quitaría"
        icon={<Icons.Eye size={12} />}
        onClick={() => cloud().edit(p.op(), p.label, true)}
      >
        Ver
      </Button>
      <Button
        size="sm"
        class={MINI}
        disabled={busy()}
        loading={cloud().busy() === p.label}
        icon={<Icons.Trash size={12} />}
        onClick={() => cloud().edit(p.op(), p.label)}
      >
        Quitar
      </Button>
    </div>
  );

  /** Acción única de una herramienta (simplificar, suavizar) */
  const ApplyAction = (p: { label: string; run: () => void }) => (
    <Button size="sm" class={MINI} disabled={busy()} loading={cloud().busy() === p.label} icon={<Icons.Check size={12} />} onClick={p.run}>
      Aplicar
    </Button>
  );

  // ═════════════════════════════════════════════════════════════════════════
  // CAPTURA
  // ═════════════════════════════════════════════════════════════════════════

  const capturePanels = () => (
    <div>
      <Panel
        id="scan.device"
        dense
        title="Escáner"
        icon={<Icons.Scan size={14} />}
        defaultOpen
        headerActions={<Help text="Revopoint POP 2 / POP 3 por USB. Sin escáner, la pestaña Nube abre nubes PLY." />}
      >
        <div class="flex items-center gap-2 text-xs">
          <span
            class={clsx(
              "w-2 h-2 rounded-full shrink-0",
              streaming() ? "bg-success" : status()?.state === "error" ? "bg-error" : connected() ? "bg-warning" : "bg-text-dim"
            )}
          />
          <span class="text-text truncate">{STATE_LABEL[status()?.state ?? "off"]}</span>
          <Show when={streaming()}>
            <span class="font-mono text-text-muted">{status()!.fps.toFixed(0)} FPS</span>
          </Show>
          <div class="flex-1" />
          <Show
            when={connected()}
            fallback={
              <Button variant="primary" size="sm" class={MINI} icon={<Icons.Lightning size={12} />} onClick={connect}>
                {status()?.state === "error" || status()?.state === "stopped" ? "Reintentar" : "Conectar"}
              </Button>
            }
          >
            <Button size="sm" class={MINI} icon={<Icons.X size={12} />} onClick={disconnect}>
              Desconectar
            </Button>
          </Show>
        </div>

        <Show when={status()?.state === "error" && status()?.message}>
          <p class="text-xs text-error leading-relaxed">{status()!.message}</p>
          <Hint>
            Revisa el cable USB y que tu usuario pueda leer <span class="font-mono">/dev/video*</span> (grupo{" "}
            <span class="font-mono">video</span> o reglas udev).
          </Hint>
        </Show>

        <Show when={deviceError()}>
          <p class="text-xs text-error leading-relaxed">{deviceError()}</p>
        </Show>

        <Show when={status()?.device}>
          <div class="space-y-1">
            <Row label="Dispositivo">{status()!.device}</Row>
            <Show when={status()!.depth_stream}>
              <Row label="Profundidad">{status()!.depth_stream}</Row>
            </Show>
            <Row label="Color">{status()!.color_stream ?? "sin color"}</Row>
          </div>
        </Show>
      </Panel>

      <Show when={connected()}>
        <Panel
          id="scan.preview"
          dense
          title="Vista en vivo"
          icon={<Icons.Eye size={14} />}
          defaultOpen
          headerActions={<Help text="Ganancia: hasta 3 es lo recomendado; más ganancia ilumina pero mete ruido." />}
        >
          <div class="relative rounded-md overflow-hidden bg-bg border border-border">
            <canvas ref={canvas} class="block w-full h-auto" />
            <Show when={!hasFrame()}>
              <div class="absolute inset-0 flex items-center justify-center text-xs text-text-dim">Esperando cuadros…</div>
            </Show>
            <div class="absolute top-1.5 left-1.5 flex p-0.5 rounded bg-bg/80 backdrop-blur-sm border border-border/60">
              {(["depth", "color"] as const).map((k) => (
                <button
                  class={clsx(
                    "px-2 h-5 rounded-sm text-[11px] transition-colors",
                    view() === k ? "bg-accent/25 text-accent font-medium" : "text-text-muted hover:text-text"
                  )}
                  onClick={() => setView(k)}
                >
                  {k === "depth" ? "Profundidad" : "Color"}
                </button>
              ))}
            </div>
          </div>
          <Show when={streaming()}>
            <div class="grid grid-cols-2 gap-1.5">
              <Stat label="Distancia">{status()!.distance_cm > 0 ? `${status()!.distance_cm.toFixed(0)} cm` : "—"}</Stat>
              <Stat label="Cobertura">{(status()!.coverage * 100).toFixed(0)} %</Stat>
            </div>
          </Show>
          <Slider
            inline
            label="Ganancia"
            value={gain()}
            min={1}
            max={16}
            step={1}
            showValue
            onChange={(v) => {
              setGain(v);
              attempt("cambiar la ganancia", () => invoke("scanner_set_gain", { gain: v }));
            }}
          />
        </Panel>

        <Panel
          id="scan.scan"
          dense
          title="Escaneo"
          icon={<Icons.ArrowsClockwise size={14} />}
          defaultOpen
          headerActions={
            <Help text="Gira el objeto (o el escáner) despacio: cada cuadro se alinea con lo escaneado y se suma. Sin escaneo, la nube sale del cuadro actual." />
          }
        >
          <div class="flex gap-1.5">
            <Show
              when={status()?.scanning}
              fallback={
                <Button
                  size="sm"
                  class="flex-1"
                  disabled={!streaming() || busy()}
                  icon={<Icons.Play size={14} />}
                  title={
                    keepScans() && scanPending()
                      ? "El escaneo actual se suma a la nube y empieza otro"
                      : status()?.points
                        ? "Empieza otro escaneo (el actual se descarta)"
                        : undefined
                  }
                  onClick={startScan}
                >
                  {!status()?.points ? "Escanear" : keepScans() ? "Escanear otro" : "Escanear de nuevo"}
                </Button>
              }
            >
              <Button size="sm" class="flex-1" icon={<Icons.Pause size={14} />} onClick={() => scan("stop")}>
                Pausar
              </Button>
            </Show>
            <ToolButton
              label="Descartar el escaneo"
              danger
              disabled={!status()?.points && !status()?.scanning}
              icon={<Icons.Trash size={14} />}
              onClick={() => scan("reset")}
            />
          </div>
          <Show when={status()?.frames}>
            <div class="grid grid-cols-3 gap-1.5">
              <Stat label="Seguimiento">
                <span class={status()!.tracking_ok ? "text-success" : "text-warning"}>{status()!.tracking_ok ? "alineado" : "perdido"}</span>
              </Stat>
              <Stat label="Cuadros">
                {status()!.registered} / {status()!.dropped}
              </Stat>
              <Stat label="Puntos">{status()!.points.toLocaleString()}</Stat>
            </div>
            <Show when={!status()!.tracking_ok}>
              <Hint>Seguimiento perdido: vuelve a una zona ya escaneada.</Hint>
            </Show>
          </Show>
          <Show
            when={info()}
            fallback={
              <Button
                variant="primary"
                size="sm"
                fullWidth
                disabled={!streaming() || busy() || status()?.scanning}
                loading={cloud().busy() === "Tomar la nube"}
                icon={<Icons.MagicWand size={14} />}
                title="Pasa la nube a la pestaña Nube para quitar ruido, fragmentos y la mesa antes de crear el modelo"
                onClick={takeCloud}
              >
                {status()?.points ? "Limpiar la nube del escaneo" : "Limpiar la nube del cuadro actual"}
              </Button>
            }
          >
            <div class="flex gap-1.5">
              <Button
                variant="primary"
                size="sm"
                class="flex-1"
                disabled={!streaming() || busy() || status()?.scanning || scanKept()}
                loading={cloud().busy() === "Sumar el escaneo"}
                icon={<Icons.Stack size={14} />}
                title="Suma este escaneo a la nube en edición, alineado sobre lo que ya hay"
                onClick={mergeScan}
              >
                {scanKept() ? "Ya está en la nube" : "Sumar a la nube"}
              </Button>
              <ToolButton
                label="Reemplazar la nube con este escaneo"
                disabled={!streaming() || busy() || status()?.scanning}
                icon={<Icons.ArrowsClockwise size={14} />}
                onClick={takeCloud}
              />
            </div>
          </Show>
          <div class="flex items-center gap-1.5">
            <Checkbox small label="Conservar cada escaneo" checked={keepScans()} onChange={setKeepScans} />
            <Help text="Al empezar otro escaneo, el anterior se suma solo a la nube (alineado) en vez de perderse. Sirve para escanear el objeto por partes: de pie, dado vuelta, de costado." />
          </div>
        </Panel>

        <Panel
          id="scan.volume"
          dense
          title="Volumen de escaneo"
          icon={<Icons.Ruler size={14} />}
          defaultOpen={false}
          headerActions={<Help text="Estos filtros actúan al capturar; los de la pestaña Nube, sobre la nube ya tomada." />}
        >
          <div class="space-y-1">
            <Slider
              inline
              label="Cerca"
              value={settings().clip_min_mm}
              min={50}
              max={1500}
              step={10}
              showValue
              formatValue={(v) => `${(v / 10).toFixed(0)} cm`}
              onChange={(v) => updateSettings({ clip_min_mm: v })}
            />
            <Slider
              inline
              label="Lejos"
              value={settings().clip_max_mm}
              min={60}
              max={1500}
              step={10}
              showValue
              formatValue={(v) => `${(v / 10).toFixed(0)} cm`}
              onChange={(v) => updateSettings({ clip_max_mm: v })}
            />
            <Slider
              inline
              label="Ancho"
              title="Ancho a los lados"
              value={settings().box_mm}
              min={0}
              max={800}
              step={10}
              showValue
              formatValue={(v) => (v === 0 ? "libre" : `${(v / 10).toFixed(0)} cm`)}
              onChange={(v) => updateSettings({ box_mm: v })}
            />
            <Slider
              inline
              label="Suavizado"
              title="Suavizado temporal: promedia varios cuadros seguidos"
              value={settings().temporal_frames}
              min={1}
              max={6}
              step={1}
              showValue
              formatValue={(v) => (v === 1 ? "no" : `${v} cuadros`)}
              onChange={(v) => updateSettings({ temporal_frames: v })}
            />
          </div>
          <div class="flex flex-col gap-1.5 pt-2 border-t border-border/40">
            <Checkbox small label="Quitar puntos sueltos" checked={settings().clean_noise} onChange={(v) => updateSettings({ clean_noise: v })} />
            <Checkbox
              small
              label="Aislar el objeto (grupo más grande)"
              checked={settings().isolate_object}
              onChange={(v) => updateSettings({ isolate_object: v })}
            />
            <Checkbox small label="Quitar bordes voladores" checked={settings().edge_filter} onChange={(v) => updateSettings({ edge_filter: v })} />
          </div>
        </Panel>
      </Show>
    </div>
  );

  // ═════════════════════════════════════════════════════════════════════════
  // NUBE
  // ═════════════════════════════════════════════════════════════════════════

  const cloudPanels = () => (
    <div>
      <Panel id="scan.cloud.source" dense title="Nube de puntos" icon={<Icons.Cube size={14} />} defaultOpen>
        <Show
          when={info()}
          fallback={
            <>
              <Hint>Toma la nube del escaneo (o del cuadro actual) para limpiarla antes de crear el modelo, o abre una nube PLY.</Hint>
              <div class="flex gap-1.5">
                <Button
                  variant="primary"
                  size="sm"
                  class="flex-1"
                  disabled={!streaming() || busy()}
                  loading={cloud().busy() === "Tomar la nube"}
                  icon={<Icons.Scan size={14} />}
                  onClick={takeCloud}
                >
                  Tomar del escáner
                </Button>
                <Button size="sm" disabled={busy()} icon={<Icons.FolderOpen size={14} />} onClick={importCloud}>
                  Abrir PLY
                </Button>
              </div>
            </>
          }
        >
          {(current) => (
            <>
              <div class="flex items-center gap-1.5">
                <ToolGroup>
                  <ToolButton
                    label={current().undo ? `Deshacer: ${current().undo} (Ctrl+Z)` : "Nada que deshacer"}
                    disabled={!current().undo || busy()}
                    icon={<Icons.ArrowCounterClockwise size={14} />}
                    onClick={() => cloud().history(false)}
                  />
                  <ToolButton
                    label={current().redo ? `Rehacer: ${current().redo} (Ctrl+Shift+Z)` : "Nada que rehacer"}
                    disabled={!current().redo || busy()}
                    icon={<Icons.ArrowClockwise size={14} />}
                    onClick={() => cloud().history(true)}
                  />
                </ToolGroup>
                <ToolGroup>
                  <ToolButton label="Abrir nube PLY" disabled={busy()} icon={<Icons.FolderOpen size={14} />} onClick={importCloud} />
                  <ToolButton label="Guardar nube PLY" disabled={busy()} icon={<Icons.FloppyDisk size={14} />} onClick={exportCloud} />
                  <Show when={streaming()}>
                    <ToolButton label="Volver a tomar del escáner" disabled={busy()} icon={<Icons.Scan size={14} />} onClick={takeCloud} />
                  </Show>
                </ToolGroup>
                <div class="flex-1" />
                <ToolGroup>
                  <ToolButton
                    label={cloud().shown() ? "Viendo la nube: clic para ver el modelo" : "Viendo el modelo: clic para ver la nube"}
                    active={cloud().shown()}
                    icon={cloud().shown() ? <Icons.Eye size={14} /> : <Icons.EyeSlash size={14} />}
                    onClick={() => cloud().setShown(!cloud().shown())}
                  />
                  <ToolButton label="Cerrar la nube" danger disabled={busy()} icon={<Icons.X size={14} />} onClick={() => cloud().discard()} />
                </ToolGroup>
              </div>
              <div class="grid grid-cols-3 gap-1.5">
                <Stat label="Puntos">{current().points.toLocaleString()}</Stat>
                <Stat label="Separación">{current().spacing_mm > 0 ? `${current().spacing_mm.toFixed(2)} mm` : "—"}</Stat>
                <Stat label="Origen">
                  <span title={current().source}>{current().source}</span>
                </Stat>
              </div>
              <Slider
                inline
                label="Tamaño punto"
                title="Tamaño de los puntos en el visor"
                value={clean().point_px}
                min={1}
                max={8}
                step={0.5}
                showValue
                formatValue={(v) => `${v} px`}
                onChange={(v) => updateClean({ point_px: v })}
              />
            </>
          )}
        </Show>
      </Panel>

      <Show when={info()}>
        <Panel
          id="scan.cloud.select"
          dense
          title="Selección"
          icon={<Icons.Cursor size={14} />}
          defaultOpen
          headerActions={
            <Help
              text={
                (cloud().tool() === "polygon"
                  ? "Clic a clic marca los vértices de la cuerda; se cierra con clic en el primer punto, doble clic o Enter. Retroceso quita el último punto y Esc cancela. "
                  : "Arrastra con el botón izquierdo sobre el visor. ") +
                "Shift suma, Ctrl resta. La vista se gira con el botón central o Alt + izquierdo. La selección atraviesa la nube: gira la vista para no tomar la cara de atrás."
              }
            />
          }
        >
          <div class="flex items-center gap-1.5">
            <ToolGroup>
              <For each={SELECT_TOOLS}>
                {(t) => (
                  <ToolButton label={t.label} active={cloud().tool() === t.id} icon={<t.icon size={15} />} onClick={() => cloud().setTool(t.id)} />
                )}
              </For>
            </ToolGroup>
            <div class="flex-1" />
            <ToolGroup>
              <ToolButton label="Seleccionar todo (A)" icon={<Icons.CheckSquare size={14} />} onClick={() => cloud().select("all")} />
              <ToolButton label="No seleccionar nada (Alt+A)" icon={<Icons.Square size={14} />} onClick={() => cloud().select("none")} />
              <ToolButton label="Invertir la selección (Ctrl+I)" icon={<Icons.ArrowsLeftRight size={14} />} onClick={() => cloud().select("invert")} />
            </ToolGroup>
          </div>
          <Show when={cloud().tool() === "brush"}>
            <Slider
              inline
              label="Pincel"
              title="Tamaño del pincel"
              value={clean().brush_px}
              min={5}
              max={150}
              step={1}
              showValue
              formatValue={(v) => `${v} px`}
              onChange={(v) => updateClean({ brush_px: v })}
            />
          </Show>
          <div class="flex items-center gap-1.5">
            <span class="text-xs text-text-muted">
              <span class="font-mono text-text">{cloud().selected().toLocaleString()}</span> seleccionados
            </span>
            <div class="flex-1" />
            <Button
              size="sm"
              variant="danger"
              class={MINI}
              disabled={cloud().selected() === 0 || busy()}
              title="Borrar los puntos seleccionados (Supr)"
              icon={<Icons.Trash size={12} />}
              onClick={() => cloud().deleteSelection()}
            >
              Borrar
            </Button>
            <Button
              size="sm"
              class={MINI}
              disabled={cloud().selected() === 0 || busy()}
              title="Conservar solo los puntos seleccionados"
              icon={<Icons.Scissors size={12} />}
              onClick={() => cloud().keepSelection()}
            >
              Recortar
            </Button>
          </div>
        </Panel>

        <Panel id="scan.cloud.noise" dense title="Limpieza" icon={<Icons.MagicWand size={14} />} defaultOpen>
          <Tool
            first
            title="Ruido"
            help="Estadístico: quita la bruma alrededor de la superficie, los puntos más lejos de sus vecinos que el resto."
            actions={
              <PreviewActions
                label="Quitar ruido"
                op={() => ({ kind: "statistical", neighbors: clean().sor_neighbors, std_ratio: clean().sor_ratio })}
              />
            }
          >
            <Slider inline label="Vecinos" value={clean().sor_neighbors} min={4} max={50} step={1} showValue onChange={(v) => updateClean({ sor_neighbors: v })} />
            <Slider
              inline
              label="Tolerancia"
              value={clean().sor_ratio}
              min={0.5}
              max={4}
              step={0.1}
              showValue
              formatValue={(v) => `${v.toFixed(1)} σ`}
              onChange={(v) => updateClean({ sor_ratio: v })}
            />
          </Tool>
          <Tool
            title="Puntos sueltos"
            help="Quita los puntos con pocos vecinos cerca: motas y restos aislados."
            actions={
              <PreviewActions
                label="Quitar puntos sueltos"
                op={() => ({ kind: "radius", radius_mm: clean().radius_mm, min_neighbors: clean().radius_neighbors })}
              />
            }
          >
            <Slider
              inline
              label="Radio"
              value={clean().radius_mm}
              min={0.5}
              max={15}
              step={0.5}
              showValue
              formatValue={(v) => `${v.toFixed(1)} mm`}
              onChange={(v) => updateClean({ radius_mm: v })}
            />
            <Slider
              inline
              label="Vecinos mín."
              title="Vecinos mínimos dentro del radio"
              value={clean().radius_neighbors}
              min={1}
              max={40}
              step={1}
              showValue
              onChange={(v) => updateClean({ radius_neighbors: v })}
            />
          </Tool>
          <Tool
            title="Fragmentos"
            help="Partes separadas del objeto (manos, soportes, restos del fondo). Dos puntos más lejos que la separación quedan en fragmentos distintos."
            actions={
              <>
                <ToolButton
                  label="Quedarse con la pieza más grande"
                  disabled={busy()}
                  icon={<Icons.CubeFocus size={14} />}
                  onClick={() => cloud().edit({ kind: "fragments", gap_mm: clean().gap_mm, min_percent: 100 }, "Quitar fragmentos")}
                />
                <PreviewActions label="Quitar fragmentos" op={() => ({ kind: "fragments", gap_mm: clean().gap_mm, min_percent: clean().min_percent })} />
              </>
            }
          >
            <Slider
              inline
              label="Separación"
              value={clean().gap_mm}
              min={1}
              max={30}
              step={0.5}
              showValue
              formatValue={(v) => `${v.toFixed(1)} mm`}
              onChange={(v) => updateClean({ gap_mm: v })}
            />
            <Slider
              inline
              label="Tamaño mín."
              title="Tamaño mínimo, en % del fragmento más grande (100 % deja solo el más grande)"
              value={clean().min_percent}
              min={1}
              max={100}
              step={1}
              showValue
              formatValue={(v) => (v >= 100 ? "solo mayor" : `${v} %`)}
              onChange={(v) => updateClean({ min_percent: v })}
            />
          </Tool>
          <Tool
            title="Mesa o base"
            help="Busca el plano más grande (la mesa, el plato giratorio) y lo quita."
            actions={<PreviewActions label="Quitar la mesa" op={() => ({ kind: "plane", threshold_mm: clean().plane_mm, below: clean().plane_below })} />}
          >
            <Slider
              inline
              label="Grosor"
              title="Grosor del plano"
              value={clean().plane_mm}
              min={0.5}
              max={10}
              step={0.5}
              showValue
              formatValue={(v) => `${v.toFixed(1)} mm`}
              onChange={(v) => updateClean({ plane_mm: v })}
            />
            <Checkbox small label="También lo que queda debajo" checked={clean().plane_below} onChange={(v) => updateClean({ plane_below: v })} />
          </Tool>
        </Panel>

        <Panel
          id="scan.cloud.merge"
          dense
          title="Fusionar nubes"
          icon={<Icons.Stack size={14} />}
          defaultOpen
          headerActions={
            <Help text="Suma otro escaneo o una nube PLY a la nube en edición sin borrar lo anterior. La automática encaja la nube nueva aunque el objeto se haya dado vuelta; cada suma se deshace con Ctrl+Z." />
          }
        >
          <div class="flex items-center gap-2">
            <span class="text-xs text-text-muted shrink-0">Alineación</span>
            <div class="flex-1 min-w-0">
              <Segmented value={mergeAlign()} options={ALIGN_MODES} onChange={setMergeAlign} />
            </div>
          </div>
          <div class="flex items-center gap-1.5">
            <Checkbox small label="Promediar el solape" checked={mergeFuse()} onChange={setMergeFuse} />
            <Help text="Donde las nubes se cubren, deja un punto por celda (el promedio) en vez de dos capas que ensucian la malla." />
            <div class="flex-1" />
            <span class="text-xs text-text-muted">
              <span class="font-mono text-text">{info()!.parts}</span> {info()!.parts === 1 ? "parte" : "partes"}
            </span>
          </div>
          <div class="flex gap-1.5">
            <Button
              size="sm"
              class="flex-1"
              disabled={!streaming() || busy() || status()?.scanning || scanKept()}
              loading={cloud().busy() === "Sumar el escaneo"}
              icon={<Icons.Scan size={14} />}
              title={scanKept() ? "El escaneo actual ya está en la nube: escanea otro en la pestaña Captura" : "Suma el escaneo actual del escáner"}
              onClick={mergeScan}
            >
              Sumar escaneo
            </Button>
            <Button
              size="sm"
              class="flex-1"
              disabled={busy()}
              loading={cloud().busy() === "Sumar la nube"}
              icon={<Icons.FolderOpen size={14} />}
              onClick={mergePly}
            >
              Sumar PLY
            </Button>
          </div>
        </Panel>

        <Panel id="scan.cloud.simplify" dense title="Simplificar y suavizar" icon={<Icons.GridFour size={14} />} defaultOpen={false}>
          <Tool
            first
            title="Simplificar"
            help="Deja un punto por celda: menos puntos, operaciones y malla más rápidas."
            actions={<ApplyAction label="Simplificar" run={() => cloud().edit({ kind: "downsample", voxel_mm: clean().voxel_mm }, "Simplificar")} />}
          >
            <Slider
              inline
              label="Celda"
              value={clean().voxel_mm}
              min={0.5}
              max={10}
              step={0.5}
              showValue
              formatValue={(v) => `${v.toFixed(1)} mm`}
              onChange={(v) => updateClean({ voxel_mm: v })}
            />
          </Tool>
          <Tool
            title="Suavizar"
            help="Lleva cada punto hacia la superficie que forman sus vecinos, sin encoger la forma."
            actions={
              <ApplyAction
                label="Suavizar"
                run={() => cloud().edit({ kind: "smooth", radius_mm: clean().smooth_mm, strength: clean().smooth_strength }, "Suavizar")}
              />
            }
          >
            <Slider
              inline
              label="Radio"
              value={clean().smooth_mm}
              min={0.5}
              max={10}
              step={0.5}
              showValue
              formatValue={(v) => `${v.toFixed(1)} mm`}
              onChange={(v) => updateClean({ smooth_mm: v })}
            />
            <Slider
              inline
              label="Intensidad"
              value={clean().smooth_strength}
              min={0.1}
              max={1}
              step={0.05}
              showValue
              formatValue={(v) => `${Math.round(v * 100)} %`}
              onChange={(v) => updateClean({ smooth_strength: v })}
            />
          </Tool>
        </Panel>

        <Panel
          id="scan.cloud.compare"
          dense
          title="Comparar"
          icon={<Icons.Compare size={14} />}
          defaultOpen={false}
          headerActions={
            <Help text="Mide cuánto se aparta cada punto de una nube de referencia (una copia antes de editar, otro escaneo o un modelo en PLY) y lo pinta: azul calza, rojo se aparta; gris no tiene pareja cerca." />
          }
        >
          <div class="flex items-center gap-1.5 min-h-7">
            <Show
              when={info()!.reference}
              fallback={<span class="text-xs text-text-dim flex-1">Sin referencia</span>}
            >
              {(ref) => (
                <span class="text-xs text-text truncate flex-1" title={ref().name}>
                  {ref().name} <span class="text-text-dim font-mono">· {ref().points.toLocaleString()}</span>
                </span>
              )}
            </Show>
            <ToolGroup>
              <ToolButton
                label="Usar la nube actual como referencia (una copia)"
                disabled={busy()}
                icon={<Icons.Copy size={14} />}
                onClick={() => cloud().setReference("current")}
              />
              <ToolButton label="Abrir una referencia PLY" disabled={busy()} icon={<Icons.FolderOpen size={14} />} onClick={referencePly} />
              <ToolButton
                label="Alinear la nube a la referencia"
                disabled={busy() || !info()!.reference}
                icon={<Icons.CornersIn size={14} />}
                onClick={() => cloud().alignToReference(mergeAlign() === "none" ? "fine" : mergeAlign())}
              />
              <ToolButton
                label="Quitar la referencia"
                danger
                disabled={busy() || !info()!.reference}
                icon={<Icons.X size={14} />}
                onClick={() => cloud().setReference("clear")}
              />
            </ToolGroup>
          </div>
          <div class="space-y-1">
            <Slider
              inline
              label="Tolerancia"
              title="Distancia que se considera que calza"
              value={compareSettings().tolerance_mm}
              min={0.1}
              max={5}
              step={0.1}
              showValue
              formatValue={(v) => `${v.toFixed(1)} mm`}
              onChange={(v) => updateCompare({ tolerance_mm: v })}
            />
            <Slider
              inline
              label="Escala de color"
              title="Distancia que se pinta de rojo"
              value={compareSettings().scale_mm}
              min={0.5}
              max={20}
              step={0.5}
              showValue
              formatValue={(v) => `${v.toFixed(1)} mm`}
              onChange={(v) => updateCompare({ scale_mm: v })}
            />
            <Slider
              inline
              label="Alcance"
              title="Más lejos que esto, el punto no tiene pareja (gris)"
              value={compareSettings().reach_mm}
              min={1}
              max={50}
              step={1}
              showValue
              formatValue={(v) => `${v} mm`}
              onChange={(v) => updateCompare({ reach_mm: v })}
            />
          </div>
          <div class="flex gap-1.5">
            <Button
              size="sm"
              variant={info()!.comparing ? "default" : "primary"}
              class="flex-1"
              disabled={busy() || !info()!.reference}
              loading={cloud().busy() === "Comparar"}
              icon={<Icons.Compare size={14} />}
              onClick={() => cloud().compare(compareSettings())}
            >
              {info()!.comparing ? "Volver a comparar" : "Comparar"}
            </Button>
            <Show when={info()!.comparing}>
              <Button size="sm" disabled={busy()} icon={<Icons.EyeSlash size={14} />} onClick={() => cloud().compare(compareSettings(), false)}>
                Colores reales
              </Button>
            </Show>
          </div>
          <Show when={cloud().comparison()}>
            {(c) => (
              <>
                <div class="space-y-1">
                  <div
                    class="h-2 rounded-full"
                    style={{ background: "linear-gradient(90deg, rgb(40,90,220), rgb(30,190,210), rgb(60,200,80), rgb(240,210,40), rgb(225,50,40))" }}
                  />
                  <div class="flex justify-between text-[10px] font-mono text-text-dim">
                    <span>0</span>
                    <span>{(compareSettings().scale_mm / 2).toFixed(1)}</span>
                    <span>≥ {compareSettings().scale_mm.toFixed(1)} mm</span>
                  </div>
                </div>
                <div class="grid grid-cols-3 gap-1.5">
                  <Stat label="Media">{c().mean_mm.toFixed(2)} mm</Stat>
                  <Stat label="RMS">{c().rms_mm.toFixed(2)} mm</Stat>
                  <Stat label="P95">{c().p95_mm.toFixed(2)} mm</Stat>
                  <Stat label="Máxima">{c().max_mm.toFixed(2)} mm</Stat>
                  <Stat label="En tolerancia">{c().within_percent.toFixed(1)} %</Stat>
                  <Stat label="Sin pareja">
                    {c().compared + c().unmatched > 0 ? ((100 * c().unmatched) / (c().compared + c().unmatched)).toFixed(1) : "0"} %
                  </Stat>
                </div>
              </>
            )}
          </Show>
        </Panel>
      </Show>
    </div>
  );

  // ═════════════════════════════════════════════════════════════════════════
  // MALLA
  // ═════════════════════════════════════════════════════════════════════════

  const meshPanels = () => (
    <div>
      <Panel
        id="scan.mesh"
        dense
        title="Crear modelo"
        icon={<Icons.Cube size={14} />}
        defaultOpen
        headerActions={<Help text="Reemplaza el modelo abierto. Queda en milímetros, apoyado en el piso." />}
      >
        <Hint>
          <Show
            when={info()}
            fallback={
              status()?.points
                ? "Desde el escaneo tal como está. Para limpiarlo antes, tómalo en la pestaña Nube."
                : "Desde el cuadro actual del escáner."
            }
          >
            {(current) => `Desde la nube en edición (${current().points.toLocaleString()} puntos, ${current().source}).`}
          </Show>
        </Hint>
        <div class="space-y-1">
          <Slider
            inline
            label="Detalle"
            value={mesh().voxel_mm}
            min={1}
            max={8}
            step={0.5}
            showValue
            formatValue={(v) => `${v.toFixed(1)} mm`}
            onChange={(v) => updateMesh({ voxel_mm: v })}
          />
          <Slider inline label="Suavizado" value={mesh().smooth} min={0} max={4} step={1} showValue onChange={(v) => updateMesh({ smooth: v })} />
          <Slider
            inline
            label="Cerrar huecos"
            title="Engrosa la superficie para cerrar los huecos chicos entre puntos (más = más liso)"
            value={mesh().fill}
            min={0}
            max={4}
            step={1}
            showValue
            onChange={(v) => updateMesh({ fill: v })}
          />
        </div>
        <Tool
          title="Tapar agujeros"
          help="Después de mallar, tapa los agujeros que quedan: también los grandes, como la base que el escáner no vio. El parche curvo sigue la forma del borde; el plano sirve para bases y cortes."
          actions={
            <Checkbox small label="Activo" checked={mesh().close_holes} onChange={(v) => updateMesh({ close_holes: v })} />
          }
        >
          <Show when={mesh().close_holes}>
            <Segmented
              value={mesh().flat_patch ? "flat" : "curved"}
              options={[
                { id: "curved", label: "Parche curvo", title: "Continúa la curvatura del borde (cabezas, figuras)" },
                { id: "flat", label: "Parche plano", title: "Tapa plana (la base, un corte recto)" },
              ]}
              onChange={(v) => updateMesh({ flat_patch: v === "flat" })}
            />
            <Slider
              inline
              label="Tamaño máx."
              title="Perímetro máximo del agujero a tapar; los más grandes quedan abiertos"
              value={mesh().max_hole_mm}
              min={0}
              max={1000}
              step={10}
              showValue
              formatValue={(v) => (v === 0 ? "todos" : `${v} mm`)}
              onChange={(v) => updateMesh({ max_hole_mm: v })}
            />
            <Checkbox small label="Quitar piezas sueltas antes" checked={mesh().remove_pieces} onChange={(v) => updateMesh({ remove_pieces: v })} />
          </Show>
        </Tool>
        <Button
          variant="primary"
          size="sm"
          fullWidth
          disabled={(!fromCloud() && !streaming()) || busy() || props.isProcessing}
          loading={props.isProcessing}
          icon={<Icons.Cube size={14} />}
          onClick={() => props.onCreateModel?.(mesh(), fromCloud())}
        >
          {fromCloud() ? "Crear modelo de la nube" : status()?.points ? "Crear modelo del escaneo" : "Crear modelo del cuadro actual"}
        </Button>
      </Panel>

      <Show when={props.hasModel}>
        <Panel
          id="scan.mesh.after"
          dense
          title="Después"
          icon={<Icons.Wrench size={14} />}
          defaultOpen
          headerActions={<Help text="La malla puede traer agujeros, piezas sueltas o caras sueltas. Reparar (en Preparar) los detecta y corrige." />}
        >
          <div class="flex items-center gap-1.5">
            <Button size="sm" class="flex-1" icon={<Icons.Wrench size={14} />} onClick={() => props.onRepair?.()}>
              Reparar la malla
            </Button>
            <Show when={info()}>
              <ToolGroup>
                <ToolButton
                  label={cloud().shown() ? "Viendo la nube: clic para ver el modelo" : "Viendo el modelo: clic para ver la nube"}
                  active={cloud().shown()}
                  icon={cloud().shown() ? <Icons.Eye size={14} /> : <Icons.EyeSlash size={14} />}
                  onClick={() => cloud().setShown(!cloud().shown())}
                />
              </ToolGroup>
            </Show>
          </div>
        </Panel>
      </Show>
    </div>
  );

  // ═════════════════════════════════════════════════════════════════════════
  // CALIBRACIÓN Y DIAGNÓSTICO
  // ═════════════════════════════════════════════════════════════════════════

  const setupPanels = () => (
    <div>
      <Panel id="scan.calibration" title="Calibración" icon={<Icons.Ruler size={14} />} defaultOpen>
        <div class="space-y-4 text-xs pb-3">
          <p class="text-text-muted leading-relaxed">
            Se hace una vez por escáner y queda guardada. Primero la profundidad y después el ancho, porque el ancho medido
            depende de la distancia.
          </p>

          <div class="space-y-2">
            <p class="text-text font-medium">1. Profundidad</p>
            <p class="text-text-muted leading-relaxed">
              Pon una superficie plana (un libro, una caja) de frente al escáner, al centro de la imagen, y mide con una regla
              desde el frente del escáner hasta ella.
            </p>
            <Row label="El escáner mide">
              {streaming() && status()!.distance_cm > 0 ? `${(status()!.distance_cm * 10).toFixed(0)} mm` : "—"}
            </Row>
            <div class="flex items-center gap-2">
              <span class="text-text-muted shrink-0">Distancia real</span>
              <NumberInput value={realDistance()} min={50} max={1500} step={1} suffix="mm" onChange={setRealDistance} />
            </div>
            <Button size="sm" fullWidth disabled={!streaming() || !(status()!.distance_cm > 0)} onClick={calibrateDepth}>
              Ajustar profundidad
            </Button>
          </div>

          <div class="space-y-2">
            <p class="text-text font-medium">2. Ancho</p>
            <p class="text-text-muted leading-relaxed">
              Sostén de frente un objeto plano de ancho conocido, sin nada más dentro del volumen de escaneo. Una tarjeta
              bancaria mide {CARD_WIDTH_MM} mm.
            </p>
            <Button size="sm" fullWidth disabled={!streaming()} icon={<Icons.Ruler size={14} />} onClick={measure}>
              Medir objeto
            </Button>
            <Show when={measureError()}>
              <p class="text-error leading-relaxed">{measureError()}</p>
            </Show>
            <Show when={measured()}>
              <Row label="Ancho × alto medidos">
                {measured()!.width_mm.toFixed(1)} × {measured()!.height_mm.toFixed(1)} mm
              </Row>
            </Show>
            <div class="flex items-center gap-2">
              <span class="text-text-muted shrink-0">Ancho real</span>
              <NumberInput value={realWidth()} min={10} max={800} step={0.1} suffix="mm" onChange={setRealWidth} />
            </div>
            <Button size="sm" fullWidth disabled={!streaming() || !measured()} onClick={calibrateWidth}>
              Ajustar ancho
            </Button>
          </div>

          <div class="flex items-center justify-between gap-2 pt-1 border-t border-border">
            <span class="text-text-dim font-mono">
              focal ×{settings().fx_scale.toFixed(3)} · {settings().depth_scale.toFixed(4)} mm/u
            </span>
            <Button size="sm" variant="ghost" onClick={resetCalibration}>
              Restablecer
            </Button>
          </div>
        </div>
      </Panel>

      <Panel id="scan.record" title="Grabar para diagnóstico" icon={<Icons.Record size={14} />} defaultOpen={false}>
        <div class="space-y-3 text-xs pb-3">
          <p class="text-text-muted leading-relaxed">
            Guarda los cuadros de profundidad tal como llegan del escáner, para reproducir el escaneo sin el escáner y revisar
            dónde falla. Graba unos 10 a 20 segundos girando el objeto como lo harías al escanear.
          </p>
          <Show
            when={status()?.recorded != null}
            fallback={
              <Button size="sm" fullWidth disabled={!streaming()} icon={<Icons.Record size={14} />} onClick={() => record("start")}>
                Grabar cuadros
              </Button>
            }
          >
            <Button size="sm" fullWidth icon={<Icons.Stop size={14} />} onClick={() => record("stop")}>
              Detener ({status()!.recorded} cuadros)
            </Button>
          </Show>
          <Show when={recordError()}>
            <p class="text-error leading-relaxed">{recordError()}</p>
          </Show>
          <Show when={lastRecording()}>
            <p class="text-text-muted leading-relaxed">
              {lastRecording()!.frames} cuadros en{" "}
              <span class="font-mono text-text break-all select-text">{lastRecording()!.path}</span>
            </p>
          </Show>
        </div>
      </Panel>
    </div>
  );

  return (
    <section class="flex flex-col h-full min-w-0 bg-bg-darker">
      <header class="flex items-center gap-1 h-9 px-2 shrink-0 border-b border-border">
        <span class="px-1.5 text-xs font-semibold text-text">Orizon3D</span>
        <For each={TABS}>
          {(t) => (
            <button
              title={t.title}
              class={clsx(
                "px-2.5 h-7 rounded text-xs transition-colors",
                props.tab === t.id ? "bg-accent/20 text-accent" : "text-text-muted hover:bg-surface/50 hover:text-text"
              )}
              onClick={() => props.onTab(t.id)}
            >
              {t.label}
            </button>
          )}
        </For>
        <div class="flex-1" />
        <Show when={cloud().busy()}>
          <span class="text-[11px] text-text-muted truncate">{cloud().busy()}…</span>
        </Show>
        <IconButton size="sm" variant="ghost" aria-label="Cerrar el panel" title="Cerrar el panel" onClick={props.onClose}>
          <Icons.X size={14} />
        </IconButton>
      </header>
      <div class="flex-1 min-h-0 overflow-y-auto px-3 py-2">
        {/* Las secciones fluyen en columnas de ~300 px sin partirse */}
        <div class="[column-width:300px] [column-gap:1rem] [&>div>div]:break-inside-avoid [&>div>div]:mb-3">
          <Switch>
            <Match when={props.tab === "capture"}>{capturePanels()}</Match>
            <Match when={props.tab === "cloud"}>{cloudPanels()}</Match>
            <Match when={props.tab === "mesh"}>{meshPanels()}</Match>
            <Match when={props.tab === "setup"}>{setupPanels()}</Match>
          </Switch>
        </div>
      </div>
    </section>
  );
};
