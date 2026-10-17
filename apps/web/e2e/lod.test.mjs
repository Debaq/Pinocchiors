// LOD al animar: decodificación del binario del backend y skin (node --experimental-strip-types)
import assert from "node:assert/strict";
import { decodeLod, skinAttributes } from "../src/lib/lod.ts";

// Binario como `Lod::to_bytes`: vértices de origen, bytes de malla, malla y pesos
const u32 = (...v) => new Uint8Array(new Uint32Array(v).buffer);
const f32 = (...v) => new Uint8Array(new Float32Array(v).buffer);
const concat = (...parts) => {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let o = 0;
  for (const p of parts) (out.set(p, o), (o += p.length));
  return out;
};
// Un triángulo: 3 vértices, 3 índices, sin UV ni quads
const mesh = concat(u32(3, 3, 0, 0), f32(0, 0, 0, 1, 0, 0, 0, 1, 0), f32(0, 0, 1, 0, 0, 1, 0, 0, 1), u32(0, 1, 2));
// Pesos: 3 vértices, 2 huesos, 2 influencias, nombres "a\nb" (3 bytes, relleno a 4)
const names = new Uint8Array([97, 10, 98, 0]);
const weights = concat(u32(3, 2, 2, 3), names, f32(0, 1, 1, 0, 0, 0.5, 1, 0.5, 1, 1, 0, 0));
const buffer = concat(u32(500_000, mesh.length), mesh, weights).buffer;

const lod = decodeLod(buffer);
assert.equal(lod.sourceVertices, 500_000);
assert.equal(lod.mesh.positions.length, 9);
assert.deepEqual([...lod.mesh.indices], [0, 1, 2]);
assert.equal(lod.weights.numVertices, 3);
assert.deepEqual(lod.weights.boneNames, ["a", "b"]);
assert.equal(lod.weights.weights.length, 12);

// Sin LOD: respuesta vacía
assert.equal(decodeLod(new ArrayBuffer(0)), null);

// Skin: hasta 4 influencias normalizadas; huesos que no deforman pasan su peso al padre
const skin = skinAttributes(lod.weights, 3, 2, [0, 1]);
for (let v = 0; v < 3; v++) {
  const sum = skin.skinWeight.slice(v * 4, v * 4 + 4).reduce((a, b) => a + b, 0);
  assert.ok(Math.abs(sum - 1) < 1e-6, `vértice ${v} suma ${sum}`);
}
assert.equal(skin.skinIndex[0], 0);
assert.equal(skin.skinWeight[0], 1); // vértice 0: solo el hueso 0
console.log("lod ok");
