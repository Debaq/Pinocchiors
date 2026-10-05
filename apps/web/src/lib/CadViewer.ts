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

export type BasePlane = "xy" | "xz" | "yz";

export type CadPick =
  | { kind: "face"; face: number; point: THREE.Vector3 }
  | { kind: "edge"; edge: number; point: THREE.Vector3 }
  | { kind: "scan"; triangle: number; point: THREE.Vector3 }
  /** Región cerrada de un sketch visible (índice en `regions`) */
  | { kind: "region"; sketch: number; region: number; point: THREE.Vector3 }
  /** Uno de los planos base */
  | { kind: "plane"; plane: BasePlane; point: THREE.Vector3 };

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
  /** Vista previa de lo que se está dibujando (coordenadas del sketch) */
  preview?: P2[][];
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
function entityPolyline(g: Sketch["entities"][number]["geometry"], point: Map<number, P2>): P2[] | undefined {
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
  const pts = g.points.map((p) => point.get(p));
  if (pts.some((p) => !p)) return undefined;
  if (g.closed && pts.length) pts.push(pts[0]);
  return pts as P2[];
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
  private sketchesGroup = new THREE.Group();
  private planesGroup = new THREE.Group();
  private selectedPlanes = new Set<BasePlane>();
  /** Lado de los planos base (mm): acompaña el tamaño del modelo */
  planeSize = 60;
  private grid: THREE.GridHelper;
  private highlightedFaces = new Set<number>();
  private highlightedEdges = new Set<number>();

  /** mm por unidad de la escena */
  mmPerUnit = 1;
  /** Se llama después de dibujar (la cámara pudo moverse): etiquetas HTML encima */
  onRender?: () => void;
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
    this.scene.add(this.sketchesGroup);
    this.scene.add(this.planesGroup);
    this.buildPlanes();
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
    this.fitPlanes(data?.positions);
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
    for (const e of sketch.entities) {
      const g = e.geometry;
      const color = hover.has(e.id) ? themeHex("cyan") : selected.has(e.id) ? sel : e.construction ? construction : normal;
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

  setPlanes(visible: boolean, selected: BasePlane[]) {
    this.selectedPlanes = new Set(selected);
    this.planesGroup.visible = visible;
    this.buildPlanes();
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

  /**
   * Qué hay bajo el puntero. `want` dice qué se puede elegir. Entre caras y
   * regiones gana lo más cercano (una región, a la cara donde está dibujada);
   * los planos base, solo si no hay nada más. Las aristas, primero.
   */
  pick(
    clientX: number,
    clientY: number,
    want: { faces?: boolean; edges?: boolean; scan?: boolean; regions?: boolean; planes?: boolean },
  ): CadPick | null {
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
      const hit = this.raycaster.intersectObject(this.body)[0];
      if (hit && hit.faceIndex != null && this.bodyData) {
        candidates.push({ d: hit.distance, rank: 1, pick: { kind: "face", face: this.bodyData.triangleFace[hit.faceIndex], point: hit.point } });
      }
    }
    if (want.planes && this.planesGroup.visible) {
      const hit = this.raycaster.intersectObjects(this.planesGroup.children.filter((o) => (o as THREE.Mesh).isMesh))[0];
      if (hit) candidates.push({ d: hit.distance, rank: 2, pick: { kind: "plane", plane: hit.object.userData.plane, point: hit.point } });
    }
    if (candidates.length) {
      // Los planos base son translúcidos y pasan por delante del modelo: solo
      // se eligen si bajo el puntero no hay caras ni regiones
      const solid = candidates.filter((c) => c.pick.kind !== "plane");
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
    this.setScan(null);
    this.setSketch(null);
    this.renderer.dispose();
    this.renderer.domElement.remove();
  }
}
