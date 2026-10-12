import { Component, Show } from "solid-js";
import { Panel, Checkbox, Select, SelectOption } from "../ui";
import * as Icons from "../icons";
import type { BoneDisplay } from "../../lib/Viewer3D";
import { t } from "../../lib/i18n";

export interface ViewSettings {
  showMesh: boolean;
  showWireframe: boolean;
  showSkeleton: boolean;
  showWeights: boolean;
  selectedBone: number;
  /** Materiales y texturas del archivo de origen */
  showTextures?: boolean;
  /** Rayos X: modelo translúcido */
  xray?: boolean;
  /** Opacidad del modelo con rayos X (0–1) */
  xrayAlpha?: number;
  boneDisplay?: BoneDisplay;
  /** Notebook: el desplazamiento gira la cámara en vez de acercar */
  trackpadNavigation?: boolean;
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
    const options: SelectOption[] = [{ value: "-1", label: t("Todos los huesos") }];
    if (props.boneNames) {
      props.boneNames.forEach((name, index) => {
        options.push({ value: index.toString(), label: name });
      });
    }
    return options;
  };

  return (
    <Panel id="view" title={t("Visualización")} icon={<Icons.Eye size={14} />} defaultOpen>
      <div class="space-y-3">
        {/* Checkboxes */}
        <div class="space-y-2">
          <Checkbox
            label={t("Mostrar malla")}
            checked={props.settings.showMesh}
            onChange={(checked) => updateSettings({ showMesh: checked })}
          />

          <Checkbox
            label={t("Texturas del modelo")}
            checked={props.settings.showTextures !== false}
            onChange={(checked) => updateSettings({ showTextures: checked })}
          />

          <Checkbox
            label={t("Rayos X (Alt+Z)")}
            checked={props.settings.xray === true}
            onChange={(checked) => updateSettings({ xray: checked })}
          />

          <Checkbox
            label={t("Wireframe")}
            checked={props.settings.showWireframe}
            onChange={(checked) => updateSettings({ showWireframe: checked })}
          />

          <Checkbox
            label={t("Mostrar esqueleto")}
            checked={props.settings.showSkeleton}
            onChange={(checked) => updateSettings({ showSkeleton: checked })}
          />

          <Checkbox
            label={t("Heatmap de pesos")}
            checked={props.settings.showWeights}
            onChange={(checked) => updateSettings({ showWeights: checked })}
            disabled={!props.hasWeights}
          />
        </div>

        {/* Navegación */}
        <div class="space-y-1.5 pt-1 border-t border-border">
          <Checkbox
            label={t("Navegación con touchpad o TrackPoint")}
            checked={props.settings.trackpadNavigation === true}
            onChange={(checked) => updateSettings({ trackpadNavigation: checked })}
            hint={t("El desplazamiento gira la cámara: dos dedos en el touchpad, o botón central + TrackPoint. Con Shift desplaza la vista; con Ctrl o pellizcando, acerca. Alt + clic izquierdo arrastrando también gira.")}
          />
        </div>

        {/* Bone Selector */}
        <Show when={props.hasWeights && props.settings.showWeights}>
          <Select
            label={t("Hueso seleccionado")}
            options={boneOptions()}
            value={props.settings.selectedBone.toString()}
            onChange={(value) => updateSettings({ selectedBone: parseInt(value) })}
          />
        </Show>
      </div>
    </Panel>
  );
};
