// Calce pieza ↔ escaneo (lib/cad.ts: fitClass y fitReport), sin navegador:
//   node --test apps/web/e2e/scanFit.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { fitClass, fitReport } from "../src/lib/cad.ts";

const range = { min: 0.1, max: 0.5 };

test("la holgura firmada cae en una de cuatro clases", () => {
  assert.equal(fitClass(-0.01, range), "interference");
  assert.equal(fitClass(0, range), "tight");
  assert.equal(fitClass(0.099, range), "tight");
  assert.equal(fitClass(0.1, range), "ok");
  assert.equal(fitClass(0.5, range), "ok");
  assert.equal(fitClass(0.51, range), "loose");
});

test("el informe pesa por área y dice dónde están la menor y la mayor holgura", () => {
  // Cuatro triángulos: choque (área 1), apretado (1), en rango (2), suelto (4)
  const perFace = [-0.2, 0.05, 0.3, 1.5];
  const areas = [1, 1, 2, 4];
  const centroids = [[0, 0, 0], [1, 0, 0], [2, 0, 0], [3, 0, 0]];
  const r = fitReport(perFace, areas, centroids, range);
  assert.ok(r);
  assert.deepEqual(r.share, { interference: 1 / 8, tight: 1 / 8, ok: 2 / 8, loose: 4 / 8 });
  assert.deepEqual(r.min, { value: -0.2, at: [0, 0, 0] });
  assert.deepEqual(r.max, { value: 1.5, at: [3, 0, 0] });
  assert.deepEqual(r.interference, [0]);
});

test("sin triángulos no hay informe", () => {
  assert.equal(fitReport([], [], [], range), null);
});
