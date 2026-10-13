// Planos de corte de Fabricar: mismas cuentas que el backend (node --experimental-strip-types)
import assert from "node:assert/strict";
import { fromUi, modelSizeMm, planeQuad, toUi, uiBounds } from "../src/lib/printCuts.ts";

// Una caja de 40 × 30 × 20 en la escena (Y arriba): x ancho, y alto, z fondo; unidades de 1 mm
const box = [];
for (const x of [0, 40]) for (const y of [0, 20]) for (const z of [0, 30]) box.push(x, y, z);
const b = uiBounds(box);
// En la interfaz (Z arriba): X = ancho, Y = fondo (−z de la escena), Z = alto
assert.deepEqual(modelSizeMm(b, 1), [40, 30, 20]);
assert.deepEqual(b.min, [0, -30, 0]);
assert.deepEqual(fromUi(toUi([1, 2, 3])), [1, 2, 3]);

// Plano a 5 mm del borde en X: pasa por x = 5 y mira a +X
let q = planeQuad(b, { axis: 0, position: 5, tilt_deg: 0 }, 1);
assert.ok(Math.abs(q.origin[0] - 5) < 1e-9 && Math.abs(q.normal[0] - 1) < 1e-9);
for (const c of q.corners) assert.ok(Math.abs(c[0] - 5) < 1e-9, "las esquinas están en el plano");

// Plano en Z (alto): en la escena es Y; a 8 mm del suelo
q = planeQuad(b, { axis: 2, position: 8, tilt_deg: 0 }, 1);
assert.ok(Math.abs(q.origin[1] - 8) < 1e-9 && Math.abs(q.normal[1] - 1) < 1e-9);

// Unidades de la escena de 0,5 mm: 5 mm son 10 unidades
q = planeQuad(b, { axis: 0, position: 5, tilt_deg: 0 }, 0.5);
assert.ok(Math.abs(q.origin[0] - 10) < 1e-9);

// Inclinado 30° en Z: la normal gira hacia X (el eje que sigue a Z)
q = planeQuad(b, { axis: 2, position: 8, tilt_deg: 30 }, 1);
assert.ok(Math.abs(q.normal[1] - Math.cos(Math.PI / 6)) < 1e-9 && Math.abs(q.normal[0] - 0.5) < 1e-9);
console.log("printCuts: ok");
