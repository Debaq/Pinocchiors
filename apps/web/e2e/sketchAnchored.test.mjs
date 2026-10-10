// Pruebas de si un sketch está ligado a una referencia (lib/cad.ts, sketchAnchored),
// sin navegador:
//   node --test apps/web/e2e/sketchAnchored.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { addEntity, addPoint, sketchAnchored } from "../src/lib/cad.ts";

function sketch() {
  return { origin: 0, points: [{ id: 0, x: 0, y: 0 }], entities: [], constraints: [] };
}
const line = (s, a, b) => {
  const start = addPoint(s, a);
  const end = addPoint(s, b);
  addEntity(s, { type: "line", start, end });
  return [start, end];
};

test("vacío: no cuenta como suelto", () => {
  assert.equal(sketchAnchored(sketch()), true);
});

test("línea suelta, aunque tenga cotas: no está ligada", () => {
  const s = sketch();
  const [a, b] = line(s, [5, 5], [15, 5]);
  s.constraints.push({ type: "distance", a, b, value: 10 }, { type: "horizontal_points", a, b });
  assert.equal(sketchAnchored(s), false);
});

test("coincidente con el origen, también a través de otra línea", () => {
  const s = sketch();
  const [, b] = line(s, [5, 5], [15, 5]);
  const [c] = line(s, [15, 5], [15, 15]);
  s.constraints.push({ type: "coincident", a: b, b: c });
  assert.equal(sketchAnchored(s), false);
  s.constraints.push({ type: "coincident", a: c, b: 0 });
  assert.equal(sketchAnchored(s), true);
});

test("una cota de referencia al origen no liga", () => {
  const s = sketch();
  const [a] = line(s, [5, 5], [15, 5]);
  s.constraints.push({ type: "distance", a, b: 0, value: 7, reference: true });
  assert.equal(sketchAnchored(s), false);
  s.constraints[0].reference = false;
  assert.equal(sketchAnchored(s), true);
});

test("punto fijo, entidad bloqueada y arista del modelo ligan", () => {
  const fixed = sketch();
  const [a] = line(fixed, [5, 5], [15, 5]);
  fixed.constraints.push({ type: "fixed", point: a, x: 5, y: 5 });
  assert.equal(sketchAnchored(fixed), true);

  const locked = sketch();
  line(locked, [5, 5], [15, 5]);
  locked.constraints.push({ type: "lock", entity: locked.entities[0].id });
  assert.equal(sketchAnchored(locked), true);

  const used = sketch();
  const [p] = line(used, [5, 5], [15, 5]);
  const [q] = line(used, [0, 0], [20, 0]);
  used.uses = [{ entity: used.entities[1].id, source: { type: "section" } }];
  used.entities[1].construction = true;
  assert.equal(sketchAnchored(used), false);
  used.constraints.push({ type: "point_on_line", point: p, line: used.entities[1].id });
  assert.equal(sketchAnchored(used), true);
  assert.ok(q !== undefined);
});
