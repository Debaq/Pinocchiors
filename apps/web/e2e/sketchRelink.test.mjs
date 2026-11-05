// Reparar una curva ligada que perdió su referencia (PLAN_SKETCH_URGENTE U8):
// toma la forma de la arista nueva, mismo id, y lo unido a sus extremos sigue unido.
//   node --test apps/web/e2e/sketchRelink.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { addEntity, addPoint, geometryPoints, relinkProjected } from "../src/lib/cad.ts";

function sketch() {
  return { origin: 0, points: [{ id: 0, x: 0, y: 0 }], entities: [], constraints: [] };
}
const at = (s, id) => {
  const p = s.points.find((q) => q.id === id);
  return [p.x, p.y];
};
const edge = (n) => ({ point: [n, 0, 0], direction: [1, 0, 0] });

test("misma clase: mueve los puntos (sin dar vuelta la línea) y cambia el vínculo", () => {
  const s = sketch();
  const [a, b] = [addPoint(s, [0, 0]), addPoint(s, [10, 0])];
  const l = addEntity(s, { type: "line", start: a, end: b });
  s.uses = [{ edge: edge(1), entity: l }];
  s.constraints.push({ type: "horizontal", entity: l });
  const dropped = relinkProjected(s, l, { kind: "line", start: [12, 1], end: [-2, 1] }, { edge: edge(2) });
  assert.equal(dropped, 0);
  assert.deepEqual(at(s, a), [-2, 1]);
  assert.deepEqual(at(s, b), [12, 1]);
  assert.deepEqual(s.uses, [{ edge: edge(2), entity: l }]);
  assert.equal(s.constraints.length, 1);
});

test("otra clase: línea que pasa a arco, mismo id, extremos de antes y relaciones con ella afuera", () => {
  const s = sketch();
  const [a, b] = [addPoint(s, [0, 0]), addPoint(s, [10, 0])];
  const l = addEntity(s, { type: "line", start: a, end: b });
  // Otra línea unida a un extremo y relaciones con la ligada
  const c = addPoint(s, [10, 5]);
  const other = addEntity(s, { type: "line", start: b, end: c });
  s.constraints.push({ type: "perpendicular", a: l, b: other }, { type: "vertical", entity: other });
  s.uses = [{ edge: edge(1), entity: l }];
  const dropped = relinkProjected(s, l, { kind: "arc", center: [5, -3], start: [10.1, 0], end: [-0.1, 0] }, { edge: edge(2) });
  assert.equal(dropped, 1);
  const g = s.entities.find((e) => e.id === l).geometry;
  assert.equal(g.type, "arc");
  assert.equal(g.start, b, "el extremo cercano sigue siendo el mismo punto");
  assert.equal(g.end, a);
  assert.deepEqual(at(s, b), [10.1, 0]);
  assert.deepEqual(at(s, g.center), [5, -3]);
  // La otra línea sigue unida (comparte el punto b) y su vertical queda
  assert.equal(s.entities.find((e) => e.id === other).geometry.start, b);
  assert.deepEqual(s.constraints, [{ type: "vertical", entity: other }]);
  assert.deepEqual(s.uses, [{ edge: edge(2), entity: l }]);
});

test("arco que pasa a círculo: los extremos sueltos se borran", () => {
  const s = sketch();
  const [c, a, b] = [addPoint(s, [0, 0]), addPoint(s, [5, 0]), addPoint(s, [0, 5])];
  const arc = addEntity(s, { type: "arc", center: c, start: a, end: b });
  s.uses = [{ source: { type: "section" }, entity: arc }];
  relinkProjected(s, arc, { kind: "circle", center: [1, 1], radius: 4 }, { edge: edge(3) });
  const g = s.entities.find((e) => e.id === arc).geometry;
  assert.equal(g.type, "circle");
  assert.equal(g.radius, 4);
  const used = new Set(s.entities.flatMap((e) => geometryPoints(e.geometry)));
  assert.ok(s.points.every((p) => p.id === s.origin || used.has(p.id)), "sin puntos sueltos");
  assert.deepEqual(s.uses, [{ edge: edge(3), entity: arc }]);
});

test("entidad que ya no está: undefined y nada cambia", () => {
  const s = sketch();
  const before = structuredClone(s);
  assert.equal(relinkProjected(s, 99, { kind: "point", at: [1, 1] }, { edge: edge(1) }), undefined);
  assert.deepEqual(s, before);
});
