// Quién puede qué (C5). El anfitrión es la autoridad: da un rol a cada persona
// (ver / comentar / editar), decide si entra (aprobación), la expulsa y cambia
// la clave de la sala. Lo que llega de alguien sin permiso se descarta en el
// anfitrión (ver `setCollabPermit`); además el invitado lo sabe y su interfaz
// lo respeta (solo lectura).
//
// Lo que un invitado jamás puede: tocar impresoras. No hay camino para eso:
// el único canal es el de datos de la sala (diseño, comentarios, bloqueos,
// chat), y el anfitrión nunca ejecuta comandos que mande un invitado.

import { invoke } from "@tauri-apps/api/core";
import { createSignal } from "solid-js";
import { defaultRole, onCollabData, onCollabJoined, onCollabLeft, onCollabRoom, sendData, setCollabPermit, status, type Peer } from "./collab";

export type Role = "view" | "comment" | "edit";
/** `pending`: entró pero el anfitrión todavía no lo admite */
type State = Role | "pending";

const LEVEL: Record<State, number> = { pending: 0, view: 1, comment: 2, edit: 3 };

/** Anfitrión: rol de cada persona (por id) */
const [roles, setRoles] = createSignal<Record<string, State>>({});
/** Invitado: mi rol (el anfitrión siempre edita) */
const [myRole, setMyRole] = createSignal<Role>("edit");
export { myRole, roles };

export const canComment = () => status().host || !status().on || LEVEL[myRole()] >= 2;
export const canEdit = () => status().host || !status().on || LEVEL[myRole()] >= 3;

/** Los que esperan que el anfitrión los admita */
export const pendingPeers = (peers: Peer[]) => peers.filter((p) => roles()[p.id] === "pending");

type RoleMsg = { role: Role };

const admitHandlers = new Set<(peer: Peer) => void>();
/** Una persona quedó admitida (anfitrión): ahí se le manda el proyecto y el estado compartido */
export function onCollabAdmitted(h: (peer: Peer) => void): () => void {
  admitHandlers.add(h);
  return () => admitHandlers.delete(h);
}

const known = new Map<string, Peer>();

function admitted(peer: Peer, role: Role) {
  setRoles({ ...roles(), [peer.id]: role });
  void invoke("collab_set_muted", { id: peer.id, muted: false }).catch(() => {});
  void sendData("collab_role", { role } satisfies RoleMsg, { only: peer.id }).catch(() => {});
  for (const h of admitHandlers) h(peer);
}

/** Anfitrión: admite a alguien que esperaba (con el rol por defecto) */
export function admit(id: string) {
  const peer = known.get(id);
  if (peer && roles()[id] === "pending") admitted(peer, defaultRole());
}

/** Anfitrión: rechaza (saca) a alguien */
export function kick(id: string) {
  void invoke("collab_kick", { id }).catch(() => {});
}

/** Anfitrión: cambia el rol de alguien ya admitido */
export function setRole(id: string, role: Role) {
  if (roles()[id] === undefined || roles()[id] === "pending") return;
  setRoles({ ...roles(), [id]: role });
  void sendData("collab_role", { role } satisfies RoleMsg, { only: id }).catch(() => {});
}

/** Anfitrión: nueva clave; el ticket anterior ya no sirve */
export async function rotateKey(): Promise<string | null> {
  const s = await invoke<{ ticket: string | null }>("collab_rotate_key");
  return s.ticket;
}

export function installCollabRoles(approval: () => boolean): () => void {
  setCollabPermit((from, need) => LEVEL[roles()[from] ?? "pending"] >= need);
  const stops = [
    onCollabJoined((peer) => {
      if (!status().host) return;
      known.set(peer.id, peer);
      if (approval()) setRoles({ ...roles(), [peer.id]: "pending" });
      else admitted(peer, defaultRole());
    }),
    onCollabLeft((id) => {
      known.delete(id);
      const { [id]: _gone, ...rest } = roles();
      setRoles(rest);
    }),
    onCollabData("collab_role", (_from, value) => {
      const m = value as RoleMsg;
      if (!status().host && m && (m.role === "view" || m.role === "comment" || m.role === "edit")) setMyRole(m.role);
    }),
    onCollabRoom(() => {
      setRoles({});
      known.clear();
      setMyRole("edit");
    }),
  ];
  return () => {
    setCollabPermit(undefined);
    for (const s of stops) s();
  };
}
