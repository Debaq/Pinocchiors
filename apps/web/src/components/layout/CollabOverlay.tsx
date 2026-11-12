import { Component, For, Show } from "solid-js";
import { peers } from "../../lib/collab";
import { cursors, ripples, useCollabPresence } from "../../lib/collabPresence";

/** Posición en pantalla de un punto relativo al visor, o nada si ese visor no está a la vista */
function place(view: string, x: number, y: number): { left: number; top: number } | undefined {
  const el = document.querySelector(`[data-collab-view="${view}"]`);
  if (!el) return undefined;
  const r = el.getBoundingClientRect();
  if (r.width < 1) return undefined;
  return { left: r.left + x * r.width, top: r.top + y * r.height };
}

const colorOf = (id: string) => peers().find((p) => p.id === id)?.color ?? "#888";
const nameOf = (id: string) => peers().find((p) => p.id === id)?.name ?? "";

/**
 * Capa por encima de todo (sin captar el mouse): el cursor de cada persona con
 * su nombre y una onda donde hace clic. Además arranca la escucha de presencia.
 */
export const CollabOverlay: Component = () => {
  useCollabPresence();
  return (
    <div class="fixed inset-0 z-[9000] pointer-events-none overflow-hidden" aria-hidden="true">
      <For each={Object.values(cursors())}>
        {(c) => (
          <Show when={place(c.view, c.x, c.y)}>
            {(pos) => (
              <div class="absolute" style={{ left: `${pos().left}px`, top: `${pos().top}px`, transition: "left 60ms linear, top 60ms linear" }}>
                <svg width="18" height="18" viewBox="0 0 24 24" style={{ color: colorOf(c.id), filter: "drop-shadow(0 1px 1px rgba(0,0,0,.5))" }}>
                  <path d="M3 2l7 19 3-8 8-3z" fill="currentColor" stroke="white" stroke-width="1.5" stroke-linejoin="round" />
                </svg>
                <span
                  class="absolute left-4 top-3 px-1.5 py-0.5 rounded text-[10px] leading-none text-white whitespace-nowrap"
                  style={{ background: colorOf(c.id) }}
                >
                  {nameOf(c.id)}
                </span>
              </div>
            )}
          </Show>
        )}
      </For>
      <For each={ripples()}>
        {(r) => (
          <Show when={place(r.view, r.x, r.y)}>
            {(pos) => (
              <span
                class="absolute rounded-full border-2 collab-ripple"
                style={{ left: `${pos().left}px`, top: `${pos().top}px`, "border-color": colorOf(r.id) }}
              />
            )}
          </Show>
        )}
      </For>
    </div>
  );
};
