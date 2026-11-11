/**
 * Calibración de impresora y filamento (PLAN_CALIBRACION.md): tipos espejo de
 * apps/desktop/src/calibrations.rs y libs/slicer/src/calib.rs, los pasos del asistente en el
 * orden recomendado y cómo se muestra cada valor.
 */
import { invoke } from "@tauri-apps/api/core";
import { createPersisted } from "./ui-state";
import { fmtNumber, t } from "./i18n";
import type { NetPrinter } from "./settings";

/**
 * Quién: la impresora física, la boquilla y el filamento (perfil del laminador). La boquilla sale del perfil de
 * máquina (`Creality K1 (0.6 nozzle)` → 0,6): cada una se calibra aparte; lo de la impresora (input shaping, VFA,
 * esquinas y escuadra) vale con cualquiera.
 */
export interface CalTarget {
  printer_id: string;
  printer_name: string;
  /** Perfil de máquina del laminador */
  machine: string;
  filament: string;
  /** Solo para forzar un diámetro (mm); normalmente se deja y sale de `machine` */
  nozzle?: number | null;
}

export interface CalValue {
  value: number;
  /** UTC, AAAA-MM-DDTHH:MMZ */
  date: string;
  test: string;
  note: string;
  history: [number, string][];
}

export interface CalibrationView {
  printer_id: string;
  printer_name: string;
  machine: string;
  filament: string;
  /** Boquilla de esta vista (mm) */
  nozzle: number;
  /** De la impresora, de la impresora con esta boquilla y del filamento con esta boquilla */
  values: Record<string, CalValue>;
  /** Filamentos calibrados en esta impresora con esta boquilla */
  filaments: string[];
  /** Boquillas con algo calibrado en esta impresora */
  nozzles: number[];
  /** Este filamento tiene algo calibrado con esta boquilla (lo de otra boquilla no cuenta) */
  calibrated: boolean;
}

export interface Block {
  value: number;
  z0: number;
  z1: number;
}
export interface Piece {
  value: number;
  mark: string;
  x: number;
  y: number;
}
export type Reading =
  | { kind: "blocks"; blocks: Block[] }
  | { kind: "pieces"; pieces: Piece[] }
  | { kind: "height"; height_mm: number; start: number; end: number }
  | { kind: "live" }
  /** Patrón con números impresos: se elige el que se ve mejor */
  | { kind: "numbers"; values: number[] }
  /** Escuadra de ejes: lado entre centros de poste y diámetro del poste (mm) */
  | { kind: "skew"; side_mm: number; pin_mm: number };

export interface CalibrationDefaults {
  start: number;
  end: number;
  step: number;
  unit: string;
  reading: Reading;
  filament_type: string;
}

export type CalibTestId =
  | "first_layer"
  | "temperature"
  | "flow_coarse"
  | "flow_fine"
  | "pressure_advance"
  | "pressure_advance_pattern"
  | "retraction"
  | "volumetric_speed"
  | "clearance"
  | "block"
  | "shaper_frequency"
  | "shaper_damping"
  | "vfa"
  | "cornering"
  | "skew";

/** Un paso del asistente */
export interface TuneStep {
  test: CalibTestId;
  /** Lo que guarda (claves de calibrations.rs) */
  keys: string[];
  title: string;
  /** Qué arregla, en una o dos líneas */
  fixes: string;
  /** Qué mirar en la pieza impresa */
  look: string;
  /** Solo en Klipper */
  klipper?: boolean;
  /** Avanzado: se muestra aparte */
  advanced?: boolean;
  /** Pasos que conviene repetir si cambia este */
  affects?: CalibTestId[];
  /** El rango se puede cambiar */
  range?: boolean;
  /** Otra forma de hacer la misma prueba (el patrón de pressure advance, además de la torre) */
  pattern?: { test: CalibTestId; look: string };
  /** Solo se aplica a las impresoras con Klipper al que se puede entrar (K1 con root); en las demás solo se anota */
  needsKlipperAccess?: boolean;
}

/** Los pasos, en el orden recomendado (los textos pasan por t() al mostrarse) */
export const TUNE_STEPS: TuneStep[] = [
  {
    test: "first_layer",
    keys: ["z_offset"],
    title: "Cama y primera capa",
    fixes: "Que la primera capa se pegue bien y salga pareja. Todo lo demás depende de esto.",
    look: "Mientras imprime, mira los parches: si las líneas se ven redondas y separadas, la boquilla está lejos; si se ven transparentes o rayadas, está cerca. Bien: una superficie lisa, sin huecos entre líneas.",
    affects: ["flow_coarse", "flow_fine"],
  },
  {
    test: "temperature",
    keys: ["temperature"],
    title: "Temperatura",
    fixes: "La temperatura de la boquilla con la que este filamento sale más limpio y fuerte.",
    look: "Elige el bloque más bajo (más frío) que todavía tenga buenos puentes y voladizos, sin hilos entre las torrecitas y con las capas bien pegadas.",
    affects: ["flow_coarse", "flow_fine", "pressure_advance", "retraction"],
    range: true,
  },
  {
    test: "flow_coarse",
    keys: ["flow_ratio"],
    title: "Flujo (grueso)",
    fixes: "Cuánto plástico sale de verdad: ni huecos ni excesos en las superficies de arriba.",
    look: "Pasa el dedo por las tapas: elige la más lisa, sin surcos (falta) ni crestas o brillo grumoso (sobra). Cada pieza tiene su número en relieve.",
    affects: ["flow_fine"],
  },
  {
    test: "flow_fine",
    keys: ["flow_ratio"],
    title: "Flujo (fino)",
    fixes: "Afina el flujo alrededor del que elegiste.",
    look: "Igual que antes: la tapa más lisa. Si dudas entre dos, quédate con la de menos flujo.",
  },
  {
    test: "pressure_advance",
    keys: ["pressure_advance"],
    title: "Presión al extruir (pressure advance)",
    fixes: "Que las esquinas salgan nítidas: sin bultos ni huecos cuando la boquilla acelera y frena.",
    look: "Mira las esquinas: elige la altura donde quedan más cuadradas, sin bulto ni hueco. Cada milímetro de la torre es un valor.",
    pattern: {
      test: "pressure_advance_pattern",
      look: "Mira el dibujo de líneas con esquinas: cada grupo tiene su número. Elige el número donde las esquinas quedan más nítidas y la línea sale pareja, sin bulto en la esquina ni hueco.",
    },
    range: true,
  },
  {
    test: "retraction",
    keys: ["retraction_length"],
    title: "Retracción",
    fixes: "Menos hilos y gotas cuando la boquilla salta de un lado a otro.",
    look: "Elige la altura más baja de la torre desde donde ya no hay hilos entre las dos columnas. Cada milímetro es un valor.",
    range: true,
  },
  {
    test: "volumetric_speed",
    keys: ["max_volumetric_speed"],
    title: "Velocidad máxima",
    fixes: "Hasta qué velocidad puede derretir plástico el hotend sin que falte.",
    look: "Busca la altura donde la pared empieza a verse rugosa, con huecos o más delgada: mídela con el calibre. Se guarda un poco menos que eso, para ir seguro.",
    range: true,
  },
  {
    test: "clearance",
    keys: ["fit_clearance"],
    title: "Holgura de los ajustes",
    fixes: "Cuánto espacio dejar entre piezas que encajan (uniones, ejes, tapas).",
    look: "Prueba la clavija en cada agujero, del 1 en adelante: ¿en cuál entra justa, con la mano y sin que baile?",
  },
  {
    test: "block",
    keys: ["shrink_x", "shrink_y", "shrink_z", "hole_comp"],
    title: "Medidas y agujeros",
    fixes: "Que las piezas salgan a la medida: cuánto encoge el material y cuánto más chicos salen los agujeros.",
    look: "Ya frío, mide el bloque con el calibre (largo, fondo y alto) y el diámetro de sus agujeros.",
  },
  {
    test: "shaper_frequency",
    keys: ["shaper_freq_x", "shaper_freq_y"],
    title: "Vibraciones (input shaping)",
    fixes: "Quitar las ondas que se ven en las paredes después de una esquina a mucha velocidad.",
    look: "Mira las paredes: mide a qué altura se ven menos ondas (ghosting) después de las esquinas.",
    klipper: true,
    advanced: true,
    affects: ["shaper_damping"],
  },
  {
    test: "shaper_damping",
    keys: ["shaper_damping"],
    title: "Vibraciones (amortiguación)",
    fixes: "Afina el input shaping con la frecuencia elegida.",
    look: "Mide a qué altura las ondas son más chicas.",
    klipper: true,
    advanced: true,
  },
  {
    test: "vfa",
    keys: ["vfa_speed"],
    title: "Marcas finas en las paredes (VFA)",
    fixes: "Velocidades en las que los motores dejan rayitas verticales en las paredes.",
    look: "Elige el bloque más rápido sin rayas verticales finas (cada bloque de 5 mm es una velocidad).",
    advanced: true,
    range: true,
  },
  {
    test: "cornering",
    keys: ["corner_speed"],
    title: "Esquinas",
    fixes: "Qué tan rápido se puede doblar en las esquinas sin que se redondeen o vibren.",
    look: "Mide a qué altura las esquinas se ven mejor: nítidas y sin ondas.",
    advanced: true,
  },
  {
    test: "skew",
    keys: ["skew_xy"],
    title: "Escuadra de los ejes (skew)",
    fixes: "Que los ángulos de las piezas sean de 90°: si el marco de la impresora quedó algo torcido, un cuadrado sale como rombo.",
    look: "Con el calibre, mide de borde exterior a borde exterior de dos postes: las dos diagonales (AC y BD) y el lado AD, el de adelante. Un cuadrado perfecto tiene las dos diagonales iguales.",
    advanced: true,
    needsKlipperAccess: true,
  },
];

/** Cómo se muestra cada valor guardado */
export const KEY_FORMAT: Record<string, { label: string; unit: string; digits: number; percent?: boolean }> = {
  z_offset: { label: "Primera capa (Z)", unit: "mm", digits: 3 },
  temperature: { label: "Temperatura", unit: "°C", digits: 0 },
  flow_ratio: { label: "Flujo", unit: "", digits: 3 },
  pressure_advance: { label: "Presión al extruir", unit: "", digits: 4 },
  retraction_length: { label: "Retracción", unit: "mm", digits: 2 },
  max_volumetric_speed: { label: "Velocidad máxima", unit: "mm³/s", digits: 1 },
  fit_clearance: { label: "Holgura", unit: "mm", digits: 3 },
  hole_comp: { label: "Agujeros", unit: "mm", digits: 3 },
  shrink_x: { label: "Contracción X", unit: "%", digits: 2, percent: true },
  shrink_y: { label: "Contracción Y", unit: "%", digits: 2, percent: true },
  shrink_z: { label: "Contracción Z", unit: "%", digits: 2, percent: true },
  shaper_freq_x: { label: "Input shaping X", unit: "Hz", digits: 1 },
  shaper_freq_y: { label: "Input shaping Y", unit: "Hz", digits: 1 },
  shaper_damping: { label: "Amortiguación", unit: "", digits: 3 },
  vfa_speed: { label: "Velocidad sin marcas", unit: "mm/s", digits: 0 },
  corner_speed: { label: "Esquinas", unit: "mm/s", digits: 1 },
  skew_xy: { label: "Escuadra de ejes", unit: "", digits: 5 },
};

/** «212 °C», «0,975», «0,40 %» */
export function fmtCal(key: string, value: number): string {
  const f = KEY_FORMAT[key];
  if (!f) return fmtNumber(value);
  const v = f.percent ? value * 100 : value;
  const n = fmtNumber(v, { maximumFractionDigits: f.digits, minimumFractionDigits: f.digits > 2 ? Math.min(2, f.digits) : 0 });
  return f.unit ? `${n} ${f.unit}` : n;
}

/** Fecha guardada (UTC) en la hora y el idioma de la interfaz */
export function fmtCalDate(date: string): string {
  const d = new Date(date.replace(/Z$/, ":00Z"));
  return Number.isNaN(d.getTime()) ? date : d.toLocaleDateString(undefined, { day: "numeric", month: "short" });
}

/** Resumen corto de lo calibrado: «212 °C · flujo 0,975 · presión al extruir 0,036» */
export function calSummary(values: Record<string, CalValue>): string {
  const parts: string[] = [];
  const v = (k: string) => values[k]?.value;
  if (v("temperature") !== undefined) parts.push(fmtCal("temperature", v("temperature")!));
  if (v("flow_ratio") !== undefined) parts.push(t("flujo {v}", { v: fmtCal("flow_ratio", v("flow_ratio")!) }));
  if (v("pressure_advance") !== undefined) parts.push(t("presión al extruir {v}", { v: fmtCal("pressure_advance", v("pressure_advance")!) }));
  if (v("retraction_length") !== undefined) parts.push(t("retracción {v}", { v: fmtCal("retraction_length", v("retraction_length")!) }));
  if (v("max_volumetric_speed") !== undefined) parts.push(fmtCal("max_volumetric_speed", v("max_volumetric_speed")!));
  if (v("z_offset") !== undefined) parts.push(t("Z {v}", { v: fmtCal("z_offset", v("z_offset")!) }));
  return parts.join(" · ");
}

/** El paso está hecho si guardó todas sus claves */
export const stepDone = (step: TuneStep, values: Record<string, CalValue>) => step.keys.every((k) => values[k] !== undefined);

/** Fecha más reciente de las claves del paso */
export const stepDate = (step: TuneStep, values: Record<string, CalValue>) =>
  step.keys.map((k) => values[k]?.date).filter((d): d is string => !!d).sort().pop();

/** Conviene repetirlo: un paso del que depende se rehízo después */
export function stepStale(step: TuneStep, values: Record<string, CalValue>): boolean {
  const mine = stepDate(step, values);
  if (!mine) return false;
  return TUNE_STEPS.some((other) => other.affects?.includes(step.test) && (stepDate(other, values) ?? "") > mine);
}

/** Usar lo calibrado al laminar (se puede apagar en Laminar) */
const [useCal, setUseCal] = createPersisted<boolean>("settings.useCalibration", true);
export const useCalibration = useCal;
export const setUseCalibration = setUseCal;

export const calTarget = (p: NetPrinter, machine: string, filament: string): CalTarget => ({ printer_id: p.id, printer_name: p.name, machine, filament });

export const getCalibration = (target: CalTarget) => invoke<CalibrationView>("calibration_get", { target });

/** ¿Esta impresora ya está calibrada con esta boquilla y este filamento? (lo de otra boquilla no cuenta) */
export const isCalibrated = (view?: CalibrationView) => !!view?.calibrated;

/** «0,4 mm» */
export const fmtNozzle = (mm: number) => `${fmtNumber(mm, { maximumFractionDigits: 2 })} mm`;

/**
 * Factor de escuadra como Klipper (`calc_skew_factor` de skew_correction.py): diagonales AC y BD y lado AD entre
 * centros de poste. Positivo si el ángulo de la esquina D pasa de 90°. Es el mismo cálculo de `skew_factor` de
 * libs/slicer/src/calib.rs
 */
export function skewFactor(ac: number, bd: number, ad: number): number | undefined {
  const inner = 2 * ac * ac + 2 * bd * bd - 4 * ad * ad;
  if (!(ac > 0 && bd > 0 && ad > 0 && inner > 0)) return undefined;
  const side = Math.sqrt(inner) / 2;
  const cos = (ac * ac - side * side - ad * ad) / (2 * side * ad);
  if (!Number.isFinite(cos) || Math.abs(cos) > 1) return undefined;
  return Math.tan(Math.PI / 2 - Math.acos(cos));
}
