/**
 * Animaciones de la línea de tiempo: keys por articulación, en cuadros.
 *
 * Se anima cada articulación (esfera del visor): su giro mueve todo lo que
 * cuelga de ella, y cualquiera puede además desplazarse (salvo que su hueso
 * lo bloquee, ver `rig.ts`). Los controles del rig tienen pistas propias
 * (`kind: "control"`, con el id del control). Las pistas se guardan por
 * nombre de hueso, así sobreviven a editar el esqueleto (mover articulaciones)
 * y las de huesos que ya no existen se ignoran. El backend recibe los clips
 * con el índice de cada articulación (ver `apps/desktop/src/animation.rs`).
 */

import {
  bezierSegment,
  defaultRotationCodecs,
  scalarCodec,
  vec3Codec,
  type Codec,
  type Handle,
  type HandleMode,
  type Marker,
  type RotationCodecs,
} from "./curves";

export type KeyInterpolation = "linear" | "step" | "bezier";
export type Quat = [number, number, number, number];
export type Vec3 = [number, number, number];

export interface Key<T> {
  frame: number;
  value: T;
  /** Cómo se llega desde esta key a la siguiente */
  interpolation: KeyInterpolation;
  /** Bézier: tipo de manija (por defecto automática) y las guardadas (alineada, libre) */
  handleMode?: HandleMode;
  handleIn?: Handle;
  handleOut?: Handle;
  /** Llega suave (manija de entrada plana) / sale suave (manija de salida plana) */
  easeIn?: boolean;
  easeOut?: boolean;
}

export interface BoneTrack {
  /** Nombre del hueso, id del control (`kind` "control") o de la cadena IK (`kind` "ik") */
  bone: string;
  kind?: "control" | "ik";
  /** Giro local (x, y, z, w) */
  rotation: Key<Quat>[];
  /** Desplazamiento respecto del reposo, en el marco del padre */
  translation: Key<Vec3>[];
  /** Cadenas IK: mezcla IK/FK (0–1), fijado (0/1, escalonado) y balanceo del pie (grados) */
  blend?: Key<number>[];
  pin?: Key<number>[];
  roll?: Key<number>[];
}

export type ScalarChannel = "blend" | "pin" | "roll";
export const SCALAR_CHANNELS: ScalarChannel[] = ["blend", "pin", "roll"];

export interface AnimationClip {
  id: string;
  name: string;
  fps: number;
  /** Rango que se reproduce, en cuadros (inclusive) */
  start: number;
  end: number;
  tracks: BoneTrack[];
  /** Marcadores con nombre en la línea de tiempo */
  markers?: Marker[];
  /** Se repite: el último cuadro empalma con el primero (manijas automáticas de las puntas) */
  cyclic?: boolean;
}

/** Giro y desplazamiento de un control respecto de su reposo */
export interface ControlPose {
  rotation: Quat;
  translation: Vec3;
}

/** Pose de las articulaciones, por índice, y de los controles, por id; lo que falta está en reposo */
export interface Pose {
  rotations: Map<number, Quat>;
  translations: Map<number, Vec3>;
  controls: Map<string, ControlPose>;
}

export const emptyPose = (): Pose => ({ rotations: new Map(), translations: new Map(), controls: new Map() });

export const clonePose = (pose: Pose): Pose => ({
  rotations: new Map(pose.rotations),
  translations: new Map(pose.translations),
  controls: new Map(pose.controls),
});

let nextId = 0;
export function createClip(name: string, fps = 24): AnimationClip {
  nextId += 1;
  return { id: `${Date.now().toString(36)}-${nextId}`, name, fps, start: 0, end: 2 * fps, tracks: [] };
}

export function duplicateClip(clip: AnimationClip, name: string): AnimationClip {
  return { ...structuredClone(clip), id: createClip(name).id, name };
}

// ─── Muestreo ───────────────────────────────────────────────────────────────

export function slerp(a: Quat, b: Quat, t: number): Quat {
  let [bx, by, bz, bw] = b;
  let dot = a[0] * bx + a[1] * by + a[2] * bz + a[3] * bw;
  // El camino corto: q y -q son el mismo giro
  if (dot < 0) {
    [bx, by, bz, bw] = [-bx, -by, -bz, -bw];
    dot = -dot;
  }
  let wa: number;
  let wb: number;
  if (dot > 0.9995) {
    wa = 1 - t;
    wb = t;
  } else {
    const theta = Math.acos(dot);
    const s = Math.sin(theta);
    wa = Math.sin((1 - t) * theta) / s;
    wb = Math.sin(t * theta) / s;
  }
  const q: Quat = [wa * a[0] + wb * bx, wa * a[1] + wb * by, wa * a[2] + wb * bz, wa * a[3] + wb * bw];
  const len = Math.hypot(...q) || 1;
  return [q[0] / len, q[1] / len, q[2] / len, q[3] / len];
}

const lerp3 = (a: Vec3, b: Vec3, t: number): Vec3 => [
  a[0] + (b[0] - a[0]) * t,
  a[1] + (b[1] - a[1]) * t,
  a[2] + (b[2] - a[2]) * t,
];

/**
 * Valor de una pista en `frame` (las keys están ordenadas); fuera del rango,
 * la key más cercana. Los tramos Bézier se interpolan por canales (`codec`)
 */
function sample<T>(keys: Key<T>[], frame: number, mix: (a: T, b: T, t: number) => T, codec?: Codec<T>, cycle?: number): T | undefined {
  if (keys.length === 0) return undefined;
  if (frame <= keys[0].frame) return keys[0].value;
  for (let i = 0; i < keys.length - 1; i++) {
    const a = keys[i];
    const b = keys[i + 1];
    if (frame < b.frame) {
      if (a.interpolation === "step") return a.value;
      if (a.interpolation === "bezier" && codec) return bezierSegment(keys, i, frame, codec, cycle);
      return mix(a.value, b.value, (frame - a.frame) / (b.frame - a.frame));
    }
  }
  return keys[keys.length - 1].value;
}

/**
 * Pose del clip en `frame`. `rotation` dice cómo se interpolan los giros en
 * los tramos Bézier (ángulos de Euler en los ejes de cada articulación, ver
 * `rig.ts`); sin él, en los ejes del modelo
 */
export function samplePose(
  clip: AnimationClip | undefined,
  frame: number,
  boneIndex: Map<string, number>,
  rotationCodecs: RotationCodecs = defaultRotationCodecs
): Pose {
  const pose = emptyPose();
  const cycle = clip?.cyclic ? clip.end - clip.start : undefined;
  for (const track of clip?.tracks ?? []) {
    if (track.kind === "ik") continue;
    const rot = (keys: Key<Quat>[]): Quat | undefined => sample(keys, frame, slerp, rotationCodecs(track), cycle);
    const pos = (keys: Key<Vec3>[]): Vec3 | undefined => sample(keys, frame, lerp3, vec3Codec, cycle);
    if (track.kind === "control") {
      const rotation = rot(track.rotation);
      const translation = pos(track.translation);
      if (rotation || translation) {
        pose.controls.set(track.bone, { rotation: rotation ?? [0, 0, 0, 1], translation: translation ?? [0, 0, 0] });
      }
      continue;
    }
    const joint = boneIndex.get(track.bone);
    if (joint === undefined) continue;
    const rotation = rot(track.rotation);
    if (rotation) pose.rotations.set(joint, rotation);
    const translation = pos(track.translation);
    if (translation) pose.translations.set(joint, translation);
  }
  return pose;
}

/** Valor de un canal escalar de la cadena IK `chain` en `frame` (`undefined` sin keys) */
export function sampleScalar(clip: AnimationClip | undefined, chain: string, channel: ScalarChannel, frame: number): number | undefined {
  const keys = clip?.tracks.find((t) => t.kind === "ik" && t.bone === chain)?.[channel];
  if (!keys || keys.length === 0) return undefined;
  if (channel === "pin") {
    // Fijar es sí o no: la key vale hasta la siguiente
    let value = keys[0].value;
    for (const k of keys) if (k.frame <= frame) value = k.value;
    return value;
  }
  return sample(keys, frame, (a, b, t) => a + (b - a) * t, scalarCodec, clip?.cyclic ? clip.end - clip.start : undefined);
}

/**
 * Cuadro en que empezó el fijado que sigue activo en `frame`: la primera de
 * las keys seguidas que fijan (`undefined` si en `frame` no está fijado)
 */
export function pinStart(clip: AnimationClip | undefined, chain: string, frame: number): number | undefined {
  const keys = clip?.tracks.find((t) => t.kind === "ik" && t.bone === chain)?.pin ?? [];
  let i = -1;
  keys.forEach((k, n) => k.frame <= frame && (i = n));
  if (i < 0 || keys[i].value < 0.5) return undefined;
  while (i > 0 && keys[i - 1].value >= 0.5) i--;
  return keys[i].frame;
}

/** Pone una key de un canal escalar de la cadena en `frame` */
export function insertScalarKey(
  clip: AnimationClip,
  chain: string,
  channel: ScalarChannel,
  frame: number,
  value: number,
  interpolation: KeyInterpolation
): AnimationClip {
  const tracks = clip.tracks.map((t) => ({ ...t }));
  let track = tracks.find((t) => t.kind === "ik" && t.bone === chain);
  if (!track) {
    track = { bone: chain, kind: "ik", rotation: [], translation: [] };
    tracks.push(track);
  }
  const step = channel === "pin" ? "step" : interpolation;
  track[channel] = withKey(track[channel] ?? [], { frame, value, interpolation: step });
  return { ...clip, tracks };
}

// ─── Edición de keys ────────────────────────────────────────────────────────

/** Referencia a las keys de un hueso en un cuadro (el rombo de la línea de tiempo) */
export const keyId = (bone: string, frame: number) => `${bone}\u0000${frame}`;
export function parseKeyId(id: string): { bone: string; frame: number } {
  const [bone, frame] = id.split("\u0000");
  return { bone, frame: Number(frame) };
}

/** Pone `key` en su cuadro; si ya había una, conserva sus manijas y su aceleración */
function withKey<T>(keys: Key<T>[], key: Key<T>): Key<T>[] {
  const old = keys.find((k) => k.frame === key.frame);
  const merged: Key<T> = old
    ? { ...key, handleMode: old.handleMode, easeIn: old.easeIn, easeOut: old.easeOut, handleIn: old.handleIn, handleOut: old.handleOut }
    : key;
  for (const f of ["handleMode", "easeIn", "easeOut", "handleIn", "handleOut"] as const) if (merged[f] === undefined) delete merged[f];
  return [...keys.filter((k) => k.frame !== key.frame), merged].sort((a, b) => a.frame - b.frame);
}

/**
 * Inserta keys en `frame` con el giro (y desplazamiento, si se da) de cada
 * hueso. Una key existente en ese cuadro se reemplaza.
 */
export function insertKeys(
  clip: AnimationClip,
  frame: number,
  entries: { bone: string; kind?: "control"; rotation: Quat; translation?: Vec3 }[],
  interpolation: KeyInterpolation
): AnimationClip {
  const tracks = clip.tracks.map((t) => ({ ...t }));
  for (const entry of entries) {
    let track = tracks.find((t) => t.bone === entry.bone && t.kind === entry.kind);
    if (!track) {
      track = { bone: entry.bone, rotation: [], translation: [] };
      if (entry.kind) track.kind = entry.kind;
      tracks.push(track);
    }
    track.rotation = withKey(track.rotation, { frame, value: entry.rotation, interpolation });
    if (entry.translation) {
      track.translation = withKey(track.translation, { frame, value: entry.translation, interpolation });
    }
  }
  return { ...clip, tracks };
}

/** Aplica `edit` a las keys seleccionadas de cada pista (rotación y desplazamiento juntos) */
function editKeys(
  clip: AnimationClip,
  selection: Set<string>,
  edit: <T>(keys: Key<T>[], selected: (k: Key<T>) => boolean) => Key<T>[]
): AnimationClip {
  const tracks = clip.tracks
    .map((track) => {
      const selected = <T,>(k: Key<T>) => selection.has(keyId(track.bone, k.frame));
      const next: BoneTrack = { ...track, rotation: edit(track.rotation, selected), translation: edit(track.translation, selected) };
      for (const c of SCALAR_CHANNELS) if (track[c]) next[c] = edit(track[c]!, selected);
      return next;
    })
    .filter((t) => trackKeys(t).length > 0);
  return { ...clip, tracks };
}

export function deleteKeys(clip: AnimationClip, selection: Set<string>): AnimationClip {
  return editKeys(clip, selection, (keys, selected) => keys.filter((k) => !selected(k)));
}

/** Corre las keys seleccionadas `delta` cuadros; pisan a las que ya estaban en el destino */
export function moveKeys(clip: AnimationClip, selection: Set<string>, delta: number): AnimationClip {
  if (delta === 0) return clip;
  return editKeys(clip, selection, (keys, selected) => {
    const moved = keys.filter(selected).map((k) => ({ ...k, frame: k.frame + delta }));
    const targets = new Set(moved.map((k) => k.frame));
    const kept = keys.filter((k) => !selected(k) && !targets.has(k.frame));
    return [...kept, ...moved].sort((a, b) => a.frame - b.frame);
  });
}

export function setKeysInterpolation(
  clip: AnimationClip,
  selection: Set<string>,
  interpolation: KeyInterpolation
): AnimationClip {
  return editKeys(clip, selection, (keys, selected) =>
    keys.map((k) => (selected(k) ? { ...k, interpolation } : k))
  );
}

/** Todas las keys de una pista (giro, desplazamiento y canales escalares) */
function trackKeys(track: BoneTrack): Key<unknown>[] {
  return [...track.rotation, ...track.translation, ...SCALAR_CHANNELS.flatMap((c) => track[c] ?? [])];
}

/** Cuadros con keys de una pista, ordenados */
export function keyFrames(track: BoneTrack | undefined): number[] {
  if (!track) return [];
  return [...new Set(trackKeys(track).map((k) => k.frame))].sort((a, b) => a - b);
}

/** Keys seleccionadas de todos los huesos en esos cuadros (el resumen de la línea de tiempo) */
export function keysAtFrames(clip: AnimationClip, frames: Set<number>): string[] {
  return clip.tracks.flatMap((t) => keyFrames(t).filter((f) => frames.has(f)).map((f) => keyId(t.bone, f)));
}

// ─── Backend ────────────────────────────────────────────────────────────────

/** Clips para exportar: índice de articulación en vez de nombre */
export function clipsForExport(clips: AnimationClip[], boneIndex: Map<string, number>) {
  return clips.map((clip) => ({
    name: clip.name,
    fps: clip.fps,
    start: clip.start,
    end: clip.end,
    tracks: clip.tracks.flatMap((t) => {
      if (t.kind) return [];
      const joint = boneIndex.get(t.bone);
      return joint === undefined ? [] : [{ joint, rotation: t.rotation, translation: t.translation }];
    }),
  }));
}
