// Pruebas de las vistas como referencia del dibujo sobre la hoja
// (lib/sheetRefs.ts y los límites de recortar/extender de lib/cad.ts):
//   node --test apps/web/e2e/sheetRefs.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { addEntity, addPoint, addProjected, emptySketch, extendLine, trimAt } from "../src/lib/cad.ts";
import { applySheetLinks, nearestViewLine, projectViewLine, sheetSketchDims, sheetToView, viewPolylines, viewToSheet } from "../src/lib/sheetRefs.ts";

const H = 210;
/** Vista con el origen de la pieza en (x, y) de la hoja (SVG) a escala k */
const view = (lines, k = 2, x = 50, y = 150, name = "front") => ({ name, x, y, scale: k, lines });
const seg = (a, b, kind = "visible") => ({ kind, points: [a, b] });
const ring = (c, r, n = 32, from = 0, to = 2 * Math.PI) => ({
  kind: "visible",
  points: Array.from({ length: n + 1 }, (_, i) => [c[0] + r * Math.cos(from + ((to - from) * i) / n), c[1] + r * Math.sin(from + ((to - from) * i) / n)]),
});
const near = (a, b, tol = 1e-9) => assert.ok(Math.abs(a - b) <= tol, `${a} ≠ ${b}`);
const at = (s, id) => {
  const p = s.points.find((q) => q.id === id);
  return [p.x, p.y];
};

test("vista ↔ hoja (mm, Y hacia arriba): ida y vuelta", () => {
  const v = view([]);
  const q = viewToSheet(v, [10, 5], H);
  assert.deepEqual(q, [70, H - 150 + 10]);
  assert.deepEqual(sheetToView(v, q, H), [10, 5]);
});

test("línea de vista como curva: recta, círculo y arco antihorario", () => {
  const v = view([seg([0, 0], [20, 0]), ring([10, 10], 4), ring([0, 0], 5, 16, Math.PI / 2, 0)]);
  const l = projectViewLine(v, v.lines[0], H);
  assert.equal(l.kind, "line");
  assert.deepEqual([l.start, l.end], [viewToSheet(v, [0, 0], H), viewToSheet(v, [20, 0], H)]);
  const c = projectViewLine(v, v.lines[1], H);
  assert.equal(c.kind, "circle");
  near(c.radius, 8, 1e-6);
  // El arco va de 90° a 0° (horario): queda antihorario de 0° a 90°
  const a = projectViewLine(v, v.lines[2], H);
  assert.equal(a.kind, "arc");
  near(a.start[0], viewToSheet(v, [5, 0], H)[0], 1e-6);
  near(a.end[1], viewToSheet(v, [0, 5], H)[1], 1e-6);
});

test("la línea más cercana y las polilíneas (ocultas solo si se dibujan)", () => {
  const v = view([seg([0, 0], [20, 0]), seg([0, 10], [20, 10], "hidden")]);
  const hit = nearestViewLine([v], H, viewToSheet(v, [10, 0.5], H), 3);
  assert.equal(hit.line, v.lines[0]);
  assert.deepEqual(hit.at, [10, 0]);
  assert.equal(nearestViewLine([v], H, viewToSheet(v, [10, 10], H), 3, false), undefined);
  assert.equal(viewPolylines([v], H, true).length, 2);
  assert.equal(viewPolylines([v], H, false).length, 1);
});

test("lo ligado sigue a su línea: otra escala, otro lugar; si desaparece se cuenta como perdido", () => {
  const v1 = view([seg([0, 0], [20, 0])]);
  const s = emptySketch();
  const [e] = addProjected(s, [projectViewLine(v1, v1.lines[0], H)], () => ({}));
  const links = [{ entity: e, view: "front", at: [10, 0] }];
  // La pieza se alargó a 30 y la hoja pasó a 1:1 en otro lugar
  const v2 = view([seg([0, 0], [30, 0])], 1, 20, 100);
  const r = applySheetLinks(s, links, [v2], H);
  assert.equal(r.lost, 0);
  const g = r.sketch.entities.find((x) => x.id === e).geometry;
  assert.deepEqual([at(r.sketch, g.start), at(r.sketch, g.end)], [viewToSheet(v2, [0, 0], H), viewToSheet(v2, [30, 0], H)]);
  assert.deepEqual(at(s, s.entities[0].geometry.end), viewToSheet(v1, [20, 0], H), "no cambia el sketch de entrada");
  // Sin la vista: perdido, queda donde estaba
  const gone = applySheetLinks(s, links, [], H);
  assert.equal(gone.lost, 1);
  assert.equal(gone.links.length, 1);
  // Con el vínculo roto (ya no está en uses) se olvida
  const broken = { ...s, uses: [] };
  assert.equal(applySheetLinks(broken, links, [v2], H).links.length, 0);
});

test("recortar y extender contra las líneas de las vistas (límites de afuera, el punto queda fijo)", () => {
  const s = emptySketch();
  const l = addEntity(s, { type: "line", start: addPoint(s, [0, 0]), end: addPoint(s, [100, 0]) });
  const limits = [[[30, -10], [30, 10]], [[70, -10], [70, 10]]];
  // Sin límites: no hay cruces y se borra entera; con ellos se quita el tramo del medio
  const copy = structuredClone(s);
  trimAt(copy, l, [50, 0]);
  assert.equal(copy.entities.length, 0);
  assert.equal(trimAt(s, l, [50, 0], limits), undefined);
  const xs = s.entities.flatMap((e) => [at(s, e.geometry.start)[0], at(s, e.geometry.end)[0]]).sort((a, b) => a - b);
  assert.deepEqual(xs, [0, 30, 70, 100]);
  assert.equal(s.constraints.filter((k) => k.type === "fixed").length, 2);
  // Extender una línea corta hasta el límite
  const t = emptySketch();
  const m = addEntity(t, { type: "line", start: addPoint(t, [0, 5]), end: addPoint(t, [10, 5]) });
  assert.match(extendLine(t, m, [10, 5]), /No hay nada/);
  assert.equal(extendLine(t, m, [10, 5], limits), undefined);
  assert.deepEqual(at(t, t.entities[0].geometry.end), [30, 5]);
  // Un círculo partido por dos límites
  const c = emptySketch();
  const circle = addEntity(c, { type: "circle", center: addPoint(c, [50, 0]), radius: 30 });
  const tall = [[[30, -40], [30, 40]], [[70, -40], [70, 40]]];
  assert.equal(trimAt(c, circle, [80, 0], tall), undefined);
  assert.equal(c.entities[0].geometry.type, "arc");
  // Se quitó el tramo de la derecha (entre los cruces de x = 70)
  const arc = c.entities[0].geometry;
  near(at(c, arc.start)[0], 70, 1e-9);
  near(at(c, arc.end)[0], 70, 1e-9);
});

test("cotas del dibujo en la hoja: largo, horizontal, radio, diámetro y ángulo (en coordenadas del SVG)", () => {
  const s = emptySketch();
  const a = addPoint(s, [10, 20]);
  const b = addPoint(s, [40, 60]);
  const l = addEntity(s, { type: "line", start: a, end: b });
  const h = addEntity(s, { type: "line", start: addPoint(s, [0, 0]), end: addPoint(s, [50, 0]) });
  const c = addEntity(s, { type: "circle", center: addPoint(s, [100, 100]), radius: 7 });
  s.constraints.push(
    { type: "length", line: l, value: 50 },
    { type: "horizontal_distance", a, b, value: 30 },
    { type: "radius", entity: c, value: 7 },
    { type: "diameter", entity: c, value: 14 },
    { type: "angle", a: h, b: l, degrees: 53 },
    { type: "coincident", a, b },
  );
  const dims = sheetSketchDims(s, H);
  assert.equal(dims.length, 5);
  near(dims[0].value, 50);
  assert.deepEqual(dims[0].a, [10, H - 20]);
  near(dims[1].value, 30);
  assert.equal(dims[2].kind, "radius");
  near(dims[2].value, 7);
  assert.equal(dims[3].prefix, "Ø");
  near(dims[3].value, 14);
  assert.equal(dims[4].kind, "angle");
  near(dims[4].value, (Math.atan2(40, 30) * 180) / Math.PI, 1e-9);
  assert.deepEqual(sheetSketchDims(null, H), []);
});
