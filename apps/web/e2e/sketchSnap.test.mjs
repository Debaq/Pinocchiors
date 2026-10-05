// Pruebas de las inferencias del sketch (lib/sketchSnap.ts), sin navegador:
//   node apps/web/e2e/sketchSnap.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { infer } from "../src/lib/sketchSnap.ts";

/** Sketch con origen, una línea (0,0)-(10,0) que no sale del origen, un círculo y un arco */
function sample() {
  return {
    origin: 0,
    points: [
      { id: 0, x: 0, y: 0 },
      { id: 1, x: 20, y: 0 },
      { id: 2, x: 30, y: 0 },
      { id: 3, x: 50, y: 50 }, // centro del círculo r = 5
      { id: 4, x: -20, y: -20 }, // centro del arco r = 4, de 0° a 90°
      { id: 5, x: -16, y: -20 },
      { id: 6, x: -20, y: -16 },
    ],
    entities: [
      { id: 10, geometry: { type: "line", start: 1, end: 2 } },
      { id: 11, geometry: { type: "circle", center: 3, radius: 5 } },
      { id: 12, geometry: { type: "arc", center: 4, start: 5, end: 6 } },
    ],
    constraints: [],
  };
}

const near = (a, b) => assert.ok(Math.hypot(a[0] - b[0], a[1] - b[1]) < 1e-9, `${a} ≠ ${b}`);

test("origen, punto y centro", () => {
  const s = sample();
  assert.deepEqual(infer(s, [0.3, -0.2], 1), { p: [0, 0], kind: "origin", id: 0 });
  assert.equal(infer(s, [20.5, 0.5], 1).kind, "point");
  const c = infer(s, [50.4, 49.8], 1);
  assert.equal(c.kind, "center");
  assert.equal(c.id, 3);
});

test("punto medio de una línea", () => {
  const r = infer(sample(), [25.3, 0.6], 1);
  assert.equal(r.kind, "midpoint");
  assert.equal(r.entity, 10);
  near(r.p, [25, 0]);
});

test("cuadrantes del círculo y solo los del arco dentro de su barrido", () => {
  const s = sample();
  const q = infer(s, [50.2, 55.4], 1);
  assert.equal(q.kind, "quadrant");
  assert.equal(q.quadrant, 1);
  near(q.p, [50, 55]);
  assert.equal(infer(s, [44.6, 50.3], 1).quadrant, 2);
  // El arco va de 0° a 90°: su cuadrante de 180° no existe
  const a = infer(s, [-24.1, -20.2], 1);
  assert.notEqual(a.kind, "quadrant");
});

test("sobre una curva y libre", () => {
  const s = sample();
  const l = infer(s, [22, 0.4], 1);
  assert.equal(l.kind, "on_line");
  near(l.p, [22, 0]);
  const c = infer(s, [50 + 5 * Math.cos(0.6) + 0.3, 50 + 5 * Math.sin(0.6)], 1);
  assert.equal(c.kind, "on_circle");
  assert.equal(c.entity, 11);
  assert.deepEqual(infer(s, [100, 100], 1), { p: [100, 100], kind: "free" });
  // Fuera del segmento no hay "sobre la línea"
  assert.equal(infer(s, [35, 0.2], 1).kind, "free");
});

test("prioridad: el punto gana al medio aunque el medio esté más cerca", () => {
  const s = sample();
  // Línea corta (20,0)-(21,0): medio en 20,5; con tolerancia 1 llegan los dos
  s.points[2].x = 21;
  assert.equal(infer(s, [20.45, 0], 1).kind, "point");
  // A igual prioridad gana el más cercano
  assert.equal(infer(s, [20.9, 0], 1).id, 2);
});

test("excluir el punto que se arrastra", () => {
  assert.equal(infer(sample(), [0.1, 0], 1, [0]).kind, "free");
});
