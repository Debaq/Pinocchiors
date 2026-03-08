import { Component } from "solid-js";
import {
  Print3DPanel,
  type Print3DPanelProps,
} from "../panels/Print3DPanel";

export type Print3DStepProps = Print3DPanelProps;

export const Print3DStep: Component<Print3DStepProps> = (props) => {
  return (
    <div class="space-y-5">
      <h3 class="text-sm font-semibold text-text">Imprimir 3D</h3>
      <p class="text-xs text-text-muted leading-relaxed">
        Análisis de dimensiones, escalado para impresión y subdivisión en piezas exportables como STL.
      </p>
      <Print3DPanel {...props} />
    </div>
  );
};
