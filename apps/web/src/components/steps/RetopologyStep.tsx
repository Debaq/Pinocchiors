import { Component } from "solid-js";
import { RetopologyPanel, type RetopologyConfig, type QuadQuality } from "../panels/RetopologyPanel";

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
}

export const RetopologyStep: Component<RetopologyStepProps> = (props) => {
  return (
    <div class="space-y-5">
      <h3 class="text-sm font-semibold text-text">Retopologia</h3>
      <p class="text-xs text-text-muted leading-relaxed">
        Genera una malla de quads limpia, alineada a la forma. Las mallas rotas se reparan
        solas; la simetría deja un loop de aristas en la línea media, útil para el rig.
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
    </div>
  );
};
