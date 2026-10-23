// Plano de un ensamble: la lista de materiales (tabla) y los globos que la
// numeran sobre una vista. Ver libs/cad/PLAN_PESTANAS.md (F7).
// Funciones puras: se prueban con node (e2e/drawingBom.test.mjs).

import type { P2 } from "./drawing.ts";

interface PartRef {
  feature: number;
  index: number;
}

export interface BomRow {
  /** Número del globo (1, 2, 3…) */
  item: number;
  name: string;
  qty: number;
  /** Instancias de esa pieza, en orden */
  instances: number[];
}

const same = (a: PartRef, b: PartRef) => a.feature === b.feature && a.index === b.index;

/** Una fila por pieza (las instancias de la misma pieza se cuentan juntas), en el orden en que aparecen */
export function bomRows(instances: { id: number; part: PartRef }[], parts: { id: PartRef; name: string }[]): BomRow[] {
  const out: BomRow[] = [];
  const of: { part: PartRef; row: BomRow }[] = [];
  for (const inst of instances) {
    const p = parts.find((x) => same(x.id, inst.part));
    if (!p) continue;
    const known = of.find((x) => same(x.part, inst.part));
    if (known) {
      known.row.qty += 1;
      known.row.instances.push(inst.id);
      continue;
    }
    const row = { item: out.length + 1, name: p.name, qty: 1, instances: [inst.id] };
    out.push(row);
    of.push({ part: inst.part, row });
  }
  return out;
}

/** Largo máximo del nombre en la tabla (la columna de rótulos es angosta) */
const NAME_MAX = 24;

/** La tabla de la hoja (dos columnas, como las de los engranes): «1  Caja» · «×2» */
export function bomTable(rows: BomRow[], title: string): { title: string; rows: [string, string][] } {
  const short = (s: string) => (s.length > NAME_MAX ? `${s.slice(0, NAME_MAX - 1)}…` : s);
  return { title, rows: rows.map((r): [string, string] => [`${r.item}  ${short(r.name)}`, `×${r.qty}`]) };
}

export interface Balloon {
  label: string;
  /** El punto al que apunta y el centro del globo (coordenadas de la vista) */
  to: P2;
  at: P2;
}

/**
 * Globos alrededor de una vista: cada uno en la dirección de su punto desde
 * el centro de `box`, sobre una elipse a `margin` de la caja, y separados al
 * menos `gap` entre sí (todo en coordenadas de la vista).
 */
export function placeBalloons(anchors: { label: string; to: P2 }[], box: { min: P2; max: P2 }, margin: number, gap: number): Balloon[] {
  const n = anchors.length;
  if (!n) return [];
  const c: P2 = [(box.min[0] + box.max[0]) / 2, (box.min[1] + box.max[1]) / 2];
  const rx = (box.max[0] - box.min[0]) / 2 + margin;
  const ry = (box.max[1] - box.min[1]) / 2 + margin;
  // Ángulo de cada uno (en el centro: repartidos)
  const list = anchors.map((a, i) => {
    const d: P2 = [a.to[0] - c[0], a.to[1] - c[1]];
    const angle = Math.hypot(d[0], d[1]) < 1e-9 ? (2 * Math.PI * i) / n : Math.atan2(d[1] / ry, d[0] / rx);
    return { ...a, angle };
  });
  list.sort((a, b) => a.angle - b.angle);
  // Separación mínima como ángulo (sobre el radio menor, para quedar del lado seguro)
  const step = Math.min(gap / Math.max(Math.min(rx, ry), 1e-9), (2 * Math.PI) / n);
  for (let pass = 0; pass < 4; pass++) {
    for (let i = 1; i < n; i++) list[i].angle = Math.max(list[i].angle, list[i - 1].angle + step);
    // Lo que se pasó de la vuelta empuja hacia atrás desde el final
    const over = list[n - 1].angle - (list[0].angle + 2 * Math.PI - step);
    if (over <= 1e-12) break;
    list[n - 1].angle -= over;
    for (let i = n - 2; i >= 0; i--) list[i].angle = Math.min(list[i].angle, list[i + 1].angle - step);
  }
  return list.map((a) => ({ label: a.label, to: a.to, at: [c[0] + rx * Math.cos(a.angle), c[1] + ry * Math.sin(a.angle)] as P2 }));
}
