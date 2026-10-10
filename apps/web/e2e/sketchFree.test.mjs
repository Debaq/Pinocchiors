// Pruebas de soltar lo elegido de lo demás y del arrastre libre (lib/sketchFree.ts):
//   node --test apps/web/e2e/sketchFree.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { addEntity, addPoint } from "../src/lib/cad.ts";
import { detachSelection, freeFromConstraints } from "../src/lib/sketchFree.ts";
import { transformSelection, translation } from "../src/lib/sketchTransform.ts";

function sketch() {
  return { origin: 0, points: [{ id: 0, x: 0, y: 0 }], entities: [], constraints: [] };
}
const at = (s, id) => {
  const p = s.points.find((q) => q.id === id);
  return [p.x, p.y];
};

/** Dos líneas en L que comparten la esquina, con cotas */
function ell() {
  const s = sketch();
  const a = addPoint(s, [0, 10]);
  const corner = addPoint(s, [10, 10]);
  const b = addPoint(s, [10, 0]);
  const top = addEntity(s, { type: "line", start: a, end: corner });
  const side = addEntity(s, { type: "line", start: corner, end: b });
  s.constraints.push(
    { type: "horizontal", line: top },
    { type: "vertical", line: side },
    { type: "length", line: top, value: 10 },
    { type: "perpendicular", a: top, b: side },
    { type: "fixed", point: corner, x: 10, y: 10 },
  );
  return { s, top, side, corner, b };
}

test("soltar: la línea elegida se lleva una copia de la esquina y deja de estar atada a la otra", () => {
  const { s, top, side, corner } = ell();
  const r = detachSelection(s, [top]);
  assert.deepEqual(r, { points: 1, constraints: 1 });
  const g = s.entities.find((e) => e.id === top).geometry;
  assert.notEqual(g.end, corner);
  assert.equal(s.entities.find((e) => e.id === side).geometry.start, corner);
  // Las suyas siguen; la perpendicular con la otra se fue; el fijo de la esquina vale para las dos
  assert.ok(s.constraints.some((k) => k.type === "horizontal" && k.line === top));
  assert.ok(s.constraints.some((k) => k.type === "length" && k.line === top));
  assert.ok(!s.constraints.some((k) => k.type === "perpendicular"));
  assert.equal(s.constraints.filter((k) => k.type === "fixed").length, 2);
  // Al moverla, la otra no se estira
  transformSelection(s, [top], translation([0, 5]));
  assert.deepEqual(at(s, s.entities.find((e) => e.id === side).geometry.start), [10, 10]);
  assert.deepEqual(at(s, g.end), [10, 15]);
});

test("sin soltar (como siempre): la esquina compartida se mueve y la otra se estira", () => {
  const { s, top, side } = ell();
  transformSelection(s, [top], translation([0, 5]));
  assert.deepEqual(at(s, s.entities.find((e) => e.id === side).geometry.start), [10, 15]);
});

test("arrastre libre: se quitan las restricciones que sujetan el punto", () => {
  const { s, corner } = ell();
  // La esquina: horizontal, vertical, largo, perpendicular (tocan sus líneas) y el fijo
  assert.equal(freeFromConstraints(s, corner), 5);
  assert.equal(s.constraints.length, 0);
});

test("arrastre libre de una línea: solo lo que la toca", () => {
  const { s, b } = ell();
  const extra = addEntity(s, { type: "line", start: addPoint(s, [30, 0]), end: addPoint(s, [40, 0]) });
  s.constraints.push({ type: "horizontal", line: extra });
  assert.equal(freeFromConstraints(s, extra), 1);
  assert.equal(s.constraints.length, 5);
  assert.ok(b);
});
