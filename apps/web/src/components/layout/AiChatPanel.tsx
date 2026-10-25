import { Component, For, Match, Show, Switch, createSignal, onCleanup, onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import * as Icons from "../icons";
import { IconButton, Select } from "../ui";
import { t } from "../../lib/i18n";
import { tBackend } from "../../lib/i18nBackend";
import { aiLabel, refreshLive } from "../../lib/mcpLive";
import { createPersisted, startDrag } from "../../lib/ui-state";
import type { ProviderView } from "./AiProviders";
import { Markdown } from "./Markdown";

/** Imagen adjunta o vista (base64) */
interface Picture {
  data: string;
  mime: string;
}

/** Una entrada del chat */
type Entry =
  | { kind: "user"; text: string; images?: Picture[] }
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

/** Tokens de la conversación (lo que informa Claude Code en cada respuesta) */
interface Totals {
  input: number;
  cacheRead: number;
  cacheWrite: number;
  output: number;
  cost: number;
}
const NO_TOTALS: Totals = { input: 0, cacheRead: 0, cacheWrite: 0, output: 0, cost: 0 };

/** La conversación guardada: sigue tras cerrar el panel y tras reiniciar la app */
interface Saved {
  entries: Entry[];
  session: string | null;
  totals: Totals;
}
const SAVED_KEY = "pinocchio.ui.chat.conversation";
const loadSaved = (): Saved => {
  try {
    const raw = localStorage.getItem(SAVED_KEY);
    if (raw) return { entries: [], session: null, totals: NO_TOTALS, ...JSON.parse(raw) };
  } catch {
    // Sin almacenamiento: conversación nueva
  }
  return { entries: [], session: null, totals: NO_TOTALS };
};
const saved = loadSaved();
const [entries, setEntries] = createSignal<Entry[]>(saved.entries);
const [session, setSession] = createSignal<string | null>(saved.session);
const [totals, setTotals] = createSignal<Totals>(saved.totals);
const [busy, setBusy] = createSignal(false);
/** Guarda la conversación (sin las imágenes, que pesan; quedan las últimas 300 entradas) */
const persist = () => {
  try {
    const kept = entries()
      .filter((e) => e.kind !== "image")
      .map((e) => (e.kind === "user" && e.images ? { kind: "user" as const, text: e.text + (e.images.length ? ` [${e.images.length} 🖼]` : "") } : e))
      .slice(-300);
    localStorage.setItem(SAVED_KEY, JSON.stringify({ entries: kept, session: session(), totals: totals() }));
  } catch {
    // Sin almacenamiento: se pierde al cerrar
  }
};
/** Modelo y esfuerzo de Claude Code ("" = los suyos por defecto); se recuerdan */
const [model, setModel] = createPersisted("chat.model", "");
const [effort, setEffort] = createPersisted("chat.effort", "");
/** Quién responde: "claude" (Claude Code) o el id de un proveedor con API */
const [engine, setEngine] = createPersisted("chat.engine", "claude");
const [providers, setProviders] = createSignal<ProviderView[]>([]);
const provider = () => providers().find((p) => p.id === engine());
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

/** Lo que se copia de una entrada */
function entryText(e: Entry): string {
  switch (e.kind) {
    case "tool":
      return `${aiLabel(e.name)} (${e.name})${e.detail ? `\n${e.detail}` : ""}`;
    case "image":
      return "";
    default:
      return e.text;
  }
}

const b64ToBytes = (b64: string) => Array.from(atob(b64), (c) => c.charCodeAt(0));
const extOf = (mime: string) => (mime === "image/jpeg" ? "jpg" : mime.split("/")[1] ?? "png");

/** Guarda una imagen (pregunta dónde) */
async function downloadPicture(p: Picture, name = "imagen") {
  const ext = extOf(p.mime);
  const path = await save({ defaultPath: `${name}.${ext}`, filters: [{ name: ext.toUpperCase(), extensions: [ext] }] });
  if (path) await invoke("cad_write_bytes", { path, bytes: b64ToBytes(p.data) });
}

/** La conversación en Markdown; las imágenes van en archivos al lado (`<nombre>-imagen-N.ext`) */
async function saveConversation() {
  const path = await save({ defaultPath: "conversacion-pinocchio.md", filters: [{ name: "Markdown", extensions: ["md"] }] });
  if (!path) return;
  const stem = path.replace(/\.md$/i, "");
  const base = stem.split(/[\\/]/).pop() ?? "conversacion";
  let n = 0;
  const lines: string[] = [`# ${t("Chat con IA")} — Pinocchio`, ""];
  const picture = async (p: Picture) => {
    n += 1;
    const file = `${stem}-imagen-${n}.${extOf(p.mime)}`;
    await invoke("cad_write_bytes", { path: file, bytes: b64ToBytes(p.data) });
    lines.push(`![](${base}-imagen-${n}.${extOf(p.mime)})`, "");
  };
  for (const e of entries()) {
    if (e.kind === "user") {
      lines.push(`**${t("Tú")}:** ${e.text}`, "");
      for (const p of e.images ?? []) await picture(p);
    } else if (e.kind === "text") lines.push(e.text, "");
    else if (e.kind === "tool") lines.push(`- ${e.state === "error" ? "✗" : "✓"} ${aiLabel(e.name)} \`${e.name}\``);
    else if (e.kind === "image") await picture(e);
    else if (e.kind === "error") lines.push(`> ⚠ ${e.text}`, "");
    else if (e.kind === "info") lines.push(`_${e.text}_`, "");
  }
  const tt = totals();
  lines.push("", `_${t("Conversación")}: ≈ ${kTokens(tt.input + tt.cacheRead + tt.cacheWrite)} ${t("de entrada")} · ${kTokens(tt.output)} ${t("de salida")}${tt.cost ? ` · USD ${tt.cost.toFixed(2)}` : ""}_`);
  await invoke("write_text_file", { path, contents: lines.join("\n") + "\n" });
}

/** 12 345 → «12.3k» */
function kTokens(n: number): string {
  return n >= 1000 ? `${(n / 1000).toFixed(n >= 100_000 ? 0 : 1)}k` : String(n);
}

/** Lo que se va a usar, para la etiqueta de la cabecera: «Sonnet · Medio» */
const choiceLabel = () => {
  const p = provider();
  if (engine() !== "claude") return p ? `${p.name} · ${p.model}` : t("Proveedor borrado");
  const m = MODEL_OPTIONS().find((o) => o.value === model());
  const e = EFFORT_OPTIONS().find((o) => o.value === effort());
  if (!model() && !effort()) return "Claude Code";
  return [model() ? m?.label : "Claude Code", effort() ? e?.label.replace(/ \(.*\)$/, "") : ""].filter(Boolean).join(" · ");
};

/** Mensajes de Claude Code (`mcp::chat::translate`) → entradas del chat */
function onEvent(m: { kind: string; [k: string]: unknown }) {
  const add = (e: Entry) => setEntries((list) => [...list, e]);
  switch (m.kind) {
    case "start":
      answering = typeof m.model === "string" ? m.model : "";
      if (typeof m.session === "string") setSession(m.session);
      return;
    case "text":
      return add({ kind: "text", text: String(m.text) });
    case "tool":
      return add({ kind: "tool", id: String(m.id), name: String(m.name), state: "running" });
    case "result":
      return setEntries((list) =>
        list.map((e) => (e.kind === "tool" && e.id === m.id ? { ...e, state: m.error ? "error" : "ok", detail: String(m.text ?? "") } : e)),
      );
    case "image":
      return add({ kind: "image", data: String(m.data), mime: String(m.mime ?? "image/png") });
    case "done":
      setBusy(false);
      // Lo que quedó «en curso» terminó con el turno
      setEntries((list) => list.map((e) => (e.kind === "tool" && e.state === "running" ? { ...e, state: m.ok ? "ok" : "error" } : e)));
      if (!m.ok && m.error) add({ kind: "error", text: tBackend(String(m.error)) });
      if (typeof m.session === "string") setSession(m.session);
      {
        const tk = (m.tokens ?? {}) as Partial<Record<"input" | "cache_read" | "cache_write" | "output", number>>;
        const cost = typeof m.cost === "number" ? m.cost : 0;
        setTotals((s) => ({
          input: s.input + (tk.input ?? 0),
          cacheRead: s.cacheRead + (tk.cache_read ?? 0),
          cacheWrite: s.cacheWrite + (tk.cache_write ?? 0),
          output: s.output + (tk.output ?? 0),
          cost: s.cost + cost,
        }));
        const used = (tk.input ?? 0) + (tk.cache_read ?? 0) + (tk.cache_write ?? 0);
        const parts = [answering, used ? t("{n} de entrada · {m} de salida", { n: kTokens(used), m: kTokens(tk.output ?? 0) }) : "", cost ? `USD ${cost.toFixed(3)}` : ""].filter(Boolean);
        if (parts.length) add({ kind: "info", text: parts.join(" · ") });
      }
      persist();
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
  /** Imágenes para el próximo mensaje */
  const [attached, setAttached] = createSignal<Picture[]>([]);
  const [copied, setCopied] = createSignal<number>();
  const [note, setNote] = createSignal<string>();
  let list: HTMLDivElement | undefined;
  let input: HTMLTextAreaElement | undefined;

  const scroll = () => queueMicrotask(() => list?.scrollTo({ top: list.scrollHeight }));

  onMount(() => {
    void invoke<ChatStatus>("ai_chat_status").then((s) => {
      setStatus(s);
      setBusy(s.busy);
    });
    void invoke<ProviderView[]>("ai_providers").then(setProviders).catch(() => {});
    const stop = listen<{ kind: string }>("ai-chat://event", (e) => {
      onEvent(e.payload);
      scroll();
    });
    onCleanup(() => void stop.then((f) => f()));
    input?.focus();
    scroll();
  });

  const copy = (i: number, text: string) =>
    void navigator.clipboard.writeText(text).then(() => {
      setCopied(i);
      setTimeout(() => setCopied((c) => (c === i ? undefined : c)), 1200);
    });

  const attachFiles = async () => {
    const chosen = await open({ multiple: true, filters: [{ name: t("Imágenes"), extensions: ["png", "jpg", "jpeg", "webp", "gif"] }] });
    const paths = chosen === null ? [] : Array.isArray(chosen) ? chosen : [chosen];
    for (const path of paths) {
      try {
        const p = await invoke<Picture>("ai_chat_read_image", { path });
        setAttached((l) => [...l, p]);
      } catch (e) {
        setNote(tBackend(String(e)));
      }
    }
  };

  /** Pegar una imagen (Ctrl+V) la adjunta */
  const onPaste = (e: ClipboardEvent) => {
    for (const item of Array.from(e.clipboardData?.items ?? [])) {
      if (!item.type.startsWith("image/")) continue;
      const file = item.getAsFile();
      if (!file) continue;
      e.preventDefault();
      if (file.size > 5 * 1024 * 1024) {
        setNote(t("La imagen pesa más de 5 MB"));
        continue;
      }
      const reader = new FileReader();
      reader.onload = () => {
        const url = String(reader.result);
        setAttached((l) => [...l, { mime: file.type, data: url.slice(url.indexOf(",") + 1) }]);
      };
      reader.readAsDataURL(file);
    }
  };

  const send = async () => {
    const text = draft().trim() || (attached().length ? t("Mira la imagen.") : "");
    if (!text || busy()) return;
    const images = attached();
    setDraft("");
    setAttached([]);
    setNote(undefined);
    setEntries((l) => [...l, { kind: "user", text, images }]);
    persist();
    setBusy(true);
    scroll();
    try {
      await invoke("ai_chat_send", { message: text, images, engine: engine(), model: model() || null, effort: effort() || null, session: engine() === "claude" ? session() : null });
      void refreshLive();
    } catch (e) {
      setBusy(false);
      setEntries((l) => [...l, { kind: "error", text: tBackend(String(e)) }]);
    }
  };

  const reset = () => {
    void invoke("ai_chat_reset");
    setEntries([]);
    setSession(null);
    setTotals(NO_TOTALS);
    setBusy(false);
    persist();
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
        <span class="text-sm font-semibold text-text">{t("Chat con IA")}</span>
        <span class="flex-1" />
        {/* Lo que se va a usar; al pulsarlo se elige */}
        <button
          class="max-w-[11rem] truncate px-2 h-6 rounded border text-[11px]"
          classList={{ "border-accent/60 text-accent": options(), "border-border text-text-muted hover:text-text hover:border-border-hover": !options() }}
          title={t("Modelo y esfuerzo")}
          aria-label={t("Modelo y esfuerzo")}
          onClick={() => setOptions(!options())}
        >
          {choiceLabel()}
        </button>
        <IconButton variant="ghost" size="sm" aria-label={t("Guardar la conversación")} title={t("Guardar la conversación")} onClick={() => void saveConversation().catch((e) => setNote(tBackend(String(e))))} disabled={entries().length === 0}>
          <Icons.FloppyDisk size={14} />
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
          <div class="col-span-2">
            <Select
              label={t("Motor")}
              options={[{ value: "claude", label: "Claude Code" }, ...providers().map((p) => ({ value: p.id, label: `${p.name} · ${p.model}` }))]}
              value={engine()}
              onChange={(v) => {
                // Otro motor: otra conversación (cada uno tiene su historia)
                if (v !== engine() && entries().length) reset();
                setEngine(v);
              }}
            />
          </div>
          <Show
            when={engine() === "claude"}
            fallback={
              <p class="col-span-2 text-[11px] text-text-muted leading-relaxed">
                {t("Usa la API de {name} con tu clave. Los proveedores se agregan en Configuración ▸ Conector para IA.", { name: provider()?.name ?? "—" })}
              </p>
            }
          >
            <Select label={t("Modelo")} options={MODEL_OPTIONS()} value={model()} onChange={setModel} />
            <Select label={t("Esfuerzo")} options={EFFORT_OPTIONS()} value={effort()} onChange={setEffort} />
            <p class="col-span-2 text-[11px] text-text-muted leading-relaxed">
              {t("Más esfuerzo piensa más antes de actuar: mejor para piezas complejas, más lento y gasta más de tu cuota. Vale desde el próximo mensaje.")}
            </p>
          </Show>
          <Show when={providers().length === 0}>
            <p class="col-span-2 text-[11px] text-text-dim leading-relaxed">
              {t("¿DeepSeek, OpenAI u Ollama? Agrégalos en Configuración ▸ Conector para IA ▸ Proveedores para el chat.")}
            </p>
          </Show>
        </div>
      </Show>

      <div ref={list} class="flex-1 overflow-y-auto px-3 py-3 space-y-2 text-sm select-text">
        <Show when={status() && !status()!.claude && engine() === "claude"}>
          <p class="text-xs text-warning leading-relaxed">
            {t("No se encontró Claude Code en este equipo. Instálalo (claude.com/claude-code) e inicia sesión una vez en la terminal; el chat usa tu cuenta.")}
          </p>
        </Show>
        <Show when={entries().length === 0 && (status()?.claude || engine() !== "claude")}>
          <p class="text-xs text-text-muted leading-relaxed">
            {t(
              "Pídele lo que quieras diseñar: «haz una caja de 80 × 50 × 30 con tapa», «redondea las aristas que seleccioné», «arma un ensamble con estas piezas». Trabaja en esta ventana y cada cambio se deshace con Ctrl+Z.",
            )}
          </p>
        </Show>
        <For each={entries()}>
          {(e, i) => (
            <div class="group relative">
              <Switch>
                <Match when={e.kind === "user" && e}>
                  {(u) => (
                    <div class="ml-8 rounded-lg bg-accent/15 border border-accent/30 px-3 py-2 text-text whitespace-pre-wrap space-y-2">
                      <div>{u().text}</div>
                      <Show when={u().images?.length}>
                        <div class="flex flex-wrap gap-1.5">
                          <For each={u().images}>{(p) => <img class="h-16 rounded border border-border object-cover" src={`data:${p.mime};base64,${p.data}`} alt="" />}</For>
                        </div>
                      </Show>
                    </div>
                  )}
                </Match>
                <Match when={e.kind === "text" && e}>
                  {(a) => <Markdown class="mr-6 text-text leading-relaxed" text={a().text} />}
                </Match>
                <Match when={e.kind === "tool" && e}>
                  {(tool) => (
                    <div class="flex items-center gap-2 pr-6 text-xs text-text-muted" title={tool().detail}>
                      <span
                        class="w-1.5 h-1.5 rounded-full shrink-0"
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
                  {(err) => <div class="pr-6 text-xs text-error whitespace-pre-wrap">{err().text}</div>}
                </Match>
                <Match when={e.kind === "info" && e}>
                  {(info) => <div class="pr-6 text-[11px] text-text-dim">{info().text}</div>}
                </Match>
              </Switch>
              {/* Copiar (las imágenes: descargar), al pasar el mouse */}
              <button
                class="absolute top-0 right-0 hidden group-hover:flex items-center justify-center w-6 h-6 rounded bg-bg-darker/90 border border-border text-text-muted hover:text-text"
                title={e.kind === "image" ? t("Descargar la imagen") : copied() === i() ? t("Copiado") : t("Copiar")}
                aria-label={e.kind === "image" ? t("Descargar la imagen") : t("Copiar")}
                onClick={() => (e.kind === "image" ? void downloadPicture(e, `pinocchio-${i() + 1}`).catch((err) => setNote(tBackend(String(err)))) : copy(i(), entryText(e)))}
              >
                <Show when={e.kind === "image"} fallback={<Show when={copied() === i()} fallback={<Icons.Copy size={12} />}><Icons.Check size={12} /></Show>}>
                  <Icons.Download size={12} />
                </Show>
              </button>
            </div>
          )}
        </For>
        <Show when={busy()}>
          <div class="flex items-center gap-2 text-xs text-text-muted">
            <span class="w-1.5 h-1.5 rounded-full bg-accent animate-pulse" />
            {t("Pensando…")}
          </div>
        </Show>
      </div>

      <Show when={totals().input + totals().cacheRead + totals().output > 0}>
        <div
          class="px-3 py-1 border-t border-border text-[10.5px] text-text-dim flex gap-1.5 flex-wrap"
          title={t("Tokens de esta conversación: entrada nueva {a}, leída de la caché {b}, escrita en la caché {c}; salida {d}", {
            a: kTokens(totals().input),
            b: kTokens(totals().cacheRead),
            c: kTokens(totals().cacheWrite),
            d: kTokens(totals().output),
          })}
        >
          <span>{t("Conversación")}:</span>
          <span>≈ {kTokens(totals().input + totals().cacheRead + totals().cacheWrite)} {t("de entrada")}</span>
          <span>· {kTokens(totals().output)} {t("de salida")}</span>
          <Show when={totals().cost > 0}>
            <span>· USD {totals().cost.toFixed(2)}</span>
          </Show>
        </div>
      </Show>

      <Show when={attached().length || note()}>
        <div class="px-2 pt-2 border-t border-border space-y-1.5">
          <Show when={attached().length}>
            <div class="flex flex-wrap gap-1.5">
              <For each={attached()}>
                {(p, i) => (
                  <div class="relative">
                    <img class="h-14 w-14 rounded border border-border object-cover" src={`data:${p.mime};base64,${p.data}`} alt="" />
                    <button
                      class="absolute -top-1.5 -right-1.5 w-4 h-4 rounded-full bg-bg-darker border border-border flex items-center justify-center text-text-muted hover:text-error"
                      aria-label={t("Quitar")}
                      onClick={() => setAttached((l) => l.filter((_, k) => k !== i()))}
                    >
                      <Icons.X size={9} />
                    </button>
                  </div>
                )}
              </For>
            </div>
          </Show>
          <Show when={note()}>
            <p class="text-[11px] text-warning">{note()}</p>
          </Show>
        </div>
      </Show>

      <div class="border-t border-border p-2 flex items-end gap-2">
        <IconButton variant="ghost" size="sm" aria-label={t("Adjuntar una imagen")} title={t("Adjuntar una imagen (también puedes pegarla con Ctrl+V)")} disabled={(engine() === "claude" && !status()?.claude) || busy()} onClick={() => void attachFiles()}>
          <Icons.Paperclip size={14} />
        </IconButton>
        <textarea
          onPaste={onPaste}
          ref={input}
          rows={2}
          class="flex-1 resize-none rounded border border-border bg-surface/40 px-2 py-1.5 text-sm text-text outline-none focus:border-accent"
          placeholder={t("Escribe qué quieres hacer… (Enter envía)")}
          value={draft()}
          disabled={engine() === "claude" && !status()?.claude}
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
            <IconButton variant="primary" size="sm" aria-label={t("Enviar")} title={t("Enviar")} disabled={(!draft().trim() && !attached().length) || (engine() === "claude" && !status()?.claude) || (engine() !== "claude" && !provider())} onClick={() => void send()}>
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
