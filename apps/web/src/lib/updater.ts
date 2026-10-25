/**
 * Actualizaciones desde las releases de GitHub (`latest.json` firmado, ver
 * .github/workflows/release.yml). Al abrir la app se busca una versión nueva;
 * con «Descargar sola» se baja en segundo plano y la ventana solo pregunta
 * cuándo reiniciar. En Linux solo la AppImage se reemplaza sola: el binario
 * suelto ofrece la página de descarga.
 */

import { createSignal } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { createPersisted } from "./ui-state.ts";

export const RELEASES_URL = "https://github.com/Debaq/Pinocchiors/releases/latest";

export interface UpdateSettings {
  /** Buscar al abrir la app */
  check: boolean;
  /** Bajarla en segundo plano apenas aparece */
  auto: boolean;
  /** Versión que el usuario pidió no volver a ofrecer */
  skipped?: string;
}
export const [updateSettings, setUpdateSettings] = createPersisted<UpdateSettings>("settings.updates", { check: true, auto: true });

export type UpdatePhase =
  /** Hay una versión nueva (sin bajar) */
  | "available"
  | "downloading"
  /** Bajada y verificada: falta instalar y reiniciar */
  | "ready"
  | "installing"
  /** Búsqueda a mano sin novedades */
  | "latest"
  | "error";

export interface UpdateState {
  phase: UpdatePhase;
  current: string;
  version?: string;
  notes?: string;
  date?: string;
  /** Se instala sola (si no, se descarga desde la página de la release) */
  canInstall: boolean;
  /** 0–1 mientras baja; sin tamaño conocido, indefinido */
  progress?: number;
  error?: string;
  /** El diálogo está a la vista (bajando sola en segundo plano, no) */
  shown: boolean;
}

const [state, setState] = createSignal<UpdateState | null>(null);
export const updateState = state;

let pending: Update | null = null;
let busy = false;
/** Lo que pregunta la app antes de reiniciar (cambios sin guardar); `false` = no reiniciar */
let beforeRestart: () => Promise<boolean> = async () => true;
export const setBeforeRestart = (fn: () => Promise<boolean>) => (beforeRestart = fn);

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

/**
 * Busca una versión nueva. `manual`: desde Configuración (muestra el diálogo
 * aunque no haya nada y no respeta la versión omitida)
 */
export async function checkForUpdate(manual = false): Promise<void> {
  if (busy) {
    if (manual && state()) setState({ ...state()!, shown: true });
    return;
  }
  busy = true;
  let current = "";
  try {
    current = await getVersion();
    const update = await check();
    if (!update) {
      if (manual) setState({ phase: "latest", current, canInstall: true, shown: true });
      return;
    }
    if (!manual && update.version === updateSettings().skipped) return;
    pending = update;
    const canInstall = await invoke<boolean>("update_can_install").catch(() => false);
    const base = { current, version: update.version, notes: update.body, date: update.date, canInstall };
    if (canInstall && updateSettings().auto && !manual) {
      // En segundo plano: el diálogo aparece cuando ya está lista
      setState({ ...base, phase: "downloading", shown: false });
      await download(update);
    } else {
      setState({ ...base, phase: "available", shown: true });
    }
  } catch (e) {
    // Al abrir, sin red o sin release para esta plataforma: en silencio
    if (manual) setState({ phase: "error", current, canInstall: false, error: message(e), shown: true });
    else setState(null);
  } finally {
    busy = false;
  }
}

async function download(update: Update) {
  let total = 0;
  let got = 0;
  await update.download((event) => {
    if (event.event === "Started") total = event.data.contentLength ?? 0;
    else if (event.event === "Progress") {
      got += event.data.chunkLength;
      setState((s) => s && { ...s, progress: total ? got / total : undefined });
    }
  });
  setState((s) => s && { ...s, phase: "ready", progress: 1, shown: true });
}

/** «Actualizar»: baja (si falta) y deja lista para reiniciar */
export async function startDownload(): Promise<void> {
  const s = state();
  if (!pending || !s) return;
  if (!s.canInstall) {
    await openUrl(RELEASES_URL).catch(() => {});
    return closeUpdate();
  }
  busy = true;
  setState({ ...s, phase: "downloading", progress: undefined, shown: true });
  try {
    await download(pending);
  } catch (e) {
    setState((x) => x && { ...x, phase: "error", error: message(e) });
  } finally {
    busy = false;
  }
}

/** Instala lo bajado y abre la versión nueva (en Windows el instalador cierra la app solo) */
export async function installAndRestart(): Promise<void> {
  const s = state();
  if (!pending || !s || s.phase !== "ready") return;
  setState({ ...s, shown: false });
  if (!(await beforeRestart())) {
    setState({ ...s, shown: true });
    return;
  }
  setState({ ...s, phase: "installing", shown: true });
  try {
    await pending.install();
    await relaunch();
  } catch (e) {
    setState((x) => x && { ...x, phase: "error", error: message(e) });
  }
}

/** No volver a ofrecer esta versión al abrir (sí la siguiente) */
export function skipVersion(): void {
  const v = state()?.version;
  if (v) setUpdateSettings({ ...updateSettings(), skipped: v });
  closeUpdate();
}

/** «Más tarde»: lo bajado queda para reiniciar desde Configuración en esta sesión */
export function closeUpdate(): void {
  const s = state();
  if (!s) return;
  if (s.phase === "ready" || s.phase === "downloading") setState({ ...s, shown: false });
  else {
    setState(null);
    pending = null;
  }
}

/** Muestra de nuevo el diálogo (una versión lista o bajando) */
export const showUpdate = () => state() && setState({ ...state()!, shown: true });
