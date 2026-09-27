import { Component, Show } from "solid-js";
import { Panel, Slider, Checkbox, Button } from "../ui";
import * as Icons from "../icons";

export interface RetopologyConfig {
  targetQuads: number;
  preserveSharp: boolean;
  sharpAngle: number;
  smoothIterations: number;
}

export interface RetopologyPanelProps {
  config: RetopologyConfig;
  onChange?: (config: RetopologyConfig) => void;
  onExecute?: () => void;
  canExecute?: boolean;
  isProcessing?: boolean;
  hasResult?: boolean;
  showQuadMesh?: boolean;
  onShowQuadMeshChange?: (show: boolean) => void;
}

export const RetopologyPanel: Component<RetopologyPanelProps> = (props) => {
  const updateConfig = (partial: Partial<RetopologyConfig>) => {
    props.onChange?.({ ...props.config, ...partial });
  };

  return (
    <Panel title="Retopología" icon={<Icons.GridFour size={14} />} defaultOpen>
      <div class="space-y-5">
        {/* Target Quads */}
        <Slider
          label="Quads objetivo"
          value={props.config.targetQuads}
          onChange={(value) => updateConfig({ targetQuads: Math.round(value) })}
          min={100}
          max={50000}
          step={100}
          formatValue={(v) => v.toLocaleString()}
        />

        {/* Preserve Sharp Edges */}
        <Checkbox
          label="Preservar bordes agudos"
          checked={props.config.preserveSharp}
          onChange={(checked) => updateConfig({ preserveSharp: checked })}
        />

        {/* Sharp Angle (only visible when preserveSharp is true) */}
        <Show when={props.config.preserveSharp}>
          <Slider
            label="Ángulo de borde"
            value={props.config.sharpAngle}
            onChange={(value) => updateConfig({ sharpAngle: value })}
            min={15}
            max={90}
            step={5}
            formatValue={(v) => `${v}°`}
          />
        </Show>

        {/* Smooth Iterations */}
        <Slider
          label="Iteraciones de suavizado"
          value={props.config.smoothIterations}
          onChange={(value) => updateConfig({ smoothIterations: Math.round(value) })}
          min={1}
          max={50}
          step={1}
          formatValue={(v) => `${v}`}
        />

        {/* Execute Button */}
        <Button
          onClick={props.onExecute}
          disabled={!props.canExecute || props.isProcessing}
          variant="primary"
          class="w-full"
        >
          <Show when={!props.isProcessing} fallback={
            <span class="flex items-center gap-2">
              <span class="animate-spin">⏳</span>
              Procesando...
            </span>
          }>
            <span class="flex items-center gap-2">
              <Icons.Lightning size={16} />
              Ejecutar QuadriFlow
            </span>
          </Show>
        </Button>

        {/* Result indicator and toggle */}
        <Show when={props.hasResult}>
          <div class="space-y-2 pt-2 border-t border-border">
            <div class="flex items-center gap-2 text-xs text-green-400">
              <svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <polyline points="20 6 9 17 4 12" />
              </svg>
              Retopología completada
            </div>

            <Checkbox
              label="Mostrar malla de quads"
              checked={props.showQuadMesh}
              onChange={(checked) => props.onShowQuadMeshChange?.(checked)}
            />
          </div>
        </Show>
      </div>
    </Panel>
  );
};
