// Diseño CAD paramétrico (backend: apps/desktop/src/cad.rs y libs/cad).
//
// El documento es una lista de operaciones con sus recetas. Se edita acá, se
// manda entero al backend y vuelve el resultado del recálculo. Deshacer y
// rehacer guardan copias del documento: son chicas (sin sólidos adentro).
//
// Coordenadas del documento: mm, Z arriba. El visor recibe la malla del
// sólido ya en Y arriba y en las unidades de la escena.

import { batch, createSignal } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import type { Snap } from "./sketchSnap";
import type { Contour } from "./sketchText";
import { breaks, ellipseArcPolyline, makeBSpline, sample, type BSpline } from "./sketchCurves.ts";

export type P2 = [number, number];
export type P3 = [number, number, number];

// ─── Documento (mismo JSON que cad_model) ─────────────────────────────────

export interface Plane {
  origin: P3;
  normal: P3;
  x_dir: P3;
}

/** Origen de una cara: operación que la creó y qué parte ("fin", "lado:7", "+z"…) */
export interface FaceTag {
  feature: number;
  name: string;
}

export interface FaceRef {
  point: P3;
  normal: P3;
  /** Orígenes de la cara: se busca por ellos antes que por posición */
  tags?: FaceTag[];
}

export interface EdgeRef {
  point: P3;
  direction: P3;
  /** Orígenes de las dos caras que separa la arista */
  sides?: FaceTag[][];
}

export type PlaneSpec =
  | { type: "xy" }
  | { type: "xz" }
  | { type: "yz" }
  | { type: "face"; face: FaceRef }
  | { type: "custom"; plane: Plane }
  /** Un plano de referencia del historial */
  | { type: "reference"; feature: number };

export type AxisSpec =
  | { type: "x" }
  | { type: "y" }
  | { type: "z" }
  | { type: "sketch_line"; sketch: number; line: number }
  | { type: "edge"; edge: EdgeRef }
  | { type: "custom"; origin: P3; direction: P3 }
  /** Un eje de referencia del historial */
  | { type: "reference"; feature: number };

/** Punto: coordenadas, centro de una arista, sobre una arista recta o de referencia */
export type PointSpec =
  | { type: "at"; point: P3 }
  | { type: "center"; edge: EdgeRef }
  | { type: "on_edge"; edge: EdgeRef; at: number }
  | { type: "reference"; feature: number }
  /** Un extremo de una arista (vértice del sólido): el final con `end` */
  | { type: "edge_end"; edge: EdgeRef; end?: boolean };

export type PlaneDef =
  | { type: "offset"; base: PlaneSpec; distance: number }
  | { type: "angle"; base: PlaneSpec; axis: AxisSpec; angle: number }
  | { type: "midplane"; a: PlaneSpec; b: PlaneSpec }
  | { type: "three_points"; points: [PointSpec, PointSpec, PointSpec] };

export type AxisDef =
  | { type: "two_points"; a: PointSpec; b: PointSpec }
  | { type: "edge"; edge: EdgeRef }
  | { type: "face"; face: FaceRef }
  | { type: "planes"; a: PlaneSpec; b: PlaneSpec };

/** Geometría de referencia calculada */
export type RefView = { id: number } & (
  | { kind: "plane"; plane: Plane }
  | { kind: "axis"; origin: P3; dir: P3 }
  | { kind: "point"; point: P3 }
  | { kind: "curve"; points: P3[] }
  /** Varias curvas (sketch 3D o envuelto): una polilínea por arista */
  | { kind: "curves"; lines: P3[][] }
);

// ─── Sketch 3D ─────────────────────────────────────────────────────────────

export interface Point3d {
  id: number;
  x: number;
  y: number;
  z: number;
}

export type Geometry3d =
  | { type: "line"; start: number; end: number }
  /** Arco por tres puntos */
  | { type: "arc"; start: number; mid: number; end: number }
  | { type: "spline"; points: number[] }
  | { type: "point"; point: number };

export interface Entity3d {
  id: number;
  construction?: boolean;
  geometry: Geometry3d;
}

export type WorldAxis = "x" | "y" | "z";

export type Constraint3d =
  | { type: "coincident"; a: number; b: number }
  | { type: "fixed"; point: number; at: P3 }
  | { type: "attach"; point: number; target: PointSpec }
  | { type: "on_plane"; point: number; plane: PlaneSpec; offset?: number }
  | { type: "along_axis"; line: number; axis: WorldAxis }
  | { type: "parallel"; a: number; b: number }
  | { type: "perpendicular"; a: number; b: number }
  | { type: "equal"; a: number; b: number }
  | { type: "tangent"; a: number; b: number }
  | { type: "midpoint"; point: number; line: number }
  | { type: "length"; line: number; value: number }
  | { type: "distance"; a: number; b: number; value: number }
  | { type: "angle"; a: number; b: number; degrees: number };

export interface Sketch3d {
  points: Point3d[];
  entities: Entity3d[];
  constraints: Constraint3d[];
  next_id: number;
}

export interface Report3d {
  status: "well_constrained" | "under_constrained" | "over_constrained" | "failed";
  dof: number;
  residual: number;
  conflicting: number[];
  missing?: number[];
  free_points: number[];
  free_entities: number[];
}

export interface Sketch3dView {
  id: number;
  sketch: Sketch3d;
  report: Report3d;
}

/** Unir funde con lo que toca (o crea pieza si no toca nada); `new` siempre crea pieza aparte */
export type BendRelief = "rectangle" | "obround" | "none";
export type CornerRelief = "round" | "square" | "none";
export type BodyOp = "join" | "cut" | "intersect" | "new";

export type RegionSelection = { type: "all" } | { type: "points"; points: P2[] };

export type Extent =
  | { type: "blind"; distance: number }
  | { type: "symmetric"; distance: number }
  | { type: "through_all" }
  | { type: "up_to_face"; face: FaceRef }
  /** `distance` hacia la normal y `second` hacia atrás */
  | { type: "two_sides"; distance: number; second: number }
  /** Hasta la primera cara del sólido que encuentra */
  | { type: "up_to_next" };

export interface SketchPoint {
  id: number;
  x: number;
  y: number;
}

export type Geometry =
  | { type: "line"; start: number; end: number }
  | { type: "circle"; center: number; radius: number }
  | { type: "arc"; center: number; start: number; end: number }
  /**
   * Manijas opcionales (abierta): dirección de salida y de llegada, en el
   * sentido de avance; `handles`: `[i, h]` da la dirección en `points[i]`
   */
  | { type: "spline"; points: number[]; closed: boolean; start_handle?: number; end_handle?: number; handles?: [number, number][] }
  /** Punto suelto: no forma perfiles */
  | { type: "point"; point: number }
  /** Elipse: extremos de los semiejes (a 90°) */
  | { type: "ellipse"; center: number; major: number; minor: number }
  /** Arco de elipse antihorario de `start` a `end` (el solver los deja sobre la elipse) */
  | { type: "ellipse_arc"; center: number; major: number; minor: number; start: number; end: number }
  /**
   * B-spline por polos: abierta pasa por el primero y el último; cerrada es
   * periódica. Con `weights`, racional (cónicas); `knots` vacío = uniforme
   */
  | { type: "bspline"; poles: number[]; degree: number; closed?: boolean; weights?: number[]; knots?: number[] };

export interface SketchEntity {
  id: number;
  construction?: boolean;
  /** Línea infinita (de construcción): se dibuja de punta a punta de la vista */
  infinite?: boolean;
  /** Línea central: eje de revolución y de los diámetros, tomado sin elegirlo */
  axis?: boolean;
  geometry: Geometry;
}

export type SketchConstraint =
  | { type: "coincident"; a: number; b: number }
  | { type: "fixed"; point: number; x: number; y: number }
  | { type: "horizontal"; line: number }
  | { type: "vertical"; line: number }
  | { type: "horizontal_points"; a: number; b: number }
  | { type: "vertical_points"; a: number; b: number }
  | { type: "parallel"; a: number; b: number }
  | { type: "collinear"; a: number; b: number }
  | { type: "perpendicular"; a: number; b: number }
  | { type: "equal"; a: number; b: number }
  | { type: "tangent"; a: number; b: number }
  | { type: "concentric"; a: number; b: number }
  | { type: "point_on_line"; point: number; line: number }
  | { type: "point_on_circle"; point: number; circle: number }
  | { type: "midpoint"; point: number; line: number }
  | { type: "symmetric"; a: number; b: number; line: number }
  | { type: "equal_offset"; a1: number; a2: number; b1: number; b2: number }
  | { type: "equal_rotation"; center: number; a1: number; a2: number; b1: number; b2: number }
  | ({ type: "distance"; a: number; b: number; value: number } & Dim)
  | ({ type: "horizontal_distance"; a: number; b: number; value: number } & Dim)
  | ({ type: "vertical_distance"; a: number; b: number; value: number } & Dim)
  | ({ type: "length"; line: number; value: number } & Dim)
  | ({ type: "radius"; entity: number; value: number } & Dim)
  | ({ type: "diameter"; entity: number; value: number } & Dim)
  /** De `a` a `b`; con `supplementary`, de `a` a `b` invertida (180° − el ángulo) */
  | ({ type: "angle"; a: number; b: number; degrees: number; supplementary?: boolean } & Dim)
  /** Distancia de un punto a la recta de una línea (entre paralelas: un extremo de una y la otra) */
  | ({ type: "point_line_distance"; point: number; line: number; value: number } & Dim)
  /** Cota simétrica respecto de un eje: el doble de la distancia (diámetro en perfiles de revolución) */
  | ({ type: "axis_diameter"; point: number; line: number; value: number } & Dim)
  | ({ type: "arc_length"; arc: number; value: number } & Dim)
  /** Largo total de una cadena de líneas, arcos y círculos */
  | ({ type: "curve_length"; entities: number[]; value: number } & Dim)
  /**
   * Distancia mínima (o máxima con `max`) entre un círculo o arco y otro, un
   * punto o una línea (`a` y `b`: ids de punto o de entidad)
   */
  | ({ type: "circle_distance"; a: number; b: number; max?: boolean; value: number } & Dim)
  /** Mismo centro y mismo radio */
  | { type: "coradial"; a: number; b: number }
  /** Dos entidades del mismo tipo simétricas respecto de una línea */
  | { type: "symmetric_entities"; a: number; b: number; line: number }
  /** Punto sobre cualquier curva (también elipse y spline) */
  | { type: "point_on_curve"; point: number; curve: number }
  /** Punto en la intersección de dos curvas */
  | { type: "intersection"; point: number; a: number; b: number }
  /** Entidad bloqueada entera (puntos y radio) */
  | { type: "lock"; entity: number }
  /** Continuidad de curvatura (G2) en el extremo común; una de las dos es una spline por polos */
  | { type: "curvature"; a: number; b: number }
  /** Perforación: el punto va donde la curva de la operación `curve` cruza el plano (`at` lo pone el historial) */
  | { type: "pierce"; point: number; curve: number; at: P2 };

/** Lo común a las cotas: de referencia (no restringe) y opciones de cómo se ven */
export interface Dim {
  reference?: boolean;
  opts?: DimOpts;
}

export interface DimOpts {
  /** No se edita por error ni la cambia transformar */
  locked?: boolean;
  /** Lugar del texto respecto del que le toca (mm del plano) */
  offset?: [number, number];
  /** Cota de ordenadas: el valor junto al punto medido */
  ordinate?: boolean;
}

export interface Sketch {
  points: SketchPoint[];
  entities: SketchEntity[];
  constraints: SketchConstraint[];
  next_id?: number;
  /** Punto origen, fijo en (0, 0): no se borra */
  origin?: number;
  /** Textos insertados como curvas: se mueven en bloque con su ancla y se pueden rehacer */
  texts?: SketchText[];
  /** Entidades ligadas al modelo: siguen a su fuente proyectada (para el solver, fijas) */
  uses?: SketchUse[];
  /** Dirección horizontal del sketch: la de este eje o arista proyectada al plano */
  x_axis?: AxisSpec | null;
  /** Normal invertida (se mira y se extruye desde el otro lado) */
  flip_normal?: boolean;
}

/** Entidad ligada: a una arista del sólido (`edge`) o a otra fuente */
export interface SketchUse {
  edge?: EdgeRef;
  source?: UseSource;
  entity: number;
}

export type UseSource =
  /** Una de las curvas donde el plano del sketch corta el sólido */
  | { type: "section" }
  /** Una de las curvas del contorno del sólido visto desde la normal del plano */
  | { type: "silhouette" }
  /** Una entidad de un sketch anterior */
  | { type: "sketch"; feature: number; entity: number };

/** Curva del modelo llevada al plano del sketch (arcos antihorario) */
export type Projected =
  | { kind: "point"; at: P2 }
  | { kind: "line"; start: P2; end: P2 }
  | { kind: "circle"; center: P2; radius: number }
  | { kind: "arc"; center: P2; start: P2; end: P2 }
  | { kind: "ellipse"; center: P2; major: P2; minor: P2 }
  | { kind: "spline"; points: P2[]; closed: boolean; handles?: [P2, P2] };

/** Arista del sólido proyectada al plano de un sketch, con su referencia */
export type ProjectedEdge = Projected & { ref: EdgeRef };

/**
 * Agrega curvas proyectadas al sketch, cada una ligada con `link(i)`. Los
 * extremos que caen en el mismo lugar comparten punto. Devuelve las entidades.
 */
export function addProjected(s: Sketch, curves: Projected[], link: (i: number) => Omit<SketchUse, "entity">): number[] {
  const made: { id: number; p: P2 }[] = [];
  const pt = (p: P2, share = true): number => {
    const tol = 1e-6 * Math.max(1, Math.hypot(p[0], p[1]));
    const old = share ? made.find((m) => Math.hypot(m.p[0] - p[0], m.p[1] - p[1]) <= tol) : undefined;
    if (old) return old.id;
    const id = addPoint(s, p);
    if (share) made.push({ id, p });
    return id;
  };
  const ids = curves.map((c, i) => {
    const g: Geometry =
      c.kind === "point"
        ? { type: "point", point: pt(c.at) }
        : c.kind === "line"
          ? { type: "line", start: pt(c.start), end: pt(c.end) }
          : c.kind === "circle"
            ? { type: "circle", center: pt(c.center, false), radius: c.radius }
            : c.kind === "arc"
              ? { type: "arc", center: pt(c.center, false), start: pt(c.start), end: pt(c.end) }
              : c.kind === "ellipse"
                ? { type: "ellipse", center: pt(c.center, false), major: pt(c.major, false), minor: pt(c.minor, false) }
                : {
                    type: "spline",
                    points: c.points.map((q, k) => pt(q, !c.closed && (k === 0 || k === c.points.length - 1))),
                    closed: c.closed,
                    ...(c.handles ? { start_handle: pt(c.handles[0], false), end_handle: pt(c.handles[1], false) } : {}),
                  };
    const entity = addEntity(s, g);
    s.uses = [...(s.uses ?? []), { ...link(i), entity }];
    return entity;
  });
  return ids;
}

/** Quita el vínculo de esas entidades: quedan donde están, propias y editables */
export function breakLinks(s: Sketch, entities: number[]): number {
  const before = s.uses?.length ?? 0;
  s.uses = (s.uses ?? []).filter((u) => !entities.includes(u.entity));
  if (s.uses.length === 0) delete s.uses;
  return before - (s.uses?.length ?? 0);
}

/** Texto del sketch: el ancla es el comienzo de la línea base */
export interface SketchText {
  id: number;
  text: string;
  size: number;
  font: string;
  anchor: number;
  entities: number[];
  points: number[];
  style?: TextStyle;
}

/** Negrita, cursiva, alineación respecto del ancla y curva que siguen las letras */
export interface TextStyle {
  bold?: boolean;
  italic?: boolean;
  align?: "left" | "center" | "right";
  path?: number;
}

export type PrimitiveShape =
  /**
   * `centered`: centrada en X e Y sobre el origen (las cajas viejas van desde la
   * esquina); `centered_z`: también en Z (el centro de la caja en el origen)
   */
  | { type: "box"; dx: number; dy: number; dz: number; centered?: boolean; centered_z?: boolean }
  | { type: "cylinder"; radius: number; height: number }
  | { type: "cone"; r1: number; r2: number; height: number }
  | { type: "sphere"; radius: number }
  | { type: "torus"; major: number; minor: number }
  /** Tornillo métrico: largo bajo la cabeza (el total en el avellanado) */
  | { type: "bolt"; size: string; length: number; head?: BoltHead; modeled?: boolean }
  | { type: "nut"; size: string; modeled?: boolean }
  | { type: "washer"; size: string };

/** Hexagonal (ISO 4017), Allen (ISO 4762) o avellanada Allen (ISO 10642) */
export type BoltHead = "hex" | "socket" | "countersunk";

export const BOLT_HEADS: { value: BoltHead; label: string }[] = [
  { value: "socket", label: "Allen (ISO 4762)" },
  { value: "hex", label: "Hexagonal (ISO 4017)" },
  { value: "countersunk", label: "Avellanada (ISO 10642)" },
];

/** Largos normales de tornillos (mm) */
export const BOLT_LENGTHS = [3, 4, 5, 6, 8, 10, 12, 16, 20, 25, 30, 35, 40, 45, 50, 55, 60, 65, 70, 80, 90, 100, 110, 120];

/** Dónde apoyar una pieza estándar en el borde de un agujero (`cad_edge_seat`) */
export interface Seat {
  origin: P3;
  z: P3;
  x: P3;
  diameter: number;
  size: string;
  /** Material atravesado al lado del agujero */
  depth?: number | null;
}

export type PatternKind =
  | { type: "linear"; direction: P3; count: number; spacing: number }
  | { type: "circular"; axis: AxisSpec; count: number; angle: number }
  /** Copias repartidas de punta a punta de un camino */
  | { type: "curve"; path: { type: "sketch"; sketch: number; entities: number[] } | { type: "curve"; feature: number }; count: number }
  /** Copias trasladadas según una tabla de desplazamientos (mm) */
  | { type: "table"; offsets: P3[] }
  /** Copias en grilla (cuadrada o hexagonal) que llenan regiones de un sketch */
  | { type: "fill"; sketch: number; regions: RegionSelection; spacing: number; hex: boolean; margin: number };

/** Con qué se parte: un plano u otra pieza (una superficie curva, por ejemplo) */
export type SplitTool = { type: "plane"; plane: PlaneSpec } | { type: "part"; part: PartId };

export type FeatureKind =
  | { type: "sketch"; plane: PlaneSpec; offset: number; sketch: Sketch }
  /** Sketch 3D: curvas en el espacio (caminos de barrido) */
  | { type: "sketch3d"; sketch: Sketch3d }
  /** Sketch envuelto sobre una cara (se dibuja en el plano tangente en su centro) */
  | { type: "surface_sketch"; face: FaceRef; sketch: Sketch }
  | {
      type: "extrude";
      sketch: number;
      regions: RegionSelection;
      extent: Extent;
      reverse: boolean;
      op: BodyOp;
      /** Desmolde de las paredes (grados; positivo = se angosta) */
      draft?: number;
      /** Extrusión delgada: espesor de la pared */
      thin?: number | null;
    }
  | { type: "revolve"; sketch: number; regions: RegionSelection; axis: AxisSpec; angle: number; op: BodyOp }
  /** `link`: tornillo o tuerca puesto en otra rosca (el marco de arriba no se usa) */
  | { type: "primitive"; shape: PrimitiveShape; origin: P3; z: P3; x: P3; op: BodyOp; link?: ThreadLink | null }
  /** Con `radius2`, variable de `radius` al comienzo de cada arista a `radius2` al final */
  | { type: "fillet"; edges: EdgeRef[]; radius: number; radius2?: number | null }
  /** Con `second`, asimétrico: otra distancia o un ángulo (`distance` va sobre una cara; `flip`, la otra) */
  | { type: "chamfer"; edges: EdgeRef[]; distance: number; second?: ChamferSecond | null }
  | { type: "shell"; faces: FaceRef[]; thickness: number }
  | { type: "draft"; faces: FaceRef[]; neutral: PlaneSpec; angle: number }
  | { type: "pattern"; features: number[]; pattern: PatternKind }
  | { type: "mirror"; features: number[]; plane: PlaneSpec }
  | { type: "split"; plane: PlaneSpec; flip: boolean }
  | { type: "import"; format: "step" | "brep"; data: number[]; op: BodyOp }
  /** Entre piezas: unir todas en la primera, restar `tools` de `targets` o dejar lo común */
  | { type: "boolean"; op: "union" | "subtract" | "intersect"; targets: PartId[]; tools: PartId[]; keep_tools: boolean }
  /** Separa los sólidos sueltos de cada pieza (vacío = todas) */
  | { type: "split_parts"; parts: PartId[] }
  | { type: "delete_parts"; parts: PartId[] }
  /** Perfil (regiones de un sketch) a lo largo de un camino (entidades de otro sketch; vacío = todas) */
  | {
      type: "sweep";
      sketch: number;
      regions: RegionSelection;
      path: { type: "sketch"; sketch: number; entities: number[] } | { type: "curve"; feature: number };
      op: BodyOp;
    }
  /** Hélice de referencia (camino de resortes y roscas) */
  | { type: "helix"; axis: AxisSpec; radius: number; pitch: number; turns: number; left: boolean }
  /** Espesor a caras del sólido */
  | { type: "thicken"; faces: FaceRef[]; thickness: number; op: BodyOp }
  /** Caras planas movidas a lo largo de su normal (afuera suma, adentro resta) */
  | { type: "move_face"; faces: FaceRef[]; distance: number }
  /** Pared desde las líneas de un sketch hasta el sólido, centrada en su plano */
  | { type: "rib"; sketch: number; thickness: number; flip: boolean }
  /** Caras planas llevadas hasta un plano (cara plana o plano de referencia) */
  | { type: "replace_face"; faces: FaceRef[]; target: PlaneSpec }
  /** Escala por eje alrededor de un punto */
  | { type: "scale"; factor: P3; center: PointSpec }
  /** Sólido que pasa por varias secciones, una región por sketch */
  | { type: "loft"; sections: { sketch: number; regions: RegionSelection }[]; ruled: boolean; op: BodyOp }
  /** Agujeros en los puntos de un sketch (o sus círculos); `reverse`: a favor de la normal del plano (sin elegir, hacia el material) */
  | {
      type: "hole";
      sketch: number;
      points: number[];
      diameter: number;
      depth: { type: "blind"; depth: number } | { type: "through_all" };
      style: { type: "simple" } | { type: "counterbore"; diameter: number; depth: number } | { type: "countersink"; diameter: number; angle: number };
      tip_angle: number;
      thread?: string | null;
      /** Rosca modelada (el filete de verdad, para imprimir) */
      modeled?: ThreadSpec | null;
      /** Medida de otra rosca, con el filete alineado en el centro que está en su eje */
      link?: ThreadLink | null;
      reverse?: boolean | null;
    }
  /** Rosca sobre una cara cilíndrica: exterior en un eje, interior en un agujero */
  | { type: "thread"; face: FaceRef; pitch: number; length: number; flip: boolean; left: boolean; clearance: number; link?: ThreadLink | null }
  /** Chapa metálica: regiones de un sketch al espesor; radio interior de doblez y factor K para pestañas y desarrollo */
  | { type: "sheet_metal"; sketch: number; regions: RegionSelection; thickness: number; radius: number; k_factor: number; flip: boolean; op: BodyOp }
  /** Pestaña: pared doblada desde una arista del borde de la chapa (`length` después del doblez) */
  | {
      type: "flange";
      edge?: EdgeRef | null;
      length: number;
      angle: number;
      flip: boolean;
      radius?: number | null;
      /** Ranura donde la chapa sigue al lado del doblez (esquina hacia adentro) */
      relief?: BendRelief;
      relief_width?: number | null;
      /** Donde se junta con el doblez de otra pestaña */
      corner_relief?: CornerRelief;
      corner_size?: number | null;
      /** Esquina cerrada con otra pestaña a 90° hacia el mismo lado */
      closed_corner?: boolean;
      corner_gap?: number | null;
    }
  /** Superficie (sin espesor) de curvas de un sketch; `entities` vacío = todas */
  | { type: "surface_extrude"; sketch: number; entities: number[]; extent: Extent; reverse: boolean }
  | { type: "surface_revolve"; sketch: number; entities: number[]; axis: AxisSpec; angle: number }
  /** Superficie que cierra un borde de aristas */
  | { type: "fill"; edges: EdgeRef[]; tangent: boolean }
  /** Une superficies por sus bordes; con `solid`, lo cerrado queda sólido */
  | { type: "sew"; parts: PartId[]; solid: boolean; tolerance: number }
  /** Partir piezas (vacío = todos los sólidos): cada pedazo queda como pieza */
  | { type: "split_by"; parts: PartId[]; tool: SplitTool }
  | { type: "plane"; def: PlaneDef }
  | { type: "axis"; def: AxisDef }
  | { type: "point"; def: PointSpec };

export type ChamferSecond = { type: "distance"; distance: number; flip: boolean } | { type: "angle"; degrees: number; flip: boolean };

/** Rosca métrica ISO (60°); `clearance` agranda las interiores y achica las exteriores (mm en el diámetro) */
export interface ThreadSpec {
  nominal: number;
  pitch: number;
  clearance: number;
  left: boolean;
}

/**
 * Coordinación entre roscas: toma la medida de otra rosca anterior y alinea el
 * filete con el suyo. En tornillos y tuercas también la posición: van en su
 * eje desde la boca (`flip`: la otra), `offset` mm hacia adentro.
 */
export interface ThreadLink {
  feature: number;
  index: number;
  flip?: boolean;
  offset?: number;
}

/** Rosca calculada (agujero roscado, rosca, tornillo o tuerca) */
export interface ThreadAxis {
  feature: number;
  /** Cuál de la operación (un agujero tiene una por centro) */
  index: number;
  spec: ThreadSpec;
  /** Hembra (agujero, tuerca) o macho (eje, tornillo) */
  internal: boolean;
  modeled: boolean;
  /** Hélice: el filete pasa por origin + r·x y avanza hacia dir */
  origin: P3;
  dir: P3;
  x: P3;
  /** Boca (sobre el eje) y hacia afuera */
  mouth: P3;
  out: P3;
  length: number;
  blind: boolean;
  /** Marco del tornillo o la tuerca: origen, Z, X */
  placed?: [P3, P3, P3] | null;
  link?: ThreadLink | null;
}

export interface Feature {
  id: number;
  name: string;
  suppressed?: boolean;
  kind: FeatureKind;
  /** Con qué piezas une, resta o interseca (vacío = las que toca) */
  scope?: PartId[];
}

/** Parámetro con nombre: `ancho = 40`, `alto = ancho / 2` */
export interface Parameter {
  name: string;
  expr: string;
}

/** Desarrollo de chapa (`cad_flat_pattern`), en mm con la esquina en (0, 0) */
export interface FlatPattern {
  outline: P2[][];
  bends: { line: [P2, P2]; angle: number; radius: number; up: boolean }[];
  thickness: number;
  k_factor: number;
  min: P2;
  max: P2;
}

export interface CadDocument {
  features: Feature[];
  rollback?: number | null;
  next_id?: number;
  parameters?: Parameter[];
  /** Campos calculados por fórmula: ruta (`<id>.kind.…`) → expresión */
  bindings?: Record<string, string>;
  /** Material del sólido (densidad en kg/m³), para la masa */
  material?: Material | null;
  /** Carpetas del árbol (solo presentación) */
  folders?: Folder[];
  /** Nombre, color y visibilidad de las piezas */
  parts?: PartProps[];
  /** Ensamble de las piezas */
  assembly?: Assembly | null;
  /** Cotas del plano 2D (ver lib/drawing.ts) */
  drawing?: { dims?: import("./drawing").UserDim[]; details?: import("./drawing").Detail[] } | null;
  /** Variantes: parámetro → expresión que lo reemplaza y operaciones suprimidas */
  configurations?: Configuration[];
  /** La que se calcula (ninguna: lo de base) */
  active_configuration?: number | null;
  /** Instantáneas con nombre (el documento de ese momento, sin sus versiones) */
  versions?: NamedVersion[];
}

export interface NamedVersion {
  name: string;
  /** Fecha ISO 8601 */
  created: string;
  note?: string | null;
  document: CadDocument;
}

/** Diferencias del diseño actual con una versión (mallas en coordenadas del visor) */
export interface VersionComparison {
  version: number;
  /** mm³ que están ahora y no estaban */
  added: number;
  /** mm³ que estaban y ya no están */
  removed: number;
  addedMesh: CadMesh | null;
  removedMesh: CadMesh | null;
}

/** Bytes de `cad_compare`: volúmenes (f64), largo de la primera malla y las dos mallas */
export function decodeComparison(buffer: ArrayBuffer, version: number): VersionComparison {
  const v = new DataView(buffer);
  const added = v.getFloat64(0, true);
  const removed = v.getFloat64(8, true);
  const len = v.getUint32(16, true);
  return {
    version,
    added,
    removed,
    addedMesh: decodeCadMesh(buffer.slice(20, 20 + len)),
    removedMesh: decodeCadMesh(buffer.slice(20 + len)),
  };
}

/** Siguiente nombre libre "v1", "v2"… */
export function nextVersionName(versions: NamedVersion[] | undefined): string {
  const used = new Set((versions ?? []).map((v) => v.name));
  let n = (versions?.length ?? 0) + 1;
  while (used.has(`v${n}`)) n++;
  return `v${n}`;
}

export interface Configuration {
  name: string;
  values?: Record<string, string>;
  suppressed?: number[];
}

// ─── Ensamble ─────────────────────────────────────────────────────────────

export interface Assembly {
  instances: AsmInstance[];
  mates: Mate[];
  next_id: number;
}

/** Una pieza puesta en el ensamble: posición (mm) y giro como vector (eje × ángulo en rad) */
export interface AsmInstance {
  id: number;
  part: PartId;
  name: string;
  position: P3;
  rotation: P3;
  fixed: boolean;
}

/** Sistema de coordenadas sobre una instancia, en coordenadas de su pieza */
export interface Connector {
  instance: number;
  origin: P3;
  z: P3;
  x: P3;
}

export type MateKind = "fastened" | "revolute" | "slider" | "cylindrical" | "planar";

export const MATE_LABELS: Record<MateKind, string> = {
  fastened: "Fija",
  revolute: "Bisagra (gira)",
  slider: "Deslizante",
  cylindrical: "Cilíndrica (gira y desliza)",
  planar: "Plana (apoyada)",
};

export interface Mate {
  id: number;
  name: string;
  kind: MateKind;
  a: Connector;
  b: Connector;
  flip: boolean;
  /** Ángulo impuesto (grados) en bisagras y cilíndricas */
  angle?: number | null;
  /** Distancia impuesta (mm) en deslizantes y cilíndricas */
  distance?: number | null;
}

export interface AssemblyView {
  solution: { poses: [number, P3, P3][]; residual: number; converged: boolean; dof: number };
  version: number;
}

/** Pieza: la operación que la creó y su número dentro de ella */
export interface PartId {
  feature: number;
  index: number;
}

export const samePart = (a: PartId, b: PartId) => a.feature === b.feature && a.index === b.index;

export interface PartProps {
  part: PartId;
  name?: string;
  /** "#rrggbb" */
  color?: string;
  hidden?: boolean;
  /** Material propio (si no, el del diseño) */
  material?: Material | null;
}

export interface PartView {
  id: PartId;
  name: string;
  /** Caras y aristas [desde, hasta) del cuerpo */
  faces: [number, number];
  edges: [number, number];
  volume: number;
  area: number;
  center: P3;
  /** Superficie (sin volumen) en vez de sólido */
  surface?: boolean;
}

/** Colores de pieza por defecto (la primera, el de siempre) */
export const PART_COLORS = ["#9aa4b8", "#c9a96e", "#8fb98b", "#c48b9f", "#7fa7c9", "#b39ddb", "#d4a373", "#80cbc4"];

/** Color de la pieza `i`: el elegido o uno de la paleta */
export function partColor(doc: CadDocument | null | undefined, p: PartView, i: number): string {
  return doc?.parts?.find((x) => samePart(x.part, p.id))?.color ?? PART_COLORS[i % PART_COLORS.length];
}

/** Material de la pieza: el propio o el del diseño */
export function partMaterial(doc: CadDocument | null | undefined, p: PartView): Material | null {
  return doc?.parts?.find((x) => samePart(x.part, p.id))?.material ?? doc?.material ?? null;
}

/** Masa en gramos (mm³ × kg/m³ × 1e−6), o null sin material */
export function partMass(doc: CadDocument | null | undefined, p: PartView): number | null {
  const m = partMaterial(doc, p);
  return m ? p.volume * m.density * 1e-6 : null;
}

/**
 * Masa del diseño: suma de las piezas con su material, y centro de masa
 * ponderado. `mass` es null si alguna pieza no tiene material; `uniform` dice
 * si todas tienen la misma densidad (entonces la inercia del cuerpo vale).
 */
export function designMass(doc: CadDocument | null | undefined, r: CadResult | null | undefined): { mass: number | null; center: P3 | null; uniform: boolean } {
  const body = r?.body;
  const parts = r?.parts ?? [];
  if (!body) return { mass: null, center: null, uniform: true };
  const masses = parts.map((p) => partMass(doc, p));
  const densities = parts.map((p) => partMaterial(doc, p)?.density ?? null);
  const uniform = densities.every((d) => d === densities[0]);
  if (!parts.length || masses.some((m) => m === null)) return { mass: null, center: body.center, uniform };
  const total = masses.reduce<number>((a, m) => a + (m ?? 0), 0);
  if (total <= 0) return { mass: total, center: body.center, uniform };
  const center = [0, 1, 2].map((k) => parts.reduce((a, p, i) => a + p.center[k] * masses[i]!, 0) / total) as P3;
  return { mass: total, center, uniform };
}

export function partHidden(doc: CadDocument | null | undefined, p: PartView): boolean {
  return !!doc?.parts?.find((x) => samePart(x.part, p.id))?.hidden;
}

/** Carpeta: las operaciones de `first` a `last` en el orden actual */
export interface Folder {
  name: string;
  first: number;
  last: number;
  collapsed?: boolean;
}

/** Posiciones [desde, hasta] de una carpeta en el documento (null si sus extremos ya no están) */
export function folderRange(doc: CadDocument, f: Folder): [number, number] | null {
  const a = doc.features.findIndex((x) => x.id === f.first);
  const b = doc.features.findIndex((x) => x.id === f.last);
  if (a < 0 || b < 0) return null;
  return a <= b ? [a, b] : [b, a];
}

/**
 * Ajusta las carpetas tras sacar o mover la operación `id` (`next` = orden
 * nuevo de ids, sin ella si se borró): si sale del tramo deja la carpeta; si
 * cae entre dos de sus operaciones, entra. Una carpeta que se queda vacía se va,
 * salvo que se mueva sola con ella.
 */
export function fixFolders(doc: CadDocument, id: number, next: number[]): Folder[] {
  const before = doc.features.map((f) => f.id);
  const out: Folder[] = [];
  for (const f of doc.folders ?? []) {
    const r = folderRange(doc, f);
    if (!r) continue;
    const members = before.slice(r[0], r[1] + 1);
    const others = members.filter((x) => x !== id);
    if (!others.length) {
      if (next.includes(id)) out.push({ ...f, first: id, last: id });
      continue;
    }
    const pos = others.map((x) => next.indexOf(x));
    out.push({ ...f, first: next[Math.min(...pos)], last: next[Math.max(...pos)] });
  }
  return out;
}

export interface Material {
  name: string;
  density: number;
}

/** Materiales comunes (densidad típica en kg/m³) */
export const MATERIALS: Material[] = [
  { name: "PLA", density: 1240 },
  { name: "PETG", density: 1270 },
  { name: "ABS", density: 1040 },
  { name: "Nailon", density: 1140 },
  { name: "Resina", density: 1180 },
  { name: "Aluminio", density: 2700 },
  { name: "Acero", density: 7850 },
  { name: "Acero inoxidable", density: 8000 },
  { name: "Latón", density: 8500 },
  { name: "Madera de pino", density: 500 },
];

/** Valor calculado de un parámetro (key = nombre) o de un campo vinculado (key = ruta) */
export interface ResolvedValue {
  key: string;
  value: number | null;
  error: string | null;
}

// ─── Resultados ───────────────────────────────────────────────────────────

/** Referencia que no se encontró: campo de la operación y posición en su lista */
export interface MissingRef {
  field: "edges" | "faces" | "regions" | "plane" | "neutral" | "axis" | "extent" | "targets" | "tools" | "parts" | "scope" | "base" | "a" | "b" | "edge" | "face" | "points" | "def" | "target";
  index: number;
}

export type FeatureState =
  | { state: "ok" }
  | { state: "error"; message: string; missing?: MissingRef[] }
  /** Se calculó con parte de lo elegido: el resto ya no está */
  | { state: "warning"; message: string; missing: MissingRef[] }
  | { state: "suppressed" }
  | { state: "rolled_back" };

/** `ms`: lo que tardó la última vez que se calculó */
/** `auto_scope`: piezas que eligió sola, sin alcance elegido (la más cercana a un agujero) */
export type FeatureStatus = { id: number; ms?: number; auto_scope?: PartId[]; handle?: FeatureHandle } & FeatureState;

/**
 * Flecha de una operación en el visor (extrusión, chapa, agujero): desde
 * `origin` hacia `dir` hasta `length` mm. `reversed`: el sentido con que se
 * calculó (el `reverse`/`flip` de la operación).
 */
export interface FeatureHandle {
  origin: P3;
  dir: P3;
  length: number;
  reversed: boolean;
  kind: "blind" | "symmetric" | "two_sides" | "fixed";
}

/** Campo que cambia al arrastrar la flecha (ruta para los vínculos), o nada */
export function handleField(k: FeatureKind, h: FeatureHandle): string | undefined {
  if (h.kind === "fixed") return undefined;
  if (k.type === "extrude") return "kind.extent.distance";
  if (k.type === "hole") return "kind.depth.depth";
  if (k.type === "sheet_metal") return "kind.thickness";
  return undefined;
}

/** Clic en la flecha: hacia el otro lado */
export function flipByHandle(k: FeatureKind, h: FeatureHandle) {
  if (k.type === "extrude" || k.type === "hole") k.reverse = !h.reversed;
  else if (k.type === "sheet_metal") k.flip = !h.reversed;
}

/**
 * Arrastrar la flecha `h` (la del comienzo) hasta `t` mm a lo largo de ella:
 * el largo nuevo; pasando del origen, se da vuelta (salvo simétrica y dos direcciones).
 */
export function dragByHandle(k: FeatureKind, h: FeatureHandle, t: number) {
  const len = Math.max(Math.abs(t), 0.01);
  const reversed = h.reversed !== t < 0;
  if (k.type === "extrude" && "distance" in k.extent) {
    if (h.kind === "symmetric") k.extent.distance = 2 * len;
    else if (h.kind === "two_sides") k.extent.distance = len;
    else {
      k.extent.distance = len;
      k.reverse = reversed;
    }
  } else if (k.type === "hole" && k.depth.type === "blind") {
    k.depth.depth = len;
    k.reverse = reversed;
  } else if (k.type === "sheet_metal") {
    k.thickness = len;
    k.flip = reversed;
  }
}

export interface Loop {
  pieces: { entity: number; reversed: boolean }[];
  polygon: P2[];
  area: number;
}

export interface Region {
  outer: Loop;
  holes: Loop[];
  depth: number;
  sample: P2;
}

export interface SolveReport {
  status: "well_constrained" | "under_constrained" | "over_constrained" | "failed";
  dof: number;
  residual: number;
  conflicting: number[];
  free_points: number[];
  /** Entidades a las que les falta definir (algún punto o el radio libre) */
  free_entities?: number[];
  /** Puntos con un solo grado libre y hacia dónde se mueven (los otros libres van a cualquier lado) */
  free_dirs?: [number, P2][];
  /** Grados libres de cada entidad libre */
  entity_dof?: [number, number][];
  /** Círculos con el radio libre */
  free_radius?: number[];
  /** Hubo conflicto: se resolvió todo menos las que chocan */
  partial?: boolean;
}

/** Restricción que le falta al sketch y por qué */
export interface SketchSuggestion {
  constraint: SketchConstraint;
  why: string;
}

export interface SketchView {
  id: number;
  plane: Plane;
  sketch: Sketch;
  report: SolveReport;
  regions: Region[];
}

export interface BodyInfo {
  volume: number;
  area: number;
  bbox_min: P3;
  bbox_max: P3;
  faces: number;
  edges: number;
  valid: boolean;
  /** Centro de masa (mm) */
  center: P3;
  /** Momentos principales de inercia con densidad 1 (mm⁵) y sus ejes */
  inertia: P3;
  axes: [P3, P3, P3];
}

export interface CadResult {
  status: FeatureStatus[];
  sketches: SketchView[];
  /** Sketches 3D resueltos */
  sketches3d?: Sketch3dView[];
  body: BodyInfo | null;
  parameters: ResolvedValue[];
  bindings: ResolvedValue[];
  version: number;
  /** Operaciones calculadas en este recálculo (las demás salieron de la caché) */
  recomputed: number;
  parts: PartView[];
  references: RefView[];
  assembly: AssemblyView | null;
  /** Roscas, para coordinarlas */
  threads?: ThreadAxis[];
}

export interface CadStatus {
  available: boolean;
  occt_version: string;
  has_document: boolean;
}

export interface SolvedSketch {
  sketch: Sketch;
  report: SolveReport;
  regions: Region[];
}

/** Malla del sólido para el visor (`cad_mesh`) */
// ─── Desviación escaneo ↔ diseño ──────────────────────────────────────────

export interface ScanDeviation {
  stats: { mean: number; meanAbs: number; rms: number; p95: number; maxAbs: number; within: number; tolerance: number };
  /** Por triángulo del escaneo (mismo orden que su malla), mm; + afuera del sólido */
  perFace: Float32Array;
}

/** Color de una desviación: verde dentro de ±tol; azul hacia adentro, rojo hacia afuera hasta `range` */
export function deviationColor(v: number, tol: number, range: number): [number, number, number] {
  const green: [number, number, number] = [0.25, 0.75, 0.35];
  if (Math.abs(v) <= tol) return green;
  const t = Math.min(1, (Math.abs(v) - tol) / Math.max(range - tol, 1e-9));
  const end: [number, number, number] = v > 0 ? [0.9, 0.2, 0.15] : [0.2, 0.35, 0.95];
  const mid: [number, number, number] = v > 0 ? [0.95, 0.8, 0.2] : [0.3, 0.75, 0.9];
  // Verde → amarillo/celeste → rojo/azul
  const lerp = (a: number[], b: number[], k: number) => a.map((x, i) => x + (b[i] - x) * k) as [number, number, number];
  return t < 0.5 ? lerp(green, mid, t * 2) : lerp(mid, end, (t - 0.5) * 2);
}

// ─── Medir ────────────────────────────────────────────────────────────────

export type MeasureItem = { kind: "face"; index: number } | { kind: "edge"; index: number } | { kind: "vertex"; point: P3 };

export interface ItemMeasure {
  /** "plane", "cylinder"… (caras); "line", "circle"… (aristas); "vertex" */
  kind: string;
  area: number | null;
  length: number | null;
  radius: number | null;
  center: P3;
  direction: P3 | null;
}

export interface Measurement {
  items: ItemMeasure[];
  distance: { value: number; a: P3; b: P3; delta: P3 } | null;
  center_distance: number | null;
  angle: number | null;
}

export interface CadMesh {
  positions: Float32Array;
  normals: Float32Array;
  indices: Uint32Array;
  /** Cara B-Rep de cada triángulo */
  triangleFace: Uint32Array;
  /** Fin (acumulado, en puntos) de cada arista */
  edgeEnds: Uint32Array;
  edgePoints: Float32Array;
}

export function decodeCadMesh(buffer: ArrayBuffer): CadMesh | null {
  const [vertices, triangles, edges, points] = new Uint32Array(buffer, 0, 4);
  if (vertices === 0) return null;
  let offset = 16;
  const floats = (n: number) => {
    const v = new Float32Array(buffer, offset, n);
    offset += n * 4;
    return v;
  };
  const uints = (n: number) => {
    const v = new Uint32Array(buffer, offset, n);
    offset += n * 4;
    return v;
  };
  const positions = floats(vertices * 3);
  const normals = floats(vertices * 3);
  const indices = uints(triangles * 3);
  const triangleFace = uints(triangles);
  const edgeEnds = uints(edges);
  const edgePoints = floats(points * 3);
  return { positions, normals, indices, triangleFace, edgeEnds, edgePoints };
}

// ─── Escaneo → CAD ────────────────────────────────────────────────────────

export interface PlanePick {
  kind: "plane";
  plane: Plane;
  rms: number;
  faces: number[];
  area: number;
  boundary: P2[][];
  depth: number | null;
}

export interface CylinderPick {
  kind: "cylinder";
  origin: P3;
  direction: P3;
  radius: number;
  length: number;
  rms: number;
  faces: number[];
  hole: boolean;
  coverage: number;
}

export type ScanPick = PlanePick | CylinderPick;

export interface Detection {
  shape:
    | { type: "plane"; plane: Plane }
    | { type: "cylinder"; origin: P3; direction: P3; radius: number }
    | { type: "sphere"; center: P3; radius: number };
  faces: number[];
  area: number;
  rms: number;
}

export type ScanFeature =
  | { kind: "plane_outline"; triangle: number; extrude: boolean; depth?: number | null; op?: BodyOp }
  | { kind: "cylinder"; triangle: number }
  | { kind: "slice"; plane: Plane }
  | { kind: "work_plane"; triangle: number };

// ─── Utilidades del documento ─────────────────────────────────────────────

const clone = <T>(v: T): T => structuredClone(v);

export const FEATURE_LABELS: Record<FeatureKind["type"], string> = {
  sketch: "Sketch",
  extrude: "Extrusión",
  revolve: "Revolución",
  primitive: "Primitiva",
  fillet: "Redondeo",
  chamfer: "Chaflán",
  shell: "Vaciado",
  draft: "Desmolde",
  pattern: "Patrón",
  mirror: "Simetría",
  split: "Corte",
  import: "Importado",
  boolean: "Booleana",
  split_parts: "Separar piezas",
  delete_parts: "Borrar pieza",
  plane: "Plano",
  axis: "Eje",
  point: "Punto",
  sweep: "Barrido",
  loft: "Transición",
  hole: "Agujero",
  helix: "Hélice",
  sketch3d: "Sketch 3D",
  surface_sketch: "Sketch envuelto",
  thicken: "Engrosar",
  move_face: "Mover cara",
  rib: "Nervio",
  thread: "Rosca",
  sheet_metal: "Chapa",
  flange: "Pestaña",
  surface_extrude: "Superficie extruida",
  surface_revolve: "Superficie de revolución",
  fill: "Relleno",
  sew: "Coser",
  split_by: "Partir",
  replace_face: "Reemplazar cara",
  scale: "Escala",
};

/**
 * Tornillos métricos ISO: broca para roscar, agujero pasante (ajuste medio),
 * caja para cabeza cilíndrica (ISO 4762) y avellanado a 90° (ISO 10642).
 */
/** Tornillos métricos ISO: paso grueso, broca para roscar, pasante, caja y avellanado */
export const METRIC_HOLES: { size: string; nominal: number; pitch: number; tap: number; clearance: number; cbore: number; cboreDepth: number; csink: number }[] = [
  { size: "M2", nominal: 2, pitch: 0.4, tap: 1.6, clearance: 2.4, cbore: 4.4, cboreDepth: 2, csink: 4.4 },
  { size: "M2.5", nominal: 2.5, pitch: 0.45, tap: 2.05, clearance: 2.9, cbore: 5.5, cboreDepth: 2.5, csink: 5.5 },
  { size: "M3", nominal: 3, pitch: 0.5, tap: 2.5, clearance: 3.4, cbore: 6.5, cboreDepth: 3, csink: 6.3 },
  { size: "M4", nominal: 4, pitch: 0.7, tap: 3.3, clearance: 4.5, cbore: 8, cboreDepth: 4, csink: 8.4 },
  { size: "M5", nominal: 5, pitch: 0.8, tap: 4.2, clearance: 5.5, cbore: 10, cboreDepth: 5, csink: 10.4 },
  { size: "M6", nominal: 6, pitch: 1, tap: 5, clearance: 6.6, cbore: 11, cboreDepth: 6, csink: 12.4 },
  { size: "M8", nominal: 8, pitch: 1.25, tap: 6.8, clearance: 9, cbore: 15, cboreDepth: 8, csink: 16.4 },
  { size: "M10", nominal: 10, pitch: 1.5, tap: 8.5, clearance: 11, cbore: 18, cboreDepth: 10, csink: 20.4 },
  { size: "M12", nominal: 12, pitch: 1.75, tap: 10.2, clearance: 13.5, cbore: 20, cboreDepth: 12, csink: 24.4 },
  { size: "M16", nominal: 16, pitch: 2, tap: 14, clearance: 17.5, cbore: 26, cboreDepth: 16, csink: 32.4 },
];

/** Alto de la tuerca hexagonal ISO 4032 por medida */
export const NUT_HEIGHT: Record<string, number> = { M2: 1.6, "M2.5": 2, M3: 2.4, M4: 3.2, M5: 4.7, M6: 5.2, M8: 6.8, M10: 8.4, M12: 10.8, M16: 14.8 };

/**
 * Largo de tornillo para atravesar `depth` mm: con tuerca (pasante) suma la
 * tuerca y dos pasos; si no, el largo normal más cercano por arriba.
 */
/** "M6", "M8×1", "M6 izq." */
export function threadName(s: ThreadSpec): string {
  const coarse = METRIC_HOLES.find((m) => Math.abs(m.nominal - s.nominal) < 1e-6 && Math.abs(m.pitch - s.pitch) < 1e-9);
  const d = String(+s.nominal.toFixed(3)).replace(".", ",");
  return `${coarse ? coarse.size : `M${d}×${String(s.pitch).replace(".", ",")}`}${s.left ? " izq." : ""}`;
}

const v3 = {
  sub: (a: P3, b: P3): P3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]],
  dot: (a: P3, b: P3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2],
  len: (a: P3) => Math.hypot(a[0], a[1], a[2]),
};

/** Distancia del punto al eje de la rosca */
export function distanceToThreadAxis(t: ThreadAxis, p: P3): number {
  const d = v3.len(t.dir) || 1;
  const v = v3.sub(p, t.origin);
  const a = v3.dot(v, t.dir) / d;
  return Math.sqrt(Math.max(0, v3.dot(v, v) - a * a));
}

/** ¿Las dos roscas están en el mismo eje? (paralelas y a menos de un cuarto del diámetro) */
export function coaxialThreads(a: ThreadAxis, b: ThreadAxis): boolean {
  const cos = Math.abs(v3.dot(a.dir, b.dir)) / ((v3.len(a.dir) || 1) * (v3.len(b.dir) || 1));
  return cos > 0.999 && distanceToThreadAxis(a, b.origin) < 0.25 * Math.max(a.spec.nominal, 0.4);
}

/** Qué rosca necesita una operación para coordinarse: tornillos, interiores; tuercas, exteriores */
export type ThreadWant = "internal" | "external" | "any";

/** Roscas anteriores a la operación `id` con las que se puede coordinar */
export function threadCandidates(doc: CadDocument | null | undefined, threads: ThreadAxis[], id: number, want: ThreadWant): ThreadAxis[] {
  const order = new Map((doc?.features ?? []).map((f, i) => [f.id, i]));
  const at = order.get(id) ?? Infinity;
  return threads.filter(
    (t) => t.feature !== id && (order.get(t.feature) ?? Infinity) < at && (want === "any" || t.internal === (want === "internal")),
  );
}

/** "Agujero 1 · centro 2 · M6" */
export function threadLabel(doc: CadDocument | null | undefined, threads: ThreadAxis[], t: ThreadAxis): string {
  const f = doc?.features.find((x) => x.id === t.feature);
  const many = threads.filter((x) => x.feature === t.feature).length > 1;
  return [f?.name ?? `#${t.feature}`, many ? `centro ${t.index + 1}` : "", threadName(t.spec)].filter(Boolean).join(" · ");
}

/** La rosca de una cara elegida: la de la operación que la hizo (por sus orígenes) o la más cercana a su eje */
export function threadOfFace(face: FaceRef, threads: ThreadAxis[]): ThreadAxis | undefined {
  const owners = new Set((face.tags ?? []).map((t) => t.feature));
  const near = (list: ThreadAxis[]) =>
    list
      .map((t) => ({ t, d: distanceToThreadAxis(t, face.point) }))
      .filter(({ t, d }) => d < t.spec.nominal)
      .sort((a, b) => a.d - b.d)[0]?.t;
  return near(threads.filter((t) => owners.has(t.feature))) ?? near(threads);
}

export function boltLengthFor(size: string, depth: number, withNut: boolean): number {
  const m = METRIC_HOLES.find((h) => h.size === size);
  const need = depth + (withNut && m ? (NUT_HEIGHT[size] ?? m.nominal) + 2 * m.pitch : 0);
  return BOLT_LENGTHS.find((l) => l >= need - 1e-6) ?? Math.ceil(need);
}

export const OP_LABELS: Record<BodyOp, string> = { join: "Unir", cut: "Restar", intersect: "Intersecar", new: "Nueva pieza" };

/** Planos base (Z arriba) */
export const BASE_PLANES: Record<"xy" | "xz" | "yz", Plane> = {
  xy: { origin: [0, 0, 0], normal: [0, 0, 1], x_dir: [1, 0, 0] },
  xz: { origin: [0, 0, 0], normal: [0, -1, 0], x_dir: [1, 0, 0] },
  yz: { origin: [0, 0, 0], normal: [1, 0, 0], x_dir: [0, 1, 0] },
};

export const PLANE_LABELS: Record<"xy" | "xz" | "yz", string> = {
  xy: "Planta (XY)",
  xz: "Frente (XZ)",
  yz: "Lateral (YZ)",
};

/** Plano base desplazado a lo largo de su normal */
export function offsetPlane(base: "xy" | "xz" | "yz", offset: number): Plane {
  const p = BASE_PLANES[base];
  return { ...p, origin: [p.normal[0] * offset, p.normal[1] * offset, p.normal[2] * offset] };
}

function nextFeatureId(doc: CadDocument): number {
  const max = doc.features.reduce((m, f) => Math.max(m, f.id + 1), 0);
  return Math.max(max, doc.next_id ?? 0);
}

const PRIMITIVE_LABELS: Record<PrimitiveShape["type"], string> = {
  box: "Caja",
  cylinder: "Cilindro",
  cone: "Cono",
  sphere: "Esfera",
  torus: "Toro",
  bolt: "Tornillo",
  nut: "Tuerca",
  washer: "Arandela",
};

/** Operación que se edita con el editor de sketch (en un plano, o envuelto sobre una cara) */
export function editsAsSketch(kind: FeatureKind): kind is Extract<FeatureKind, { type: "sketch" | "surface_sketch" }> {
  return kind.type === "sketch" || kind.type === "surface_sketch";
}

/** Nombre del tipo de operación (las primitivas, por su forma) */
export function kindLabel(kind: FeatureKind): string {
  return kind.type === "primitive" ? PRIMITIVE_LABELS[kind.shape.type] : FEATURE_LABELS[kind.type];
}

function defaultName(doc: CadDocument, kind: FeatureKind): string {
  const label = kindLabel(kind);
  const n = doc.features.filter((f) => kindLabel(f.kind) === label).length + 1;
  return `${label} ${n}`;
}

/** Operaciones de las que depende `kind` (mismo criterio que el backend) */
export function dependencies(kind: FeatureKind): number[] {
  const axis = (a: AxisSpec) => (a.type === "sketch_line" ? [a.sketch] : a.type === "reference" ? [a.feature] : []);
  const plane = (p: PlaneSpec) => (p.type === "reference" ? [p.feature] : []);
  const point = (p: PointSpec) => (p.type === "reference" ? [p.feature] : []);
  switch (kind.type) {
    case "sketch":
      return [
        ...plane(kind.plane),
        ...(kind.sketch.x_axis ? axis(kind.sketch.x_axis) : []),
        ...kind.sketch.constraints.flatMap((c) => (c.type === "pierce" ? [c.curve] : [])),
      ];
    case "sketch3d":
      return kind.sketch.constraints.flatMap((c) => (c.type === "attach" ? point(c.target) : c.type === "on_plane" ? plane(c.plane) : []));
    case "draft":
      return plane(kind.neutral);
    case "plane": {
      const d = kind.def;
      if (d.type === "offset") return plane(d.base);
      if (d.type === "angle") return [...plane(d.base), ...axis(d.axis)];
      if (d.type === "midplane") return [...plane(d.a), ...plane(d.b)];
      return d.points.flatMap(point);
    }
    case "axis": {
      const d = kind.def;
      if (d.type === "two_points") return [...point(d.a), ...point(d.b)];
      if (d.type === "planes") return [...plane(d.a), ...plane(d.b)];
      return [];
    }
    case "point":
      return point(kind.def);
    case "sweep":
      return [kind.sketch, kind.path.type === "sketch" ? kind.path.sketch : kind.path.feature];
    case "helix":
      return axis(kind.axis);
    case "loft":
      return kind.sections.map((x) => x.sketch);
    case "hole":
      return [kind.sketch];
    case "extrude":
    case "sheet_metal":
      return [kind.sketch];
    case "revolve":
      return [kind.sketch, ...axis(kind.axis)];
    case "pattern": {
      const p = kind.pattern;
      const extra = p.type === "circular" ? axis(p.axis) : p.type === "curve" ? [p.path.type === "sketch" ? p.path.sketch : p.path.feature] : [];
      return [...kind.features, ...extra];
    }
    case "scale":
      return point(kind.center);
    case "mirror":
      return [...kind.features, ...plane(kind.plane)];
    case "boolean":
      return [...kind.targets, ...kind.tools].map((p) => p.feature);
    case "split_parts":
    case "delete_parts":
      return kind.parts.map((p) => p.feature);
    default:
      return [];
  }
}

/** Dependencias de una operación, incluidas las piezas de su alcance */
export function featureDeps(f: Feature): number[] {
  return [...dependencies(f.kind), ...(f.scope ?? []).map((p) => p.feature)];
}

export function emptySketch(): Sketch {
  return { points: [{ id: 0, x: 0, y: 0 }], entities: [], constraints: [], next_id: 1, origin: 0 };
}

/** Agrega el punto origen a un sketch que no lo tiene (los de antes) */
export function ensureOrigin(s: Sketch): number {
  if (s.origin !== undefined && s.points.some((p) => p.id === s.origin)) return s.origin;
  s.origin = addPoint(s, [0, 0]);
  return s.origin;
}

/** Id nuevo dentro de un sketch (puntos y entidades comparten numeración) */
export function sketchId(s: Sketch): number {
  const max = Math.max(-1, ...s.points.map((p) => p.id), ...s.entities.map((e) => e.id));
  const id = Math.max(max + 1, s.next_id ?? 0);
  s.next_id = id + 1;
  return id;
}

export function addPoint(s: Sketch, p: P2): number {
  const id = sketchId(s);
  s.points.push({ id, x: p[0], y: p[1] });
  return id;
}

export function addEntity(s: Sketch, geometry: Geometry): number {
  const id = sketchId(s);
  s.entities.push({ id, geometry });
  return id;
}

/** Rectángulo con esquinas compartidas y restricciones horizontal/vertical */
export function addRectangle(s: Sketch, a: P2, b: P2): number[] {
  const [x0, x1] = [Math.min(a[0], b[0]), Math.max(a[0], b[0])];
  const [y0, y1] = [Math.min(a[1], b[1]), Math.max(a[1], b[1])];
  const p = [addPoint(s, [x0, y0]), addPoint(s, [x1, y0]), addPoint(s, [x1, y1]), addPoint(s, [x0, y1])];
  const l = [0, 1, 2, 3].map((i) => addEntity(s, { type: "line", start: p[i], end: p[(i + 1) % 4] }));
  s.constraints.push({ type: "horizontal", line: l[0] }, { type: "horizontal", line: l[2] });
  s.constraints.push({ type: "vertical", line: l[1] }, { type: "vertical", line: l[3] });
  return l;
}

/** Contorno de una elipse de centro `c` y semiejes hasta `a` y `b` */
export function ellipsePolyline(c: P2, a: P2, b: P2, n = 64): P2[] {
  const u: P2 = [a[0] - c[0], a[1] - c[1]];
  const v: P2 = [b[0] - c[0], b[1] - c[1]];
  return Array.from({ length: n + 1 }, (_, i) => {
    const t = (i / n) * 2 * Math.PI;
    return [c[0] + u[0] * Math.cos(t) + v[0] * Math.sin(t), c[1] + u[1] * Math.cos(t) + v[1] * Math.sin(t)] as P2;
  });
}

/**
 * Tangentes de la spline por puntos en cada punto (Catmull-Rom): `t0`/`t1`
 * son las direcciones de las manijas de las puntas y `dirs[i]` la de una
 * manija en el punto `i` (el largo queda el de Catmull-Rom).
 */
export function splineTangents(p: P2[], closed: boolean, t0?: P2, t1?: P2, dirs?: (P2 | undefined)[]): P2[] {
  const n = p.length;
  const sub = (a: P2, b: P2): P2 => [a[0] - b[0], a[1] - b[1]];
  const len = (v: P2) => Math.hypot(v[0], v[1]) || 1;
  const along = (dir: P2, ref: P2): P2 => [(dir[0] / len(dir)) * len(ref), (dir[1] / len(dir)) * len(ref)];
  const at = (i: number) => p[((i % n) + n) % n];
  return p.map((_, i) => {
    let base: P2;
    if (closed) base = sub(at(i + 1), at(i - 1)).map((v) => v / 2) as P2;
    else if (i === 0) base = t0 ? along(t0, sub(p[1], p[0])) : sub(p[1], p[0]);
    else if (i === n - 1) base = t1 ? along(t1, sub(p[n - 1], p[n - 2])) : sub(p[n - 1], p[n - 2]);
    else base = sub(p[i + 1], p[i - 1]).map((v) => v / 2) as P2;
    const d = dirs?.[i];
    return d && Math.hypot(d[0], d[1]) > 1e-12 ? along(d, base) : base;
  });
}

/**
 * Curva suave por los puntos (Catmull-Rom, como se ve antes de recalcular).
 * `t0`/`t1`: dirección de salida y de llegada impuestas por las manijas;
 * `dirs`: direcciones en puntos intermedios.
 */
export function splinePolyline(p: P2[], closed: boolean, t0?: P2, t1?: P2, steps = 16, dirs?: (P2 | undefined)[]): P2[] {
  const n = p.length;
  if (n < 2) return p.slice();
  const tg = splineTangents(p, closed, t0, t1, dirs);
  const at = (i: number) => p[((i % n) + n) % n];
  const out: P2[] = [];
  const segs = closed ? n : n - 1;
  for (let i = 0; i < segs; i++) {
    const [a, b, ma, mb] = [at(i), at(i + 1), tg[i], tg[(i + 1) % n]];
    for (let k = 0; k < steps; k++) {
      const t = k / steps;
      const [h00, h10, h01, h11] = [2 * t ** 3 - 3 * t ** 2 + 1, t ** 3 - 2 * t ** 2 + t, -2 * t ** 3 + 3 * t ** 2, t ** 3 - t ** 2];
      out.push([h00 * a[0] + h10 * ma[0] + h01 * b[0] + h11 * mb[0], h00 * a[1] + h10 * ma[1] + h01 * b[1] + h11 * mb[1]]);
    }
  }
  out.push(closed ? p[0] : p[n - 1]);
  return out;
}

/** Puntos, tangentes de las puntas y direcciones intermedias de una spline del sketch */
export function splineParts(
  g: Extract<Geometry, { type: "spline" }>,
  point: (id: number) => P2 | undefined,
): { p: P2[]; t0?: P2; t1?: P2; dirs: (P2 | undefined)[] } | undefined {
  const pts = g.points.map(point);
  if (pts.some((p) => !p)) return undefined;
  const p = pts as P2[];
  const h0 = g.start_handle !== undefined ? point(g.start_handle) : undefined;
  const h1 = g.end_handle !== undefined ? point(g.end_handle) : undefined;
  const t0: P2 | undefined = h0 && [h0[0] - p[0][0], h0[1] - p[0][1]];
  const t1: P2 | undefined = h1 && [h1[0] - p[p.length - 1][0], h1[1] - p[p.length - 1][1]];
  const dirs: (P2 | undefined)[] = p.map(() => undefined);
  for (const [i, h] of g.handles ?? []) {
    const q = point(h);
    if (q && p[i]) dirs[i] = [q[0] - p[i][0], q[1] - p[i][1]];
  }
  return { p, t0, t1, dirs };
}

/** Contorno de una spline del sketch (con sus manijas) */
export function splineOf(g: Extract<Geometry, { type: "spline" }>, point: (id: number) => P2 | undefined): P2[] | undefined {
  const sp = splineParts(g, point);
  return sp && splinePolyline(sp.p, g.closed, sp.t0, sp.t1, 16, sp.dirs);
}

/** B-spline del sketch lista para evaluar */
export function bsplineOf(g: Extract<Geometry, { type: "bspline" }>, point: (id: number) => P2 | undefined): BSpline | undefined {
  const poles = g.poles.map(point);
  if (poles.some((p) => !p)) return undefined;
  return makeBSpline({ poles: poles as P2[], degree: g.degree, closed: g.closed, weights: g.weights, knots: g.knots });
}

/** Polilínea de un arco de elipse o una B-spline (las curvas de `sketchCurves`) */
export function curvePolyline(g: Geometry, point: (id: number) => P2 | undefined): P2[] | undefined {
  if (g.type === "bspline") {
    const s = bsplineOf(g, point);
    if (!s) return undefined;
    const spans = breaks(s).length - 1;
    const pl = sample(s, Math.max(16, Math.ceil(64 / Math.max(1, spans))));
    // Abierta: las puntas exactas en el primer y el último polo (para unirse con lo que comparte el punto)
    if (!g.closed) [pl[0], pl[pl.length - 1]] = [point(g.poles[0])!, point(g.poles[g.poles.length - 1])!];
    return pl;
  }
  if (g.type === "ellipse_arc") {
    const [c, a, b, p, q] = [point(g.center), point(g.major), point(g.minor), point(g.start), point(g.end)];
    if (!c || !a || !b || !p || !q) return undefined;
    const pl = ellipseArcPolyline(c, a, b, p, q);
    [pl[0], pl[pl.length - 1]] = [p, q];
    return pl;
  }
  return undefined;
}

/** Manijas en los extremos de una spline abierta (o las quita si ya tiene) */
export function toggleSplineHandles(s: Sketch, id: number): string | undefined {
  const e = s.entities.find((x) => x.id === id);
  if (e?.geometry.type !== "spline") return "Elegir una spline";
  const g = e.geometry;
  if (g.closed) return "Las manijas son para splines abiertas";
  if (g.start_handle !== undefined || g.end_handle !== undefined) {
    const hs = [g.start_handle, g.end_handle].filter((x): x is number => x !== undefined);
    delete g.start_handle;
    delete g.end_handle;
    s.points = s.points.filter((q) => !hs.includes(q.id) || s.entities.some((x) => geometryPoints(x.geometry).includes(q.id)));
    s.constraints = s.constraints.filter((k) => !constraintIds(k).some((v) => hs.includes(v)));
    return undefined;
  }
  const p = g.points.map((q) => pointOf(s, q));
  const n = p.length;
  // A un tercio del tramo vecino, en el sentido de avance
  g.start_handle = addPoint(s, [p[0][0] + (p[1][0] - p[0][0]) / 3, p[0][1] + (p[1][1] - p[0][1]) / 3]);
  g.end_handle = addPoint(s, [p[n - 1][0] + (p[n - 1][0] - p[n - 2][0]) / 3, p[n - 1][1] + (p[n - 1][1] - p[n - 2][1]) / 3]);
  return undefined;
}

/**
 * Agrega al sketch los contornos de un texto (ver `sketchText.outlineContours`):
 * los tramos de un mismo contorno comparten sus extremos. Devuelve las
 * entidades creadas.
 */
export function addTextContours(s: Sketch, contours: Contour[]): number[] {
  const out: number[] = [];
  for (const c of contours) {
    if ("closed" in c) {
      if (c.closed.length < 3) continue;
      out.push(addEntity(s, { type: "spline", points: c.closed.map((p) => addPoint(s, p)), closed: true }));
      continue;
    }
    const ids = new Map<string, number>();
    const at = (p: P2) => {
      const k = `${p[0].toFixed(9)},${p[1].toFixed(9)}`;
      if (!ids.has(k)) ids.set(k, addPoint(s, p));
      return ids.get(k)!;
    };
    for (const piece of c.pieces) {
      if (piece.kind === "line") out.push(addEntity(s, { type: "line", start: at(piece.a), end: at(piece.b) }));
      else {
        const pts = piece.points;
        const ids2 = pts.map((p, i) => (i === 0 || i === pts.length - 1 ? at(p) : addPoint(s, p)));
        out.push(addEntity(s, { type: "spline", points: ids2, closed: false }));
      }
    }
  }
  return out;
}

/** Inserta un texto (sus contornos ya armados en `at`) como bloque rígido con ancla en `at` */
export function addText(s: Sketch, contours: Contour[], at: P2, meta: { text: string; size: number; font: string; style?: TextStyle }, anchor?: number, id?: number): SketchText {
  const ents = addTextContours(s, contours);
  const used = new Set(ents.flatMap((e) => geometryPoints(s.entities.find((x) => x.id === e)!.geometry)));
  const t: SketchText = { id: id ?? sketchId(s), ...meta, anchor: anchor ?? addPoint(s, at), entities: ents, points: [...used] };
  s.texts = [...(s.texts ?? []), t];
  return t;
}

/** El texto al que pertenece una entidad o un punto (su ancla incluida) */
export function textOf(s: Sketch, id: number): SketchText | undefined {
  return s.texts?.find((t) => t.anchor === id || t.entities.includes(id) || t.points.includes(id));
}

/** Saca un texto entero: sus curvas, sus puntos y (salvo `keepAnchor`) el ancla */
export function removeText(s: Sketch, t: SketchText, keepAnchor = false): void {
  s.texts = (s.texts ?? []).filter((x) => x.id !== t.id);
  for (const e of t.entities) removeEntity(s, e);
  if (!keepAnchor) {
    s.points = s.points.filter((p) => p.id !== t.anchor);
    s.constraints = s.constraints.filter((c) => !constraintMentions(c, [t.anchor]));
  }
}

/** Puntos que usa una geometría */
export function geometryPoints(g: Geometry): number[] {
  switch (g.type) {
    case "line":
      return [g.start, g.end];
    case "circle":
      return [g.center];
    case "arc":
      return [g.center, g.start, g.end];
    case "spline":
      return [
        ...g.points,
        ...(g.start_handle !== undefined ? [g.start_handle] : []),
        ...(g.end_handle !== undefined ? [g.end_handle] : []),
        ...(g.handles ?? []).map((h) => h[1]),
      ];
    case "point":
      return [g.point];
    case "ellipse":
      return [g.center, g.major, g.minor];
    case "ellipse_arc":
      return [g.center, g.major, g.minor, g.start, g.end];
    case "bspline":
      return [...g.poles];
  }
}

/**
 * Copia de la geometría con sus puntos pasados por `map`; `flip` da vuelta el
 * sentido de los arcos (una simetría lo invierte).
 */
export function mapGeometry(g: Geometry, map: (p: number) => number, flip = false): Geometry {
  switch (g.type) {
    case "line":
      return { type: "line", start: map(g.start), end: map(g.end) };
    case "circle":
      return { type: "circle", center: map(g.center), radius: g.radius };
    case "arc":
      return flip ? { type: "arc", center: map(g.center), start: map(g.end), end: map(g.start) } : { type: "arc", center: map(g.center), start: map(g.start), end: map(g.end) };
    case "spline":
      return {
        type: "spline",
        points: g.points.map(map),
        closed: g.closed,
        ...(g.start_handle !== undefined ? { start_handle: map(g.start_handle) } : {}),
        ...(g.end_handle !== undefined ? { end_handle: map(g.end_handle) } : {}),
        ...(g.handles?.length ? { handles: g.handles.map(([i, h]) => [i, map(h)] as [number, number]) } : {}),
      };
    case "ellipse":
      return { type: "ellipse", center: map(g.center), major: map(g.major), minor: map(g.minor) };
    case "ellipse_arc": {
      const [a, b] = flip ? [g.end, g.start] : [g.start, g.end];
      return { type: "ellipse_arc", center: map(g.center), major: map(g.major), minor: map(g.minor), start: map(a), end: map(b) };
    }
    case "bspline":
      return { ...structuredClone(g), poles: g.poles.map(map) };
    case "point":
      return { type: "point", point: map(g.point) };
  }
}

/**
 * Copia de las entidades con sus puntos pasados por `map` (los que devuelve
 * igual se comparten). Los círculos copiados quedan con el mismo radio;
 * `flip` invierte los arcos (la simetría cambia el sentido).
 */
function copyEntities(s: Sketch, ids: number[], map: (p: number) => number, flip = false): number[] {
  const out: number[] = [];
  for (const id of ids) {
    const e = s.entities.find((x) => x.id === id);
    if (!e) continue;
    const g = e.geometry;
    const c = addEntity(s, mapGeometry(g, map, flip));
    if (e.construction) s.entities.find((x) => x.id === c)!.construction = true;
    if (g.type === "circle") s.constraints.push({ type: "equal", a: id, b: c });
    out.push(c);
  }
  return out;
}

/** Puntos de las entidades, sin repetir, en orden */
function pointsOfEntities(s: Sketch, ids: number[]): number[] {
  const out: number[] = [];
  for (const id of ids) {
    const e = s.entities.find((x) => x.id === id);
    for (const p of e ? geometryPoints(e.geometry) : []) if (!out.includes(p)) out.push(p);
  }
  return out;
}

/**
 * Simetría: copia las entidades reflejadas respecto de la línea `axis`. Cada
 * punto copiado queda simétrico de su original; los que están sobre el eje
 * se comparten. Devuelve un mensaje si no se puede.
 */
export function mirrorEntities(s: Sketch, ids: number[], axis: number): string | undefined {
  const ax = s.entities.find((e) => e.id === axis)?.geometry;
  if (ax?.type !== "line") return "El eje de la simetría tiene que ser una línea";
  const [A, B] = [pointOf(s, ax.start), pointOf(s, ax.end)];
  const d: P2 = [B[0] - A[0], B[1] - A[1]];
  const l2 = d[0] * d[0] + d[1] * d[1];
  if (l2 === 0) return "El eje no tiene largo";
  const items = ids.filter((id) => id !== axis);
  if (!items.length) return "Elegir también lo que se refleja";
  const tol = 1e-9 * Math.max(1, Math.sqrt(l2));
  const made = new Map<number, number>();
  const map = (p: number): number => {
    if (made.has(p)) return made.get(p)!;
    const P = pointOf(s, p);
    const t = ((P[0] - A[0]) * d[0] + (P[1] - A[1]) * d[1]) / l2;
    const foot: P2 = [A[0] + t * d[0], A[1] + t * d[1]];
    if (Math.hypot(P[0] - foot[0], P[1] - foot[1]) <= tol) {
      made.set(p, p);
      return p;
    }
    // Ya reflejado antes (la simetría dinámica refleja de a un tramo): el mismo punto
    const twin = s.constraints.find((k) => k.type === "symmetric" && k.line === axis && (k.a === p || k.b === p)) as { a: number; b: number } | undefined;
    if (twin) {
      const q = twin.a === p ? twin.b : twin.a;
      made.set(p, q);
      return q;
    }
    const q = addPoint(s, [2 * foot[0] - P[0], 2 * foot[1] - P[1]]);
    s.constraints.push({ type: "symmetric", a: p, b: q, line: axis });
    made.set(p, q);
    return q;
  };
  copyEntities(s, items, map, true);
  return undefined;
}

/** Marca una entidad como de construcción */
function construction(s: Sketch, id: number): number {
  s.entities.find((x) => x.id === id)!.construction = true;
  return id;
}

/** Punto de construcción (entidad punto) que guía un patrón */
function guidePoint(s: Sketch, p: P2): number {
  const q = addPoint(s, p);
  construction(s, addEntity(s, { type: "point", point: q }));
  return q;
}

/** Una copia de las entidades con sus puntos llevados por `place`; devuelve original → copia */
function placeCopy(s: Sketch, ids: number[], pts: number[], place: (p: number) => P2, keep: number[] = []): Map<number, number> {
  const map = new Map<number, number>();
  for (const p of pts) map.set(p, keep.includes(p) ? p : addPoint(s, place(p)));
  copyEntities(s, ids, (p) => map.get(p) ?? p);
  return map;
}

/** Los demás puntos de la copia guardan la forma del original: mismo desplazamiento que `lead` */
function keepShape(s: Sketch, pts: number[], lead: number, map: Map<number, number>): void {
  for (const p of pts) if (p !== lead) s.constraints.push({ type: "equal_offset", a1: lead, a2: p, b1: map.get(lead)!, b2: map.get(p)! });
}

/**
 * Patrón lineal: `count` en total por fila (el original y count − 1 copias)
 * corridas de a `offset`; con `rows` > 1, filas corridas de a `offset2`. Solo
 * el primer par de cada dirección lleva cotas (distancia horizontal y
 * vertical); el resto sigue con "mismo desplazamiento".
 */
export function linearPattern(s: Sketch, ids: number[], count: number, offset: P2, rows = 1, offset2: P2 = [0, 0]): string | undefined {
  const pts = pointsOfEntities(s, ids);
  if (!pts.length || count < 1 || rows < 1 || count * rows < 2) return "Elegir qué repetir y al menos 2 en total";
  const grid = new Map<string, Map<number, number>>([["0,0", new Map(pts.map((p) => [p, p]))]]);
  const masters: [[number, number] | undefined, [number, number] | undefined] = [undefined, undefined];
  for (let j = 0; j < rows; j++)
    for (let k = 0; k < count; k++) {
      if (!j && !k) continue;
      const d: P2 = [k * offset[0] + j * offset2[0], k * offset[1] + j * offset2[1]];
      const map = placeCopy(s, ids, pts, (p) => {
        const P = pointOf(s, p);
        return [P[0] + d[0], P[1] + d[1]];
      });
      // Cada copia sigue a la anterior de su fila (o a la de la fila anterior si es la primera)
      const dir = k > 0 ? 0 : 1;
      const parent = grid.get(k > 0 ? `${k - 1},${j}` : `${k},${j - 1}`)!;
      for (const p of pts) {
        const [from, q] = [parent.get(p)!, map.get(p)!];
        const m = masters[dir];
        if (m) s.constraints.push({ type: "equal_offset", a1: m[0], a2: m[1], b1: from, b2: q });
        else {
          const off = dir ? offset2 : offset;
          masters[dir] = [from, q];
          s.constraints.push({ type: "horizontal_distance", a: from, b: q, value: off[0] }, { type: "vertical_distance", a: from, b: q, value: off[1] });
        }
      }
      grid.set(`${k},${j}`, map);
    }
  return undefined;
}

/**
 * Patrón circular alrededor del punto `center`: `count` en total repartidos
 * en `span` grados (360: la vuelta entera; menos: de punta a punta; negativo:
 * horario). El primer par lleva radios iguales y una cota de ángulo (entre
 * dos líneas de construcción); el resto sigue con "mismo giro". Sin `rotate`
 * las copias se trasladan sin girar.
 */
export function circularPattern(s: Sketch, ids: number[], count: number, center: number, span = 360, rotate = true): string | undefined {
  const pts = pointsOfEntities(s, ids);
  const lead = pts.find((p) => p !== center);
  if (lead === undefined || count < 2) return "Elegir qué repetir y al menos 2 en total";
  if (span === 0) return "El ángulo no puede ser 0";
  const full = Math.abs(span) >= 360 - 1e-9;
  const step = ((full ? Math.sign(span) * 360 : span) / (full ? count : count - 1)) * (Math.PI / 180);
  const half = Math.abs(Math.abs(step) - Math.PI) < 1e-9;
  if (!half && Math.abs(step) > Math.PI) return "Entre una copia y la siguiente tiene que haber menos de 180°";
  const C = pointOf(s, center);
  const rot = (P: P2, a: number): P2 => {
    const [x, y] = [P[0] - C[0], P[1] - C[1]];
    return [C[0] + x * Math.cos(a) - y * Math.sin(a), C[1] + x * Math.sin(a) + y * Math.cos(a)];
  };
  const L = pointOf(s, lead);
  let prev = new Map(pts.map((p) => [p, p]));
  let master: [number, number] | undefined;
  for (let k = 1; k < count; k++) {
    // Girando, cada punto da la vuelta; sin girar, todos siguen al principal
    const shift: P2 = [rot(L, k * step)[0] - L[0], rot(L, k * step)[1] - L[1]];
    const next = placeCopy(
      s,
      ids,
      pts,
      (p) => (rotate ? rot(pointOf(s, p), k * step) : [pointOf(s, p)[0] + shift[0], pointOf(s, p)[1] + shift[1]]),
      rotate ? [center] : [],
    );
    if (!master) {
      master = [lead, next.get(lead)!];
      if (half) {
        // Media vuelta: el centro es el punto medio (un ángulo de 180° es inestable en el solver)
        const d = construction(s, addEntity(s, { type: "line", start: master[0], end: master[1] }));
        s.constraints.push({ type: "midpoint", point: center, line: d });
      } else {
        const r0 = construction(s, addEntity(s, { type: "line", start: center, end: master[0] }));
        const r1 = construction(s, addEntity(s, { type: "line", start: center, end: master[1] }));
        s.constraints.push({ type: "equal", a: r0, b: r1 }, { type: "angle", a: r0, b: r1, degrees: +((step * 180) / Math.PI).toFixed(6) });
      }
    }
    for (const p of rotate ? pts : [lead]) {
      if (p === center || (k === 1 && p === lead)) continue;
      s.constraints.push({ type: "equal_rotation", center, a1: master[0], a2: master[1], b1: prev.get(p)!, b2: next.get(p)! });
    }
    if (!rotate) keepShape(s, pts, lead, next);
    prev = next;
  }
  return undefined;
}

/** Polilínea de una entidad del sketch (para recorrerla o medirla) */
function polylineOf(s: Sketch, g: Geometry): P2[] {
  const at = (id: number) => pointOf(s, id);
  if (g.type === "line") return [at(g.start), at(g.end)];
  if (g.type === "point") return [at(g.point)];
  if (g.type === "ellipse") return ellipsePolyline(at(g.center), at(g.major), at(g.minor));
  if (g.type === "spline") return splineOf(g, (id) => s.points.find((p) => p.id === id) && at(id)) ?? [];
  if (g.type === "bspline" || g.type === "ellipse_arc") return curvePolyline(g, (id) => s.points.find((p) => p.id === id) && at(id)) ?? [];
  const c = circleOf(s, g)!;
  const [a0, sweep] = [c.a0 ?? 0, c.sweep ?? 2 * Math.PI];
  const n = Math.max(8, Math.ceil((sweep / (2 * Math.PI)) * 64));
  return Array.from({ length: n + 1 }, (_, i) => [c.c[0] + c.r * Math.cos(a0 + (sweep * i) / n), c.c[1] + c.r * Math.sin(a0 + (sweep * i) / n)] as P2);
}

/** Puntos a distancias iguales (por largo) sobre una polilínea; cerrada: sin repetir la punta */
function alongPolyline(poly: P2[], count: number, closed: boolean): { at: P2; dir: number }[] {
  const lens = [0];
  for (let i = 1; i < poly.length; i++) lens.push(lens[i - 1] + Math.hypot(poly[i][0] - poly[i - 1][0], poly[i][1] - poly[i - 1][1]));
  const total = lens[lens.length - 1];
  const out: { at: P2; dir: number }[] = [];
  for (let k = 0; k < count; k++) {
    const want = (total * k) / (closed ? count : count - 1);
    let i = 1;
    while (i < poly.length - 1 && lens[i] < want) i++;
    const [a, b] = [poly[i - 1], poly[i]];
    const t = lens[i] > lens[i - 1] ? (want - lens[i - 1]) / (lens[i] - lens[i - 1]) : 0;
    out.push({ at: [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])], dir: Math.atan2(b[1] - a[1], b[0] - a[0]) });
  }
  return out;
}

/**
 * Patrón en curva: `count` en total repartidos de punta a punta de la
 * entidad `path` (en un círculo o curva cerrada, en la vuelta). Cada copia se
 * traslada lo que avanza el camino desde su comienzo; con `rotate` además
 * gira con él. En líneas, arcos y círculos las copias quedan atadas al
 * camino; en splines y elipses quedan donde caen.
 */
export function curvePattern(s: Sketch, ids: number[], count: number, path: number, rotate = false): string | undefined {
  const g = s.entities.find((e) => e.id === path)?.geometry;
  if (!g || g.type === "point") return "El camino tiene que ser una línea, arco, círculo, spline o elipse";
  const items = ids.filter((id) => id !== path);
  const pts = pointsOfEntities(s, items);
  if (!pts.length || count < 2) return "Elegir el camino primero, después qué repetir, y al menos 2 en total";
  const lead = pts[0];
  if (g.type === "circle" && rotate) return circularPattern(s, items, count, g.center);
  const move = (map: (P: P2) => P2) => (p: number) => map(pointOf(s, p));
  if (g.type === "line") {
    // Pasos iguales y el último en la punta: "mismo desplazamiento" que la línea entera
    const [A, B] = [pointOf(s, g.start), pointOf(s, g.end)];
    let [prev, first] = [new Map(pts.map((p) => [p, p])), undefined as number | undefined];
    for (let k = 1; k < count; k++) {
      const t = k / (count - 1);
      const next = placeCopy(s, items, pts, move((P) => [P[0] + t * (B[0] - A[0]), P[1] + t * (B[1] - A[1])]));
      if (first === undefined) first = next.get(lead)!;
      else s.constraints.push({ type: "equal_offset", a1: lead, a2: first, b1: prev.get(lead)!, b2: next.get(lead)! });
      if (k === count - 1) s.constraints.push({ type: "equal_offset", a1: g.start, a2: g.end, b1: lead, b2: next.get(lead)! });
      keepShape(s, pts, lead, next);
      prev = next;
    }
    return undefined;
  }
  if (g.type === "arc" || g.type === "circle") {
    // Guías sobre el camino a giros iguales; las copias siguen a su guía
    const { c: C, r, a0: arcStart, sweep: arcSweep } = circleOf(s, g)!;
    const L = pointOf(s, lead);
    const a0 = arcStart ?? (Math.hypot(L[0] - C[0], L[1] - C[1]) > 1e-9 ? Math.atan2(L[1] - C[1], L[0] - C[0]) : 0);
    const step = arcSweep !== undefined ? arcSweep / (count - 1) : (2 * Math.PI) / count;
    const on = (a: number): P2 => [C[0] + r * Math.cos(a), C[1] + r * Math.sin(a)];
    let start: number;
    if (g.type === "arc") start = g.start;
    else {
      // Comienzo en el círculo, en el rayo que va del centro al punto principal
      start = guidePoint(s, on(a0));
      s.constraints.push({ type: "point_on_circle", point: start, circle: path });
      if (Math.hypot(L[0] - C[0], L[1] - C[1]) > 1e-9) {
        const ray = construction(s, addEntity(s, { type: "line", start: g.center, end: start }));
        s.constraints.push({ type: "point_on_line", point: lead, line: ray });
      } else s.constraints.push({ type: "horizontal_points", a: g.center, b: start });
    }
    const guides = [start];
    for (let k = 1; k < count; k++) guides.push(g.type === "arc" && k === count - 1 ? g.end : guidePoint(s, on(a0 + k * step)));
    for (let k = 2; k < count; k++) s.constraints.push({ type: "equal_rotation", center: g.center, a1: start, a2: guides[1], b1: guides[k - 1], b2: guides[k] });
    // Cerrar la vuelta: la última guía vuelve al comienzo con el mismo giro
    if (g.type === "circle") s.constraints.push({ type: "equal_rotation", center: g.center, a1: start, a2: guides[1], b1: guides[count - 1], b2: start });
    for (let k = 1; k < count; k++) {
      const a = k * step;
      const S = on(a0);
      const next = placeCopy(
        s,
        items,
        pts,
        move((P) => {
          if (!rotate) return [P[0] + on(a0 + a)[0] - S[0], P[1] + on(a0 + a)[1] - S[1]];
          const [x, y] = [P[0] - C[0], P[1] - C[1]];
          return [C[0] + x * Math.cos(a) - y * Math.sin(a), C[1] + x * Math.sin(a) + y * Math.cos(a)];
        }),
      );
      for (const p of pts)
        s.constraints.push(
          rotate
            ? { type: "equal_rotation", center: g.center, a1: start, a2: guides[k], b1: p, b2: next.get(p)! }
            : { type: "equal_offset", a1: start, a2: guides[k], b1: p, b2: next.get(p)! },
        );
    }
    return undefined;
  }
  // Spline o elipse: sin restricción "sobre la curva", las copias guardan la forma y nada más
  const closed = g.type === "ellipse" || (g.type === "spline" && g.closed);
  const stops = alongPolyline(polylineOf(s, g), count, closed);
  if (stops.length < 2) return "El camino no tiene largo";
  const [S, d0] = [stops[0].at, stops[0].dir];
  for (const { at, dir } of stops.slice(1)) {
    const a = rotate ? dir - d0 : 0;
    const next = placeCopy(
      s,
      items,
      pts,
      move((P) => {
        const [x, y] = [P[0] - S[0], P[1] - S[1]];
        return [at[0] + x * Math.cos(a) - y * Math.sin(a), at[1] + x * Math.sin(a) + y * Math.cos(a)];
      }),
    );
    if (!rotate) keepShape(s, pts, lead, next);
  }
  return undefined;
}

/**
 * Patrón por tabla: una copia por desplazamiento (mm, relativo al original).
 * Cada copia lleva sus cotas horizontal y vertical desde el original.
 */
export function tablePattern(s: Sketch, ids: number[], offsets: P2[]): string | undefined {
  const pts = pointsOfEntities(s, ids);
  if (!pts.length || !offsets.length) return "Elegir qué repetir y al menos una fila en la tabla";
  const lead = pts[0];
  for (const d of offsets) {
    const next = placeCopy(s, ids, pts, (p) => [pointOf(s, p)[0] + d[0], pointOf(s, p)[1] + d[1]]);
    const q = next.get(lead)!;
    s.constraints.push({ type: "horizontal_distance", a: lead, b: q, value: d[0] }, { type: "vertical_distance", a: lead, b: q, value: d[1] });
    keepShape(s, pts, lead, next);
  }
  return undefined;
}

function inPolygon(p: P2, poly: P2[]): boolean {
  let inside = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const [a, b] = [poly[i], poly[j]];
    if (a[1] > p[1] !== b[1] > p[1] && p[0] < ((b[0] - a[0]) * (p[1] - a[1])) / (b[1] - a[1]) + a[0]) inside = !inside;
  }
  return inside;
}

/** Distancia de `p` al segmento ab */
function segmentDistance(p: P2, a: P2, b: P2): number {
  const [dx, dy] = [b[0] - a[0], b[1] - a[1]];
  const l2 = dx * dx + dy * dy;
  const t = l2 > 0 ? Math.min(1, Math.max(0, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2)) : 0;
  return Math.hypot(p[0] - a[0] - t * dx, p[1] - a[1] - t * dy);
}

/** Máximo de copias de un patrón de relleno (con más, cada recálculo del sketch pasa del medio segundo) */
const FILL_MAX = 120;

/**
 * Patrón de relleno: copias en grilla (cuadrada o hexagonal) a `spacing` que
 * entran enteras en `region`, con todo a no menos de `margin` del borde. La
 * grilla pasa por el original; su paso es una cota sobre líneas de
 * construcción y cada copia sigue a su vecina con "mismo desplazamiento".
 */
export function fillPattern(s: Sketch, ids: number[], region: Region, spacing: number, hex: boolean, margin: number): string | undefined {
  const pts = pointsOfEntities(s, ids);
  if (!pts.length) return "Elegir la región primero y después qué repetir";
  if (!(spacing > 0)) return "La separación tiene que ser mayor que cero";
  const lead = pts[0];
  const L = pointOf(s, lead);
  // Contorno de lo que se repite, para ver si cada copia entra
  const samples: { p: P2; r: number }[] = [];
  for (const id of ids) {
    const g = s.entities.find((e) => e.id === id)?.geometry;
    if (!g) continue;
    if (g.type === "circle") samples.push({ p: pointOf(s, g.center), r: g.radius });
    else for (const p of polylineOf(s, g)) samples.push({ p, r: 0 });
  }
  const loops = [region.outer, ...region.holes].map((l) => l.polygon);
  const fits = (d: P2) =>
    samples.every(({ p, r }) => {
      const q: P2 = [p[0] + d[0], p[1] + d[1]];
      if (!inPolygon(q, region.outer.polygon) || region.holes.some((h) => inPolygon(q, h.polygon))) return false;
      return loops.every((poly) => poly.every((a, i) => segmentDistance(q, a, poly[(i + 1) % poly.length]) >= r + margin - 1e-9));
    });
  // Base de la grilla: A en x, B arriba (corrida medio paso si es hexagonal)
  const A: P2 = [spacing, 0];
  const B: P2 = hex ? [spacing / 2, (spacing * Math.sqrt(3)) / 2] : [0, spacing];
  const xs = region.outer.polygon.map((p) => p[0] - L[0]);
  const ys = region.outer.polygon.map((p) => p[1] - L[1]);
  const [b0, b1] = [Math.floor(Math.min(...ys) / B[1]) - 1, Math.ceil(Math.max(...ys) / B[1]) + 1];
  const cells = new Map<string, [number, number]>();
  for (let b = b0; b <= b1; b++) {
    const [a0, a1] = [Math.floor((Math.min(...xs) - b * B[0]) / spacing) - 1, Math.ceil((Math.max(...xs) - b * B[0]) / spacing) + 1];
    for (let a = a0; a <= a1; a++) {
      if ((a || b) && fits([a * A[0] + b * B[0], a * A[1] + b * B[1]])) cells.set(`${a},${b}`, [a, b]);
      if (cells.size > FILL_MAX) return `Saldrían más de ${FILL_MAX} copias: subir la separación, o usar el patrón de relleno de operaciones (Patrones → De relleno)`;
    }
  }
  if (!cells.size) return "No entra ninguna copia en la región: bajar la separación o el margen";
  // Paso de la grilla: líneas de construcción desde el original
  const pA = guidePoint(s, [L[0] + A[0], L[1] + A[1]]);
  const pB = guidePoint(s, [L[0] + B[0], L[1] + B[1]]);
  const lA = construction(s, addEntity(s, { type: "line", start: lead, end: pA }));
  const lB = construction(s, addEntity(s, { type: "line", start: lead, end: pB }));
  s.constraints.push({ type: "horizontal", line: lA }, { type: "length", line: lA, value: spacing }, { type: "equal", a: lA, b: lB });
  if (hex) {
    const lAB = construction(s, addEntity(s, { type: "line", start: pA, end: pB }));
    s.constraints.push({ type: "equal", a: lA, b: lAB });
  } else s.constraints.push({ type: "perpendicular", a: lA, b: lB });
  // Recorrido desde el original: cada copia sigue a una vecina ya puesta
  const placed = new Map<string, number>([["0,0", lead]]);
  const queue: [number, number][] = [[0, 0]];
  const link = (a: number, b: number) => {
    const d: P2 = [a * A[0] + b * B[0], a * A[1] + b * B[1]];
    const next = placeCopy(s, ids, pts, (p) => [pointOf(s, p)[0] + d[0], pointOf(s, p)[1] + d[1]]);
    placed.set(`${a},${b}`, next.get(lead)!);
    keepShape(s, pts, lead, next);
    queue.push([a, b]);
    return next.get(lead)!;
  };
  const steps: [number, number, number][] = [
    [1, 0, pA],
    [-1, 0, pA],
    [0, 1, pB],
    [0, -1, pB],
  ];
  while (placed.size <= cells.size) {
    const cur = queue.shift();
    if (!cur) {
      // Parte de la región que no se toca con lo ya puesto: se acota desde el original
      const [a, b] = [...cells.values()].find(([a, b]) => !placed.has(`${a},${b}`))!;
      const q = link(a, b);
      const d = [a * A[0] + b * B[0], a * A[1] + b * B[1]].map((v) => +v.toFixed(6));
      s.constraints.push({ type: "horizontal_distance", a: lead, b: q, value: d[0] }, { type: "vertical_distance", a: lead, b: q, value: d[1] });
      continue;
    }
    const [a, b] = cur;
    const from = placed.get(`${a},${b}`)!;
    for (const [da, db, guide] of steps) {
      const key = `${a + da},${b + db}`;
      if (!cells.has(key) || placed.has(key)) continue;
      const q = link(a + da, b + db);
      // Vecina hacia adelante: q − from = guía − original; hacia atrás, al revés
      s.constraints.push(da + db > 0 ? { type: "equal_offset", a1: lead, a2: guide, b1: from, b2: q } : { type: "equal_offset", a1: lead, a2: guide, b1: q, b2: from });
    }
  }
  return undefined;
}

/** Borra una entidad, sus restricciones y los puntos que quedan sueltos */
export function removeEntity(s: Sketch, id: number): void {
  const e = s.entities.find((x) => x.id === id);
  if (!e) return;
  s.entities = s.entities.filter((x) => x.id !== id);
  const pointsOf = geometryPoints;
  const mentions = constraintMentions;
  s.constraints = s.constraints.filter((c) => !mentions(c, new Set([id])));
  const anchors = new Set((s.texts ?? []).map((t) => t.anchor));
  const loose = pointsOf(e.geometry).filter((p) => p !== s.origin && !anchors.has(p) && !s.entities.some((x) => pointsOf(x.geometry).includes(p)));
  s.points = s.points.filter((p) => !loose.includes(p.id));
  s.constraints = s.constraints.filter((c) => !mentions(c, new Set(loose)));
  // Una arista usada deja de estarlo con su entidad
  if (s.uses) s.uses = s.uses.filter((u) => u.entity !== id);
  // Los textos se quedan con lo que les queda
  for (const t of s.texts ?? []) {
    t.entities = t.entities.filter((x) => x !== id);
    t.points = t.points.filter((p) => !loose.includes(p));
  }
}

/** Líneas que llegan a un punto (para redondear esquinas) */
export function linesAt(s: Sketch, point: number): SketchEntity[] {
  return s.entities.filter((e) => e.geometry.type === "line" && (e.geometry.start === point || e.geometry.end === point));
}

/**
 * Redondea la esquina entre las dos líneas que comparten `point`: las acorta
 * hasta los puntos de tangencia y pone un arco de radio `r` con su cota.
 * Devuelve un mensaje si no se puede.
 */
export function filletCorner(s: Sketch, point: number, r: number): string | undefined {
  const lines = linesAt(s, point);
  if (lines.length !== 2) return "La esquina debe unir exactamente dos líneas";
  const pos = (id: number): P2 => {
    const p = s.points.find((q) => q.id === id)!;
    return [p.x, p.y];
  };
  const P = pos(point);
  const other = (e: SketchEntity) => {
    const g = e.geometry as { start: number; end: number };
    return g.start === point ? g.end : g.start;
  };
  const [A, B] = [pos(other(lines[0])), pos(other(lines[1]))];
  const unit = (v: P2): P2 => {
    const l = Math.hypot(v[0], v[1]);
    return [v[0] / l, v[1] / l];
  };
  const u = unit([A[0] - P[0], A[1] - P[1]]);
  const v = unit([B[0] - P[0], B[1] - P[1]]);
  const cos = Math.max(-1, Math.min(1, u[0] * v[0] + u[1] * v[1]));
  const half = Math.acos(cos) / 2;
  if (half < 1e-3 || Math.PI / 2 - half < 1e-3) return "Las líneas están alineadas";
  const t = r / Math.tan(half);
  if (t >= Math.hypot(A[0] - P[0], A[1] - P[1]) || t >= Math.hypot(B[0] - P[0], B[1] - P[1])) return "El radio no entra en esas líneas";
  const bis = unit([u[0] + v[0], u[1] + v[1]]);
  const d = r / Math.sin(half);
  const T1: P2 = [P[0] + u[0] * t, P[1] + u[1] * t];
  const T2: P2 = [P[0] + v[0] * t, P[1] + v[1] * t];
  const C: P2 = [P[0] + bis[0] * d, P[1] + bis[1] * d];
  const t1 = addPoint(s, T1);
  const t2 = addPoint(s, T2);
  const c = addPoint(s, C);
  for (const [line, tp] of [[lines[0], t1], [lines[1], t2]] as const) {
    const g = line.geometry as { start: number; end: number };
    if (g.start === point) g.start = tp;
    else g.end = tp;
  }
  // Arco antihorario por el lado corto
  const cross = (T1[0] - C[0]) * (T2[1] - C[1]) - (T1[1] - C[1]) * (T2[0] - C[0]);
  const arc = addEntity(s, cross > 0 ? { type: "arc", center: c, start: t1, end: t2 } : { type: "arc", center: c, start: t2, end: t1 });
  // La esquina vieja desaparece con sus restricciones
  s.constraints = s.constraints.filter((k) => !constraintMentions(k, [point]));
  if (!s.entities.some((e) => geometryPoints(e.geometry).includes(point))) s.points = s.points.filter((q) => q.id !== point);
  // Las líneas quedaron más cortas: sus cotas de largo ya no valen
  s.constraints = s.constraints.filter((k) => !(k.type === "length" && (k.line === lines[0].id || k.line === lines[1].id)));
  s.constraints.push({ type: "tangent", a: lines[0].id, b: arc }, { type: "tangent", a: lines[1].id, b: arc }, { type: "radius", entity: arc, value: r });
  return undefined;
}

/**
 * Parte la línea `lineId` en el punto `point` (que debe estar sobre ella): dos
 * líneas que lo comparten, así las regiones se cierran en ese cruce. Copia
 * horizontal/vertical (o deja las dos mitades paralelas: alineadas, porque
 * comparten el punto); la cota de largo de la línea entera pasa a ser la
 * distancia entre sus extremos. Devuelve la mitad nueva.
 */
/**
 * Tramos nuevos que salieron de partir una línea durante un cambio (no son
 * dibujo nuevo: la simetría dinámica no los refleja)
 */
export const splitRests = new WeakMap<Sketch, Set<number>>();

export function splitLineAt(s: Sketch, lineId: number, point: number): number | undefined {
  const line = s.entities.find((e) => e.id === lineId);
  if (!line || line.geometry.type !== "line") return;
  const g = line.geometry;
  if (g.start === point || g.end === point) return;
  const [start, oldEnd] = [g.start, g.end];
  g.end = point;
  const rest = addEntity(s, { type: "line", start: point, end: oldEnd });
  if (line.construction) s.entities.find((e) => e.id === rest)!.construction = true;
  if (!splitRests.has(s)) splitRests.set(s, new Set());
  splitRests.get(s)!.add(rest);
  let axis = false;
  for (const k of [...s.constraints]) {
    if ((k.type === "horizontal" || k.type === "vertical") && k.line === lineId) {
      s.constraints.push({ type: k.type, line: rest });
      axis = true;
    }
  }
  if (!axis) s.constraints.push({ type: "parallel", a: lineId, b: rest });
  // En el lugar (mismo índice): la fórmula vinculada sigue valiendo
  s.constraints.forEach((k, i) => {
    if (k.type === "length" && k.line === lineId) {
      const expr = (k as { expr?: string }).expr;
      s.constraints[i] = { type: "distance", a: start, b: oldEnd, value: k.value, ...(k.reference ? { reference: true } : {}), ...(expr ? { expr } : {}) } as SketchConstraint;
    }
  });
  // El punto ya es extremo compartido: no necesita "punto en línea"
  s.constraints = s.constraints.filter((k) => !(k.type === "point_on_line" && k.point === point && k.line === lineId));
  return rest;
}

/**
 * Recorta una línea: quita el tramo entre cruces (con otras líneas, círculos
 * o arcos) donde cayó el clic `p`. Los extremos nuevos quedan pegados a la
 * curva que cruzan. Devuelve un mensaje si no hay nada que recortar.
 */
export function trimLine(s: Sketch, lineId: number, p: P2): string | undefined {
  const line = s.entities.find((e) => e.id === lineId);
  if (!line || line.geometry.type !== "line") return "Solo se recortan líneas";
  const g = line.geometry;
  const pos = (id: number): P2 => {
    const q = s.points.find((x) => x.id === id)!;
    return [q.x, q.y];
  };
  const A = pos(g.start);
  const B = pos(g.end);
  const d: P2 = [B[0] - A[0], B[1] - A[1]];
  const len2 = d[0] * d[0] + d[1] * d[1];
  const eps = 1e-9;
  // Cruces: parámetro sobre la línea y con qué entidad
  const cuts: { t: number; entity: number; kind: "line" | "circle" }[] = [];
  for (const e of s.entities) {
    if (e.id === lineId || e.construction) continue;
    const o = e.geometry;
    if (o.type === "line") {
      const [C, D] = [pos(o.start), pos(o.end)];
      const f: P2 = [D[0] - C[0], D[1] - C[1]];
      const den = d[0] * f[1] - d[1] * f[0];
      if (Math.abs(den) < eps) continue;
      const t = ((C[0] - A[0]) * f[1] - (C[1] - A[1]) * f[0]) / den;
      const u = ((C[0] - A[0]) * d[1] - (C[1] - A[1]) * d[0]) / den;
      if (t > eps && t < 1 - eps && u >= -eps && u <= 1 + eps) cuts.push({ t, entity: e.id, kind: "line" });
    } else if (o.type === "circle" || o.type === "arc") {
      const c = pos(o.center);
      const r = o.type === "circle" ? o.radius : Math.hypot(pos(o.start)[0] - c[0], pos(o.start)[1] - c[1]);
      const m: P2 = [A[0] - c[0], A[1] - c[1]];
      const b = 2 * (m[0] * d[0] + m[1] * d[1]);
      const cc = m[0] * m[0] + m[1] * m[1] - r * r;
      const disc = b * b - 4 * len2 * cc;
      if (disc < 0) continue;
      for (const t of [(-b - Math.sqrt(disc)) / (2 * len2), (-b + Math.sqrt(disc)) / (2 * len2)]) {
        if (t <= eps || t >= 1 - eps) continue;
        if (o.type === "arc") {
          // ¿El cruce cae dentro del barrido del arco?
          const q: P2 = [A[0] + t * d[0] - c[0], A[1] + t * d[1] - c[1]];
          const a0 = Math.atan2(pos(o.start)[1] - c[1], pos(o.start)[0] - c[0]);
          let sweep = Math.atan2(pos(o.end)[1] - c[1], pos(o.end)[0] - c[0]) - a0;
          while (sweep <= 0) sweep += 2 * Math.PI;
          let a = Math.atan2(q[1], q[0]) - a0;
          while (a < 0) a += 2 * Math.PI;
          if (a > sweep) continue;
        }
        cuts.push({ t, entity: e.id, kind: "circle" });
      }
    }
  }
  if (cuts.length === 0) {
    // Sin cruces: el tramo entero se va
    removeEntity(s, lineId);
    return undefined;
  }
  cuts.sort((x, y) => x.t - y.t);
  const tc = ((p[0] - A[0]) * d[0] + (p[1] - A[1]) * d[1]) / len2;
  const before = [...cuts].reverse().find((c) => c.t < tc);
  const after = cuts.find((c) => c.t > tc);
  const at = (c: { t: number }): P2 => [A[0] + c.t * d[0], A[1] + c.t * d[1]];
  // Las líneas cruzadas se parten en el punto (cierra regiones); en curvas, pegado
  const stick = (point: number, c: { entity: number; kind: "line" | "circle" }) => stickTo(s, point, c);
  if (before && after) {
    // Tramo del medio: la línea queda partida en dos
    const p1 = addPoint(s, at(before));
    const p2 = addPoint(s, at(after));
    const oldEnd = g.end;
    g.end = p1;
    const rest = addEntity(s, { type: "line", start: p2, end: oldEnd });
    stick(p1, before);
    stick(p2, after);
    // Horizontal/vertical valen para las dos partes
    for (const k of [...s.constraints]) {
      if ((k.type === "horizontal" || k.type === "vertical") && k.line === lineId) s.constraints.push({ type: k.type, line: rest });
    }
    // Cotas de largo de la línea entera ya no tienen sentido
    s.constraints = s.constraints.filter((k) => !(k.type === "length" && k.line === lineId));
  } else if (after) {
    const np = addPoint(s, at(after));
    g.start = np;
    stick(np, after);
    s.constraints = s.constraints.filter((k) => !(k.type === "length" && k.line === lineId));
  } else if (before) {
    const np = addPoint(s, at(before));
    g.end = np;
    stick(np, before);
    s.constraints = s.constraints.filter((k) => !(k.type === "length" && k.line === lineId));
  }
  // Puntos que quedaron sueltos
  const used = new Set(s.entities.flatMap((e) => geometryPoints(e.geometry)));
  const loose = new Set(s.points.filter((q) => !used.has(q.id) && q.id !== s.origin).map((q) => q.id));
  s.points = s.points.filter((q) => !loose.has(q.id));
  s.constraints = s.constraints.filter((k) => !constraintMentions(k, loose));
  return undefined;
}

/**
 * Equidistante de lo elegido a distancia `d` (positiva = hacia afuera):
 * círculos concéntricos (mismo centro) y lazos cerrados de líneas como
 * polígono paralelo (horizontal/vertical se copian). Devuelve un mensaje si
 * algo no se pudo.
 */
export function offsetEntities(s: Sketch, regions: Region[], ids: number[], d: number): string | undefined {
  const pos = (id: number): P2 => {
    const q = s.points.find((x) => x.id === id)!;
    return [q.x, q.y];
  };
  const done = new Set<number>();
  let skipped = 0;
  for (const id of ids) {
    if (done.has(id)) continue;
    const e = s.entities.find((x) => x.id === id);
    if (!e) continue;
    if (e.geometry.type === "circle") {
      const r = e.geometry.radius + d;
      if (r <= 0) {
        skipped++;
        continue;
      }
      addEntity(s, { type: "circle", center: e.geometry.center, radius: r });
      done.add(id);
      continue;
    }
    // El lazo cerrado que contiene la entidad, solo de líneas
    const loop = regions.flatMap((r) => [r.outer, ...r.holes]).find((l) => l.pieces.some((p) => p.entity === id));
    if (!loop || loop.pieces.some((p) => s.entities.find((x) => x.id === p.entity)?.geometry.type !== "line")) {
      skipped++;
      continue;
    }
    // Vértices en el orden del lazo (antihorario) y aristas desplazadas
    const verts: P2[] = loop.pieces.map((p) => {
      const g = s.entities.find((x) => x.id === p.entity)!.geometry as { start: number; end: number };
      return pos(p.reversed ? g.end : g.start);
    });
    const n = verts.length;
    const lines = verts.map((a, i) => {
      const b = verts[(i + 1) % n];
      const len = Math.hypot(b[0] - a[0], b[1] - a[1]);
      const nx = (b[1] - a[1]) / len;
      const ny = -(b[0] - a[0]) / len;
      // Antihorario: la normal saliente es la derecha del recorrido
      return { a: [a[0] + nx * d, a[1] + ny * d] as P2, dir: [(b[0] - a[0]) / len, (b[1] - a[1]) / len] as P2 };
    });
    const corner = (l1: (typeof lines)[0], l2: (typeof lines)[0]): P2 => {
      const den = l1.dir[0] * l2.dir[1] - l1.dir[1] * l2.dir[0];
      if (Math.abs(den) < 1e-12) return l2.a;
      const t = ((l2.a[0] - l1.a[0]) * l2.dir[1] - (l2.a[1] - l1.a[1]) * l2.dir[0]) / den;
      return [l1.a[0] + l1.dir[0] * t, l1.a[1] + l1.dir[1] * t];
    };
    const pts = lines.map((l, i) => addPoint(s, corner(lines[(i + n - 1) % n], l)));
    loop.pieces.forEach((piece, i) => {
      const line = addEntity(s, { type: "line", start: pts[i], end: pts[(i + 1) % n] });
      for (const k of [...s.constraints]) {
        if ((k.type === "horizontal" || k.type === "vertical") && k.line === piece.entity) s.constraints.push({ type: k.type, line });
      }
      done.add(piece.entity);
    });
  }
  return skipped ? "Algunas entidades no se pudieron desplazar (solo círculos y lazos cerrados de líneas)" : undefined;
}

// ─── Geometría de sketch: cruces, recortes, extensiones ───────────────────

type Hit = { entity: number; kind: "line" | "circle" };

function pointOf(s: Sketch, id: number): P2 {
  const q = s.points.find((x) => x.id === id)!;
  return [q.x, q.y];
}

/** Centro y radio de un círculo o arco; barrido y ángulo inicial si es arco */
function circleOf(s: Sketch, g: Geometry): { c: P2; r: number; a0?: number; sweep?: number } | undefined {
  if (g.type === "circle") return { c: pointOf(s, g.center), r: g.radius };
  if (g.type === "arc") {
    const [c, a, b] = [pointOf(s, g.center), pointOf(s, g.start), pointOf(s, g.end)];
    const a0 = Math.atan2(a[1] - c[1], a[0] - c[0]);
    let sweep = Math.atan2(b[1] - c[1], b[0] - c[0]) - a0;
    while (sweep <= 1e-12) sweep += 2 * Math.PI;
    return { c, r: Math.hypot(a[0] - c[0], a[1] - c[1]), a0, sweep };
  }
  return undefined;
}

/** Ángulo relativo al inicio del arco, en [0, 2π) */
function relAngle(p: P2, c: P2, a0: number): number {
  let t = Math.atan2(p[1] - c[1], p[0] - c[0]) - a0;
  while (t < 0) t += 2 * Math.PI;
  while (t >= 2 * Math.PI) t -= 2 * Math.PI;
  return t;
}

/** Puntos donde las demás entidades (no de construcción) cruzan un círculo o arco */
function crossingsOnCircle(s: Sketch, id: number, c: P2, r: number): { p: P2; hit: Hit }[] {
  const out: { p: P2; hit: Hit }[] = [];
  for (const e of s.entities) {
    if (e.id === id || e.construction) continue;
    const g = e.geometry;
    if (g.type === "line") {
      const [A, B] = [pointOf(s, g.start), pointOf(s, g.end)];
      const d: P2 = [B[0] - A[0], B[1] - A[1]];
      const m: P2 = [A[0] - c[0], A[1] - c[1]];
      const qa = d[0] * d[0] + d[1] * d[1];
      const qb = 2 * (m[0] * d[0] + m[1] * d[1]);
      const qc = m[0] * m[0] + m[1] * m[1] - r * r;
      const disc = qb * qb - 4 * qa * qc;
      if (disc < 0 || qa === 0) continue;
      for (const u of [(-qb - Math.sqrt(disc)) / (2 * qa), (-qb + Math.sqrt(disc)) / (2 * qa)]) {
        if (u >= -1e-9 && u <= 1 + 1e-9) out.push({ p: [A[0] + u * d[0], A[1] + u * d[1]], hit: { entity: e.id, kind: "line" } });
      }
    } else {
      const o = circleOf(s, g);
      if (!o) continue;
      const dx = o.c[0] - c[0];
      const dy = o.c[1] - c[1];
      const dd = Math.hypot(dx, dy);
      if (dd < 1e-12 || dd > r + o.r || dd < Math.abs(r - o.r)) continue;
      const a = (r * r - o.r * o.r + dd * dd) / (2 * dd);
      const h = Math.sqrt(Math.max(0, r * r - a * a));
      const base: P2 = [c[0] + (a * dx) / dd, c[1] + (a * dy) / dd];
      for (const sgn of h > 1e-12 ? [1, -1] : [1]) {
        const p: P2 = [base[0] - (sgn * h * dy) / dd, base[1] + (sgn * h * dx) / dd];
        if (o.sweep !== undefined && relAngle(p, o.c, o.a0!) > o.sweep + 1e-9) continue;
        out.push({ p, hit: { entity: e.id, kind: "circle" } });
      }
    }
  }
  return out;
}

/** Vértice, centro o medio de una arista del sólido: el punto queda fijo ahí */
export const isSolidPoint = (snap: Snap) => snap.kind === "solid_vertex" || snap.kind === "solid_center" || snap.kind === "solid_midpoint";

/**
 * Punto para un anclaje (ver `sketchSnap.infer`): el existente, o uno nuevo
 * con la restricción que corresponde. Punto medio: parte la línea y deja las
 * dos mitades iguales. Cuadrante: sobre la curva y alineado con el centro.
 * Intersección: sobre las dos curvas. Alineado: a la misma altura o en la
 * misma vertical que esos puntos. La dirección (paralela, perpendicular,
 * tangente) la pone la herramienta Línea, que crea la línea.
 * Sobre una línea: la parte (así cierra regiones). Sobre una curva: punto en
 * círculo. Vértice, centro o medio del sólido: fijo en esa posición (no sigue
 * al sólido si cambia). Sobre una arista del sólido: solo la posición.
 */
export function placeSnap(s: Sketch, snap: Snap): number {
  if (snap.id !== undefined && s.points.some((q) => q.id === snap.id)) return snap.id;
  const id = addPoint(s, snap.p);
  if (snap.align?.h !== undefined) s.constraints.push({ type: "horizontal_points", a: snap.align.h, b: id });
  if (snap.align?.v !== undefined) s.constraints.push({ type: "vertical_points", a: snap.align.v, b: id });
  if (isSolidPoint(snap)) s.constraints.push({ type: "fixed", point: id, x: snap.p[0], y: snap.p[1] });
  const e = snap.entity;
  if (e === undefined) return id;
  if (snap.kind === "midpoint") {
    const g = s.entities.find((x) => x.id === e)?.geometry;
    // La línea pudo partirse con otro punto de la misma forma: entonces solo "sobre"
    const mid = g?.type === "line" ? [(pointOf(s, g.start)[0] + pointOf(s, g.end)[0]) / 2, (pointOf(s, g.start)[1] + pointOf(s, g.end)[1]) / 2] : undefined;
    if (mid && Math.hypot(mid[0] - snap.p[0], mid[1] - snap.p[1]) <= 1e-9 * Math.max(1, Math.hypot(snap.p[0], snap.p[1]))) {
      const rest = splitLineAt(s, e, id);
      if (rest !== undefined) s.constraints.push({ type: "equal", a: e, b: rest });
      return id;
    }
    stickTo(s, id, { entity: e, kind: "line" });
    return id;
  }
  if (snap.kind === "quadrant") {
    const g = s.entities.find((x) => x.id === e)?.geometry;
    s.constraints.push({ type: "point_on_circle", point: id, circle: e });
    if (g?.type === "circle" || g?.type === "arc") {
      const q = snap.quadrant ?? 0;
      s.constraints.push({ type: q % 2 === 0 ? "horizontal_points" : "vertical_points", a: g.center, b: id });
    }
    return id;
  }
  if (snap.kind === "intersection") {
    // Sobre las dos curvas: las líneas se parten ahí (cierra regiones)
    for (const c of [e, snap.other]) {
      const g = s.entities.find((x) => x.id === c)?.geometry;
      if (c !== undefined && g) stickTo(s, id, { entity: c, kind: g.type === "line" ? "line" : "circle" });
    }
    return id;
  }
  if (snap.kind === "on_line") stickTo(s, id, { entity: e, kind: "line" });
  else if (snap.kind === "on_circle") s.constraints.push({ type: "point_on_circle", point: id, circle: e });
  return id;
}

/**
 * Deja el punto nuevo sobre lo que cruza: parte la línea o lo pega a la curva.
 * Si la línea ya se partió antes (dos cruces sobre el mismo lado), se parte el
 * tramo que de verdad contiene el punto.
 */
function stickTo(s: Sketch, point: number, hit: Hit) {
  if (hit.kind !== "line") {
    s.constraints.push({ type: "point_on_circle", point, circle: hit.entity });
    return;
  }
  const p = pointOf(s, point);
  const onSegment = (id: number) => {
    const g = s.entities.find((e) => e.id === id)?.geometry;
    if (g?.type !== "line") return Infinity;
    const [a, b] = [pointOf(s, g.start), pointOf(s, g.end)];
    const d: P2 = [b[0] - a[0], b[1] - a[1]];
    const l2 = d[0] * d[0] + d[1] * d[1] || 1;
    const t = ((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / l2;
    if (t < -1e-9 || t > 1 + 1e-9) return Infinity;
    return Math.hypot(p[0] - a[0] - t * d[0], p[1] - a[1] - t * d[1]);
  };
  const tol = 1e-6 * Math.max(1, Math.hypot(p[0], p[1]));
  if (onSegment(hit.entity) <= tol) return splitLineAt(s, hit.entity, point);
  // La línea cruzada ya se partió: el tramo que contiene el punto (sin contar
  // las que ya lo tienen como extremo)
  const lines = s.entities.filter(
    (e) => e.geometry.type === "line" && e.geometry.start !== point && e.geometry.end !== point,
  );
  const best = lines.reduce<{ id: number; d: number } | undefined>((acc, e) => {
    const d = onSegment(e.id);
    return d <= tol && (!acc || d < acc.d) ? { id: e.id, d } : acc;
  }, undefined);
  if (best) splitLineAt(s, best.id, point);
  else s.constraints.push({ type: "point_on_line", point, line: hit.entity });
}

/**
 * Recorta un círculo o un arco en el tramo entre cruces donde cayó `p`: el
 * círculo pasa a ser arco; el arco se acorta o se parte en dos. Sin cruces se
 * borra entero.
 */
export function trimCurve(s: Sketch, id: number, p: P2): string | undefined {
  const e = s.entities.find((x) => x.id === id);
  if (!e) return undefined;
  const o = circleOf(s, e.geometry);
  if (!o) return "Solo círculos y arcos";
  const g = e.geometry as { center: number };
  const a0 = o.a0 ?? 0;
  const sweep = o.sweep ?? 2 * Math.PI;
  const cuts = crossingsOnCircle(s, id, o.c, o.r)
    .map((x) => ({ ...x, t: relAngle(x.p, o.c, a0) }))
    .filter((x) => (o.sweep === undefined ? true : x.t > 1e-9 && x.t < sweep - 1e-9))
    .sort((x, y) => x.t - y.t);
  if (cuts.length === 0) {
    removeEntity(s, id);
    return undefined;
  }
  const tc = relAngle(p, o.c, a0);
  if (e.geometry.type === "circle") {
    if (cuts.length < 2) return "El círculo necesita al menos dos cruces para recortarlo";
    // Tramo quitado: entre el último cruce antes del clic y el primero después
    const after = cuts.find((x) => x.t > tc) ?? cuts[0];
    const before = [...cuts].reverse().find((x) => x.t < tc) ?? cuts[cuts.length - 1];
    const ps = addPoint(s, after.p);
    const pe = addPoint(s, before.p);
    e.geometry = { type: "arc", center: g.center, start: ps, end: pe };
    stickTo(s, ps, after.hit);
    stickTo(s, pe, before.hit);
    return undefined;
  }
  const arc = e.geometry as { center: number; start: number; end: number };
  const before = [...cuts].reverse().find((x) => x.t < tc);
  const after = cuts.find((x) => x.t > tc);
  if (before && after) {
    const pb = addPoint(s, before.p);
    const pa = addPoint(s, after.p);
    const oldEnd = arc.end;
    arc.end = pb;
    addEntity(s, { type: "arc", center: arc.center, start: pa, end: oldEnd });
    stickTo(s, pb, before.hit);
    stickTo(s, pa, after.hit);
  } else if (after) {
    const pa = addPoint(s, after.p);
    arc.start = pa;
    stickTo(s, pa, after.hit);
  } else if (before) {
    const pb = addPoint(s, before.p);
    arc.end = pb;
    stickTo(s, pb, before.hit);
  }
  dropLoosePoints(s);
  return undefined;
}

function dropLoosePoints(s: Sketch) {
  const used = new Set(
    s.entities.flatMap((e) => geometryPoints(e.geometry)),
  );
  const loose = new Set(s.points.filter((q) => !used.has(q.id)).map((q) => q.id));
  if (loose.size === 0) return;
  s.points = s.points.filter((q) => !loose.has(q.id));
  s.constraints = s.constraints.filter((k) => !constraintIds(k).some((v) => loose.has(v)));
}

/**
 * Alarga una línea por el extremo más cercano a `p` hasta lo primero que
 * cruce (otra línea, círculo o arco). El extremo nuevo queda sobre eso.
 */
export function extendLine(s: Sketch, lineId: number, p: P2): string | undefined {
  const line = s.entities.find((e) => e.id === lineId);
  if (!line || line.geometry.type !== "line") return "Solo se extienden líneas";
  const g = line.geometry;
  const [A, B] = [pointOf(s, g.start), pointOf(s, g.end)];
  const atEnd = Math.hypot(p[0] - B[0], p[1] - B[1]) <= Math.hypot(p[0] - A[0], p[1] - A[1]);
  const from = atEnd ? B : A;
  const other = atEnd ? A : B;
  const len = Math.hypot(from[0] - other[0], from[1] - other[1]);
  const d: P2 = [(from[0] - other[0]) / len, (from[1] - other[1]) / len];
  let best: { t: number; hit: Hit } | undefined;
  const consider = (t: number, hit: Hit) => {
    if (t > 1e-9 && (!best || t < best.t)) best = { t, hit };
  };
  for (const e of s.entities) {
    if (e.id === lineId || e.construction) continue;
    const og = e.geometry;
    if (og.type === "line") {
      const [C, D] = [pointOf(s, og.start), pointOf(s, og.end)];
      const f: P2 = [D[0] - C[0], D[1] - C[1]];
      const den = d[0] * f[1] - d[1] * f[0];
      if (Math.abs(den) < 1e-12) continue;
      const t = ((C[0] - from[0]) * f[1] - (C[1] - from[1]) * f[0]) / den;
      const u = ((C[0] - from[0]) * d[1] - (C[1] - from[1]) * d[0]) / den;
      if (u >= -1e-9 && u <= 1 + 1e-9) consider(t, { entity: e.id, kind: "line" });
    } else {
      const o = circleOf(s, og);
      if (!o) continue;
      const m: P2 = [from[0] - o.c[0], from[1] - o.c[1]];
      const b = 2 * (m[0] * d[0] + m[1] * d[1]);
      const c = m[0] * m[0] + m[1] * m[1] - o.r * o.r;
      const disc = b * b - 4 * c;
      if (disc < 0) continue;
      for (const t of [(-b - Math.sqrt(disc)) / 2, (-b + Math.sqrt(disc)) / 2]) {
        const q: P2 = [from[0] + t * d[0], from[1] + t * d[1]];
        if (o.sweep !== undefined && relAngle(q, o.c, o.a0!) > o.sweep + 1e-9) continue;
        consider(t, { entity: e.id, kind: "circle" });
      }
    }
  }
  if (!best) return "No hay nada en esa dirección hasta donde extender";
  const target: P2 = [from[0] + best.t * d[0], from[1] + best.t * d[1]];
  // Punto nuevo para el extremo (si el viejo lo comparte otra entidad, queda allí)
  const np = addPoint(s, target);
  if (atEnd) g.end = np;
  else g.start = np;
  s.constraints = s.constraints.filter((k) => !(k.type === "length" && k.line === lineId));
  stickTo(s, np, best.hit);
  dropLoosePoints(s);
  return undefined;
}

/** Recorta lo que haya bajo el clic: líneas, círculos o arcos */
export function trimAt(s: Sketch, id: number, p: P2): string | undefined {
  const e = s.entities.find((x) => x.id === id);
  if (!e) return undefined;
  if (e.geometry.type === "line") return trimLine(s, id, p);
  if (e.geometry.type === "circle" || e.geometry.type === "arc") return trimCurve(s, id, p);
  return "Las splines no se recortan";
}

/** Campos numéricos de una restricción que no son ids */
const NOT_IDS = ["value", "degrees", "x", "y"];

/** En la perforación `curve` es una operación, no una entidad */
const notIds = (c: SketchConstraint) => (c.type === "pierce" ? [...NOT_IDS, "curve"] : NOT_IDS);

/** Puntos y entidades que nombra una restricción (para resaltarla) */
export function constraintIds(c: SketchConstraint): number[] {
  const skip = notIds(c);
  return Object.entries(c).flatMap(([k, v]) =>
    skip.includes(k) ? [] : typeof v === "number" ? [v] : k === "entities" && Array.isArray(v) ? (v as number[]) : [],
  );
}

/** Copia de una restricción con sus ids cambiados por `f` */
export function mapConstraintIds<C extends SketchConstraint>(c: C, f: (id: number) => number): C {
  const k = structuredClone(c) as C & Record<string, unknown>;
  const skip = notIds(c);
  for (const [key, v] of Object.entries(k)) {
    if (skip.includes(key)) continue;
    if (typeof v === "number") (k as Record<string, unknown>)[key] = f(v);
    else if (key === "entities" && Array.isArray(v)) (k as Record<string, unknown>)[key] = (v as number[]).map(f);
  }
  return k;
}

/** Si una restricción nombra alguno de los ids */
export function constraintMentions(c: SketchConstraint, ids: Set<number> | number[]): boolean {
  const set = ids instanceof Set ? ids : new Set(ids);
  return constraintIds(c).some((v) => set.has(v));
}

/**
 * Arco tangente que sale de `start` en la dirección `t` (unitaria) y llega a
 * `end`. Devuelve centro, si va antihorario y la dirección de salida en `end`.
 */
export function tangentArc(start: P2, t: P2, end: P2): { center: P2; ccw: boolean; outDir: P2 } | undefined {
  const n: P2 = [-t[1], t[0]];
  const se: P2 = [start[0] - end[0], start[1] - end[1]];
  const den = 2 * (n[0] * se[0] + n[1] * se[1]);
  if (Math.abs(den) < 1e-12) return undefined; // en línea recta: no hay arco
  const k = -(se[0] * se[0] + se[1] * se[1]) / den;
  const center: P2 = [start[0] + n[0] * k, start[1] + n[1] * k];
  const ccw = k > 0;
  const r: P2 = [end[0] - center[0], end[1] - center[1]];
  const rl = Math.hypot(r[0], r[1]);
  const outDir: P2 = ccw ? [-r[1] / rl, r[0] / rl] : [r[1] / rl, -r[0] / rl];
  return { center, ccw, outDir };
}

/** Dirección en que se sale de `point` siguiendo la entidad hacia afuera de ella */
export function leavingDirection(s: Sketch, entity: number, point: number): P2 | undefined {
  const e = s.entities.find((x) => x.id === entity);
  if (!e) return undefined;
  const g = e.geometry;
  if (g.type === "line") {
    const [p, q] = g.end === point ? [pointOf(s, g.end), pointOf(s, g.start)] : [pointOf(s, g.start), pointOf(s, g.end)];
    const l = Math.hypot(p[0] - q[0], p[1] - q[1]);
    return [(p[0] - q[0]) / l, (p[1] - q[1]) / l];
  }
  if (g.type === "arc") {
    const c = pointOf(s, g.center);
    const r = pointOf(s, point);
    const v: P2 = [r[0] - c[0], r[1] - c[1]];
    const l = Math.hypot(v[0], v[1]);
    // Al final de un arco antihorario se sigue antihorario; al inicio, al revés
    return g.end === point ? [-v[1] / l, v[0] / l] : [v[1] / l, -v[0] / l];
  }
  return undefined;
}

/** Texto que es solo un número (con punto o coma decimal) */
export function plainNumber(text: string): number | undefined {
  const t = text.trim().replace(",", ".");
  return /^[-+]?(\d+\.?\d*|\.\d+)(e[-+]?\d+)?$/i.test(t) ? parseFloat(t) : undefined;
}

/** Ruta de vínculo del valor de una restricción de un sketch */
export function constraintPath(feature: number, index: number, c: SketchConstraint): string {
  return `${feature}.kind.sketch.constraints.${index}.${c.type === "angle" ? "degrees" : "value"}`;
}

/** Cambia el nombre de un parámetro dentro de una fórmula (solo palabras completas) */
export function renameInExpr(expr: string, from: string, to: string): string {
  const esc = from.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return expr.replace(new RegExp(`(?<![\\p{L}\\p{N}_])${esc}(?![\\p{L}\\p{N}_])`, "gu"), to);
}

/** Valor editable de una restricción (cota), si tiene */
/** Cota de referencia: muestra la medida, no restringe */
export function isReference(c: SketchConstraint): boolean {
  return "reference" in c && !!c.reference;
}

export function constraintValue(c: SketchConstraint): number | undefined {
  if ("value" in c) return c.value;
  if (c.type === "angle") return c.degrees;
  return undefined;
}

export const CONSTRAINT_LABELS: Record<SketchConstraint["type"], string> = {
  coincident: "Coincidente",
  fixed: "Fijo",
  horizontal: "Horizontal",
  vertical: "Vertical",
  horizontal_points: "Alineados horizontal",
  vertical_points: "Alineados vertical",
  parallel: "Paralelas",
  collinear: "Colineales",
  perpendicular: "Perpendiculares",
  equal: "Iguales",
  tangent: "Tangente",
  concentric: "Concéntricos",
  point_on_line: "Punto en línea",
  point_on_circle: "Punto en círculo",
  midpoint: "Punto medio",
  symmetric: "Simétricos",
  equal_offset: "Patrón lineal",
  equal_rotation: "Patrón circular",
  distance: "Distancia",
  horizontal_distance: "Distancia horizontal",
  vertical_distance: "Distancia vertical",
  length: "Largo",
  radius: "Radio",
  diameter: "Diámetro",
  angle: "Ángulo",
  point_line_distance: "Distancia a la línea",
  axis_diameter: "Diámetro respecto del eje",
  arc_length: "Largo de arco",
  curve_length: "Largo total",
  circle_distance: "Distancia mín./máx.",
  coradial: "Coradiales",
  symmetric_entities: "Simétricas",
  point_on_curve: "Punto en la curva",
  intersection: "Punto en la intersección",
  lock: "Bloqueada",
  curvature: "Curvatura igual (G2)",
  pierce: "Perforación",
};

// ─── Store ────────────────────────────────────────────────────────────────

const HISTORY_LIMIT = 100;

/**
 * Operación abierta en su diálogo: los cambios van a `doc` (con vista previa
 * en el visor) y recién al aceptar pasan al documento en un solo paso de
 * deshacer; cancelar vuelve a `base`.
 */
export interface Draft {
  feature: number;
  /** Creada en este diálogo (cancelar la quita) */
  isNew: boolean;
  base: CadDocument;
  doc: CadDocument;
  /** Eligiendo en una caja de selección: se ve el sólido de antes de la operación */
  selecting?: boolean;
}

/** Lo que se evalúa para la vista previa: hasta la operación en edición, como Onshape */
export function previewDocument(d: Draft): CadDocument {
  const doc = clone(d.doc);
  const i = doc.features.findIndex((f) => f.id === d.feature);
  const end = d.selecting ? i : i + 1;
  doc.rollback = i < 0 || end >= doc.features.length ? null : end;
  return doc;
}

/** Qué cara o arista del sólido mostrado es cada referencia (`null` = no está) */
export interface ResolvedRefs {
  faces: (number | null)[];
  edges: (number | null)[];
}

export function createCadStore() {
  const [status, setStatus] = createSignal<CadStatus>();
  const [committed, setDoc] = createSignal<CadDocument | null>(null);
  const [draft, setDraft] = createSignal<Draft>();
  // Herramienta de la operación en el diálogo (vista previa verde/roja)
  const [tool, setTool] = createSignal<{ mesh: CadMesh; op: BodyOp } | null>(null);
  // Versión abierta en solo lectura (índice en `versions`)
  const [viewing, setViewing] = createSignal<number>();
  const [comparison, setComparison] = createSignal<VersionComparison | null>(null);
  const viewed = () => {
    const i = viewing();
    return i === undefined ? undefined : committed()?.versions?.[i]?.document;
  };
  // Con un diálogo abierto, todo lo que lee el documento ve el borrador; con
  // una versión abierta, esa versión
  const doc = () => draft()?.doc ?? viewed() ?? committed();
  const [result, setResult] = createSignal<CadResult | null>(null);
  const [mesh, setMesh] = createSignal<CadMesh | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string>();
  const [selected, setSelected] = createSignal<number>();
  const undoStack: CadDocument[] = [];
  const redoStack: CadDocument[] = [];
  const [historyVersion, setHistoryVersion] = createSignal(0);
  let meshVersion = -1;
  // Envíos encadenados: el último gana y los intermedios no pisan el resultado
  let sendSeq = 0;
  let lastSend: Promise<void> = Promise.resolve();

  const refreshMesh = async (version: number) => {
    if (version === meshVersion) return;
    meshVersion = version;
    const buf = await invoke<ArrayBuffer>("cad_mesh");
    if (meshVersion === version) setMesh(decodeCadMesh(buf));
  };

  const apply = async (r: CadResult) => {
    setResult(r);
    await refreshMesh(r.version);
  };

  /** Herramienta de la operación del diálogo, si suma, resta o interseca (no mientras se elige en una caja) */
  const refreshTool = async (seq: number) => {
    const d = draft();
    const f = d && !d.selecting ? d.doc.features.find((x) => x.id === d.feature) : undefined;
    const op = f?.kind.type === "hole" ? "cut" : f && "op" in f.kind && f.kind.type !== "boolean" ? f.kind.op : undefined;
    if (!d || !op) return setTool(null);
    const mesh = decodeCadMesh(await invoke<ArrayBuffer>("cad_tool_mesh", { feature: d.feature }));
    if (seq === sendSeq) setTool(mesh ? { mesh, op } : null);
  };

  // Cola de envíos: mientras el backend calcula, los cambios que llegan se
  // juntan y solo se manda el último de cada tipo seguido (escribir "25"
  // calcula "2" y "25", no "2", "25" y lo que venga en el medio). Guardar y
  // vista previa no se mezclan: se mandan en orden.
  const queue: { next: CadDocument | null; preview: boolean }[] = [];
  let pumping: Promise<void> | null = null;
  /** Cuántos envíos se saltearon por llegar otro detrás (para las pruebas) */
  let skipped = 0;
  const pump = async () => {
    while (queue.length) {
      const q = queue.shift()!;
      await sendNow(q.next, q.preview);
    }
    pumping = null;
  };

  /** Guarda `next` en el backend, o con `preview` solo lo muestra (`null` = volver al documento) */
  const send = (next: CadDocument | null, preview = false) => {
    const last = queue[queue.length - 1];
    if (last && last.preview === preview) {
      last.next = next;
      skipped++;
    } else queue.push({ next, preview });
    pumping ??= pump();
    return (lastSend = pumping);
  };
  const sendNow = async (next: CadDocument | null, preview: boolean) => {
    const seq = ++sendSeq;
    setBusy(true);
    try {
      const r = preview
        ? await invoke<CadResult>("cad_preview", { document: next })
        : await invoke<CadResult>("cad_set_document", { document: next });
      if (seq === sendSeq) {
        setError(undefined);
        await apply(r);
        await refreshTool(seq);
      }
    } catch (e) {
      if (seq === sendSeq) setError(String(e));
    } finally {
      if (seq === sendSeq) setBusy(false);
    }
  };

  /** Cambia el documento (deshacible) y recalcula; con un diálogo abierto, cambia el borrador */
  const commit = (mutate: (d: CadDocument) => void) => {
    if (viewing() !== undefined) {
      setError("Estás viendo una versión guardada: vuelve al diseño o restáurala para editar");
      return lastSend;
    }
    setComparison(null);
    const d = draft();
    if (d) {
      const next = clone(d.doc);
      mutate(next);
      const nd = { ...d, doc: next };
      setDraft(nd);
      return send(previewDocument(nd), true);
    }
    const current = committed() ?? { features: [] };
    const next = clone(current);
    mutate(next);
    undoStack.push(current);
    if (undoStack.length > HISTORY_LIMIT) undoStack.shift();
    redoStack.length = 0;
    setHistoryVersion((v) => v + 1);
    setDoc(next);
    return send(next);
  };

  const openDraft = (d: Draft) => {
    if (viewing() !== undefined) return lastSend;
    batch(() => {
      setDraft(d);
      setSelected(d.feature);
    });
    return send(previewDocument(d), true);
  };

  /** Acepta el diálogo abierto: un paso de deshacer si cambió algo */
  const acceptDraft = () => {
    const d = draft();
    if (!d) return lastSend;
    const unchanged = !d.isNew && JSON.stringify(d.doc) === JSON.stringify(d.base);
    if (!unchanged) {
      undoStack.push(d.base);
      if (undoStack.length > HISTORY_LIMIT) undoStack.shift();
      redoStack.length = 0;
    }
    // Todo junto: el panel nunca ve el diálogo cerrado con el documento viejo
    batch(() => {
      setDraft(undefined);
      setSelected(undefined);
      if (!unchanged) {
        setHistoryVersion((v) => v + 1);
        setDoc(d.doc);
      }
    });
    return unchanged ? send(null, true) : send(d.doc);
  };

  /** Cierra el diálogo sin cambiar el documento */
  const cancelDraft = () => {
    if (!draft()) return lastSend;
    batch(() => {
      setDraft(undefined);
      setSelected(undefined);
    });
    return send(null, true);
  };

  const store = {
    status,
    doc,
    /** El documento guardado (sin el borrador del diálogo abierto) */
    committed,
    /** Herramienta de la operación en el diálogo, para dibujarla translúcida */
    tool,
    draft,
    acceptDraft,
    cancelDraft,
    /** Espera a que termine el último recálculo pedido */
    settled: () => lastSend,
    /** Envíos que no hizo falta mandar porque llegó otro detrás */
    skippedSends: () => skipped,
    /** Con una caja de selección activa, la vista previa muestra el sólido de antes de la operación */
    setSelecting(on: boolean) {
      const d = draft();
      if (!d || !!d.selecting === on) return lastSend;
      const nd = { ...d, selecting: on };
      setDraft(nd);
      return send(previewDocument(nd), true);
    },
    /** Desarrollo de chapa de una pieza (sin ella: la de la chapa) */
    flatPattern: (part?: PartId) => invoke<FlatPattern>("cad_flat_pattern", { part: part ?? null }),
    /** Apoyo para una pieza estándar en el borde circular `edge` */
    edgeSeat: (edge: number) => invoke<Seat>("cad_edge_seat", { edge }),
    resolveRefs: (faces: FaceRef[], edges: EdgeRef[]) => invoke<ResolvedRefs>("cad_resolve_refs", { faces, edges }),
    result,
    mesh,
    busy,
    error,
    setError,
    selected,
    /**
     * Elige una operación: abre su diálogo (los sketches se editan aparte).
     * Si había otro abierto, se acepta.
     */
    select(id: number | undefined) {
      const d = draft();
      if (d?.feature === id && id !== undefined) return;
      if (d) void acceptDraft();
      setSelected(id);
      const base = committed();
      const f = base?.features.find((x) => x.id === id);
      if (!base || !f || f.kind.type === "sketch") return;
      void openDraft({ feature: f.id, isNew: false, base, doc: clone(base) });
    },
    canUndo: () => (historyVersion(), undoStack.length > 0),
    canRedo: () => (historyVersion(), redoStack.length > 0),

    async init() {
      const s = await invoke<CadStatus>("cad_status");
      setStatus(s);
      await store.reload();
    },

    /** Trae el documento del backend (al abrir un proyecto o empezar uno nuevo) */
    async reload() {
      const d = await invoke<CadDocument | null>("cad_get_document");
      setViewing(undefined);
      setComparison(null);
      undoStack.length = 0;
      redoStack.length = 0;
      setHistoryVersion((v) => v + 1);
      setDraft(undefined);
      setDoc(d);
      setSelected(undefined);
      if (!d) {
        setResult(null);
        setMesh(null);
        meshVersion = -1;
        return;
      }
      try {
        await apply(await invoke<CadResult>("cad_evaluate"));
      } catch (e) {
        setError(String(e));
      }
    },

    async newDesign() {
      const r = await invoke<CadResult>("cad_new");
      setDraft(undefined);
      undoStack.length = 0;
      redoStack.length = 0;
      setHistoryVersion((v) => v + 1);
      setDoc({ features: [] });
      setSelected(undefined);
      await apply(r);
    },

    async close() {
      await invoke("cad_close");
      await store.reload();
    },

    commit,

    /**
     * Agrega una operación al final (o donde está la barra de retroceso) y abre
     * su diálogo; los sketches entran directo al documento.
     */
    async addFeature(kind: FeatureKind, name?: string): Promise<number> {
      if (draft()) void acceptDraft();
      const current = committed() ?? { features: [] };
      const id = nextFeatureId(current);
      const insert = (d: CadDocument) => {
        const at = Math.min(d.rollback ?? d.features.length, d.features.length);
        d.features.splice(at, 0, { id, name: name ?? defaultName(d, kind), suppressed: false, kind });
        d.next_id = id + 1;
        if (d.rollback != null) d.rollback += 1;
      };
      if (editsAsSketch(kind)) {
        setSelected(id);
        await commit(insert);
        return id;
      }
      const next = clone(current);
      insert(next);
      await openDraft({ feature: id, isNew: true, base: current, doc: next });
      return id;
    },

    updateFeature(id: number, mutate: (f: Feature) => void) {
      return commit((d) => {
        const f = d.features.find((x) => x.id === id);
        if (f) mutate(f);
      });
    },

    /** Borra la operación; `false` si otras dependen de ella */
    removeFeature(id: number): boolean {
      const open = draft();
      if (open?.feature === id && open.isNew) {
        void cancelDraft();
        return true;
      }
      if (open) void acceptDraft();
      const d = committed();
      if (!d) return false;
      if (d.features.some((f) => featureDeps(f).includes(id))) return false;
      void commit((n) => {
        const i = n.features.findIndex((f) => f.id === id);
        if (i < 0) return;
        if (n.folders?.length) n.folders = fixFolders(n, id, n.features.filter((f) => f.id !== id).map((f) => f.id));
        n.features.splice(i, 1);
        if (n.rollback != null && i < n.rollback) n.rollback -= 1;
        // Sus campos vinculados se van con ella
        for (const k of Object.keys(n.bindings ?? {})) if (k.startsWith(`${id}.`)) delete n.bindings![k];
      });
      if (selected() === id) setSelected(undefined);
      return true;
    },

    /** Mueve la operación una posición; `false` si rompe dependencias */
    moveFeature(id: number, delta: -1 | 1): boolean {
      const i = committed()?.features.findIndex((f) => f.id === id) ?? -1;
      if (i < 0) return false;
      // Bajar una = insertarla después de la siguiente
      return store.moveFeatureTo(id, delta < 0 ? i - 1 : i + 2);
    },

    /**
     * Mueve la operación al hueco `to` (0 = antes de la primera, n = al final,
     * contando con la lista de antes de moverla); `false` si rompe dependencias.
     */
    moveFeatureTo(id: number, to: number): boolean {
      if (draft()) void acceptDraft();
      const d = committed();
      if (!d) return false;
      const i = d.features.findIndex((f) => f.id === id);
      if (i < 0 || to < 0 || to > d.features.length) return false;
      if (to === i || to === i + 1) return true;
      const order = [...d.features];
      const [moved] = order.splice(i, 1);
      order.splice(to > i ? to - 1 : to, 0, moved);
      const pos = (x: number) => order.findIndex((f) => f.id === x);
      const broken = order.some((f, k) => featureDeps(f).some((dep) => pos(dep) < 0 || pos(dep) > k));
      if (broken) return false;
      void commit((n) => {
        if (n.folders?.length) n.folders = fixFolders(n, id, order.map((f) => f.id));
        n.features = order.map((f) => n.features.find((x) => x.id === f.id)!);
      });
      return true;
    },

    /** Agrupa en una carpeta las operaciones de `a` a `b` (posiciones); `false` si pisa otra carpeta */
    addFolder(a: number, b: number, name = "Carpeta"): boolean {
      if (draft()) void acceptDraft();
      const d = committed();
      if (!d || a < 0 || b >= d.features.length) return false;
      const [lo, hi] = a <= b ? [a, b] : [b, a];
      const overlaps = (d.folders ?? []).some((f) => {
        const r = folderRange(d, f);
        return r && r[0] <= hi && lo <= r[1];
      });
      if (overlaps) return false;
      void commit((n) => {
        n.folders = [...(n.folders ?? []), { name, first: n.features[lo].id, last: n.features[hi].id }];
      });
      return true;
    },

    /** Cambia una carpeta (por su posición en la lista); `null` la deshace (las operaciones quedan) */
    updateFolder(index: number, change: Partial<Folder> | null) {
      return commit((n) => {
        const list = [...(n.folders ?? [])];
        if (!list[index]) return;
        if (change === null) list.splice(index, 1);
        else list[index] = { ...list[index], ...change };
        n.folders = list;
      });
    },

    setRollback(index: number | null) {
      if (draft()) void acceptDraft();
      return commit((d) => {
        d.rollback = index == null || index >= d.features.length ? null : index;
      });
    },

    /** Con un diálogo abierto, deshacer lo cancela */
    undo() {
      if (viewing() !== undefined) return;
      if (draft()) return void cancelDraft();
      const prev = undoStack.pop();
      if (!prev) return;
      const current = doc();
      if (current) redoStack.push(current);
      setHistoryVersion((v) => v + 1);
      setDoc(prev);
      void send(prev);
    },

    redo() {
      if (draft() || viewing() !== undefined) return;
      const next = redoStack.pop();
      if (!next) return;
      const current = doc();
      if (current) undoStack.push(current);
      setHistoryVersion((v) => v + 1);
      setDoc(next);
      void send(next);
    },

    // ─── Versiones con nombre ───
    /** Índice de la versión abierta en solo lectura */
    viewing,
    /** Diferencias con una versión (lo agregado y lo quitado), para el visor */
    comparison,
    /** Guarda el diseño de ahora como versión con nombre */
    saveVersion(name: string, note?: string) {
      if (draft()) void acceptDraft();
      return commit((d) => {
        const { versions, ...rest } = d;
        const snapshot = clone(rest as CadDocument);
        d.versions = [
          ...(versions ?? []),
          { name: name.trim() || nextVersionName(versions), created: new Date().toISOString(), note: note?.trim() || undefined, document: snapshot },
        ];
      });
    },
    renameVersion(i: number, name: string, note?: string) {
      return commit((d) => {
        const v = d.versions?.[i];
        if (!v) return;
        if (name.trim()) v.name = name.trim();
        if (note !== undefined) v.note = note.trim() || undefined;
      });
    },
    deleteVersion(i: number) {
      if (viewing() === i) void store.viewVersion(undefined);
      return commit((d) => {
        d.versions?.splice(i, 1);
        if (d.versions?.length === 0) delete d.versions;
      });
    },
    /** Abre una versión en solo lectura (`undefined` vuelve al diseño) */
    viewVersion(i: number | undefined) {
      if (draft()) void acceptDraft();
      const v = i === undefined ? undefined : committed()?.versions?.[i];
      batch(() => {
        setViewing(v ? i : undefined);
        setSelected(undefined);
        setComparison(null);
      });
      return send(v ? v.document : null, true);
    },
    /** Vuelve el diseño a una versión (deshacible); las versiones se conservan */
    restoreVersion(i: number) {
      const v = committed()?.versions?.[i];
      if (!v) return lastSend;
      if (draft()) void acceptDraft();
      batch(() => {
        setViewing(undefined);
        setSelected(undefined);
      });
      return commit((d) => {
        const versions = d.versions;
        for (const k of Object.keys(d)) delete (d as unknown as Record<string, unknown>)[k];
        Object.assign(d, clone(v.document), { versions });
      });
    },
    /** Compara el diseño guardado con una versión (`undefined` quita la comparación) */
    async compareVersion(i: number | undefined) {
      const v = i === undefined ? undefined : committed()?.versions?.[i];
      if (!v) return void setComparison(null);
      if (viewing() !== undefined) await store.viewVersion(undefined);
      await lastSend;
      setBusy(true);
      try {
        const buf = await invoke<ArrayBuffer>("cad_compare", { document: v.document });
        setComparison(decodeComparison(buf, i!));
      } catch (e) {
        setError(String(e));
      } finally {
        setBusy(false);
      }
    },

    stateOf(id: number): FeatureStatus | undefined {
      return result()?.status.find((s) => s.id === id);
    },

    sketchView(id: number): SketchView | undefined {
      return result()?.sketches.find((s) => s.id === id);
    },

    // ─── Parámetros y fórmulas ─────────────────────────────────────────
    /** Fórmula vinculada a un campo, si tiene */
    bindingOf: (path: string): string | undefined => doc()?.bindings?.[path],
    /** Valor calculado de un campo vinculado (o su error) */
    bindingResult: (path: string): ResolvedValue | undefined => result()?.bindings.find((b) => b.key === path),
    parameterResult: (name: string): ResolvedValue | undefined => result()?.parameters.find((p) => p.key === name),

    /** Calcula una fórmula con los parámetros actuales (lanza el error si no se puede) */
    evalExpr: (expr: string) => invoke<number>("cad_eval_expr", { expr, parameters: doc()?.parameters ?? [] }),

    /**
     * Cambia un campo numérico de una operación: con un número lo desvincula,
     * con una fórmula la guarda (y el número calculado) en un solo paso deshacible.
     */
    async setField(feature: number, path: string, text: string, write: (f: Feature, v: number) => void): Promise<string | undefined> {
      const n = plainNumber(text);
      let value = n;
      if (value === undefined) {
        try {
          value = await store.evalExpr(text);
        } catch (e) {
          return String(e);
        }
      }
      await commit((d) => {
        const f = d.features.find((x) => x.id === feature);
        if (f) write(f, value!);
        d.bindings = { ...(d.bindings ?? {}) };
        if (n === undefined) d.bindings[path] = text.trim();
        else delete d.bindings[path];
      });
      return undefined;
    },

    addParameter(name: string, expr: string) {
      return commit((d) => {
        d.parameters = [...(d.parameters ?? []), { name, expr }];
      });
    },

    /** Edita un parámetro; al renombrarlo se actualizan las fórmulas que lo usan */
    updateParameter(index: number, change: Partial<Parameter>) {
      return commit((d) => {
        const params = [...(d.parameters ?? [])];
        const old = params[index];
        if (!old) return;
        params[index] = { ...old, ...change };
        if (change.name && change.name !== old.name) {
          for (const p of params) p.expr = renameInExpr(p.expr, old.name, change.name);
          for (const k of Object.keys(d.bindings ?? {})) d.bindings![k] = renameInExpr(d.bindings![k], old.name, change.name);
          // Las configuraciones lo reemplazan por nombre
          for (const c of d.configurations ?? []) {
            if (c.values && old.name in c.values) {
              c.values[change.name] = c.values[old.name];
              delete c.values[old.name];
            }
          }
        }
        d.parameters = params;
      });
    },

    removeParameter(index: number) {
      return commit((d) => {
        d.parameters = (d.parameters ?? []).filter((_, i) => i !== index);
      });
    },

    faceRef: (face: number) => invoke<FaceRef>("cad_face_ref", { face }),
    /** Desviación del escaneo respecto del sólido: resumen y un valor por triángulo (mm, + afuera) */
    async deviation(tolerance: number): Promise<ScanDeviation> {
      const buf = await invoke<ArrayBuffer>("cad_deviation", { tolerance });
      const f = new Float32Array(buf);
      const [count, mean, meanAbs, rms, p95, maxAbs, within, tol] = f;
      return { stats: { mean, meanAbs, rms, p95, maxAbs, within, tolerance: tol }, perFace: f.slice(8, 8 + count) };
    },

    /** Medidas de una o dos cosas elegidas en el sólido mostrado */
    measure: (items: MeasureItem[]) => invoke<Measurement>("cad_measure", { items }),
    edgeRef: (edge: number) => invoke<EdgeRef>("cad_edge_ref", { edge }),
    /** Vértice del sólido en `point` como punto que lo sigue (extremo de una arista) */
    vertexSpec: (point: P3) => invoke<PointSpec>("cad_vertex_spec", { point }),
    projectEdge: (edge: number, plane: Plane) => invoke<ProjectedEdge>("cad_project_edge", { edge, plane }),
    /** Contorno de una cara: cada arista proyectada y ligada a la suya */
    projectFace: (face: number, plane: Plane) => invoke<ProjectedEdge[]>("cad_project_face", { face, plane }),
    /** Intersección del plano con el sólido o su silueta, con el sólido de antes de la operación `index` */
    projectModel: (document: CadDocument, index: number, plane: Plane, what: "section" | "silhouette") =>
      invoke<Projected[]>("cad_project_model", { document, index, plane, what }),
    /** Entidades de otro sketch (ya calculado) en este plano */
    projectSketch: (feature: number, plane: Plane) => invoke<(Projected & { entity: number })[]>("cad_project_sketch", { feature, plane }),
    solveSketch: (sketch: Sketch, drag?: [number, P2]) => invoke<SolvedSketch>("cad_solve_sketch", { sketch, drag: drag ?? null }),
    /** Relaciones casi cumplidas y cotas que faltan (solo las que restringen algo) */
    suggestSketch: (sketch: Sketch) => invoke<SketchSuggestion[]>("cad_sketch_suggest", { sketch }),
    /** Lo que define el sketch entero (cotas con lo que mide; con `relations`, también las relaciones casi cumplidas) */
    defineSketch: (sketch: Sketch, relations: boolean) => invoke<SketchConstraint[]>("cad_sketch_define", { sketch, relations }),

    /** Exporta el diseño (todas las piezas, o solo `part`) */
    /** Con `assembly`, las instancias del ensamble en su lugar */
    exportDesign: (path: string, format: string, part?: PartId, assembly = false) => invoke<number>("cad_export", { path, format, part: part ?? null, assembly }),

    // ─── Ensamble ─────────────────────────────────────────────────────
    assemblyMesh: async () => decodeCadMesh(await invoke<ArrayBuffer>("cad_assembly_mesh")),
    assemblyFaces: () => invoke<[number, number, number][]>("cad_assembly_faces"),
    assemblyConnector: (face: number) => invoke<Connector>("cad_assembly_connector", { face }),
    assemblyInterference: () => invoke<[number, number, number][]>("cad_assembly_interference"),
    /** Cambia el ensamble (deshacible); lo crea si no hay */
    editAssembly(mutate: (a: Assembly) => void) {
      return commit((d) => {
        const a = d.assembly ?? { instances: [], mates: [], next_id: 1 };
        mutate(a);
        d.assembly = a;
      });
    },

    /** Piezas que hay justo antes de la operación (para elegirlas en su diálogo) */
    partsBefore(featureId: number): Promise<PartView[]> {
      const d = doc();
      const index = d?.features.findIndex((f) => f.id === featureId) ?? -1;
      if (!d || index < 0) return Promise.resolve([]);
      return invoke<PartView[]>("cad_parts_at", { document: d, index });
    },

    /** Cambia nombre, color o visibilidad de una pieza (deshacible; no recalcula) */
    setPartProps(id: PartId, change: Omit<Partial<PartProps>, "part">) {
      return commit((d) => {
        const list = [...(d.parts ?? [])];
        const i = list.findIndex((p) => samePart(p.part, id));
        const next = { ...(i >= 0 ? list[i] : { part: id }), ...change };
        if (i >= 0) list[i] = next;
        else list.push(next);
        d.parts = list;
      });
    },

    async importStep(path: string, op: BodyOp = "join") {
      // El backend cambia el documento y descarta el borrador
      setDraft(undefined);
      setSelected(undefined);
      setBusy(true);
      try {
        await apply(await invoke<CadResult>("cad_import_step", { path, op }));
        await store.syncDocument();
      } finally {
        setBusy(false);
      }
    },

    // Escaneo → CAD
    scanPick: (kind: "plane" | "cylinder", triangle: number) => invoke<ScanPick>("cad_scan_pick", { kind, triangle, options: null }),
    scanDetect: () => invoke<Detection[]>("cad_scan_detect", { options: null }),

    async scanAdd(feature: ScanFeature) {
      setDraft(undefined);
      setSelected(undefined);
      const before = committed();
      setBusy(true);
      try {
        await apply(await invoke<CadResult>("cad_scan_add", { feature, options: null }));
        if (before) {
          undoStack.push(before);
          redoStack.length = 0;
          setHistoryVersion((v) => v + 1);
        }
        await store.syncDocument();
      } finally {
        setBusy(false);
      }
    },

    /** El backend agregó operaciones por su cuenta: traer el documento sin perder el historial */
    async syncDocument() {
      const d = await invoke<CadDocument | null>("cad_get_document");
      setDoc(d);
      const last = d?.features[d.features.length - 1];
      if (last) setSelected(last.id);
    },
  };
  return store;
}

export type CadStore = ReturnType<typeof createCadStore>;
