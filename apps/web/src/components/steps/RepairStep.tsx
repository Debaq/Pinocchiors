import { Component } from "solid-js";
import {
  RepairPanel,
  type RepairPanelProps,
} from "../panels/RepairPanel";

export type RepairStepProps = RepairPanelProps;

export const RepairStep: Component<RepairStepProps> = (props) => {
  return (
    <div class="space-y-5">
      <h3 class="text-sm font-semibold text-text">Reparar Malla</h3>
      <p class="text-xs text-text-muted leading-relaxed">
        Analiza y repara problemas de geometría: agujeros, vértices duplicados, caras degeneradas y normales inconsistentes.
      </p>
      <RepairPanel {...props} />
    </div>
  );
};
