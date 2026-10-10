// Pruebas de los planos 2D del documento (lib/drawingDoc.ts), sin navegador:
//   node --test apps/web/e2e/drawingDoc.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import {
  addDrawing,
  drawingSheets,
  drawingTree,
  elementNodeId,
  elementVisible,
  parseElementNode,
  removeDrawing,
  removeElement,
  sheetElements,
  toggleElement,
  updateDrawing,
} from "../src/lib/drawingDoc.ts";

test("el plano viejo (uno solo, sin lista) se lee como «Plano 1» y se reescribe al primer cambio", () => {
  const doc = { drawing: { dims: [{ view: "front", kind: "length", refs: [[1, 2]] }], details: [], sketch: null } };
  const [s] = drawingSheets(doc);
  assert.equal(s.name, "Plano 1");
  assert.equal(s.dims.length, 1);
  assert.equal(s.sheet, "A4");
  assert.ok(doc.drawing.dims, "leer no cambia el documento");
  updateDrawing(doc, 1, (x) => (x.title = "Soporte"));
  assert.deepEqual(Object.keys(doc.drawing).sort(), ["next_id", "sheets"]);
  assert.equal(doc.drawing.sheets[0].title, "Soporte");
  assert.equal(doc.drawing.sheets[0].dims.length, 1);
  assert.deepEqual(drawingSheets({ drawing: null }), []);
  assert.deepEqual(drawingSheets({ drawing: {} }), []);
});

test("agregar y borrar planos: los ids no se repiten", () => {
  const doc = {};
  assert.equal(addDrawing(doc), 1);
  assert.equal(addDrawing(doc), 2);
  removeDrawing(doc, 2);
  assert.equal(addDrawing(doc), 3, "el id del borrado no vuelve");
  assert.deepEqual(drawingSheets(doc).map((s) => s.name), ["Plano 1", "Plano 3"]);
  assert.equal(updateDrawing(doc, 9, () => {}), false);
});

test("ids del Outliner de ida y vuelta", () => {
  const els = [
    undefined,
    { kind: "sheet" },
    { kind: "view", view: "iso" },
    { kind: "overall" },
    { kind: "section" },
    { kind: "detail", label: "B" },
    { kind: "dim", index: 12 },
    { kind: "sketch" },
  ];
  for (const el of els) assert.deepEqual(parseElementNode(elementNodeId(4, el)), el ? { drawing: 4, element: el } : { drawing: 4 });
  assert.equal(parseElementNode("drawing-1:view:nada"), undefined);
  assert.equal(parseElementNode("cad-3"), undefined);
});

test("ocultar y quitar elementos; las vistas solo se ocultan", () => {
  const doc = {};
  const id = addDrawing(doc);
  updateDrawing(doc, id, (s) => {
    s.section = { y: 2 };
    s.details = [{ label: "B", view: "front", center: [0, 0], radius: 3 }];
    s.dims = [{ view: "top", kind: "diameter", refs: [[0, 0]] }, { view: "front", kind: "length", refs: [[1, 0]] }];
  });
  const s = drawingSheets(doc)[0];
  assert.deepEqual(sheetElements(s).map((e) => e.kind), ["sheet", ...Array(7).fill("view"), "overall", "section", "detail", "dim", "dim"]);
  for (const el of [{ kind: "view", view: "iso" }, { kind: "section" }, { kind: "detail", label: "B" }, { kind: "dim", index: 1 }, { kind: "overall" }]) {
    assert.ok(elementVisible(s, el));
    toggleElement(s, el);
    assert.ok(!elementVisible(s, el), JSON.stringify(el));
    toggleElement(s, el);
    assert.ok(elementVisible(s, el));
  }
  removeElement(s, { kind: "dim", index: 0 });
  assert.equal(s.dims[0].kind, "length");
  removeElement(s, { kind: "section" });
  assert.equal(s.section, null);
});

test("árbol: el plano abierto y lo elegido se marcan", () => {
  const doc = {};
  addDrawing(doc);
  const id = addDrawing(doc);
  updateDrawing(doc, id, (s) => (s.details = [{ label: "B", view: "front", center: [0, 0], radius: 3, hidden: true }]));
  const tree = drawingTree(drawingSheets(doc), id, { kind: "detail", label: "B" });
  assert.deepEqual(tree.map((n) => [n.id, n.selected, n.expanded]), [["drawing-1", false, false], ["drawing-2", false, true]]);
  const det = tree[1].children.find((c) => c.id === "drawing-2:detail:B");
  assert.ok(det.selected && !det.visible && det.deletable);
  const view = tree[1].children.find((c) => c.id === "drawing-2:view:front");
  assert.ok(!view.deletable && view.visible);
  assert.equal(drawingTree(drawingSheets(doc), id, undefined)[1].selected, true, "sin nada elegido se marca el plano");
});

test("seis vistas: al empezar se ven frente, superior, la lateral de la derecha del frente e isométrica; los planos viejos pasan su «side»", () => {
  const doc = {};
  addDrawing(doc);
  assert.deepEqual(drawingSheets(doc)[0].hiddenViews, ["bottom", "right", "back"]);
  // Plano de antes en tercer diedro con la isométrica oculta y una cota en la lateral
  const old = { drawing: { sheets: [{ id: 1, name: "P", projection: "third", hiddenViews: ["iso"], dims: [{ view: "side", kind: "length", refs: [[0, 0]] }], details: [{ label: "B", view: "side", center: [0, 0], radius: 2 }] }], next_id: 2 } };
  const [s] = drawingSheets(old);
  assert.deepEqual(s.hiddenViews, ["bottom", "left", "back", "iso"]);
  assert.equal(s.dims[0].view, "right");
  assert.equal(s.details[0].view, "right");
  // Del formato de un solo plano (primer diedro): la lateral es la izquierda
  const [one] = drawingSheets({ drawing: { dims: [{ view: "side", kind: "length", refs: [[0, 0]] }] } });
  assert.equal(one.dims[0].view, "left");
  assert.ok(!one.hiddenViews.includes("left"));
});
