import { Component } from "solid-js";
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

        {/* Translation */}
        <div class="space-y-1.5">
          <span class="text-xs text-text-muted">Posición</span>
          <div class="flex gap-2">
            <NumberInput
              label="X"
              value={props.transform.translation[0]}
              onChange={(v) => updateTranslation(0, v)}
              step={0.05}
              disabled={props.disabled}
            />
            <NumberInput
              label="Y"
              value={props.transform.translation[1]}
              onChange={(v) => updateTranslation(1, v)}
              step={0.05}
              disabled={props.disabled}
            />
            <NumberInput
              label="Z"
              value={props.transform.translation[2]}
              onChange={(v) => updateTranslation(2, v)}
              step={0.05}
              disabled={props.disabled}
            />
          </div>
        </div>

        {/* Rotation */}
        <div class="space-y-1.5">
          <span class="text-xs text-text-muted">Rotación (grados)</span>
          <div class="flex gap-2">
            <NumberInput
              label="X"
              value={props.transform.rotation[0]}
              onChange={(v) => updateRotation(0, v)}
              step={5}
              min={-180}
              max={180}
              disabled={props.disabled}
            />
            <NumberInput
              label="Y"
              value={props.transform.rotation[1]}
              onChange={(v) => updateRotation(1, v)}
              step={5}
              min={-180}
              max={180}
              disabled={props.disabled}
            />
            <NumberInput
              label="Z"
              value={props.transform.rotation[2]}
              onChange={(v) => updateRotation(2, v)}
              step={5}
              min={-180}
              max={180}
              disabled={props.disabled}
            />
          </div>
        </div>

        {/* Actions */}
        <div class="flex gap-2">
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
          <Button
            onClick={props.onReset}
            disabled={props.disabled}
            variant="ghost"
          >
            Resetear
          </Button>
        </div>
      </div>
    </Panel>
  );
};
