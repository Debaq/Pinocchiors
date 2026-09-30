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

/** Puntos del actor en cada cuadro del video (`null` donde no hay nadie) */
export async function detectVideo(file: File, options: DetectOptions): Promise<(CaptureFrame | null)[]> {
  const { PoseLandmarker } = await import("@mediapipe/tasks-vision");
  const create = (delegate: "GPU" | "CPU") =>
    PoseLandmarker.createFromOptions(
      { wasmLoaderPath, wasmBinaryPath },
      {
        baseOptions: options.model ? { modelAssetBuffer: options.model, delegate } : { modelAssetPath: POSE_MODEL_URL, delegate },
        runningMode: "VIDEO",
        numPoses: 1,
      }
    );
  // En Linux (WebKitGTK) la GPU del webview puede no estar: se cae a CPU
  const landmarker = await create("GPU").catch(() => create("CPU"));
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
      const result = landmarker.detectForVideo(video, Math.round((i * 1000) / fps));
      const world = result.worldLandmarks[0];
      const image = result.landmarks[0];
      frames.push(world && image ? { world: world.map((p) => ({ ...p })), image: image.map((p) => ({ ...p })) } : null);
      options.onProgress?.(i + 1, total);
    }
    return frames;
  } finally {
    landmarker.close();
    video.removeAttribute("src");
    URL.revokeObjectURL(url);
  }
}
