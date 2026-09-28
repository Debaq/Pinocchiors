import { Component, Show } from "solid-js";
import { clsx } from "clsx";
import { ProgressBar } from "../ui";

export interface StatusBarProps {
  message?: string;
  progress?: {
    /** Sin valor: indeterminado */
    value?: number;
    label?: string;
  };
  showMemory?: boolean;
  memory?: string;
}

export const StatusBar: Component<StatusBarProps> = (props) => {
  return (
    <footer
      class={clsx(
        "flex items-center justify-between",
        "h-8 px-6",
        "bg-bg-darker/50 border-t border-border"
      )}
    >
      {/* Left - Status Message */}
      <div class="flex items-center gap-3 flex-1 min-w-0">
        {/* Progress Bar */}
        <Show when={props.progress}>
          <div class="w-48 flex items-center gap-2">
            <ProgressBar
              value={props.progress!.value}
              size="sm"
              class="flex-1"
            />
            <Show when={props.progress!.value !== undefined}>
              <span class="text-xs text-text-muted whitespace-nowrap">
                {props.progress!.value}%
              </span>
            </Show>
          </div>
        </Show>

        {/* Message */}
        <span class="text-xs text-text-muted truncate">
          {props.message ?? "Listo"}
        </span>
      </div>

      {/* Right - Info */}
      <div class="flex items-center gap-4">
        <Show when={props.showMemory && props.memory}>
          <span class="text-xs font-mono text-text-dim">
            {props.memory}
          </span>
        </Show>
      </div>
    </footer>
  );
};
