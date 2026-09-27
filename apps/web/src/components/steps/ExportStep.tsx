import { Component } from "solid-js";
import { Button, Checkbox } from "../ui";
import * as Icons from "../icons";

export interface ExportStepProps {
  onExport?: () => void;
  canExport?: boolean;
  autorigComplete?: boolean;
  hasQuadMesh?: boolean;
  includeRig?: boolean;
  onIncludeRigChange?: (value: boolean) => void;
  useRetopology?: boolean;
  onUseRetopologyChange?: (value: boolean) => void;
}

export const ExportStep: Component<ExportStepProps> = (props) => {
  return (
    <div class="space-y-5">
      <h3 class="text-sm font-semibold text-text">Exportar Modelo</h3>
      <p class="text-xs text-text-muted leading-relaxed">
        Exporta el modelo, opcionalmente con el rig (esqueleto y pesos) y la malla retopologizada.
      </p>

      {/* Opciones */}
      <div class="space-y-3 p-3 rounded-md bg-current/30 border border-border">
        <Checkbox
          label="Incluir esqueleto y pesos"
          checked={props.includeRig && props.autorigComplete}
          disabled={!props.autorigComplete}
          onChange={props.onIncludeRigChange}
        />
        <Checkbox
          label="Usar malla retopologizada"
          checked={props.useRetopology && props.hasQuadMesh}
          disabled={!props.hasQuadMesh}
          onChange={props.onUseRetopologyChange}
        />
        <p class="text-[10px] text-text-dim leading-relaxed">
          {props.autorigComplete ? "" : "Ejecuta el autorig para exportar el rig. "}
          {props.hasQuadMesh ? "" : "Ejecuta la retopología para exportar la malla de quads."}
        </p>
      </div>

      <p class="text-[10px] text-text-dim leading-relaxed">
        Formatos: GLB y USDZ conservan el rig; OBJ y STL exportan solo la geometría. El formato se elige en
        el diálogo de guardado.
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
