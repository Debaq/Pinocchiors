// Búsqueda y etiquetas de la biblioteca de poses (sin dependencias, para probar con node)
import type { StoredPose } from "./poseTools.ts";

/** Etiquetas de un texto separado por comas: sin vacías ni repetidas */
export function parseTags(text: string): string[] {
  const seen = new Set<string>();
  for (const raw of text.split(",")) {
    const tag = raw.trim();
    if (tag) seen.add(tag);
  }
  return [...seen];
}

/** ¿La pose coincide con la búsqueda (en su nombre, categoría o etiquetas) y la categoría elegida? */
export function poseMatches(pose: StoredPose, query: string, category: string | null): boolean {
  if (category !== null && (pose.category ?? "") !== category) return false;
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return [pose.name, pose.category ?? "", ...(pose.tags ?? [])].some((s) => s.toLowerCase().includes(q));
}
