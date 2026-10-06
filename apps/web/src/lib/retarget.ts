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
import type { AnimationClip, BoneTrack, Key, Pose, Quat, Vec3 } from "./animation";
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

// ─── Orientación por la geometría ───────────────────────────────────────────

/**
 * Ejes del cuerpo por su forma: adelante es hacia donde apuntan los dedos de
 * las patas (o la cabeza, si está adelante del cuerpo) y la derecha es la
 * anatómica. `chirality` compara con los nombres: +1 si "left" está a la
 * izquierda anatómica, −1 si está espejado (las plantillas propias ponen
 * `_l` en −X mirando a +Z; Mixamo, BVH y MediaPipe, en +X), 0 si no se sabe.
 * Un espejo no se corrige girando: con quiralidades distintas se cruzan los
 * lados del mapeo.
 */
export interface BodyFrame {
  up: Vec3;
  forward: Vec3;
  right: Vec3;
  chirality: -1 | 0 | 1;
}

export function bodyFrame(bones: SkeletonBone[], body: Body | null = analyzeBody(bones)): BodyFrame | null {
  if (!body) return null;
  const up = body.up;
  const flat = (v: Vec3): Vec3 => {
    const d = v[0] * up[0] + v[1] * up[1] + v[2] * up[2];
    return [v[0] - d * up[0], v[1] - d * up[1], v[2] - d * up[2]];
  };
  const add = (a: Vec3, b: Vec3): Vec3 => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
  let toes: Vec3 = [0, 0, 0];
  for (const c of body.chains) {
    if (c.kind !== "leg" || c.joints.length < 2) continue;
    const tip = bones[c.joints[c.joints.length - 1]].position;
    const before = bones[c.joints[c.joints.length - 2]].position;
    toes = add(toes, flat([tip[0] - before[0], tip[1] - before[1], tip[2] - before[2]]));
  }
  let geometric: Vec3 | null = Math.hypot(...toes) > 0.02 * body.size ? toes : null;
  if (!geometric) {
    const head = bones.findIndex((b) => /head|skull/i.test(b.name));
    if (head >= 0) {
      const d = flat([bones[head].position[0] - bones[body.root].position[0], 0, bones[head].position[2] - bones[body.root].position[2]]);
      if (Math.hypot(...d) > 0.15 * body.size) geometric = d;
    }
  }
  const unit = (v: Vec3): Vec3 => {
    const l = Math.hypot(...v) || 1;
    return [v[0] / l, v[1] / l, v[2] / l];
  };
  const forward = geometric ? unit(geometric) : body.forward;
  const cross = (a: Vec3, b: Vec3): Vec3 => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
  // Derecha anatómica: mirando hacia `forward` con `up` arriba
  const right = unit(cross(forward, up));
  // Lado que dicen los nombres: de los "left" a sus pares "right"
  const names = canonicalNames(bones);
  let lateral: Vec3 = [0, 0, 0];
  names.forEach((n, i) => {
    if (n.side !== -1) return;
    const k = names.findIndex((m) => m.side === 1 && m.key === n.key);
    if (k >= 0) lateral = add(lateral, [bones[k].position[0] - bones[i].position[0], bones[k].position[1] - bones[i].position[1], bones[k].position[2] - bones[i].position[2]]);
  });
  const agree = lateral[0] * right[0] + lateral[1] * right[1] + lateral[2] * right[2];
  const chirality = !geometric || Math.hypot(...lateral) < 1e-9 ? 0 : agree > 0 ? 1 : -1;
  return { up, forward, right, chirality };
}

/** Mapeo automático: por nombre y, lo que falte, por cadenas del cuerpo */
export function autoMap(target: SkeletonBone[], source: SkeletonBone[]): RetargetMap {
  const map: RetargetMap = new Map();
  const t = canonicalNames(target);
  const s = canonicalNames(source);
  const tb = analyzeBody(target);
  const sb = analyzeBody(source);
  const tf = bodyFrame(target, tb);
  const sf = bodyFrame(source, sb);
  // Esqueletos espejados entre sí: el "left" de uno es el "right" del otro
  const flip = tf && sf && tf.chirality !== 0 && sf.chirality !== 0 && tf.chirality !== sf.chirality ? -1 : 1;
  const used = new Set<number>();
  t.forEach((info, j) => {
    const k = s.findIndex((x, i) => !used.has(i) && x.key === info.key && x.side === info.side * flip);
    if (k >= 0) {
      map.set(j, k);
      used.add(k);
    }
  });
  if (tb && sb && tf && sf) {
    if (!map.has(tb.root)) map.set(tb.root, sb.root);
    mapChains(map, { body: tb, frame: tf }, { body: sb, frame: sf });
  }
  return map;
}

/**
 * Cadenas del mismo tipo y lado, en orden de adelante hacia atrás,
 * articulación a articulación. Lado y orden salen de la geometría (no de los
 * nombres), así sirven también entre esqueletos espejados.
 */
function mapChains(map: RetargetMap, target: { body: Body; frame: BodyFrame }, source: { body: Body; frame: BodyFrame }): void {
  const tb = target.body;
  const sb = source.body;
  const pairs: [number[], number[]][] = [[tb.spine, sb.spine], [tb.neck, sb.neck]];
  const groups = ({ body: b, frame }: { body: Body; frame: BodyFrame }) => {
    const out = new Map<string, number[][]>();
    const root = b.bones[b.root].position;
    const along = (j: number) => {
      const p = b.bones[j].position;
      return (p[0] - root[0]) * frame.forward[0] + (p[1] - root[1]) * frame.forward[1] + (p[2] - root[2]) * frame.forward[2];
    };
    const side = (joints: number[]) => {
      const p = b.bones[joints[joints.length - 1]].position;
      const d = (p[0] - root[0]) * frame.right[0] + (p[1] - root[1]) * frame.right[1] + (p[2] - root[2]) * frame.right[2];
      return Math.abs(d) < 0.02 * b.size ? 0 : Math.sign(d);
    };
    for (const c of [...b.chains].sort((x, y) => along(y.joints[0]) - along(x.joints[0]))) {
      const key = `${c.kind}:${side(c.joints)}`;
      if (!out.has(key)) out.set(key, []);
      out.get(key)!.push(c.joints);
    }
    return out;
  };
  const tg = groups(target);
  const sg = groups(source);
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
 * Pose del modelo para un cuadro del origen. Se arma una vez por esqueleto y
 * mapeo (ejes, escala, qué hijos dan la dirección de cada articulación) y se
 * usa cuadro a cuadro: para hornear un clip o para seguir al actor en vivo.
 */
export class Retargeter {
  readonly root: number;
  private align: THREE.Quaternion;
  private alignInverse: THREE.Quaternion;
  private scale: number;
  private sRoot: number;
  private aims: number[][];
  private rest: THREE.Vector3[];
  private pose: Pose = { rotations: new Map<number, Quat>(), translations: new Map<number, Vec3>(), controls: new Map() };
  private fk: Fk;

  constructor(
    target: SkeletonBone[],
    source: SkeletonBone[],
    private map: RetargetMap
  ) {
    const tb = analyzeBody(target);
    const sb = analyzeBody(source);
    const tf = bodyFrame(target, tb);
    const sf = bodyFrame(source, sb);
    // De los ejes del origen a los del modelo (arriba y adelante, por la geometría)
    this.align = tf && sf ? new THREE.Quaternion(...rotationBetweenFrames(sf.up, sf.forward, tf.up, tf.forward)) : new THREE.Quaternion();
    this.alignInverse = this.align.clone().invert();
    this.scale = tb && sb && sb.height > 1e-9 ? tb.height / sb.height : 1;
    this.root = tb?.root ?? target.findIndex((b) => b.parent === null);
    this.sRoot = map.get(this.root) ?? sb?.root ?? source.findIndex((b) => b.parent === null);

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
    this.aims = target.map((_, j) => {
      if (!map.has(j)) return [] as number[];
      const kids = children[j].flatMap((c) => {
        const d = mappedBelow(c);
        return d !== undefined && map.get(d) !== map.get(j) ? [d] : [];
      });
      // Primero el del centro (columna), después el de un costado
      kids.sort((a, b) => Math.abs(sides[a]) - Math.abs(sides[b]));
      return kids.slice(0, 2);
    });
    this.rest = source.map((b) => v3(b.position));
    this.fk = new Fk(target, this.pose);
  }

  /**
   * Pose para las posiciones `src` del origen y el giro de su raíz. La pose
   * devuelta se reusa en la llamada siguiente: copiarla si se guarda.
   */
  solve(src: Vec3[], rootRotation: Quat, rootMotion = true): Pose {
    const { pose, fk, align, map } = this;
    pose.rotations.clear();
    pose.translations.clear();
    // Raíz: giro del origen pasado a los ejes del modelo
    const R = new THREE.Quaternion(...rootRotation);
    pose.rotations.set(this.root, fq(align.clone().multiply(R).multiply(this.alignInverse).normalize()));
    if (rootMotion && this.sRoot >= 0 && src[this.sRoot]) {
      const d = v3(src[this.sRoot]).sub(this.rest[this.sRoot]).applyQuaternion(align).multiplyScalar(this.scale);
      pose.translations.set(this.root, toV(d));
    }
    fk.update();
    // Resto: cada hueso apunta como el del origen
    for (const j of fk.order) {
      if (j === this.root) continue;
      const aim = this.aims[j];
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
    return pose;
  }
}

/**
 * Clip del modelo que reproduce `motion` con el mapeo `map`. Una key por
 * cuadro, reducida después (se ve igual con menos keys).
 */
export function retargetClip(target: SkeletonBone[], motion: SourceMotion, map: RetargetMap, options: RetargetOptions = {}): AnimationClip {
  const retargeter = new Retargeter(target, motion.bones, map);
  const tRoot = retargeter.root;
  const [first, last] = options.range ?? [0, motion.positions.length - 1];
  const rotations = new Map<number, Key<Quat>[]>();
  const translations: Key<Vec3>[] = [];

  for (let f = first; f <= last; f++) {
    const src = motion.positions[f];
    if (!src) break;
    const pose = retargeter.solve(src, motion.rootRotation[f], options.rootMotion !== false);
    const frame = f - first;
    for (const [j, q] of pose.rotations) {
      if (!rotations.has(j)) rotations.set(j, []);
      rotations.get(j)!.push({ frame, value: q, interpolation: "linear" });
    }
    const t = pose.translations.get(tRoot);
    if (t) translations.push({ frame, value: t, interpolation: "linear" });
  }

  const size = analyzeBody(target)?.size ?? 1;
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
