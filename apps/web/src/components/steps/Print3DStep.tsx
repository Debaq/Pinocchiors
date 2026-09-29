import { Component } from "solid-js";
import {
  Print3DPanel,
  type Print3DPanelProps,
} from "../panels/Print3DPanel";

export type Print3DStepProps = Print3DPanelProps;

export const Print3DStep: Component<Print3DStepProps> = (props) => {
  return (
    <div class="space-y-5">
      <h3 class="text-sm font-semibold text-text">Fabricar</h3>
      <p class="text-xs text-text-muted leading-relaxed">
        Convertir el modelo en objetos físicos: medir, escalar y partirlo en piezas que encajan, listas para
        exportar.
      </p>
      <Print3DPanel {...props} />
    </div>
  );
};
