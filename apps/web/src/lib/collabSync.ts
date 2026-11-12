// Estado compartido (C4). Dos partes:
//  1. El proyecto entero: al entrar alguien a la sala el anfitrión le manda una
//     copia (un .pinocchio por un stream propio) y el invitado la abre; también
//     si el invitado pide «Resincronizar».
//  2. El diseño CAD en vivo (`collabDesign.ts`).
// Lo demás (mallas, rig, animación) se comparte con la copia del proyecto: si
// cambia, el anfitrión la vuelve a mandar con «Resincronizar».

import { invoke } from "@tauri-apps/api/core";
import { t } from "./i18n";
import { onCollabData, onCollabJoined, onCollabProject, onCollabRoom, requestResync, status } from "./collab";

export interface CollabSyncHost {
  /** Lo mismo que lleva «Guardar»: la interfaz como texto y el diseño con el sketch abierto */
  snapshot: () => { ui: string; design: unknown };
  /** El invitado abre el proyecto recibido (pregunta antes si hay trabajo sin guardar) */
  openShared: (path: string) => Promise<void>;
  notify: (message: string) => void;
}

let host: CollabSyncHost | undefined;
/** A quién se le está mandando el proyecto (no mandar dos veces a la vez) */
const sending = new Set<string>();

export function installCollabSync(h: CollabSyncHost): () => void {
  host = h;
  const stops = [
    onCollabJoined((peer) => void sendProject(peer.id, peer.name)),
    onCollabData("resync", (from) => {
      if (status().host) void sendProject(from);
    }),
    onCollabProject(async (path) => {
      try {
        await host?.openShared(path);
      } catch (e) {
        host?.notify(String(e));
      }
    }),
    onCollabRoom(() => sending.clear()),
  ];
  return () => {
    for (const s of stops) s();
    host = undefined;
  };
}

async function sendProject(to: string, name?: string) {
  if (!status().host || !host || sending.has(to)) return;
  sending.add(to);
  const who = name ?? "";
  try {
    host.notify(who ? t("Mandando el proyecto a {name}…", { name: who }) : t("Mandando el proyecto…"));
    const { ui, design } = host.snapshot();
    const bytes = await invoke<number>("collab_send_project", { to, ui, design: design ?? null });
    host.notify(t("Proyecto enviado ({mb} MB)", { mb: (bytes / 1e6).toFixed(1) }));
  } catch (e) {
    host.notify(t("No se pudo mandar el proyecto: {e}", { e: String(e) }));
  } finally {
    sending.delete(to);
  }
}

/** Invitado: pide al anfitrión que le mande el proyecto otra vez */
export const resync = requestResync;
