// Presencia (C1): dónde tiene el mouse cada persona y sus clics. Se manda por
// datagramas (sin garantía: un cursor perdido no importa). La posición va
// relativa al visor que está debajo (`data-collab-view`): "viewport" (3D) o
// "design" (CAD), no a la ventana, porque cada uno tiene su propio diseño.

import { createEffect, createSignal, onCleanup, onMount } from "solid-js";
import type { CameraPose } from "./cameraRig";
import { onCollabLight, peers, sendLight, status } from "./collab";
import { collabView } from "./collabViews";

export interface RemoteCursor {
  id: string;
  /** Visor donde está */
  view: string;
  /** 0..1 dentro del visor */
  x: number;
  y: number;
  /** Punto 3D (metros) bajo el cursor, si el visor lo sabe: se ve igual con otra cámara */
  p?: [number, number, number];
  /** Qué tiene elegido (objeto o pestaña de diseño) */
  label?: string;
  /** Última vez que se movió (ms) */
  at: number;
}

export interface Ripple {
  key: number;
  id: string;
  view: string;
  x: number;
  y: number;
  p?: [number, number, number];
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
  p?: [number, number, number];
  s?: string;
  /** Cámara de ese visor (mensaje aparte del cursor) */
  cam?: CameraPose;
}

/** Lo que cada uno tiene elegido, lo pone la interfaz (`setPresenceLabel`) */
const labels: Record<string, string> = {};
export function setPresenceLabel(view: "viewport" | "design", label: string | undefined) {
  if (label) labels[view] = label;
  else delete labels[view];
}

/** A quién se está siguiendo la cámara (id) */
const [followId, setFollowId] = createSignal<string>();
export { followId };
export function toggleFollow(id: string) {
  setFollowId(followId() === id ? undefined : id);
}

/** Visor que se está usando: Diseñar tapa al principal cuando está abierto */
const activeView = () => (document.querySelector('[data-collab-view="design"]') ? "design" : "viewport");

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
    send({ id: myId(), v: at.view, x: at.x, y: at.y, ...extra(at) });
  };
  const down = (e: PointerEvent) => {
    if (!active()) return;
    const at = viewAt(e.target, e);
    if (!at) return;
    // Tocar el visor suelta la cámara del otro (como en cualquier editor compartido)
    if (followId()) setFollowId(undefined);
    send({ id: myId(), v: at.view, x: at.x, y: at.y, c: 1, ...extra(at) });
  };
  const wheel = (e: WheelEvent) => {
    if (followId() && (e.target as Element | null)?.closest?.("[data-collab-view]")) setFollowId(undefined);
  };
  /** Punto 3D y qué tiene elegido, si se saben */
  const extra = (at: { view: string; x: number; y: number }): Partial<Payload> => {
    const out: Partial<Payload> = {};
    const v = collabView(at.view);
    if (v) {
      try {
        out.p = v.rayPoint([at.x * 2 - 1, 1 - at.y * 2]).map((n) => Math.round(n * 1e5) / 1e5) as [number, number, number];
      } catch {
        // Sin cámara lista: queda la posición plana
      }
    }
    if (labels[at.view]) out.s = labels[at.view];
    return out;
  };

  // Cámara propia: se manda al cambiar (y cada tanto, para quien entra o empieza a seguir)
  let lastCam = "";
  let lastCamAt = 0;
  const camTimer = setInterval(() => {
    if (!active() || followId()) return;
    const view = activeView();
    const v = collabView(view);
    if (!v) return;
    let pose: CameraPose;
    try {
      pose = v.pose();
    } catch {
      return;
    }
    const key = JSON.stringify(pose, (_k, n) => (typeof n === "number" ? Math.round(n * 1e5) / 1e5 : n));
    const now = performance.now();
    if (key === lastCam && now - lastCamAt < 1500) return;
    lastCam = key;
    lastCamAt = now;
    send({ id: myId(), v: view, x: 0, y: 0, cam: pose });
  }, 120);

  onMount(() => {
    window.addEventListener("pointermove", move, { capture: true, passive: true });
    window.addEventListener("pointerdown", down, { capture: true, passive: true });
    window.addEventListener("wheel", wheel, { capture: true, passive: true });
    const off = onCollabLight((_from, bytes) => {
      let p: Payload;
      try {
        p = JSON.parse(dec.decode(bytes)) as Payload;
      } catch {
        return;
      }
      if (typeof p.id !== "string" || p.id === myId() || typeof p.x !== "number" || typeof p.y !== "number") return;
      if (p.cam) {
        // Cámara del otro: solo si lo sigo
        if (followId() === p.id) {
          try {
            collabView(p.v)?.setPose(p.cam);
          } catch {
            // El visor no está listo
          }
        }
        return;
      }
      const at = Date.now();
      setCursors((c) => ({ ...c, [p.id]: { id: p.id, view: p.v, x: p.x, y: p.y, p: p.p, label: p.s, at } }));
      if (p.c) {
        const key = ++rippleKey;
        setRipples((r) => [...r, { key, id: p.id, view: p.v, x: p.x, y: p.y, p: p.p }]);
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
      window.removeEventListener("wheel", wheel, { capture: true } as EventListenerOptions);
      clearInterval(camTimer);
      off();
      clearInterval(sweep);
    });
  });

  // Al salir de la sala o irse alguien, se limpian sus cursores
  createEffect(() => {
    const ids = new Set(peers().map((p) => p.id));
    if (followId() && !ids.has(followId()!)) setFollowId(undefined);
    setCursors((c) => {
      const keep = Object.entries(c).filter(([id]) => ids.has(id));
      return keep.length === Object.keys(c).length ? c : Object.fromEntries(keep);
    });
  });
}
