import { Component, For, JSX, Show, createSignal } from "solid-js";
import { Button } from "../ui";
import { t } from "../../lib/i18n";
import { tBackend } from "../../lib/i18nBackend";
import {
  chat,
  collabColor,
  collabName,
  COLORS,
  hostCollab,
  joinCollab,
  leaveCollab,
  peers,
  sendChat,
  setCollabColor,
  setCollabName,
  status,
} from "../../lib/collab";

const Section = (props: { title: string; children: JSX.Element }) => (
  <section class="space-y-3 pb-4 mb-4 border-b border-border last:border-0 last:mb-0 last:pb-0">
    <h3 class="text-[11px] uppercase tracking-wide text-text-muted">{props.title}</h3>
    {props.children}
  </section>
);

const Hint = (props: { children: JSX.Element }) => <p class="text-xs text-text-muted leading-relaxed">{props.children}</p>;

const inputClass = "w-full px-2 py-1 rounded border border-border bg-surface/40 text-sm text-text outline-none focus:border-accent";

/**
 * Configuración ▸ Colaborar: crear una sala o unirse a la de otra persona.
 * Conexión directa entre las dos apps (P2P, cifrada). El chat de acá es el de
 * prueba de la conexión; el definitivo va en un panel izquierdo.
 */
export const CollabSettings: Component = () => {
  const [ticket, setTicket] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [message, setMessage] = createSignal<{ text: string; error?: boolean }>();
  const [line, setLine] = createSignal("");

  const run = async (job: () => Promise<void>) => {
    setBusy(true);
    setMessage(undefined);
    try {
      await job();
    } catch (e) {
      setMessage({ text: tBackend(String(e)), error: true });
    } finally {
      setBusy(false);
    }
  };

  const copy = async () => {
    const tk = status().ticket;
    if (!tk) return;
    try {
      await navigator.clipboard.writeText(tk);
      setMessage({ text: t("Ticket copiado. Pásaselo a la otra persona.") });
    } catch {
      setMessage({ text: t("No se pudo copiar; selecciónalo a mano."), error: true });
    }
  };

  const send = () => {
    const text = line();
    setLine("");
    void run(() => sendChat(text));
  };

  return (
    <div class="space-y-1">
      <Section title={t("Tu nombre")}>
        <input
          class={inputClass}
          value={collabName()}
          placeholder={t("Cómo te ven los demás")}
          disabled={status().on}
          spellcheck={false}
          onInput={(e) => setCollabName(e.currentTarget.value)}
        />
        <div class="flex gap-2">
          <For each={COLORS}>
            {(c) => (
              <button
                class="w-6 h-6 rounded-full border-2 disabled:opacity-50"
                style={{ background: c, "border-color": collabColor() === c ? "var(--color-text, #fff)" : "transparent" }}
                disabled={status().on}
                aria-label={c}
                onClick={() => setCollabColor(c)}
              />
            )}
          </For>
        </div>
      </Section>

      <Show
        when={status().on}
        fallback={
          <Section title={t("Colaborar")}>
            <Hint>
              {t(
                "Trabaja en el mismo proyecto con otra persona. Una crea la sala y comparte el ticket; la otra lo pega acá. La conexión es directa entre las dos apps y va cifrada.",
              )}
            </Hint>
            <Button variant="primary" disabled={busy()} onClick={() => void run(hostCollab)}>
              {t("Crear una sala")}
            </Button>
            <div class="space-y-2">
              <input
                class={`${inputClass} font-mono`}
                value={ticket()}
                placeholder={t("Pega acá el ticket de la sala")}
                spellcheck={false}
                onInput={(e) => setTicket(e.currentTarget.value)}
              />
              <Button disabled={busy() || !ticket().trim()} onClick={() => void run(() => joinCollab(ticket()))}>
                {t("Unirme")}
              </Button>
            </div>
          </Section>
        }
      >
        <Section title={status().host ? t("Eres el anfitrión") : t("Estás en una sala")}>
          <Show when={status().host}>
            <Hint>{t("Comparte este ticket. Solo quien lo tenga puede entrar.")}</Hint>
            <textarea
              class={`${inputClass} font-mono text-xs h-20 resize-none`}
              readOnly
              value={status().ticket ?? ""}
              onFocus={(e) => e.currentTarget.select()}
            />
            <Button onClick={() => void copy()}>{t("Copiar el ticket")}</Button>
          </Show>
          <ul class="space-y-1">
            <For each={peers()} fallback={<li class="text-xs text-text-muted">{t("Esperando a alguien…")}</li>}>
              {(p) => (
                <li class="flex items-center gap-2 text-sm text-text">
                  <span class="w-2.5 h-2.5 rounded-full" style={{ background: p.color }} />
                  {p.name}
                </li>
              )}
            </For>
          </ul>
          <Button disabled={busy()} onClick={() => void run(leaveCollab)}>
            {status().host ? t("Cerrar la sala") : t("Salir de la sala")}
          </Button>
        </Section>

        <Section title={t("Chat")}>
          <div class="max-h-40 overflow-y-auto space-y-1 text-sm">
            <For each={chat()}>
              {(c) => (
                <div class={c.mine ? "text-right text-accent" : "text-text"}>
                  <span class="text-text-muted text-xs">{c.mine ? "" : `${c.name}: `}</span>
                  {c.text}
                </div>
              )}
            </For>
          </div>
          <input
            class={inputClass}
            value={line()}
            placeholder={t("Escribe y presiona Enter")}
            onInput={(e) => setLine(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") send();
            }}
          />
        </Section>
      </Show>

      <Show when={message()}>{(m) => <p class={`text-xs ${m().error ? "text-red-400" : "text-text-muted"}`}>{m().text}</p>}</Show>
    </div>
  );
};
