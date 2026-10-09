// Pruebas de las formas del sketch (lib/sketchShapes.ts), sin navegador:
//   node --test apps/web/e2e/sketchShapes.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { addEntity, addPoint } from "../src/lib/cad.ts";
import {
  addArcSlot,
  addCircumscribedPolygon,
  addRect3,
  addSlot,
  arcSlotOutline,
  ccwThrough,
  circumcircle,
  circumscribedVertices,
  nearestOnEntity,
  rectFrom3,
} from "../src/lib/sketchShapes.ts";
import { constraintHolds } from "../src/lib/sketchTransform.ts";

function sketch() {
  return { origin: 0, points: [{ id: 0, x: 0, y: 0 }], entities: [], constraints: [] };
}
const near = (a, b, what, tol = 1e-9) => assert.ok(Math.abs(a - b) < tol, `${what}: ${a} ≠ ${b}`);
const at = (s, id) => {
  const p = s.points.find((q) => q.id === id);
  return [p.x, p.y];
};
/** Todas las restricciones se cumplen en el dibujo inicial (el solver no tiene que mover nada) */
const allHold = (s) => {
  for (const c of s.constraints) assert.ok(constraintHolds(s, c, 1e-9), `no se cumple ${JSON.stringify(c)}`);
};

test("círculo por tres puntos", () => {
  const c = circumcircle([1, 0], [0, 1], [-1, 0]);
  near(c.c[0], 0, "cx");
  near(c.c[1], 0, "cy");
  near(c.r, 1, "r");
  assert.equal(circumcircle([0, 0], [1, 1], [2, 2]), undefined, "alineados");
});

test("punto más cercano en línea y círculo", () => {
  const s = sketch();
  const l = addEntity(s, { type: "line", start: addPoint(s, [0, 0]), end: addPoint(s, [10, 0]) });
  assert.deepEqual(nearestOnEntity(s, l, [3, 5]), [3, 0]);
  const c = addEntity(s, { type: "circle", center: addPoint(s, [0, 20]), radius: 2 });
  const q = nearestOnEntity(s, c, [0, 30]);
  near(q[0], 0, "x");
  near(q[1], 22, "y");
});

test("rectángulo por tres puntos: inclinado, del lado del tercer punto", () => {
  const r = rectFrom3([0, 0], [4, 3], [-3, 4]);
  assert.deepEqual(r[2].map((v) => +v.toFixed(9)), [1, 7]);
  assert.deepEqual(r[3].map((v) => +v.toFixed(9)), [-3, 4]);
  assert.equal(rectFrom3([0, 0], [4, 3], [8, 6]), undefined, "sin ancho");
  const s = sketch();
  const [a, b] = [addPoint(s, r[0]), addPoint(s, r[1])];
  const lines = addRect3(s, a, b, r[2], r[3]);
  assert.equal(lines.length, 4);
  allHold(s);
});

test("polígono circunscrito: lados tangentes al círculo, el clic en el medio de un lado", () => {
  const v = circumscribedVertices([0, 0], [0, 5], 6);
  // Medio del primer lado = el clic
  near((v[0][0] + v[1][0]) / 2, 0, "medio x");
  near((v[0][1] + v[1][1]) / 2, 5, "medio y");
  const s = sketch();
  const { lines } = addCircumscribedPolygon(s, 0, [0, 5], 6);
  assert.equal(lines.length, 6);
  allHold(s);
});

test("ranura en arco: tapas tangentes, cerrada y antihoraria", () => {
  const s = sketch();
  const c = addPoint(s, [0, 0]);
  const a = addPoint(s, [10, 0]);
  const b = addPoint(s, [0, 10]);
  const r = addArcSlot(s, c, a, b, 2);
  assert.equal(typeof r, "object");
  allHold(s);
  // Contorno cerrado: cada extremo de un arco visible es extremo de otro
  const visible = s.entities.filter((e) => !e.construction);
  const ends = visible.flatMap((e) => [e.geometry.start, e.geometry.end]);
  for (const p of ends) assert.equal(ends.filter((x) => x === p).length, 2, `punto ${p} suelto`);
  assert.equal(typeof addArcSlot(s, c, a, b, 20), "string", "más ancha que el radio");
  assert.ok(ccwThrough([0, 0], [10, 0], [0, 10], [7, 7]));
  assert.ok(!ccwThrough([0, 0], [10, 0], [0, 10], [-7, -7]));
  assert.ok(arcSlotOutline([0, 0], [10, 0], Math.PI / 2, 2).length > 60);
});

test("ranura recta", () => {
  const s = sketch();
  const r = addSlot(s, addPoint(s, [0, 0]), addPoint(s, [10, 0]), 2);
  assert.equal(Object.keys(r).length, 4);
  allHold(s);
});
