// Colaboración en tiempo real (C0): estado de la sala P2P que abre el backend
// (`collab.rs`, crate `pinocchio-collab`). El anfitrión crea la sala y comparte
// un ticket; los invitados se unen con él. Acá viven quién está, el chat y las
// acciones; el backend pasa todo lo que llega por el evento `collab://event`.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { createSignal } from "solid-js";
import { t } from "./i18n";
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
  /** Aviso de la sala («Ana cambió el diseño…»), no es un mensaje de nadie */
  system?: boolean;
}

/** Mensajes fiables (`pinocchio_collab::Message`) */
export type CollabMessage =
  | { t: "hello"; name: string; color: string }
  | { t: "ping"; n: number }
  | { t: "pong"; n: number }
  | { t: "chat"; text: string; from?: string }
  | { t: "data"; kind: string; value: unknown };

type CollabEvent =
  | { kind: "joined"; id: string; name: string; color: string }
  | { kind: "left"; id: string }
  | { kind: "message"; from: string; msg: CollabMessage }
  | { kind: "light"; from: string; bytes: number[] }
  | { kind: "project"; from: string; path: string; bytes: number }
  | { kind: "project_error"; from: string; error: string }
  | { kind: "rejected"; reason: string }
  | { kind: "blob"; from: string; name: string };

const COLORS = ["#ff8800", "#2ec4b6", "#e63946", "#6a4c93", "#3a86ff", "#8ac926"];

/** Nombre y color que ven los demás (se recuerdan entre sesiones) */
const [collabName, setCollabName] = createPersisted("collab.name", "");
const [collabColor, setCollabColor] = createPersisted("collab.color", COLORS[Math.floor(Math.random() * COLORS.length)]);
/** Pedir mi aprobación a quien entre a mi sala, y con qué rol entra (se recuerdan) */
const [approvalWanted, setApprovalWanted] = createPersisted("collab.approval", true);
const [defaultRole, setDefaultRole] = createPersisted<"view" | "comment" | "edit">("collab.role", "edit");
export { approvalWanted, defaultRole, setApprovalWanted, setDefaultRole };
const [status, setStatus] = createSignal<CollabStatus>({ on: false, host: false, ticket: null, my_id: null });
const [peers, setPeers] = createSignal<Peer[]>([]);
const [chat, setChat] = createSignal<ChatLine[]>([]);
/** Panel izquierdo de colaboración (chat y comentarios) */
const [collabPanelOpen, setCollabPanelOpen] = createSignal(false);
const [collabPanelTab, setCollabPanelTab] = createSignal<"chat" | "comments">("chat");
/** Mensajes de chat sin leer (no cuenta mientras se mira la pestaña Chat) */
const [chatUnread, setChatUnread] = createSignal(0);

export { chat, chatUnread, collabColor, collabPanelOpen, collabPanelTab, setCollabPanelOpen, setCollabPanelTab, setChatUnread, collabName, COLORS, peers, setCollabColor, setCollabName, status };

/** Escuchadores de datagramas livianos (presencia, en la fase siguiente) */
type LightHandler = (from: string, bytes: Uint8Array) => void;
const lightHandlers = new Set<LightHandler>();
export function onCollabLight(h: LightHandler): () => void {
  lightHandlers.add(h);
  return () => lightHandlers.delete(h);
}

/** Cargas libres (`Message::Data`) por tipo: los comentarios y lo que venga después */
type DataHandler = (from: string, value: unknown) => void;
const dataHandlers = new Map<string, Set<DataHandler>>();
export function onCollabData(kind: string, h: DataHandler): () => void {
  const set = dataHandlers.get(kind) ?? new Set();
  set.add(h);
  dataHandlers.set(kind, set);
  return () => set.delete(h);
}

/** Llegó el proyecto del anfitrión (ruta del archivo en disco) */
type ProjectHandler = (path: string) => void | Promise<void>;
const projectHandlers = new Set<ProjectHandler>();
export function onCollabProject(h: ProjectHandler): () => void {
  projectHandlers.add(h);
  return () => projectHandlers.delete(h);
}
/** Avisos de la colaboración (errores de transporte, etc.) para mostrarlos al usuario */
const noticeHandlers = new Set<(text: string) => void>();
export function onCollabNotice(h: (text: string) => void): () => void {
  noticeHandlers.add(h);
  return () => noticeHandlers.delete(h);
}

/** Invitado: pide al anfitrión que le mande el proyecto otra vez */
export function requestResync(): Promise<void> {
  return sendData("resync", null);
}

/** Alguien entró (el anfitrión le manda el estado compartido) / la sala cambió */
const joinHandlers = new Set<(peer: Peer) => void>();
export function onCollabJoined(h: (peer: Peer) => void): () => void {
  joinHandlers.add(h);
  return () => joinHandlers.delete(h);
}
const roomHandlers = new Set<(on: boolean, host: boolean) => void>();
export function onCollabRoom(h: (on: boolean, host: boolean) => void): () => void {
  roomHandlers.add(h);
  return () => roomHandlers.delete(h);
}

/** Carga libre: anfitrión a todos (o a uno / menos uno); invitado al anfitrión */
export function sendData(kind: string, value: unknown, opts?: { except?: string; only?: string }): Promise<void> {
  return sendMessage({ t: "data", kind, value }, opts?.except, opts?.only);
}

/**
 * El anfitrión filtra lo que llega según el rol de quien lo manda (ver
 * `collabRoles.ts`): devuelve `false` para descartarlo. `need` es el nivel
 * que pide el mensaje: 1 ver, 2 comentar, 3 editar.
 */
let permit: ((from: string, need: 1 | 2 | 3) => boolean) | undefined;
export function setCollabPermit(fn: typeof permit) {
  permit = fn;
}
function needOf(msg: CollabMessage): 1 | 2 | 3 {
  if (msg.t === "chat") return 2;
  if (msg.t === "data") {
    if (msg.kind === "comment_op") return 2;
    if (msg.kind === "resync" || msg.kind === "where" || msg.kind === "standby_ready") return 1;
    return 3;
  }
  return 1;
}

/**
 * Reemplazo del anfitrión (lo manda el anfitrión a cada invitado): quién es,
 * el ticket de su sala de reemplazo y el rol de cada miembro. Si el anfitrión
 * se va o se cae, el reemplazo pasa a ser anfitrión y los demás se pasan solos.
 */
export interface Successor {
  id: string;
  ticket: string;
  roles: Record<string, "view" | "comment" | "edit">;
}
let successor: Successor | undefined;
export function setSuccessor(s: Successor | undefined) {
  successor = s;
}
/** Me sacaron o no me dejaron entrar: no hay que pasar al reemplazo */
let ending = false;

/** Se cambió de anfitrión: `promoted` = ahora lo soy yo */
const handoffHandlers = new Set<(promoted: boolean, roles: Successor["roles"]) => void>();
export function onCollabHandoff(h: (promoted: boolean, roles: Successor["roles"]) => void): () => void {
  handoffHandlers.add(h);
  return () => handoffHandlers.delete(h);
}

/** Mientras se cambia de anfitrión, lo que llega espera (si no, un «entró» se tomaría como el anfitrión viejo) */
let held: CollabEvent[] | null = null;

/**
 * Se fue el anfitrión y hay reemplazo: si soy yo, tomo la sala; si no, me
 * conecto a la del reemplazo (con mi mismo id, sin aprobación ni proyecto
 * nuevo: ya tengo el mismo). El chat y lo demás siguen.
 */
async function takeOver(old: string, s: Successor) {
  held = [];
  const oldName = nameOf(old) || t("El anfitrión");
  const newName = nameOf(s.id) || t("otra persona");
  successor = undefined;
  hostId = undefined;
  // Los demás vuelven a aparecer a medida que se reconectan
  setPeers([]);
  try {
    if (s.id === status().my_id) {
      setStatus(await invoke<CollabStatus>("collab_promote"));
      await invoke("collab_set_approval", { on: approvalWanted() }).catch(() => {});
      for (const h of roomHandlers) h(true, true);
      for (const h of handoffHandlers) h(true, s.roles);
      addSystemLine(t("{old} salió: ahora llevas tú la sala", { old: oldName }));
    } else {
      let next: CollabStatus | undefined;
      let lastError: unknown;
      // El reemplazo puede tardar un momento en tomar la sala
      for (let i = 0; i < 6 && !next; i++) {
        try {
          next = await invoke<CollabStatus>("collab_join", { ticket: s.ticket, ...me() });
        } catch (e) {
          lastError = e;
          await new Promise((r) => setTimeout(r, 1500));
        }
      }
      if (!next) throw lastError;
      setStatus(next);
      for (const h of roomHandlers) h(true, false);
      for (const h of handoffHandlers) h(false, s.roles);
      addSystemLine(t("{old} salió: ahora la sala la lleva {name}", { old: oldName, name: newName }));
    }
  } catch (e) {
    for (const h of noticeHandlers) h(t("No se pudo pasar a la sala de {name}: {e}", { name: newName, e: String(e) }));
    held = null;
    void leaveCollab();
    return;
  }
  const queued = held ?? [];
  held = null;
  for (const e of queued) handle(e);
}

/** Alguien se fue de la sala */
const leftHandlers = new Set<(id: string) => void>();
export function onCollabLeft(h: (id: string) => void): () => void {
  leftHandlers.add(h);
  return () => leftHandlers.delete(h);
}

/** Una línea de aviso en el chat de la sala */
export function addSystemLine(text: string) {
  setChat((c) => [...c, { from: "", name: "", text, mine: false, system: true }]);
}

/**
 * Invitado: la lista de la sala que manda el anfitrión (cada invitado solo
 * está conectado al anfitrión; así se conocen entre ellos). Los que ya no
 * están pasan por los avisos de «se fue».
 */
export function applyRoster(list: Peer[]) {
  if (status().host) return;
  const me = status().my_id;
  const next = list.filter((p) => p && typeof p.id === "string" && p.id !== me);
  const gone = peers().filter((p) => !next.some((n) => n.id === p.id));
  for (const g of gone) for (const h of leftHandlers) h(g.id);
  setPeers(next);
}

export const nameOf = (id: string) => peers().find((p) => p.id === id)?.name ?? "";

function handle(e: CollabEvent) {
  switch (e.kind) {
    case "joined":
      // El invitado solo ve entrar al anfitrión (a los demás los conoce por la lista)
      if (!status().host && hostId === undefined) hostId = e.id;
      setPeers((p) => [...p.filter((x) => x.id !== e.id), { id: e.id, name: e.name, color: e.color }]);
      for (const h of joinHandlers) h({ id: e.id, name: e.name, color: e.color });
      break;
    case "left":
      // Se fue el anfitrión y hay reemplazo: la sala sigue con él
      if (!status().host && e.id === hostId && successor && !ending) {
        void takeOver(e.id, successor);
        break;
      }
      for (const h of leftHandlers) h(e.id);
      setPeers((p) => p.filter((x) => x.id !== e.id));
      // Si era el anfitrión, la sala terminó para el invitado
      if (!status().host && e.id === hostId) {
        for (const h of noticeHandlers) h("La sala se cerró o te sacaron de ella");
        void leaveCollab();
      }
      break;
    case "message":
      if (status().host && permit && !permit(e.from, needOf(e.msg))) break;
      if (e.msg.t === "chat") {
        const text = e.msg.text;
        // Lo que reparte el anfitrión trae quién lo escribió de verdad
        const from = (!status().host && e.msg.from) || e.from;
        setChat((c) => [...c, { from, name: nameOf(from), text, mine: false }]);
        if (!(collabPanelOpen() && collabPanelTab() === "chat")) setChatUnread((n) => n + 1);
        // El anfitrión reparte el chat a los demás invitados, con el autor
        if (status().host) void sendMessage({ t: "chat", text, from }, from);
      } else if (e.msg.t === "data") {
        for (const h of dataHandlers.get(e.msg.kind) ?? []) h(e.from, e.msg.value);
      } else if (e.msg.t === "ping") {
        void sendMessage({ t: "pong", n: e.msg.n }, undefined, e.from);
      }
      break;
    case "project":
      // Solo el invitado abre un proyecto de la sala (el del anfitrión)
      if (status().host) break;
      for (const h of projectHandlers) void h(e.path);
      break;
    case "project_error":
      for (const h of noticeHandlers) h(e.error);
      break;
    case "rejected":
      ending = true;
      for (const h of noticeHandlers) h(`No se pudo entrar a la sala: ${e.reason}`);
      void leaveCollab();
      break;
    case "light":
      if (status().host && permit && !permit(e.from, 1)) break;
      for (const h of lightHandlers) h(e.from, new Uint8Array(e.bytes));
      break;
  }
}

let unlisten: UnlistenFn | undefined;
async function ensureListening() {
  if (!unlisten)
    unlisten = await listen<CollabEvent>("collab://event", (ev) => {
      if (held) held.push(ev.payload);
      else handle(ev.payload);
    });
}

/** Invitado: id del anfitrión de la sala en la que estoy */
let hostId: string | undefined;

function reset() {
  hostId = undefined;
  successor = undefined;
  ending = false;
  setPeers([]);
  setChat([]);
  setChatUnread(0);
}

const me = () => ({ name: collabName().trim() || "Anónimo", color: collabColor() });

/** Crea una sala y queda de anfitrión */
export async function hostCollab(): Promise<void> {
  await ensureListening();
  reset();
  setStatus(await invoke<CollabStatus>("collab_host", { ...me(), approval: approvalWanted() }));
  for (const h of roomHandlers) h(true, true);
}

/**
 * Antes de entrar a una sala (la interfaz pregunta si hay cambios sin guardar):
 * adentro, el proyecto de la sala reemplaza al propio sin preguntar.
 */
let beforeJoin: () => Promise<boolean> = async () => true;
export function setBeforeJoin(fn: () => Promise<boolean>) {
  beforeJoin = fn;
}

/** Se une a la sala de otra persona con su ticket */
export async function joinCollab(ticket: string): Promise<void> {
  if (!(await beforeJoin())) return;
  await ensureListening();
  reset();
  setStatus(await invoke<CollabStatus>("collab_join", { ticket: ticket.trim(), ...me() }));
  for (const h of roomHandlers) h(true, false);
}

export async function leaveCollab(): Promise<void> {
  setStatus(await invoke<CollabStatus>("collab_leave"));
  reset();
  for (const h of roomHandlers) h(false, false);
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
