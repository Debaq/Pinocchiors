/**
 * Plantillas de esqueleto: cuántas veces se eligió cada una (para mostrar
 * primero las más usadas), búsqueda sin tildes y los grupos de la lista.
 */

import { createPersisted } from "./ui-state";

const [usage, setUsage] = createPersisted<Record<string, number>>("templates.usage", {});

/** Veces que se eligió cada plantilla */
export const templateUsage = usage;

/** Suma un uso a la plantilla `id` */
export function recordTemplateUse(id: string): void {
  const current = usage();
  setUsage({ ...current, [id]: (current[id] ?? 0) + 1 });
}

/** Las más comunes, para ordenar mientras no hay uso propio */
const POPULAR = [
  "plan:human",
  "plan:dog",
  "plan:cat",
  "plan:horse",
  "plan:bird_walker",
  "plan:fish",
  "plan:dragon",
  "plan:spider",
  "plan:snake",
  "plan:bear",
  "plan:cow",
  "plan:rabbit",
  "plan:eagle",
  "plan:trex",
  "plan:lizard",
  "plan:octopus",
  "plan:elephant",
  "plan:deer",
  "plan:insect",
  "plan:gorilla",
];

/** Ordena por uso (más usadas primero); a igual uso, las más comunes y luego el orden original */
export function byUse<T extends { id: string }>(items: T[]): T[] {
  const uses = usage();
  const rank = (id: string) => {
    const i = POPULAR.indexOf(id);
    return i < 0 ? POPULAR.length : i;
  };
  return items
    .map((item, index) => ({ item, index }))
    .sort((a, b) => (uses[b.item.id] ?? 0) - (uses[a.item.id] ?? 0) || rank(a.item.id) - rank(b.item.id) || a.index - b.index)
    .map(({ item }) => item);
}

/** Hay plantillas elegidas alguna vez */
export const hasUsage = () => Object.keys(usage()).length > 0;

/** Minúsculas y sin tildes: "arana" encuentra "Araña" */
export const normalize = (text: string) => text.normalize("NFD").replace(/\p{Diacritic}/gu, "").toLowerCase();

/** Cada palabra buscada aparece en el nombre o la descripción */
export function matches(query: string, item: { name: string; description: string }): boolean {
  const words = normalize(query).split(/\s+/).filter(Boolean);
  const text = normalize(`${item.name} ${item.description}`);
  return words.every((w) => text.includes(w));
}

/** Grupos de la lista, en orden */
export const CATEGORIES: { id: string; label: string }[] = [
  { id: "primates", label: "Humanos y primates" },
  { id: "mammals", label: "Mamíferos" },
  { id: "birds", label: "Aves" },
  { id: "reptiles", label: "Reptiles, anfibios y dinosaurios" },
  { id: "aquatic", label: "Peces y animales marinos" },
  { id: "invertebrates", label: "Insectos, arañas y crustáceos" },
  { id: "fantasy", label: "Criaturas fantásticas" },
  { id: "other", label: "Otros" },
];
