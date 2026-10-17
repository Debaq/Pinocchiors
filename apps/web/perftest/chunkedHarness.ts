// Prueba de createChunked: no bloquea, reinicia al cambiar la fuente, conserva el último resultado
import { createRoot, createSignal, createEffect } from "solid-js";
import { createChunked } from "../src/lib/chunked";
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const work = () => { const t = performance.now(); while (performance.now() - t < 2) {} }; // 2 ms por cuadro
const out: any = {};
await createRoot(async () => {
  const [n, setN] = createSignal(1);
  const seen: number[] = [];
  const res = createChunked<number, number[], number>({
    source: () => n(), frames: () => [0, 99], start: () => [0], delay: 20,
    step: (_s, acc) => { work(); acc[0]++; }, finish: (acc) => acc[0] * n(), empty: -1,
  });
  createEffect(() => seen.push(res()));
  let maxGap = 0, last = performance.now(); const gaps: any[] = []; const t00 = last;
  const id = setInterval(() => { const now = performance.now(); if (now - last > 15) gaps.push([Math.round(now - t00), Math.round(now - last)]); maxGap = Math.max(maxGap, now - last); last = now; }, 1);
  await sleep(100);
  setN(2); await sleep(60); setN(3); // cambia a mitad de camino: lo de n=2 se descarta
  await sleep(900);
  clearInterval(id);
  Object.assign(out, { seen, maxGapMs: Math.round(maxGap), gaps });
});
(window as any).__result = out;
