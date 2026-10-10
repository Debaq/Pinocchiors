// Pruebas de bloques y capas del sketch (lib/sketchBlocks.ts), sin navegador:
//   node --test apps/web/e2e/sketchBlocks.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { addEntity, addPoint, addRectangle } from "../src/lib/cad.ts";
import { addLayer, blockSketch, createBlock, explodeBlock, insertBlock, layerState, refreshBlock, removeLayer, saveBlock, setLayer } from "../src/lib/sketchBlocks.ts";
import { rotateSketch } from "../src/lib/sketchTransform.ts";

function sketch() {
  return { origin: 0, points: [{ id: 0, x: 0, y: 0 }], entities: [], constraints: [] };
}
const at = (s, id) => {
  const p = s.points.find((q) => q.id === id);
  return [+p.x.toFixed(9), +p.y.toFixed(9)];
};
const near = (a, b, what = "") => assert.ok(Math.abs(a - b) < 1e-9, `${what} ${a} ≠ ${b}`);

/** Rectángulo 4 × 2 desde (10, 10) con un largo, hecho bloque con origen en su esquina */
function withBlock() {
  const s = sketch();
  const l = addRectangle(s, [10, 10], [14, 12]);
  s.constraints.push({ type: "length", line: l[0], value: 4 });
  const corner = s.entities.find((e) => e.id === l[0]).geometry.start;
  const t = createBlock(s, [...l, corner], "Tapa");
  return { s, l, t, corner };
}

test("crear un bloque: lo elegido es la primera copia y las internas quedan en el bloque", () => {
  const { s, l, t, corner } = withBlock();
  assert.equal(typeof t, "object");
  assert.equal(t.anchor, corner);
  assert.deepEqual(t.entities, l);
  const b = s.blocks[0];
  assert.equal(b.name, "Tapa");
  assert.deepEqual(b.points.find((p) => p.id === 0), { id: 0, x: 0, y: 0 });
  assert.deepEqual(b.points.map((p) => [p.x, p.y]).sort(), [[0, 0], [0, 2], [4, 0], [4, 2]].sort());
  // horizontal/vertical y el largo pasaron al bloque
  assert.equal(b.constraints.length, 5);
  assert.equal(s.constraints.length, 0);
  assert.match(createBlock(s, [l[0]], "Otra"), /texto o una copia/);
});

test("insertar copias giradas y escaladas, y editar el bloque cambia todas", () => {
  const { s } = withBlock();
  const c = insertBlock(s, 1, [50, 0], 90, 2);
  // Esquina en (50, 0); el lado de 4 girado 90° y al doble: hasta (50, 8)
  const pts = c.points.map((p) => at(s, p));
  assert.ok(pts.some(([x, y]) => x === 50 && y === 8), JSON.stringify(pts));
  assert.ok(pts.some(([x, y]) => x === 46 && y === 8), JSON.stringify(pts));
  assert.equal(c.size, 2);
  // Editar: el lado de 4 pasa a 6 (los puntos de x = 4 se corren)
  const ed = blockSketch(s.blocks[0]);
  for (const p of ed.points) if (p.x === 4) p.x = 6;
  assert.equal(saveBlock(s, 1, ed), undefined);
  const first = s.texts.find((t) => t.anchor !== c.anchor);
  assert.ok(first.points.some((p) => at(s, p)[0] === 16), "la primera copia se alargó");
  const second = s.texts.find((t) => t.anchor === c.anchor);
  assert.ok(second.points.some((p) => at(s, p)[1] === 12), "la girada también (6 × 2 = 12)");
  assert.equal(s.texts.length, 2);
});

test("la copia sigue su giro: girar el sketch y rehacer deja todo en su lugar", () => {
  const { s } = withBlock();
  rotateSketch(s, 30);
  const before = s.texts[0].points.map((p) => at(s, p));
  refreshBlock(s, 1);
  const after = s.texts[0].points.map((p) => at(s, p));
  for (let i = 0; i < before.length; i++) {
    near(before[i][0], after[i][0], "x");
    near(before[i][1], after[i][1], "y");
  }
});

test("desarmar una copia: queda suelta con las restricciones del bloque", () => {
  const { s, t } = withBlock();
  explodeBlock(s, t.id);
  assert.equal(s.texts.length, 0);
  assert.equal(s.constraints.length, 5);
  assert.ok(s.constraints.every((c) => Object.values(c).every((v) => typeof v !== "number" || c.value === v || s.points.some((p) => p.id === v) || s.entities.some((e) => e.id === v))));
});

test("capas: ocultar, bloquear, pasar entidades y borrar la capa", () => {
  const s = sketch();
  const a = addEntity(s, { type: "line", start: addPoint(s, [0, 0]), end: addPoint(s, [1, 0]) });
  const b = addEntity(s, { type: "line", start: addPoint(s, [0, 1]), end: addPoint(s, [1, 1]) });
  const L = addLayer(s, "Cotas");
  setLayer(s, [a], L);
  s.layers[0].hidden = true;
  assert.deepEqual([...layerState(s).hidden], [a]);
  s.layers[0].locked = true;
  assert.deepEqual([...layerState(s).locked], [a]);
  setLayer(s, [a], null);
  assert.equal(layerState(s).hidden.size, 0);
  setLayer(s, [b], L);
  removeLayer(s, L);
  assert.equal(s.entities.find((e) => e.id === b).layer, undefined);
});
