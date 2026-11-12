import { Component, For, Show, createEffect, createSignal, onCleanup, onMount } from "solid-js";
import { peers } from "../../lib/collab";
import { collabView } from "../../lib/collabViews";
import { t } from "../../lib/i18n";
import { activeDesignTab, lockedByOther } from "../../lib/collabLocks";
import { setCollabPanelOpen, setCollabPanelTab } from "../../lib/collab";
import { anchorAt, draft, placing, selectedThread, setDraft, setPlacing, setSelectedThread, threads, type CommentView } from "../../lib/collabComments";
import { presenceLabel } from "../../lib/collabPresence";
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

/** Cuadro actual: los pines y cursores 3D se recalculan cuando la cámara se mueve */
const [frame, setFrame] = createSignal(0);
const here = (a: Pick<RemoteCursor, "view" | "x" | "y" | "p">) => {
  frame();
  return place(a);
};

const colorOf = (id: string) => peers().find((p) => p.id === id)?.color ?? "#888";
const nameOf = (id: string) => peers().find((p) => p.id === id)?.name ?? "";

/**
 * Capa por encima de todo (sin captar el mouse): el cursor de cada persona con
 * su nombre y una onda donde hace clic. Además arranca la escucha de presencia.
 */
export const CollabOverlay: Component = () => {
  useCollabPresence();

  // Mientras haya algo anclado al modelo se sigue a la cámara cuadro a cuadro
  createEffect(() => {
    if (threads().length === 0 && !draft() && Object.keys(cursors()).length === 0) return;
    let raf = requestAnimationFrame(function tick() {
      setFrame((n) => (n + 1) % 1e9);
      raf = requestAnimationFrame(tick);
    });
    onCleanup(() => cancelAnimationFrame(raf));
  });

  // «Poner un comentario»: el próximo clic en un visor fija el punto (y no hace nada más)
  onMount(() => {
    let swallow = false;
    const at = (e: PointerEvent) => {
      const el = (e.target as Element | null)?.closest?.("[data-collab-view]");
      if (!el) return undefined;
      const r = el.getBoundingClientRect();
      const view = el.getAttribute("data-collab-view") as CommentView;
      return { view, x: (e.clientX - r.left) / r.width, y: (e.clientY - r.top) / r.height };
    };
    const down = (e: PointerEvent) => {
      if (!placing()) return;
      const a = at(e);
      if (!a) return;
      e.preventDefault();
      e.stopImmediatePropagation();
      swallow = true;
      setPlacing(false);
      setDraft(anchorAt(a.view, a.x, a.y, presenceLabel(a.view)));
      setCollabPanelOpen(true);
      setCollabPanelTab("comments");
    };
    // Lo que sigue al clic (soltar, click) tampoco llega a los visores
    const after = (e: Event) => {
      if (!swallow) return;
      e.stopImmediatePropagation();
      e.preventDefault();
      if (e.type === "click") swallow = false;
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape" && placing()) setPlacing(false);
    };
    window.addEventListener("pointerdown", down, true);
    for (const t of ["pointerup", "mousedown", "mouseup", "click"]) window.addEventListener(t, after, true);
    window.addEventListener("keydown", key);
    onCleanup(() => {
      window.removeEventListener("pointerdown", down, true);
      for (const t of ["pointerup", "mousedown", "mouseup", "click"]) window.removeEventListener(t, after, true);
      window.removeEventListener("keydown", key);
    });
  });

  return (
    <div class="fixed inset-0 z-[9000] pointer-events-none overflow-hidden">
      {/* Pestaña tomada por otra persona: solo lectura */}
      <Show when={lockedByOther(activeDesignTab())}>
        {(l) => (
          <div
            class="absolute top-12 left-1/2 -translate-x-1/2 px-3 py-1 rounded-full text-xs text-white shadow"
            style={{ background: l().color }}
            role="status"
          >
            {t("{name} está editando esta pestaña: solo lectura", { name: l().name })}
          </div>
        )}
      </Show>
      {/* Pines de los comentarios */}
      <For each={threads()}>
        {(th) => (
          <Show when={here(th.anchor) !== undefined}>
            {(() => {
              const pos = () => here(th.anchor) ?? { left: -99, top: -99 };
              return (
              <button
                class="absolute pointer-events-auto -translate-x-1/2 -translate-y-1/2 w-6 h-6 rounded-full text-[11px] text-white border-2 border-white shadow flex items-center justify-center"
                classList={{ "opacity-40": th.resolved && selectedThread() !== th.id, "ring-2 ring-accent scale-110": selectedThread() === th.id }}
                style={{ left: `${pos().left}px`, top: `${pos().top}px`, background: th.comments[0]?.color }}
                title={th.comments[0]?.text}
                aria-label={`Comentario ${th.n}`}
                onClick={() => {
                  setSelectedThread(th.id);
                  setCollabPanelOpen(true);
                  setCollabPanelTab("comments");
                }}
              >
                {th.n}
              </button>
              );
            })()}
          </Show>
        )}
      </For>
      <Show when={draft() && here(draft()!) !== undefined}>
        <span
          class="absolute -translate-x-1/2 -translate-y-1/2 w-6 h-6 rounded-full border-2 border-dashed border-accent bg-accent/30"
          style={{ left: `${(here(draft()!) ?? { left: -99 }).left}px`, top: `${(here(draft()!) ?? { top: -99 }).top}px` }}
        />
      </Show>
      <div aria-hidden="true">
      <For each={Object.values(cursors())}>
        {(c) => (
          <Show when={here(c) !== undefined}>
            {(() => {
              const pos = () => here(c) ?? { left: -99, top: -99 };
              return (
              <div class="absolute" style={{ left: `${pos().left}px`, top: `${pos().top}px` }}>
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
              );
            })()}
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
    </div>
  );
};
