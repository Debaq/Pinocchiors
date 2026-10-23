// Plano de un ensamble: lista de materiales y globos (lib/drawingBom.ts y su dibujo en la hoja), sin navegador:
//   node --test apps/web/e2e/drawingBom.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { SHEETS, sheetDxf, sheetSvg } from "../src/lib/drawing.ts";
import { bomRows, bomTable, placeBalloons } from "../src/lib/drawingBom.ts";

const part = (feature, index = 0) => ({ feature, index });

test("una fila por pieza, con sus instancias contadas", () => {
  const parts = [
    { id: part(3), name: "Caja" },
    { id: part(1048579), name: "Tapa" },
  ];
  const instances = [
    { id: 1, part: part(3) },
    { id: 2, part: part(1048579) },
    { id: 3, part: part(3) },
    // De una pieza que ya no está: no cuenta
    { id: 4, part: part(9) },
  ];
  const rows = bomRows(instances, parts);
  assert.deepEqual(rows, [
    { item: 1, name: "Caja", qty: 2, instances: [1, 3] },
    { item: 2, name: "Tapa", qty: 1, instances: [2] },
  ]);
  const table = bomTable(rows, "Lista de materiales");
  assert.equal(table.title, "Lista de materiales");
  assert.deepEqual(table.rows, [
    ["1  Caja", "×2"],
    ["2  Tapa", "×1"],
  ]);
  // Nombres largos se acortan
  const long = bomTable([{ item: 1, name: "Soporte del motor con agujeros M3", qty: 1, instances: [1] }], "L").rows[0][0];
  assert.equal(long, "1  Soporte del motor con a…");
});

test("los globos salen hacia su pieza, fuera de la vista y sin pisarse", () => {
  const box = { min: [0, 0], max: [100, 50] };
  const anchors = [
    { label: "1", to: [90, 25] },
    { label: "2", to: [10, 25] },
    // Dos en el mismo lugar: se separan
    { label: "3", to: [50, 45] },
    { label: "4", to: [50, 45] },
  ];
  const out = placeBalloons(anchors, box, 10, 8);
  assert.equal(out.length, 4);
  const at = Object.fromEntries(out.map((b) => [b.label, b.at]));
  assert.ok(at["1"][0] > 100, "a la derecha");
  assert.ok(at["2"][0] < 0, "a la izquierda");
  assert.ok(at["3"][1] > 50 && at["4"][1] > 50, "arriba");
  for (const b of out) {
    const inside = b.at[0] > 0 && b.at[0] < 100 && b.at[1] > 0 && b.at[1] < 50;
    assert.ok(!inside, `${b.label} fuera de la caja`);
    assert.deepEqual(b.to, anchors.find((a) => a.label === b.label).to);
  }
  for (let i = 0; i < out.length; i++)
    for (let j = i + 1; j < out.length; j++) {
      const d = Math.hypot(out[i].at[0] - out[j].at[0], out[i].at[1] - out[j].at[1]);
      assert.ok(d >= 7.9, `${out[i].label}-${out[j].label}: ${d}`);
    }
  // Muchos: se reparten la vuelta entera
  const many = placeBalloons(Array.from({ length: 30 }, (_, i) => ({ label: String(i), to: [50, 25] })), box, 10, 8);
  assert.equal(new Set(many.map((b) => b.at.join())).size, 30);
  assert.deepEqual(placeBalloons([], box, 10, 8), []);
});

test("globos y lista en la hoja y en el DXF", () => {
  const sheet = SHEETS[0];
  const placed = [{ name: "iso", x: 100, y: 100, scale: 1, lines: [{ kind: "visible", points: [[0, 0], [10, 0]] }] }];
  const info = { title: "", author: "", material: "", date: "", scale: "1:1", projection: "first", sheet: "A4" };
  const marks = { balloons: [{ view: "iso", label: "1", to: [0, 0], at: [-20, 0] }], tables: [bomTable([{ item: 1, name: "Caja", qty: 2, instances: [1, 2] }], "Lista de materiales")] };
  const svg = sheetSvg(placed, sheet, info, { hidden: false, smooth: false, marks });
  assert.match(svg, /data-balloon="1"/);
  // El globo a 20 mm a la izquierda; la línea sale de su borde (radio 3,5) hasta el punto
  assert.match(svg, /<circle cx="80" cy="100" r="3.5" fill="#fff"\/>/);
  assert.match(svg, /<line x1="83.5" y1="100" x2="100" y2="100"\/>/);
  assert.match(svg, /data-gear-table="Lista de materiales"/);
  assert.match(svg, />1  Caja</);
  const dxf = sheetDxf(placed, sheet, { hidden: false, smooth: false, marks });
  assert.match(dxf, /CIRCLE\n8\nCOTAS\n10\n80/);
  assert.match(dxf, /\n1  Caja\n/);
  assert.doesNotMatch(sheetSvg(placed, sheet, info, { hidden: false, smooth: false }), /data-balloon/);
});
