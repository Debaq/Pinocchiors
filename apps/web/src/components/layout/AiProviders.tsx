import { Component, For, Show, createSignal, onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { Button, Checkbox, Select } from "../ui";
import { t } from "../../lib/i18n";
import { tBackend } from "../../lib/i18nBackend";

/** Un proveedor guardado (`mcp::agent::ProviderView`: sin la clave) */
export interface ProviderView {
  id: string;
  name: string;
  base_url: string;
  model: string;
  vision: boolean;
  has_key: boolean;
}

/** Atajos: dirección, modelo sugerido, si ve imágenes y si pide clave */
const PRESETS: Record<string, { name: string; url: string; model: string; hint: string; vision: boolean; key: boolean }> = {
  deepseek: { name: "DeepSeek", url: "https://api.deepseek.com", model: "deepseek-chat", hint: "deepseek-chat", vision: false, key: true },
  openai: { name: "OpenAI", url: "https://api.openai.com/v1", model: "", hint: "gpt-…", vision: true, key: true },
  openrouter: { name: "OpenRouter", url: "https://openrouter.ai/api/v1", model: "", hint: "deepseek/deepseek-chat", vision: false, key: true },
  ollama: { name: "Ollama", url: "http://localhost:11434/v1", model: "", hint: "qwen2.5:14b", vision: false, key: false },
  lmstudio: { name: "LM Studio", url: "http://localhost:1234/v1", model: "", hint: "", vision: false, key: false },
  custom: { name: "", url: "", model: "", hint: "", vision: false, key: true },
};

const PRESET_OPTIONS = () => [
  { value: "deepseek", label: "DeepSeek" },
  { value: "openai", label: "OpenAI" },
  { value: "openrouter", label: "OpenRouter" },
  { value: "ollama", label: t("Ollama (en este equipo)") },
  { value: "lmstudio", label: t("LM Studio (en este equipo)") },
  { value: "custom", label: t("Otro compatible con OpenAI") },
];

interface Form {
  id?: string;
  preset: string;
  name: string;
  base_url: string;
  api_key: string;
  model: string;
  vision: boolean;
  /** Ya tiene clave guardada (vacío = la deja) */
  has_key: boolean;
}

const blank = (preset = "deepseek"): Form => {
  const p = PRESETS[preset];
  return { preset, name: p.name, base_url: p.url, api_key: "", model: p.model, vision: p.vision, has_key: false };
};

const Field: Component<{ label: string; value: string; placeholder?: string; type?: string; onInput: (v: string) => void }> = (props) => (
  <label class="block space-y-1">
    <span class="text-xs text-text-muted">{props.label}</span>
    <input
      type={props.type ?? "text"}
      class="w-full px-2 py-1 rounded border border-border bg-surface/40 text-sm text-text outline-none focus:border-accent"
      value={props.value}
      placeholder={props.placeholder}
      autocomplete="off"
      spellcheck={false}
      onInput={(e) => props.onInput(e.currentTarget.value)}
      onKeyDown={(e) => e.stopPropagation()}
    />
  </label>
);

/**
 * Configuración ▸ Conector para IA ▸ Proveedores para el chat: APIs
 * compatibles con OpenAI (DeepSeek, OpenAI, OpenRouter, Ollama, LM Studio…)
 * que el chat puede usar en vez de Claude Code.
 */
export const AiProviders: Component = () => {
  const [list, setList] = createSignal<ProviderView[]>([]);
  const [form, setForm] = createSignal<Form>();
  const [busy, setBusy] = createSignal(false);
  const [message, setMessage] = createSignal<{ text: string; error?: boolean }>();

  onMount(() => void invoke<ProviderView[]>("ai_providers").then(setList).catch(() => {}));

  const edit = (p?: ProviderView) => {
    setMessage(undefined);
    if (!p) return setForm(blank());
    const preset = Object.entries(PRESETS).find(([, v]) => v.url === p.base_url)?.[0] ?? "custom";
    setForm({ id: p.id, preset, name: p.name, base_url: p.base_url, api_key: "", model: p.model, vision: p.vision, has_key: p.has_key });
  };

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

  const saveForm = () =>
    run(async () => {
      const f = form()!;
      const saved = await invoke<ProviderView[]>("ai_provider_save", {
        provider: { id: f.id ?? null, name: f.name, base_url: f.base_url, api_key: f.api_key.trim() ? f.api_key : f.id ? null : "", model: f.model, vision: f.vision },
      });
      setList(saved);
      setForm(undefined);
      setMessage({ text: t("Proveedor guardado. Elígelo en el chat, en «Motor».") });
    });

  const test = (id: string) =>
    run(async () => {
      setMessage({ text: tBackend(await invoke<string>("ai_provider_test", { id })) });
    });

  const remove = (id: string) =>
    run(async () => {
      setList(await invoke<ProviderView[]>("ai_provider_delete", { id }));
      setForm(undefined);
    });

  const set = (patch: Partial<Form>) => setForm((f) => (f ? { ...f, ...patch } : f));

  return (
    <div class="space-y-3">
      <p class="text-xs text-text-muted leading-relaxed">
        {t(
          "Además de Claude Code, el chat puede usar cualquier API compatible con OpenAI: DeepSeek, OpenAI, OpenRouter, o un modelo en este equipo con Ollama o LM Studio. Se paga por uso con tu clave (los locales son gratis). Los modelos chicos se equivocan más con piezas complejas.",
        )}
      </p>
      <Show when={list().length}>
        <ul class="divide-y divide-border/60">
          <For each={list()}>
            {(p) => (
              <li class="flex items-center gap-2 py-2">
                <div class="flex-1 min-w-0">
                  <div class="text-sm text-text truncate">{p.name}</div>
                  <div class="text-xs text-text-muted truncate">
                    {p.model} · {p.has_key ? t("con clave") : t("sin clave")}
                    {p.vision ? ` · ${t("ve imágenes")}` : ""}
                  </div>
                </div>
                <Button size="sm" variant="ghost" disabled={busy()} onClick={() => void test(p.id)}>
                  {t("Probar")}
                </Button>
                <Button size="sm" variant="ghost" disabled={busy()} onClick={() => edit(p)}>
                  {t("Editar")}
                </Button>
              </li>
            )}
          </For>
        </ul>
      </Show>

      <Show
        when={form()}
        fallback={
          <Button size="sm" variant="primary" onClick={() => edit()}>
            {t("Agregar un proveedor")}
          </Button>
        }
      >
        {(f) => (
          <div class="space-y-2 rounded border border-border p-3">
            <Show when={!f().id}>
              <Select
                label={t("Proveedor")}
                options={PRESET_OPTIONS()}
                value={f().preset}
                onChange={(preset) => setForm({ ...blank(preset), api_key: f().api_key })}
              />
            </Show>
            <Field label={t("Nombre")} value={f().name} onInput={(name) => set({ name })} />
            <Field label={t("Dirección de la API")} value={f().base_url} placeholder="https://…/v1" onInput={(base_url) => set({ base_url })} />
            <Show when={PRESETS[f().preset]?.key !== false}>
              <Field
                label={t("Clave de la API")}
                type="password"
                value={f().api_key}
                placeholder={f().has_key ? t("guardada (escribe otra para cambiarla)") : "sk-…"}
                onInput={(api_key) => set({ api_key })}
              />
            </Show>
            <Field label={t("Modelo")} value={f().model} placeholder={PRESETS[f().preset]?.hint} onInput={(model) => set({ model })} />
            <Checkbox label={t("El modelo ve imágenes")} checked={f().vision} onChange={(vision) => set({ vision })} small />
            <div class="flex gap-2 pt-1">
              <Button size="sm" variant="primary" disabled={busy() || !f().model.trim() || !f().base_url.trim()} onClick={() => void saveForm()}>
                {t("Guardar")}
              </Button>
              <Button size="sm" variant="ghost" disabled={busy()} onClick={() => setForm(undefined)}>
                {t("Cancelar")}
              </Button>
              <Show when={f().id}>
                <span class="flex-1" />
                <Button size="sm" variant="ghost" disabled={busy()} onClick={() => void remove(f().id!)}>
                  {t("Borrar")}
                </Button>
              </Show>
            </div>
          </div>
        )}
      </Show>

      <Show when={message()}>{(m) => <p class={`text-xs ${m().error ? "text-error" : "text-text-muted"}`}>{m().text}</p>}</Show>
    </div>
  );
};
