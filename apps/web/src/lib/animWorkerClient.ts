// Cliente del worker de animación: un único worker perezoso, cancelable, con
// respaldo síncrono si no hay `Worker` o no se puede crear/usar.
import type { AnimationClip } from "./animation";
import type { JointLimits } from "./jointLimits";
import type { SkeletonBone } from "./presetAnimations";
import type { RigSettings } from "./rig";
import { runAnimJob, type AnimJob, type JointKind } from "./animWorkerCore";

export type AnimProgress = (done: number, total: number) => void;

interface Pending {
  resolve: (v: unknown) => void;
  reject: (e: Error) => void;
  onProgress?: AnimProgress;
  job: AnimJob;
}

let worker: Worker | null = null;
let disabled = false; // el worker no se pudo crear o falló al cargar: todo va síncrono
let nextId = 1;
const pending = new Map<number, Pending>();
/** Plazo para que el worker avise que cargó (si el módulo no carga y no hay `error`, no se queda esperando) */
const READY_TIMEOUT_MS = 8000;

const abortError = () => new DOMException("Cancelado", "AbortError");
export const isAbort = (e: unknown) => e instanceof DOMException && e.name === "AbortError";

/** Respaldo: corre en el hilo principal, cediéndole el turno al navegador antes para que pinte el aviso */
async function runSync(job: AnimJob, onProgress?: AnimProgress, signal?: AbortSignal): Promise<unknown> {
  await new Promise((r) => setTimeout(r, 0));
  if (signal?.aborted) throw abortError();
  return runAnimJob(job, onProgress);
}

function killWorker(reason: Error) {
  worker?.terminate();
  worker = null;
  const all = [...pending.values()];
  pending.clear();
  all.forEach((p) => p.reject(reason));
}

/** Desactiva el worker y rehace lo pendiente en el hilo principal */
function fallBack(w: Worker) {
  disabled = true;
  const all = [...pending.values()];
  pending.clear();
  w.terminate();
  if (worker === w) worker = null;
  all.forEach((p) => runSync(p.job, p.onProgress).then(p.resolve, p.reject));
}

function getWorker(): Worker | null {
  if (disabled || typeof Worker === "undefined") return null;
  if (worker) return worker;
  try {
    const w = new Worker(new URL("./animWorker.ts", import.meta.url), { type: "module" });
    let ready = false;
    const readyTimer = setTimeout(() => {
      if (!ready && worker === w) fallBack(w);
    }, READY_TIMEOUT_MS);
    w.onmessage = (ev: MessageEvent<{ id?: number; ready?: boolean; progress?: { done: number; total: number }; result?: unknown; error?: string }>) => {
      ready = true;
      clearTimeout(readyTimer);
      const m = ev.data;
      if (m.id === undefined) return;
      const p = pending.get(m.id);
      if (!p) return;
      if (m.progress) return p.onProgress?.(m.progress.done, m.progress.total);
      pending.delete(m.id);
      if (m.error !== undefined) p.reject(new Error(m.error));
      else p.resolve(m.result);
    };
    // Error del propio worker (no cargó el módulo, etc.): se desactiva y lo pendiente se rehace síncrono
    w.onerror = (ev) => {
      ev.preventDefault?.();
      clearTimeout(readyTimer);
      fallBack(w);
    };
    worker = w;
    return w;
  } catch {
    disabled = true;
    return null;
  }
}

async function run<T>(job: AnimJob, onProgress?: AnimProgress, signal?: AbortSignal): Promise<T> {
  if (signal?.aborted) throw abortError();
  const w = getWorker();
  if (!w) return (await runSync(job, onProgress, signal)) as T;
  return new Promise<T>((resolve, reject) => {
    const id = nextId++;
    const onAbort = () => {
      // El worker hace un bucle síncrono: la única forma de cortarlo es terminarlo (se recrea en la próxima)
      pending.delete(id);
      killWorker(new Error("Worker reiniciado"));
      reject(abortError());
    };
    const done = () => signal?.removeEventListener("abort", onAbort);
    const entry: Pending = {
      resolve: (v) => (done(), resolve(v as T)),
      reject: (e) => (done(), reject(e)),
      onProgress,
      job,
    };
    pending.set(id, entry);
    signal?.addEventListener("abort", onAbort, { once: true });
    try {
      w.postMessage({ id, job });
    } catch {
      // Datos no clonables (p. ej. proxies de un store): se hace en el hilo principal
      pending.delete(id);
      done();
      runSync(job, onProgress, signal).then((v) => resolve(v as T), reject);
    }
  });
}

/** Hornea los clips (los ya horneados pasan tal cual) fuera del hilo principal */
export function bakeClipsAsync(
  clips: AnimationClip[],
  bones: SkeletonBone[],
  settings: RigSettings,
  onProgress?: AnimProgress,
  signal?: AbortSignal
): Promise<AnimationClip[]> {
  return run<AnimationClip[]>({ type: "bake", clips, bones, settings }, onProgress, signal);
}

/** Límites observados de las animaciones, fuera del hilo principal */
export async function observedLimitsAsync(
  clips: AnimationClip[],
  bones: SkeletonBone[],
  settings: RigSettings,
  joints: number[],
  kinds: Map<number, JointKind>,
  signal?: AbortSignal,
  margin?: number
): Promise<Map<number, JointLimits>> {
  const pairs = await run<[number, JointLimits][]>({ type: "limits", clips, bones, settings, joints, kinds: [...kinds], margin }, undefined, signal);
  return new Map(pairs);
}

/** Termina el worker (se vuelve a crear solo si hace falta) */
export function disposeAnimWorker() {
  killWorker(new Error("Worker terminado"));
}
