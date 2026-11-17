// Configuración ▸ Conector para IA ▸ Combinaciones de modelos: qué modelo
// planifica, cuál construye y cuál revisa (apps/PLAN_MODELOS.md, M2).

import { Component, For, Show, createSignal, onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { Button, Select } from "../ui";
import { t } from "../../lib/i18n";
import {
  type AiTeam,
  type CompanyInfo,
  chatChoice,
  modelChoices,
  newTeamId,
  parseRef,
  refAvailable,
  refKey,
  refLabel,
  setChatChoice,
  setCompanies,
  setTeams,
  teams,
} from "../../lib/aiTeams";

const NOBODY = "";
const FIX_OPTIONS = () => [0, 1, 2, 3, 4].map((n) => ({ value: String(n), label: n === 0 ? t("Ninguna (solo avisa)") : t("Hasta {n}", { n }) }));

/** Para empezar: revisa y planifica el potente, construye el económico */
const starter = (): AiTeam => ({
  id: newTeamId(),
  name: t("Calidad"),
  plan: { company: "claude", model: "opus" },
  build: { company: "claude", model: "sonnet" },
  review: { company: "claude", model: "opus" },
  maxFixes: 2,
});

export const AiTeams: Component = () => {
  const [form, setForm] = createSignal<AiTeam>();
  onMount(() => void invoke<CompanyInfo[]>("ai_providers").then(setCompanies).catch(() => {}));

  const save = () => {
    const f = form();
    if (!f) return;
    const team = { ...f, name: f.name.trim() || t("Combinación") };
    const list = teams();
    setTeams(list.some((x) => x.id === team.id) ? list.map((x) => (x.id === team.id ? team : x)) : [...list, team]);
    setForm(undefined);
  };
  const remove = (id: string) => {
    setTeams(teams().filter((x) => x.id !== id));
    if (chatChoice() === `team:${id}`) setChatChoice("model:claude|");
    setForm(undefined);
  };
  const set = (patch: Partial<AiTeam>) => setForm((f) => (f ? { ...f, ...patch } : f));
  const roleOptions = (optional: boolean) => [...(optional ? [{ value: NOBODY, label: t("— (nadie)") }] : []), ...modelChoices()];
  const missing = (x: AiTeam) => [x.plan, x.build, x.review].some((r) => r && !refAvailable(r));

  return (
    <div class="space-y-3">
      <p class="text-xs text-text-muted leading-relaxed">
        {t(
          "Cada rol puede ser un modelo de otra empresa. Planifica arma el plan y hace las preguntas; Construye hace cada paso; Revisa mira el resultado sin cambiar nada y aprueba o pide correcciones. Conviene un modelo potente para planificar y revisar, y uno económico para construir. En el chat se elige en «Usar».",
        )}
      </p>
      <Show when={teams().length}>
        <ul class="divide-y divide-border/60">
          <For each={teams()}>
            {(x) => (
              <li class="flex items-center gap-2 py-2">
                <div class="flex-1 min-w-0">
                  <div class="text-sm text-text truncate">
                    {x.name}
                    <Show when={chatChoice() === `team:${x.id}`}>
                      <span class="ml-1.5 text-[10px] text-accent">{t("en uso")}</span>
                    </Show>
                  </div>
                  <div class={`text-xs truncate ${missing(x) ? "text-error" : "text-text-muted"}`}>
                    {t("Planifica: {p} · Construye: {b} · Revisa: {r}", {
                      p: x.plan ? refLabel(x.plan) : t("quien construye"),
                      b: refLabel(x.build),
                      r: refLabel(x.review),
                    })}
                  </div>
                </div>
                <Button size="sm" variant="ghost" onClick={() => setChatChoice(`team:${x.id}`)}>
                  {t("Usar")}
                </Button>
                <Button size="sm" variant="ghost" onClick={() => setForm({ ...x })}>
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
          <Button size="sm" variant="primary" onClick={() => setForm(starter())}>
            {t("Agregar una combinación")}
          </Button>
        }
      >
        {(f) => (
          <div class="space-y-2 rounded border border-border p-3" data-ai-team-form>
            <label class="block space-y-1">
              <span class="text-xs text-text-muted">{t("Nombre")}</span>
              <input
                class="w-full px-2 py-1 rounded border border-border bg-surface/40 text-sm text-text outline-none focus:border-accent"
                value={f().name}
                onInput={(e) => set({ name: e.currentTarget.value })}
                onKeyDown={(e) => e.stopPropagation()}
              />
            </label>
            <Select
              label={t("Planifica (plan y preguntas)")}
              options={roleOptions(true)}
              value={f().plan ? refKey(f().plan!) : NOBODY}
              onChange={(v) => set({ plan: v ? parseRef(v) : null })}
            />
            <Select label={t("Construye (cada paso)")} options={roleOptions(false)} value={refKey(f().build)} onChange={(v) => set({ build: parseRef(v) })} />
            <Select
              label={t("Revisa (solo mira)")}
              options={roleOptions(true)}
              value={f().review ? refKey(f().review!) : NOBODY}
              onChange={(v) => set({ review: v ? parseRef(v) : null })}
            />
            <Show when={f().review}>
              <Select label={t("Correcciones por paso")} options={FIX_OPTIONS()} value={String(f().maxFixes)} onChange={(v) => set({ maxFixes: Number(v) })} />
              <p class="text-[11px] text-text-dim leading-relaxed">{t("Si después de esas correcciones sigue mal, se detiene y te pregunta.")}</p>
            </Show>
            <div class="flex gap-2 pt-1">
              <Button size="sm" variant="primary" onClick={save}>
                {t("Guardar")}
              </Button>
              <Button size="sm" variant="ghost" onClick={() => setForm(undefined)}>
                {t("Cancelar")}
              </Button>
              <Show when={teams().some((x) => x.id === f().id)}>
                <span class="flex-1" />
                <Button size="sm" variant="ghost" onClick={() => remove(f().id)}>
                  {t("Borrar")}
                </Button>
              </Show>
            </div>
          </div>
        )}
      </Show>
    </div>
  );
};
