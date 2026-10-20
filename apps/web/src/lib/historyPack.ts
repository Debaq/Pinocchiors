import type { SavedHistory } from "./history.ts";

/**
 * Compacta el historial para guardarlo. Cada edición de un clip guarda el clip
 * entero antes y después, pero en memoria los pasos comparten las listas de
 * keys que no cambiaron (la misma referencia). Al serializar a JSON esa
 * compartición se pierde y 50 ediciones de un clip pesado dan cientos de MB
 * (con más, `JSON.stringify` falla). Aquí cada lista distinta se guarda una
 * sola vez en `pool` y los pasos apuntan a ella con `{ $ref }`; al cargar se
 * vuelve a compartir, sin perder nada.
 */

/** Las listas más cortas no valen la pena */
const MIN_POOLED = 8;

type Ref = { $ref: number };
const isRef = (v: unknown): v is Ref =>
  !!v && typeof v === "object" && !Array.isArray(v) && Object.keys(v).length === 1 && typeof (v as Ref).$ref === "number";
const isPlain = (v: unknown): v is Record<string, unknown> => !!v && typeof v === "object" && Object.getPrototypeOf(v) === Object.prototype;

export function packHistory(saved: SavedHistory): SavedHistory {
  const pool: unknown[] = [];
  const ids = new Map<unknown[], number>();
  const pack = (v: unknown): unknown => {
    if (Array.isArray(v)) {
      if (v.length < MIN_POOLED) return v.map(pack);
      let id = ids.get(v);
      if (id === undefined) {
        id = pool.length;
        ids.set(v, id);
        pool.push(null);
        pool[id] = v.map(pack);
      }
      return { $ref: id } satisfies Ref;
    }
    if (isPlain(v)) {
      const out: Record<string, unknown> = {};
      for (const k in v) out[k] = pack(v[k]);
      return out;
    }
    return v;
  };
  const nodes = saved.nodes.map((n) => (n.step?.data === undefined ? n : { ...n, step: { ...n.step, data: pack(n.step.data) } }));
  return pool.length === 0 ? saved : { ...saved, nodes, pool };
}

/** Deshace `packHistory`; un historial sin `pool` (proyectos viejos) queda igual */
export function unpackHistory(saved: SavedHistory): SavedHistory {
  const pool = saved.pool;
  if (!Array.isArray(pool) || !Array.isArray(saved.nodes)) return saved;
  const resolved = new Map<number, unknown[]>();
  const unpack = (v: unknown): unknown => {
    if (isRef(v)) {
      let list = resolved.get(v.$ref);
      if (!list) {
        const raw = pool[v.$ref];
        if (!Array.isArray(raw)) return [];
        list = [];
        resolved.set(v.$ref, list);
        for (const item of raw) list.push(unpack(item));
      }
      return list;
    }
    if (Array.isArray(v)) return v.map(unpack);
    if (isPlain(v)) {
      const out: Record<string, unknown> = {};
      for (const k in v) out[k] = unpack(v[k]);
      return out;
    }
    return v;
  };
  const { pool: _, ...rest } = saved;
  return { ...rest, nodes: saved.nodes.map((n) => (n.step?.data === undefined ? n : { ...n, step: { ...n.step, data: unpack(n.step.data) } })) };
}
