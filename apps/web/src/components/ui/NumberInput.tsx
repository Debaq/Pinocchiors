import { clsx } from "clsx";
import { Show } from "solid-js";

export interface NumberInputProps {
  value?: number;
  onChange?: (value: number) => void;
  min?: number;
  max?: number;
  step?: number;
  label?: string;
  suffix?: string;
  disabled?: boolean;
  class?: string;
}

export const NumberInput = (props: NumberInputProps) => {
  const step = () => props.step ?? 1;

  const handleInput = (e: InputEvent) => {
    const target = e.target as HTMLInputElement;
    const val = parseFloat(target.value);
    if (!isNaN(val)) {
      props.onChange?.(val);
    }
  };

  // Rótulo de eje (X, Y, Z): campo compacto que reparte la fila o la celda con
  // sus vecinos sin salirse. Rótulo largo: baja a su propia línea si no cabe, y
  // el campo con su unidad van siempre juntos
  const axis = () => !!props.label && props.label.length <= 2;

  return (
    <label class={clsx("flex items-center gap-y-1", axis() ? "flex-1 min-w-0 gap-x-1" : "flex-wrap gap-x-2", props.class)}>
      <Show when={props.label}>
        <span class={clsx("text-xs text-text-muted", axis() ? "w-3 text-center shrink-0" : "leading-tight")}>{props.label}</span>
      </Show>
      <span class={clsx("flex flex-1 items-center gap-1", axis() ? "min-w-0" : "min-w-[4.5rem]")}>
        <input
          type="number"
          value={props.value ?? 0}
          onInput={handleInput}
          min={props.min}
          max={props.max}
          step={step()}
          disabled={props.disabled}
          class={clsx(
            "w-0 flex-1 min-w-0 py-1 rounded",
            axis() ? "px-1.5" : "px-2",
            "bg-surface/40 border border-border",
            "text-xs text-text font-mono",
            "outline-none focus:border-accent",
            "disabled:opacity-50",
            "[appearance:textfield]",
            "[&::-webkit-outer-spin-button]:appearance-none",
            "[&::-webkit-inner-spin-button]:appearance-none"
          )}
        />
        <Show when={props.suffix}>
          <span class="text-xs text-text-muted shrink-0">{props.suffix}</span>
        </Show>
      </span>
    </label>
  );
};
