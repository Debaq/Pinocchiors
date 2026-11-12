import { Component, For, Show } from "solid-js";
import { t } from "../../lib/i18n";
import { chatUnread, collabPanelOpen, peers, setCollabPanelOpen, status } from "../../lib/collab";
import { threads } from "../../lib/collabComments";
import { followId, toggleFollow } from "../../lib/collabPresence";

/**
 * En la cabecera: abre el panel de colaboración (chat y comentarios) y, con
 * sala, muestra quién está (un punto de color por persona). Un clic en el
 * punto hace que tu cámara siga a esa persona; otro clic (o tocar el visor)
 * la suelta.
 */
export const CollabBadge: Component = () => {
  const open = () => threads().filter((x) => !x.resolved).length;
  return (
    <div class="h-7 flex items-center gap-1.5 px-2 rounded-md border border-border text-xs" classList={{ "bg-accent/15": collabPanelOpen() }}>
      <button class="text-text hover:text-accent" title={t("Chat y comentarios")} aria-pressed={collabPanelOpen()} onClick={() => setCollabPanelOpen(!collabPanelOpen())}>
        {status().on ? (status().host ? t("Sala") : t("En sala")) : t("Comentarios")}
        <Show when={chatUnread() > 0}>
          <span class="ml-1 px-1 rounded-full bg-accent text-white">{chatUnread()}</span>
        </Show>
        <Show when={chatUnread() === 0 && open() > 0}>
          <span class="ml-1 text-text-muted">{open()}</span>
        </Show>
      </button>
      <Show when={status().on}>
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
      </Show>
    </div>
  );
};
