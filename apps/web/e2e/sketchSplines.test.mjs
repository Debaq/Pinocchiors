// Pruebas de la edición de splines del sketch (lib/sketchSplines.ts), sin navegador:
//   node --test apps/web/e2e/sketchSplines.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { addEntity, addPoint } from "../src/lib/cad.ts";
import { evalAt, sample } from "../src/lib/sketchCurves.ts";
import { addParallelogram, convertToBSpline, curveOf, fitSplineToPoints, insertSplinePoint, removeSplinePoint, simplifySpline, splineCurvature, toggleSplineHandle } from "../src/lib/sketchSplines.ts";

function sketch() {
  return { origin: 0, points: [{ id: 0, x: 0, y: 0 }], entities: [], constraints: [] };
}
const near = (a, b, tol, what) => assert.ok(Math.abs(a - b) <= tol, `${what}: ${a} ≠ ${b}`);
const at = (s, id) => {
  const p = s.points.find((q) => q.id === id);
  return [p.x, p.y];
};
/** Mayor distancia de las muestras de `a` a la polilínea `b` */
function gap(a, b) {
  const seg = (p, u, v) => {
    const d = [v[0] - u[0], v[1] - u[1]];
    const l2 = d[0] ** 2 + d[1] ** 2 || 1;
    const t = Math.max(0, Math.min(1, ((p[0] - u[0]) * d[0] + (p[1] - u[1]) * d[1]) / l2));
    return Math.hypot(p[0] - u[0] - t * d[0], p[1] - u[1] - t * d[1]);
  };
  return Math.max(...a.map((p) => Math.min(...b.slice(1).map((q, i) => seg(p, b[i], q)))));
}

test("paralelogramo: cuarta esquina y lados opuestos paralelos", () => {
  const s = sketch();
  const [a, b, c] = [addPoint(s, [0, 0]), addPoint(s, [10, 0]), addPoint(s, [13, 5])];
  const l = addParallelogram(s, a, b, c);
  assert.equal(l.length, 4);
  const d = s.entities.find((e) => e.id === l[2]).geometry.end;
  assert.deepEqual(at(s, d), [3, 5]);
  assert.equal(s.constraints.filter((k) => k.type === "parallel").length, 2);
});

test("agregar un polo a una spline por polos: la forma no cambia", () => {
  const s = sketch();
  const poles = [[0, 0], [3, 6], [7, -2], [10, 3]].map((p) => addPoint(s, p));
  const id = addEntity(s, { type: "bspline", poles: [...poles], degree: 3 });
  const before = curveOf(s, id);
  assert.equal(insertSplinePoint(s, id, [5, 2]), undefined);
  const g = s.entities.find((e) => e.id === id).geometry;
  assert.equal(g.poles.length, 5);
  assert.equal(new Set(g.poles).size, 5);
  const after = curveOf(s, id);
  for (const u of [0, 0.2, 0.5, 0.77, 1]) {
    near(evalAt(after, u)[0], evalAt(before, u)[0], 1e-9, "x");
    near(evalAt(after, u)[1], evalAt(before, u)[1], 1e-9, "y");
  }
  // Las puntas siguen siendo las mismas
  assert.equal(g.poles[0], poles[0]);
  assert.equal(g.poles[4], poles[3]);
});

test("spline por puntos: agregar y quitar un punto, manija en el medio", () => {
  const s = sketch();
  const pts = [[0, 0], [5, 4], [10, 0], [15, 4]].map((p) => addPoint(s, p));
  const id = addEntity(s, { type: "spline", points: [...pts], closed: false });
  const g = s.entities.find((e) => e.id === id).geometry;
  assert.equal(insertSplinePoint(s, id, [7.5, 2.5]), undefined);
  assert.equal(g.points.length, 5);
  assert.equal(g.points.indexOf(pts[2]), 3, "el nuevo va entre el 1 y el 2");
  assert.equal(toggleSplineHandle(s, id, pts[2]), undefined);
  assert.deepEqual(g.handles.map((h) => h[0]), [3]);
  // Quitar el punto nuevo corre el índice de la manija
  assert.equal(removeSplinePoint(s, id, g.points[2]), undefined);
  assert.deepEqual(g.handles.map((h) => h[0]), [2]);
  // Sacar la manija borra su punto
  const h = g.handles[0][1];
  toggleSplineHandle(s, id, pts[2]);
  assert.equal(g.handles, undefined);
  assert.ok(!s.points.some((q) => q.id === h));
});

test("ajuste a puntos y simplificar", () => {
  const s = sketch();
  const ids = Array.from({ length: 30 }, (_, i) => addPoint(s, [i, 3 * Math.sin(i / 5)]));
  const id = fitSplineToPoints(s, ids, 0.01);
  assert.equal(typeof id, "number");
  const g = s.entities.find((e) => e.id === id).geometry;
  assert.ok(g.poles.length < 12, `${g.poles.length} polos`);
  assert.equal(g.poles[0], ids[0]);
  // Una spline por puntos con puntos de sobra sobre una recta: quedan las puntas
  const line = Array.from({ length: 6 }, (_, i) => addPoint(s, [i * 2, 50]));
  const sp = addEntity(s, { type: "spline", points: line, closed: false });
  const removed = simplifySpline(s, sp, 1e-6);
  assert.equal(removed, 4);
});

test("convertir a polos: línea + arco + spline en una cadena, misma forma", () => {
  const s = sketch();
  const [a, b] = [addPoint(s, [0, 0]), addPoint(s, [10, 0])];
  const l = addEntity(s, { type: "line", start: a, end: b });
  const c = addPoint(s, [10, 5]);
  const e = addPoint(s, [15, 5]);
  const arc = addEntity(s, { type: "arc", center: c, start: b, end: e });
  const q = [addPoint(s, [20, 8]), addPoint(s, [25, 5])];
  const sp = addEntity(s, { type: "spline", points: [e, ...q], closed: false });
  const old = sample(curveOf(s, sp), 16);
  const id = convertToBSpline(s, [arc, sp, l]);
  assert.equal(typeof id, "number");
  assert.equal(s.entities.length, 1);
  const g = s.entities[0].geometry;
  // Las puntas de la cadena (en cualquiera de los dos sentidos)
  assert.deepEqual([g.poles[0], g.poles[g.poles.length - 1]].sort(), [a, q[1]].sort());
  assert.ok(g.poles.includes(b) && g.poles.includes(e), "las uniones conservan su punto");
  const curve = curveOf(s, id);
  // El tramo de la spline queda exacto
  assert.ok(gap(old, sample(curve, 32)) < 1e-6);
  // La línea: el medio en (5, 0); el arco a radio 5 del centro
  const pl = sample(curve, 32);
  assert.ok(pl.some((p) => Math.abs(p[0] - 5) < 0.2 && Math.abs(p[1]) < 1e-9));
  const onArc = pl.filter((p) => p[0] > 10.01 && p[0] < 14.99 && p[1] < 5);
  for (const p of onArc) near(Math.hypot(p[0] - 10, p[1] - 5), 5, 5 * 3e-4, "radio");
  assert.ok([0, 25].includes(evalAt(curve, 0)[0]));
});

test("curvatura: una S tiene una inflexión", () => {
  const s = sketch();
  const id = addEntity(s, { type: "bspline", poles: [[0, 0], [2, 2], [4, -2], [6, 0]].map((p) => addPoint(s, p)), degree: 3 });
  const info = splineCurvature(s, id, 5);
  assert.equal(info.inflections.length, 1);
  assert.ok(info.comb.length > 10);
});
