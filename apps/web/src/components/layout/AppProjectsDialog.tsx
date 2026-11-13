import { Component, For, Show, createResource, onCleanup, onMount } from "solid-js";
import { Button } from "../ui";
import * as Icons from "../icons";
import { t } from "../../lib/i18n";
import { listAppProjects, deleteAppProject, saveCopyToPhone, shareFromApp, type AppProject } from "../../lib/files";

export interface AppProjectsDialogProps {
  /** Abrir este proyecto */
  onOpen: (path: string) => void;
  /** Traer un .pinocchio desde el teléfono */
  onFromPhone: () => void;
  onClose: () => void;
  /** Pregunta antes de borrar (diálogo propio de la app) */
  confirmDelete: (name: string) => Promise<boolean>;
}

/**
 * Android: los proyectos guardados dentro de la app (en el teléfono no hay
 * rutas de archivo normales). Abrir, guardar una copia en el teléfono,
 * compartir, borrar, o traer uno de afuera.
 */
export const AppProjectsDialog: Component<AppProjectsDialogProps> = (props) => {
  const [projects, { refetch }] = createResource(listAppProjects);

  onMount(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        props.onClose();
      }
    };
    window.addEventListener("keydown", onKey, true);
    onCleanup(() => window.removeEventListener("keydown", onKey, true));
  });

  const when = (p: AppProject) =>
    new Date(p.modified * 1000).toLocaleString([], { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
  const size = (p: AppProject) => `${(p.bytes / 1e6).toFixed(1)} MB`;

  const remove = async (p: AppProject) => {
    if (!(await props.confirmDelete(p.name))) return;
    await deleteAppProject(p.path).catch(() => {});
    refetch();
  };

  return (
    <div
      class="fixed inset-0 z-50 flex items-center justify-center bg-bg-darker/70 backdrop-blur-sm p-4"
      onPointerDown={(e) => e.target === e.currentTarget && props.onClose()}
    >
      <div role="dialog" aria-label={t("Mis proyectos")} class="w-full max-w-md max-h-full flex flex-col rounded-lg border border-border bg-bg shadow-xl">
        <div class="flex items-center justify-between px-4 h-11 border-b border-border shrink-0">
          <h2 class="flex items-center gap-2 text-sm font-semibold text-text">
            <Icons.FolderOpen size={16} />
            {t("Mis proyectos")}
          </h2>
          <button
            class="w-8 h-8 flex items-center justify-center rounded text-text-muted hover:text-text hover:bg-surface/40"
            aria-label={t("Cerrar")}
            onClick={props.onClose}
          >
            <Icons.X size={14} />
          </button>
        </div>
        <div class="flex-1 overflow-y-auto p-2 min-h-24">
          <Show when={!projects.loading} fallback={<p class="p-3 text-xs text-text-muted">{t("Cargando...")}</p>}>
            <Show
              when={(projects() ?? []).length > 0}
              fallback={<p class="p-3 text-xs text-text-muted leading-relaxed">{t("Todavía no hay proyectos guardados en la app.")}</p>}
            >
              <For each={projects()}>
                {(p) => (
                  <div data-app-project={p.name} class="flex items-center gap-1 rounded hover:bg-surface/40">
                    <button class="flex-1 min-w-0 text-left px-3 py-2" onClick={() => props.onOpen(p.path)}>
                      <div class="text-sm text-text truncate">{p.name}</div>
                      <div class="text-xs text-text-dim">
                        {when(p)} · {size(p)}
                      </div>
                    </button>
                    <button
                      class="w-9 h-9 flex items-center justify-center rounded text-text-muted hover:text-text"
                      title={t("Guardar una copia en el teléfono")}
                      aria-label={t("Guardar una copia en el teléfono")}
                      onClick={() => void saveCopyToPhone(p.path, t("Guardar una copia en el teléfono"))}
                    >
                      <Icons.Export size={16} />
                    </button>
                    <button
                      class="w-9 h-9 flex items-center justify-center rounded text-text-muted hover:text-text"
                      title={t("Compartir")}
                      aria-label={t("Compartir")}
                      onClick={() => void shareFromApp(p.path, t("Compartir proyecto"))}
                    >
                      <Icons.ArrowsOut size={16} />
                    </button>
                    <button
                      class="w-9 h-9 flex items-center justify-center rounded text-text-muted hover:text-red"
                      title={t("Borrar")}
                      aria-label={t("Borrar")}
                      onClick={() => void remove(p)}
                    >
                      <Icons.Trash size={16} />
                    </button>
                  </div>
                )}
              </For>
            </Show>
          </Show>
        </div>
        <div class="flex justify-between gap-2 px-4 py-3 border-t border-border shrink-0">
          <Button size="sm" variant="ghost" onClick={props.onFromPhone}>
            {t("Traer desde el teléfono…")}
          </Button>
          <Button size="sm" variant="ghost" onClick={props.onClose}>
            {t("Cancelar")}
          </Button>
        </div>
      </div>
    </div>
  );
};
