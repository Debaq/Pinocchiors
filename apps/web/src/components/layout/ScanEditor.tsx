import { Component, For, Match, Show, Switch, createEffect, createSignal, onCleanup, onMount, type JSX } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { clsx } from "clsx";
import { Panel, Slider, Checkbox, Button, IconButton, NumberInput } from "../ui";
import { createPersisted } from "../../lib/ui-state";
import type { ScanCloud } from "../../lib/scanCloud";
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

const DEFAULT_MESH: ScanMeshSettings = { voxel_mm: 2, fill: 1, smooth: 2 };

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
  const [mesh, setMesh] = createPersisted<ScanMeshSettings>("scan.mesh", DEFAULT_MESH);
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
      setStatus(await invoke<ScannerStatus>("scanner_status"));
    });

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
    if (cloud().info()) props.onTab("cloud");
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
    <div class="flex gap-2">
      <Button size="sm" variant="ghost" class="flex-1" disabled={busy()} icon={<Icons.Eye size={14} />} onClick={() => cloud().edit(p.op(), p.label, true)}>
        Ver
      </Button>
      <Button size="sm" class="flex-1" disabled={busy()} loading={cloud().busy() === p.label} icon={<Icons.Trash size={14} />} onClick={() => cloud().edit(p.op(), p.label)}>
        Quitar
      </Button>
    </div>
  );

  // ═════════════════════════════════════════════════════════════════════════
  // CAPTURA
  // ═════════════════════════════════════════════════════════════════════════

  const capturePanels = () => (
    <div>
      <Panel id="scan.device" title="Escáner" icon={<Icons.Scan size={14} />} defaultOpen>
        <div class="space-y-3 pb-3">
          <div class="flex items-center gap-2 text-xs">
            <span
              class={clsx(
                "w-2 h-2 rounded-full shrink-0",
                streaming() ? "bg-success" : status()?.state === "error" ? "bg-error" : connected() ? "bg-warning" : "bg-text-dim"
              )}
            />
            <span class="text-text">{STATE_LABEL[status()?.state ?? "off"]}</span>
            <Show when={streaming()}>
              <span class="ml-auto font-mono text-text-muted">{status()!.fps.toFixed(0)} FPS</span>
            </Show>
          </div>

          <Show when={status()?.state === "error" && status()?.message}>
            <p class="text-xs text-error leading-relaxed">{status()!.message}</p>
            <p class="text-xs text-text-muted leading-relaxed">
              Revisa el cable USB y que tu usuario pueda leer <span class="font-mono">/dev/video*</span> (grupo{" "}
              <span class="font-mono">video</span> o reglas udev).
            </p>
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

          <Show
            when={connected()}
            fallback={
              <Button variant="primary" size="sm" fullWidth icon={<Icons.Lightning size={14} />} onClick={connect}>
                {status()?.state === "error" || status()?.state === "stopped" ? "Reintentar" : "Conectar"}
              </Button>
            }
          >
            <Button size="sm" fullWidth icon={<Icons.X size={14} />} onClick={disconnect}>
              Desconectar
            </Button>
          </Show>
          <Hint>Revopoint POP 2 / POP 3 por USB. Sin escáner, la pestaña Nube abre nubes PLY.</Hint>
        </div>
      </Panel>

      <Show when={connected()}>
        <Panel id="scan.preview" title="Vista en vivo" icon={<Icons.Eye size={14} />} defaultOpen>
          <div class="space-y-2 pb-3">
            <div class="flex gap-1">
              {(["depth", "color"] as const).map((k) => (
                <button
                  class={clsx(
                    "flex-1 h-7 rounded-md text-xs transition-colors",
                    view() === k ? "bg-accent/20 text-accent font-medium" : "text-text-muted hover:text-text hover:bg-surface/40"
                  )}
                  onClick={() => setView(k)}
                >
                  {k === "depth" ? "Profundidad" : "Color"}
                </button>
              ))}
            </div>
            <div class="relative rounded-md overflow-hidden bg-bg-darker border border-border">
              <canvas ref={canvas} class="block w-full h-auto" />
              <Show when={!hasFrame()}>
                <div class="absolute inset-0 flex items-center justify-center text-xs text-text-dim">Esperando cuadros…</div>
              </Show>
            </div>
            <Show when={streaming()}>
              <div class="flex justify-between text-xs">
                <span class="text-text-muted">
                  Distancia{" "}
                  <span class="text-text font-mono">
                    {status()!.distance_cm > 0 ? `${status()!.distance_cm.toFixed(0)} cm` : "—"}
                  </span>
                </span>
                <span class="text-text-muted">
                  Cobertura <span class="text-text font-mono">{(status()!.coverage * 100).toFixed(0)} %</span>
                </span>
              </div>
            </Show>
            <Slider
              label="Ganancia del sensor"
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
            <Hint>Hasta 3 es lo recomendado: más ganancia ilumina pero mete ruido.</Hint>
          </div>
        </Panel>

        <Panel id="scan.scan" title="Escaneo" icon={<Icons.ArrowsClockwise size={14} />} defaultOpen>
          <div class="space-y-3 pb-3">
            <p class="text-xs text-text-muted leading-relaxed">
              Gira el objeto (o el escáner) despacio: cada cuadro se alinea con lo escaneado y se suma. Sin escaneo, la
              nube sale del cuadro actual.
            </p>
            <div class="flex gap-2">
              <Show
                when={status()?.scanning}
                fallback={
                  <Button size="sm" class="flex-1" disabled={!streaming()} icon={<Icons.Play size={14} />} onClick={() => scan("start")}>
                    {status()?.points ? "Escanear de nuevo" : "Escanear"}
                  </Button>
                }
              >
                <Button size="sm" class="flex-1" icon={<Icons.Pause size={14} />} onClick={() => scan("stop")}>
                  Pausar
                </Button>
              </Show>
              <Button
                size="sm"
                variant="ghost"
                disabled={!status()?.points && !status()?.scanning}
                icon={<Icons.Trash size={14} />}
                onClick={() => scan("reset")}
              >
                Descartar
              </Button>
            </div>
            <Show when={status()?.frames}>
              <div class="space-y-1 text-xs">
                <div class="flex justify-between">
                  <span class="text-text-muted">Seguimiento</span>
                  <span class={status()!.tracking_ok ? "text-success" : "text-warning"}>
                    {status()!.tracking_ok ? "alineado" : "perdido: vuelve a una zona ya escaneada"}
                  </span>
                </div>
                <Row label="Cuadros sumados / descartados">
                  {status()!.registered} / {status()!.dropped}
                </Row>
                <Row label="Puntos">{status()!.points.toLocaleString()}</Row>
              </div>
            </Show>
            <Button
              variant="primary"
              size="sm"
              fullWidth
              disabled={!streaming() || busy() || status()?.scanning}
              loading={cloud().busy() === "Tomar la nube"}
              icon={<Icons.MagicWand size={14} />}
              onClick={takeCloud}
            >
              {status()?.points ? "Limpiar la nube del escaneo" : "Limpiar la nube del cuadro actual"}
            </Button>
            <Hint>Pasa la nube a la pestaña Nube para quitar ruido, fragmentos y la mesa antes de crear el modelo.</Hint>
          </div>
        </Panel>

        <Panel id="scan.volume" title="Volumen de escaneo" icon={<Icons.Ruler size={14} />} defaultOpen={false}>
          <div class="space-y-4 pb-3">
            <Slider
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
              label="Ancho a los lados"
              value={settings().box_mm}
              min={0}
              max={800}
              step={10}
              showValue
              formatValue={(v) => (v === 0 ? "sin límite" : `${(v / 10).toFixed(0)} cm`)}
              onChange={(v) => updateSettings({ box_mm: v })}
            />
            <Slider
              label="Suavizado temporal"
              value={settings().temporal_frames}
              min={1}
              max={6}
              step={1}
              showValue
              formatValue={(v) => (v === 1 ? "no" : `${v} cuadros`)}
              onChange={(v) => updateSettings({ temporal_frames: v })}
            />
            <Checkbox label="Quitar puntos sueltos" checked={settings().clean_noise} onChange={(v) => updateSettings({ clean_noise: v })} />
            <Checkbox
              label="Aislar el objeto (grupo más grande)"
              checked={settings().isolate_object}
              onChange={(v) => updateSettings({ isolate_object: v })}
            />
            <Checkbox label="Quitar bordes voladores" checked={settings().edge_filter} onChange={(v) => updateSettings({ edge_filter: v })} />
            <Hint>Estos filtros actúan al capturar; los de la pestaña Nube, sobre la nube ya tomada.</Hint>
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
      <Panel id="scan.cloud.source" title="Nube de puntos" icon={<Icons.Cube size={14} />} defaultOpen>
        <div class="space-y-3 pb-3">
          <Show
            when={info()}
            fallback={
              <>
                <p class="text-xs text-text-muted leading-relaxed">
                  Toma la nube del escaneo (o del cuadro actual) para limpiarla antes de crear el modelo, o abre una nube PLY.
                </p>
                <Button
                  variant="primary"
                  size="sm"
                  fullWidth
                  disabled={!streaming() || busy()}
                  loading={cloud().busy() === "Tomar la nube"}
                  icon={<Icons.Scan size={14} />}
                  onClick={takeCloud}
                >
                  Tomar la nube del escáner
                </Button>
              </>
            }
          >
            {(current) => (
              <>
                <div class="space-y-1">
                  <Row label="Origen">{current().source}</Row>
                  <Row label="Puntos">{current().points.toLocaleString()}</Row>
                  <Row label="Separación típica">{current().spacing_mm > 0 ? `${current().spacing_mm.toFixed(2)} mm` : "—"}</Row>
                </div>
                <div class="flex gap-2">
                  <Button
                    size="sm"
                    class="flex-1"
                    disabled={!current().undo || busy()}
                    title={current().undo ? `Deshacer: ${current().undo} (Ctrl+Z)` : "Nada que deshacer"}
                    icon={<Icons.ArrowCounterClockwise size={14} />}
                    onClick={() => cloud().history(false)}
                  >
                    Deshacer
                  </Button>
                  <Button
                    size="sm"
                    class="flex-1"
                    disabled={!current().redo || busy()}
                    title={current().redo ? `Rehacer: ${current().redo} (Ctrl+Shift+Z)` : "Nada que rehacer"}
                    icon={<Icons.ArrowClockwise size={14} />}
                    onClick={() => cloud().history(true)}
                  >
                    Rehacer
                  </Button>
                </div>
                <Checkbox
                  label="Ver la nube en lugar del modelo"
                  checked={cloud().shown()}
                  onChange={(v) => cloud().setShown(v)}
                />
                <Slider
                  label="Tamaño de los puntos"
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
          <div class="flex gap-2">
            <Button size="sm" variant="ghost" class="flex-1" disabled={busy()} icon={<Icons.FolderOpen size={14} />} onClick={importCloud}>
              Abrir PLY
            </Button>
            <Show when={info()}>
              <Button size="sm" variant="ghost" class="flex-1" disabled={busy()} icon={<Icons.FloppyDisk size={14} />} onClick={exportCloud}>
                Guardar PLY
              </Button>
            </Show>
          </div>
          <Show when={info()}>
            <div class="flex gap-2">
              <Show when={streaming()}>
                <Button size="sm" variant="ghost" class="flex-1" disabled={busy()} icon={<Icons.Scan size={14} />} onClick={takeCloud}>
                  Volver a tomar
                </Button>
              </Show>
              <Button size="sm" variant="ghost" class="flex-1" disabled={busy()} icon={<Icons.X size={14} />} onClick={() => cloud().discard()}>
                Cerrar la nube
              </Button>
            </div>
          </Show>
        </div>
      </Panel>

      <Show when={info()}>
        <Panel id="scan.cloud.select" title="Selección" icon={<Icons.Cursor size={14} />} defaultOpen>
          <div class="space-y-3 pb-3">
            <div class="flex gap-1">
              <For each={SELECT_TOOLS}>
                {(t) => (
                  <button
                    class={clsx(
                      "flex-1 h-8 rounded-md text-xs flex items-center justify-center gap-1.5 transition-colors",
                      cloud().tool() === t.id ? "bg-accent/20 text-accent font-medium" : "text-text-muted hover:text-text hover:bg-surface/40"
                    )}
                    onClick={() => cloud().setTool(t.id)}
                  >
                    <t.icon size={14} />
                    {t.label}
                  </button>
                )}
              </For>
            </div>
            <Show when={cloud().tool() === "brush"}>
              <Slider
                label="Tamaño del pincel"
                value={clean().brush_px}
                min={5}
                max={150}
                step={1}
                showValue
                formatValue={(v) => `${v} px`}
                onChange={(v) => updateClean({ brush_px: v })}
              />
            </Show>
            <Hint>
              {cloud().tool() === "polygon"
                ? "Clic a clic marca los vértices de la cuerda; se cierra con clic en el primer punto, doble clic o Enter. Retroceso quita el último punto y Esc cancela. "
                : "Arrastra con el botón izquierdo sobre el visor. "}
              Shift suma, Ctrl resta. La vista se gira con el botón central o Alt + izquierdo. La selección atraviesa la
              nube: gira la vista para no tomar la cara de atrás.
            </Hint>
            <Row label="Seleccionados">{cloud().selected().toLocaleString()}</Row>
            <div class="flex gap-1">
              <Button size="sm" variant="ghost" class="flex-1" title="Seleccionar todo (A)" onClick={() => cloud().select("all")}>
                Todo
              </Button>
              <Button size="sm" variant="ghost" class="flex-1" title="No seleccionar nada (Alt+A)" onClick={() => cloud().select("none")}>
                Nada
              </Button>
              <Button size="sm" variant="ghost" class="flex-1" title="Invertir la selección (Ctrl+I)" onClick={() => cloud().select("invert")}>
                Invertir
              </Button>
            </div>
            <div class="flex gap-2">
              <Button
                size="sm"
                variant="danger"
                class="flex-1"
                disabled={cloud().selected() === 0 || busy()}
                title="Borrar los puntos seleccionados (Supr)"
                icon={<Icons.Trash size={14} />}
                onClick={() => cloud().deleteSelection()}
              >
                Borrar
              </Button>
              <Button
                size="sm"
                class="flex-1"
                disabled={cloud().selected() === 0 || busy()}
                title="Conservar solo los puntos seleccionados"
                icon={<Icons.Scissors size={14} />}
                onClick={() => cloud().keepSelection()}
              >
                Recortar
              </Button>
            </div>
          </div>
        </Panel>

        <Panel id="scan.cloud.noise" title="Ruido" icon={<Icons.MagicWand size={14} />} defaultOpen>
          <div class="space-y-4 pb-3">
            <div class="space-y-2">
              <p class="text-xs text-text font-medium">Estadístico</p>
              <Hint>Quita la bruma alrededor de la superficie: los puntos más lejos de sus vecinos que el resto.</Hint>
              <Slider
                label="Vecinos"
                value={clean().sor_neighbors}
                min={4}
                max={50}
                step={1}
                showValue
                onChange={(v) => updateClean({ sor_neighbors: v })}
              />
              <Slider
                label="Tolerancia"
                value={clean().sor_ratio}
                min={0.5}
                max={4}
                step={0.1}
                showValue
                formatValue={(v) => `${v.toFixed(1)} σ`}
                onChange={(v) => updateClean({ sor_ratio: v })}
              />
              <PreviewActions
                label="Quitar ruido"
                op={() => ({ kind: "statistical", neighbors: clean().sor_neighbors, std_ratio: clean().sor_ratio })}
              />
            </div>
            <div class="space-y-2 pt-3 border-t border-border/50">
              <p class="text-xs text-text font-medium">Puntos sueltos</p>
              <Hint>Quita los puntos con pocos vecinos cerca: motas y restos aislados.</Hint>
              <Slider
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
                label="Vecinos mínimos"
                value={clean().radius_neighbors}
                min={1}
                max={40}
                step={1}
                showValue
                onChange={(v) => updateClean({ radius_neighbors: v })}
              />
              <PreviewActions
                label="Quitar puntos sueltos"
                op={() => ({ kind: "radius", radius_mm: clean().radius_mm, min_neighbors: clean().radius_neighbors })}
              />
            </div>
          </div>
        </Panel>

        <Panel id="scan.cloud.fragments" title="Fragmentos sueltos" icon={<Icons.Scissors size={14} />} defaultOpen>
          <div class="space-y-2 pb-3">
            <Hint>
              Partes separadas del objeto (manos, soportes, restos del fondo). Dos puntos más lejos que la separación quedan
              en fragmentos distintos.
            </Hint>
            <Slider
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
              label="Tamaño mínimo"
              value={clean().min_percent}
              min={1}
              max={100}
              step={1}
              showValue
              formatValue={(v) => (v >= 100 ? "solo el más grande" : `${v} % del más grande`)}
              onChange={(v) => updateClean({ min_percent: v })}
            />
            <PreviewActions label="Quitar fragmentos" op={() => ({ kind: "fragments", gap_mm: clean().gap_mm, min_percent: clean().min_percent })} />
            <Button
              size="sm"
              variant="ghost"
              fullWidth
              disabled={busy()}
              icon={<Icons.CubeFocus size={14} />}
              onClick={() => cloud().edit({ kind: "fragments", gap_mm: clean().gap_mm, min_percent: 100 }, "Quitar fragmentos")}
            >
              Quedarse con la pieza más grande
            </Button>
          </div>
        </Panel>

        <Panel id="scan.cloud.plane" title="Mesa o base" icon={<Icons.Square size={14} />} defaultOpen={false}>
          <div class="space-y-2 pb-3">
            <Hint>Busca el plano más grande (la mesa, el plato giratorio) y lo quita.</Hint>
            <Slider
              label="Grosor del plano"
              value={clean().plane_mm}
              min={0.5}
              max={10}
              step={0.5}
              showValue
              formatValue={(v) => `${v.toFixed(1)} mm`}
              onChange={(v) => updateClean({ plane_mm: v })}
            />
            <Checkbox
              label="También lo que queda debajo"
              checked={clean().plane_below}
              onChange={(v) => updateClean({ plane_below: v })}
            />
            <PreviewActions label="Quitar la mesa" op={() => ({ kind: "plane", threshold_mm: clean().plane_mm, below: clean().plane_below })} />
          </div>
        </Panel>

        <Panel id="scan.cloud.simplify" title="Simplificar y suavizar" icon={<Icons.GridFour size={14} />} defaultOpen={false}>
          <div class="space-y-4 pb-3">
            <div class="space-y-2">
              <Hint>Deja un punto por celda: menos puntos, operaciones y malla más rápidas.</Hint>
              <Slider
                label="Celda"
                value={clean().voxel_mm}
                min={0.5}
                max={10}
                step={0.5}
                showValue
                formatValue={(v) => `${v.toFixed(1)} mm`}
                onChange={(v) => updateClean({ voxel_mm: v })}
              />
              <Button
                size="sm"
                fullWidth
                disabled={busy()}
                loading={cloud().busy() === "Simplificar"}
                onClick={() => cloud().edit({ kind: "downsample", voxel_mm: clean().voxel_mm }, "Simplificar")}
              >
                Simplificar
              </Button>
            </div>
            <div class="space-y-2 pt-3 border-t border-border/50">
              <Hint>Lleva cada punto hacia la superficie que forman sus vecinos, sin encoger la forma.</Hint>
              <Slider
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
                label="Intensidad"
                value={clean().smooth_strength}
                min={0.1}
                max={1}
                step={0.05}
                showValue
                formatValue={(v) => `${Math.round(v * 100)} %`}
                onChange={(v) => updateClean({ smooth_strength: v })}
              />
              <Button
                size="sm"
                fullWidth
                disabled={busy()}
                loading={cloud().busy() === "Suavizar"}
                onClick={() => cloud().edit({ kind: "smooth", radius_mm: clean().smooth_mm, strength: clean().smooth_strength }, "Suavizar")}
              >
                Suavizar
              </Button>
            </div>
          </div>
        </Panel>
      </Show>
    </div>
  );

  // ═════════════════════════════════════════════════════════════════════════
  // MALLA
  // ═════════════════════════════════════════════════════════════════════════

  const meshPanels = () => (
    <div>
      <Panel id="scan.mesh" title="Crear modelo" icon={<Icons.Cube size={14} />} defaultOpen>
        <div class="space-y-4 pb-3">
          <p class="text-xs text-text-muted leading-relaxed">
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
          </p>
          <Slider
            label="Detalle"
            value={mesh().voxel_mm}
            min={1}
            max={8}
            step={0.5}
            showValue
            formatValue={(v) => `${v.toFixed(1)} mm`}
            onChange={(v) => setMesh({ ...mesh(), voxel_mm: v })}
          />
          <Slider label="Suavizado" value={mesh().smooth} min={0} max={4} step={1} showValue onChange={(v) => setMesh({ ...mesh(), smooth: v })} />
          <Slider label="Rellenar huecos" value={mesh().fill} min={0} max={4} step={1} showValue onChange={(v) => setMesh({ ...mesh(), fill: v })} />
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
          <Hint>Reemplaza el modelo abierto. Queda en milímetros, apoyado en el piso.</Hint>
        </div>
      </Panel>

      <Show when={props.hasModel}>
        <Panel id="scan.mesh.after" title="Después" icon={<Icons.Wrench size={14} />} defaultOpen>
          <div class="space-y-2 pb-3">
            <Hint>
              La malla puede traer agujeros, piezas sueltas o caras sueltas. Reparar (en Preparar) los detecta y corrige.
            </Hint>
            <Button size="sm" fullWidth icon={<Icons.Wrench size={14} />} onClick={() => props.onRepair?.()}>
              Reparar la malla
            </Button>
            <Show when={info()}>
              <Checkbox label="Ver la nube en lugar del modelo" checked={cloud().shown()} onChange={(v) => cloud().setShown(v)} />
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
