/**
 * Modelos de impresora que conoce Pinocchio, con su foto (de los perfiles de OrcaSlicer),
 * para que se vea a qué impresora se lamina y se manda.
 */
import k1 from "../assets/printer-k1.png";
import k1c from "../assets/printer-k1c.png";
import k1max from "../assets/printer-k1max.png";
import ad5x from "../assets/printer-ad5x.png";
import type { NetPrinterKind } from "./settings";

export interface PrinterModel {
  /** `printer_model` de Orca, igual que en el laminador */
  model: string;
  image: string;
  /** Cómo se conecta por red con el firmware que usamos */
  kind: NetPrinterKind;
}

export const PRINTER_MODELS: PrinterModel[] = [
  { model: "Creality K1", image: k1, kind: "moonraker" },
  { model: "Creality K1C", image: k1c, kind: "moonraker" },
  { model: "Creality K1 Max", image: k1max, kind: "moonraker" },
  { model: "Flashforge AD5X", image: ad5x, kind: "flashforge" },
];

/** Foto de un modelo (o de un nombre que lo contenga: «K1-436A», «AD5X»), si la hay */
export function printerImage(model: string | undefined): string | undefined {
  if (!model) return undefined;
  const m = model.toLowerCase().replace(/\s+/g, "");
  const exact = PRINTER_MODELS.find((p) => p.model.toLowerCase().replace(/\s+/g, "") === m);
  if (exact) return exact.image;
  if (m.includes("ad5x")) return ad5x;
  if (m.includes("k1max")) return k1max;
  if (m.includes("k1c")) return k1c;
  if (m.includes("k1")) return k1;
  return undefined;
}
