// Bloqueo por pestaña de diseño (C3): quien abre un sketch o un diálogo de
// operación en una pestaña la toma; los demás la ven con su nombre y color y no
// pueden cambiarla hasta que la suelte. El anfitrión es quien decide: el
// invitado pide («acquire») y el anfitrión contesta con el estado de todos los
// bloqueos. Si el que la tenía se va de la sala, se libera sola.

import { createSignal } from "solid-js";
import { nameOf, onCollabData, onCollabLeft, onCollabRoom, peers, sendData, status } from "./collab";
import { collabColor, collabName } from "./collab";
import { canEdit, onCollabAdmitted } from "./collabRoles";

export interface Lock {
  tab: number;
  /** Id de quien la tiene */
  owner: string;
  name: string;
  color: string;
}

type LockMsg =
  | { op: "acquire"; tab: number }
  | { op: "release"; tab: number }
  | { op: "state"; locks: Lock[] }
  | { op: "denied"; tab: number; name: string };

const [locks, setLocks] = createSignal<Lock[]>([]);
export { locks };

/** Pestaña de diseño activa (la pone la interfaz) */
const [activeDesignTab, setActiveDesignTab] = createSignal<number | null>(null);
export { activeDesignTab, setActiveDesignTab };

const myId = () => status().my_id ?? "";
const colorOf = (id: string) => peers().find((p) => p.id === id)?.color;

/** Quién la tiene, si es otra persona */
export const lockedByOther = (tab: number | null | undefined): Lock | undefined =>
  tab == null ? undefined : locks().find((l) => l.tab === tab && l.owner !== myId());

/** Pestaña que estoy editando (con sketch o diálogo abiertos) */
let mine: number | null = null;
let onDenied: (tab: number, by: string) => void = () => {};

const broadcast = (list: Lock[], only?: string) => void sendData("design_lock", { op: "state", locks: list } satisfies LockMsg, only ? { only } : undefined).catch(() => {});

/** Anfitrión: intenta dar la pestaña a `owner`; `false` si la tiene otro */
function grant(tab: number, owner: string, name: string, color: string): Lock | undefined {
  const cur = locks().find((l) => l.tab === tab);
  if (cur && cur.owner !== owner) return cur;
  if (!cur) {
    const next = [...locks(), { tab, owner, name, color }];
    setLocks(next);
    broadcast(next);
  }
  return undefined;
}

function free(tab: number, owner: string) {
  const next = locks().filter((l) => !(l.tab === tab && l.owner === owner));
  if (next.length !== locks().length) {
    setLocks(next);
    broadcast(next);
  }
}

/** La interfaz avisa qué pestaña está editando (`null`: ninguna) */
export function setEditing(tab: number | null) {
  if (tab === mine) return;
  const before = mine;
  mine = tab;
  if (!status().on) return;
  if (before !== null) {
    if (status().host) free(before, myId());
    else void sendData("design_lock", { op: "release", tab: before } satisfies LockMsg).catch(() => {});
  }
  if (tab === null) return;
  // Sin permiso de editar no se toma nada
  if (!canEdit()) {
    mine = null;
    onDenied(tab, "");
    return;
  }
  const other = lockedByOther(tab);
  if (other) {
    mine = null;
    onDenied(tab, other.name);
    return;
  }
  if (status().host) {
    const held = grant(tab, myId(), collabName().trim() || "Anónimo", collabColor());
    if (held) {
      mine = null;
      onDenied(tab, held.name);
    }
  } else {
    void sendData("design_lock", { op: "acquire", tab } satisfies LockMsg).catch(() => {});
  }
}

export function installCollabLocks(denied: (tab: number, by: string) => void): () => void {
  onDenied = denied;
  const stops = [
    onCollabData("design_lock", (from, value) => {
      const m = value as LockMsg;
      if (!m || typeof m.op !== "string") return;
      if (status().host) {
        if (m.op === "acquire") {
          const held = grant(m.tab, from, nameOf(from), colorOf(from) ?? "#888");
          if (held) void sendData("design_lock", { op: "denied", tab: m.tab, name: held.name } satisfies LockMsg, { only: from }).catch(() => {});
        } else if (m.op === "release") free(m.tab, from);
      } else if (m.op === "state") {
        setLocks(m.locks);
      } else if (m.op === "denied") {
        if (mine === m.tab) mine = null;
        onDenied(m.tab, m.name);
      }
    }),
    // El anfitrión le cuenta el estado al que entra
    onCollabAdmitted((peer) => broadcast(locks(), peer.id)),
    // Si se va quien tenía una pestaña, se libera
    onCollabLeft((id) => {
      if (!status().host) return;
      const next = locks().filter((l) => l.owner !== id);
      if (next.length !== locks().length) {
        setLocks(next);
        broadcast(next);
      }
    }),
    onCollabRoom(() => {
      setLocks([]);
      mine = null;
    }),
  ];
  return () => {
    for (const s of stops) s();
    onDenied = () => {};
  };
}
