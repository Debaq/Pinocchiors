// Prueba en navegador del worker de animación: carga como módulo, mismo resultado que el síncrono, no bloquea el hilo
import { bakeClipsAsync } from "../src/lib/animWorkerClient";
import { runAnimJob } from "../src/lib/animWorkerCore";
import { loadRigSettings } from "../src/lib/rig";
const out: any = {};
let ctorCalls = 0;
const RealWorker = window.Worker;
(window as any).Worker = class extends RealWorker { constructor(...a: any[]) { super(...(a as [any, any])); ctorCalls++; } };
const BONES = 60, KEYS = 200;
let seed = 1; const rnd = () => ((seed = (seed * 16807) % 2147483647) / 2147483647) * 2 - 1;
const bones = Array.from({ length: BONES }, (_, i) => ({ name: `b${i}`, position: [0, i * 0.1, 0] as [number, number, number], parent: i === 0 ? null : i - 1 }));
const clip: any = { id: "c", name: "s", fps: 24, start: 0, end: KEYS * 2, tracks: bones.map((b) => ({ bone: b.name, rotation: Array.from({ length: KEYS }, (_, i) => ({ frame: i * 2, value: [rnd() * 0.1, 0, 0, 1], interpolation: i % 2 ? "bezier" : "linear" })), translation: [] })) };
const settings = loadRigSettings({});
// latido del hilo principal durante el trabajo
let maxGap = 0, last = performance.now();
const hb = setInterval(() => { const n = performance.now(); maxGap = Math.max(maxGap, n - last); last = n; }, 5);
const t0 = performance.now();
const viaWorker = await bakeClipsAsync([clip], bones as any, settings);
out.workerMs = Math.round(performance.now() - t0);
clearInterval(hb);
out.maxHeartbeatGapMs = Math.round(maxGap);
out.workerCtorCalls = ctorCalls;
const t1 = performance.now();
const direct = runAnimJob({ type: "bake", clips: [clip], bones: bones as any, settings }) as any[];
out.syncMs = Math.round(performance.now() - t1);
out.equal = JSON.stringify(viaWorker) === JSON.stringify(direct);
out.tracksBaked = viaWorker[0].tracks.length;
(window as any).__result = out;
