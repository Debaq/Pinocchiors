// Soltar parte del sketch de lo demás: al transformar, que lo elegido se
// separe de lo que no se mueve (en vez de estirarlo), y el arrastre libre,
// que quita las restricciones de lo que se arrastra.
// Funciones puras: se prueban con node (e2e/sketchFree.test.mjs).

import { addPoint, constraintIds, geometryPoints, mapConstraintIds, mapGeometry, type Sketch, type SketchConstraint } from "./cad.ts";
import { selectedEntities, selectionPoints } from "./sketchTransform.ts";

/** Los puntos que toca una restricción (los suyos y los de las entidades que nombra) */
function touches(s: Sketch, c: SketchConstraint): Set<number> {
  const out = new Set<number>();
  for (const id of constraintIds(c)) {
    const e = s.entities.find((x) => x.id === id);
    if (e) for (const p of geometryPoints(e.geometry)) out.add(p);
    else out.add(id);
  }
  return out;
}

/**
 * Separa lo elegido de lo demás antes de moverlo: los puntos que comparte con
 * lo que no se mueve se duplican (lo elegido se queda con la copia) y se
 * quitan las restricciones entre lo elegido y lo de afuera. Devuelve cuántas
 * uniones y restricciones se soltaron.
 */
export function detachSelection(s: Sketch, ids: number[]): { points: number; constraints: number } {
  const ents = selectedEntities(s, ids);
  const inside = new Set(ents.map((e) => e.id));
  const moving = new Set(selectionPoints(s, ids));
  // Puntos de lo elegido que usa también algo de afuera (o el origen)
  const shared = [...moving].filter((p) => s.entities.some((e) => !inside.has(e.id) && geometryPoints(e.geometry).includes(p)));
  const own = new Set(ents.flatMap((e) => geometryPoints(e.geometry)));
  if (s.origin !== undefined && own.has(s.origin)) shared.push(s.origin);
  const copy = new Map<number, number>();
  for (const p of shared) {
    const q = s.points.find((x) => x.id === p)!;
    copy.set(p, addPoint(s, [q.x, q.y]));
  }
  const swap = (id: number) => copy.get(id) ?? id;
  for (const e of ents) e.geometry = mapGeometry(e.geometry, swap);
  // Lo de adentro: las entidades elegidas y sus puntos (ya con las copias)
  const mine = new Set<number>([...inside, ...ents.flatMap((e) => geometryPoints(e.geometry)), ...[...moving].filter((p) => !copy.has(p))]);
  let removed = 0;
  const keep: SketchConstraint[] = [];
  for (const c of s.constraints) {
    const cids = constraintIds(c);
    const isShared = (id: number) => copy.has(id);
    // Solo entre lo elegido (contando los compartidos): sigue, con las copias
    if (cids.every((id) => mine.has(id) || isShared(id))) {
      if (cids.some(isShared) && cids.every(isShared)) keep.push(c); // de un compartido solo: vale para los dos
      keep.push(cids.some(isShared) ? mapConstraintIds(c, swap) : c);
      continue;
    }
    // Entre lo elegido y lo de afuera: se suelta
    if (cids.some((id) => mine.has(id))) {
      removed++;
      continue;
    }
    keep.push(c);
  }
  s.constraints = keep;
  return { points: copy.size, constraints: removed };
}

/**
 * Arrastre libre: quita las restricciones que sujetan el punto o la entidad
 * `id` (las que tocan sus puntos). Devuelve cuántas.
 */
export function freeFromConstraints(s: Sketch, id: number): number {
  const e = s.entities.find((x) => x.id === id);
  const pts = new Set(e ? geometryPoints(e.geometry) : [id]);
  const before = s.constraints.length;
  s.constraints = s.constraints.filter((c) => {
    if (constraintIds(c).includes(id)) return false;
    for (const p of touches(s, c)) if (pts.has(p)) return false;
    return true;
  });
  return before - s.constraints.length;
}
