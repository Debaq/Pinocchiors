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
  assert.deepEqual(p, { pieces: [[[-2, 4], [-1, 1], [0, 0], [1, 1], [2, 4]]], skipped: 0 });
  const c = sampleCurve({ kind: "parametric", fx: "r*cos(t)", fy: "r*sin(t)", from: 0, to: 2 * Math.PI, samples: 9 }, { r: 5 });
  for (const q of c.pieces[0]) near(Math.hypot(q[0], q[1]), 5);
  // Lo que no da número corta la curva (no la pierde entera)
  assert.deepEqual(sampleCurve({ kind: "explicit", fx: "sqrt(x)", from: -1, to: 1, samples: 3 }), { pieces: [[[0, 0], [1, 1]]], skipped: 1 });
  assert.match(sampleCurve({ kind: "explicit", fx: "sqrt(x)", from: -2, to: -1, samples: 3 }), /no da un número en x = -2/);
  assert.match(sampleCurve({ kind: "explicit", fx: "x +", from: 0, to: 1, samples: 3 }), /Fórmula/);
  assert.match(sampleCurve({ kind: "explicit", fx: "x", from: 1, to: 1, samples: 3 }), /intervalo/);
});

test("1/x: la división por cero parte la curva en dos tramos", () => {
  const r = sampleCurve({ kind: "explicit", fx: "1/x", from: -1, to: 1, samples: 5 });
  assert.equal(r.skipped, 1);
  assert.deepEqual(r.pieces, [[[-1, -1], [-0.5, -2]], [[0.5, 2], [1, 1]]]);
});

test("funciones con la cantidad justa de argumentos", () => {
  assert.throws(() => ev("sqrt(2, 5)"), /un argumento/);
  assert.throws(() => ev("pow(2)"), /2 argumentos/);
  near(ev("pow(2, 3)"), 8);
  near(ev("atan2(1, 1)"), Math.PI / 4);
});

test("parámetros con acentos o ñ; nada heredado de los objetos", () => {
  near(ev("diámetro / 2", { "diámetro": 10 }), 5);
  near(ev("2año", { "año": 3 }), 6);
  assert.throws(() => ev("constructor"), /no conozco «constructor»/);
  assert.throws(() => ev("toString(1)"), /función/);
});
