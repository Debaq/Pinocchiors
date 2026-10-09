import * as THREE from "three";
import type { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { NavDrag } from "./navDrag";
import { ViewCube } from "./ViewCube";

/**
 * Dónde está la cámara, en metros y con los ejes internos (Y arriba): igual en
 * los dos visores, así pasar de uno al otro no cambia la vista.
 */
export interface CameraPose {
  position: [number, number, number];
  target: [number, number, number];
  up: [number, number, number];
}

/**
 * Lo que comparten los dos visores (el principal y el de Diseñar) para mover
 * la cámara: girar y desplazar arrastrando (`NavDrag`), el cubo de vistas
 * (clic y resaltado con cursor de mano), el giro animado hacia una vista y
 * encuadrar. Así se manejan igual (ver libs/cad/PLAN_VISOR.md).
 */
export class CameraRig {
  readonly nav: NavDrag;
  readonly viewCube = new ViewCube();
  private transition: { from: THREE.Vector3; turn: THREE.Quaternion; up: THREE.Vector3; start: number } | null = null;
  private cursorBeforeCube: string | null = null;

  constructor(
    readonly camera: THREE.PerspectiveCamera,
    readonly controls: OrbitControls,
    private element: HTMLCanvasElement,
    private onChange: () => void,
  ) {
    this.nav = new NavDrag(camera, controls, element, onChange);
  }

  /** Hay un giro hacia una vista en curso */
  get moving(): boolean {
    return this.transition !== null;
  }

  /**
   * Gira la cámara (~0,3 s) hasta mirar hacia el centro de la vista desde
   * `direction` (en el visor, Y arriba). Desde arriba o abajo, un pelo hacia el
   * frente para que la pantalla tenga un "arriba".
   */
  lookFrom(direction: THREE.Vector3) {
    const dir = direction.clone().normalize();
    if (Math.abs(dir.x) < 1e-9 && Math.abs(dir.z) < 1e-9) dir.z = 1e-3 * Math.sign(dir.y || 1);
    dir.normalize();
    const from = this.camera.position.clone().sub(this.controls.target);
    const turn = new THREE.Quaternion().setFromUnitVectors(from.clone().normalize(), dir);
    this.transition = { from, turn, up: this.camera.up.clone(), start: performance.now() };
    this.onChange();
  }

  /** Corta el giro en curso (encuadrar, cargar otro modelo…) */
  stop() {
    this.transition = null;
  }

  /**
   * Avanza el giro; se llama en cada cuadro antes de dibujar. Mientras siga,
   * pide el siguiente cuadro.
   */
  step() {
    const t0 = this.transition;
    if (!t0) return;
    const t = Math.min(1, (performance.now() - t0.start) / 300);
    const eased = 1 - Math.pow(1 - t, 3);
    const turn = new THREE.Quaternion().slerp(t0.turn, eased);
    this.camera.position.copy(this.controls.target).add(t0.from.clone().applyQuaternion(turn));
    // Si se había dado la vuelta por encima (o se venía de un sketch inclinado), vuelve con Y arriba
    this.camera.up.copy(t0.up).lerp(new THREE.Vector3(0, 1, 0), eased).normalize();
    if (this.camera.up.lengthSq() < 0.5) this.camera.up.set(0, 1, 0);
    this.camera.lookAt(this.controls.target);
    this.controls.update();
    if (t < 1) this.onChange();
    else this.transition = null;
  }

  /** Clic en el cubo: gira hacia esa cara, arista o vértice. `true` si lo usó */
  cubeDown(clientX: number, clientY: number): boolean {
    if (!this.viewCube.contains(this.element, clientX, clientY)) return false;
    const dir = this.viewCube.pick(this.element, clientX, clientY);
    if (!dir) return false;
    this.lookFrom(dir);
    return true;
  }

  /**
   * Resalta la zona del cubo bajo el puntero; `true` si está sobre él. Con
   * `cursor`, encima pone la mano (si no, el cursor lo maneja quien llama).
   */
  cubeHover(clientX: number, clientY: number, enabled = true, cursor = true): boolean {
    const over = enabled ? this.viewCube.pick(this.element, clientX, clientY) : null;
    if (this.viewCube.setHover(over)) this.onChange();
    if (!cursor) return over !== null;
    if (over && this.cursorBeforeCube === null) {
      this.cursorBeforeCube = this.element.style.cursor;
      this.element.style.cursor = "pointer";
    } else if (!over && this.cursorBeforeCube !== null) {
      this.element.style.cursor = this.cursorBeforeCube;
      this.cursorBeforeCube = null;
    }
    return over !== null;
  }

  /**
   * Encuadra la esfera: el centro de giro a su centro y la distancia para que
   * entre entera (con un margen), mirando desde donde se miraba. Ajusta los
   * planos de corte de la cámara a esa distancia.
   */
  frame(sphere: THREE.Sphere, margin = 1.1) {
    this.stop();
    const dir = this.camera.position.clone().sub(this.controls.target).normalize();
    if (dir.lengthSq() < 1e-9) dir.set(0.6, 0.5, 0.8).normalize();
    const radius = Math.max(sphere.radius, 1e-6);
    // Que entre a lo alto y a lo ancho (en pantallas angostas manda el ancho)
    const vfov = (this.camera.fov * Math.PI) / 180;
    const hfov = 2 * Math.atan(Math.tan(vfov / 2) * this.camera.aspect);
    const dist = (radius / Math.sin(Math.min(vfov, hfov) / 2)) * margin;
    this.controls.target.copy(sphere.center);
    this.camera.position.copy(sphere.center).addScaledVector(dir, dist);
    this.camera.near = dist / 1000;
    this.camera.far = dist * 100;
    this.camera.updateProjectionMatrix();
    this.controls.update();
    this.onChange();
  }

  /** La pose de la cámara en metros (`metersPerUnit`: metros por unidad de la escena) */
  pose(metersPerUnit: number): CameraPose {
    const m = (v: THREE.Vector3) => v.clone().multiplyScalar(metersPerUnit).toArray() as [number, number, number];
    return { position: m(this.camera.position), target: m(this.controls.target), up: this.camera.up.toArray() as [number, number, number] };
  }

  /** Pone la cámara en `pose` (en metros) */
  setPose(pose: CameraPose, metersPerUnit: number) {
    this.stop();
    const k = 1 / metersPerUnit;
    this.camera.position.fromArray(pose.position).multiplyScalar(k);
    this.controls.target.fromArray(pose.target).multiplyScalar(k);
    this.camera.up.fromArray(pose.up);
    const dist = Math.max(this.camera.position.distanceTo(this.controls.target), 1e-9);
    this.camera.near = dist / 1000;
    this.camera.far = dist * 100;
    this.camera.updateProjectionMatrix();
    this.camera.lookAt(this.controls.target);
    this.controls.update();
    this.onChange();
  }

  /** Dibuja el cubo en la esquina (después de la escena) */
  renderCube(renderer: THREE.WebGLRenderer) {
    this.viewCube.render(renderer, this.camera, this.controls.target);
  }
}
