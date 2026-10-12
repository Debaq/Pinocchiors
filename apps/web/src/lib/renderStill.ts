import * as THREE from "three";
import { GTAOPass } from "three/addons/postprocessing/GTAOPass.js";
import { GlowHalo } from "./glowHalo";
import { t } from "./i18n.ts";

export type StillFormat = "png" | "jpeg" | "webp";
export type StillBackground = "transparent" | "viewer" | "color";

/** Lo que pide el diálogo de Render */
export interface RenderStillOptions {
  width: number;
  height: number;
  background: StillBackground;
  /** Color del fondo cuando `background` es "color" (#rrggbb) */
  color: string;
  /** Cuadros sacudidos medio píxel que se promedian: 1, 4 o 16 */
  samples: number;
  format: StillFormat;
  /** Calidad de JPEG y WebP, de 0 a 1 */
  quality: number;
  hideGrid: boolean;
  hideSkeleton: boolean;
  hideGizmos: boolean;
  hideSelection: boolean;
  /** Sombras de la luz principal, suavizadas moviéndola un poco en cada cuadro */
  shadows: boolean;
  /** Oclusión ambiental (GTAO): oscurece rincones y contactos */
  ambientOcclusion: boolean;
}

export interface StillImage {
  bytes: Uint8Array;
  width: number;
  height: number;
}

/** Lo que el visor le presta al render */
export interface StillSource {
  renderer: THREE.WebGLRenderer;
  scene: THREE.Scene;
  camera: THREE.PerspectiveCamera;
  /** Relación de aspecto de la vista en pantalla, para recortar alrededor del centro */
  viewAspect: number;
  /** La luz principal y la escena que ilumina: centro y tamaño (diagonal) del modelo */
  key: { light: THREE.DirectionalLight; center: THREE.Vector3; radius: number };
  /** Dibuja el halo del glow sobre el cuadro ya dibujado en `target`; null si no hay glow */
  glow: ((halo: GlowHalo, target: THREE.WebGLRenderTarget) => void) | null;
}

/** Lado máximo de un mosaico: más grande pasaría de la memoria de video razonable */
const TILE = 2048;
/** Lado máximo de la imagen final */
export const MAX_STILL_SIDE = 16384;

const MIME: Record<StillFormat, string> = { png: "image/png", jpeg: "image/jpeg", webp: "image/webp" };

/** Sumador: copia una textura multiplicada por `weight` encima de lo que hay (mezcla aditiva) */
function blitMaterial(additive: boolean): THREE.ShaderMaterial {
  return new THREE.ShaderMaterial({
    uniforms: { tMap: { value: null }, weight: { value: 1 } },
    vertexShader: "varying vec2 vUv; void main() { vUv = uv; gl_Position = vec4(position.xy, 0.0, 1.0); }",
    fragmentShader: "varying vec2 vUv; uniform sampler2D tMap; uniform float weight; void main() { gl_FragColor = texture2D(tMap, vUv) * weight; }",
    blending: additive ? THREE.CustomBlending : THREE.NoBlending,
    blendEquation: THREE.AddEquation,
    blendSrc: THREE.OneFactor,
    blendDst: THREE.OneFactor,
    blendSrcAlpha: THREE.OneFactor,
    blendDstAlpha: THREE.OneFactor,
    depthTest: false,
    depthWrite: false,
  });
}

/** Posiciones de la rejilla de cuadros dentro del píxel, de -0.5 a 0.5 */
function jitter(samples: number): [number, number][] {
  const k = Math.max(1, Math.round(Math.sqrt(samples)));
  const out: [number, number][] = [];
  for (let i = 0; i < k * k; i++) out.push(k === 1 ? [0, 0] : [((i % k) + 0.5) / k - 0.5, (Math.floor(i / k) + 0.5) / k - 0.5]);
  return out;
}

/** Luz principal desviada `angle` radianes en la dirección `turn`, a la misma distancia del centro */
function tilted(base: THREE.Vector3, center: THREE.Vector3, angle: number, turn: number): THREE.Vector3 {
  const v = base.clone().sub(center);
  const length = v.length();
  const up = Math.abs(v.y) < 0.99 * length ? new THREE.Vector3(0, 1, 0) : new THREE.Vector3(1, 0, 0);
  const u = new THREE.Vector3().crossVectors(v, up).normalize();
  const w = new THREE.Vector3().crossVectors(v, u).normalize();
  const offset = u.multiplyScalar(Math.cos(turn)).add(w.multiplyScalar(Math.sin(turn))).multiplyScalar(length * Math.tan(angle));
  return center.clone().add(v.add(offset).setLength(length));
}

const nextTick = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

/** Cuadro (en píxeles de la vista) que sale en la imagen: el de esa relación de aspecto, centrado y lo más grande posible */
export function renderFrameRect(viewW: number, viewH: number, aspect: number): { x: number; y: number; w: number; h: number } {
  const w = aspect >= viewW / viewH ? viewW : viewH * aspect;
  const h = aspect >= viewW / viewH ? viewW / aspect : viewH;
  return { x: (viewW - w) / 2, y: (viewH - h) / 2, w, h };
}

/**
 * Render fuera de pantalla de lo que ve la cámara del visor, al tamaño pedido.
 *
 * - Mosaicos de hasta 2048 px (`setViewOffset`), así una imagen grande no pide una textura enorme.
 * - Supersampling: promedia varios cuadros con la cámara sacudida menos de un píxel (más 4x MSAA del propio cuadro).
 * - Los cuadros se dibujan en un target marcado como "XR" a propósito: es la única forma de que three le
 *   aplique el tono y la codificación sRGB del visor fuera de pantalla, y así el fondo, el glow y los
 *   colores salen igual que en la ventana.
 */
export async function renderStillImage(
  src: StillSource,
  opts: RenderStillOptions,
  onProgress?: (fraction: number) => void,
  signal?: AbortSignal,
): Promise<StillImage> {
  const { renderer, scene, camera } = src;
  const W = Math.max(1, Math.min(MAX_STILL_SIDE, Math.round(opts.width)));
  const H = Math.max(1, Math.min(MAX_STILL_SIDE, Math.round(opts.height)));
  const tw = Math.min(TILE, W);
  const th = Math.min(TILE, H);
  const tilesX = Math.ceil(W / tw);
  const tilesY = Math.ceil(H / th);
  const offsets = jitter(opts.samples);
  const total = tilesX * tilesY * offsets.length;
  // Sin canal alfa en JPEG: el fondo transparente sale blanco
  const transparent = opts.background === "transparent" && opts.format !== "jpeg";

  const pixels = new Uint8ClampedArray(W * H * 4);

  // Estado del visor que se toca y se devuelve
  const savedCamera = { fov: camera.fov, aspect: camera.aspect, view: camera.view ? { ...camera.view } : null };
  const savedBackground = scene.background;
  const savedClear = renderer.getClearColor(new THREE.Color());
  const savedClearAlpha = renderer.getClearAlpha();
  const savedTarget = renderer.getRenderTarget();

  const type = THREE.HalfFloatType;
  const sample = new THREE.WebGLRenderTarget(tw, th, { type, samples: 4, colorSpace: THREE.SRGBColorSpace });
  (sample as unknown as { isXRRenderTarget: boolean }).isXRRenderTarget = true;
  const accum = new THREE.WebGLRenderTarget(tw, th, { type, depthBuffer: false });
  const out = new THREE.WebGLRenderTarget(tw, th, { type: THREE.UnsignedByteType, depthBuffer: false });
  const quadScene = new THREE.Scene();
  const quadCamera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0, 1);
  const add = blitMaterial(true);
  const copy = blitMaterial(false);
  const quad = new THREE.Mesh(new THREE.PlaneGeometry(2, 2), add);
  quad.frustumCulled = false;
  quadScene.add(quad);
  const halo = src.glow ? new GlowHalo(renderer, tw, th) : null;
  // Con oclusión ambiental el cuadro pasa por el GTAO antes del halo; el resultado va a otro target
  const sceneRadius = src.key.radius;
  let occlusion: GTAOPass | null = null;
  let occluded: THREE.WebGLRenderTarget | null = null;
  if (opts.ambientOcclusion) {
    occlusion = new GTAOPass(scene, camera, tw, th);
    occlusion.updateGtaoMaterial({ radius: sceneRadius * 0.1, distanceExponent: 1, thickness: sceneRadius * 0.05, scale: 1, samples: 32, distanceFallOff: 1, screenSpaceRadius: false });
    occlusion.updatePdMaterial({ lumaPhi: 10, depthPhi: 2, normalPhi: 3, radius: 10, radiusExponent: 1, rings: 3, samples: 24 });
    occluded = new THREE.WebGLRenderTarget(tw, th, { type, colorSpace: THREE.SRGBColorSpace, depthBuffer: false });
    (occluded as unknown as { isXRRenderTarget: boolean }).isXRRenderTarget = true;
  }
  const tile = new Uint8Array(tw * th * 4);
  const gl = renderer.getContext();
  const autoClear = renderer.autoClear;

  // Sombras: se prenden solo durante el render y se devuelven como estaban
  const light = src.key.light;
  const savedShadow = {
    enabled: renderer.shadowMap.enabled,
    type: renderer.shadowMap.type,
    cast: light.castShadow,
    position: light.position.clone(),
    mapSize: light.shadow.mapSize.clone(),
    bias: light.shadow.bias,
    normalBias: light.shadow.normalBias,
    camera: { l: light.shadow.camera.left, r: light.shadow.camera.right, t: light.shadow.camera.top, b: light.shadow.camera.bottom, n: light.shadow.camera.near, f: light.shadow.camera.far },
  };
  const savedMeshes: { mesh: THREE.Mesh; cast: boolean; receive: boolean }[] = [];
  /** Los materiales recompilan al cambiar las sombras */
  const refreshMaterials = () => {
    for (const { mesh } of savedMeshes) for (const m of Array.isArray(mesh.material) ? mesh.material : [mesh.material]) m.needsUpdate = true;
  };

  try {
    if (opts.shadows) {
      scene.traverseVisible((o) => {
        const mesh = o as THREE.Mesh;
        if (!mesh.isMesh) return;
        savedMeshes.push({ mesh, cast: mesh.castShadow, receive: mesh.receiveShadow });
        const materials = Array.isArray(mesh.material) ? mesh.material : [mesh.material];
        // El vidrio y lo translúcido no tapan la luz
        const solid = materials.every((m) => !m.transparent && !(m as THREE.MeshPhysicalMaterial).transmission);
        mesh.castShadow = solid;
        mesh.receiveShadow = true;
      });
      renderer.shadowMap.enabled = true;
      renderer.shadowMap.type = THREE.PCFSoftShadowMap;
      const reach = sceneRadius;
      const camera3 = light.shadow.camera;
      Object.assign(camera3, { left: -reach, right: reach, top: reach, bottom: -reach, near: reach * 0.4, far: reach * 3.6 });
      camera3.updateProjectionMatrix();
      const size = Math.min(4096, renderer.capabilities.maxTextureSize);
      light.shadow.mapSize.set(size, size);
      light.shadow.map?.dispose();
      light.shadow.map = null;
      light.shadow.bias = -0.0003;
      light.shadow.normalBias = sceneRadius * 0.004;
      light.castShadow = true;
      refreshMaterials();
    }

    // Recorte alrededor del centro: si la imagen es más ancha que la vista se conserva el ancho; si no, el alto
    const aspect = W / H;
    if (aspect > src.viewAspect) {
      camera.fov = THREE.MathUtils.radToDeg(2 * Math.atan((Math.tan(THREE.MathUtils.degToRad(camera.fov) / 2) * src.viewAspect) / aspect));
    }
    camera.aspect = aspect;

    if (transparent) {
      scene.background = null;
      renderer.setClearColor(0x000000, 0);
    } else if (opts.background === "color" || opts.background === "transparent") {
      scene.background = new THREE.Color(opts.background === "color" ? opts.color : "#ffffff");
    } else if (!(scene.background instanceof THREE.Color)) {
      scene.background = null;
      renderer.setClearColor(0x000000, 0);
    }

    let done = 0;
    for (let ty = 0; ty < tilesY; ty++) {
      for (let tx = 0; tx < tilesX; tx++) {
        renderer.setRenderTarget(accum);
        renderer.setClearColor(0x000000, 0);
        renderer.clear();
        for (let sampleIndex = 0; sampleIndex < offsets.length; sampleIndex++) {
          const [jx, jy] = offsets[sampleIndex];
          if (signal?.aborted) throw new DOMException(t("Render cancelado"), "AbortError");
          camera.setViewOffset(W, H, tx * tw + jx, ty * th + jy, tw, th);
          renderer.setRenderTarget(sample);
          if (transparent || !(scene.background instanceof THREE.Color)) renderer.setClearColor(0x000000, 0);
          renderer.autoClear = true;
          if (opts.shadows && offsets.length > 1) {
            // Disco de Vogel: la luz recorre un cono de ~3° y las sombras ganan penumbra
            const angle = 0.05 * Math.sqrt((sampleIndex + 0.5) / offsets.length);
            light.position.copy(tilted(savedShadow.position, src.key.center, angle, sampleIndex * 2.39996));
          }
          renderer.render(scene, camera);
          let drawn = sample;
          if (occlusion && occluded) {
            occlusion.render(renderer, occluded, sample, 0, false);
            drawn = occluded;
          }
          if (halo && src.glow) src.glow(halo, drawn);

          renderer.setRenderTarget(accum);
          renderer.autoClear = false;
          quad.material = add;
          add.uniforms.tMap.value = drawn.texture;
          add.uniforms.weight.value = 1 / offsets.length;
          renderer.render(quadScene, quadCamera);
          renderer.autoClear = true;

          gl.finish();
          onProgress?.(++done / total);
          await nextTick();
        }
        renderer.setRenderTarget(out);
        quad.material = copy;
        copy.uniforms.tMap.value = accum.texture;
        renderer.render(quadScene, quadCamera);
        renderer.readRenderTargetPixels(out, 0, 0, tw, th, tile);

        // Las filas del target van de abajo hacia arriba
        const w = Math.min(tw, W - tx * tw);
        const h = Math.min(th, H - ty * th);
        for (let row = 0; row < h; row++) {
          let from = ((th - 1 - row) * tw) * 4;
          let to = ((ty * th + row) * W + tx * tw) * 4;
          for (let col = 0; col < w; col++, from += 4, to += 4) {
            const a = tile[from + 3];
            if (a === 255) {
              pixels[to] = tile[from];
              pixels[to + 1] = tile[from + 1];
              pixels[to + 2] = tile[from + 2];
              pixels[to + 3] = 255;
            } else if (a > 0) {
              // Premultiplicado -> alfa recto
              pixels[to] = Math.min(255, (tile[from] * 255) / a);
              pixels[to + 1] = Math.min(255, (tile[from + 1] * 255) / a);
              pixels[to + 2] = Math.min(255, (tile[from + 2] * 255) / a);
              pixels[to + 3] = a;
            }
          }
        }
      }
    }
  } finally {
    renderer.autoClear = autoClear;
    camera.fov = savedCamera.fov;
    camera.aspect = savedCamera.aspect;
    camera.view = savedCamera.view;
    camera.updateProjectionMatrix();
    scene.background = savedBackground;
    renderer.setClearColor(savedClear, savedClearAlpha);
    renderer.setRenderTarget(savedTarget);
    if (opts.shadows) {
      light.position.copy(savedShadow.position);
      light.castShadow = savedShadow.cast;
      light.shadow.bias = savedShadow.bias;
      light.shadow.normalBias = savedShadow.normalBias;
      light.shadow.mapSize.copy(savedShadow.mapSize);
      const c = savedShadow.camera;
      Object.assign(light.shadow.camera, { left: c.l, right: c.r, top: c.t, bottom: c.b, near: c.n, far: c.f });
      light.shadow.camera.updateProjectionMatrix();
      light.shadow.map?.dispose();
      light.shadow.map = null;
      renderer.shadowMap.enabled = savedShadow.enabled;
      renderer.shadowMap.type = savedShadow.type;
      for (const m of savedMeshes) {
        m.mesh.castShadow = m.cast;
        m.mesh.receiveShadow = m.receive;
      }
      refreshMaterials();
    }
    occlusion?.dispose();
    occluded?.dispose();
    sample.dispose();
    accum.dispose();
    out.dispose();
    add.dispose();
    copy.dispose();
    quad.geometry.dispose();
    halo?.dispose();
  }

  const canvas = document.createElement("canvas");
  canvas.width = W;
  canvas.height = H;
  const context = canvas.getContext("2d");
  if (!context) throw new Error(t("No se pudo crear la imagen"));
  context.putImageData(new ImageData(pixels, W, H), 0, 0);
  const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, MIME[opts.format], opts.quality));
  canvas.width = canvas.height = 0;
  if (!blob || blob.type !== MIME[opts.format]) {
    throw new Error(t("Esta ventana no puede guardar {f}; prueba con PNG", { f: opts.format.toUpperCase() }));
  }
  return { bytes: new Uint8Array(await blob.arrayBuffer()), width: W, height: H };
}

/** Una vista ya renderizada (en PNG) para la hoja, con su rótulo */
export interface SheetCell {
  image: StillImage;
  label: string;
}

export interface SheetOptions {
  cols: number;
  rows: number;
  /** Medidas de cada celda; la hoja mide `cols × cellWidth` por `rows × cellHeight` */
  cellWidth: number;
  cellHeight: number;
  background: StillBackground;
  color: string;
  format: StillFormat;
  quality: number;
  /** Escribe el nombre de cada vista en su celda */
  labels: boolean;
}

/** Junta las vistas en una sola imagen, de izquierda a derecha y de arriba abajo */
export async function composeSheet(cells: SheetCell[], opts: SheetOptions): Promise<StillImage> {
  const W = opts.cols * opts.cellWidth;
  const H = opts.rows * opts.cellHeight;
  const canvas = document.createElement("canvas");
  canvas.width = W;
  canvas.height = H;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error(t("No se pudo crear la imagen"));
  const bitmaps = await Promise.all(cells.map((c) => createImageBitmap(new Blob([c.image.bytes as BlobPart], { type: MIME.png }))));
  try {
    // Las celdas vacías llevan el fondo de la imagen
    if (opts.background === "color") ctx.fillStyle = opts.color;
    else if (opts.background === "transparent") ctx.fillStyle = opts.format === "jpeg" ? "#ffffff" : "rgba(0,0,0,0)";
    else {
      ctx.drawImage(bitmaps[0], 0, 0);
      const [r, g, b, a] = ctx.getImageData(0, 0, 1, 1).data;
      ctx.clearRect(0, 0, W, H);
      ctx.fillStyle = `rgba(${r},${g},${b},${a / 255})`;
    }
    ctx.fillRect(0, 0, W, H);
    const fontSize = Math.max(12, Math.round(opts.cellHeight * 0.045));
    ctx.font = `600 ${fontSize}px sans-serif`;
    ctx.textBaseline = "bottom";
    ctx.lineJoin = "round";
    bitmaps.forEach((bitmap, i) => {
      const x = (i % opts.cols) * opts.cellWidth;
      const y = Math.floor(i / opts.cols) * opts.cellHeight;
      ctx.drawImage(bitmap, x, y, opts.cellWidth, opts.cellHeight);
      if (!opts.labels) return;
      const pad = fontSize * 0.6;
      ctx.lineWidth = fontSize * 0.3;
      ctx.strokeStyle = "rgba(0,0,0,0.6)";
      ctx.fillStyle = "#ffffff";
      ctx.strokeText(cells[i].label, x + pad, y + opts.cellHeight - pad);
      ctx.fillText(cells[i].label, x + pad, y + opts.cellHeight - pad);
    });
  } finally {
    bitmaps.forEach((b) => b.close());
  }
  const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, MIME[opts.format], opts.quality));
  canvas.width = canvas.height = 0;
  if (!blob || blob.type !== MIME[opts.format]) {
    throw new Error(t("Esta ventana no puede guardar {f}; prueba con PNG", { f: opts.format.toUpperCase() }));
  }
  return { bytes: new Uint8Array(await blob.arrayBuffer()), width: W, height: H };
}
