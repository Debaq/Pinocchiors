// «¿Cuál?» entre lo encimado: qué distingue a cada candidata, la vista
// separada y juntar dos iguales, sin navegador:
//   node --test apps/web/e2e/sketchClarify.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { addEntity, addPoint } from "../src/lib/cad.ts";
import { describeEntity, spreadCandidates } from "../src/lib/sketchClarify.ts";
import { mergeDuplicate, sameEntities } from "../src/lib/sketchEdit.ts";

const near = (a, b, msg, eps = 1e-9) => assert.ok(Math.abs(a - b) < eps, `${msg}: ${a} ≠ ${b}`);
const blank = () => ({ points: [], entities: [], constraints: [] });

/** Dos líneas iguales de (0,0) a (10,0), con puntos propios; la primera con su largo */
function twins() {
  const s = blank();
  const [a0, a1, b0, b1] = [addPoint(s, [0, 0]), addPoint(s, [10, 0]), addPoint(s, [0, 0]), addPoint(s, [10, 0])];
  const a = addEntity(s, { type: "line", start: a0, end: a1 });
  const b = addEntity(s, { type: "line", start: b1, end: b0 });
  s.constraints.push({ type: "length", line: a, value: 10 }, { type: "horizontal", line: b });
  // Otra línea que usa un extremo de la segunda
  const c = addEntity(s, { type: "line", start: b1, end: addPoint(s, [10, 5]) });
  return { s, a, b, c, a1, b1 };
}

test("cada candidata dice lo que la distingue", () => {
  const { s, a, b } = twins();
  const da = describeEntity(s, a);
  assert.equal(da.name, `Línea ${a}`);
  assert.match(da.detail, /^10 mm · P\d+ → P\d+$/);
  assert.deepEqual(da.constraints, ["Largo 10"]);
  assert.deepEqual(describeEntity(s, b).constraints, ["Horizontal"]);
});

test("vista separada: de costado a la línea, centrada, una por color", () => {
  const { s, a, b } = twins();
  const c = spreadCandidates(s, [a, b], [5, 0], 2);
  assert.equal(c.length, 2);
  // Horizontal: se corren en Y, una a cada lado
  near(c[0].offset[0], 0, "sin corrimiento en X");
  near(c[0].offset[1], -c[1].offset[1], "simétricas");
  near(Math.abs(c[0].offset[1] - c[1].offset[1]), 2, "separadas el paso");
  assert.notEqual(c[0].color, c[1].color);
  // La etiqueta a mitad de la línea, corrida con ella
  near(c[0].label[0], 5, "etiqueta en el medio");
  near(c[0].label[1], c[0].offset[1], "etiqueta corrida");
});

test("juntar dos iguales: queda una con las restricciones de las dos y lo que usaba sus puntos", () => {
  const { s, a, b, c, a1, b1 } = twins();
  assert.ok(sameEntities(s, a, b));
  assert.ok(mergeDuplicate(s, a, b));
  assert.equal(s.entities.some((e) => e.id === b), false);
  assert.ok(s.constraints.some((k) => k.type === "horizontal" && k.line === a), "la horizontal pasó a la que queda");
  assert.ok(s.constraints.some((k) => k.type === "length" && k.line === a));
  // La tercera línea ahora sale del punto de la que quedó
  assert.equal(s.entities.find((e) => e.id === c).geometry.start, a1);
  assert.equal(s.points.some((p) => p.id === b1), false);
  // Distintas no se juntan
  const other = addEntity(s, { type: "line", start: addPoint(s, [0, 1]), end: addPoint(s, [10, 1]) });
  assert.equal(sameEntities(s, a, other), false);
  assert.equal(mergeDuplicate(s, a, other), false);
});

// ─── Dibujar encima de una línea no la parte ──────────────────────────────

import { placeSnap } from "../src/lib/cad.ts";
import { rejoinSplit } from "../src/lib/sketchEdit.ts";

/** Una horizontal de 40 de (0,0) a (40,0), con su largo */
function forty() {
  const s = blank();
  const [p0, p1] = [addPoint(s, [0, 0]), addPoint(s, [40, 0])];
  const l = addEntity(s, { type: "line", start: p0, end: p1 });
  s.constraints.push({ type: "horizontal", line: l }, { type: "length", line: l, value: 40 });
  return { s, l, p0, p1 };
}
const lines = (s) => s.entities.filter((e) => e.geometry.type === "line");

test("de una punta al medio, encima: la de 40 sigue entera y el punto queda en su medio", () => {
  const { s, l, p0, p1 } = forty();
  const id = placeSnap(s, { p: [20, 0], kind: "midpoint", entity: l }, [0, 0]);
  addEntity(s, { type: "line", start: p0, end: id });
  assert.equal(lines(s).length, 2, "la de 40 y la nueva");
  const g = s.entities.find((e) => e.id === l).geometry;
  assert.equal(g.end, p1, "la de 40 no se acortó");
  assert.ok(s.constraints.some((k) => k.type === "midpoint" && k.point === id && k.line === l));
});

test("de una punta a un punto cualquiera de la línea, encima: tampoco la parte", () => {
  const { s, l, p0 } = forty();
  const id = placeSnap(s, { p: [12, 0], kind: "on_line", entity: l }, [0, 0]);
  addEntity(s, { type: "line", start: p0, end: id });
  assert.equal(lines(s).length, 2);
  assert.ok(s.constraints.some((k) => k.type === "point_on_line" && k.point === id && k.line === l));
});

test("llegar al medio de costado (en T) sí la parte, como antes", () => {
  const { s, l } = forty();
  const id = placeSnap(s, { p: [20, 0], kind: "midpoint", entity: l }, [20, 10]);
  assert.equal(lines(s).length, 2, "la de 40 en dos mitades");
  assert.ok(s.constraints.some((k) => k.type === "equal"));
  assert.notEqual(id, undefined);
});

test("salir del medio (el clic la partió) y seguir encima: se vuelve a unir", () => {
  const { s, l, p0, p1 } = forty();
  // Primer clic en el medio, sin saber a dónde va: la parte
  const known = new Set(s.entities.map((e) => e.id));
  const mid = placeSnap(s, { p: [20, 0], kind: "midpoint", entity: l });
  const rest = s.entities.find((e) => !known.has(e.id)).id;
  // Segundo clic en la punta: el tramo va encima
  const drawn = addEntity(s, { type: "line", start: mid, end: p1 });
  assert.ok(rejoinSplit(s, [l, rest], mid, drawn));
  assert.equal(lines(s).length, 2, "la de 40 entera y la nueva");
  const g = s.entities.find((e) => e.id === l).geometry;
  assert.deepEqual([g.start, g.end].sort(), [p0, p1].sort(), "la de 40 de punta a punta");
  assert.ok(s.constraints.some((k) => k.type === "midpoint" && k.point === mid && k.line === l), "el punto en su medio");
  assert.equal(s.constraints.filter((k) => k.type === "horizontal" && k.line === l).length, 1, "sin horizontales repetidas");
  // Saliendo de costado no se une
  const s2 = forty();
  const k2 = new Set(s2.s.entities.map((e) => e.id));
  const m2 = placeSnap(s2.s, { p: [20, 0], kind: "midpoint", entity: s2.l });
  const r2 = s2.s.entities.find((e) => !k2.has(e.id)).id;
  const up = addEntity(s2.s, { type: "line", start: m2, end: addPoint(s2.s, [20, 10]) });
  assert.equal(rejoinSplit(s2.s, [s2.l, r2], m2, up), false);
  assert.equal(lines(s2.s).length, 3);
});

// ─── Sobre una arista del sólido: el punto no queda libre (issue #17) ─────

import { infer, straightEdge } from "../src/lib/sketchSnap.ts";

test("sobre una arista recta del sólido: el punto queda sobre ella y comparte la guía", () => {
  const s = blank();
  const edge = [[0, 0], [30, 30]];
  const refs = { vertices: [], centers: [], midpoints: [], edges: [edge] };
  const snap = infer(s, [12, 11], 2, { solid: refs });
  assert.equal(snap.kind, "on_edge");
  assert.deepEqual(snap.edgeLine, [[0, 0], [30, 30]]);
  const a = placeSnap(s, snap);
  const guide = s.entities.find((e) => e.construction && e.geometry.type === "line");
  assert.ok(guide, "línea de construcción sobre la arista");
  assert.ok(s.constraints.some((k) => k.type === "point_on_line" && k.point === a && k.line === guide.id));
  const fixed = s.constraints.filter((k) => k.type === "fixed").map((k) => [k.x, k.y]);
  assert.deepEqual(fixed, [[0, 0], [30, 30]], "la guía tiene los extremos fijos");
  // Otro punto sobre la misma arista: la misma guía
  const b = placeSnap(s, { ...snap, p: [20, 20] });
  assert.equal(s.entities.filter((e) => e.construction).length, 1);
  assert.ok(s.constraints.some((k) => k.type === "point_on_line" && k.point === b && k.line === guide.id));
});

test("sobre una arista curva del sólido: el punto queda fijo ahí", () => {
  const s = blank();
  const arc = Array.from({ length: 9 }, (_, i) => [10 * Math.cos((i * Math.PI) / 16), 10 * Math.sin((i * Math.PI) / 16)]);
  assert.equal(straightEdge(arc), undefined);
  const id = placeSnap(s, { p: arc[3], kind: "on_edge" });
  assert.ok(s.constraints.some((k) => k.type === "fixed" && k.point === id));
});

// ─── Splines y elipses: sobre la curva y cruces ───────────────────────────

test("sobre una spline: el punto queda sobre ella", () => {
  const s = blank();
  const ids = [[0, 0], [10, 5], [20, 0]].map((p) => addPoint(s, p));
  const sp = addEntity(s, { type: "spline", points: ids, closed: false });
  const poly = Array.from({ length: 21 }, (_, i) => [i, 5 * Math.sin((i / 20) * Math.PI)]);
  const snap = infer(s, [5, 3.8], 1, { curves: [{ id: sp, poly }] });
  assert.equal(snap.kind, "on_curve");
  const id = placeSnap(s, snap);
  assert.ok(s.constraints.some((k) => k.type === "point_on_curve" && k.point === id && k.curve === sp));
});

test("cruce de una línea con una spline: sobre las dos", () => {
  const s = blank();
  const ids = [[0, 0], [10, 5], [20, 0]].map((p) => addPoint(s, p));
  const sp = addEntity(s, { type: "spline", points: ids, closed: false });
  const l = addEntity(s, { type: "line", start: addPoint(s, [5, -5]), end: addPoint(s, [5, 10]) });
  const poly = Array.from({ length: 21 }, (_, i) => [i, 5 * Math.sin((i / 20) * Math.PI)]);
  const snap = infer(s, [5.1, 3.5], 1, { curves: [{ id: sp, poly }] });
  assert.equal(snap.kind, "intersection");
  const id = placeSnap(s, snap);
  assert.ok(s.constraints.some((k) => k.type === "point_on_curve" && k.point === id && k.curve === sp));
  // La línea se parte en el cruce (cierra regiones): el punto es punta de un tramo
  assert.ok(lines(s).some((e) => e.geometry.start === id || e.geometry.end === id), "la línea se parte ahí");
});

// ─── Soltar un punto arrastrado sobre algo: queda pegado ──────────────────

import { attachDropped, linkAnchor } from "../src/lib/cad.ts";

test("soltar sobre una línea, un punto o su propia línea", () => {
  const { s, l } = forty();
  const free = addPoint(s, [10, 3]);
  const other = addEntity(s, { type: "line", start: free, end: addPoint(s, [10, 20]) });
  assert.equal(attachDropped(s, free, { p: [12, 0], kind: "on_line", entity: l }), 1);
  assert.deepEqual([s.points.find((p) => p.id === free).x, s.points.find((p) => p.id === free).y], [12, 0]);
  assert.ok(s.constraints.some((k) => k.type === "point_on_line" && k.point === free && k.line === l));
  // Sobre su propia línea: nada
  assert.equal(attachDropped(s, free, { p: [10, 8], kind: "on_line", entity: other }), 0);
  // Sobre la otra punta de su propia línea: nada (la aplastaría)
  const tip = s.entities.find((e) => e.id === other).geometry.end;
  assert.equal(attachDropped(s, free, { p: [10, 20], kind: "point", id: tip }), 0);
  // Sobre un punto de otra: coincidente
  const lone = addPoint(s, [30, 30]);
  assert.equal(attachDropped(s, lone, { p: [0, 0], kind: "point", id: s.entities.find((e) => e.id === l).geometry.start }), 1);
  // Alineado no pega
  assert.equal(attachDropped(s, lone, { p: [0, 0], kind: "aligned", align: { h: free } }), 0);
});

// ─── Anclas al sólido ligadas a la arista de verdad ───────────────────────

test("el ancla sobre la arista pasa a la arista ligada y la guía provisoria se va", () => {
  const s = blank();
  const snap = { p: [5, 5], kind: "on_edge", edgeLine: [[0, 0], [10, 10]], src: { edge: 3 } };
  const id = placeSnap(s, snap);
  const ref = { point: [0, 0, 0], direction: [1, 1, 0] };
  assert.equal(linkAnchor(s, id, snap, { kind: "line", start: [0, 0], end: [10, 10] }, { edge: ref }), true);
  assert.equal(s.uses.length, 1);
  const linked = s.uses[0].entity;
  assert.ok(s.entities.find((e) => e.id === linked).construction);
  assert.ok(s.constraints.some((k) => k.type === "point_on_line" && k.point === id && k.line === linked));
  assert.ok(!s.constraints.some((k) => k.type === "fixed"), "sin la guía fija");
  assert.equal(s.entities.length, 1, "solo la ligada");
  // Otro punto sobre la misma arista: la misma ligada
  const id2 = placeSnap(s, { ...snap, p: [7, 7] });
  assert.equal(linkAnchor(s, id2, { ...snap, p: [7, 7] }, { kind: "line", start: [0, 0], end: [10, 10] }, { edge: ref }), true);
  assert.equal(s.uses.length, 1);
});

test("vértice y centro del sólido: coincidentes con la punta y el centro de lo ligado", () => {
  const s = blank();
  const v = { p: [10, 0], kind: "solid_vertex", src: { edge: 1, end: 1 } };
  const a = placeSnap(s, v);
  assert.ok(linkAnchor(s, a, v, { kind: "line", start: [0, 0], end: [10, 0] }, { edge: { point: [5, 0, 0], direction: [1, 0, 0] } }));
  const line = s.entities.find((e) => e.geometry.type === "line").geometry;
  assert.ok(s.constraints.some((k) => k.type === "coincident" && k.a === a && k.b === line.end));
  const c = { p: [30, 0], kind: "solid_center", src: { edge: 2 } };
  const b = placeSnap(s, c);
  assert.ok(linkAnchor(s, b, c, { kind: "circle", center: [30, 0], radius: 4 }, { edge: { point: [34, 0, 0], direction: [0, 1, 0] } }));
  const circle = s.entities.find((e) => e.geometry.type === "circle").geometry;
  assert.ok(s.constraints.some((k) => k.type === "coincident" && k.a === b && k.b === circle.center));
  assert.ok(!s.constraints.some((k) => k.type === "fixed"));
  // Si el ancla provisoria ya no está (se deshizo), no hace nada
  assert.equal(linkAnchor(s, a, v, { kind: "line", start: [0, 0], end: [10, 0] }, { edge: { point: [5, 0, 0], direction: [1, 0, 0] } }), false);
});

/** Línea (20,0)-(30,0), círculo de centro (50,50) r = 5 y un punto suelto en (52,-10) */
function guided() {
  const s = blank();
  const [a, b] = [addPoint(s, [20, 0]), addPoint(s, [30, 0])];
  const line = addEntity(s, { type: "line", start: a, end: b });
  const c = addPoint(s, [50, 50]);
  const circle = addEntity(s, { type: "circle", center: c, radius: 5 });
  const loose = addPoint(s, [52, -10]);
  return { s, a, b, line, circle, loose };
}

test("colocar sobre una prolongación: punto en la recta, sin partir la línea", () => {
  const { s, line } = guided();
  const n = s.entities.length;
  const id = placeSnap(s, { p: [40, 0], kind: "extension", entity: line });
  assert.deepEqual(s.constraints, [{ type: "point_on_line", point: id, line }]);
  assert.equal(s.entities.length, n);
});

test("colocar en el cruce de una guía vertical con un círculo", () => {
  const { s, circle, loose } = guided();
  const id = placeSnap(s, { p: [52, 50 + Math.sqrt(21)], kind: "guide_cross", entity: circle, align: { v: loose } });
  assert.deepEqual(s.constraints, [
    { type: "vertical_points", a: loose, b: id },
    { type: "point_on_circle", point: id, circle },
  ]);
});

test("colocar en el cruce de una prolongación con otra y con una guía", () => {
  const { s, line, loose } = guided();
  const b = addPoint(s, [40, 10]);
  const v = addEntity(s, { type: "line", start: b, end: addPoint(s, [40, 20]) });
  const id = placeSnap(s, { p: [40, 0], kind: "guide_cross", ext: [line, v] });
  assert.deepEqual(s.constraints, [
    { type: "point_on_line", point: id, line },
    { type: "point_on_line", point: id, line: v },
  ]);
  const id2 = placeSnap(s, { p: [52, 0], kind: "guide_cross", ext: [line], align: { v: loose } });
  assert.deepEqual(s.constraints.slice(2), [
    { type: "vertical_points", a: loose, b: id2 },
    { type: "point_on_line", point: id2, line },
  ]);
});

test("colocar sobre una guía del arco: punto libre, sin restricciones", () => {
  const { s, circle } = guided();
  for (const kind of ["guide_tangent", "guide_radial"]) {
    const id = placeSnap(s, { p: [6, 8], kind, entity: circle });
    assert.ok(s.points.some((q) => q.id === id));
  }
  assert.deepEqual(s.constraints, []);
});

test("soltar un punto sobre una prolongación o un cruce de guías", () => {
  const { s, a, line, circle, loose } = guided();
  // Prolongación: sobre la recta de la línea y queda ahí
  const p = addPoint(s, [41, 3]);
  assert.equal(attachDropped(s, p, { p: [40, 0], kind: "extension", entity: line }), 1);
  assert.deepEqual(s.constraints, [{ type: "point_on_line", point: p, line }]);
  assert.deepEqual([s.points.find((q) => q.id === p).x, s.points.find((q) => q.id === p).y], [40, 0]);
  // Pegarse a su propia línea: nada
  assert.equal(attachDropped(s, a, { p: [40, 0], kind: "extension", entity: line }), 0);
  // Cruce guía vertical × círculo: las dos restricciones
  const q = addPoint(s, [53, 55]);
  const n = s.constraints.length;
  assert.equal(attachDropped(s, q, { p: [52, 50 + Math.sqrt(21)], kind: "guide_cross", entity: circle, align: { v: loose } }), 2);
  assert.deepEqual(s.constraints.slice(n), [
    { type: "vertical_points", a: loose, b: q },
    { type: "point_on_circle", point: q, circle },
  ]);
  // Todo o nada: si parte de lo suyo, no se pega a medias
  const m = s.constraints.length;
  assert.equal(attachDropped(s, a, { p: [40, 0], kind: "guide_cross", ext: [line], align: { v: loose } }), 0);
  assert.equal(s.constraints.length, m);
  // Las guías del arco no pegan al soltar
  assert.equal(attachDropped(s, q, { p: [6, 8], kind: "guide_radial", entity: circle }), 0);
});
