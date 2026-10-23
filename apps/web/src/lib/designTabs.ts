// Lo que no depende de la interfaz en las pestañas de Diseñar (ver
// components/layout/DesignTabs.tsx y libs/cad/PLAN_PESTANAS.md).
// Funciones puras: se prueban con node (e2e/designTabs.test.mjs).

/** La pestaña de al lado de la activa (`step` = 1 la siguiente, −1 la anterior), en ronda */
export function neighborTab(tabs: { id: number }[], active: number | null, step: 1 | -1): number | undefined {
  if (tabs.length < 2) return undefined;
  const i = tabs.findIndex((x) => x.id === active);
  return tabs[(i + step + tabs.length) % tabs.length].id;
}

/** Igual que `tab_feature` del backend: la operación `f` de la pestaña `tab` con un número único entre pestañas */
export const tabFeature = (tab: number, f: number) => tab * 2 ** 20 + f;

/** Posición final de una pestaña que estaba en `from` y se suelta en el hueco `slot` (0 = antes de la primera) */
export const dropIndex = (from: number, slot: number) => (slot > from ? slot - 1 : slot);

export interface TabHistory<T> {
  undo: T[];
  redo: T[];
}

/**
 * Deshacer por pestaña: `undo`/`redo` son las pilas en uso (se cambian en el
 * lugar). La pestaña que se deja (si sigue) guarda las suyas en `saved`, la
 * nueva recupera las que tenía y se olvidan las de pestañas borradas.
 */
export function swapTabHistory<T>(saved: Map<number, TabHistory<T>>, undo: T[], redo: T[], before: number | null, next: { tabs: { id: number }[]; active: number | null }): void {
  const alive = (id: number) => next.tabs.some((t) => t.id === id);
  for (const id of [...saved.keys()]) if (!alive(id)) saved.delete(id);
  if (next.active === before) return;
  if (before !== null && alive(before)) saved.set(before, { undo: undo.splice(0), redo: redo.splice(0) });
  undo.length = 0;
  redo.length = 0;
  const mine = next.active === null ? undefined : saved.get(next.active);
  if (mine) {
    undo.push(...mine.undo);
    redo.push(...mine.redo);
    saved.delete(next.active!);
  }
}
