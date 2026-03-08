import { Component, Show } from "solid-js";
import { Button } from "../ui";
import { SkeletonPanel, type SkeletonPreset } from "../panels/SkeletonPanel";
import { ConfigPanel, type AutorigConfig } from "../panels/ConfigPanel";
import { SkeletonTransformPanel, type SkeletonTransform } from "../panels/SkeletonTransformPanel";
import * as Icons from "../icons";

export interface SkeletonStepProps {
  // Skeleton
  presets: SkeletonPreset[];
  selectedPreset?: string;
  onPresetChange?: (presetId: string) => void;
  // Config
  autorigConfig: AutorigConfig;
  onAutorigConfigChange?: (config: AutorigConfig) => void;
  // Autorig
  onAutorig?: () => void;
  canAutorig?: boolean;
  isProcessing?: boolean;
  autorigComplete?: boolean;
  numBones?: number;
  // Transform
  skeletonTransform?: SkeletonTransform;
  onTransformChange?: (transform: SkeletonTransform) => void;
  onAutoFit?: () => void;
  onResetTransform?: () => void;
}

export const SkeletonStep: Component<SkeletonStepProps> = (props) => {
  return (
    <div class="space-y-5">
      <h3 class="text-sm font-semibold text-text">Esqueleto y Autorig</h3>
      <p class="text-xs text-text-muted leading-relaxed">
        Selecciona un preset de esqueleto y ejecuta el autorig para calcular pesos.
      </p>

      {/* Skeleton Preset */}
      <SkeletonPanel
        presets={props.presets}
        selectedPreset={props.selectedPreset}
        onPresetChange={props.onPresetChange}
      />

      {/* Transform Panel - visible when skeleton is selected */}
      <Show when={props.selectedPreset && props.skeletonTransform}>
        <SkeletonTransformPanel
          transform={props.skeletonTransform!}
          onChange={props.onTransformChange}
          onAutoFit={props.onAutoFit}
          onReset={props.onResetTransform}
          disabled={props.isProcessing}
        />
      </Show>

      {/* Config */}
      <ConfigPanel
        config={props.autorigConfig}
        onChange={props.onAutorigConfigChange}
      />

      {/* Execute */}
      <Button
        onClick={props.onAutorig}
        disabled={!props.canAutorig || props.isProcessing}
        variant="primary"
        fullWidth
      >
        <Show
          when={!props.isProcessing}
          fallback={
            <span class="flex items-center gap-2">
              <span class="animate-spin">&#9203;</span>
              Procesando...
            </span>
          }
        >
          <span class="flex items-center gap-2">
            <Icons.PersonArmsSpread size={16} />
            Ejecutar Autorig
          </span>
        </Show>
      </Button>

      {/* Result */}
      <Show when={props.autorigComplete}>
        <div class="flex items-center gap-2 text-xs text-green p-2 rounded-md bg-green/10 border border-green/20">
          <Icons.Check size={14} />
          Autorig completado
          <Show when={props.numBones}>
            <span class="text-text-muted ml-auto font-mono">{props.numBones} huesos</span>
          </Show>
        </div>
      </Show>
    </div>
  );
};
