import * as THREE from "three";
import { themeColor, themeHex } from "./theme";

/** Lado del cubo en pantalla (px CSS) y margen a la esquina superior derecha */
const SIZE = 130;
const MARGIN = 8;
/** Fracción de cada cara que pertenece al centro; el resto es arista o vértice */
const EDGE = 0.6;

/** Colores del tema actual (styles/app.css) */
const colors = () => ({
  face: themeColor("cube-face"),
  border: themeColor("cube-border"),
  text: themeColor("cube-text"),
  hover: themeHex("accent"),
  x: themeHex("axis-x"),
  y: themeHex("axis-y"),
  z: themeHex("axis-z"),
});
type Colors = ReturnType<typeof colors>;

/**
 * El visor es Y arriba con el frente hacia +Z (se muestra como Z arriba y
 * frente hacia −Y, ver lib/axes.ts). Orden de BoxGeometry: +X −X +Y −Y +Z −Z
 */
const FACE_LABELS = ["DERECHA", "IZQUIERDA", "ARRIBA", "ABAJO", "FRENTE", "ATRÁS"];

function faceTexture(label: string, COLORS: Colors): THREE.CanvasTexture {
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = 128;
  const ctx = canvas.getContext("2d")!;
  ctx.fillStyle = COLORS.face;
  ctx.fillRect(0, 0, 128, 128);
  ctx.strokeStyle = COLORS.border;
  ctx.lineWidth = 6;
  ctx.strokeRect(3, 3, 122, 122);
  ctx.fillStyle = COLORS.text;
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  // Las etiquetas largas se achican para caber en la cara
  let size = 26;
  do {
    ctx.font = `bold ${size}px system-ui, sans-serif`;
    size -= 1;
  } while (ctx.measureText(label).width > 112 && size > 12);
  ctx.fillText(label, 64, 66);
  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.SRGBColorSpace;
  texture.anisotropy = 4;
  return texture;
}

function axisLabel(text: string, color: number): THREE.Sprite {
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = 64;
  const ctx = canvas.getContext("2d")!;
  ctx.fillStyle = `#${color.toString(16).padStart(6, "0")}`;
  ctx.font = "bold 44px system-ui, sans-serif";
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillText(text, 32, 34);
  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.SRGBColorSpace;
  // Con prueba de profundidad: la Y apunta hacia atrás y detrás del cubo no se ve
  const sprite = new THREE.Sprite(new THREE.SpriteMaterial({ map: texture }));
  sprite.scale.setScalar(0.5);
  sprite.renderOrder = 2;
  return sprite;
}

/**
 * Cubo de orientación en la esquina del visor: gira con la cámara y, al
 * hacer clic en una cara, arista o vértice, lleva la cámara a esa vista.
 * Los ejes salen del vértice izquierdo-inferior-delantero con los colores de
 * Blender (X rojo, Y verde, Z azul), con Z hacia arriba.
 */
export class ViewCube {
  private scene = new THREE.Scene();
  private camera = new THREE.OrthographicCamera(-2.3, 2.3, 2.3, -2.3, 0.1, 20);
  private cube!: THREE.Mesh;
  private highlight!: THREE.Mesh;
  private raycaster = new THREE.Raycaster();
  private disposables: { dispose(): void }[] = [];

  constructor() {
    this.build();
  }

  /** Vuelve a armar el cubo con los colores del tema actual */
  applyTheme(): void {
    const hover = this.highlight.visible ? this.highlight.userData.key : null;
    this.dispose();
    this.disposables = [];
    this.scene.clear();
    this.build();
    if (hover !== null) this.setHover(new THREE.Vector3(...(hover as string).split(",").map(Number)));
  }

  private build(): void {
    const COLORS = colors();
    const geometry = new THREE.BoxGeometry(2, 2, 2);
    const materials = FACE_LABELS.map((label) => {
      const map = faceTexture(label, COLORS);
      const material = new THREE.MeshBasicMaterial({ map });
      this.disposables.push(map, material);
      return material;
    });
    this.cube = new THREE.Mesh(geometry, materials);
    this.scene.add(this.cube);

    const highlightMaterial = new THREE.MeshBasicMaterial({ color: COLORS.hover, transparent: true, opacity: 0.55 });
    const highlightGeometry = new THREE.BoxGeometry(1, 1, 1);
    this.highlight = new THREE.Mesh(highlightGeometry, highlightMaterial);
    this.highlight.visible = false;
    this.scene.add(this.highlight);
    this.disposables.push(geometry, highlightMaterial, highlightGeometry);

    // Ejes como en Blender (Z arriba, Y hacia atrás) desde el vértice
    // izquierdo-inferior-delantero, apenas por fuera del cubo. En el visor
    // Y es arriba y +Z el frente: Y de la vista es −Z del visor
    const origin = new THREE.Vector3(-1.04, -1.04, 1.04);
    const axes: [THREE.Vector3, number, string][] = [
      [new THREE.Vector3(1, 0, 0), COLORS.x, "X"],
      [new THREE.Vector3(0, 0, -1), COLORS.y, "Y"],
      [new THREE.Vector3(0, 1, 0), COLORS.z, "Z"],
    ];
    for (const [dir, color, text] of axes) {
      const end = origin.clone().addScaledVector(dir, 2.4);
      const lineGeometry = new THREE.BufferGeometry().setFromPoints([origin, end]);
      const lineMaterial = new THREE.LineBasicMaterial({ color });
      const line = new THREE.Line(lineGeometry, lineMaterial);
      const label = axisLabel(text, color);
      label.position.copy(origin).addScaledVector(dir, 2.7);
      this.scene.add(line, label);
      this.disposables.push(lineGeometry, lineMaterial, label.material, label.material.map!);
    }
  }

  /** Rectángulo del cubo en coordenadas de pantalla (px CSS) */
  private rect(canvas: HTMLCanvasElement) {
    const bounds = canvas.getBoundingClientRect();
    return { left: bounds.right - SIZE - MARGIN, top: bounds.top + MARGIN, bounds };
  }

  /** Dibuja el cubo en su esquina, orientado como la cámara principal */
  render(renderer: THREE.WebGLRenderer, camera: THREE.Camera, target: THREE.Vector3): void {
    const canvas = renderer.domElement;
    const width = canvas.clientWidth;
    const height = canvas.clientHeight;
    if (width < SIZE * 2 || height < SIZE * 2) return;

    const offset = camera.position.clone().sub(target).normalize();
    this.camera.position.copy(offset).multiplyScalar(8);
    this.camera.up.copy(camera.up);
    this.camera.quaternion.copy(camera.quaternion);
    this.camera.updateMatrixWorld();

    // Viewport y tijera usan origen abajo a la izquierda
    const x = width - SIZE - MARGIN;
    const y = height - SIZE - MARGIN;
    const autoClear = renderer.autoClear;
    renderer.autoClear = false;
    renderer.clearDepth();
    renderer.setScissorTest(true);
    renderer.setScissor(x, y, SIZE, SIZE);
    renderer.setViewport(x, y, SIZE, SIZE);
    renderer.render(this.scene, this.camera);
    renderer.setScissorTest(false);
    renderer.setViewport(0, 0, width, height);
    renderer.autoClear = autoClear;
  }

  /**
   * Dirección de vista bajo el puntero: cara (un eje), arista (dos) o
   * vértice (tres). `null` si el puntero no está sobre el cubo.
   */
  pick(canvas: HTMLCanvasElement, clientX: number, clientY: number): THREE.Vector3 | null {
    const { left, top } = this.rect(canvas);
    const u = (clientX - left) / SIZE;
    const v = (clientY - top) / SIZE;
    if (u < 0 || u > 1 || v < 0 || v > 1) return null;
    this.raycaster.setFromCamera(new THREE.Vector2(u * 2 - 1, 1 - v * 2), this.camera);
    const hit = this.raycaster.intersectObject(this.cube)[0];
    if (!hit) return null;
    const p = hit.point;
    const snap = (c: number) => (c > EDGE ? 1 : c < -EDGE ? -1 : 0);
    return new THREE.Vector3(snap(p.x), snap(p.y), snap(p.z));
  }

  /** ¿Está el puntero dentro del recuadro del cubo? */
  contains(canvas: HTMLCanvasElement, clientX: number, clientY: number): boolean {
    const { left, top } = this.rect(canvas);
    return clientX >= left && clientX <= left + SIZE && clientY >= top && clientY <= top + SIZE;
  }

  /** Resalta la zona (cara, arista o vértice) de `dir`; `null` la apaga. Devuelve si cambió */
  setHover(dir: THREE.Vector3 | null): boolean {
    const was = this.highlight.visible ? this.highlight.userData.key : null;
    const key = dir ? dir.toArray().join(",") : null;
    if (was === key) return false;
    this.highlight.visible = dir !== null;
    this.highlight.userData.key = key;
    if (dir) {
      // En cada eje: si la zona toca esa cara, una franja fina por fuera; si no, el centro
      const size = new THREE.Vector3();
      const center = new THREE.Vector3();
      for (const axis of [0, 1, 2]) {
        const c = dir.getComponent(axis);
        size.setComponent(axis, c === 0 ? EDGE * 2 : 1 - EDGE + 0.02);
        center.setComponent(axis, c * (1 + EDGE + 0.02) / 2);
      }
      this.highlight.scale.copy(size);
      this.highlight.position.copy(center);
    }
    return true;
  }

  dispose(): void {
    for (const d of this.disposables) d.dispose();
  }
}
