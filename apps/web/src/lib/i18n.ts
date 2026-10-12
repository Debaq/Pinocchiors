/**
 * Idiomas de la interfaz. El texto en español del código es la clave: `t("Guardar")`
 * devuelve ese mismo texto en español y, con otro idioma, su traducción de
 * `lib/i18n/<idioma>.ts` (si falta, queda el español). Así el español nunca se
 * rompe por una traducción que falta, y los e2e que buscan texto siguen valiendo.
 *
 * Valores variables con `{nombre}`: `t("Cargando {n} objetos", { n: 3 })`.
 * Todo `t()` lee la señal del idioma: dentro de JSX o de un efecto se actualiza
 * solo al cambiarlo. Fuera (constantes de módulo) usar getters o funciones.
 */

import { createSignal } from "solid-js";
import { createPersisted } from "./ui-state.ts";
import { en } from "./i18n/en.ts";

export type Locale = "es" | "en";
export type Catalog = Record<string, string>;

/** Idiomas disponibles, con su nombre en su propio idioma */
export const LOCALES: { value: Locale; label: string }[] = [
  { value: "es", label: "Español" },
  { value: "en", label: "English" },
];

const CATALOGS: Record<Locale, Catalog | null> = { es: null, en };

const isLocale = (v: unknown): v is Locale => LOCALES.some((l) => l.value === v);

/** "system" = el idioma del sistema operativo */
export type LocaleSetting = Locale | "system";

/** Idioma del sistema si está soportado; si no, inglés */
function systemLocale(): Locale {
  for (const l of typeof navigator === "undefined" ? [] : navigator.languages?.length ? navigator.languages : [navigator.language]) {
    const base = l?.toLowerCase().split("-")[0];
    if (isLocale(base)) return base;
  }
  return "en";
}

const resolve = (s: unknown): Locale => (isLocale(s) ? s : systemLocale());

const [stored, storeLocale] = createPersisted<LocaleSetting>("settings.language", "system");
const [current, setCurrent] = createSignal<Locale>(resolve(stored()));

/** Idioma activo (reactivo) */
export const locale = current;
/** Lo que eligió el usuario (puede ser "system") */
export const localeSetting = stored;

/** Cambia el idioma, lo recuerda y lo avisa al documento (`<html lang>`) */
export function setLocale(next: LocaleSetting): void {
  if (next !== "system" && !isLocale(next)) return;
  storeLocale(next);
  setCurrent(resolve(next));
  document.documentElement.lang = current();
  // Lo dibujado en lienzos (cubo de vistas, rótulos) no es reactivo: se rehace
  // con el mismo aviso del tema. Sin `window` (tests en node) no hay nada que avisar
  window.dispatchEvent?.(new Event("pinocchio-theme"));
}

/** Al arrancar, antes del primer cuadro */
export function applyLocale(): void {
  document.documentElement.lang = current();
}

const fill = (text: string, vars?: Record<string, string | number>): string =>
  vars ? text.replace(/\{(\w+)\}/g, (m, k: string) => (k in vars ? String(vars[k]) : m)) : text;

/** Traduce `es` (el texto en español) al idioma activo */
export function t(es: string, vars?: Record<string, string | number>): string {
  const catalog = CATALOGS[current()];
  return fill(catalog?.[es] ?? es, vars);
}

/**
 * Igual que `t`, para una palabra española con más de un sentido en inglés:
 * busca primero `contexto|es` en el catálogo y si no está, `es` a secas
 */
export function tx(context: string, es: string, vars?: Record<string, string | number>): string {
  const catalog = CATALOGS[current()];
  return fill(catalog?.[`${context}|${es}`] ?? catalog?.[es] ?? es, vars);
}

/** Número con los separadores del idioma activo */
export const fmtNumber = (n: number, opts?: Intl.NumberFormatOptions): string =>
  new Intl.NumberFormat(current(), opts).format(n);

/** Plural: `tn(n, "{n} objeto", "{n} objetos")`; `{n}` se rellena solo */
export function tn(n: number, one: string, many: string): string {
  return t(n === 1 ? one : many, { n });
}
