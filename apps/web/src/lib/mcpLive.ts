// Modo en vivo del conector MCP: una IA maneja esta ventana por turnos. El
// backend (`mcp::live`) pregunta y avisa con eventos; acá se contestan: el
// turno (se niega si el usuario está editando), lo que el usuario tiene
// elegido, y deshacer o rehacer los pasos de la IA.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createSignal } from "solid-js";
import type { CadDocument, CadStore } from "./cad";
import type { CadUi } from "./cadUi";
import { t } from "./i18n";
import { createPersisted } from "./ui-state";

export interface LiveStatus {
  on: boolean;
  paused: boolean;
  port: number | null;
}

/** Lo que eligió el usuario (se recuerda entre sesiones) */
const [liveWanted, setLiveWanted] = createPersisted("mcp.live", false);
const [live, setLive] = createSignal<LiveStatus>({ on: false, paused: false, port: null });

export { live, liveWanted };

/** Lo que está haciendo la IA, por el nombre de la herramienta, para mostrarlo */
export function aiLabel(tool: string): string {
  const labels: Record<string, () => string> = {
    status: () => t("Mirando el estado"),
    help: () => t("Leyendo la guía"),
    selection: () => t("Mirando lo que elegiste"),
    undo: () => t("Deshaciendo su último paso"),
    redo: () => t("Rehaciendo su último paso"),
    cad_tab_create: () => t("Creando una pestaña"),
    cad_tab_rename: () => t("Renombrando una pestaña"),
    cad_tab_delete: () => t("Borrando una pestaña"),
    cad_design: () => t("Leyendo el diseño"),
    cad_geometry: () => t("Mirando caras y aristas"),
    view: () => t("Mirando el diseño"),
    measure: () => t("Midiendo"),
    sketch_create: () => t("Dibujando un sketch"),
    sketch_add: () => t("Editando un sketch"),
    cad_extrude: () => t("Extruyendo"),
    cad_revolve: () => t("Haciendo una revolución"),
    cad_primitive: () => t("Agregando un sólido"),
    cad_fillet: () => t("Redondeando aristas"),
    cad_chamfer: () => t("Achaflanando aristas"),
    cad_shell: () => t("Vaciando"),
    cad_add_feature: () => t("Agregando una operación"),
    cad_edit_feature: () => t("Cambiando una operación"),
    cad_delete_feature: () => t("Borrando una operación"),
    cad_parameters: () => t("Cambiando parámetros"),
    cad_export: () => t("Exportando"),
    asm_insert: () => t("Poniendo piezas en el ensamble"),
    asm_mate: () => t("Uniendo piezas"),
    asm_instance: () => t("Moviendo una pieza"),
    asm_mate_edit: () => t("Cambiando una unión"),
    asm_check: () => t("Revisando el ensamble"),
    drawing_export: () => t("Haciendo el plano"),
  };
  return labels[tool]?.() ?? tool;
}

/** Prende o apaga el modo en vivo */
export async function setLiveMode(on: boolean) {
  setLiveWanted(on);
  setLive(await invoke<LiveStatus>("mcp_live_set", { on }));
}

/** Pausa (la IA recibe «pausada» y espera) o reanuda */
export async function setLivePaused(paused: boolean) {
  setLive(await invoke<LiveStatus>("mcp_live_pause", { paused }));
}

/** Por qué no se puede dar el turno ahora (el usuario está en medio de algo) */
export function busyReason(cad: CadStore, ui: CadUi): string | undefined {
  if (ui.session()) return "the user is editing a sketch";
  if (cad.draft()) return "an operation dialog is open";
  if (cad.viewing() !== undefined) return "the user is looking at a saved version";
  return undefined;
}

/** Atiende al backend; devuelve con qué dejar de escuchar */
export function startMcpLive(cad: CadStore, ui: CadUi, notify: (message: string) => void): () => void {
  const answer = (id: number, payload: unknown) => void invoke("mcp_live_answer", { id, payload }).catch(() => {});
  const stops = [
    listen<{ id: number; title: string }>("mcp-live://turn", (e) => {
      const reason = busyReason(cad, ui);
      if (reason) return answer(e.payload.id, { ok: false, reason });
      cad.setAiTurn(e.payload.title || "…");
      answer(e.payload.id, { ok: true });
    }),
    listen<{ ok: boolean; title: string; changes: { tab: number; before: CadDocument }[] }>("mcp-live://done", async (e) => {
      try {
        await cad.adoptExternal(e.payload.ok ? e.payload.changes : []);
      } finally {
        cad.setAiTurn(undefined);
      }
      const title = e.payload.title && aiLabel(e.payload.title);
      if (title) notify(e.payload.ok ? t("IA: {title}", { title }) : t("IA: {title} — no se pudo, quedó como estaba", { title }));
    }),
    listen<{ id: number; what: string }>("mcp-live://ask", async (e) => {
      const { id, what } = e.payload;
      if (what === "selection") {
        const tab = cad.activeTab();
        return answer(id, { value: { tab: tab ? { id: tab.id, name: tab.name } : null, picks: ui.picks() } });
      }
      const back = what === "undo";
      if (await cad.aiUndo(back)) return answer(id, { value: back ? "the AI's last change" : "the AI's last undone change" });
      answer(id, { error: back ? "the last change in the app is the user's (or there is none): the AI only undoes its own steps" : "there is no undone AI step to redo" });
    }),
  ];
  // Prendido la vez anterior: se vuelve a prender al abrir la app
  if (liveWanted()) void setLiveMode(true).catch((err) => notify(String(err)));
  else void invoke<LiveStatus>("mcp_live_status").then(setLive).catch(() => {});
  return () => stops.forEach((s) => void s.then((stop) => stop()));
}
