// Tarjetas del panel de chat: el plan del proyecto (objetivo y pasos, lo
// edita el usuario y la IA con `plan`) y la encuesta que manda la IA con
// `ask_user` (cada pregunta se contesta antes de enviar).

import { Component, For, Index, Show, createMemo, createSignal } from "solid-js";
import { createStore } from "solid-js/store";
import { clsx } from "clsx";
import * as Icons from "../icons";
import { Button, Checkbox, IconButton } from "../ui";
import { t } from "../../lib/i18n";
import { createPersisted } from "../../lib/ui-state";
import { copyPlan, finishSurvey, plan, planIsEmpty, setPlan, survey, type AiPlan, type Survey } from "../../lib/aiAssistant";

/** Cambia el plan sobre una copia */
const editPlan = (change: (p: AiPlan) => void) => {
  const next: AiPlan = copyPlan();
  change(next);
  setPlan(next);
};

/** El plan del proyecto, arriba del chat: plegable, con el avance a la vista */
export const PlanCard: Component = () => {
  const [open, setOpen] = createPersisted("chat.planOpen", true);
  const [editingGoal, setEditingGoal] = createSignal(false);
  const [editingStep, setEditingStep] = createSignal<number>();
  const [editingNotes, setEditingNotes] = createSignal(false);
  const [newStep, setNewStep] = createSignal("");
  const done = () => plan().steps.filter((s) => s.done).length;
  const addStep = () => {
    const text = newStep().trim();
    if (!text) return;
    editPlan((p) => p.steps.push({ text, done: false }));
    setNewStep("");
  };
  const move = (i: number, by: number) =>
    editPlan((p) => {
      const j = i + by;
      if (j < 0 || j >= p.steps.length) return;
      [p.steps[i], p.steps[j]] = [p.steps[j], p.steps[i]];
    });
  // Los atajos de la app no se disparan mientras se escribe
  const keys = (e: KeyboardEvent, enter: () => void, escape: () => void) => {
    e.stopPropagation();
    if (e.key === "Enter") {
      e.preventDefault();
      enter();
    } else if (e.key === "Escape") escape();
  };
  return (
    <div data-ai-plan class="shrink-0 border-b border-border text-xs">
      <button type="button" class="w-full flex items-center gap-1.5 px-3 h-7 hover:bg-surface/30" onClick={() => setOpen(!open())} aria-expanded={open()}>
        <Show when={open()} fallback={<Icons.CaretRight size={10} />}>
          <Icons.CaretDown size={10} />
        </Show>
        <span class="font-semibold uppercase tracking-wide text-[10px] text-text-muted">{t("Plan")}</span>
        <span class="flex-1 truncate text-left text-text">{plan().goal || (planIsEmpty() ? t("Sin plan todavía") : "")}</span>
        <Show when={plan().steps.length}>
          <span class={clsx("font-mono text-[10px]", done() === plan().steps.length ? "text-green" : "text-text-muted")}>
            {done()}/{plan().steps.length}
          </span>
        </Show>
      </button>
      <Show when={open()}>
        <div class="px-3 pb-2 space-y-1.5 max-h-72 overflow-y-auto">
          {/* Objetivo */}
          <Show
            when={editingGoal()}
            fallback={
              <p
                class={clsx("leading-snug cursor-text rounded px-1 -mx-1 hover:bg-surface/40", plan().goal ? "text-text" : "text-text-dim italic")}
                title={t("Clic para editar el objetivo")}
                onClick={() => setEditingGoal(true)}
              >
                {plan().goal || t("Objetivo: qué quieres lograr (la IA también lo puede escribir)")}
              </p>
            }
          >
            <textarea
              rows={2}
              class="w-full resize-none rounded border border-accent bg-surface/40 px-1.5 py-1 text-xs text-text outline-none"
              value={plan().goal}
              ref={(el) => queueMicrotask(() => el.focus())}
              onKeyDown={(e) => keys(e, () => e.currentTarget.blur(), () => setEditingGoal(false))}
              onBlur={(e) => {
                const goal = e.currentTarget.value.trim();
                if (goal !== plan().goal) editPlan((p) => (p.goal = goal));
                setEditingGoal(false);
              }}
            />
          </Show>
          {/* Pasos */}
          <Index each={plan().steps}>
            {(step, i) => (
              <div class="group flex items-start gap-1.5">
                <Checkbox small checked={step().done} onChange={(v) => editPlan((p) => (p.steps[i].done = v))} />
                <Show
                  when={editingStep() === i}
                  fallback={
                    <span
                      class={clsx("flex-1 leading-snug cursor-text", step().done ? "line-through text-text-dim" : "text-text")}
                      onDblClick={() => setEditingStep(i)}
                      title={t("Doble clic para editar")}
                    >
                      <span class="text-text-dim font-mono mr-1">{i + 1}.</span>
                      {step().text}
                    </span>
                  }
                >
                  <input
                    class="flex-1 min-w-0 rounded border border-accent bg-surface/40 px-1 text-xs text-text outline-none"
                    value={step().text}
                    ref={(el) => queueMicrotask(() => el.focus())}
                    onKeyDown={(e) => keys(e, () => e.currentTarget.blur(), () => setEditingStep(undefined))}
                    onBlur={(e) => {
                      const text = e.currentTarget.value.trim();
                      if (text && text !== step().text) editPlan((p) => (p.steps[i].text = text));
                      setEditingStep(undefined);
                    }}
                  />
                </Show>
                <span class="flex shrink-0 opacity-0 group-hover:opacity-100 transition-opacity">
                  <IconButton variant="ghost" size="sm" aria-label={t("Subir")} onClick={() => move(i, -1)}>
                    <Icons.CaretDown size={10} class="rotate-180" />
                  </IconButton>
                  <IconButton variant="ghost" size="sm" aria-label={t("Bajar")} onClick={() => move(i, 1)}>
                    <Icons.CaretDown size={10} />
                  </IconButton>
                  <IconButton variant="ghost" size="sm" aria-label={t("Quitar el paso")} onClick={() => editPlan((p) => p.steps.splice(i, 1))}>
                    <Icons.X size={10} />
                  </IconButton>
                </span>
              </div>
            )}
          </Index>
          <div class="flex items-center gap-1">
            <input
              class="flex-1 min-w-0 rounded border border-border bg-surface/40 px-1.5 py-0.5 text-xs text-text outline-none focus:border-accent"
              placeholder={t("Agregar un paso… (Enter)")}
              value={newStep()}
              onInput={(e) => setNewStep(e.currentTarget.value)}
              onKeyDown={(e) => keys(e, addStep, () => setNewStep(""))}
            />
            <Show when={!planIsEmpty()}>
              <IconButton variant="ghost" size="sm" aria-label={t("Borrar el plan")} title={t("Borrar el plan, las notas y lo que pediste")} onClick={() => setPlan({ goal: "", steps: [] })}>
                <Icons.Trash size={11} />
              </IconButton>
            </Show>
          </div>
          {/* Cómo se hace: la libreta de la IA (y del usuario). Va con cada mensaje */}
          <div class="pt-1 space-y-0.5">
            <p class="text-[10px] font-semibold uppercase tracking-wide text-text-muted">{t("Cómo se hace")}</p>
            <Show
              when={editingNotes()}
              fallback={
                <p
                  class={clsx("whitespace-pre-wrap leading-snug cursor-text rounded px-1 -mx-1 hover:bg-surface/40", plan().notes?.trim() ? "text-text" : "text-text-dim italic")}
                  title={t("Clic para editar")}
                  onClick={() => setEditingNotes(true)}
                >
                  {plan().notes?.trim() || t("Decisiones, medidas y reglas del trabajo. La IA las anota aquí y las recuerda aunque limpies o resumas el chat")}
                </p>
              }
            >
              <textarea
                rows={4}
                class="w-full resize-y rounded border border-accent bg-surface/40 px-1.5 py-1 text-xs text-text outline-none"
                value={plan().notes ?? ""}
                ref={(el) => queueMicrotask(() => el.focus())}
                onKeyDown={(e) => {
                  e.stopPropagation();
                  if (e.key === "Escape") setEditingNotes(false);
                }}
                onBlur={(e) => {
                  const notes = e.currentTarget.value.trim();
                  if (notes !== (plan().notes ?? "")) editPlan((p) => (p.notes = notes));
                  setEditingNotes(false);
                }}
              />
            </Show>
          </div>
          {/* Lo que pidió el usuario: se anota solo y va con cada mensaje */}
          <Show when={plan().asks?.length}>
            <div class="pt-1 space-y-0.5">
              <p class="text-[10px] font-semibold uppercase tracking-wide text-text-muted">{t("Lo que pediste (la IA lo recuerda siempre)")}</p>
              <Index each={plan().asks}>
                {(ask, i) => (
                  <div class="group flex items-start gap-1">
                    <span class="flex-1 leading-snug text-text-muted line-clamp-3" title={ask()}>
                      <span class="text-text-dim font-mono mr-1">{i + 1}.</span>
                      {ask()}
                    </span>
                    <IconButton
                      variant="ghost"
                      size="sm"
                      class="shrink-0 opacity-0 group-hover:opacity-100 transition-opacity"
                      aria-label={t("Olvidar este pedido")}
                      title={t("Olvidar este pedido")}
                      onClick={() => editPlan((p) => p.asks?.splice(i, 1))}
                    >
                      <Icons.X size={10} />
                    </IconButton>
                  </div>
                )}
              </Index>
            </div>
          </Show>
        </div>
      </Show>
    </div>
  );
};

/** Respuesta a una pregunta mientras se contesta */
interface Draft {
  picked: string[];
  other: string;
}

/** La encuesta de la IA: cada pregunta se contesta antes de enviar */
export const SurveyCard: Component = () => (
  <Show when={survey()} keyed>
    {(s) => <SurveyForm survey={s} />}
  </Show>
);

const SurveyForm: Component<{ survey: Survey }> = (props) => {
  const [drafts, setDrafts] = createStore<Draft[]>(props.survey.questions.map(() => ({ picked: [], other: "" })));
  const answerOf = (i: number): string | string[] | undefined => {
    const q = props.survey.questions[i];
    const d = drafts[i];
    const other = d.other.trim();
    if (!q.options?.length) return other || undefined;
    if (q.multiple) {
      const all = [...d.picked, ...(other ? [other] : [])];
      return all.length ? all : undefined;
    }
    return other || d.picked[0];
  };
  const missing = createMemo(() => props.survey.questions.filter((_, i) => answerOf(i) === undefined).length);
  const send = () => {
    if (missing()) return;
    finishSurvey({ answers: props.survey.questions.map((q, i) => ({ question: q.question, answer: answerOf(i)! })) });
  };
  const pick = (i: number, option: string, multiple: boolean) => {
    if (multiple) {
      const has = drafts[i].picked.includes(option);
      setDrafts(i, "picked", has ? drafts[i].picked.filter((x) => x !== option) : [...drafts[i].picked, option]);
    } else {
      setDrafts(i, { picked: [option], other: "" });
    }
  };
  return (
    <div data-ai-survey class="shrink-0 border-t-2 border-accent bg-accent/5 max-h-[60%] flex flex-col text-xs">
      <div class="flex items-center gap-2 px-3 pt-2">
        <Icons.Info size={14} class="text-accent shrink-0" />
        <span class="flex-1 font-semibold text-text truncate">{props.survey.title || t("La IA necesita tus respuestas")}</span>
        <span class="text-[10px] text-text-muted">
          {missing() ? t("Faltan {n}", { n: missing() }) : t("Todo respondido")}
        </span>
      </div>
      <div class="flex-1 overflow-y-auto px-3 py-2 space-y-3">
        <For each={props.survey.questions}>
          {(q, i) => {
            const answered = () => answerOf(i()) !== undefined;
            return (
              <fieldset class="space-y-1">
                <legend class="flex gap-1.5 leading-snug text-text">
                  <span class={clsx("font-mono shrink-0", answered() ? "text-green" : "text-accent")}>{answered() ? "✓" : `${i() + 1}.`}</span>
                  <span>{q.question}</span>
                </legend>
                <Show when={q.options?.length}>
                  <div class="space-y-0.5 pl-4">
                    <For each={q.options}>
                      {(o) => (
                        <label class="flex items-center gap-1.5 cursor-pointer rounded px-1 hover:bg-surface/40">
                          <input
                            type={q.multiple ? "checkbox" : "radio"}
                            name={`survey-${i()}`}
                            class="accent-[var(--color-accent)]"
                            checked={drafts[i()].picked.includes(o) && (q.multiple || !drafts[i()].other.trim())}
                            onChange={() => pick(i(), o, !!q.multiple)}
                          />
                          <span class="text-text">{o}</span>
                        </label>
                      )}
                    </For>
                  </div>
                </Show>
                <Show when={!q.options?.length || q.other !== false}>
                  <textarea
                    rows={q.options?.length ? 1 : 2}
                    class="w-full resize-none rounded border border-border bg-surface/40 px-1.5 py-1 text-xs text-text outline-none focus:border-accent ml-4 max-w-[calc(100%-1rem)]"
                    placeholder={q.options?.length ? t("Otra respuesta…") : t("Tu respuesta…")}
                    value={drafts[i()].other}
                    onInput={(e) => setDrafts(i(), "other", e.currentTarget.value)}
                    onKeyDown={(e) => {
                      e.stopPropagation();
                      if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
                        e.preventDefault();
                        send();
                      }
                    }}
                  />
                </Show>
              </fieldset>
            );
          }}
        </For>
      </div>
      <div class="flex items-center gap-2 px-3 pb-2">
        <Button size="sm" variant="ghost" onClick={() => finishSurvey({ skipped: true })}>
          {t("Cerrar sin responder")}
        </Button>
        <span class="flex-1" />
        <Button size="sm" variant="primary" disabled={missing() > 0} onClick={send} title={missing() ? t("Responde todas las preguntas para enviar") : undefined}>
          {t("Enviar respuestas")}
        </Button>
      </div>
    </div>
  );
};
