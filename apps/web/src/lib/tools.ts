import type { PipelineStepId } from "./pipeline";
import { t } from "./i18n.ts";

export type ToolId = "select" | "move" | "rotate" | "scale" | "paint" | "measure";

/**
 * Sobre qué actúan las herramientas, como los modos de Blender: el modelo
 * entero (Preparar, Fabricar, Exportar), el esqueleto o la pose animada.
 */
export type ToolContext = "object" | "skeleton" | "animate";

export interface ToolDef {
  id: ToolId;
  label: string;
  /** Tecla que elige la herramienta (sin tecla: G/R son operaciones modales) */
  shortcut?: string;
  /** Ayuda en el visor mientras la herramienta está activa */
  hint?: string;
}

const MEASURE: ToolDef = {
  id: "measure",
  get label() { return t("Medir"); },
  shortcut: "M",
  get hint() { return t("Medir: clic en dos puntos de la superficie · Ctrl+clic: al vértice más cercano"); },
};

export const TOOLSETS: Record<ToolContext, ToolDef[]> = {
  object: [
    { id: "select", get label() { return t("Seleccionar"); }, shortcut: "Q" },
    { id: "move", get label() { return t("Mover modelo"); }, shortcut: "G", get hint() { return t("Arrastra una flecha o un plano del gizmo para mover el modelo"); } },
    { id: "rotate", get label() { return t("Rotar modelo"); }, shortcut: "R", get hint() { return t("Arrastra un anillo del gizmo para girar el modelo en torno a su centro"); } },
    { id: "scale", get label() { return t("Escalar modelo"); }, shortcut: "S", get hint() { return t("Arrastra el gizmo para escalar el modelo (parejo, desde la base)"); } },
    MEASURE,
  ],
  skeleton: [
    { id: "select", get label() { return t("Seleccionar articulación"); }, shortcut: "Q", get hint() { return t("Clic: seleccionar articulación · G mover · R rotar · rueda/botón central: vista"); } },
    { id: "move", get label() { return t("Mover articulación"); }, get hint() { return t("Selecciona una articulación y arrastra el gizmo (G: mover con el mouse)"); } },
    { id: "rotate", get label() { return t("Rotar esqueleto / probar pose"); }, get hint() { return t("Sin pesos gira el esqueleto entero; con pesos prueba poses por articulación"); } },
    { id: "scale", get label() { return t("Escalar esqueleto"); }, shortcut: "S", get hint() { return t("El gizmo escala el esqueleto entero desde su centro"); } },
    { id: "paint", get label() { return t("Pintar pesos"); }, shortcut: "B" },
    MEASURE,
  ],
  animate: [
    { id: "select", get label() { return t("Seleccionar articulación"); }, shortcut: "Q", get hint() { return t("Clic: articulación (Shift suma, doble clic: cadena) · R girar · G mover · I insertar key"); } },
    { id: "rotate", get label() { return t("Girar articulación"); }, get hint() { return t("Selecciona una articulación y gira el gizmo (R: girar con el mouse)"); } },
    { id: "move", get label() { return t("Mover articulación"); }, get hint() { return t("Selecciona una articulación y arrastra el gizmo (G: mover con el mouse)"); } },
    MEASURE,
  ],
};

/** Contexto de las herramientas según la sección abierta */
export function toolContext(step: PipelineStepId, animating: boolean): ToolContext {
  if (step === "animate" && animating) return "animate";
  if (step === "skeleton" || step === "animate") return "skeleton";
  return "object";
}
