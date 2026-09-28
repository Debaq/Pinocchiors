export type ToolId = "select" | "move" | "rotate" | "scale" | "paint" | "measure" | "resetView";

export interface ToolDef {
  id: ToolId;
  label: string;
  shortcut: string;
  cursor: string;
}

export const TOOLS: ToolDef[] = [
  { id: "select", label: "Seleccionar", shortcut: "Q", cursor: "default" },
  { id: "move", label: "Mover", shortcut: "G", cursor: "move" },
  { id: "rotate", label: "Rotar", shortcut: "R", cursor: "crosshair" },
  { id: "scale", label: "Escalar esqueleto", shortcut: "S", cursor: "ns-resize" },
  { id: "paint", label: "Pintar pesos", shortcut: "B", cursor: "none" },
  { id: "measure", label: "Medir", shortcut: "M", cursor: "crosshair" },
  { id: "resetView", label: "Reset Vista", shortcut: "Home", cursor: "default" },
];
