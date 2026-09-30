/**
 * Retargeting (F7): pasar un movimiento de un esqueleto a otro.
 *
 * El origen es un movimiento cualquiera (un BVH, otro rig, una captura):
 * posiciones de sus articulaciones por cuadro y giro de su raíz. Se mapea
 * cada articulación del modelo a una del origen, por nombre (con sinónimos
 * de Mixamo, Blender, BVH y los propios) y, lo que falte, por cadenas del
 * cuerpo (`analyzeBody`: patas con patas, cola con cola, del mismo lado).
 *
 * Por cuadro, cada articulación del modelo gira para que su hueso apunte
 * hacia donde apunta el del origen (con dos hijos, también el giro sobre el
 * eje: torso, cadera). Así las proporciones y la pose de reposo pueden ser
 * distintas (pose T contra pose A). La raíz copia el giro del origen y su
 * desplazamiento escalado por la altura.
 */

import * as THREE from "three";
import type { AnimationClip, BoneTrack, Key, Quat, Vec3 } from "./animation";
import { slerp } from "./animation";
import { Fk, rotationBetween, rotationBetweenFrames } from "./ik";
import { analyzeBody, type Body, type SkeletonBone } from "./presetAnimations";
import { quatAngle, quatInverse, quatMultiply, reduceKeys } from "./rig";

/** Movimiento de origen */
export interface SourceMotion {
  /** Esqueleto en reposo */
  bones: SkeletonBone[];
  fps: number;
  /** Posición de cada articulación en cada cuadro */
  positions: Vec3[][];
  /** Giro de la raíz en cada cuadro (respecto de su reposo) */
  rootRotation: Quat[];
}

/** Articulación del modelo → articulación del origen */
export type RetargetMap = Map<number, number>;

// ─── Mapeo por nombre ───────────────────────────────────────────────────────

interface NameInfo {
  side: -1 | 0 | 1;
  /** Lugar de la articulación: pelvis, spine1, shoulder, elbow, wrist… */
  key: string;
}

/** Lado y palabras del nombre, sin prefijos (`mixamorig:`) ni separadores */
function splitName(name: string): { side: -1 | 0 | 1; word: string } {
  let n = name.replace(/^.*[:|]/, "");
  let side: -1 | 0 | 1 = 0;
  const words = n.replace(/([a-z])([A-Z])/g, "$1 $2").toLowerCase();
  if (/\bleft\b|^left|left$/.test(words.replace(/[._-]/g, " "))) side = -1;
  else if (/\bright\b|^right|right$/.test(words.replace(/[._-]/g, " "))) side = 1;
  else if (/(^|[._\-\s])l($|[._\-\s])/i.test(n) || /[._-]l$/i.test(n) || /^l[._-]/i.test(n)) side = -1;
  else if (/(^|[._\-\s])r($|[._\-\s])/i.test(n) || /[._-]r$/i.test(n) || /^r[._-]/i.test(n)) side = 1;
  n = words
    .replace(/left|right/g, "")
    .replace(/(^|[._\-\s])[lr](?=$|[._\-\s])/g, "$1")
    .replace(/[^a-z0-9]/g, "");
  return { side, word: n };
}

const SYNONYMS: [RegExp, string][] = [
  [/^(hips?|pelvis|root)$/, "pelvis"],
  [/^(clavicle|collar(bone)?)$/, "clavicle"],
  [/^(upperarm|arm|humerus)$/, "shoulder"],
  [/^(forearm|lowerarm|elbow)$/, "elbow"],
  [/^(upleg|upperleg|thigh|femur)$/, "hip"],
  [/^(leg|lowerleg|shin|calf|knee|tibia)$/, "knee"],
  [/^(toebase|toes?|ball)$/, "toe"],
  [/^(headtop|headend|headtopend)$/, "headend"],
];

/** Nombres de todo un esqueleto pasados a lugares de articulación */
export function canonicalNames(bones: SkeletonBone[]): NameInfo[] {
  const raw = bones.map((b) => splitName(b.name));
  const has = (side: number, word: string) => raw.some((r) => r.side === side && r.word === word);
  return raw.map(({ side, word }) => {
    // Las puntas del BVH (`<padre>_end`) quedan como "<lugar>end" y solo calzan con otras puntas
    let key = word;
    for (const [re, to] of SYNONYMS) if (re.test(key)) key = to;
    // Mixamo: "Shoulder" es la clavícula si también hay "Arm"
    if (word === "shoulder" && (has(side, "arm") || has(side, "upperarm"))) key = "clavicle";
    // "Hand" es la muñeca, salvo que haya "Wrist" (entonces es la punta)
    if (word === "hand") key = has(side, "wrist") ? "handend" : "wrist";
    // "Foot" es el tobillo, salvo que haya "Ankle" (entonces es la punta)
    if (word === "foot") key = has(side, "ankle") ? "toe" : "ankle";
    // "Hip" con lado es la cadera de la pierna; sin lado, la pelvis
    if (word === "hip" && side === 0) key = "pelvis";
    return { side, key };
  });
}

/** Mapeo automático: por nombre y, lo que falte, por cadenas del cuerpo */
export function autoMap(target: SkeletonBone[], source: SkeletonBone[]): RetargetMap {
  const map: RetargetMap = new Map();
  const t = canonicalNames(target);
  const s = canonicalNames(source);
  const used = new Set<number>();
  t.forEach((info, j) => {
    const k = s.findIndex((x, i) => !used.has(i) && x.key === info.key && x.side === info.side);
    if (k >= 0) {
      map.set(j, k);
      used.add(k);
    }
  });
  const tb = analyzeBody(target);
  const sb = analyzeBody(source);
  if (tb && sb) {
    if (!map.has(tb.root)) map.set(tb.root, sb.root);
    mapChains(map, tb, sb);
  }
  return map;
}

/** Cadenas del mismo tipo y lado, en orden de adelante hacia atrás, articulación a articulación */
function mapChains(map: RetargetMap, tb: Body, sb: Body): void {
  const pairs: [number[], number[]][] = [[tb.spine, sb.spine], [tb.neck, sb.neck]];
  const groups = (b: Body) => {
    const out = new Map<string, number[][]>();
    for (const c of [...b.chains].sort((x, y) => y.along - x.along)) {
      const key = `${c.kind}:${Math.sign(c.side)}`;
      if (!out.has(key)) out.set(key, []);
      out.get(key)!.push(c.joints);
    }
    return out;
  };
  const tg = groups(tb);
  const sg = groups(sb);
  for (const [key, chains] of tg) {
    const other = sg.get(key) ?? [];
    chains.forEach((c, i) => other[i] && pairs.push([c, other[i]]));
  }
  for (const [tc, sc] of pairs) {
    if (tc.length === 0 || sc.length === 0) continue;
    tc.forEach((j, i) => {
      if (map.has(j)) return;
      const k = tc.length === 1 ? 0 : Math.round((i * (sc.length - 1)) / (tc.length - 1));
      map.set(j, sc[k]);
    });
  }
}

// ─── Retargeting ────────────────────────────────────────────────────────────

const v3 = (v: Vec3) => new THREE.Vector3(v[0], v[1], v[2]);
const toV = (v: THREE.Vector3): Vec3 => [v.x, v.y, v.z];
const fq = (q: THREE.Quaternion): Quat => [q.x, q.y, q.z, q.w];

export interface RetargetOptions {
  /** Desplazar la raíz (si no, queda en su lugar: animación "en el sitio") */
  rootMotion?: boolean;
  /** Primer y último cuadro del origen a usar */
  range?: [number, number];
  name?: string;
}

/**
 * Clip del modelo que reproduce `motion` con el mapeo `map`. Una key por
 * cuadro, reducida después (se ve igual con menos keys).
 */
export function retargetClip(target: SkeletonBone[], motion: SourceMotion, map: RetargetMap, options: RetargetOptions = {}): AnimationClip {
  const tb = analyzeBody(target);
  const sb = analyzeBody(motion.bones);
  // De los ejes del origen a los del modelo (arriba y adelante)
  const align =
    tb && sb
      ? new THREE.Quaternion(...rotationBetweenFrames(sb.up, sb.forward, tb.up, tb.forward))
      : new THREE.Quaternion();
  const scale = tb && sb && sb.height > 1e-9 ? tb.height / sb.height : 1;
  const tRoot = tb?.root ?? target.findIndex((b) => b.parent === null);
  const sRoot = map.get(tRoot) ?? sb?.root ?? motion.bones.findIndex((b) => b.parent === null);

  const children = target.map(() => [] as number[]);
  target.forEach((b, i) => b.parent !== null && children[b.parent]?.push(i));
  // Descendiente mapeado más cercano por cada hijo (siguiendo tramos sin ramificar)
  const mappedBelow = (c: number): number | undefined => {
    for (let j: number | undefined = c, depth = 0; j !== undefined && depth < 4; depth++) {
      if (map.has(j)) return j;
      j = children[j].length === 1 ? children[j][0] : undefined;
    }
    return undefined;
  };
  const sides = canonicalNames(target).map((n) => n.side);
  // Para cada articulación que gira: uno o dos hijos que dan su dirección
  const aims = target.map((_, j) => {
    if (!map.has(j)) return [] as number[];
    const kids = children[j].flatMap((c) => {
      const d = mappedBelow(c);
      return d !== undefined && map.get(d) !== map.get(j) ? [d] : [];
    });
    // Primero el del centro (columna), después el de un costado
    kids.sort((a, b) => Math.abs(sides[a]) - Math.abs(sides[b]));
    return kids.slice(0, 2);
  });

  const [first, last] = options.range ?? [0, motion.positions.length - 1];
  const rest = motion.bones.map((b) => v3(b.position));
  const rotations = new Map<number, Key<Quat>[]>();
  const translations: Key<Vec3>[] = [];
  const pose = { rotations: new Map<number, Quat>(), translations: new Map<number, Vec3>(), controls: new Map() };
  const fk = new Fk(target, pose);
  const order = fk.order;

  for (let f = first; f <= last; f++) {
    const src = motion.positions[f];
    if (!src) break;
    pose.rotations.clear();
    pose.translations.clear();
    // Raíz: giro del origen pasado a los ejes del modelo
    const R = new THREE.Quaternion(...motion.rootRotation[f]);
    pose.rotations.set(tRoot, fq(align.clone().multiply(R).multiply(align.clone().invert()).normalize()));
    if (options.rootMotion !== false && sRoot >= 0) {
      const d = v3(src[sRoot]).sub(rest[sRoot]).applyQuaternion(align).multiplyScalar(scale);
      pose.translations.set(tRoot, toV(d));
    }
    fk.update();
    // Resto: cada hueso apunta como el del origen
    for (const j of order) {
      if (j === tRoot) continue;
      const aim = aims[j];
      if (aim.length === 0) continue;
      const s = map.get(j)!;
      const want = (c: number) => v3(src[map.get(c)!]).sub(v3(src[s])).applyQuaternion(align);
      const have = (c: number) => v3(fk.position(c)).sub(v3(fk.position(j)));
      const w0 = want(aim[0]);
      const h0 = have(aim[0]);
      if (w0.lengthSq() < 1e-12 || h0.lengthSq() < 1e-12) continue;
      const delta =
        aim.length === 2
          ? rotationBetweenFrames(toV(h0), toV(have(aim[1])), toV(w0), toV(want(aim[1])))
          : rotationBetween(toV(h0), toV(w0));
      fk.rotate(j, delta);
    }
    const frame = f - first;
    for (const [j, q] of pose.rotations) {
      if (!rotations.has(j)) rotations.set(j, []);
      rotations.get(j)!.push({ frame, value: q, interpolation: "linear" });
    }
    const t = pose.translations.get(tRoot);
    if (t) translations.push({ frame, value: t, interpolation: "linear" });
  }

  const size = tb?.size ?? 1;
  const tracks: BoneTrack[] = [...rotations.keys()]
    .sort((a, b) => a - b)
    .map((j) => ({
      bone: target[j].name,
      rotation: reduceKeys(rotations.get(j)!, (a, b) => quatAngle(quatMultiply(quatInverse(a), b)) < 2e-3, slerp),
      translation:
        j === tRoot
          ? reduceKeys(translations, (a, b) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]) < 1e-4 * size, (a, b, t) => [
              a[0] + (b[0] - a[0]) * t,
              a[1] + (b[1] - a[1]) * t,
              a[2] + (b[2] - a[2]) * t,
            ])
          : [],
    }));
  const frames = Math.max(0, last - first);
  return {
    id: `clip-${Date.now().toString(36)}`,
    name: options.name ?? "Animación importada",
    fps: motion.fps,
    start: 0,
    end: frames,
    tracks,
  };
}

/**
 * Movimiento de origen a partir de un clip de otro esqueleto de la app
 * (FK de sus keys): para pasar animaciones entre modelos.
 */
export function clipMotion(bones: SkeletonBone[], clip: AnimationClip, evaluate: (frame: number) => { rotations: Map<number, Quat>; translations: Map<number, Vec3> }): SourceMotion {
  const positions: Vec3[][] = [];
  const rootRotation: Quat[] = [];
  const root = bones.findIndex((b) => b.parent === null);
  for (let f = Math.round(clip.start); f <= Math.round(clip.end); f++) {
    const pose = { ...evaluate(f), controls: new Map() };
    const fk = new Fk(bones, pose);
    positions.push(bones.map((_, j) => fk.position(j)));
    rootRotation.push(pose.rotations.get(root) ?? [0, 0, 0, 1]);
  }
  return { bones, fps: clip.fps, positions, rootRotation };
}
