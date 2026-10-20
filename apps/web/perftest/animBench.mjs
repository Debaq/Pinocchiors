// Banco de estrés de animación: muchos huesos x muchas keys (node --experimental-strip-types perftest/animBench.mjs)
// Mide lo que hace la app en el hilo principal al reproducir y al editar keys.
import { createRigContext, evaluatePose, loadRigSettings, runPoseStack } from "../src/lib/rig.ts";
import { samplePose, insertKeys, moveKeys, keysAtFrames, keysByFrame, trackKeyInfo, keyFrames, keyId, clonePose } from "../src/lib/animation.ts";

const BONES = Number(process.env.BONES ?? 120);
const KEYS = Number(process.env.KEYS ?? 300); // keys por pista
const STEP = Number(process.env.STEP ?? 2); // cuadros entre keys
const BEZIER = process.env.BEZIER !== "0";

let seed = 1;
const rnd = () => ((seed = (seed * 16807) % 2147483647) / 2147483647) * 2 - 1;

// Árbol: columna de 12 + ramas de 4 huesos hasta llegar a BONES
const bones = [];
for (let i = 0; i < 12 && bones.length < BONES; i++) bones.push({ name: `sp${i}`, position: [0, 0.1 * i, 0], parent: i === 0 ? null : i - 1 });
let n = 0;
while (bones.length < BONES) {
  const base = 2 + (n % 10);
  const side = n % 2 ? 1 : -1;
  for (let k = 0; k < 4 && bones.length < BONES; k++) {
    const parent = k === 0 ? base : bones.length - 1;
    bones.push({ name: `r${n}_${k}`, position: [side * (0.1 + 0.08 * k), 0.1 * base - 0.05 * k, 0], parent });
  }
  n++;
}

const quat = (a) => {
  const x = rnd() * a, y = rnd() * a, z = rnd() * a;
  const l = Math.hypot(x, y, z, 1);
  return [x / l, y / l, z / l, 1 / l];
};

const settings = loadRigSettings({});
for (const b of bones) {
  settings.bones ??= {};
}
const clip = {
  id: "c", name: "stress", fps: 24, start: 0, end: KEYS * STEP,
  tracks: bones.map((b) => ({
    bone: b.name,
    rotation: Array.from({ length: KEYS }, (_, i) => ({ frame: i * STEP, value: quat(0.3), interpolation: BEZIER && i % 2 ? "bezier" : "linear" })),
    translation: b.parent === null ? Array.from({ length: KEYS }, (_, i) => ({ frame: i * STEP, value: [rnd() * 0.1, 0, 0], interpolation: "linear" })) : [],
  })),
};

const time = (label, fn, reps = 1) => {
  fn(); // calentar
  const t0 = performance.now();
  for (let i = 0; i < reps; i++) fn();
  const ms = (performance.now() - t0) / reps;
  console.log(`${label.padEnd(58)} ${ms.toFixed(2).padStart(9)} ms`);
  return ms;
};

console.log(`huesos=${bones.length} keys/pista=${KEYS} cuadros=${clip.end} keys totales=${clip.tracks.reduce((s, t) => s + t.rotation.length + t.translation.length, 0)}`);
console.log(`presupuesto por cuadro a 60 Hz: 16.7 ms\n`);

const ctx = createRigContext(bones, settings);
const mid = Math.round(clip.end / 2) + 1; // lejos de las keys, caso típico

time("samplePose (1 cuadro)", () => samplePose(clip, mid, ctx.boneIndex, ctx.rotation), 50);
time("runPoseStack (1 cuadro, sobre pose muestreada)", () => runPoseStack(clonePose(samplePose(clip, mid, ctx.boneIndex, ctx.rotation)), ctx, { clip, frame: mid }), 50);
time("evaluatePose (1 cuadro) = lo de reproducir/arrastrar", () => evaluatePose(clip, mid, ctx), 50);
const unl = { ...ctx, settings: { ...ctx.settings, limitsOff: true } };
time("violations: evaluatePose x 600 cuadros", () => { for (let f = 0; f <= Math.min(clip.end, 600); f++) evaluatePose(clip, f, unl); });
time("insertKeys (poner 1 key en todos los huesos)", () => insertKeys(clip, mid, bones.map((b) => ({ bone: b.name, rotation: [0, 0, 0, 1] })), "linear"), 5);
const sel = new Set(clip.tracks.slice(0, 20).flatMap((t) => t.rotation.slice(0, 50).map((k) => keyId(t.bone, k.frame))));
time("moveKeys (1000 keys seleccionadas)", () => moveKeys(clip, sel, 1), 5);
time("Timeline: keyFrames de todas las pistas", () => clip.tracks.forEach((t) => keyFrames(t)), 5);
const summary = [...new Set(clip.tracks.flatMap(keyFrames))].sort((a, b) => a - b);
time(`Timeline: fila resumen (${summary.length} x keysAtFrames)`, () => summary.forEach((f) => keysAtFrames(clip, new Set([f]))), 1);
time("Timeline: fila resumen nueva (keysByFrame, 1 pasada)", () => keysByFrame(clip), 5);
time("Timeline: trackKeyInfo de todas las pistas", () => clip.tracks.forEach((t) => trackKeyInfo(t)), 5);
