// Pruebas de los cortes del plano (lib/drawingCuts.ts), sin navegador:
//   node --test apps/web/e2e/drawingCuts.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { viewSpec, viewUp } from "../src/lib/drawing.ts";
import { breakPrism, halfPrism, reachOf, sectionArrow, sectionPrism, sectionViewDir, viewToWorld } from "../src/lib/drawingCuts.ts";

const box = { min: [-20, -10, -5], max: [20, 10, 5] };
/** ¿Está `p` dentro del prisma? (polígono convexo o no, en su plano, y entre las tapas) */
function inside(prism, p) {
  const d = prism.dir;
  const L = Math.hypot(...d);
  const n = d.map((x) => x / L);
  const o = prism.points[0];
  const t = (p[0] - o[0]) * n[0] + (p[1] - o[1]) * n[1] + (p[2] - o[2]) * n[2];
  if (t < 0 || t > L) return false;
  // Ejes del plano del polígono
  const a = [1, 0, 0].every((_, i) => true) && Math.abs(n[0]) < 0.9 ? [1, 0, 0] : [0, 1, 0];
  const u = [a[1] * n[2] - a[2] * n[1], a[2] * n[0] - a[0] * n[2], a[0] * n[1] - a[1] * n[0]];
  const ul = Math.hypot(...u);
  const U = u.map((x) => x / ul);
  const V = [n[1] * U[2] - n[2] * U[1], n[2] * U[0] - n[0] * U[2], n[0] * U[1] - n[1] * U[0]];
  const pr = (q) => [(q[0] - o[0]) * U[0] + (q[1] - o[1]) * U[1] + (q[2] - o[2]) * U[2], (q[0] - o[0]) * V[0] + (q[1] - o[1]) * V[1] + (q[2] - o[2]) * V[2]];
  const poly = prism.points.map(pr);
  const [x, y] = pr(p);
  let c = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const [xi, yi] = poly[i];
    const [xj, yj] = poly[j];
    if (yi > y !== yj > y && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) c = !c;
  }
  return c;
}

test("corte por una línea horizontal en la superior: se quita lo de adelante y se mira como el frente", () => {
  const top = viewSpec("top");
  const def = { label: "B", view: "top", points: [[-30, 2], [30, 2]] };
  assert.deepEqual(sectionArrow(def), [0, 1], "las flechas miran hacia atrás (arriba en la superior)");
  const prism = sectionPrism(def, top, reachOf(box));
  assert.ok(inside(prism, [0, -5, 0]), "lo de adelante (y < 2) se va");
  assert.ok(!inside(prism, [0, 5, 0]), "lo de atrás queda");
  assert.ok(inside(prism, [15, -9, 4]) && inside(prism, [-15, -9, -4]), "en toda la altura");
  const v = sectionViewDir(def, top);
  assert.deepEqual(v.eye, [0, -1, 0]);
  assert.deepEqual(viewUp(v).map((x) => x + 0), [0, 0, 1], "arriba, la altura de la pieza");
  // Al revés: se quita lo de atrás y se mira desde atrás
  const flip = { ...def, flip: true };
  assert.ok(inside(sectionPrism(flip, top, reachOf(box)), [0, 5, 0]));
  assert.deepEqual(sectionViewDir(flip, top).eye, [0, 1, 0]);
});

test("corte por una línea vertical en el frente: se mira como la lateral, arriba sigue arriba", () => {
  const front = viewSpec("front");
  const def = { label: "C", view: "front", points: [[5, 10], [5, -10]] };
  const v = sectionViewDir(def, front);
  // Recorrido hacia abajo: la izquierda es +x de la hoja (la derecha de la pieza)
  assert.deepEqual(sectionArrow(def), [1, 0]);
  assert.deepEqual(v.eye, [-1, 0, 0]);
  assert.deepEqual(viewUp(v).map((x) => x + 0), [0, 0, 1]);
  const prism = sectionPrism(def, front, reachOf(box));
  assert.ok(inside(prism, [0, 0, 0]) && !inside(prism, [10, 0, 0]));
});

test("corte escalonado: quita lo de adelante de cada escalón", () => {
  const top = viewSpec("top");
  const def = { label: "D", view: "top", points: [[-30, -4], [0, -4], [0, 4], [30, 4]] };
  const prism = sectionPrism(def, top, reachOf(box));
  assert.ok(inside(prism, [-10, -6, 0]) && !inside(prism, [-10, -2, 0]), "primer escalón en y = −4");
  assert.ok(inside(prism, [10, 2, 0]) && !inside(prism, [10, 6, 0]), "segundo escalón en y = 4");
});

test("medio corte: se quita el cuarto hacia quien mira, del lado elegido", () => {
  const front = viewSpec("front");
  const right = halfPrism(front, box, "right");
  assert.ok(inside(right, [10, -5, 0]), "adelante a la derecha");
  assert.ok(!inside(right, [-10, -5, 0]) && !inside(right, [10, 5, 0]), "ni a la izquierda ni atrás");
  const left = halfPrism(front, box, "left");
  assert.ok(inside(left, [-10, -5, 0]) && !inside(left, [10, -5, 0]));
});

test("rotura: dentro del contorno, desde adelante hasta la profundidad", () => {
  const front = viewSpec("front");
  const prism = breakPrism(front, { points: [[0, -3], [10, -3], [10, 3], [0, 3]], depth: 4 }, box);
  assert.ok(inside(prism, [5, -9, 0]), "cerca del frente (y = −10)");
  assert.ok(!inside(prism, [5, -5, 0]), "más hondo que 4 mm no");
  assert.ok(!inside(prism, [-5, -9, 0]), "fuera del contorno no");
  assert.deepEqual(viewToWorld(front, [3, 2], 1), [3, -1, 2]);
});
