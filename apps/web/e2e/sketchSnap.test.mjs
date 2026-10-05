// Pruebas de las inferencias del sketch (lib/sketchSnap.ts), sin navegador:
//   node apps/web/e2e/sketchSnap.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { infer, solidRefs } from "../src/lib/sketchSnap.ts";

/** Sketch con origen, una línea (0,0)-(10,0) que no sale del origen, un círculo y un arco */
function sample() {
  return {
    origin: 0,
    points: [
      { id: 0, x: 0, y: 0 },
      { id: 1, x: 20, y: 0 },
      { id: 2, x: 30, y: 0 },
      { id: 3, x: 50, y: 50 }, // centro del círculo r = 5
      { id: 4, x: -20, y: -20 }, // centro del arco r = 4, de 0° a 90°
      { id: 5, x: -16, y: -20 },
      { id: 6, x: -20, y: -16 },
    ],
    entities: [
      { id: 10, geometry: { type: "line", start: 1, end: 2 } },
      { id: 11, geometry: { type: "circle", center: 3, radius: 5 } },
      { id: 12, geometry: { type: "arc", center: 4, start: 5, end: 6 } },
    ],
    constraints: [],
  };
}

const near = (a, b) => assert.ok(Math.hypot(a[0] - b[0], a[1] - b[1]) < 1e-9, `${a} ≠ ${b}`);

test("origen, punto y centro", () => {
  const s = sample();
  assert.deepEqual(infer(s, [0.3, -0.2], 1), { p: [0, 0], kind: "origin", id: 0 });
  assert.equal(infer(s, [20.5, 0.5], 1).kind, "point");
  const c = infer(s, [50.4, 49.8], 1);
  assert.equal(c.kind, "center");
  assert.equal(c.id, 3);
});

test("punto medio de una línea", () => {
  const r = infer(sample(), [25.3, 0.6], 1);
  assert.equal(r.kind, "midpoint");
  assert.equal(r.entity, 10);
  near(r.p, [25, 0]);
});

test("cuadrantes del círculo y solo los del arco dentro de su barrido", () => {
  const s = sample();
  const q = infer(s, [50.2, 55.4], 1);
  assert.equal(q.kind, "quadrant");
  assert.equal(q.quadrant, 1);
  near(q.p, [50, 55]);
  assert.equal(infer(s, [44.6, 50.3], 1).quadrant, 2);
  // El arco va de 0° a 90°: su cuadrante de 180° no existe
  const a = infer(s, [-24.1, -20.2], 1);
  assert.notEqual(a.kind, "quadrant");
});

test("sobre una curva y libre", () => {
  const s = sample();
  const l = infer(s, [22, 0.4], 1);
  assert.equal(l.kind, "on_line");
  near(l.p, [22, 0]);
  const c = infer(s, [50 + 5 * Math.cos(0.6) + 0.3, 50 + 5 * Math.sin(0.6)], 1);
  assert.equal(c.kind, "on_circle");
  assert.equal(c.entity, 11);
  assert.deepEqual(infer(s, [100, 100], 1), { p: [100, 100], kind: "free" });
  // Fuera del segmento no hay "sobre la línea": queda alineado con los puntos a y = 0
  const a = infer(s, [35, 0.2], 1);
  assert.equal(a.kind, "aligned");
  near(a.p, [35, 0]);
});

test("prioridad: el punto gana al medio aunque el medio esté más cerca", () => {
  const s = sample();
  // Línea corta (20,0)-(21,0): medio en 20,5; con tolerancia 1 llegan los dos
  s.points[2].x = 21;
  assert.equal(infer(s, [20.45, 0], 1).kind, "point");
  // A igual prioridad gana el más cercano
  assert.equal(infer(s, [20.9, 0], 1).id, 2);
});

test("excluir el punto que se arrastra", () => {
  assert.equal(infer(sample(), [0.1, 3], 1, { exclude: [0] }).kind, "free");
});

/** Dos líneas en X, un círculo y un arco de 0° a 90° */
function crossingSample() {
  return {
    points: [
      { id: 0, x: 0, y: 0 },
      { id: 1, x: 10, y: 10 },
      { id: 2, x: 0, y: 8 },
      { id: 3, x: 12, y: 2 },
      { id: 4, x: 20, y: 0 }, // centro círculo r = 5
      { id: 5, x: 40, y: 0 }, // centro arco r = 5 (0° a 90°)
      { id: 6, x: 45, y: 0 },
      { id: 7, x: 40, y: 5 },
    ],
    entities: [
      { id: 10, geometry: { type: "line", start: 0, end: 1 } },
      { id: 11, geometry: { type: "line", start: 2, end: 3 } },
      { id: 12, geometry: { type: "circle", center: 4, radius: 5 } },
      { id: 13, geometry: { type: "line", start: 4, end: 5 } }, // cruza el círculo en x = 25
      { id: 14, geometry: { type: "arc", center: 5, start: 6, end: 7 } },
      { id: 15, geometry: { type: "line", start: 6, end: 7 } }, // cuerda del arco: se tocan en sus extremos
    ],
    constraints: [],
  };
}

test("intersección línea-línea", () => {
  // y = x con y = 8 − x/2 → x = 16/3
  const r = infer(crossingSample(), [5.5, 5.2], 1);
  assert.equal(r.kind, "intersection");
  assert.deepEqual([r.entity, r.other].sort(), [10, 11]);
  near(r.p, [16 / 3, 16 / 3]);
});

test("intersección línea-círculo y fuera del barrido del arco", () => {
  const s = crossingSample();
  const r = infer(s, [25.3, 0.2], 1);
  assert.equal(r.kind, "intersection");
  near(r.p, [25, 0]);
  // (35, 0) sería el cruce de la línea 13 con el arco como círculo completo
  // (y su cuadrante de 180°), pero el arco va de 0° a 90°: queda "sobre la línea"
  assert.equal(infer(s, [35, 0.1], 1).kind, "on_line");
});

test("intersección círculo-círculo, arco recortado", () => {
  const s = {
    points: [
      { id: 0, x: 0, y: 0 },
      { id: 1, x: 6, y: 0 },
      { id: 2, x: 11, y: 0 },
      { id: 3, x: 6, y: 5 },
    ],
    entities: [
      { id: 10, geometry: { type: "circle", center: 0, radius: 5 } },
      // Arco de centro (6,0) r = 5 de 0° a 90°: no llega a los cruces con el círculo (x = 3)
      { id: 11, geometry: { type: "arc", center: 1, start: 2, end: 3 } },
    ],
    constraints: [],
  };
  assert.notEqual(infer(s, [3, 4], 1).kind, "intersection");
  // Como círculo completo sí se cruzan en (3, ±4)
  s.entities[1] = { id: 11, geometry: { type: "circle", center: 1, radius: 5 } };
  const r = infer(s, [3.2, 3.8], 1);
  assert.equal(r.kind, "intersection");
  near(r.p, [3, 4]);
});

test("extremos compartidos ganan a la intersección", () => {
  const r = infer(crossingSample(), [45.2, 0.1], 1);
  assert.equal(r.kind, "point");
  assert.equal(r.id, 6);
});

test("alineado en horizontal y vertical a la vez", () => {
  const s = sample();
  // x = 20 (punto 1) e y = 50 (centro del círculo)
  const r = infer(s, [20.4, 49.5], 1);
  assert.equal(r.kind, "aligned");
  assert.deepEqual(r.align, { h: 3, v: 1 });
  near(r.p, [20, 50]);
  assert.equal(r.guides.length, 2);
  // noAlign: sin esos puntos solo queda lo que haya
  assert.equal(infer(s, [20.4, 49.5], 1, { noAlign: [1, 3] }).kind, "free");
});

test("dibujando desde un punto: paralela, perpendicular y tangente", () => {
  const s = {
    points: [
      { id: 0, x: 0, y: 0 },
      { id: 1, x: 10, y: 5 },
      { id: 2, x: 30, y: 0 }, // desde acá se dibuja
      { id: 3, x: 50, y: 0 }, // centro del arco
      { id: 4, x: 60, y: 0 },
      { id: 5, x: 50, y: 10 }, // fin del arco (90°)
    ],
    entities: [
      { id: 10, geometry: { type: "line", start: 0, end: 1 } },
      { id: 11, geometry: { type: "arc", center: 3, start: 4, end: 5 } },
    ],
    constraints: [],
  };
  // Paralela a (0,0)-(10,5) desde (30,0): (30,0) + t (2,1)/√5
  const par = infer(s, [40.3, 4.8], 1, { from: 2 });
  assert.equal(par.kind, "parallel");
  assert.equal(par.direction.entity, 10);
  near(par.p, [30 + 2 * ((10.3 * 2 + 4.8) / 5), (10.3 * 2 + 4.8) / 5]);
  // Perpendicular: dirección (−1, 2)
  const per = infer(s, [25.2, 10.1], 1, { from: 2 });
  assert.equal(per.kind, "perpendicular");
  // Tangente al salir del fin del arco (50,10): dirección horizontal → la da el eje, no la tangente
  assert.notEqual(infer(s, [40, 10.2], 1, { from: 5 }).kind, "tangent");
  // Arco que termina a 45°: la tangente sale a 135°
  const r = 10 / Math.SQRT2;
  s.points[5] = { id: 5, x: 50 + r, y: r };
  const tan = infer(s, [50 + r - 5, r + 5.3], 1, { from: 5 });
  assert.equal(tan.kind, "tangent");
  assert.equal(tan.direction.entity, 11);
  // Sin `from` no hay direcciones
  assert.notEqual(infer(s, [40.3, 4.8], 1).kind, "parallel");
});

test("dibujando desde un punto: horizontal y vertical, y combinadas con alinearse", () => {
  const s = {
    points: [
      { id: 0, x: 0, y: 0 }, // desde acá
      { id: 1, x: 20, y: 30 },
    ],
    entities: [],
    constraints: [],
  };
  const h = infer(s, [12, 0.6], 1, { from: 0 });
  assert.equal(h.kind, "horizontal");
  near(h.p, [12, 0]);
  const v = infer(s, [-0.4, 9], 1, { from: 0 });
  assert.equal(v.kind, "vertical");
  near(v.p, [0, 9]);
  // Horizontal y en la vertical del punto 1
  const hv = infer(s, [20.5, 0.4], 1, { from: 0 });
  assert.equal(hv.axis, "horizontal");
  assert.deepEqual(hv.align, { v: 1 });
  near(hv.p, [20, 0]);
  assert.equal(hv.guides.length, 2);
  // Lejos de los dos ejes: nada
  assert.equal(infer(s, [10, 7], 1, { from: 0 }).kind, "free");
});

/** Cara superior (z = 10) de una caja 40×20 con un agujero r = 5 en (10, 10) */
function boxTop() {
  const circle = Array.from({ length: 33 }, (_, i) => {
    const t = (i / 32) * 2 * Math.PI;
    return [10 + 5 * Math.cos(t), 10 + 5 * Math.sin(t), 10];
  });
  return [
    [[0, 0, 10], [40, 0, 10]],
    [[40, 0, 10], [40, 20, 10]],
    [[40, 20, 10], [0, 20, 10]],
    [[0, 20, 10], [0, 0, 10]],
    [[0, 0, 0], [0, 0, 10]], // vertical: fuera del plano
    circle,
  ];
}
const top = { origin: [0, 0, 10], normal: [0, 0, 1], x_dir: [1, 0, 0] };
const empty = () => ({ origin: 0, points: [{ id: 0, x: -100, y: -100 }], entities: [], constraints: [] });

test("aristas del sólido: vértices, medios, centros y aristas en el plano", () => {
  const r = solidRefs(boxTop(), top);
  assert.equal(r.vertices.length, 4); // la vertical se proyecta sobre (0, 0)
  assert.equal(r.midpoints.length, 4); // la vertical proyectada mide cero
  assert.equal(r.centers.length, 1);
  near(r.centers[0], [10, 10]);
  assert.equal(r.edges.length, 5); // cuatro lados y el círculo; la vertical no
});

test("anclajes al sólido al dibujar sobre una cara", () => {
  const solid = solidRefs(boxTop(), top);
  const s = empty();
  const v = infer(s, [40.3, 19.8], 1, { solid });
  assert.equal(v.kind, "solid_vertex");
  near(v.p, [40, 20]);
  const c = infer(s, [10.4, 9.7], 1, { solid });
  assert.equal(c.kind, "solid_center");
  near(c.p, [10, 10]);
  const m = infer(s, [20.2, 0.4], 1, { solid });
  assert.equal(m.kind, "solid_midpoint");
  near(m.p, [20, 0]);
  const e = infer(s, [30, 0.5], 1, { solid });
  assert.equal(e.kind, "on_edge");
  near(e.p, [30, 0]);
  // Un punto del sketch encima le gana al vértice del sólido
  s.points.push({ id: 1, x: 40, y: 20 });
  assert.equal(infer(s, [40.3, 19.8], 1, { solid }).kind, "point");
});

test("sketch en otro plano: vértices y centros proyectados, sin aristas", () => {
  const base = { origin: [0, 0, 0], normal: [0, 0, 1], x_dir: [1, 0, 0] };
  const r = solidRefs(boxTop(), base);
  assert.equal(r.edges.length, 0);
  assert.equal(r.centers.length, 1);
  assert.equal(infer(empty(), [39.6, 0.2], 1, { solid: r }).kind, "solid_vertex");
});
