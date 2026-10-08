import * as THREE from "three";
import type { OrbitControls } from "three/addons/controls/OrbitControls.js";

/** Reubica el cursor (coordenadas de cliente); `false` si no se puede */
type Warp = (x: number, y: number) => Promise<boolean>;

let warpCursor: Warp | null | undefined;

/**
 * En la app de escritorio el cursor se puede llevar a otro lado de la
 * ventana; en el navegador (o en Wayland, que no lo deja) no.
 */
async function warp(x: number, y: number): Promise<boolean> {
  if (warpCursor === undefined) {
    warpCursor = null;
    if ("__TAURI_INTERNALS__" in window) {
      try {
        const { getCurrentWindow, LogicalPosition } = await import("@tauri-apps/api/window");
        const win = getCurrentWindow();
        warpCursor = async (cx, cy) => {
          try {
            await win.setCursorPosition(new LogicalPosition(cx, cy));
            return true;
          } catch {
            warpCursor = null;
            return false;
          }
        };
      } catch {
        warpCursor = null;
      }
    }
  }
  return warpCursor ? warpCursor(x, y) : false;
}

/**
 * Girar y desplazar la vista arrastrando, sin tope en los polos: el giro
 * vertical también gira el "arriba" de la cámara, así que se puede dar la
 * vuelta completa por encima del modelo (la vista queda de cabeza). Mientras
 * se arrastra, el cursor que llega a un borde del visor sigue desde el borde
 * opuesto. El zoom queda en `OrbitControls`.
 */
export class NavDrag {
  private drag?: {
    mode: "rotate" | "pan";
    id: number;
    x: number;
    y: number;
    /** Salto del cursor pedido: los movimientos que quedaron en cola se ignoran */
    warp?: { x: number; y: number; t: number };
  };

  constructor(
    private camera: THREE.PerspectiveCamera,
    private controls: OrbitControls,
    private element: HTMLElement,
    private onChange: () => void,
  ) {
    element.addEventListener("pointermove", (e) => this.move(e));
    const end = (e: PointerEvent) => {
      if (this.drag?.id === e.pointerId) this.drag = undefined;
    };
    element.addEventListener("pointerup", end);
    element.addEventListener("pointercancel", end);
  }

  get active(): boolean {
    return !!this.drag;
  }

  start(e: PointerEvent, mode: "rotate" | "pan") {
    this.drag = { mode, id: e.pointerId, x: e.clientX, y: e.clientY };
    try {
      this.element.setPointerCapture(e.pointerId);
    } catch {
      // Sin captura igual funciona mientras el puntero esté encima
    }
  }

  private move(e: PointerEvent) {
    const d = this.drag;
    if (!d || e.pointerId !== d.id) return;
    if (d.warp) {
      // Hasta que llegue un movimiento cerca de donde se llevó el cursor
      if (Math.hypot(e.clientX - d.warp.x, e.clientY - d.warp.y) > 60 && performance.now() - d.warp.t < 200) return;
      d.warp = undefined;
      d.x = e.clientX;
      d.y = e.clientY;
      return;
    }
    const dx = e.clientX - d.x;
    const dy = e.clientY - d.y;
    d.x = e.clientX;
    d.y = e.clientY;
    if (d.mode === "rotate") this.rotate(dx, dy);
    else this.pan(dx, dy);
    this.controls.update();
    this.onChange();
    this.wrap(e, d);
  }

  /** Como la órbita de siempre (una altura de visor = una vuelta), pero sin tope */
  private rotate(dx: number, dy: number) {
    const h = this.element.clientHeight || 1;
    const target = this.controls.target;
    this.camera.updateMatrixWorld();
    const right = new THREE.Vector3().setFromMatrixColumn(this.camera.matrixWorld, 0);
    const up = new THREE.Vector3().setFromMatrixColumn(this.camera.matrixWorld, 1);
    const worldUp = new THREE.Vector3(0, 1, 0);
    // De cabeza, el giro horizontal se invierte para que siga a la mano
    const yawSign = up.dot(worldUp) < 0 ? 1 : -1;
    const pitch = new THREE.Quaternion().setFromAxisAngle(right, (-2 * Math.PI * dy) / h);
    const yaw = new THREE.Quaternion().setFromAxisAngle(worldUp, (yawSign * 2 * Math.PI * dx) / h);
    const turn = yaw.multiply(pitch);
    const offset = this.camera.position.clone().sub(target).applyQuaternion(turn);
    this.camera.up.copy(up.applyQuaternion(turn)).normalize();
    this.camera.position.copy(target).add(offset);
    this.camera.lookAt(target);
  }

  /** La vista sigue al puntero a la distancia del centro de giro */
  private pan(dx: number, dy: number) {
    const h = this.element.clientHeight || 1;
    const dist = this.camera.position.distanceTo(this.controls.target);
    const s = (2 * dist * Math.tan((this.camera.fov * Math.PI) / 360)) / h;
    this.camera.updateMatrixWorld();
    const right = new THREE.Vector3().setFromMatrixColumn(this.camera.matrixWorld, 0);
    const up = new THREE.Vector3().setFromMatrixColumn(this.camera.matrixWorld, 1);
    const delta = right.multiplyScalar(-dx * s).addScaledVector(up, dy * s);
    this.camera.position.add(delta);
    this.controls.target.add(delta);
  }

  /** En un borde del visor, el cursor pasa al borde opuesto */
  private wrap(e: PointerEvent, d: NonNullable<NavDrag["drag"]>) {
    const r = this.element.getBoundingClientRect();
    const m = 2;
    let x = e.clientX;
    let y = e.clientY;
    if (x <= r.left + m) x = r.right - m - 2;
    else if (x >= r.right - m) x = r.left + m + 2;
    if (y <= r.top + m) y = r.bottom - m - 2;
    else if (y >= r.bottom - m) y = r.top + m + 2;
    if (x === e.clientX && y === e.clientY) return;
    d.warp = { x, y, t: performance.now() };
    void warp(x, y).then((ok) => {
      // Sin poder llevar el cursor, sigue como estaba
      if (!ok && this.drag === d) d.warp = undefined;
    });
  }
}
