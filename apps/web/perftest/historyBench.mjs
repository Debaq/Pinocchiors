// Costo de guardar el historial de edición con muchas keys (el árbol guarda el clip completo en cada paso)
import { insertKeys } from "../src/lib/animation.ts";
import { packHistory, unpackHistory } from "../src/lib/historyPack.ts";
const BONES = Number(process.env.BONES ?? 120), KEYS = Number(process.env.KEYS ?? 300), EDITS = Number(process.env.EDITS ?? 50);
let seed = 1;
const rnd = () => ((seed = (seed * 16807) % 2147483647) / 2147483647) * 2 - 1;
const names = Array.from({ length: BONES }, (_, i) => `b${i}`);
let clip = {
  id: "c", name: "stress", fps: 24, start: 0, end: KEYS * 2,
  tracks: names.map((bone) => ({ bone, rotation: Array.from({ length: KEYS }, (_, i) => ({ frame: i * 2, value: [rnd() * 0.1, rnd() * 0.1, 0, 1], interpolation: i % 2 ? "bezier" : "linear" })), translation: [] })),
};
const nodes = [];
const t0 = performance.now();
for (let e = 0; e < EDITS; e++) {
  const before = clip;
  // Edición típica: auto-key de 3 huesos en un cuadro
  clip = insertKeys(clip, 1 + e * 2, [0, 1, 2].map((k) => ({ bone: names[(e + k) % BONES], rotation: [0.01, 0, 0, 1] })), "linear");
  nodes.push({ id: e + 1, parent: e, children: [], description: "Poner key", time: Date.now(), step: { kind: "clip", data: { before, after: clip } } });
}
console.log(`huesos=${BONES} keys/pista=${KEYS} ediciones=${EDITS}`);
console.log(`crear ${EDITS} pasos: ${(performance.now() - t0).toFixed(0)} ms`);
const saved = { version: 1, nodes, current: EDITS };
const t1 = performance.now();
let json;
try { json = JSON.stringify(saved); console.log(`SIN compactar: stringify ${(performance.now() - t1).toFixed(0)} ms, ${(json.length / 1e6).toFixed(1)} MB`); } catch (e) { console.log(`SIN compactar: FALLA (${e.message})`); }
const t2 = performance.now();
const packed = packHistory(saved);
const t3 = performance.now();
const pj = JSON.stringify(packed);
console.log(`COMPACTO: pack ${(t3 - t2).toFixed(0)} ms + stringify ${(performance.now() - t3).toFixed(0)} ms, ${(pj.length / 1e6).toFixed(2)} MB`);
const t4 = performance.now();
const back = unpackHistory(JSON.parse(pj));
console.log(`COMPACTO: parse + unpack ${(performance.now() - t4).toFixed(0)} ms (nodos ${back.nodes.length})`);
