import { Component } from "solid-js";
import { Panel, Select, Slider, SelectOption } from "../ui";
import * as Icons from "../icons";
import { t } from "../../lib/i18n";

export interface AutorigConfig {
  quality: "fast" | "default" | "high";
  diffusionWeight: number;
  maxInfluences: number;
}

export interface ConfigPanelProps {
  config: AutorigConfig;
  onChange?: (config: AutorigConfig) => void;
}

const qualityOptions: SelectOption[] = [
  { value: "fast", get label() { return t("Rápida"); }, get description() { return t("Menor precisión, más velocidad"); } },
  { value: "default", get label() { return t("Normal"); }, get description() { return t("Balance entre calidad y velocidad"); } },
  { value: "high", get label() { return t("Alta"); }, get description() { return t("Máxima precisión, más lento"); } },
];

export const ConfigPanel: Component<ConfigPanelProps> = (props) => {
  const updateConfig = (partial: Partial<AutorigConfig>) => {
    props.onChange?.({ ...props.config, ...partial });
  };

  return (
    <Panel title={t("Configuración")} icon={<Icons.Sliders size={14} />} defaultOpen>
      <div class="space-y-4">
        {/* Quality */}
        <Select
          label={t("Calidad")}
          options={qualityOptions}
          value={props.config.quality}
          onChange={(value) => updateConfig({ quality: value as AutorigConfig["quality"] })}
        />

        {/* Diffusion Weight */}
        <Slider
          label={t("Peso de difusión")}
          value={props.config.diffusionWeight}
          onChange={(value) => updateConfig({ diffusionWeight: value })}
          min={0.1}
          max={2.0}
          step={0.1}
          formatValue={(v) => v.toFixed(1)}
        />

        {/* Max Influences */}
        <Slider
          label={t("Máx. influencias")}
          value={props.config.maxInfluences}
          onChange={(value) => updateConfig({ maxInfluences: Math.round(value) })}
          min={1}
          max={8}
          step={1}
          formatValue={(v) => v.toString()}
        />
      </div>
    </Panel>
  );
};
