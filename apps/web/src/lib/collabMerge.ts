// Volver a una sala con una copia del mismo proyecto: lo que alguien avanzó
// solo en sus diseños y ensambles se suma a la sala en vez de perderse.
//
// Cada proyecto que pasa por una sala lleva un número propio (`id`) y cada
// copia recuerda cómo estaba cada pestaña la última vez que estuvo en la sala
// (`base`: un hash por pestaña). Al entrar con una copia del mismo proyecto,
// pestaña por pestaña:
//  - si solo la cambié yo, mi versión reemplaza a la de la sala;
//  - si la cambiamos los dos (o la sala la borró), la mía entra como pestaña
//    aparte, «Nombre (de Ana)», para que nadie pierda nada;
//  - si la creé yo, se agrega.
// Las mallas, el rig y la animación se toman del anfitrión, como siempre.
//
// El número y la base van en el proyecto al guardar, y además la app los anota
// por ruta de archivo: así el anfitrión que no guardó después de la sala igual
// reconoce su proyecto la próxima vez.

import { invoke } from "@tauri-apps/api/core";
import type { CadStore } from "./cad";
import { addSystemLine, collabName, onCollabRoom, sendData, status } from "./collab";
import { hash, layoutOf, type TabsMsg } from "./collabDesign";
import { canEdit } from "./collabRoles";
import { t } from "./i18n";
import { createPersisted } from "./ui-state";

/** Una pestaña entera, tal como la guarda el backend */
interface RawTab {
  id: number;
  name: string;
  kind: unknown;
}

/** Lo que va en el proyecto (`ui.json`) */
interface Saved {
  id: string;
  base: Record<string, string>;
}

let projectId: string | undefined;
let base: Record<string, string> = {};
/** Estoy con el estado de una sala (anfitrión, o invitado con el proyecto ya recibido) */
let inRoom = false;
let store: CadStore | undefined;
/** Ruta del archivo del proyecto abierto (la pone la interfaz) */
let pathOf: () => string | undefined = () => undefined;

/** Número y base de cada proyecto que pasó por una sala, por ruta de archivo */
const [registry, setRegistry] = createPersisted<Record<string, Saved & { at: number }>>("collab.projects", {});
const KEEP = 60;

function register() {
  const path = pathOf();
  if (!path || !projectId) return;
  const all = { ...(registry() ?? {}), [path]: { id: projectId, base, at: Date.now() } };
  // Solo los más recientes
  const keys = Object.keys(all).sort((a, b) => all[b].at - all[a].at);
  for (const k of keys.slice(KEEP)) delete all[k];
  setRegistry(all);
}

/** Abrí un archivo mío: si no trae número pero la app lo conoce, lo recupera */
export function adoptFromRegistry(path: string) {
  if (projectId) {
    register();
    return;
  }
  const saved = registry()?.[path];
  if (saved && typeof saved.id === "string") {
    projectId = saved.id;
    base = { ...(saved.base ?? {}) };
  }
}

/** Guardé con otra ruta: la app la anota */
export const registerPath = register;
/** Mi copia justo antes de entrar a la sala */
let mine: { id: string; base: Record<string, string>; tabs: RawTab[] } | undefined;

const tabHash = (x: RawTab) => hash(x.kind);
const uid = () => `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 9)}`;

/** Para guardar en el proyecto (nada si el proyecto nunca estuvo en una sala) */
export const collabProjectUi = (): Saved | undefined => (projectId ? { id: projectId, base } : undefined);

/** Al abrir un proyecto (o empezar uno nuevo) */
export function loadCollabProject(value: unknown) {
  const v = value as Partial<Saved> | undefined;
  projectId = v && typeof v.id === "string" ? v.id : undefined;
  base = v && v.base && typeof v.base === "object" ? { ...v.base } : {};
}

async function rawTabs(cad: CadStore): Promise<RawTab[]> {
  const ids = cad.tabs().tabs.map((x) => x.id);
  const list = await Promise.all(ids.map((id) => invoke<RawTab | null>("cad_tab_raw", { id }).catch(() => null)));
  return list.filter((x): x is RawTab => !!x && typeof x.id === "number");
}

/** Cómo están ahora mis pestañas pasa a ser «como estaban en la sala» */
async function captureBase(cad: CadStore) {
  const tabs = await rawTabs(cad);
  base = Object.fromEntries(tabs.map((x) => [String(x.id), tabHash(x)]));
  register();
}

/** Antes de guardar dentro de una sala: lo que se guarda es lo de la sala */
export async function captureIfInRoom() {
  if (inRoom && store) await captureBase(store);
}

/** Antes de entrar a una sala: me guardo mi copia para juntarla con la de la sala */
export async function rememberMine() {
  mine = projectId && store ? { id: projectId, base: { ...base }, tabs: await rawTabs(store) } : undefined;
}

/** Invitado: el proyecto de la sala ya está abierto. La primera vez, se le suma mi copia. */
export async function afterSharedOpen(notify: (message: string) => void) {
  const cad = store;
  if (!cad || !status().on) return;
  const prev = mine;
  mine = undefined;
  inRoom = true;
  try {
    if (prev && prev.id === projectId) await merge(cad, prev, notify);
  } catch (e) {
    notify(t("No se pudieron sumar tus cambios a la sala: {e}", { e: String(e) }));
  } finally {
    await captureBase(cad).catch(() => {});
  }
}

async function merge(cad: CadStore, prev: NonNullable<typeof mine>, notify: (message: string) => void) {
  const room = new Map((await rawTabs(cad)).map((x) => [x.id, x]));
  const who = collabName().trim() || t("Anónimo");
  const replace: RawTab[] = [];
  const add: RawTab[] = [];
  for (const x of prev.tabs) {
    const was = prev.base[String(x.id)];
    const now = tabHash(x);
    if (was === now) continue;
    const there = room.get(x.id);
    // Ya está igual en la sala
    if (there && tabHash(there) === now) continue;
    if (was === undefined) add.push(x);
    else if (there && tabHash(there) === was) replace.push({ ...x, name: there.name });
    else add.push({ ...x, name: t("{name} (de {who})", { name: x.name, who }) });
  }
  if (!replace.length && !add.length) return;
  if (!canEdit()) {
    notify(t("Tu copia tiene cambios, pero en esta sala no puedes editar: quedan en tu archivo"));
    return;
  }
  const r = await cad.mergeTabs(replace, add);
  const full = (await Promise.all(r.added.map((id) => invoke<RawTab | null>("cad_tab_raw", { id }).catch(() => null)))).filter((x) => x);
  await sendData("design_tabs", { layout: layoutOf(r.tabs), full, replace } satisfies TabsMsg);
  const names = [...replace.map((x) => `«${x.name}»`), ...r.added.map((id) => `«${r.tabs.tabs.find((x) => x.id === id)?.name ?? ""}»`)];
  addSystemLine(t("Se sumaron tus cambios a la sala: {list}", { list: names.join(", ") }));
  notify(t("Se sumaron tus cambios a la sala"));
}

export function installCollabMerge(cad: CadStore, path: () => string | undefined): () => void {
  store = cad;
  pathOf = path;
  const stop = onCollabRoom((on, host) => {
    if (on && host) {
      // El proyecto queda con número propio desde la primera sala
      if (!projectId) projectId = uid();
      inRoom = true;
      void captureBase(cad).catch(() => {});
    } else if (!on) {
      // Al salir, lo que quedó es lo último que se vio en la sala
      if (inRoom) void captureBase(cad).catch(() => {});
      inRoom = false;
    }
  });
  return () => {
    stop();
    store = undefined;
  };
}
