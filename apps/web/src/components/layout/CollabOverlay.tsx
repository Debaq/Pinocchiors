import { Component, For, Show } from "solid-js";
import { peers } from "../../lib/collab";
import { collabView } from "../../lib/collabViews";
import { cursors, ripples, useCollabPresence, type RemoteCursor } from "../../lib/collabPresence";

/**
 * Posición en pantalla de un cursor, o nada si ese visor no está a la vista.
 * Con punto 3D se proyecta con la cámara propia (se ve en el mismo lugar del
 * modelo aunque las cámaras difieran); sin él, vale la posición relativa.
 */
function place(c: Pick<RemoteCursor, "view" | "x" | "y" | "p">): { left: number; top: number } | undefined {
  const el = document.querySelector(`[data-collab-view="${c.view}"]`);
  if (!el) return undefined;
  const r = el.getBoundingClientRect();
  if (r.width < 1) return undefined;
  let x = c.x;
  let y = c.y;
  const viewer = c.p ? collabView(c.view) : undefined;
  if (c.p && viewer) {
    const ndc = viewer.toNdc(c.p);
    if (!ndc) return undefined;
    x = (ndc[0] + 1) / 2;
    y = (1 - ndc[1]) / 2;
  }
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
          <Show when={place(c)}>
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
                  <Show when={c.label}>{` · ${c.label}`}</Show>
                </span>
              </div>
            )}
          </Show>
        )}
      </For>
      <For each={ripples()}>
        {(r) => (
          <Show when={place(r)}>
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
