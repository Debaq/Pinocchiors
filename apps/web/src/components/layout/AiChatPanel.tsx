import { clsx } from "clsx";
import { Component, For, Match, Show, Switch, createEffect, createSignal, on, onCleanup, onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { base64Bytes, writeBytes } from "../../lib/writeBytes";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "../../lib/files";
import * as Icons from "../icons";
import { Button, IconButton, Select } from "../ui";
import { t } from "../../lib/i18n";
import { tBackend } from "../../lib/i18nBackend";
import { aiLabel, refreshLive } from "../../lib/mcpLive";
import { createPersisted, startDrag } from "../../lib/ui-state";
import type { ProviderView } from "./AiProviders";
import { Markdown } from "./Markdown";
import { PlanCard, SurveyCard } from "./AiPlanSurvey";
import { activeTeam, chatChoice, chatChoices, refLabel, refPrice, setChatChoice, setCompanies, type ModelRef } from "../../lib/aiTeams";
import { applyPlanChange, onSurveyDone, plan, planSummary, survey } from "../../lib/aiAssistant";

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
  | { kind: "info"; text: string }
  /** Una encuesta de la IA con lo que contestó el usuario (o que la cerró sin contestar) */
  | { kind: "answers"; title?: string; items: { question: string; answer: string }[]; skipped?: boolean }
  /** Empieza el turno de un rol de la combinación: quién habla desde acá */
  | { kind: "role"; role: "plan" | "build" | "review"; who: string };

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

/**
 * La conversación es de cada proyecto: va en su ui.json (`chatForProject`) y
 * vuelve al abrirlo (`restoreChat`); un proyecto nuevo empieza sin chat
 */
interface Saved {
  entries: Entry[];
  session: string | null;
  totals: Totals;
  /** Tokens que ocupa la conversación (los del último paso del modelo) */
  context?: number;
  /** Conversaciones de Claude Code de quien planifica y quien revisa */
  roleSessions?: { plan: string | null; review: string | null };
  /** Lo gastado por cada rol */
  roleTotals?: Partial<Record<"plan" | "build" | "review", Totals>>;
}
// Antes la conversación era una sola para toda la app, guardada en el navegador
try {
  localStorage.removeItem("pinocchio.ui.chat.conversation");
} catch {
  // Sin almacenamiento: nada que limpiar
}
const [entries, setEntries] = createSignal<Entry[]>([]);
const [session, setSession] = createSignal<string | null>(null);
const [totals, setTotals] = createSignal<Totals>(NO_TOTALS);
const [context, setContext] = createSignal(0);
const [busy, setBusy] = createSignal(false);
/** Sube con cada cambio de la conversación (el proyecto queda con cambios sin guardar) */
const [chatRevision, setChatRevision] = createSignal(0);
export { chatRevision };
const persist = () => setChatRevision((n) => n + 1);

// Cada encuesta queda escrita en el chat: qué se preguntó y qué se contestó
onSurveyDone((s, r) => {
  const items = s.questions.map((q, i) => {
    const a = "answers" in r ? r.answers[i]?.answer : undefined;
    return { question: q.question, answer: Array.isArray(a) ? a.join("; ") : (a ?? "") };
  });
  setEntries((list) => [...list, { kind: "answers", title: s.title, items, skipped: "skipped" in r || undefined }]);
  persist();
});

/** La conversación para el proyecto (sin las imágenes, que pesan; quedan las últimas 300 entradas) */
export function chatForProject(): Saved | undefined {
  if (entries().length === 0 && !session()) return undefined;
  const kept = entries()
    .filter((e) => e.kind !== "image")
    .map((e) => (e.kind === "user" && e.images ? { kind: "user" as const, text: e.text + (e.images.length ? ` [${e.images.length} 🖼]` : "") } : e))
    .slice(-300);
  return { entries: kept, session: session(), totals: totals(), context: context(), roleSessions: roleSessions(), roleTotals: roleTotals() };
}

/**
 * Pone la conversación de otro proyecto (sin nada, una vacía). La IA olvida
 * la anterior: con Claude Code sigue la sesión guardada en el proyecto; con un
 * proveedor con API empieza de nuevo (el chat se ve igual)
 */
export function restoreChat(raw: unknown) {
  if (busy()) void invoke("ai_chat_stop").catch(() => {});
  void invoke("ai_chat_reset").catch(() => {});
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const s: any = raw && typeof raw === "object" ? raw : {};
  setEntries(Array.isArray(s.entries) ? s.entries.filter((e: unknown) => e && typeof e === "object" && typeof (e as Entry).kind === "string") : []);
  setSession(typeof s.session === "string" ? s.session : null);
  setTotals(s.totals && typeof s.totals === "object" ? { ...NO_TOTALS, ...s.totals } : NO_TOTALS);
  setContext(typeof s.context === "number" ? s.context : 0);
  setRoleSessions({
    plan: typeof s.roleSessions?.plan === "string" ? s.roleSessions.plan : null,
    review: typeof s.roleSessions?.review === "string" ? s.roleSessions.review : null,
  });
  setRoleTotals(s.roleTotals && typeof s.roleTotals === "object" ? s.roleTotals : {});
  pipeline = null;
  setBusy(false);
  // La IA no recuerda esta conversación (salvo Claude Code con su sesión): se le cuenta con el próximo mensaje
  needsContext = entries().length > 0;
}

/** La IA que va a responder no conoce la conversación que se ve (proyecto reabierto u otro motor) */
let needsContext = false;

/**
 * La conversación de antes en texto, para que una IA que no la vio siga desde
 * ahí: lo dicho por el usuario y la IA y las encuestas, lo más nuevo entero
 */
function earlierConversation(): string {
  const lines: string[] = [];
  for (const e of entries()) {
    if (e.kind === "user") lines.push(`User: ${e.text}`);
    else if (e.kind === "text") lines.push(`Assistant: ${e.text}`);
    else if (e.kind === "answers" && !e.skipped) lines.push(`Survey${e.title ? ` "${e.title}"` : ""}: ${e.items.map((x) => `${x.question} → ${x.answer}`).join(" | ")}`);
  }
  const short = lines.map((l) => (l.length > 600 ? `${l.slice(0, 600)}…` : l));
  // Hasta unos 12 000 caracteres, de lo más nuevo hacia atrás
  const kept: string[] = [];
  let size = 0;
  for (let i = short.length - 1; i >= 0 && size + short[i].length < 12_000; i--) {
    kept.unshift(short[i]);
    size += short[i].length + 1;
  }
  if (kept.length < short.length) kept.unshift("(older messages left out)");
  return kept.join("\n");
}
/** Esfuerzo de Claude Code ("" = el suyo por defecto); se recuerda */
const [effort, setEffort] = createPersisted("chat.effort", "");
// Lo elegido antes de las combinaciones (motor y modelo sueltos) pasa a «Usar»
if (!chatChoice()) {
  try {
    const old = (k: string) => JSON.parse(localStorage.getItem(`pinocchio.ui.${k}`) ?? "null") as string | null;
    const engineBefore = old("chat.engine") ?? "claude";
    setChatChoice(`model:${engineBefore}|${engineBefore === "claude" ? (old("chat.model") ?? "") : ""}`);
  } catch {
    setChatChoice("model:claude|");
  }
}
/** Paso a paso: la IA hace un paso y se detiene; el usuario revisa y pulsa Continuar */
const [stepMode, setStepMode] = createPersisted("chat.stepMode", true);
/** Con más tokens que esto la conversación se resume sola antes del próximo mensaje ("0" = nunca) */
const [compactAt, setCompactAt] = createPersisted("chat.compactAt", "100000");
const COMPACT_OPTIONS = () => [
  { value: "50000", label: t("Al pasar de 50 mil tokens") },
  { value: "100000", label: t("Al pasar de 100 mil tokens") },
  { value: "200000", label: t("Al pasar de 200 mil tokens") },
  { value: "0", label: t("Nunca (solo con el botón)") },
];
/** Quién construye (responde): "claude" (Claude Code) o el id de un proveedor con API, y su modelo */
const engine = () => activeTeam().build.company;
const model = () => activeTeam().build.model;
/** Otra combinación o modelo: no vio la conversación, se le cuenta con el próximo mensaje */
const choose = (value: string) => {
  if (value !== chatChoice() && entries().length) needsContext = true;
  setChatChoice(value);
};
const [providers, setProvidersValue] = createSignal<ProviderView[]>([]);
const setProviders = (list: ProviderView[]) => {
  setProvidersValue(list);
  setCompanies(list);
};
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
    case "role":
      return `${ROLE_LABEL()[e.role]} · ${e.who}`;
    case "answers":
      return [e.title ?? "", ...e.items.map((x) => `${x.question} → ${e.skipped ? "—" : x.answer}`)].filter(Boolean).join("\n");
    default:
      return e.text;
  }
}

const extOf = (mime: string) => (mime === "image/jpeg" ? "jpg" : mime.split("/")[1] ?? "png");

/** Guarda una imagen (pregunta dónde) */
async function downloadPicture(p: Picture, name = "imagen") {
  const ext = extOf(p.mime);
  const path = await save({ defaultPath: `${name}.${ext}`, filters: [{ name: ext.toUpperCase(), extensions: [ext] }] });
  if (path) await writeBytes(path, base64Bytes(p.data));
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
    await writeBytes(file, base64Bytes(p.data));
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
    else if (e.kind === "role") lines.push(`**▸ ${ROLE_LABEL()[e.role]} · ${e.who}**`, "");
    else if (e.kind === "answers") {
      lines.push(`**${e.title || t("Preguntas de la IA")}**${e.skipped ? ` _(${t("cerrada sin responder")})_` : ""}`, "");
      for (const x of e.items) lines.push(`- ${x.question} → ${e.skipped ? "—" : x.answer}`);
      lines.push("");
    }
  }
  const tt = totals();
  lines.push("", `_${t("Conversación")}: ≈ ${kTokens(tt.input + tt.cacheRead + tt.cacheWrite)} ${t("de entrada")} · ${kTokens(tt.output)} ${t("de salida")}${tt.cost ? ` · USD ${tt.cost.toFixed(2)}` : ""}_`);
  await invoke("write_text_file", { path, contents: lines.join("\n") + "\n" });
}

/** 12 345 → «12.3k» */
function kTokens(n: number): string {
  return n >= 1000 ? `${(n / 1000).toFixed(n >= 100_000 ? 0 : 1)}k` : String(n);
}

/** Lo que se va a usar, para la etiqueta de la cabecera: «Calidad», «Sonnet · Medio», «DeepSeek · deepseek-chat» */
const choiceLabel = () => {
  const team = activeTeam();
  if (team.id) return team.name;
  if (engine() !== "claude") return provider() ? `${provider()!.name} · ${model() || provider()!.model}` : t("Proveedor borrado");
  const m = MODEL_OPTIONS().find((o) => o.value === model());
  const e = EFFORT_OPTIONS().find((o) => o.value === effort());
  if (!model() && !effort()) return "Claude Code";
  return [model() ? m?.label : "Claude Code", effort() ? e?.label.replace(/ \(.*\)$/, "") : ""].filter(Boolean).join(" · ");
};

/** Suma a los totales lo que costó un paso (tokens y dinero) */
function addTotals(tokens: unknown, costValue: unknown) {
  const tk = (tokens ?? {}) as Partial<Record<"input" | "cache_read" | "cache_write" | "output", number>>;
  // Claude Code dice cuánto costó; con una API se calcula con el precio anotado del modelo (si lo hay)
  const price = currentRef ? refPrice(currentRef) : undefined;
  const estimated = price ? (((tk.input ?? 0) + (tk.cache_read ?? 0) + (tk.cache_write ?? 0)) * price.in + (tk.output ?? 0) * price.out) / 1e6 : 0;
  const cost = typeof costValue === "number" ? costValue : estimated;
  const add = (s: Totals): Totals => ({
    input: s.input + (tk.input ?? 0),
    cacheRead: s.cacheRead + (tk.cache_read ?? 0),
    cacheWrite: s.cacheWrite + (tk.cache_write ?? 0),
    output: s.output + (tk.output ?? 0),
    cost: s.cost + cost,
  });
  setTotals(add);
  setRoleTotals((r) => ({ ...r, [currentRole]: add(r[currentRole] ?? NO_TOTALS) }));
  return { tk, cost };
}

/** Herramientas que solo miran: con ellas el trabajo no cambió y el plan no tiene por qué cambiar */
const READ_ONLY_TOOLS = new Set([
  "status", "help", "selection", "view", "measure", "viewer", "cad_design", "cad_geometry", "cad_versions", "asm_check",
  "mesh_info", "print_check", "print_machines", "resin_tilts", "printer_list", "printer_status", "calibration_status", "plan", "ask_user",
]);
const bareTool = (name: string) => name.replace(/^mcp__pinocchio__/, "");
/** Herramientas que usó la IA en este turno */
let turnTools: string[] = [];
/** Este turno es la revisión automática del plan (no se encadena otra) */
let checkingPlan = false;
/** Sube cuando la IA cambió el trabajo sin tocar el plan: el panel le pide ponerlo al día */
const [planCheckDue, setPlanCheckDue] = createSignal(0);

/** Los roles de una combinación (apps/PLAN_MODELOS.md) */
type Role = "plan" | "build" | "review";
const ROLE_LABEL = (): Record<Role, string> => ({ plan: t("Planifica"), build: t("Construye"), review: t("Revisa") });
/** Color de cada rol en el chat */
const ROLE_STYLE: Record<Role, string> = {
  plan: "text-accent border-accent/50",
  build: "text-green border-green/50",
  review: "text-warning border-warning/50",
};
/** Quién está respondiendo ahora, y con qué modelo (para el precio) */
let currentRole: Role = "build";
let currentRef: ModelRef | null = null;
/** Lo gastado por cada rol en esta conversación */
const [roleTotals, setRoleTotals] = createSignal<Partial<Record<Role, Totals>>>({});
/** Conversación de Claude Code de quien planifica y de quien revisa (la de quien construye es `session`) */
const [roleSessions, setRoleSessions] = createSignal<{ plan: string | null; review: string | null }>({ plan: null, review: null });
/** Los textos de la IA en este turno (el veredicto del revisor, el informe del constructor) */
let turnTexts: string[] = [];
const turnText = () => turnTexts.join("\n\n");
/** El ciclo en curso: en qué rol va, cuántas correcciones lleva y qué paso se revisa */
let pipeline: { role: Role; fixes: number; step?: { n: number; text: string }; report?: string } | null = null;
/** Terminó un turno del ciclo (`ok`): el panel decide el siguiente rol */
const [turnEnded, setTurnEnded] = createSignal<{ n: number; ok: boolean }>({ n: 0, ok: true });
/** Guarda la sesión de Claude Code del rol que respondió */
const keepSession = (s: string) => {
  if (currentRole === "build") setSession(s);
  else setRoleSessions((r) => ({ ...r, [currentRole]: s }));
};

/** Mensajes de Claude Code (`mcp::chat::translate`) → entradas del chat */
function onEvent(m: { kind: string; [k: string]: unknown }) {
  const add = (e: Entry) => setEntries((list) => [...list, e]);
  switch (m.kind) {
    case "context":
      if (typeof m.tokens === "number") setContext(m.tokens);
      return;
    case "compacting":
      return add({ kind: "info", text: m.auto ? t("La conversación está larga: la resumo antes de seguir…") : t("Resumiendo la conversación…") });
    case "compacted": {
      if (m.skipped) return add({ kind: "info", text: t("Todavía es corta: no hace falta resumirla") });
      if (!m.ok) return add({ kind: "error", text: tBackend(String(m.error ?? t("No se pudo resumir la conversación"))) });
      addTotals(m.tokens, m.cost);
      const before = typeof m.before === "number" ? m.before : 0;
      const after = typeof m.after === "number" ? m.after : 0;
      setContext(after);
      add({
        kind: "info",
        text: before
          ? t("Conversación resumida: de ≈ {a} a ≈ {b} tokens. Sigue igual, desde el resumen.", { a: kTokens(before), b: kTokens(after) })
          : t("Conversación resumida. Sigue igual, desde el resumen."),
      });
      persist();
      return;
    }
    case "start":
      answering = typeof m.model === "string" ? m.model : "";
      if (typeof m.session === "string") keepSession(m.session);
      return;
    case "text":
      turnTexts.push(String(m.text));
      return add({ kind: "text", text: String(m.text) });
    case "tool":
      turnTools.push(bareTool(String(m.name)));
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
      if (typeof m.session === "string") keepSession(m.session);
      // Fin de «Resumir»: ya se avisó con `compacted`
      if (m.quiet) return void persist();
      {
        const { tk, cost } = addTotals(m.tokens, m.cost);
        const used = (tk.input ?? 0) + (tk.cache_read ?? 0) + (tk.cache_write ?? 0);
        const parts = [answering, used ? t("{n} de entrada · {m} de salida", { n: kTokens(used), m: kTokens(tk.output ?? 0) }) : "", cost ? `USD ${cost.toFixed(3)}` : ""].filter(Boolean);
        if (parts.length) add({ kind: "info", text: parts.join(" · ") });
      }
      if (pipeline) {
        // En un ciclo con roles: el panel sigue con el próximo rol (o se detiene)
        setTurnEnded((x) => ({ n: x.n + 1, ok: !!m.ok }));
      } else {
        // Cambió el trabajo y no tocó el plan (con pasos pendientes): se le pide ponerlo al día, una vez
        const changed = turnTools.some((n) => !READ_ONLY_TOOLS.has(n));
        if (m.ok && !checkingPlan && changed && !turnTools.includes("plan") && plan().steps.some((s) => !s.done)) setPlanCheckDue((n) => n + 1);
        checkingPlan = false;
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
export const AiChatPanel: Component<{
  onClose: () => void;
  /** Mensaje (y fotos) que otra parte de la app deja listo para mandar; cambia `id` para ponerlo de nuevo */
  prefill?: { id: number; text: string; images: Picture[] };
}> = (props) => {
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

  createEffect(
    on(
      () => props.prefill?.id,
      () => {
        const p = props.prefill;
        if (!p) return;
        setDraft(p.text);
        setAttached(p.images);
        queueMicrotask(() => input?.focus());
      }
    )
  );

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

  /** Manda lo escrito (o `typed`, el texto del botón Continuar: ese no vuelve a planificar) */
  const send = async (typed?: string) => {
    const text = typed ?? (draft().trim() || (attached().length ? t("Mira la imagen.") : ""));
    if (!text || busy()) return;
    // Como en las terminales de IA: /clear (o /limpiar) empieza de cero
    if (/^\/(clear|limpiar)$/i.test(text)) {
      setDraft("");
      return reset();
    }
    // /compact (o /resumir) resume la conversación para que ocupe menos
    if (/^\/(compact|resumir)$/i.test(text)) {
      setDraft("");
      return compact();
    }
    const images = typed === undefined ? attached() : [];
    if (typed === undefined) {
      setDraft("");
      setAttached([]);
    }
    setNote(undefined);
    setEntries((l) => [...l, { kind: "user", text, images }]);
    persist();
    const team = activeTeam();
    // Con roles: planifica primero (salvo Continuar); si no, construye
    if (team.plan || team.review) {
      if (team.plan && typed === undefined) return void startRole("plan", planMessage(text), images, { role: "plan", fixes: 0, report: text });
      return void startBuild(text, images);
    }
    pipeline = null;
    await startRole("build", buildMessage(text, false), images);
  };

  /** Lo que se le manda a quien construye: el pedido, la conversación si no la vio, el plan y cómo trabajar */
  const buildMessage = (text: string, reviewed: boolean) => {
    const planText = planSummary();
    // Proyecto reabierto u otro motor: la conversación que se ve, para que la IA siga desde ahí
    // (Claude Code con su sesión ya la tiene)
    const before = needsContext && (engine() !== "claude" || !session()) ? earlierConversation() : "";
    needsContext = false;
    let message = text;
    if (before) message = `[Earlier conversation in this project, which you do not remember — continue from it]\n${before}\n\n[New message]\n${message}`;
    if (planText)
      message += reviewed
        ? `\n\n[Project plan — the user's visible checklist. Do NOT mark steps done: a reviewer checks your work and marks them. If this message changes what the user wants, edit/add/remove steps first]\n${planText}`
        : `\n\n[Project plan — the user's visible checklist. Keep it true: mark steps done with \`plan\` as you finish them, and if this message changes what the user wants, edit/add/remove steps first]\n${planText}`;
    if (reviewed)
      message +=
        "\n\n[You are the builder: do only the next pending step of the plan, check it with `view` or `asm_check`, then STOP and report in two or three lines exactly what you did (values, faces, names). A reviewer will check it.]";
    // Paso a paso: un paso, revisarlo y detenerse para que el usuario lo vea y pulse Continuar
    else if (stepMode())
      message +=
        "\n\n[Step by step: do only the next pending step of the plan (or, without a plan, the next small part of the job), check it with `view` or `asm_check`, mark it done with `plan`, then STOP and say in two or three lines what you did and what the next step is. The user reviews it and presses Continue.]";
    return message;
  };

  /** Lo que se le manda a quien planifica: solo arma el plan y pregunta, no construye */
  const planMessage = (text: string) => {
    const planText = planSummary();
    return `[You are the planner: you can only look and plan; you cannot change the work. Another model builds each step and another reviews it.]\n\n[The user's message]\n${text}\n\n${
      planText ? `[Current project plan]\n${planText}\n\n` : ""
    }[Your job] Look at the current state if needed (status, cad_design, view). Then set or update the project plan with \`plan\`: a one-line goal and short concrete steps a builder can do one at a time (sizes, materials, which tab). If the user's message changes the job, edit, add or remove steps. If you need decisions or data, ask them all at once with \`ask_user\` before writing the steps. Finish with one or two lines saying what the plan is.`;
  };

  /** Empieza a construir el próximo paso (o corrige lo que pidió el revisor) */
  const startBuild = (text: string, images: Picture[] = [], fixes = 0) => {
    const team = activeTeam();
    const reviewed = !!team.review;
    pipeline = { role: "build", fixes, step: nextStep() };
    return startRole("build", buildMessage(text, reviewed), images, pipeline);
  };

  /** Lo que se le manda a quien revisa: qué paso, qué dijo el constructor, y cómo contestar */
  const reviewMessage = (step: { n: number; text: string } | undefined, report: string) =>
    `[You are the reviewer: you can only look (view, measure, cad_design, cad_geometry, asm_check, mesh_info, print_check); you cannot change anything.]\n\n${
      step ? `[Step under review] ${step.n}. ${step.text}` : "[Under review] the builder's last change"
    }\n\n[What the builder reported]\n${report.slice(0, 1500) || "(nothing)"}\n\n[Project plan]\n${planSummary() || "(no plan)"}\n\n[Your job] Check the real result in the app, not the report: sizes, positions, collisions, that it matches the goal. If it is right, ${
      step ? `mark step ${step.n} done with \`plan\` and ` : ""
    }reply starting with "APROBADO" and one line. If something is wrong or missing, reply starting with "CORREGIR:" and list concrete fixes (what, where, which value). Do not suggest optional improvements.`;

  /** Un turno de un rol: con su modelo, su conversación y sus herramientas (quien planifica o revisa solo mira) */
  const startRole = async (role: Role, message: string, images: Picture[], stage?: typeof pipeline) => {
    const team = activeTeam();
    const ref = (role === "plan" ? team.plan : role === "review" ? team.review : null) ?? team.build;
    pipeline = stage ?? null;
    currentRole = role;
    currentRef = ref;
    turnTexts = [];
    turnTools = [];
    // Quién habla (con una combinación)
    if (team.plan || team.review) setEntries((l) => [...l, { kind: "role", role, who: refLabel(ref) }]);
    setBusy(true);
    scroll();
    try {
      const roleSession = role === "build" ? session() : roleSessions()[role];
      await invoke("ai_chat_send", {
        message,
        images,
        engine: ref.company,
        model: ref.model || null,
        effort: effort() || null,
        session: ref.company === "claude" ? roleSession : null,
        compactAt: Number(compactAt()) || null,
        readOnly: role !== "build",
      });
      void refreshLive();
    } catch (e) {
      pipeline = null;
      setBusy(false);
      setEntries((l) => [...l, { kind: "error", text: tBackend(String(e)) }]);
    }
  };

  /** Manda un mensaje ya armado a quien construye (la revisión automática del plan) */
  const dispatch = (message: string, images: Picture[]) => startRole("build", message, images);

  /** Terminó un turno del ciclo: sigue con el próximo rol, corrige o se detiene */
  createEffect(
    on(
      turnEnded,
      ({ ok }) => {
        const stage = pipeline;
        if (!stage) return;
        const team = activeTeam();
        if (!ok) {
          pipeline = null;
          return;
        }
        if (stage.role === "plan") {
          // Con el plan listo: el primer paso pendiente
          const step = nextStep();
          if (!step) {
            pipeline = null;
            return;
          }
          void startBuild(t("Haz el paso {n}: {text}", { n: step.n, text: step.text }));
          return;
        }
        if (stage.role === "build") {
          if (!team.review) {
            pipeline = null;
            return;
          }
          void startRole("review", reviewMessage(stage.step, turnText()), [], { ...stage, role: "review", report: turnText() });
          return;
        }
        // Veredicto del revisor: el último de sus textos que empieza con APROBADO o CORREGIR
        const clean = (x: string) => x.trim().replace(/^[*_#>\s]+/, "");
        const verdict = [...turnTexts].reverse().map(clean).find((x) => /^(APROBAD[OA]|CORREGIR)/i.test(x)) ?? clean(turnText());
        if (/^APROBAD[OA]/i.test(verdict)) {
          pipeline = null;
          // Si el revisor no marcó el paso, se marca (si no, el constructor lo haría de nuevo)
          const reviewed = stage.step;
          if (reviewed) {
            const steps = plan().steps;
            const i = steps[reviewed.n - 1]?.text === reviewed.text ? reviewed.n - 1 : steps.findIndex((x) => x.text === reviewed.text);
            if (i >= 0 && !steps[i].done) applyPlanChange({ done: [i + 1] });
          }
          // Sin paso a paso: sigue solo con el próximo paso pendiente
          const step = nextStep();
          if (!stepMode() && step) void startBuild(t("Haz el paso {n}: {text}", { n: step.n, text: step.text }));
          return;
        }
        if (stage.fixes < team.maxFixes) {
          void startBuild(`[Reviewer's corrections${stage.step ? ` for step ${stage.step.n}` : ""} — fix exactly these, then stop and report]\n${verdict}`, [], stage.fixes + 1);
          return;
        }
        pipeline = null;
        setEntries((l) => [
          ...l,
          {
            kind: "error",
            text:
              team.maxFixes > 0
                ? t("El revisor sigue pidiendo correcciones después de {n}. Míralo tú y dime cómo seguir.", { n: team.maxFixes })
                : t("El revisor encontró problemas. Míralo tú y dime cómo seguir."),
          },
        ]);
        persist();
      },
      { defer: true },
    ),
  );

  /** La IA cambió el trabajo sin tocar el plan: un turno corto solo para ponerlo al día */
  createEffect(
    on(
      planCheckDue,
      () => {
        if (busy()) return;
        checkingPlan = true;
        turnTools = [];
        setEntries((l) => [...l, { kind: "info", text: t("Poniendo al día el plan…") }]);
        setBusy(true);
        scroll();
        const message = `[Automatic check, not from the user] In your last reply you changed the work but did not touch the project plan. Update it now with the \`plan\` tool only: mark done the steps you finished and edit/add/remove steps if the job changed. Do nothing else and answer in one short line.\n\n[Project plan]\n${planSummary()}`;
        void dispatch(message, []).catch((e) => {
          checkingPlan = false;
          setBusy(false);
          setEntries((l) => [...l, { kind: "error", text: tBackend(String(e)) }]);
        });
      },
      { defer: true },
    ),
  );

  /** Resume la conversación ahora: la IA sigue desde el resumen y cada mensaje cuesta menos */
  const compact = async () => {
    if (busy() || entries().length === 0) return;
    setBusy(true);
    scroll();
    currentRole = "build";
    currentRef = activeTeam().build;
    try {
      await invoke("ai_chat_compact", { engine: engine(), model: model() || null, effort: effort() || null, session: engine() === "claude" ? session() : null });
    } catch (e) {
      setBusy(false);
      setEntries((l) => [...l, { kind: "error", text: tBackend(String(e)) }]);
    }
  };

  /** El primer paso del plan sin hacer (número desde 1) */
  const nextStep = () => {
    const i = plan().steps.findIndex((s) => !s.done);
    return i < 0 ? undefined : { n: i + 1, text: plan().steps[i].text };
  };
  /** La IA terminó de responder (y no espera una encuesta): se puede decirle que siga */
  const canContinue = () => {
    if (busy() || survey() || entries().length === 0) return false;
    if (entries()[entries().length - 1].kind === "user") return false;
    return engine() === "claude" ? !!status()?.claude : !!provider();
  };

  const reset = () => {
    void invoke("ai_chat_reset");
    pipeline = null;
    setEntries([]);
    setSession(null);
    setRoleSessions({ plan: null, review: null });
    setRoleTotals({});
    setContext(0);
    setTotals(NO_TOTALS);
    setBusy(false);
    persist();
  };

  return (
    <aside data-ai-chat class="relative flex flex-col shrink-0 bg-bg-darker border-r border-border overflow-hidden" style={{ width: `${width()}px` }} aria-label={t("Chat con IA")}>
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
        <IconButton
          variant="ghost"
          size="sm"
          aria-label={t("Resumir la conversación")}
          title={t("Resumir la conversación para que ocupe menos: la IA sigue desde el resumen y cada mensaje cuesta menos (también /resumir)")}
          onClick={() => void compact()}
          disabled={busy() || entries().length === 0}
        >
          <Icons.CornersIn size={14} />
        </IconButton>
        <IconButton variant="ghost" size="sm" aria-label={t("Limpiar el chat")} title={t("Limpiar el chat y empezar una conversación nueva (también /clear)")} onClick={reset} disabled={busy() || entries().length === 0}>
          <Icons.Trash size={14} />
        </IconButton>
        <IconButton variant="ghost" size="sm" aria-label={t("Cerrar")} onClick={() => props.onClose()}>
          <Icons.X size={14} />
        </IconButton>
      </div>

      <Show when={options()}>
        <div class="grid grid-cols-2 gap-2 px-3 py-2 border-b border-border">
          <div class="col-span-2">
            <Select label={t("Usar")} options={chatChoices()} value={chatChoice()} onChange={choose} />
            <p class="mt-1 text-[11px] text-text-muted leading-relaxed">
              <Show
                when={activeTeam().id}
                fallback={t("Un solo modelo hace todo. Las combinaciones (uno planifica, otro construye, otro revisa) se arman en Configuración ▸ Conector para IA.")}
              >
                {t("Planifica: {p} · Construye: {b} · Revisa: {r}", {
                  p: activeTeam().plan ? refLabel(activeTeam().plan) : t("quien construye"),
                  b: refLabel(activeTeam().build),
                  r: refLabel(activeTeam().review),
                })}
              </Show>
            </p>
          </div>
          <Show
            when={engine() === "claude"}
            fallback={
              <p class="col-span-2 text-[11px] text-text-muted leading-relaxed">
                {t("Usa la API de {name} con tu clave. Los proveedores se agregan en Configuración ▸ Conector para IA.", { name: provider()?.name ?? "—" })}
              </p>
            }
          >
            <Select class="col-span-2" label={t("Esfuerzo")} options={EFFORT_OPTIONS()} value={effort()} onChange={setEffort} />
            <p class="col-span-2 text-[11px] text-text-muted leading-relaxed">
              {t("Más esfuerzo piensa más antes de actuar: mejor para piezas complejas, más lento y gasta más de tu cuota. Vale desde el próximo mensaje.")}
            </p>
          </Show>
          <div class="col-span-2">
            <Select label={t("Resumir sola")} options={COMPACT_OPTIONS()} value={compactAt()} onChange={setCompactAt} />
            <p class="mt-1 text-[11px] text-text-muted leading-relaxed">
              {t("Cuando la conversación crece, la IA la resume antes del próximo mensaje: recuerda lo importante y gasta menos. Los resultados viejos y las imágenes ya vistas se acortan solos.")}
            </p>
          </div>
          <Show when={providers().length === 0}>
            <p class="col-span-2 text-[11px] text-text-dim leading-relaxed">
              {t("¿DeepSeek, OpenAI u Ollama? Agrégalos en Configuración ▸ Conector para IA ▸ Proveedores para el chat.")}
            </p>
          </Show>
        </div>
      </Show>

      {/* El plan del proyecto (objetivo y pasos) */}
      <PlanCard />

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
                <Match when={e.kind === "role" && e}>
                  {(r) => (
                    <div class="flex items-center gap-1.5 pt-1 text-[10.5px]">
                      <span class={clsx("rounded-full border px-1.5 py-px font-semibold uppercase tracking-wide", ROLE_STYLE[r().role])}>{ROLE_LABEL()[r().role]}</span>
                      <span class="text-text-dim truncate">{r().who}</span>
                      <span class="flex-1 border-t border-border/50" />
                    </div>
                  )}
                </Match>
                <Match when={e.kind === "answers" && e}>
                  {(a) => (
                    <div class="ml-8 rounded-lg bg-accent/10 border border-accent/30 px-3 py-2 text-xs space-y-1.5">
                      <div class="font-semibold text-text">
                        {a().title || t("Preguntas de la IA")}
                        <Show when={a().skipped}>
                          <span class="ml-1 font-normal text-text-dim">({t("cerrada sin responder")})</span>
                        </Show>
                      </div>
                      <For each={a().items}>
                        {(x) => (
                          <div>
                            <div class="text-text-muted">{x.question}</div>
                            <div class="text-text whitespace-pre-wrap">{a().skipped ? "—" : x.answer}</div>
                          </div>
                        )}
                      </For>
                    </div>
                  )}
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

      {/* Preguntas de la IA: se contestan todas antes de enviar */}
      <SurveyCard />

      <Show when={totals().input + totals().cacheRead + totals().output > 0 || context() > 0}>
        <div
          class="px-3 py-1 border-t border-border text-[10.5px] text-text-dim flex gap-1.5 flex-wrap"
          title={t("Tokens de esta conversación: entrada nueva {a}, leída de la caché {b}, escrita en la caché {c}; salida {d}", {
            a: kTokens(totals().input),
            b: kTokens(totals().cacheRead),
            c: kTokens(totals().cacheWrite),
            d: kTokens(totals().output),
          })}
        >
          <Show when={context() > 0}>
            <span
              classList={{ "text-warning": Number(compactAt()) > 0 && context() > Number(compactAt()) }}
              title={t("Lo que ocupa la conversación ahora: se manda entera con cada mensaje")}
            >
              {t("Ocupa ≈ {n}", { n: kTokens(context()) })} ·
            </span>
          </Show>
          <span>{t("Conversación")}:</span>
          <span>≈ {kTokens(totals().input + totals().cacheRead + totals().cacheWrite)} {t("de entrada")}</span>
          <span>· {kTokens(totals().output)} {t("de salida")}</span>
          <Show when={totals().cost > 0}>
            <span>· USD {totals().cost.toFixed(2)}</span>
          </Show>
          {/* Con una combinación: lo de cada rol */}
          <Show when={Object.keys(roleTotals()).length > 1}>
            <span class="w-full flex gap-2 flex-wrap" title={t("Lo gastado por cada rol (con una API, según el precio anotado del modelo)")}>
              <For each={(["plan", "build", "review"] as const).filter((r) => roleTotals()[r])}>
                {(r) => (
                  <span>
                    <span class={ROLE_STYLE[r].split(" ")[0]}>{ROLE_LABEL()[r]}</span>{" "}
                    {roleTotals()[r]!.cost > 0
                      ? `USD ${roleTotals()[r]!.cost.toFixed(2)}`
                      : `≈ ${kTokens(roleTotals()[r]!.input + roleTotals()[r]!.cacheRead + roleTotals()[r]!.cacheWrite + roleTotals()[r]!.output)}`}
                  </span>
                )}
              </For>
            </span>
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

      {/* Continuar: la IA terminó su paso; se revisa y se le dice que siga (o se escribe otra cosa) */}
      <div class="border-t border-border px-2 pt-2 flex items-center gap-2 text-xs">
        <Show when={canContinue()}>
          <Button
            size="sm"
            variant="primary"
            data-ai-continue
            class="min-w-0"
            title={nextStep() ? t("Sigue con el paso {n}: {text}", { n: nextStep()!.n, text: nextStep()!.text }) : t("Que la IA siga con lo que viene")}
            onClick={() => void send(nextStep() ? t("Continúa con el paso {n}: {text}", { n: nextStep()!.n, text: nextStep()!.text }) : t("Continúa."))}
          >
            <Icons.Play size={12} class="shrink-0" />
            <span class="truncate">{nextStep() ? t("Continuar: paso {n}", { n: nextStep()!.n }) : t("Continuar")}</span>
          </Button>
        </Show>
        <span class="flex-1" />
        <label class="flex items-center gap-1.5 cursor-pointer text-text-muted" title={t("La IA hace un paso, lo revisa y se detiene: tú miras el resultado y pulsas Continuar")}>
          <input type="checkbox" class="accent-[var(--color-accent)]" checked={stepMode()} onChange={(e) => setStepMode(e.currentTarget.checked)} />
          {t("Paso a paso")}
        </label>
      </div>
      <div class="p-2 flex items-end gap-2">
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
