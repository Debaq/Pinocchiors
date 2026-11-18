// Pruebas de los objetos de la escena por pestaña del diseño (lib/objects.ts y
// el agrupado del Outliner en lib/scene-tree.ts), sin navegador:
//   node --test apps/web/e2e/objectsTabs.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { dropOrphanTabObjects, isPartObject, studioOf, syncCadObjects } from "../src/lib/objects.ts";
import { groupByTab } from "../src/lib/scene-tree.ts";

const part = (feature, index = 0) => ({ feature, index });

test("un objeto de antes (sin pestaña) es de la pestaña 0", () => {
  const old = { id: 1, name: "Caja", source: { kind: "cad", part: part(0), hash: "abc" } };
  assert.equal(studioOf(old), 0);
  assert.ok(isPartObject(old, 0, part(0)));
  assert.ok(!isPartObject(old, 1, part(0)), "la misma pieza de otra pestaña es otra");
  // Sincronizar la pestaña 0 no lo duplica ni le pone pestaña
  const same = syncCadObjects([old], [{ id: part(0), name: "Caja" }], 0);
  assert.equal(same.length, 1);
  assert.equal(same[0], old);
});

test("cada pestaña tiene sus objetos", () => {
  let list = [{ id: 1, name: "Caja", source: { kind: "cad", part: part(0) } }];
  // Pestaña 1 con una pieza de la misma operación y número: objeto nuevo, con su pestaña
  list = syncCadObjects(list, [{ id: part(0), name: "Tapa" }], 1);
  assert.equal(list.length, 2);
  assert.deepEqual(list[1], { id: 2, name: "Tapa", source: { kind: "cad", studio: 1, part: part(0) } });
  assert.equal(list[0].name, "Caja", "la de la pestaña 0 no se renombra");
  // Renombrar en la pestaña 1 solo toca su objeto
  list = syncCadObjects(list, [{ id: part(0), name: "Tapa nueva" }], 1);
  assert.deepEqual(list.map((o) => o.name), ["Caja", "Tapa nueva"]);
  // Volver a la 0 sin cambios: la misma lista
  assert.equal(syncCadObjects(list, [{ id: part(0), name: "Caja" }], 0), list);
});

test("los objetos vacíos de piezas que ya no están se borran", () => {
  const cad = (id, feature, extra = {}) => ({ id, name: `Pieza ${id}`, source: { kind: "cad", studio: 1, part: part(feature), ...extra.source }, ...extra.obj });
  const list = [
    cad(1, 10), // vacía y su pieza ya no está: se borra
    cad(2, 11, { source: { hash: "h" } }), // activa con malla: se queda
    cad(3, 12, { obj: { ui: "{}" } }), // con malla guardada: se queda
    cad(4, 13, { obj: { history: { nodes: [], current: 0 } } }), // con historial: se queda
    cad(5, 14), // tachada (sigue en `keep`): se queda
    { id: 6, name: "Escaneo", source: { kind: "import" } },
    { id: 7, name: "Otra", source: { kind: "cad", studio: 2, part: part(10) } }, // de otra pestaña: no se toca
  ];
  const keep = { parts: new Set(["20:0", "14:0"]), active: 2 };
  const out = syncCadObjects(list, [{ id: part(20), name: "Nueva" }], 1, keep);
  assert.deepEqual(out.map((o) => o.id), [2, 3, 4, 5, 6, 7, 8]);
  assert.deepEqual(out.at(-1).source, { kind: "cad", studio: 1, part: part(20) });
  // Sin `keep` (como antes): no borra nada
  assert.equal(syncCadObjects(list, [], 1).length, list.length);
  // Nada que cambiar: la misma lista
  assert.equal(syncCadObjects(out, [{ id: part(20), name: "Nueva" }], 1, keep), out);
});

test("los objetos vacíos de pestañas borradas se borran", () => {
  const list = [
    { id: 1, name: "A", source: { kind: "cad", part: part(0) } }, // pestaña 0 (de antes), ya no existe
    { id: 2, name: "B", source: { kind: "cad", studio: 5, part: part(0) } }, // pestaña 5 borrada
    { id: 3, name: "C", source: { kind: "cad", studio: 5, part: part(1) }, ui: "{}" }, // con malla: se queda
    { id: 4, name: "D", source: { kind: "cad", studio: 1, part: part(0) } }, // pestaña que sigue
    { id: 5, name: "E", source: { kind: "cad", studio: 9, part: part(0) } }, // activo: se queda
    { id: 6, name: "Escaneo", source: { kind: "import" } },
  ];
  assert.deepEqual(dropOrphanTabObjects(list, [1, 2], 5).map((o) => o.id), [3, 4, 5, 6]);
  assert.equal(dropOrphanTabObjects(list, [], 5), list, "sin pestañas cargadas no se toca");
  assert.equal(dropOrphanTabObjects(list, [0, 1, 5, 9], undefined), list, "todas siguen: la misma lista");
});

test("el Outliner agrupa por pestaña solo con más de una", () => {
  const node = (id) => ({ id, type: "part", label: id, visible: true, expanded: false, selected: false, children: [] });
  const tab = (id) => ({ id, name: `Diseño ${id + 1}` });
  // Una sola pestaña: tal cual
  const one = [node("a"), node("b")];
  assert.equal(groupByTab([{ tab: tab(0) }, { tab: tab(0) }], one), one);
  // Importado suelto, piezas de dos pestañas en subgrupos (en el lugar de la primera)
  const rows = [{ tab: tab(0) }, {}, { tab: tab(1) }, { tab: tab(0) }];
  const out = groupByTab(rows, [node("a"), node("cubo"), node("b"), node("c")]);
  assert.deepEqual(out.map((n) => n.id), ["group-tab-0", "cubo", "group-tab-1"]);
  assert.deepEqual(out[0].children.map((n) => n.id), ["a", "c"]);
  assert.equal(out[2].label, "Diseño 2");
});
