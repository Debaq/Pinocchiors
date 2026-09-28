import { Component, Show } from "solid-js";
import { Panel, Checkbox, Select, SelectOption } from "../ui";
import * as Icons from "../icons";

export interface ViewSettings {
  showMesh: boolean;
  showWireframe: boolean;
  showSkeleton: boolean;
  showWeights: boolean;
  selectedBone: number;
  /** Materiales y texturas del archivo de origen */
  showTextures?: boolean;
}

export interface ViewPanelProps {
  settings: ViewSettings;
  onChange?: (settings: ViewSettings) => void;
  boneNames?: string[];
  hasWeights?: boolean;
}

export const ViewPanel: Component<ViewPanelProps> = (props) => {
  const updateSettings = (partial: Partial<ViewSettings>) => {
    props.onChange?.({ ...props.settings, ...partial });
  };

  const boneOptions = (): SelectOption[] => {
    const options: SelectOption[] = [{ value: "-1", label: "Todos los huesos" }];
    if (props.boneNames) {
      props.boneNames.forEach((name, index) => {
        options.push({ value: index.toString(), label: name });
      });
    }
    return options;
  };

  return (
    <Panel id="view" title="Visualización" icon={<Icons.Eye size={14} />} defaultOpen>
      <div class="space-y-3">
        {/* Checkboxes */}
        <div class="space-y-2">
          <Checkbox
            label="Mostrar malla"
            checked={props.settings.showMesh}
            onChange={(checked) => updateSettings({ showMesh: checked })}
          />

          <Checkbox
            label="Texturas del modelo"
            checked={props.settings.showTextures !== false}
            onChange={(checked) => updateSettings({ showTextures: checked })}
          />

          <Checkbox
            label="Wireframe"
            checked={props.settings.showWireframe}
            onChange={(checked) => updateSettings({ showWireframe: checked })}
          />

          <Checkbox
            label="Mostrar esqueleto"
            checked={props.settings.showSkeleton}
            onChange={(checked) => updateSettings({ showSkeleton: checked })}
          />

          <Checkbox
            label="Heatmap de pesos"
            checked={props.settings.showWeights}
            onChange={(checked) => updateSettings({ showWeights: checked })}
            disabled={!props.hasWeights}
          />
        </div>

        {/* Bone Selector */}
        <Show when={props.hasWeights && props.settings.showWeights}>
          <Select
            label="Hueso seleccionado"
            options={boneOptions()}
            value={props.settings.selectedBone.toString()}
            onChange={(value) => updateSettings({ selectedBone: parseInt(value) })}
          />
        </Show>
      </div>
    </Panel>
  );
};
