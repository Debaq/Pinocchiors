// Pruebas de integridad de datos del sketch (PLAN_SKETCH_URGENTE U2): que
// editar no suelte patrones, no deje referencias colgando ni pierda marcas.
//   node --test apps/web/e2e/sketchIntegrity.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { addEntity, addPoint, addRectangle, constraintIds, geometryPoints, removeEntity, trimLine } from "../src/lib/cad.ts";
import { createBlock, explodeBlock, insertBlock } from "../src/lib/sketchBlocks.ts";
import { dropDegenerate, mergePoint } from "../src/lib/sketchEdit.ts";
import { editPattern, makePattern } from "../src/lib/sketchPatterns.ts";
import { splitEntityAt, transformSelection } from "../src/lib/sketchTransform.ts";

function sketch() {
  return { origin: 0, points: [{ id: 0, x: 0, y: 0 }], entities: [], constraints: [] };
}
const line = (s, a, b) => addEntity(s, { type: "line", start: addPoint(s, a), end: addPoint(s, b) });
const ent = (s, id) => s.entities.find((e) => e.id === id);
const alive = (s, c) => constraintIds(c).every((id) => s.points.some((p) => p.id === id) || s.entities.some((e) => e.id === id));

test("dropDegenerate conserva las cadenas de un patrón de 3+ copias y saca solo las degeneradas", () => {
  const s = sketch();
  const [a, b, c, d] = [1, 2, 3, 4].map((x) => addPoint(s, [x, 0]));
  s.constraints.push(
    // Cadena de patrón: b es fin de un par y comienzo del siguiente
    { type: "equal_offset", a1: a, a2: b, b1: b, b2: c },
    { type: "equal_rotation", center: 0, a1: a, a2: b, b1: c, b2: d },
    // Degeneradas
    { type: "equal_offset", a1: a, a2: a, b1: b, b2: c },
    { type: "equal_offset", a1: a, a2: b, b1: a, b2: b },
    { type: "equal_rotation", center: a, a1: a, a2: b, b1: c, b2: d },
    { type: "distance", a: c, b: c, value: 1 },
  );
  dropDegenerate(s);
  assert.deepEqual(s.constraints.map((k) => k.type), ["equal_offset", "equal_rotation"]);
});

test("fusionar puntos no suelta un patrón lineal de 4 copias", () => {
  const s = sketch();
  const c = addEntity(s, { type: "circle", center: addPoint(s, [0, 5]), radius: 1 });
  assert.equal(makePattern(s, [c], { kind: "linear", count: 4, offset: [10, 0], rows: 1, offset2: [0, 0] }), undefined);
  const before = s.constraints.filter((k) => k.type === "equal_offset").length;
  assert.ok(before >= 2);
  // Un punto suelto encima de otro, fusionado (como hace «Reparar»)
  const loose = addPoint(s, [50, 50]);
  const other = line(s, [60, 60], [70, 70]);
  mergePoint(s, loose, ent(s, other).geometry.start);
  assert.equal(s.constraints.filter((k) => k.type === "equal_offset").length, before);
});

test("bloque con una curva que toca el origen: el origen del bloque es el del sketch", () => {
  const s = sketch();
  const l = addEntity(s, { type: "line", start: 0, end: addPoint(s, [5, 0]) });
  const l2 = line(s, [5, 0], [5, 3]);
  const t = createBlock(s, [l, l2], "Esquina");
  assert.equal(typeof t, "object");
  assert.equal(t.anchor, 0);
  const b = s.blocks[0];
  for (const e of b.entities) for (const p of geometryPoints(e.geometry)) assert.ok(b.points.some((q) => q.id === p), `punto ${p} definido`);
  // Elegir otro origen con el origen adentro no se puede
  const s2 = sketch();
  const m = addEntity(s2, { type: "line", start: 0, end: addPoint(s2, [5, 0]) });
  const end = ent(s2, m).geometry.end;
  assert.equal(typeof createBlock(s2, [m, end], "X"), "string");
});

test("desarmar una copia escalada lleva las cotas a su escala; girada, sin horizontales", () => {
  const s = sketch();
  const l = addRectangle(s, [10, 10], [14, 12]);
  s.constraints.push({ type: "length", line: l[0], value: 4 });
  const corner = ent(s, l[0]).geometry.start;
  createBlock(s, [...l, corner], "Tapa");
  const big = insertBlock(s, 1, [50, 0], 0, 2);
  explodeBlock(s, big.id);
  const len = s.constraints.find((k) => k.type === "length" && big.entities.includes(k.line));
  assert.equal(len.value, 8);
  const turned = insertBlock(s, 1, [80, 0], 30, 1);
  explodeBlock(s, turned.id);
  const mine = s.constraints.filter((k) => constraintIds(k).some((id) => turned.entities.includes(id)));
  assert.ok(mine.length > 0);
  assert.ok(mine.every((k) => k.type !== "horizontal" && k.type !== "vertical"), JSON.stringify(mine.map((k) => k.type)));
});

test("desarmar una copia a la que le borraron una entidad empareja bien las que quedan", () => {
  const s = sketch();
  const l = addRectangle(s, [10, 10], [14, 12]);
  s.constraints.push({ type: "length", line: l[1], value: 2 });
  const corner = ent(s, l[0]).geometry.start;
  createBlock(s, [...l, corner], "Tapa");
  const copy = insertBlock(s, 1, [50, 0], 0, 1);
  // Se borra el primer lado: con el índice, el largo 2 caería en el lado de 4
  removeEntity(s, copy.entities[0]);
  const rest = [...s.texts.find((t) => t.id === copy.id).entities];
  explodeBlock(s, copy.id);
  const len = s.constraints.find((k) => k.type === "length" && rest.includes(k.line));
  const g = ent(s, len.line).geometry;
  const [p, q] = [g.start, g.end].map((id) => s.points.find((x) => x.id === id));
  assert.ok(Math.abs(Math.hypot(q.x - p.x, q.y - p.y) - 2) < 1e-9);
});

test("borrar o fusionar el centro de un patrón circular no lo rompe", () => {
  const s = sketch();
  const center = addPoint(s, [0, 0]);
  const hub = addEntity(s, { type: "circle", center, radius: 0.5 });
  const c = addEntity(s, { type: "circle", center: addPoint(s, [10, 0]), radius: 1 });
  assert.equal(makePattern(s, [c], { kind: "circular", count: 4, center, span: 360, rotate: true }), undefined);
  removeEntity(s, hub);
  assert.ok(s.points.some((p) => p.id === center), "el centro se queda");
  assert.equal(editPattern(s, s.patterns[0].id, { ...s.patterns[0].spec, count: 5 }), undefined);
  // Fusionado con otro punto: el patrón sigue al nuevo
  const other = addPoint(s, [0, 0]);
  mergePoint(s, center, other);
  assert.equal(s.patterns[0].spec.center, other);
  assert.equal(editPattern(s, s.patterns[0].id), undefined);
  // Si igual falta, un mensaje en vez de un error
  s.points = s.points.filter((p) => p.id !== other);
  assert.equal(typeof editPattern(s, s.patterns[0].id), "string");
});

test("recortar y partir conservan construcción, capa y eje", () => {
  const s = sketch();
  const l = line(s, [0, 0], [10, 0]);
  Object.assign(ent(s, l), { construction: true, layer: 2, axis: true });
  line(s, [3, -1], [3, 1]);
  line(s, [7, -1], [7, 1]);
  assert.equal(trimLine(s, l, [5, 0]), undefined);
  const pieces = s.entities.filter((e) => e.construction);
  assert.equal(pieces.length, 2);
  assert.ok(pieces.every((e) => e.layer === 2 && e.axis));
  const a = addEntity(s, { type: "arc", center: addPoint(s, [0, 20]), start: addPoint(s, [5, 20]), end: addPoint(s, [-5, 20]) });
  Object.assign(ent(s, a), { construction: true, layer: 3 });
  const before = s.entities.length;
  assert.equal(typeof splitEntityAt(s, a, [0, 25]), "number");
  const rest = s.entities[before];
  assert.equal(rest.construction, true);
  assert.equal(rest.layer, 3);
});

test("partir un arco pasa su largo de arco a los dos tramos", () => {
  const s = sketch();
  const a = addEntity(s, { type: "arc", center: addPoint(s, [0, 0]), start: addPoint(s, [5, 0]), end: addPoint(s, [-5, 0]) });
  s.constraints.push({ type: "arc_length", arc: a, value: 15.7, expr: "L" });
  splitEntityAt(s, a, [0, 5]);
  assert.ok(!s.constraints.some((k) => k.type === "arc_length"));
  const k = s.constraints.find((x) => x.type === "curve_length");
  assert.equal(k.entities.length, 2);
  assert.equal(k.value, 15.7);
  assert.equal(k.expr, "L");
  assert.ok(alive(s, k));
});

test("escalar no cambia el radio de un círculo bloqueado", () => {
  const s = sketch();
  const free = addEntity(s, { type: "circle", center: addPoint(s, [0, 0]), radius: 2 });
  const held = addEntity(s, { type: "circle", center: addPoint(s, [10, 0]), radius: 2 });
  s.constraints.push({ type: "lock", entity: held });
  transformSelection(s, [free, held], (p) => [p[0] * 2, p[1] * 2], 2);
  assert.equal(ent(s, free).geometry.radius, 4);
  assert.equal(ent(s, held).geometry.radius, 2);
});

test("borrar una entidad no deja referencias en textos ni patrones", () => {
  const s = sketch();
  const path = line(s, [0, 0], [10, 0]);
  const anchor = addPoint(s, [0, 0]);
  s.texts = [{ id: 900, text: "A", size: 1, font: "", anchor, entities: [], points: [], style: { path } }];
  removeEntity(s, path);
  assert.equal(s.texts[0].style.path, undefined);
  const c = addEntity(s, { type: "circle", center: addPoint(s, [0, 5]), radius: 1 });
  makePattern(s, [c], { kind: "table", offsets: [[5, 0]] });
  for (const id of [...s.patterns[0].source, ...s.patterns[0].owned]) removeEntity(s, id);
  assert.equal(s.patterns, undefined);
});
