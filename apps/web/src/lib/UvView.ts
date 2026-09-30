/**
 * Lienzo 2D del editor de texturas: la imagen en el cuadrado UV [0, 1]² con
 * las aristas de la malla encima. Convención glTF: UV (0, 0) es la esquina
 * superior izquierda de la imagen, `v` crece hacia abajo.
 *
 * Con WebGL porque una malla escaneada tiene cientos de miles de triángulos:
 * en un canvas 2D cada zoom redibujaría millones de líneas en la CPU.
 */

import * as THREE from "three";
import { THEME_EVENT, themeColor, themeHex } from "./theme";

/** Qué canal de la imagen se ve: todos, uno en gris o el alfa */
export type ImageChannel = "rgba" | "r" | "g" | "b" | "a";

const CHANNEL_INDEX: Record<ImageChannel, number> = { rgba: 0, r: 1, g: 2, b: 3, a: 4 };

/** Color de tema con alfa opcional (#rrggbbaa) como color y opacidad */
function themeRgba(name: string): { color: THREE.Color; opacity: number } {
  const value = themeColor(name);
  const alpha = value.startsWith("#") && value.length === 9 ? parseInt(value.slice(7, 9), 16) / 255 : 1;
  return { color: new THREE.Color(themeHex(name)), opacity: alpha };
}

/** Qué aristas de la malla se dibujan sobre la imagen */
export type UvEdgeMode = "all" | "borders";

/**
 * Aristas de la malla en el espacio UV: pares de vértices, sin repetir. Con
 * quads (4 índices por quad) sin la diagonal; si no, los lados de cada
 * triángulo. `faces` elige qué caras (índice de quad o de triángulo).
 *
 * Los vértices se identifican por su UV (no por su índice: el archivo puede
 * partir un vértice por normales sin cortar la UV). Con `"borders"` quedan
 * solo las aristas de una sola cara: el contorno de cada isla.
 */
export function uvEdges(
  uvs: Float32Array,
  indices: Uint32Array,
  perFace: 3 | 4,
  faces: Iterable<number>,
  mode: UvEdgeMode
): Uint32Array {
  // Vértice representante de cada UV (cuantizada a 2^-20)
  const canonical = new Map<number, number>();
  const id = (v: number) => {
    const key = Math.round(uvs[2 * v] * 1048576) * 4194304 + Math.round(uvs[2 * v + 1] * 1048576);
    let c = canonical.get(key);
    if (c === undefined) canonical.set(key, (c = v));
    return c;
  };
  const stride = uvs.length / 2;
  const count = new Map<number, number>();
  for (const f of faces) {
    const base = f * perFace;
    for (let k = 0; k < perFace; k++) {
      const a = id(indices[base + k]);
      const b = id(indices[base + ((k + 1) % perFace)]);
      if (a === b) continue;
      const key = a < b ? a * stride + b : b * stride + a;
      count.set(key, (count.get(key) ?? 0) + 1);
    }
  }
  const edges: number[] = [];
  for (const [key, n] of count) {
    if (mode === "borders" && n !== 1) continue;
    edges.push(Math.floor(key / stride), key % stride);
  }
  return new Uint32Array(edges);
}

export class UvView {
  private canvas: HTMLCanvasElement;
  private renderer: THREE.WebGLRenderer;
  private scene = new THREE.Scene();
  private camera = new THREE.OrthographicCamera(0, 1, 0, -1, -1, 1);
  private imageMesh: THREE.Mesh;
  private imageMaterial: THREE.ShaderMaterial;
  private texture: THREE.Texture | null = null;
  private wire: THREE.LineSegments;
  private border: THREE.LineLoop;
  private frame: number | null = null;
  private resizeObserver: ResizeObserver;
  private onTheme = () => this.applyTheme();

  /** Centro de la vista (en UV) y píxeles de pantalla por unidad UV */
  private center = new THREE.Vector2(0.5, 0.5);
  private zoom = 1;
  private fitted = false;
  private drag: { x: number; y: number; center: THREE.Vector2 } | null = null;

  /** Posición del cursor en UV (`null` = fuera del lienzo) */
  onCursor?: (uv: [number, number] | null) => void;

  constructor(canvas: HTMLCanvasElement) {
    this.canvas = canvas;
    this.renderer = new THREE.WebGLRenderer({ canvas, antialias: true });
    this.renderer.setPixelRatio(window.devicePixelRatio);

    // La imagen tal cual (sin conversión de color): se ven los valores del
    // archivo, como en un editor de imágenes. El alfa sobre un tablero
    this.imageMaterial = new THREE.ShaderMaterial({
      uniforms: { map: { value: null }, channel: { value: 0 }, hasMap: { value: false } },
      vertexShader: `
        varying vec2 vUv;
        void main() {
          vUv = uv;
          gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
        }`,
      fragmentShader: `
        uniform sampler2D map;
        uniform int channel;
        uniform bool hasMap;
        varying vec2 vUv;
        void main() {
          vec2 cell = floor(gl_FragCoord.xy / 8.0);
          vec3 checker = mix(vec3(0.62), vec3(0.78), mod(cell.x + cell.y, 2.0));
          if (!hasMap) { gl_FragColor = vec4(checker * 0.5, 1.0); return; }
          vec4 c = texture2D(map, vUv);
          if (channel == 1) c = vec4(c.rrr, 1.0);
          else if (channel == 2) c = vec4(c.ggg, 1.0);
          else if (channel == 3) c = vec4(c.bbb, 1.0);
          else if (channel == 4) c = vec4(c.aaa, 1.0);
          gl_FragColor = vec4(mix(checker, c.rgb, c.a), 1.0);
        }`,
    });
    // Cuadrado UV: esquina (u, v) en (u, −v); la textura sin voltear
    const plane = new THREE.BufferGeometry();
    plane.setAttribute("position", new THREE.Float32BufferAttribute([0, 0, 0, 1, 0, 0, 1, -1, 0, 0, -1, 0], 3));
    plane.setAttribute("uv", new THREE.Float32BufferAttribute([0, 0, 1, 0, 1, 1, 0, 1], 2));
    plane.setIndex([0, 2, 1, 0, 3, 2]);
    this.imageMesh = new THREE.Mesh(plane, this.imageMaterial);
    this.scene.add(this.imageMesh);

    this.border = new THREE.LineLoop(
      new THREE.BufferGeometry().setAttribute(
        "position",
        new THREE.Float32BufferAttribute([0, 0, 0, 1, 0, 0, 1, -1, 0, 0, -1, 0], 3)
      ),
      new THREE.LineBasicMaterial({ depthTest: false })
    );
    this.border.renderOrder = 1;
    this.scene.add(this.border);

    this.wire = new THREE.LineSegments(
      new THREE.BufferGeometry(),
      new THREE.LineBasicMaterial({ transparent: true, depthTest: false })
    );
    this.wire.renderOrder = 1;
    this.scene.add(this.wire);

    this.applyTheme();
    window.addEventListener(THEME_EVENT, this.onTheme);

    canvas.addEventListener("wheel", this.onWheel, { passive: false });
    canvas.addEventListener("pointerdown", this.onPointerDown);
    canvas.addEventListener("pointermove", this.onPointerMove);
    canvas.addEventListener("pointerup", this.onPointerUp);
    canvas.addEventListener("pointerleave", () => this.onCursor?.(null));
    canvas.addEventListener("dblclick", () => this.fit());
    this.resizeObserver = new ResizeObserver(() => this.resize());
    this.resizeObserver.observe(canvas.parentElement!);
    this.resize();
  }

  private applyTheme(): void {
    this.scene.background = new THREE.Color(themeHex("viewport"));
    const edge = themeRgba("uv-edge");
    const wire = this.wire.material as THREE.LineBasicMaterial;
    wire.color.copy(edge.color);
    wire.opacity = edge.opacity * (this.wire.userData.opacity ?? 1);
    (this.border.material as THREE.LineBasicMaterial).color.set(themeHex("border-hover"));
    this.requestRender();
  }

  /** Imagen de fondo (`undefined` = sin textura en ese canal) */
  setImage(image: ImageBitmap | undefined): void {
    if (this.texture?.image === image) return;
    this.texture?.dispose();
    this.texture = null;
    if (image) {
      const texture = new THREE.Texture(image);
      texture.flipY = false;
      texture.colorSpace = THREE.NoColorSpace;
      // De cerca se ven los píxeles, como en un editor de imágenes
      texture.magFilter = THREE.NearestFilter;
      texture.anisotropy = this.renderer.capabilities.getMaxAnisotropy();
      texture.needsUpdate = true;
      this.texture = texture;
    }
    this.imageMaterial.uniforms.map.value = this.texture;
    this.imageMaterial.uniforms.hasMap.value = this.texture !== null;
    this.requestRender();
  }

  setChannel(channel: ImageChannel): void {
    this.imageMaterial.uniforms.channel.value = CHANNEL_INDEX[channel];
    this.requestRender();
  }

  /** UV por vértice (2 floats) y aristas como pares de vértices */
  setLayout(uvs: Float32Array | undefined, edges: Uint32Array | undefined): void {
    const geometry = new THREE.BufferGeometry();
    if (uvs && edges && edges.length > 0) {
      const positions = new Float32Array((uvs.length / 2) * 3);
      for (let i = 0, j = 0; i < uvs.length; i += 2, j += 3) {
        positions[j] = uvs[i];
        positions[j + 1] = -uvs[i + 1];
      }
      geometry.setAttribute("position", new THREE.BufferAttribute(positions, 3));
      geometry.setIndex(new THREE.BufferAttribute(edges, 1));
    }
    this.wire.geometry.dispose();
    this.wire.geometry = geometry;
    this.requestRender();
  }

  /** Opacidad de la malla (0 = oculta), sobre la del tema */
  setWireOpacity(opacity: number): void {
    this.wire.userData.opacity = opacity;
    this.wire.visible = opacity > 0;
    this.applyTheme();
  }

  /** Encuadra el cuadrado UV con un margen */
  fit(): void {
    const { width, height } = this.size();
    if (width === 0 || height === 0) return;
    this.center.set(0.5, 0.5);
    this.zoom = 0.92 * Math.min(width, height);
    this.fitted = true;
    this.updateCamera();
  }

  private size() {
    return { width: this.canvas.clientWidth, height: this.canvas.clientHeight };
  }

  private resize(): void {
    const { width, height } = this.size();
    if (width === 0 || height === 0) return;
    this.renderer.setSize(width, height, false);
    if (!this.fitted) this.fit();
    else this.updateCamera();
  }

  private updateCamera(): void {
    const { width, height } = this.size();
    const halfW = width / 2 / this.zoom;
    const halfH = height / 2 / this.zoom;
    this.camera.left = this.center.x - halfW;
    this.camera.right = this.center.x + halfW;
    this.camera.top = -this.center.y + halfH;
    this.camera.bottom = -this.center.y - halfH;
    this.camera.updateProjectionMatrix();
    this.requestRender();
  }

  /** Punto UV bajo una posición de pantalla (relativa al lienzo) */
  private toUv(x: number, y: number): THREE.Vector2 {
    const { width, height } = this.size();
    return new THREE.Vector2(this.center.x + (x - width / 2) / this.zoom, this.center.y + (y - height / 2) / this.zoom);
  }

  private onWheel = (e: WheelEvent): void => {
    e.preventDefault();
    // El punto bajo el cursor queda quieto
    const before = this.toUv(e.offsetX, e.offsetY);
    const factor = Math.exp(-e.deltaY * (e.deltaMode === 1 ? 0.05 : 0.0015));
    this.zoom = THREE.MathUtils.clamp(this.zoom * factor, 20, 200000);
    const after = this.toUv(e.offsetX, e.offsetY);
    this.center.add(before.sub(after));
    this.updateCamera();
  };

  private onPointerDown = (e: PointerEvent): void => {
    // Izquierdo o medio: desplazar
    if (e.button !== 0 && e.button !== 1) return;
    e.preventDefault();
    this.canvas.setPointerCapture(e.pointerId);
    this.drag = { x: e.clientX, y: e.clientY, center: this.center.clone() };
    this.canvas.style.cursor = "grabbing";
  };

  private onPointerMove = (e: PointerEvent): void => {
    if (this.drag) {
      this.center.set(
        this.drag.center.x - (e.clientX - this.drag.x) / this.zoom,
        this.drag.center.y - (e.clientY - this.drag.y) / this.zoom
      );
      this.updateCamera();
    }
    const uv = this.toUv(e.offsetX, e.offsetY);
    this.onCursor?.([uv.x, uv.y]);
  };

  private onPointerUp = (e: PointerEvent): void => {
    if (!this.drag) return;
    this.drag = null;
    this.canvas.releasePointerCapture(e.pointerId);
    this.canvas.style.cursor = "";
  };

  private requestRender(): void {
    if (this.frame !== null) return;
    this.frame = requestAnimationFrame(() => {
      this.frame = null;
      this.renderer.render(this.scene, this.camera);
    });
  }

  dispose(): void {
    if (this.frame !== null) cancelAnimationFrame(this.frame);
    window.removeEventListener(THEME_EVENT, this.onTheme);
    this.resizeObserver.disconnect();
    this.texture?.dispose();
    this.wire.geometry.dispose();
    this.imageMesh.geometry.dispose();
    this.imageMaterial.dispose();
    this.renderer.dispose();
    // dispose() no suelta el contexto WebGL: abrir y cerrar el editor muchas
    // veces agotaría el límite del navegador y se perdería el del visor
    this.renderer.forceContextLoss();
  }
}
