// Historial compacto: cada lista distinta se guarda una vez (node --experimental-strip-types)
import assert from "node:assert/strict";
import { packHistory, unpackHistory } from "../src/lib/historyPack.ts";
// Como insertKeys: copia las pistas pero comparte las listas que no cambian
const insertKeys = (c, frame, entries) => ({
  ...c,
  tracks: c.tracks.map((t) => (entries.some((e) => e.bone === t.bone) ? { ...t, rotation: [...t.rotation, { frame, value: [0, 0, 0, 1], interpolation: "linear" }] } : { ...t })),
});

const keys = (n, seed) => Array.from({ length: n }, (_, i) => ({ frame: i, value: [seed, i, 0, 1], interpolation: "linear" }));
let clip = { id: "c", name: "x", fps: 24, start: 0, end: 100, tracks: ["a", "b", "c"].map((bone, i) => ({ bone, rotation: keys(100, i), translation: [] })) };
const nodes = [{ id: 0, parent: null, children: [1], description: "Inicio", time: 0, step: null }];
for (let e = 1; e <= 20; e++) {
  const before = clip;
  clip = insertKeys(clip, 200 + e, [{ bone: "a", rotation: [0, 0, 0, 1] }], "linear");
  nodes.push({ id: e, parent: e - 1, children: e < 20 ? [e + 1] : [], description: "key", time: e, step: { kind: "clip", data: { before, after: clip } } });
}
const saved = { version: 1, nodes, current: 20 };

const packed = packHistory(saved);
const plain = JSON.stringify(saved).length;
const small = JSON.stringify(packed).length;
assert.ok(small < plain / 3, `compacto ${small} vs ${plain}`);

// Ida y vuelta por JSON: mismos datos
const back = unpackHistory(JSON.parse(JSON.stringify(packed)));
assert.deepEqual(back, saved);
assert.equal(back.pool, undefined);

// Se vuelve a compartir: las pistas b y c no cambian entre pasos
const d1 = back.nodes[1].step.data, d2 = back.nodes[2].step.data;
assert.equal(d1.after.tracks[1].rotation, d2.before.tracks[1].rotation);

// Proyectos viejos (sin pool) y datos sin listas largas quedan igual
const old = { version: 1, nodes: [nodes[0]], current: 0 };
assert.equal(unpackHistory(old), old);
assert.equal(packHistory(old), old);
// Una lista corta no se agrupa
const tiny = { version: 1, nodes: [{ ...nodes[0] }, { id: 1, parent: 0, children: [], description: "t", time: 1, step: { kind: "k", data: { v: [1, 2, 3] } } }], current: 1 };
assert.equal(packHistory(tiny), tiny);
// Pool con nodos inválidos no revienta
assert.deepEqual(unpackHistory({ version: 1, nodes: "x", current: 0, pool: [] }).nodes, "x");

console.log(`historyPack ok (${(plain / 1e6).toFixed(1)} MB -> ${(small / 1e6).toFixed(2)} MB)`);
