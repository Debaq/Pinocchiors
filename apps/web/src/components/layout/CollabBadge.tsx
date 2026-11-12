import { Component, For, Show } from "solid-js";
import { t } from "../../lib/i18n";
import { peers, status } from "../../lib/collab";

/** En la cabecera: quién está en la sala (un punto de color por persona) */
export const CollabBadge: Component = () => (
  <Show when={status().on}>
    <div class="h-7 flex items-center gap-1.5 px-2 rounded-md border border-border text-xs" role="status" title={t("Sala de colaboración")}>
      <span class="text-text">{status().host ? t("Sala") : t("En sala")}</span>
      <For each={peers()} fallback={<span class="text-text-muted">{t("esperando…")}</span>}>
        {(p) => <span class="w-2.5 h-2.5 rounded-full" style={{ background: p.color }} title={p.name} />}
      </For>
    </div>
  </Show>
);
