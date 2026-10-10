// Patrones del sketch que se pueden volver a editar: cada patrón queda
// anotado en `Sketch.patterns` (qué se repitió, cómo y qué copias salieron).
// Rehacerlo borra lo que había armado y lo vuelve a armar con los datos
// nuevos (y con el original como esté ahora). Una copia omitida queda de
// construcción: así no se corta la cadena de restricciones entre copias.
// Funciones puras: se prueban con node (e2e/sketchPatterns.test.mjs).

import {
  circularPattern,
  collectCopies,
  curvePattern,
  linearPattern,
  removeEntity,
  tablePattern,
  type PatternSpec,
  type Sketch,
  type SketchPatternRecord,
} from "./cad.ts";

export type { PatternSpec, SketchPatternRecord };

/** Arma el patrón `spec` de `source` (sin anotarlo) */
function run(s: Sketch, source: number[], spec: PatternSpec): string | undefined {
  switch (spec.kind) {
    case "linear":
      return linearPattern(s, source, spec.count, spec.offset, spec.rows, spec.offset2);
    case "circular":
      return circularPattern(s, source, spec.count, spec.center, spec.span, spec.rotate);
    case "curve":
      return curvePattern(s, [spec.path, ...source], spec.count, spec.path, spec.rotate);
    case "table":
      return tablePattern(s, source, spec.offsets);
  }
}

/** Arma el patrón y lo anota; devuelve un mensaje si no se pudo */
export function makePattern(s: Sketch, source: number[], spec: PatternSpec): string | undefined {
  const src = source.filter((id) => !(spec.kind === "curve" && id === spec.path));
  const before = new Set(s.entities.map((e) => e.id));
  const { result, copies } = collectCopies(() => run(s, src, spec));
  if (result) return result;
  const owned = s.entities.filter((e) => !before.has(e.id)).map((e) => e.id);
  const id = Math.max(0, ...(s.patterns ?? []).map((p) => p.id)) + 1;
  (s.patterns ??= []).push({ id, spec: structuredClone(spec), source: src, owned, copies, skip: [] });
  return undefined;
}

/** El patrón al que pertenece la entidad: `copy` −1 si es del original */
export function patternAt(s: Sketch, entity: number | undefined): { pattern: SketchPatternRecord; copy: number } | undefined {
  if (entity === undefined) return undefined;
  for (const pattern of s.patterns ?? []) {
    if (pattern.source.includes(entity)) return { pattern, copy: -1 };
    const copy = pattern.copies.findIndex((c) => c.includes(entity));
    if (copy >= 0) return { pattern, copy };
    // Las guías del patrón (líneas y puntos de construcción) también lo eligen
    if (pattern.owned.includes(entity)) return { pattern, copy: -1 };
  }
  return undefined;
}

/**
 * Rehace el patrón `id` con `spec` (o con el que tenía): borra lo que había
 * armado y lo vuelve a armar desde el original como está ahora. Las copias
 * omitidas siguen omitidas (las que todavía existen en la cantidad nueva).
 */
export function editPattern(s: Sketch, id: number, spec?: PatternSpec): string | undefined {
  const p = s.patterns?.find((x) => x.id === id);
  if (!p) return "Ese patrón ya no está";
  const source = p.source.filter((e) => s.entities.some((x) => x.id === e));
  if (!source.length) {
    dropPattern(s, id);
    return "El original del patrón ya no está";
  }
  const next = spec ?? p.spec;
  // Probar en una copia: si no se puede, no se toca nada
  const trial: Sketch = structuredClone(s);
  for (const e of p.owned) removeEntity(trial, e);
  const msg = run(trial, source, next);
  if (msg) return msg;
  for (const e of p.owned) removeEntity(s, e);
  const before = new Set(s.entities.map((e) => e.id));
  const { copies } = collectCopies(() => run(s, source, next));
  p.spec = structuredClone(next);
  p.source = source;
  p.owned = s.entities.filter((e) => !before.has(e.id)).map((e) => e.id);
  p.copies = copies;
  p.skip = p.skip.filter((k) => k < copies.length);
  for (const k of p.skip) setSkipped(s, p, k, true);
  return undefined;
}

function setSkipped(s: Sketch, p: SketchPatternRecord, copy: number, on: boolean) {
  p.copies[copy]?.forEach((id, i) => {
    const e = s.entities.find((x) => x.id === id);
    if (!e) return;
    const original = s.entities.find((x) => x.id === p.source[i]);
    if (on || original?.construction) e.construction = true;
    else delete e.construction;
  });
}

/** Omite la copia `copy` del patrón (o la vuelve a poner) */
export function toggleSkip(s: Sketch, id: number, copy: number): string | undefined {
  const p = s.patterns?.find((x) => x.id === id);
  if (!p || !p.copies[copy]) return "Esa copia no es del patrón";
  const on = !p.skip.includes(copy);
  p.skip = on ? [...p.skip, copy].sort((a, b) => a - b) : p.skip.filter((k) => k !== copy);
  setSkipped(s, p, copy, on);
  return undefined;
}

/** Olvida el patrón: las copias quedan sueltas (con sus restricciones) */
export function dropPattern(s: Sketch, id: number): void {
  s.patterns = (s.patterns ?? []).filter((p) => p.id !== id);
  if (!s.patterns.length) delete s.patterns;
}

/** Nombre corto para la interfaz */
export function patternLabel(p: SketchPatternRecord): string {
  const kinds = { linear: "lineal", circular: "circular", curve: "en curva", table: "por tabla" } as const;
  const n = p.copies.length + 1 - p.skip.length;
  return `Patrón ${kinds[p.spec.kind]} · ${n} de ${p.copies.length + 1}`;
}

