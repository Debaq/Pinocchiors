import type { PipelineStepId } from "./pipeline";

export type ToolId = "select" | "move" | "rotate" | "scale" | "paint" | "measure";

/**
 * Sobre qué actúan las herramientas, como los modos de Blender: el modelo
 * entero (Preparar, Imprimir 3D, Exportar), el esqueleto o la pose animada.
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
  label: "Medir",
  shortcut: "M",
  hint: "Medir: clic en dos puntos de la superficie · Ctrl+clic: al vértice más cercano",
};

export const TOOLSETS: Record<ToolContext, ToolDef[]> = {
  object: [
    { id: "select", label: "Seleccionar", shortcut: "Q" },
    { id: "move", label: "Mover modelo", shortcut: "G", hint: "Arrastra una flecha o un plano del gizmo para mover el modelo" },
    { id: "rotate", label: "Rotar modelo", shortcut: "R", hint: "Arrastra un anillo del gizmo para girar el modelo en torno a su centro" },
    { id: "scale", label: "Escalar modelo", shortcut: "S", hint: "Arrastra el gizmo para escalar el modelo (parejo, desde la base)" },
    MEASURE,
  ],
  skeleton: [
    { id: "select", label: "Seleccionar articulación", shortcut: "Q", hint: "Clic: seleccionar articulación · G mover · R rotar · rueda/botón central: vista" },
    { id: "move", label: "Mover articulación", hint: "Selecciona una articulación y arrastra el gizmo (G: mover con el mouse)" },
    { id: "rotate", label: "Rotar esqueleto / probar pose", hint: "Sin pesos gira el esqueleto entero; con pesos prueba poses por articulación" },
    { id: "scale", label: "Escalar esqueleto", shortcut: "S", hint: "El gizmo escala el esqueleto entero desde su centro" },
    { id: "paint", label: "Pintar pesos", shortcut: "B" },
    MEASURE,
  ],
  animate: [
    { id: "select", label: "Seleccionar articulación", shortcut: "Q", hint: "Clic: articulación · R girar · G mover la raíz · I insertar key" },
    { id: "rotate", label: "Girar articulación", hint: "Selecciona una articulación y gira el gizmo (R: girar con el mouse)" },
    { id: "move", label: "Mover raíz", hint: "Selecciona la raíz y arrastra el gizmo (G: mover con el mouse)" },
    MEASURE,
  ],
};

/** Contexto de las herramientas según la sección abierta */
export function toolContext(step: PipelineStepId, animating: boolean): ToolContext {
  if (step === "animate" && animating) return "animate";
  if (step === "skeleton" || step === "animate") return "skeleton";
  return "object";
}
