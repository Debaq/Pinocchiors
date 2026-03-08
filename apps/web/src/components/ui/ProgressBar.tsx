import { Show } from "solid-js";
import { clsx } from "clsx";

export interface ProgressBarProps {
  value: number; // 0-100
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
  const value = () => Math.min(100, Math.max(0, props.value));
  const size = () => props.size ?? "md";
  const variant = () => props.variant ?? "default";

  return (
    <div class={clsx("w-full", props.class)}>
      <Show when={props.label || props.showValue}>
        <div class="flex items-center justify-between mb-1">
          <Show when={props.label}>
            <span class="text-xs text-text-muted">{props.label}</span>
          </Show>
          <Show when={props.showValue}>
            <span class="text-xs font-mono text-text-muted">{value()}%</span>
          </Show>
        </div>
      </Show>

      <div
        class={clsx(
          "w-full rounded-full bg-current overflow-hidden",
          size() === "sm" ? "h-1" : "h-2"
        )}
      >
        <div
          class={clsx(
            "h-full rounded-full transition-all duration-300 ease-out",
            variantColors[variant()]
          )}
          style={{ width: `${value()}%` }}
        />
      </div>
    </div>
  );
};
