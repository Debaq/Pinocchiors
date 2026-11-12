/**
 * Modelos de impresora que conoce Pinocchio, con su foto (las de filamento, de los perfiles
 * de OrcaSlicer; las de resina, de las que muestra Lychee Slicer), para que se vea a qué
 * impresora se lamina y se manda.
 */
import k1 from "../assets/printer-k1.png";
import k1c from "../assets/printer-k1c.png";
import k1max from "../assets/printer-k1max.png";
import ad5x from "../assets/printer-ad5x.png";
import cr10max from "../assets/printer-cr10max.png";
import ld002h from "../assets/printer-ld002h.png";
import halotsky from "../assets/printer-halotsky.png";
import creatorpro from "../assets/printer-creatorpro.png";
import type { NetPrinterKind } from "./settings";

/** Modelo de una impresora con Klipper que no está en la lista */
export const GENERIC_KLIPPER = "Klipper";

export interface PrinterModel {
  /** `printer_model` de Orca, igual que en el laminador */
  model: string;
  image?: string;
  /** Cómo se conecta por red con el firmware que usamos */
  kind: NetPrinterKind;
}

export const PRINTER_MODELS: PrinterModel[] = [
  { model: "Creality K1", image: k1, kind: "moonraker" },
  { model: "Creality K1C", image: k1c, kind: "moonraker" },
  { model: "Creality K1 Max", image: k1max, kind: "moonraker" },
  { model: "Flashforge AD5X", image: ad5x, kind: "flashforge" },
  // Con Marlin no tiene red; con una Raspberry y Klipper + Moonraker queda como la K1
  { model: "Creality CR-10 Max", image: cr10max, kind: "moonraker" },
  // La de fábrica es Marlin con Bowden; esta es la del taller con Klipper, extrusor directo y
  // retracción corta (perfiles aparte: la de fábrica queda tal cual para quien no la modificó)
  { model: "Creality CR-10 Max (Klipper)", image: cr10max, kind: "moonraker" },
  // De fábrica lee .x3g por USB/SD; con Klipper se maneja por red como las demás
  { model: "Flashforge Creator Pro", image: creatorpro, kind: "moonraker" },
  // Cualquier otra con Klipper + Moonraker (sin foto ni perfil del laminador)
  { model: GENERIC_KLIPPER, kind: "moonraker" },
];

/** Nombre corto de un modelo para mostrar: «Creality CR-10 Max» → «CR-10 Max» */
export const shortModel = (model: string) => model.replace(/^(Creality|Flashforge)\s+/i, "");

/** Foto de un modelo (o de un nombre que lo contenga: «K1-436A», «AD5X»), si la hay */
export function printerImage(model: string | undefined): string | undefined {
  if (!model) return undefined;
  const m = model.toLowerCase().replace(/\s+/g, "");
  const exact = PRINTER_MODELS.find((p) => p.model.toLowerCase().replace(/\s+/g, "") === m);
  if (exact) return exact.image;
  if (m.includes("creator") && m.includes("pro") && !/creator(pro)?[0-9]/.test(m)) return creatorpro;
  if (m.includes("ad5x")) return ad5x;
  // Resina: no van en PRINTER_MODELS (no se manejan por red todavía)
  if (m.includes("ld-002h") || m.includes("ld002h")) return ld002h;
  if (m.includes("halot")) return halotsky;
  if (m.includes("cr-10max") || m.includes("cr10max")) return cr10max;
  if (m.includes("k1max")) return k1max;
  if (m.includes("k1c")) return k1c;
  if (m.includes("k1")) return k1;
  return undefined;
}
