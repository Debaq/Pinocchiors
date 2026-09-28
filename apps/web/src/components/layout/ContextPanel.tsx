import { Component, Switch, Match, Show } from "solid-js";
import { clsx } from "clsx";
import type { PipelineStepId } from "../../lib/pipeline";
import type { ViewSettings } from "../panels/ViewPanel";
import type { SceneNode } from "../../lib/scene-tree";
import { ViewPanel } from "../panels/ViewPanel";
import { LightsPanel } from "../panels/LightsPanel";
import type { LightSettings } from "../../lib/Viewer3D";
import { SceneOutliner } from "../panels/SceneOutliner";
import { ImportStep, type ImportStepProps } from "../steps/ImportStep";
import { StructureStep, type StructureStepProps } from "../steps/StructureStep";
import { RetopologyStep, type RetopologyStepProps } from "../steps/RetopologyStep";
import { SkeletonStep, type SkeletonStepProps } from "../steps/SkeletonStep";
import { UvStep, type UvStepProps } from "../steps/UvStep";
import { ExportStep, type ExportStepProps } from "../steps/ExportStep";
import { RepairStep, type RepairStepProps } from "../steps/RepairStep";
import { AnimateStep } from "../steps/AnimateStep";
import { Print3DStep, type Print3DStepProps } from "../steps/Print3DStep";

export interface ContextPanelProps {
  activeStep: PipelineStepId;

  // Import
  importProps: ImportStepProps;

  // Estructura del archivo de origen
  structureProps: StructureStepProps;

  // Repair
  repairProps: RepairStepProps;

  // Retopology
  retopologyProps: RetopologyStepProps;

  // UV / Piel
  uvProps: UvStepProps;

  // Skeleton
  skeletonProps: SkeletonStepProps;

  // Print3D
  print3dProps: Print3DStepProps;

  // Export
  exportProps: ExportStepProps;

  // View
  viewSettings: ViewSettings;
  onViewSettingsChange?: (settings: ViewSettings) => void;
  boneNames?: string[];
  hasWeights?: boolean;

  // Luces
  lights: LightSettings;
  onLightsChange?: (lights: LightSettings) => void;

  // Outliner
  sceneTree?: SceneNode;
  onToggleVisibility?: (nodeId: string) => void;
  onSelectNode?: (nodeId: string) => void;
}

export const ContextPanel: Component<ContextPanelProps> = (props) => {
  return (
    <aside
      class={clsx(
        "flex flex-col",
        "w-[300px] min-w-[300px]",
        "bg-bg-darker border-l border-border",
        "overflow-hidden"
      )}
    >
      {/* Step content - scrollable */}
      <div class="flex-1 min-h-0 overflow-y-auto px-5 py-5">
        <Switch>
          <Match when={props.activeStep === "import"}>
            <ImportStep {...props.importProps} />
          </Match>
          <Match when={props.activeStep === "structure"}>
            <StructureStep {...props.structureProps} />
          </Match>
          <Match when={props.activeStep === "repair"}>
            <RepairStep {...props.repairProps} />
          </Match>
          <Match when={props.activeStep === "retopology"}>
            <RetopologyStep {...props.retopologyProps} />
          </Match>
          <Match when={props.activeStep === "uv"}>
            <UvStep {...props.uvProps} />
          </Match>
          <Match when={props.activeStep === "skeleton"}>
            <SkeletonStep {...props.skeletonProps} />
          </Match>
          <Match when={props.activeStep === "animate"}>
            <AnimateStep />
          </Match>
          <Match when={props.activeStep === "print3d"}>
            <Print3DStep {...props.print3dProps} />
          </Match>
          <Match when={props.activeStep === "export"}>
            <ExportStep {...props.exportProps} />
          </Match>
        </Switch>
      </div>

      {/* Outliner */}
      <Show when={props.sceneTree}>
        <div class="shrink-0 border-t border-border px-3 py-3">
          <SceneOutliner
            tree={props.sceneTree!}
            onToggleVisibility={props.onToggleVisibility}
            onSelectNode={props.onSelectNode}
          />
        </div>
      </Show>

      {/* View Panel - always visible at bottom */}
      <div class="shrink-0 border-t border-border px-5 py-3 space-y-3 max-h-[45vh] overflow-y-auto">
        <ViewPanel
          settings={props.viewSettings}
          onChange={props.onViewSettingsChange}
          boneNames={props.boneNames}
          hasWeights={props.hasWeights}
        />
        <LightsPanel lights={props.lights} onChange={props.onLightsChange} />
      </div>
    </aside>
  );
};
