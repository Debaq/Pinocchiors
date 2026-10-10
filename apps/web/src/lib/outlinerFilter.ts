// Texto del buscador del Outliner: lo usan el árbol de la escena y el de
// operaciones de Diseñar (que va como contenido propio dentro del Outliner).

import { createSignal } from "solid-js";

const [outlinerQuery, setOutlinerQuery] = createSignal("");
export { outlinerQuery, setOutlinerQuery };

/** Sin tildes ni mayúsculas, para comparar */
const fold = (s: string) => s.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();

/** ¿El nombre coincide con lo buscado? (sin búsqueda, todo) */
export function matchesOutliner(label: string, query = outlinerQuery()): boolean {
  const q = fold(query.trim());
  return !q || fold(label).includes(q);
}
