// Pruebas de las curvas del sketch (lib/sketchCurves.ts), sin navegador:
//   node --test apps/web/e2e/sketchCurves.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { closestParam, curvature, curvatureInfo, derivs, domain, ellipseArcPolyline, evalAt, fitBSpline, hermiteToBSpline, insertKnot, makeBSpline, rhoWeight, sample } from "../src/lib/sketchCurves.ts";

const near = (a, b, tol, what) => assert.ok(Math.abs(a - b) <= tol, `${what}: ${a} ≠ ${b}`);

test("cúbica sujeta: pasa por los polos de las puntas, derivadas de Bézier", () => {
  const P = [[0, 0], [1, 2], [3, 2], [4, 0]];
  const s = makeBSpline({ poles: P, degree: 3 });
  const [a, b] = domain(s);
  assert.deepEqual(evalAt(s, a), [0, 0]);
  near(evalAt(s, b)[0], 4, 1e-12, "fin");
  const [, d1, d2] = derivs(s, a);
  near(d1[0], 3, 1e-9, "d1x");
  near(d1[1], 6, 1e-9, "d1y");
  near(d2[0], 6 * (0 - 2 + 3), 1e-9, "d2x");
  near(d2[1], 6 * (0 - 4 + 2), 1e-9, "d2y");
});

test("cuarto de círculo racional y parábola por rho 0,5", () => {
  const s = makeBSpline({ poles: [[1, 0], [1, 1], [0, 1]], weights: [1, Math.SQRT1_2, 1], degree: 2 });
  for (const u of [0, 0.3, 0.7, 1]) {
    near(Math.hypot(...evalAt(s, u)), 1, 1e-12, "radio");
    near(curvature(s, u), 1, 1e-9, "curvatura");
  }
  near(rhoWeight(0.5), 1, 1e-12, "parábola");
  const par = makeBSpline({ poles: [[-1, 1], [0, -1], [1, 1]], weights: [1, 1, 1], degree: 2 });
  // y = x² en una parábola de Bézier: el medio en (0, 0)
  const m = evalAt(par, 0.5);
  near(m[0], 0, 1e-12, "x");
  near(m[1], 0, 1e-12, "y");
});

test("arco de elipse antihorario", () => {
  const pl = ellipseArcPolyline([0, 0], [4, 0], [0, 2], [4, 0], [0, 2]);
  near(pl[0][0], 4, 1e-12, "inicio");
  near(pl[pl.length - 1][1], 2, 1e-12, "fin");
  for (const [x, y] of pl) near((x / 4) ** 2 + (y / 2) ** 2, 1, 1e-12, "sobre la elipse");
  assert.ok(pl[10][0] > 0 && pl[10][1] > 0, "primer cuadrante");
});

test("ajuste: pocos polos para puntos de una curva suave, dentro de la tolerancia", () => {
  const pts = Array.from({ length: 40 }, (_, i) => [i / 4, Math.sin(i / 8)]);
  const f = fitBSpline(pts, 1e-3);
  assert.ok(f.error <= 1e-3, `error ${f.error}`);
  assert.ok(f.poles.length < 15, `${f.poles.length} polos`);
  assert.deepEqual(f.poles[0], pts[0]);
});

test("Hermite a polos: la misma curva; insertar un nudo no la cambia", () => {
  const pts = [[0, 0], [5, 3], [10, 0]];
  const m = [[5, 3], [5, 0], [5, -3]];
  const h = hermiteToBSpline(pts, m, false);
  const s = makeBSpline({ poles: h.poles, degree: 3, knots: h.knots });
  // A mitad del primer tramo: Hermite en t = 0,5
  const t = 0.5;
  const [h00, h10, h01, h11] = [2 * t ** 3 - 3 * t ** 2 + 1, t ** 3 - 2 * t ** 2 + t, -2 * t ** 3 + 3 * t ** 2, t ** 3 - t ** 2];
  const want = [0, 1].map((k) => h00 * pts[0][k] + h10 * m[0][k] + h01 * pts[1][k] + h11 * m[1][k]);
  const got = evalAt(s, 0.5);
  near(got[0], want[0], 1e-12, "x");
  near(got[1], want[1], 1e-12, "y");
  const ins = insertKnot({ poles: h.poles, degree: 3, knots: h.knots }, 1.3);
  const s2 = makeBSpline({ poles: ins.poles, degree: 3, knots: ins.knots });
  assert.equal(ins.poles.length, h.poles.length + 1);
  for (const u of [0.2, 1.3, 1.7]) {
    near(evalAt(s2, u)[0], evalAt(s, u)[0], 1e-12, "igual x");
    near(evalAt(s2, u)[1], evalAt(s, u)[1], 1e-12, "igual y");
  }
});

test("curvatura: inflexión de una S y radio mínimo de un arco", () => {
  const s = makeBSpline({ poles: [[0, 0], [2, 2], [4, -2], [6, 0]], degree: 3 });
  const info = curvatureInfo(s, 5);
  assert.equal(info.inflections.length, 1);
  near(info.inflections[0][0], 3, 1e-6, "inflexión en el medio");
  const arc = makeBSpline({ poles: [[1, 0], [1, 1], [0, 1]], weights: [1, Math.SQRT1_2, 1], degree: 2 });
  near(curvatureInfo(arc, 1).minRadius, 1, 1e-9, "radio");
  const u = closestParam(arc, [2, 2]);
  near(Math.atan2(...evalAt(arc, u).reverse()), Math.PI / 4, 1e-6, "más cercano");
  assert.ok(sample(s, 8).length === 9);
});

test("cerrada periódica: sin costura", () => {
  const s = makeBSpline({ poles: [[1, 0], [0, 1], [-1, 0], [0, -1]], degree: 3, closed: true });
  const [a, b] = domain(s);
  near(evalAt(s, a)[0], evalAt(s, b)[0], 1e-12, "x");
  near(evalAt(s, a)[1], evalAt(s, b)[1], 1e-12, "y");
});
