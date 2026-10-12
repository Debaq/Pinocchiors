/**
 * Escala de toda la interfaz. En la app de escritorio se usa el zoom del
 * webview (escala texto, bordes y lienzos por igual); en la vista previa del
 * navegador, el `zoom` de CSS.
 */

import { createEffect } from "solid-js";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { uiScale } from "./settings";

let tauriZoom: boolean | undefined;

export async function applyUiScale(scale: number): Promise<void> {
  if (tauriZoom !== false) {
    try {
      await getCurrentWebview().setZoom(scale);
      tauriZoom = true;
      document.body.style.zoom = "";
      return;
    } catch {
      // Sin Tauri (vista previa) o sin permiso: queda el zoom de CSS
      tauriZoom = false;
    }
  }
  document.body.style.zoom = scale === 1 ? "" : String(scale);
}

/** Aplica la escala guardada y la sigue; llamar dentro de un root reactivo */
export function followUiScale(): void {
  createEffect(() => void applyUiScale(uiScale()));
}
