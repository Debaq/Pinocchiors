// Colaboración en tiempo real (C0): estado de la sala P2P que abre el backend
// (`collab.rs`, crate `pinocchio-collab`). El anfitrión crea la sala y comparte
// un ticket; los invitados se unen con él. Acá viven quién está, el chat y las
// acciones; el backend pasa todo lo que llega por el evento `collab://event`.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { createSignal } from "solid-js";
import { createPersisted } from "./ui-state";

export interface CollabStatus {
  on: boolean;
  host: boolean;
  ticket: string | null;
  my_id: string | null;
}

export interface Peer {
  id: string;
  name: string;
  color: string;
}

export interface ChatLine {
  from: string;
  name: string;
  text: string;
  mine: boolean;
}

/** Mensajes fiables (`pinocchio_collab::Message`) */
export type CollabMessage =
  | { t: "hello"; name: string; color: string }
  | { t: "ping"; n: number }
  | { t: "pong"; n: number }
  | { t: "chat"; text: string }
  | { t: "data"; kind: string; value: unknown };

type CollabEvent =
  | { kind: "joined"; id: string; name: string; color: string }
  | { kind: "left"; id: string }
  | { kind: "message"; from: string; msg: CollabMessage }
  | { kind: "light"; from: string; bytes: number[] };

const COLORS = ["#ff8800", "#2ec4b6", "#e63946", "#6a4c93", "#3a86ff", "#8ac926"];

/** Nombre y color que ven los demás (se recuerdan entre sesiones) */
const [collabName, setCollabName] = createPersisted("collab.name", "");
const [collabColor, setCollabColor] = createPersisted("collab.color", COLORS[Math.floor(Math.random() * COLORS.length)]);
const [status, setStatus] = createSignal<CollabStatus>({ on: false, host: false, ticket: null, my_id: null });
const [peers, setPeers] = createSignal<Peer[]>([]);
const [chat, setChat] = createSignal<ChatLine[]>([]);

export { chat, collabColor, collabName, COLORS, peers, setCollabColor, setCollabName, status };

/** Escuchadores de datagramas livianos (presencia, en la fase siguiente) */
type LightHandler = (from: string, bytes: Uint8Array) => void;
const lightHandlers = new Set<LightHandler>();
export function onCollabLight(h: LightHandler): () => void {
  lightHandlers.add(h);
  return () => lightHandlers.delete(h);
}

const nameOf = (id: string) => peers().find((p) => p.id === id)?.name ?? "";

function handle(e: CollabEvent) {
  switch (e.kind) {
    case "joined":
      setPeers((p) => [...p.filter((x) => x.id !== e.id), { id: e.id, name: e.name, color: e.color }]);
      break;
    case "left":
      setPeers((p) => p.filter((x) => x.id !== e.id));
      // Si era el anfitrión, la sala terminó para el invitado
      if (!status().host && peers().length === 0) void leaveCollab();
      break;
    case "message":
      if (e.msg.t === "chat") {
        const text = e.msg.text;
        const from = e.from;
        setChat((c) => [...c, { from, name: nameOf(from), text, mine: false }]);
        // El anfitrión reparte el chat a los demás invitados
        if (status().host) void sendMessage(e.msg, from);
      } else if (e.msg.t === "ping") {
        void sendMessage({ t: "pong", n: e.msg.n }, undefined, e.from);
      }
      break;
    case "light":
      for (const h of lightHandlers) h(e.from, new Uint8Array(e.bytes));
      break;
  }
}

let unlisten: UnlistenFn | undefined;
async function ensureListening() {
  if (!unlisten) unlisten = await listen<CollabEvent>("collab://event", (ev) => handle(ev.payload));
}

function reset() {
  setPeers([]);
  setChat([]);
}

const me = () => ({ name: collabName().trim() || "Anónimo", color: collabColor() });

/** Crea una sala y queda de anfitrión */
export async function hostCollab(): Promise<void> {
  await ensureListening();
  reset();
  setStatus(await invoke<CollabStatus>("collab_host", me()));
}

/** Se une a la sala de otra persona con su ticket */
export async function joinCollab(ticket: string): Promise<void> {
  await ensureListening();
  reset();
  setStatus(await invoke<CollabStatus>("collab_join", { ticket: ticket.trim(), ...me() }));
}

export async function leaveCollab(): Promise<void> {
  setStatus(await invoke<CollabStatus>("collab_leave"));
  reset();
}

/** Manda un mensaje fiable. `except`/`only` sirven al anfitrión para repartir. */
async function sendMessage(msg: CollabMessage, except?: string, only?: string): Promise<void> {
  await invoke("collab_send", { msg, except: except ?? null, only: only ?? null });
}

export async function sendChat(text: string): Promise<void> {
  const t = text.trim();
  if (!t) return;
  await sendMessage({ t: "chat", text: t });
  setChat((c) => [...c, { from: status().my_id ?? "", name: collabName(), text: t, mine: true }]);
}

/** Datagrama sin garantía a todos (cursores, clics) */
export function sendLight(bytes: Uint8Array): void {
  void invoke("collab_send_light", { bytes: Array.from(bytes) }).catch(() => {});
}
