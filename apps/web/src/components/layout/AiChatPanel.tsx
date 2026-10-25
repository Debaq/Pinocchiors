import { Component, For, Match, Show, Switch, createSignal, onCleanup, onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import * as Icons from "../icons";
import { IconButton, Select } from "../ui";
import { t } from "../../lib/i18n";
import { tBackend } from "../../lib/i18nBackend";
import { aiLabel, refreshLive } from "../../lib/mcpLive";
import { createPersisted, startDrag } from "../../lib/ui-state";

/** Una entrada del chat */
type Entry =
  | { kind: "user"; text: string }
  | { kind: "text"; text: string }
  | { kind: "tool"; id: string; name: string; state: "running" | "ok" | "error"; detail?: string }
  | { kind: "image"; data: string; mime: string }
  | { kind: "error"; text: string }
  /** Con qué modelo respondió y cuánto costó (lo que informa Claude Code) */
  | { kind: "info"; text: string };

interface ChatStatus {
  claude: string | null;
  busy: boolean;
  ongoing: boolean;
}

// La conversación sigue aunque se cierre el panel
const [entries, setEntries] = createSignal<Entry[]>([]);
const [busy, setBusy] = createSignal(false);
/** Modelo y esfuerzo de Claude Code ("" = los suyos por defecto); se recuerdan */
const [model, setModel] = createPersisted("chat.model", "");
const [effort, setEffort] = createPersisted("chat.effort", "");
/** Ancho del panel (se arrastra su borde derecho) */
const [width, setWidth] = createPersisted("chat.width", 360);
const MIN_WIDTH = 280;
const MAX_WIDTH = 640;
/** Modelo que está respondiendo (lo dice Claude Code al empezar) */
let answering = "";

const MODEL_OPTIONS = () => [
  { value: "", label: t("El de Claude Code") },
  { value: "fable", label: "Fable" },
  { value: "opus", label: "Opus" },
  { value: "sonnet", label: "Sonnet" },
  { value: "haiku", label: "Haiku" },
];
const EFFORT_OPTIONS = () => [
  { value: "", label: t("El de Claude Code") },
  { value: "low", label: t("Bajo (rápido)") },
  { value: "medium", label: t("Medio") },
  { value: "high", label: t("Alto (piensa más)") },
  { value: "xhigh", label: t("Muy alto") },
  { value: "max", label: t("Máximo (lento)") },
];

/** Mensajes de Claude Code (`mcp::chat::translate`) → entradas del chat */
function onEvent(m: { kind: string; [k: string]: unknown }) {
  const add = (e: Entry) => setEntries((list) => [...list, e]);
  switch (m.kind) {
    case "start":
      answering = typeof m.model === "string" ? m.model : "";
      return;
    case "text":
      return add({ kind: "text", text: String(m.text) });
    case "tool":
      return add({ kind: "tool", id: String(m.id), name: String(m.name), state: "running" });
    case "result":
      return setEntries((list) =>
        list.map((e) => (e.kind === "tool" && e.id === m.id ? { ...e, state: m.error ? "error" : "ok", detail: m.error ? String(m.text) : undefined } : e)),
      );
    case "image":
      return add({ kind: "image", data: String(m.data), mime: String(m.mime ?? "image/png") });
    case "done":
      setBusy(false);
      // Lo que quedó «en curso» terminó con el turno
      setEntries((list) => list.map((e) => (e.kind === "tool" && e.state === "running" ? { ...e, state: m.ok ? "ok" : "error" } : e)));
      if (!m.ok && m.error) add({ kind: "error", text: tBackend(String(m.error)) });
      if (answering || typeof m.cost === "number") {
        const cost = typeof m.cost === "number" ? ` · ${t("costo estimado")} USD ${m.cost.toFixed(3)}` : "";
        add({ kind: "info", text: `${answering}${cost}` });
      }
      return;
  }
}

/**
 * Chat con la IA dentro de Pinocchio, acoplado a la izquierda del visor (no
 * tapa nada; el visor se angosta). Lo que escribes va al Claude Code
 * instalado, que trabaja en esta ventana con el conector (modo en vivo).
 */
export const AiChatPanel: Component<{ onClose: () => void }> = (props) => {
  const [status, setStatus] = createSignal<ChatStatus>();
  const [draft, setDraft] = createSignal("");
  const [options, setOptions] = createSignal(false);
  let list: HTMLDivElement | undefined;
  let input: HTMLTextAreaElement | undefined;

  const scroll = () => queueMicrotask(() => list?.scrollTo({ top: list.scrollHeight }));

  onMount(() => {
    void invoke<ChatStatus>("ai_chat_status").then((s) => {
      setStatus(s);
      setBusy(s.busy);
    });
    const stop = listen<{ kind: string }>("ai-chat://event", (e) => {
      onEvent(e.payload);
      scroll();
    });
    onCleanup(() => void stop.then((f) => f()));
    input?.focus();
    scroll();
  });

  const send = async () => {
    const text = draft().trim();
    if (!text || busy()) return;
    setDraft("");
    setEntries((l) => [...l, { kind: "user", text }]);
    setBusy(true);
    scroll();
    try {
      await invoke("ai_chat_send", { message: text, model: model() || null, effort: effort() || null });
      void refreshLive();
    } catch (e) {
      setBusy(false);
      setEntries((l) => [...l, { kind: "error", text: tBackend(String(e)) }]);
    }
  };

  const reset = () => {
    void invoke("ai_chat_reset");
    setEntries([]);
    setBusy(false);
  };

  return (
    <aside class="relative flex flex-col shrink-0 bg-bg-darker border-r border-border overflow-hidden" style={{ width: `${width()}px` }} aria-label={t("Chat con IA")}>
      {/* Borde para cambiar el ancho */}
      <div
        class="absolute inset-y-0 right-0 w-1 z-10 cursor-col-resize hover:bg-accent/50 transition-colors"
        onPointerDown={(e) => {
          const start = width();
          startDrag(e, "col-resize", (dx) => setWidth(Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, start + dx))));
        }}
      />
      <div class="flex items-center gap-2 px-3 h-8 shrink-0 border-b border-border">
        <Icons.MagicWand size={14} />
        <span class="flex-1 text-sm font-semibold text-text">{t("Chat con IA")}</span>
        <IconButton variant={options() ? "primary" : "ghost"} size="sm" aria-label={t("Modelo y esfuerzo")} title={t("Modelo y esfuerzo")} onClick={() => setOptions(!options())}>
          <Icons.Sliders size={14} />
        </IconButton>
        <IconButton variant="ghost" size="sm" aria-label={t("Conversación nueva")} title={t("Conversación nueva")} onClick={reset} disabled={busy()}>
          <Icons.FilePlus size={14} />
        </IconButton>
        <IconButton variant="ghost" size="sm" aria-label={t("Cerrar")} onClick={() => props.onClose()}>
          <Icons.X size={14} />
        </IconButton>
      </div>

      <Show when={options()}>
        <div class="grid grid-cols-2 gap-2 px-3 py-2 border-b border-border">
          <Select label={t("Modelo")} options={MODEL_OPTIONS()} value={model()} onChange={setModel} />
          <Select label={t("Esfuerzo")} options={EFFORT_OPTIONS()} value={effort()} onChange={setEffort} />
          <p class="col-span-2 text-[11px] text-text-muted leading-relaxed">
            {t("Más esfuerzo piensa más antes de actuar: mejor para piezas complejas, más lento y gasta más de tu cuota. Vale desde el próximo mensaje.")}
          </p>
        </div>
      </Show>

      <div ref={list} class="flex-1 overflow-y-auto px-3 py-3 space-y-2 text-sm">
        <Show when={status() && !status()!.claude}>
          <p class="text-xs text-warning leading-relaxed">
            {t("No se encontró Claude Code en este equipo. Instálalo (claude.com/claude-code) e inicia sesión una vez en la terminal; el chat usa tu cuenta.")}
          </p>
        </Show>
        <Show when={entries().length === 0 && status()?.claude}>
          <p class="text-xs text-text-muted leading-relaxed">
            {t(
              "Pídele lo que quieras diseñar: «haz una caja de 80 × 50 × 30 con tapa», «redondea las aristas que seleccioné», «arma un ensamble con estas piezas». Trabaja en esta ventana y cada cambio se deshace con Ctrl+Z.",
            )}
          </p>
        </Show>
        <For each={entries()}>
          {(e) => (
            <Switch>
              <Match when={e.kind === "user" && e}>
                {(u) => <div class="ml-8 rounded-lg bg-accent/15 border border-accent/30 px-3 py-2 text-text whitespace-pre-wrap">{u().text}</div>}
              </Match>
              <Match when={e.kind === "text" && e}>
                {(a) => <div class="mr-6 text-text whitespace-pre-wrap leading-relaxed">{a().text}</div>}
              </Match>
              <Match when={e.kind === "tool" && e}>
                {(tool) => (
                  <div class="flex items-center gap-2 text-xs text-text-muted" title={tool().detail}>
                    <span
                      class="w-1.5 h-1.5 rounded-full"
                      classList={{ "bg-accent animate-pulse": tool().state === "running", "bg-success": tool().state === "ok", "bg-error": tool().state === "error" }}
                    />
                    <span classList={{ "text-error": tool().state === "error" }}>{aiLabel(tool().name)}</span>
                    <Show when={tool().state === "error"}>
                      <span class="truncate">— {t("no se pudo")}</span>
                    </Show>
                  </div>
                )}
              </Match>
              <Match when={e.kind === "image" && e}>
                {(img) => <img class="rounded border border-border max-h-48 object-contain bg-bg" src={`data:${img().mime};base64,${img().data}`} alt={t("Lo que vio la IA")} />}
              </Match>
              <Match when={e.kind === "error" && e}>
                {(err) => <div class="text-xs text-error whitespace-pre-wrap">{err().text}</div>}
              </Match>
              <Match when={e.kind === "info" && e}>
                {(info) => <div class="text-[11px] text-text-dim">{info().text}</div>}
              </Match>
            </Switch>
          )}
        </For>
        <Show when={busy()}>
          <div class="flex items-center gap-2 text-xs text-text-muted">
            <span class="w-1.5 h-1.5 rounded-full bg-accent animate-pulse" />
            {t("Pensando…")}
          </div>
        </Show>
      </div>

      <div class="border-t border-border p-2 flex items-end gap-2">
        <textarea
          ref={input}
          rows={2}
          class="flex-1 resize-none rounded border border-border bg-surface/40 px-2 py-1.5 text-sm text-text outline-none focus:border-accent"
          placeholder={t("Escribe qué quieres hacer… (Enter envía)")}
          value={draft()}
          disabled={!status()?.claude}
          onInput={(e) => setDraft(e.currentTarget.value)}
          onKeyDown={(e) => {
            // Los atajos de la app no se disparan mientras se escribe
            e.stopPropagation();
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              void send();
            }
          }}
        />
        <Show
          when={busy()}
          fallback={
            <IconButton variant="primary" size="sm" aria-label={t("Enviar")} title={t("Enviar")} disabled={!draft().trim() || !status()?.claude} onClick={() => void send()}>
              <Icons.Play size={14} />
            </IconButton>
          }
        >
          <IconButton variant="ghost" size="sm" aria-label={t("Detener")} title={t("Detener")} onClick={() => void invoke("ai_chat_stop")}>
            <Icons.Stop size={14} />
          </IconButton>
        </Show>
      </div>
    </aside>
  );
};
