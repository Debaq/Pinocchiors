import { Component, For, Show } from "solid-js";
import { clsx } from "clsx";
import { t, tx } from "../../lib/i18n";
import type { KeyHint } from "../../lib/designHints";

export interface StatusBarProps {
  message?: string;
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
        <Show
          when={props.hints}
          fallback={<span class="text-xs text-text-muted truncate">{props.message ?? tx("estado", "Listo")}</span>}
        >
          {(hints) => (
            <div class="flex items-center gap-3 min-w-0 overflow-hidden whitespace-nowrap" data-hints>
              <For each={hints()}>
                {(h) => (
                  <span class="flex items-center gap-1 text-xs text-text-muted">
                    <kbd class="px-1 rounded border border-border bg-surface/40 font-sans text-[10px] text-text">{h.keys}</kbd>
                    {t(h.label)}
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
          <span class="text-xs text-text-muted truncate">{t(props.message!)}</span>
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
