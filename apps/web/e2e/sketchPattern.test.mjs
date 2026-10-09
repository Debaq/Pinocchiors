// Pruebas de los patrones del sketch (lib/cad.ts), sin navegador:
//   node apps/web/e2e/sketchPattern.test.mjs
// Con SKETCH_OUT=<carpeta> además guarda cada sketch en JSON (para pasarlo por el solver)
import assert from "node:assert/strict";
import { writeFileSync } from "node:fs";
import { test } from "node:test";
import { addEntity, addPoint, addRectangle, circularPattern, curvePattern, fillPattern, linearPattern, tablePattern } from "../src/lib/cad.ts";

function sketch() {
  return { origin: 0, points: [{ id: 0, x: 0, y: 0 }], entities: [], constraints: [] };
}

function circle(s, x, y, r) {
  return addEntity(s, { type: "circle", center: addPoint(s, [x, y]), radius: r });
}

/** Centros de los círculos, ordenados */
function centers(s) {
  return s.entities
    .filter((e) => e.geometry.type === "circle")
    .map((e) => s.points.find((p) => p.id === e.geometry.center))
    .map((p) => [+p.x.toFixed(6), +p.y.toFixed(6)])
    .sort((a, b) => a[0] - b[0] || a[1] - b[1]);
}

function has(list, x, y) {
  assert.ok(
    list.some(([a, b]) => Math.abs(a - x) < 1e-6 && Math.abs(b - y) < 1e-6),
    `falta (${x}, ${y}) en ${JSON.stringify(list)}`,
  );
}

function save(name, s) {
  if (process.env.SKETCH_OUT) writeFileSync(`${process.env.SKETCH_OUT}/${name}.json`, JSON.stringify(s));
}

test("lineal en dos direcciones: grilla con cotas en el primer par de cada una", () => {
  const s = sketch();
  const c = circle(s, 5, 5, 1);
  assert.equal(linearPattern(s, [c], 3, [10, 0], 2, [0, 8]), undefined);
  const list = centers(s);
  assert.equal(list.length, 6);
  for (const x of [5, 15, 25]) for (const y of [5, 13]) has(list, x, y);
  assert.equal(s.constraints.filter((k) => k.type === "horizontal_distance").length, 2);
  assert.equal(s.constraints.filter((k) => k.type === "equal_offset").length, 3);
  save("grilla", s);
});

test("circular en un arco de 90°: de punta a punta", () => {
  const s = sketch();
  const c = circle(s, 10, 0, 1);
  assert.equal(circularPattern(s, [c], 4, 0, 90), undefined);
  const list = centers(s);
  assert.equal(list.length, 4);
  for (const a of [0, 30, 60, 90]) has(list, 10 * Math.cos((a * Math.PI) / 180), 10 * Math.sin((a * Math.PI) / 180));
  assert.equal(s.constraints.find((k) => k.type === "angle").degrees, 30);
  save("circular-90", s);
});

test("circular sin girar: las copias se trasladan", () => {
  const s = sketch();
  const [a, b] = [addPoint(s, [10, 0]), addPoint(s, [12, 0])];
  const l = addEntity(s, { type: "line", start: a, end: b });
  assert.equal(circularPattern(s, [l], 4, 0, 360, false), undefined);
  const lines = s.entities.filter((e) => e.geometry.type === "line" && !e.construction);
  assert.equal(lines.length, 4);
  for (const e of lines) {
    const [p, q] = [e.geometry.start, e.geometry.end].map((id) => s.points.find((x) => x.id === id));
    assert.ok(Math.abs(q.x - p.x - 2) < 1e-9 && Math.abs(q.y - p.y) < 1e-9, "la copia giró");
  }
  save("circular-sin-girar", s);
});

test("circular: más de 180° entre copias no se puede", () => {
  const s = sketch();
  const c = circle(s, 10, 0, 1);
  assert.match(circularPattern(s, [c], 2, 0, 270), /180/);
});

test("en curva: línea, de punta a punta", () => {
  const s = sketch();
  const path = addEntity(s, { type: "line", start: addPoint(s, [0, 0]), end: addPoint(s, [30, 30]) });
  const c = circle(s, 0, 5, 1);
  assert.equal(curvePattern(s, [path, c], 4, path), undefined);
  const list = centers(s);
  for (const k of [0, 1, 2, 3]) has(list, 10 * k, 5 + 10 * k);
  save("curva-linea", s);
});

test("en curva: arco, trasladando o girando", () => {
  for (const rotate of [false, true]) {
    const s = sketch();
    const [c0, a, b] = [addPoint(s, [0, 0]), addPoint(s, [10, 0]), addPoint(s, [-10, 0])];
    const arc = addEntity(s, { type: "arc", center: c0, start: a, end: b });
    const c = circle(s, 12, 0, 1);
    assert.equal(curvePattern(s, [arc, c], 3, arc, rotate), undefined);
    const list = centers(s);
    has(list, 12, 0);
    // A media vuelta: girando queda a 12 del centro; trasladando, corrido lo mismo que el arco
    has(list, rotate ? 0 : 2, rotate ? 12 : 10);
    has(list, rotate ? -12 : -8, 0);
    save(`curva-arco-${rotate ? "gira" : "traslada"}`, s);
  }
});

test("en curva: círculo cerrado sin repetir la punta", () => {
  const s = sketch();
  const path = circle(s, 0, 0, 10);
  const c = circle(s, 10, 0, 1);
  assert.equal(curvePattern(s, [path, c], 4, path), undefined);
  const list = centers(s).filter(([x, y]) => x || y);
  assert.equal(list.length, 4);
  for (const [x, y] of [[10, 0], [0, 10], [-10, 0], [0, -10]]) has(list, x, y);
  save("curva-circulo", s);
});

test("en curva: spline, a distancias iguales por largo", () => {
  const s = sketch();
  const pts = [addPoint(s, [0, 0]), addPoint(s, [20, 0]), addPoint(s, [40, 0])];
  const sp = addEntity(s, { type: "spline", points: pts, closed: false });
  const c = circle(s, 0, 3, 1);
  assert.equal(curvePattern(s, [sp, c], 5, sp), undefined);
  const list = centers(s);
  for (const k of [0, 1, 2, 3, 4]) has(list, 10 * k, 3);
});

test("por tabla: una copia por fila con sus cotas", () => {
  const s = sketch();
  const c = circle(s, 1, 1, 0.5);
  assert.equal(tablePattern(s, [c], [[5, 0], [0, 7], [-3, -4]]), undefined);
  const list = centers(s);
  for (const [x, y] of [[1, 1], [6, 1], [1, 8], [-2, -3]]) has(list, x, y);
  assert.equal(s.constraints.filter((k) => k.type === "horizontal_distance").length, 3);
  save("tabla", s);
});

/** Región cuadrada 0..size con un agujero opcional */
function square(size, hole) {
  const loop = (poly) => ({ pieces: [], polygon: poly, area: 0 });
  return { outer: loop([[0, 0], [size, 0], [size, size], [0, size]]), holes: hole ? [loop(hole)] : [], depth: 0, sample: [1, 1] };
}

test("de relleno: grilla cuadrada dentro de la región, con margen", () => {
  const s = sketch();
  const r = addRectangle(s, [0, 0], [50, 50]);
  const c = circle(s, 5, 5, 2);
  assert.equal(fillPattern(s, [c], square(50), 10, false, 1), undefined);
  // Centros de 5 a 45: 5 × 5 (el radio más el margen deja 3 al borde)
  const list = centers(s);
  assert.equal(list.length, 25);
  has(list, 45, 45);
  // El paso es una cota sola sobre una línea de construcción
  assert.equal(s.constraints.filter((k) => k.type === "length" && k.value === 10).length, 1);
  void r;
  save("relleno", s);
});

test("de relleno: hexagonal y con agujero", () => {
  const s = sketch();
  const c = circle(s, 5, 5, 1);
  const hole = [[20, 20], [30, 20], [30, 30], [20, 30]];
  assert.equal(fillPattern(s, [c], square(50, hole), 10, true, 0), undefined);
  const list = centers(s);
  for (const [x, y] of list) {
    assert.ok(x >= 1 - 1e-9 && x <= 49 + 1e-9 && y >= 1 - 1e-9 && y <= 49 + 1e-9, `fuera: ${x}, ${y}`);
    assert.ok(!(x > 19 && x < 31 && y > 19 && y < 31), `en el agujero: ${x}, ${y}`);
  }
  // Fila de arriba corrida medio paso
  has(list, 10, 5 + 5 * Math.sqrt(3));
  save("relleno-hex", s);
});

test("de relleno: demasiadas copias", () => {
  const s = sketch();
  const c = circle(s, 1, 1, 0.1);
  assert.match(fillPattern(s, [c], square(1000), 1, false, 0), /separación/);
});
