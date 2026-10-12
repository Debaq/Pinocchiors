import type { Vec3 } from "./animation";
import { t } from "./i18n.ts";

/** Cámara predefinida del render: mira al modelo entero desde una dirección (en el visor, Y arriba) */
export interface RenderCamera {
  id: string;
  /** Desde dónde mira; null = la vista de ahora */
  direction: Vec3 | null;
  label: () => string;
}

export const RENDER_CAMERAS: RenderCamera[] = [
  { id: "current", direction: null, label: () => t("Vista actual") },
  { id: "front", direction: [0, 0, 1], label: () => t("Frente") },
  { id: "back", direction: [0, 0, -1], label: () => t("Atrás") },
  { id: "left", direction: [-1, 0, 0], label: () => t("Izquierda") },
  { id: "right", direction: [1, 0, 0], label: () => t("Derecha") },
  { id: "top", direction: [0, 1, 0], label: () => t("Arriba") },
  { id: "bottom", direction: [0, -1, 0], label: () => t("Abajo") },
  { id: "three-quarter", direction: [0.6, 0.45, 0.8], label: () => t("3/4") },
];

export const renderCameraById = (id: string): RenderCamera | undefined => RENDER_CAMERAS.find((c) => c.id === id);

/** Filas y columnas de una hoja con `count` vistas en como mucho `columns` columnas */
export function sheetGrid(count: number, columns: number): { cols: number; rows: number } {
  const cols = Math.max(1, Math.min(columns, count));
  return { cols, rows: Math.max(1, Math.ceil(count / cols)) };
}

/** Sufijo del archivo de una cámara: su nombre en el idioma de la interfaz, sin tildes ni caracteres de ruta */
export function cameraFileSuffix(camera: RenderCamera): string {
  const slug = camera.label().normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "");
  return slug || camera.id;
}
