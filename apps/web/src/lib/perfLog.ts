// Diagnóstico de rendimiento (PLAN_RENDIMIENTO): cuánto tarda cada llamada al
// backend y cada acción pesada del visor. Lo lee la herramienta `viewer` del
// conector para IA (acción `diagnostics`) para medir en la app real, antes y
// después de un cambio. Solo guarda totales por nombre: no pesa.

interface Stat {
  count: number;
  totalMs: number;
  maxMs: number;
  lastMs: number;
}

const calls = new Map<string, Stat>();
const marks = new Map<string, Stat>();

function add(map: Map<string, Stat>, name: string, ms: number): void {
  const s = map.get(name);
  if (s) {
    s.count++;
    s.totalMs += ms;
    s.maxMs = Math.max(s.maxMs, ms);
    s.lastMs = ms;
  } else {
    map.set(name, { count: 1, totalMs: ms, maxMs: ms, lastMs: ms });
  }
}

/** Anota cuánto tardó una acción del visor (pincelada, pose, texturas…) */
export function perfMark(name: string, ms: number): void {
  add(marks, name, ms);
}

/** Corre `fn` y anota cuánto tardó */
export function timed<T>(name: string, fn: () => T): T {
  const t0 = performance.now();
  try {
    return fn();
  } finally {
    add(marks, name, performance.now() - t0);
  }
}

/**
 * Mide todas las llamadas al backend: envuelve el `invoke` interno de Tauri,
 * que es por donde pasan todas (las de cada archivo importan el mismo). Sin
 * Tauri (vista previa web) no hace nada
 */
export function installInvokeTiming(): void {
  const internals = (window as unknown as { __TAURI_INTERNALS__?: { invoke?: (...args: unknown[]) => unknown } }).__TAURI_INTERNALS__;
  const original = internals?.invoke;
  if (!internals || !original) return;
  internals.invoke = (...args: unknown[]) => {
    const cmd = String(args[0]);
    const t0 = performance.now();
    return Promise.resolve(original.apply(internals, args)).finally(() => add(calls, cmd, performance.now() - t0));
  };
}

const round = (ms: number) => Math.round(ms * 10) / 10;

function rows(map: Map<string, Stat>, limit: number) {
  return [...map.entries()]
    .sort((a, b) => b[1].totalMs - a[1].totalMs)
    .slice(0, limit)
    .map(([name, s]) => ({ name, count: s.count, totalMs: round(s.totalMs), avgMs: round(s.totalMs / s.count), maxMs: round(s.maxMs), lastMs: round(s.lastMs) }));
}

/** Las llamadas y acciones que más tiempo sumaron; `reset` empieza de cero después de leer */
export function perfReport(reset = false) {
  const report = { calls: rows(calls, 30), marks: rows(marks, 30) };
  if (reset) {
    calls.clear();
    marks.clear();
  }
  return report;
}
