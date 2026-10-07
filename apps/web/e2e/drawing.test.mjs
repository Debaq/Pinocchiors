// Pruebas del armado de planos (lib/drawing.ts), sin navegador:
//   node --test apps/web/e2e/drawing.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { layout, sheetSvg, SHEETS, viewSpec, scaleLabel, bounds } from "../src/lib/drawing.ts";

/** Rectángulo de w × h (como lo devuelve la proyección) */
const rect = (w, h) => [
  { kind: "visible", points: [[0, 0], [w, 0]] },
  { kind: "visible", points: [[w, 0], [w, h]] },
  { kind: "visible", points: [[w, h], [0, h]] },
  { kind: "visible", points: [[0, h], [0, 0]] },
];

test("vistas: frente desde −Y, planta desde +Z y lateral según el diedro", () => {
  assert.deepEqual(viewSpec("front", "first"), { eye: [0, -1, 0], xdir: [1, 0, 0] });
  assert.deepEqual(viewSpec("top", "first").eye, [0, 0, 1]);
  assert.deepEqual(viewSpec("side", "first").eye, [-1, 0, 0]);
  assert.deepEqual(viewSpec("side", "third").eye, [1, 0, 0]);
});

test("escala normalizada: la mayor con la que todo entra en la hoja", () => {
  const a4 = SHEETS[0];
  // Pieza de 40 × 20 × 10: a 2:1 entra en A4; una de 400 no, va a 1:5 o menos
  const small = { front: rect(40, 10), top: rect(40, 20), side: rect(20, 10) };
  assert.equal(scaleLabel(layout(small, a4, "first").scale), "2:1");
  const big = { front: rect(400, 100), top: rect(400, 200), side: rect(200, 100) };
  const s = layout(big, a4, "first").scale;
  assert.ok(s <= 1 / 5, scaleLabel(s));
  assert.equal(scaleLabel(1 / 20), "1:20");
});

test("primer diedro: la planta va debajo del frente; tercero: arriba", () => {
  const views = { front: rect(40, 10), top: rect(40, 20) };
  const first = layout(views, SHEETS[1], "first", 1).placed;
  const third = layout(views, SHEETS[1], "third", 1).placed;
  const y = (pl, name) => pl.find((p) => p.name === name).y;
  assert.ok(y(first, "top") > y(first, "front"), "primer diedro");
  assert.ok(y(third, "top") < y(third, "front"), "tercer diedro");
});

test("SVG: una polilínea por línea, ocultas solo si se piden, y cajetín", () => {
  const views = { front: [...rect(40, 10), { kind: "hidden", points: [[10, 0], [10, 10]] }] };
  const { placed, scale } = layout(views, SHEETS[0], "first");
  const info = { title: "Soporte <A>", date: "2026-10-07", scale: scaleLabel(scale), projection: "first", sheet: "A4" };
  const with_ = sheetSvg(placed, SHEETS[0], info, { hidden: true, smooth: false });
  const without = sheetSvg(placed, SHEETS[0], info, { hidden: false, smooth: false });
  assert.equal((with_.match(/<polyline/g) ?? []).length, 5);
  assert.equal((without.match(/<polyline/g) ?? []).length, 4);
  assert.ok(with_.includes('viewBox="0 0 297 210"'));
  assert.ok(with_.includes("Soporte &lt;A&gt;"), "el título se escapa");
  assert.ok(with_.includes("Primer diedro"));
  // A escala: el frente mide 40 × escala en la hoja
  const pts = [...with_.matchAll(/data-kind="visible" points="([^"]+)"/g)].flatMap((m) => m[1].split(" ").map((p) => p.split(",").map(Number)));
  const xs = pts.map((p) => p[0]);
  assert.ok(Math.abs(Math.max(...xs) - Math.min(...xs) - 40 * scale) < 1e-6);
  assert.deepEqual(bounds(views.front).max, [40, 10]);
});
