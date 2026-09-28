/**
 * Pinocchio 3D Viewer - Three.js visualization
 * Rewritten for SolidJS integration
 */

import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { TransformControls } from "three/addons/controls/TransformControls.js";
import { boneWeight, paintRow } from "./weightPaint";

// ═══════════════════════════════════════════════════════════════════════════
// TYPES
// ═══════════════════════════════════════════════════════════════════════════

export interface MeshData {
  positions: Float32Array;
  normals: Float32Array;
  indices: Uint32Array;
  uvs?: Float32Array;
  /** Optional quad indices for quad wireframe visualization (4 indices per quad) */
  quadIndices?: Uint32Array;
}

export interface SkeletonData {
  bones: BoneData[];
  edges: [number, number][];
}

export interface BoneData {
  name: string;
  position: [number, number, number];
  parent: number | null;
  isLeaf: boolean;
}

export interface WeightsData {
  numVertices: number;
  numBones: number;
  boneNames: string[];
  /** Por vértice, `maxInfluences` pares (hueso, peso) */
  weights: Float32Array;
  maxInfluences: number;
}

export interface ViewerSettings {
  showMesh: boolean;
  showWireframe: boolean;
  showSkeleton: boolean;
  showWeights: boolean;
  selectedBone: number;
}

/** Opciones del pincel de pesos */
export interface PaintSettings {
  /** Hueso que se pinta */
  bone: number;
  mode: "add" | "subtract" | "smooth";
  /** Radio en fracción de la diagonal de la malla */
  radius: number;
  /** Intensidad por pincelada, 0–1 */
  strength: number;
  /** Pintar también el vértice espejo con el hueso par */
  mirror: boolean;
}

/** Un trazo del pincel: filas de influencias (pares hueso, peso) antes y después */
export interface PaintStroke {
  vertices: Uint32Array;
  before: Float32Array;
  after: Float32Array;
}

export interface ViewerCallbacks {
  onWeightsPainted?: (stroke: PaintStroke) => void;
  onFpsUpdate?: (fps: number) => void;
  onGroundSelected?: () => void;
  onBoneSelected?: (index: number) => void;
  onBoneMoved?: (index: number, position: [number, number, number]) => void;
}

// ═══════════════════════════════════════════════════════════════════════════
// HEATMAP COLORS
// ═══════════════════════════════════════════════════════════════════════════

const HEATMAP_COLORS = [
  new THREE.Color(0x6272a4), // 0.0 - Dracula comment (blue-gray)
  new THREE.Color(0x8be9fd), // 0.25 - cyan
  new THREE.Color(0x50fa7b), // 0.5 - green
  new THREE.Color(0xf1fa8c), // 0.75 - yellow
  new THREE.Color(0xff5555), // 1.0 - red
];

function getHeatmapColor(value: number): THREE.Color {
  const clamped = Math.max(0, Math.min(1, value));
  const segment = clamped * (HEATMAP_COLORS.length - 1);
  const index = Math.floor(segment);
  const t = segment - index;

  if (index >= HEATMAP_COLORS.length - 1) {
    return HEATMAP_COLORS[HEATMAP_COLORS.length - 1].clone();
  }

  return HEATMAP_COLORS[index].clone().lerp(HEATMAP_COLORS[index + 1], t);
}

// ═══════════════════════════════════════════════════════════════════════════
// VIEWER CLASS
// ═══════════════════════════════════════════════════════════════════════════

/** Texturas ya decodificadas para la vista previa de la piel */
export interface MeshTextures {
  base?: ImageBitmap;
  normal?: ImageBitmap;
}

export class Viewer3D {
  // Core Three.js objects
  private canvas: HTMLCanvasElement;
  private scene: THREE.Scene;
  private camera: THREE.PerspectiveCamera;
  private renderer: THREE.WebGLRenderer;
  private controls: OrbitControls;

  // Groups
  private meshGroup: THREE.Group;
  private skeletonGroup: THREE.Group;

  // Current objects
  private currentMesh: THREE.Mesh | null = null;
  private currentWireframe: THREE.Mesh | null = null;
  private quadWireframe: THREE.LineSegments | null = null;
  private weightsMesh: THREE.Mesh | null = null;

  // Texturas de la piel (color base y normal), si la malla tiene UV
  private textures: MeshTextures = {};

  // Data
  private meshData: MeshData | null = null;
  private weightsData: WeightsData | null = null;
  private skeletonData: SkeletonData | null = null;

  // Pincel de pesos
  private paintSettings: PaintSettings | null = null;
  private paintMirror: { vertex: Uint32Array; bone: Uint32Array } | null = null;
  private paintNeighbors: { offsets: Uint32Array; list: Uint32Array } | null = null;
  /** Filas originales de los vértices tocados en el trazo en curso */
  private stroke: Map<number, Float32Array> | null = null;
  private lastDab: THREE.Vector3 | null = null;
  private brushCursor: THREE.Mesh | null = null;

  /** Pose de prueba en curso: posiciones y normales de reposo para restaurar */
  private pose: {
    joint: number;
    positions: Float32Array;
    normals: Float32Array;
    /** Huesos que se mueven (descendientes de la articulación) */
    moving: boolean[];
  } | null = null;

  // Settings
  private settings: ViewerSettings = {
    showMesh: true,
    showWireframe: false,
    showSkeleton: true,
    showWeights: false,
    selectedBone: -1,
  };

  // FPS tracking
  private frameCount = 0;
  private lastFpsUpdate = 0;
  private animationId: number | null = null;

  // Callbacks
  private callbacks: ViewerCallbacks = {};

  // Raycaster for ground selection
  private raycaster: THREE.Raycaster;
  private mouse: THREE.Vector2;
  private groundSelectionMode = false;
  private highlightedFace: THREE.Mesh | null = null;

  // Bone editing
  private boneSpheres: THREE.Mesh[] = [];
  private boneEditMode = false;
  private transformControls: TransformControls | null = null;
  private selectedBoneIndex = -1;
  private gridHelper: THREE.GridHelper | null = null;

  constructor(canvas: HTMLCanvasElement) {
    this.canvas = canvas;

    // Scene with Dracula background
    this.scene = new THREE.Scene();
    this.scene.background = new THREE.Color(0x282a36);

    // Camera
    const aspect = canvas.clientWidth / canvas.clientHeight;
    this.camera = new THREE.PerspectiveCamera(45, aspect, 0.01, 1000);
    this.camera.position.set(2, 1.5, 2);

    // Renderer
    this.renderer = new THREE.WebGLRenderer({
      canvas: this.canvas,
      antialias: true,
    });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.setSize(canvas.clientWidth, canvas.clientHeight);
    this.renderer.toneMapping = THREE.ACESFilmicToneMapping;
    this.renderer.toneMappingExposure = 1.0;

    // Controls
    this.controls = new OrbitControls(this.camera, this.canvas);
    this.controls.enableDamping = true;
    this.controls.dampingFactor = 0.05;
    this.controls.target.set(0, 0.5, 0);
    this.controls.update();

    // Groups
    this.meshGroup = new THREE.Group();
    this.skeletonGroup = new THREE.Group();
    this.scene.add(this.meshGroup);
    this.scene.add(this.skeletonGroup);

    // Grid - Dracula style
    this.gridHelper = new THREE.GridHelper(10, 20, 0x44475a, 0x383a4a);
    this.scene.add(this.gridHelper);

    // Lights
    this.setupLights();

    // Raycaster
    this.raycaster = new THREE.Raycaster();
    this.mouse = new THREE.Vector2();

    // Event listeners
    this.setupEventListeners();

    // Start render loop
    this.animate();
  }

  private setupLights(): void {
    // Ambient light
    const ambientLight = new THREE.AmbientLight(0xffffff, 0.4);
    this.scene.add(ambientLight);

    // Main directional light
    const mainLight = new THREE.DirectionalLight(0xffffff, 0.8);
    mainLight.position.set(5, 10, 5);
    mainLight.castShadow = false;
    this.scene.add(mainLight);

    // Fill light
    const fillLight = new THREE.DirectionalLight(0x8be9fd, 0.3);
    fillLight.position.set(-5, 5, -5);
    this.scene.add(fillLight);

    // Rim light (purple tint for Dracula feel)
    const rimLight = new THREE.DirectionalLight(0xbd93f9, 0.2);
    rimLight.position.set(0, -5, -5);
    this.scene.add(rimLight);
  }

  private setupEventListeners(): void {
    // Resize
    const resizeObserver = new ResizeObserver(() => this.onResize());
    resizeObserver.observe(this.canvas.parentElement!);

    // Mouse events for ground selection
    this.canvas.addEventListener("click", (e) => this.onCanvasClick(e));
    this.canvas.addEventListener("mousemove", (e) => this.onCanvasMouseMove(e));

    // Pincel de pesos
    this.canvas.addEventListener("pointerdown", (e) => this.onPaintDown(e));
    this.canvas.addEventListener("pointermove", (e) => this.onPaintMove(e));
    window.addEventListener("pointerup", () => this.finishStroke());
  }

  // ═══════════════════════════════════════════════════════════════════════════
  // PINCEL DE PESOS
  // ═══════════════════════════════════════════════════════════════════════════

  /** Activa el pincel con esas opciones (`null` lo apaga) */
  setPaintMode(settings: PaintSettings | null): void {
    this.paintSettings = settings;
    if (!settings) {
      this.finishStroke();
      if (this.brushCursor) this.brushCursor.visible = false;
    }
    this.canvas.style.cursor = settings ? "none" : "default";
  }

  /** Vértice espejo de cada vértice y hueso par de cada hueso (`0xffffffff` = ninguno) */
  setPaintMirror(vertex: Uint32Array, bone: Uint32Array): void {
    this.paintMirror = { vertex, bone };
  }

  /** Reemplaza filas de influencias (deshacer/rehacer un trazo) */
  applyWeightRows(vertices: Uint32Array, rows: Float32Array): void {
    const weights = this.weightsData;
    if (!weights) return;
    const width = weights.maxInfluences * 2;
    vertices.forEach((v, i) => {
      weights.weights.set(rows.subarray(i * width, (i + 1) * width), v * width);
      this.recolorVertex(v);
    });
    this.markColorsDirty();
  }

  private paintableMesh(): THREE.Mesh | null {
    const weights = this.weightsData;
    const mesh = this.meshData;
    if (!this.paintSettings || !weights || !mesh || weights.numVertices * 3 !== mesh.positions.length) return null;
    return this.weightsMesh ?? this.currentMesh;
  }

  private brushHit(event: PointerEvent): THREE.Intersection | null {
    const target = this.paintableMesh();
    if (!target) return null;
    this.raycaster.setFromCamera(this.getMousePosition(event), this.camera);
    return this.raycaster.intersectObject(target)[0] ?? null;
  }

  private brushRadius(): number {
    if (!this.paintSettings || !this.currentMesh) return 0;
    const size = new THREE.Box3().setFromObject(this.currentMesh).getSize(new THREE.Vector3());
    return this.paintSettings.radius * size.length();
  }

  private updateBrushCursor(hit: THREE.Intersection | null): void {
    if (!hit || !hit.face) {
      if (this.brushCursor) this.brushCursor.visible = false;
      return;
    }
    if (!this.brushCursor) {
      const ring = new THREE.RingGeometry(0.92, 1, 48);
      const material = new THREE.MeshBasicMaterial({ color: 0xff79c6, side: THREE.DoubleSide, depthTest: false, transparent: true });
      this.brushCursor = new THREE.Mesh(ring, material);
      this.brushCursor.renderOrder = 2;
      this.scene.add(this.brushCursor);
    }
    const radius = this.brushRadius();
    this.brushCursor.visible = true;
    this.brushCursor.scale.setScalar(radius);
    this.brushCursor.position.copy(hit.point);
    const normal = hit.face.normal.clone().transformDirection(hit.object.matrixWorld);
    this.brushCursor.lookAt(hit.point.clone().add(normal));
  }

  private onPaintDown(event: PointerEvent): void {
    if (event.button !== 0) return;
    const hit = this.brushHit(event);
    if (!hit) return;
    this.controls.enabled = false;
    this.stroke = new Map();
    this.lastDab = null;
    this.dab(this.meshGroup.worldToLocal(hit.point.clone()));
  }

  private onPaintMove(event: PointerEvent): void {
    if (!this.paintSettings) return;
    const hit = this.brushHit(event);
    this.updateBrushCursor(hit);
    if (!this.stroke || !hit) return;
    const point = this.meshGroup.worldToLocal(hit.point.clone());
    // Una pincelada cada cuarto de radio: el trazo no depende de la velocidad
    if (this.lastDab && this.lastDab.distanceTo(point) < 0.25 * this.brushRadius()) return;
    this.dab(point);
  }

  private finishStroke(): void {
    const stroke = this.stroke;
    this.stroke = null;
    this.controls.enabled = true;
    const weights = this.weightsData;
    if (!stroke || stroke.size === 0 || !weights) return;
    const width = weights.maxInfluences * 2;
    const vertices = Uint32Array.from(stroke.keys());
    const before = new Float32Array(vertices.length * width);
    const after = new Float32Array(vertices.length * width);
    vertices.forEach((v, i) => {
      before.set(stroke.get(v)!, i * width);
      after.set(weights.weights.subarray(v * width, (v + 1) * width), i * width);
    });
    this.callbacks.onWeightsPainted?.({ vertices, before, after });
  }

  /** Una pincelada centrada en `center` (coordenadas de la malla) */
  private dab(center: THREE.Vector3): void {
    const settings = this.paintSettings;
    const mesh = this.meshData;
    const weights = this.weightsData;
    if (!settings || !mesh || !weights) return;
    this.lastDab = center.clone();
    const radius = this.brushRadius();
    const r2 = radius * radius;
    const positions = mesh.positions;
    const hits: [number, number][] = [];
    for (let v = 0; v < weights.numVertices; v++) {
      const dx = positions[3 * v] - center.x;
      const dy = positions[3 * v + 1] - center.y;
      const dz = positions[3 * v + 2] - center.z;
      const d2 = dx * dx + dy * dy + dz * dz;
      if (d2 < r2) {
        const t = 1 - d2 / r2;
        hits.push([v, t * t]);
      }
    }
    const mirror = settings.mirror ? this.paintMirror : null;
    // El suavizado lee los pesos de antes de la pincelada
    const smoothTargets = settings.mode === "smooth" ? this.smoothTargets(hits, settings.bone) : null;
    hits.forEach(([v, falloff], i) => {
      this.paintVertex(v, settings.bone, falloff * settings.strength, smoothTargets?.[i]);
      const mv = mirror?.vertex[v];
      if (mirror && mv !== undefined && mv !== 0xffffffff && mv !== v) {
        const pair = mirror.bone[settings.bone];
        const mb = pair === 0xffffffff ? settings.bone : pair;
        const target = settings.mode === "smooth" ? this.smoothTargets([[mv, 1]], mb)[0] : undefined;
        this.paintVertex(mv, mb, falloff * settings.strength, target);
      }
    });
    this.markColorsDirty();
  }

  private boneWeight(v: number, bone: number): number {
    const weights = this.weightsData!;
    return boneWeight(weights.weights, v * weights.maxInfluences * 2, weights.maxInfluences, bone);
  }

  /** Promedio del peso de `bone` en los vecinos de cada vértice */
  private smoothTargets(hits: [number, number][], bone: number): number[] {
    const neighbors = this.neighborsOf();
    return hits.map(([v]) => {
      let sum = 0;
      let count = 0;
      for (let j = neighbors.offsets[v]; j < neighbors.offsets[v + 1]; j++) {
        sum += this.boneWeight(neighbors.list[j], bone);
        count++;
      }
      return count > 0 ? sum / count : this.boneWeight(v, bone);
    });
  }

  /** Vecinos por aristas de la malla (formato CSR), calculados una vez */
  private neighborsOf(): { offsets: Uint32Array; list: Uint32Array } {
    if (this.paintNeighbors) return this.paintNeighbors;
    const mesh = this.meshData!;
    const n = mesh.positions.length / 3;
    const sets: Set<number>[] = Array.from({ length: n }, () => new Set());
    for (let t = 0; t + 2 < mesh.indices.length; t += 3) {
      const [a, b, c] = [mesh.indices[t], mesh.indices[t + 1], mesh.indices[t + 2]];
      sets[a].add(b).add(c);
      sets[b].add(a).add(c);
      sets[c].add(a).add(b);
    }
    // Vértices en la misma posición (costuras UV) comparten vecinos
    const byPosition = new Map<string, number[]>();
    for (let v = 0; v < n; v++) {
      const key = `${mesh.positions[3 * v]},${mesh.positions[3 * v + 1]},${mesh.positions[3 * v + 2]}`;
      const group = byPosition.get(key);
      if (group) group.push(v);
      else byPosition.set(key, [v]);
    }
    for (const group of byPosition.values()) {
      if (group.length < 2) continue;
      const union = new Set<number>();
      group.forEach((v) => sets[v].forEach((w) => union.add(w)));
      group.forEach((v) => (sets[v] = union));
    }
    const offsets = new Uint32Array(n + 1);
    for (let v = 0; v < n; v++) offsets[v + 1] = offsets[v] + sets[v].size;
    const list = new Uint32Array(offsets[n]);
    for (let v = 0; v < n; v++) list.set([...sets[v]], offsets[v]);
    this.paintNeighbors = { offsets, list };
    return this.paintNeighbors;
  }

  /** Aplica el pincel al vértice `v` y guarda su fila original en el trazo */
  private paintVertex(v: number, bone: number, amount: number, smoothTarget?: number): void {
    const weights = this.weightsData!;
    const k = weights.maxInfluences;
    const base = v * k * 2;
    if (this.stroke && !this.stroke.has(v)) {
      this.stroke.set(v, weights.weights.slice(base, base + k * 2));
    }
    if (paintRow(weights.weights, base, k, bone, this.paintSettings!.mode, amount, smoothTarget, this.fallbackBone(bone))) {
      this.recolorVertex(v);
    }
  }

  /**
   * Hueso que recibe el peso que pierde `bone` donde es la única influencia:
   * su padre, o si el padre es la raíz (sin segmento), su primer hijo.
   */
  private fallbackBone(bone: number): number | undefined {
    const bones = this.skeletonData?.bones;
    if (!bones) return undefined;
    const parent = bones[bone]?.parent;
    if (parent !== null && parent !== undefined && bones[parent].parent !== null) return parent;
    const child = bones.findIndex((b) => b.parent === bone);
    return child >= 0 ? child : undefined;
  }

  /** Color del mapa de calor de un vértice según el hueso seleccionado */
  private recolorVertex(v: number): void {
    const colors = this.weightsMesh?.geometry.getAttribute("color") as THREE.BufferAttribute | undefined;
    if (!colors || !this.weightsData) return;
    const bone = this.settings.selectedBone;
    let weight = 0;
    const k = this.weightsData.maxInfluences;
    for (let i = 0; i < k; i++) {
      const w = this.weightsData.weights[(v * k + i) * 2 + 1];
      if (bone === -1) weight = Math.max(weight, w);
      else if (this.weightsData.weights[(v * k + i) * 2] === bone) weight = w;
    }
    const color = getHeatmapColor(weight);
    colors.setXYZ(v, color.r, color.g, color.b);
  }

  private markColorsDirty(): void {
    const colors = this.weightsMesh?.geometry.getAttribute("color");
    if (colors) colors.needsUpdate = true;
  }

  private onResize(): void {
    const parent = this.canvas.parentElement;
    if (!parent) return;

    const width = parent.clientWidth;
    const height = parent.clientHeight;

    this.camera.aspect = width / height;
    this.camera.updateProjectionMatrix();
    this.renderer.setSize(width, height);
  }

  private animate = (): void => {
    this.animationId = requestAnimationFrame(this.animate);

    this.controls.update();
    this.renderer.render(this.scene, this.camera);

    // FPS counter
    this.frameCount++;
    const now = performance.now();
    if (now - this.lastFpsUpdate >= 1000) {
      const fps = Math.round((this.frameCount * 1000) / (now - this.lastFpsUpdate));
      this.callbacks.onFpsUpdate?.(fps);
      this.frameCount = 0;
      this.lastFpsUpdate = now;
    }
  };

  // ═══════════════════════════════════════════════════════════════════════════
  // PUBLIC API
  // ═══════════════════════════════════════════════════════════════════════════

  setCallbacks(callbacks: ViewerCallbacks): void {
    this.callbacks = callbacks;
  }

  loadMesh(data: MeshData): void {
    this.resetPose();
    this.meshData = data;
    this.paintNeighbors = null;
    this.clearMesh();

    // Create geometry
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute("position", new THREE.BufferAttribute(data.positions, 3));
    geometry.setAttribute("normal", new THREE.BufferAttribute(data.normals, 3));
    if (data.uvs) {
      geometry.setAttribute("uv", new THREE.BufferAttribute(data.uvs, 2));
    }
    geometry.setIndex(new THREE.BufferAttribute(data.indices, 1));

    // Material - Dracula style
    const material = new THREE.MeshStandardMaterial({
      color: 0x6272a4,
      metalness: 0.1,
      roughness: 0.7,
      side: THREE.DoubleSide,
    });
    this.applyTextures(material, geometry);

    this.currentMesh = new THREE.Mesh(geometry, material);
    this.currentMesh.visible = this.settings.showMesh && !this.settings.showWeights;
    this.meshGroup.add(this.currentMesh);

    // Wireframe - use quad wireframe if quad indices are available
    if (data.quadIndices && data.quadIndices.length > 0) {
      // Create quad wireframe (draws the 4 edges of each quad)
      const quadWireframeGeometry = this.createQuadWireframeGeometry(data.positions, data.quadIndices);
      const quadWireframeMaterial = new THREE.LineBasicMaterial({
        color: 0x50fa7b, // Dracula green - high contrast against model
        depthTest: false,
      });
      this.quadWireframe = new THREE.LineSegments(quadWireframeGeometry, quadWireframeMaterial);
      this.quadWireframe.renderOrder = 1;
      this.quadWireframe.visible = this.settings.showWireframe;
      this.meshGroup.add(this.quadWireframe);
    } else {
      // Fallback to triangle wireframe. Comparte la geometría y dibuja en modo
      // wireframe: WireframeGeometry deduplica aristas con claves de texto en
      // JS y congela la ventana varios segundos con mallas de millones de caras.
      const wireframeMaterial = new THREE.MeshBasicMaterial({
        color: 0x44475a,
        opacity: 0.5,
        transparent: true,
        wireframe: true,
      });
      this.currentWireframe = new THREE.Mesh(geometry, wireframeMaterial);
      this.currentWireframe.visible = this.settings.showWireframe;
      this.meshGroup.add(this.currentWireframe);
    }

    // Fit camera to mesh
    this.fitCamera();
  }

  /** Texturas de la piel; se aplican solo si la malla actual tiene UV */
  setTextures(textures: MeshTextures): void {
    this.textures = textures;
    if (this.currentMesh) {
      this.applyTextures(this.currentMesh.material as THREE.MeshStandardMaterial, this.currentMesh.geometry);
    }
  }

  private applyTextures(material: THREE.MeshStandardMaterial, geometry: THREE.BufferGeometry): void {
    material.map?.dispose();
    material.normalMap?.dispose();
    const hasUv = geometry.getAttribute("uv") !== undefined;
    // Convención glTF: UV (0, 0) = esquina superior izquierda, sin voltear
    const texture = (image: ImageBitmap | undefined, srgb: boolean) => {
      if (!hasUv || !image) return null;
      const t = new THREE.Texture(image);
      t.flipY = false;
      t.wrapS = t.wrapT = THREE.RepeatWrapping;
      t.anisotropy = this.renderer.capabilities.getMaxAnisotropy();
      if (srgb) t.colorSpace = THREE.SRGBColorSpace;
      t.needsUpdate = true;
      return t;
    };
    material.map = texture(this.textures.base, true);
    material.normalMap = texture(this.textures.normal, false);
    // Sin atributo de tangentes Three usa derivadas de pantalla, con el eje Y
    // opuesto al de glTF (igual que GLTFLoader)
    material.normalScale.set(1, -1);
    material.color.set(material.map ? 0xffffff : 0x6272a4);
    material.needsUpdate = true;
  }

  loadSkeleton(data: SkeletonData): void {
    this.resetPose();
    const sameStructure =
      this.skeletonData !== null &&
      this.boneSpheres.length === data.bones.length &&
      data.bones.every((b, i) => b.parent === this.skeletonData!.bones[i].parent);
    this.skeletonData = data;
    // Misma estructura (mover un hueso, centrar, espejo): actualizar en el
    // lugar, así el gizmo sigue enganchado a su esfera durante el arrastre
    if (sameStructure) {
      data.bones.forEach((bone, i) => this.boneSpheres[i].position.set(...bone.position));
      this.updateBoneLines();
      return;
    }
    this.clearSkeleton();
    this.boneSpheres = [];

    const bonesGroup = new THREE.Group();
    bonesGroup.name = "bones";
    this.skeletonGroup.add(bonesGroup);

    // Joint spheres
    const jointGeometry = new THREE.SphereGeometry(this.jointRadius(), 16, 16);
    const jointMaterial = new THREE.MeshBasicMaterial({ color: 0xffb86c }); // Dracula orange
    const leafMaterial = new THREE.MeshBasicMaterial({ color: 0x50fa7b }); // Dracula green

    for (let i = 0; i < data.bones.length; i++) {
      const bone = data.bones[i];
      const material = bone.isLeaf ? leafMaterial.clone() : jointMaterial.clone();
      const sphere = new THREE.Mesh(jointGeometry, material);
      sphere.position.set(bone.position[0], bone.position[1], bone.position[2]);
      sphere.userData.boneName = bone.name;
      sphere.userData.boneIndex = i;
      sphere.userData.isLeaf = bone.isLeaf;
      bonesGroup.add(sphere);
      this.boneSpheres.push(sphere);
    }

    // Bone lines
    const linePoints: number[] = [];
    for (const [parentIdx, childIdx] of data.edges) {
      const parent = data.bones[parentIdx];
      const child = data.bones[childIdx];
      linePoints.push(
        parent.position[0], parent.position[1], parent.position[2],
        child.position[0], child.position[1], child.position[2]
      );
    }

    if (linePoints.length > 0) {
      const lineGeometry = new THREE.BufferGeometry();
      lineGeometry.setAttribute("position", new THREE.Float32BufferAttribute(linePoints, 3));
      const lineMaterial = new THREE.LineBasicMaterial({
        color: 0xffb86c,
        linewidth: 2,
      });
      const lines = new THREE.LineSegments(lineGeometry, lineMaterial);
      lines.name = "boneLines";
      this.skeletonGroup.add(lines);
    }

    this.skeletonGroup.visible = this.settings.showSkeleton;
    // Las esferas son nuevas: volver a marcar (y enganchar) la seleccionada
    if (this.selectedBoneIndex >= 0 && this.selectedBoneIndex < this.boneSpheres.length) {
      this.selectBone(this.selectedBoneIndex);
    } else {
      this.transformControls?.detach();
    }
  }

  /** Radio de las esferas de las articulaciones, proporcional a la malla */
  private jointRadius(): number {
    if (!this.currentMesh) return 0.02;
    const size = new THREE.Box3().setFromObject(this.currentMesh).getSize(new THREE.Vector3());
    return Math.max(size.length() * 0.006, 1e-4);
  }

  /** Redibuja las líneas de los huesos con las posiciones de las esferas */
  private updateBoneLines(): void {
    const lines = this.skeletonGroup.getObjectByName("boneLines") as THREE.LineSegments | undefined;
    if (!lines || !this.skeletonData) return;
    const attr = lines.geometry.getAttribute("position") as THREE.BufferAttribute;
    this.skeletonData.edges.forEach(([parent, child], k) => {
      const a = this.boneSpheres[parent].position;
      const b = this.boneSpheres[child].position;
      attr.setXYZ(2 * k, a.x, a.y, a.z);
      attr.setXYZ(2 * k + 1, b.x, b.y, b.z);
    });
    attr.needsUpdate = true;
    lines.geometry.computeBoundingSphere();
  }

  /**
   * Pose de prueba: gira la articulación `joint` y deforma la malla con los
   * pesos (skinning lineal), para ver si los pesos doblan bien. El peso de un
   * hueso es el de su segmento padre → hueso, así que se mueven los huesos
   * descendientes de la articulación.
   */
  private applyPose(joint: number, rotation: THREE.Quaternion): void {
    const mesh = this.meshData;
    const weights = this.weightsData;
    const skeleton = this.skeletonData;
    if (!mesh || !weights || !skeleton || weights.numVertices * 3 !== mesh.positions.length) return;

    if (!this.pose || this.pose.joint !== joint) {
      this.resetPose();
      const moving = skeleton.bones.map((_, b) => {
        for (let p = skeleton.bones[b].parent; p !== null; p = skeleton.bones[p].parent) {
          if (p === joint) return true;
        }
        return false;
      });
      this.pose = { joint, positions: mesh.positions.slice(), normals: mesh.normals.slice(), moving };
    }
    const { positions: rest, normals: restNormals, moving } = this.pose;
    const center = this.boneSpheres[joint].position.clone();
    const p = new THREE.Vector3();
    const n = new THREE.Vector3();
    const k = weights.maxInfluences;
    for (let v = 0; v < weights.numVertices; v++) {
      let w = 0;
      for (let i = 0; i < k; i++) {
        const bone = weights.weights[(v * k + i) * 2];
        if (moving[bone]) w += weights.weights[(v * k + i) * 2 + 1];
      }
      const o = 3 * v;
      if (w <= 0) {
        mesh.positions[o] = rest[o];
        mesh.positions[o + 1] = rest[o + 1];
        mesh.positions[o + 2] = rest[o + 2];
        mesh.normals[o] = restNormals[o];
        mesh.normals[o + 1] = restNormals[o + 1];
        mesh.normals[o + 2] = restNormals[o + 2];
        continue;
      }
      p.set(rest[o], rest[o + 1], rest[o + 2]).sub(center).applyQuaternion(rotation).add(center);
      mesh.positions[o] = rest[o] + w * (p.x - rest[o]);
      mesh.positions[o + 1] = rest[o + 1] + w * (p.y - rest[o + 1]);
      mesh.positions[o + 2] = rest[o + 2] + w * (p.z - rest[o + 2]);
      n.set(restNormals[o], restNormals[o + 1], restNormals[o + 2]).applyQuaternion(rotation);
      n.set(
        restNormals[o] + w * (n.x - restNormals[o]),
        restNormals[o + 1] + w * (n.y - restNormals[o + 1]),
        restNormals[o + 2] + w * (n.z - restNormals[o + 2])
      ).normalize();
      mesh.normals[o] = n.x;
      mesh.normals[o + 1] = n.y;
      mesh.normals[o + 2] = n.z;
    }
    this.markMeshDirty();

    // Articulaciones descendientes, giradas
    skeleton.bones.forEach((bone, b) => {
      if (!moving[b]) return;
      p.set(...bone.position).sub(center).applyQuaternion(rotation).add(center);
      this.boneSpheres[b].position.copy(p);
    });
    this.updateBoneLines();
  }

  /** Vuelve a la pose de reposo (malla y esqueleto) */
  resetPose(): void {
    const pose = this.pose;
    this.pose = null;
    if (!pose) return;
    if (this.meshData && this.meshData.positions.length === pose.positions.length) {
      this.meshData.positions.set(pose.positions);
      this.meshData.normals.set(pose.normals);
      this.markMeshDirty();
    }
    if (this.skeletonData) {
      this.skeletonData.bones.forEach((bone, b) => this.boneSpheres[b]?.position.set(...bone.position));
      this.boneSpheres[pose.joint]?.quaternion.identity();
      this.updateBoneLines();
    }
  }

  /** Hay una pose de prueba aplicada */
  isPosed(): boolean {
    return this.pose !== null;
  }

  private markMeshDirty(): void {
    for (const mesh of [this.currentMesh, this.weightsMesh]) {
      if (!mesh) continue;
      mesh.geometry.getAttribute("position").needsUpdate = true;
      mesh.geometry.getAttribute("normal").needsUpdate = true;
      mesh.geometry.computeBoundingSphere();
    }
  }

  loadWeights(data: WeightsData): void {
    this.resetPose();
    this.weightsData = data;
    this.updateWeightsVisualization();
  }

  updateSettings(settings: Partial<ViewerSettings>): void {
    this.settings = { ...this.settings, ...settings };
    this.applySettings();
  }

  setGroundSelectionMode(enabled: boolean): void {
    this.groundSelectionMode = enabled;
    this.canvas.style.cursor = enabled ? "crosshair" : "default";

    if (!enabled && this.highlightedFace) {
      this.meshGroup.remove(this.highlightedFace);
      this.highlightedFace.geometry.dispose();
      (this.highlightedFace.material as THREE.Material).dispose();
      this.highlightedFace = null;
    }
  }

  isGroundSelectionMode(): boolean {
    return this.groundSelectionMode;
  }

  resetView(): void {
    this.meshGroup.quaternion.identity();
    this.meshGroup.position.set(0, 0, 0);
    this.skeletonGroup.quaternion.identity();
    this.skeletonGroup.position.set(0, 0, 0);
    this.fitCamera();
  }

  setBoneEditMode(enabled: boolean): void {
    this.boneEditMode = enabled;

    if (enabled) {
      if (!this.transformControls) {
        this.transformControls = new TransformControls(this.camera, this.canvas);
        this.transformControls.setMode("translate");
        this.transformControls.setSize(0.5);
        this.scene.add(this.transformControls.getHelper());

        this.transformControls.addEventListener("dragging-changed", (event) => {
          this.controls.enabled = !event.value;
        });

        this.transformControls.addEventListener("objectChange", () => {
          const object = this.transformControls?.object;
          if (this.selectedBoneIndex < 0 || !object) return;
          if (this.transformControls?.getMode() === "rotate") {
            // Rotar una articulación con pesos calculados: pose de prueba
            this.applyPose(this.selectedBoneIndex, object.quaternion);
          } else {
            const pos = object.position;
            this.callbacks.onBoneMoved?.(this.selectedBoneIndex, [pos.x, pos.y, pos.z]);
          }
        });
      }
    } else {
      if (this.transformControls) {
        this.transformControls.detach();
      }
      this.selectedBoneIndex = -1;
    }
  }

  selectBone(index: number): void {
    this.selectedBoneIndex = index;

    // Reset all sphere colors
    for (let i = 0; i < this.boneSpheres.length; i++) {
      const sphere = this.boneSpheres[i];
      const mat = sphere.material as THREE.MeshBasicMaterial;
      mat.color.setHex(sphere.userData.boneIndex !== undefined && this.boneSpheres[i].userData.isLeaf ? 0x50fa7b : 0xffb86c);
    }

    // Highlight selected
    if (index >= 0 && index < this.boneSpheres.length) {
      const sphere = this.boneSpheres[index];
      (sphere.material as THREE.MeshBasicMaterial).color.setHex(0xbd93f9); // Dracula purple

      if (this.boneEditMode && this.transformControls) {
        this.transformControls.attach(sphere);
      }
    } else if (this.transformControls) {
      this.transformControls.detach();
    }
  }

  setGridVisible(visible: boolean): void {
    if (this.gridHelper) {
      this.gridHelper.visible = visible;
    }
  }

  setActiveTool(tool: string): void {
    if (tool !== "rotate") this.resetPose();
    if (!this.transformControls) return;

    switch (tool) {
      case "move":
        this.transformControls.setMode("translate");
        this.canvas.style.cursor = "move";
        break;
      case "rotate":
        this.transformControls.setMode("rotate");
        this.canvas.style.cursor = "crosshair";
        break;
      case "scale":
        this.transformControls.setMode("scale");
        this.canvas.style.cursor = "ns-resize";
        break;
      default:
        this.canvas.style.cursor = "default";
        if (this.transformControls) {
          this.transformControls.detach();
        }
        break;
    }
  }

  setView(name: string): void {
    const target = this.controls.target.clone();
    const dist = this.camera.position.distanceTo(target);

    switch (name) {
      case "front":
        this.camera.position.set(target.x, target.y, target.z + dist);
        break;
      case "right":
        this.camera.position.set(target.x + dist, target.y, target.z);
        break;
      case "top":
        this.camera.position.set(target.x, target.y + dist, target.z);
        break;
    }

    this.camera.lookAt(target);
    this.controls.update();
  }

  focusSelection(): void {
    if (this.selectedBoneIndex >= 0 && this.selectedBoneIndex < this.boneSpheres.length) {
      const sphere = this.boneSpheres[this.selectedBoneIndex];
      this.controls.target.copy(sphere.position);
      this.controls.update();
    } else {
      this.fitCamera();
    }
  }

  dispose(): void {
    if (this.animationId !== null) {
      cancelAnimationFrame(this.animationId);
    }
    if (this.transformControls) {
      this.scene.remove(this.transformControls.getHelper());
      this.transformControls.dispose();
    }
    this.clearMesh();
    this.clearSkeleton();
    this.renderer.dispose();
  }

  // ═══════════════════════════════════════════════════════════════════════════
  // PRIVATE METHODS
  // ═══════════════════════════════════════════════════════════════════════════

  private applySettings(): void {
    // Mesh visibility
    if (this.currentMesh) {
      this.currentMesh.visible = this.settings.showMesh && !this.settings.showWeights;
    }

    // Wireframe (triangle or quad)
    if (this.currentWireframe) {
      this.currentWireframe.visible = this.settings.showWireframe;
    }
    if (this.quadWireframe) {
      this.quadWireframe.visible = this.settings.showWireframe;
    }

    // Skeleton
    this.skeletonGroup.visible = this.settings.showSkeleton;

    // Weights
    if (this.settings.showWeights && this.weightsData) {
      this.updateWeightsVisualization();
      if (this.weightsMesh) {
        this.weightsMesh.visible = true;
      }
      if (this.currentMesh) {
        this.currentMesh.visible = false;
      }
    } else {
      if (this.weightsMesh) {
        this.weightsMesh.visible = false;
      }
      if (this.currentMesh) {
        this.currentMesh.visible = this.settings.showMesh;
      }
    }
  }

  private updateWeightsVisualization(): void {
    if (!this.meshData || !this.weightsData) return;

    // Remove existing weights mesh
    if (this.weightsMesh) {
      this.meshGroup.remove(this.weightsMesh);
      this.weightsMesh.geometry.dispose();
      (this.weightsMesh.material as THREE.Material).dispose();
      this.weightsMesh = null;
    }

    // Create geometry
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute("position", new THREE.BufferAttribute(this.meshData.positions, 3));
    geometry.setAttribute("normal", new THREE.BufferAttribute(this.meshData.normals, 3));
    geometry.setIndex(new THREE.BufferAttribute(this.meshData.indices, 1));

    // Compute vertex colors
    const colors = new Float32Array(this.weightsData.numVertices * 3);
    const maxInfluences = this.weightsData.maxInfluences;

    for (let v = 0; v < this.weightsData.numVertices; v++) {
      let weight = 0;
      const baseIdx = v * maxInfluences * 2;

      if (this.settings.selectedBone === -1) {
        // Max weight for any bone
        for (let i = 0; i < maxInfluences; i++) {
          const w = this.weightsData.weights[baseIdx + i * 2 + 1];
          weight = Math.max(weight, w);
        }
      } else {
        // Weight for specific bone
        for (let i = 0; i < maxInfluences; i++) {
          const boneIdx = this.weightsData.weights[baseIdx + i * 2];
          const w = this.weightsData.weights[baseIdx + i * 2 + 1];
          if (boneIdx === this.settings.selectedBone) {
            weight = w;
            break;
          }
        }
      }

      const color = getHeatmapColor(weight);
      colors[v * 3] = color.r;
      colors[v * 3 + 1] = color.g;
      colors[v * 3 + 2] = color.b;
    }

    geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));

    const material = new THREE.MeshBasicMaterial({
      vertexColors: true,
      side: THREE.DoubleSide,
    });

    this.weightsMesh = new THREE.Mesh(geometry, material);
    this.weightsMesh.visible = this.settings.showWeights;
    this.meshGroup.add(this.weightsMesh);
  }

  private clearMesh(): void {
    if (this.currentMesh) {
      this.meshGroup.remove(this.currentMesh);
      this.currentMesh.geometry.dispose();
      const material = this.currentMesh.material as THREE.MeshStandardMaterial;
      material.map?.dispose();
      material.normalMap?.dispose();
      material.dispose();
      this.currentMesh = null;
    }

    if (this.currentWireframe) {
      // La geometría es la de currentMesh: se libera con ella
      this.meshGroup.remove(this.currentWireframe);
      (this.currentWireframe.material as THREE.Material).dispose();
      this.currentWireframe = null;
    }

    if (this.quadWireframe) {
      this.meshGroup.remove(this.quadWireframe);
      this.quadWireframe.geometry.dispose();
      (this.quadWireframe.material as THREE.Material).dispose();
      this.quadWireframe = null;
    }

    if (this.weightsMesh) {
      this.meshGroup.remove(this.weightsMesh);
      this.weightsMesh.geometry.dispose();
      (this.weightsMesh.material as THREE.Material).dispose();
      this.weightsMesh = null;
    }
  }

  private clearSkeleton(): void {
    while (this.skeletonGroup.children.length > 0) {
      const child = this.skeletonGroup.children[0];
      this.skeletonGroup.remove(child);
      if (child instanceof THREE.Mesh || child instanceof THREE.LineSegments) {
        child.geometry.dispose();
        if (Array.isArray(child.material)) {
          child.material.forEach((m) => m.dispose());
        } else {
          child.material.dispose();
        }
      }
    }
  }

  private fitCamera(): void {
    if (!this.currentMesh) return;

    const box = new THREE.Box3().setFromObject(this.currentMesh);
    const center = box.getCenter(new THREE.Vector3());
    const size = box.getSize(new THREE.Vector3());

    const maxDim = Math.max(size.x, size.y, size.z);
    const distance = maxDim * 2;

    this.camera.position.set(
      center.x + distance,
      center.y + distance * 0.5,
      center.z + distance
    );
    this.controls.target.copy(center);
    this.controls.update();
  }

  // ═══════════════════════════════════════════════════════════════════════════
  // GROUND SELECTION
  // ═══════════════════════════════════════════════════════════════════════════

  private getMousePosition(event: MouseEvent): THREE.Vector2 {
    const rect = this.canvas.getBoundingClientRect();
    return new THREE.Vector2(
      ((event.clientX - rect.left) / rect.width) * 2 - 1,
      -((event.clientY - rect.top) / rect.height) * 2 + 1
    );
  }

  private onCanvasClick(event: MouseEvent): void {
    this.mouse = this.getMousePosition(event);
    this.raycaster.setFromCamera(this.mouse, this.camera);

    // Ground selection mode
    if (this.groundSelectionMode && this.currentMesh) {
      const intersects = this.raycaster.intersectObject(this.currentMesh);
      if (intersects.length > 0 && intersects[0].face) {
        const normal = intersects[0].face.normal.clone();
        normal.transformDirection(this.currentMesh.matrixWorld);
        this.alignToGround(normal);
        this.setGroundSelectionMode(false);
        this.callbacks.onGroundSelected?.();
      }
      return;
    }

    // Bone selection mode
    if (this.boneEditMode && this.boneSpheres.length > 0) {
      const intersects = this.raycaster.intersectObjects(this.boneSpheres);
      if (intersects.length > 0) {
        const boneIndex = intersects[0].object.userData.boneIndex as number;
        this.selectBone(boneIndex);
        this.callbacks.onBoneSelected?.(boneIndex);
      }
    }
  }

  private onCanvasMouseMove(event: MouseEvent): void {
    if (!this.groundSelectionMode || !this.currentMesh) return;

    this.mouse = this.getMousePosition(event);
    this.raycaster.setFromCamera(this.mouse, this.camera);

    const intersects = this.raycaster.intersectObject(this.currentMesh);

    // Clear previous highlight
    if (this.highlightedFace) {
      this.meshGroup.remove(this.highlightedFace);
      this.highlightedFace.geometry.dispose();
      (this.highlightedFace.material as THREE.Material).dispose();
      this.highlightedFace = null;
    }

    if (intersects.length > 0 && intersects[0].face && intersects[0].faceIndex !== undefined) {
      const intersection = intersects[0];
      const geometry = this.currentMesh.geometry;
      const positions = geometry.getAttribute("position");
      const indices = geometry.getIndex();

      if (indices) {
        const faceIndex = intersection.faceIndex!;
        const a = indices.getX(faceIndex * 3);
        const b = indices.getX(faceIndex * 3 + 1);
        const c = indices.getX(faceIndex * 3 + 2);

        const highlightGeometry = new THREE.BufferGeometry();
        const highlightPositions = new Float32Array([
          positions.getX(a), positions.getY(a), positions.getZ(a),
          positions.getX(b), positions.getY(b), positions.getZ(b),
          positions.getX(c), positions.getY(c), positions.getZ(c),
        ]);
        highlightGeometry.setAttribute("position", new THREE.BufferAttribute(highlightPositions, 3));

        const highlightMaterial = new THREE.MeshBasicMaterial({
          color: 0xbd93f9, // Dracula purple
          side: THREE.DoubleSide,
          transparent: true,
          opacity: 0.6,
          depthTest: false,
        });

        this.highlightedFace = new THREE.Mesh(highlightGeometry, highlightMaterial);
        this.highlightedFace.renderOrder = 999;
        this.meshGroup.add(this.highlightedFace);
      }
    }
  }

  private alignToGround(faceNormal: THREE.Vector3): void {
    const targetDirection = new THREE.Vector3(0, -1, 0);
    const quaternion = new THREE.Quaternion();
    quaternion.setFromUnitVectors(faceNormal.normalize(), targetDirection);

    this.meshGroup.applyQuaternion(quaternion);
    this.skeletonGroup.applyQuaternion(quaternion);

    this.repositionOnGround();
    this.fitCamera();
  }

  private repositionOnGround(): void {
    const box = new THREE.Box3().setFromObject(this.meshGroup);
    const offset = -box.min.y;
    this.meshGroup.position.y += offset;
    this.skeletonGroup.position.y += offset;
  }

  /**
   * Creates a wireframe geometry for quad meshes.
   * Unlike the standard WireframeGeometry (which shows triangle edges),
   * this draws only the 4 edges of each quad.
   */
  private createQuadWireframeGeometry(positions: Float32Array, quadIndices: Uint32Array): THREE.BufferGeometry {
    // Each quad has 4 vertices (v0, v1, v2, v3) and we draw 4 edges:
    // v0-v1, v1-v2, v2-v3, v3-v0
    // Each edge needs 2 vertices * 3 components = 6 floats
    // Each quad needs 4 edges = 24 floats
    const numQuads = quadIndices.length / 4;
    const linePositions = new Float32Array(numQuads * 4 * 2 * 3);

    let lineIdx = 0;
    for (let q = 0; q < numQuads; q++) {
      const baseIdx = q * 4;
      const v0 = quadIndices[baseIdx];
      const v1 = quadIndices[baseIdx + 1];
      const v2 = quadIndices[baseIdx + 2];
      const v3 = quadIndices[baseIdx + 3];

      // Get vertex positions
      const p0x = positions[v0 * 3], p0y = positions[v0 * 3 + 1], p0z = positions[v0 * 3 + 2];
      const p1x = positions[v1 * 3], p1y = positions[v1 * 3 + 1], p1z = positions[v1 * 3 + 2];
      const p2x = positions[v2 * 3], p2y = positions[v2 * 3 + 1], p2z = positions[v2 * 3 + 2];
      const p3x = positions[v3 * 3], p3y = positions[v3 * 3 + 1], p3z = positions[v3 * 3 + 2];

      // Edge v0 -> v1
      linePositions[lineIdx++] = p0x; linePositions[lineIdx++] = p0y; linePositions[lineIdx++] = p0z;
      linePositions[lineIdx++] = p1x; linePositions[lineIdx++] = p1y; linePositions[lineIdx++] = p1z;

      // Edge v1 -> v2
      linePositions[lineIdx++] = p1x; linePositions[lineIdx++] = p1y; linePositions[lineIdx++] = p1z;
      linePositions[lineIdx++] = p2x; linePositions[lineIdx++] = p2y; linePositions[lineIdx++] = p2z;

      // Edge v2 -> v3
      linePositions[lineIdx++] = p2x; linePositions[lineIdx++] = p2y; linePositions[lineIdx++] = p2z;
      linePositions[lineIdx++] = p3x; linePositions[lineIdx++] = p3y; linePositions[lineIdx++] = p3z;

      // Edge v3 -> v0
      linePositions[lineIdx++] = p3x; linePositions[lineIdx++] = p3y; linePositions[lineIdx++] = p3z;
      linePositions[lineIdx++] = p0x; linePositions[lineIdx++] = p0y; linePositions[lineIdx++] = p0z;
    }

    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute("position", new THREE.BufferAttribute(linePositions, 3));
    return geometry;
  }
}
