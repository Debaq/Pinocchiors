import { Component } from "solid-js";
import { Panel, Select, Slider, SelectOption } from "../ui";
import * as Icons from "../icons";

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
  { value: "fast", label: "Rápida", description: "Menor precisión, más velocidad" },
  { value: "default", label: "Normal", description: "Balance entre calidad y velocidad" },
  { value: "high", label: "Alta", description: "Máxima precisión, más lento" },
];

export const ConfigPanel: Component<ConfigPanelProps> = (props) => {
  const updateConfig = (partial: Partial<AutorigConfig>) => {
    props.onChange?.({ ...props.config, ...partial });
  };

  return (
    <Panel title="Configuración" icon={<Icons.Sliders size={14} />} defaultOpen>
      <div class="space-y-4">
        {/* Quality */}
        <Select
          label="Calidad"
          options={qualityOptions}
          value={props.config.quality}
          onChange={(value) => updateConfig({ quality: value as AutorigConfig["quality"] })}
        />

        {/* Diffusion Weight */}
        <Slider
          label="Peso de difusión"
          value={props.config.diffusionWeight}
          onChange={(value) => updateConfig({ diffusionWeight: value })}
          min={0.1}
          max={2.0}
          step={0.1}
          formatValue={(v) => v.toFixed(1)}
        />

        {/* Max Influences */}
        <Slider
          label="Máx. influencias"
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
