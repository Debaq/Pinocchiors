// Pruebas de coordenadas, DXF y SVG del sketch (lib/sketchFiles.ts), sin navegador:
//   node --test apps/web/e2e/sketchFiles.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { addEntity, addPoint, addRectangle } from "../src/lib/cad.ts";
import { checkSketch } from "../src/lib/sketchCheck.ts";
import { addShapes, parseCoordinates, parseDxf, parseSvg, sketchToDxf, sketchToSvg } from "../src/lib/sketchFiles.ts";

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
  // La polilínea cerró; el arco termina en su vértice (0, 1 pulgada) y empieza
  // en (1 pulgada, 0), en medio del lado de abajo: apoyado en T, no queda suelto
  assert.equal(checkSketch(s).looseEnds.length, 0);
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

test("coordenadas: coma decimal separada por espacios", () => {
  assert.deepEqual(parseCoordinates("1,5 2,5\n3,25 4\n"), [[1.5, 2.5], [3.25, 4]]);
  // Coma entre x e y con espacio: sigue siendo separador
  assert.deepEqual(parseCoordinates("10, 5\n20, 0\n"), [[10, 5], [20, 0]]);
});

/** DXF con una sección de bloques */
const withBlocks = (blocks, body, units = 4) =>
  ["0", "SECTION", "2", "HEADER", "9", "$INSUNITS", "70", String(units), "0", "ENDSEC", "0", "SECTION", "2", "BLOCKS", ...blocks, "0", "ENDSEC", "0", "SECTION", "2", "ENTITIES", ...body, "0", "ENDSEC", "0", "EOF"].join("\n");

test("DXF: bloques insertados (girados, escalados, anidados y en columnas)", () => {
  const blocks = [
    "0", "BLOCK", "2", "B", "10", "1", "20", "1",
    "0", "LINE", "10", "1", "20", "1", "11", "3", "21", "1",
    "0", "CIRCLE", "10", "2", "20", "2", "40", "1",
    "0", "ENDBLK",
    "0", "BLOCK", "2", "N", "10", "0", "20", "0",
    "0", "INSERT", "2", "B", "10", "1", "20", "1",
    "0", "ENDBLK",
  ];
  // B en (10, 0), girado 90° y al doble
  let r = parseDxf(withBlocks(blocks, ["0", "INSERT", "2", "B", "10", "10", "20", "0", "41", "2", "42", "2", "50", "90"]));
  assert.deepEqual(r.skipped, {});
  const [line, circle] = r.shapes;
  near(line.a[0], 10); near(line.a[1], 0); near(line.b[0], 10); near(line.b[1], 4);
  assert.equal(circle.kind, "circle");
  near(circle.c[0], 8); near(circle.c[1], 2); near(circle.r, 2);
  // Escala despareja: el círculo pasa a elipse
  r = parseDxf(withBlocks(blocks, ["0", "INSERT", "2", "B", "10", "0", "20", "0", "41", "3", "42", "1"]));
  const e = r.shapes[1];
  assert.equal(e.kind, "ellipse");
  near(Math.hypot(e.major[0] - e.c[0], e.major[1] - e.c[1]), 3);
  near(Math.hypot(e.minor[0] - e.c[0], e.minor[1] - e.c[1]), 1);
  // Anidado (N inserta B en (1,1)) y en dos columnas a 20
  r = parseDxf(withBlocks(blocks, ["0", "INSERT", "2", "N", "10", "0", "20", "0", "70", "2", "44", "20"]));
  assert.equal(r.shapes.length, 4);
  near(r.shapes[0].a[0], 1); near(r.shapes[2].a[0], 21);
});

test("DXF: arco con extrusión −Z (espejado en x) y unidades poco comunes", () => {
  const r = parseDxf(dxf(["0", "ARC", "10", "5", "20", "0", "40", "1", "50", "0", "51", "90", "230", "-1"]));
  const a = r.shapes[0];
  near(a.c[0], -5); near(a.c[1], 0);
  // Antihorario en el mundo: de (−5, 1) a (−6, 0)
  near(a.a[0], -5); near(a.a[1], 1); near(a.b[0], -6); near(a.b[1], 0);
  // Millas (3) y kilómetros (7)
  const mi = parseDxf(dxf(["0", "LINE", "10", "0", "20", "0", "11", "1", "21", "0"], 3));
  near(mi.shapes[0].b[0], 1609344);
  const km = parseDxf(dxf(["0", "LINE", "10", "0", "20", "0", "11", "1", "21", "0"], 7));
  near(km.shapes[0].b[0], 1e6);
});

test("DXF: radio cero y números rotos se saltan (y se cuentan)", () => {
  const r = parseDxf(dxf(["0", "CIRCLE", "10", "0", "20", "0", "40", "0", "0", "LINE", "10", "nan", "20", "0", "11", "1", "21", "0", "0", "LINE", "10", "0", "20", "0", "11", "1", "21", "1"]));
  assert.equal(r.shapes.length, 1);
  assert.deepEqual(r.skipped, { CIRCLE: 1, LINE: 1 });
});

test("DXF: SPLINE sin sujetar empieza sobre la curva; la periódica cerrada se cierra", () => {
  // Grado 2, nudos uniformes sin sujetar: la curva va de (2,5; 5) a (12,5; 5), no de polo a polo
  const open = parseDxf(dxf(["0", "SPLINE", "70", "8", "71", "2", ...[0, 1, 2, 3, 4, 5, 6].flatMap((k) => ["40", String(k)]), ...[[0, 0], [5, 10], [10, 0], [15, 10]].flatMap(([x, y]) => ["10", String(x), "20", String(y)])]));
  const b = open.shapes[0];
  assert.equal(b.kind, "bspline");
  near(b.poles[0][0], 2.5); near(b.poles[0][1], 5);
  near(b.poles[b.poles.length - 1][0], 12.5); near(b.poles[b.poles.length - 1][1], 5);
  assert.deepEqual(b.knots.slice(0, 3), [2, 2, 2]);
  // Periódica (70 = 11) con un período de nudos: cerrada, sus puntas en el mismo lugar
  const sq = [[0, 0], [10, 0], [10, 10], [0, 10]];
  const per = parseDxf(dxf(["0", "SPLINE", "70", "11", "71", "3", ...[0, 1, 2, 3, 4].flatMap((k) => ["40", String(k)]), ...sq.flatMap(([x, y]) => ["10", String(x), "20", String(y)])]));
  const c = per.shapes[0];
  const [p0, p1] = [c.poles[0], c.poles[c.poles.length - 1]];
  near(p0[0], p1[0]); near(p0[1], p1[1]);
  const s = sketch();
  addShapes(s, per.shapes);
  const g = s.entities[0].geometry;
  assert.equal(g.poles[0], g.poles[g.poles.length - 1], "un solo punto en la unión: lazo cerrado");
});

test("exportar DXF: vuelta entera como círculo; capas en ASCII y las ocultas apagadas", () => {
  const s = sketch();
  const c = addPoint(s, [0, 0]);
  addEntity(s, { type: "arc", center: c, start: addPoint(s, [5, 0]), end: addPoint(s, [5, 0]) });
  s.layers = [{ id: 1, name: "Pieza ñandú" }, { id: 2, name: "oculta", hidden: true }];
  const l = addEntity(s, { type: "line", start: addPoint(s, [0, 0]), end: addPoint(s, [1, 0]) });
  s.entities.find((e) => e.id === l).layer = 1;
  const text = sketchToDxf(s);
  assert.doesNotMatch(text, /\nARC\n/);
  const back = parseDxf(text);
  assert.equal(back.shapes[0].kind, "circle");
  near(back.shapes[0].r, 5);
  assert.match(text, /\nPIEZA_NANDU\n/);
  assert.doesNotMatch(text, /[^\x00-\x7f]/);
  // La oculta con color negativo
  assert.match(text, /OCULTA\n70\n0\n62\n-7\n/);
});

test("coordenadas georreferenciadas: no se unen extremos distintos ni se borran líneas cortas", () => {
  const X = 5e6;
  const s = sketch();
  const made = addShapes(s, [
    { kind: "line", a: [X, X], b: [X + 0.1, X] },
    { kind: "line", a: [X + 0.15, X], b: [X + 10, X] },
    { kind: "line", a: [X + 10, X], b: [X + 10, X + 10] },
  ]);
  assert.equal(made.length, 3, "la de 0,1 mm sigue");
  const g = (i) => s.entities.find((e) => e.id === made[i]).geometry;
  assert.notEqual(g(0).end, g(1).start, "a 0,05 mm no se juntan");
  assert.equal(g(1).end, g(2).start, "las que sí coinciden, juntas");
});

// ─── SVG (U8) ─────────────────────────────────────────────────────────────

const svgDoc = (inner, attrs = 'width="100mm" height="50mm" viewBox="0 0 100 50"') => `<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg" ${attrs}>${inner}</svg>`;
const round6 = (p) => p.map((x) => +x.toFixed(6));

test("SVG: rectángulo en mm con la y hacia arriba, cierra una región", () => {
  const r = parseSvg(svgDoc('<rect x="10" y="5" width="20" height="10"/>'));
  assert.equal(typeof r, "object");
  assert.equal(r.shapes.length, 4);
  const xs = r.shapes.flatMap((s) => [s.a, s.b]);
  // y de SVG 5..15 → 45..35 hacia arriba (alto 50)
  assert.deepEqual([Math.min(...xs.map((p) => p[1])), Math.max(...xs.map((p) => p[1]))], [35, 45]);
  const s = { origin: 0, points: [{ id: 0, x: 0, y: 0 }], entities: [], constraints: [] };
  addShapes(s, r.shapes);
  assert.equal(checkSketch(s).looseEnds.length, 0);
});

test("SVG: sin unidades son px (96 por pulgada) y los grupos transforman", () => {
  const r = parseSvg(svgDoc('<g transform="translate(96 0)"><circle cx="0" cy="0" r="48"/></g>', 'width="192" height="96"'));
  const c = r.shapes[0];
  assert.equal(c.kind, "circle");
  assert.ok(Math.abs(c.r - 12.7) < 1e-9);
  assert.deepEqual(round6(c.c), [25.4, 25.4]);
});

test("SVG: trazado con Bézier, arco y Z; texto contado como salteado", () => {
  const r = parseSvg(svgDoc('<path d="M0,0 C10,0 20,10 20,20 A10 10 0 0 1 0 20 z"/><text x="1" y="1">hola</text>'));
  assert.deepEqual(r.skipped, { text: 1 });
  const kinds = r.shapes.map((s) => s.kind);
  assert.deepEqual(kinds, ["bspline", "arc", "line"]);
  assert.equal(r.shapes[0].degree, 3);
  // El arco: de (20,20) a (0,20) en SVG con centro (10,20) → y hacia arriba en 30
  const arc = r.shapes[1];
  assert.deepEqual(round6(arc.c), [10, 30]);
  // En el SVG (y hacia abajo) el arco pasa por y = 30, abajo en pantalla: abajo del centro también acá
  const mid = [arc.c[0], arc.c[1] - 10];
  const ang = (p) => Math.atan2(p[1] - arc.c[1], p[0] - arc.c[0]);
  let sweep = ang(arc.b) - ang(arc.a);
  while (sweep <= 0) sweep += 2 * Math.PI;
  let toMid = ang(mid) - ang(arc.a);
  while (toMid <= 0) toMid += 2 * Math.PI;
  assert.ok(toMid < sweep, "el arco pasa por abajo del centro, como se ve el SVG");
  const s = { origin: 0, points: [{ id: 0, x: 0, y: 0 }], entities: [], constraints: [] };
  addShapes(s, r.shapes);
  assert.equal(checkSketch(s).looseEnds.length, 0);
});

test("SVG: defs y ocultos no se dibujan; un archivo que no es SVG da mensaje", () => {
  const r = parseSvg(svgDoc('<defs><rect width="5" height="5"/></defs><g style="display:none"><line x1="0" y1="0" x2="1" y2="1"/></g><line x1="0" y1="0" x2="10" y2="0"/>'));
  assert.equal(r.shapes.length, 1);
  assert.equal(typeof parseSvg("hola"), "string");
});
