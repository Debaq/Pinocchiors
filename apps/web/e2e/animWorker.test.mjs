// El trabajo del worker de animación (datos planos + contexto reconstruido) da EXACTAMENTE lo mismo que
// bakeClip / observedLimits directos, y el mensaje es clonable (node e2e/animWorker.test.mjs).
// Node puro no resuelve los imports sin extensión de lib/: se empaqueta con esbuild a un temporal.
import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { build } from "esbuild";

const dir = mkdtempSync(join(tmpdir(), "animworker-"));
const entry = join(dir, "entry.ts");
const out = join(dir, "bundle.mjs");
const lib = new URL("../src/lib/", import.meta.url).pathname;
await build({
  stdin: {
    contents: `export * from ${JSON.stringify(lib + "rig.ts")}; export * from ${JSON.stringify(lib + "jointLimits.ts")}; export * from ${JSON.stringify(lib + "animWorkerCore.ts")};`,
    resolveDir: lib,
    loader: "ts",
  },
  bundle: true, format: "esm", platform: "node", outfile: out, logLevel: "error",
});
const m = await import(pathToFileURL(out).href);
rmSync(dir, { recursive: true, force: true });

const BONES = Number(process.env.BONES ?? 40), KEYS = Number(process.env.KEYS ?? 60), STEP = 2;
let seed = 1;
const rnd = () => ((seed = (seed * 16807) % 2147483647) / 2147483647) * 2 - 1;
const bones = [];
for (let i = 0; i < 12 && bones.length < BONES; i++) bones.push({ name: `sp${i}`, position: [0, 0.1 * i, 0], parent: i === 0 ? null : i - 1 });
let n = 0;
while (bones.length < BONES) {
  const base = 2 + (n % 10), side = n % 2 ? 1 : -1;
  for (let k = 0; k < 4 && bones.length < BONES; k++)
    bones.push({ name: `r${n}_${k}`, position: [side * (0.1 + 0.08 * k), 0.1 * base - 0.05 * k, 0], parent: k === 0 ? base : bones.length - 1 });
  n++;
}
const quat = (a) => { const x = rnd() * a, y = rnd() * a, z = rnd() * a, l = Math.hypot(x, y, z, 1); return [x / l, y / l, z / l, 1 / l]; };
const settings = m.loadRigSettings({});
const clip = (id) => ({
  id, name: id, fps: 24, start: 0, end: KEYS * STEP,
  tracks: bones.map((b) => ({
    bone: b.name,
    rotation: Array.from({ length: KEYS }, (_, i) => ({ frame: i * STEP, value: quat(0.3), interpolation: i % 2 ? "bezier" : "linear" })),
    translation: b.parent === null ? Array.from({ length: KEYS }, (_, i) => ({ frame: i * STEP, value: [rnd() * 0.1, 0, 0], interpolation: "linear" })) : [],
  })),
});
const clips = [clip("a"), { ...clip("b"), baked: true }, clip("c")];

// El mensaje del worker tiene que sobrevivir al structured clone (lo que hace postMessage)
const job = { type: "bake", clips, bones, settings };
const cloned = structuredClone(job);

// Horneado: directo vs datos planos (clonados, como llegan al worker)
const ctx = m.createRigContext(bones, settings);
const direct = clips.map((c) => (c.baked ? c : m.bakeClip(c, ctx)));
const progress = [];
const viaWorker = structuredClone(m.runAnimJob(cloned, (d, t) => progress.push([d, t])));
assert.deepEqual(viaWorker, direct);
assert.deepEqual(progress, [[1, 3], [2, 3], [3, 3]]);
assert.ok(viaWorker[0].tracks.length > 0 && viaWorker[0].tracks[0].rotation.length > 0);

// Límites observados
const joints = bones.map((_, i) => i).filter((j) => ctx.children[j].length > 0);
const kinds = new Map(joints.map((j) => [j, j % 2 ? "hinge" : "ball"]));
const directLimits = m.observedLimits([clips[0], clips[2]], ctx, joints, kinds);
const lim = structuredClone(m.runAnimJob(structuredClone({ type: "limits", clips: [clips[0], clips[2]], bones, settings, joints, kinds: [...kinds] }), undefined));
assert.deepEqual(new Map(lim), directLimits);
assert.ok(directLimits.size > 0);

console.log(`animWorker: OK (huesos=${BONES}, clips=3, articulaciones=${joints.length})`);
