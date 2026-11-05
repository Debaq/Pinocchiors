// Huecos del sketch (PLAN_SKETCH_URGENTE U8): radio ↔ diámetro, simetría
// respecto de un punto y equidistante de splines y elipses.
//   node --test apps/web/e2e/sketchU8.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { addEntity, addPoint, circularPattern, pointMirror, swapDiameter } from "../src/lib/cad.ts";
import { offsetChain } from "../src/lib/sketchEdit.ts";
import { polylineOf } from "../src/lib/sketchCheck.ts";

function sketch() {
  return { origin: 0, points: [{ id: 0, x: 0, y: 0 }], entities: [], constraints: [] };
}
const at = (s, id) => {
  const p = s.points.find((q) => q.id === id);
  return [p.x, p.y];
};
const near = (a, b, eps = 1e-9) => Math.abs(a - b) <= eps;

test("radio ↔ diámetro: el valor se dobla o se parte, la fórmula sigue y las opciones quedan", () => {
  const r = { type: "radius", entity: 3, value: 5, reference: true, expr: "a + 1" };
  const d = swapDiameter(r);
  assert.deepEqual(d, { type: "diameter", entity: 3, value: 10, reference: true, expr: "(a + 1) * 2" });
  assert.deepEqual(swapDiameter(d), { type: "radius", entity: 3, value: 5, reference: true, expr: "((a + 1) * 2) / 2" });
  assert.equal(swapDiameter({ type: "point_line_distance", point: 1, line: 2, value: 4 }).type, "axis_diameter");
  assert.equal(swapDiameter({ type: "length", line: 2, value: 4 }), undefined);
});

test("simetría en un punto: copia girada media vuelta, el centro en el medio", () => {
  const s = sketch();
  const c = addPoint(s, [1, 1]);
  const [a, b] = [addPoint(s, [3, 1]), addPoint(s, [3, 4])];
  const l = addEntity(s, { type: "line", start: a, end: b });
  assert.equal(pointMirror(s, [l], c), undefined);
  assert.equal(s.entities.filter((e) => e.geometry.type === "line" && !e.construction).length, 2);
  const copy = s.entities.find((e) => e.id !== l && e.geometry.type === "line" && !e.construction).geometry;
  assert.deepEqual([at(s, copy.start), at(s, copy.end)].map((p) => p.map((x) => +x.toFixed(9))), [
    [-1, 1],
    [-1, -2],
  ]);
  assert.ok(s.constraints.some((k) => k.type === "midpoint" && k.point === c));
  assert.equal(pointMirror(s, [], c), "Elegir también lo que se refleja");
});

test("equidistante de una elipse: spline cerrada a la distancia pedida", () => {
  const s = sketch();
  const [c, M, m] = [addPoint(s, [0, 0]), addPoint(s, [10, 0]), addPoint(s, [0, 5])];
  const el = addEntity(s, { type: "ellipse", center: c, major: M, minor: m });
  const r = offsetChain(s, [el], 1);
  assert.equal(typeof r, "object", r);
  const [id] = r.entities;
  const g = s.entities.find((e) => e.id === id).geometry;
  assert.equal(g.type, "spline");
  assert.ok(g.closed);
  // Cada punto de la copia a 1 mm de la elipse (de un lado)
  const src = polylineOf(s, s.entities.find((e) => e.id === el).geometry);
  for (const p of g.points.map((q) => at(s, q))) {
    const d = Math.min(...src.map((q) => Math.hypot(q[0] - p[0], q[1] - p[1])));
    assert.ok(near(d, 1, 0.05), `${d}`);
  }
  // Hacia adentro más que la curvatura de la punta (b²/a = 2,5): no
  assert.equal(typeof offsetChain(s, [el], 3), "string");
  // A los dos lados
  assert.equal(offsetChain(s, [el], 1, { both: true }).entities.length, 2);
});

test("equidistante de una spline abierta: los extremos se corren por la normal", () => {
  const s = sketch();
  const pts = [[0, 0], [10, 0], [20, 0]].map((p) => addPoint(s, p));
  const sp = addEntity(s, { type: "spline", points: pts, closed: false, construction: undefined });
  s.entities.find((e) => e.id === sp).construction = true;
  const r = offsetChain(s, [sp], 2);
  const g = s.entities.find((e) => e.id === r.entities[0]);
  assert.equal(g.construction, true, "hereda las marcas");
  const ps = g.geometry.points.map((q) => at(s, q));
  assert.ok(near(ps[0][0], 0, 1e-6) && near(ps[0][1], 2, 1e-6), `${ps[0]}`);
  assert.ok(near(ps[ps.length - 1][0], 20, 1e-6) && near(ps[ps.length - 1][1], 2, 1e-6));
});

test("patrón circular de dos sigue igual (lo que usa la simetría en un punto)", () => {
  const s = sketch();
  const l = addEntity(s, { type: "line", start: addPoint(s, [1, 0]), end: addPoint(s, [2, 0]) });
  assert.equal(circularPattern(s, [l], 2, 0, 360, true), undefined);
});
