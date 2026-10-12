/**
 * Mensajes del backend (Rust): llegan en español, a veces con valores dentro
 * ("Acción desconocida: abc"). `tBackend` los lleva al idioma activo: primero
 * por el texto exacto y si no por plantillas (`{valor}` en las claves del
 * catálogo; `{}` o `{e:?}` también valen). Si nada calza, queda el original.
 */

import { locale, t } from "./i18n.ts";
import { en } from "./i18n/en";

interface Pattern {
  regex: RegExp;
  /** Nombre (o posición) de cada hueco de la clave, en orden */
  slots: string[];
  /** Cantidad de texto fijo: gana la plantilla más específica */
  literal: number;
  to: string;
}

const HOLE = /\{([^{}]*)\}/g;

/** Plantillas de un catálogo: las claves con `{…}`, de la más específica a la más general */
function buildPatterns(catalog: Record<string, string>): Pattern[] {
  const out: Pattern[] = [];
  for (const [es, to] of Object.entries(catalog)) {
    const slots: string[] = [];
    let literal = 0;
    let last = 0;
    let source = "";
    const escape = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    for (const m of es.matchAll(HOLE)) {
      const piece = es.slice(last, m.index);
      literal += piece.length;
      source += escape(piece) + "([\\s\\S]*?)";
      slots.push(m[1].split(":")[0] || String(slots.length));
      last = m.index + m[0].length;
    }
    if (slots.length === 0) continue;
    const tail = es.slice(last);
    literal += tail.length;
    source += escape(tail);
    out.push({ regex: new RegExp(`^${source}$`), slots, literal, to });
  }
  return out.sort((a, b) => b.literal - a.literal);
}

let cache: Pattern[] | undefined;
const patterns = () => (cache ??= buildPatterns(en));

/** Traduce un mensaje del backend al idioma activo */
export function tBackend(message: string): string {
  const exact = t(message);
  if (exact !== message || locale() !== "en") return exact;
  for (const p of patterns()) {
    const m = p.regex.exec(message);
    if (!m) continue;
    const values = new Map(p.slots.map((slot, i) => [slot, tBackend(m[i + 1])]));
    let position = 0;
    return p.to.replace(HOLE, (whole, name: string) => {
      const key = name.split(":")[0];
      if (key === "") return values.get(String(position++)) ?? whole;
      return values.get(key) ?? whole;
    });
  }
  return message;
}
