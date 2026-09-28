import { Component, Show } from "solid-js";
import { ProgressBar } from "../ui";

export interface ProgressOverlayProps {
  progress?: {
    /** 0-100; sin valor la barra es indeterminada */
    value?: number;
    label?: string;
  };
}

/**
 * Tarjeta flotante con el avance del trabajo en curso, sobre el visor.
 *
 * No captura el mouse: el visor sigue respondiendo mientras el backend
 * trabaja en otro hilo.
 */
export const ProgressOverlay: Component<ProgressOverlayProps> = (props) => {
  return (
    <Show when={props.progress}>
      <div class="pointer-events-none absolute inset-x-0 bottom-6 flex justify-center z-20">
        <div class="animate-slide-up w-80 max-w-[90%] rounded-lg border border-border bg-bg-darker/90 px-4 py-3 shadow-lg backdrop-blur">
          <div class="flex items-center justify-between gap-3 mb-2">
            <span class="text-xs text-text truncate">{props.progress!.label ?? "Trabajando..."}</span>
            <Show when={props.progress!.value !== undefined}>
              <span class="text-xs font-mono text-text-muted">{props.progress!.value}%</span>
            </Show>
          </div>
          <ProgressBar value={props.progress!.value} size="sm" />
        </div>
      </div>
    </Show>
  );
};
