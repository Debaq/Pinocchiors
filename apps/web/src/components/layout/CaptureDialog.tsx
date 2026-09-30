import { Component, Show, createSignal, onCleanup, onMount } from "solid-js";
import { Button, Checkbox, Slider } from "../ui";
import * as Icons from "../icons";
import { captureMotion, type CaptureMotion } from "../../lib/capture";
import type { LiveCapture } from "../../lib/captureRunner";

/** Uniones que se dibujan sobre la vista previa (índices de MediaPipe Pose) */
const BONES: [number, number][] = [
  [11, 12], [11, 13], [13, 15], [12, 14], [14, 16], [11, 23], [12, 24], [23, 24],
  [23, 25], [25, 27], [27, 31], [24, 26], [26, 28], [28, 32], [0, 11], [0, 12],
];

export interface CaptureDialogProps {
  /** Movimiento listo: la app abre el mapeo del retargeting */
  onMotion: (motion: CaptureMotion, name: string) => void;
  onCancel: () => void;
}

const pickFile = (accept: string) =>
  new Promise<File | null>((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = accept;
    input.onchange = () => resolve(input.files?.[0] ?? null);
    input.oncancel = () => resolve(null);
    input.click();
  });

/**
 * Captura de movimiento desde un video (MediaPipe): elegir el video, el
 * suavizado y el modelo; se detecta cuadro a cuadro y se pasa al retargeting
 */
export const CaptureDialog: Component<CaptureDialogProps> = (props) => {
  const [video, setVideo] = createSignal<File | null>(null);
  const [model, setModel] = createSignal<File | null>(null);
  const [useLocalModel, setUseLocalModel] = createSignal(false);
  const [fps, setFps] = createSignal(30);
  const [smoothing, setSmoothing] = createSignal(0.5);
  const [progress, setProgress] = createSignal<{ done: number; total: number } | null>(null);
  const [error, setError] = createSignal<string>();
  const [source, setSource] = createSignal<"file" | "camera">("file");
  const [live, setLive] = createSignal<LiveCapture | null>(null);
  const [liveState, setLiveState] = createSignal<"idle" | "opening" | "countdown" | "recording">("idle");
  const [countdown, setCountdown] = createSignal(0);
  let abort: AbortController | null = null;
  let preview: HTMLDivElement | undefined;
  let overlay: HTMLCanvasElement | undefined;
  let drawFrame = 0;

  const closeLive = () => {
    cancelAnimationFrame(drawFrame);
    live()?.close();
    setLive(null);
    setLiveState("idle");
  };

  /** Abre la cámara y dibuja los puntos detectados encima */
  const openCamera = async () => {
    setError(undefined);
    setLiveState("opening");
    try {
      const { startLive } = await import("../../lib/captureRunner");
      const local = useLocalModel() ? model() : null;
      const capture = await startLive(local ? new Uint8Array(await local.arrayBuffer()) : undefined);
      setLive(capture);
      setLiveState("idle");
      capture.video.className = "w-full rounded -scale-x-100";
      preview?.prepend(capture.video);
      const draw = () => {
        drawFrame = requestAnimationFrame(draw);
        const c = overlay;
        if (!c) return;
        c.width = capture.video.clientWidth;
        c.height = capture.video.clientHeight;
        const g = c.getContext("2d");
        const f = capture.last();
        if (!g) return;
        g.clearRect(0, 0, c.width, c.height);
        if (!f) return;
        g.strokeStyle = liveState() === "recording" ? "#ff5555" : "#50fa7b";
        g.lineWidth = 3;
        // Espejado como la vista previa
        const at = (k: number) => [(1 - f.image[k].x) * c.width, f.image[k].y * c.height] as const;
        for (const [a, b] of BONES) {
          g.beginPath();
          g.moveTo(...at(a));
          g.lineTo(...at(b));
          g.stroke();
        }
      };
      draw();
    } catch (e) {
      setLiveState("idle");
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const record = async () => {
    const capture = live();
    if (!capture) return;
    setLiveState("countdown");
    for (let s = 3; s > 0; s--) {
      setCountdown(s);
      await new Promise((r) => setTimeout(r, 1000));
      if (live() !== capture) return;
    }
    capture.startRecording();
    setLiveState("recording");
  };

  const stopRecording = () => {
    const capture = live();
    if (!capture) return;
    const { frames, fps: rate } = capture.stopRecording();
    closeLive();
    if (frames.filter(Boolean).length < 2) {
      setError("No se vio a nadie durante la grabación (el cuerpo entero tiene que estar en cuadro)");
      return;
    }
    const motion = captureMotion(frames, rate, { smoothing: smoothing() });
    if (!motion) {
      setError("Faltan partes del cuerpo en toda la grabación (caderas, hombros o piernas)");
      return;
    }
    props.onMotion(motion, `Cámara ${new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`);
  };

  const run = async () => {
    const file = video();
    if (!file) return;
    setError(undefined);
    abort = new AbortController();
    setProgress({ done: 0, total: 1 });
    try {
      const { detectVideo } = await import("../../lib/captureRunner");
      const local = useLocalModel() ? model() : null;
      const frames = await detectVideo(file, {
        fps: fps(),
        model: local ? new Uint8Array(await local.arrayBuffer()) : undefined,
        onProgress: (done, total) => setProgress({ done, total }),
        signal: abort.signal,
      });
      const found = frames.filter(Boolean).length;
      if (found < 2) throw new Error("No se encontró a nadie en el video (se necesita el cuerpo entero a la vista)");
      const motion = captureMotion(frames, fps(), { smoothing: smoothing() });
      if (!motion) throw new Error("Faltan partes del cuerpo en todo el video (caderas, hombros o piernas)");
      props.onMotion(motion, file.name);
    } catch (e) {
      if (e instanceof DOMException && e.name === "AbortError") return;
      console.error("Capture error:", e);
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setProgress(null);
      abort = null;
    }
  };

  const cancel = () => {
    if (abort) abort.abort();
    else if (liveState() === "recording" || liveState() === "countdown") {
      closeLive();
    } else {
      closeLive();
      props.onCancel();
    }
  };

  onMount(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        cancel();
      } else if (e.key !== "Tab") {
        e.stopPropagation();
      }
    };
    window.addEventListener("keydown", onKey, true);
    onCleanup(() => {
      window.removeEventListener("keydown", onKey, true);
      abort?.abort();
      closeLive();
    });
  });

  return (
    <div class="fixed inset-0 z-50 flex items-center justify-center bg-bg-darker/70 backdrop-blur-sm p-4">
      <div role="dialog" aria-label="Capturar movimiento" class="w-full max-w-md rounded-lg border border-border bg-bg shadow-xl">
        <div class="flex items-center justify-between px-4 h-11 border-b border-border">
          <h2 class="flex items-center gap-2 text-sm font-semibold text-text">
            <Icons.Record size={16} class="text-accent" />
            Capturar movimiento
          </h2>
          <button class="w-7 h-7 flex items-center justify-center rounded text-text-muted hover:text-text hover:bg-surface/40" aria-label="Cancelar" onClick={cancel}>
            <Icons.X size={14} />
          </button>
        </div>
        <div class="p-4 space-y-3">
          <p class="text-xs text-text-muted leading-relaxed">
            Una persona, de cuerpo entero, con buena luz y ropa que no tape las articulaciones. La profundidad sale de una
            sola cámara: el giro del antebrazo y lo que queda tapado son poco fiables. Empezar quieto, de frente, ayuda.
          </p>
          <div class="grid grid-cols-2 gap-1">
            <Button size="sm" variant={source() === "file" ? "primary" : "default"} disabled={!!progress() || !!live()} onClick={() => setSource("file")}>
              Archivo de video
            </Button>
            <Button size="sm" variant={source() === "camera" ? "primary" : "default"} disabled={!!progress()} onClick={() => setSource("camera")}>
              Cámara en vivo
            </Button>
          </div>
          <Show when={source() === "file"}>
            <div class="flex items-center gap-2">
              <Button size="sm" onClick={async () => setVideo(await pickFile("video/*"))} disabled={!!progress()}>
                <Icons.FolderOpen size={12} /> Video…
              </Button>
              <span class="text-xs font-mono text-text-muted truncate">{video()?.name ?? "ninguno"}</span>
            </div>
          </Show>
          <Show when={source() === "camera"}>
            <div ref={preview} class="relative rounded bg-bg-darker min-h-12">
              <canvas ref={overlay} class="absolute inset-0 w-full h-full pointer-events-none" />
              <Show when={liveState() === "countdown"}>
                <div class="absolute inset-0 flex items-center justify-center text-5xl font-bold text-white drop-shadow">{countdown()}</div>
              </Show>
            </div>
            <div class="flex gap-1">
              <Show
                when={live()}
                fallback={
                  <Button size="sm" fullWidth disabled={liveState() === "opening" || (useLocalModel() && !model())} onClick={() => void openCamera()}>
                    {liveState() === "opening" ? "Abriendo…" : "Abrir cámara"}
                  </Button>
                }
              >
                <Show
                  when={liveState() === "recording"}
                  fallback={
                    <Button size="sm" variant="primary" fullWidth disabled={liveState() === "countdown"} onClick={() => void record()}>
                      <Icons.Record size={12} /> Grabar (cuenta de 3)
                    </Button>
                  }
                >
                  <Button size="sm" variant="danger" fullWidth onClick={stopRecording}>
                    <Icons.Stop size={12} /> Detener y usar
                  </Button>
                </Show>
              </Show>
            </div>
          </Show>
          <div class="grid grid-cols-2 gap-3">
            <Show when={source() === "file"} fallback={<div />}>
              <Slider label="Cuadros por segundo" value={fps()} onChange={(v) => setFps(Math.round(v))} min={12} max={60} step={1} disabled={!!progress()} />
            </Show>
            <Slider
              label="Suavizado"
              value={smoothing()}
              onChange={setSmoothing}
              min={0}
              max={1}
              step={0.05}
              formatValue={(v) => `${Math.round(v * 100)} %`}
              disabled={!!progress()}
            />
          </div>
          <Checkbox
            label="Usar un modelo descargado (.task, sin conexión)"
            checked={useLocalModel()}
            onChange={setUseLocalModel}
          />
          <Show
            when={useLocalModel()}
            fallback={<p class="text-[11px] text-text-dim leading-relaxed">El modelo de pose (unos 9 MB) se baja de Google la primera vez.</p>}
          >
            <div class="flex items-center gap-2">
              <Button size="sm" onClick={async () => setModel(await pickFile(".task"))} disabled={!!progress()}>
                Modelo…
              </Button>
              <span class="text-xs font-mono text-text-muted truncate">{model()?.name ?? "pose_landmarker_full.task"}</span>
            </div>
          </Show>
          <Show when={progress()}>
            {(p) => (
              <div class="space-y-1">
                <div class="h-1.5 rounded bg-surface overflow-hidden">
                  <div class="h-full bg-accent" style={{ width: `${(100 * p().done) / p().total}%` }} />
                </div>
                <p class="text-xs text-text-muted">
                  Detectando: cuadro {p().done} de {p().total}
                </p>
              </div>
            )}
          </Show>
          <Show when={error()}>
            <p class="text-xs text-error leading-relaxed">{error()}</p>
          </Show>
        </div>
        <div class="flex justify-end gap-2 px-4 py-3 border-t border-border">
          <Button size="sm" variant="ghost" onClick={cancel}>
            {progress() ? "Detener" : "Cancelar"}
          </Button>
          <Show when={source() === "file"}>
            <Button size="sm" variant="primary" onClick={() => void run()} disabled={!video() || !!progress() || (useLocalModel() && !model())}>
              Detectar
            </Button>
          </Show>
        </div>
      </div>
    </div>
  );
};
