// Filtro de lo que se puede elegir en el sketch (sirve también dentro de las
// herramientas: recortar, extender, partir…). Funciones puras: se prueban con
// node (e2e/sketchPick.test.mjs).

import type { Sketch } from "./cad.ts";
import { t as tr } from "./i18n.ts";

export type SketchFilter = "all" | "points" | "lines" | "round" | "curves";

export const SKETCH_FILTERS: { value: SketchFilter; label: string; tip: string }[] = [
  { value: "all", get label() { return tr("Todo"); }, get tip() { return tr("Se elige lo que esté más cerca"); } },
  { value: "points", get label() { return tr("Puntos"); }, get tip() { return tr("Solo puntos (extremos, centros, puntos sueltos)"); } },
  { value: "lines", get label() { return tr("Líneas"); }, get tip() { return tr("Solo líneas"); } },
  { value: "round", get label() { return tr("Círculos"); }, get tip() { return tr("Solo círculos, arcos y elipses"); } },
  { value: "curves", get label() { return tr("Splines"); }, get tip() { return tr("Solo splines"); } },
];

/** ¿Lo que hay bajo el cursor pasa el filtro? */
export function passesFilter(s: Sketch, hit: { point?: number; entity?: number }, f: SketchFilter): boolean {
  if (f === "all") return true;
  if (hit.point !== undefined) return f === "points";
  const t = s.entities.find((e) => e.id === hit.entity)?.geometry.type;
  if (f === "lines") return t === "line";
  if (f === "round") return t === "circle" || t === "arc" || t === "ellipse" || t === "ellipse_arc";
  if (f === "curves") return t === "spline" || t === "bspline";
  return false;
}
