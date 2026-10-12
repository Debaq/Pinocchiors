// Ángulos del sketch: lo que se dibuja es lo que mide el solver, el ángulo
// nuevo toma el rincón visible y anda con arcos y elipses, sin navegador:
//   node --test apps/web/e2e/sketchAngle.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { addEntity, addPoint } from "../src/lib/cad.ts";
import { angleBetweenEntities, angleView, hasAngleDirection, measureConstraint } from "../src/lib/sketchTransform.ts";

const near = (a, b, msg, eps = 1e-6) => assert.ok(Math.abs(a - b) < eps, `${msg}: ${a} ≠ ${b}`);
const blank = () => ({ points: [], entities: [], constraints: [] });
const line = (s, p, q) => addEntity(s, { type: "line", start: addPoint(s, p), end: addPoint(s, q) });
const deg = (u) => (Math.atan2(u[1], u[0]) * 180) / Math.PI;

test("el arco dibujado barre lo que mide el ángulo", () => {
  const s = blank();
  // Líneas dibujadas "al revés" (de afuera hacia el vértice): antes medía 150 y dibujaba 30
  const a = line(s, [10, 0], [0, 0]);
  const b = line(s, [10 * Math.cos(Math.PI / 6), 10 * Math.sin(Math.PI / 6)], [0, 0]);
  const c = angleBetweenEntities(s, a, b);
  near(Math.abs(c.degrees), 30, "rincón visible", 1e-3);
  near(c.degrees, 30, "de la referencia a la otra, antihorario", 1e-3);
  const v = angleView(s, c);
  near(v.degrees, measureConstraint(s, c), "misma cuenta");
  // u1 hacia la referencia, u2 = u1 girado lo que mide
  near(deg(v.u1), 0, "hacia la referencia");
  near(deg(v.u2), 30, "hacia la que gira", 1e-3);
  near(v.x[0], 0, "vértice");
});

test("con signo: antihorario positivo, horario negativo", () => {
  const s = blank();
  const a = line(s, [0, 0], [10, 0]);
  const up = line(s, [0, 0], [0, 10]);
  const down = line(s, [0, 0], [0, -10]);
  near(angleBetweenEntities(s, a, up).degrees, 90, "antihorario");
  near(angleBetweenEntities(s, a, down).degrees, -90, "horario");
  // Del otro lado de la referencia: el mismo dibujo con el signo cambiado
  near(angleBetweenEntities(s, up, a).degrees, -90, "referencia cambiada");
});

test("el suplementario dibuja el otro rincón", () => {
  const s = blank();
  const a = line(s, [0, 0], [10, 0]);
  const b = line(s, [0, 0], [5, 5]);
  const c = { ...angleBetweenEntities(s, a, b), supplementary: true };
  const v = angleView(s, c);
  near(Math.abs(v.degrees), 135, "180 − 45");
  near(Math.abs(deg(v.u2)), 135, "hacia la prolongación");
});

test("línea y arco que se tocan: la tangente en ese extremo", () => {
  const s = blank();
  const o = addPoint(s, [0, 0]);
  const l = addEntity(s, { type: "line", start: addPoint(s, [-10, 0]), end: o });
  // Antihorario desde el origen (centro arriba): sale hacia +x, tangente a la línea
  const arc = addEntity(s, { type: "arc", center: addPoint(s, [0, 5]), start: o, end: addPoint(s, [5, 5]) });
  assert.ok(hasAngleDirection(s, arc));
  const c = angleBetweenEntities(s, l, arc);
  assert.equal(c.b_end, "start");
  near(Math.abs(c.degrees), 180, "tangentes");
  near(measureConstraint(s, c), angleView(s, c).degrees, "misma cuenta");
});

test("elipse: su eje mayor; un círculo no tiene dirección", () => {
  const s = blank();
  const a = line(s, [0, 0], [10, 0]);
  const e = addEntity(s, { type: "ellipse", center: addPoint(s, [3, 3]), major: addPoint(s, [3 + 5 * Math.cos(Math.PI / 6), 3 + 5 * Math.sin(Math.PI / 6)]), minor: addPoint(s, [2, 5]) });
  near(angleBetweenEntities(s, a, e).degrees, 30, "eje mayor", 1e-3);
  const circle = addEntity(s, { type: "circle", center: addPoint(s, [0, 0]), radius: 2 });
  assert.equal(hasAngleDirection(s, circle), false);
  assert.equal(angleBetweenEntities(s, a, circle), undefined);
});
