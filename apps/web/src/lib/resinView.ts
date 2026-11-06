/**
 * Vista previa de resina en el visor: la pieza elevada, sus soportes y la balsa sobre la
 * pantalla de la impresora, cortadas a la altura de la capa elegida (lo que ya se imprimió),
 * con los venteos y las islas sin soporte marcados. La impresora usa Z arriba en mm; la
 * escena Y arriba, así que (x, y, z) → (x, z, −y).
 */
import * as THREE from "three";
import type { ResinViewData, ResinViewOptions } from "./resinPreview";
import { buildPrintBed } from "./printBed";

export class ResinView {
  readonly group = new THREE.Group();
  private data: ResinViewData;
  /** Corta lo que está por encima de la capa: normal hacia abajo (−Y en la escena) */
  private clip = new THREE.Plane(new THREE.Vector3(0, -1, 0), 0);
  private supports: THREE.Mesh;

  constructor(data: ResinViewData) {
    this.data = data;
    const model = this.mesh(data.model, 0xb8c4d0);
    this.supports = this.mesh(data.supports, 0x7fb8e6);
    this.group.add(buildPrintBed(data.bed[0], data.bed[1]), model, this.supports, this.markers());
  }

  /** Esfera que encuadra la pantalla y lo preparado */
  bounds(): THREE.Sphere {
    const [bx, by] = this.data.bed;
    const top = Math.max(this.data.layerZ[this.data.layerZ.length - 1] ?? 10, 10);
    return new THREE.Box3(new THREE.Vector3(0, 0, -by), new THREE.Vector3(bx, top, 0)).getBoundingSphere(new THREE.Sphere());
  }

  update(options: ResinViewOptions): void {
    const z = this.data.layerZ[Math.min(Math.max(options.layer, 0), this.data.layerZ.length - 1)] ?? Infinity;
    this.clip.constant = z + 1e-4;
    this.supports.visible = options.supports;
  }

  dispose(): void {
    this.group.traverse((o) => {
      if (o instanceof THREE.Mesh || o instanceof THREE.Line) {
        o.geometry.dispose();
        const m = o.material as THREE.Material | THREE.Material[];
        (Array.isArray(m) ? m : [m]).forEach((x) => x.dispose());
      }
    });
  }

  private mesh(tris: Float32Array, color: number): THREE.Mesh {
    const pos = new Float32Array(tris.length);
    for (let i = 0; i < tris.length; i += 3) {
      pos[i] = tris[i];
      pos[i + 1] = tris[i + 2];
      pos[i + 2] = -tris[i + 1];
    }
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute("position", new THREE.BufferAttribute(pos, 3));
    geometry.computeVertexNormals();
    const material = new THREE.MeshStandardMaterial({ color, roughness: 0.6, metalness: 0.05, clippingPlanes: [this.clip], side: THREE.DoubleSide });
    const mesh = new THREE.Mesh(geometry, material);
    mesh.frustumCulled = false;
    return mesh;
  }

  /** Venteos (anillos naranjas) e islas sin soporte (esferas rojas), siempre a la vista */
  private markers(): THREE.Group {
    const group = new THREE.Group();
    const ring = new THREE.LineBasicMaterial({ color: 0xff9a3c, depthTest: false });
    for (const v of this.data.vents) {
      const pts = Array.from({ length: 33 }, (_, i) => {
        const a = (i / 32) * Math.PI * 2;
        return new THREE.Vector3(v.pos[0] + v.radius * Math.cos(a), v.bottom, -(v.pos[1] + v.radius * Math.sin(a)));
      });
      const top = pts.map((p) => new THREE.Vector3(p.x, v.top, p.z));
      for (const line of [pts, top]) {
        const l = new THREE.Line(new THREE.BufferGeometry().setFromPoints(line), ring);
        l.renderOrder = 10;
        group.add(l);
      }
    }
    const ball = new THREE.SphereGeometry(0.6, 12, 8);
    const red = new THREE.MeshBasicMaterial({ color: 0xff3b3b, depthTest: false, transparent: true, opacity: 0.9 });
    for (const i of this.data.islands) {
      const m = new THREE.Mesh(ball, red);
      m.position.set(i.pos[0], i.pos[2], -i.pos[1]);
      m.renderOrder = 11;
      group.add(m);
    }
    return group;
  }
}
