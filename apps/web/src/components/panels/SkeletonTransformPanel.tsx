import { Component, For, Show } from "solid-js";
import { AXES, VIEW_AXES } from "../../lib/axes";
import { Panel, Slider, NumberInput, Button } from "../ui";
import * as Icons from "../icons";

export interface SkeletonTransform {
  scale: number;
  translation: [number, number, number];
  rotation: [number, number, number];
}

export interface SkeletonTransformPanelProps {
  transform: SkeletonTransform;
  onChange?: (transform: SkeletonTransform) => void;
  onAutoFit?: () => void;
  onReset?: () => void;
  disabled?: boolean;
}

export const SkeletonTransformPanel: Component<SkeletonTransformPanelProps> = (props) => {
  const update = (partial: Partial<SkeletonTransform>) => {
    props.onChange?.({ ...props.transform, ...partial });
  };

  const updateTranslation = (axis: 0 | 1 | 2, value: number) => {
    const t: [number, number, number] = [...props.transform.translation];
    t[axis] = value;
    update({ translation: t });
  };

  const updateRotation = (axis: 0 | 1 | 2, value: number) => {
    const r: [number, number, number] = [...props.transform.rotation];
    r[axis] = value;
    update({ rotation: r });
  };

  return (
    <Panel title="Transformar" icon={<Icons.ArrowsClockwise size={14} />} defaultOpen>
      <div class="space-y-3">
        {/* Scale */}
        <Slider
          label="Escala"
          value={props.transform.scale}
          onChange={(v) => update({ scale: v })}
          min={0.1}
          max={5}
          step={0.05}
          formatValue={(v) => `${v.toFixed(2)}x`}
          disabled={props.disabled}
        />

        {/* Posición y giro en los ejes que se muestran (Z arriba) */}
        <div class="space-y-1.5">
          <span class="text-xs text-text-muted">Posición</span>
          <div class="flex gap-2">
            <For each={AXES}>
              {(axis) => (
                <NumberInput
                  label={axis.toUpperCase()}
                  value={VIEW_AXES[axis].sign * props.transform.translation[VIEW_AXES[axis].index]}
                  onChange={(v) => updateTranslation(VIEW_AXES[axis].index, VIEW_AXES[axis].sign * v)}
                  step={0.05}
                  disabled={props.disabled}
                />
              )}
            </For>
          </div>
        </div>

        <div class="space-y-1.5">
          <span class="text-xs text-text-muted">Rotación (grados)</span>
          <div class="flex gap-2">
            <For each={AXES}>
              {(axis) => (
                <NumberInput
                  label={axis.toUpperCase()}
                  value={VIEW_AXES[axis].sign * props.transform.rotation[VIEW_AXES[axis].index]}
                  onChange={(v) => updateRotation(VIEW_AXES[axis].index, VIEW_AXES[axis].sign * v)}
                  step={5}
                  min={-180}
                  max={180}
                  disabled={props.disabled}
                />
              )}
            </For>
          </div>
        </div>

        {/* Actions */}
        <div class="flex gap-2">
          <Show when={props.onAutoFit}>
            <Button
              onClick={props.onAutoFit}
              disabled={props.disabled}
              variant="primary"
              fullWidth
            >
              <span class="flex items-center gap-1.5">
                <Icons.MagicWand size={14} />
                Auto-fit
              </span>
            </Button>
          </Show>
          <Button
            onClick={props.onReset}
            disabled={props.disabled}
            variant="ghost"
            fullWidth={!props.onAutoFit}
          >
            Volver al preset
          </Button>
        </div>
      </div>
    </Panel>
  );
};
