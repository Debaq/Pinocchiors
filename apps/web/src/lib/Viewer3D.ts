/**
 * Pinocchio 3D Viewer - Three.js visualization
 * Rewritten for SolidJS integration
 */

import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { TransformControls } from "three/addons/controls/TransformControls.js";

// ═══════════════════════════════════════════════════════════════════════════
// TYPES
// ═══════════════════════════════════════════════════════════════════════════

export interface MeshData {
  positions: number[];
  normals: number[];
  indices: number[];
  /** Optional quad indices for quad wireframe visualization (4 indices per quad) */
  quadIndices?: number[];
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
  weights: number[];
  maxInfluences: number;
}

export interface ViewerSettings {
  showMesh: boolean;
  showWireframe: boolean;
  showSkeleton: boolean;
  showWeights: boolean;
  selectedBone: number;
}

export interface ViewerCallbacks {
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
  private currentWireframe: THREE.LineSegments | null = null;
  private quadWireframe: THREE.LineSegments | null = null;
  private weightsMesh: THREE.Mesh | null = null;

  // Data
  private meshData: MeshData | null = null;
  private weightsData: WeightsData | null = null;

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
    this.meshData = data;
    this.clearMesh();

    // Create geometry
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute("position", new THREE.Float32BufferAttribute(data.positions, 3));
    geometry.setAttribute("normal", new THREE.Float32BufferAttribute(data.normals, 3));
    geometry.setIndex(new THREE.Uint32BufferAttribute(data.indices, 1));

    // Material - Dracula style
    const material = new THREE.MeshStandardMaterial({
      color: 0x6272a4,
      metalness: 0.1,
      roughness: 0.7,
      side: THREE.DoubleSide,
    });

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
      // Fallback to triangle wireframe
      const wireframeMaterial = new THREE.LineBasicMaterial({
        color: 0x44475a,
        opacity: 0.5,
        transparent: true,
      });
      const wireframeGeometry = new THREE.WireframeGeometry(geometry);
      this.currentWireframe = new THREE.LineSegments(wireframeGeometry, wireframeMaterial);
      this.currentWireframe.visible = this.settings.showWireframe;
      this.meshGroup.add(this.currentWireframe);
    }

    // Fit camera to mesh
    this.fitCamera();
  }

  loadSkeleton(data: SkeletonData): void {
    this.clearSkeleton();
    this.boneSpheres = [];

    const bonesGroup = new THREE.Group();
    bonesGroup.name = "bones";
    this.skeletonGroup.add(bonesGroup);

    // Joint spheres
    const jointGeometry = new THREE.SphereGeometry(0.02, 16, 16);
    const jointMaterial = new THREE.MeshBasicMaterial({ color: 0xffb86c }); // Dracula orange
    const leafMaterial = new THREE.MeshBasicMaterial({ color: 0x50fa7b }); // Dracula green

    for (let i = 0; i < data.bones.length; i++) {
      const bone = data.bones[i];
      const material = bone.isLeaf ? leafMaterial.clone() : jointMaterial.clone();
      const sphere = new THREE.Mesh(jointGeometry, material);
      sphere.position.set(bone.position[0], bone.position[1], bone.position[2]);
      sphere.userData.boneName = bone.name;
      sphere.userData.boneIndex = i;
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
  }

  loadWeights(data: WeightsData): void {
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
          if (this.selectedBoneIndex >= 0 && this.transformControls?.object) {
            const pos = this.transformControls.object.position;
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
    geometry.setAttribute("position", new THREE.Float32BufferAttribute(this.meshData.positions, 3));
    geometry.setAttribute("normal", new THREE.Float32BufferAttribute(this.meshData.normals, 3));
    geometry.setIndex(new THREE.Uint32BufferAttribute(this.meshData.indices, 1));

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
      (this.currentMesh.material as THREE.Material).dispose();
      this.currentMesh = null;
    }

    if (this.currentWireframe) {
      this.meshGroup.remove(this.currentWireframe);
      this.currentWireframe.geometry.dispose();
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
  private createQuadWireframeGeometry(positions: number[], quadIndices: number[]): THREE.BufferGeometry {
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
