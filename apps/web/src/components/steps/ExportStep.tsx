import { Component } from "solid-js";
import { Button } from "../ui";
import * as Icons from "../icons";

export interface ExportStepProps {
  onExport?: () => void;
  canExport?: boolean;
  autorigComplete?: boolean;
}

export const ExportStep: Component<ExportStepProps> = (props) => {
  return (
    <div class="space-y-5">
      <h3 class="text-sm font-semibold text-text">Exportar Modelo</h3>
      <p class="text-xs text-text-muted leading-relaxed">
        Exporta el modelo con esqueleto y pesos a distintos formatos.
      </p>

      {/* Info */}
      <div class="space-y-2 p-3 rounded-md bg-current/30 border border-border">
        <div class="flex justify-between text-xs">
          <span class="text-text-muted">Esqueleto</span>
          <span class="text-text">Incluido</span>
        </div>
        <div class="flex justify-between text-xs">
          <span class="text-text-muted">Pesos</span>
          <span class={props.autorigComplete ? "text-green" : "text-text-dim"}>
            {props.autorigComplete ? "Incluidos" : "No disponibles"}
          </span>
        </div>
      </div>

      <p class="text-[10px] text-text-dim leading-relaxed">
        Formatos soportados: GLB, OBJ, STL, USDZ. El formato se elige en el diálogo de guardado.
      </p>

      {/* Export Button */}
      <Button
        onClick={props.onExport}
        disabled={!props.canExport}
        variant="primary"
        fullWidth
        icon={<Icons.Export size={16} />}
      >
        Exportar
      </Button>
    </div>
  );
};
