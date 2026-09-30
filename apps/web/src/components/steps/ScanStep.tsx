import { Component, Show } from "solid-js";
import { Button } from "../ui";
import type { CloudInfo } from "../../lib/scanCloud";
import * as Icons from "../icons";

export type { ScanMeshSettings } from "../layout/ScanEditor";

export interface ScanStepProps {
  /** El panel de Orizon3D está abierto a la izquierda del visor */
  editorOpen?: boolean;
  onOpenEditor?: () => void;
  /** Nube en edición, si hay */
  cloud?: CloudInfo;
}

/**
 * Escáneres 3D (Orizon3D) en el panel de propiedades: el trabajo se hace en
 * el panel lateral junto al visor; aquí queda un resumen y cómo abrirlo.
 */
export const ScanStep: Component<ScanStepProps> = (props) => (
  <div class="space-y-4">
    <h3 class="text-sm font-semibold text-text">Orizon3D</h3>
    <p class="text-xs text-text-muted leading-relaxed">
      Escáneres 3D Revopoint POP 2 / POP 3 por USB. Captura, limpieza de la nube y creación del modelo están en el panel
      lateral, junto al visor, en pestañas: Captura, Nube, Malla y Calibración.
    </p>
    <Show when={props.cloud}>
      {(cloud) => (
        <div class="space-y-1 text-xs">
          <div class="flex justify-between gap-3">
            <span class="text-text-muted">Nube en edición</span>
            <span class="text-text font-mono truncate">{cloud().source}</span>
          </div>
          <div class="flex justify-between gap-3">
            <span class="text-text-muted">Puntos</span>
            <span class="text-text font-mono">{cloud().points.toLocaleString()}</span>
          </div>
        </div>
      )}
    </Show>
    <Show when={!props.editorOpen}>
      <Button variant="primary" size="sm" fullWidth icon={<Icons.SidebarSimple size={14} />} onClick={() => props.onOpenEditor?.()}>
        Abrir el panel de Orizon3D
      </Button>
    </Show>
  </div>
);
