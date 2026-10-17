// Lógica pura de las operaciones pesadas de animación, sobre datos planos.
// La usan el Web Worker (animWorker.ts) y el respaldo síncrono del cliente:
// `RigContext` lleva funciones y Maps (no se clona), así que cada lado lo
// reconstruye con `createRigContext(bones, settings)`.
import type { AnimationClip } from "./animation";
import type { JointLimits } from "./jointLimits";
import { observedLimits } from "./jointLimits";
import type { SkeletonBone } from "./presetAnimations";
import { bakeClip, createRigContext, type RigSettings } from "./rig";

export type JointKind = "hinge" | "ball";

/** Hornea cada clip (los ya horneados pasan igual); `onProgress` se llama al terminar cada uno */
export function bakeClipsPlain(
  clips: AnimationClip[],
  bones: SkeletonBone[],
  settings: RigSettings,
  onProgress?: (done: number, total: number) => void
): AnimationClip[] {
  const ctx = createRigContext(bones, settings);
  return clips.map((c, i) => {
    const out = c.baked ? c : bakeClip(c, ctx);
    onProgress?.(i + 1, clips.length);
    return out;
  });
}

/** Límites observados de las animaciones; `kinds` viaja como pares [articulación, tipo] */
export function observedLimitsPlain(
  clips: AnimationClip[],
  bones: SkeletonBone[],
  settings: RigSettings,
  joints: number[],
  kinds: [number, JointKind][],
  margin?: number
): [number, JointLimits][] {
  const ctx = createRigContext(bones, settings);
  return [...observedLimits(clips, ctx, joints, new Map(kinds), margin)];
}

export type AnimJob =
  | { type: "bake"; clips: AnimationClip[]; bones: SkeletonBone[]; settings: RigSettings }
  | { type: "limits"; clips: AnimationClip[]; bones: SkeletonBone[]; settings: RigSettings; joints: number[]; kinds: [number, JointKind][]; margin?: number };

/** Ejecuta un trabajo (en el worker o en el hilo principal) */
export function runAnimJob(job: AnimJob, onProgress?: (done: number, total: number) => void): unknown {
  if (job.type === "bake") return bakeClipsPlain(job.clips, job.bones, job.settings, onProgress);
  return observedLimitsPlain(job.clips, job.bones, job.settings, job.joints, job.kinds, job.margin);
}
