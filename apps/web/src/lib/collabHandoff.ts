// Reemplazo del anfitrión: la sala no se cierra cuando el anfitrión se va o
// se cae. El anfitrión elige a un invitado de reemplazo (el primero que puede
// editar; si no hay, el primero admitido) y le pasa quiénes son los miembros y
// su rol. El reemplazo deja lista su sala (`collab_standby`) y devuelve el
// ticket; el anfitrión se lo reparte a todos. Si el anfitrión se va, el cambio
// lo hace `collab.ts` (`takeOver`): el reemplazo toma la sala y los demás se
// conectan a él con su mismo id, su rol y el proyecto que ya tienen.

import { invoke } from "@tauri-apps/api/core";
import { createEffect, createRoot } from "solid-js";
import { onCollabData, sendData, setSuccessor, status, type Successor } from "./collab";
import { roles, type Role } from "./collabRoles";

interface Ask {
  allow: string[];
}

export function installCollabHandoff(): () => void {
  /** Anfitrión: a quién se le pidió ser reemplazo y su ticket */
  let pick: string | undefined;
  let ticket: string | undefined;
  let sentKey = "";
  let members: Successor["roles"] = {};

  const share = () => {
    if (!pick || !ticket) return;
    void sendData("successor", { id: pick, ticket, roles: members } satisfies Successor).catch(() => {});
  };

  const dispose = createRoot((dispose) => {
    createEffect(() => {
      const s = status();
      const all = roles();
      if (!s.on || !s.host) {
        pick = undefined;
        ticket = undefined;
        sentKey = "";
        return;
      }
      const admitted = Object.entries(all).filter((e): e is [string, Role] => e[1] !== "pending");
      const candidate = (admitted.find(([, r]) => r === "edit") ?? admitted[0])?.[0];
      if (!candidate) {
        pick = undefined;
        ticket = undefined;
        sentKey = "";
        return;
      }
      // Yo también puedo volver (si me caigo y la app sigue abierta), como editor
      members = { ...Object.fromEntries(admitted), ...(s.my_id ? { [s.my_id]: "edit" as Role } : {}) };
      const allow = Object.keys(members).filter((id) => id !== candidate);
      const key = JSON.stringify([candidate, members]);
      if (key === sentKey) return;
      sentKey = key;
      if (candidate !== pick) {
        pick = candidate;
        ticket = undefined;
      }
      void sendData("standby_ask", { allow } satisfies Ask, { only: candidate }).catch(() => {});
      // Los que ya tenían el ticket se enteran de los roles nuevos
      share();
    });
    return dispose;
  });

  const stops = [
    // Invitado: el anfitrión me pide ser reemplazo
    onCollabData("standby_ask", (_from, value) => {
      const m = value as Ask;
      if (status().host || !m || !Array.isArray(m.allow)) return;
      void invoke<string>("collab_standby", { allow: m.allow.filter((x) => typeof x === "string") })
        .then((t) => sendData("standby_ready", { ticket: t }))
        .catch(() => {});
    }),
    // Anfitrión: el reemplazo tiene su sala lista; todos reciben el ticket
    onCollabData("standby_ready", (from, value) => {
      const t = (value as { ticket?: unknown })?.ticket;
      if (!status().host || from !== pick || typeof t !== "string") return;
      ticket = t;
      share();
    }),
    // Invitado: quién es el reemplazo y cómo llegar a su sala
    onCollabData("successor", (_from, value) => {
      const m = value as Successor;
      if (status().host || !m || typeof m.id !== "string" || typeof m.ticket !== "string" || !m.roles || typeof m.roles !== "object") return;
      setSuccessor(m);
    }),
  ];
  return () => {
    dispose();
    for (const s of stops) s();
  };
}
