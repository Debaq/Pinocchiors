// Organización del sketch: bloques (un dibujo con su origen que se inserta
// varias veces; editar el bloque cambia todas las copias) y capas (ocultar o
// bloquear por grupo). Cada copia de un bloque es un `SketchText` con
// `block`: rígida con su ancla, como los textos.
// Funciones puras: se prueban con node (e2e/sketchBlocks.test.mjs).

import {
  addEntity,
  addPoint,
  constraintIds,
  geometryPoints,
  mapConstraintIds,
  mapGeometry,
  removeEntity,
  sketchId,
  type P2,
  type Sketch,
  type SketchBlock,
  type SketchConstraint,
  type SketchEntity,
  type SketchText,
} from "./cad.ts";
import { selectedEntities } from "./sketchTransform.ts";
import { t as tr } from "./i18n.ts";

const pos = (s: Sketch, id: number): P2 => {
  const p = s.points.find((q) => q.id === id)!;
  return [p.x, p.y];
};

/** Lo elegido que pertenece a un texto o a otra copia de bloque */
function inGroup(s: Sketch, ids: number[]): boolean {
  return (s.texts ?? []).some((t) => ids.some((id) => t.anchor === id || t.entities.includes(id) || t.points.includes(id)));
}

/**
 * Hace un bloque con lo elegido: el origen del bloque es el punto elegido
 * (si se eligió uno solo) o el centro de la caja. Lo elegido pasa a ser la
 * primera copia (rígida: sus restricciones internas quedan en el bloque).
 * Devuelve la copia o un mensaje.
 */
export function createBlock(s: Sketch, ids: number[], name: string): SketchText | string {
  const ents = selectedEntities(s, ids);
  if (!ents.length) return tr("Elegir lo que forma el bloque");
  if (inGroup(s, ids)) return tr("Un texto o una copia de bloque no puede ir dentro de otro bloque");
  const entIds = new Set(ents.map((e) => e.id));
  const all = [...new Set(ents.flatMap((e) => geometryPoints(e.geometry)))];
  const pts = all.filter((p) => p !== s.origin);
  if (!pts.length) return tr("Lo elegido no tiene puntos propios");
  // Origen: un punto elegido suelto, o el centro de la caja. Si lo elegido
  // toca el origen del sketch, ese es el origen del bloque (no se mueve)
  const chosen = ids.filter((id) => pts.includes(id) && !entIds.has(id));
  const onOrigin = s.origin !== undefined && all.includes(s.origin);
  if (onOrigin && chosen.length === 1) return tr("Lo elegido toca el origen del sketch: el origen del bloque tiene que ser ese");
  let anchor: number;
  if (onOrigin) anchor = s.origin!;
  else if (chosen.length === 1) anchor = chosen[0];
  else {
    const xs = pts.map((p) => pos(s, p)[0]);
    const ys = pts.map((p) => pos(s, p)[1]);
    anchor = addPoint(s, [(Math.min(...xs) + Math.max(...xs)) / 2, (Math.min(...ys) + Math.max(...ys)) / 2]);
  }
  const base = pos(s, anchor);
  // Definición: el ancla es el punto 0; el resto, numerados desde 1
  const map = new Map<number, number>([[anchor, 0]]);
  let next = 1;
  for (const p of pts) if (!map.has(p)) map.set(p, next++);
  for (const e of ents) map.set(e.id, next++);
  const inside = new Set([anchor, ...pts, ...entIds]);
  const internal = s.constraints.filter((c) => c.type !== "fixed" && c.type !== "lock" && constraintIds(c).every((id) => inside.has(id)));
  const block: SketchBlock = {
    id: Math.max(0, ...(s.blocks ?? []).map((b) => b.id)) + 1,
    name,
    points: [{ id: 0, x: 0, y: 0 }, ...pts.filter((p) => p !== anchor).map((p) => ({ id: map.get(p)!, x: pos(s, p)[0] - base[0], y: pos(s, p)[1] - base[1] }))],
    entities: ents.map((e) => ({ ...structuredClone(e), id: map.get(e.id)!, geometry: mapGeometry(e.geometry, (p) => map.get(p)!) })),
    constraints: internal.map((c) => mapConstraintIds(c, (id) => map.get(id) ?? id)),
  };
  s.blocks = [...(s.blocks ?? []), block];
  // La copia: las mismas entidades, rígidas con el ancla (las internas sobran)
  s.constraints = s.constraints.filter((c) => !internal.includes(c));
  const t: SketchText = { id: sketchId(s), text: name, size: 1, font: "", anchor, entities: ents.map((e) => e.id), points: pts.filter((p) => p !== anchor), block: block.id };
  s.texts = [...(s.texts ?? []), t];
  return t;
}

/** Giro (grados) y escala → marco 2×2 por filas */
function frameOf(degrees: number, scale: number): [number, number, number, number] {
  const a = (degrees * Math.PI) / 180;
  const [c, sn] = [Math.cos(a) * scale, Math.sin(a) * scale];
  return [c, -sn, sn, c];
}

/**
 * Inserta una copia del bloque con el origen en `at` (o en el punto `anchor`
 * que ya existe), girada `degrees` y con la escala `scale`. Las copias no
 * llevan las restricciones del bloque (son rígidas). Devuelve la copia.
 */
export function insertBlock(s: Sketch, blockId: number, at: P2, degrees = 0, scale = 1, reuse?: { anchor: number; id: number; frame?: [number, number, number, number] }): SketchText | string {
  const b = (s.blocks ?? []).find((x) => x.id === blockId);
  if (!b) return tr("No existe ese bloque");
  const anchor = reuse?.anchor ?? addPoint(s, at);
  const base = reuse ? pos(s, anchor) : at;
  const m = reuse?.frame ? reuse.frame.map((v) => v * scale) : frameOf(degrees, scale);
  const map = new Map<number, number>([[0, anchor]]);
  for (const p of b.points) if (p.id !== 0) map.set(p.id, addPoint(s, [base[0] + m[0] * p.x + m[1] * p.y, base[1] + m[2] * p.x + m[3] * p.y]));
  // Un reflejo da vuelta el sentido de los arcos
  const flip = m[0] * m[3] - m[1] * m[2] < 0;
  const ents: number[] = [];
  for (const e of b.entities) {
    const g = mapGeometry(e.geometry, (p) => map.get(p)!, flip);
    if (g.type === "circle") g.radius *= Math.abs(scale);
    const id = addEntity(s, g);
    const added = s.entities.find((x) => x.id === id)! as SketchEntity;
    for (const k of ["construction", "axis", "layer", "infinite"] as const) if (e[k] != null && e[k] !== false) (added as unknown as Record<string, unknown>)[k] = e[k];
    ents.push(id);
  }
  const pts = [...new Set(b.points.filter((p) => p.id !== 0).map((p) => map.get(p.id)!))];
  const style = reuse?.frame ? { frame: reuse.frame } : degrees ? { frame: frameOf(degrees, 1) } : undefined;
  const t: SketchText = { id: reuse?.id ?? sketchId(s), text: b.name, size: scale, font: "", anchor, entities: ents, points: pts, block: b.id, ...(style ? { style } : {}) };
  s.texts = [...(s.texts ?? []), t];
  return t;
}

/** Rehace todas las copias de un bloque desde su definición (mismo ancla, giro y escala) */
export function refreshBlock(s: Sketch, blockId: number): number {
  const copies = (s.texts ?? []).filter((t) => t.block === blockId);
  for (const t of copies) {
    // Se sacan las curvas con la copia todavía anotada: así su ancla (que el
    // bloque puede usar) y lo que la ata quedan
    for (const e of t.entities) removeEntity(s, e);
    s.texts = (s.texts ?? []).filter((x) => x.id !== t.id);
    insertBlock(s, blockId, pos(s, t.anchor), 0, t.size || 1, { anchor: t.anchor, id: t.id, frame: t.style?.frame });
  }
  return copies.length;
}

/** El sketch para editar un bloque (su origen es el punto 0) */
export function blockSketch(b: SketchBlock): Sketch {
  const ids = [...b.points.map((p) => p.id), ...b.entities.map((e) => e.id)];
  return { points: structuredClone(b.points), entities: structuredClone(b.entities), constraints: structuredClone(b.constraints), origin: 0, next_id: Math.max(0, ...ids) + 1 };
}

/** Guarda lo editado como la definición del bloque y rehace sus copias */
export function saveBlock(s: Sketch, blockId: number, edited: Sketch): string | undefined {
  const b = (s.blocks ?? []).find((x) => x.id === blockId);
  if (!b) return tr("No existe ese bloque");
  if (!edited.entities.length) return tr("El bloque quedó vacío");
  // El origen del sketch del bloque pasa a ser el punto 0
  const o = edited.origin ?? 0;
  const f = (id: number) => (id === o ? 0 : id === 0 ? o : id);
  b.points = edited.points.map((p) => ({ id: f(p.id), x: p.x, y: p.y }));
  b.entities = edited.entities.map((e) => ({ ...structuredClone(e), id: f(e.id), geometry: mapGeometry(e.geometry, f) }));
  b.constraints = edited.constraints.map((c) => mapConstraintIds(c, f));
  refreshBlock(s, blockId);
  return undefined;
}

/**
 * Desarma una copia: sus entidades quedan sueltas (con las restricciones del
 * bloque, para que no se deformen solas) y deja de seguir al bloque. Las cotas
 * se llevan a la escala de la copia; un reflejo invierte los ángulos.
 */
export function explodeBlock(s: Sketch, textId: number): void {
  const t = (s.texts ?? []).find((x) => x.id === textId);
  const b = t && (s.blocks ?? []).find((x) => x.id === t.block);
  if (!t || !b) return;
  const scale = Math.abs(t.size || 1);
  const f = t.style?.frame;
  const flip = !!f && f[0] * f[3] - f[1] * f[2] < 0;
  const map = new Map<number, number>([[0, t.anchor]]);
  for (const [e, id] of pairCopy(s, t, b, scale, flip)) {
    map.set(e.id, id);
    const g = s.entities.find((x) => x.id === id)!.geometry;
    // Con reflejo, los arcos de la copia tienen inicio y fin cambiados
    const own = geometryPoints(mapGeometry(e.geometry, (p) => p, flip));
    own.forEach((p, k) => map.set(p, geometryPoints(g)[k]));
  }
  // Girada o reflejada: las de dirección (horizontal, cota en x) ya no valen
  const turned = !!f && (Math.abs(f[1]) > 1e-9 || Math.abs(f[2]) > 1e-9 || f[0] < 0 || f[3] < 0);
  const axial = new Set(["horizontal", "vertical", "horizontal_points", "vertical_points", "horizontal_distance", "vertical_distance"]);
  for (const c of b.constraints) {
    if (!constraintIds(c).every((id) => map.has(id))) continue;
    if (turned && axial.has(c.type)) continue;
    const k = mapConstraintIds(c, (id) => map.get(id)!) as SketchConstraint & { value?: number; degrees?: number };
    if (typeof k.value === "number") k.value *= scale;
    if (flip && k.type === "angle") k.degrees = -k.degrees || 0;
    s.constraints.push(k);
  }
  s.texts = (s.texts ?? []).filter((x) => x.id !== textId);
}

/**
 * Cada entidad de la copia con la suya del bloque. Van en el mismo orden,
 * pero si se borró alguna de la copia, el índice ya no sirve: se avanza por
 * el bloque saltando las que no están (mismo tipo y misma distancia de sus
 * puntos al ancla, que no cambia con el giro).
 */
function pairCopy(s: Sketch, t: SketchText, b: SketchBlock, scale: number, flip: boolean): [SketchEntity, number][] {
  const ents = t.entities.map((id) => s.entities.find((x) => x.id === id)).filter((e): e is SketchEntity => !!e);
  if (ents.length === b.entities.length && ents.every((e, i) => e.geometry.type === b.entities[i].geometry.type)) return b.entities.map((e, i) => [e, ents[i].id]);
  const a = pos(s, t.anchor);
  const local = new Map(b.points.map((p) => [p.id, Math.hypot(p.x, p.y) * scale]));
  const reach = (e: SketchEntity, map: (p: number) => number) => geometryPoints(e.geometry).map(map);
  const tol = 1e-6 * Math.max(1, ...local.values());
  const out: [SketchEntity, number][] = [];
  let i = 0;
  for (const c of ents) {
    const got = reach(c, (p) => Math.hypot(pos(s, p)[0] - a[0], pos(s, p)[1] - a[1]));
    while (i < b.entities.length) {
      const e = b.entities[i++];
      const want = geometryPoints(mapGeometry(e.geometry, (p) => p, flip)).map((p) => local.get(p) ?? NaN);
      if (e.geometry.type === c.geometry.type && want.length === got.length && want.every((d, k) => Math.abs(d - got[k]) <= tol)) {
        out.push([e, c.id]);
        break;
      }
    }
  }
  return out;
}

// ─── Capas ────────────────────────────────────────────────────────────────

/** Entidades en capas ocultas o bloqueadas (no se ven / no se tocan) */
export function layerState(s: Sketch): { hidden: Set<number>; locked: Set<number> } {
  const hidden = new Set<number>();
  const locked = new Set<number>();
  const byId = new Map((s.layers ?? []).map((l) => [l.id, l]));
  for (const e of s.entities) {
    const l = e.layer != null ? byId.get(e.layer) : undefined;
    if (l?.hidden) hidden.add(e.id);
    if (l?.locked) locked.add(e.id);
  }
  return { hidden, locked };
}

/** Capa nueva; devuelve su id */
export function addLayer(s: Sketch, name: string): number {
  const id = Math.max(0, ...(s.layers ?? []).map((l) => l.id)) + 1;
  s.layers = [...(s.layers ?? []), { id, name }];
  return id;
}

/** Pasa entidades a una capa (`null`: sin capa); un texto o copia de bloque va entero */
export function setLayer(s: Sketch, ids: number[], layer: number | null): void {
  const set = new Set(selectedEntities(s, ids).map((e) => e.id));
  for (const e of s.entities) {
    if (!set.has(e.id)) continue;
    if (layer == null) delete e.layer;
    else e.layer = layer;
  }
}

/** Borra una capa: sus entidades quedan sin capa */
export function removeLayer(s: Sketch, id: number): void {
  s.layers = (s.layers ?? []).filter((l) => l.id !== id);
  for (const e of s.entities) if (e.layer === id) delete e.layer;
}
