import { Show } from "solid-js";
import { clsx } from "clsx";

export interface ProgressBarProps {
  /** 0-100; sin valor la barra es indeterminada (trabajo sin avance medible) */
  value?: number;
  label?: string;
  showValue?: boolean;
  class?: string;
  size?: "sm" | "md";
  variant?: "default" | "success" | "warning" | "error";
}

const variantColors: Record<string, string> = {
  default: "bg-accent",
  success: "bg-green",
  warning: "bg-orange",
  error: "bg-red",
};

export const ProgressBar = (props: ProgressBarProps) => {
  const value = () => Math.min(100, Math.max(0, props.value ?? 0));
  const indeterminate = () => props.value === undefined;
  const size = () => props.size ?? "md";
  const variant = () => props.variant ?? "default";

  return (
    <div class={clsx("w-full", props.class)}>
      <Show when={props.label || props.showValue}>
        <div class="flex items-center justify-between mb-1">
          <Show when={props.label}>
            <span class="text-xs text-text-muted">{props.label}</span>
          </Show>
          <Show when={props.showValue && !indeterminate()}>
            <span class="text-xs font-mono text-text-muted">{value()}%</span>
          </Show>
        </div>
      </Show>

      <div
        class={clsx(
          "w-full rounded-full bg-surface overflow-hidden",
          size() === "sm" ? "h-1" : "h-2"
        )}
      >
        <div
          class={clsx(
            "h-full rounded-full",
            indeterminate() ? "w-1/3 animate-indeterminate" : "transition-all duration-300 ease-out",
            variantColors[variant()]
          )}
          style={indeterminate() ? undefined : { width: `${value()}%` }}
        />
      </div>
    </div>
  );
};
