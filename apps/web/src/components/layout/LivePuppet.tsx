import { Component, Show, createSignal, onCleanup, onMount } from "solid-js";
import { Button, Checkbox, Slider } from "../ui";
import * as Icons from "../icons";
import { clonePose, type Pose } from "../../lib/animation";
import { LiveSolver, captureMotion, mirrorFrame, type CaptureFrame, type CaptureMotion } from "../../lib/capture";
import type { LiveCapture } from "../../lib/captureRunner";
import type { SkeletonBone } from "../../lib/presetAnimations";
import { Retargeter, type RetargetMap } from "../../lib/retarget";

/** Uniones que se dibujan sobre la vista previa (índices de MediaPipe Pose) */
const BONES: [number, number][] = [
  [11, 12], [11, 13], [13, 15], [12, 14], [14, 16], [11, 23], [12, 24], [23, 24],
  [23, 25], [25, 27], [27, 31], [24, 26], [26, 28], [28, 32], [0, 11], [0, 12],
];

export interface LivePuppetProps {
  /** Esqueleto del modelo */
  target: SkeletonBone[];
  /** Mapeo del actor al modelo (plantilla guardada o automático) */
  mapFor: (source: SkeletonBone[]) => RetargetMap;
  /** Pose del modelo en cada cuadro de la cámara */
  onPose: (pose: Pose) => void;
  /** Grabación terminada: la app abre el mapeo y crea el clip */
  onRecorded: (motion: CaptureMotion, name: string) => void;
  onClose: () => void;
}

/**
 * Títere en vivo: la cámara mueve el esqueleto mientras la persona se mueve
 * (MediaPipe → esqueleto del actor → retargeting cuadro a cuadro). Grabar
 * guarda los cuadros y los pasa al mismo mapeo que la captura desde video.
 */
export const LivePuppet: Component<LivePuppetProps> = (props) => {
  const [state, setState] = createSignal<"opening" | "calibrating" | "live" | "countdown" | "recording">("opening");
  const [error, setError] = createSignal<string>();
  const [countdown, setCountdown] = createSignal(0);
  const [smoothing, setSmoothing] = createSignal(0.5);
  const [mirror, setMirror] = createSignal(true);
  const [rootMotion, setRootMotion] = createSignal(true);
  const solver = new LiveSolver(smoothing());
  let retargeter: Retargeter | null = null;
  let capture: LiveCapture | null = null;
  let recording: { frames: (CaptureFrame | null)[]; start: number } | null = null;
  let lastTime = -1;
  let preview: HTMLDivElement | undefined;
  let overlay: HTMLCanvasElement | undefined;
  let drawFrame = 0;
  let closed = false;

  const onFrame = (raw: CaptureFrame | null, time: number) => {
    const frame = mirror() ? mirrorFrame(raw) : raw;
    recording?.frames.push(frame);
    const dt = lastTime >= 0 && time > lastTime ? time - lastTime : 1 / 30;
    lastTime = time;
    const result = solver.push(frame, dt);
    if (!result || !solver.bones) return;
    if (!retargeter) {
      retargeter = new Retargeter(props.target, solver.bones, props.mapFor(solver.bones));
      if (state() === "calibrating") setState("live");
    }
    props.onPose(clonePose(retargeter.solve(result.positions, result.rootRotation, rootMotion())));
  };

  const recalibrate = () => {
    solver.recalibrate();
    retargeter = null;
    if (state() === "live") setState("calibrating");
  };

  const draw = () => {
    drawFrame = requestAnimationFrame(draw);
    const c = overlay;
    const f = capture?.last();
    if (!c || !capture) return;
    c.width = capture.video.clientWidth;
    c.height = capture.video.clientHeight;
    const g = c.getContext("2d");
    if (!g) return;
    g.clearRect(0, 0, c.width, c.height);
    if (!f) return;
    g.strokeStyle = state() === "recording" ? "#ff5555" : "#50fa7b";
    g.lineWidth = 2;
    // Espejado como la vista previa
    const at = (k: number) => [(1 - f.image[k].x) * c.width, f.image[k].y * c.height] as const;
    for (const [a, b] of BONES) {
      g.beginPath();
      g.moveTo(...at(a));
      g.lineTo(...at(b));
      g.stroke();
    }
  };

  const record = async () => {
    setState("countdown");
    for (let s = 3; s > 0; s--) {
      setCountdown(s);
      await new Promise((r) => setTimeout(r, 1000));
      if (closed || state() !== "countdown") return;
    }
    recording = { frames: [], start: performance.now() };
    setState("recording");
  };

  const stop = () => {
    const r = recording;
    recording = null;
    setState("live");
    if (!r) return;
    const seconds = Math.max(1e-3, (performance.now() - r.start) / 1000);
    const fps = Math.max(1, Math.round(r.frames.length / seconds));
    if (r.frames.filter(Boolean).length < 2) {
      setError("No se vio a nadie durante la grabación (el cuerpo entero tiene que estar en cuadro)");
      return;
    }
    const motion = captureMotion(r.frames, fps, { smoothing: smoothing() });
    if (!motion) {
      setError("Faltan partes del cuerpo en toda la grabación (caderas, hombros o piernas)");
      return;
    }
    setError(undefined);
    props.onRecorded(motion, `Cámara ${new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`);
  };

  onMount(async () => {
    try {
      const { startLive } = await import("../../lib/captureRunner");
      const live = await startLive(undefined, onFrame);
      if (closed) return live.close();
      capture = live;
      live.video.className = "w-full rounded -scale-x-100";
      preview?.prepend(live.video);
      setState("calibrating");
      draw();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  });

  onCleanup(() => {
    closed = true;
    cancelAnimationFrame(drawFrame);
    capture?.close();
  });

  const busy = () => state() === "countdown" || state() === "recording";

  return (
    <div class="absolute left-3 bottom-3 z-20 w-64 rounded-lg border border-border bg-bg/95 shadow-xl backdrop-blur-sm" role="dialog" aria-label="Cámara en vivo">
      <div class="flex items-center justify-between px-3 h-9 border-b border-border">
        <span class="flex items-center gap-2 text-xs font-semibold text-text">
          <Icons.PersonArmsSpread size={14} class="text-accent" />
          Cámara en vivo
        </span>
        <button class="w-6 h-6 flex items-center justify-center rounded text-text-muted hover:text-text hover:bg-surface/40" aria-label="Cerrar la cámara" onClick={props.onClose}>
          <Icons.X size={12} />
        </button>
      </div>
      <div class="p-2 space-y-2">
        <div ref={preview} class="relative rounded bg-bg-darker min-h-24">
          <canvas ref={overlay} class="absolute inset-0 w-full h-full pointer-events-none" />
          <Show when={state() === "countdown"}>
            <div class="absolute inset-0 flex items-center justify-center text-4xl font-bold text-white drop-shadow">{countdown()}</div>
          </Show>
          <Show when={state() === "recording"}>
            <span class="absolute top-1 left-1 flex items-center gap-1 px-1.5 rounded bg-error/80 text-[10px] text-white">
              <Icons.Record size={10} /> Grabando
            </span>
          </Show>
        </div>
        <Show when={error()}>
          <p class="text-xs text-error leading-relaxed">{error()}</p>
        </Show>
        <Show when={state() === "opening" && !error()}>
          <p class="text-xs text-text-muted">Abriendo la cámara y el modelo de pose (unos 9 MB la primera vez)…</p>
        </Show>
        <Show when={state() === "calibrating"}>
          <p class="text-xs text-text-muted leading-relaxed">
            Ponte de cuerpo entero, de frente y quieto: esa pose se toma como reposo.
          </p>
        </Show>
        <Show when={state() !== "opening"}>
          <Show
            when={state() === "recording"}
            fallback={
              <Button size="sm" variant="primary" fullWidth disabled={state() !== "live"} onClick={() => void record()}>
                <Icons.Record size={12} /> Grabar animación (cuenta de 3)
              </Button>
            }
          >
            <Button size="sm" variant="danger" fullWidth onClick={stop}>
              <Icons.Stop size={12} /> Detener y crear clip
            </Button>
          </Show>
          <Button size="sm" variant="ghost" fullWidth disabled={busy()} onClick={recalibrate} title="Vuelve a tomar la pose de reposo">
            <Icons.ArrowsClockwise size={12} /> Tomar el reposo de nuevo
          </Button>
          <Slider
            label="Suavizado"
            value={smoothing()}
            onChange={(v) => {
              setSmoothing(v);
              solver.setSmoothing(v);
            }}
            min={0}
            max={1}
            step={0.05}
            formatValue={(v) => `${Math.round(v * 100)} %`}
          />
          <Checkbox
            label="Como espejo (tu derecha mueve su izquierda)"
            checked={mirror()}
            disabled={busy()}
            onChange={(v) => {
              setMirror(v);
              recalibrate();
            }}
          />
          <Checkbox label="Desplazar el cuerpo" checked={rootMotion()} onChange={setRootMotion} />
        </Show>
      </div>
    </div>
  );
};
