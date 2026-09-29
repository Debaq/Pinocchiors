import type { SkeletonData } from "./Viewer3D";
import type { SceneStructure } from "../components/steps/StructureStep";

export interface SceneNode {
  id: string;
  type: "scene" | "mesh" | "wireframe" | "skeleton" | "bone" | "quadmesh" | "grid" | "weights" | "node";
  label: string;
  visible: boolean;
  expanded: boolean;
  selected: boolean;
  children: SceneNode[];
  boneIndex?: number;
  /** Se puede borrar desde el Outliner */
  deletable?: boolean;
  /** Sin botón de visibilidad (nodos del archivo: la malla se une al importar) */
  readonly?: boolean;
  /** Texto al pasar el mouse */
  hint?: string;
}

export interface SceneTreeState {
  hasMesh: boolean;
  hasWireframe: boolean;
  hasSkeleton: boolean;
  hasWeights: boolean;
  hasQuadMesh: boolean;
  showMesh: boolean;
  showWireframe: boolean;
  showSkeleton: boolean;
  showWeights: boolean;
  showGrid: boolean;
  showQuadMesh: boolean;
  skeletonData?: SkeletonData;
  selectedBone: number;
  /** Jerarquía de nodos del archivo importado */
  structure?: SceneStructure;
}

/** Nodos del archivo como hijos de la malla (solo lectura) */
function fileNodes(structure: SceneStructure): SceneNode[] {
  const build = (index: number): SceneNode => {
    const node = structure.nodes[index];
    const parts = [
      node.mesh !== null ? `malla ${structure.meshes[node.mesh]?.name || node.mesh}` : undefined,
      node.skin !== null ? `skin ${structure.skeletons[node.skin]?.name || node.skin}` : undefined,
    ].filter(Boolean);
    return {
      id: `node-${index}`,
      type: node.mesh !== null ? "mesh" : node.skin !== null ? "skeleton" : "node",
      label: node.name || `nodo ${index}`,
      visible: true,
      expanded: false,
      selected: false,
      readonly: true,
      hint: parts.length > 0 ? parts.join(" · ") : "nodo vacío",
      children: node.children.map(build),
    };
  };
  return structure.roots.map(build);
}

export function buildSceneTree(state: SceneTreeState): SceneNode {
  const root: SceneNode = {
    id: "scene",
    type: "scene",
    label: "Escena",
    visible: true,
    expanded: true,
    selected: false,
    children: [],
  };

  // Grid
  root.children.push({
    id: "grid",
    type: "grid",
    label: "Grid",
    visible: state.showGrid,
    expanded: false,
    selected: false,
    children: [],
  });

  // Mesh
  if (state.hasMesh) {
    root.children.push({
      id: "mesh",
      type: "mesh",
      label: "Malla",
      visible: state.showMesh,
      expanded: false,
      selected: false,
      children: state.structure ? fileNodes(state.structure) : [],
    });
  }

  // Wireframe
  if (state.hasWireframe) {
    root.children.push({
      id: "wireframe",
      type: "wireframe",
      label: "Wireframe",
      visible: state.showWireframe,
      expanded: false,
      selected: false,
      children: [],
    });
  }

  // Quad mesh
  if (state.hasQuadMesh) {
    root.children.push({
      id: "quadmesh",
      type: "quadmesh",
      label: "Quad Mesh",
      visible: state.showQuadMesh,
      deletable: true,
      expanded: false,
      selected: false,
      children: [],
    });
  }

  // Skeleton + bones
  if (state.hasSkeleton && state.skeletonData) {
    const skeletonNode: SceneNode = {
      id: "skeleton",
      type: "skeleton",
      label: "Esqueleto",
      visible: state.showSkeleton,
      deletable: true,
      expanded: false,
      selected: false,
      children: state.skeletonData.bones.map((bone, i) => ({
        id: `bone-${i}`,
        type: "bone" as const,
        label: bone.name,
        visible: state.showSkeleton,
        expanded: false,
        selected: state.selectedBone === i,
        children: [],
        boneIndex: i,
      })),
    };
    root.children.push(skeletonNode);
  }

  // Weights
  if (state.hasWeights) {
    root.children.push({
      id: "weights",
      type: "weights",
      label: "Heatmap Pesos",
      visible: state.showWeights,
      deletable: true,
      expanded: false,
      selected: false,
      children: [],
    });
  }

  return root;
}
