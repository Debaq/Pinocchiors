import { Component, For, Show } from "solid-js";
import { t } from "../../lib/i18n";
import { peers, status } from "../../lib/collab";
import { followId, toggleFollow } from "../../lib/collabPresence";

/**
 * En la cabecera: quién está en la sala (un punto de color por persona). Un
 * clic en el punto hace que tu cámara siga a esa persona; otro clic (o tocar
 * el visor) la suelta.
 */
export const CollabBadge: Component = () => (
  <Show when={status().on}>
    <div class="h-7 flex items-center gap-1.5 px-2 rounded-md border border-border text-xs" role="status" title={t("Sala de colaboración")}>
      <span class="text-text">{status().host ? t("Sala") : t("En sala")}</span>
      <For each={peers()} fallback={<span class="text-text-muted">{t("esperando…")}</span>}>
        {(p) => (
          <button
            class="w-3 h-3 rounded-full outline-offset-2"
            style={{ background: p.color, outline: followId() === p.id ? `2px solid ${p.color}` : "none" }}
            title={followId() === p.id ? t("Dejar de seguir a {name}", { name: p.name }) : t("Seguir la cámara de {name}", { name: p.name })}
            aria-pressed={followId() === p.id}
            onClick={() => toggleFollow(p.id)}
          />
        )}
      </For>
      <Show when={followId()}>
        <span class="text-text-muted">{t("siguiendo")}</span>
      </Show>
    </div>
  </Show>
);
