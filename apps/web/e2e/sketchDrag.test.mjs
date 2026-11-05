// Arrastre por pasos (PLAN_SKETCH_URGENTE U4): el paso del backend trae solo
// lo que cambió y se aplica sin copiar lo demás.
//   node --test apps/web/e2e/sketchDrag.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { addEntity, addPoint, applyDragDelta } from "../src/lib/cad.ts";

function sketch() {
  return { origin: 0, points: [{ id: 0, x: 0, y: 0 }], entities: [], constraints: [] };
}

test("aplica puntos, radios y valores; lo que no cambió es el mismo objeto", () => {
  const s = sketch();
  const [a, b] = [addPoint(s, [0, 0]), addPoint(s, [10, 0])];
  const l = addEntity(s, { type: "line", start: a, end: b });
  const c = addEntity(s, { type: "circle", center: addPoint(s, [5, 5]), radius: 2 });
  s.constraints.push({ type: "length", line: l, value: 10, reference: true, expr: "w" }, { type: "angle", a: l, b: l, degrees: 30 }, { type: "horizontal", line: l });
  const before = structuredClone(s);
  const next = applyDragDelta(s, { points: [[b, 12, 1]], radii: [[c, 3]], values: [[0, 12.04], [1, -45]] });
  assert.deepEqual(s, before, "el original no se toca");
  assert.deepEqual(next.points.find((p) => p.id === b), { id: b, x: 12, y: 1 });
  assert.equal(next.points.find((p) => p.id === a), s.points.find((p) => p.id === a), "punto sin cambio: el mismo objeto");
  assert.equal(next.entities.find((e) => e.id === c).geometry.radius, 3);
  assert.equal(next.entities.find((e) => e.id === l), s.entities.find((e) => e.id === l));
  assert.deepEqual(next.constraints[0], { type: "length", line: l, value: 12.04, reference: true, expr: "w" });
  assert.equal(next.constraints[1].degrees, -45);
  assert.equal(next.constraints[2], s.constraints[2]);
});

test("sin cambios: comparte los arreglos", () => {
  const s = sketch();
  const next = applyDragDelta(s, { points: [], radii: [], values: [] });
  assert.notEqual(next, s);
  assert.equal(next.points, s.points);
  assert.equal(next.entities, s.entities);
});
