import type { SkeletonData } from "./Viewer3D";

export interface SceneNode {
  id: string;
  type: "scene" | "mesh" | "wireframe" | "skeleton" | "bone" | "quadmesh" | "grid" | "weights";
  label: string;
  visible: boolean;
  expanded: boolean;
  selected: boolean;
  children: SceneNode[];
  boneIndex?: number;
  /** Se puede borrar desde el Outliner */
  deletable?: boolean;
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
      children: [],
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
