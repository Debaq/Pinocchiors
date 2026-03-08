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

  return (
    <label class={clsx("flex items-center gap-2", props.class)}>
      <Show when={props.label}>
        <span class="text-xs text-text-muted w-4 text-center shrink-0">{props.label}</span>
      </Show>
      <input
        type="number"
        value={props.value ?? 0}
        onInput={handleInput}
        min={props.min}
        max={props.max}
        step={step()}
        disabled={props.disabled}
        class={clsx(
          "w-full min-w-0 px-2 py-1 rounded",
          "bg-current/40 border border-border",
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
    </label>
  );
};
