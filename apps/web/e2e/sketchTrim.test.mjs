// Pruebas de recortar y extender splines y elipses, y del límite elegido a
// mano (lib/sketchTrim.ts), sin navegador:
//   node --test apps/web/e2e/sketchTrim.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { addEntity, addPoint, splinePolyline, trimAt } from "../src/lib/cad.ts";
import { polylineOf } from "../src/lib/sketchCheck.ts";
import { trimByStroke } from "../src/lib/sketchEdit.ts";
import { bsplineToThrough, extendAny, trimAny } from "../src/lib/sketchTrim.ts";

function sketch() {
  return { origin: 0, points: [{ id: 0, x: 0, y: 0 }], entities: [], constraints: [] };
}
const at = (s, id) => {
  const p = s.points.find((q) => q.id === id);
  return [p.x, p.y];
};
const line = (s, a, b) => addEntity(s, { type: "line", start: addPoint(s, a), end: addPoint(s, b) });
const spline = (s, pts) => addEntity(s, { type: "spline", points: pts.map((p) => addPoint(s, p)), closed: false });
const geo = (s, id) => s.entities.find((e) => e.id === id).geometry;
const near = (a, b, tol = 1e-6) => Math.hypot(a[0] - b[0], a[1] - b[1]) <= tol;
/** Distancia de `p` a la polilínea */
const toPoly = (pl, p) => {
  let best = Infinity;
  for (let i = 0; i + 1 < pl.length; i++) {
    const [a, b] = [pl[i], pl[i + 1]];
    const d = [b[0] - a[0], b[1] - a[1]];
    const t = Math.max(0, Math.min(1, ((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / (d[0] ** 2 + d[1] ** 2 || 1)));
    best = Math.min(best, Math.hypot(p[0] - a[0] - t * d[0], p[1] - a[1] - t * d[1]));
  }
  return best;
};
/** Referencia fina de una spline por puntos (la polilínea de 16 por tramo erra ~0,01 en las crestas) */
const fine = (s, g) => splinePolyline(g.points.map((id) => at(s, id)), g.closed, undefined, undefined, 256);
const WAVE = [[0, 0], [5, 5], [10, 0], [15, 5], [20, 0]];

test("recortar una spline por puntos entre dos líneas: quedan dos tramos por polos con la misma forma", () => {
  const s = sketch();
  const sp = spline(s, WAVE);
  const before = fine(s, geo(s, sp));
  const geo0 = [...geo(s, sp).points];
  line(s, [7, -10], [7, 10]);
  line(s, [13, -10], [13, 10]);
  assert.equal(trimAny(s, sp, [10, 0]), undefined);
  const pieces = s.entities.filter((e) => e.geometry.type === "bspline");
  assert.equal(pieces.length, 2);
  // La entidad vieja se queda con el primer tramo, que empieza en la punta de siempre
  const g0 = geo(s, sp);
  assert.equal(g0.points, undefined);
  assert.ok(near(at(s, g0.poles[0]), [0, 0]));
  assert.ok(Math.abs(at(s, g0.poles.at(-1))[0] - 7) < 1e-6);
  const g1 = pieces.find((e) => e.id !== sp).geometry;
  assert.ok(Math.abs(at(s, g1.poles[0])[0] - 13) < 1e-6);
  assert.ok(near(at(s, g1.poles.at(-1)), [20, 0]));
  // Los puntos de paso que quedaron son uniones de la curva nueva (mismo id)
  assert.ok(g0.poles.includes(geo0[1]) && g1.poles.includes(geo0[3]));
  // La forma no cambia (lo que queda está sobre la curva de antes)
  for (const e of pieces) for (const p of polylineOf(s, e.geometry)) assert.ok(toPoly(before, p) < 0.01, `se corrió ${toPoly(before, p)}`);
  // Las líneas se parten en el corte: el extremo de la spline es su punto
  const verticals = s.entities.filter((e) => e.geometry.type === "line");
  assert.equal(verticals.length, 4);
  assert.ok(verticals.some((e) => [e.geometry.start, e.geometry.end].includes(g0.poles.at(-1))));
});

test("recortar una elipse: pasa a arco de elipse (lo de arriba se va)", () => {
  const s = sketch();
  const c = addPoint(s, [0, 0]);
  const el = addEntity(s, { type: "ellipse", center: c, major: addPoint(s, [10, 0]), minor: addPoint(s, [0, 5]) });
  line(s, [-20, 0], [20, 0]);
  assert.equal(trimAny(s, el, [0, 5]), undefined);
  const g = geo(s, el);
  assert.equal(g.type, "ellipse_arc");
  for (const p of polylineOf(s, g)) assert.ok(p[1] <= 1e-6, `queda arriba ${p}`);
  // Las puntas en ±10
  assert.deepEqual([at(s, g.start)[0], at(s, g.end)[0]].map(Math.round).sort((a, b) => a - b), [-10, 10]);
});

test("recortar un arco de elipse en el medio: se parte en dos", () => {
  const s = sketch();
  const c = addPoint(s, [0, 0]);
  const arc = addEntity(s, { type: "ellipse_arc", center: c, major: addPoint(s, [10, 0]), minor: addPoint(s, [0, 5]), start: addPoint(s, [10, 0]), end: addPoint(s, [-10, 0]) });
  line(s, [-3, -10], [-3, 10]);
  line(s, [3, -10], [3, 10]);
  assert.equal(trimAny(s, arc, [0, 5]), undefined);
  const arcs = s.entities.filter((e) => e.geometry.type === "ellipse_arc");
  assert.equal(arcs.length, 2);
  for (const e of arcs) for (const p of polylineOf(s, e.geometry)) assert.ok(Math.abs(p[0]) >= 3 - 1e-3, `queda en el medio ${p}`);
});

test("una línea se recorta contra una spline (y queda pegada a ella)", () => {
  const s = sketch();
  const sp = spline(s, WAVE);
  const l = line(s, [-5, 2.5], [25, 2.5]);
  // Cruza la onda cuatro veces; el clic en x=10 (abajo la onda está en 0) quita el tramo de ese valle
  assert.equal(trimAt(s, l, [10, 2.5]), undefined);
  const lines = s.entities.filter((e) => e.geometry.type === "line");
  assert.equal(lines.length, 2);
  const onCurve = s.constraints.filter((k) => k.type === "point_on_curve" && k.curve === sp);
  assert.equal(onCurve.length, 2);
  const xs = onCurve.map((k) => at(s, k.point)[0]).sort((a, b) => a - b);
  assert.ok(xs[0] > 5 && xs[0] < 10 && xs[1] > 10 && xs[1] < 15, `cortes en ${xs}`);
});

test("recortar una spline por polos: el tramo es la misma curva", () => {
  const s = sketch();
  const bs = addEntity(s, { type: "bspline", poles: [[0, 0], [3, 6], [7, 6], [10, 0]].map((p) => addPoint(s, p)), degree: 3 });
  const before = polylineOf(s, geo(s, bs));
  line(s, [5, -10], [5, 10]);
  assert.equal(trimAny(s, bs, [8, 3]), undefined);
  const g = geo(s, bs);
  assert.equal(g.type, "bspline");
  assert.ok(near(at(s, g.poles[0]), [0, 0]));
  assert.ok(Math.abs(at(s, g.poles.at(-1))[0] - 5) < 1e-6);
  for (const p of polylineOf(s, g)) assert.ok(toPoly(before, p) < 0.01, `se corrió ${toPoly(before, p)}`);
  for (const p of polylineOf(s, g)) assert.ok(p[0] <= 5 + 1e-6, `pasa del corte ${p}`);
});

test("recortar una spline por puntos cerrada: queda el tramo de afuera, entero y exacto", () => {
  const s = sketch();
  const sp = addEntity(s, { type: "spline", points: [[10, 0], [0, 10], [-10, 0], [0, -10]].map((p) => addPoint(s, p)), closed: true });
  const before = fine(s, geo(s, sp));
  line(s, [-3, -20], [-3, 20]);
  line(s, [3, -20], [3, 20]);
  // Clic arriba: se va lo de arriba entre las dos líneas; queda una sola curva que da la vuelta por abajo
  assert.equal(trimAny(s, sp, [0, 10]), undefined);
  const curves = s.entities.filter((e) => e.geometry.type === "bspline");
  assert.equal(curves.length, 1);
  const pl = polylineOf(s, curves[0].geometry);
  for (const p of pl) assert.ok(toPoly(before, p) < 0.01, `se corrió ${toPoly(before, p)}`);
  assert.ok(pl.some((p) => p[1] < -9), "pasa por abajo");
  assert.ok(!pl.some((p) => p[1] > 9), "lo de arriba se fue");
});

test("sin cruces la spline se borra entera; una cerrada necesita dos", () => {
  const s = sketch();
  const sp = spline(s, WAVE);
  assert.equal(trimAny(s, sp, [10, 0]), undefined);
  assert.equal(s.entities.length, 0);
  const t = sketch();
  const c = addPoint(t, [0, 0]);
  const el = addEntity(t, { type: "ellipse", center: c, major: addPoint(t, [10, 0]), minor: addPoint(t, [0, 5]) });
  line(t, [0, 0], [20, 0]);
  assert.match(trimAny(t, el, [0, 5]), /dos cruces/);
});

test("límite elegido a mano: solo lo elegido corta", () => {
  const s = sketch();
  const h = line(s, [0, 0], [20, 0]);
  const a = line(s, [5, -5], [5, 5]);
  const b = line(s, [10, -5], [10, 5]);
  line(s, [15, -5], [15, 5]);
  // Con todo: el clic en 12 quita de 10 a 15
  const all = structuredClone(s);
  trimAny(all, h, [12, 0]);
  const xs = (k) => k.entities.filter((e) => e.geometry.type === "line" && at(k, e.geometry.start)[1] === 0 && at(k, e.geometry.end)[1] === 0).map((e) => [at(k, e.geometry.start)[0], at(k, e.geometry.end)[0]].sort((p, q) => p - q));
  assert.deepEqual(xs(all).sort((p, q) => p[0] - q[0]), [[0, 10], [15, 20]]);
  // Solo con la de x=5: se va de 5 hasta el final, y las otras no se tocan
  assert.equal(trimAny(s, h, [12, 0], [], [a]), undefined);
  assert.deepEqual(xs(s), [[0, 5]]);
  assert.ok(!s.entities.find((e) => e.id === b).construction, "la marca de construcción vuelve como estaba");
  assert.equal(s.entities.filter((e) => e.geometry.type === "line").length, 5);
});

test("extender una spline hasta una línea sigue la tangente de la punta", () => {
  const s = sketch();
  const sp = spline(s, [[0, 0], [5, 3], [10, 0]]);
  line(s, [15, -20], [15, 20]);
  assert.equal(extendAny(s, sp, [9, 0]), undefined);
  const g = geo(s, sp);
  assert.equal(g.points.length, 4);
  const p = at(s, g.points[3]);
  assert.ok(Math.abs(p[0] - 15) < 1e-6 && p[1] < 0, `punta nueva en ${p}`);
  // Desde el comienzo también (el punto nuevo va primero)
  line(s, [-6, -20], [-6, 20]);
  assert.equal(extendAny(s, sp, [1, 0]), undefined);
  assert.ok(Math.abs(at(s, geo(s, sp).points[0])[0] + 6) < 1e-6);
});

test("extender un arco de elipse sigue la elipse hasta lo que cruza", () => {
  const s = sketch();
  const c = addPoint(s, [0, 0]);
  const arc = addEntity(s, { type: "ellipse_arc", center: c, major: addPoint(s, [10, 0]), minor: addPoint(s, [0, 5]), start: addPoint(s, [10, 0]), end: addPoint(s, [0, 5]) });
  line(s, [-20, 2], [20, 2]);
  line(s, [-20, -2], [20, -2]);
  // El fin (arriba) avanza antihorario hasta y=2 del lado izquierdo
  assert.equal(extendAny(s, arc, [0, 5]), undefined);
  const e = at(s, geo(s, arc).end);
  assert.ok(e[0] < 0 && Math.abs(e[1] - 2) < 0.05, `fin en ${e}`);
  // El comienzo retrocede hasta y=−2 del lado derecho
  assert.equal(extendAny(s, arc, [10, 0]), undefined);
  const b = at(s, geo(s, arc).start);
  assert.ok(b[0] > 0 && Math.abs(b[1] + 2) < 0.05, `comienzo en ${b}`);
});

test("pasar una spline por polos a puntos de paso: misma curva, mismas puntas", () => {
  const s = sketch();
  const poles = [[0, 0], [3, 6], [7, 6], [10, 0], [14, -4], [18, 0]].map((p) => addPoint(s, p));
  const bs = addEntity(s, { type: "bspline", poles, degree: 3 });
  const before = polylineOf(s, geo(s, bs));
  assert.equal(bsplineToThrough(s, bs), undefined);
  const g = geo(s, bs);
  assert.equal(g.type, "spline");
  assert.equal(g.points[0], poles[0]);
  assert.equal(g.points.at(-1), poles.at(-1));
  for (const p of polylineOf(s, g)) assert.ok(toPoly(before, p) < 0.15, `se corrió ${toPoly(before, p)}`);
  // Los polos de adentro que no son de la curva se van
  assert.ok(!s.points.some((q) => q.id === poles[1]));
});

test("el trazo de recortar también corta splines", () => {
  const s = sketch();
  const sp = spline(s, WAVE);
  line(s, [7, -10], [7, 10]);
  line(s, [13, -10], [13, 10]);
  const r = trimByStroke(s, [[10, -1], [10, 1]]);
  assert.equal(r.trimmed, 1);
  assert.equal(r.message, undefined);
  assert.equal(s.entities.filter((e) => e.geometry.type === "bspline").length, 2);
  assert.ok(s.entities.some((e) => e.id === sp));
});
