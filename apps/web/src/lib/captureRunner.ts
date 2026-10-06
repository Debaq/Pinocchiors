/**
 * Detección de la pose del actor en un video con MediaPipe Pose Landmarker
 * (Apache 2.0). Se carga recién al usarla: el WASM viaja con la app y el
 * modelo se baja la primera vez de Google (o se elige un `.task` local, sin
 * conexión). El video se recorre cuadro a cuadro, sin tiempo real, así el
 * resultado no depende de la velocidad del equipo.
 */

import wasmLoaderPath from "@mediapipe/tasks-vision/vision_wasm_internal.js?url";
import wasmBinaryPath from "@mediapipe/tasks-vision/vision_wasm_internal.wasm?url";
import type { CaptureFrame } from "./capture";

/** Modelo completo de pose (mejor que el liviano, alcanza para captura offline) */
export const POSE_MODEL_URL =
  "https://storage.googleapis.com/mediapipe-models/pose_landmarker/pose_landmarker_full/float16/latest/pose_landmarker_full.task";

export interface DetectOptions {
  fps: number;
  /** Modelo `.task` local; sin él, se baja de `POSE_MODEL_URL` */
  model?: Uint8Array;
  onProgress?: (done: number, total: number) => void;
  signal?: AbortSignal;
}

const once = (target: EventTarget, event: string) =>
  new Promise<void>((resolve, reject) => {
    const ok = () => {
      target.removeEventListener("error", fail);
      resolve();
    };
    const fail = () => {
      target.removeEventListener(event, ok);
      reject(new Error("No se pudo leer el video (formato no soportado por el visor)"));
    };
    target.addEventListener(event, ok, { once: true });
    target.addEventListener("error", fail, { once: true });
  });

async function createLandmarker(model?: Uint8Array) {
  const { PoseLandmarker } = await import("@mediapipe/tasks-vision");
  const create = (delegate: "GPU" | "CPU") =>
    PoseLandmarker.createFromOptions(
      { wasmLoaderPath, wasmBinaryPath },
      {
        baseOptions: model ? { modelAssetBuffer: model, delegate } : { modelAssetPath: POSE_MODEL_URL, delegate },
        runningMode: "VIDEO",
        numPoses: 1,
      }
    );
  // En Linux (WebKitGTK) la GPU del webview puede no estar: se cae a CPU
  return create("GPU").catch(() => create("CPU"));
}

type Result = { worldLandmarks: { x: number; y: number; z: number; visibility?: number }[][]; landmarks: { x: number; y: number; z: number; visibility?: number }[][] };
const toFrame = (result: Result): CaptureFrame | null => {
  const world = result.worldLandmarks[0];
  const image = result.landmarks[0];
  return world && image ? { world: world.map((p) => ({ ...p })), image: image.map((p) => ({ ...p })) } : null;
};

/** Captura en vivo desde la cámara: vista previa con los puntos y grabación */
export interface LiveCapture {
  video: HTMLVideoElement;
  /** Puntos del último cuadro (para dibujarlos sobre la vista previa) */
  last: () => CaptureFrame | null;
  startRecording: () => void;
  /** Termina la grabación: cuadros grabados y su ritmo medio */
  stopRecording: () => { frames: (CaptureFrame | null)[]; fps: number };
  close: () => void;
}

/** `onFrame` recibe cada cuadro detectado con su tiempo en segundos (para seguir al actor en vivo) */
export async function startLive(model?: Uint8Array, onFrame?: (frame: CaptureFrame | null, time: number) => void): Promise<LiveCapture> {
  if (!navigator.mediaDevices?.getUserMedia) {
    throw new Error("Este visor no da acceso a la cámara (en Linux, WebKitGTK puede tenerlo desactivado): graba un video y usa el archivo");
  }
  const stream = await navigator.mediaDevices.getUserMedia({ video: { width: 1280, height: 720 }, audio: false }).catch((e) => {
    throw new Error(`No se pudo abrir la cámara: ${e instanceof Error ? e.message : e}`);
  });
  const landmarker = await createLandmarker(model).catch((e) => {
    stream.getTracks().forEach((t) => t.stop());
    throw e;
  });
  const video = document.createElement("video");
  video.muted = true;
  video.playsInline = true;
  video.srcObject = stream;
  await video.play();
  let last: CaptureFrame | null = null;
  let recording: { frames: (CaptureFrame | null)[]; start: number } | null = null;
  let lastTime = -1;
  let raf = 0;
  const loop = () => {
    raf = requestAnimationFrame(loop);
    if (video.readyState < 2 || video.currentTime === lastTime) return;
    lastTime = video.currentTime;
    last = toFrame(landmarker.detectForVideo(video, performance.now()) as unknown as Result);
    recording?.frames.push(last);
    onFrame?.(last, lastTime);
  };
  raf = requestAnimationFrame(loop);
  return {
    video,
    last: () => last,
    startRecording: () => (recording = { frames: [], start: performance.now() }),
    stopRecording: () => {
      const r = recording ?? { frames: [], start: performance.now() };
      recording = null;
      const seconds = Math.max(1e-3, (performance.now() - r.start) / 1000);
      return { frames: r.frames, fps: Math.max(1, Math.round(r.frames.length / seconds)) };
    },
    close: () => {
      cancelAnimationFrame(raf);
      landmarker.close();
      stream.getTracks().forEach((t) => t.stop());
      video.srcObject = null;
    },
  };
}

/** Puntos del actor en cada cuadro del video (`null` donde no hay nadie) */
export async function detectVideo(file: File, options: DetectOptions): Promise<(CaptureFrame | null)[]> {
  const landmarker = await createLandmarker(options.model);
  const url = URL.createObjectURL(file);
  const video = document.createElement("video");
  video.muted = true;
  video.playsInline = true;
  video.preload = "auto";
  try {
    video.src = url;
    await once(video, "loadedmetadata");
    const fps = options.fps > 0 ? options.fps : 30;
    const total = Math.max(1, Math.floor(video.duration * fps));
    const frames: (CaptureFrame | null)[] = [];
    for (let i = 0; i < total; i++) {
      if (options.signal?.aborted) throw new DOMException("Captura cancelada", "AbortError");
      video.currentTime = Math.min(video.duration, i / fps);
      await once(video, "seeked");
      frames.push(toFrame(landmarker.detectForVideo(video, Math.round((i * 1000) / fps)) as unknown as Result));
      options.onProgress?.(i + 1, total);
    }
    return frames;
  } finally {
    landmarker.close();
    video.removeAttribute("src");
    URL.revokeObjectURL(url);
  }
}
