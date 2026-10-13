// Engranes en el plano 2D (lib/gearSheet.ts y su dibujo en la hoja), sin navegador:
//   node --test apps/web/e2e/gearSheet.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { SHEETS, sheetDxf, sheetSvg, viewSpec } from "../src/lib/drawing.ts";
import { gearMarks, gearTable, gearTableBoxes } from "../src/lib/gearSheet.ts";

const spec = { kind: "external", module: 2, teeth: 20, pressure_angle: 20, helix_angle: 0, shift: 0, backlash: 0, addendum: 1, dedendum: 1.25, root_fillet: 0.25, cone_angle: 45, shaft_angle: 90, worm_diameter: 0, left_hand: false };
const gear = (over = {}, part = {}) => ({
  feature: 3,
  part: { spec: { ...spec, ...over }, width: 10, bore: 8, hub_diameter: 0, hub_length: 0, keyway: false, outer: 0, hole_count: 0, hole_diameter: 0, hole_circle: 0, print_clearance: 0, ...part },
  origin: [5, 7, 0],
  z: [0, 0, 1],
  x: [1, 0, 0],
  pitch_radius: 20,
  center_distance: 0,
});
const info = { pitch_diameter: 40, base_diameter: 37.588, tip_diameter: 44, root_diameter: 35, module_t: 2, pressure_angle_t: 20, min_teeth: 18, warnings: [], span_teeth: 3, span: 15.321, pin_diameter: 3.4, over_pins: 43.2, error: null };

test("el círculo primitivo solo va en las vistas que miran a lo largo del eje", () => {
  const views = ["front", "top", "left", "iso"].map((name) => ({ name, dir: viewSpec(name) }));
  const marks = gearMarks([gear()], views);
  // El eje es Z: lo miran la superior (y la inferior si estuviera)
  assert.deepEqual(marks.map((m) => m.view), ["top"]);
  assert.equal(marks[0].radius, 20);
  // El centro es el del engrane en esa vista (x, y del modelo, hacia arriba de la hoja)
  assert.ok(Math.abs(marks[0].center[0] - 5) < 1e-9);
  assert.ok(Math.abs(Math.abs(marks[0].center[1]) - 7) < 1e-9);
  // La cremallera no tiene círculo
  assert.equal(gearMarks([gear({ kind: "rack" })], views).length, 0);
});

test("la tabla trae lo que corresponde a cada tipo", () => {
  const rows = (t) => Object.fromEntries(t.rows);
  const t = gearTable(gear(), info, "Engrane 1");
  assert.equal(t.title, "Engrane 1");
  const r = rows(t);
  assert.equal(r["Dientes (z)"], "20");
  assert.equal(r["Ø primitivo"], "40");
  assert.equal(r["Cota sobre 3 dientes"], "15,321");
  assert.equal(r["Cota sobre bolas Ø3,4"], "43,2");
  assert.equal(r["Ángulo de hélice (β)"], undefined);
  assert.equal(rows(gearTable(gear({ helix_angle: -15, shift: 0.3 }), info, "H"))["Ángulo de hélice (β)"], "-15°");
  const bevel = rows(gearTable(gear({ kind: "bevel" }), { ...info, span: 0, over_pins: 0 }, "C"));
  assert.equal(bevel["Ángulo del cono (δ)"], "45°");
  assert.equal(bevel["Ø base"], undefined);
  const worm = rows(gearTable(gear({ kind: "worm", teeth: 2, worm_diameter: 20 }), { ...info, pitch_diameter: 20, span: 0, over_pins: 0 }, "S"));
  assert.equal(worm["Entradas (z₁)"], "2");
  assert.equal(worm["Avance"], "12,566");
  assert.equal(worm["Ángulo de avance (γ)"], "11,31°");
  assert.equal(rows(gearTable(gear({ kind: "rack", teeth: 5 }), info, "R"))["Paso"], "6,283");
});

test("las tablas se apilan sobre el cajetín, pegadas a la derecha", () => {
  const t = (n) => ({ title: `G${n}`, rows: [["a", "1"], ["b", "2"]] });
  const boxes = gearTableBoxes([t(1), t(2)], 297, 210, 10, 28);
  assert.equal(boxes.length, 2);
  assert.equal(boxes[0].x + boxes[0].w, 297 - 10);
  assert.ok(boxes[0].y + boxes[0].h <= 210 - 10 - 28);
  assert.ok(boxes[1].y + boxes[1].h < boxes[0].y);
});

test("la hoja dibuja el círculo, su cruz y la tabla (SVG y DXF)", () => {
  const sheet = SHEETS[0];
  const placed = [{ name: "top", x: 100, y: 100, scale: 1, lines: [{ kind: "visible", points: [[0, 0], [10, 0]] }] }];
  const info2 = { title: "T", date: "2026-10-13", scale: "1:1", projection: "first", sheet: sheet.name };
  const marks = { gears: [{ view: "top", center: [5, 7], radius: 20 }], tables: [gearTable(gear(), info, "Engrane 1")] };
  const svg = sheetSvg(placed, sheet, info2, { hidden: false, smooth: false, marks });
  assert.match(svg, /data-pitch-circle="top"/);
  assert.match(svg, /<circle cx="105" cy="93" r="20"\/>/);
  assert.match(svg, /stroke-dasharray="8 1.5 1.5 1.5"/);
  assert.match(svg, /data-gear-table="Engrane 1"/);
  assert.match(svg, />Dientes \(z\)</);
  const dxf = sheetDxf(placed, sheet, { hidden: false, smooth: false, marks });
  assert.match(dxf, /CIRCLE/);
  assert.match(dxf, /Cota sobre 3 dientes/);
  // Sin marcas, nada de esto
  assert.doesNotMatch(sheetSvg(placed, sheet, info2, { hidden: false, smooth: false }), /pitch-circle|gear-table/);
});
