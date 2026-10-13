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

test("una pieza con varias uniones ofrece cada forma de moverla y se elige por clave", async () => {
  const { dragJointOf, jointKey, jointsOf } = await import("../src/lib/assemblyDrag.ts");
  // La 2 gira en la 1 (con la fija), desliza en la 2 y gira otra vez en la 3
  const a = asm([mate(1, 1, 2), mate(2, 2, 3, "slider"), mate(3, 2, 3)]);
  const all = jointsOf(a, 2).map(jointKey);
  assert.deepEqual(all, ["1:turn", "3:turn", "2:slide"]);
  // Sin preferencia, la de siempre (la que la sostiene contra la fija)
  assert.equal(jointKey(dragJointOf(a, 2)), "1:turn");
  // Con una clave válida, esa; con una que ya no existe, vuelve a la primera
  assert.equal(jointKey(dragJointOf(a, 2, "2:slide")), "2:slide");
  assert.equal(jointKey(dragJointOf(a, 2, "9:turn")), "1:turn");
  // Una cilíndrica se puede girar o deslizar
  const c = asm([mate(1, 1, 2, "cylindrical")]);
  assert.deepEqual(jointsOf(c, 2).map(jointKey), ["1:turn", "1:slide"]);
  // La pieza `a` cuenta el sentido al revés
  assert.equal(dragJointOf(asm([mate(1, 1, 2, "slider")]), 1).sign, -1);
  assert.equal(dragJointOf(asm([mate(1, 1, 2, "fastened")]), 2), undefined);
});

test("la distancia medida y la nueva distancia al deslizar", async () => {
  const { measuredDistance, nextDistance } = await import("../src/lib/assemblyDrag.ts");
  const m = mate(1, 1, 2, "slider");
  // b está 7 mm arriba de a, a lo largo del Z de a (a girado 90° alrededor de X: su Z mira a -Y... aquí sin giro)
  const d = measuredDistance(m, [[1, [0, 0, 0], [0, 0, 0]], [2, [3, 4, 7], [0, 0, 0.5]]]);
  assert.ok(Math.abs(d - 7) < 1e-9, String(d));
  assert.equal(nextDistance(7, 2.5, 1), 9.5);
  assert.equal(nextDistance(7, 2.5, -1), 4.5);
});
