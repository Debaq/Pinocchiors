// Comentarios anclados (C2): un hilo se pega a un punto de la pieza (o, si el
// visor no lo sabe, a una posición del visor) y tiene respuestas y un estado
// abierto/resuelto. Funcionan solos (anotaciones del proyecto) y, dentro de una
// sala, se comparten: el anfitrión es el dueño del estado, los invitados le
// mandan operaciones y él las reparte. Se guardan en el proyecto (`ui.json`).

import { createSignal } from "solid-js";
import { collabColor, collabName, onCollabData, onCollabRoom, sendData, status } from "./collab";
import { canComment, onCollabAdmitted } from "./collabRoles";
import { collabView } from "./collabViews";

export type CommentView = "viewport" | "design";

export interface Anchor {
  view: CommentView;
  /** 0..1 dentro del visor (por si no hay punto 3D) */
  x: number;
  y: number;
  /** Punto de la superficie, en metros */
  p?: [number, number, number];
  /** Qué tenía elegido el autor (objeto o pestaña de diseño) */
  target?: string;
}

export interface Comment {
  id: string;
  author: string;
  color: string;
  text: string;
  at: number;
}

export interface Thread {
  id: string;
  anchor: Anchor;
  comments: Comment[];
  resolved: boolean;
  /** Número que se ve en el pin (crece, no se reutiliza) */
  n: number;
}

type Op =
  | { op: "thread"; thread: Thread }
  | { op: "reply"; thread: string; comment: Comment }
  | { op: "resolve"; thread: string; resolved: boolean }
  | { op: "delete"; thread: string };

const [threads, setThreads] = createSignal<Thread[]>([]);
/** El hilo abierto en el panel (y resaltado en el visor) */
const [selectedThread, setSelectedThread] = createSignal<string>();
/** Modo «poner un comentario»: el próximo clic en un visor fija el punto */
const [placing, setPlacing] = createSignal(false);
/** Punto elegido, esperando el texto del primer comentario */
const [draft, setDraft] = createSignal<Anchor>();
export { draft, placing, selectedThread, setDraft, setPlacing, setSelectedThread, threads };

const uid = () => `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`;
const me = () => ({ author: collabName().trim() || "Anónimo", color: collabColor() });

function apply(o: Op) {
  setThreads((list) => {
    switch (o.op) {
      case "thread":
        return list.some((t) => t.id === o.thread.id) ? list : [...list, o.thread];
      case "reply":
        return list.map((t) => (t.id === o.thread && !t.comments.some((c) => c.id === o.comment.id) ? { ...t, comments: [...t.comments, o.comment] } : t));
      case "resolve":
        return list.map((t) => (t.id === o.thread ? { ...t, resolved: o.resolved } : t));
      case "delete":
        return list.filter((t) => t.id !== o.thread);
    }
  });
}

/** Una operación propia: se aplica ya y se manda (el anfitrión a todos, el invitado al anfitrión) */
function local(o: Op) {
  apply(o);
  if (status().on) void sendData("comment_op", o).catch(() => {});
}

export function addThread(anchor: Anchor, text: string): string | undefined {
  const t = text.trim();
  if (!t || !canComment()) return undefined;
  const n = Math.max(0, ...threads().map((x) => x.n)) + 1;
  const thread: Thread = { id: uid(), anchor, comments: [{ id: uid(), ...me(), text: t, at: Date.now() }], resolved: false, n };
  local({ op: "thread", thread });
  setSelectedThread(thread.id);
  return thread.id;
}

export function addReply(thread: string, text: string) {
  const t = text.trim();
  if (t && canComment()) local({ op: "reply", thread, comment: { id: uid(), ...me(), text: t, at: Date.now() } });
}

export function setResolved(thread: string, resolved: boolean) {
  if (!canComment()) return;
  local({ op: "resolve", thread, resolved });
}

export function deleteThread(thread: string) {
  if (!canComment()) return;
  if (selectedThread() === thread) setSelectedThread(undefined);
  local({ op: "delete", thread });
}

/** Punto de un clic en un visor: de la superficie si toca la pieza, si no el del rayo */
export function anchorAt(view: CommentView, x: number, y: number, target?: string): Anchor {
  const anchor: Anchor = { view, x, y, target };
  const v = collabView(view);
  if (v) {
    const ndc: [number, number] = [x * 2 - 1, 1 - y * 2];
    try {
      anchor.p = v.surface?.(ndc) ?? v.rayPoint(ndc);
    } catch {
      // Sin cámara lista: queda la posición del visor
    }
  }
  return anchor;
}

/** Lleva la cámara a un hilo sin cambiar hacia dónde mira (traslada el objetivo al punto) */
export function goToThread(id: string) {
  const a = threads().find((t) => t.id === id)?.anchor;
  const v = a && collabView(a.view);
  if (!a?.p || !v) return;
  try {
    const pose = v.pose();
    const d = a.p.map((n, i) => n - pose.target[i]);
    v.setPose({
      position: pose.position.map((n, i) => n + d[i]) as [number, number, number],
      target: [...a.p] as [number, number, number],
      up: pose.up,
    });
  } catch {
    // El visor no está listo
  }
}

// ── Guardado en el proyecto ───────────────────────────────────────────────────

export const commentsForProject = (): Thread[] => threads();

export function loadComments(list: unknown) {
  setSelectedThread(undefined);
  setDraft(undefined);
  setThreads(Array.isArray(list) ? (list as Thread[]).filter((t) => t && typeof t.id === "string" && Array.isArray(t.comments)) : []);
}

// ── Sincronización por la sala ───────────────────────────────────────────────

/** Lo que había antes de unirme a una sala ajena (se recupera al salir) */
let stashed: Thread[] | undefined;

onCollabData("comment_op", (from, value) => {
  const o = value as Op;
  if (!o || typeof o.op !== "string") return;
  apply(o);
  // El anfitrión reparte a los demás lo que mandó un invitado
  if (status().host) void sendData("comment_op", o, { except: from }).catch(() => {});
});

onCollabData("comments_full", (_from, value) => {
  if (!Array.isArray(value)) return;
  if (!stashed) stashed = threads();
  setThreads(value as Thread[]);
});

onCollabAdmitted((peer) => {
  if (status().host) void sendData("comments_full", threads(), { only: peer.id }).catch(() => {});
});

onCollabRoom((on, host) => {
  if (!on && stashed) {
    setThreads(stashed);
    stashed = undefined;
  }
  if (!on) {
    setSelectedThread(undefined);
    setDraft(undefined);
  }
  if (on && host) stashed = undefined;
});
