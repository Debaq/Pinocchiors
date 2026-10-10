// Pruebas del armado de planos (lib/drawing.ts), sin navegador:
//   node --test apps/web/e2e/drawing.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { dragAxis, placementProblems, placeExtraViews, projectPolylines, rotateViewDir, viewDirFromCamera, viewUp, layout, sheetSvg, sheetDxf, sheetLayerLines, overallDimensions, userDimensions, SHEETS, viewSpec, scaleLabel, bounds } from "../src/lib/drawing.ts";

/** Rectángulo de w × h (como lo devuelve la proyección) */
const rect = (w, h) => [
  { kind: "visible", points: [[0, 0], [w, 0]] },
  { kind: "visible", points: [[w, 0], [w, h]] },
  { kind: "visible", points: [[w, h], [0, h]] },
  { kind: "visible", points: [[0, h], [0, 0]] },
];

test("seis vistas: desde dónde se mira y qué queda arriba en la hoja (eye × xdir)", () => {
  const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
  const want = {
    front: { eye: [0, -1, 0], up: [0, 0, 1] },
    back: { eye: [0, 1, 0], up: [0, 0, 1] },
    left: { eye: [-1, 0, 0], up: [0, 0, 1] },
    right: { eye: [1, 0, 0], up: [0, 0, 1] },
    // Superior: lo de atrás arriba; inferior: lo de adelante arriba
    top: { eye: [0, 0, 1], up: [0, 1, 0] },
    bottom: { eye: [0, 0, -1], up: [0, -1, 0] },
  };
  for (const [name, w] of Object.entries(want)) {
    const v = viewSpec(name);
    assert.deepEqual(v.eye, w.eye, name);
    assert.deepEqual(cross(v.eye, v.xdir).map((x) => x + 0), w.up, name);
  }
  // La misma vista en los dos diedros: cambia el lugar, no la vista
  assert.deepEqual(viewSpec("left", "first"), viewSpec("left", "third"));
});

test("primer diedro: superior abajo, inferior arriba, izquierda a la derecha; tercero al revés; posterior al final", () => {
  const box = { front: rect(40, 10), back: rect(40, 10), top: rect(40, 20), bottom: rect(40, 20), left: rect(20, 10), right: rect(20, 10) };
  const at = (pl, name) => pl.find((p) => p.name === name);
  const first = layout(box, SHEETS[2], "first", 1).placed;
  const third = layout(box, SHEETS[2], "third", 1).placed;
  const f = (n) => at(first, n);
  const t = (n) => at(third, n);
  assert.ok(f("top").y > f("front").y && f("bottom").y < f("front").y, "primer diedro: arriba y abajo");
  assert.ok(f("left").x > f("front").x && f("right").x < f("front").x, "primer diedro: los costados");
  assert.ok(t("top").y < t("front").y && t("bottom").y > t("front").y, "tercer diedro: arriba y abajo");
  assert.ok(t("left").x < t("front").x && t("right").x > t("front").x, "tercer diedro: los costados");
  for (const pl of [first, third]) {
    const xs = pl.map((p) => p.x);
    assert.equal(at(pl, "back").x, Math.max(...xs), "la posterior al final");
    // Alineadas: la fila del frente a la misma altura, la columna al mismo x
    for (const n of ["left", "right", "back"]) assert.equal(at(pl, n).y, at(pl, "front").y);
    for (const n of ["top", "bottom"]) assert.equal(at(pl, n).x, at(pl, "front").x);
  }
});

test("escala normalizada: la mayor con la que todo entra en la hoja", () => {
  const a4 = SHEETS[0];
  // Pieza de 40 × 20 × 10: a 2:1 entra en A4; una de 400 no, va a 1:5 o menos
  const small = { front: rect(40, 10), top: rect(40, 20), left: rect(20, 10) };
  assert.equal(scaleLabel(layout(small, a4, "first").scale), "2:1");
  const big = { front: rect(400, 100), top: rect(400, 200), left: rect(200, 100) };
  const s = layout(big, a4, "first").scale;
  assert.ok(s <= 1 / 5, scaleLabel(s));
  assert.equal(scaleLabel(1 / 20), "1:20");
});

test("primer diedro: la planta va debajo del frente; tercero: arriba", () => {
  const views = { front: rect(40, 10), top: rect(40, 20) };
  const first = layout(views, SHEETS[1], "first", 1).placed;
  const third = layout(views, SHEETS[1], "third", 1).placed;
  const y = (pl, name) => pl.find((p) => p.name === name).y;
  assert.ok(y(first, "top") > y(first, "front"), "primer diedro");
  assert.ok(y(third, "top") < y(third, "front"), "tercer diedro");
});

test("SVG: una polilínea por línea, ocultas solo si se piden, y cajetín", () => {
  const views = { front: [...rect(40, 10), { kind: "hidden", points: [[10, 0], [10, 10]] }] };
  const { placed, scale } = layout(views, SHEETS[0], "first");
  const info = { title: "Soporte <A>", date: "2026-10-07", scale: scaleLabel(scale), projection: "first", sheet: "A4" };
  const with_ = sheetSvg(placed, SHEETS[0], info, { hidden: true, smooth: false });
  const without = sheetSvg(placed, SHEETS[0], info, { hidden: false, smooth: false });
  assert.equal((with_.match(/<polyline/g) ?? []).length, 5);
  assert.equal((without.match(/<polyline/g) ?? []).length, 4);
  assert.ok(with_.includes('viewBox="0 0 297 210"'));
  assert.ok(with_.includes("Soporte &lt;A&gt;"), "el título se escapa");
  assert.ok(with_.includes("Primer diedro"));
  // A escala: el frente mide 40 × escala en la hoja
  const pts = [...with_.matchAll(/data-kind="visible" points="([^"]+)"/g)].flatMap((m) => m[1].split(" ").map((p) => p.split(",").map(Number)));
  const xs = pts.map((p) => p[0]);
  assert.ok(Math.abs(Math.max(...xs) - Math.min(...xs) - 40 * scale) < 1e-6);
  assert.deepEqual(bounds(views.front).max, [40, 10]);
});

test("DXF: capas, líneas por tramo y una vista sola a 1:1", () => {
  const views = { front: [...rect(40, 10), { kind: "hidden", points: [[10, 0], [10, 10]] }], top: rect(40, 20) };
  const { placed } = layout(views, SHEETS[0], "first");
  const dxf = sheetDxf(placed, SHEETS[0], { hidden: true, smooth: false });
  const lines = dxf.split("\n");
  assert.equal(lines.at(-2), "EOF");
  // 4 + 1 + 4 tramos de las vistas y 4 del recuadro
  assert.equal(lines.filter((l) => l === "LINE").length, 13);
  assert.ok(dxf.includes("OCULTA") && dxf.includes("DASHED"));
  // Solo el frente, 1:1: coordenadas de la pieza
  const one = sheetDxf(placed, SHEETS[0], { hidden: false, smooth: false, view: "front" });
  assert.equal(one.split("\n").filter((l) => l === "LINE").length, 4);
  const xs = [];
  const ls = one.split("\n");
  for (let i = 0; i < ls.length - 1; i++) if (ls[i] === "10" || ls[i] === "11") xs.push(Number(ls[i + 1]));
  assert.equal(Math.max(...xs) - Math.min(...xs), 40);
});

test("cotas generales: ancho y alto de cada vista, en mm de la pieza", () => {
  const views = { front: rect(40, 10), top: rect(40, 20), iso: rect(30, 30) };
  const { placed } = layout(views, SHEETS[0], "first");
  const dims = overallDimensions(placed);
  assert.deepEqual(dims.map((d) => d.value).sort((a, b) => a - b), [10, 20, 40, 40]);
  const svg = sheetSvg(placed, SHEETS[0], { title: "x", date: "", scale: "", projection: "first", sheet: "A4" }, { hidden: true, smooth: false, dimensions: true });
  assert.equal((svg.match(/data-dimension/g) ?? []).length, 4);
  assert.ok(svg.includes(">40<") && svg.includes(">20<"));
});

test("corte A-A: rayado en el frente, rótulo y línea en la planta", () => {
  const views = { front: rect(40, 10), top: rect(40, 20) };
  const hatch = { front: [[[0, 0], [40, 0], [40, 10]], [[0, 0], [40, 10], [0, 10]]] };
  const { placed } = layout(views, SHEETS[0], "first", undefined, hatch);
  const svg = sheetSvg(placed, SHEETS[0], { title: "x", date: "", scale: "", projection: "first", sheet: "A4" }, { hidden: true, smooth: false, section: { label: "A", at: 10 } });
  assert.equal((svg.match(/<polygon points/g) ?? []).length >= 2, true);
  assert.ok(svg.includes('data-hatch="front"') && svg.includes("url(#rayado)"));
  assert.ok(svg.includes("Corte A-A") && svg.includes("data-section-line"));
});

test("el SVG es XML válido (atributos con valor)", () => {
  const views = { front: rect(40, 10), top: rect(40, 20) };
  const { placed } = layout(views, SHEETS[0], "first");
  const svg = sheetSvg(placed, SHEETS[0], { title: "x", date: "", scale: "", projection: "first", sheet: "A4" }, { hidden: true, smooth: false, dimensions: true, section: { label: "A", at: 5 } });
  // Ningún atributo suelto: cada nombre=valor
  for (const tag of svg.match(/<[a-z][^>]*>/g)) {
    const attrs = tag.replace(/^<[a-z]+/, "").replace(/\/?>$/, "").replace(/[A-Za-z0-9:-]+="[^"]*"/g, "").trim();
    assert.equal(attrs, "", tag);
  }
});

test("cotas del usuario: largo, diámetro y distancia entre paralelas, y siguen al modelo", () => {
  const circle = { kind: "visible", points: Array.from({ length: 33 }, (_, i) => [20 + 4 * Math.cos((i / 32) * 2 * Math.PI), 5 + 4 * Math.sin((i / 32) * 2 * Math.PI)]) };
  const dims = [
    { view: "front", kind: "length", refs: [[20, 0]] },
    { view: "front", kind: "diameter", refs: [[24, 5]] },
    { view: "front", kind: "distance", refs: [[20, 0], [20, 10]] },
  ];
  let { placed } = layout({ front: [...rect(40, 10), circle] }, SHEETS[0], "first");
  assert.deepEqual(userDimensions(placed, dims).map((d) => [d.prefix ?? "", Math.round(d.value * 100) / 100]), [["", 40], ["Ø", 8], ["", 10]]);
  // El modelo cambia (más ancho y alto): las mismas cotas dan los valores nuevos
  ({ placed } = layout({ front: [...rect(50, 12), circle] }, SHEETS[0], "first"));
  assert.deepEqual(userDimensions(placed, dims).map((d) => Math.round(d.value * 100) / 100), [50, 8, 12]);
  // Una referencia que ya no encuentra línea se omite
  assert.equal(userDimensions(placed, [{ view: "front", kind: "length", refs: [[200, 200]] }]).length, 0);
  const svg = sheetSvg(placed, SHEETS[0], { title: "x", date: "", scale: "", projection: "first", sheet: "A4" }, { hidden: true, smooth: false, userDims: dims });
  assert.equal((svg.match(/data-user-dim/g) ?? []).length, 3);
  assert.ok(svg.includes(">Ø8<"));
});

import { asArc, angleBetween, clipToCircle, placeDetails, detailScale } from "../src/lib/drawing.ts";

/** Arco de radio r con centro c de a0 a a1 (grados) */
const arc = (c, r, a0, a1, k = 24) => ({
  kind: "visible",
  points: Array.from({ length: k + 1 }, (_, i) => {
    const t = ((a0 + ((a1 - a0) * i) / k) * Math.PI) / 180;
    return [c[0] + r * Math.cos(t), c[1] + r * Math.sin(t)];
  }),
});

test("arcos: centro y radio; las rectas y los círculos no son arcos", () => {
  const a = asArc(arc([5, 3], 4, 0, 90));
  assert.ok(a && Math.abs(a.r - 4) < 1e-9 && Math.abs(a.c[0] - 5) < 1e-9 && Math.abs(a.c[1] - 3) < 1e-9);
  assert.equal(asArc({ kind: "visible", points: [[0, 0], [1, 0], [2, 0], [3, 0]] }), null);
  assert.equal(asArc(arc([0, 0], 4, 0, 360, 36)), null);
});

test("ángulo entre dos rectas: hacia el lado de los clics", () => {
  const h = [[0, 0], [10, 0]];
  const d = [[0, 0], [10, 10]];
  const a = angleBetween(h, d, [8, 0], [5, 5]);
  assert.ok(Math.abs(a.degrees - 45) < 1e-9);
  assert.deepEqual(a.x, [0, 0]);
  // El suplemento si el clic va del otro lado del vértice
  const b = angleBetween([[-10, 0], [10, 0]], d, [-8, 0], [5, 5]);
  assert.ok(Math.abs(b.degrees - 135) < 1e-9);
  assert.equal(angleBetween(h, [[0, 2], [10, 2]], [1, 0], [1, 2]), null);
});

test("cotas de radio y de ángulo resueltas en la hoja", () => {
  const views = { front: [...rect(40, 10), arc([40, 10], 5, 90, 180)] };
  const { placed } = layout(views, SHEETS[0], "first", 1);
  const dims = userDimensions(placed, [
    { view: "front", kind: "radius", refs: [[40 - 5 * Math.SQRT1_2, 10 + 5 * Math.SQRT1_2]] },
    { view: "front", kind: "angle", refs: [[20, 0], [40, 5]] },
  ]);
  assert.equal(dims.length, 2);
  assert.equal(dims[0].kind, "radius");
  assert.ok(Math.abs(dims[0].value - 5) < 1e-9);
  assert.equal(dims[1].kind, "angle");
  assert.ok(Math.abs(dims[1].value - 90) < 1e-9);
  const svg = sheetSvg(placed, SHEETS[0], { title: "x", date: "2026-10-08", scale: "1:1", projection: "first", sheet: "A4" }, {
    hidden: true,
    smooth: false,
    userDims: [
      { view: "front", kind: "radius", refs: [[40 - 5 * Math.SQRT1_2, 10 + 5 * Math.SQRT1_2]] },
      { view: "front", kind: "angle", refs: [[20, 0], [40, 5]] },
    ],
  });
  assert.ok(svg.includes(">R5<"), "radio");
  assert.ok(svg.includes(">90°<"), "ángulo");
  const dxf = sheetDxf(placed, SHEETS[0], { hidden: true, smooth: false, userDims: [{ view: "front", kind: "angle", refs: [[20, 0], [40, 5]] }] });
  assert.ok(dxf.includes("\n90°\n"));
});

test("detalle: recorta al círculo, amplía y no pisa las vistas", () => {
  // Recorte: una recta que cruza el círculo de radio 1 queda de −1 a 1
  const parts = clipToCircle([[-5, 0], [5, 0]], [0, 0], 1);
  assert.equal(parts.length, 1);
  assert.deepEqual(parts[0].map((p) => p.map((x) => Math.round(x * 1e9) / 1e9)), [[-1, 0], [1, 0]]);
  assert.equal(clipToCircle([[-5, 3], [5, 3]], [0, 0], 1).length, 0);
  // Una polilínea que entra y sale dos veces: dos tramos
  assert.equal(clipToCircle([[-5, 0], [5, 0], [5, 0.5], [-5, 0.5]], [0, 0], 1).length, 2);
  assert.equal(detailScale(1), 2);
  assert.equal(detailScale(1 / 2), 1);
  assert.equal(detailScale(2), 5);
  const views = { front: rect(40, 10), top: rect(40, 20), left: rect(20, 10) };
  const { placed, scale } = layout(views, SHEETS[1], "first");
  const det = placeDetails(placed, [{ label: "B", view: "front", center: [40, 10], radius: 4 }], SHEETS[1]);
  assert.equal(det.length, 1);
  assert.equal(det[0].scale, detailScale(scale));
  // Solo lo que cae dentro del círculo: dos tramos de 4 mm en la esquina
  const len = det[0].lines.reduce((a, l) => a + Math.hypot(l.points[1][0] - l.points[0][0], l.points[1][1] - l.points[0][1]), 0);
  assert.ok(Math.abs(len - 8) < 1e-9, String(len));
  // No se superpone con ninguna vista
  const box = (v) => {
    const b = bounds(v.lines);
    return [v.x + b.min[0] * v.scale, v.y - b.max[1] * v.scale, v.x + b.max[0] * v.scale, v.y - b.min[1] * v.scale];
  };
  const d = box(det[0]);
  for (const v of placed) {
    const q = box(v);
    assert.ok(!(d[0] < q[2] && d[2] > q[0] && d[1] < q[3] && d[3] > q[1]), `pisa ${v.name}`);
  }
  const svg = sheetSvg([...placed, ...det], SHEETS[1], { title: "x", date: "2026-10-08", scale: "1:1", projection: "first", sheet: "A3" }, { hidden: true, smooth: false, dimensions: true });
  assert.ok(svg.includes('data-detail-mark="B"') && svg.includes(`Detalle B (${scaleLabel(det[0].scale)})`));
  // Las cotas generales no van en el detalle
  assert.equal(overallDimensions([...placed, ...det]).length, overallDimensions(placed).length);
});

test("ángulo: un clic un poco fuera de la recta no cambia el valor", () => {
  const a = angleBetween([[10, 0], [-10, 0]], [[-10, 0], [-5, 10]], [-4, 0.8], [-8.3, 4.1]);
  assert.ok(Math.abs(a.degrees - (Math.atan2(10, 5) * 180) / Math.PI) < 1e-9, String(a.degrees));
});

test("lo dibujado sobre la hoja: sin construcción ni capas ocultas, Y hacia arriba en el SVG y el DXF", () => {
  const pt = (id, x, y) => ({ id, x, y });
  const line = (id, a, b, extra = {}) => ({ id, geometry: { type: "line", start: a, end: b }, construction: false, ...extra });
  const sketch = {
    points: [pt(0, 0, 0), pt(1, 20, 30), pt(2, 60, 30), pt(3, 20, 50), pt(4, 60, 50), pt(5, 100, 100)],
    entities: [
      line(10, 1, 2),
      line(11, 3, 4, { construction: true }),
      line(12, 2, 4, { layer: 7 }),
      { id: 13, geometry: { type: "circle", center: 5, radius: 10 }, construction: false },
      { id: 14, geometry: { type: "point", point: 0 }, construction: false },
    ],
    constraints: [],
    layers: [{ id: 7, name: "Oculta", hidden: true }],
  };
  const lines = sheetLayerLines(sketch);
  assert.equal(lines.length, 2, "la línea y el círculo");
  assert.deepEqual(lines[0], [[20, 30], [60, 30]]);
  assert.deepEqual(sheetLayerLines(null), []);
  const a4 = SHEETS[0];
  const { placed } = layout({ front: rect(40, 10) }, a4, "first");
  const info = { title: "x", date: "2026-10-10", scale: "1:1", projection: "first", sheet: "A4" };
  const svg = sheetSvg(placed, a4, info, { hidden: true, smooth: false, layer: lines });
  // En el SVG la Y crece hacia abajo: y = alto de la hoja − y del sketch
  assert.ok(svg.includes(`<polyline points="20,${a4.height - 30} 60,${a4.height - 30}"/>`), "la línea dibujada en el SVG");
  assert.ok(!sheetSvg(placed, a4, info, { hidden: true, smooth: false }).includes("data-layer"));
  const dxf = sheetDxf(placed, a4, { hidden: true, smooth: false, layer: lines });
  const segments = dxf.split("\n").filter((l, i, all) => l === "DIBUJO" && all[i - 1] === "8" && all[i - 2] === "LINE").length;
  assert.equal(segments, 1 + 64, "la línea y el círculo en tramos");
  assert.ok(/LINE\n8\nDIBUJO\n10\n20\n20\n30\n/.test(dxf), "en mm de hoja con Y hacia arriba");
  // Una vista sola a 1:1 no lleva lo dibujado sobre la hoja
  assert.ok(!sheetDxf(placed, a4, { hidden: true, smooth: false, layer: lines, view: "front" }).includes("\nDIBUJO\n10"));
});

test("escala propia de una vista: ocupa lo suyo y lleva su rótulo debajo", () => {
  const views = { front: rect(40, 10), top: rect(40, 20), left: rect(20, 10) };
  const base = layout(views, SHEETS[1], "first", 1);
  const own = layout(views, SHEETS[1], "first", 1, undefined, { left: 2 });
  const L = (r) => r.placed.find((p) => p.name === "left");
  assert.equal(L(own).scale, 2);
  assert.equal(L(own).label, "Lateral izquierda (2:1)");
  assert.equal(L(base).label, undefined);
  assert.equal(own.placed.find((p) => p.name === "front").scale, 1);
  const info = { title: "x", date: "2026-10-10", scale: "1:1", projection: "first", sheet: "A3" };
  const svg = sheetSvg(own.placed, SHEETS[1], info, { hidden: true, smooth: false, dimensions: true });
  assert.ok(svg.includes('data-view-label="left"') && svg.includes("Lateral izquierda (2:1)"));
  // La escala automática de la hoja cuenta el tamaño de la vista a su escala
  const auto = layout({ front: rect(40, 10), left: rect(20, 10) }, SHEETS[0], "first", undefined, undefined, { left: 10 });
  assert.equal(auto.placed.find((p) => p.name === "left").scale, 10);
  assert.ok(auto.scale < layout({ front: rect(40, 10), left: rect(20, 10) }, SHEETS[0], "first").scale, "deja lugar a la ampliada");
});

test("vistas puestas a mano: van donde se dejaron, se arrastran alineadas y avisan si se salen o pisan", () => {
  const views = { front: rect(40, 10), top: rect(40, 20), left: rect(20, 10) };
  const auto = layout(views, SHEETS[1], "first", 1).placed;
  const moved = layout(views, SHEETS[1], "first", 1, undefined, undefined, { left: [300, 100] }).placed;
  const L = (pl) => pl.find((p) => p.name === "left");
  assert.deepEqual([L(moved).x, L(moved).y, L(moved).moved], [300, 100, true]);
  assert.equal(L(auto).moved, undefined);
  assert.deepEqual(moved.find((p) => p.name === "front"), auto.find((p) => p.name === "front"), "las otras siguen en su lugar");
  // Primer diedro: la superior va en la columna (vertical), la izquierda en la fila (horizontal)
  assert.equal(dragAxis("top", "first"), "y");
  assert.equal(dragAxis("left", "first"), "x");
  assert.equal(dragAxis("back", "third"), "x");
  assert.equal(dragAxis("front", "first"), "free");
  assert.deepEqual(placementProblems(auto, SHEETS[1]), [], "lo automático no avisa");
  const out = layout(views, SHEETS[1], "first", 1, undefined, undefined, { left: [500, 100] }).placed;
  assert.match(placementProblems(out, SHEETS[1], { left: "Lateral izquierda" })[0], /Lateral izquierda se sale de la hoja/);
  const front = auto.find((p) => p.name === "front");
  const onTop = layout(views, SHEETS[1], "first", 1, undefined, undefined, { left: [front.x + 5, front.y] }).placed;
  assert.ok(placementProblems(onTop, SHEETS[1]).some((m) => m.includes("pisa front")));
});

test("girar una vista: a la derecha se ve el costado izquierdo, hacia abajo se ve desde arriba, en el plano gira la hoja", () => {
  const front = viewSpec("front");
  const r = rotateViewDir(front, 90);
  assert.deepEqual(r.eye, viewSpec("left").eye, "la pieza gira a la derecha: queda adelante su costado izquierdo");
  assert.deepEqual(viewUp(r), [0, 0, 1]);
  const down = rotateViewDir(front, 0, -90);
  assert.deepEqual(down.eye, [0, 0, 1], "inclinada hacia atrás: se la ve desde arriba");
  assert.deepEqual(rotateViewDir(front, 0, 90).eye, [0, 0, -1]);
  const roll = rotateViewDir(front, 0, 0, 90);
  assert.deepEqual(roll.eye, front.eye);
  assert.deepEqual(viewUp(roll), [-1, 0, 0]);
  // Cuatro giros de 90° vuelven al comienzo; siempre ortonormal
  let q = viewSpec("iso");
  for (let i = 0; i < 4; i++) q = rotateViewDir(q, 90);
  for (let i = 0; i < 3; i++) assert.ok(Math.abs(q.eye[i] - viewSpec("iso").eye[i]) < 1e-9);
  const odd = rotateViewDir(viewSpec("iso"), 17, 33, 5);
  const dot = odd.eye[0] * odd.xdir[0] + odd.eye[1] * odd.xdir[1] + odd.eye[2] * odd.xdir[2];
  assert.ok(Math.abs(dot) < 1e-9 && Math.abs(Math.hypot(...odd.eye) - 1) < 1e-9);
});

test("la vista de la cámara del visor (Y arriba) pasada al CAD (Z arriba)", () => {
  // Cámara delante de la pieza (−Y del CAD = +Z del visor), mirando al origen, arriba +Y del visor
  const d = viewDirFromCamera({ position: [0, 0, 5], target: [0, 0, 0], up: [0, 1, 0] });
  assert.deepEqual(d.eye.map((x) => x + 0), [0, -1, 0]);
  assert.deepEqual(d.xdir.map((x) => x + 0), [1, 0, 0]);
  // Lo proyectado con esa dirección: x a la derecha, z arriba
  assert.deepEqual(projectPolylines([[[3, 7, 2]]], d)[0][0], [3, 2]);
});

test("vistas de más (nuevas y de cortes): en el lugar libre de la hoja, sin pisar nada; si no entran lo dicen", () => {
  const { placed, scale } = layout({ front: rect(40, 10), top: rect(40, 20) }, SHEETS[1], "first", 1);
  const r = placeExtraViews(placed, [{ name: "vista-1", title: "Vista 1", lines: rect(30, 30) }, { name: "corte-B", title: "Corte B-B", lines: rect(10, 10), scale: 2 }], scale, SHEETS[1]);
  assert.ok(r.fits);
  const box = (v) => {
    const b = bounds(v.lines);
    return [v.x + b.min[0] * v.scale, v.y - b.max[1] * v.scale, v.x + b.max[0] * v.scale, v.y - b.min[1] * v.scale];
  };
  const all = [...placed, ...r.placed].map(box);
  for (let i = 0; i < all.length; i++)
    for (let j = i + 1; j < all.length; j++) assert.ok(!(all[i][0] < all[j][2] && all[i][2] > all[j][0] && all[i][1] < all[j][3] && all[i][3] > all[j][1]), `se pisan ${i} y ${j}`);
  for (const b of r.placed.map(box)) assert.ok(b[0] >= 10 && b[2] <= SHEETS[1].width - 10 && b[1] >= 10 && b[3] <= SHEETS[1].height - 10, "dentro del recuadro");
  assert.deepEqual(r.placed.map((v) => v.label), ["Vista 1", "Corte B-B (2:1)"]);
  // Una enorme no entra: avisa
  assert.equal(placeExtraViews(placed, [{ name: "vista-2", title: "V", lines: rect(500, 500) }], scale, SHEETS[1]).fits, false);
  const moved = placeExtraViews(placed, [{ name: "vista-1", title: "Vista 1", lines: rect(30, 30) }], scale, SHEETS[1], { "vista-1": [10, 20] });
  assert.deepEqual([moved.placed[0].x, moved.placed[0].y, moved.placed[0].moved], [10, 20, true]);
});

test("cortes en la hoja: línea con flechas y letras, central del medio corte, rotura, rayados por estilo e imagen sombreada", () => {
  const views = { front: rect(40, 10), top: rect(40, 20) };
  const { placed } = layout(views, SHEETS[1], "first", 1);
  const top = placed.find((p) => p.name === "top");
  const front = placed.find((p) => p.name === "front");
  const hatched = [
    { ...front, hatch: [[[0, 0], [10, 0], [0, 10]]], hatchStyle: { angle: 135, spacing: 3, cross: true } },
    { ...top, hatch: [[[0, 0], [10, 0], [0, 10]]], image: { href: "data:image/png;base64,AAAA", min: [0, 0], max: [40, 20], lines: false } },
  ];
  const info = { title: "x", date: "2026-10-10", scale: "1:1", projection: "first", sheet: "A3" };
  const svg = sheetSvg(hatched, SHEETS[1], info, {
    hidden: true,
    smooth: false,
    marks: { lines: [{ view: "top", points: [[-5, 10], [45, 10]], arrow: [0, 1], label: "B" }], centers: [{ view: "front", a: [20, -2], b: [20, 12] }], breaks: [{ view: "front", index: 0, points: [[1, 1], [5, 1], [5, 5]] }] },
  });
  assert.ok(svg.includes('data-cut-line="B"') && (svg.match(/>B<\/text>/g) ?? []).length === 2, "dos letras B, una por flecha");
  assert.ok(svg.includes('data-center-line="front"') && svg.includes('data-break-line="front" data-break-index="0"'));
  assert.ok(svg.includes('patternTransform="rotate(135)"') && svg.includes('fill="url(#rayado-1)"'), "el rayado propio del frente");
  // Sombreada sin líneas: la imagen, sin sus polilíneas ni su rayado
  assert.ok(svg.includes('data-view-image="top"'));
  assert.equal((svg.match(/<g data-view="top"[^>]*>\s*<\/g>/g) ?? []).length, 1, "las líneas de la superior no van");
  assert.ok(!svg.includes('data-hatch="top"'));
});
