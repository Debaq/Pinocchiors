import type { JSX } from "solid-js";
import type { SkeletonData } from "./Viewer3D";
import type { SceneStructure } from "../components/steps/StructureStep";

export interface SceneNode {
  id: string;
  type:
    | "scene" | "mesh" | "wireframe" | "skeleton" | "bone" | "quadmesh" | "grid" | "weights" | "node"
    // Grupos del Outliner y lo que viene del CAD
    | "group" | "plane" | "axis" | "point" | "sketch" | "feature" | "folder" | "part";
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
  /** Atenuado (operación suprimida o después de la barra de retroceso) */
  muted?: boolean;
  /** Con error al calcular */
  error?: boolean;
  /** Muestra de color (piezas del CAD) */
  color?: string;
  /** Contenido propio en vez de hijos (el árbol de operaciones del diseño) */
  content?: JSX.Element;
  /** Cuántos elementos tiene (si no, los hijos) */
  count?: number;
}

/** Elemento del CAD ya armado por App (ids `plane-xy`, `cad-<id>`, `part-<op>-<n>`…) */
export type CadOutlineItem = Omit<SceneNode, "expanded" | "children"> & { children?: SceneNode[] };

/** Lo que el diseño paramétrico agrega al Outliner */
export interface CadOutline {
  /** Planos base y de referencia (también ejes y puntos), en orden del árbol */
  planes: CadOutlineItem[];
  /** Árbol de operaciones (el de Diseñar: arrastrar, carpetas, barra de retroceso) */
  featureTree: JSX.Element;
  /** Cuántas operaciones hay */
  featureCount: number;
}

/** Un objeto de la escena (id `obj-<n>`); del activo cuelga su modelo */
export type ObjectRow = CadOutlineItem & {
  active: boolean;
  /** Lo hecho sobre su malla (orientar, reparar, escalar…), en orden */
  modifications?: SceneNode[];
  /** Nombre de la malla del activo ("Malla generada" en las piezas del diseño) */
  meshLabel?: string;
};

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
  /** Nodos del archivo ocultos en el visor */
  hiddenNodes?: Set<number>;
  /** Nodo del archivo elegido */
  selectedNode?: number;
  /** Planos y operaciones del diseño paramétrico */
  cad?: CadOutline;
  /** Objetos en orden de creación (sin objetos, el modelo va suelto) */
  objects?: ObjectRow[];
}

/** Hay geometría en el nodo o debajo de él (se puede ocultar o borrar) */
function hasGeometry(structure: SceneStructure, index: number, seen = new Set<number>()): boolean {
  if (seen.has(index)) return false;
  seen.add(index);
  const node = structure.nodes[index];
  return !!node && (node.mesh !== null || node.children.some((c) => hasGeometry(structure, c, seen)));
}

/** Nodos del archivo como hijos de la malla: se ocultan, se eligen y se borran */
function fileNodes(structure: SceneStructure, hidden: Set<number>, selected?: number): SceneNode[] {
  const build = (index: number): SceneNode => {
    const node = structure.nodes[index];
    const geometry = hasGeometry(structure, index);
    const parts = [
      node.mesh !== null ? `malla ${structure.meshes[node.mesh]?.name || node.mesh}` : undefined,
      node.skin !== null ? `skin ${structure.skeletons[node.skin]?.name || node.skin}` : undefined,
    ].filter(Boolean);
    return {
      id: `node-${index}`,
      type: node.mesh !== null ? "mesh" : node.skin !== null ? "skeleton" : "node",
      label: node.name || `nodo ${index}`,
      visible: !hidden.has(index),
      expanded: false,
      selected: selected === index,
      readonly: !geometry,
      deletable: geometry,
      hint: parts.length > 0 ? parts.join(" · ") : "nodo vacío",
      children: node.children.map(build),
    };
  };
  return structure.roots.map(build);
}

const leaf = (item: CadOutlineItem): SceneNode => ({ expanded: false, ...item, children: item.children ?? [] });

function group(id: string, label: string, children: SceneNode[]): SceneNode {
  return { id, type: "group", label, visible: true, expanded: true, selected: false, readonly: true, children };
}

/**
 * Árbol del Outliner en tres grupos: Planos (grilla, planos base y de
 * referencia), Operaciones (el historial del diseño) y Objetos (modelos
 * importados y piezas del diseño, por orden de creación; del activo cuelgan
 * su malla, retopología, esqueleto y pesos). Los grupos vacíos no se muestran.
 */
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

  // ── Planos ──
  const planes: SceneNode[] = [
    {
      id: "grid",
      type: "grid",
      label: "Grid",
      visible: state.showGrid,
      expanded: false,
      selected: false,
      children: [],
    },
    ...(state.cad?.planes ?? []).map(leaf),
  ];

  // ── Modelo del objeto activo ──
  const model: SceneNode[] = [];
  if (state.hasMesh) {
    model.push({
      id: "mesh",
      type: "mesh",
      label: "Malla",
      visible: state.showMesh,
      expanded: false,
      selected: false,
      children: state.structure ? fileNodes(state.structure, state.hiddenNodes ?? new Set(), state.selectedNode) : [],
    });
  }
  if (state.hasWireframe) {
    model.push({
      id: "wireframe",
      type: "wireframe",
      label: "Wireframe",
      visible: state.showWireframe,
      expanded: false,
      selected: false,
      children: [],
    });
  }
  if (state.hasQuadMesh) {
    model.push({
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
  if (state.hasSkeleton && state.skeletonData) {
    model.push({
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
    });
  }
  if (state.hasWeights) {
    model.push({
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
  // ── Objetos ──
  const objects: SceneNode[] = state.objects?.length
    ? state.objects.map(({ active, modifications = [], meshLabel, ...row }) => {
        // Del activo: su malla, lo hecho sobre ella y lo que salió después (quads, esqueleto, pesos)
        const [first, ...rest] = model;
        const mesh = first?.id === "mesh" ? [{ ...first, label: meshLabel ?? first.label }] : [];
        const children = active ? [...mesh, ...modifications, ...(mesh.length ? rest : model)] : modifications;
        return { ...leaf(row), expanded: active, children };
      })
    : model;

  root.children.push(group("group-planes", "Planos", planes));
  if (state.cad) root.children.push({ ...group("group-features", "Operaciones", []), content: state.cad.featureTree, count: state.cad.featureCount });
  if (objects.length) root.children.push(group("group-objects", "Objetos", objects));
  return root;
}
