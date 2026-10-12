/**
 * Preferencias de la app que se cambian en Configuración y se recuerdan entre
 * sesiones. Las que ya vivían en App (tema, grilla, guardado automático,
 * navegación con trackpad) siguen allí; acá van las nuevas.
 */

import { createPersisted } from "./ui-state";

/** Escalas de la interfaz que se ofrecen (1 = 100 %) */
export const UI_SCALES = [0.8, 0.9, 1, 1.1, 1.25, 1.5] as const;

const [scale, setScale] = createPersisted<number>("settings.uiScale", 1);
const [fps, setFps] = createPersisted<boolean>("settings.showFps", true);
const [format, setFormat] = createPersisted<string>("settings.exportFormat", "glb");

/** Tamaño de la interfaz (reactivo); un valor guardado que ya no existe vale 1 */
export const uiScale = () => ((UI_SCALES as readonly number[]).includes(scale()) ? scale() : 1);
export const setUiScale = setScale;

/** Contador de FPS en la cabecera */
export const showFps = fps;
export const setShowFps = setFps;

/** Formato con que arranca el panel Exportar en un proyecto nuevo */
export const defaultExportFormat = format;
export const setDefaultExportFormat = setFormat;

const PREFIX = "pinocchio.ui.";
/** Lo que no es disposición de la interfaz: se conserva al restablecer */
const KEEP = ["settings.", "templates."];

/**
 * Olvida lo recordado de la disposición (anchos, paneles abiertos, pestañas,
 * alto de la línea de tiempo…) y deja las preferencias y las plantillas
 * favoritas. Devuelve cuántas claves borró; hay que recargar para verlo
 */
export function resetLayout(): number {
  let removed = 0;
  try {
    for (const key of Object.keys(localStorage)) {
      if (!key.startsWith(PREFIX)) continue;
      const name = key.slice(PREFIX.length);
      if (KEEP.some((k) => name.startsWith(k))) continue;
      localStorage.removeItem(key);
      removed++;
    }
  } catch {
    // Sin almacenamiento: no hay nada que olvidar
  }
  return removed;
}
