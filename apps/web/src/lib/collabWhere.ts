// Dónde está cada persona de la sala: en qué área (Preparar, Diseñar, Rig…) y,
// en Diseñar, en qué pestaña de diseño o ensamble. Se ve como una línea de su
// color debajo de esa pestaña. Va por mensajes fiables (es estado, no un
// cursor): cada uno manda el suyo al cambiar y el anfitrión lo reparte.

import { createSignal } from "solid-js";
import { onCollabData, onCollabLeft, onCollabRoom, peers, sendData, status } from "./collab";
import { onCollabAdmitted } from "./collabRoles";

export interface Where {
  /** Área (id del espacio de trabajo) */
  ws?: string;
  /** Pestaña de diseño activa (solo cuenta en Diseñar) */
  tab?: number | null;
}

type WhereMsg = Where & { id?: string };

const [whereOf, setWhereOf] = createSignal<Record<string, Where>>({});
export { whereOf };

let mine: Where = {};
/** Dónde estoy yo (para saber si otro está en otra parte) */
const [myWhere, setMyWhereSignal] = createSignal<Where>({});
export { myWhere };
const myId = () => status().my_id ?? "";

const clean = (v: unknown): Where | undefined => {
  if (!v || typeof v !== "object") return undefined;
  const o = v as WhereMsg;
  return {
    ws: typeof o.ws === "string" ? o.ws : undefined,
    tab: typeof o.tab === "number" ? o.tab : null,
  };
};

function send() {
  if (!status().on) return;
  void sendData("where", { ...mine, id: myId() } satisfies WhereMsg).catch(() => {});
}

/** La interfaz avisa dónde estoy (se manda solo si cambió) */
export function setMyWhere(w: Where) {
  if (w.ws === mine.ws && (w.tab ?? null) === (mine.tab ?? null)) return;
  mine = { ws: w.ws, tab: w.tab ?? null };
  setMyWhereSignal(mine);
  send();
}

/** Vuelve a mandar el mío (al entrar alguien o al quedar admitido) */
export const resendWhere = send;

/** Personas en un área (y, si se da, en una pestaña de diseño), en el orden de la sala */
export function peersAt(ws: string, tab?: number) {
  const map = whereOf();
  return peers().filter((p) => {
    const w = map[p.id];
    if (!w || w.ws !== ws) return false;
    return tab === undefined || w.tab === tab;
  });
}

onCollabData("where", (from, value) => {
  const w = clean(value);
  if (!w) return;
  // Lo que reparte el anfitrión trae de quién es
  const id = !status().host && typeof (value as WhereMsg).id === "string" ? (value as WhereMsg).id! : from;
  if (id === myId()) return;
  setWhereOf((m) => ({ ...m, [id]: w }));
  if (status().host) void sendData("where", { ...w, id } satisfies WhereMsg, { except: from }).catch(() => {});
});

// El anfitrión le cuenta al que entra dónde están todos (él incluido)
onCollabAdmitted((peer) => {
  if (!status().host) return;
  const list: WhereMsg[] = [{ ...mine, id: myId() }, ...Object.entries(whereOf()).filter(([id]) => id !== peer.id).map(([id, w]) => ({ ...w, id }))];
  for (const w of list) void sendData("where", w, { only: peer.id }).catch(() => {});
});

// El invitado recién admitido vuelve a mandar el suyo (antes el anfitrión lo descartaba)
onCollabData("collab_role", () => {
  if (!status().host) send();
});

onCollabLeft((id) => setWhereOf((m) => {
  if (!(id in m)) return m;
  const next = { ...m };
  delete next[id];
  return next;
}));

onCollabRoom((on) => {
  setWhereOf({});
  if (on) send();
});
