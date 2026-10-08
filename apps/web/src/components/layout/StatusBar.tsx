import { Component, For, Show } from "solid-js";
import { clsx } from "clsx";
import { ProgressBar } from "../ui";
import type { KeyHint } from "../../lib/designHints";

export interface StatusBarProps {
  message?: string;
  progress?: {
    /** Sin valor: indeterminado */
    value?: number;
    label?: string;
  };
  showMemory?: boolean;
  memory?: string;
  /** Resumen a la derecha (articulación activa al posar) */
  info?: string;
  /** Atajos del momento: con ellos el mensaje pasa a la derecha */
  hints?: KeyHint[];
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

        <Show
          when={props.hints}
          fallback={<span class="text-xs text-text-muted truncate">{props.message ?? "Listo"}</span>}
        >
          {(hints) => (
            <div class="flex items-center gap-3 min-w-0 overflow-hidden whitespace-nowrap" data-hints>
              <For each={hints()}>
                {(h) => (
                  <span class="flex items-center gap-1 text-xs text-text-muted">
                    <kbd class="px-1 rounded border border-border bg-surface/40 font-sans text-[10px] text-text">{h.keys}</kbd>
                    {h.label}
                  </span>
                )}
              </For>
            </div>
          )}
        </Show>
      </div>

      {/* Right - Info */}
      <div class="flex items-center gap-4 min-w-0">
        <Show when={props.hints && props.message}>
          <span class="text-xs text-text-muted truncate">{props.message}</span>
        </Show>
        <Show when={props.info}>
          <span class="text-xs font-mono text-text-muted truncate">{props.info}</span>
        </Show>
        <Show when={props.showMemory && props.memory}>
          <span class="text-xs font-mono text-text-dim">
            {props.memory}
          </span>
        </Show>
      </div>
    </footer>
  );
};
