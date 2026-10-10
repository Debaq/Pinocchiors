// Pruebas de coordenadas, DXF y SVG del sketch (lib/sketchFiles.ts), sin navegador:
//   node --test apps/web/e2e/sketchFiles.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { addEntity, addPoint, addRectangle } from "../src/lib/cad.ts";
import { checkSketch } from "../src/lib/sketchCheck.ts";
import { addShapes, parseCoordinates, parseDxf, sketchToDxf, sketchToSvg } from "../src/lib/sketchFiles.ts";

function sketch() {
  return { origin: 0, points: [{ id: 0, x: 0, y: 0 }], entities: [], constraints: [] };
}
const near = (a, b, what = "") => assert.ok(Math.abs(a - b) < 1e-6, `${what} ${a} ≠ ${b}`);
const dxf = (body, units = 4) => ["0", "SECTION", "2", "HEADER", "9", "$INSUNITS", "70", String(units), "0", "ENDSEC", "0", "SECTION", "2", "ENTITIES", ...body, "0", "ENDSEC", "0", "EOF"].join("\n");

test("coordenadas: comas, punto y coma con coma decimal, encabezado y z", () => {
  assert.deepEqual(parseCoordinates("x,y\n0,0\n10,5\n# nota\n20, 0\n"), [[0, 0], [10, 5], [20, 0]]);
  assert.deepEqual(parseCoordinates("0;0\n2,5;1,5;9\n"), [[0, 0], [2.5, 1.5]]);
  assert.deepEqual(parseCoordinates("1 2 3\n4\t5\n"), [[1, 2], [4, 5]]);
  assert.match(parseCoordinates("hola\n1,2"), /dos filas/);
});

test("DXF: rectángulo en polilínea con un lado curvo, círculo y arco, en pulgadas", () => {
  // Polilínea cerrada (0,0)-(2,0)-(2,1)-(0,1) con el último tramo en media vuelta (bulge −1: horario)
  const body = [
    "0", "LWPOLYLINE", "8", "0", "90", "4", "70", "1",
    "10", "0", "20", "0", "10", "2", "20", "0", "10", "2", "20", "1", "10", "0", "20", "1", "42", "-1",
    "0", "CIRCLE", "10", "5", "20", "5", "40", "1",
    "0", "ARC", "10", "0", "20", "0", "40", "1", "50", "0", "51", "90",
    "0", "INSERT", "2", "X",
  ];
  const r = parseDxf(dxf(body, 1));
  assert.equal(typeof r, "object");
  assert.deepEqual(r.skipped, { INSERT: 1 });
  const s = sketch();
  const ids = addShapes(s, r.shapes);
  assert.equal(ids.length, 6);
  const types = s.entities.map((e) => e.geometry.type);
  assert.deepEqual(types, ["line", "line", "line", "arc", "circle", "arc"]);
  // El lado curvo: media vuelta de radio 0,5 pulgada con centro en (0, 0,5 pulgada)
  const arc = s.entities[3].geometry;
  const at = (id) => s.points.find((p) => p.id === id);
  near(at(arc.center).x, 0, "cx");
  near(at(arc.center).y, 12.7, "cy");
  near(s.entities[4].geometry.radius, 25.4, "círculo");
  // La polilínea cerró; el arco termina en su vértice (0, 1 pulgada): queda suelto solo su comienzo
  assert.equal(checkSketch(s).looseEnds.length, 1);
});

test("DXF: spline por polos con sus nudos, por puntos de paso, elipse y lo que no es DXF", () => {
  const body = [
    "0", "SPLINE", "70", "8", "71", "2", "72", "6", "73", "3", "40", "0", "40", "0", "40", "0", "40", "1", "40", "1", "40", "1",
    "10", "0", "20", "0", "10", "5", "20", "10", "10", "10", "20", "0",
    "0", "SPLINE", "70", "8", "71", "3", "74", "3", "11", "0", "21", "0", "11", "1", "21", "1", "11", "2", "21", "0",
    "0", "ELLIPSE", "10", "0", "20", "0", "11", "4", "21", "0", "40", "0.5", "41", "0", "42", "3.141592653589793",
  ];
  const r = parseDxf(dxf(body));
  assert.deepEqual(r.shapes.map((x) => x.kind), ["bspline", "spline", "ellipse"]);
  assert.deepEqual(r.shapes[0].knots, [0, 0, 0, 1, 1, 1]);
  const e = r.shapes[2];
  assert.deepEqual(e.minor, [0, 2]);
  near(e.end[0], -4);
  assert.match(parseDxf("AutoCAD binary DXF\n\x1a"), /DXF/);
});

test("exportar: DXF que se vuelve a leer igual y SVG con la construcción punteada", () => {
  const s = sketch();
  addRectangle(s, [0, 0], [10, 5]);
  const c = addPoint(s, [20, 0]);
  addEntity(s, { type: "arc", center: c, start: addPoint(s, [25, 0]), end: addPoint(s, [20, 5]) });
  const k = addEntity(s, { type: "line", start: addPoint(s, [0, -2]), end: addPoint(s, [10, -2]) });
  s.entities.find((e) => e.id === k).construction = true;
  const text = sketchToDxf(s);
  assert.match(text, /CONSTRUCCION/);
  const back = parseDxf(text);
  assert.deepEqual(back.shapes.map((x) => x.kind), ["line", "line", "line", "line", "arc", "line"]);
  const arc = back.shapes[4];
  near(arc.a[0], 25);
  near(arc.b[1], 5);
  const svg = sketchToSvg(s);
  assert.match(svg, /viewBox="0 0 /);
  assert.equal((svg.match(/<polyline/g) ?? []).length, 6);
  assert.equal((svg.match(/stroke-dasharray/g) ?? []).length, 1);
});
