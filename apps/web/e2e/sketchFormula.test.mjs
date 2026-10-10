// Pruebas de las curvas por ecuación (lib/sketchFormula.ts), sin navegador:
//   node --test apps/web/e2e/sketchFormula.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { evalFormula, parseFormula, sampleCurve } from "../src/lib/sketchFormula.ts";

const ev = (t, v = {}) => evalFormula(parseFormula(t), v);
const near = (a, b) => assert.ok(Math.abs(a - b) < 1e-9, `${a} ≠ ${b}`);

test("operaciones, precedencia, potencia a la derecha y funciones", () => {
  near(ev("1 + 2 * 3"), 7);
  near(ev("-2^2"), -4);
  near(ev("2^3^2"), 512);
  near(ev("2 ** 3"), 8);
  near(ev("max(1, 2*3, 4)"), 6);
  near(ev("sin(pi/2) + cos(0)"), 2);
  near(ev("2x + 1", { x: 3 }), 7);
  near(ev("2(x+1)", { x: 3 }), 8);
  near(ev("ancho / 2", { ancho: 30 }), 15);
  near(ev("1.5e2"), 150);
});

test("errores con mensaje", () => {
  for (const [t, msg] of [["1 +", /incompleta/], ["foo(2)", /función/], ["(1", /falta/], ["2 3", /sobra/], ["x y", /sobra/]]) assert.throws(() => ev(t, { x: 1, y: 1 }), msg, t);
  assert.throws(() => ev("q + 1"), /no conozco «q»/);
});

test("muestrear: explícita, paramétrica (círculo) y lo que no da número", () => {
  const p = sampleCurve({ kind: "explicit", fx: "x^2", from: -2, to: 2, samples: 5 });
  assert.deepEqual(p, [[-2, 4], [-1, 1], [0, 0], [1, 1], [2, 4]]);
  const c = sampleCurve({ kind: "parametric", fx: "r*cos(t)", fy: "r*sin(t)", from: 0, to: 2 * Math.PI, samples: 9 }, { r: 5 });
  for (const q of c) near(Math.hypot(q[0], q[1]), 5);
  assert.match(sampleCurve({ kind: "explicit", fx: "sqrt(x)", from: -1, to: 1, samples: 3 }), /no da un número en x = -1/);
  assert.match(sampleCurve({ kind: "explicit", fx: "x +", from: 0, to: 1, samples: 3 }), /Fórmula/);
  assert.match(sampleCurve({ kind: "explicit", fx: "x", from: 1, to: 1, samples: 3 }), /intervalo/);
});
