// Diseño CAD en vivo (C4): cada cambio confirmado del documento de la pestaña
// activa se manda a los demás, que lo aplican y lo recalculan con su propio
// motor. El anfitrión manda; sin bloqueos todavía (C3), pero un choque se
// detecta: cada cambio dice sobre qué versión se hizo (`base`, un hash del
// documento anterior) y el anfitrión rechaza el que no coincide, devolviendo
// su versión.
//
// Lo que no se puede aplicar ahora (un diálogo abierto, otra pestaña) queda
// pendiente y se aplica cuando se puede. Pestañas nuevas, borradas o
// reordenadas no se sincronizan por acá: «Resincronizar».

import { addSystemLine, nameOf, onCollabData, onCollabProject, onCollabRoom, peers, sendData, status } from "./collab";
import type { CadDocument, CadStore } from "./cad";
import { t } from "./i18n";
import { resync } from "./collabSync";

interface DocMsg {
  tab: number;
  /** Hash del documento sobre el que se hizo el cambio (`null`: no se conoce) */
  base: string | null;
  doc: CadDocument;
  /** El anfitrión impone su versión (respuesta a un choque) */
  forced?: boolean;
  /** Quién hizo el cambio (id), para el aviso del chat */
  by?: string;
}

/** JSON con las llaves ordenadas: el mismo documento da el mismo hash en las dos apps */
function canon(x: unknown): string {
  if (Array.isArray(x)) return `[${x.map(canon).join(",")}]`;
  if (x && typeof x === "object") {
    const o = x as Record<string, unknown>;
    return `{${Object.keys(o)
      .filter((k) => o[k] !== undefined)
      .sort()
      .map((k) => `${JSON.stringify(k)}:${canon(o[k])}`)
      .join(",")}}`;
  }
  return JSON.stringify(x) ?? "null";
}

/** FNV-1a de 32 bits (basta para detectar versiones distintas) */
function hash(doc: unknown): string {
  const s = canon(doc);
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return (h >>> 0).toString(36) + s.length.toString(36);
}

export function installCollabDesign(cad: CadStore, notify: (message: string) => void): () => void {
  /** Último documento de cada pestaña que se sabe en común (el del anfitrión, si soy anfitrión) */
  const latest = new Map<number, { h: string; doc: CadDocument }>();
  /** Lo que llegó y todavía no se pudo aplicar */
  const pending = new Map<number, CadDocument>();
  let asked = 0;
  /** Último aviso al chat por pestaña */
  const told = new Map<number, number>();

  const active = () => cad.tabs().active;

  /**
   * Aplica lo pendiente de la pestaña activa. Un solo repaso a la vez, y cada
   * documento sale de la lista *antes* de aplicarse: si no, un segundo repaso
   * lo volvía a aplicar mientras el primero esperaba al backend y pisaba lo que
   * la persona acababa de editar.
   */
  let flushing = false;
  const flush = async () => {
    if (flushing) return;
    flushing = true;
    try {
      const tab = active();
      const doc = tab === null ? undefined : pending.get(tab);
      if (tab === null || !doc) return;
      pending.delete(tab);
      const applied = await cad.applyRemote(doc);
      // No se pudo ahora (diálogo abierto, IA trabajando): vuelve a la lista si no llegó otro
      if (!applied && !pending.has(tab)) pending.set(tab, doc);
    } finally {
      flushing = false;
    }
  };

  /** Cambio propio confirmado: se comparte (el anfitrión a todos; el invitado al anfitrión) */
  cad.setCollabHook((tab, doc) => {
    if (!status().on || peers().length === 0) return;
    const h = hash(doc);
    const before = latest.get(tab)?.h;
    // Es el eco de lo que acabo de recibir: ya se conoce
    if (before === h) return;
    latest.set(tab, { h, doc });
    // Lo que había llegado de afuera para esta pestaña ya quedó viejo
    pending.delete(tab);
    const msg: DocMsg = { tab, base: before ?? null, doc, by: status().my_id ?? undefined };
    void sendData("design_doc", msg).catch(() => {});
  });

  const stopData = onCollabData("design_doc", async (from, value) => {
    const m = value as DocMsg;
    if (!m || typeof m.tab !== "number" || !m.doc) return;
    const known = cad.tabs().tabs.some((t) => t.id === m.tab);
    if (!known) {
      // Una pestaña que no tengo: pido el proyecto de nuevo (como mucho cada 10 s)
      if (!status().host && Date.now() - asked > 10_000) {
        asked = Date.now();
        notify(t("Hay pestañas nuevas del anfitrión: resincronizando…"));
        void resync().catch(() => {});
      }
      return;
    }
    if (status().host) {
      // Un invitado propone un cambio: vale si parte de la versión que tengo.
      // Sin base (recién entró o recién se resincronizó: no conoce versiones) se acepta.
      const cur = latest.get(m.tab);
      if (cur !== undefined && m.base !== null && m.base !== cur.h) {
        notify(t("Un cambio de otra persona chocó con el tuyo; se mantuvo el tuyo."));
        void sendData("design_doc", { tab: m.tab, base: null, doc: cur.doc, forced: true } satisfies DocMsg, { only: from }).catch(() => {});
        return;
      }
      latest.set(m.tab, { h: hash(m.doc), doc: m.doc });
      void sendData("design_doc", m, { except: from }).catch(() => {});
    } else {
      if (m.forced) notify(t("Tu cambio chocó con otro del anfitrión; quedó la versión del anfitrión."));
      latest.set(m.tab, { h: hash(m.doc), doc: m.doc });
    }
    pending.set(m.tab, m.doc);
    // Quién cambió qué, en el chat (como mucho un aviso por pestaña cada 30 s)
    if (!m.forced) {
      const now = Date.now();
      if (now - (told.get(m.tab) ?? 0) > 30_000) {
        told.set(m.tab, now);
        const tabName = cad.tabs().tabs.find((t) => t.id === m.tab)?.name ?? "";
        addSystemLine(t("{who} cambió el diseño de «{tab}»", { who: (m.by && nameOf(m.by)) || t("Alguien"), tab: tabName }));
      }
    }
    await flush();
  });

  // Lo pendiente se aplica en cuanto se puede (se abrió esa pestaña, se cerró un diálogo)
  const timer = setInterval(() => void flush(), 500);
  // Sala nueva o llegó una copia del proyecto: las versiones conocidas dejan de
  // valer (la copia ya trae el diseño tal como está). Que entre alguien no las
  // borra: el que entra manda su primer cambio sin base y se acepta.
  const reset = () => {
    latest.clear();
    pending.clear();
  };
  const stopRoom = onCollabRoom(reset);
  const stopProject = onCollabProject(reset);

  return () => {
    cad.setCollabHook(undefined);
    clearInterval(timer);
    stopData();
    stopRoom();
    stopProject();
  };
}
