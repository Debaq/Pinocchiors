/**
 * Vista previa del laminado: lo que se lee del gcode (backend `slicer_preview`)
 * y el estado de la vista (capa, viajes, tipos ocultos). La comparten el panel
 * «Laminar» y el visor.
 */
import { createSignal } from "solid-js";

/** Lo que el backend manda en JSON junto al resultado (ver `Preview` en libs/slicer/src/gcode.rs) */
export interface GcodeMeta {
  layer_z: number[];
  layer_height: number[];
  layer_start: number[];
  travel_start: number[];
  segments: number;
  travel_segments: number;
  length_by_feature: number[];
  min: [number, number, number];
  max: [number, number, number];
}

export interface GcodePreview {
  meta: GcodeMeta;
  /** 6 floats por segmento (mm, Z arriba, como la impresora) */
  positions: Float32Array;
  widths: Float32Array;
  travels: Float32Array;
  features: Uint8Array;
  /** Cama (mm) */
  bed: [number, number];
}

/** Tipos de extrusión, en el orden de `Feature` (libs/slicer/src/gcode.rs). Colores parecidos a los de Orca */
export const FEATURES: { name: string; color: number }[] = [
  { name: "Pared interna", color: 0xffe64d },
  { name: "Pared externa", color: 0xff7d38 },
  { name: "Pared en voladizo", color: 0x1f3fff },
  { name: "Relleno", color: 0xb03029 },
  { name: "Relleno sólido", color: 0x9654cc },
  { name: "Superficie superior", color: 0xf04040 },
  { name: "Superficie inferior", color: 0x665cc7 },
  { name: "Planchado", color: 0xff8c69 },
  { name: "Puente", color: 0x4d80ba },
  { name: "Puente interno", color: 0x6699cc },
  { name: "Relleno de huecos", color: 0xffffff },
  { name: "Falda", color: 0x00876e },
  { name: "Borde", color: 0x00a383 },
  { name: "Soporte", color: 0x00ff00 },
  { name: "Interfaz de soporte", color: 0x1f611f },
  { name: "Transición de soporte", color: 0x808080 },
  { name: "Torre de purga", color: 0xb3e3ab },
  { name: "Personalizado", color: 0x5ed194 },
  { name: "Otro", color: 0x999999 },
];

/** Nombre de Orca de cada tipo (para cruzar con el filamento por tipo del motor) */
export const ORCA_ROLE_NAMES = [
  "Inner wall",
  "Outer wall",
  "Overhang wall",
  "Sparse infill",
  "Internal solid infill",
  "Top surface",
  "Bottom surface",
  "Ironing",
  "Bridge",
  "Internal Bridge",
  "Gap infill",
  "Skirt",
  "Brim",
  "Support",
  "Support interface",
  "Support transition",
  "Prime tower",
  "Custom",
  "Undefined",
];

/** Desarma el binario de `slicer_preview`: `[u32 n][u32 t][f32 6n][f32 n][f32 6t][u8 n]` */
export function decodePreview(buffer: ArrayBuffer, meta: GcodeMeta, bed: [number, number]): GcodePreview {
  const head = new DataView(buffer, 0, 8);
  const n = head.getUint32(0, true);
  const t = head.getUint32(4, true);
  let offset = 8;
  // Copias alineadas: el binario no garantiza múltiplos de 4 en el último tramo
  const floats = (count: number) => {
    const out = new Float32Array(buffer.slice(offset, offset + count * 4));
    offset += count * 4;
    return out;
  };
  const positions = floats(6 * n);
  const widths = floats(n);
  const travels = floats(6 * t);
  const features = new Uint8Array(buffer.slice(offset, offset + n));
  return { meta, positions, widths, travels, features, bed };
}

const [preview, setPreview] = createSignal<GcodePreview>();
const [visible, setVisible] = createSignal(false);
/** Última capa que se dibuja (índice) */
const [layer, setLayer] = createSignal(0);
const [travels, setTravels] = createSignal(false);
/** Tipos ocultos (índices de FEATURES) */
const [hidden, setHidden] = createSignal<number[]>([]);

export const gcodePreview = { preview, setPreview, visible, setVisible, layer, setLayer, travels, setTravels, hidden, setHidden };

/** Opciones de dibujo que lee el visor */
export interface GcodeViewOptions {
  layer: number;
  travels: boolean;
  hidden: number[];
}
