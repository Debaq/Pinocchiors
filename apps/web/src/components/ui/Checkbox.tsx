import { Checkbox as KobalteCheckbox } from "@kobalte/core/checkbox";
import { clsx } from "clsx";

export interface CheckboxProps {
  checked?: boolean;
  onChange?: (checked: boolean) => void;
  disabled?: boolean;
  label?: string;
  /** Etiqueta chica, para paneles densos */
  small?: boolean;
  class?: string;
}

export const Checkbox = (props: CheckboxProps) => {
  return (
    <KobalteCheckbox
      checked={props.checked}
      onChange={props.onChange}
      disabled={props.disabled}
      class={clsx(
        "inline-flex items-center gap-2 cursor-pointer",
        "data-[disabled]:opacity-50 data-[disabled]:pointer-events-none",
        props.class
      )}
    >
      <KobalteCheckbox.Input class="peer sr-only" />
      <KobalteCheckbox.Control
        class={clsx(
          "w-4 h-4 rounded",
          "border border-border bg-bg-lighter",
          "flex items-center justify-center",
          "transition-all duration-100",
          "peer-hover:border-border-hover",
          "peer-focus-visible:ring-2 peer-focus-visible:ring-accent/50",
          "data-[checked]:bg-accent data-[checked]:border-accent"
        )}
      >
        <KobalteCheckbox.Indicator>
          <svg class="w-3 h-3 text-bg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3">
            <polyline points="20 6 9 17 4 12" />
          </svg>
        </KobalteCheckbox.Indicator>
      </KobalteCheckbox.Control>

      {props.label && (
        <KobalteCheckbox.Label class={clsx(props.small ? "text-xs text-text-muted" : "text-sm text-text", "select-none")}>
          {props.label}
        </KobalteCheckbox.Label>
      )}
    </KobalteCheckbox>
  );
};
