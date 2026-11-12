import { Component, For, Show, createEffect, createSignal, onCleanup, onMount, untrack } from "solid-js";
import { chat, collabPanelOpen, collabPanelTab, peers } from "../../lib/collab";
import { myWhere, whereOf } from "../../lib/collabWhere";
import { collabView } from "../../lib/collabViews";
import { t } from "../../lib/i18n";
import { activeDesignTab, lockedByOther } from "../../lib/collabLocks";
import { setCollabPanelOpen, setCollabPanelTab } from "../../lib/collab";
import { anchorAt, draft, hoveredThread, pinShown, placing, selectedThread, setDraft, setPlacing, setSelectedThread, threads, type CommentView } from "../../lib/collabComments";
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

/** Mensaje del chat que se ve como globo unos segundos (el último de cada persona) */
interface Bubble {
  key: number;
  text: string;
  /** Su cursor cuando llegó: el globo no salta si el cursor se oculta por quieto */
  cursor?: RemoteCursor;
}
const BUBBLE_MS = 6000;
const [bubbles, setBubbles] = createSignal<Record<string, Bubble>>({});
let bubbleKey = 0;

/**
 * Dónde va el globo de una persona: si está en otra área, bajo esa pestaña de
 * arriba; si está en otro diseño, sobre esa pestaña de abajo; si está donde yo,
 * junto a su cursor. Sin ninguno de esos, va a la esquina (devuelve nada).
 */
function bubbleSpot(id: string, b: Bubble): { left: number; top: number; center: boolean; up: boolean } | undefined {
  const w = whereOf()[id];
  const me = myWhere();
  const clampX = (x: number) => Math.min(Math.max(x, 140), window.innerWidth - 140);
  if (w?.ws && w.ws !== me.ws) {
    const r = document.querySelector(`[data-ws="${w.ws}"]`)?.getBoundingClientRect();
    if (r && r.width > 0) return { left: clampX(r.left + r.width / 2), top: r.bottom + 6, center: true, up: false };
  }
  if (w?.ws === "design" && me.ws === "design" && w.tab != null && w.tab !== me.tab) {
    const r = document.querySelector(`[data-design-tabs] [data-tab="${w.tab}"]`)?.getBoundingClientRect();
    if (r && r.width > 0) return { left: clampX(r.left + r.width / 2), top: r.top - 6, center: true, up: true };
  }
  const c = cursors()[id] ?? b.cursor;
  const pos = c && here(c);
  if (pos) return { left: pos.left + 16, top: pos.top + 26, center: false, up: false };
  return undefined;
}

const dropBubble = (id: string) =>
  setBubbles((m) => {
    if (!(id in m)) return m;
    const next = { ...m };
    delete next[id];
    return next;
  });

/** El globo: nombre y mensaje; un clic abre el chat */
const BubbleBox = (props: { id: string }) => (
  <button
    class="pointer-events-auto block max-w-64 rounded-lg px-2.5 py-1.5 text-left text-xs leading-snug text-white shadow-lg collab-bubble"
    style={{ background: colorOf(props.id) }}
    title={t("Abrir el chat")}
    onClick={() => {
      setCollabPanelTab("chat");
      setCollabPanelOpen(true);
      dropBubble(props.id);
    }}
  >
    <span class="block font-semibold">{nameOf(props.id)}</span>
    <span class="line-clamp-3 break-words">{bubbles()[props.id]?.text}</span>
  </button>
);

/**
 * Su cursor y sus clics se ven solo si está donde yo: en la misma área y, en
 * Diseñar, en la misma pestaña de diseño o ensamble. Si todavía no se sabe
 * dónde está, se ven.
 */
function sameSpot(id: string, view: string): boolean {
  const w = whereOf()[id];
  if (!w?.ws) return true;
  const me = myWhere();
  if (w.ws !== me.ws) return false;
  return view !== "design" || (w.tab ?? null) === (me.tab ?? null);
}

const colorOf = (id: string) => peers().find((p) => p.id === id)?.color ?? "#888";
const nameOf = (id: string) => peers().find((p) => p.id === id)?.name ?? "";

/**
 * Capa por encima de todo (sin captar el mouse): el cursor de cada persona con
 * su nombre y una onda donde hace clic. Además arranca la escucha de presencia.
 */
export const CollabOverlay: Component = () => {
  useCollabPresence();

  // Los mensajes del chat de los demás salen como globo (salvo con el chat a la vista)
  let seen = chat().length;
  createEffect(() => {
    const list = chat();
    if (list.length < seen) seen = 0;
    const fresh = list.slice(seen);
    seen = list.length;
    untrack(() => {
      if (collabPanelOpen() && collabPanelTab() === "chat") return;
      for (const line of fresh) {
        if (line.mine || line.system || !line.from) continue;
        const key = ++bubbleKey;
        const id = line.from;
        setBubbles((m) => ({ ...m, [id]: { key, text: line.text, cursor: cursors()[id] } }));
        setTimeout(() => bubbles()[id]?.key === key && dropBubble(id), BUBBLE_MS);
      }
    });
  });

  // Mientras haya algo anclado al modelo se sigue a la cámara cuadro a cuadro
  createEffect(() => {
    if (threads().length === 0 && !draft() && Object.keys(cursors()).length === 0 && Object.keys(bubbles()).length === 0) return;
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
          <Show when={pinShown(th) && here(th.anchor) !== undefined}>
            {(() => {
              const pos = () => here(th.anchor) ?? { left: -99, top: -99 };
              return (
              <button
                class="absolute pointer-events-auto -translate-x-1/2 -translate-y-1/2 w-6 h-6 rounded-full text-[11px] text-white border-2 border-white shadow flex items-center justify-center"
                classList={{
                  "opacity-60": th.resolved,
                  "ring-2 ring-accent scale-110": selectedThread() === th.id || hoveredThread() === th.id,
                }}
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
          <Show when={sameSpot(c.id, c.view) && here(c) !== undefined}>
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
      </div>
      {/* Mensajes del chat como globos: junto a la persona, o en la esquina si no se sabe dónde está */}
      <For each={Object.keys(bubbles()).filter((id) => bubbleSpot(id, bubbles()[id]))}>
        {(id) => (
          <Show when={bubbles()[id] && bubbleSpot(id, bubbles()[id])}>
            {(s) => (
              <div
                class="absolute"
                style={{
                  left: `${s().left}px`,
                  top: `${s().top}px`,
                  transform: `${s().center ? "translateX(-50%)" : ""} ${s().up ? "translateY(-100%)" : ""}`,
                }}
              >
                <BubbleBox id={id} />
              </div>
            )}
          </Show>
        )}
      </For>
      <div class="absolute right-4 top-14 flex flex-col items-end gap-2">
        <For each={Object.keys(bubbles()).filter((id) => !bubbleSpot(id, bubbles()[id]))}>{(id) => <BubbleBox id={id} />}</For>
      </div>
      <div aria-hidden="true">
      <For each={ripples()}>
        {(r) => (
          <Show when={sameSpot(r.id, r.view) && place(r)}>
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
