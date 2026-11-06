/**
 * Resina: tipos (espejo de apps/desktop/src/resin.rs y libs/slicer/src/sla*.rs) y el
 * estado de la vista previa que comparten el panel «Laminar» y el visor.
 */
import { createSignal } from "solid-js";

export interface ResinSettings {
  layer_height: number;
  exposure: number;
  bottom_exposure: number;
  bottom_layers: number;
  light_off: number;
  bottom_light_off: number;
  lift: number;
  lift_speed: number;
  bottom_lift: number;
  bottom_lift_speed: number;
  retract_speed: number;
  pwm: number;
  bottom_pwm: number;
  density: number;
  cost_per_ml: number;
  /** Subfilas por fila para los bordes en grises (1 = apagado). */
  antialias: number;
  /** Compensaciones en mm (por lado). */
  xy_offset: number;
  elephant_foot: number;
  hole_offset: number;
  hole_max: number;
  /** Subida más lenta y alta en capas con mucha área (solo `.ctb` v3) */
  dynamic_lift: DynamicLift;
  /** Exposición por tramos de altura (solo `.ctb` v3) */
  exposure_ranges: ExposureRange[];
  /** 2 = mismos valores en todas las capas; 3 = valores por capa */
  ctb_version: number;
  /** Operaciones sobre las capas ya laminadas */
  ops: LayerOps;
  /** `.cxdlp` de la Halot-Sky: unidad de la exposición (null = sin confirmar) y espejo */
  cxdlp?: { exposure_unit: "tenths" | "seconds" | null; mirror: "none" | "x" | "y" | "xy" };
}

/** Operaciones sobre las capas (0 o false = apagada) */
export interface LayerOps {
  /** Quitar islas de hasta tantos píxeles */
  remove_islands_px: number;
  /** Cerrar huecos encerrados de hasta tantos píxeles */
  fill_holes_px: number;
  /** Alivio de la balsa: cruces cada `relief_spacing` mm con brazos de `relief_width` mm */
  raft_relief: boolean;
  relief_spacing: number;
  relief_width: number;
  /** Atenuar el interior: a más de `dim_wall` mm del borde, `dim_percent` % de luz */
  dim: boolean;
  dim_wall: number;
  dim_percent: number;
  /** Doble exposición (.ctb v3): primero achicada `double_erode` px con `double_first` de la exposición */
  double_exposure: boolean;
  double_erode: number;
  double_first: number;
}

export interface DynamicLift {
  enabled: boolean;
  /** mm²: hasta `area_min` la subida normal; desde `area_max`, `lift_max` mm a `speed_min` mm/min */
  area_min: number;
  area_max: number;
  speed_min: number;
  lift_max: number;
}

/** Exposición entre dos alturas (mm, `from` incluido) */
export interface ExposureRange {
  from: number;
  to: number;
  exposure: number;
}

/** Lo que hace la impresora en una capa */
export interface LayerParams {
  exposure: number;
  light_off: number;
  lift: number;
  lift_speed: number;
  retract_speed: number;
  pwm: number;
}

export type SupportDensity = "light" | "medium" | "heavy";

export interface SupportSettings {
  spacing: number;
  overhang_angle: number;
  tip_diameter: number;
  tip_length: number;
  tip_penetration: number;
  pillar_diameter: number;
  foot_diameter: number;
  foot_height: number;
  lift: number;
  raft_thickness: number;
  raft_margin: number;
  on_model: boolean;
  segments: number;
  tip_normal: boolean;
  tip_max_angle: number;
  braces: boolean;
  brace_diameter: number;
  brace_max_distance: number;
  tree: boolean;
  tree_radius: number;
  tree_branches: number;
  raft_bevel: number;
  /** Zonas pintadas con el pincel (mm de la impresora, como los puntos) */
  zones?: SupportZone[];
  /** Soportes laterales en partes delgadas (sección de menos de `side_max_area` mm² que suben más de `side_every` mm sin anclaje) */
  side_supports: boolean;
  side_max_area: number;
  side_every: number;
}

/** Pincelada: «block» = sin soportes adentro, «reinforce» = más soportes */
export interface SupportZone {
  center: [number, number, number];
  radius: number;
  kind: "block" | "reinforce";
}

/** Qué hace un clic en la pieza al editar */
export type ResinTool = "points" | "block" | "reinforce" | "drain" | "erase";

export interface PrepareSettings {
  supports: SupportSettings | null;
  vents: boolean;
  vent_diameter: number;
  /** Vaciar la pieza; null = maciza */
  hollow: HollowSettings | null;
  /** Drenajes puestos a mano (mm de la impresora, como los puntos) */
  drains?: [number, number, number][];
  drain_diameter: number;
  /** Un tapón imprimible por drenaje, en la placa */
  plugs: boolean;
  /** Texto en relieve delante de la balsa; vacío = sin nombre */
  label?: string;
}

/** Vaciado: pared mínima y relleno (columnas o rejilla) */
export interface HollowSettings {
  wall: number;
  infill: "none" | "pillars" | "grid";
  infill_spacing: number;
  infill_width: number;
}

export interface ResinPrinterInfo {
  id: string;
  name: string;
  resolution: [number, number];
  bed: [number, number, number];
  pixel_um: [number, number];
  format: string;
  can_write: boolean;
}

export interface ResinProfile {
  id: string;
  printer: string;
  name: string;
  settings: ResinSettings;
}

export interface ResinInfo {
  printers: ResinPrinterInfo[];
  profiles: ResinProfile[];
  supports: [SupportDensity, SupportSettings][];
  prepare: PrepareSettings;
  /** Vaciado de fábrica */
  hollow: HollowSettings;
  /** Hileras de la pieza de calibración (de adelante hacia atrás), tamaños en mm */
  calibration: { name: string; sizes: number[] }[];
}

/** Giro de la placa en grados: X, después Y, después Z (este solo para que entre), por el centro */
export interface ResinTurn {
  x: number;
  y: number;
  z: number;
}

/** Cómo queda la pieza con un giro (`resin_tilts`, orientación automática) */
export interface TiltOption {
  turn: ResinTurn;
  fits: boolean;
  height_mm: number;
  /** Capa más grande: fuerza al despegar */
  max_area_mm2: number;
  suction_mm2: number;
  islands: number;
  /** Área en voladizo que pide soportes, proyectada en la placa */
  overhang_mm2: number;
  score: number;
}

/** Giro guardado en la preparación; los proyectos viejos guardaban `{ axis, degrees }`. `undefined` = sin girar */
export function prepTurn(prep: ResinPrep): ResinTurn | undefined {
  const t = prep.tilt as ResinTurn | { axis: "x" | "y"; degrees: number } | undefined;
  if (!t) return undefined;
  const turn = "axis" in t ? { x: t.axis === "x" ? t.degrees : 0, y: t.axis === "y" ? t.degrees : 0, z: 0 } : { x: t.x ?? 0, y: t.y ?? 0, z: t.z ?? 0 };
  return turn.x || turn.y || turn.z ? turn : undefined;
}

export interface Island {
  layer: number;
  pos: [number, number, number];
  area_mm2: number;
}

export interface Cavity {
  first_layer: number;
  last_layer: number;
  floor: [number, number, number];
  volume_ml: number;
  suction_layers: number;
  max_suction_mm2: number;
  trapped: boolean;
  on_plate: boolean;
}

export interface VentHole {
  pos: [number, number];
  bottom: number;
  top: number;
  radius: number;
}

export interface SupportPoint {
  pos: [number, number, number];
  kind: "overhang" | "island" | "manual" | "side";
  /** Diámetro de la punta de este punto (mm); sin valor, el de los ajustes */
  tip?: number;
}

export type SkipReason = "too_close" | "inside" | "on_model_off" | "vent" | "too_low" | "no_room";

export interface ResinResult {
  plate: number;
  plates: number;
  unplaced: string[];
  layers: number;
  height_mm: number;
  time_s: number;
  volume_ml: number;
  grams: number;
  cost: number;
  max_area_mm2: number;
  max_area_layer: number;
  lift: number;
  islands: Island[];
  tiny_islands: number;
  cavities: Cavity[];
  unvented: Cavity[];
  vents: VentHole[];
  points: SupportPoint[];
  skipped: [number, SkipReason][];
  columns: number;
  tilted: number;
  braces: number;
  /** Resina ahorrada al vaciar (ml) y tapones de drenaje en la placa */
  hollowed_ml: number;
  plugs: number;
  /** El nombre que quedó en la balsa (en mayúsculas, sin tildes) */
  label: string;
  notes: string[];
  warnings: string[];
  bed: [number, number];
  format: string;
  file_bytes: number | null;
  layer_z: number[];
  /** Área expuesta por capa, mm² */
  layer_area: number[];
  layer_params: LayerParams[];
}

/** Lo que se elige para resina; se guarda en el proyecto con el resto de Fabricar */
export interface ResinPrep {
  /** Perfil de resina (id de `ResinProfile`) */
  profile?: string;
  /** Cambios sobre el perfil */
  settings?: Partial<ResinSettings>;
  supports: boolean;
  density: SupportDensity;
  /** Cambios sobre los soportes de fábrica de esa densidad */
  supportOverrides?: Partial<SupportSettings>;
  vents: boolean;
  ventDiameter: number;
  /** Vaciar (cambios sobre el vaciado de fábrica); undefined = maciza */
  hollow?: Partial<HollowSettings>;
  /** Diámetro de los drenajes a mano (mm) y si se imprime su tapón */
  drainDiameter?: number;
  plugs?: boolean;
  /** Nombre en la balsa y su texto (vacío = placa, resina y fecha) */
  label?: boolean;
  labelText?: string;
  /** Giro de la placa antes de preparar (ver `prepTurn`) */
  tilt?: ResinTurn;
  plate?: number;
}

export const defaultResinPrep: ResinPrep = { supports: true, density: "medium", vents: true, ventDiameter: 2.5 };

/** Pieza elevada y soportes, como triángulos (mm, Z arriba) */
export interface ResinViewData {
  model: Float32Array;
  supports: Float32Array;
  bed: [number, number];
  layerZ: number[];
  layerHeight: number;
  vents: VentHole[];
  islands: Island[];
}

/** Desarma el binario de `resin_view`: `[u32 n][u32 m][f32 9n][f32 9m]` */
export function decodeResinView(buffer: ArrayBuffer): { model: Float32Array; supports: Float32Array } {
  const head = new DataView(buffer, 0, 8);
  const n = head.getUint32(0, true);
  const m = head.getUint32(4, true);
  const model = new Float32Array(buffer.slice(8, 8 + n * 36));
  const supports = new Float32Array(buffer.slice(8 + n * 36, 8 + (n + m) * 36));
  return { model, supports };
}

/** Máscara de una capa (`resin_layer`): tamaño, origen y celda en mm, y grises con la fila 0 en y mínima */
export interface LayerMask {
  width: number;
  height: number;
  origin: [number, number];
  cell: [number, number];
  data: Uint8Array;
}

export function decodeLayerMask(buffer: ArrayBuffer): LayerMask {
  const v = new DataView(buffer);
  const width = v.getUint32(0, true);
  const height = v.getUint32(4, true);
  return {
    width,
    height,
    origin: [v.getFloat32(8, true), v.getFloat32(12, true)],
    cell: [v.getFloat32(16, true), v.getFloat32(20, true)],
    data: new Uint8Array(buffer.slice(24, 24 + width * height)),
  };
}

/** Un cambio pedido desde el visor: agregar un punto (mm, Z arriba), quitar el de ese índice o pintar ahí (pincel) */
export type PointEdit = { add: [number, number, number] } | { remove: number } | { paint: [number, number, number] };

const [data, setData] = createSignal<ResinViewData>();
/** Puntos de soporte que se dibujan (y se editan) y los que no se pudieron poner */
const [points, setPoints] = createSignal<SupportPoint[]>([]);
const [skipped, setSkipped] = createSignal<number[]>([]);
/** Zonas pintadas y la herramienta de los clics */
const [zones, setZones] = createSignal<SupportZone[]>([]);
/** Drenajes puestos a mano (mm de la impresora) y su radio, para dibujarlos */
const [drains, setDrains] = createSignal<[number, number, number][]>([]);
const [drainRadius, setDrainRadius] = createSignal(1.5);
const [tool, setTool] = createSignal<ResinTool>("points");
/** Quién recibe los clics del visor al editar puntos; undefined = no se edita */
const [editor, setEditor] = createSignal<((e: PointEdit) => void) | undefined>();
const [visible, setVisible] = createSignal(false);
/** Capa hasta la que se dibuja (índice) */
const [layer, setLayer] = createSignal(0);
const [showSupports, setShowSupports] = createSignal(true);

export const resinPreview = {
  data,
  setData,
  visible,
  setVisible,
  layer,
  setLayer,
  showSupports,
  setShowSupports,
  points,
  setPoints,
  skipped,
  setSkipped,
  editor,
  setEditor,
  zones,
  setZones,
  drains,
  setDrains,
  drainRadius,
  setDrainRadius,
  tool,
  setTool,
};

export interface ResinViewOptions {
  layer: number;
  supports: boolean;
}
