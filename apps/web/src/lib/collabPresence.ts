// Presencia (C1): dónde tiene el mouse cada persona y sus clics. Se manda por
// datagramas (sin garantía: un cursor perdido no importa). La posición va
// relativa al visor que está debajo (`data-collab-view`): "viewport" (3D) o
// "design" (CAD), no a la ventana, porque cada uno tiene su propio diseño.

import { createEffect, createSignal, onCleanup, onMount } from "solid-js";
import { onCollabLight, peers, sendLight, status } from "./collab";

export interface RemoteCursor {
  id: string;
  /** Visor donde está */
  view: string;
  /** 0..1 dentro del visor */
  x: number;
  y: number;
  /** Última vez que se movió (ms) */
  at: number;
}

export interface Ripple {
  key: number;
  id: string;
  view: string;
  x: number;
  y: number;
}

/** Sin movimiento por tanto tiempo, el cursor se oculta */
const CURSOR_TTL = 4000;
const RIPPLE_MS = 700;
/** Cada cuánto se manda la posición como máximo */
const SEND_EVERY = 40;

const [cursors, setCursors] = createSignal<Record<string, RemoteCursor>>({});
const [ripples, setRipples] = createSignal<Ripple[]>([]);
export { cursors, ripples };

interface Payload {
  id: string;
  v: string;
  x: number;
  y: number;
  /** 1 = clic */
  c?: number;
}

const enc = new TextEncoder();
const dec = new TextDecoder();
let rippleKey = 0;

function viewAt(target: EventTarget | null, e: PointerEvent): { view: string; x: number; y: number } | undefined {
  const el = (target as Element | null)?.closest?.("[data-collab-view]");
  if (!el) return undefined;
  const r = el.getBoundingClientRect();
  if (r.width < 1 || r.height < 1) return undefined;
  return { view: el.getAttribute("data-collab-view")!, x: (e.clientX - r.left) / r.width, y: (e.clientY - r.top) / r.height };
}

/** Escucha el mouse local mientras hay sala con alguien más, y los datagramas de los demás */
export function useCollabPresence() {
  let last = 0;
  const send = (p: Payload) => sendLight(enc.encode(JSON.stringify(p)));
  const myId = () => status().my_id ?? "";
  const active = () => status().on && peers().length > 0;

  const move = (e: PointerEvent) => {
    if (!active()) return;
    const now = performance.now();
    if (now - last < SEND_EVERY) return;
    const at = viewAt(e.target, e);
    if (!at) return;
    last = now;
    send({ id: myId(), v: at.view, x: at.x, y: at.y });
  };
  const down = (e: PointerEvent) => {
    if (!active()) return;
    const at = viewAt(e.target, e);
    if (at) send({ id: myId(), v: at.view, x: at.x, y: at.y, c: 1 });
  };

  onMount(() => {
    window.addEventListener("pointermove", move, { capture: true, passive: true });
    window.addEventListener("pointerdown", down, { capture: true, passive: true });
    const off = onCollabLight((_from, bytes) => {
      let p: Payload;
      try {
        p = JSON.parse(dec.decode(bytes)) as Payload;
      } catch {
        return;
      }
      if (typeof p.id !== "string" || p.id === myId() || typeof p.x !== "number" || typeof p.y !== "number") return;
      const at = Date.now();
      setCursors((c) => ({ ...c, [p.id]: { id: p.id, view: p.v, x: p.x, y: p.y, at } }));
      if (p.c) {
        const key = ++rippleKey;
        setRipples((r) => [...r, { key, id: p.id, view: p.v, x: p.x, y: p.y }]);
        setTimeout(() => setRipples((r) => r.filter((x) => x.key !== key)), RIPPLE_MS);
      }
    });
    // Oculta a quien dejó de moverse
    const sweep = setInterval(() => {
      const now = Date.now();
      setCursors((c) => {
        const keep = Object.entries(c).filter(([, v]) => now - v.at < CURSOR_TTL);
        return keep.length === Object.keys(c).length ? c : Object.fromEntries(keep);
      });
    }, 1000);
    onCleanup(() => {
      window.removeEventListener("pointermove", move, { capture: true } as EventListenerOptions);
      window.removeEventListener("pointerdown", down, { capture: true } as EventListenerOptions);
      off();
      clearInterval(sweep);
    });
  });

  // Al salir de la sala o irse alguien, se limpian sus cursores
  createEffect(() => {
    const ids = new Set(peers().map((p) => p.id));
    setCursors((c) => {
      const keep = Object.entries(c).filter(([id]) => ids.has(id));
      return keep.length === Object.keys(c).length ? c : Object.fromEntries(keep);
    });
  });
}
