import { Component } from "solid-js";
import { RetopologyPanel, type RetopologyConfig } from "../panels/RetopologyPanel";

export interface RetopologyStepProps {
  config: RetopologyConfig;
  onChange?: (config: RetopologyConfig) => void;
  onExecute?: () => void;
  canExecute?: boolean;
  isProcessing?: boolean;
  hasResult?: boolean;
  showQuadMesh?: boolean;
  onShowQuadMeshChange?: (show: boolean) => void;
}

export const RetopologyStep: Component<RetopologyStepProps> = (props) => {
  return (
    <div class="space-y-5">
      <h3 class="text-sm font-semibold text-text">Retopologia</h3>
      <p class="text-xs text-text-muted leading-relaxed">
        Genera una malla de quads limpia usando QuadriFlow.
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
      />
    </div>
  );
};
