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

/** Fuente de prueba: cada letra es un cuadrado de `size` con avance `size` */
const boxFont = {
  getAdvanceWidth: (text, size) => [...text].length * size,
  getPath: (text, x, y, size) => ({
    commands: [...text].flatMap((_, i) => {
      const x0 = x + i * size;
      return [
        { type: "M", x: x0, y },
        { type: "L", x: x0 + size, y },
        { type: "L", x: x0 + size, y: y - size },
        { type: "L", x: x0, y: y - size },
        { type: "Z" },
      ];
    }),
  }),
};
const xs = (contours) => contours.flatMap((c) => c.pieces.flatMap((p) => [p.a[0], p.b[0]]));

test("alineación: izquierda, centro y derecha del ancla", async () => {
  const { layoutText } = await import("../src/lib/sketchText.ts");
  const at = [100, 0];
  assert.equal(Math.min(...xs(layoutText(boxFont, "ab", 10, at, "left"))), 100);
  assert.equal(Math.min(...xs(layoutText(boxFont, "ab", 10, at, "center"))), 90);
  assert.equal(Math.max(...xs(layoutText(boxFont, "ab", 10, at, "right"))), 100);
});

test("texto sobre una curva: cada letra sigue la dirección de la curva", async () => {
  const { layoutText } = await import("../src/lib/sketchText.ts");
  // Recta vertical: las letras quedan giradas 90° (la base sube por x = 0)
  const out = layoutText(boxFont, "ab", 10, [0, 0], "left", [[0, 0], [0, 100]]);
  assert.equal(out.length, 2);
  const pts = out[1].pieces.flatMap((p) => [p.a, p.b]);
  // La segunda letra va de y = 10 a 20, del lado izquierdo de la recta (x ≤ 0)
  assert.ok(pts.every(([x, y]) => x <= 1e-9 && x >= -10 - 1e-9 && y >= 10 - 1e-9 && y <= 20 + 1e-9), JSON.stringify(pts));
});
