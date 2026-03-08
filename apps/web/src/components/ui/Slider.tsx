import { Slider as KobalteSlider } from "@kobalte/core/slider";
import { Show } from "solid-js";
import { clsx } from "clsx";

export interface SliderProps {
  value?: number;
  onChange?: (value: number) => void;
  min?: number;
  max?: number;
  step?: number;
  disabled?: boolean;
  label?: string;
  showValue?: boolean;
  formatValue?: (value: number) => string;
  class?: string;
}

export const Slider = (props: SliderProps) => {
  const min = () => props.min ?? 0;
  const max = () => props.max ?? 100;
  const step = () => props.step ?? 1;
  const value = () => props.value ?? min();
  const formatValue = () => props.formatValue ?? ((v: number) => v.toString());

  return (
    <KobalteSlider
      value={[value()]}
      onChange={(values) => props.onChange?.(values[0])}
      minValue={min()}
      maxValue={max()}
      step={step()}
      disabled={props.disabled}
      class={clsx("w-full", props.class)}
    >
      <div class="flex items-center justify-between mb-1.5">
        <Show when={props.label}>
          <KobalteSlider.Label class="text-xs text-text-muted">
            {props.label}
          </KobalteSlider.Label>
        </Show>

        <Show when={props.showValue !== false}>
          <KobalteSlider.ValueLabel class="text-xs font-mono text-text-muted">
            {formatValue()(value())}
          </KobalteSlider.ValueLabel>
        </Show>
      </div>

      <KobalteSlider.Track
        class={clsx(
          "relative h-1.5 w-full rounded-full",
          "bg-current",
          "data-[disabled]:opacity-50"
        )}
      >
        <KobalteSlider.Fill
          class="absolute h-full rounded-full bg-accent"
        />
        <KobalteSlider.Thumb
          class={clsx(
            "block w-4 h-4 -top-[5px]",
            "bg-accent rounded-full",
            "border-2 border-bg",
            "shadow-md",
            "transition-transform duration-100",
            "hover:scale-110",
            "focus:outline-none focus:ring-2 focus:ring-accent/50",
            "data-[disabled]:pointer-events-none"
          )}
        >
          <KobalteSlider.Input />
        </KobalteSlider.Thumb>
      </KobalteSlider.Track>
    </KobalteSlider>
  );
};
