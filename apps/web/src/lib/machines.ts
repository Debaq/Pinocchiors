/**
 * Las impresoras del laboratorio, una sola lista para todo Fabricar (preparar, cortar,
 * uniones, placas y laminar). Las de filamento con perfil de OrcaSlicer se laminan con su
 * motor; las de resina con el núcleo SLA propio (PLAN_SLA.md); la Creator Pro todavía
 * no (lee .x3g: se le cambiará el firmware, ver PLAN_IMPRESORAS.md) y sirve para preparar
 * y exportar. Cada boquilla de una impresora es una máquina aparte (su perfil del laminador).
 */
import { invoke } from "@tauri-apps/api/core";
import { fmtNumber } from "./i18n";
import { selectedNetPrinter } from "./settings";
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
  /** Filamento (OrcaSlicer) o resina (núcleo SLA) */
  kind: "fdm" | "resin";
}

/** Impresoras de resina (ids de `libs/slicer/src/sla.rs`); la «boquilla» es la holgura base */
export const RESIN_MACHINES: Machine[] = [
  { id: "ld-002h", name: "LD-002H", model: "Creality LD-002H", bed: [82.62, 130.56, 160], nozzle: 0.15, tools: 1, sliceable: true, kind: "resin" },
  { id: "halot-sky", name: "Halot-Sky", model: "Creality Halot-Sky", bed: [192, 120, 200], nozzle: 0.15, tools: 1, sliceable: true, kind: "resin" },
];

/** Si el id del laminador es de una impresora de resina */
export const isResinId = (id: string | undefined) => RESIN_MACHINES.some((m) => m.id === id);

/** Modelos del laminador que se usan en el laboratorio, en este orden (con todas sus boquillas) */
const SLICER_MODELS: Record<string, string> = {
  "Creality K1": "K1",
  "Flashforge AD5X": "AD5X",
  "Creality CR-10 Max": "CR-10 Max",
};

/** La Flashforge Creator Pro (dos extrusores, lee .x3g): sin perfil de laminado por ahora */
export const CREATOR_1: Machine = {
  id: "flashforge-creator-1",
  name: "Creator Pro",
  model: "Flashforge Creator Pro",
  bed: [227, 148, 150],
  nozzle: 0.4,
  tools: 2,
  sliceable: false,
  kind: "fdm",
};

let cached: Promise<SlicerInfo | undefined> | undefined;
/** Perfiles del laminador (se piden una vez) */
export function loadSlicerInfo(): Promise<SlicerInfo | undefined> {
  cached ??= invoke<SlicerInfo>("slicer_presets").catch(() => undefined);
  return cached;
}

/** Todas las máquinas: cada boquilla de cada modelo (la de 0.4 primero), resina y la Creator Pro */
export function machinesFrom(printers: SlicerPrinter[] | undefined): Machine[] {
  const out: Machine[] = [];
  for (const [model, name] of Object.entries(SLICER_MODELS)) {
    const variants = (printers ?? []).filter((x) => x.model === model && x.bed);
    variants.sort((a, b) => Number(a.nozzle !== 0.4) - Number(b.nozzle !== 0.4) || a.nozzle - b.nozzle);
    for (const p of variants) out.push({ id: p.name, name, model, bed: p.bed!, nozzle: p.nozzle, tools: p.tools ?? 1, sliceable: true, kind: "fdm" });
  }
  out.push(...RESIN_MACHINES, CREATOR_1);
  return out;
}

/** Las boquillas de la misma impresora que `m` (de menor a mayor) */
export function nozzlesOf(m: Machine, machines: Machine[]): Machine[] {
  return machines.filter((x) => x.model === m.model && x.kind === m.kind).sort((a, b) => a.nozzle - b.nozzle);
}

/**
 * Una tarjeta por impresora: la elegida tal cual; las demás con la misma boquilla que la
 * elegida si la tienen, si no la de 0.4.
 */
export function machineCards(machines: Machine[], current: Machine | undefined): Machine[] {
  const out: Machine[] = [];
  for (const m of machines) {
    if (out.some((x) => x.model === m.model)) continue;
    const same = nozzlesOf(m, machines);
    out.push(same.find((x) => x.id === current?.id) ?? same.find((x) => x.nozzle === current?.nozzle) ?? m);
  }
  return out;
}

/** Nombre de una boquilla: «0,4 mm» */
export const nozzleLabel = (n: number) => `${fmtNumber(n, { maximumFractionDigits: 2 })} mm`;

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

/**
 * Elegir máquina: cama y boquilla para preparar, y el perfil del laminador. Con otra boquilla
 * de la misma impresora se conserva el filamento; la calidad vuelve a la del perfil.
 */
export function withMachine(prep: PrepSettings, m: Machine): PrepSettings {
  const sameModel = prep.printer?.name === m.model;
  const same = sameModel && prep.printer?.nozzle === m.nozzle;
  const printer: PrinterProfile = {
    name: m.model,
    build_volume: m.bed,
    nozzle: m.nozzle,
    layer_height: same ? prep.printer?.layer_height || 0.2 : m.kind === "resin" ? 0.05 : Math.round(m.nozzle * 50) / 100,
    fit_clearance: same ? prep.printer?.fit_clearance : undefined,
    hole_comp: same ? prep.printer?.hole_comp : undefined,
  };
  const slicer = { ...(prep.slicer ?? { overrides: {} }), printer: m.id };
  if (prep.slicer?.printer !== m.id) Object.assign(slicer, sameModel ? { process: undefined, overrides: {} } : { process: undefined, filament: undefined, overrides: {} });
  return { ...prep, printer, slicer };
}

/** Elegir filamento: también el material de Fabricar (densidad, voladizos, holguras) */
export function withFilament(prep: PrepSettings, filament: string, materials: MaterialProfile[]): PrepSettings {
  return { ...prep, slicer: { ...(prep.slicer ?? { overrides: {} }), filament }, material: materialFor(materialTypeOf(filament), prep.material, materials) };
}

/** La máquina (modelo y boquilla instalada) de la impresora de red elegida, si su modelo tiene perfil */
export function machineOfNetPrinter(machines: Machine[]): Machine | undefined {
  const p = selectedNetPrinter();
  if (!p) return undefined;
  const options = machines.filter((m) => m.model === p.model && m.kind === "fdm" && m.sliceable);
  return options.find((m) => m.nozzle === (p.nozzle ?? 0.4)) ?? options.find((m) => m.nozzle === 0.4) ?? options[0];
}

/** Última impresora de red (con su boquilla) a la que ya se siguió: solo un cambio nuevo mueve la máquina de Fabricar */
let lastNetKey: string | undefined;

/**
 * Si la impresora o el material de Fabricar no siguen a la máquina y el filamento, la versión corregida.
 * Al elegir otra impresora de red (o cambiarle la boquilla) Fabricar pasa a su máquina; después se puede
 * elegir otra a mano.
 */
export function syncedPrep(prep: PrepSettings, info: SlicerInfo | undefined, materials: MaterialProfile[]): PrepSettings | undefined {
  const all = machinesFrom(info?.printers);
  const net = machineOfNetPrinter(all);
  const key = `${selectedNetPrinter()?.id ?? ""}|${net?.id ?? ""}`;
  const original = prep;
  if (lastNetKey !== undefined && key !== lastNetKey && net && prep.slicer?.printer !== net.id) prep = withMachine(prep, net);
  lastNetKey = key;
  const m = machineOf(prep, all);
  if (!m) return prep === original ? undefined : prep;
  let next = prep;
  const p = prep.printer;
  if (!p || p.name !== m.model || p.build_volume.some((v, i) => Math.abs(v - m.bed[i]) > 1e-6) || prep.slicer?.printer !== m.id) next = withMachine(next, m);
  // Resina: el material de Fabricar es «Resina» (densidad, voladizos, holguras)
  if (m.kind === "resin") {
    const resin = materials.find((x) => x.name === "Resina");
    if (resin && next.material?.name !== "Resina") next = { ...next, material: resin };
    return next === original ? undefined : next;
  }
  const f = filamentOf(next, info?.printers.find((x) => x.name === m.id));
  if (f && next.material?.name !== materialTypeOf(f) && materials.some((x) => x.name === materialTypeOf(f))) next = withFilament(next, f, materials);
  return next === original ? undefined : next;
}
