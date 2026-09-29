import { Component, onCleanup, onMount } from "solid-js";
import { Button } from "../ui";
import * as Icons from "../icons";

export interface ConfirmRequest {
  title: string;
  message: string;
  /** Texto del botón que confirma (por defecto "Continuar") */
  confirmLabel?: string;
  /** La acción descarta trabajo: el botón va en rojo */
  danger?: boolean;
}

export interface ConfirmDialogProps extends ConfirmRequest {
  onConfirm: () => void;
  onCancel: () => void;
}

/** Pregunta antes de una acción que no se puede deshacer (reemplaza al diálogo del sistema) */
export const ConfirmDialog: Component<ConfirmDialogProps> = (props) => {
  let box: HTMLDivElement | undefined;

  onMount(() => {
    box?.querySelector<HTMLButtonElement>("[data-confirm]")?.focus();
    // En captura: que Esc y Enter no lleguen a los atajos del visor
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        props.onCancel();
      } else if (e.key === "Enter") {
        e.preventDefault();
        e.stopPropagation();
        props.onConfirm();
      } else if (e.key !== "Tab") {
        e.stopPropagation();
      }
    };
    window.addEventListener("keydown", onKey, true);
    onCleanup(() => window.removeEventListener("keydown", onKey, true));
  });

  return (
    <div
      class="fixed inset-0 z-50 flex items-center justify-center bg-bg-darker/70 backdrop-blur-sm p-4"
      onPointerDown={(e) => e.target === e.currentTarget && props.onCancel()}
    >
      <div ref={box} role="alertdialog" aria-label={props.title} class="w-full max-w-sm rounded-lg border border-border bg-bg shadow-xl">
        <div class="flex items-center justify-between px-4 h-11 border-b border-border">
          <h2 class="flex items-center gap-2 text-sm font-semibold text-text">
            <Icons.Warning size={16} class={props.danger ? "text-red" : "text-warning"} />
            {props.title}
          </h2>
          <button
            class="w-7 h-7 flex items-center justify-center rounded text-text-muted hover:text-text hover:bg-surface/40"
            aria-label="Cancelar"
            onClick={props.onCancel}
          >
            <Icons.X size={14} />
          </button>
        </div>
        <p class="p-4 text-xs text-text-muted leading-relaxed whitespace-pre-line">{props.message}</p>
        <div class="flex justify-end gap-2 px-4 pb-4">
          <Button size="sm" variant="ghost" onClick={props.onCancel}>
            Cancelar
          </Button>
          <Button
            data-confirm
            size="sm"
            variant={props.danger ? "danger" : "primary"}
            onClick={props.onConfirm}
          >
            {props.confirmLabel ?? "Continuar"}
          </Button>
        </div>
      </div>
    </div>
  );
};
