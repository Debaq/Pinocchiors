// Aceleración GPU (experimental, PLAN_RENDIMIENTO R12): con la opción
// encendida, el visor principal dibuja con `WebGPURenderer` de three (WebGPU
// donde el navegador lo trae; si no, su respaldo en WebGL2). Apagada, todo
// queda como siempre con `WebGLRenderer`: este módulo ni siquiera carga
// `three/webgpu` (va en un pedazo aparte del paquete).
//
// Lo que el dibujador nuevo no tiene se resuelve aparte: el render de imagen y
// el halo de selección siguen con WebGL; los parches de shader propios
// (cuaterniones duales, suavizado automático) no se aplican.

import * as THREE from "three";
import { createSignal } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { gpuAcceleration } from "./settings";

type GpuModule = typeof import("three/webgpu");
type GpuLines = typeof import("three/addons/lines/webgpu/LineSegments2.js");

let gpu: GpuModule | null = null;
let gpuLines: GpuLines | null = null;
let gpuLine2: typeof import("three/addons/lines/webgpu/Line2.js") | null = null;
const [backend, setBackend] = createSignal<"webgpu" | "webgl2" | null>(null);
const [failure, setFailure] = createSignal<string | null>(null);

const RELOAD_KEY = "pinocchio.webgpuReload";

/**
 * Antes de montar la interfaz: con la opción encendida, enciende WebGPU en el
 * WebView si hace falta (Linux: WebKitGTK lo trae apagado; se recarga una vez)
 * y carga el dibujador nuevo
 */
export async function startGpu(): Promise<void> {
  if (!gpuAcceleration()) return;
  if (!("gpu" in navigator)) {
    const enabled = await invoke<boolean>("enable_webgpu").catch(() => false);
    let reloaded = false;
    try {
      reloaded = sessionStorage.getItem(RELOAD_KEY) === "1";
      if (enabled && !reloaded) sessionStorage.setItem(RELOAD_KEY, "1");
    } catch {
      reloaded = true;
    }
    if (enabled && !reloaded) {
      location.reload();
      await new Promise(() => {});
    }
  }
  try {
    [gpu, gpuLines, gpuLine2] = await Promise.all([import("three/webgpu"), import("three/addons/lines/webgpu/LineSegments2.js"), import("three/addons/lines/webgpu/Line2.js")]);
  } catch (e) {
    gpu = null;
    gpuLines = null;
    gpuLine2 = null;
    setFailure(String(e));
    console.error("Aceleración GPU: no se pudo cargar el dibujador nuevo", e);
  }
}

/** El visor dibuja con el dibujador nuevo */
export const usingGpuRenderer = (): boolean => gpu !== null;

/** Este dibujador concreto es el nuevo (otros visores, como el de CAD, siguen en WebGL aunque la opción esté encendida) */
export const isGpuRenderer = (renderer: object): boolean => (renderer as { isWebGPURenderer?: boolean }).isWebGPURenderer === true;

/** Con qué dibuja el dibujador nuevo (cuando ya arrancó), o por qué no se pudo usar */
export const gpuStatus = (): { backend: "webgpu" | "webgl2" | null; failure: string | null } => ({ backend: backend(), failure: failure() });

const ready = new WeakMap<object, Promise<void>>();

/**
 * Dibujador del visor: `WebGLRenderer` como siempre o, con la aceleración GPU,
 * `WebGPURenderer` con la forma del de WebGL que usa el visor. Hasta que
 * arranca (es asíncrono) no dibuja ni limpia nada; `onReady` avisa para pedir
 * el primer cuadro
 */
export function createViewRenderer(params: THREE.WebGLRendererParameters & { canvas: HTMLCanvasElement }, onReady: () => void): THREE.WebGLRenderer {
  if (!gpu) {
    const renderer = new THREE.WebGLRenderer(params);
    ready.set(renderer, Promise.resolve());
    return renderer;
  }
  // Con los mismos valores por defecto que WebGL (el nuevo trae el lienzo transparente)
  const renderer = new gpu.WebGPURenderer({ canvas: params.canvas, antialias: params.antialias ?? false, alpha: params.alpha ?? false, stencil: params.stencil ?? false });
  let initialized = false;
  // Antes de arrancar, dibujar o limpiar tira error: se saltan esos cuadros
  const guard = <K extends "render" | "clear" | "clearColor" | "clearDepth" | "clearStencil">(name: K) => {
    const original = (renderer[name] as (...args: unknown[]) => unknown).bind(renderer);
    (renderer as unknown as Record<string, unknown>)[name] = (...args: unknown[]) => (initialized ? original(...args) : undefined);
  };
  for (const name of ["render", "clear", "clearColor", "clearDepth", "clearStencil"] as const) guard(name);
  // Lo del de WebGL que usa el visor y este no tiene con ese nombre
  const shim = renderer as unknown as Record<string, unknown>;
  shim.capabilities = { getMaxAnisotropy: () => renderer.getMaxAnisotropy(), maxTextureSize: 8192 };
  shim.forceContextLoss = () => {};
  shim.localClippingEnabled = true;
  const started = renderer.init().then(
    () => {
      initialized = true;
      const b = (renderer as unknown as { backend?: { isWebGPUBackend?: boolean } }).backend;
      setBackend(b?.isWebGPUBackend ? "webgpu" : "webgl2");
      onReady();
    },
    (e) => {
      setFailure(String(e));
      console.error("Aceleración GPU: el dibujador nuevo no arrancó", e);
    },
  );
  ready.set(renderer, started);
  return renderer as unknown as THREE.WebGLRenderer;
}

/** Se resuelve cuando el dibujador ya puede dibujar (al tiro con WebGL) */
export const rendererReady = (renderer: THREE.WebGLRenderer): Promise<void> => ready.get(renderer) ?? Promise.resolve();

/** Mapa de entorno (reflejos) armado con el dibujador que corresponde; espera a que arranque */
export async function environmentFromScene(renderer: THREE.WebGLRenderer, scene: THREE.Scene, sigma: number): Promise<THREE.Texture> {
  await rendererReady(renderer);
  const pmrem = gpu && isGpuRenderer(renderer) ? new gpu.PMREMGenerator(renderer as unknown as InstanceType<GpuModule["WebGPURenderer"]>) : new THREE.PMREMGenerator(renderer);
  const texture = pmrem.fromScene(scene, sigma).texture;
  pmrem.dispose();
  return texture;
}

/** Material de líneas gruesas del dibujador que corresponde (LineMaterial o Line2NodeMaterial) */
export function thickLineMaterial(
  params: { vertexColors?: boolean; worldUnits?: boolean; linewidth?: number; resolution?: THREE.Vector2 },
  WebGlMaterial: typeof import("three/addons/lines/LineMaterial.js").LineMaterial,
): ThickLineMaterial {
  if (gpu) {
    // Sin `resolution`: el material de nodos toma el tamaño del dibujador
    const options: { vertexColors?: boolean; worldUnits?: boolean; linewidth?: number } = {};
    if (params.vertexColors !== undefined) options.vertexColors = params.vertexColors;
    if (params.worldUnits !== undefined) options.worldUnits = params.worldUnits;
    if (params.linewidth !== undefined) options.linewidth = params.linewidth;
    return new gpu.Line2NodeMaterial(options) as unknown as ThickLineMaterial;
  }
  return new WebGlMaterial(params);
}

/** Material de líneas gruesas: `resolution` solo existe en el de WebGL */
export type ThickLineMaterial = THREE.Material & { linewidth: number; resolution?: THREE.Vector2 };

/** Líneas gruesas (LineSegments2) de la clase del dibujador que corresponde */
export function thickLineSegments(
  geometry: import("three/addons/lines/LineSegmentsGeometry.js").LineSegmentsGeometry,
  material: ThickLineMaterial,
  WebGlLines: typeof import("three/addons/lines/LineSegments2.js").LineSegments2,
): THREE.Mesh {
  if (gpu && gpuLines) return new gpuLines.LineSegments2(geometry, material as never) as unknown as THREE.Mesh;
  return new WebGlLines(geometry, material as never);
}

type LineParams = ConstructorParameters<typeof import("three/addons/lines/LineMaterial.js").LineMaterial>[0];

/**
 * Para los visores que dibujan líneas gruesas con todas las opciones (color, punteado, transparencia,
 * profundidad): el material del dibujador de ESE visor. `renderer` es el del visor (CAD sigue en WebGL)
 */
export function lineMaterialFor(renderer: object, params: NonNullable<LineParams>, WebGlMaterial: typeof import("three/addons/lines/LineMaterial.js").LineMaterial): import("three/addons/lines/LineMaterial.js").LineMaterial {
  if (gpu && isGpuRenderer(renderer)) {
    const { resolution: _resolution, ...rest } = params;
    return new gpu.Line2NodeMaterial(rest as never) as unknown as import("three/addons/lines/LineMaterial.js").LineMaterial;
  }
  return new WebGlMaterial(params);
}

/** Polilínea gruesa (Line2) o segmentos gruesos (LineSegments2) de la clase que corresponde al visor */
export function thickMeshFor(
  renderer: object,
  kind: "line" | "segments",
  geometry: object,
  material: object,
  WebGl: { line: typeof import("three/addons/lines/Line2.js").Line2; segments: typeof import("three/addons/lines/LineSegments2.js").LineSegments2 },
): import("three/addons/lines/LineSegments2.js").LineSegments2 {
  if (gpu && gpuLines && gpuLine2 && isGpuRenderer(renderer)) {
    const C = kind === "line" ? gpuLine2.Line2 : gpuLines.LineSegments2;
    return new C(geometry as never, material as never) as unknown as import("three/addons/lines/LineSegments2.js").LineSegments2;
  }
  const C = kind === "line" ? WebGl.line : WebGl.segments;
  return new C(geometry as never, material as never);
}

/** ¿Es un objeto de líneas gruesas, de cualquiera de los dos dibujadores? */
export const isThickLines = (o: unknown): o is import("three/addons/lines/LineSegments2.js").LineSegments2 => (o as { isLineSegments2?: boolean })?.isLineSegments2 === true;

/** Tamaño de la vista para el material de WebGL; el de nodos lo toma del dibujador */
export function setLineResolution(material: object, w: number, h: number) {
  const m = material as { resolution?: THREE.Vector2 };
  if (m.resolution) m.resolution.set(w, h);
}
