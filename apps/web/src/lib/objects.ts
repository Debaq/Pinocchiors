// Objetos de la escena: cada uno es un modelo completo (malla, esqueleto,
// quads, pesos…). El activo vive en el backend y en la interfaz como siempre;
// los demás guardan acá su interfaz y su historial, y el backend su estado
// (ver `object_activate`). Las piezas del diseño son objetos cuya malla se
// genera sola cuando hace falta.

import type { PartId } from "./cad";
import type { HistoryNode, SavedHistory } from "./history";

export type ObjectSource =
  | { kind: "import" }
  /** Pieza del diseño; `hash` = huella de la malla generada (sin generar todavía si falta) */
  | { kind: "cad"; part: PartId; hash?: string };

export interface SceneObject {
  id: number;
  name: string;
  source: ObjectSource;
  /** Interfaz e historial mientras no está activo */
  ui?: string;
  history?: SavedHistory;
  /** Sube cada vez que deja de estar activo (su malla guardada pudo cambiar) */
  rev?: number;
}

export const partKey = (p: PartId) => `${p.feature}:${p.index}`;

export const isCadObject = (o: SceneObject | undefined): o is SceneObject & { source: { kind: "cad"; part: PartId; hash?: string } } =>
  o?.source.kind === "cad";

/** Ya tiene malla (o la tuvo): no se puede descartar sin perder trabajo */
export const materialized = (o: SceneObject, active: number | undefined) =>
  o.id === active ? o.source.kind === "import" || !!(o.source.kind === "cad" && o.source.hash) : !!o.ui;

export const nextObjectId = (list: SceneObject[]) => list.reduce((m, o) => Math.max(m, o.id), 0) + 1;

/**
 * Agrega un objeto por cada pieza del diseño que todavía no tiene y renombra
 * los de las piezas que cambiaron de nombre. Los de piezas que ya no están se
 * quedan (si tenían malla, es trabajo hecho; si no, el Outliner los esconde).
 */
export function syncCadObjects(list: SceneObject[], parts: { id: PartId; name: string }[]): SceneObject[] {
  let changed = false;
  let next = nextObjectId(list);
  const out = list.map((o) => {
    if (o.source.kind !== "cad") return o;
    const key = partKey(o.source.part);
    const p = parts.find((x) => partKey(x.id) === key);
    if (!p || p.name === o.name) return o;
    changed = true;
    return { ...o, name: p.name };
  });
  for (const p of parts) {
    if (out.some((o) => o.source.kind === "cad" && partKey(o.source.part) === partKey(p.id))) continue;
    out.push({ id: next++, name: p.name, source: { kind: "cad", part: p.id } });
    changed = true;
  }
  return changed ? out : list;
}

/** Pasos hechos de un historial guardado (de la raíz, sin ella, al actual) */
export function savedTrail(saved: SavedHistory | undefined): HistoryNode[] {
  if (!saved?.nodes[saved.current]) return [];
  const out: HistoryNode[] = [];
  for (let n: HistoryNode | undefined = saved.nodes[saved.current]; n && n.parent !== null; n = saved.nodes[n.parent]) out.push(n);
  return out.reverse();
}

/** Los pasos que cambian la geometría (los que se rehacen si cambia el diseño) */
export const isGeometryStep = (node: HistoryNode) =>
  node.step?.kind === "placement" || (node.step?.kind === "snapshot" && ((node.step.data as { ops?: unknown[] })?.ops?.length ?? 0) > 0);
