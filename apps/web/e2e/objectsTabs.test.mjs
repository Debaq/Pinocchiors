// Pruebas de los objetos de la escena por pestaña del diseño (lib/objects.ts y
// el agrupado del Outliner en lib/scene-tree.ts), sin navegador:
//   node --test apps/web/e2e/objectsTabs.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { isPartObject, studioOf, syncCadObjects } from "../src/lib/objects.ts";
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
