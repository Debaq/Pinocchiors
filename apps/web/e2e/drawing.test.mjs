// Pruebas del armado de planos (lib/drawing.ts), sin navegador:
//   node --test apps/web/e2e/drawing.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { layout, sheetSvg, sheetDxf, overallDimensions, userDimensions, SHEETS, viewSpec, scaleLabel, bounds } from "../src/lib/drawing.ts";

/** Rectángulo de w × h (como lo devuelve la proyección) */
const rect = (w, h) => [
  { kind: "visible", points: [[0, 0], [w, 0]] },
  { kind: "visible", points: [[w, 0], [w, h]] },
  { kind: "visible", points: [[w, h], [0, h]] },
  { kind: "visible", points: [[0, h], [0, 0]] },
];

test("vistas: frente desde −Y, planta desde +Z y lateral según el diedro", () => {
  assert.deepEqual(viewSpec("front", "first"), { eye: [0, -1, 0], xdir: [1, 0, 0] });
  assert.deepEqual(viewSpec("top", "first").eye, [0, 0, 1]);
  assert.deepEqual(viewSpec("side", "first").eye, [-1, 0, 0]);
  assert.deepEqual(viewSpec("side", "third").eye, [1, 0, 0]);
});

test("escala normalizada: la mayor con la que todo entra en la hoja", () => {
  const a4 = SHEETS[0];
  // Pieza de 40 × 20 × 10: a 2:1 entra en A4; una de 400 no, va a 1:5 o menos
  const small = { front: rect(40, 10), top: rect(40, 20), side: rect(20, 10) };
  assert.equal(scaleLabel(layout(small, a4, "first").scale), "2:1");
  const big = { front: rect(400, 100), top: rect(400, 200), side: rect(200, 100) };
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
  const views = { front: rect(40, 10), top: rect(40, 20), side: rect(20, 10) };
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
