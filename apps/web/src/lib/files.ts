/**
 * Abrir y guardar archivos con el diálogo del sistema, también en Android.
 *
 * Mismas funciones que `@tauri-apps/plugin-dialog` (`open`, `save`): en
 * escritorio son las de siempre. En Android el selector entrega `content://…`,
 * que el backend no puede abrir, así que:
 * - `open` copia cada archivo elegido dentro de la app y devuelve la ruta local
 *   (sin filtrar por tipo en el selector: Android filtra por MIME y `.stl`,
 *   `.step` o `.pinocchio` no tienen uno, así que no se verían; el tipo se revisa
 *   al copiar);
 * - `save` pregunta «Guardar en el teléfono…» o «Compartir» y devuelve una ruta
 *   local de paso: cuando la app termina de escribir ahí, el backend lo copia al
 *   destino elegido o abre el menú de compartir de Android, y avisa
 *   (`onAndroidExport`).
 * Carpetas (cuadros de un render, pruebas de exposición): en Android quedan en
 * una carpeta de la app.
 */
import {
  open as dialogOpen,
  save as dialogSave,
  type OpenDialogOptions,
  type OpenDialogReturn,
  type SaveDialogOptions,
} from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { IS_ANDROID } from "./platform";
import { t } from "./i18n.ts";
import { tBackend } from "./i18nBackend";

export type { OpenDialogOptions, SaveDialogOptions };

/** Quién muestra los avisos (la barra de estado de la app) */
let notify: (message: string) => void = (message) => console.warn(message);
export function setFileNotifier(fn: (message: string) => void) {
  notify = fn;
}

/** Android: guardar en el teléfono o compartir (diálogo propio de la app) */
export type ExportChoice = "save" | "share" | null;
let chooseExport: (name: string) => Promise<ExportChoice> = async () => "save";
export function setExportChooser(fn: (name: string) => Promise<ExportChoice>) {
  chooseExport = fn;
}

let batches = 0;

/** Como `open` del diálogo; en Android trae copias de lo elegido */
export async function open<T extends OpenDialogOptions>(options?: T): Promise<OpenDialogReturn<T>> {
  if (!IS_ANDROID) return dialogOpen(options);
  const result = (value: string[] | null) =>
    (value === null || value.length === 0 ? null : options?.multiple ? value : value[0]) as OpenDialogReturn<T>;
  try {
    if (options?.directory) {
      const dir = await invoke<string>("android_output_dir", { name: options.title ?? t("Carpeta") });
      notify(t("En el teléfono los archivos quedan dentro de la app"));
      return result([dir]);
    }
    const allowed = [...new Set((options?.filters ?? []).flatMap((f) => f.extensions).map((e) => e.toLowerCase()))].filter(
      (e) => e !== "*"
    );
    const picked = await dialogOpen({ multiple: options?.multiple ?? false, title: options?.title });
    if (!picked) return result(null);
    const uris = Array.isArray(picked) ? picked : [picked];
    // Una carpeta por elección: un OBJ encuentra su .mtl y sus texturas al lado
    const batch = `${Date.now()}-${batches++}`;
    const paths: string[] = [];
    for (const uri of uris) {
      paths.push(uri.startsWith("content://") ? await invoke<string>("android_import_uri", { uri, batch, allowed }) : uri);
    }
    return result(paths);
  } catch (e) {
    notify(t("No se pudo abrir el archivo: {e}", { e: tBackend(String(e)) }));
    return result(null);
  }
}

/** Nombre para «Guardar en…»: el sugerido, con la extensión del primer filtro si le falta */
export function exportName(options?: SaveDialogOptions): string {
  const base = (options?.defaultPath ?? "").split(/[\\/]/).pop() ?? "";
  const extensions = (options?.filters ?? []).flatMap((f) => f.extensions).filter((e) => e !== "*");
  const first = extensions[0];
  let name = base || t("archivo");
  const lower = name.toLowerCase();
  if (first && !extensions.some((e) => lower.endsWith(`.${e.toLowerCase()}`))) name = `${name}.${first}`;
  return name;
}

/** Como `save` del diálogo; en Android, «Guardar en…» del teléfono o «Compartir» */
export async function save(options?: SaveDialogOptions): Promise<string | null> {
  if (!IS_ANDROID) return dialogSave(options);
  try {
    const name = exportName(options);
    const choice = await chooseExport(name);
    if (!choice) return null;
    if (choice === "share") {
      return await invoke<string>("android_export_begin", { uri: null, name, shareTitle: options?.title ?? name });
    }
    // Sin filtros: con un tipo que Android no conoce el selector no deja guardar
    const uri = await dialogSave({ title: options?.title, defaultPath: name });
    if (!uri) return null;
    if (!uri.startsWith("content://")) return uri;
    return await invoke<string>("android_export_begin", { uri, name, shareTitle: null });
  } catch (e) {
    notify(t("No se pudo guardar el archivo: {e}", { e: tBackend(String(e)) }));
    return null;
  }
}

/** Copia un archivo de la app (un proyecto guardado) a «Guardar en…» del teléfono */
export async function saveCopyToPhone(path: string, title: string): Promise<boolean> {
  const name = path.split(/[\\/]/).pop() ?? "archivo";
  try {
    const uri = await dialogSave({ title, defaultPath: name });
    if (!uri) return false;
    await invoke("android_copy_to_uri", { path, uri });
    notify(t("Guardado en el teléfono: {name}", { name }));
    return true;
  } catch (e) {
    notify(t("No se pudo guardar el archivo: {e}", { e: tBackend(String(e)) }));
    return false;
  }
}

/** Abre el menú de compartir de Android con un archivo de la app (un proyecto guardado) */
export async function shareFromApp(path: string, title: string): Promise<boolean> {
  try {
    await invoke("android_share", { path, title });
    return true;
  } catch (e) {
    notify(t("No se pudo compartir: {e}", { e: tBackend(String(e)) }));
    return false;
  }
}

/** Avisa cuando un archivo exportado ya llegó a su lugar en el teléfono */
export async function onAndroidExport(): Promise<() => void> {
  if (!IS_ANDROID) return () => {};
  return listen<{ name: string; ok: boolean; shared: boolean; error?: string }>("android-export", ({ payload }) => {
    const e = tBackend(payload.error ?? "");
    notify(
      payload.shared
        ? payload.ok
          ? t("Listo para compartir: {name}", { name: payload.name })
          : t("No se pudo compartir: {e}", { e })
        : payload.ok
          ? t("Guardado en el teléfono: {name}", { name: payload.name })
          : t("No se pudo guardar {name} en el teléfono: {e}", { name: payload.name, e })
    );
  });
}

/** Un proyecto guardado dentro de la app (Android) */
export interface AppProject {
  path: string;
  name: string;
  /** Segundos desde 1970 */
  modified: number;
  bytes: number;
}

export const listAppProjects = () => invoke<AppProject[]>("android_projects_list");
export const appProjectPath = (name: string) => invoke<string>("android_project_path", { name });
export const deleteAppProject = (path: string) => invoke<void>("android_project_delete", { path });
