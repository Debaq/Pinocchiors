import * as THREE from "three";
import { t } from "./i18n.ts";

/**
 * Cuatro vistas: el visor principal ocupa el cuadrante de arriba a la izquierda (su lienzo
 * se achica a la mitad, así todo lo que mide el lienzo sigue valiendo) y las otras tres son
 * vistas ortográficas fijas (arriba, frente y derecha) que se dibujan con el mismo renderer
 * y se copian a lienzos 2D puestos en los demás cuadrantes. En ellas la rueda acerca, arrastrar
 * desplaza y el doble clic lleva la vista principal a esa dirección.
 *
 * El visor es Y arriba con el frente hacia +Z (lib/axes.ts).
 */
interface QuadView {
  name: string;
  /** Hacia dónde está la cámara, desde el centro */
  dir: THREE.Vector3;
  up: THREE.Vector3;
  /** Lugar del cuadrante: columna y fila (0 o 1) */
  col: number;
  row: number;
  camera: THREE.OrthographicCamera;
  canvas: HTMLCanvasElement;
  ctx: CanvasRenderingContext2D;
  /** Desplazamiento en el plano de la vista (unidades de la escena) y acercamiento */
  pan: THREE.Vector2;
  zoom: number;
}

export class QuadViews {
  private views: QuadView[] = [];
  private fitted = "";

  constructor(
    host: HTMLElement,
    private renderer: THREE.WebGLRenderer,
    private scene: THREE.Scene,
    /** Esfera que contiene lo que hay en la escena (null si no hay nada) */
    private bounds: () => THREE.Sphere | null,
    /** Lleva la vista principal a mirar desde `dir` */
    private lookFrom: (dir: THREE.Vector3) => void,
    private onChange: () => void,
  ) {
    const defs: { name: () => string; dir: [number, number, number]; up: [number, number, number]; col: number; row: number }[] = [
      { name: () => t("Superior"), dir: [0, 1, 0], up: [0, 0, -1], col: 1, row: 0 },
      { name: () => t("Frontal"), dir: [0, 0, 1], up: [0, 1, 0], col: 0, row: 1 },
      { name: () => t("Derecha"), dir: [1, 0, 0], up: [0, 1, 0], col: 1, row: 1 },
    ];
    for (const def of defs) {
      const canvas = document.createElement("canvas");
      canvas.style.position = "absolute";
      canvas.style.userSelect = "none";
      canvas.style.touchAction = "none";
      canvas.style.left = `${def.col * 50}%`;
      canvas.style.top = `${def.row * 50}%`;
      canvas.style.width = "50%";
      canvas.style.height = "50%";
      canvas.style.cursor = "grab";
      const view: QuadView = {
        name: def.name(),
        dir: new THREE.Vector3(...def.dir),
        up: new THREE.Vector3(...def.up),
        col: def.col,
        row: def.row,
        camera: new THREE.OrthographicCamera(-1, 1, 1, -1, 0.001, 1000),
        canvas,
        ctx: canvas.getContext("2d")!,
        pan: new THREE.Vector2(),
        zoom: 1,
      };
      this.attach(view);
      host.appendChild(canvas);
      this.views.push(view);
    }
  }

  private attach(view: QuadView): void {
    const { canvas } = view;
    canvas.addEventListener("wheel", (e) => {
      e.preventDefault();
      view.zoom = Math.min(200, Math.max(0.05, view.zoom * (e.deltaY < 0 ? 1.15 : 1 / 1.15)));
      this.onChange();
    }, { passive: false });
    canvas.addEventListener("dblclick", () => this.lookFrom(view.dir.clone()));
    canvas.addEventListener("pointerdown", (e) => {
      if (e.button !== 0 && e.button !== 1) return;
      canvas.setPointerCapture(e.pointerId);
      canvas.style.cursor = "grabbing";
      let last = { x: e.clientX, y: e.clientY };
      const move = (m: PointerEvent) => {
        const sphere = this.bounds();
        const rect = canvas.getBoundingClientRect();
        if (!sphere || rect.height === 0) return;
        // Una unidad de pantalla alta = 2 · (radio · margen / zoom) de la escena
        const perPixel = (2 * sphere.radius * 1.2) / view.zoom / rect.height;
        view.pan.x -= (m.clientX - last.x) * perPixel;
        view.pan.y += (m.clientY - last.y) * perPixel;
        last = { x: m.clientX, y: m.clientY };
        this.onChange();
      };
      const up = () => {
        canvas.style.cursor = "grab";
        canvas.removeEventListener("pointermove", move);
        canvas.removeEventListener("pointerup", up);
        canvas.removeEventListener("pointercancel", up);
      };
      canvas.addEventListener("pointermove", move);
      canvas.addEventListener("pointerup", up);
      canvas.addEventListener("pointercancel", up);
    });
  }

  /** Dibuja las tres vistas con el renderer (que debe tener ya el tamaño de un cuadrante) */
  render(): void {
    const sphere = this.bounds() ?? new THREE.Sphere(new THREE.Vector3(0, 0.5, 0), 1);
    // Con otro modelo, las vistas vuelven a encuadrar
    const key = [sphere.center.x, sphere.center.y, sphere.center.z, sphere.radius].map((v) => v.toPrecision(3)).join(",");
    if (key !== this.fitted) {
      this.fitted = key;
      for (const v of this.views) {
        v.pan.set(0, 0);
        v.zoom = 1;
      }
    }
    const size = this.renderer.getSize(new THREE.Vector2());
    const aspect = size.x / Math.max(1, size.y);
    const source = this.renderer.domElement;
    for (const v of this.views) {
      const half = (Math.max(sphere.radius, 1e-6) * 1.2) / v.zoom;
      const cam = v.camera;
      cam.left = -half * aspect;
      cam.right = half * aspect;
      cam.top = half;
      cam.bottom = -half;
      cam.near = 0.001;
      cam.far = sphere.radius * 20 + 10;
      cam.up.copy(v.up);
      // Plano de la vista: derecha = up × dir, arriba = up
      const right = new THREE.Vector3().crossVectors(v.up, v.dir).normalize();
      const target = sphere.center.clone().addScaledVector(right, v.pan.x).addScaledVector(v.up, v.pan.y);
      cam.position.copy(target).addScaledVector(v.dir, sphere.radius * 5 + 5);
      cam.lookAt(target);
      cam.updateProjectionMatrix();
      this.renderer.render(this.scene, cam);

      if (v.canvas.width !== source.width || v.canvas.height !== source.height) {
        v.canvas.width = source.width;
        v.canvas.height = source.height;
      }
      v.ctx.drawImage(source, 0, 0);
      this.label(v);
    }
  }

  private label(v: QuadView): void {
    const { ctx, canvas } = v;
    const dpr = canvas.width / Math.max(1, canvas.clientWidth || canvas.width);
    ctx.save();
    ctx.strokeStyle = "rgba(128,128,128,0.6)";
    ctx.lineWidth = dpr;
    ctx.strokeRect(0.5, 0.5, canvas.width - 1, canvas.height - 1);
    ctx.font = `${12 * dpr}px system-ui, sans-serif`;
    ctx.fillStyle = "rgba(255,255,255,0.9)";
    ctx.shadowColor = "rgba(0,0,0,0.7)";
    ctx.shadowBlur = 3 * dpr;
    ctx.textBaseline = "top";
    ctx.fillText(v.name, 8 * dpr, 6 * dpr);
    ctx.restore();
  }

  dispose(): void {
    for (const v of this.views) v.canvas.remove();
    this.views = [];
  }
}
