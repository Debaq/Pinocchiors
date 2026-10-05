// Pruebas del texto del sketch (lib/sketchText.ts) con la fuente incluida:
//   node --test apps/web/e2e/sketchText.test.mjs
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import opentype from "opentype.js";
import { outlineContours } from "../src/lib/sketchText.ts";

const buf = readFileSync(new URL("../public/fonts/LiberationSans-Regular.ttf", import.meta.url));
const font = opentype.parse(buf.buffer.slice(buf.byteOffset, buf.byteOffset + buf.byteLength));
const contours = (text, size = 10, at = [0, 0]) => outlineContours(font.getPath(text, 0, 0, size).commands, at);

test("H: un contorno de rectas, cerrado", () => {
  const [c, ...rest] = contours("H");
  assert.equal(rest.length, 0);
  assert.ok("pieces" in c);
  assert.equal(c.pieces.length, 12);
  assert.ok(c.pieces.every((p) => p.kind === "line"));
  // Encadenado: cada tramo empieza donde terminó el anterior y el último vuelve al primero
  for (let i = 0; i < c.pieces.length; i++) assert.deepEqual(c.pieces[i].b, c.pieces[(i + 1) % c.pieces.length].a);
});

test("o: dos splines cerradas (afuera y el agujero)", () => {
  const cs = contours("o");
  assert.equal(cs.length, 2);
  assert.ok(cs.every((c) => "closed" in c && c.closed.length >= 8));
});

test("D: rectas y una spline abierta por la panza, sin quiebres dentro de la spline", () => {
  const [outer] = contours("D");
  assert.ok("pieces" in outer);
  const kinds = outer.pieces.map((p) => p.kind);
  assert.ok(kinds.includes("line") && kinds.includes("spline"), kinds.join());
});

test("posición y escala: la línea base en `at`, y hacia arriba", () => {
  const pts = contours("H", 20, [100, 50]).flatMap((c) => ("closed" in c ? c.closed : c.pieces.flatMap((p) => (p.kind === "line" ? [p.a, p.b] : p.points))));
  const ys = pts.map((p) => p[1]);
  assert.ok(Math.abs(Math.min(...ys) - 50) < 1e-9, `base ${Math.min(...ys)}`);
  // Alto de mayúscula de Liberation Sans ≈ 0,69 em
  assert.ok(Math.abs(Math.max(...ys) - 50 - 0.69 * 20) < 0.3, `alto ${Math.max(...ys) - 50}`);
  assert.ok(Math.min(...pts.map((p) => p[0])) >= 100);
});
