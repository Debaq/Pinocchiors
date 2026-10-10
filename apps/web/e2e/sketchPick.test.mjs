// Pruebas del filtro de selección del sketch (lib/sketchPick.ts):
//   node --test apps/web/e2e/sketchPick.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { addEntity, addPoint } from "../src/lib/cad.ts";
import { passesFilter } from "../src/lib/sketchPick.ts";

test("cada filtro deja pasar lo suyo", () => {
  const s = { origin: 0, points: [{ id: 0, x: 0, y: 0 }], entities: [], constraints: [] };
  const a = addPoint(s, [0, 0]);
  const l = addEntity(s, { type: "line", start: a, end: addPoint(s, [5, 0]) });
  const c = addEntity(s, { type: "circle", center: addPoint(s, [9, 0]), radius: 1 });
  const sp = addEntity(s, { type: "spline", points: [addPoint(s, [0, 5]), addPoint(s, [3, 7]), addPoint(s, [6, 5])], closed: false });
  const hits = { point: { point: a }, line: { entity: l }, circle: { entity: c }, spline: { entity: sp } };
  const ok = (f) => Object.entries(hits).filter(([, h]) => passesFilter(s, h, f)).map(([k]) => k);
  assert.deepEqual(ok("all"), ["point", "line", "circle", "spline"]);
  assert.deepEqual(ok("points"), ["point"]);
  assert.deepEqual(ok("lines"), ["line"]);
  assert.deepEqual(ok("round"), ["circle"]);
  assert.deepEqual(ok("curves"), ["spline"]);
});
