import type { LightSettings } from "./lightRig";
import { t } from "./i18n.ts";
import type { RenderStillOptions, StillImage } from "./renderStill";

/** Lo que el visor le presta a GeppettoRay: la cámara tal como se ve y sus luces */
export interface RayView {
  position: [number, number, number];
  target: [number, number, number];
  up: [number, number, number];
  /** Campo de visión vertical (grados) a la relación de aspecto de la vista */
  fov: number;
  viewAspect: number;
  lights: LightSettings;
}

/** Cómo va el render: fracción total y lo que se muestra al lado de la barra */
export interface RayProgress {
  fraction: number;
  /** Animación: cuadro en curso (1 = el primero) y total; 0 en una imagen suelta */
  frame: number;
  frames: number;
  pass: number;
  passes: number;
  samples: number;
  totalSamples: number;
  elapsedSecs: number;
  etaSecs: number;
}

export interface RayExtras {
  /** Muestras por píxel */
  samples: number;
  denoise: boolean;
  /** Imagen HDR de entorno (ruta a un .hdr); sin ella, el cielo uniforme del visor */
  hdri?: { path: string; intensity: number; rotationDeg: number };
}

/** Radio angular del disco del sol: sombras con una penumbra leve, como las del visor */
export const SUN_ANGULAR_RADIUS = 0.035;
/** Opacidad máxima de la sombra sobre el suelo invisible */
export const SHADOW_FLOOR_STRENGTH = 0.7;

type Invoke = <T>(cmd: string, args?: unknown, options?: unknown) => Promise<T>;

interface StatusDto {
  phase: "idle" | "preparing" | "running" | "done" | "cancelled" | "error";
  pass: number;
  passes: number;
  samples: number;
  totalSamples: number;
  elapsedSecs: number;
  etaSecs: number;
  version: number;
  error: string | null;
  frame: number;
  frames: number;
  fraction: number;
  skipped: number;
  video: string | null;
  note: string | null;
}

const sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

/** #rrggbb (sRGB) → componentes lineales */
export function hexToLinear(hex: string): [number, number, number] {
  const n = parseInt(hex.replace("#", "").padEnd(6, "0").slice(0, 6), 16) || 0;
  const lin = (c: number) => {
    const v = c / 255;
    return v <= 0.04045 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4);
  };
  return [lin((n >> 16) & 255), lin((n >> 8) & 255), lin(n & 255)];
}

/** Dirección hacia la luz, como en `LightRig` (acimut alrededor del modelo y elevación) */
function lightDirection(azimuthDeg: number, elevationDeg: number): [number, number, number] {
  const a = (azimuthDeg * Math.PI) / 180;
  const e = (elevationDeg * Math.PI) / 180;
  return [Math.cos(e) * Math.sin(a), Math.sin(e), Math.cos(e) * Math.cos(a)];
}

const scale = (c: [number, number, number], k: number): [number, number, number] => [c[0] * k, c[1] * k, c[2] * k];

/**
 * Pasa las luces del visor a las de GeppettoRay. Las direccionales de Three y las del motor
 * comparten la unidad (la intensidad es la irradiancia), así que la principal y el relleno
 * pasan tal cual. El ambiente y el entorno del visor se resumen en un cielo uniforme
 * (aproximado: el visor usa una imagen de estudio para los reflejos).
 */
export function raySun(lights: LightSettings, forward: [number, number, number]) {
  const suns = [
    { direction: lightDirection(lights.azimuth, lights.elevation), color: scale(hexToLinear(lights.color), lights.intensity) },
  ];
  if (lights.fill > 0) {
    suns.push({ direction: lightDirection(lights.azimuth + 180, 20), color: scale(hexToLinear("#8be9fd"), lights.fill) });
  }
  if (lights.headlight) {
    // Luz de cámara: sale de donde mira la cámara hacia el modelo
    suns.push({ direction: [-forward[0], -forward[1], -forward[2]], color: scale(hexToLinear(lights.color), 0.6 * lights.intensity) });
  }
  const ambient = ((lights.ambient * 0.64) / Math.PI + lights.environment * 0.25);
  return { suns, ambient: [ambient, ambient, ambient] as [number, number, number] };
}

/** Campo de visión vertical que sale en una imagen de otra relación de aspecto (se recorta al centro) */
export function imageFov(viewFov: number, viewAspect: number, imageAspect: number): number {
  if (imageAspect <= viewAspect) return viewFov;
  return (2 * Math.atan((Math.tan((viewFov * Math.PI) / 360) * viewAspect) / imageAspect) * 180) / Math.PI;
}

function pngToCanvas(bytes: Uint8Array): Promise<HTMLImageElement> {
  const url = URL.createObjectURL(new Blob([bytes as BlobPart], { type: "image/png" }));
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => {
      URL.revokeObjectURL(url);
      resolve(img);
    };
    img.onerror = () => {
      URL.revokeObjectURL(url);
      reject(new Error(t("No se pudo crear la imagen")));
    };
    img.src = url;
  });
}

const MIME = { png: "image/png", jpeg: "image/jpeg", webp: "image/webp" } as const;

/** PNG con alfa → imagen final: pone el degradado debajo y/o cambia de formato */
async function finish(png: Uint8Array, opts: RenderStillOptions, gradient: boolean): Promise<StillImage> {
  const width = opts.width;
  const height = opts.height;
  // El PNG de GeppettoRay ya es la imagen final: no hace falta decodificarlo
  if (opts.format === "png" && !gradient) return { bytes: png, width, height };
  const img = await pngToCanvas(png);
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error(t("No se pudo crear la imagen"));
  if (gradient) {
    const fill = ctx.createLinearGradient(0, 0, 0, height);
    fill.addColorStop(0, opts.color);
    fill.addColorStop(1, opts.color2);
    ctx.fillStyle = fill;
    ctx.fillRect(0, 0, width, height);
  }
  ctx.drawImage(img, 0, 0);
  const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, MIME[opts.format], opts.quality));
  canvas.width = canvas.height = 0;
  if (!blob || blob.type !== MIME[opts.format]) {
    throw new Error(t("Esta ventana no puede guardar {f}; prueba con PNG", { f: opts.format.toUpperCase() }));
  }
  return { bytes: new Uint8Array(await blob.arrayBuffer()), width, height };
}

/** La petición común del motor: cámara, luces, fondo y calidad */
function buildRequest(view: RayView, opts: RenderStillOptions, extras: RayExtras) {
  const aspect = opts.width / opts.height;
  const forward: [number, number, number] = [
    view.target[0] - view.position[0],
    view.target[1] - view.position[1],
    view.target[2] - view.position[2],
  ];
  const { suns, ambient } = raySun(view.lights, forward);
  const gradient = opts.background === "gradient";
  // JPG no guarda alfa: el fondo "transparente" sale blanco, como en el render del visor
  const transparent = (opts.background === "transparent" && opts.format !== "jpeg") || gradient;
  const background = transparent
    ? null
    : opts.background === "color" || opts.background === "transparent"
      ? hexToLinear(opts.background === "color" ? opts.color : "#ffffff")
      : null;
  const request = {
    width: opts.width,
    height: opts.height,
    samples: extras.samples,
    passes: Math.min(extras.samples, 12),
    maxDepth: 6,
    denoise: extras.denoise,
    transparent,
    background,
    camera: { position: view.position, target: view.target, up: view.up, vfovDeg: imageFov(view.fov, view.viewAspect, aspect) },
    suns,
    ambient,
    sunAngularRadius: SUN_ANGULAR_RADIUS,
    // Misma curva de tono que el visor, para que el motor y el visor den el mismo tono
    toneMap: true,
    shadowFloor: opts.groundShadow ? SHADOW_FLOOR_STRENGTH : null,
    hdri: extras.hdri ?? null,
  };
  return { request, gradient };
}

/**
 * Sigue el trabajo en curso hasta que termina: avisa el avance, entrega la vista previa cada
 * vez que cambia y cancela con `signal`. Devuelve el estado final.
 */
async function follow(
  invoke: Invoke,
  onProgress?: (p: RayProgress) => void,
  signal?: AbortSignal,
  onPreview?: (png: Uint8Array) => void,
): Promise<StatusDto> {
  let seen = 0;
  let cancelSent = false;
  for (;;) {
    if (signal?.aborted && !cancelSent) {
      cancelSent = true;
      await invoke<void>("ray_render_cancel");
    }
    const st = await invoke<StatusDto>("ray_render_status");
    if (st.error) throw new Error(st.error);
    if (st.phase === "running" || st.phase === "done" || st.phase === "cancelled") {
      onProgress?.({
        fraction: st.frames > 0 ? st.fraction : st.totalSamples > 0 ? st.samples / st.totalSamples : 0,
        frame: st.frames > 0 ? st.frame : 0,
        frames: st.frames,
        pass: st.pass,
        passes: st.passes,
        samples: st.samples,
        totalSamples: st.totalSamples,
        elapsedSecs: st.elapsedSecs,
        etaSecs: st.etaSecs,
      });
    }
    if (st.version !== seen && onPreview && st.phase !== "cancelled") {
      seen = st.version;
      onPreview(new Uint8Array(await invoke<ArrayBuffer>("ray_render_image")));
    }
    if (st.phase === "cancelled") throw new DOMException("cancelado", "AbortError");
    if (st.phase === "done") return st;
    if (st.phase === "idle") throw new Error(t("El render se detuvo sin terminar"));
    await sleep(150);
  }
}

/**
 * Renderiza con GeppettoRay: el motor corre en segundo plano en Rust y esta función sigue
 * su avance. `onPreview` recibe la imagen (PNG) cada vez que una pasada la afina.
 * Con `signal` se cancela (lanza `AbortError`, como el render del visor).
 */
export async function renderRay(
  invoke: Invoke,
  view: RayView,
  opts: RenderStillOptions,
  extras: RayExtras,
  onProgress?: (p: RayProgress) => void,
  signal?: AbortSignal,
  onPreview?: (png: Uint8Array) => void,
): Promise<StillImage> {
  const { request, gradient } = buildRequest(view, opts, extras);
  await invoke<void>("ray_render_start", { request });
  await follow(invoke, onProgress, signal, onPreview);
  return finish(new Uint8Array(await invoke<ArrayBuffer>("ray_render_image")), opts, gradient);
}

/** Lo que pide una animación: qué clip, qué cuadros y dónde van */
export interface RayAnimation {
  /** Clips ya horneados, como los de la exportación */
  clips: unknown[];
  clip: number;
  firstFrame: number;
  lastFrame: number;
  fps: number;
  dir: string;
  name: string;
  /** Fracción del cuadro con el obturador abierto; 0 = sin desenfoque de movimiento */
  shutter: number;
  motionSamples: number;
  /** Saltar los cuadros que ya están en la carpeta */
  resume: boolean;
  /** Armar un video con ffmpeg si está instalado */
  video: boolean;
  nonDeforming: number[];
  useRetopology: boolean;
}

export interface RayAnimationResult {
  frames: number;
  /** Cuadros que ya existían y se saltaron */
  skipped: number;
  /** Ruta del video armado, si lo hubo */
  video: string | null;
  /** Aviso del motor (por qué no hay video) */
  note: string | null;
}

/** #rrggbb → [r, g, b] de 8 bits */
function hexToRgb8(hex: string): [number, number, number] {
  const n = parseInt(hex.replace("#", "").padEnd(6, "0").slice(0, 6), 16) || 0;
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

/**
 * Renderiza una animación con GeppettoRay: un PNG por cuadro en `anim.dir`. Devuelve al
 * terminar; con `signal` se cancela (los cuadros hechos quedan y se pueden reanudar).
 */
export async function renderRayAnimation(
  invoke: Invoke,
  view: RayView,
  opts: RenderStillOptions,
  extras: RayExtras,
  anim: RayAnimation,
  onProgress?: (p: RayProgress) => void,
  signal?: AbortSignal,
  onPreview?: (png: Uint8Array) => void,
): Promise<RayAnimationResult> {
  const { request, gradient } = buildRequest(view, { ...opts, format: "png" }, extras);
  await invoke<void>("ray_render_animation_start", {
    request,
    animation: { ...anim, gradient: gradient ? { top: hexToRgb8(opts.color), bottom: hexToRgb8(opts.color2) } : null },
  });
  const st = await follow(invoke, onProgress, signal, onPreview);
  return { frames: st.frames, skipped: st.skipped, video: st.video, note: st.note };
}

/** "1:05" o "12 s" para el tiempo que falta */
export function formatEta(secs: number): string {
  if (!Number.isFinite(secs) || secs < 0) return "";
  const s = Math.round(secs);
  if (s < 60) return `${s} s`;
  const m = Math.floor(s / 60);
  return `${m}:${String(s % 60).padStart(2, "0")}`;
}
