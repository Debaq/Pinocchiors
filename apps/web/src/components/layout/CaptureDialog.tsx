import { Component, Show, createSignal, onCleanup, onMount } from "solid-js";
import { Button, Checkbox, Slider } from "../ui";
import * as Icons from "../icons";
import { captureMotion, type CaptureMotion } from "../../lib/capture";

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
  let abort: AbortController | null = null;

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
    else props.onCancel();
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
    });
  });

  return (
    <div class="fixed inset-0 z-50 flex items-center justify-center bg-bg-darker/70 backdrop-blur-sm p-4">
      <div role="dialog" aria-label="Capturar movimiento" class="w-full max-w-md rounded-lg border border-border bg-bg shadow-xl">
        <div class="flex items-center justify-between px-4 h-11 border-b border-border">
          <h2 class="flex items-center gap-2 text-sm font-semibold text-text">
            <Icons.Record size={16} class="text-accent" />
            Capturar movimiento de un video
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
          <div class="flex items-center gap-2">
            <Button size="sm" onClick={async () => setVideo(await pickFile("video/*"))} disabled={!!progress()}>
              <Icons.FolderOpen size={12} /> Video…
            </Button>
            <span class="text-xs font-mono text-text-muted truncate">{video()?.name ?? "ninguno"}</span>
          </div>
          <div class="grid grid-cols-2 gap-3">
            <Slider label="Cuadros por segundo" value={fps()} onChange={(v) => setFps(Math.round(v))} min={12} max={60} step={1} disabled={!!progress()} />
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
          <Button size="sm" variant="primary" onClick={() => void run()} disabled={!video() || !!progress() || (useLocalModel() && !model())}>
            Detectar
          </Button>
        </div>
      </div>
    </div>
  );
};
