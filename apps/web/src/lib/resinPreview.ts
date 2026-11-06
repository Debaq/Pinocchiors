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
}

export interface PrepareSettings {
  supports: SupportSettings | null;
  vents: boolean;
  vent_diameter: number;
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
  /** Hileras de la pieza de calibración (de adelante hacia atrás), tamaños en mm */
  calibration: { name: string; sizes: number[] }[];
}

export type TiltAxis = "x" | "y";

export interface TiltOption {
  axis: TiltAxis;
  degrees: number;
  fits: boolean;
  height_mm: number;
  max_area_mm2: number;
  suction_mm2: number;
  islands: number;
  score: number;
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
  kind: "overhang" | "island" | "manual";
}

export type SkipReason = "too_close" | "inside" | "on_model_off" | "vent" | "too_low";

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
  notes: string[];
  warnings: string[];
  bed: [number, number];
  format: string;
  file_bytes: number | null;
  layer_z: number[];
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
  /** Giro de la placa antes de preparar */
  tilt?: { axis: TiltAxis; degrees: number };
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

/** Un cambio de puntos pedido desde el visor: agregar uno (mm, Z arriba) o quitar el de ese índice */
export type PointEdit = { add: [number, number, number] } | { remove: number };

const [data, setData] = createSignal<ResinViewData>();
/** Puntos de soporte que se dibujan (y se editan) y los que no se pudieron poner */
const [points, setPoints] = createSignal<SupportPoint[]>([]);
const [skipped, setSkipped] = createSignal<number[]>([]);
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
};

export interface ResinViewOptions {
  layer: number;
  supports: boolean;
}
