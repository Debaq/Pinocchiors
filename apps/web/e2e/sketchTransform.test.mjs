// Pruebas de mover, copiar, girar, escalar, partir y del portapapeles del
// sketch (lib/sketchTransform.ts), sin navegador:
//   node --test apps/web/e2e/sketchTransform.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { addEntity, addPoint, addRectangle } from "../src/lib/cad.ts";
import {
  clipCenter,
  constraintHolds,
  extractClip,
  insertClip,
  measureConstraint,
  rotation,
  scaling,
  selectionCenter,
  splitEntityAt,
  transformSelection,
  translation,
} from "../src/lib/sketchTransform.ts";

function sketch() {
  return { origin: 0, points: [{ id: 0, x: 0, y: 0 }], entities: [], constraints: [] };
}
const at = (s, id) => {
  const p = s.points.find((q) => q.id === id);
  return [+p.x.toFixed(9), +p.y.toFixed(9)];
};
const near = (a, b, what) => assert.ok(Math.abs(a - b) < 1e-9, `${what}: ${a} ≠ ${b}`);
const types = (s) => s.constraints.map((c) => c.type).sort();

/** Rectángulo 10 × 5 desde (0, 0) con sus cotas */
function box() {
  const s = sketch();
  const l = addRectangle(s, [0, 0], [10, 5]);
  s.constraints.push({ type: "length", line: l[0], value: 10 }, { type: "length", line: l[1], value: 5 });
  return { s, l };
}

test("mover un rectángulo: las cotas siguen igual y nada se quita", () => {
  const { s, l } = box();
  const before = types(s);
  assert.equal(transformSelection(s, l, translation([3, 4])), undefined);
  const xs = s.points.filter((p) => p.id !== 0).map((p) => [p.x, p.y]);
  assert.deepEqual(xs.sort(), [[3, 4], [13, 4], [13, 9], [3, 9]].sort());
  assert.deepEqual(types(s), before);
  assert.equal(at(s, 0)[0], 0, "el origen no se mueve");
});

test("mover un extremo que comparte con lo no elegido: lo de al lado se estira y su cota toma el valor nuevo", () => {
  const s = sketch();
  const a = addPoint(s, [0, 0]);
  const b = addPoint(s, [10, 0]);
  const c = addPoint(s, [10, 10]);
  const l1 = addEntity(s, { type: "line", start: a, end: b });
  const l2 = addEntity(s, { type: "line", start: b, end: c });
  s.constraints.push({ type: "horizontal", line: l1 }, { type: "vertical", line: l2 }, { type: "length", line: l1, value: 10 });
  // Solo la vertical, corrida 5 a la derecha: la horizontal crece a 15 y sigue horizontal
  transformSelection(s, [l2], translation([5, 0]));
  assert.deepEqual(at(s, b), [15, 0]);
  near(s.constraints.find((k) => k.type === "length").value, 15, "largo");
  assert.ok(s.constraints.some((k) => k.type === "horizontal"));
  assert.ok(s.constraints.some((k) => k.type === "vertical"));
});

test("girar 90°: horizontal ↔ vertical; girar 30°: se quitan", () => {
  const { s, l } = box();
  transformSelection(s, l, rotation([0, 0], 90));
  assert.deepEqual(types(s), ["horizontal", "horizontal", "length", "length", "vertical", "vertical"]);
  const p = s.points.find((q) => q.id !== 0 && Math.abs(q.x + 5) < 1e-9 && Math.abs(q.y - 10) < 1e-9);
  assert.ok(p, "la esquina (10, 5) pasa a (−5, 10)");
  for (const c of s.constraints) assert.ok(constraintHolds(s, c, 1e-9) || c.type === "length", c.type);
  transformSelection(s, l, rotation([0, 0], 30));
  assert.deepEqual(types(s), ["length", "length"]);
  near(s.constraints[0].value, 10, "largo");
});

test("escalar al doble: cotas y radios al doble, el fijo se va con el punto", () => {
  const s = sketch();
  const c = addPoint(s, [2, 0]);
  const circle = addEntity(s, { type: "circle", center: c, radius: 3 });
  s.constraints.push({ type: "radius", entity: circle, value: 3 }, { type: "fixed", point: c, x: 2, y: 0 });
  transformSelection(s, [circle], scaling([0, 0], 2), 2);
  assert.deepEqual(at(s, c), [4, 0]);
  assert.equal(s.entities[0].geometry.radius, 6);
  near(measureConstraint(s, s.constraints[0]), 6, "radio medido");
  assert.equal(s.constraints[0].value, 6);
  assert.deepEqual([s.constraints[1].x, s.constraints[1].y], [4, 0]);
});

test("cota con fórmula: no se toca", () => {
  const { s, l } = box();
  s.constraints[4].expr = "ancho";
  transformSelection(s, l, scaling([0, 0], 2), 2);
  assert.equal(s.constraints[4].value, 10);
  assert.equal(s.constraints[5].value, 10);
});

test("ángulo con signo como en Rust", () => {
  const s = sketch();
  const a = addEntity(s, { type: "line", start: addPoint(s, [0, 0]), end: addPoint(s, [1, 0]) });
  const b = addEntity(s, { type: "line", start: addPoint(s, [0, 0]), end: addPoint(s, [0, -1]) });
  near(measureConstraint(s, { type: "angle", a, b, degrees: 0 }), -90, "ángulo");
});

test("copiar y pegar: ids nuevos, restricciones internas, sin fijos ni lo de afuera", () => {
  const { s, l } = box();
  const outside = addEntity(s, { type: "line", start: addPoint(s, [20, 0]), end: addPoint(s, [30, 0]) });
  s.constraints.push({ type: "parallel", a: l[0], b: outside }, { type: "fixed", point: s.entities[0].geometry.start, x: 0, y: 0 });
  const clip = extractClip(s, l);
  assert.deepEqual(clipCenter(clip), [5, 2.5]);
  assert.equal(clip.constraints.length, 6, "4 de ejes + 2 cotas");
  const n = s.constraints.length;
  const made = insertClip(s, clip, translation([0, 20]));
  assert.equal(made.length, 4);
  assert.equal(s.constraints.length, n + 6);
  const ids = new Set(s.entities.map((e) => e.id).concat(s.points.map((p) => p.id)));
  assert.equal(ids.size, s.entities.length + s.points.length, "ids repetidos");
  const copied = s.entities.filter((e) => made.includes(e.id)).flatMap((e) => [e.geometry.start, e.geometry.end]);
  for (const id of copied) assert.ok(at(s, id)[1] >= 20, "la copia queda 20 arriba");
  // Las restricciones nuevas nombran solo cosas de la copia
  for (const c of s.constraints.slice(n)) for (const v of [c.line, c.a, c.b].filter((x) => x !== undefined)) assert.ok(made.includes(v));
});

test("copiar un texto lo copia entero con su bloque", () => {
  const s = sketch();
  const anchor = addPoint(s, [0, 0]);
  const p1 = addPoint(s, [1, 0]);
  const p2 = addPoint(s, [1, 1]);
  const e = addEntity(s, { type: "line", start: p1, end: p2 });
  s.texts = [{ id: 1, text: "I", size: 10, font: "x", anchor, entities: [e], points: [p1, p2] }];
  insertClip(s, extractClip(s, [e]), translation([5, 0]));
  assert.equal(s.texts.length, 2);
  const t = s.texts[1];
  assert.notEqual(t.anchor, anchor);
  assert.deepEqual(at(s, t.anchor), [5, 0]);
  assert.equal(t.entities.length, 1);
});

test("centro de lo elegido cuenta el borde de los círculos", () => {
  const s = sketch();
  const c = addEntity(s, { type: "circle", center: addPoint(s, [10, 10]), radius: 2 });
  assert.deepEqual(selectionCenter(s, [c]), [10, 10]);
});

test("partir una línea, un arco y un círculo", () => {
  const s = sketch();
  const line = addEntity(s, { type: "line", start: addPoint(s, [0, 0]), end: addPoint(s, [10, 0]) });
  const q = splitEntityAt(s, line, [4, 1]);
  assert.equal(typeof q, "number");
  assert.deepEqual(at(s, q), [4, 0]);
  assert.equal(s.entities.filter((e) => e.geometry.type === "line").length, 2);
  assert.equal(typeof splitEntityAt(s, line, [-1, 0]), "string", "fuera de los extremos");

  const c = addPoint(s, [0, 20]);
  const arc = addEntity(s, { type: "arc", center: c, start: addPoint(s, [5, 20]), end: addPoint(s, [-5, 20]) });
  const m = splitEntityAt(s, arc, [0, 30]);
  assert.deepEqual(at(s, m), [0, 25]);
  const arcs = s.entities.filter((e) => e.geometry.type === "arc");
  assert.equal(arcs.length, 2);
  assert.equal(arcs[0].geometry.end, m);
  assert.equal(arcs[1].geometry.start, m);
  assert.equal(typeof splitEntityAt(s, arc, [0, 10]), "string", "del otro lado del arco");

  const k = addPoint(s, [0, 50]);
  const circle = addEntity(s, { type: "circle", center: k, radius: 2 });
  const r = splitEntityAt(s, circle, [3, 50]);
  assert.deepEqual(at(s, r), [2, 50]);
  const halves = s.entities.filter((e) => e.geometry.type === "arc" && e.geometry.center === k);
  assert.equal(halves.length, 2);
  assert.ok(!s.entities.some((e) => e.geometry.type === "circle"));
});

test("medir las cotas nuevas: punto-línea, diámetro respecto del eje y largo de arco", () => {
  const s = sketch();
  const line = addEntity(s, { type: "line", start: addPoint(s, [0, 0]), end: addPoint(s, [0, 10]) });
  const p = addPoint(s, [6, 3]);
  near(measureConstraint(s, { type: "point_line_distance", point: p, line, value: 0 }), 6, "distancia");
  near(measureConstraint(s, { type: "axis_diameter", point: p, line, value: 0 }), 12, "diámetro");
  // Tres cuartos de vuelta de radio 2
  const arc = addEntity(s, { type: "arc", center: addPoint(s, [0, 0]), start: addPoint(s, [2, 0]), end: addPoint(s, [0, -2]) });
  near(measureConstraint(s, { type: "arc_length", arc, value: 0 }), 3 * Math.PI, "largo de arco");
  // Al escalar al doble, el largo de arco también
  const c = { type: "arc_length", arc, value: 3 * Math.PI };
  s.constraints.push(c);
  transformSelection(s, [arc], scaling([0, 0], 2), 2);
  near(c.value, 6 * Math.PI, "largo escalado");
});

test("cotas II: suplementario, largo total y distancia mínima o máxima con círculos", () => {
  const s = sketch();
  const a = addEntity(s, { type: "line", start: addPoint(s, [0, 0]), end: addPoint(s, [10, 0]) });
  const b = addEntity(s, { type: "line", start: addPoint(s, [0, 0]), end: addPoint(s, [0, 5]) });
  near(measureConstraint(s, { type: "angle", a, b, degrees: 0, supplementary: true }), -90, "suplementario");
  const c1 = addEntity(s, { type: "circle", center: addPoint(s, [0, 20]), radius: 3 });
  const c2 = addEntity(s, { type: "circle", center: addPoint(s, [20, 20]), radius: 7 });
  near(measureConstraint(s, { type: "curve_length", entities: [a, b, c1], value: 0 }), 15 + 6 * Math.PI, "largo total");
  near(measureConstraint(s, { type: "circle_distance", a: c1, b: c2, value: 0 }), 10, "mínima");
  near(measureConstraint(s, { type: "circle_distance", a: c1, b: c2, max: true, value: 0 }), 30, "máxima");
  // Línea (y = 0) contra el círculo de radio 3 en y = 20: mínima 17, máxima 23
  near(measureConstraint(s, { type: "circle_distance", a, b: c1, value: 0 }), 17, "línea mínima");
  near(measureConstraint(s, { type: "circle_distance", a, b: c1, max: true, value: 0 }), 23, "línea máxima");
  // Adentro: centro a 2 del grande (radio 10), radio 3 → 5
  const big = addEntity(s, { type: "circle", center: addPoint(s, [50, 0]), radius: 10 });
  const small = addEntity(s, { type: "circle", center: addPoint(s, [52, 0]), radius: 3 });
  near(measureConstraint(s, { type: "circle_distance", a: big, b: small, value: 0 }), 5, "por dentro");
});

test("ids de las restricciones con lista de entidades; lo bloqueado no se mueve y la cota bloqueada no cambia", async () => {
  const { constraintIds, mapConstraintIds } = await import("../src/lib/cad.ts");
  const c = { type: "curve_length", entities: [4, 5], value: 3, opts: { offset: [1, 2] } };
  assert.deepEqual(constraintIds(c), [4, 5]);
  assert.deepEqual(mapConstraintIds(c, (id) => id + 10).entities, [14, 15]);
  assert.deepEqual(c.entities, [4, 5]);
  const { s, l } = box();
  s.constraints.push({ type: "lock", entity: l[0] });
  s.constraints[4].opts = { locked: true };
  transformSelection(s, l, scaling([0, 0], 2), 2);
  // La línea de abajo (bloqueada) quedó donde estaba; su cota bloqueada sigue en 10
  const g = s.entities.find((e) => e.id === l[0]).geometry;
  assert.deepEqual(at(s, g.start), [0, 0]);
  assert.deepEqual(at(s, g.end), [10, 0]);
  assert.equal(s.constraints.find((k) => k.type === "length" && k.line === l[0]).value, 10);
  // El copiar no lleva el bloqueo
  assert.ok(!extractClip(s, l).constraints.some((k) => k.type === "lock"));
});
