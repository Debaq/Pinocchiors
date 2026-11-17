// Combinaciones de modelos para el chat de IA (apps/PLAN_MODELOS.md, M2): cada
// rol —Planifica, Construye, Revisa— con un modelo de cualquier empresa (la
// cuenta de Claude Code o un proveedor con API). Una combinación con solo
// «Construye» es el chat de siempre con ese modelo.

import { createSignal } from "solid-js";
import { createPersisted } from "./ui-state";
import { t } from "./i18n";

/** Un modelo de una empresa: `company` = "claude" (Claude Code) o el id de un proveedor; `model` "" = el de siempre */
export interface ModelRef {
  company: string;
  model: string;
}

export interface AiTeam {
  id: string;
  name: string;
  /** Arma el plan y hace las preguntas (sin él, lo hace quien construye) */
  plan: ModelRef | null;
  /** Hace cada paso */
  build: ModelRef;
  /** Mira el resultado sin cambiar nada y aprueba o pide correcciones (sin él, no se revisa) */
  review: ModelRef | null;
  /** Correcciones como máximo por paso antes de preguntarle al usuario */
  maxFixes: number;
}

/** Lo que el chat necesita saber de cada empresa (`ProviderView` de AiProviders) */
export interface CompanyInfo {
  id: string;
  name: string;
  model: string;
  models: { id: string; vision: boolean; price_in?: number | null; price_out?: number | null }[];
}

/** Los modelos de Claude Code (alias que entiende `claude --model`) */
export const CLAUDE_MODELS = ["fable", "opus", "sonnet", "haiku"] as const;
const capital = (s: string) => (s ? s[0].toUpperCase() + s.slice(1) : s);

const [teamsValue, saveTeams] = createPersisted<AiTeam[]>("ai.teams", []);
export const teams = teamsValue;
export const setTeams = (list: AiTeam[]) => saveTeams(list);

/** Las empresas con API guardadas (las carga el chat y Configuración) */
const [companies, setCompanies] = createSignal<CompanyInfo[]>([]);
export { companies, setCompanies };

export const refKey = (r: ModelRef) => `${r.company}|${r.model}`;
export const parseRef = (key: string): ModelRef => {
  const i = key.indexOf("|");
  return i < 0 ? { company: key, model: "" } : { company: key.slice(0, i), model: key.slice(i + 1) };
};

/** «Anthropic · Opus», «DeepSeek · deepseek-chat» */
export function refLabel(r: ModelRef | null | undefined): string {
  if (!r) return t("— (nadie)");
  if (r.company === "claude") return `Claude Code · ${r.model ? capital(r.model) : t("el de siempre")}`;
  const c = companies().find((x) => x.id === r.company);
  if (!c) return t("Proveedor borrado");
  return `${c.name} · ${r.model || c.model}`;
}

/** Todos los modelos que se pueden elegir para un rol, por empresa */
export function modelChoices(): { value: string; label: string }[] {
  const out = [{ value: "claude|", label: `Claude Code · ${t("el de siempre")}` }, ...CLAUDE_MODELS.map((m) => ({ value: `claude|${m}`, label: `Claude Code · ${capital(m)}` }))];
  for (const c of companies()) {
    const ids = c.models.length ? c.models.map((m) => m.id) : [c.model];
    for (const id of ids) if (id) out.push({ value: `${c.id}|${id}`, label: `${c.name} · ${id}` });
  }
  return out;
}

/** El modelo existe todavía (la empresa no se borró) */
export const refAvailable = (r: ModelRef) => r.company === "claude" || companies().some((c) => c.id === r.company);

/** Precio por millón de tokens (entrada, salida) de un modelo con API, si se anotó */
export function refPrice(r: ModelRef): { in: number; out: number } | undefined {
  const c = companies().find((x) => x.id === r.company);
  const m = c?.models.find((x) => x.id === (r.model || c.model));
  return m && m.price_in != null && m.price_out != null ? { in: m.price_in, out: m.price_out } : undefined;
}

export const newTeamId = () => `t${Date.now().toString(36)}`;

/**
 * Lo elegido en el chat: «team:<id>» (una combinación) o «model:<empresa>|<modelo>»
 * (un solo modelo, el chat de siempre). Se recuerda.
 */
const [choiceValue, setChoiceValue] = createPersisted("chat.choice", "");
export const chatChoice = choiceValue;
export const setChatChoice = setChoiceValue;

/** La combinación elegida en el chat (un modelo suelto es una de un solo rol) */
export function activeTeam(): AiTeam {
  const c = chatChoice();
  if (c.startsWith("team:")) {
    const team = teams().find((x) => x.id === c.slice(5));
    if (team) return team;
  }
  const ref = c.startsWith("model:") ? parseRef(c.slice(6)) : { company: "claude", model: "" };
  return { id: "", name: refLabel(ref), plan: null, build: ref, review: null, maxFixes: 0 };
}

/** Lo que se puede elegir en el chat: las combinaciones y cada modelo suelto */
export function chatChoices(): { value: string; label: string }[] {
  return [...teams().map((x) => ({ value: `team:${x.id}`, label: `${x.name} (${t("combinación")})` })), ...modelChoices().map((o) => ({ value: `model:${o.value}`, label: o.label }))];
}
