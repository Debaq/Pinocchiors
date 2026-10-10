// Pruebas del valor escrito al mover, copiar, girar y escalar (lib/sketchTyped.ts):
//   node --test apps/web/e2e/sketchTyped.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { parseTypedTransform as parse } from "../src/lib/sketchTyped.ts";

const close = (a, b) => assert.ok(Math.hypot(a[0] - b[0], a[1] - b[1]) < 1e-9, `${a} ≠ ${b}`);

test("mover: una distancia va hacia el cursor", () => {
  close(parse("move", "10", [0, 3]).delta, [0, 10]);
  close(parse("move", "2,5", [4, 0]).delta, [2.5, 0]);
  // Sin dirección: en x
  close(parse("copy", "7").delta, [7, 0]);
});

test("mover: dx; dy, con coma y espacio, con espacio y polar", () => {
  close(parse("move", "10; -2,5").delta, [10, -2.5]);
  close(parse("move", "10, 5").delta, [10, 5]);
  close(parse("move", "10 5").delta, [10, 5]);
  close(parse("move", "10<90").delta, [0, 10]);
});

test("girar y escalar; lo que no se entiende da un mensaje", () => {
  assert.deepEqual(parse("rotate", "-45°"), { degrees: -45 });
  assert.deepEqual(parse("scale", "x1,5"), { factor: 1.5 });
  assert.equal(typeof parse("scale", "0"), "string");
  assert.equal(typeof parse("move", "abc"), "string");
  assert.equal(typeof parse("rotate", ""), "string");
});
