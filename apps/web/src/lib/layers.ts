/**
 * Capas de animación (mezclador, NLA): clips puestos en el tiempo de la
 * escena, uno sobre otro. Cada capa reemplaza lo que tenían debajo las
 * articulaciones que anima (con su peso) o se suma encima (caminar +
 * respirar + mirar). Se repite, se recorta, entra y sale con transiciones y
 * puede afectar solo a algunos grupos de huesos.
 *
 * La mezcla se hace con las keys (FK); después pasa por la pila del rig
 * (IK, límites) como cualquier pose. Para exportar se hornea a un clip.
 */

import {
  clonePose,
  createClip,
  emptyPose,
  samplePose,
  slerp,
  type AnimationClip,
  type Key,
  type Pose,
  type Quat,
  type Vec3,
} from "./animation";
import { IDENTITY, quatMultiply, reduceKeys, runPoseStack, quatAngle, quatInverse, type RigContext } from "./rig";

export interface AnimationLayer {
  id: string;
  clipId: string;
  mode: "replace" | "add";
  /** 0–1 */
  weight: number;
  /** Cuadro de la escena donde empieza */
  start: number;
  /** Tramo del clip que se usa (por defecto, su rango) */
  trimStart?: number;
  trimEnd?: number;
  /** Veces que se repite el tramo */
  repeat: number;
  /** Cuadros de entrada y salida gradual (transiciones) */
  blendIn: number;
  blendOut: number;
  /** Grupos de huesos a los que afecta (sin máscara, a todos) */
  mask?: string[];
  muted?: boolean;
}

export interface Mixer {
  /** El visor muestra la mezcla en vez del clip activo */
  enabled: boolean;
  /** De abajo hacia arriba */
  layers: AnimationLayer[];
}

export const emptyMixer = (): Mixer => ({ enabled: false, layers: [] });

export function loadMixer(raw: unknown): Mixer {
  const m = raw as Partial<Mixer> | undefined;
  if (!m || !Array.isArray(m.layers)) return emptyMixer();
  return { enabled: m.enabled === true, layers: m.layers.filter((l) => l && typeof l.id === "string" && typeof l.clipId === "string") };
}

/** Tramo del clip que usa la capa (inclusive) */
export function layerTrim(layer: AnimationLayer, clip: AnimationClip): [number, number] {
  const a = layer.trimStart ?? clip.start;
  const b = Math.max(a, layer.trimEnd ?? clip.end);
  return [a, b];
}

/** Cuadros que dura la capa en la escena */
export function layerLength(layer: AnimationLayer, clip: AnimationClip): number {
  const [a, b] = layerTrim(layer, clip);
  return (b - a + 1) * Math.max(1, layer.repeat);
}

/** Cuadro del clip que suena en el cuadro `frame` de la escena (`null` fuera de la capa) */
export function layerLocalFrame(layer: AnimationLayer, clip: AnimationClip, frame: number): number | null {
  const length = layerLength(layer, clip);
  const t = frame - layer.start;
  if (t < 0 || t >= length) return null;
  const [a, b] = layerTrim(layer, clip);
  const span = b - a + 1;
  return a + (t % span);
}

/** Peso de la capa en `frame`, con las rampas de entrada y salida */
export function layerWeight(layer: AnimationLayer, clip: AnimationClip, frame: number): number {
  const length = layerLength(layer, clip);
  const t = frame - layer.start;
  if (t < 0 || t >= length) return 0;
  let w = Math.max(0, Math.min(1, layer.weight));
  if (layer.blendIn > 0) w *= Math.min(1, (t + 1) / layer.blendIn);
  if (layer.blendOut > 0) w *= Math.min(1, (length - t) / layer.blendOut);
  return w;
}

/** Rango de la escena que cubren las capas */
export function mixerRange(mixer: Mixer, clips: AnimationClip[]): [number, number] {
  let lo = Infinity;
  let hi = -Infinity;
  for (const l of mixer.layers) {
    const clip = clips.find((c) => c.id === l.clipId);
    if (!clip) continue;
    lo = Math.min(lo, l.start);
    hi = Math.max(hi, l.start + layerLength(l, clip) - 1);
  }
  return Number.isFinite(lo) ? [lo, hi] : [0, 0];
}

const lerp3 = (a: Vec3, b: Vec3, t: number): Vec3 => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];

/**
 * Pose de las keys de todas las capas en `frame` (antes del IK y los
 * límites) y la capa de más arriba que suena, con su cuadro local (sus
 * canales de IK son los que valen)
 */
export function mixKeys(mixer: Mixer, clips: AnimationClip[], frame: number, ctx: RigContext): { pose: Pose; top?: { clip: AnimationClip; frame: number } } {
  const out = emptyPose();
  let top: { clip: AnimationClip; frame: number } | undefined;
  for (const layer of mixer.layers) {
    if (layer.muted) continue;
    const clip = clips.find((c) => c.id === layer.clipId);
    if (!clip) continue;
    const local = layerLocalFrame(layer, clip, frame);
    const w = layerWeight(layer, clip, frame);
    if (local === null || w <= 0) continue;
    top = { clip, frame: local };
    const pose = samplePose(clip, local, ctx.boneIndex, ctx.rotation);
    const allowed = (j: number) => {
      if (!layer.mask || layer.mask.length === 0) return true;
      const group = ctx.settings.bones[ctx.bones[j]?.name]?.group;
      return !!group && layer.mask.includes(group);
    };
    for (const [j, q] of pose.rotations) {
      if (!allowed(j)) continue;
      const base = out.rotations.get(j) ?? IDENTITY;
      out.rotations.set(j, layer.mode === "add" ? quatMultiply(slerp(IDENTITY, q, w), base) : slerp(base, q, w));
    }
    for (const [j, t] of pose.translations) {
      if (!allowed(j)) continue;
      const base = out.translations.get(j) ?? [0, 0, 0];
      out.translations.set(j, layer.mode === "add" ? [base[0] + t[0] * w, base[1] + t[1] * w, base[2] + t[2] * w] : lerp3(base, t, w));
    }
    if (!layer.mask || layer.mask.length === 0) {
      for (const [id, c] of pose.controls) {
        const base = out.controls.get(id) ?? { rotation: IDENTITY, translation: [0, 0, 0] as Vec3 };
        out.controls.set(
          id,
          layer.mode === "add"
            ? { rotation: quatMultiply(slerp(IDENTITY, c.rotation, w), base.rotation), translation: lerp3(base.translation, [base.translation[0] + c.translation[0], base.translation[1] + c.translation[1], base.translation[2] + c.translation[2]], w) }
            : { rotation: slerp(base.rotation, c.rotation, w), translation: lerp3(base.translation, c.translation, w) }
        );
      }
    }
  }
  return { pose: out, top };
}

/** Pose final de la mezcla en `frame` (con IK y límites) */
export function evaluateMixer(mixer: Mixer, clips: AnimationClip[], frame: number, ctx: RigContext): Pose {
  const { pose, top } = mixKeys(mixer, clips, frame, ctx);
  return runPoseStack(clonePose(pose), ctx, top ? { clip: top.clip, frame: top.frame } : {});
}

/**
 * La mezcla horneada a un clip nuevo: una key por cuadro en cada
 * articulación que se mueve, y se quitan las que sobran
 */
export function bakeMixer(mixer: Mixer, clips: AnimationClip[], ctx: RigContext, name: string, fps: number): AnimationClip {
  const [start, end] = mixerRange(mixer, clips);
  const rotations = new Map<number, Key<Quat>[]>();
  const translations = new Map<number, Key<Vec3>[]>();
  for (let f = start; f <= end; f++) {
    const pose = evaluateMixer(mixer, clips, f, ctx);
    for (const [j, q] of pose.rotations) {
      if (!rotations.has(j)) rotations.set(j, []);
      rotations.get(j)!.push({ frame: f - start, value: q, interpolation: "linear" });
    }
    for (const [j, t] of pose.translations) {
      if (!translations.has(j)) translations.set(j, []);
      translations.get(j)!.push({ frame: f - start, value: t, interpolation: "linear" });
    }
  }
  const joints = [...new Set([...rotations.keys(), ...translations.keys()])].sort((a, b) => a - b);
  const clip = createClip(name, fps);
  return {
    ...clip,
    start: 0,
    end: end - start,
    tracks: joints.map((j) => ({
      bone: ctx.bones[j].name,
      rotation: reduceKeys(rotations.get(j) ?? [], (a, b) => quatAngle(quatMultiply(quatInverse(a), b)) < 1e-4, slerp),
      translation: reduceKeys(translations.get(j) ?? [], (a, b) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]) < 1e-6, lerp3),
    })),
  };
}
