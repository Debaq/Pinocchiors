/**
 * Herramientas de pose (FK): reiniciar, espejo, copiar/pegar, biblioteca,
 * pose intermedia y empujar/relajar.
 *
 * Trabajan sobre `Pose` (índices de articulación) y devuelven solo lo que
 * cambia, para aplicarlo con `Viewer3D.applyPartialPose` y guardarlo en keys.
 * Lo que se guarda o se copia va por nombre de hueso, así sirve entre
 * clips, esqueletos editados y proyectos.
 */

import { emptyPose, samplePose, slerp, type AnimationClip, type ControlPose, type Pose, type Quat, type Vec3 } from "./animation";
import { IDENTITY, mirrorName, reflectQuat, reflectVector, vec, type RigContext } from "./rig";

/** Giro y desplazamiento de un hueso, por nombre */
export interface PoseEntry {
  rotation: Quat;
  translation?: Vec3;
}

export type PoseByName = Record<string, PoseEntry>;

/** Pose guardada en la biblioteca del proyecto */
export interface StoredPose {
  id: string;
  name: string;
  /** Miniatura (data URL JPEG) */
  thumbnail?: string;
  bones: PoseByName;
}

/** Articulaciones que se pueden posar: la raíz y las que tienen algo que girar */
export function animatableJoints(ctx: RigContext): number[] {
  return ctx.bones.flatMap((b, j) => (b.parent === null || ctx.children[j].length > 0 ? [j] : []));
}

const lerp3 = (a: Vec3, b: Vec3, t: number): Vec3 => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
const ZERO: Vec3 = [0, 0, 0];

// ─── Por nombre ─────────────────────────────────────────────────────────────

/** Pose de `joints` (todas las animables si no se dan), por nombre; las que están en reposo también */
export function poseToNames(pose: Pose, ctx: RigContext, joints: number[] = animatableJoints(ctx)): PoseByName {
  const out: PoseByName = {};
  for (const j of joints) {
    const bone = ctx.bones[j];
    if (!bone) continue;
    const entry: PoseEntry = { rotation: pose.rotations.get(j) ?? IDENTITY };
    const t = pose.translations.get(j);
    if (t && vec.length(t) > 1e-12) entry.translation = t;
    out[bone.name] = entry;
  }
  return out;
}

/** Pose con las entradas cuyos huesos existen en este esqueleto */
export function namesToPose(entries: PoseByName, ctx: RigContext): Pose {
  const pose = emptyPose();
  for (const [name, entry] of Object.entries(entries)) {
    const j = ctx.boneIndex.get(name);
    if (j === undefined) continue;
    pose.rotations.set(j, entry.rotation);
    pose.translations.set(j, entry.translation ?? ZERO);
  }
  return pose;
}

// ─── Mezclas ────────────────────────────────────────────────────────────────

/** De `from` hacia `to` en las articulaciones y controles de `to` (`t` puede pasar de 1 o ser negativo) */
export function blendPose(from: Pose, to: Pose, t: number): Pose {
  const out = emptyPose();
  for (const [j, q] of to.rotations) out.rotations.set(j, slerp(from.rotations.get(j) ?? IDENTITY, q, t));
  for (const [j, v] of to.translations) out.translations.set(j, lerp3(from.translations.get(j) ?? ZERO, v, t));
  for (const [id, c] of to.controls) {
    const a = from.controls.get(id);
    out.controls.set(id, {
      rotation: slerp(a?.rotation ?? IDENTITY, c.rotation, t),
      translation: lerp3(a?.translation ?? ZERO, c.translation, t),
    });
  }
  return out;
}

/** Reposo de `joints`: giro, desplazamiento o ambos */
export function resetPose(joints: number[], what: "rotation" | "translation" | "all", controls: string[] = []): Pose {
  const pose = emptyPose();
  for (const j of joints) {
    if (what !== "translation") pose.rotations.set(j, IDENTITY);
    if (what !== "rotation") pose.translations.set(j, ZERO);
  }
  for (const id of controls) pose.controls.set(id, { rotation: IDENTITY, translation: ZERO });
  return pose;
}

/**
 * Empujar (`factor` > 1) o relajar (< 1): la pose de `joints` se aleja o se
 * acerca al reposo
 */
export function pushRelax(pose: Pose, joints: number[], factor: number): Pose {
  const rest = emptyPose();
  const target = emptyPose();
  for (const j of joints) {
    target.rotations.set(j, pose.rotations.get(j) ?? IDENTITY);
    const t = pose.translations.get(j);
    if (t) target.translations.set(j, t);
  }
  return blendPose(rest, target, factor);
}

/**
 * Pose intermedia: para cada articulación de `joints` con keys antes y
 * después de `frame`, la mezcla entre esas dos keys (`t` = 0 la anterior, 1
 * la siguiente)
 */
export function breakdownPose(clip: AnimationClip, frame: number, joints: number[], ctx: RigContext, t: number): Pose {
  const out = emptyPose();
  const f = Math.round(frame);
  for (const j of joints) {
    const name = ctx.bones[j]?.name;
    const track = clip.tracks.find((tr) => tr.bone === name && !tr.kind);
    if (!track) continue;
    const frames = [...new Set([...track.rotation, ...track.translation].map((k) => k.frame))].sort((a, b) => a - b);
    const prev = frames.filter((k) => k < f).pop();
    const next = frames.find((k) => k > f);
    if (prev === undefined || next === undefined) continue;
    const a = samplePose(clip, prev, ctx.boneIndex, ctx.rotation);
    const b = samplePose(clip, next, ctx.boneIndex, ctx.rotation);
    out.rotations.set(j, slerp(a.rotations.get(j) ?? IDENTITY, b.rotations.get(j) ?? IDENTITY, t));
    if (track.translation.length > 0) {
      out.translations.set(j, lerp3(a.translations.get(j) ?? ZERO, b.translations.get(j) ?? ZERO, t));
    }
  }
  return out;
}

// ─── Espejo ─────────────────────────────────────────────────────────────────

/** Lado de la articulación: −1 izquierda, 1 derecha, 0 sobre el plano de simetría */
export function jointSide(ctx: RigContext, j: number): number {
  const d = vec.dot(vec.sub(ctx.bones[j].position, ctx.symmetry.point), ctx.symmetry.normal);
  const size = ctx.body?.size ?? 1;
  return Math.abs(d) < 1e-3 * size ? 0 : Math.sign(d);
}

/** El giro y el desplazamiento vistos en el espejo */
function reflectEntry(ctx: RigContext, rotation: Quat, translation: Vec3 | undefined): { rotation: Quat; translation: Vec3 } {
  const n = ctx.symmetry.normal;
  return { rotation: reflectQuat(rotation, n), translation: reflectVector(translation ?? ZERO, n) };
}

/**
 * Espejo de la pose:
 * - "flip": voltea todo (cada lado toma el reflejo del otro; el centro se refleja);
 * - "leftToRight" / "rightToLeft": copia un lado sobre el otro.
 * Con `joints`, solo esas articulaciones hacen de origen.
 */
export function mirrorPose(pose: Pose, ctx: RigContext, mode: "flip" | "leftToRight" | "rightToLeft", joints?: number[]): Pose {
  const out = emptyPose();
  const sources = joints ?? animatableJoints(ctx);
  const rotationOf = (j: number) => pose.rotations.get(j) ?? IDENTITY;
  const set = (target: number, source: number) => {
    const r = reflectEntry(ctx, rotationOf(source), pose.translations.get(source));
    out.rotations.set(target, r.rotation);
    out.translations.set(target, r.translation);
  };
  for (const j of sources) {
    const pair = ctx.mirror[j];
    const side = jointSide(ctx, j);
    if (mode === "flip") {
      if (pair !== null) {
        set(pair, j);
        // Con selección parcial, el par también cambia al reflejo de este
        if (!out.rotations.has(j)) set(j, pair);
      } else if (side === 0) set(j, j);
    } else if (pair !== null && side === (mode === "leftToRight" ? -1 : 1)) {
      set(pair, j);
    }
  }
  // Controles: se voltean por nombre (hand_l_ctl ↔ hand_r_ctl) o se reflejan
  if (mode === "flip" && !joints) {
    const byName = new Map(ctx.settings.controls.map((c) => [c.name, c.id]));
    for (const control of ctx.settings.controls) {
      const twin = mirrorName(control.name);
      const source = twin ? byName.get(twin) : undefined;
      const c: ControlPose | undefined = pose.controls.get(source ?? control.id);
      const r = reflectEntry(ctx, c?.rotation ?? IDENTITY, c?.translation);
      out.controls.set(control.id, r);
    }
  }
  return out;
}

/** Entradas por nombre pegadas del otro lado: hand_l va a hand_r reflejada; las del centro se reflejan */
export function mirrorEntries(entries: PoseByName, ctx: RigContext): PoseByName {
  const out: PoseByName = {};
  for (const [name, entry] of Object.entries(entries)) {
    const twin = mirrorName(name);
    const target = twin && ctx.boneIndex.has(twin) ? twin : name;
    out[target] = reflectEntry(ctx, entry.rotation, entry.translation);
  }
  return out;
}

// ─── Portapapeles ───────────────────────────────────────────────────────────

const CLIPBOARD_KEY = "pinocchio.poseClipboard";
let clipboard: PoseByName | null = null;

/** Copia la pose (queda también para otros proyectos y sesiones) */
export function copyPose(entries: PoseByName): void {
  clipboard = entries;
  try {
    localStorage.setItem(CLIPBOARD_KEY, JSON.stringify(entries));
  } catch {
    // Sin almacenamiento: queda solo en memoria
  }
}

export function readPoseClipboard(): PoseByName | null {
  if (clipboard) return clipboard;
  try {
    const raw = localStorage.getItem(CLIPBOARD_KEY);
    clipboard = raw ? (JSON.parse(raw) as PoseByName) : null;
  } catch {
    clipboard = null;
  }
  return clipboard;
}

/** Biblioteca guardada en el proyecto: solo las poses bien formadas */
export function loadPoseLibrary(raw: unknown): StoredPose[] {
  if (!Array.isArray(raw)) return [];
  return raw.filter((p) => p && typeof p.id === "string" && typeof p.name === "string" && p.bones && typeof p.bones === "object");
}
