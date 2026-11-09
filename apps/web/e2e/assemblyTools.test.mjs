// Ensamble (lib/assemblyTools.ts y pruneAssembly de lib/cad.ts), sin navegador:
//   node --test apps/web/e2e/assemblyTools.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { pruneAssembly } from "../src/lib/cad.ts";
import { applyPose, compose, explodeOffsets, inverse, moveAssemblyMesh, poseOf, rotateInstance, rotationVector } from "../src/lib/assemblyTools.ts";

const close = (a, b, eps = 1e-9) => a.forEach((x, i) => assert.ok(Math.abs(x - b[i]) < eps, `${a} ≠ ${b}`));

test("poses: componer, invertir y vector de giro", () => {
  const a = poseOf([1, 2, 3], [0, 0, Math.PI / 2]);
  close(applyPose(a, [1, 0, 0]), [1, 3, 3]);
  const id = compose(a, inverse(a));
  close(applyPose(id, [4, 5, 6]), [4, 5, 6]);
  close(rotationVector(a.r), [0, 0, Math.PI / 2]);
  // Media vuelta
  const h = poseOf([0, 0, 0], [Math.PI, 0, 0]);
  close(rotationVector(h.r).map(Math.abs), [Math.PI, 0, 0], 1e-6);
  // Girar 90° alrededor de Z del mundo una instancia sin giro
  close(rotateInstance([0, 0, 0], [0, 0, 0], [0, 0, 1], 90), [0, 0, Math.PI / 2]);
});

test("explosión por pasos", () => {
  const steps = [
    { instances: [2], dir: [0, 0, 2], distance: 30 },
    { instances: [2, 3], dir: [1, 0, 0], distance: 10 },
  ];
  const off = explodeOffsets(steps, 1.5);
  close(off.get(2), [5, 0, 30]);
  close(off.get(3), [5, 0, 0]);
  assert.equal(explodeOffsets(steps, 0).size, 0);
});

test("mover una instancia dentro de la malla (visor con Y arriba)", () => {
  // Un triángulo (cara 0, instancia 7) y una arista; mmPerUnit 2
  const mesh = {
    positions: Float32Array.from([0, 0, 0, 1, 0, 0, 0, 1, 0]),
    normals: Float32Array.from([0, 0, 1, 0, 0, 1, 0, 0, 1]),
    indices: Uint32Array.from([0, 1, 2]),
    triangleFace: Uint32Array.from([0]),
    edgePoints: Float32Array.from([0, 0, 0, 1, 0, 0]),
    edgeEnds: Uint32Array.from([2]),
  };
  const moved = moveAssemblyMesh(mesh, [[7, 0, 1, 0, 1]], new Map([[7, poseOf([10, 0, 0], [0, 0, 0])]]), 2);
  // 10 mm en X del CAD = 5 unidades en X del visor
  close(Array.from(moved.positions.slice(0, 3)), [5, 0, 0], 1e-6);
  close(Array.from(moved.edgePoints.slice(3, 6)), [6, 0, 0], 1e-6);
  // Subir 4 mm (Z del CAD) = 2 unidades en Y del visor
  const up = moveAssemblyMesh(mesh, [[7, 0, 1, 0, 1]], new Map([[7, poseOf([0, 0, 4], [0, 0, 0])]]), 2);
  close(Array.from(up.positions.slice(0, 3)), [0, 2, 0], 1e-6);
});

test("pruneAssembly: sin acoplamientos ni pasos colgando", () => {
  const c = (instance) => ({ instance, origin: [0, 0, 0], z: [0, 0, 1], x: [1, 0, 0] });
  const a = {
    instances: [{ id: 1 }, { id: 2 }],
    mates: [
      { id: 10, a: c(1), b: c(2) },
      { id: 11, a: c(1), b: c(3) },
    ],
    relations: [{ type: "gear", a: 10, b: 11, ratio: -1 }, { type: "screw", cylindrical: 10, pitch: 1 }],
    groups: [{ id: 5, name: "", members: [2, 3] }],
    motion: [{ mate: 11, var: "angle", from: 0, to: 1, seconds: 1 }],
    explode: [{ instances: [3], dir: [0, 0, 1], distance: 5 }],
    patterns: [{ id: 6, source: 1, kind: { type: "points", reference: 2, offsets: [[1, 0, 0], [2, 0, 0]] }, copies: [3, 2] }],
    next_id: 20,
  };
  pruneAssembly(a);
  assert.deepEqual(a.mates.map((m) => m.id), [10]);
  assert.deepEqual(a.relations.map((r) => r.type), ["screw"]);
  assert.equal(a.groups.length, 0);
  assert.equal(a.motion.length, 0);
  assert.equal(a.explode.length, 0);
  assert.deepEqual(a.patterns[0].copies, [2]);
  assert.deepEqual(a.patterns[0].kind.offsets, [[2, 0, 0]]);
});

test("ajustar un conector de antes (sin cara) o ligado", async () => {
  const { adjustConnector } = await import("../src/lib/assemblyTools.ts");
  const c = { instance: 1, origin: [0, 0, 0], z: [0, 0, 1], x: [1, 0, 0] };
  const moved = adjustConnector(c, { offset: 2, spin: 90, reverse: true });
  close(moved.origin, [0, 0, 2]);
  close(moved.x, [0, 1, 0]);
  close(moved.z, [0, 0, -1]);
  // Ligado: solo guarda los números
  const tied = adjustConnector({ ...c, at: { type: "vertex", point: [0, 0, 0] } }, { offset: 2 });
  close(tied.origin, [0, 0, 0]);
  assert.equal(tied.offset, 2);
});
