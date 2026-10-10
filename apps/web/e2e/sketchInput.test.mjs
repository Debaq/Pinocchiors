// Pruebas de las coordenadas escritas y la rejilla del sketch
// (lib/sketchInput.ts), sin navegador:
//   node --test apps/web/e2e/sketchInput.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { gridSegments, parseCoords, snapToGrid } from "../src/lib/sketchInput.ts";

const close = (a, b) => assert.ok(Math.hypot(a[0] - b[0], a[1] - b[1]) < 1e-9, `${a} ≠ ${b}`);

test("coordenadas absolutas, relativas y polares", () => {
  assert.deepEqual(parseCoords("10, 20", [5, 5]), [10, 20]);
  assert.deepEqual(parseCoords("10 -20", undefined), [10, -20]);
  assert.deepEqual(parseCoords("2,5; 3", undefined), [2.5, 3]);
  assert.deepEqual(parseCoords("@10, 0", [5, 5]), [15, 5]);
  close(parseCoords("@10<90", [5, 5]), [5, 15]);
  close(parseCoords("20 < 180", [5, 5]), [-20, 0]);
  close(parseCoords("@10<-45", undefined), [Math.SQRT1_2 * 10, -Math.SQRT1_2 * 10]);
});

test("lo que no se entiende devuelve un mensaje", () => {
  for (const t of ["", "10", "a, b", "1, 2, 3", "@<45", "1,5,2"]) assert.equal(typeof parseCoords(t, [0, 0]), "string", t);
});

test("anclaje a la rejilla", () => {
  assert.deepEqual(snapToGrid([7.4, -2.6], 5), [5, -5]);
  assert.deepEqual(snapToGrid([0.31, 0.29], 0.1), [0.3, 0.3]);
  assert.deepEqual(snapToGrid([-0.1, 0.1], 5), [0, 0]);
  assert.deepEqual(snapToGrid([1.2, 3.4], 0), [1.2, 3.4]);
});

test("líneas de la rejilla: cubren la caja, cada 5 principal y con tope", () => {
  const segs = gridSegments([-12, -3, 12, 3], 5);
  const xs = segs.filter((s) => s.a[0] === s.b[0]).map((s) => s.a[0]);
  assert.deepEqual(xs, [-15, -10, -5, 0, 5, 10, 15]);
  assert.deepEqual(segs.filter((s) => s.major).map((s) => s.a[0] === s.b[0] ? `x${s.a[0]}` : `y${s.a[1]}`).sort(), ["x0", "y0"]);
  // Caja enorme: el paso crece para no pasar de 200 por eje
  const big = gridSegments([0, 0, 10000, 10], 1);
  assert.ok(big.filter((s) => s.a[0] === s.b[0]).length <= 202);
});
