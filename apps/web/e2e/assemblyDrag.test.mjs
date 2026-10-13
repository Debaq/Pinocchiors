// Arrastrar piezas del ensamble (lib/assemblyDrag.ts), sin navegador:
//   node --test apps/web/e2e/assemblyDrag.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { angleOf, dragPlane, jointOf, mateAxis, nextAngle, rotationMatrix, turned } from "../src/lib/assemblyDrag.ts";

const conn = (instance, origin = [0, 0, 0]) => ({ instance, origin, z: [0, 0, 1], x: [1, 0, 0] });
const mate = (id, a, b, kind = "revolute") => ({ id, name: `m${id}`, kind, a: conn(a), b: conn(b), flip: false });
const asm = (mates, fixed = [1]) => ({ instances: [1, 2, 3].map((id) => ({ id, fixed: fixed.includes(id) })), mates, relations: [], next_id: 9 });

test("la matriz de giro gira X a Y con 90° alrededor de Z", () => {
  const m = rotationMatrix([0, 0, Math.PI / 2]);
  assert.ok(Math.abs(m[1][0] - 1) < 1e-12 && Math.abs(m[0][0]) < 1e-12);
});

test("el eje de la unión sigue a la pose de la instancia a", () => {
  const axis = mateAxis(mate(1, 1, 2), [[1, [10, 0, 0], [0, Math.PI / 2, 0]], [2, [0, 0, 0], [0, 0, 0]]]);
  // Z girado 90° alrededor de Y queda en +X
  assert.ok(Math.abs(axis.z[0] - 1) < 1e-12 && Math.abs(axis.origin[0] - 10) < 1e-12);
});

test("la unión de la pieza agarrada y el sentido del ángulo", () => {
  const a = asm([mate(1, 1, 2), mate(2, 2, 3)]);
  // La 2 es `b` de la primera y `a` de la segunda: gana la que la une con la fija
  assert.deepEqual({ id: jointOf(a, 2).mate.id, sign: jointOf(a, 2).sign }, { id: 1, sign: 1 });
  // La 1 es `a`: arrastrarla gira el ángulo al revés
  assert.equal(jointOf(a, 1).sign, -1);
  assert.equal(jointOf(asm([mate(1, 1, 2, "slider")]), 2), undefined);
});

test("el giro del puntero cruza los 180° por el camino corto", () => {
  assert.ok(Math.abs(turned([1, 0], [0, 1]) - 90) < 1e-9);
  assert.ok(Math.abs(turned([-1, -0.01], [-1, 0.01]) - -1.145) < 0.01);
  assert.ok(Math.abs(angleOf([0, -1]) + 90) < 1e-9);
  assert.equal(nextAngle(30, 10, -1), 20);
  assert.deepEqual(dragPlane({ origin: [1, 2, 3], z: [0, 0, 1], x: [1, 0, 0] }).normal, [0, 0, 1]);
});

test("el ángulo medido es el giro de b respecto de a", async () => {
  const { measuredAngle } = await import("../src/lib/assemblyDrag.ts");
  const m = mate(1, 1, 2);
  const a = measuredAngle(m, [[1, [0, 0, 0], [0, 0, 0]], [2, [5, 0, 0], [0, 0, Math.PI / 6]]]);
  assert.ok(Math.abs(a - 30) < 1e-9, String(a));
});
