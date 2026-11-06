/**
 * Las impresoras del laboratorio, una sola lista para todo Fabricar (preparar, cortar,
 * uniones, placas y laminar). Las que tienen perfil de OrcaSlicer se laminan; la Creator
 * original todavía no (lee .x3g: se le cambiará el firmware, ver PLAN_IMPRESORAS.md) y
 * sirve para preparar y exportar.
 */
import { invoke } from "@tauri-apps/api/core";
import type { SlicerInfo, SlicerPrinter } from "../components/panels/SlicerSection";

export interface Machine {
  /** Perfil del laminador (`Creality K1 (0.4 nozzle)`) o un id propio si no se lamina */
  id: string;
  /** Nombre corto para mostrar */
  name: string;
  /** Modelo (`printer_model` de Orca): elige la foto y la impresora de red */
  model: string;
  /** Cama: ancho, fondo y alto (mm) */
  bed: [number, number, number];
  nozzle: number;
  /** Carretes (AD5X: 4) */
  tools: number;
  /** Tiene perfil de laminado */
  sliceable: boolean;
}

/** Perfiles del laminador que se usan en el laboratorio, en este orden */
const SLICER_MACHINES: Record<string, string> = {
  "Creality K1 (0.4 nozzle)": "K1",
  "Flashforge AD5X 0.4 nozzle": "AD5X",
  "Creality CR-10 Max 0.4 nozzle": "CR-10 Max",
};

/** La Flashforge Creator original (2013): sin perfil de laminado por ahora */
export const CREATOR_1: Machine = {
  id: "flashforge-creator-1",
  name: "Creator",
  model: "Flashforge Creator",
  bed: [225, 145, 150],
  nozzle: 0.4,
  tools: 1,
  sliceable: false,
};

let cached: Promise<SlicerInfo | undefined> | undefined;
/** Perfiles del laminador (se piden una vez) */
export function loadSlicerInfo(): Promise<SlicerInfo | undefined> {
  cached ??= invoke<SlicerInfo>("slicer_presets").catch(() => undefined);
  return cached;
}

export function machinesFrom(printers: SlicerPrinter[] | undefined): Machine[] {
  const out: Machine[] = [];
  for (const [id, name] of Object.entries(SLICER_MACHINES)) {
    const p = printers?.find((x) => x.name === id);
    if (!p?.bed) continue;
    out.push({ id, name, model: p.model, bed: p.bed, nozzle: p.nozzle, tools: p.tools ?? 1, sliceable: true });
  }
  out.push(CREATOR_1);
  return out;
}

/** Material de Fabricar (densidad, voladizos, holguras) según el tipo del filamento */
export function materialTypeOf(filament: string): "PLA" | "PETG" | "ABS" | "ASA" | "TPU" {
  const f = filament.toUpperCase();
  if (f.includes("TPU")) return "TPU";
  if (f.includes("PETG") || f.includes("PET-") || f.includes(" PET ")) return "PETG";
  if (f.includes("ASA")) return "ASA";
  if (f.includes("ABS")) return "ABS";
  return "PLA";
}

// ── La impresora y el material de Fabricar siguen a la máquina y al filamento elegidos ──

import type { MaterialProfile, PrepSettings, PrinterProfile } from "../components/panels/PrintPrepPanel";

/** La máquina elegida: por el perfil del laminador, si no por el nombre de la de Fabricar, si no la primera */
export function machineOf(prep: PrepSettings, machines: Machine[]): Machine | undefined {
  return machines.find((m) => m.id === prep.slicer?.printer) ?? machines.find((m) => m.model === prep.printer?.name) ?? machines[0];
}

/** El filamento elegido (o el de fábrica de esa impresora, o el primero) */
export function filamentOf(prep: PrepSettings, printer: SlicerPrinter | undefined): string | undefined {
  if (!printer) return undefined;
  const f = prep.slicer?.filament;
  if (f && printer.filaments.includes(f)) return f;
  return printer.filaments.includes(printer.default_filament) ? printer.default_filament : printer.filaments[0];
}

function materialFor(type: string, current: MaterialProfile | undefined, materials: MaterialProfile[]): MaterialProfile | undefined {
  const base = materials.find((m) => m.name === type);
  if (!base) return current;
  // Lo calibrado (contracción, factor de holgura) se conserva si sigue siendo el mismo material
  return current?.name === base.name ? { ...base, ...current } : base;
}

/** Elegir máquina: cama y boquilla para preparar, y el perfil del laminador */
export function withMachine(prep: PrepSettings, m: Machine): PrepSettings {
  const same = prep.printer?.name === m.model;
  const printer: PrinterProfile = {
    name: m.model,
    build_volume: m.bed,
    nozzle: m.nozzle,
    layer_height: prep.printer?.layer_height || 0.2,
    fit_clearance: same ? prep.printer?.fit_clearance : undefined,
    hole_comp: same ? prep.printer?.hole_comp : undefined,
  };
  const slicer = { ...(prep.slicer ?? { overrides: {} }), printer: m.id };
  if (prep.slicer?.printer !== m.id) Object.assign(slicer, { process: undefined, filament: undefined, overrides: {} });
  return { ...prep, printer, slicer };
}

/** Elegir filamento: también el material de Fabricar (densidad, voladizos, holguras) */
export function withFilament(prep: PrepSettings, filament: string, materials: MaterialProfile[]): PrepSettings {
  return { ...prep, slicer: { ...(prep.slicer ?? { overrides: {} }), filament }, material: materialFor(materialTypeOf(filament), prep.material, materials) };
}

/** Si la impresora o el material de Fabricar no siguen a la máquina y el filamento, la versión corregida */
export function syncedPrep(prep: PrepSettings, info: SlicerInfo | undefined, materials: MaterialProfile[]): PrepSettings | undefined {
  const m = machineOf(prep, machinesFrom(info?.printers));
  if (!m) return undefined;
  let next = prep;
  const p = prep.printer;
  if (!p || p.name !== m.model || p.build_volume.some((v, i) => Math.abs(v - m.bed[i]) > 1e-6) || prep.slicer?.printer !== m.id) next = withMachine(next, m);
  const f = filamentOf(next, info?.printers.find((x) => x.name === m.id));
  if (f && next.material?.name !== materialTypeOf(f) && materials.some((x) => x.name === materialTypeOf(f))) next = withFilament(next, f, materials);
  return next === prep ? undefined : next;
}
