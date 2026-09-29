import { Component, Show, createSignal, onCleanup, onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { clsx } from "clsx";
import { Panel, Slider, Checkbox, Button } from "../ui";
import { createPersisted } from "../../lib/ui-state";
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
}

/** Reconstrucción de la malla (`MeshSettingsDto`) */
export interface ScanMeshSettings {
  voxel_mm: number;
  fill: number;
  smooth: number;
}

const DEFAULT_SETTINGS: ScanSettings = {
  clip_min_mm: 120,
  clip_max_mm: 450,
  box_mm: 300,
  clean_noise: true,
  isolate_object: true,
  edge_filter: true,
  temporal_frames: 3,
};

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

export interface ScanStepProps {
  /** Crea el modelo de trabajo con la malla del escaneo */
  onCreateModel?: (mesh: ScanMeshSettings) => Promise<void> | void;
  isProcessing?: boolean;
}

/**
 * Escáneres 3D (Orizon3D): Revopoint POP 2 / POP 3 por V4L2. Conectar, ver
 * profundidad y color en vivo, escanear girando el objeto y traer la malla.
 */
export const ScanStep: Component<ScanStepProps> = (props) => {
  const [status, setStatus] = createSignal<ScannerStatus>();
  const [settings, setSettings] = createPersisted<ScanSettings>("scan.settings", DEFAULT_SETTINGS);
  const [mesh, setMesh] = createPersisted<ScanMeshSettings>("scan.mesh", DEFAULT_MESH);
  const [gain, setGain] = createPersisted("scan.gain", 1);
  const [view, setView] = createPersisted<"depth" | "color">("scan.view", "depth");
  const [hasFrame, setHasFrame] = createSignal(false);
  let canvas: HTMLCanvasElement | undefined;
  let timer: number | undefined;
  let alive = true;

  const connected = () => {
    const s = status()?.state;
    return s === "connecting" || s === "streaming";
  };

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

  // Una petición a la vez: la siguiente sale cuando termina la anterior
  const poll = async () => {
    timer = undefined;
    try {
      const s = await invoke<ScannerStatus>("scanner_status");
      if (!alive) return;
      setStatus(s);
      if (s.state === "streaming") await drawPreview();
    } catch (e) {
      console.error("Escáner:", e);
    }
    if (alive && connected()) timer = window.setTimeout(poll, POLL_MS);
  };

  onMount(poll);
  onCleanup(() => {
    alive = false;
    if (timer !== undefined) window.clearTimeout(timer);
  });

  const connect = async () => {
    setHasFrame(false);
    setStatus(await invoke<ScannerStatus>("scanner_connect", { settings: settings() }));
    await invoke("scanner_set_gain", { gain: gain() });
    if (timer === undefined) poll();
  };

  const disconnect = async () => {
    await invoke("scanner_disconnect");
    setHasFrame(false);
    await poll();
  };

  const updateSettings = (partial: Partial<ScanSettings>) => {
    const next = { ...settings(), ...partial };
    if (next.clip_max_mm < next.clip_min_mm + 10) next.clip_max_mm = next.clip_min_mm + 10;
    setSettings(next);
    invoke("scanner_set_settings", { settings: next });
  };

  const scan = async (action: "start" | "stop" | "reset") => {
    await invoke("scanner_scan", { action });
    setStatus(await invoke<ScannerStatus>("scanner_status"));
  };

  const streaming = () => status()?.state === "streaming";

  return (
    <div class="space-y-5">
      <h3 class="text-sm font-semibold text-text">Orizon3D</h3>
      <p class="text-xs text-text-muted leading-relaxed">
        Escáneres 3D Revopoint POP 2 / POP 3 por USB. Conecta el escáner, apunta al objeto y crea el modelo desde un
        cuadro o desde un escaneo girando alrededor.
      </p>

      <Panel id="scan.device" title="Escáner" icon={<Icons.Scan size={14} />} defaultOpen>
        <div class="space-y-3">
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

          <Show when={status()?.device}>
            <div class="space-y-1 text-xs">
              <div class="flex justify-between gap-3">
                <span class="text-text-muted">Dispositivo</span>
                <span class="text-text font-mono truncate">{status()!.device}</span>
              </div>
              <Show when={status()!.depth_stream}>
                <div class="flex justify-between gap-3">
                  <span class="text-text-muted">Profundidad</span>
                  <span class="text-text font-mono">{status()!.depth_stream}</span>
                </div>
              </Show>
              <div class="flex justify-between gap-3">
                <span class="text-text-muted">Color</span>
                <span class="text-text font-mono">{status()!.color_stream ?? "sin color"}</span>
              </div>
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
        </div>
      </Panel>

      <Show when={connected()}>
        <Panel id="scan.preview" title="Vista en vivo" icon={<Icons.Eye size={14} />} defaultOpen>
          <div class="space-y-2">
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
                invoke("scanner_set_gain", { gain: v });
              }}
            />
            <p class="text-[11px] text-text-dim leading-relaxed">
              Hasta 3 es lo recomendado: más ganancia ilumina pero mete ruido.
            </p>
          </div>
        </Panel>

        <Panel id="scan.volume" title="Volumen de escaneo" icon={<Icons.Ruler size={14} />} defaultOpen={false}>
          <div class="space-y-4">
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
            <Checkbox
              label="Quitar puntos sueltos"
              checked={settings().clean_noise}
              onChange={(v) => updateSettings({ clean_noise: v })}
            />
            <Checkbox
              label="Aislar el objeto (grupo más grande)"
              checked={settings().isolate_object}
              onChange={(v) => updateSettings({ isolate_object: v })}
            />
            <Checkbox
              label="Quitar bordes voladores"
              checked={settings().edge_filter}
              onChange={(v) => updateSettings({ edge_filter: v })}
            />
          </div>
        </Panel>

        <Panel id="scan.scan" title="Escaneo" icon={<Icons.ArrowsClockwise size={14} />} defaultOpen>
          <div class="space-y-3">
            <p class="text-xs text-text-muted leading-relaxed">
              Gira el objeto (o el escáner) despacio: cada cuadro se alinea con lo escaneado y se suma. Sin escaneo, el
              modelo sale del cuadro actual.
            </p>
            <div class="flex gap-2">
              <Show
                when={status()?.scanning}
                fallback={
                  <Button
                    size="sm"
                    class="flex-1"
                    disabled={!streaming()}
                    icon={<Icons.Play size={14} />}
                    onClick={() => scan("start")}
                  >
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
                <div class="flex justify-between">
                  <span class="text-text-muted">Cuadros sumados / descartados</span>
                  <span class="text-text font-mono">
                    {status()!.registered} / {status()!.dropped}
                  </span>
                </div>
                <div class="flex justify-between">
                  <span class="text-text-muted">Puntos</span>
                  <span class="text-text font-mono">{status()!.points.toLocaleString()}</span>
                </div>
              </div>
            </Show>
          </div>
        </Panel>

        <Panel id="scan.mesh" title="Crear modelo" icon={<Icons.Cube size={14} />} defaultOpen>
          <div class="space-y-4">
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
            <Slider
              label="Suavizado"
              value={mesh().smooth}
              min={0}
              max={4}
              step={1}
              showValue
              onChange={(v) => setMesh({ ...mesh(), smooth: v })}
            />
            <Slider
              label="Rellenar huecos"
              value={mesh().fill}
              min={0}
              max={4}
              step={1}
              showValue
              onChange={(v) => setMesh({ ...mesh(), fill: v })}
            />
            <Button
              variant="primary"
              size="sm"
              fullWidth
              disabled={!streaming() || props.isProcessing}
              loading={props.isProcessing}
              icon={<Icons.Cube size={14} />}
              onClick={() => props.onCreateModel?.(mesh())}
            >
              {status()?.points ? "Crear modelo del escaneo" : "Crear modelo del cuadro actual"}
            </Button>
            <p class="text-[11px] text-text-dim leading-relaxed">
              Reemplaza el modelo abierto. Queda en milímetros, apoyado en el piso.
            </p>
          </div>
        </Panel>
      </Show>
    </div>
  );
};
