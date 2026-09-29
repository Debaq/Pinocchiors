/**
 * Animaciones de la línea de tiempo: keys por articulación, en cuadros.
 *
 * Se anima cada articulación (esfera del visor): su giro mueve todo lo que
 * cuelga de ella, y la raíz además se desplaza. Las pistas se guardan por
 * nombre de hueso, así sobreviven a editar el esqueleto (mover articulaciones)
 * y las de huesos que ya no existen se ignoran. El backend recibe los clips
 * con el índice de cada articulación (ver `apps/desktop/src/animation.rs`).
 */

export type KeyInterpolation = "linear" | "step";
export type Quat = [number, number, number, number];
export type Vec3 = [number, number, number];

export interface Key<T> {
  frame: number;
  value: T;
  /** Cómo se llega desde esta key a la siguiente */
  interpolation: KeyInterpolation;
}

export interface BoneTrack {
  bone: string;
  /** Giro local (x, y, z, w) */
  rotation: Key<Quat>[];
  /** Solo la raíz: desplazamiento respecto del reposo */
  translation: Key<Vec3>[];
}

export interface AnimationClip {
  id: string;
  name: string;
  fps: number;
  /** Rango que se reproduce, en cuadros (inclusive) */
  start: number;
  end: number;
  tracks: BoneTrack[];
}

/** Pose de las articulaciones, por índice; las que faltan están en reposo */
export interface Pose {
  rotations: Map<number, Quat>;
  translations: Map<number, Vec3>;
}

export const emptyPose = (): Pose => ({ rotations: new Map(), translations: new Map() });

let nextId = 0;
export function createClip(name: string, fps = 24): AnimationClip {
  nextId += 1;
  return { id: `${Date.now().toString(36)}-${nextId}`, name, fps, start: 0, end: 2 * fps, tracks: [] };
}

export function duplicateClip(clip: AnimationClip, name: string): AnimationClip {
  return { ...structuredClone(clip), id: createClip(name).id, name };
}

// ─── Muestreo ───────────────────────────────────────────────────────────────

function slerp(a: Quat, b: Quat, t: number): Quat {
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

/** Valor de una pista en `frame` (las keys están ordenadas); fuera del rango, la key más cercana */
function sample<T>(keys: Key<T>[], frame: number, mix: (a: T, b: T, t: number) => T): T | undefined {
  if (keys.length === 0) return undefined;
  if (frame <= keys[0].frame) return keys[0].value;
  for (let i = 0; i < keys.length - 1; i++) {
    const a = keys[i];
    const b = keys[i + 1];
    if (frame < b.frame) {
      return a.interpolation === "step" ? a.value : mix(a.value, b.value, (frame - a.frame) / (b.frame - a.frame));
    }
  }
  return keys[keys.length - 1].value;
}

export function samplePose(clip: AnimationClip | undefined, frame: number, boneIndex: Map<string, number>): Pose {
  const pose = emptyPose();
  for (const track of clip?.tracks ?? []) {
    const joint = boneIndex.get(track.bone);
    if (joint === undefined) continue;
    const rotation = sample(track.rotation, frame, slerp);
    if (rotation) pose.rotations.set(joint, rotation);
    const translation = sample(track.translation, frame, lerp3);
    if (translation) pose.translations.set(joint, translation);
  }
  return pose;
}

// ─── Edición de keys ────────────────────────────────────────────────────────

/** Referencia a las keys de un hueso en un cuadro (el rombo de la línea de tiempo) */
export const keyId = (bone: string, frame: number) => `${bone}\u0000${frame}`;
export function parseKeyId(id: string): { bone: string; frame: number } {
  const [bone, frame] = id.split("\u0000");
  return { bone, frame: Number(frame) };
}

function withKey<T>(keys: Key<T>[], key: Key<T>): Key<T>[] {
  return [...keys.filter((k) => k.frame !== key.frame), key].sort((a, b) => a.frame - b.frame);
}

/**
 * Inserta keys en `frame` con el giro (y desplazamiento, si se da) de cada
 * hueso. Una key existente en ese cuadro se reemplaza.
 */
export function insertKeys(
  clip: AnimationClip,
  frame: number,
  entries: { bone: string; rotation: Quat; translation?: Vec3 }[],
  interpolation: KeyInterpolation
): AnimationClip {
  const tracks = clip.tracks.map((t) => ({ ...t }));
  for (const entry of entries) {
    let track = tracks.find((t) => t.bone === entry.bone);
    if (!track) {
      track = { bone: entry.bone, rotation: [], translation: [] };
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
      return { ...track, rotation: edit(track.rotation, selected), translation: edit(track.translation, selected) };
    })
    .filter((t) => t.rotation.length > 0 || t.translation.length > 0);
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

/** Cuadros con keys de un hueso (rotación o desplazamiento), ordenados */
export function keyFrames(track: BoneTrack | undefined): number[] {
  if (!track) return [];
  return [...new Set([...track.rotation, ...track.translation].map((k) => k.frame))].sort((a, b) => a - b);
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
      const joint = boneIndex.get(t.bone);
      return joint === undefined ? [] : [{ joint, rotation: t.rotation, translation: t.translation }];
    }),
  }));
}
