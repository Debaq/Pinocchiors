// Visor del espacio Diseñar: el sólido CAD con sus aristas, el modelo cargado
// como referencia translúcida (para calcar un escaneo) y el sketch en edición.
//
// La escena está en coordenadas del visor (Y arriba, unidades de la escena).
// El documento CAD está en mm con Z arriba: `toView` convierte.

import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { NavDrag } from "./navDrag";
import { THEME_EVENT, themeHex } from "./theme";
import { deviationColor, ellipsePolyline, splineOf, type BodyOp, type ScanDeviation, type CadMesh, type P2, type P3, type Plane, type RefView, type Region, type Sketch } from "./cad";
import type { MeshData } from "./Viewer3D";
import { ViewCube } from "./ViewCube";

export type BasePlane = "xy" | "xz" | "yz";

/** Color (0xrrggbb) y visibilidad de una pieza: sus caras y aristas [desde, hasta) del cuerpo */
export interface PartStyle {
  faces: [number, number];
  edges: [number, number];
  color: number;
  hidden: boolean;
}

export type CadPick =
  | { kind: "face"; face: number; point: THREE.Vector3 }
  | { kind: "edge"; edge: number; point: THREE.Vector3 }
  /** Vértice del sólido (extremo de aristas); `at` en mm del CAD */
  | { kind: "vertex"; at: P3; point: THREE.Vector3 }
  | { kind: "scan"; triangle: number; point: THREE.Vector3 }
  /** Región cerrada de un sketch visible (índice en `regions`) */
  | { kind: "region"; sketch: number; region: number; point: THREE.Vector3 }
  /** Uno de los planos base */
  | { kind: "plane"; plane: BasePlane; point: THREE.Vector3 }
  /** Un plano de referencia del historial */
  | { kind: "refplane"; feature: number; point: THREE.Vector3 };

/** Sketch visible fuera de la edición: líneas y regiones elegibles */
export interface VisibleSketch {
  id: number;
  plane: Plane;
  sketch: Sketch;
  regions: Region[];
  /** Índices de las regiones elegidas */
  selected: number[];
}

export interface SketchOverlay {
  plane: Plane;
  sketch: Sketch;
  regions: Region[];
  /** Entidades y puntos resaltados (seleccionados) */
  selected?: number[];
  /** Resaltado pasajero (restricción bajo el mouse en el panel) */
  hover?: number[];
  /** Puntos libres (todavía se pueden mover) */
  freePoints?: number[];
  /** Entidades a las que les falta definir (azul) y en conflicto (rojo) */
  freeEntities?: number[];
  conflictEntities?: number[];
  /** Vista previa de lo que se está dibujando (coordenadas del sketch) */
  preview?: P2[][];
  /** Punto al que se pega el cursor (anclaje activo) */
  snap?: P2;
  /** Líneas guía del anclaje (alineaciones, dirección) */
  guides?: [P2, P2][];
  /** Eligiendo regiones: se muestran todas y se resaltan las que contienen estos puntos */
  chosen?: P2[];
}

function inPolygon(p: P2, poly: P2[]): boolean {
  let inside = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const [a, b] = [poly[i], poly[j]];
    if (a[1] > p[1] !== b[1] > p[1] && p[0] < ((b[0] - a[0]) * (p[1] - a[1])) / (b[1] - a[1]) + a[0]) inside = !inside;
  }
  return inside;
}

export function regionContains(r: Region, p: P2): boolean {
  return inPolygon(p, r.outer.polygon) && !r.holes.some((h) => inPolygon(p, h.polygon));
}

const BODY_COLOR = 0x9aa4b8;

export function planeToWorld(plane: Plane, p: P2): P3 {
  const [o, n, x] = [plane.origin, plane.normal, plane.x_dir];
  const y: P3 = [n[1] * x[2] - n[2] * x[1], n[2] * x[0] - n[0] * x[2], n[0] * x[1] - n[1] * x[0]];
  return [o[0] + x[0] * p[0] + y[0] * p[1], o[1] + x[1] * p[0] + y[1] * p[1], o[2] + x[2] * p[0] + y[2] * p[1]];
}

/** Polilínea de una entidad del sketch (para dibujarla) */
export function entityPolyline(g: Sketch["entities"][number]["geometry"], point: Map<number, P2>): P2[] | undefined {
  if (g.type === "line") {
    const [a, b] = [point.get(g.start), point.get(g.end)];
    return a && b ? [a, b] : undefined;
  }
  if (g.type === "circle") {
    const c = point.get(g.center);
    if (!c) return undefined;
    return Array.from({ length: 65 }, (_, i) => {
      const t = (i / 64) * Math.PI * 2;
      return [c[0] + g.radius * Math.cos(t), c[1] + g.radius * Math.sin(t)] as P2;
    });
  }
  if (g.type === "arc") {
    const [c, a, b] = [point.get(g.center), point.get(g.start), point.get(g.end)];
    if (!c || !a || !b) return undefined;
    const r = Math.hypot(a[0] - c[0], a[1] - c[1]);
    const a0 = Math.atan2(a[1] - c[1], a[0] - c[0]);
    let sweep = Math.atan2(b[1] - c[1], b[0] - c[0]) - a0;
    while (sweep <= 1e-9) sweep += Math.PI * 2;
    return Array.from({ length: 33 }, (_, i) => [c[0] + r * Math.cos(a0 + (sweep * i) / 32), c[1] + r * Math.sin(a0 + (sweep * i) / 32)] as P2);
  }
  if (g.type === "point") return undefined;
  if (g.type === "ellipse") {
    const [c, a, b] = [point.get(g.center), point.get(g.major), point.get(g.minor)];
    return c && a && b ? ellipsePolyline(c, a, b) : undefined;
  }
  return splineOf(g, (id) => point.get(id));
}

export class CadViewer {
  private renderer: THREE.WebGLRenderer;
  private scene = new THREE.Scene();
  private camera: THREE.PerspectiveCamera;
  private controls: OrbitControls;
  /** Girar y desplazar sin tope en los polos (el zoom queda en `controls`) */
  private nav: NavDrag;
  private raycaster = new THREE.Raycaster();
  private resizeObserver: ResizeObserver;
  private frame = 0;

  private body?: THREE.Mesh;
  private bodyEdges?: THREE.LineSegments;
  private bodyData: CadMesh | null = null;
  private tool?: THREE.Group;
  /** Flecha de la operación en edición: el grupo y la punta (se escala con la distancia) */
  private handle?: { group: THREE.Group; tip: THREE.Object3D; origin: THREE.Vector3; dir: THREE.Vector3 };
  private compare?: THREE.Group;
  private scan?: THREE.Mesh;
  private scanHighlight?: THREE.Mesh;
  private sketchGroup = new THREE.Group();
  private sketchesGroup = new THREE.Group();
  private planesGroup = new THREE.Group();
  /** Planos, ejes y puntos de referencia */
  private refsGroup = new THREE.Group();
  private selectedPlanes = new Set<BasePlane>();
  private hiddenPlanes = new Set<BasePlane>();
  /** Lado de los planos base (mm): acompaña el tamaño del modelo */
  planeSize = 60;
  private grid: THREE.GridHelper;
  private highlightedFaces = new Set<number>();
  private highlightedEdges = new Set<number>();
  /** Color y visibilidad de cada pieza (rangos de caras y aristas del cuerpo) */
  private partStyles: PartStyle[] = [];
  private hiddenFaces = new Set<number>();
  private hiddenEdges = new Set<number>();
  /** Cara de cada triángulo dibujado (sin los de piezas ocultas) */
  private visibleTriFace = new Uint32Array();
  /** Vértices del sólido (extremos de aristas), en coordenadas del visor */
  private bodyVertices: THREE.Vector3[] = [];
  /** Vértices elegidos y la línea de la distancia medida */
  private marks = new THREE.Group();
  /** Vista de corte: plano (en el visor) que deja ver lo de su lado positivo, y la tapa */
  private section: THREE.Plane | null = null;
  private sectionGroup = new THREE.Group();
  /** Cubo de vistas en la esquina superior derecha */
  private viewCube = new ViewCube();
  /** Giro de cámara en curso hacia una vista (ver `lookFrom`) */
  private viewTransition: { from: THREE.Vector3; turn: THREE.Quaternion; up: THREE.Vector3; start: number } | null = null;
  /** Centro de masa: tres trazos con los colores de los ejes */
  private centerMark?: THREE.LineSegments;

  /** mm por unidad de la escena */
  mmPerUnit = 1;
  /** Se llama después de dibujar (la cámara pudo moverse): etiquetas HTML encima */
  onRender?: () => void;
  scanOpacity = 0.35;
  showScan = true;

  constructor(private container: HTMLElement) {
    this.renderer = new THREE.WebGLRenderer({ antialias: true, stencil: true });
    this.renderer.localClippingEnabled = true;
    this.renderer.setPixelRatio(window.devicePixelRatio);
    container.appendChild(this.renderer.domElement);
    this.renderer.domElement.style.display = "block";

    this.camera = new THREE.PerspectiveCamera(40, 1, 0.1, 100000);
    this.camera.position.set(150, 120, 200);
    this.controls = new OrbitControls(this.camera, this.renderer.domElement);
    this.controls.enableDamping = false;
    // Girar y desplazar van por `nav`; los controles solo hacen zoom con la rueda
    const none = null as unknown as THREE.MOUSE;
    this.controls.mouseButtons = { LEFT: none, MIDDLE: none, RIGHT: none };
    this.controls.addEventListener("change", () => this.requestRender());
    this.nav = new NavDrag(this.camera, this.controls, this.renderer.domElement, () => this.requestRender());
    // Como el visor principal: botón del medio orbita, Mayús+medio desplaza, el
    // derecho desplaza; Alt + izquierdo también orbita (el izquierdo solo queda
    // para elegir y dibujar)
    this.renderer.domElement.addEventListener(
      "pointerdown",
      (e) => {
        if (e.button === 1 || (e.button === 0 && e.altKey)) this.nav.start(e, e.shiftKey ? "pan" : "rotate");
        else if (e.button === 2) this.nav.start(e, "pan");
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
    this.scene.add(this.sketchesGroup);
    this.scene.add(this.planesGroup);
    this.scene.add(this.refsGroup);
    this.marks.renderOrder = 6;
    this.scene.add(this.marks);
    this.scene.add(this.sectionGroup);
    this.buildPlanes();
    this.applyTheme();
    window.addEventListener(THEME_EVENT, this.onTheme);

    this.resizeObserver = new ResizeObserver(() => this.resize());
    this.resizeObserver.observe(container);
    this.resize();
  }

  private onTheme = () => {
    this.applyTheme();
    this.viewCube.applyTheme();
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
      this.stepViewTransition();
      this.scaleHandle();
      this.renderer.render(this.scene, this.camera);
      this.viewCube.render(this.renderer, this.camera, this.controls.target);
      this.onRender?.();
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

  /**
   * Colorea el escaneo con la desviación por triángulo (promediada en cada
   * vértice) o le devuelve su color con `null`.
   */
  setScanDeviation(dev: ScanDeviation | null) {
    if (!this.scan) return;
    const g = this.scan.geometry;
    const m = this.scan.material as THREE.MeshStandardMaterial;
    if (!dev) {
      g.deleteAttribute("color");
      m.vertexColors = false;
      m.color.set(themeHex("comment"));
      m.opacity = this.scanOpacity;
      m.needsUpdate = true;
      return this.requestRender();
    }
    const idx = g.getIndex()!;
    const nv = (g.getAttribute("position") as THREE.BufferAttribute).count;
    const sum = new Float32Array(nv);
    const cnt = new Uint16Array(nv);
    for (let t = 0; t < dev.perFace.length && t * 3 + 2 < idx.count; t++) {
      for (let c = 0; c < 3; c++) {
        const v = idx.getX(t * 3 + c);
        sum[v] += dev.perFace[t];
        cnt[v]++;
      }
    }
    const tol = dev.stats.tolerance;
    const range = Math.max(dev.stats.p95, tol * 3);
    const colors = new Float32Array(nv * 3);
    for (let v = 0; v < nv; v++) {
      const [r, gg, b] = deviationColor(cnt[v] ? sum[v] / cnt[v] : 0, tol, range);
      colors.set([r, gg, b], v * 3);
    }
    g.setAttribute("color", new THREE.BufferAttribute(colors, 3));
    m.vertexColors = true;
    m.color.set(0xffffff);
    m.opacity = Math.max(this.scanOpacity, 0.9);
    m.needsUpdate = true;
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
    this.bodyData = data;
    this.fitPlanes(data?.positions);
    this.buildBody();
  }

  /**
   * Color y visibilidad de cada pieza: rangos de caras y aristas del cuerpo.
   * Las ocultas no se dibujan ni se pueden elegir.
   */
  setParts(styles: PartStyle[]) {
    this.partStyles = styles;
    this.buildBody();
  }

  /** Arma la malla y las aristas del cuerpo con las piezas visibles */
  private buildBody() {
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
    this.bodyVertices = [];
    const data = this.bodyData;
    this.hiddenFaces = new Set();
    const hiddenEdges = (this.hiddenEdges = new Set<number>());
    for (const p of this.partStyles) {
      if (!p.hidden) continue;
      for (let f = p.faces[0]; f < p.faces[1]; f++) this.hiddenFaces.add(f);
      for (let e = p.edges[0]; e < p.edges[1]; e++) hiddenEdges.add(e);
    }
    if (data) {
      // Triángulos de las caras visibles (y de qué cara es cada uno, para elegir)
      const idx: number[] = [];
      const triFace: number[] = [];
      for (let t = 0; t < data.triangleFace.length; t++) {
        if (this.hiddenFaces.has(data.triangleFace[t])) continue;
        idx.push(data.indices[t * 3], data.indices[t * 3 + 1], data.indices[t * 3 + 2]);
        triFace.push(data.triangleFace[t]);
      }
      this.visibleTriFace = Uint32Array.from(triFace);
      const g = new THREE.BufferGeometry();
      g.setAttribute("position", new THREE.BufferAttribute(data.positions, 3));
      g.setAttribute("normal", new THREE.BufferAttribute(data.normals, 3));
      g.setAttribute("color", new THREE.BufferAttribute(new Float32Array(data.positions.length), 3));
      g.setIndex(new THREE.BufferAttribute(Uint32Array.from(idx), 1));
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

      // Vértices: extremos de las aristas visibles, sin repetir
      const ends: THREE.Vector3[] = [];
      let first = 0;
      data.edgeEnds.forEach((end, e) => {
        if (end > first && !hiddenEdges.has(e)) {
          for (const i of [first, end - 1]) {
            const v = new THREE.Vector3(data.edgePoints[i * 3], data.edgePoints[i * 3 + 1], data.edgePoints[i * 3 + 2]);
            if (!ends.some((w) => w.distanceToSquared(v) < 1e-12)) ends.push(v);
          }
        }
        first = end;
      });
      this.bodyVertices = ends;

      // Aristas: cada polilínea en segmentos, con el índice de arista por segmento
      const segs: number[] = [];
      const segEdge: number[] = [];
      let start = 0;
      data.edgeEnds.forEach((end, e) => {
        if (!hiddenEdges.has(e)) {
          for (let i = start; i < end - 1; i++) {
            segs.push(...data.edgePoints.slice(i * 3, i * 3 + 6));
            segEdge.push(e);
          }
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
    this.applySection();
    this.requestRender();
  }

  /**
   * Herramienta de la operación en edición, translúcida como en Onshape: verde
   * si suma, roja si resta, ámbar si interseca.
   */
  setTool(data: CadMesh | null, op: BodyOp = "join") {
    if (this.tool) this.dropGroup(this.tool);
    this.tool = undefined;
    if (data) {
      this.tool = this.ghost(data, themeHex(op === "join" ? "green" : op === "cut" ? "red" : op === "new" ? "cyan" : "warning"));
      this.scene.add(this.tool);
    }
    this.requestRender();
  }

  /**
   * Flecha de la operación (mm, Z arriba): desde `origin` hacia `dir` hasta
   * `length`, con la punta del mismo tamaño en pantalla; `hot` la resalta
   * (bajo el puntero o arrastrándola). `null` la quita.
   */
  setHandle(h: { origin: P3; dir: P3; length: number } | null, hot = false) {
    if (this.handle) this.dropGroup(this.handle.group);
    this.handle = undefined;
    if (h) {
      const origin = this.toView(h.origin);
      const dir = this.dirToView(h.dir);
      const tipAt = origin.clone().addScaledVector(dir, h.length / this.mmPerUnit);
      const color = themeHex(hot ? "warning" : "accent");
      const group = new THREE.Group();
      const shaft = new THREE.Line(
        new THREE.BufferGeometry().setFromPoints([origin, tipAt]),
        new THREE.LineBasicMaterial({ color, depthTest: false, transparent: true }),
      );
      shaft.renderOrder = 10;
      // Punta de alto 1 apuntando a +Y, con su base en el origen; la escala la pone del tamaño en pantalla
      const cone = new THREE.ConeGeometry(0.32, 1, 20);
      cone.translate(0, 0.5, 0);
      const tip = new THREE.Mesh(cone, new THREE.MeshBasicMaterial({ color, depthTest: false, transparent: true }));
      tip.position.copy(tipAt);
      tip.quaternion.setFromUnitVectors(new THREE.Vector3(0, 1, 0), dir);
      tip.renderOrder = 11;
      // Para pinchar: una esfera invisible más grande que la punta
      const hit = new THREE.Mesh(new THREE.SphereGeometry(0.9, 12, 8), new THREE.MeshBasicMaterial({ visible: false }));
      hit.position.y = 0.5;
      tip.add(hit);
      group.add(shaft, tip);
      this.scene.add(group);
      this.handle = { group, tip, origin, dir };
    }
    this.requestRender();
  }

  /** La punta de la flecha mide unos 30 px de alto en pantalla */
  private scaleHandle() {
    if (!this.handle) return;
    const h = this.renderer.domElement.clientHeight || 1;
    const dist = this.camera.position.distanceTo(this.handle.tip.position);
    const px = (2 * dist * Math.tan((this.camera.fov * Math.PI) / 360)) / h;
    this.handle.tip.scale.setScalar(30 * px);
  }

  /** ¿El puntero está sobre la punta de la flecha? */
  handleHit(clientX: number, clientY: number): boolean {
    if (!this.handle) return false;
    this.scaleHandle();
    this.handle.tip.updateMatrixWorld(true);
    this.setRay(clientX, clientY);
    return this.raycaster.intersectObject(this.handle.tip, true).length > 0;
  }

  /**
   * Dónde queda el puntero a lo largo de la línea de la flecha: mm desde su
   * origen (negativo, del otro lado); `null` si la línea apunta a la cámara.
   */
  handleParam(clientX: number, clientY: number): number | null {
    if (!this.handle) return null;
    this.setRay(clientX, clientY);
    const { origin: o, dir: d } = this.handle;
    const { origin: r, direction: v } = this.raycaster.ray;
    // Punto de la línea más cercano al rayo
    const w = o.clone().sub(r);
    const b = d.dot(v);
    const den = 1 - b * b;
    if (den < 1e-4) return null;
    return ((b * v.dot(w) - d.dot(w)) / den) * this.mmPerUnit;
  }

  /**
   * Comparación con una versión: lo agregado desde entonces en verde y lo
   * quitado en rojo, translúcidos sobre el sólido.
   */
  setComparison(added: CadMesh | null, removed: CadMesh | null) {
    if (this.compare) this.dropGroup(this.compare);
    this.compare = undefined;
    if (added || removed) {
      this.compare = new THREE.Group();
      if (added) this.compare.add(this.ghost(added, themeHex("green"), 0.45));
      if (removed) this.compare.add(this.ghost(removed, themeHex("red"), 0.45));
      this.scene.add(this.compare);
    }
    this.requestRender();
  }

  private dropGroup(group: THREE.Group) {
    this.scene.remove(group);
    group.traverse((o) => {
      if (o instanceof THREE.Mesh || o instanceof THREE.Line) {
        o.geometry.dispose();
        (o.material as THREE.Material).dispose();
      }
    });
  }

  /** Malla translúcida con sus aristas siempre visibles */
  private ghost(data: CadMesh, color: number, opacity = 0.22): THREE.Group {
    const g = new THREE.BufferGeometry();
    g.setAttribute("position", new THREE.BufferAttribute(data.positions, 3));
    g.setAttribute("normal", new THREE.BufferAttribute(data.normals, 3));
    g.setIndex(new THREE.BufferAttribute(data.indices, 1));
    g.computeBoundingSphere();
    // Desplazada hacia la cámara: lo agregado coincide con las caras del sólido
    const mesh = new THREE.Mesh(
      g,
      new THREE.MeshBasicMaterial({
        color,
        transparent: true,
        opacity,
        depthWrite: false,
        side: THREE.DoubleSide,
        polygonOffset: true,
        polygonOffsetFactor: -1,
        polygonOffsetUnits: -4,
      }),
    );
    mesh.renderOrder = 4;
    const segs: number[] = [];
    let start = 0;
    for (const end of data.edgeEnds) {
      for (let i = start; i < end - 1; i++) segs.push(...data.edgePoints.slice(i * 3, i * 3 + 6));
      start = end;
    }
    const eg = new THREE.BufferGeometry();
    eg.setAttribute("position", new THREE.BufferAttribute(new Float32Array(segs), 3));
    const lines = new THREE.LineSegments(eg, new THREE.LineBasicMaterial({ color, transparent: true, opacity: 0.9, depthTest: false }));
    lines.renderOrder = 4;
    const group = new THREE.Group();
    group.add(mesh, lines);
    return group;
  }

  /**
   * Vértices elegidos (puntos) y, si se pasa, el segmento de la distancia
   * mínima medida; todo en mm del CAD.
   */
  setMarks(vertices: P3[], segment?: [P3, P3]) {
    for (const o of [...this.marks.children]) {
      this.marks.remove(o);
      const m = o as THREE.Points | THREE.Line;
      m.geometry.dispose();
      (m.material as THREE.Material).dispose();
    }
    if (vertices.length) {
      const g = new THREE.BufferGeometry().setFromPoints(vertices.map((p) => this.toView(p)));
      const pts = new THREE.Points(g, new THREE.PointsMaterial({ color: themeHex("orange"), size: 9, sizeAttenuation: false, depthTest: false }));
      pts.renderOrder = 6;
      this.marks.add(pts);
    }
    if (segment) {
      const [a, b] = segment.map((p) => this.toView(p));
      const g = new THREE.BufferGeometry().setFromPoints([a, b]);
      const line = new THREE.Line(g, new THREE.LineDashedMaterial({ color: themeHex("cyan"), dashSize: 1, gapSize: 0.6, depthTest: false }));
      line.computeLineDistances();
      // Rayas proporcionales al largo, para que se vean a cualquier escala
      const len = a.distanceTo(b) || 1;
      const mat = line.material as THREE.LineDashedMaterial;
      mat.dashSize = len / 20;
      mat.gapSize = len / 30;
      line.renderOrder = 6;
      const ends = new THREE.Points(g.clone(), new THREE.PointsMaterial({ color: themeHex("cyan"), size: 7, sizeAttenuation: false, depthTest: false }));
      ends.renderOrder = 6;
      this.marks.add(line, ends);
    }
    this.requestRender();
  }

  /** Centro de masa (mm) dibujado como una cruz 3D por encima de todo; `null` lo quita */
  setCenterOfMass(p: P3 | null) {
    if (this.centerMark) {
      this.scene.remove(this.centerMark);
      this.centerMark.geometry.dispose();
      (this.centerMark.material as THREE.Material).dispose();
      this.centerMark = undefined;
    }
    if (p) {
      const c = this.toView(p);
      // Del tamaño de un 6 % del modelo
      const size = (this.body?.geometry.boundingSphere?.radius ?? 10) * 0.12;
      const pts: number[] = [];
      const cols: number[] = [];
      const axes: [THREE.Vector3, number][] = [
        [this.dirToView([1, 0, 0]), themeHex("axis-x")],
        [this.dirToView([0, 1, 0]), themeHex("axis-y")],
        [this.dirToView([0, 0, 1]), themeHex("axis-z")],
      ];
      for (const [d, color] of axes) {
        const a = c.clone().addScaledVector(d, -size);
        const b = c.clone().addScaledVector(d, size);
        pts.push(a.x, a.y, a.z, b.x, b.y, b.z);
        const k = new THREE.Color(color);
        cols.push(k.r, k.g, k.b, k.r, k.g, k.b);
      }
      const g = new THREE.BufferGeometry();
      g.setAttribute("position", new THREE.Float32BufferAttribute(pts, 3));
      g.setAttribute("color", new THREE.Float32BufferAttribute(cols, 3));
      this.centerMark = new THREE.LineSegments(g, new THREE.LineBasicMaterial({ vertexColors: true, depthTest: false }));
      this.centerMark.renderOrder = 7;
      this.scene.add(this.centerMark);
    }
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
      const partColors = this.partStyles.map((p) => ({ faces: p.faces, color: new THREE.Color(p.color) }));
      const faceColor = (f: number) => partColors.find((p) => p.faces[0] <= f && f < p.faces[1])?.color ?? base;
      const colors = this.body.geometry.getAttribute("color") as THREE.BufferAttribute;
      const idx = this.bodyData.indices;
      for (let t = 0; t < this.bodyData.triangleFace.length; t++) {
        const f = this.bodyData.triangleFace[t];
        const c = this.highlightedFaces.has(f) ? hi : faceColor(f);
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
    const hover = new Set(overlay.hover ?? []);

    // Regiones cerradas, rellenas apenas (eligiendo: todas, las elegidas fuerte)
    const choosing = overlay.chosen !== undefined;
    for (const r of overlay.regions.filter((r) => choosing || r.depth % 2 === 0)) {
      // La más interna que contiene el punto es la elegida
      const picked = choosing && overlay.chosen!.some((p) => regionContains(r, p) && !overlay.regions.some((o) => o !== r && o.depth > r.depth && regionContains(o, p)));
      const shape = new THREE.Shape(r.outer.polygon.map((p) => new THREE.Vector2(p[0], p[1])));
      shape.holes = r.holes.map((h) => new THREE.Path(h.polygon.map((p) => new THREE.Vector2(p[0], p[1]))));
      const g = new THREE.ShapeGeometry(shape);
      const pos = g.getAttribute("position") as THREE.BufferAttribute;
      for (let i = 0; i < pos.count; i++) {
        const v = w([pos.getX(i), pos.getY(i)]);
        pos.setXYZ(i, v.x, v.y, v.z);
      }
      const m = new THREE.MeshBasicMaterial({
        color: themeHex(picked ? "cyan" : "accent"),
        transparent: true,
        opacity: picked ? 0.45 : choosing ? 0.08 : 0.12,
        side: THREE.DoubleSide,
        depthWrite: false,
        depthTest: !choosing,
      });
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
    const freeE = new Set(overlay.freeEntities ?? []);
    const conflictE = new Set(overlay.conflictEntities ?? []);
    // Como Onshape: azul = le falta definir, color del texto = definida, rojo = en conflicto
    const entityColor = (id: number, construct: boolean) =>
      hover.has(id)
        ? themeHex("cyan")
        : selected.has(id)
          ? sel
          : conflictE.has(id)
            ? themeHex("error")
            : construct
              ? construction
              : freeE.has(id)
                ? themeHex("sketch-free")
                : normal;
    for (const e of sketch.entities) {
      const g = e.geometry;
      const color = entityColor(e.id, !!e.construction);
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
      } else if (g.type === "spline") {
        pts = splineOf(g, (id) => point.get(id)) ?? [];
        // Manijas: línea punteada del extremo a la manija
        for (const [end, h] of [
          [g.points[0], g.start_handle],
          [g.points[g.points.length - 1], g.end_handle],
        ] as const) {
          if (h !== undefined) lines([point.get(end), point.get(h)], construction, true);
        }
      } else if (g.type === "ellipse") pts = entityPolyline(g, point) ?? [];
      if (pts.length && pts.every(Boolean)) lines(pts, color, !!e.construction);
    }
    for (const pv of overlay.preview ?? []) lines(pv, sel);
    for (const g of overlay.guides ?? []) lines(g, themeHex("cyan"), true);

    // Puntos: libres en amarillo, definidos en verde
    const free = new Set(overlay.freePoints ?? []);
    const pos: number[] = [];
    const col: number[] = [];
    // Origen y anclaje activo: puntos más grandes, aparte
    const marker = (p: P2, color: number, size: number) => {
      const v = w(p);
      const g = new THREE.BufferGeometry();
      g.setAttribute("position", new THREE.Float32BufferAttribute([v.x, v.y, v.z], 3));
      const m = new THREE.Points(g, new THREE.PointsMaterial({ color, size, sizeAttenuation: false, depthTest: false }));
      m.renderOrder = 9;
      this.sketchGroup.add(m);
    };
    for (const p of sketch.points) {
      if (p.id === sketch.origin) {
        marker([p.x, p.y], hover.has(p.id) ? themeHex("cyan") : selected.has(p.id) ? themeHex("orange") : themeHex("purple"), 10);
        continue;
      }
      const v = w([p.x, p.y]);
      pos.push(v.x, v.y, v.z);
      const c = new THREE.Color(
        hover.has(p.id) ? themeHex("cyan") : selected.has(p.id) ? themeHex("orange") : free.has(p.id) ? themeHex("yellow") : themeHex("green"),
      );
      col.push(c.r, c.g, c.b);
    }
    const pg = new THREE.BufferGeometry();
    pg.setAttribute("position", new THREE.Float32BufferAttribute(pos, 3));
    pg.setAttribute("color", new THREE.Float32BufferAttribute(col, 3));
    const pts = new THREE.Points(pg, new THREE.PointsMaterial({ size: 7, sizeAttenuation: false, vertexColors: true, depthTest: false }));
    pts.renderOrder = 8;
    this.sketchGroup.add(pts);
    if (overlay.snap) marker(overlay.snap, themeHex("cyan"), 12);

    // Ejes del plano
    const axisLen = 20;
    lines([[0, 0], [axisLen, 0]], themeHex("axis-x"));
    lines([[0, 0], [0, axisLen]], themeHex("axis-y"));
    this.requestRender();
  }

  // ─── Sketches visibles y planos base ────────────────────────────────────

  private clearGroup(g: THREE.Group) {
    for (const child of [...g.children]) {
      g.remove(child);
      const o = child as THREE.Mesh;
      o.geometry?.dispose();
      (o.material as THREE.Material | undefined)?.dispose();
    }
  }

  /** Sketches que se ven fuera de la edición, con sus regiones elegibles */
  setVisibleSketches(list: VisibleSketch[]) {
    this.clearGroup(this.sketchesGroup);
    for (const vs of list) {
      const w = (p: P2) => this.toView(planeToWorld(vs.plane, p));
      const chosen = new Set(vs.selected);
      vs.regions.forEach((r, index) => {
        const shape = new THREE.Shape(r.outer.polygon.map((p) => new THREE.Vector2(p[0], p[1])));
        shape.holes = r.holes.map((h) => new THREE.Path(h.polygon.map((p) => new THREE.Vector2(p[0], p[1]))));
        const g = new THREE.ShapeGeometry(shape);
        const pos = g.getAttribute("position") as THREE.BufferAttribute;
        for (let i = 0; i < pos.count; i++) {
          const v = w([pos.getX(i), pos.getY(i)]);
          pos.setXYZ(i, v.x, v.y, v.z);
        }
        const picked = chosen.has(index);
        const m = new THREE.MeshBasicMaterial({
          color: themeHex(picked ? "cyan" : "accent"),
          transparent: true,
          opacity: picked ? 0.5 : 0.1,
          side: THREE.DoubleSide,
          depthWrite: false,
          // Delante de la cara donde está dibujado
          polygonOffset: true,
          polygonOffsetFactor: -2,
          polygonOffsetUnits: -2,
        });
        const mesh = new THREE.Mesh(g, m);
        mesh.renderOrder = 4;
        mesh.userData = { sketch: vs.id, region: index };
        this.sketchesGroup.add(mesh);
      });
      const point = new Map(vs.sketch.points.map((p) => [p.id, [p.x, p.y] as P2]));
      for (const e of vs.sketch.entities) {
        const pts = entityPolyline(e.geometry, point);
        if (!pts) continue;
        const g = new THREE.BufferGeometry().setFromPoints(pts.map(w));
        const color = themeHex(e.construction ? "comment" : "fg");
        const l = e.construction
          ? new THREE.Line(g, new THREE.LineDashedMaterial({ color, dashSize: 2 / this.mmPerUnit, gapSize: 1.5 / this.mmPerUnit }))
          : new THREE.Line(g, new THREE.LineBasicMaterial({ color }));
        if (e.construction) l.computeLineDistances();
        l.renderOrder = 5;
        this.sketchesGroup.add(l);
      }
    }
    this.requestRender();
  }

  /**
   * Geometría de referencia: planos como cuadros ámbar translúcidos (elegibles),
   * ejes como líneas punteadas y puntos. `chosen`: planos resaltados.
   */
  setReferences(list: RefView[], chosen: number[] = []) {
    this.clearGroup(this.refsGroup);
    const size = Math.max(this.planeSize * 0.6, 10 / this.mmPerUnit);
    const color = themeHex("warning");
    for (const r of list) {
      if (r.kind === "plane") {
        const n = this.dirToView(r.plane.normal);
        const x = this.dirToView(r.plane.x_dir);
        const y = new THREE.Vector3().crossVectors(n, x).normalize();
        const basis = new THREE.Matrix4().makeBasis(x, y, n).setPosition(this.toView(r.plane.origin));
        const g = new THREE.PlaneGeometry(size, size);
        const picked = chosen.includes(r.id);
        const mesh = new THREE.Mesh(
          g,
          new THREE.MeshBasicMaterial({ color: picked ? themeHex("cyan") : color, transparent: true, opacity: picked ? 0.3 : 0.1, side: THREE.DoubleSide, depthWrite: false }),
        );
        mesh.applyMatrix4(basis);
        mesh.userData = { refPlane: r.id };
        mesh.renderOrder = 2;
        const edges = new THREE.LineSegments(new THREE.EdgesGeometry(g), new THREE.LineBasicMaterial({ color: picked ? themeHex("cyan") : color, transparent: true, opacity: 0.7 }));
        edges.applyMatrix4(basis);
        this.refsGroup.add(mesh, edges);
      } else if (r.kind === "axis") {
        const o = this.toView(r.origin);
        const d = this.dirToView(r.dir).multiplyScalar(size);
        const g = new THREE.BufferGeometry().setFromPoints([o.clone().sub(d), o.clone().add(d)]);
        const line = new THREE.Line(g, new THREE.LineDashedMaterial({ color, dashSize: size / 20, gapSize: size / 40 }));
        line.computeLineDistances();
        line.renderOrder = 5;
        this.refsGroup.add(line);
      } else if (r.kind === "curve") {
        const g = new THREE.BufferGeometry().setFromPoints(r.points.map((p) => this.toView(p)));
        const line = new THREE.Line(g, new THREE.LineBasicMaterial({ color }));
        line.renderOrder = 5;
        this.refsGroup.add(line);
      } else {
        const g = new THREE.BufferGeometry().setFromPoints([this.toView(r.point)]);
        const pts = new THREE.Points(g, new THREE.PointsMaterial({ color, size: 8, sizeAttenuation: false, depthTest: false }));
        pts.renderOrder = 6;
        this.refsGroup.add(pts);
      }
    }
    this.requestRender();
  }

  /** Planos base (planta, frente, lateral), como cuadros translúcidos elegibles */
  private buildPlanes() {
    this.clearGroup(this.planesGroup);
    const size = this.planeSize / this.mmPerUnit;
    const defs: { id: BasePlane; rot: THREE.Euler }[] = [
      // En el visor Y es arriba: planta (XY del CAD) es el plano horizontal
      { id: "xy", rot: new THREE.Euler(-Math.PI / 2, 0, 0) },
      { id: "xz", rot: new THREE.Euler(0, 0, 0) },
      { id: "yz", rot: new THREE.Euler(0, Math.PI / 2, 0) },
    ];
    for (const d of defs) {
      if (this.hiddenPlanes.has(d.id)) continue;
      const picked = this.selectedPlanes.has(d.id);
      const g = new THREE.PlaneGeometry(size, size);
      const m = new THREE.MeshBasicMaterial({
        color: themeHex(picked ? "cyan" : "comment"),
        transparent: true,
        opacity: picked ? 0.35 : 0.06,
        side: THREE.DoubleSide,
        depthWrite: false,
      });
      const mesh = new THREE.Mesh(g, m);
      mesh.rotation.copy(d.rot);
      mesh.userData = { plane: d.id };
      mesh.renderOrder = 2;
      this.planesGroup.add(mesh);
      const edges = new THREE.LineSegments(
        new THREE.EdgesGeometry(g),
        new THREE.LineBasicMaterial({ color: themeHex(picked ? "cyan" : "comment"), transparent: true, opacity: picked ? 0.9 : 0.4 }),
      );
      edges.rotation.copy(d.rot);
      this.planesGroup.add(edges);
    }
    this.requestRender();
  }

  /** Planos base un poco más grandes que el modelo (mínimo 20 mm) */
  private fitPlanes(positions?: Float32Array) {
    let size = 60;
    const scanPos = this.scan?.geometry.getAttribute("position")?.array as Float32Array | undefined;
    for (const pos of [positions, scanPos]) {
      if (!pos || pos.length < 3) continue;
      let m = 0;
      for (let i = 0; i < pos.length; i++) m = Math.max(m, Math.abs(pos[i]));
      size = Math.max(20, 2.4 * m * this.mmPerUnit);
    }
    if (Math.abs(size - this.planeSize) > 1e-9) {
      this.planeSize = size;
      this.buildPlanes();
    }
  }

  /** Grilla del suelo (la misma que se oculta desde el Outliner) */
  setGridVisible(visible: boolean) {
    this.grid.visible = visible;
    this.requestRender();
  }

  setPlanes(visible: boolean, selected: BasePlane[], hidden: BasePlane[] = []) {
    this.selectedPlanes = new Set(selected);
    this.hiddenPlanes = new Set(hidden);
    this.planesGroup.visible = visible;
    this.buildPlanes();
  }

  // ─── Cámara ─────────────────────────────────────────────────────────────

  /** Mira el plano de frente (para dibujar) */
  /** Mira el plano de frente; con `fit` (centro y tamaño en mm, en coordenadas del plano) encuadra eso */
  lookAtPlane(plane: Plane, fit?: { center: [number, number]; size: number }) {
    const y = [
      plane.normal[1] * plane.x_dir[2] - plane.normal[2] * plane.x_dir[1],
      plane.normal[2] * plane.x_dir[0] - plane.normal[0] * plane.x_dir[2],
      plane.normal[0] * plane.x_dir[1] - plane.normal[1] * plane.x_dir[0],
    ];
    const at = fit
      ? ([0, 1, 2].map((i) => plane.origin[i] + plane.x_dir[i] * fit.center[0] + y[i] * fit.center[1]) as [number, number, number])
      : plane.origin;
    const target = this.toView(at);
    const n = this.dirToView(plane.normal);
    const dist = fit
      ? (Math.max(fit.size, 10) / 2 / this.mmPerUnit / Math.tan((this.camera.fov * Math.PI) / 360)) * 1.4
      : this.camera.position.distanceTo(this.controls.target) || 200 / this.mmPerUnit;
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

  private savedView?: { position: THREE.Vector3; target: THREE.Vector3; up: THREE.Vector3 };

  /** Guarda la vista actual (antes de entrar a un sketch) */
  saveView() {
    this.savedView = { position: this.camera.position.clone(), target: this.controls.target.clone(), up: this.camera.up.clone() };
  }

  /** Vuelve a la vista guardada (al salir del sketch) */
  restoreView() {
    const v = this.savedView;
    if (!v) return this.resetUp();
    this.camera.position.copy(v.position);
    this.camera.up.copy(v.up);
    this.controls.target.copy(v.target);
    this.camera.lookAt(v.target);
    this.controls.update();
    this.savedView = undefined;
    this.requestRender();
  }

  // ─── Vistas ─────────────────────────────────────────────────────────────

  /**
   * Lleva la cámara (con un giro corto) a mirar hacia el centro de la vista
   * desde `direction` (en el visor, Y arriba); Z del CAD queda arriba en pantalla.
   */
  private lookFromView(direction: THREE.Vector3) {
    const dir = direction.clone().normalize();
    // Vista cenital o desde abajo: un pelo hacia el frente para que +Y del CAD quede arriba
    if (Math.abs(dir.x) < 1e-9 && Math.abs(dir.z) < 1e-9) dir.z = 1e-3 * Math.sign(dir.y || 1);
    dir.normalize();
    const from = this.camera.position.clone().sub(this.controls.target);
    const turn = new THREE.Quaternion().setFromUnitVectors(from.clone().normalize(), dir);
    this.viewTransition = { from, turn, up: this.camera.up.clone(), start: performance.now() };
    this.requestRender();
  }

  /** Mira desde una dirección del CAD (mm, Z arriba): [0, −1, 0] = de frente */
  lookFrom(direction: P3) {
    this.lookFromView(this.dirToView(direction));
  }

  /** Avanza el giro hacia la vista elegida (~0,3 s, con frenado) */
  private stepViewTransition() {
    const t0 = this.viewTransition;
    if (!t0) return;
    const t = Math.min(1, (performance.now() - t0.start) / 300);
    const eased = 1 - Math.pow(1 - t, 3);
    const turn = new THREE.Quaternion().slerp(t0.turn, eased);
    this.camera.position.copy(this.controls.target).add(t0.from.clone().applyQuaternion(turn));
    // De un sketch se puede venir con la cámara inclinada: vuelve a Y arriba
    this.camera.up.copy(t0.up).lerp(new THREE.Vector3(0, 1, 0), eased).normalize();
    // Desde una vista de cabeza el promedio pasa por cero
    if (this.camera.up.lengthSq() < 0.5) this.camera.up.set(0, 1, 0);
    this.camera.lookAt(this.controls.target);
    this.controls.update();
    // Se llama dentro del cuadro (con `frame` ya en 0): pide el siguiente
    if (t < 1) this.requestRender();
    else this.viewTransition = null;
  }

  /** Clic en el cubo de vistas: gira hacia esa cara, arista o vértice. `true` si lo usó */
  cubeDown(clientX: number, clientY: number): boolean {
    if (!this.viewCube.contains(this.renderer.domElement, clientX, clientY)) return false;
    const dir = this.viewCube.pick(this.renderer.domElement, clientX, clientY);
    if (!dir) return false;
    this.lookFromView(dir);
    return true;
  }

  /** Resalta la zona del cubo bajo el puntero; `true` si está sobre él */
  cubeHover(clientX: number, clientY: number): boolean {
    const dir = this.viewCube.pick(this.renderer.domElement, clientX, clientY);
    if (this.viewCube.setHover(dir)) this.requestRender();
    return dir !== null;
  }

  /**
   * Selección por caja (coordenadas de cliente). `window`: lo que queda entero
   * adentro; si no, lo que la toca (como arrastrar hacia la izquierda en Onshape).
   */
  boxSelect(
    x0: number,
    y0: number,
    x1: number,
    y1: number,
    want: { faces?: boolean; edges?: boolean; vertices?: boolean; regions?: boolean },
    window: boolean,
  ): { faces: number[]; edges: number[]; vertices: P3[]; regions: { sketch: number; region: number }[] } {
    const rect = this.renderer.domElement.getBoundingClientRect();
    const box = { l: Math.min(x0, x1), r: Math.max(x0, x1), t: Math.min(y0, y1), b: Math.max(y0, y1) };
    const v = new THREE.Vector3();
    /** Punto del visor en pantalla (null si queda detrás de la cámara) */
    const screen = (x: number, y: number, z: number): [number, number] | null => {
      v.set(x, y, z).project(this.camera);
      if (v.z > 1) return null;
      return [rect.left + ((v.x + 1) / 2) * rect.width, rect.top + ((1 - v.y) / 2) * rect.height];
    };
    const inBox = (p: [number, number]) => p[0] >= box.l && p[0] <= box.r && p[1] >= box.t && p[1] <= box.b;
    // Cada elemento es una lista de segmentos o triángulos en pantalla: con
    // ventana tienen que estar todos sus puntos adentro; con cruce alcanza que
    // alguno toque la caja
    const out = { faces: [] as number[], edges: [] as number[], vertices: [] as P3[], regions: [] as { sketch: number; region: number }[] };
    const d = this.bodyData;
    const pt = (arr: ArrayLike<number>, i: number) => screen(arr[i * 3], arr[i * 3 + 1], arr[i * 3 + 2]);
    // Solo lo que se ve: un rayo desde la cámara a unos puntos de muestra
    const cam = this.camera.position;
    const ray = new THREE.Raycaster();
    const visible = (p: THREE.Vector3) => {
      if (!this.body) return true;
      const dist = cam.distanceTo(p);
      ray.set(cam, p.clone().sub(cam).normalize());
      ray.far = dist * 1.01;
      if (this.section && this.section.distanceToPoint(p) < 0) return false;
      const hit = this.kept(ray.intersectObject(this.body));
      return !hit || hit.distance >= dist - dist * 2e-3;
    };
    const vec = (arr: ArrayLike<number>, i: number) => new THREE.Vector3(arr[i * 3], arr[i * 3 + 1], arr[i * 3 + 2]);
    if (d && want.faces) {
      const state = new Map<number, { all: boolean; any: boolean }>();
      for (let tri = 0; tri < d.triangleFace.length; tri++) {
        const f = d.triangleFace[tri];
        if (this.hiddenFaces.has(f)) continue;
        const st = state.get(f) ?? { all: true, any: false };
        const ps = [0, 1, 2].map((k) => pt(d.positions, d.indices[tri * 3 + k]));
        if (ps.some((p) => !p)) st.all = false;
        else {
          const tp = ps as [number, number][];
          st.all &&= tp.every(inBox);
          if (!window && !st.any) st.any = triangleTouchesBox(tp, box);
        }
        state.set(f, st);
      }
      // Visible si se ve el centro de alguno de sus triángulos (hasta 8 de muestra)
      const tris = new Map<number, number[]>();
      for (let tri = 0; tri < d.triangleFace.length; tri++) {
        const f = d.triangleFace[tri];
        const st = state.get(f);
        if (!st) continue;
        if (!(window ? st.all : st.any)) continue;
        let list = tris.get(f);
        if (!list) tris.set(f, (list = []));
        list.push(tri);
      }
      for (const [f, list] of tris) {
        const step = Math.max(1, Math.floor(list.length / 8));
        for (let k = 0; k < list.length; k += step) {
          const tri = list[k];
          const c = new THREE.Vector3();
          for (let j = 0; j < 3; j++) c.add(vec(d.positions, d.indices[tri * 3 + j]));
          if (visible(c.divideScalar(3))) {
            out.faces.push(f);
            break;
          }
        }
      }
    }
    const polyline = (arr: ArrayLike<number>, from: number, to: number) => {
      let all = to > from;
      let any = false;
      let prev: [number, number] | null = null;
      for (let i = from; i < to; i++) {
        const p = pt(arr, i);
        if (!p) {
          all = false;
          prev = null;
          continue;
        }
        all &&= inBox(p);
        if (!window && !any) any = inBox(p) || (prev !== null && segmentTouchesBox(prev, p, box));
        prev = p;
      }
      return window ? all : any;
    };
    if (d && want.edges) {
      let start = 0;
      d.edgeEnds.forEach((end, e) => {
        if (!this.hiddenEdges.has(e) && polyline(d.edgePoints, start, end)) {
          // Visible si se ve alguno de sus puntos (hasta 5 de muestra, sin los extremos)
          const n = end - start;
          const samples =
            n <= 2
              ? [vec(d.edgePoints, start).add(vec(d.edgePoints, end - 1)).multiplyScalar(0.5)]
              : [1, 2, 3, 4, 5].map((k) => vec(d.edgePoints, start + Math.round((k * (n - 1)) / 6)));
          if (samples.some(visible)) out.edges.push(e);
        }
        start = end;
      });
    }
    if (want.vertices) for (const p of this.bodyVertices) {
      const sp = screen(p.x, p.y, p.z);
      if (sp && inBox(sp) && visible(p)) out.vertices.push(this.fromView(p));
    }
    if (want.regions) {
      for (const o of this.sketchesGroup.children) {
        const m = o as THREE.Mesh;
        if (!m.isMesh) continue;
        const pos = m.geometry.getAttribute("position") as THREE.BufferAttribute;
        const idx = m.geometry.getIndex();
        const n = idx ? idx.count : pos.count;
        const at = (k: number) => screen(pos.getX(k), pos.getY(k), pos.getZ(k));
        let all = n > 0;
        let any = false;
        for (let k = 0; k + 2 < n; k += 3) {
          const ps = [0, 1, 2].map((j) => at(idx ? idx.getX(k + j) : k + j));
          if (ps.some((p) => !p)) {
            all = false;
            continue;
          }
          const tp = ps as [number, number][];
          all &&= tp.every(inBox);
          if (!window && !any) any = triangleTouchesBox(tp, box);
        }
        if (window ? all : any) out.regions.push({ sketch: m.userData.sketch, region: m.userData.region });
      }
    }
    return out;
  }

  /** Dibuja ya, sin esperar al cuadro (para leer píxeles en las pruebas) */
  renderNow() {
    this.renderer.render(this.scene, this.camera);
    this.viewCube.render(this.renderer, this.camera, this.controls.target);
  }

  /** Primer impacto que no quedó cortado por la vista de corte */
  private kept<T extends THREE.Intersection>(hits: T[]): T | undefined {
    const s = this.section;
    return s ? hits.find((h) => s.distanceToPoint(h.point) >= -1e-6) : hits[0];
  }

  /**
   * Vista de corte por un plano del CAD (punto y normal en mm): se ve lo que
   * queda del lado de la normal y la sección se rellena (tapa). `null` la saca.
   */
  setSection(plane: { origin: P3; normal: P3 } | null) {
    this.section = plane ? new THREE.Plane().setFromNormalAndCoplanarPoint(this.dirToView(plane.normal), this.toView(plane.origin)) : null;
    this.applySection();
    this.requestRender();
  }

  private applySection() {
    const planes = this.section ? [this.section] : [];
    for (const o of [this.body, this.bodyEdges]) if (o) (o.material as THREE.Material).clippingPlanes = planes;
    for (const group of [this.tool, this.compare])
      group?.traverse((o) => {
        if (o instanceof THREE.Mesh || o instanceof THREE.LineSegments) (o.material as THREE.Material).clippingPlanes = planes;
      });
    // Tapa: las caras de atrás suman y las de adelante restan en el stencil;
    // donde queda distinto de cero el plano corta material y se pinta la tapa
    for (const o of [...this.sectionGroup.children]) {
      this.sectionGroup.remove(o);
      const m = o as THREE.Mesh;
      m.geometry.dispose();
      (m.material as THREE.Material).dispose();
    }
    if (!this.section || !this.body) return;
    const geometry = this.body.geometry;
    const base = { depthWrite: false, depthTest: false, colorWrite: false, stencilWrite: true, stencilFunc: THREE.AlwaysStencilFunc, clippingPlanes: planes };
    const back = new THREE.Mesh(
      geometry.clone(),
      new THREE.MeshBasicMaterial({
        ...base,
        side: THREE.BackSide,
        stencilFail: THREE.IncrementWrapStencilOp,
        stencilZFail: THREE.IncrementWrapStencilOp,
        stencilZPass: THREE.IncrementWrapStencilOp,
      }),
    );
    const front = new THREE.Mesh(
      geometry.clone(),
      new THREE.MeshBasicMaterial({
        ...base,
        side: THREE.FrontSide,
        stencilFail: THREE.DecrementWrapStencilOp,
        stencilZFail: THREE.DecrementWrapStencilOp,
        stencilZPass: THREE.DecrementWrapStencilOp,
      }),
    );
    back.renderOrder = front.renderOrder = 0.5;
    const r = (geometry.boundingSphere?.radius ?? 100) * 4;
    const cap = new THREE.Mesh(
      new THREE.PlaneGeometry(r, r),
      new THREE.MeshStandardMaterial({
        color: themeHex("orange"),
        metalness: 0.1,
        roughness: 0.75,
        side: THREE.DoubleSide,
        stencilWrite: true,
        stencilRef: 0,
        stencilFunc: THREE.NotEqualStencilFunc,
        stencilFail: THREE.ReplaceStencilOp,
        stencilZFail: THREE.ReplaceStencilOp,
        stencilZPass: THREE.ReplaceStencilOp,
      }),
    );
    // Sobre el plano, mirando hacia lo que se ve; centrado en el modelo
    const center = this.section.projectPoint(geometry.boundingSphere?.center ?? new THREE.Vector3(), new THREE.Vector3());
    cap.position.copy(center);
    cap.lookAt(center.clone().sub(this.section.normal));
    cap.renderOrder = 0.6;
    cap.onAfterRender = (renderer) => renderer.clearStencil();
    this.sectionGroup.add(back, front, cap);
  }

  /** Acerca la cámara a lo elegido: caras y aristas del sólido y puntos (mm); vacío = todo */
  frameSelection(faces: number[], edges: number[], points: P3[] = []) {
    const box = new THREE.Box3();
    const d = this.bodyData;
    if (d) {
      const fs = new Set(faces);
      for (let t = 0; t < d.triangleFace.length; t++) {
        if (!fs.has(d.triangleFace[t])) continue;
        for (let k = 0; k < 3; k++) {
          const i = d.indices[t * 3 + k];
          box.expandByPoint(new THREE.Vector3(d.positions[i * 3], d.positions[i * 3 + 1], d.positions[i * 3 + 2]));
        }
      }
      const es = new Set(edges);
      let start = 0;
      d.edgeEnds.forEach((end, e) => {
        if (es.has(e)) for (let i = start; i < end; i++) box.expandByPoint(new THREE.Vector3(d.edgePoints[i * 3], d.edgePoints[i * 3 + 1], d.edgePoints[i * 3 + 2]));
        start = end;
      });
    }
    for (const p of points) box.expandByPoint(this.toView(p));
    if (box.isEmpty()) return this.frameAll();
    const sphere = box.getBoundingSphere(new THREE.Sphere());
    // Un punto solo no tiene tamaño: un entorno de 10 mm
    sphere.radius = Math.max(sphere.radius, 5 / this.mmPerUnit);
    const dir = this.camera.position.clone().sub(this.controls.target).normalize();
    const dist = (sphere.radius / Math.sin((this.camera.fov * Math.PI) / 360)) * 1.3;
    this.controls.target.copy(sphere.center);
    this.camera.position.copy(sphere.center).addScaledVector(dir, dist);
    this.camera.near = Math.min(this.camera.near, dist / 1000);
    this.camera.updateProjectionMatrix();
    this.controls.update();
    this.requestRender();
  }

  /** Encuadra todo lo visible */
  frameAll() {
    const box = new THREE.Box3();
    for (const o of [this.body, this.scan]) if (o?.visible) box.expandByObject(o);
    if (this.planesGroup.visible) box.expandByObject(this.planesGroup);
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
    const visible = this.grid.visible;
    this.grid = new THREE.GridHelper(Math.ceil(size / step) * step, Math.ceil(size / step));
    this.grid.visible = visible;
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

  /**
   * Qué hay bajo el puntero. `want` dice qué se puede elegir. Entre caras y
   * regiones gana lo más cercano (una región, a la cara donde está dibujada);
   * los planos base, solo si no hay nada más. Las aristas, primero.
   */
  pick(
    clientX: number,
    clientY: number,
    want: { faces?: boolean; edges?: boolean; vertices?: boolean; scan?: boolean; regions?: boolean; planes?: boolean; refPlanes?: boolean },
  ): CadPick | null {
    this.setRay(clientX, clientY);
    const dist = this.camera.position.distanceTo(this.controls.target);
    if (want.vertices && this.bodyVertices.length) {
      const rect = this.renderer.domElement.getBoundingClientRect();
      const front = this.body ? this.kept(this.raycaster.intersectObject(this.body))?.distance : undefined;
      let best: { d: number; v: THREE.Vector3 } | undefined;
      for (const v of this.bodyVertices) {
        const s = v.clone().project(this.camera);
        if (s.z > 1) continue;
        const px = Math.hypot(rect.left + ((s.x + 1) / 2) * rect.width - clientX, rect.top + ((1 - s.y) / 2) * rect.height - clientY);
        if (px > 8 || (best && px >= best.d)) continue;
        // Tapado: hay una cara bastante más cerca que el vértice en ese rayo
        if (front !== undefined && this.camera.position.distanceTo(v) > front + dist * 0.01) continue;
        best = { d: px, v };
      }
      if (best && (!this.section || this.section.distanceToPoint(best.v) >= 0)) return { kind: "vertex", at: this.fromView(best.v), point: best.v };
    }
    if (want.edges && this.bodyEdges) {
      this.raycaster.params.Line = { threshold: dist * 0.006 };
      const hit = this.kept(this.raycaster.intersectObject(this.bodyEdges));
      if (hit && hit.index != null) {
        const segEdge: number[] = this.bodyEdges.geometry.userData.segEdge;
        return { kind: "edge", edge: segEdge[hit.index / 2], point: hit.point };
      }
    }
    const candidates: { d: number; rank: number; pick: CadPick }[] = [];
    const eps = dist * 1e-4;
    if (want.regions) {
      const hit = this.raycaster.intersectObjects(this.sketchesGroup.children.filter((o) => (o as THREE.Mesh).isMesh))[0];
      if (hit) {
        const u = hit.object.userData as { sketch: number; region: number };
        candidates.push({ d: hit.distance - eps, rank: 0, pick: { kind: "region", sketch: u.sketch, region: u.region, point: hit.point } });
      }
    }
    if (want.faces && this.body) {
      const hit = this.kept(this.raycaster.intersectObject(this.body));
      if (hit && hit.faceIndex != null && this.bodyData) {
        candidates.push({ d: hit.distance, rank: 1, pick: { kind: "face", face: this.visibleTriFace[hit.faceIndex], point: hit.point } });
      }
    }
    if (want.refPlanes) {
      const hit = this.raycaster.intersectObjects(this.refsGroup.children.filter((o) => (o as THREE.Mesh).isMesh && o.userData.refPlane !== undefined))[0];
      if (hit) candidates.push({ d: hit.distance, rank: 2, pick: { kind: "refplane", feature: hit.object.userData.refPlane, point: hit.point } });
    }
    if (want.planes && this.planesGroup.visible) {
      const hit = this.raycaster.intersectObjects(this.planesGroup.children.filter((o) => (o as THREE.Mesh).isMesh))[0];
      if (hit) candidates.push({ d: hit.distance, rank: 2, pick: { kind: "plane", plane: hit.object.userData.plane, point: hit.point } });
    }
    if (candidates.length) {
      // Los planos base son translúcidos y pasan por delante del modelo: solo
      // se eligen si bajo el puntero no hay caras ni regiones
      const solid = candidates.filter((c) => c.pick.kind !== "plane" && c.pick.kind !== "refplane");
      const pool = solid.length ? solid : candidates;
      pool.sort((a, b) => a.d - b.d || a.rank - b.rank);
      return pool[0].pick;
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
    this.setTool(null);
    this.setHandle(null);
    this.setComparison(null, null);
    this.setScan(null);
    this.setSketch(null);
    this.viewCube.dispose();
    this.renderer.dispose();
    this.renderer.domElement.remove();
  }
}

// ─── Geometría en pantalla (selección por caja) ─────────────────────────────

type Box2 = { l: number; r: number; t: number; b: number };

/** ¿El segmento pq toca el rectángulo? (Liang–Barsky) */
function segmentTouchesBox(p: [number, number], q: [number, number], box: Box2): boolean {
  let t0 = 0;
  let t1 = 1;
  const dx = q[0] - p[0];
  const dy = q[1] - p[1];
  for (const [den, num] of [
    [-dx, p[0] - box.l],
    [dx, box.r - p[0]],
    [-dy, p[1] - box.t],
    [dy, box.b - p[1]],
  ]) {
    if (den === 0) {
      if (num < 0) return false;
    } else {
      const t = num / den;
      if (den < 0) t0 = Math.max(t0, t);
      else t1 = Math.min(t1, t);
      if (t0 > t1) return false;
    }
  }
  return true;
}

/** ¿El triángulo toca el rectángulo? Algún lado lo cruza o el rectángulo queda adentro */
function triangleTouchesBox(tri: [number, number][], box: Box2): boolean {
  for (let i = 0; i < 3; i++) if (segmentTouchesBox(tri[i], tri[(i + 1) % 3], box)) return true;
  // Rectángulo entero dentro del triángulo: basta con una esquina
  const [a, b, c] = tri;
  const p = [box.l, box.t];
  const s = (u: number[], v: number[], w: number[]) => (v[0] - u[0]) * (w[1] - u[1]) - (v[1] - u[1]) * (w[0] - u[0]);
  const d1 = s(a, b, p);
  const d2 = s(b, c, p);
  const d3 = s(c, a, p);
  return (d1 >= 0 && d2 >= 0 && d3 >= 0) || (d1 <= 0 && d2 <= 0 && d3 <= 0);
}
