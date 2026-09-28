import { Component, Show } from "solid-js";
import { Button } from "../ui";
import * as Icons from "../icons";
import { PlacementPanel, type PlacementPanelProps } from "../panels/PlacementPanel";

export interface ImportStepProps {
  meshInfo?: { vertices: number; faces: number; format: string };
  onImport?: () => void;
  /** Orientación del modelo (piso, frente, origen) */
  placement?: PlacementPanelProps;
}

export const ImportStep: Component<ImportStepProps> = (props) => {
  return (
    <div class="space-y-5">
      <h3 class="text-sm font-semibold text-text">Importar Modelo</h3>

      {/* Model Info */}
      <Show when={props.meshInfo}>
        <div class="space-y-2 p-3 rounded-md bg-current/30 border border-border">
          <p class="text-xs text-text-muted">Modelo actual</p>
          <div class="space-y-1">
            <div class="flex justify-between text-xs">
              <span class="text-text-muted">Formato</span>
              <span class="text-text font-mono uppercase">{props.meshInfo!.format}</span>
            </div>
            <div class="flex justify-between text-xs">
              <span class="text-text-muted">Vertices</span>
              <span class="text-text font-mono">{props.meshInfo!.vertices.toLocaleString()}</span>
            </div>
            <div class="flex justify-between text-xs">
              <span class="text-text-muted">Caras</span>
              <span class="text-text font-mono">{props.meshInfo!.faces.toLocaleString()}</span>
            </div>
          </div>
        </div>
      </Show>

      {/* Actions */}
      <div class="space-y-2">
        <Button
          onClick={props.onImport}
          variant="primary"
          fullWidth
          icon={<Icons.FolderOpen size={16} />}
        >
          {props.meshInfo ? "Importar otro modelo" : "Importar modelo"}
        </Button>
      </div>

      <Show when={props.meshInfo && props.placement}>
        <PlacementPanel {...props.placement!} />
      </Show>
    </div>
  );
};
