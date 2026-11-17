// Lo que la IA comparte con el usuario en el panel de chat, además de los
// mensajes: el plan del proyecto (objetivo y pasos, guardado en el proyecto)
// y las encuestas (preguntas que el usuario contesta una por una). La IA los
// usa con las herramientas `plan` y `ask_user` del conector (ver mcpLive.ts).

import { createSignal } from "solid-js";

export interface PlanStep {
  text: string;
  done: boolean;
}
export interface AiPlan {
  goal: string;
  steps: PlanStep[];
}
const EMPTY_PLAN: AiPlan = { goal: "", steps: [] };

const [plan, setPlanSignal] = createSignal<AiPlan>(EMPTY_PLAN);
/** Sube con cada cambio del plan (el proyecto queda con cambios sin guardar) */
const [planRevision, setPlanRevision] = createSignal(0);
export { plan, planRevision };

export const setPlan = (next: AiPlan) => {
  setPlanSignal({ goal: next.goal, steps: next.steps.map((s) => ({ text: s.text, done: s.done })) });
  setPlanRevision((n) => n + 1);
};
export const planIsEmpty = (p: AiPlan = plan()) => !p.goal.trim() && p.steps.length === 0;

/** El plan para el proyecto (nada si está vacío) */
export const planForProject = (): AiPlan | undefined => (planIsEmpty() ? undefined : plan());

/** El plan de otro proyecto (sin nada, uno vacío) */
export function restorePlan(raw: unknown) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const r: any = raw && typeof raw === "object" ? raw : {};
  const steps = Array.isArray(r.steps)
    ? r.steps.filter((s: unknown) => s && typeof (s as PlanStep).text === "string").map((s: PlanStep) => ({ text: s.text, done: !!s.done }))
    : [];
  setPlanSignal({ goal: typeof r.goal === "string" ? r.goal : "", steps });
}

/**
 * Lo que pide la IA con `plan` (ver `mcp/tools/assistant.rs`), en este orden:
 * clear, goal, steps, edit, done, undone, remove, insert, add. Los números
 * son los del plan antes del pedido (desde 1).
 */
export function applyPlanChange(args: unknown): AiPlan {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const a: any = args && typeof args === "object" ? args : {};
  const strings = (v: unknown): string[] => (Array.isArray(v) ? v.filter((x): x is string => typeof x === "string").map((x) => x.trim()).filter(Boolean) : []);
  const numbers = (v: unknown): Set<number> => new Set(Array.isArray(v) ? v.filter((x): x is number => Number.isInteger(x)).map((x) => x - 1) : []);
  const changes = ["clear", "goal", "steps", "edit", "done", "undone", "remove", "insert", "add"].some((k) => a[k] !== undefined && a[k] !== false);
  if (!changes) return plan();
  const base: AiPlan = a.clear === true ? { goal: "", steps: [] } : { goal: plan().goal, steps: plan().steps.map((s) => ({ ...s })) };
  const goal = typeof a.goal === "string" ? a.goal.trim() : base.goal;
  // Cada paso con su número de antes: quitar e insertar no corren los demás números
  const items: { step: PlanStep; removed?: boolean }[] = (Array.isArray(a.steps) ? strings(a.steps).map((text) => ({ text, done: false })) : base.steps).map((step) => ({ step }));
  if (a.edit && typeof a.edit === "object" && !Array.isArray(a.edit)) {
    for (const [k, v] of Object.entries(a.edit)) {
      const i = Number(k) - 1;
      if (Number.isInteger(i) && items[i] && typeof v === "string" && v.trim()) items[i].step.text = v.trim();
    }
  }
  for (const i of numbers(a.done)) if (items[i]) items[i].step.done = true;
  for (const i of numbers(a.undone)) if (items[i]) items[i].step.done = false;
  for (const i of numbers(a.remove)) if (items[i]) items[i].removed = true;
  if (a.insert && typeof a.insert === "object" && Number.isInteger(a.insert.after)) {
    const at = Math.max(0, Math.min(items.length, a.insert.after));
    items.splice(at, 0, ...strings(a.insert.steps).map((text) => ({ step: { text, done: false } })));
  }
  const steps = items.filter((x) => !x.removed).map((x) => x.step);
  for (const text of strings(a.add)) steps.push({ text, done: false });
  setPlan({ goal, steps });
  return plan();
}

/** El plan en texto, para darle contexto a la IA con cada mensaje (vacío si no hay) */
export function planSummary(): string {
  const p = plan();
  if (planIsEmpty(p)) return "";
  const lines = [`Goal: ${p.goal.trim() || "(none)"}`, ...p.steps.map((s, i) => `${i + 1} [${s.done ? "x" : " "}] ${s.text}`)];
  return lines.join("\n");
}

// ─── Encuestas ──────────────────────────────────────────────────────────────

export interface SurveyQuestion {
  question: string;
  options?: string[];
  multiple?: boolean;
  /** Se puede escribir otra respuesta además de las opciones (por defecto sí) */
  other?: boolean;
}
export interface Survey {
  title?: string;
  questions: SurveyQuestion[];
}
export type SurveyResult = { answers: { question: string; answer: string | string[] }[] } | { skipped: true };

const [survey, setSurvey] = createSignal<Survey>();
export { survey };
let resolveSurvey: ((r: SurveyResult) => void) | undefined;
/** Quien quiere enterarse de cada encuesta terminada (el chat la deja escrita) */
const surveyListeners = new Set<(s: Survey, r: SurveyResult) => void>();
export const onSurveyDone = (listener: (s: Survey, r: SurveyResult) => void) => surveyListeners.add(listener);

/** Muestra la encuesta en el panel de chat y espera las respuestas (una a la vez: la anterior queda sin contestar) */
export function askSurvey(raw: unknown): Promise<SurveyResult> {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const r: any = raw && typeof raw === "object" ? raw : {};
  const questions: SurveyQuestion[] = (Array.isArray(r.questions) ? r.questions : [])
    .filter((q: unknown) => q && typeof (q as SurveyQuestion).question === "string")
    .slice(0, 10)
    .map((q: SurveyQuestion) => ({
      question: q.question,
      options: Array.isArray(q.options) ? q.options.filter((o) => typeof o === "string") : undefined,
      multiple: !!q.multiple,
      other: q.other !== false,
    }));
  resolveSurvey?.({ skipped: true });
  return new Promise((resolve) => {
    resolveSurvey = resolve;
    setSurvey({ title: typeof r.title === "string" ? r.title : undefined, questions });
  });
}

/** El usuario contestó (o cerró la encuesta) */
export function finishSurvey(result: SurveyResult) {
  const done = resolveSurvey;
  const asked = survey();
  resolveSurvey = undefined;
  setSurvey(undefined);
  if (asked) for (const l of surveyListeners) l(asked, result);
  done?.(result);
}
