/**
 * Pinocchio 3D Viewer - Three.js visualization
 * Rewritten for SolidJS integration
 */

import * as THREE from "three";
import { THEME_EVENT, themeHex } from "./theme";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { TransformControls } from "three/addons/controls/TransformControls.js";
import { RoomEnvironment } from "three/addons/environments/RoomEnvironment.js";
import { boneWeight, paintRow } from "./weightPaint";
import { boneHue, estimateInfluence, type Influence } from "./boneInfluence";
import { boundsAfter, type FloorCandidate } from "./placement";
import type { Pose, Quat, Vec3 } from "./animation";
import { ViewCube } from "./ViewCube";

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
  /** Rangos de índices por material: [inicio, cantidad, material] (0xffffffff = sin material) */
  groups?: Uint32Array;
}

/** Material del archivo de origen, con sus texturas ya decodificadas */
export interface SceneMaterial {
  name: string;
  baseColor: [number, number, number, number];
  metallic: number;
  roughness: number;
  emissive: [number, number, number];
  normalScale: number;
  occlusionStrength: number;
  alphaMode: "opaque" | "mask" | "blend";
  alphaCutoff: number;
  unlit: boolean;
  maps: {
    base?: ImageBitmap;
    metallicRoughness?: ImageBitmap;
    normal?: ImageBitmap;
    occlusion?: ImageBitmap;
    emissive?: ImageBitmap;
  };
}

/** Luces del visor */
export interface LightSettings {
  /** Dirección de la luz principal (grados): acimut alrededor del modelo y elevación */
  azimuth: number;
  elevation: number;
  intensity: number;
  color: string;
  /** Luz de relleno (del lado opuesto) y ambiente */
  fill: number;
  ambient: number;
  /** Luz que sale desde la cámara */
  headlight: boolean;
  /** Reflejos de entorno (iluminación por imagen) */
  environment: number;
}

export const defaultLights: LightSettings = {
  azimuth: 45,
  elevation: 45,
  intensity: 2,
  color: "#ffffff",
  fill: 0.6,
  ambient: 0.3,
  headlight: false,
  environment: 0.6,
};

export interface SkeletonData {
  bones: BoneData[];
  edges: [number, number][];
  /** Centro de escala y rotación del esqueleto entero (por defecto, el de su caja) */
  pivot?: [number, number, number];
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
  /** Materiales y texturas del archivo de origen */
  showTextures?: boolean;
  /** Rayos X: la malla translúcida, para ver el esqueleto y lo de atrás */
  xray?: boolean;
  /** Opacidad de la malla con rayos X (0–1) */
  xrayAlpha?: number;
  /** Forma de los huesos: octaedros o líneas */
  boneDisplay?: BoneDisplay;
  /**
   * Notebook: el desplazamiento (dos dedos, o botón central + TrackPoint, que
   * el sistema convierte en rueda) gira la cámara; Shift desplaza, Ctrl acerca
   */
  trackpadNavigation?: boolean;
}

export type BoneDisplay = "octahedral" | "stick";

/** Unidades de la grilla del piso */
export interface GridUnits {
  /** Metros por unidad de la escena (según el archivo, o la que elija el usuario) */
  metersPerUnit: number;
  /** Metros por unidad en que se mide la grilla */
  unitMeters: number;
  /** "mm", "cm", "m", "in" */
  unitLabel: string;
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

/** Qué se elige al orientar: el piso, el frente o un punto (origen) */
export type PlacementMode = "floor" | "front" | "point";

/** Lo elegido con un clic al orientar */
export type PlacementPick =
  | { kind: "candidate"; index: number }
  | { kind: "surface"; point: [number, number, number]; normal: [number, number, number] };

export interface ViewerCallbacks {
  onWeightsPainted?: (stroke: PaintStroke) => void;
  /** La luz principal se movió arrastrando con L */
  onLightsChanged?: (lights: LightSettings) => void;
  onFpsUpdate?: (fps: number) => void;
  onPlacementPick?: (pick: PlacementPick) => void;
  onBoneSelected?: (index: number) => void;
  onBoneMoved?: (index: number, position: [number, number, number]) => void;
  /** Fin de un movimiento con G: posición inicial y final (para deshacer) */
  onBoneMoveCommitted?: (
    index: number,
    from: [number, number, number],
    to: [number, number, number]
  ) => void;
  /** Texto de ayuda de la operación en curso (null al terminar) */
  onHint?: (text: string | null) => void;
  /** La grilla cambió de escala: texto con el tamaño de celda */
  onGridChanged?: (label: string) => void;
  /** Clic derecho en el visor (sin operación que cancelar): posición en pantalla */
  onContextMenu?: (x: number, y: number) => void;
  /** Radio o intensidad del pincel cambiados con F / Shift+F */
  onPaintSettingsChanged?: (change: Partial<PaintSettings>) => void;
  /** Modo animación: se confirmó un giro (R) o desplazamiento (G) de la articulación */
  onPoseEdited?: (joint: number) => void;
  /** Se soltó el gizmo del esqueleto entero: factor de escala o giro (x, y, z, w) alrededor del pivote */
  onSkeletonTransformed?: (change: { scale: number } | { rotation: [number, number, number, number] }) => void;
  /**
   * Se soltó el gizmo del modelo: transformación a aplicar (matriz 4×4 por
   * columnas, como `apply_placement`). La malla queda en la vista previa
   * hasta que llegue la nueva (o `clearObjectPreview` si falla)
   */
  onObjectTransformed?: (matrix: number[], mode: "translate" | "rotate" | "scale") => void;
  /** Texto de la herramienta Medir (null: sin medida) */
  onMeasure?: (text: string | null) => void;
}

/**
 * Aristas de triángulos en blanco translúcido: contrasta con el fondo y con la
 * malla, y en mallas densas no satura la vista como un blanco opaco.
 */
function wireframeMaterial(): THREE.MeshBasicMaterial {
  return new THREE.MeshBasicMaterial({
    color: 0xf8f8f2, // Dracula foreground
    opacity: 0.35,
    transparent: true,
    depthWrite: false,
    wireframe: true,
  });
}

// ═══════════════════════════════════════════════════════════════════════════
// HEATMAP COLORS
// ═══════════════════════════════════════════════════════════════════════════

// Escala clásica de los editores 3D: azul (sin peso) → cian → verde → amarillo → rojo (peso 1)
const HEATMAP_COLORS = [
  new THREE.Color(0x0000ff),
  new THREE.Color(0x00ffff),
  new THREE.Color(0x00ff00),
  new THREE.Color(0xffff00),
  new THREE.Color(0xff0000),
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

/** Color propio de un hueso (el mismo en la malla y en su octaedro) */
function boneColor(bone: number): THREE.Color {
  return new THREE.Color().setHSL(boneHue(bone), 0.75, 0.55);
}

/** Zona sin influencia en la vista de todos los huesos */
const NO_INFLUENCE = new THREE.Color(0x44475a);

/**
 * Color del vértice `v`: con un hueso elegido, el mapa de calor de su peso;
 * con todos (−1), la mezcla de los colores de sus huesos según el peso.
 */
function weightColor(weights: Influence, v: number, bone: number, out: THREE.Color): THREE.Color {
  const k = weights.maxInfluences;
  const base = v * k * 2;
  if (bone >= 0) {
    let w = 0;
    for (let i = 0; i < k; i++) {
      if (weights.weights[base + i * 2] === bone) {
        w = weights.weights[base + i * 2 + 1];
        break;
      }
    }
    return out.copy(getHeatmapColor(w));
  }
  out.setRGB(0, 0, 0);
  let sum = 0;
  const c = new THREE.Color();
  for (let i = 0; i < k; i++) {
    const w = weights.weights[base + i * 2 + 1];
    if (w <= 0) continue;
    sum += w;
    c.copy(boneColor(weights.weights[base + i * 2]));
    out.r += c.r * w;
    out.g += c.g * w;
    out.b += c.b * w;
  }
  const rest = Math.max(0, 1 - sum);
  out.r += NO_INFLUENCE.r * rest;
  out.g += NO_INFLUENCE.g * rest;
  out.b += NO_INFLUENCE.b * rest;
  return out;
}

/**
 * Octaedro de un hueso de largo 1 sobre +Y, con la cintura al 10 % del largo
 * (el hueso "octaédrico" clásico de los editores 3D)
 */
function octahedronGeometry(): THREE.BufferGeometry {
  const w = 0.1;
  const head = [0, 0, 0];
  const tail = [0, 1, 0];
  const ring = [
    [w, w, 0],
    [0, w, -w],
    [-w, w, 0],
    [0, w, w],
  ];
  const positions: number[] = [];
  for (let i = 0; i < 4; i++) {
    const a = ring[i];
    const b = ring[(i + 1) % 4];
    positions.push(...head, ...b, ...a, ...tail, ...a, ...b);
  }
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
  return geometry;
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
  /** Influencia estimada mientras no hay pesos (se calcula al mostrarla) */
  private estimatedInfluence: Influence | null = null;
  /** Recalcular la estimación cuando el esqueleto deja de moverse */
  private influenceTimer: number | null = null;
  /** Octaedros de los huesos: relleno y aristas, por hueso (índice = articulación de la punta) */
  private boneShapes: { bone: number; fill: THREE.Mesh; edges: THREE.LineSegments }[] = [];

  // Pincel de pesos
  private paintSettings: PaintSettings | null = null;
  private paintMirror: { vertex: Uint32Array; bone: Uint32Array } | null = null;
  private paintNeighbors: { offsets: Uint32Array; list: Uint32Array } | null = null;
  /** Filas originales de los vértices tocados en el trazo en curso */
  private stroke: Map<number, Float32Array> | null = null;
  private lastDab: THREE.Vector3 | null = null;
  /** Modo del trazo en curso (depende de Ctrl/Shift al empezar) */
  private strokeMode: PaintSettings["mode"] = "add";
  private brushCursor: THREE.Mesh | null = null;

  /** Pose de prueba en curso: posiciones y normales de reposo para restaurar */
  private pose: {
    joint: number;
    positions: Float32Array;
    normals: Float32Array;
    /** Huesos que se mueven (descendientes de la articulación) */
    moving: boolean[];
  } | null = null;

  /**
   * Modo animación: la malla sigue al esqueleto con skinning en GPU. Un
   * `THREE.Bone` por hueso `b`, en la cabeza de su segmento (la posición del
   * padre), igual que el rig exportado; girar la articulación `J` gira los
   * huesos de sus hijos (la raíz, el suyo).
   */
  private animationMode = false;
  /** Última pose pedida: se vuelve a aplicar si el rig se rehace */
  private lastPose: Pose | null = null;
  private rig: {
    root: THREE.Group;
    bones: THREE.Bone[];
    parents: (number | null)[];
    children: number[][];
    /** Posición local de reposo de cada hueso */
    rest: THREE.Vector3[];
    /** Articulación respecto de la cabeza de su segmento (reposo) */
    tip: THREE.Vector3[];
    skeleton: THREE.Skeleton;
    /** Malla con skin; sin modelo (solo esqueleto) el rig son solo los huesos */
    mesh: THREE.SkinnedMesh | null;
    wireframe: THREE.SkinnedMesh | null;
  } | null = null;

  // Settings
  private settings: ViewerSettings = {
    showMesh: true,
    showWireframe: false,
    showSkeleton: true,
    showWeights: false,
    selectedBone: -1,
    showTextures: true,
    xray: false,
    xrayAlpha: 0.35,
    boneDisplay: "octahedral",
  };

  // Materiales del archivo de origen y sus texturas (compartidas entre materiales)
  private sceneMaterials: SceneMaterial[] = [];
  private sceneTextureCache = new Map<ImageBitmap, THREE.Texture>();

  // Luces
  private lights: LightSettings = { ...defaultLights };
  private keyLight = new THREE.DirectionalLight(0xffffff, 1);
  private fillLight = new THREE.DirectionalLight(0x8be9fd, 1);
  private ambientLight = new THREE.HemisphereLight(0xffffff, 0x44475a, 1);
  private headLight = new THREE.DirectionalLight(0xffffff, 0);
  private environmentMap: THREE.Texture | null = null;
  /** Arrastre de la luz principal (L + clic izquierdo) */
  private lightDrag: { x: number; y: number; azimuth: number; elevation: number } | null = null;
  private sunMarker: THREE.Mesh | null = null;

  // Cuadros bajo demanda y FPS
  private frameCount = 0;
  private fpsTimer: number | null = null;
  private animationId: number | null = null;
  /** Cubo de orientación de la esquina */
  private viewCube = new ViewCube();
  /** El último clic derecho canceló una operación modal */
  private suppressContextMenu = false;
  /** Cursor del lienzo antes de pasar sobre el cubo */
  private cursorBeforeCube: string | null = null;
  /** Giro animado de la cámara hacia una vista del cubo */
  private viewTransition: { from: THREE.Vector3; turn: THREE.Quaternion; start: number } | null = null;

  // Callbacks
  private callbacks: ViewerCallbacks = {};

  // Orientación: planos candidatos a piso o clic sobre la superficie
  private raycaster: THREE.Raycaster;
  private mouse: THREE.Vector2;
  private placementMode: PlacementMode | null = null;
  private placementGroup = new THREE.Group();
  private placementPlanes: THREE.Mesh[] = [];
  private hoveredPlane = -1;

  // Bone editing
  private boneSpheres: THREE.Mesh[] = [];
  private boneEditMode = false;
  private transformControls: TransformControls | null = null;
  private selectedBoneIndex = -1;
  private activeTool = "select";
  /** Asa del gizmo del esqueleto entero (herramienta Escalar, o Rotar sin pesos) */
  private skeletonPivot = new THREE.Object3D();
  /** Posiciones de las articulaciones al empezar a arrastrar ese gizmo */
  private pivotDragStart: THREE.Vector3[] | null = null;
  /** Las herramientas actúan sobre el modelo entero (no sobre el esqueleto) */
  private objectTools = false;
  /** Pivote del gizmo del modelo: la malla lo sigue como vista previa */
  private objectPivot = new THREE.Object3D();
  private objectDragStart: THREE.Matrix4 | null = null;
  /** Tras soltar el gizmo del modelo, la malla nueva llega sin mover la cámara */
  private keepCamera = false;
  /** Pivote del gizmo de pose en la animación (el rig gira huesos, no esferas) */
  private posePivot = new THREE.Object3D();
  private poseDragStart: {
    joint: number;
    rotation?: THREE.Quaternion;
    position: THREE.Vector3;
    pivot: THREE.Vector3;
  } | null = null;
  /** Herramienta Medir: puntos elegidos (0 a 2) y su dibujo */
  private measurePoints: THREE.Vector3[] = [];
  private measureGroup = new THREE.Group();

  // Gestos de Blender
  private keysDown = new Set<string>();
  private lastPointer = { x: 0, y: 0 };
  private modal: {
    kind: "grab" | "rotate" | "radius" | "strength";
    bone: number;
    mouse: { x: number; y: number };
    /** Posición inicial de la articulación, en coordenadas de mundo */
    start: THREE.Vector3;
    axis: "x" | "y" | "z" | null;
    /** Radio o intensidad inicial del pincel */
    value: number;
    /** Modo animación: giro y posición locales al empezar */
    startRotation?: THREE.Quaternion;
    startPosition?: THREE.Vector3;
  } | null = null;
  private pendingMove: { bone: number; position: [number, number, number] } | null = null;
  private moveFrame: number | null = null;
  private hintTimer: number | null = null;
  /** Grilla del piso: líneas menores, mayores (cada 10) y ejes X/Z */
  private grid = new THREE.Group();
  private gridUnits: GridUnits = { metersPerUnit: 1, unitMeters: 1, unitLabel: "m" };

  constructor(canvas: HTMLCanvasElement) {
    this.canvas = canvas;

    // Se dibuja solo cuando algo cambia (quieto no gasta CPU): todo cambio
    // pasa por un método del visor, sea de la API o de un evento, y cada
    // método pide un cuadro al terminar. La cámara lo pide al moverse
    const self = this as unknown as Record<string, unknown>;
    for (const name of Object.getOwnPropertyNames(Viewer3D.prototype)) {
      const method = Object.getOwnPropertyDescriptor(Viewer3D.prototype, name)?.value;
      if (name === "constructor" || name === "requestRender" || name === "dispose") continue;
      if (typeof method !== "function") continue;
      self[name] = (...args: unknown[]) => {
        const result = method.apply(this, args);
        this.requestRender();
        return result;
      };
    }

    // Fondo y grilla salen del tema (styles/app.css)
    this.scene = new THREE.Scene();
    this.scene.background = new THREE.Color(themeHex("viewport"));
    window.addEventListener(THEME_EVENT, () => this.applyTheme());

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
    // También durante la amortiguación, tras soltar el botón
    this.controls.addEventListener("change", () => this.requestRender());

    // Groups
    this.meshGroup = new THREE.Group();
    this.skeletonGroup = new THREE.Group();
    this.scene.add(this.meshGroup);
    this.scene.add(this.placementGroup);
    this.scene.add(this.skeletonGroup);
    this.scene.add(this.measureGroup);

    // Grid - Dracula style
    this.scene.add(this.grid);
    this.rebuildGrid();

    // Lights
    this.setupLights();

    // Raycaster
    this.raycaster = new THREE.Raycaster();
    this.mouse = new THREE.Vector2();

    // Event listeners
    this.setupEventListeners();

    this.fpsTimer = window.setInterval(() => {
      this.callbacks.onFpsUpdate?.(this.frameCount);
      this.frameCount = 0;
    }, 1000);
    this.requestRender();
  }

  private setupLights(): void {
    this.scene.add(this.ambientLight);
    this.scene.add(this.keyLight, this.keyLight.target);
    this.scene.add(this.fillLight, this.fillLight.target);
    // Luz de cámara: cuelga de la cámara y apunta hacia adelante
    this.camera.add(this.headLight, this.headLight.target);
    this.headLight.target.position.set(0, 0, -1);
    this.scene.add(this.camera);
    // Entorno para los reflejos de los materiales PBR (metales sobre todo)
    const pmrem = new THREE.PMREMGenerator(this.renderer);
    this.environmentMap = pmrem.fromScene(new RoomEnvironment(), 0.04).texture;
    pmrem.dispose();
    this.placeLights();
  }

  /** Cambia las luces */
  setLights(lights: LightSettings): void {
    this.lights = { ...lights };
    this.placeLights();
  }

  /** Luces actuales */
  getLights(): LightSettings {
    return { ...this.lights };
  }

  /** Ubica las luces alrededor del modelo según `this.lights` */
  private placeLights(): void {
    const s = this.lights;
    const center = this.controls.target.clone();
    const radius = this.currentMesh
      ? Math.max(new THREE.Box3().setFromObject(this.currentMesh).getSize(new THREE.Vector3()).length(), 1e-3)
      : 5;
    const direction = (azimuth: number, elevation: number) => {
      const a = THREE.MathUtils.degToRad(azimuth);
      const e = THREE.MathUtils.degToRad(elevation);
      return new THREE.Vector3(Math.cos(e) * Math.sin(a), Math.sin(e), Math.cos(e) * Math.cos(a));
    };
    this.keyLight.position.copy(center).addScaledVector(direction(s.azimuth, s.elevation), 2 * radius);
    this.keyLight.target.position.copy(center);
    this.keyLight.color.set(s.color);
    this.keyLight.intensity = s.intensity;
    this.fillLight.position.copy(center).addScaledVector(direction(s.azimuth + 180, 20), 2 * radius);
    this.fillLight.target.position.copy(center);
    this.fillLight.intensity = s.fill;
    this.ambientLight.intensity = s.ambient;
    this.headLight.intensity = s.headlight ? 0.6 * s.intensity : 0;
    this.scene.environment = s.environment > 0 ? this.environmentMap : null;
    this.scene.environmentIntensity = s.environment;
    if (this.sunMarker) {
      this.sunMarker.position.copy(this.keyLight.position);
      this.sunMarker.scale.setScalar(radius * 0.03);
    }
  }

  /** L + arrastrar: gira la luz principal alrededor del modelo */
  private onLightDown(event: PointerEvent): void {
    this.lightDrag = { x: event.clientX, y: event.clientY, azimuth: this.lights.azimuth, elevation: this.lights.elevation };
    this.controls.enabled = false;
    if (!this.sunMarker) {
      this.sunMarker = new THREE.Mesh(
        new THREE.SphereGeometry(1, 16, 12),
        new THREE.MeshBasicMaterial({ color: 0xf1fa8c, depthTest: false })
      );
      this.sunMarker.renderOrder = 3;
      this.scene.add(this.sunMarker);
    }
    this.sunMarker.visible = true;
    this.placeLights();
  }

  private onLightMove(event: PointerEvent): void {
    const drag = this.lightDrag;
    if (!drag) return;
    const azimuth = drag.azimuth - (event.clientX - drag.x) * 0.4;
    const elevation = THREE.MathUtils.clamp(drag.elevation + (event.clientY - drag.y) * 0.4, -10, 89);
    this.lights = { ...this.lights, azimuth: ((azimuth % 360) + 540) % 360 - 180, elevation };
    this.placeLights();
    this.callbacks.onLightsChanged?.(this.getLights());
  }

  private onLightUp(): void {
    if (!this.lightDrag) return;
    this.lightDrag = null;
    this.controls.enabled = true;
    if (this.sunMarker) this.sunMarker.visible = false;
  }

  private setupEventListeners(): void {
    // Resize
    const resizeObserver = new ResizeObserver(() => this.onResize());
    resizeObserver.observe(this.canvas.parentElement!);

    // Mouse events for ground selection
    this.canvas.addEventListener("click", (e) => this.onCanvasClick(e));
    this.canvas.addEventListener("mousemove", (e) => this.onCanvasMouseMove(e));

    // Navegación como Blender: antes que OrbitControls (fase de captura),
    // se decide qué hace cada botón según los modificadores
    window.addEventListener("pointerdown", (e) => {
      if (e.target !== this.canvas) return;
      // El cubo de orientación va primero: ni la cámara ni la selección ven el clic
      if (e.button === 0 && !this.modal && this.onViewCubeDown(e)) {
        e.preventDefault();
        e.stopImmediatePropagation();
        return;
      }
      this.configureNavigation(e);
    }, true);
    this.canvas.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      // El clic derecho que canceló G/R/F no abre el menú
      if (this.suppressContextMenu) {
        this.suppressContextMenu = false;
        return;
      }
      this.callbacks.onContextMenu?.(e.clientX, e.clientY);
    });
    window.addEventListener("wheel", (e) => {
      if (e.target === this.canvas) this.onTrackpadWheel(e);
    }, { capture: true, passive: false });

    // Clic: modal, luz, pincel, gizmo o selección (en ese orden)
    this.canvas.addEventListener("pointerdown", (e) => this.onPointerDown(e));
    window.addEventListener("pointermove", (e) => this.onPointerMove(e));
    window.addEventListener("pointerup", () => {
      this.onLightUp();
      this.finishStroke();
    });

    // Teclas mientras hay una operación modal (G, R, F…): antes que los atajos
    window.addEventListener("keydown", (e) => this.onModalKey(e), true);
    window.addEventListener("keydown", (e) => this.keysDown.add(e.key.toLowerCase()));
    window.addEventListener("keyup", (e) => this.keysDown.delete(e.key.toLowerCase()));
    window.addEventListener("blur", () => this.keysDown.clear());
  }

  // ═══════════════════════════════════════════════════════════════════════════
  // GESTOS DE BLENDER
  // ═══════════════════════════════════════════════════════════════════════════

  /**
   * Botón central orbita (Shift desplaza, Ctrl hace zoom); sin botón
   * central, lo mismo con Alt + izquierdo. El izquierdo solo no mueve la
   * cámara: selecciona, pinta o confirma.
   */
  private configureNavigation(e: PointerEvent): void {
    // OrbitControls ya cambia ROTATE por PAN si hay Shift (o Ctrl, por eso
    // Ctrl va aparte como DOLLY, que no mira modificadores)
    const action = (e.ctrlKey || e.metaKey) && !e.shiftKey ? THREE.MOUSE.DOLLY : THREE.MOUSE.ROTATE;
    const none = null as unknown as THREE.MOUSE;
    this.controls.mouseButtons = {
      LEFT: e.altKey && !this.modal ? action : none,
      MIDDLE: action,
      RIGHT: none,
    };
  }

  /**
   * Modo notebook: la rueda gira (Shift desplaza) en vez de acercar. Ctrl y el
   * gesto de pellizco (que llega como Ctrl + rueda) siguen haciendo zoom.
   */
  private onTrackpadWheel(e: WheelEvent): void {
    if (!this.settings.trackpadNavigation || e.ctrlKey || e.metaKey || !this.controls.enabled) return;
    e.preventDefault();
    e.stopImmediatePropagation();
    const unit = e.deltaMode === WheelEvent.DOM_DELTA_LINE ? 16 : e.deltaMode === WheelEvent.DOM_DELTA_PAGE ? 400 : 1;
    const dx = e.deltaX * unit;
    const dy = e.deltaY * unit;
    // Métodos internos de OrbitControls: el mismo camino que un arrastre
    const orbit = this.controls as unknown as {
      _rotateLeft(angle: number): void;
      _rotateUp(angle: number): void;
      _pan(dx: number, dy: number): void;
    };
    if (e.shiftKey) {
      orbit._pan(-dx, -dy);
    } else {
      const height = this.canvas.clientHeight || 1;
      orbit._rotateLeft((2 * Math.PI * dx) / height);
      orbit._rotateUp((2 * Math.PI * dy) / height);
    }
    this.controls.update();
  }

  /** Clic en el cubo: la cámara va a la vista de esa cara, arista o vértice */
  private onViewCubeDown(e: PointerEvent): boolean {
    if (!this.viewCube.contains(this.canvas, e.clientX, e.clientY)) return false;
    const dir = this.viewCube.pick(this.canvas, e.clientX, e.clientY);
    if (!dir) return false;
    this.lookFrom([dir.x, dir.y, dir.z]);
    return true;
  }

  /** Lleva la cámara (con un giro corto) a mirar desde `dir` hacia el centro de la vista */
  lookFrom(direction: Vec3): void {
    const dir = new THREE.Vector3(...direction);
    // Vista cenital pura: un pelo hacia el frente para que "arriba" en pantalla sea −Z
    if (dir.x === 0 && dir.z === 0) dir.z = 1e-3;
    const from = this.camera.position.clone().sub(this.controls.target);
    const turn = new THREE.Quaternion().setFromUnitVectors(from.clone().normalize(), dir.normalize());
    this.viewTransition = { from, turn, start: performance.now() };
  }

  private updateViewCubeHover(e: PointerEvent): void {
    const over = e.target === this.canvas && !this.modal && e.buttons === 0
      ? this.viewCube.pick(this.canvas, e.clientX, e.clientY)
      : null;
    this.viewCube.setHover(over);
    if (over && this.cursorBeforeCube === null) {
      this.cursorBeforeCube = this.canvas.style.cursor;
      this.canvas.style.cursor = "pointer";
    } else if (!over && this.cursorBeforeCube !== null) {
      this.canvas.style.cursor = this.cursorBeforeCube;
      this.cursorBeforeCube = null;
    }
  }

  /** Avanza el giro hacia la vista elegida en el cubo (~0,3 s, con frenado) */
  private stepViewTransition(): void {
    const transition = this.viewTransition;
    if (!transition) return;
    const t = Math.min(1, (performance.now() - transition.start) / 300);
    const eased = 1 - Math.pow(1 - t, 3);
    const turn = new THREE.Quaternion().slerp(transition.turn, eased);
    this.camera.position.copy(this.controls.target).add(transition.from.clone().applyQuaternion(turn));
    if (t < 1) this.requestRender();
    else this.viewTransition = null;
  }

  private onPointerDown(e: PointerEvent): void {
    this.lastPointer = { x: e.clientX, y: e.clientY };
    if (this.modal) {
      if (e.button === 0) this.confirmModal();
      else if (e.button === 2) {
        this.cancelModal();
        this.suppressContextMenu = true;
      }
      return;
    }
    if (e.button !== 0 || e.altKey) return;
    if (this.keysDown.has("l")) {
      this.onLightDown(e);
      return;
    }
    if (this.paintSettings) {
      this.onPaintDown(e);
      return;
    }
    if (this.placementMode) return;
    if (this.activeTool === "measure") {
      this.onMeasureDown(e);
      return;
    }
    // Sobre el gizmo de las herramientas, lo maneja el gizmo
    if (this.transformControls?.dragging || this.transformControls?.axis) return;
    const joint = this.pickJoint(e);
    if (joint !== this.selectedBoneIndex) {
      this.selectBone(joint);
      this.callbacks.onBoneSelected?.(joint);
    }
  }

  private onPointerMove(e: PointerEvent): void {
    this.updateViewCubeHover(e);
    if (e.target === this.canvas || this.modal) this.lastPointer = { x: e.clientX, y: e.clientY };
    if (this.modal) {
      this.modalMove(e.clientX, e.clientY);
      return;
    }
    this.onLightMove(e);
    if (e.target === this.canvas) this.onPaintMove(e);
  }

  /**
   * Articulación a menos de 16 px del cursor en pantalla; si no hay, el hueso
   * (segmento padre → articulación) a menos de 10 px, que se elige por su
   * articulación hija: es la que lleva sus pesos. -1 si ninguno.
   */
  private pickJoint(e: PointerEvent): number {
    if (!this.settings.showSkeleton || this.boneSpheres.length === 0) return -1;
    const rect = this.canvas.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;
    const p = new THREE.Vector3();
    const screen = this.boneSpheres.map((sphere) => {
      sphere.getWorldPosition(p).project(this.camera);
      return p.z > 1 ? null : { x: ((p.x + 1) / 2) * rect.width, y: ((1 - p.y) / 2) * rect.height };
    });
    let best = -1;
    let bestDistance = 16;
    screen.forEach((s, i) => {
      if (!s) return;
      const d = Math.hypot(s.x - mx, s.y - my);
      if (d < bestDistance) {
        bestDistance = d;
        best = i;
      }
    });
    if (best >= 0 || !this.skeletonData) return best;

    bestDistance = 10;
    for (const [parent, child] of this.skeletonData.edges) {
      const a = screen[parent];
      const b = screen[child];
      if (!a || !b) continue;
      const dx = b.x - a.x;
      const dy = b.y - a.y;
      const len2 = dx * dx + dy * dy;
      const t = len2 > 0 ? Math.max(0, Math.min(1, ((mx - a.x) * dx + (my - a.y) * dy) / len2)) : 0;
      const d = Math.hypot(a.x + t * dx - mx, a.y + t * dy - my);
      if (d < bestDistance) {
        bestDistance = d;
        best = child;
      }
    }
    return best;
  }

  /**
   * Empieza una operación modal como en Blender: "grab" (G) mueve la
   * articulación seleccionada, "rotate" (R) la gira como pose de prueba,
   * "radius" (F) y "strength" (Shift+F) ajustan el pincel. Devuelve `false`
   * si no se puede (sin selección, sin pesos…).
   */
  startModal(kind: "grab" | "rotate" | "radius" | "strength"): boolean {
    if (this.modal) return false;
    const bone = this.selectedBoneIndex;
    if ((kind === "grab" || kind === "rotate") && (bone < 0 || !this.boneSpheres[bone])) {
      this.flashHint("Selecciona una articulación (clic izquierdo cerca de ella)");
      return false;
    }
    if (kind === "rotate" && !this.canPose()) {
      this.flashHint("Calcula los pesos para probar poses con R");
      return false;
    }
    const rig = this.rig;
    if (rig && kind === "grab" && rig.parents[bone] !== null) {
      this.flashHint("En la animación solo se desplaza la raíz: gira las articulaciones con R");
      return false;
    }
    if (rig && kind === "rotate" && this.jointBones(bone).length === 0) {
      this.flashHint("Esta articulación es una punta: no tiene nada que girar");
      return false;
    }
    if ((kind === "radius" || kind === "strength") && !this.paintSettings) return false;
    const start = bone >= 0 && this.boneSpheres[bone] ? this.boneSpheres[bone].getWorldPosition(new THREE.Vector3()) : new THREE.Vector3();
    this.modal = {
      kind,
      bone,
      mouse: { ...this.lastPointer },
      start,
      axis: null,
      value: kind === "radius" ? this.paintSettings!.radius : kind === "strength" ? this.paintSettings!.strength : 0,
      startRotation: rig ? this.jointBones(bone)[0]?.quaternion.clone() : undefined,
      startPosition: rig ? rig.bones[bone].position.clone() : undefined,
    };
    this.controls.enabled = false;
    this.transformControls?.detach();
    this.showModalHint();
    return true;
  }

  /** Aviso breve (se borra solo) */
  private flashHint(text: string): void {
    this.callbacks.onHint?.(text);
    if (this.hintTimer !== null) clearTimeout(this.hintTimer);
    this.hintTimer = window.setTimeout(() => {
      this.hintTimer = null;
      if (!this.modal) this.callbacks.onHint?.(null);
    }, 2500);
  }

  /** Hay una operación modal en curso */
  isModal(): boolean {
    return this.modal !== null;
  }

  private canPose(): boolean {
    const mesh = this.meshData;
    const weights = this.weightsData;
    return !!mesh && !!weights && weights.numVertices * 3 === mesh.positions.length;
  }

  private showModalHint(): void {
    const m = this.modal;
    if (!m) return;
    const axis = m.axis ? ` · eje ${m.axis.toUpperCase()}` : "";
    const texts = {
      grab: "Mover: arrastra el mouse · X/Y/Z restringe al eje",
      rotate: "Rotar (pose de prueba): gira el mouse alrededor de la articulación · X/Y/Z eje",
      radius: "Radio del pincel: mueve el mouse a los lados",
      strength: "Intensidad del pincel: mueve el mouse a los lados",
    };
    this.callbacks.onHint?.(`${texts[m.kind]}${axis} · clic o Enter confirma · clic derecho o Esc cancela`);
  }

  private onModalKey(e: KeyboardEvent): void {
    const m = this.modal;
    if (!m) return;
    const key = e.key.toLowerCase();
    if (key === "escape") this.cancelModal();
    else if (key === "enter" || key === " ") this.confirmModal();
    else if ((key === "x" || key === "y" || key === "z") && (m.kind === "grab" || m.kind === "rotate")) {
      m.axis = m.axis === key ? null : key;
      this.showModalHint();
      this.modalMove(this.lastPointer.x, this.lastPointer.y);
    } else {
      return;
    }
    // La tecla la usa la operación: los atajos de la app no la ven
    e.preventDefault();
    e.stopImmediatePropagation();
  }

  /** Eje de la tecla X/Y/Z como se muestra (Z arriba), en coordenadas del visor */
  private axisVector(axis: "x" | "y" | "z"): THREE.Vector3 {
    return new THREE.Vector3(axis === "x" ? 1 : 0, axis === "z" ? 1 : 0, axis === "y" ? -1 : 0);
  }

  private modalMove(x: number, y: number): void {
    const m = this.modal;
    if (!m) return;
    const rect = this.canvas.getBoundingClientRect();
    const dx = x - m.mouse.x;
    const dy = y - m.mouse.y;
    if (m.kind === "radius" || m.kind === "strength") {
      const settings = this.paintSettings!;
      const value = m.kind === "radius"
        ? THREE.MathUtils.clamp(m.value * Math.exp(dx * 0.01), 0.002, 0.3)
        : THREE.MathUtils.clamp(m.value + dx * 0.004, 0.02, 1);
      this.paintSettings = { ...settings, [m.kind]: value };
      this.callbacks.onPaintSettingsChanged?.({ [m.kind]: value });
      return;
    }
    const sphere = this.boneSpheres[m.bone];
    if (!sphere?.parent) return;
    if (m.kind === "grab") {
      // El punto sigue al mouse en el plano de la pantalla, a la misma profundidad
      const s = m.start.clone().project(this.camera);
      const target = new THREE.Vector3(s.x + (dx / rect.width) * 2, s.y - (dy / rect.height) * 2, s.z).unproject(this.camera);
      if (m.axis) {
        // Como Blender: el movimiento del mouse proyectado sobre el eje tal
        // como se ve en pantalla (píxeles por unidad a lo largo del eje)
        const axis = this.axisVector(m.axis);
        const toPixels = (p: THREE.Vector3) => {
          const q = p.clone().project(this.camera);
          return new THREE.Vector2((q.x + 1) * rect.width / 2, (1 - q.y) * rect.height / 2);
        };
        const along = toPixels(m.start.clone().add(axis)).sub(toPixels(m.start));
        const t = along.lengthSq() > 1e-6 ? (dx * along.x + dy * along.y) / along.lengthSq() : 0;
        target.copy(m.start).addScaledVector(axis, t);
      }
      if (this.rig) {
        // La raíz no tiene padre: su posición local es la del mundo del rig
        this.rig.bones[m.bone].position.copy(this.rig.root.worldToLocal(target.clone()));
        this.updatePosedSpheres();
        return;
      }
      sphere.position.copy(sphere.parent.worldToLocal(target.clone()));
      this.updateBoneLines();
      this.scheduleBoneMoved(m.bone, sphere.position);
      return;
    }
    // Rotar: ángulo del mouse alrededor de la articulación en pantalla
    const c = m.start.clone().project(this.camera);
    const cx = rect.left + ((c.x + 1) / 2) * rect.width;
    const cy = rect.top + ((1 - c.y) / 2) * rect.height;
    const angle = -(Math.atan2(y - cy, x - cx) - Math.atan2(m.mouse.y - cy, m.mouse.x - cx));
    const axisWorld = m.axis ? this.axisVector(m.axis) : this.camera.getWorldDirection(new THREE.Vector3()).negate();
    // Al espacio de la malla (el grupo puede estar girado por "suelo")
    const groupRotation = this.meshGroup.getWorldQuaternion(new THREE.Quaternion());
    const axisLocal = axisWorld.applyQuaternion(groupRotation.clone().invert()).normalize();
    if (this.rig && m.startRotation) {
      this.rotateJoint(m.bone, m.startRotation, new THREE.Quaternion().setFromAxisAngle(axisWorld.normalize(), angle));
      return;
    }
    this.applyPose(m.bone, new THREE.Quaternion().setFromAxisAngle(axisLocal, angle));
  }

  /** Avisa el movimiento de una articulación, como mucho una vez por cuadro */
  private scheduleBoneMoved(bone: number, position: THREE.Vector3): void {
    this.pendingMove = { bone, position: [position.x, position.y, position.z] };
    if (this.moveFrame !== null) return;
    this.moveFrame = requestAnimationFrame(() => {
      this.moveFrame = null;
      const move = this.pendingMove;
      this.pendingMove = null;
      if (move) this.callbacks.onBoneMoved?.(move.bone, move.position);
    });
  }

  private confirmModal(): void {
    const m = this.modal;
    if (!m) return;
    this.modal = null;
    this.controls.enabled = true;
    this.callbacks.onHint?.(null);
    if (this.rig && (m.kind === "grab" || m.kind === "rotate")) {
      this.callbacks.onPoseEdited?.(m.bone);
      return;
    }
    if (m.kind === "grab") {
      const sphere = this.boneSpheres[m.bone];
      const from = sphere.parent!.worldToLocal(m.start.clone());
      this.callbacks.onBoneMoveCommitted?.(m.bone, [from.x, from.y, from.z], [sphere.position.x, sphere.position.y, sphere.position.z]);
    }
    if (this.boneEditMode) this.selectBone(this.selectedBoneIndex);
  }

  private cancelModal(): void {
    const m = this.modal;
    if (!m) return;
    this.modal = null;
    this.controls.enabled = true;
    this.callbacks.onHint?.(null);
    if (this.rig && (m.kind === "grab" || m.kind === "rotate")) {
      if (m.startRotation) for (const bone of this.jointBones(m.bone)) bone.quaternion.copy(m.startRotation);
      if (m.startPosition) this.rig.bones[m.bone].position.copy(m.startPosition);
      this.updatePosedSpheres();
      return;
    }
    if (m.kind === "grab") {
      const sphere = this.boneSpheres[m.bone];
      sphere.position.copy(sphere.parent!.worldToLocal(m.start.clone()));
      this.updateBoneLines();
      this.scheduleBoneMoved(m.bone, sphere.position);
    } else if (m.kind === "rotate") {
      this.resetPose();
    } else {
      const value = m.value;
      this.paintSettings = this.paintSettings ? { ...this.paintSettings, [m.kind]: value } : null;
      this.callbacks.onPaintSettingsChanged?.({ [m.kind]: value });
    }
    if (this.boneEditMode) this.selectBone(this.selectedBoneIndex);
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
    // Como en Blender: Ctrl invierte (sumar ↔ restar) y Shift suaviza
    const mode = this.paintSettings!.mode;
    this.strokeMode = event.shiftKey
      ? "smooth"
      : event.ctrlKey || event.metaKey
        ? mode === "subtract" ? "add" : "subtract"
        : mode;
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
    const smoothTargets = this.strokeMode === "smooth" ? this.smoothTargets(hits, settings.bone) : null;
    hits.forEach(([v, falloff], i) => {
      this.paintVertex(v, settings.bone, falloff * settings.strength, smoothTargets?.[i]);
      const mv = mirror?.vertex[v];
      if (mirror && mv !== undefined && mv !== 0xffffffff && mv !== v) {
        const pair = mirror.bone[settings.bone];
        const mb = pair === 0xffffffff ? settings.bone : pair;
        const target = this.strokeMode === "smooth" ? this.smoothTargets([[mv, 1]], mb)[0] : undefined;
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
    if (paintRow(weights.weights, base, k, bone, this.strokeMode, amount, smoothTarget, this.fallbackBone(bone))) {
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
    const color = weightColor(this.weightsData, v, this.settings.selectedBone, new THREE.Color());
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

  /** Pide un cuadro; varias peticiones antes del próximo se juntan en uno */
  requestRender(): void {
    if (this.animationId === null) {
      this.animationId = requestAnimationFrame(this.animate);
    }
  }

  private animate = (): void => {
    this.animationId = null;
    if (this.viewTransition) {
      this.stepViewTransition();
      // Sin arrastre residual de la amortiguación durante el giro
      this.controls.enableDamping = this.viewTransition === null;
    }
    // Si la cámara sigue amortiguando, su evento "change" pide el siguiente
    this.controls.update();
    this.renderer.render(this.scene, this.camera);
    this.viewCube.render(this.renderer, this.camera, this.controls.target);
    this.frameCount++;
  };

  // ═══════════════════════════════════════════════════════════════════════════
  // PUBLIC API
  // ═══════════════════════════════════════════════════════════════════════════

  setCallbacks(callbacks: ViewerCallbacks): void {
    this.callbacks = callbacks;
  }

  loadMesh(data: MeshData): void {
    this.resetPose();
    this.resetObjectPreview();
    this.clearMeasure();
    this.meshData = data;
    this.paintNeighbors = null;
    this.estimatedInfluence = null;
    this.clearMesh();

    // Create geometry
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute("position", new THREE.BufferAttribute(data.positions, 3));
    geometry.setAttribute("normal", new THREE.BufferAttribute(data.normals, 3));
    if (data.uvs) {
      geometry.setAttribute("uv", new THREE.BufferAttribute(data.uvs, 2));
    }
    geometry.setIndex(new THREE.BufferAttribute(data.indices, 1));

    const material = this.buildMaterial(geometry);

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
      this.currentWireframe = new THREE.Mesh(geometry, wireframeMaterial());
      this.currentWireframe.visible = this.settings.showWireframe;
      this.meshGroup.add(this.currentWireframe);
    }

    // Tras mover el modelo con el gizmo, la cámara queda donde estaba
    if (this.keepCamera) this.placeLights();
    else this.fitCamera();
    this.keepCamera = false;
    this.rebuildGrid();
    this.applySettings();
    this.buildRig();
    this.attachGizmo();
  }

  /** Texturas de la piel; se aplican solo si la malla actual tiene UV */
  setTextures(textures: MeshTextures): void {
    this.textures = textures;
    // Con los materiales del archivo de origen la piel del paso UV no aplica
    if (this.currentMesh && !Array.isArray(this.currentMesh.material)) {
      this.applyTextures(this.currentMesh.material as THREE.MeshStandardMaterial, this.currentMesh.geometry);
    }
  }

  /** Materiales del archivo de origen (el índice es el de los grupos de la malla) */
  setSceneMaterials(materials: SceneMaterial[]): void {
    for (const texture of this.sceneTextureCache.values()) texture.dispose();
    this.sceneTextureCache.clear();
    this.sceneMaterials = materials;
    this.refreshMeshMaterial();
  }

  private refreshMeshMaterial(): void {
    const mesh = this.currentMesh;
    if (!mesh) return;
    this.disposeMaterial(mesh.material);
    mesh.material = this.buildMaterial(mesh.geometry);
    if (this.rig?.mesh) this.rig.mesh.material = mesh.material;
    this.applyXray();
  }

  /**
   * Rayos X: la malla (y el mapa de pesos) translúcida y sin escribir
   * profundidad, así se ven el esqueleto y las caras de atrás. Guarda la
   * opacidad propia de cada material para volver a ella.
   */
  private applyXray(): void {
    const xray = this.settings.xray === true;
    const alpha = this.settings.xrayAlpha ?? 0.35;
    for (const mesh of [this.currentMesh, this.weightsMesh]) {
      if (!mesh) continue;
      // Los colores de los pesos se leen mal muy translúcidos: algo más opacos
      const opacity = mesh === this.weightsMesh ? 0.3 + 0.7 * alpha : alpha;
      for (const m of Array.isArray(mesh.material) ? mesh.material : [mesh.material]) {
        const base = (m.userData.opaque ??= { opacity: m.opacity, transparent: m.transparent, depthWrite: m.depthWrite }) as {
          opacity: number;
          transparent: boolean;
          depthWrite: boolean;
        };
        const transparent = xray || base.transparent;
        if (transparent !== m.transparent) m.needsUpdate = true;
        m.transparent = transparent;
        m.opacity = base.opacity * (xray ? opacity : 1);
        m.depthWrite = xray ? false : base.depthWrite;
      }
    }
  }

  private disposeMaterial(material: THREE.Material | THREE.Material[]): void {
    for (const m of Array.isArray(material) ? material : [material]) {
      // Las texturas de los materiales de origen son compartidas (caché)
      if (!m.userData.sceneMaterial) {
        const standard = m as THREE.MeshStandardMaterial;
        standard.map?.dispose();
        standard.normalMap?.dispose();
      }
      m.dispose();
    }
  }

  private defaultMaterial(): THREE.MeshStandardMaterial {
    return new THREE.MeshStandardMaterial({ color: 0x6272a4, metalness: 0.1, roughness: 0.7, side: THREE.DoubleSide });
  }

  /**
   * Material de la malla: los del archivo de origen por grupo (si la malla
   * los trae y "texturas" está activo), o el neutro con la piel del paso UV.
   */
  private buildMaterial(geometry: THREE.BufferGeometry): THREE.Material | THREE.Material[] {
    const groups = this.meshData?.groups;
    geometry.clearGroups();
    if (this.settings.showTextures !== false && groups && groups.length >= 3 && this.sceneMaterials.length > 0) {
      const fallback = this.sceneMaterials.length;
      for (let g = 0; g + 2 < groups.length; g += 3) {
        geometry.addGroup(groups[g], groups[g + 1], groups[g + 2] < fallback ? groups[g + 2] : fallback);
      }
      const hasUv = geometry.getAttribute("uv") !== undefined;
      return [...this.sceneMaterials.map((m) => this.threeMaterial(m, hasUv)), this.defaultMaterial()];
    }
    const material = this.defaultMaterial();
    this.applyTextures(material, geometry);
    return material;
  }

  /** Textura compartida de una imagen (convención glTF: sin voltear) */
  private sharedTexture(image: ImageBitmap, srgb: boolean): THREE.Texture {
    let texture = this.sceneTextureCache.get(image);
    if (!texture) {
      texture = new THREE.Texture(image);
      texture.flipY = false;
      texture.wrapS = texture.wrapT = THREE.RepeatWrapping;
      texture.anisotropy = this.renderer.capabilities.getMaxAnisotropy();
      if (srgb) texture.colorSpace = THREE.SRGBColorSpace;
      texture.needsUpdate = true;
      this.sceneTextureCache.set(image, texture);
    }
    return texture;
  }

  /** Material de Three para un material PBR de glTF */
  private threeMaterial(m: SceneMaterial, hasUv: boolean): THREE.Material {
    const map = (image: ImageBitmap | undefined, srgb: boolean) => (hasUv && image ? this.sharedTexture(image, srgb) : null);
    // Los factores de glTF son lineales, como el espacio de trabajo de Three
    const color = new THREE.Color().setRGB(m.baseColor[0], m.baseColor[1], m.baseColor[2]);
    const common = {
      color,
      map: map(m.maps.base, true),
      transparent: m.alphaMode === "blend",
      opacity: m.baseColor[3],
      alphaTest: m.alphaMode === "mask" ? m.alphaCutoff : 0,
      side: THREE.DoubleSide,
    };
    let material: THREE.Material;
    if (m.unlit) {
      material = new THREE.MeshBasicMaterial(common);
    } else {
      const metalRough = map(m.maps.metallicRoughness, false);
      material = new THREE.MeshStandardMaterial({
        ...common,
        metalness: m.metallic,
        roughness: m.roughness,
        // glTF guarda rugosidad en G y metal en B, como lee Three
        metalnessMap: metalRough,
        roughnessMap: metalRough,
        normalMap: map(m.maps.normal, false),
        // Sin tangentes Three usa derivadas de pantalla, con Y opuesto a glTF
        normalScale: new THREE.Vector2(m.normalScale, -m.normalScale),
        aoMap: map(m.maps.occlusion, false),
        aoMapIntensity: m.occlusionStrength,
        emissive: new THREE.Color().setRGB(m.emissive[0], m.emissive[1], m.emissive[2]),
        emissiveMap: map(m.maps.emissive, true),
      });
    }
    material.name = m.name;
    material.userData.sceneMaterial = true;
    return material;
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
      this.refreshEstimatedWeights();
      this.buildRig();
      this.attachGizmo();
      return;
    }
    this.clearSkeleton();
    this.boneSpheres = [];
    this.estimatedInfluence = null;

    const bonesGroup = new THREE.Group();
    bonesGroup.name = "bones";
    this.skeletonGroup.add(bonesGroup);

    // Joint spheres. Siempre por delante de la malla ("In Front" de Blender):
    // el esqueleto va dentro del modelo y si no quedaría tapado
    const jointGeometry = new THREE.SphereGeometry(this.jointRadius(), 16, 16);
    const jointMaterial = new THREE.MeshBasicMaterial({ color: 0xffb86c, depthTest: false }); // Dracula orange
    const leafMaterial = new THREE.MeshBasicMaterial({ color: 0x50fa7b, depthTest: false }); // Dracula green

    for (let i = 0; i < data.bones.length; i++) {
      const bone = data.bones[i];
      const material = bone.isLeaf ? leafMaterial.clone() : jointMaterial.clone();
      const sphere = new THREE.Mesh(jointGeometry, material);
      sphere.position.set(bone.position[0], bone.position[1], bone.position[2]);
      sphere.userData.boneName = bone.name;
      sphere.userData.boneIndex = i;
      sphere.userData.isLeaf = bone.isLeaf;
      sphere.renderOrder = 6;
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
        depthTest: false,
      });
      const lines = new THREE.LineSegments(lineGeometry, lineMaterial);
      lines.renderOrder = 5;
      lines.name = "boneLines";
      this.skeletonGroup.add(lines);
    }

    // Octaedros, uno por hueso (segmento padre → articulación)
    const shapes = new THREE.Group();
    shapes.name = "boneShapes";
    this.skeletonGroup.add(shapes);
    const octahedron = octahedronGeometry();
    const octahedronEdges = new THREE.EdgesGeometry(octahedron);
    this.boneShapes = data.edges.map(([, child]) => {
      const fill = new THREE.Mesh(
        octahedron,
        new THREE.MeshBasicMaterial({ transparent: true, opacity: 0.35, depthTest: false, depthWrite: false, side: THREE.DoubleSide })
      );
      fill.renderOrder = 4;
      const edges = new THREE.LineSegments(octahedronEdges, new THREE.LineBasicMaterial({ depthTest: false }));
      edges.renderOrder = 5;
      shapes.add(fill, edges);
      return { bone: child, fill, edges };
    });
    this.updateBoneLines();

    this.skeletonGroup.visible = this.settings.showSkeleton;
    // Las esferas son nuevas: volver a marcar (y enganchar) la seleccionada
    if (this.selectedBoneIndex >= 0 && this.selectedBoneIndex < this.boneSpheres.length) {
      this.selectBone(this.selectedBoneIndex);
    }
    // Sin modelo, el esqueleto es lo que hay que encuadrar
    if (!this.currentMesh) this.fitCamera();
    this.buildRig();
    this.attachGizmo();
  }

  /** Radio de las esferas de las articulaciones, proporcional a la malla (o al esqueleto, sin malla) */
  private jointRadius(): number {
    const size = this.currentMesh
      ? new THREE.Box3().setFromObject(this.currentMesh).getSize(new THREE.Vector3())
      : this.skeletonBox()?.getSize(new THREE.Vector3());
    return size ? Math.max(size.length() * 0.012, 1e-4) : 0.02;
  }

  /** Caja de las articulaciones en reposo, en el espacio del visor */
  private skeletonBox(): THREE.Box3 | null {
    const bones = this.skeletonData?.bones;
    if (!bones || bones.length === 0) return null;
    const box = new THREE.Box3();
    for (const b of bones) box.expandByPoint(new THREE.Vector3(...b.position));
    return box.applyMatrix4(this.meshGroup.matrixWorld);
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

    const up = new THREE.Vector3(0, 1, 0);
    const dir = new THREE.Vector3();
    this.skeletonData.edges.forEach(([parent, child], k) => {
      const shape = this.boneShapes[k];
      if (!shape) return;
      const a = this.boneSpheres[parent].position;
      dir.subVectors(this.boneSpheres[child].position, a);
      const length = dir.length();
      for (const o of [shape.fill, shape.edges]) {
        o.position.copy(a);
        o.scale.setScalar(Math.max(length, 1e-9));
        if (length > 0) o.quaternion.setFromUnitVectors(up, dir.divideScalar(length));
      }
    });
  }

  /**
   * Colores de los octaedros: el propio de cada hueso con la vista de todos
   * los pesos, violeta el elegido, naranja el resto
   */
  private recolorBoneShapes(): void {
    const byBone = this.settings.showWeights && this.settings.selectedBone < 0 && this.displayWeights() !== null;
    for (const { bone, fill, edges } of this.boneShapes) {
      const color =
        bone === this.selectedBoneIndex ? new THREE.Color(0xbd93f9) : byBone ? boneColor(bone) : new THREE.Color(0xffb86c);
      (fill.material as THREE.MeshBasicMaterial).color.copy(color);
      (edges.material as THREE.LineBasicMaterial).color.copy(color);
    }
  }

  private applyBoneDisplay(): void {
    const octahedral = this.settings.boneDisplay !== "stick";
    const shapes = this.skeletonGroup.getObjectByName("boneShapes");
    const lines = this.skeletonGroup.getObjectByName("boneLines");
    if (shapes) shapes.visible = octahedral;
    if (lines) lines.visible = !octahedral;
    this.recolorBoneShapes();
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

  /** Quita el esqueleto (y el rig que lo usa) del visor */
  unloadSkeleton(): void {
    if (!this.skeletonData) return;
    this.resetPose();
    this.disposeRig();
    this.clearSkeleton();
    this.boneSpheres = [];
    this.skeletonData = null;
    this.estimatedInfluence = null;
    this.selectedBoneIndex = -1;
    this.applySettings();
    this.attachGizmo();
  }

  /** Quita los pesos: sin heatmap ni rig */
  unloadWeights(): void {
    if (!this.weightsData) return;
    this.resetPose();
    this.disposeRig();
    this.weightsData = null;
    if (this.weightsMesh) {
      this.meshGroup.remove(this.weightsMesh);
      this.weightsMesh.geometry.dispose();
      (this.weightsMesh.material as THREE.Material).dispose();
      this.weightsMesh = null;
    }
    this.applySettings();
    this.attachGizmo();
  }

  loadWeights(data: WeightsData): void {
    this.resetPose();
    this.weightsData = data;
    this.applySettings();
    this.buildRig();
    // Con pesos, Rotar pasa a probar poses por articulación
    this.attachGizmo();
  }

  updateSettings(settings: Partial<ViewerSettings>): void {
    const texturesChanged = settings.showTextures !== undefined && settings.showTextures !== this.settings.showTextures;
    this.settings = { ...this.settings, ...settings };
    if (texturesChanged) this.refreshMeshMaterial();
    this.applySettings();
  }

  /**
   * Modo de orientación: con "floor" se dibujan los planos candidatos (verdes
   * los estables); con "front" y "point" se elige un punto de la superficie.
   */
  setPlacementMode(mode: PlacementMode | null, candidates: FloorCandidate[] = []): void {
    this.placementMode = mode;
    this.canvas.style.cursor = mode ? "crosshair" : "default";
    // Al elegir piso, frente u origen el gizmo del modelo estorba
    this.attachGizmo();
    for (const child of [...this.placementGroup.children]) {
      this.placementGroup.remove(child);
      if (child instanceof THREE.Mesh || child instanceof THREE.LineLoop) {
        child.geometry.dispose();
        (child.material as THREE.Material).dispose();
      }
    }
    this.placementPlanes = [];
    this.hoveredPlane = -1;
    if (mode !== "floor") return;

    // Un poco hacia afuera para que no parpadee contra la malla
    const size = this.meshData ? boundsAfter(this.meshData.positions).getSize(new THREE.Vector3()).length() : 1;
    for (const c of candidates) {
      const normal = new THREE.Vector3(...c.normal);
      const lift = normal.clone().multiplyScalar(size * 2e-3);
      const ring = c.polygon.map((p) => new THREE.Vector3(...p).add(lift));
      const fan: number[] = [];
      for (let i = 1; i + 1 < ring.length; i++) {
        for (const p of [ring[0], ring[i], ring[i + 1]]) fan.push(p.x, p.y, p.z);
      }
      const geometry = new THREE.BufferGeometry();
      geometry.setAttribute("position", new THREE.Float32BufferAttribute(fan, 3));
      const plane = new THREE.Mesh(
        geometry,
        new THREE.MeshBasicMaterial({
          color: c.stable ? 0x50fa7b : 0xff5555,
          transparent: true,
          opacity: this.planeOpacity(c.stable, false),
          side: THREE.DoubleSide,
          depthWrite: false,
        })
      );
      plane.userData.stable = c.stable;
      plane.renderOrder = 998;
      this.placementPlanes.push(plane);
      this.placementGroup.add(plane);
      const outline = new THREE.LineLoop(
        new THREE.BufferGeometry().setFromPoints(ring),
        new THREE.LineBasicMaterial({ color: c.stable ? 0x50fa7b : 0xff5555, transparent: true, opacity: 0.6 })
      );
      this.placementGroup.add(outline);
    }
  }

  private planeOpacity(stable: boolean, hovered: boolean): number {
    if (hovered) return 0.6;
    return stable ? 0.22 : 0.08;
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
    if (enabled) this.ensureTransformControls();
    this.attachGizmo();
  }

  /** Herramientas sobre el modelo entero (Preparar, Imprimir 3D) o sobre el esqueleto */
  setObjectTools(enabled: boolean): void {
    if (enabled === this.objectTools) return;
    this.objectTools = enabled;
    this.attachGizmo();
  }

  /**
   * Colores del gizmo como en la vista (Z arriba): el eje vertical del visor
   * (Y) es Z, azul; el de profundidad (Z) es Y, verde. Los materiales son
   * compartidos entre asas: se cambian una vez cada uno.
   */
  private recolorGizmo(controls: TransformControls): void {
    const colors = new Map([
      [0xff0000, themeHex("axis-x")],
      [0x00ff00, themeHex("axis-z")],
      [0x0000ff, themeHex("axis-y")],
    ]);
    const seen = new Set<THREE.Material>();
    controls.getHelper().traverse((object) => {
      const material = (object as THREE.Mesh).material as THREE.MeshBasicMaterial | undefined;
      if (!material?.color || seen.has(material)) return;
      seen.add(material);
      const color = colors.get(material.color.getHex());
      if (color !== undefined) material.color.setHex(color);
    });
  }

  private gizmoMode(): "translate" | "rotate" | "scale" {
    return this.activeTool === "rotate" ? "rotate" : this.activeTool === "scale" ? "scale" : "translate";
  }

  /** El gizmo se crea la primera vez que una herramienta lo necesita */
  private ensureTransformControls(): TransformControls {
    if (this.transformControls) return this.transformControls;
    const controls = new TransformControls(this.camera, this.canvas);
    this.transformControls = controls;
    controls.setMode(this.gizmoMode());
    controls.setSize(0.5);
    this.recolorGizmo(controls);
    this.scene.add(controls.getHelper());
    // Resalta ejes al pasar el mouse sin pasar por métodos del visor
    controls.addEventListener("change", () => this.requestRender());

    controls.addEventListener("dragging-changed", (event) => {
      this.controls.enabled = !event.value;
      const object = controls.object;
      if (object === this.objectPivot) {
        if (event.value) this.objectDragStart = this.pivotMatrix(this.objectPivot);
        else this.commitObjectGizmo();
      } else if (object === this.posePivot) {
        if (event.value) this.startPoseGizmo();
        else this.commitPoseGizmo();
      } else if (object === this.skeletonPivot) {
        if (event.value) this.pivotDragStart = this.boneSpheres.map((s) => s.position.clone());
        else this.commitSkeletonGizmo();
      }
    });

    controls.addEventListener("objectChange", () => {
      const object = controls.object;
      if (object === this.objectPivot) {
        this.previewObjectGizmo();
        return;
      }
      if (object === this.posePivot) {
        this.previewPoseGizmo();
        return;
      }
      if (object === this.skeletonPivot) {
        this.previewSkeletonGizmo();
        return;
      }
      if (this.selectedBoneIndex < 0 || !object) return;
      if (controls.getMode() === "rotate") {
        // Rotar una articulación con pesos calculados: pose de prueba
        this.applyPose(this.selectedBoneIndex, object.quaternion);
      } else {
        const pos = object.position;
        this.callbacks.onBoneMoved?.(this.selectedBoneIndex, [pos.x, pos.y, pos.z]);
      }
    });
    return controls;
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

    }
    this.recolorBoneShapes();
    this.attachGizmo();
  }

  /**
   * Herramienta que actúa sobre el esqueleto entero: Escalar siempre (escalar
   * una articulación no significa nada) y Rotar mientras no haya pesos para
   * probar poses
   */
  private wholeSkeletonTool(): "scale" | "rotate" | null {
    if (this.rig || this.boneSpheres.length === 0) return null;
    if (this.activeTool === "scale") return "scale";
    if (this.activeTool === "rotate" && !this.canPose()) return "rotate";
    return null;
  }

  /** Engancha el gizmo a lo que corresponde según la herramienta y la selección */
  private attachGizmo(): void {
    if (this.transformControls?.dragging) return;
    if (this.objectTools) {
      const wanted = ["move", "rotate", "scale"].includes(this.activeTool);
      if (!wanted || !this.currentMesh || this.placementMode) {
        this.transformControls?.detach();
        return;
      }
      const controls = this.ensureTransformControls();
      controls.setMode(this.gizmoMode());
      controls.setSize(0.9);
      this.placeObjectPivot();
      controls.attach(this.objectPivot);
      return;
    }
    const controls = this.transformControls;
    if (!controls) return;
    if (!this.boneEditMode) {
      controls.detach();
      return;
    }
    controls.setSize(0.5);
    if (this.wholeSkeletonTool()) {
      const pivot = this.skeletonPivot;
      if (pivot.parent !== this.skeletonGroup) this.skeletonGroup.add(pivot);
      pivot.position.copy(this.skeletonCenter());
      pivot.quaternion.identity();
      pivot.scale.setScalar(1);
      controls.attach(pivot);
      return;
    }
    const joint = this.selectedBoneIndex;
    const sphere = this.boneSpheres[joint];
    const rig = this.rig;
    if (sphere && rig) {
      // Animación: girar articulaciones con hijos; mover solo la raíz
      const can =
        (this.activeTool === "rotate" && this.jointBones(joint).length > 0) ||
        (this.activeTool === "move" && rig.parents[joint] === null);
      if (!can) {
        controls.detach();
        return;
      }
      const pivot = this.posePivot;
      if (pivot.parent !== this.scene) this.scene.add(pivot);
      sphere.getWorldPosition(pivot.position);
      pivot.quaternion.identity();
      pivot.scale.setScalar(1);
      controls.attach(pivot);
      return;
    }
    if (sphere) controls.attach(sphere);
    else controls.detach();
  }

  private pivotMatrix(object: THREE.Object3D): THREE.Matrix4 {
    return new THREE.Matrix4().compose(object.position, object.quaternion, object.scale);
  }

  /** Pivote del modelo: centro de la caja para girar, centro de la base para mover y escalar */
  private placeObjectPivot(): void {
    const geometry = this.currentMesh!.geometry;
    if (!geometry.boundingBox) geometry.computeBoundingBox();
    const box = geometry.boundingBox!;
    const pivot = this.objectPivot;
    box.getCenter(pivot.position);
    if (this.activeTool !== "rotate") pivot.position.y = box.min.y;
    pivot.quaternion.identity();
    pivot.scale.setScalar(1);
    if (pivot.parent !== this.scene) this.scene.add(pivot);
  }

  /** Cambio del pivote desde que empezó el arrastre */
  private objectDelta(): THREE.Matrix4 {
    return this.pivotMatrix(this.objectPivot).multiply(this.objectDragStart!.clone().invert());
  }

  /** Mientras se arrastra el gizmo del modelo: la malla lo sigue */
  private previewObjectGizmo(): void {
    if (!this.objectDragStart) return;
    const pivot = this.objectPivot;
    if (this.transformControls?.getMode() === "scale") {
      // apply_placement solo acepta escala pareja: manda el eje que más cambió
      const s = pivot.scale;
      const factor = [s.x, s.y, s.z].reduce((a, b) => (Math.abs(b - 1) > Math.abs(a - 1) ? b : a), 1);
      pivot.scale.setScalar(Math.max(factor, 0.01));
    }
    this.objectDelta().decompose(this.meshGroup.position, this.meshGroup.quaternion, this.meshGroup.scale);
  }

  private commitObjectGizmo(): void {
    if (!this.objectDragStart) return;
    const delta = this.objectDelta();
    this.objectDragStart = null;
    const identity = new THREE.Matrix4();
    if (delta.elements.every((v, i) => Math.abs(v - identity.elements[i]) < 1e-7)) {
      this.resetObjectPreview();
      return;
    }
    this.keepCamera = true;
    this.callbacks.onObjectTransformed?.(delta.toArray(), this.transformControls?.getMode() ?? "translate");
  }

  private resetObjectPreview(): void {
    this.meshGroup.position.set(0, 0, 0);
    this.meshGroup.quaternion.identity();
    this.meshGroup.scale.setScalar(1);
    this.meshGroup.updateMatrixWorld(true);
  }

  /** Deshace la vista previa del gizmo del modelo (la transformación no se aplicó) */
  clearObjectPreview(): void {
    this.keepCamera = false;
    this.resetObjectPreview();
    this.attachGizmo();
  }

  /** Animación: arranca un giro (o desplazamiento de la raíz) con el gizmo */
  private startPoseGizmo(): void {
    const rig = this.rig;
    const joint = this.selectedBoneIndex;
    if (!rig || !rig.bones[joint]) return;
    this.poseDragStart = {
      joint,
      rotation: this.jointBones(joint)[0]?.quaternion.clone(),
      position: rig.bones[joint].position.clone(),
      pivot: this.posePivot.position.clone(),
    };
  }

  private previewPoseGizmo(): void {
    const start = this.poseDragStart;
    const rig = this.rig;
    if (!start || !rig) return;
    if (this.transformControls?.getMode() === "rotate") {
      // El pivote empieza sin giro: su giro es el delta en mundo
      if (start.rotation) this.rotateJoint(start.joint, start.rotation, this.posePivot.quaternion);
      return;
    }
    const from = rig.root.worldToLocal(start.pivot.clone());
    const to = rig.root.worldToLocal(this.posePivot.position.clone());
    rig.bones[start.joint].position.copy(start.position).add(to.sub(from));
    this.updatePosedSpheres();
  }

  private commitPoseGizmo(): void {
    const start = this.poseDragStart;
    this.poseDragStart = null;
    if (start) this.callbacks.onPoseEdited?.(start.joint);
    this.attachGizmo();
  }

  // ═══════════════════════════════════════════════════════════════════════════
  // MEDIR
  // ═══════════════════════════════════════════════════════════════════════════

  /** Clic con Medir: un punto de la superficie (Ctrl: el vértice más cercano) */
  private onMeasureDown(e: PointerEvent): void {
    this.raycaster.setFromCamera(this.getMousePosition(e), this.camera);
    const targets = this.meshGroup.children.filter(
      (o): o is THREE.Mesh =>
        o instanceof THREE.Mesh && o.visible && o !== this.currentWireframe && o !== this.rig?.wireframe
    );
    const hit = this.raycaster.intersectObjects(targets, false)[0];
    if (!hit) return;
    let point = hit.point.clone();
    if ((e.ctrlKey || e.metaKey) && hit.face) {
      const mesh = hit.object as THREE.Mesh;
      const v = new THREE.Vector3();
      let best = Infinity;
      for (const index of [hit.face.a, hit.face.b, hit.face.c]) {
        mesh.getVertexPosition(index, v).applyMatrix4(mesh.matrixWorld);
        const d = v.distanceToSquared(hit.point);
        if (d < best) {
          best = d;
          point = v.clone();
        }
      }
    }
    if (this.measurePoints.length >= 2) this.measurePoints = [];
    this.measurePoints.push(point);
    this.drawMeasure();
  }

  private clearMeasureGraphics(): void {
    for (const child of [...this.measureGroup.children]) {
      this.measureGroup.remove(child);
      const object = child as THREE.Mesh | THREE.Line;
      object.geometry.dispose();
      (object.material as THREE.Material).dispose();
    }
  }

  private clearMeasure(): void {
    const had = this.measurePoints.length > 0;
    this.measurePoints = [];
    this.clearMeasureGraphics();
    if (had) this.callbacks.onMeasure?.(null);
  }

  /** Puntos y línea por delante de la malla, y el texto con la distancia */
  private drawMeasure(): void {
    this.clearMeasureGraphics();
    const points = this.measurePoints;
    if (points.length === 0) return;
    const color = themeHex("yellow");
    const radius = this.jointRadius() * 0.6;
    for (const p of points) {
      const dot = new THREE.Mesh(
        new THREE.SphereGeometry(radius, 12, 12),
        new THREE.MeshBasicMaterial({ color, depthTest: false })
      );
      dot.position.copy(p);
      dot.renderOrder = 3;
      this.measureGroup.add(dot);
    }
    if (points.length < 2) {
      this.callbacks.onMeasure?.("Medir: clic en el segundo punto");
      return;
    }
    const line = new THREE.Line(
      new THREE.BufferGeometry().setFromPoints(points),
      new THREE.LineBasicMaterial({ color, depthTest: false })
    );
    line.renderOrder = 3;
    this.measureGroup.add(line);

    const { metersPerUnit, unitMeters, unitLabel } = this.gridUnits;
    const toUnit = metersPerUnit / unitMeters;
    const format = (v: number) => `${Number((v * toUnit).toPrecision(4)).toLocaleString()} ${unitLabel}`;
    // Ejes como se muestran: Z es la altura (Y del visor)
    const d = points[1].clone().sub(points[0]);
    this.callbacks.onMeasure?.(
      `Distancia ${format(d.length())} · X ${format(Math.abs(d.x))} · Y ${format(Math.abs(d.z))} · Z ${format(Math.abs(d.y))}`
    );
  }

  /** Pivote del esqueleto entero, en el espacio de las articulaciones */
  private skeletonCenter(): THREE.Vector3 {
    const pivot = this.skeletonData?.pivot;
    if (pivot) return new THREE.Vector3(...pivot);
    return new THREE.Box3().setFromPoints(this.boneSpheres.map((s) => s.position)).getCenter(new THREE.Vector3());
  }

  /** Mientras se arrastra el gizmo del esqueleto: las articulaciones lo siguen */
  private previewSkeletonGizmo(): void {
    const start = this.pivotDragStart;
    if (!start) return;
    const pivot = this.skeletonPivot;
    const s = pivot.scale;
    // Un eje del gizmo escala solo ese eje: el esqueleto se escala parejo
    const factor = [s.x, s.y, s.z].reduce((a, b) => (Math.abs(b - 1) > Math.abs(a - 1) ? b : a), 1);
    const scale = Math.max(factor, 0.01);
    const offset = new THREE.Vector3();
    this.boneSpheres.forEach((sphere, i) => {
      offset.copy(start[i]).sub(pivot.position);
      if (this.transformControls?.getMode() === "scale") offset.multiplyScalar(scale);
      else offset.applyQuaternion(pivot.quaternion);
      sphere.position.copy(pivot.position).add(offset);
    });
    this.updateBoneLines();
  }

  private commitSkeletonGizmo(): void {
    this.pivotDragStart = null;
    const pivot = this.skeletonPivot;
    if (this.transformControls?.getMode() === "scale") {
      const s = pivot.scale;
      const factor = [s.x, s.y, s.z].reduce((a, b) => (Math.abs(b - 1) > Math.abs(a - 1) ? b : a), 1);
      if (Math.abs(factor - 1) > 1e-4) this.callbacks.onSkeletonTransformed?.({ scale: Math.max(factor, 0.01) });
    } else {
      const q = pivot.quaternion;
      if (Math.abs(q.w) < 1 - 1e-8) this.callbacks.onSkeletonTransformed?.({ rotation: [q.x, q.y, q.z, q.w] });
    }
    // Hasta que responda el backend quedan las posiciones de la vista previa
    pivot.quaternion.identity();
    pivot.scale.setScalar(1);
  }

  setGridVisible(visible: boolean): void {
    this.grid.visible = visible;
  }

  /** Unidades de la grilla: en qué está el modelo y en qué se muestra */
  /** Relee los colores del tema (cambió claro/oscuro) */
  applyTheme(): void {
    (this.scene.background as THREE.Color).setHex(themeHex("viewport"));
    this.rebuildGrid();
    this.viewCube.applyTheme();
    this.drawMeasure();
  }

  setGridUnits(units: GridUnits): void {
    this.gridUnits = units;
    this.rebuildGrid();
    this.drawMeasure();
  }

  /**
   * Rehace la grilla a la medida del modelo: celdas de una potencia de 10 de
   * la unidad elegida (10 a 100 celdas a lo ancho del modelo), líneas mayores
   * cada 10 celdas y ejes X (rojo) y Z (celeste) por el origen.
   */
  private rebuildGrid(): void {
    for (const child of [...this.grid.children]) {
      this.grid.remove(child);
      const line = child as THREE.LineSegments;
      line.geometry.dispose();
      (line.material as THREE.Material).dispose();
    }
    const { metersPerUnit, unitMeters, unitLabel } = this.gridUnits;
    // Escena → unidad elegida
    const toUnit = metersPerUnit / unitMeters;
    let extent = 5 / toUnit;
    if (this.currentMesh) {
      const size = new THREE.Box3().setFromObject(this.currentMesh).getSize(new THREE.Vector3());
      extent = Math.max(size.x, size.y, size.z) || extent;
    }
    const extentUnits = extent * toUnit;
    const step = Math.pow(10, Math.floor(Math.log10(extentUnits)) - 1);
    const major = step * 10;
    const half = Math.max(Math.ceil((extentUnits * 1.5) / major), 1) * major;
    const toScene = 1 / toUnit;

    const lines = (every: number, color: number, opacity: number, skipEvery?: number) => {
      const points: number[] = [];
      const n = Math.round(half / every);
      for (let i = -n; i <= n; i++) {
        if (i === 0 || (skipEvery && i % skipEvery === 0)) continue;
        const c = i * every * toScene;
        const h = half * toScene;
        points.push(-h, 0, c, h, 0, c, c, 0, -h, c, 0, h);
      }
      const geometry = new THREE.BufferGeometry();
      geometry.setAttribute("position", new THREE.Float32BufferAttribute(points, 3));
      return new THREE.LineSegments(
        geometry,
        new THREE.LineBasicMaterial({ color, transparent: true, opacity, depthWrite: false })
      );
    };
    const axis = (from: THREE.Vector3, to: THREE.Vector3, color: number) =>
      new THREE.LineSegments(
        new THREE.BufferGeometry().setFromPoints([from, to]),
        new THREE.LineBasicMaterial({ color, transparent: true, opacity: 0.6, depthWrite: false })
      );
    const h = half * toScene;
    this.grid.add(
      lines(step, themeHex("grid-minor"), 0.35, 10),
      lines(major, themeHex("grid-major"), 0.45),
      axis(new THREE.Vector3(-h, 0, 0), new THREE.Vector3(h, 0, 0), themeHex("axis-x")),
      // Z del visor = Y de la vista (Z arriba, como Blender): verde
      axis(new THREE.Vector3(0, 0, -h), new THREE.Vector3(0, 0, h), themeHex("axis-y"))
    );

    const format = (v: number) => `${Number(v.toPrecision(6)).toLocaleString()} ${unitLabel}`;
    this.callbacks.onGridChanged?.(`Grilla: ${format(step)} · líneas mayores ${format(major)}`);
  }

  setActiveTool(tool: string): void {
    if (tool !== "rotate") this.resetPose();
    if (tool !== "measure") this.clearMeasure();
    this.activeTool = tool;
    const cursors: Record<string, string> = { move: "move", rotate: "crosshair", scale: "ns-resize", measure: "crosshair" };
    this.canvas.style.cursor = cursors[tool] ?? "default";
    this.transformControls?.setMode(this.gizmoMode());
    this.attachGizmo();
  }

  /** Vistas con nombre (atajos 1, 3, 7 y Ctrl): el mismo giro que el cubo */
  setView(name: string): void {
    const directions: Record<string, Vec3> = {
      front: [0, 0, 1],
      back: [0, 0, -1],
      right: [1, 0, 0],
      left: [-1, 0, 0],
      top: [0, 1, 0],
      bottom: [0, -1, 0],
    };
    if (directions[name]) this.lookFrom(directions[name]);
  }

  focusSelection(): void {
    if (this.selectedBoneIndex >= 0 && this.selectedBoneIndex < this.boneSpheres.length) {
      const sphere = this.boneSpheres[this.selectedBoneIndex];
      const center = sphere.getWorldPosition(new THREE.Vector3());
      // La cámara se traslada con el centro: mantiene dirección y distancia
      this.camera.position.add(center.clone().sub(this.controls.target));
      this.controls.target.copy(center);
      this.controls.update();
    } else {
      this.fitCamera();
    }
  }

  dispose(): void {
    if (this.influenceTimer !== null) clearTimeout(this.influenceTimer);
    if (this.animationId !== null) {
      cancelAnimationFrame(this.animationId);
    }
    if (this.fpsTimer !== null) {
      clearInterval(this.fpsTimer);
    }
    if (this.transformControls) {
      this.scene.remove(this.transformControls.getHelper());
      this.transformControls.dispose();
    }
    this.clearMesh();
    this.clearSkeleton();
    this.viewCube.dispose();
    this.renderer.dispose();
  }

  // ═══════════════════════════════════════════════════════════════════════════
  // PRIVATE METHODS
  // ═══════════════════════════════════════════════════════════════════════════

  private applySettings(): void {
    // Wireframe (triangle or quad)
    if (this.currentWireframe) {
      this.currentWireframe.visible = this.settings.showWireframe;
    }
    if (this.quadWireframe) {
      this.quadWireframe.visible = this.settings.showWireframe;
    }

    // Skeleton
    this.skeletonGroup.visible = this.settings.showSkeleton;

    // Pesos (calculados, o la influencia estimada si solo hay esqueleto)
    const weights = this.settings.showWeights ? this.displayWeights() : null;
    if (weights) this.updateWeightsVisualization(weights);
    if (this.weightsMesh) this.weightsMesh.visible = weights !== null;
    if (this.currentMesh) this.currentMesh.visible = this.settings.showMesh && weights === null;

    this.applyXray();
    this.applyBoneDisplay();
    this.syncRigVisibility();
  }

  /** Pesos de la malla actual, o la influencia estimada del esqueleto */
  private displayWeights(): Influence | null {
    const mesh = this.meshData;
    if (!mesh) return null;
    const weights = this.weightsData;
    if (weights && weights.numVertices * 3 === mesh.positions.length) return weights;
    const skeleton = this.skeletonData;
    if (!skeleton) return null;
    this.estimatedInfluence ??= estimateInfluence(mesh.positions, skeleton.bones);
    return this.estimatedInfluence;
  }

  /**
   * Se movieron articulaciones: la estimación se rehace cuando el esqueleto
   * se queda quieto (con mallas grandes tarda); mientras, sigue la anterior
   */
  private refreshEstimatedWeights(): void {
    if (this.estimatedInfluence === null) return;
    if (this.influenceTimer !== null) clearTimeout(this.influenceTimer);
    this.influenceTimer = window.setTimeout(() => {
      this.influenceTimer = null;
      this.estimatedInfluence = null;
      if (this.settings.showWeights) this.applySettings();
    }, 250);
  }

  private updateWeightsVisualization(weights: Influence): void {
    if (!this.meshData) return;

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

    // Colores por vértice: un hueso (mapa de calor) o todos (color de cada hueso)
    const colors = new Float32Array(weights.numVertices * 3);
    const color = new THREE.Color();
    for (let v = 0; v < weights.numVertices; v++) {
      weightColor(weights, v, this.settings.selectedBone, color);
      colors[v * 3] = color.r;
      colors[v * 3 + 1] = color.g;
      colors[v * 3 + 2] = color.b;
    }

    geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));

    // Con luz, para que se lea la forma bajo los colores
    const material = new THREE.MeshLambertMaterial({
      vertexColors: true,
      side: THREE.DoubleSide,
    });

    this.weightsMesh = new THREE.Mesh(geometry, material);
    this.weightsMesh.visible = this.settings.showWeights;
    this.meshGroup.add(this.weightsMesh);
  }

  // ═══════════════════════════════════════════════════════════════════════════
  // MODO ANIMACIÓN
  // ═══════════════════════════════════════════════════════════════════════════

  /** Entra o sale del modo animación; devuelve si quedó activo (hacen falta pesos) */
  setAnimationMode(enabled: boolean): boolean {
    if (enabled === this.animationMode) return this.rig !== null;
    this.animationMode = enabled;
    this.resetPose();
    this.buildRig();
    this.attachGizmo();
    return this.rig !== null;
  }

  /** Hay un esqueleto listo para animar */
  isAnimating(): boolean {
    return this.rig !== null;
  }

  /**
   * Arma (o rehace, si cambió la malla, los pesos o el esqueleto) el rig con
   * skin. Sin modelo se anima el esqueleto solo; con modelo hacen falta sus pesos.
   */
  private buildRig(): void {
    this.disposeRig();
    const mesh = this.currentMesh;
    const data = this.meshData;
    const weights = this.weightsData;
    const skeleton = this.skeletonData;
    if (!this.animationMode || !skeleton || skeleton.bones.length === 0) return;
    const numBones = skeleton.bones.length;
    const skin = mesh && data && weights ? { mesh, data, weights } : null;
    if (mesh && !skin) return;
    if (skin && (skin.weights.numVertices !== skin.data.positions.length / 3 || skin.weights.numBones !== numBones)) return;

    const parents = skeleton.bones.map((b) => b.parent);
    const children = parents.map((_, j) => parents.flatMap((p, b) => (p === j ? [b] : [])));
    const position = (b: number) => new THREE.Vector3(...skeleton.bones[b].position);
    const head = (b: number) => position(parents[b] ?? b);
    const rest = parents.map((p, b) => (p === null ? head(b) : head(b).sub(head(p))));
    const tip = parents.map((_, b) => position(b).sub(head(b)));

    const root = new THREE.Group();
    root.name = "rig";
    const bones = rest.map((r, b) => {
      const bone = new THREE.Bone();
      bone.name = skeleton.bones[b].name;
      bone.position.copy(r);
      return bone;
    });
    bones.forEach((bone, b) => (parents[b] === null ? root : bones[parents[b]!]).add(bone));
    this.meshGroup.add(root);
    root.updateMatrixWorld(true);
    const threeSkeleton = new THREE.Skeleton(bones);

    const skinned = skin ? this.skinRig(skin, threeSkeleton) : null;
    this.rig = {
      root,
      bones,
      parents,
      children,
      rest,
      tip,
      skeleton: threeSkeleton,
      mesh: skinned?.mesh ?? null,
      wireframe: skinned?.wireframe ?? null,
    };
    this.syncRigVisibility();
    if (this.lastPose) this.setPose(this.lastPose);
    else this.updatePosedSpheres();
  }

  /** Mallas con skin (sólida y alambre) que comparten los atributos de la malla de reposo */
  private skinRig(
    skin: { mesh: THREE.Mesh; data: MeshData; weights: WeightsData },
    threeSkeleton: THREE.Skeleton
  ): { mesh: THREE.SkinnedMesh; wireframe: THREE.SkinnedMesh } {
    const { mesh, data, weights } = skin;
    const numBones = threeSkeleton.bones.length;
    const numVertices = data.positions.length / 3;
    // Hasta 4 influencias por vértice (como el GLB exportado), renormalizadas
    const k = weights.maxInfluences;
    const skinIndex = new Uint16Array(numVertices * 4);
    const skinWeight = new Float32Array(numVertices * 4);
    const pairs: [number, number][] = [];
    for (let v = 0; v < numVertices; v++) {
      pairs.length = 0;
      for (let i = 0; i < k; i++) {
        const w = weights.weights[(v * k + i) * 2 + 1];
        if (w > 0) pairs.push([weights.weights[(v * k + i) * 2], w]);
      }
      pairs.sort((a, b) => b[1] - a[1]);
      const top = pairs.slice(0, 4);
      const sum = top.reduce((acc, [, w]) => acc + w, 0);
      top.forEach(([bone, w], slot) => {
        skinIndex[v * 4 + slot] = Math.min(bone, numBones - 1);
        skinWeight[v * 4 + slot] = w / sum;
      });
      // Sin pesos: sigue a la raíz
      if (top.length === 0) skinWeight[v * 4] = 1;
    }

    // Comparte los atributos de la malla de reposo; solo el skin es nuevo
    const geometry = new THREE.BufferGeometry();
    for (const name of ["position", "normal", "uv"]) {
      const attribute = mesh.geometry.getAttribute(name);
      if (attribute) geometry.setAttribute(name, attribute);
    }
    geometry.setIndex(mesh.geometry.index);
    for (const group of mesh.geometry.groups) geometry.addGroup(group.start, group.count, group.materialIndex);
    geometry.setAttribute("skinIndex", new THREE.Uint16BufferAttribute(skinIndex, 4));
    geometry.setAttribute("skinWeight", new THREE.Float32BufferAttribute(skinWeight, 4));

    const skinned = (material: THREE.Material | THREE.Material[]) => {
      const m = new THREE.SkinnedMesh(geometry, material);
      // El volumen de reposo no sirve para descartar la malla movida
      m.frustumCulled = false;
      m.bind(threeSkeleton);
      this.meshGroup.add(m);
      return m;
    };
    return { mesh: skinned(mesh.material), wireframe: skinned(wireframeMaterial()) };
  }

  private disposeRig(): void {
    const rig = this.rig;
    if (!rig) return;
    this.rig = null;
    this.meshGroup.remove(rig.root);
    if (rig.mesh && rig.wireframe) {
      this.meshGroup.remove(rig.mesh, rig.wireframe);
      // Solo los atributos de skin son propios; los compartidos se vuelven a subir si hace falta
      rig.mesh.geometry.deleteAttribute("skinIndex");
      rig.mesh.geometry.deleteAttribute("skinWeight");
      (rig.wireframe.material as THREE.Material).dispose();
    }
    rig.skeleton.dispose();
    // Esferas en reposo y mallas de reposo visibles otra vez
    this.skeletonData?.bones.forEach((bone, i) => this.boneSpheres[i]?.position.set(...bone.position));
    this.updateBoneLines();
    this.applySettings();
  }

  private syncRigVisibility(): void {
    const rig = this.rig;
    if (!rig?.mesh || !rig.wireframe) return;
    for (const o of [this.currentMesh, this.currentWireframe, this.quadWireframe, this.weightsMesh]) {
      if (o) o.visible = false;
    }
    rig.mesh.visible = this.settings.showMesh;
    rig.wireframe.visible = this.settings.showWireframe;
  }

  /** Huesos que gira la articulación: los de sus hijos (la raíz, el suyo) */
  private jointBones(joint: number): THREE.Bone[] {
    const rig = this.rig;
    if (!rig || !rig.bones[joint]) return [];
    return rig.parents[joint] === null ? [rig.bones[joint]] : rig.children[joint].map((c) => rig.bones[c]);
  }

  /** Articulaciones animables (la raíz y las que tienen algo que girar), en orden */
  animatableJoints(): number[] {
    const rig = this.rig;
    if (!rig) return [];
    return rig.parents.flatMap((p, j) => (p === null || rig.children[j].length > 0 ? [j] : []));
  }

  /**
   * Gira la articulación `delta` (en mundo) a partir del giro local `start`:
   * el giro local vive en el marco del hueso de la articulación (el padre de
   * los que gira), o del rig en la raíz.
   */
  private rotateJoint(joint: number, start: THREE.Quaternion, delta: THREE.Quaternion): void {
    const rig = this.rig!;
    const frameObject = rig.parents[joint] === null ? rig.root : rig.bones[joint];
    const frame = frameObject.getWorldQuaternion(new THREE.Quaternion());
    const local = frame.clone().invert().multiply(delta).multiply(frame).multiply(start).normalize();
    for (const bone of this.jointBones(joint)) bone.quaternion.copy(local);
    this.updatePosedSpheres();
  }

  /** Pone el esqueleto en `pose`; las articulaciones que no figuran quedan en reposo */
  setPose(pose: Pose): void {
    const rig = this.rig;
    this.lastPose = pose;
    // Durante un giro o desplazamiento manda el mouse
    if (!rig || this.modal || this.poseDragStart) return;
    rig.bones.forEach((bone, b) => {
      bone.quaternion.identity();
      bone.position.copy(rig.rest[b]);
    });
    for (const [joint, q] of pose.rotations) {
      for (const bone of this.jointBones(joint)) bone.quaternion.set(...q);
    }
    for (const [joint, t] of pose.translations) {
      if (rig.parents[joint] === null && rig.bones[joint]) {
        rig.bones[joint].position.copy(rig.rest[joint]).add(new THREE.Vector3(...t));
      }
    }
    this.updatePosedSpheres();
  }

  /** Giro local de la articulación y, si es raíz, su desplazamiento desde el reposo */
  getJointPose(joint: number): { rotation: Quat; translation?: Vec3 } | null {
    const rig = this.rig;
    const bones = this.jointBones(joint);
    if (!rig || bones.length === 0) return null;
    const q = bones[0].quaternion;
    const pose: { rotation: Quat; translation?: Vec3 } = { rotation: [q.x, q.y, q.z, q.w] };
    if (rig.parents[joint] === null) {
      const t = rig.bones[joint].position.clone().sub(rig.rest[joint]);
      pose.translation = [t.x, t.y, t.z];
    }
    return pose;
  }

  /** Las articulaciones siguen a sus huesos */
  private updatePosedSpheres(): void {
    const rig = this.rig;
    if (!rig) return;
    rig.root.updateMatrixWorld(true);
    const p = new THREE.Vector3();
    rig.bones.forEach((bone, b) => {
      const sphere = this.boneSpheres[b];
      if (!sphere?.parent) return;
      p.copy(rig.tip[b]).applyMatrix4(bone.matrixWorld);
      sphere.position.copy(sphere.parent.worldToLocal(p));
    });
    this.updateBoneLines();
  }

  /** Quita el modelo (proyecto de solo esqueleto): el esqueleto se anima solo */
  unloadMesh(): void {
    this.disposeRig();
    this.meshData = null;
    this.estimatedInfluence = null;
    this.clearMesh();
    this.buildRig();
  }

  private clearMesh(): void {
    if (this.currentMesh) {
      this.meshGroup.remove(this.currentMesh);
      this.currentMesh.geometry.dispose();
      this.disposeMaterial(this.currentMesh.material);
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
      child.traverse((o) => {
        if (o instanceof THREE.Mesh || o instanceof THREE.LineSegments) {
          o.geometry.dispose();
          if (Array.isArray(o.material)) {
            o.material.forEach((m) => m.dispose());
          } else {
            o.material.dispose();
          }
        }
      });
    }
    this.boneShapes = [];
  }

  private fitCamera(): void {
    const box = this.currentMesh ? new THREE.Box3().setFromObject(this.currentMesh) : this.skeletonBox();
    if (!box) return;
    this.viewTransition = null;

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
    // Las luces se ubican alrededor del modelo
    this.placeLights();
  }

  // ORIENTACIÓN (piso, frente, origen)
  // ═══════════════════════════════════════════════════════════════════════════

  private getMousePosition(event: MouseEvent): THREE.Vector2 {
    const rect = this.canvas.getBoundingClientRect();
    return new THREE.Vector2(
      ((event.clientX - rect.left) / rect.width) * 2 - 1,
      -((event.clientY - rect.top) / rect.height) * 2 + 1
    );
  }

  /** Clic al orientar: un plano candidato, o la superficie (Shift en modo piso) */
  private onCanvasClick(event: MouseEvent): void {
    if (!this.placementMode || event.button !== 0 || event.altKey) return;
    this.mouse = this.getMousePosition(event);
    this.raycaster.setFromCamera(this.mouse, this.camera);

    if (this.placementMode === "floor" && !event.shiftKey) {
      const hit = this.raycaster.intersectObjects(this.placementPlanes)[0];
      if (hit) this.callbacks.onPlacementPick?.({ kind: "candidate", index: this.placementPlanes.indexOf(hit.object as THREE.Mesh) });
      return;
    }
    const visible = this.meshGroup.children.filter((o): o is THREE.Mesh => o instanceof THREE.Mesh && o.visible);
    const hit = this.raycaster.intersectObjects(visible)[0];
    if (!hit?.face) return;
    const normal = hit.face.normal.clone().transformDirection(hit.object.matrixWorld);
    this.callbacks.onPlacementPick?.({
      kind: "surface",
      point: [hit.point.x, hit.point.y, hit.point.z],
      normal: [normal.x, normal.y, normal.z],
    });
  }

  /** Resalta el plano candidato bajo el cursor */
  private onCanvasMouseMove(event: MouseEvent): void {
    if (this.placementMode !== "floor" || this.placementPlanes.length === 0) return;
    this.mouse = this.getMousePosition(event);
    this.raycaster.setFromCamera(this.mouse, this.camera);
    const hit = event.shiftKey ? undefined : this.raycaster.intersectObjects(this.placementPlanes)[0];
    const index = hit ? this.placementPlanes.indexOf(hit.object as THREE.Mesh) : -1;
    if (index === this.hoveredPlane) return;
    this.placementPlanes.forEach((plane, i) => {
      (plane.material as THREE.MeshBasicMaterial).opacity = this.planeOpacity(plane.userData.stable, i === index);
    });
    this.hoveredPlane = index;
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
