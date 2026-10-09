// Pruebas de los íconos de restricciones (lib/sketchGlyphs.ts), sin navegador:
//   node --test apps/web/e2e/sketchGlyphs.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { addEntity, addPoint, addRectangle } from "../src/lib/cad.ts";
import { constraintGlyphs, isDimension } from "../src/lib/sketchGlyphs.ts";

function sketch() {
  return { origin: 0, points: [{ id: 0, x: 0, y: 0 }], entities: [], constraints: [] };
}

test("rectángulo: H y V en el medio de cada lado", () => {
  const s = sketch();
  const l = addRectangle(s, [0, 0], [10, 4]);
  const g = constraintGlyphs(s);
  assert.equal(g.length, 4);
  const bottom = g.find((x) => x.host === `e${l[0]}`);
  assert.equal(bottom.glyph, "H");
  assert.deepEqual(bottom.at, [5, 0]);
  assert.equal(g.find((x) => x.host === `e${l[1]}`).glyph, "V");
});

test("las de dos entidades van en las dos; la tangencia con extremo común, en el punto", () => {
  const s = sketch();
  const a = addEntity(s, { type: "line", start: addPoint(s, [0, 0]), end: addPoint(s, [10, 0]) });
  const b = addEntity(s, { type: "line", start: addPoint(s, [0, 5]), end: addPoint(s, [10, 5]) });
  s.constraints.push({ type: "parallel", a, b });
  const shared = s.entities.find((e) => e.id === a).geometry.end;
  const arc = addEntity(s, { type: "arc", center: addPoint(s, [10, 3]), start: shared, end: addPoint(s, [13, 3]) });
  s.constraints.push({ type: "tangent", a, b: arc });
  const g = constraintGlyphs(s);
  assert.equal(g.filter((x) => x.index === 0).length, 2);
  const t = g.filter((x) => x.index === 1);
  assert.equal(t.length, 1);
  assert.deepEqual(t[0].at, [10, 0]);
  assert.equal(t[0].host, `p${shared}`);
});

test("cotas y patrones sin ícono", () => {
  const s = sketch();
  const l = addEntity(s, { type: "line", start: addPoint(s, [0, 0]), end: addPoint(s, [10, 0]) });
  s.constraints.push({ type: "length", line: l, value: 10 }, { type: "equal_offset", a1: 1, a2: 2, b1: 1, b2: 2 });
  assert.equal(constraintGlyphs(s).length, 0);
  assert.ok(isDimension(s.constraints[0]));
  assert.ok(!isDimension(s.constraints[1]));
});
