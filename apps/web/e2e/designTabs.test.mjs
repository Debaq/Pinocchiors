// Pruebas de las pestañas de Diseñar (lib/designTabs.ts), sin navegador:
//   node --test apps/web/e2e/designTabs.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { clipFormulas, dropIndex, instanceStudio, neighborTab, nestableAssemblies, pasteFormulas, splitTabFeature, swapTabHistory, tabFeature, VARIANT_BASE } from "../src/lib/designTabs.ts";

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

test("la pestaña de la pieza de una instancia, también en una variante", () => {
  assert.deepEqual(splitTabFeature(tabFeature(3, 12)), [3, 12]);
  assert.equal(instanceStudio({ part: { feature: tabFeature(2, 5) } }), 2);
  // Variante (otra configuración): la pestaña es la que trae la instancia
  assert.equal(instanceStudio({ studio: 4, part: { feature: tabFeature(VARIANT_BASE + 1, 5) } }), 4);
  assert.equal(instanceStudio({ part: { feature: 7 } }), 0);
});

test("las operaciones copiadas llevan sus fórmulas y los parámetros que faltan", () => {
  const source = {
    parameters: [
      { name: "ancho", expr: "2 * base" },
      { name: "base", expr: "10" },
      { name: "alto", expr: "5" },
      { name: "suelto", expr: "1" },
    ],
    bindings: { "3.kind.extent.distance": "alto", "3.kind.draft": "1", "4.kind.shape.dx": "ancho + 1", "9.kind.radius": "suelto" },
  };
  const clip = clipFormulas(source, [3, 4]);
  assert.deepEqual(Object.keys(clip.bindings).sort(), ["3.kind.draft", "3.kind.extent.distance", "4.kind.shape.dx"]);
  // Otra pestaña que ya tiene «alto» (con otro valor: se respeta)
  const target = { parameters: [{ name: "alto", expr: "8" }], bindings: { "1.kind.offset": "alto" } };
  const added = pasteFormulas(target, clip, new Map([[3, 20], [4, 21]]));
  assert.deepEqual(added, ["ancho", "base"]);
  assert.deepEqual(target.bindings, { "1.kind.offset": "alto", "20.kind.extent.distance": "alto", "20.kind.draft": "1", "21.kind.shape.dx": "ancho + 1" });
  assert.deepEqual(target.parameters.map((p) => `${p.name}=${p.expr}`), ["alto=8", "ancho=2 * base", "base=10"]);
  // Sin fórmulas: nada cambia
  const plain = {};
  assert.deepEqual(pasteFormulas(plain, clipFormulas(undefined, [1]), new Map([[1, 2]])), []);
  assert.deepEqual(plain, {});
});

test("qué ensambles pueden ir dentro de otro sin formar un ciclo", () => {
  const tab = (id, kind, used_by = []) => ({ id, kind, used_by });
  // 0 diseño; 1 lo usa 2; 2 lo usa 3; 4 suelto
  const tabs = [tab(0, "design", [1]), tab(1, "assembly", [2]), tab(2, "assembly", [3]), tab(3, "assembly"), tab(4, "assembly")];
  assert.deepEqual(nestableAssemblies(tabs, 1), [4], "ni él mismo ni los que lo llevan (2 y, por 2, el 3)");
  assert.deepEqual(nestableAssemblies(tabs, 3), [1, 2, 4]);
  assert.deepEqual(nestableAssemblies(tabs, 4), [1, 2, 3]);
});
