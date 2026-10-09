// Pruebas de la revisión del contorno y la selección por cadena o tipo
// (lib/sketchCheck.ts), sin navegador:
//   node --test apps/web/e2e/sketchCheck.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { addEntity, addPoint, addRectangle } from "../src/lib/cad.ts";
import { checkSketch, connectedChain, problemsText, selectByKind, whyNoRegions } from "../src/lib/sketchCheck.ts";
import { addSlot } from "../src/lib/sketchShapes.ts";

function sketch() {
  return { origin: 0, points: [{ id: 0, x: 0, y: 0 }], entities: [], constraints: [] };
}
const line = (s, a, b) => addEntity(s, { type: "line", start: addPoint(s, a), end: addPoint(s, b) });

test("rectángulo, ranura y círculo cerrados: sin problemas", () => {
  const s = sketch();
  addRectangle(s, [0, 0], [10, 5]);
  addSlot(s, addPoint(s, [20, 0]), addPoint(s, [30, 0]), 2);
  addEntity(s, { type: "circle", center: addPoint(s, [50, 0]), radius: 3 });
  const p = checkSketch(s);
  assert.deepEqual(p, { looseEnds: [], crossings: [], overlaps: [] });
  assert.equal(problemsText(p), undefined);
});

test("una línea suelta: dos extremos sueltos", () => {
  const s = sketch();
  const l = line(s, [0, 0], [10, 0]);
  const p = checkSketch(s);
  assert.equal(p.looseEnds.length, 2);
  assert.ok(p.looseEnds.every((x) => x.entity === l));
  assert.equal(problemsText(p), "2 extremos sueltos");
  assert.match(whyNoRegions(s), /2 extremos sueltos/);
});

test("extremos en el mismo lugar con puntos distintos: unidos (como las regiones)", () => {
  const s = sketch();
  line(s, [0, 0], [10, 0]);
  line(s, [10, 0], [10, 10]);
  line(s, [10, 10], [0, 0]);
  assert.equal(checkSketch(s).looseEnds.length, 0);
});

test("cruce de líneas sin punto común y una que toca otra sin partirla", () => {
  const s = sketch();
  line(s, [0, 0], [10, 10]);
  line(s, [0, 10], [10, 0]);
  // En T: el extremo cae en el medio de la otra
  line(s, [20, 0], [30, 0]);
  line(s, [25, 0], [25, 8]);
  const p = checkSketch(s);
  assert.equal(p.crossings.length, 2);
  assert.ok(p.crossings.some((c) => Math.abs(c.p[0] - 5) < 1e-6 && Math.abs(c.p[1] - 5) < 1e-6));
  assert.ok(p.crossings.some((c) => Math.abs(c.p[0] - 25) < 1e-6 && Math.abs(c.p[1]) < 1e-6));
});

test("línea que cruza un círculo", () => {
  const s = sketch();
  addEntity(s, { type: "circle", center: addPoint(s, [0, 0]), radius: 5 });
  line(s, [-10, 0], [10, 0]);
  assert.equal(checkSketch(s).crossings.length, 2);
});

test("la construcción no cuenta", () => {
  const s = sketch();
  addRectangle(s, [0, 0], [10, 5]);
  const c = line(s, [-5, 2], [15, 2]);
  s.entities.find((e) => e.id === c).construction = true;
  assert.deepEqual(checkSketch(s), { looseEnds: [], crossings: [], overlaps: [] });
});

test("encimadas: la misma recta y el mismo círculo", () => {
  const s = sketch();
  const a = line(s, [0, 0], [10, 0]);
  const b = line(s, [5, 0], [15, 0]);
  const c1 = addEntity(s, { type: "circle", center: addPoint(s, [30, 0]), radius: 2 });
  const c2 = addEntity(s, { type: "circle", center: addPoint(s, [30, 0]), radius: 2 });
  const p = checkSketch(s);
  assert.deepEqual(p.overlaps.sort(), [[a, b], [c1, c2]].sort());
  assert.equal(p.crossings.length, 0, "las encimadas no suman cruces");
  // Colineales que solo se tocan en la punta: no están encimadas
  const s2 = sketch();
  line(s2, [0, 0], [10, 0]);
  line(s2, [10, 0], [20, 0]);
  assert.equal(checkSketch(s2).overlaps.length, 0);
});

test("cadena: lo unido por los extremos, sin la construcción", () => {
  const s = sketch();
  const l = addRectangle(s, [0, 0], [10, 5]);
  const extra = line(s, [10, 5], [20, 5]);
  const sueltas = line(s, [40, 0], [50, 0]);
  const c = line(s, [0, 0], [10, 5]);
  s.entities.find((e) => e.id === c).construction = true;
  assert.deepEqual(connectedChain(s, l[0]).sort(), [...l, extra].sort());
  assert.deepEqual(connectedChain(s, sueltas), [sueltas]);
  assert.deepEqual(connectedChain(s, c), [c]);
});

test("elegir por tipo", () => {
  const s = sketch();
  const l = addRectangle(s, [0, 0], [10, 5]);
  const circle = addEntity(s, { type: "circle", center: addPoint(s, [30, 0]), radius: 2 });
  const p = addPoint(s, [40, 0]);
  addEntity(s, { type: "point", point: p });
  l.slice(0, 1).forEach((id) => (s.entities.find((e) => e.id === id).construction = true));
  assert.deepEqual(selectByKind(s, "lines").sort(), l.slice(1).sort());
  assert.deepEqual(selectByKind(s, "round"), [circle]);
  assert.deepEqual(selectByKind(s, "construction"), [l[0]]);
  assert.deepEqual(selectByKind(s, "points"), [p]);
  assert.deepEqual(selectByKind(s, "free", [circle, 999]), [circle]);
});
