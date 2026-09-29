import { Component, Show } from "solid-js";
import { PlacementPanel, type PlacementPanelProps } from "./PlacementPanel";

export interface ObjectTabProps {
  meshInfo?: { vertices: number; faces: number; format: string };
  /** Transformar el modelo: piso, frente, girar, espejar, escalar, mover, origen */
  placement?: PlacementPanelProps;
}

/** Pestaña Objeto: el modelo y sus transformaciones, disponible en cualquier espacio */
export const ObjectTab: Component<ObjectTabProps> = (props) => (
  <Show
    when={props.meshInfo}
    fallback={<p class="text-xs text-text-muted">Importa un modelo (Archivo → Importar modelo) para transformarlo.</p>}
  >
    {(info) => (
      <div class="space-y-4">
        <div class="flex justify-between gap-2 text-xs px-3 py-2 rounded-md bg-current/30 border border-border">
          <span class="text-text font-mono uppercase">{info().format}</span>
          <span class="text-text-muted">
            <span class="text-text font-mono">{info().vertices.toLocaleString()}</span> vértices ·{" "}
            <span class="text-text font-mono">{info().faces.toLocaleString()}</span> caras
          </span>
        </div>
        <Show when={props.placement}>{(placement) => <PlacementPanel {...placement()} />}</Show>
      </div>
    )}
  </Show>
);
