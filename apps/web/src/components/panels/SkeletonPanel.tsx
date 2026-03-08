import { Component, Show } from "solid-js";
import { Panel, Select, SelectOption } from "../ui";
import * as Icons from "../icons";

export interface SkeletonPreset {
  id: string;
  name: string;
  description: string;
  numBones: number;
}

export interface SkeletonPanelProps {
  presets: SkeletonPreset[];
  selectedPreset?: string;
  onPresetChange?: (presetId: string) => void;
}

export const SkeletonPanel: Component<SkeletonPanelProps> = (props) => {
  const options = (): SelectOption[] =>
    props.presets.map((p) => ({
      value: p.id,
      label: p.name,
      description: p.description,
    }));

  const selectedPreset = () =>
    props.presets.find((p) => p.id === props.selectedPreset);

  return (
    <Panel title="Esqueleto" icon={<Icons.Bone size={14} />} defaultOpen>
      <div class="space-y-3">
        {/* Preset Selector */}
        <Select
          label="Preset"
          options={options()}
          value={props.selectedPreset}
          onChange={props.onPresetChange}
          placeholder="Seleccionar tipo..."
        />

        {/* Selected Info */}
        <Show when={selectedPreset()}>
          <div class="p-2 rounded bg-current/30 space-y-1">
            <p class="text-xs text-text-muted">
              {selectedPreset()!.description}
            </p>
            <p class="text-xs">
              <span class="text-text-muted">Huesos:</span>{" "}
              <span class="text-accent font-mono">
                {selectedPreset()!.numBones}
              </span>
            </p>
          </div>
        </Show>
      </div>
    </Panel>
  );
};
