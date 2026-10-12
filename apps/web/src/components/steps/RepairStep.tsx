import { Component } from "solid-js";
import {
  RepairPanel,
  type RepairPanelProps,
} from "../panels/RepairPanel";
import { Hint } from "../ui";
import { t } from "../../lib/i18n";

export type RepairStepProps = RepairPanelProps;

export const RepairStep: Component<RepairStepProps> = (props) => {
  return (
    <div class="space-y-5">
      <div class="flex items-center justify-between">
        <h3 class="text-sm font-semibold text-text">{t("Reparar Malla")}</h3>
        <Hint text={t("Analiza y repara problemas de geometría: agujeros, vértices duplicados, caras degeneradas y normales inconsistentes.")} />
      </div>
      <RepairPanel {...props} />
    </div>
  );
};
