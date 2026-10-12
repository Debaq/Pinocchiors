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
