import { Component, Show } from "solid-js";
import { RetopologyPanel, type RetopologyConfig, type QuadQuality, type RetopologyObject } from "../panels/RetopologyPanel";
import { Checkbox } from "../ui";
import { t } from "../../lib/i18n";

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
  objects?: RetopologyObject[];
  /** Esqueleto y pesos usan la malla retopologizada */
  useForNextSteps?: boolean;
  onUseForNextStepsChange?: (use: boolean) => void;
}

export const RetopologyStep: Component<RetopologyStepProps> = (props) => {
  return (
    <div class="space-y-5">
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
        objects={props.objects}
      />
      <Show when={props.hasResult}>
        <Checkbox
          label={t("Usar esta malla en las etapas siguientes")}
          hint={t("Esqueleto, pesos, pincel y pose usan la malla que se exporta. Desmarcado usan la original (los pesos se trasladan al exportar).")}
          checked={props.useForNextSteps}
          onChange={props.onUseForNextStepsChange}
          disabled={props.isProcessing}
        />
      </Show>
    </div>
  );
};
