import { Component, Show } from "solid-js";
import { Button } from "../ui";
import * as Icons from "../icons";

/** Lo que tienen en común los paneles de Remallar: vista previa, aplicar y descartar */
export interface RemeshActionsProps {
  hasPreview: boolean;
  onPreview: () => void;
  onApply: () => void;
  onDiscard: () => void;
  canExecute: boolean;
  isProcessing: boolean;
}

export const RemeshActions: Component<RemeshActionsProps> = (props) => (
  <>
    <div class="flex gap-2">
      <Button
        data-remesh-preview
        class="flex-1"
        onClick={() => props.onPreview()}
        disabled={!props.canExecute || props.isProcessing}
        icon={<Icons.Eye size={14} />}
      >
        Vista previa
      </Button>
      <Button
        data-remesh-apply
        variant="primary"
        class="flex-1"
        onClick={() => props.onApply()}
        disabled={!props.canExecute || props.isProcessing}
        loading={props.isProcessing}
      >
        Aplicar
      </Button>
    </div>
    <Show when={props.hasPreview}>
      <Button data-remesh-discard variant="ghost" size="sm" fullWidth onClick={() => props.onDiscard()} disabled={props.isProcessing}>
        Descartar la vista previa
      </Button>
    </Show>
  </>
);
