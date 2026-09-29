/**
 * Curvas de animación: interpolación Bézier por canal y la edición de keys
 * del editor de curvas (manijas, aceleración/freno, filtros, retiempo).
 *
 * Una key guarda su valor (giro, desplazamiento o número) y cómo sigue hasta
 * la siguiente. Con Bézier, el tramo se interpola por **canales**: los giros
 * como ángulos de Euler (grados) en los ejes de la articulación, con el orden
 * de su modo de rotación (ver `rig.ts`); los desplazamientos por x, y, z; los
 * números tal cual. Los ángulos se "desenrollan" de key a key (el filtro
 * Euler): de dos ángulos equivalentes se usa el más cercano al anterior, así
 * no hay saltos de ±180°.
 *
 * Las manijas son desplazamientos (en cuadros y en unidades del canal) desde
 * la key. Automáticas: salen de la pendiente entre las vecinas (recortadas en
 * los extremos, para no pasarse); vector: apuntan a la vecina; alineadas y
 * libres: las guardadas. Acelerar/frenar deja la manija plana de ese lado.
 */

import * as THREE from "three";
import type { AnimationClip, BoneTrack, Key, KeyInterpolation, Quat, ScalarChannel, Vec3 } from "./animation";

export type HandleMode = "autoClamped" | "auto" | "vector" | "aligned" | "free";

export const HANDLE_MODES: { value: HandleMode; label: string }[] = [
  { value: "autoClamped", label: "Automática" },
  { value: "auto", label: "Automática libre" },
  { value: "vector", label: "Vector" },
  { value: "aligned", label: "Alineada" },
  { value: "free", label: "Libre" },
];

/** Manija: por canal, cuadros y valor desde la key */
export interface Handle {
  dt: number[];
  dv: number[];
}

/** Traducción de los valores de una pista a canales (y de vuelta) */
export interface Codec<T> {
  id: string;
  to: (value: T) => number[];
  from: (channels: number[]) => T;
  /** Canales angulares (grados): se desenrollan */
  angular: boolean;
}

const deg = THREE.MathUtils.radToDeg;
const rad = THREE.MathUtils.degToRad;

/** Giros como Euler XYZ en ejes del modelo (controles, o sin rig) */
export const eulerCodec = (order: THREE.EulerOrder = "XYZ", id = `euler:${order}`, frame?: { toLocal: (q: Quat) => Quat; toModel: (q: Quat) => Quat }): Codec<Quat> => ({
  id,
  angular: true,
  to: (q) => {
    const local = frame ? frame.toLocal(q) : q;
    const e = new THREE.Euler().setFromQuaternion(new THREE.Quaternion(...local), order);
    return [deg(e.x), deg(e.y), deg(e.z)];
  },
  from: (c) => {
    const q = new THREE.Quaternion().setFromEuler(new THREE.Euler(rad(c[0]), rad(c[1]), rad(c[2]), order));
    const out: Quat = [q.x, q.y, q.z, q.w];
    return frame ? frame.toModel(out) : out;
  },
});

export const vec3Codec: Codec<Vec3> = { id: "xyz", angular: false, to: (v) => [...v], from: (c) => [c[0], c[1], c[2]] };
export const scalarCodec: Codec<number> = { id: "n", angular: false, to: (v) => [v], from: (c) => c[0] };

/** Cómo se traducen los giros de cada pista (lo da el rig: ejes y orden de cada articulación) */
export type RotationCodecs = (track: BoneTrack) => Codec<Quat>;
export const defaultRotationCodecs: RotationCodecs = () => eulerCodec();

// ─── Desenrollado ───────────────────────────────────────────────────────────

const near = (a: number, ref: number) => a + 360 * Math.round((ref - a) / 360);

/** Ángulos de Euler equivalentes al de `e` más cercanos a `prev` (±360 y la otra solución) */
function unwrapEuler(prev: number[], e: number[]): number[] {
  const a = e.map((v, i) => near(v, prev[i]));
  const alt = [e[0] + 180, 180 - e[1], e[2] + 180].map((v, i) => near(v, prev[i]));
  const dist = (x: number[]) => x.reduce((s, v, i) => s + Math.abs(v - prev[i]), 0);
  return dist(alt) < dist(a) ? alt : a;
}

interface ChannelData {
  times: number[];
  values: number[][];
}

const cache = new WeakMap<object, Map<string, ChannelData>>();

/** Canales de todas las keys (desenrollados), guardados mientras la lista no cambie */
export function channelData<T>(keys: Key<T>[], codec: Codec<T>): ChannelData {
  let byCodec = cache.get(keys);
  if (!byCodec) {
    byCodec = new Map();
    cache.set(keys, byCodec);
  }
  const hit = byCodec.get(codec.id);
  if (hit) return hit;
  const values: number[][] = [];
  keys.forEach((k, i) => {
    const v = codec.to(k.value);
    values.push(codec.angular && i > 0 ? unwrapEuler(values[i - 1], v) : v);
  });
  const data = { times: keys.map((k) => k.frame), values };
  byCodec.set(codec.id, data);
  return data;
}

// ─── Manijas ────────────────────────────────────────────────────────────────

/**
 * Manijas efectivas de la key `i` (entrada y salida), en cuadros y valor por
 * canal. `cycle`: largo del ciclo si la animación se repite (las automáticas
 * de los extremos miran a la otra punta)
 */
export function resolveHandles<T>(keys: Key<T>[], data: ChannelData, i: number, cycle?: number): { in: Handle; out: Handle } {
  const { times, values } = data;
  const n = keys.length;
  const k = keys[i];
  const channels = values[i].length;
  // Vecinas (con el ciclo, la de la otra punta corrida un período)
  const prev = i > 0 ? { t: times[i - 1], v: values[i - 1] } : cycle && n > 2 ? { t: times[n - 2] - cycle, v: values[n - 2] } : null;
  const next = i < n - 1 ? { t: times[i + 1], v: values[i + 1] } : cycle && n > 2 ? { t: times[1] + cycle, v: values[1] } : null;
  const dtIn = prev ? (times[i] - prev.t) / 3 : next ? (next.t - times[i]) / 3 : 1;
  const dtOut = next ? (next.t - times[i]) / 3 : dtIn;
  const mode = k.handleMode ?? "autoClamped";
  const make = (dt: number, dv: number[]): Handle => ({ dt: dv.map(() => dt), dv });
  let hIn: Handle;
  let hOut: Handle;
  if ((mode === "aligned" || mode === "free") && k.handleIn && k.handleOut) {
    hIn = k.handleIn;
    hOut = k.handleOut;
  } else if (mode === "vector") {
    hIn = make(-dtIn, prev ? values[i].map((v, c) => (prev.v[c] - v) / 3) : values[i].map(() => 0));
    hOut = make(dtOut, next ? values[i].map((v, c) => (next.v[c] - v) / 3) : values[i].map(() => 0));
  } else {
    const slope = values[i].map((v, c) => {
      if (!prev && !next) return 0;
      if (!prev) return mode === "auto" ? (next!.v[c] - v) / (next!.t - times[i]) : 0;
      if (!next) return mode === "auto" ? (v - prev.v[c]) / (times[i] - prev.t) : 0;
      // Extremo local: plana, para que la curva no se pase del valor de la key
      if (mode === "autoClamped" && (v - prev.v[c]) * (next.v[c] - v) <= 0) return 0;
      return (next.v[c] - prev.v[c]) / (next.t - prev.t);
    });
    hIn = make(-dtIn, slope.map((s) => -s * dtIn));
    hOut = make(dtOut, slope.map((s) => s * dtOut));
  }
  // Acelerar / frenar: la manija de ese lado queda plana
  if (k.easeIn) hIn = { dt: hIn.dt, dv: hIn.dv.map(() => 0) };
  if (k.easeOut) hOut = { dt: hOut.dt, dv: hOut.dv.map(() => 0) };
  void channels;
  return { in: hIn, out: hOut };
}

/** Valor de un Bézier cúbico (tiempo, valor) en el cuadro `x` */
function bezierAt(t0: number, v0: number, t1: number, v1: number, out: [number, number], inn: [number, number], x: number): number {
  const span = t1 - t0;
  // Manijas dentro del tramo: así el tiempo avanza siempre
  const x1 = t0 + Math.max(0, Math.min(span, out[0]));
  const y1 = v0 + (out[0] !== 0 ? out[1] * ((x1 - t0) / out[0]) : out[1]);
  const x2 = t1 - Math.max(0, Math.min(span, -inn[0]));
  const y2 = v1 + (inn[0] !== 0 ? inn[1] * ((t1 - x2) / -inn[0]) : inn[1]);
  const at = (u: number, a: number, b: number, c: number, d: number) => {
    const m = 1 - u;
    return m * m * m * a + 3 * m * m * u * b + 3 * m * u * u * c + u * u * u * d;
  };
  let lo = 0;
  let hi = 1;
  for (let it = 0; it < 30; it++) {
    const mid = (lo + hi) / 2;
    if (at(mid, t0, x1, x2, t1) < x) lo = mid;
    else hi = mid;
  }
  return at((lo + hi) / 2, v0, y1, y2, v1);
}

/** Valor Bézier del tramo `i` → `i + 1` en `frame` */
export function bezierSegment<T>(keys: Key<T>[], i: number, frame: number, codec: Codec<T>, cycle?: number): T {
  const data = channelData(keys, codec);
  const a = resolveHandles(keys, data, i, cycle);
  const b = resolveHandles(keys, data, i + 1, cycle);
  const values = data.values[i].map((v0, c) =>
    bezierAt(data.times[i], v0, data.times[i + 1], data.values[i + 1][c], [a.out.dt[c], a.out.dv[c]], [b.in.dt[c], b.in.dv[c]], frame)
  );
  return codec.from(values);
}

// ─── Canales de una pista ───────────────────────────────────────────────────

export type ChannelGroup = "rotation" | "translation" | ScalarChannel;

export const GROUP_CHANNELS: Record<ChannelGroup, string[]> = {
  rotation: ["Giro X", "Giro Y", "Giro Z"],
  translation: ["Posición X", "Posición Y", "Posición Z"],
  blend: ["Mezcla IK"],
  pin: ["Fijado"],
  roll: ["Balanceo"],
};

/** Keys y traductor de un grupo de canales de una pista */
export function groupKeys(track: BoneTrack, group: ChannelGroup, rotation: RotationCodecs): { keys: Key<unknown>[]; codec: Codec<unknown> } {
  if (group === "rotation") return { keys: track.rotation as Key<unknown>[], codec: rotation(track) as Codec<unknown> };
  if (group === "translation") return { keys: track.translation as Key<unknown>[], codec: vec3Codec as Codec<unknown> };
  return { keys: (track[group] ?? []) as Key<unknown>[], codec: scalarCodec as Codec<unknown> };
}

/** Pista con otras keys en el grupo */
function withGroup(track: BoneTrack, group: ChannelGroup, keys: Key<unknown>[]): BoneTrack {
  return { ...track, [group]: keys } as BoneTrack;
}

/** Referencia a una key en un grupo: pista (nombre y tipo), grupo y cuadro */
export interface KeyRef {
  bone: string;
  kind?: BoneTrack["kind"];
  group: ChannelGroup;
  frame: number;
}

export const keyRefId = (r: Omit<KeyRef, "frame"> & { frame: number }) => `${r.kind ?? "b"}\u0000${r.bone}\u0000${r.group}\u0000${r.frame}`;
export function parseKeyRef(id: string): KeyRef {
  const [kind, bone, group, frame] = id.split("\u0000");
  return { kind: kind === "b" ? undefined : (kind as BoneTrack["kind"]), bone, group: group as ChannelGroup, frame: Number(frame) };
}

const sameTrack = (t: BoneTrack, r: { bone: string; kind?: BoneTrack["kind"] }) => t.bone === r.bone && t.kind === r.kind;

/** Aplica `edit` a las keys de cada grupo que tenga keys elegidas */
function editGroups(
  clip: AnimationClip,
  selection: Set<string>,
  rotation: RotationCodecs,
  edit: (keys: Key<unknown>[], selected: (k: Key<unknown>) => boolean, codec: Codec<unknown>) => Key<unknown>[]
): AnimationClip {
  const groups = new Map<string, KeyRef>();
  for (const id of selection) {
    const r = parseKeyRef(id);
    groups.set(`${r.kind}|${r.bone}|${r.group}`, r);
  }
  let tracks = clip.tracks;
  for (const r of groups.values()) {
    tracks = tracks.map((t) => {
      if (!sameTrack(t, r)) return t;
      const { keys, codec } = groupKeys(t, r.group, rotation);
      const selected = (k: Key<unknown>) => selection.has(keyRefId({ ...r, frame: k.frame }));
      return withGroup(t, r.group, edit(keys, selected, codec));
    });
  }
  return { ...clip, tracks };
}

// ─── Edición ────────────────────────────────────────────────────────────────

export function setInterpolation(clip: AnimationClip, selection: Set<string>, interpolation: KeyInterpolation, rotation: RotationCodecs): AnimationClip {
  return editGroups(clip, selection, rotation, (keys, selected) => keys.map((k) => (selected(k) ? { ...k, interpolation } : k)));
}

/** Tipo de manija; al pasar a alineada o libre se guardan las que se veían, así la curva no cambia */
export function setHandleMode(clip: AnimationClip, selection: Set<string>, mode: HandleMode, rotation: RotationCodecs): AnimationClip {
  return editGroups(clip, selection, rotation, (keys, selected, codec) => {
    const data = channelData(keys, codec);
    const cycle = clip.cyclic ? clip.end - clip.start : undefined;
    return keys.map((k, i) => {
      if (!selected(k)) return k;
      if (mode === "aligned" || mode === "free") {
        const h = resolveHandles(keys, data, i, cycle);
        return { ...k, handleMode: mode, handleIn: h.in, handleOut: h.out, interpolation: k.interpolation === "linear" ? "bezier" : k.interpolation };
      }
      const { handleIn: _i, handleOut: _o, ...rest } = k;
      return { ...rest, handleMode: mode };
    });
  });
}

/** Aceleración y freno: "in" = arranca suave (sale plana), "out" = llega suave, "both", "none" */
export function setEase(clip: AnimationClip, selection: Set<string>, ease: "in" | "out" | "both" | "none", rotation: RotationCodecs): AnimationClip {
  return editGroups(clip, selection, rotation, (keys, selected) =>
    keys.map((k) => {
      if (!selected(k)) return k;
      const { easeIn: _i, easeOut: _o, ...rest } = k;
      const out = { ...rest } as Key<unknown>;
      if (ease === "in" || ease === "both") out.easeOut = true;
      if (ease === "out" || ease === "both") out.easeIn = true;
      if (ease !== "none") out.interpolation = "bezier";
      return out;
    })
  );
}

/**
 * Cambia un canal de una key: `value` nuevo del canal y/o su manija (lado y
 * nueva posición en cuadros y valor). Tocar una manija la pasa a alineada
 * (la opuesta sigue en línea) o libre (con `free`)
 */
export function editKeyChannel(
  clip: AnimationClip,
  ref: KeyRef,
  channel: number,
  change: { value?: number; frame?: number; handle?: { side: "in" | "out"; dt: number; dv: number; free?: boolean } },
  rotation: RotationCodecs
): AnimationClip {
  const tracks = clip.tracks.map((t) => {
    if (!sameTrack(t, ref)) return t;
    const { keys, codec } = groupKeys(t, ref.group, rotation);
    const data = channelData(keys, codec);
    const i = keys.findIndex((k) => k.frame === ref.frame);
    if (i < 0) return t;
    let key: Key<unknown> = { ...keys[i] };
    if (change.value !== undefined) {
      const values = [...data.values[i]];
      values[channel] = change.value;
      key.value = codec.from(values);
    }
    if (change.handle) {
      const cycle = clip.cyclic ? clip.end - clip.start : undefined;
      const h = resolveHandles(keys, data, i, cycle);
      const hIn = { dt: [...h.in.dt], dv: [...h.in.dv] };
      const hOut = { dt: [...h.out.dt], dv: [...h.out.dv] };
      const { side, dt, dv } = change.handle;
      const mine = side === "in" ? hIn : hOut;
      const other = side === "in" ? hOut : hIn;
      mine.dt[channel] = side === "in" ? Math.min(-1e-3, dt) : Math.max(1e-3, dt);
      mine.dv[channel] = dv;
      if (!change.handle.free) {
        // Alineada: la otra manija sigue la dirección opuesta, con su largo
        const len = Math.hypot(other.dt[channel], other.dv[channel]);
        const dir = Math.hypot(mine.dt[channel], mine.dv[channel]) || 1;
        other.dt[channel] = (-mine.dt[channel] / dir) * len;
        other.dv[channel] = (-mine.dv[channel] / dir) * len;
      }
      key = { ...key, handleMode: change.handle.free ? "free" : "aligned", handleIn: hIn, handleOut: hOut, interpolation: "bezier" };
    }
    let next = keys.map((k, n) => (n === i ? key : k));
    if (change.frame !== undefined && change.frame !== ref.frame) {
      const moved = { ...key, frame: change.frame };
      next = [...next.filter((k, n) => n !== i && k.frame !== change.frame), moved].sort((a, b) => a.frame - b.frame);
    }
    return withGroup(t, ref.group, next);
  });
  return { ...clip, tracks };
}

// ─── Filtros ────────────────────────────────────────────────────────────────

/** Rehace las keys elegidas con canales nuevos (`fn` recibe los de todas, desenrollados) */
function mapChannels(
  clip: AnimationClip,
  selection: Set<string>,
  rotation: RotationCodecs,
  fn: (values: number[][], i: number, times: number[]) => number[]
): AnimationClip {
  return editGroups(clip, selection, rotation, (keys, selected, codec) => {
    const data = channelData(keys, codec);
    return keys.map((k, i) => (selected(k) ? { ...k, value: codec.from(fn(data.values, i, data.times)) } : k));
  });
}

/** Suaviza las keys elegidas hacia el promedio con sus vecinas (`strength` 0–1) */
export function smoothKeys(clip: AnimationClip, selection: Set<string>, strength: number, rotation: RotationCodecs): AnimationClip {
  return mapChannels(clip, selection, rotation, (values, i) =>
    values[i].map((v, c) => {
      const a = values[i - 1]?.[c] ?? v;
      const b = values[i + 1]?.[c] ?? v;
      return v + strength * ((a + 2 * v + b) / 4 - v);
    })
  );
}

/** Ruido parejo de ± `amplitude` (grados en giros, unidades en posiciones) en las keys elegidas */
export function noiseKeys(clip: AnimationClip, selection: Set<string>, amplitude: number, rotation: RotationCodecs, seed = 1): AnimationClip {
  let s = seed;
  const random = () => ((s = (s * 16807) % 2147483647) / 2147483647) * 2 - 1;
  return mapChannels(clip, selection, rotation, (values, i) => values[i].map((v) => v + amplitude * random()));
}

/**
 * Filtro Euler: los cuaterniones seguidos en el mismo hemisferio (q y −q son
 * el mismo giro) y los canales desenrollados guardados como están
 */
export function eulerFilter(clip: AnimationClip, selection: Set<string>, rotation: RotationCodecs): AnimationClip {
  return editGroups(clip, selection, rotation, (keys, selected, codec) => {
    if (!codec.angular) return keys;
    let prev: Quat | null = null;
    return keys.map((k) => {
      let q = k.value as Quat;
      if (prev && prev[0] * q[0] + prev[1] * q[1] + prev[2] * q[2] + prev[3] * q[3] < 0) q = [-q[0], -q[1], -q[2], -q[3]];
      prev = q;
      return selected(k) ? { ...k, value: q } : k;
    });
  });
}

/**
 * Quita las keys elegidas que no cambian la curva más que `tolerance`
 * (grados o unidades): se prueba sacar cada una y se mide contra la curva
 * original cuadro a cuadro
 */
export function reduceSelectedKeys(clip: AnimationClip, selection: Set<string>, tolerance: number, rotation: RotationCodecs): AnimationClip {
  return editGroups(clip, selection, rotation, (keys, selected, codec) => {
    if (keys.length <= 2) return keys;
    const cycle = clip.cyclic ? clip.end - clip.start : undefined;
    const first = keys[0].frame;
    const last = keys[keys.length - 1].frame;
    const original = channelData(keys, codec);
    const target: number[][] = [];
    for (let f = first; f <= last; f++) target.push(sampleChannels(keys, f, codec, original, cycle));
    let current = keys;
    for (const k of keys.slice(1, -1)) {
      if (!selected(k)) continue;
      const trial = current.filter((x) => x !== k);
      const data = channelData(trial, codec);
      let ok = true;
      for (let f = first; f <= last && ok; f++) {
        const v = sampleChannels(trial, f, codec, data, cycle);
        ok = v.every((x, c) => Math.abs(x - target[f - first][c]) <= tolerance);
      }
      if (ok) current = trial;
    }
    return current;
  });
}

/** Canales de la curva en `frame` (para medir): lineal o Bézier según cada tramo */
export function sampleChannels<T>(keys: Key<T>[], frame: number, codec: Codec<T>, data = channelData(keys, codec), cycle?: number): number[] {
  const n = keys.length;
  if (frame <= keys[0].frame) return data.values[0];
  if (frame >= keys[n - 1].frame) return data.values[n - 1];
  let i = 0;
  while (i < n - 2 && frame >= keys[i + 1].frame) i++;
  const a = keys[i];
  if (a.interpolation === "step") return data.values[i];
  if (a.interpolation === "bezier") {
    const h0 = resolveHandles(keys, data, i, cycle);
    const h1 = resolveHandles(keys, data, i + 1, cycle);
    return data.values[i].map((v0, c) =>
      bezierAt(data.times[i], v0, data.times[i + 1], data.values[i + 1][c], [h0.out.dt[c], h0.out.dv[c]], [h1.in.dt[c], h1.in.dv[c]], frame)
    );
  }
  const t = (frame - a.frame) / (keys[i + 1].frame - a.frame);
  return data.values[i].map((v, c) => v + (data.values[i + 1][c] - v) * t);
}

// ─── Tiempo ─────────────────────────────────────────────────────────────────

/** Rehace los cuadros de todas las keys del clip con `map` (redondeados; si chocan, gana la última) */
function remapFrames(clip: AnimationClip, map: (frame: number, selected: boolean, ref: Omit<KeyRef, "frame">) => number, selection?: Set<string>): AnimationClip {
  const remap = <T,>(keys: Key<T>[] | undefined, ref: Omit<KeyRef, "frame">): Key<T>[] | undefined => {
    if (!keys) return keys;
    const out = new Map<number, Key<T>>();
    for (const k of keys) {
      const sel = selection ? selection.has(keyRefId({ ...ref, frame: k.frame })) : true;
      const f = Math.round(map(k.frame, sel, ref));
      out.set(f, { ...k, frame: f });
    }
    return [...out.values()].sort((a, b) => a.frame - b.frame);
  };
  const tracks = clip.tracks.map((t) => {
    const base = { bone: t.bone, kind: t.kind };
    return {
      ...t,
      rotation: remap(t.rotation, { ...base, group: "rotation" })!,
      translation: remap(t.translation, { ...base, group: "translation" })!,
      blend: remap(t.blend, { ...base, group: "blend" }),
      pin: remap(t.pin, { ...base, group: "pin" }),
      roll: remap(t.roll, { ...base, group: "roll" }),
    };
  });
  return { ...clip, tracks };
}

/** Escala las keys elegidas alrededor del cuadro `pivot` */
export function scaleKeys(clip: AnimationClip, selection: Set<string>, pivot: number, factor: number): AnimationClip {
  return remapFrames(clip, (f, sel) => (sel ? pivot + (f - pivot) * factor : f), selection);
}

/**
 * Retiempo: el tramo [a, b] dura `factor` veces lo que duraba; lo que viene
 * después se corre lo mismo, así nada se superpone
 */
export function retime(clip: AnimationClip, a: number, b: number, factor: number): AnimationClip {
  const shift = (b - a) * (factor - 1);
  const out = remapFrames(clip, (f) => (f < a ? f : f <= b ? a + (f - a) * factor : f + shift));
  const markers = clip.markers?.map((m) => ({ ...m, frame: Math.round(m.frame < a ? m.frame : m.frame <= b ? a + (m.frame - a) * factor : m.frame + shift) }));
  return { ...out, end: Math.round(clip.end > b ? clip.end + shift : clip.end), markers };
}

/**
 * Cerrar ciclo: en las pistas dadas (todas si no), el último cuadro del clip
 * toma la key del primero; con `cyclic`, las manijas automáticas de las
 * puntas miran a la otra, así la curva sigue sin quiebre al repetirse
 */
export function closeCycle(clip: AnimationClip, tracks: Set<string> | null, rotation: RotationCodecs): AnimationClip {
  const start = Math.round(clip.start);
  const end = Math.round(clip.end);
  const close = <T,>(t: BoneTrack, group: ChannelGroup, keys: Key<T>[] | undefined): Key<T>[] | undefined => {
    if (!keys || keys.length === 0) return keys;
    const { codec } = groupKeys(t, group, rotation) as unknown as { codec: Codec<T> };
    const value = codec.from(sampleChannels(keys, start, codec));
    const first = keys.find((k) => k.frame === start) ?? { ...keys[0], frame: start, value };
    const inside = keys.filter((k) => k.frame > start && k.frame < end);
    return [first, ...inside, { ...first, frame: end }];
  };
  const out = clip.tracks.map((t) => {
    if (tracks && !tracks.has(`${t.kind ?? "b"}|${t.bone}`)) return t;
    return {
      ...t,
      rotation: close(t, "rotation", t.rotation)!,
      translation: close(t, "translation", t.translation)!,
      blend: close(t, "blend", t.blend),
      roll: close(t, "roll", t.roll),
    };
  });
  return { ...clip, tracks: out, cyclic: true };
}

// ─── Marcadores ─────────────────────────────────────────────────────────────

export interface Marker {
  frame: number;
  name: string;
}

export function withMarker(clip: AnimationClip, frame: number, name: string): AnimationClip {
  const markers = [...(clip.markers ?? []).filter((m) => m.frame !== frame), { frame, name }].sort((a, b) => a.frame - b.frame);
  return { ...clip, markers };
}

export function withoutMarker(clip: AnimationClip, frame: number): AnimationClip {
  return { ...clip, markers: (clip.markers ?? []).filter((m) => m.frame !== frame) };
}

/** Borra las keys de las referencias (cada una, todos los canales de su grupo en ese cuadro) */
export function deleteKeyRefs(clip: AnimationClip, selection: Set<string>, rotation: RotationCodecs): AnimationClip {
  const out = editGroups(clip, selection, rotation, (keys, selected) => keys.filter((k) => !selected(k)));
  const empty = (t: BoneTrack) =>
    t.rotation.length === 0 && t.translation.length === 0 && !t.blend?.length && !t.pin?.length && !t.roll?.length;
  return { ...out, tracks: out.tracks.filter((t) => !empty(t)) };
}

/**
 * Referencias (de todos los grupos con key en ese cuadro) de las keys de la
 * hoja de claves: ids `hueso\u0000cuadro`
 */
export function dopeToRefs(clip: AnimationClip, selection: Set<string>): Set<string> {
  const out = new Set<string>();
  for (const id of selection) {
    const [bone, f] = id.split("\u0000");
    const frame = Number(f);
    for (const t of clip.tracks.filter((x) => x.bone === bone)) {
      for (const group of ["rotation", "translation", "blend", "pin", "roll"] as ChannelGroup[]) {
        const keys = (group === "rotation" ? t.rotation : group === "translation" ? t.translation : t[group]) as Key<unknown>[] | undefined;
        if (keys?.some((k) => k.frame === frame)) out.add(keyRefId({ bone, kind: t.kind, group, frame }));
      }
    }
  }
  return out;
}

/** Todas las keys de las pistas dadas (o de todo el clip), como referencias */
export function allRefs(clip: AnimationClip, tracks?: Set<string>): Set<string> {
  const out = new Set<string>();
  for (const t of clip.tracks) {
    if (tracks && !tracks.has(`${t.kind ?? "b"}|${t.bone}`)) continue;
    for (const group of ["rotation", "translation", "blend", "pin", "roll"] as ChannelGroup[]) {
      const keys = (group === "rotation" ? t.rotation : group === "translation" ? t.translation : t[group]) as Key<unknown>[] | undefined;
      for (const k of keys ?? []) out.add(keyRefId({ bone: t.bone, kind: t.kind, group, frame: k.frame }));
    }
  }
  return out;
}
