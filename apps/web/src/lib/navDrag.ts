import * as THREE from "three";
import type { OrbitControls } from "three/addons/controls/OrbitControls.js";

/**
 * Girar y desplazar la vista arrastrando, sin tope en los polos: el giro
 * vertical también gira el "arriba" de la cámara, así que se puede dar la
 * vuelta completa por encima del modelo (la vista queda de cabeza).
 *
 * Con `lock` el puntero queda bloqueado (Pointer Lock): llegan movimientos
 * relativos sin límite aunque el ratón llegue al borde de la pantalla, y en
 * su lugar se dibuja un cursor que, en un borde del visor, sigue desde el
 * borde opuesto. Mover el cursor del sistema no sirve: Wayland no lo deja y
 * Tauri igual responde que sí. Si el bloqueo falla, el arrastre sigue normal.
 * El zoom queda en `OrbitControls`.
 */
export class NavDrag {
  private drag?: {
    mode: "rotate" | "pan";
    id: number;
    x: number;
    y: number;
    /** Cursor dibujado mientras el puntero está bloqueado */
    cursor?: { el: HTMLDivElement; x: number; y: number };
    /** `controls` estaban encendidos al empezar (se apagan durante el arrastre) */
    controls: boolean;
  };

  constructor(
    private camera: THREE.PerspectiveCamera,
    private controls: OrbitControls,
    private element: HTMLElement,
    private onChange: () => void,
  ) {
    element.addEventListener("pointermove", (e) => this.move(e));
    const end = (e: PointerEvent) => {
      if (this.drag?.id === e.pointerId) this.stop();
    };
    // Con el puntero bloqueado los eventos llegan al elemento; sin él, la captura
    element.addEventListener("pointerup", end);
    element.addEventListener("pointercancel", end);
    document.addEventListener("pointerlockchange", () => {
      const d = this.drag;
      if (!d) return;
      if (document.pointerLockElement === element) this.showCursor(d);
      // Esc o la ventana perdió el foco: termina el arrastre
      else if (d.cursor) this.stop();
    });
  }

  get active(): boolean {
    return !!this.drag;
  }

  start(e: PointerEvent, mode: "rotate" | "pan", lock = true) {
    this.stop();
    // `OrbitControls` (que ve el mismo pointerdown después) quiere capturar el
    // puntero, y con el bloqueo pedido el navegador lo rechaza con un error:
    // apagados mientras dura el arrastre, no lo intentan
    this.drag = { mode, id: e.pointerId, x: e.clientX, y: e.clientY, controls: this.controls.enabled };
    this.controls.enabled = false;
    try {
      this.element.setPointerCapture(e.pointerId);
    } catch {
      // Sin captura igual funciona mientras el puntero esté encima
    }
    // Se pide en el pointerdown: WebKit solo bloquea durante un gesto del usuario
    if (lock && this.element.requestPointerLock) {
      try {
        const p = this.element.requestPointerLock() as unknown as Promise<void> | undefined;
        p?.catch?.(() => {});
      } catch {
        // Sin bloqueo: arrastre normal
      }
    }
  }

  private stop() {
    const d = this.drag;
    this.drag = undefined;
    d?.cursor?.el.remove();
    if (d?.controls) this.controls.enabled = true;
    if (document.pointerLockElement === this.element) document.exitPointerLock();
  }

  private showCursor(d: NonNullable<NavDrag["drag"]>) {
    if (d.cursor) return;
    const el = document.createElement("div");
    el.setAttribute("aria-hidden", "true");
    el.style.cssText =
      "position:fixed;left:0;top:0;width:22px;height:22px;margin:-11px 0 0 -11px;" +
      "pointer-events:none;z-index:2147483647;color:#fff;filter:drop-shadow(0 0 1.5px #000)";
    el.innerHTML =
      d.mode === "rotate"
        ? '<svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12a9 9 0 1 1-3-6.7"/><path d="M21 3v6h-6"/></svg>'
        : '<svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2v20M2 12h20M12 2l-3 3M12 2l3 3M12 22l-3-3M12 22l3-3M2 12l3-3M2 12l3 3M22 12l-3-3M22 12l-3 3"/></svg>';
    document.body.appendChild(el);
    d.cursor = { el, x: d.x, y: d.y };
    this.placeCursor(d.cursor);
  }

  private placeCursor(c: { el: HTMLDivElement; x: number; y: number }) {
    c.el.style.transform = `translate(${c.x}px, ${c.y}px)`;
  }

  private move(e: PointerEvent) {
    const d = this.drag;
    if (!d || e.pointerId !== d.id) return;
    let dx: number;
    let dy: number;
    if (d.cursor) {
      dx = e.movementX;
      dy = e.movementY;
      this.wrap(d.cursor, dx, dy);
    } else {
      dx = e.clientX - d.x;
      dy = e.clientY - d.y;
      d.x = e.clientX;
      d.y = e.clientY;
    }
    if (!dx && !dy) return;
    if (d.mode === "rotate") this.rotate(dx, dy);
    else this.pan(dx, dy);
    this.controls.update();
    this.onChange();
  }

  /** El cursor dibujado avanza y, al salir del visor, entra por el borde opuesto */
  private wrap(c: { el: HTMLDivElement; x: number; y: number }, dx: number, dy: number) {
    const r = this.element.getBoundingClientRect();
    const w = Math.max(r.width, 1);
    const h = Math.max(r.height, 1);
    c.x = r.left + ((((c.x + dx - r.left) % w) + w) % w);
    c.y = r.top + ((((c.y + dy - r.top) % h) + h) % h);
    this.placeCursor(c);
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
}
