import { For, Match, Show, Switch, createSignal, onCleanup, onMount } from "solid-js";
import { clsx } from "clsx";

export type MenuEntry =
  | {
      label: string;
      onSelect: () => void;
      /** Casilla marcada o no (sin el campo, no hay casilla) */
      checked?: boolean;
      disabled?: boolean;
      /** Acción que no se puede deshacer: pide un segundo clic */
      danger?: boolean;
      /** Atajo de teclado que se muestra a la derecha */
      shortcut?: string;
    }
  | { separator: true }
  | { header: string };

export interface ContextMenuProps {
  /** Posición del clic, en px de pantalla */
  x: number;
  y: number;
  items: MenuEntry[];
  onClose: () => void;
}

/** Menú del clic derecho: se cierra al elegir, con Escape o al hacer clic fuera */
export const ContextMenu = (props: ContextMenuProps) => {
  let ref: HTMLDivElement | undefined;
  const [pos, setPos] = createSignal({ x: props.x, y: props.y });
  // Índice del ítem peligroso que espera el segundo clic
  const [confirming, setConfirming] = createSignal<number | null>(null);

  onMount(() => {
    // Que no se salga de la ventana
    const rect = ref!.getBoundingClientRect();
    setPos({
      x: Math.max(4, Math.min(props.x, window.innerWidth - rect.width - 4)),
      y: Math.max(4, Math.min(props.y, window.innerHeight - rect.height - 4)),
    });
    const onDown = (e: PointerEvent) => {
      if (!ref!.contains(e.target as Node)) props.onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        props.onClose();
      }
    };
    window.addEventListener("pointerdown", onDown, true);
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("resize", props.onClose);
    window.addEventListener("blur", props.onClose);
    onCleanup(() => {
      window.removeEventListener("pointerdown", onDown, true);
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("resize", props.onClose);
      window.removeEventListener("blur", props.onClose);
    });
  });

  return (
    <div
      ref={ref}
      role="menu"
      class="fixed z-50 min-w-48 py-1 rounded-md border border-border bg-bg-lighter shadow-lg text-sm select-none"
      style={{ left: `${pos().x}px`, top: `${pos().y}px` }}
      onContextMenu={(e) => e.preventDefault()}
    >
      <For each={props.items}>
        {(item, i) => (
          <Switch>
            <Match when={"separator" in item}>
              <div class="my-1 h-px bg-border" />
            </Match>
            <Match when={"header" in item && item}>
              {(header) => (
                <div class="px-3 pt-1.5 pb-0.5 text-[10px] uppercase tracking-wide text-text-muted">
                  {(header() as { header: string }).header}
                </div>
              )}
            </Match>
            <Match when={"label" in item && item}>
              {(entry) => {
                const e = entry() as Extract<MenuEntry, { label: string }>;
                const armed = () => confirming() === i();
                return (
                  <button
                    role="menuitem"
                    disabled={e.disabled}
                    class={clsx(
                      "w-full flex items-center gap-2 px-3 py-1 text-left",
                      "disabled:opacity-40 disabled:pointer-events-none",
                      e.danger
                        ? armed() ? "bg-red/20 text-red" : "text-red hover:bg-red/10"
                        : "text-text hover:bg-accent/15"
                    )}
                    onClick={() => {
                      if (e.danger && !armed()) {
                        setConfirming(i());
                        return;
                      }
                      props.onClose();
                      e.onSelect();
                    }}
                  >
                    <span class="w-3 text-accent text-xs">{e.checked ? "✓" : ""}</span>
                    <span class="flex-1">{armed() ? "Clic de nuevo para confirmar" : e.label}</span>
                    <Show when={e.shortcut}>
                      <span class="text-[10px] font-mono text-text-muted">{e.shortcut}</span>
                    </Show>
                  </button>
                );
              }}
            </Match>
          </Switch>
        )}
      </For>
    </div>
  );
};
