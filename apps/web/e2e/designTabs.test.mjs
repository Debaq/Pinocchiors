// Pruebas de las pestañas de Diseñar (lib/designTabs.ts), sin navegador:
//   node --test apps/web/e2e/designTabs.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { dropIndex, neighborTab, swapTabHistory, tabFeature } from "../src/lib/designTabs.ts";

const tabs = (...ids) => ids.map((id) => ({ id }));

test("pestaña vecina en ronda", () => {
  assert.equal(neighborTab(tabs(0, 3, 5), 3, 1), 5);
  assert.equal(neighborTab(tabs(0, 3, 5), 5, 1), 0);
  assert.equal(neighborTab(tabs(0, 3, 5), 0, -1), 5);
  assert.equal(neighborTab(tabs(0), 0, 1), undefined, "una sola: nada");
  assert.equal(neighborTab(tabs(0, 3), null, 1), 0, "sin activa: la primera");
});

test("números de operación entre pestañas, como el backend (pestaña arriba de 20 bits)", () => {
  assert.equal(tabFeature(0, 7), 7, "la pestaña 0 no cambia los números");
  assert.equal(tabFeature(3, 12), 3 * 1048576 + 12);
  assert.ok(tabFeature(4095, 2 ** 20 - 1) > 0, "sin pasar a negativo");
});

test("posición al soltar", () => {
  // [A B C]: A soltada después de B (hueco 2) queda en 1; C antes de A (hueco 0) en 0
  assert.equal(dropIndex(0, 2), 1);
  assert.equal(dropIndex(2, 0), 0);
  assert.equal(dropIndex(1, 1), 1, "en su lugar");
  assert.equal(dropIndex(1, 2), 1, "justo después de sí misma: no se mueve");
});

test("cada pestaña conserva su deshacer", () => {
  const saved = new Map();
  const undo = ["a1", "a2"];
  const redo = ["a3"];
  // De la 0 a una nueva (1): pilas vacías, las de la 0 guardadas
  swapTabHistory(saved, undo, redo, 0, { tabs: tabs(0, 1), active: 1 });
  assert.deepEqual([undo, redo], [[], []]);
  undo.push("b1");
  // Vuelta a la 0: las suyas; las de la 1 guardadas
  swapTabHistory(saved, undo, redo, 1, { tabs: tabs(0, 1), active: 0 });
  assert.deepEqual([undo, redo], [["a1", "a2"], ["a3"]]);
  assert.deepEqual(saved.get(1), { undo: ["b1"], redo: [] });
  // Renombrar o mover (la activa no cambia): nada se toca
  swapTabHistory(saved, undo, redo, 0, { tabs: tabs(1, 0), active: 0 });
  assert.deepEqual(undo, ["a1", "a2"]);
  // Borrar la 1: se olvida su deshacer
  swapTabHistory(saved, undo, redo, 0, { tabs: tabs(0), active: 0 });
  assert.equal(saved.size, 0);
  // Borrar la activa: la que queda empieza sin deshacer (no hay a quién devolverle las pilas)
  swapTabHistory(saved, undo, redo, 0, { tabs: tabs(2), active: 2 });
  assert.deepEqual([undo, redo, saved.size], [[], [], 0]);
});
