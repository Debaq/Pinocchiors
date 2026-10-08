import { Component, Show } from "solid-js";
import { RetopologyPanel, type RetopologyConfig, type QuadQuality } from "../panels/RetopologyPanel";
import { Checkbox } from "../ui";

export interface RetopologyStepProps {
  config: RetopologyConfig;
  onChange?: (config: RetopologyConfig) => void;
  onExecute?: () => void;
  canExecute?: boolean;
  isProcessing?: boolean;
  hasResult?: boolean;
  showQuadMesh?: boolean;
  onShowQuadMeshChange?: (show: boolean) => void;
  quality?: QuadQuality;
  /** Esqueleto y pesos usan la malla retopologizada */
  useForNextSteps?: boolean;
  onUseForNextStepsChange?: (use: boolean) => void;
}

export const RetopologyStep: Component<RetopologyStepProps> = (props) => {
  return (
    <div class="space-y-5">
      <p class="text-xs text-text-muted leading-relaxed">
        Las mallas rotas se reparan solas; la simetría deja un loop de aristas en la línea
        media, útil para el rig.
      </p>
      <RetopologyPanel
        config={props.config}
        onChange={props.onChange}
        onExecute={props.onExecute}
        canExecute={props.canExecute}
        isProcessing={props.isProcessing}
        hasResult={props.hasResult}
        showQuadMesh={props.showQuadMesh}
        onShowQuadMeshChange={props.onShowQuadMeshChange}
        quality={props.quality}
      />
      <Show when={props.hasResult}>
        <div class="space-y-2">
          <Checkbox
            label="Usar esta malla en las etapas siguientes"
            checked={props.useForNextSteps}
            onChange={props.onUseForNextStepsChange}
            disabled={props.isProcessing}
          />
          <p class="text-xs text-text-muted leading-relaxed">
            El esqueleto, los pesos, el pincel y la pose de prueba trabajan sobre la malla que se
            exporta. Desmarcado, usan la malla original (los pesos se trasladan al exportar).
            Al cambiarlo, los pesos pasan a la otra malla.
          </p>
        </div>
      </Show>
    </div>
  );
};
