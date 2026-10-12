// Valor escrito en el campo flotante de mover, copiar, girar y escalar
// (después del punto base o del centro). Funciones puras: se prueban con node
// (e2e/sketchTyped.test.mjs).

import type { P2 } from "./cad.ts";
import { t as tr } from "./i18n.ts";

/** Un número con coma o punto decimal */
function num(text: string): number | undefined {
  const t = text.trim().replace(",", ".");
  if (!/^[-+]?(\d+\.?\d*|\.\d+)(e[-+]?\d+)?$/i.test(t)) return undefined;
  return Number(t);
}

export type TypedTransform = { delta: P2 } | { degrees: number } | { factor: number };

/** Lo que se pide en cada herramienta */
export const TYPED_HINT: Record<"move" | "copy" | "rotate" | "scale", string> = {
  get move() { return tr("distancia · dx; dy · d<ángulo"); },
  get copy() { return tr("distancia · dx; dy · d<ángulo"); },
  get rotate() { return tr("ángulo °"); },
  get scale() { return tr("factor"); },
};

/**
 * Lee el valor escrito. Mover y copiar: una distancia sola va hacia el cursor
 * (`dir`, desde el punto base), `dx; dy` (o `dx, dy` con espacio, o
 * `dx dy`) corre en x e y, y `d<ángulo` en esa dirección. Girar: grados
 * antihorario. Escalar: el factor. Devuelve un mensaje si no se entiende.
 */
export function parseTypedTransform(tool: "move" | "copy" | "rotate" | "scale", text: string, dir?: P2): TypedTransform | string {
  const t = text.trim();
  if (!t) return tr("Escribir {hint}", { hint: TYPED_HINT[tool] });
  if (tool === "rotate") {
    const a = num(t.replace(/°$/, ""));
    return a === undefined ? tr("El ángulo en grados (por ejemplo 90 o -45)") : { degrees: a };
  }
  if (tool === "scale") {
    const k = num(t.replace(/^[x×]/i, ""));
    return k === undefined || !(k > 0) ? tr("El factor tiene que ser un número mayor que 0") : { factor: k };
  }
  if (t.includes("<")) {
    const [d, a] = t.split("<").map(num);
    if (d === undefined || a === undefined) return tr("Polar: distancia<ángulo (por ejemplo 20<45)");
    const r = (a * Math.PI) / 180;
    return { delta: [d * Math.cos(r), d * Math.sin(r)] };
  }
  // Dos números: con «;», con «, » o con espacio; «2,5» solo es un número con coma decimal
  const parts = t.includes(";") ? t.split(";") : /,\s/.test(t) ? t.split(",") : /\s/.test(t) ? t.split(/\s+/) : [t];
  if (parts.length === 2) {
    const [x, y] = parts.map(num);
    return x === undefined || y === undefined ? tr("Corrimiento: dx; dy (por ejemplo 10; -5)") : { delta: [x, y] };
  }
  const d = num(t);
  if (d === undefined || parts.length > 2) return tr("Escribir {hint}", { hint: TYPED_HINT[tool] });
  const len = dir ? Math.hypot(dir[0], dir[1]) : 0;
  const u: P2 = len > 1e-12 ? [dir![0] / len, dir![1] / len] : [1, 0];
  return { delta: [u[0] * d, u[1] * d] };
}
