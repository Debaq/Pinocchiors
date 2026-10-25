import { Component, Match, Show, Switch, onCleanup, onMount } from "solid-js";
import { Button, ProgressBar } from "../ui";
import * as Icons from "../icons";
import { Markdown } from "./Markdown";
import { t, tx } from "../../lib/i18n";
import { tBackend } from "../../lib/i18nBackend";
import { closeUpdate, installAndRestart, skipVersion, startDownload, updateState } from "../../lib/updater";

/** Versión nueva: novedades, avance de la descarga y reiniciar (mismo estilo que ConfirmDialog) */
export const UpdateDialog: Component = () => {
  const s = () => updateState()!;
  const busy = () => s().phase === "downloading" || s().phase === "installing";

  onMount(() => {
    // En captura: que Esc y Enter no lleguen a los atajos del visor
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        if (!busy() || s().phase === "downloading") closeUpdate();
      } else if (e.key === "Enter") {
        e.preventDefault();
        e.stopPropagation();
        if (s().phase === "available") void startDownload();
        else if (s().phase === "ready") void installAndRestart();
        else if (!busy()) closeUpdate();
      } else if (e.key !== "Tab") {
        e.stopPropagation();
      }
    };
    window.addEventListener("keydown", onKey, true);
    onCleanup(() => window.removeEventListener("keydown", onKey, true));
  });

  const title = () =>
    s().phase === "latest"
      ? t("Pinocchio está al día")
      : s().phase === "error"
        ? t("No se pudo actualizar")
        : s().phase === "ready"
          ? t("Versión nueva lista")
          : t("Hay una versión nueva");

  return (
    <div
      class="fixed inset-0 z-50 flex items-center justify-center bg-bg-darker/70 backdrop-blur-sm p-4"
      onPointerDown={(e) => e.target === e.currentTarget && !busy() && closeUpdate()}
    >
      <div role="dialog" aria-label={title()} data-update-dialog class="w-full max-w-md rounded-lg border border-border bg-bg shadow-xl">
        <div class="flex items-center justify-between px-4 h-11 border-b border-border">
          <h2 class="flex items-center gap-2 text-sm font-semibold text-text">
            <Switch fallback={<Icons.Download size={16} class="text-accent" />}>
              <Match when={s().phase === "error"}>
                <Icons.Warning size={16} class="text-warning" />
              </Match>
              <Match when={s().phase === "latest"}>
                <Icons.Check size={16} class="text-success" />
              </Match>
            </Switch>
            {title()}
          </h2>
          <Show when={!busy() || s().phase === "downloading"}>
            <button
              class="w-7 h-7 flex items-center justify-center rounded text-text-muted hover:text-text hover:bg-surface/40"
              aria-label={t("Cerrar")}
              onClick={closeUpdate}
            >
              <Icons.X size={14} />
            </button>
          </Show>
        </div>

        <div class="p-4 space-y-3 text-xs text-text-muted leading-relaxed">
          <Switch>
            <Match when={s().phase === "latest"}>
              <p>{t("Tienes la versión más reciente ({version}).", { version: s().current })}</p>
            </Match>
            <Match when={s().phase === "error"}>
              <p class="whitespace-pre-line">{tBackend(s().error ?? "")}</p>
            </Match>
            <Match when={true}>
              <p>
                {t("Versión instalada")} <span class="font-mono text-text">{s().current}</span> → {tx("actualizar", "nueva")}{" "}
                <span class="font-mono text-text">{s().version}</span>
              </p>
              <Show when={s().notes?.trim()}>
                <div class="max-h-56 overflow-y-auto rounded-md border border-border bg-bg-darker p-3 text-text-muted select-text">
                  <Markdown text={s().notes!} />
                </div>
              </Show>
              <Show when={s().phase === "downloading"}>
                <ProgressBar value={s().progress === undefined ? undefined : Math.round(s().progress! * 100)} label={t("Descargando…")} showValue size="sm" />
                <p class="text-text-dim">{t("Puedes cerrar esta ventana y seguir trabajando: avisa cuando esté lista.")}</p>
              </Show>
              <Show when={s().phase === "ready"}>
                <p>{t("Ya está descargada y verificada. Al reiniciar se instala y Pinocchio se vuelve a abrir.")}</p>
              </Show>
              <Show when={s().phase === "installing"}>
                <ProgressBar label={t("Instalando…")} size="sm" />
              </Show>
              <Show when={s().phase === "available" && !s().canInstall}>
                <p class="text-text-dim">{t("Esta copia no se puede reemplazar sola (solo la AppImage en Linux): se abre la página de descarga.")}</p>
              </Show>
            </Match>
          </Switch>
        </div>

        <div class="flex justify-end gap-2 px-4 pb-4">
          <Switch>
            <Match when={s().phase === "available"}>
              <Button size="sm" variant="ghost" onClick={skipVersion}>
                {t("Omitir esta versión")}
              </Button>
              <Button size="sm" variant="ghost" onClick={closeUpdate}>
                {t("Más tarde")}
              </Button>
              <Button size="sm" variant="primary" onClick={() => void startDownload()}>
                {s().canInstall ? t("Actualizar") : t("Ir a la descarga")}
              </Button>
            </Match>
            <Match when={s().phase === "downloading"}>
              <Button size="sm" variant="ghost" onClick={closeUpdate}>
                {t("Seguir en segundo plano")}
              </Button>
            </Match>
            <Match when={s().phase === "ready"}>
              <Button size="sm" variant="ghost" onClick={closeUpdate}>
                {t("Más tarde")}
              </Button>
              <Button size="sm" variant="primary" onClick={() => void installAndRestart()}>
                {t("Reiniciar y actualizar")}
              </Button>
            </Match>
            <Match when={s().phase === "latest" || s().phase === "error"}>
              <Button size="sm" variant="primary" onClick={closeUpdate}>
                {t("Aceptar")}
              </Button>
            </Match>
          </Switch>
        </div>
      </div>
    </div>
  );
};
