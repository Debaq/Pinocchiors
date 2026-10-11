// Pruebas de alinear el escaneo por rasgos (lib/scanAlign.ts), sin navegador:
//   node --test apps/web/e2e/scanAlign.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { applyMat, axisToVertical, faceToFloor, faceToFront, pointToOrigin, toSceneMatrix } from "../src/lib/scanAlign.ts";

const near = (a, b, tol = 1e-9) => a.forEach((v, i) => assert.ok(Math.abs(v - b[i]) <= tol, `${a} ≠ ${b}`));
const dirOf = (m, d) => {
  const o = applyMat(m, [0, 0, 0]);
  const p = applyMat(m, d);
  return [p[0] - o[0], p[1] - o[1], p[2] - o[2]];
};

test("cara al piso: la normal queda hacia abajo y la cara en z = 0", () => {
  const plane = { origin: [3, 4, 5], normal: [Math.SQRT1_2, 0, Math.SQRT1_2], x_dir: [Math.SQRT1_2, 0, -Math.SQRT1_2] };
  const m = faceToFloor(plane);
  near(dirOf(m, plane.normal), [0, 0, -1]);
  assert.ok(Math.abs(applyMat(m, plane.origin)[2]) < 1e-9);
  // Otro punto de la cara también queda en el piso
  const q = [3 + plane.x_dir[0] * 10, 4, 5 + plane.x_dir[2] * 10];
  assert.ok(Math.abs(applyMat(m, q)[2]) < 1e-9);
  // Una cara que ya mira hacia abajo solo baja
  const down = faceToFloor({ origin: [1, 2, 7], normal: [0, 0, -1], x_dir: [1, 0, 0] });
  near(applyMat(down, [1, 2, 7]), [1, 2, 0]);
  // Una que mira hacia arriba da media vuelta
  const up = faceToFloor({ origin: [0, 0, 7], normal: [0, 0, 1], x_dir: [1, 0, 0] });
  near(dirOf(up, [0, 0, 1]), [0, 0, -1]);
});

test("cara al frente: gira solo en torno al eje vertical", () => {
  const plane = { origin: [10, 0, 2], normal: [1, 0, 0], x_dir: [0, 1, 0] };
  const m = faceToFront(plane);
  near(dirOf(m, [1, 0, 0]), [0, -1, 0]);
  near(dirOf(m, [0, 0, 1]), [0, 0, 1]);
  near(applyMat(m, plane.origin), plane.origin);
  assert.equal(faceToFront({ origin: [0, 0, 0], normal: [0, 0, 1], x_dir: [1, 0, 0] }), null);
});

test("eje vertical por el origen", () => {
  const m = axisToVertical([5, 6, 7], [0, -1, 0]);
  // Un eje no tiene sentido: queda vertical para un lado o el otro
  near(dirOf(m, [0, 1, 0]).map(Math.abs), [0, 0, 1]);
  const a = applyMat(m, [5, 6, 7]);
  const b = applyMat(m, [5, 16, 7]);
  near([a[0], a[1], b[0], b[1]], [0, 0, 0, 0]);
});

test("punto al origen", () => {
  near(applyMat(pointToOrigin([1, -2, 3]), [1, -2, 3]), [0, 0, 0]);
});

test("del CAD al modelo: Y arriba y sus unidades", () => {
  // Bajar 5 mm en el CAD = bajar 0,005 en Y de un modelo en metros
  const s = toSceneMatrix(pointToOrigin([0, 0, 5]), 1000);
  near(applyMat(s, [0, 0.005, 0]), [0, 0, 0]);
  // Correr 2 mm en Y del CAD (hacia atrás) = −Z del modelo
  near(applyMat(toSceneMatrix(pointToOrigin([0, 2, 0]), 1), [0, 0, -2]), [0, 0, 0]);
});
