import { Component } from "solid-js";
import {
  RepairPanel,
  type RepairPanelProps,
} from "../panels/RepairPanel";
import { t } from "../../lib/i18n";

export type RepairStepProps = RepairPanelProps;

export const RepairStep: Component<RepairStepProps> = (props) => {
  return (
    <div class="space-y-5">
      <h3 class="text-sm font-semibold text-text">{t("Reparar Malla")}</h3>
      <p class="text-xs text-text-muted leading-relaxed">
        {t("Analiza y repara problemas de geometría: agujeros, vértices duplicados, caras degeneradas y normales inconsistentes.")}
      </p>
      <RepairPanel {...props} />
    </div>
  );
};
