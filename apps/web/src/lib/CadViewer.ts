// Visor del espacio Diseñar: el sólido CAD con sus aristas, el modelo cargado
// como referencia translúcida (para calcar un escaneo) y el sketch en edición.
//
// La escena está en coordenadas del visor (Y arriba, unidades de la escena).
// El documento CAD está en mm con Z arriba: `toView` convierte.

import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { THEME_EVENT, themeHex } from "./theme";
import type { CadMesh, P2, P3, Plane, Region, Sketch } from "./cad";
import type { MeshData } from "./Viewer3D";

export type CadPick =
  | { kind: "face"; face: number; point: THREE.Vector3 }
  | { kind: "edge"; edge: number; point: THREE.Vector3 }
  | { kind: "scan"; triangle: number; point: THREE.Vector3 };

export interface SketchOverlay {
  plane: Plane;
  sketch: Sketch;
  regions: Region[];
  /** Entidades resaltadas (seleccionadas) */
  selected?: number[];
  /** Puntos libres (todavía se pueden mover) */
  freePoints?: number[];
  /** Vista previa de lo que se está dibujando (coordenadas del sketch) */
  preview?: P2[][];
}

const BODY_COLOR = 0x9aa4b8;

function planeToWorld(plane: Plane, p: P2): P3 {
  const [o, n, x] = [plane.origin, plane.normal, plane.x_dir];
  const y: P3 = [n[1] * x[2] - n[2] * x[1], n[2] * x[0] - n[0] * x[2], n[0] * x[1] - n[1] * x[0]];
  return [o[0] + x[0] * p[0] + y[0] * p[1], o[1] + x[1] * p[0] + y[1] * p[1], o[2] + x[2] * p[0] + y[2] * p[1]];
}

export class CadViewer {
  private renderer: THREE.WebGLRenderer;
  private scene = new THREE.Scene();
  private camera: THREE.PerspectiveCamera;
  private controls: OrbitControls;
  private raycaster = new THREE.Raycaster();
  private resizeObserver: ResizeObserver;
  private frame = 0;

  private body?: THREE.Mesh;
  private bodyEdges?: THREE.LineSegments;
  private bodyData: CadMesh | null = null;
  private scan?: THREE.Mesh;
  private scanHighlight?: THREE.Mesh;
  private sketchGroup = new THREE.Group();
  private grid: THREE.GridHelper;
  private highlightedFaces = new Set<number>();
  private highlightedEdges = new Set<number>();

  /** mm por unidad de la escena */
  mmPerUnit = 1;
  scanOpacity = 0.35;
  showScan = true;

  constructor(private container: HTMLElement) {
    this.renderer = new THREE.WebGLRenderer({ antialias: true });
    this.renderer.setPixelRatio(window.devicePixelRatio);
    container.appendChild(this.renderer.domElement);
    this.renderer.domElement.style.display = "block";

    this.camera = new THREE.PerspectiveCamera(40, 1, 0.1, 100000);
    this.camera.position.set(150, 120, 200);
    this.controls = new OrbitControls(this.camera, this.renderer.domElement);
    this.controls.enableDamping = false;
    // Como el visor principal: botón del medio orbita, Mayús+medio desplaza
    this.controls.mouseButtons = { LEFT: null as unknown as THREE.MOUSE, MIDDLE: THREE.MOUSE.ROTATE, RIGHT: THREE.MOUSE.PAN };
    this.controls.addEventListener("change", () => this.requestRender());
    // Alt + izquierdo también orbita (el izquierdo solo queda para elegir y dibujar)
    this.renderer.domElement.addEventListener(
      "pointerdown",
      (e) => {
        this.controls.mouseButtons.LEFT = e.altKey ? THREE.MOUSE.ROTATE : (null as unknown as THREE.MOUSE);
      },
      { capture: true },
    );

    this.scene.add(new THREE.HemisphereLight(0xffffff, 0x444455, 1.6));
    const key = new THREE.DirectionalLight(0xffffff, 1.6);
    key.position.set(1, 2, 1.5);
    this.camera.add(key);
    this.scene.add(this.camera);

    this.grid = new THREE.GridHelper(400, 40);
    this.scene.add(this.grid);
    this.scene.add(this.sketchGroup);
    this.applyTheme();
    window.addEventListener(THEME_EVENT, this.onTheme);

    this.resizeObserver = new ResizeObserver(() => this.resize());
    this.resizeObserver.observe(container);
    this.resize();
  }

  private onTheme = () => {
    this.applyTheme();
    this.requestRender();
  };

  private applyTheme() {
    this.scene.background = new THREE.Color(themeHex("viewport"));
    const mats = (Array.isArray(this.grid.material) ? this.grid.material : [this.grid.material]) as THREE.LineBasicMaterial[];
    mats.forEach((m) => {
      m.color = new THREE.Color(themeHex("grid-minor"));
      m.vertexColors = false;
    });
    if (this.bodyEdges) (this.bodyEdges.material as THREE.LineBasicMaterial).color.set(themeHex("bg-darker"));
  }

  private resize() {
    const w = this.container.clientWidth;
    const h = this.container.clientHeight;
    if (w === 0 || h === 0) return;
    this.renderer.setSize(w, h);
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
    this.requestRender();
  }

  requestRender() {
    if (this.frame) return;
    this.frame = requestAnimationFrame(() => {
      this.frame = 0;
      this.renderer.render(this.scene, this.camera);
    });
  }

  /** Punto del CAD (mm, Z arriba) → visor (Y arriba, unidades de la escena) */
  toView(p: P3): THREE.Vector3 {
    const s = 1 / this.mmPerUnit;
    return new THREE.Vector3(p[0] * s, p[2] * s, -p[1] * s);
  }

  /** Dirección del CAD → visor */
  dirToView(d: P3): THREE.Vector3 {
    return new THREE.Vector3(d[0], d[2], -d[1]).normalize();
  }

  /** Visor → CAD (mm, Z arriba) */
  fromView(v: THREE.Vector3): P3 {
    const s = this.mmPerUnit;
    return [v.x * s, -v.z * s, v.y * s];
  }

  // ─── Modelo de referencia (escaneo) ─────────────────────────────────────

  setScan(data: MeshData | null | undefined) {
    if (this.scan) {
      this.scene.remove(this.scan);
      this.scan.geometry.dispose();
      (this.scan.material as THREE.Material).dispose();
      this.scan = undefined;
    }
    this.setScanHighlight([]);
    if (data) {
      const g = new THREE.BufferGeometry();
      g.setAttribute("position", new THREE.BufferAttribute(data.positions, 3));
      g.setAttribute("normal", new THREE.BufferAttribute(data.normals, 3));
      g.setIndex(new THREE.BufferAttribute(data.indices, 1));
      g.computeBoundingSphere();
      const m = new THREE.MeshStandardMaterial({
        color: themeHex("comment"),
        transparent: true,
        opacity: this.scanOpacity,
        depthWrite: false,
        side: THREE.DoubleSide,
      });
      this.scan = new THREE.Mesh(g, m);
      this.scan.renderOrder = 2;
      this.scan.visible = this.showScan;
      this.scene.add(this.scan);
    }
    this.requestRender();
  }

  setScanVisible(visible: boolean, opacity?: number) {
    this.showScan = visible;
    if (opacity !== undefined) this.scanOpacity = opacity;
    if (this.scan) {
      this.scan.visible = visible;
      (this.scan.material as THREE.MeshStandardMaterial).opacity = this.scanOpacity;
    }
    this.requestRender();
  }

  /** Resalta triángulos del escaneo (la zona que tomó un clic) */
  setScanHighlight(triangles: number[]) {
    if (this.scanHighlight) {
      this.scene.remove(this.scanHighlight);
      this.scanHighlight.geometry.dispose();
      this.scanHighlight = undefined;
    }
    if (!this.scan || triangles.length === 0) return this.requestRender();
    const src = this.scan.geometry;
    const pos = src.getAttribute("position") as THREE.BufferAttribute;
    const idx = src.getIndex()!;
    const out = new Float32Array(triangles.length * 9);
    triangles.forEach((t, k) => {
      for (let c = 0; c < 3; c++) {
        const v = idx.getX(t * 3 + c);
        out.set([pos.getX(v), pos.getY(v), pos.getZ(v)], k * 9 + c * 3);
      }
    });
    const g = new THREE.BufferGeometry();
    g.setAttribute("position", new THREE.BufferAttribute(out, 3));
    const m = new THREE.MeshBasicMaterial({
      color: themeHex("cyan"),
      transparent: true,
      opacity: 0.55,
      depthTest: false,
      side: THREE.DoubleSide,
    });
    this.scanHighlight = new THREE.Mesh(g, m);
    this.scanHighlight.renderOrder = 5;
    this.scene.add(this.scanHighlight);
    this.requestRender();
  }

  // ─── Sólido ─────────────────────────────────────────────────────────────

  setBody(data: CadMesh | null) {
    if (this.body) {
      this.scene.remove(this.body);
      this.body.geometry.dispose();
      (this.body.material as THREE.Material).dispose();
      this.body = undefined;
    }
    if (this.bodyEdges) {
      this.scene.remove(this.bodyEdges);
      this.bodyEdges.geometry.dispose();
      (this.bodyEdges.material as THREE.Material).dispose();
      this.bodyEdges = undefined;
    }
    this.bodyData = data;
    if (data) {
      const g = new THREE.BufferGeometry();
      g.setAttribute("position", new THREE.BufferAttribute(data.positions, 3));
      g.setAttribute("normal", new THREE.BufferAttribute(data.normals, 3));
      g.setAttribute("color", new THREE.BufferAttribute(new Float32Array(data.positions.length), 3));
      g.setIndex(new THREE.BufferAttribute(data.indices, 1));
      g.computeBoundingSphere();
      const m = new THREE.MeshStandardMaterial({
        vertexColors: true,
        metalness: 0.1,
        roughness: 0.55,
        polygonOffset: true,
        polygonOffsetFactor: 1,
        polygonOffsetUnits: 1,
      });
      this.body = new THREE.Mesh(g, m);
      this.body.renderOrder = 1;
      this.scene.add(this.body);

      // Aristas: cada polilínea en segmentos, con el índice de arista por segmento
      const segs: number[] = [];
      const segEdge: number[] = [];
      let start = 0;
      data.edgeEnds.forEach((end, e) => {
        for (let i = start; i < end - 1; i++) {
          segs.push(...data.edgePoints.slice(i * 3, i * 3 + 6));
          segEdge.push(e);
        }
        start = end;
      });
      const eg = new THREE.BufferGeometry();
      eg.setAttribute("position", new THREE.BufferAttribute(new Float32Array(segs), 3));
      eg.setAttribute("color", new THREE.BufferAttribute(new Float32Array(segs.length), 3));
      eg.userData.segEdge = segEdge;
      this.bodyEdges = new THREE.LineSegments(eg, new THREE.LineBasicMaterial({ vertexColors: true }));
      this.bodyEdges.renderOrder = 3;
      this.scene.add(this.bodyEdges);
    }
    this.paintBody();
    this.requestRender();
  }

  /** Caras y aristas resaltadas (selección de referencias) */
  setHighlight(faces: number[], edges: number[]) {
    this.highlightedFaces = new Set(faces);
    this.highlightedEdges = new Set(edges);
    this.paintBody();
    this.requestRender();
  }

  private paintBody() {
    if (this.body && this.bodyData) {
      const base = new THREE.Color(BODY_COLOR);
      const hi = new THREE.Color(themeHex("accent"));
      const colors = this.body.geometry.getAttribute("color") as THREE.BufferAttribute;
      const idx = this.bodyData.indices;
      for (let t = 0; t < this.bodyData.triangleFace.length; t++) {
        const c = this.highlightedFaces.has(this.bodyData.triangleFace[t]) ? hi : base;
        for (let k = 0; k < 3; k++) colors.setXYZ(idx[t * 3 + k], c.r, c.g, c.b);
      }
      colors.needsUpdate = true;
    }
    if (this.bodyEdges) {
      const segEdge: number[] = this.bodyEdges.geometry.userData.segEdge;
      const base = new THREE.Color(themeHex("bg-darker"));
      const hi = new THREE.Color(themeHex("orange"));
      const colors = this.bodyEdges.geometry.getAttribute("color") as THREE.BufferAttribute;
      segEdge.forEach((e, s) => {
        const c = this.highlightedEdges.has(e) ? hi : base;
        colors.setXYZ(s * 2, c.r, c.g, c.b);
        colors.setXYZ(s * 2 + 1, c.r, c.g, c.b);
      });
      colors.needsUpdate = true;
    }
  }

  // ─── Sketch ─────────────────────────────────────────────────────────────

  setSketch(overlay: SketchOverlay | null) {
    for (const child of [...this.sketchGroup.children]) {
      this.sketchGroup.remove(child);
      const o = child as THREE.Mesh;
      o.geometry?.dispose();
      (o.material as THREE.Material | undefined)?.dispose();
    }
    if (!overlay) return this.requestRender();
    const { plane, sketch } = overlay;
    const w = (p: P2) => this.toView(planeToWorld(plane, p));
    const point = new Map(sketch.points.map((p) => [p.id, [p.x, p.y] as P2]));
    const selected = new Set(overlay.selected ?? []);

    // Regiones cerradas, rellenas apenas
    for (const r of overlay.regions.filter((r) => r.depth % 2 === 0)) {
      const shape = new THREE.Shape(r.outer.polygon.map((p) => new THREE.Vector2(p[0], p[1])));
      shape.holes = r.holes.map((h) => new THREE.Path(h.polygon.map((p) => new THREE.Vector2(p[0], p[1]))));
      const g = new THREE.ShapeGeometry(shape);
      const pos = g.getAttribute("position") as THREE.BufferAttribute;
      for (let i = 0; i < pos.count; i++) {
        const v = w([pos.getX(i), pos.getY(i)]);
        pos.setXYZ(i, v.x, v.y, v.z);
      }
      const m = new THREE.MeshBasicMaterial({ color: themeHex("accent"), transparent: true, opacity: 0.12, side: THREE.DoubleSide, depthWrite: false });
      const mesh = new THREE.Mesh(g, m);
      mesh.renderOrder = 6;
      this.sketchGroup.add(mesh);
    }

    const lines = (pts: (P2 | undefined)[], color: number, dashed = false) => {
      if (pts.some((p) => !p)) return;
      const g = new THREE.BufferGeometry().setFromPoints((pts as P2[]).map(w));
      const m = dashed
        ? new THREE.LineDashedMaterial({ color, dashSize: 2 / this.mmPerUnit, gapSize: 1.5 / this.mmPerUnit, depthTest: false })
        : new THREE.LineBasicMaterial({ color, depthTest: false });
      const l = new THREE.Line(g, m);
      if (dashed) l.computeLineDistances();
      l.renderOrder = 7;
      this.sketchGroup.add(l);
    };
    const normal = themeHex("fg");
    const construction = themeHex("comment");
    const sel = themeHex("orange");
    for (const e of sketch.entities) {
      const g = e.geometry;
      const color = selected.has(e.id) ? sel : e.construction ? construction : normal;
      let pts: P2[] = [];
      if (g.type === "line") pts = [point.get(g.start)!, point.get(g.end)!];
      else if (g.type === "circle") {
        const c = point.get(g.center)!;
        pts = Array.from({ length: 65 }, (_, i) => {
          const a = (i / 64) * Math.PI * 2;
          return [c[0] + g.radius * Math.cos(a), c[1] + g.radius * Math.sin(a)];
        });
      } else if (g.type === "arc") {
        const [c, a, b] = [point.get(g.center)!, point.get(g.start)!, point.get(g.end)!];
        const r = Math.hypot(a[0] - c[0], a[1] - c[1]);
        const a0 = Math.atan2(a[1] - c[1], a[0] - c[0]);
        let sweep = Math.atan2(b[1] - c[1], b[0] - c[0]) - a0;
        while (sweep <= 1e-9) sweep += Math.PI * 2;
        pts = Array.from({ length: 33 }, (_, i) => {
          const t = a0 + (sweep * i) / 32;
          return [c[0] + r * Math.cos(t), c[1] + r * Math.sin(t)];
        });
      } else {
        pts = g.points.map((p) => point.get(p)!);
        if (g.closed && pts.length) pts.push(pts[0]);
      }
      if (pts.every(Boolean)) lines(pts, color, !!e.construction);
    }
    for (const pv of overlay.preview ?? []) lines(pv, sel);

    // Puntos: libres en amarillo, definidos en verde
    const free = new Set(overlay.freePoints ?? []);
    const pos: number[] = [];
    const col: number[] = [];
    for (const p of sketch.points) {
      const v = w([p.x, p.y]);
      pos.push(v.x, v.y, v.z);
      const c = new THREE.Color(free.has(p.id) ? themeHex("yellow") : themeHex("green"));
      col.push(c.r, c.g, c.b);
    }
    const pg = new THREE.BufferGeometry();
    pg.setAttribute("position", new THREE.Float32BufferAttribute(pos, 3));
    pg.setAttribute("color", new THREE.Float32BufferAttribute(col, 3));
    const pts = new THREE.Points(pg, new THREE.PointsMaterial({ size: 7, sizeAttenuation: false, vertexColors: true, depthTest: false }));
    pts.renderOrder = 8;
    this.sketchGroup.add(pts);

    // Ejes del plano
    const axisLen = 20;
    lines([[0, 0], [axisLen, 0]], themeHex("axis-x"));
    lines([[0, 0], [0, axisLen]], themeHex("axis-y"));
    this.requestRender();
  }

  // ─── Cámara ─────────────────────────────────────────────────────────────

  /** Mira el plano de frente (para dibujar) */
  lookAtPlane(plane: Plane) {
    const target = this.toView(plane.origin);
    const n = this.dirToView(plane.normal);
    const dist = this.camera.position.distanceTo(this.controls.target) || 200 / this.mmPerUnit;
    this.controls.target.copy(target);
    this.camera.position.copy(target).addScaledVector(n, dist);
    // Que la "y" del sketch quede hacia arriba en pantalla
    const x = this.dirToView(plane.x_dir);
    this.camera.up.copy(new THREE.Vector3().crossVectors(n, x).normalize());
    this.camera.lookAt(target);
    this.controls.update();
    this.requestRender();
  }

  resetUp() {
    this.camera.up.set(0, 1, 0);
    this.controls.update();
  }

  /** Encuadra todo lo visible */
  frameAll() {
    const box = new THREE.Box3();
    for (const o of [this.body, this.scan]) if (o?.visible) box.expandByObject(o);
    if (box.isEmpty()) box.setFromCenterAndSize(new THREE.Vector3(), new THREE.Vector3(100, 100, 100).divideScalar(this.mmPerUnit));
    const sphere = box.getBoundingSphere(new THREE.Sphere());
    const dir = this.camera.position.clone().sub(this.controls.target).normalize();
    if (dir.lengthSq() < 1e-9) dir.set(0.6, 0.5, 0.8).normalize();
    const dist = sphere.radius / Math.sin((this.camera.fov * Math.PI) / 360) * 1.1;
    this.controls.target.copy(sphere.center);
    this.camera.position.copy(sphere.center).addScaledVector(dir, dist);
    this.camera.near = dist / 1000;
    this.camera.far = dist * 100;
    this.camera.updateProjectionMatrix();
    const size = Math.max(sphere.radius * 4, 1);
    this.scene.remove(this.grid);
    this.grid.geometry.dispose();
    const step = Math.pow(10, Math.floor(Math.log10(size / 4)));
    this.grid = new THREE.GridHelper(Math.ceil(size / step) * step, Math.ceil(size / step));
    this.scene.add(this.grid);
    this.applyTheme();
    this.controls.update();
    this.requestRender();
  }

  // ─── Elegir ─────────────────────────────────────────────────────────────

  private setRay(clientX: number, clientY: number) {
    const rect = this.renderer.domElement.getBoundingClientRect();
    const ndc = new THREE.Vector2(((clientX - rect.left) / rect.width) * 2 - 1, -((clientY - rect.top) / rect.height) * 2 + 1);
    this.raycaster.setFromCamera(ndc, this.camera);
  }

  /** Qué hay bajo el puntero. `want` limita a sólido (caras/aristas) o escaneo. */
  pick(clientX: number, clientY: number, want: { faces?: boolean; edges?: boolean; scan?: boolean }): CadPick | null {
    this.setRay(clientX, clientY);
    const dist = this.camera.position.distanceTo(this.controls.target);
    if (want.edges && this.bodyEdges) {
      this.raycaster.params.Line = { threshold: dist * 0.006 };
      const hit = this.raycaster.intersectObject(this.bodyEdges)[0];
      if (hit && hit.index != null) {
        const segEdge: number[] = this.bodyEdges.geometry.userData.segEdge;
        return { kind: "edge", edge: segEdge[hit.index / 2], point: hit.point };
      }
    }
    if (want.faces && this.body) {
      const hit = this.raycaster.intersectObject(this.body)[0];
      if (hit && hit.faceIndex != null && this.bodyData) {
        return { kind: "face", face: this.bodyData.triangleFace[hit.faceIndex], point: hit.point };
      }
    }
    if (want.scan && this.scan?.visible) {
      const hit = this.raycaster.intersectObject(this.scan)[0];
      if (hit && hit.faceIndex != null) return { kind: "scan", triangle: hit.faceIndex, point: hit.point };
    }
    return null;
  }

  /** Punto del puntero sobre un plano del CAD, en coordenadas del sketch (mm) */
  planePoint(clientX: number, clientY: number, plane: Plane): P2 | null {
    this.setRay(clientX, clientY);
    const n = this.dirToView(plane.normal);
    const o = this.toView(plane.origin);
    const p = new THREE.Plane().setFromNormalAndCoplanarPoint(n, o);
    const hit = this.raycaster.ray.intersectPlane(p, new THREE.Vector3());
    if (!hit) return null;
    const w = this.fromView(hit);
    const d: P3 = [w[0] - plane.origin[0], w[1] - plane.origin[1], w[2] - plane.origin[2]];
    const x = plane.x_dir;
    const nn = plane.normal;
    const y: P3 = [nn[1] * x[2] - nn[2] * x[1], nn[2] * x[0] - nn[0] * x[2], nn[0] * x[1] - nn[1] * x[0]];
    return [d[0] * x[0] + d[1] * x[1] + d[2] * x[2], d[0] * y[0] + d[1] * y[1] + d[2] * y[2]];
  }

  /** Posición en pantalla (coordenadas de cliente) de un punto del CAD */
  screenOf(p: P3): [number, number] {
    const v = this.toView(p).project(this.camera);
    const rect = this.renderer.domElement.getBoundingClientRect();
    return [rect.left + ((v.x + 1) / 2) * rect.width, rect.top + ((1 - v.y) / 2) * rect.height];
  }

  /** Tamaño en mm de un píxel cerca del objetivo (para tolerancias de clic) */
  pixelSizeMm(): number {
    const h = this.renderer.domElement.clientHeight || 1;
    const dist = this.camera.position.distanceTo(this.controls.target);
    return ((2 * dist * Math.tan((this.camera.fov * Math.PI) / 360)) / h) * this.mmPerUnit;
  }

  get canvas(): HTMLCanvasElement {
    return this.renderer.domElement;
  }

  dispose() {
    cancelAnimationFrame(this.frame);
    window.removeEventListener(THEME_EVENT, this.onTheme);
    this.resizeObserver.disconnect();
    this.controls.dispose();
    this.setBody(null);
    this.setScan(null);
    this.setSketch(null);
    this.renderer.dispose();
    this.renderer.domElement.remove();
  }
}
