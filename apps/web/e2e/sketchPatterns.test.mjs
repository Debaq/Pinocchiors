// Pruebas de los patrones que se vuelven a editar (lib/sketchPatterns.ts):
//   node --test apps/web/e2e/sketchPatterns.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { addEntity, addPoint } from "../src/lib/cad.ts";
import { dropPattern, editPattern, makePattern, patternAt, toggleSkip } from "../src/lib/sketchPatterns.ts";

function sketch() {
  return { origin: 0, points: [{ id: 0, x: 0, y: 0 }], entities: [], constraints: [] };
}
const at = (s, id) => {
  const p = s.points.find((q) => q.id === id);
  return [p.x, p.y];
};
const circles = (s) => s.entities.filter((e) => e.geometry.type === "circle");
const centers = (s) => circles(s).map((e) => at(s, e.geometry.center).map((v) => +v.toFixed(6)));
const LINEAR = { kind: "linear", count: 4, offset: [10, 0], rows: 1, offset2: [0, 0] };

test("patrón lineal anotado: copias por separado y se encuentra desde cualquier copia", () => {
  const s = sketch();
  const c = addEntity(s, { type: "circle", center: addPoint(s, [0, 0]), radius: 2 });
  assert.equal(makePattern(s, [c], LINEAR), undefined);
  assert.equal(circles(s).length, 4);
  const p = s.patterns[0];
  assert.equal(p.copies.length, 3);
  assert.ok(p.copies.every((k) => k.length === 1));
  assert.deepEqual(patternAt(s, p.copies[1][0]), { pattern: p, copy: 1 });
  assert.equal(patternAt(s, c).copy, -1);
});

test("rehacer con otra cantidad y otro paso: borra las copias viejas y arma las nuevas", () => {
  const s = sketch();
  const c = addEntity(s, { type: "circle", center: addPoint(s, [0, 0]), radius: 2 });
  makePattern(s, [c], LINEAR);
  const id = s.patterns[0].id;
  assert.equal(editPattern(s, id, { ...LINEAR, count: 6, offset: [0, 5] }), undefined);
  assert.equal(circles(s).length, 6);
  assert.deepEqual(centers(s).sort((a, b) => a[1] - b[1]), [0, 5, 10, 15, 20, 25].map((y) => [0, y]));
  // Sin restricciones colgadas de lo que se borró
  const ids = new Set([...s.points.map((q) => q.id), ...s.entities.map((e) => e.id)]);
  for (const k of s.constraints) for (const [f, v] of Object.entries(k)) if (["a", "b", "a1", "a2", "b1", "b2", "point", "entity"].includes(f)) assert.ok(ids.has(v), `${k.type}.${f} → ${v}`);
});

test("omitir una copia la deja de construcción y sigue omitida al rehacer", () => {
  const s = sketch();
  const c = addEntity(s, { type: "circle", center: addPoint(s, [0, 0]), radius: 2 });
  makePattern(s, [c], LINEAR);
  const p = s.patterns[0];
  assert.equal(toggleSkip(s, p.id, 1), undefined);
  assert.ok(s.entities.find((e) => e.id === p.copies[1][0]).construction);
  editPattern(s, p.id, { ...LINEAR, offset: [20, 0] });
  assert.deepEqual(s.patterns[0].skip, [1]);
  const skipped = s.entities.find((e) => e.id === s.patterns[0].copies[1][0]);
  assert.ok(skipped.construction);
  assert.deepEqual(at(s, skipped.geometry.center).map(Math.round), [40, 0]);
  // Volver a ponerla
  toggleSkip(s, p.id, 1);
  assert.ok(!s.entities.find((e) => e.id === s.patterns[0].copies[1][0]).construction);
  // Con menos copias, la omisión que ya no existe se olvida
  toggleSkip(s, p.id, 2);
  editPattern(s, p.id, { ...LINEAR, count: 2 });
  assert.deepEqual(s.patterns[0].skip, []);
});

test("rehacer toma el original como está ahora (una entidad más no, pero su forma sí)", () => {
  const s = sketch();
  const a = addPoint(s, [0, 0]);
  const l = addEntity(s, { type: "line", start: a, end: addPoint(s, [4, 0]) });
  makePattern(s, [l], { ...LINEAR, count: 3 });
  // El original cambia de largo: al rehacer, las copias también
  s.points.find((q) => q.id === s.entities.find((e) => e.id === l).geometry.end).x = 8;
  editPattern(s, s.patterns[0].id);
  const lens = s.entities.filter((e) => e.geometry.type === "line").map((e) => at(s, e.geometry.end)[0] - at(s, e.geometry.start)[0]);
  assert.deepEqual(lens.map(Math.round), [8, 8, 8]);
});

test("circular: rehacer con otra cantidad y otro ángulo (con sus guías)", () => {
  const s = sketch();
  const c = addEntity(s, { type: "circle", center: addPoint(s, [10, 0]), radius: 1 });
  assert.equal(makePattern(s, [c], { kind: "circular", count: 4, center: 0, span: 360, rotate: true }), undefined);
  assert.equal(circles(s).length, 4);
  const guides = s.entities.filter((e) => e.geometry.type === "line").length;
  assert.equal(editPattern(s, s.patterns[0].id, { kind: "circular", count: 3, center: 0, span: 360, rotate: true }), undefined);
  assert.equal(circles(s).length, 3);
  // Las guías viejas se fueron con el patrón (quedan solo las del nuevo)
  assert.equal(s.entities.filter((e) => e.geometry.type === "line").length, guides);
  for (const [x, y] of centers(s)) assert.ok(Math.abs(Math.hypot(x, y) - 10) < 1e-6);
});

test("si no se puede rehacer, no se toca nada; soltar el patrón deja las copias", () => {
  const s = sketch();
  const c = addEntity(s, { type: "circle", center: addPoint(s, [0, 0]), radius: 2 });
  makePattern(s, [c], LINEAR);
  const before = structuredClone(s);
  assert.match(editPattern(s, s.patterns[0].id, { ...LINEAR, count: 1 }), /al menos 2/);
  assert.deepEqual(s, before);
  dropPattern(s, s.patterns[0].id);
  assert.equal(s.patterns, undefined);
  assert.equal(circles(s).length, 4);
});
