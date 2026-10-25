/**
 * Estado de la ventana (maximizada, pantalla completa), compartido entre la
 * cabecera y el marco de la app: la ventana no tiene decoraciones del sistema.
 */

import { createSignal } from "solid-js";
import { getCurrentWindow } from "@tauri-apps/api/window";

const [maximized, setMaximized] = createSignal(false);
const [fullscreen, setFullscreen] = createSignal(false);
export { maximized as windowMaximized, fullscreen as windowFullscreen };

let watching = false;

async function refresh() {
  const win = getCurrentWindow();
  try {
    setMaximized(!!(await win.isMaximized()));
    setFullscreen(!!(await win.isFullscreen()));
  } catch {
    // Sin Tauri (pruebas en el navegador): ventana normal
  }
}

/** Empieza a seguir los cambios de la ventana (una sola vez) */
export function watchWindow(): void {
  if (watching) return;
  watching = true;
  void refresh();
  getCurrentWindow()
    .onResized(() => void refresh())
    .catch(() => {});
}

export async function toggleFullscreen(): Promise<void> {
  try {
    await getCurrentWindow().setFullscreen(!fullscreen());
  } catch {
    // Sin Tauri no hay pantalla completa
  }
  await refresh();
}
