/**
 * Animaciones básicas generadas a partir del esqueleto: reposo, caminar,
 * correr o trotar, saludar, aletear, nadar y reptar.
 *
 * No depende de la plantilla elegida: parte el esqueleto en cadenas (tramos
 * sin ramificar) y reconoce patas, brazos, alas, cola, cabeza, trompa,
 * orejas, tentáculos… por el nombre de sus huesos y, si no lo dice, por
 * geometría (una cadena que llega al suelo es una pata). Así sirve para las
 * plantillas, los planes de cuerpo y esqueletos editados. El resultado son
 * keys normales: se editan después en la línea de tiempo.
 *
 * Marco de los giros: en reposo todos los huesos del rig tienen orientación
 * identidad (ver `Viewer3D.buildRig`), así que el giro local de una
 * articulación se expresa en los ejes del modelo y el de sus hijas queda en
 * el marco ya girado del padre (la rodilla se dobla alrededor del muslo).
 */

import { createClip, type AnimationClip, type BoneTrack, type Quat, type Vec3 } from "./animation";

export interface SkeletonBone {
  name: string;
  position: Vec3;
  parent: number | null;
}

export type ChainKind =
  | "leg"
  | "arm"
  | "wing"
  | "tail"
  | "head"
  | "trunk"
  | "ear"
  | "antenna"
  | "pincer"
  | "fin"
  | "tentacle"
  | "body"
  | "other";

/** Tramo sin ramificar que termina en una punta */
export interface Chain {
  kind: ChainKind;
  /** Articulaciones de la base a la punta */
  joints: number[];
  /** Las que giran: todas menos la punta */
  rotating: number[];
  /** -1 izquierda, 1 derecha, 0 al centro */
  side: number;
  /** De la base a la punta (unitaria) */
  dir: Vec3;
  /** Posición de la base hacia adelante (ordena patas delanteras y traseras) */
  along: number;
  /** Pata abierta hacia el costado (artrópodos), no bajo el cuerpo */
  sprawl: boolean;
}

export interface Body {
  bones: SkeletonBone[];
  root: number;
  up: Vec3;
  forward: Vec3;
  right: Vec3;
  /** Mayor medida de la caja del esqueleto */
  size: number;
  height: number;
  /** Altura de las puntas de las patas en reposo */
  ground: number;
  chains: Chain[];
  /** Tronco entre la raíz y las ramificaciones (sin la raíz ni el cuello) */
  spine: number[];
  /** Cuello que ramifica (orejas colgando de él) */
  neck: number[];
}

export type PresetAnimationId = "idle" | "walk" | "run" | "trot" | "wave" | "fly" | "swim" | "slither";

export interface PresetAnimation {
  id: PresetAnimationId;
  name: string;
  description: string;
}

export interface PresetOptions {
  /** Nado con ondulación vertical (delfín, ballena) en vez de lateral */
  verticalSwim?: boolean;
  fps?: number;
}

// ─── Vectores y cuaterniones ────────────────────────────────────────────────

const add = (a: Vec3, b: Vec3): Vec3 => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
const sub = (a: Vec3, b: Vec3): Vec3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const scale = (a: Vec3, s: number): Vec3 => [a[0] * s, a[1] * s, a[2] * s];
const dot = (a: Vec3, b: Vec3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a: Vec3, b: Vec3): Vec3 => [
  a[1] * b[2] - a[2] * b[1],
  a[2] * b[0] - a[0] * b[2],
  a[0] * b[1] - a[1] * b[0],
];
const length = (a: Vec3) => Math.hypot(a[0], a[1], a[2]);
const flat = (a: Vec3): Vec3 => [a[0], 0, a[2]];

/** `a` unitario, o `fallback` si es casi nulo */
function unit(a: Vec3, fallback: Vec3): Vec3 {
  const l = length(a);
  return l > 1e-6 ? scale(a, 1 / l) : fallback;
}

const IDENTITY: Quat = [0, 0, 0, 1];

function axisAngle(axis: Vec3, angle: number): Quat {
  const s = Math.sin(angle / 2);
  return [axis[0] * s, axis[1] * s, axis[2] * s, Math.cos(angle / 2)];
}

function mul(a: Quat, b: Quat): Quat {
  const [ax, ay, az, aw] = a;
  const [bx, by, bz, bw] = b;
  return [
    aw * bx + ax * bw + ay * bz - az * by,
    aw * by - ax * bz + ay * bw + az * bx,
    aw * bz + ax * by - ay * bx + az * bw,
    aw * bw - ax * bx - ay * by - az * bz,
  ];
}

function rotate(q: Quat, v: Vec3): Vec3 {
  const u: Vec3 = [q[0], q[1], q[2]];
  const t = scale(cross(u, v), 2);
  return add(add(v, scale(t, q[3])), cross(u, t));
}

const deg = (d: number) => (d * Math.PI) / 180;
const TAU = 2 * Math.PI;
const wave = (t: number, cycles = 1, phase = 0) => Math.sin(TAU * (cycles * t - phase));
const smoothstep = (x: number) => (x <= 0 ? 0 : x >= 1 ? 1 : x * x * (3 - 2 * x));

// ─── Análisis del esqueleto ─────────────────────────────────────────────────

/** Tipo de cadena por nombre de hueso; `undefined` si el nombre no lo dice */
function kindByName(name: string): ChainKind | undefined {
  const n = name.toLowerCase();
  if (/wing|humerus|radius|carpus|digits/.test(n)) return "wing";
  if (/trunk|proboscis/.test(n)) return "trunk";
  if (/(^|[_.])ear/.test(n)) return "ear";
  if (/antenna/.test(n)) return "antenna";
  if (/pincer|pedipalp|claw/.test(n)) return "pincer";
  if (/dorsal/.test(n)) return "other";
  if (/pectoral|fin/.test(n)) return "fin";
  if (/tail|vertebra/.test(n)) return "tail";
  if (/head|neck|skull|sensor|jaw/.test(n)) return "head";
  if (/^arm\d|tentacle/.test(n)) return "tentacle";
  if (/leg|paw|hoof|foot|toe/.test(n)) return "leg";
  return undefined;
}

/** Nombre del hueso derecho que hace par con `name`: hip_l, paw_fl, hand.L, LeftArm */
function rightTwin(name: string): string | undefined {
  const m = /^(.*[._][a-z]*)([lL])$/.exec(name);
  if (m) return m[1] + (m[2] === "l" ? "r" : "R");
  if (/left/i.test(name)) return name.replace(/left/i, (s) => (s === "LEFT" ? "RIGHT" : s[0] === "L" ? "Right" : "right"));
  return undefined;
}

export function analyzeBody(bones: SkeletonBone[]): Body | null {
  const n = bones.length;
  if (n < 2) return null;
  const pos = bones.map((b) => b.position);
  const parent = bones.map((b) => (b.parent !== null && b.parent >= 0 && b.parent < n ? b.parent : null));
  const children = bones.map(() => [] as number[]);
  parent.forEach((p, i) => p !== null && children[p].push(i));

  // Raíz principal: la que tiene más descendientes
  const descendants = (j: number): number => children[j].reduce((sum, c) => sum + 1 + descendants(c), 0);
  const roots = parent.flatMap((p, i) => (p === null ? [i] : []));
  if (roots.length === 0) return null;
  const root = roots.reduce((best, r) => (descendants(r) > descendants(best) ? r : best));

  const min: Vec3 = [Infinity, Infinity, Infinity];
  const max: Vec3 = [-Infinity, -Infinity, -Infinity];
  for (const p of pos) {
    for (let k = 0; k < 3; k++) {
      min[k] = Math.min(min[k], p[k]);
      max[k] = Math.max(max[k], p[k]);
    }
  }
  const size = Math.max(max[0] - min[0], max[1] - min[1], max[2] - min[2]) || 1;
  const height = max[1] - min[1] || size;
  const up: Vec3 = [0, 1, 0];

  // Costado: de los huesos _l a sus pares _r; adelante: hacia donde mira la cabeza
  const byName = new Map(bones.map((b, i) => [b.name, i]));
  let lateral: Vec3 = [0, 0, 0];
  for (const [i, b] of bones.entries()) {
    const twin = rightTwin(b.name);
    const j = twin === undefined ? undefined : byName.get(twin);
    if (j !== undefined) lateral = add(lateral, sub(pos[j], pos[i]));
  }
  const center = scale(add(min, max), 0.5);
  const heads = bones.flatMap((b, i) => (/head|skull/i.test(b.name) ? [pos[i]] : []));
  const headDir = heads.length > 0 ? flat(sub(heads[0], center)) : ([0, 0, 0] as Vec3);
  let right = unit(flat(lateral), [0, 0, 0]);
  let forward: Vec3;
  if (length(right) > 0) {
    forward = cross(right, up);
    if (length(headDir) > 0.15 * size && dot(forward, headDir) < 0) {
      forward = scale(forward, -1);
      right = scale(right, -1);
    }
  } else if (length(headDir) > 0.15 * size) {
    forward = unit(headDir, [0, 0, 1]);
    right = cross(up, forward);
  } else {
    // Sin pares ni cabeza no se sabe hacia dónde mira: mejor no inventar
    return null;
  }

  // Cadenas: se sigue cada rama mientras no se divida
  const raw: { joints: number[]; fromRoot: boolean }[] = [];
  const trunk: number[] = [];
  const walk = (first: number, fromRoot: boolean) => {
    const joints = [first];
    let last = first;
    while (children[last].length === 1) {
      last = children[last][0];
      joints.push(last);
    }
    if (children[last].length === 0) raw.push({ joints, fromRoot });
    else {
      trunk.push(...joints);
      children[last].forEach((c) => walk(c, false));
    }
  };
  if (children[root].length === 1) walk(root, true);
  else children[root].forEach((c) => walk(c, false));

  const rootPos = pos[root];
  const chains: Chain[] = raw
    .filter((c) => c.joints.length >= 2)
    .map(({ joints, fromRoot }) => {
      const base = pos[joints[0]];
      const tip = pos[joints[joints.length - 1]];
      const offset = dot(sub(tip, rootPos), right);
      const side = Math.abs(offset) > 0.03 * size ? Math.sign(offset) : 0;
      let kind: ChainKind | undefined = fromRoot ? "body" : undefined;
      if (!kind) {
        const votes = new Map<ChainKind, number>();
        for (const j of joints) {
          const k = kindByName(bones[j].name);
          if (k) votes.set(k, (votes.get(k) ?? 0) + 1);
        }
        kind = [...votes.entries()].sort((a, b) => b[1] - a[1])[0]?.[0];
      }
      if (!kind) {
        if (tip[1] - min[1] < 0.12 * height) kind = "leg";
        else if (side !== 0) kind = "arm";
        else kind = dot(sub(tip, center), forward) > 0 ? "head" : "tail";
      }
      const reach = sub(tip, base);
      // Abierta si, desde donde se engancha al cuerpo, se aleja más de lo que baja
      const hang = sub(tip, pos[parent[joints[0]] ?? joints[0]]);
      return {
        kind,
        joints,
        rotating: joints.slice(0, -1),
        side,
        dir: unit(reach, [0, -1, 0]),
        along: dot(base, forward),
        sprawl: length(flat(hang)) > Math.abs(hang[1]),
      };
    });

  const legs = chains.filter((c) => c.kind === "leg");
  const ground = legs.length > 0 ? Math.min(...legs.map((c) => pos[c.joints[c.joints.length - 1]][1])) : min[1];
  const isNeck = (j: number) => /neck|head/i.test(bones[j].name);
  return {
    bones,
    root,
    up,
    forward,
    right,
    size,
    height,
    ground,
    chains,
    spine: trunk.filter((j) => j !== root && !isNeck(j)),
    neck: trunk.filter(isNeck),
  };
}

// ─── Qué animaciones admite cada cuerpo ─────────────────────────────────────

const ANIMATIONS: Record<PresetAnimationId, Omit<PresetAnimation, "id">> = {
  idle: { name: "Reposo", description: "Respira y mira alrededor; mueve cola, orejas y apéndices" },
  walk: { name: "Caminar", description: "Ciclo de paso en el lugar, con los pies en el suelo" },
  run: { name: "Correr", description: "Zancada larga con los brazos doblados" },
  trot: { name: "Trotar", description: "Patas en diagonal, más rápido que al paso" },
  wave: { name: "Saludar", description: "Levanta un brazo y saluda" },
  fly: { name: "Aletear", description: "Batido de alas con las patas recogidas" },
  swim: { name: "Nadar", description: "Ondulación del cuerpo y la cola, o pulsos de los tentáculos" },
  slither: { name: "Reptar", description: "Onda que recorre el cuerpo de la cabeza a la cola" },
};

const count = (body: Body, kind: ChainKind) => body.chains.filter((c) => c.kind === kind).length;

/** Animaciones que tienen sentido para este esqueleto */
export function availableAnimations(body: Body): PresetAnimation[] {
  const legs = body.chains.filter((c) => c.kind === "leg");
  const upright = legs.filter((c) => !c.sprawl);
  const ids: PresetAnimationId[] = ["idle"];
  if (legs.length >= 2) ids.push("walk");
  if (upright.length === 2) ids.push("run");
  if (upright.length >= 4) ids.push("trot");
  if (count(body, "arm") > 0) ids.push("wave");
  if (count(body, "wing") >= 2) ids.push("fly");
  if (legs.length === 0 && (count(body, "tentacle") >= 3 || count(body, "tail") + count(body, "fin") > 0)) {
    ids.push("swim");
  }
  if (legs.length === 0 && body.chains.some((c) => c.kind === "body" && c.rotating.length >= 3)) ids.push("slither");
  return ids.map((id) => ({ id, ...ANIMATIONS[id] }));
}

// ─── Pose y horneado ────────────────────────────────────────────────────────

class PoseBuilder {
  readonly rotations = new Map<number, Quat>();
  offset: Vec3 = [0, 0, 0];

  /** Suma un giro de `angle` radianes alrededor de `axis` (unitario) */
  turn(joint: number, axis: Vec3, angle: number): void {
    if (Math.abs(angle) < 1e-7) return;
    this.rotations.set(joint, mul(axisAngle(axis, angle), this.rotations.get(joint) ?? IDENTITY));
  }

  move(v: Vec3): void {
    this.offset = add(this.offset, v);
  }
}

/** Posición de cada articulación con la pose (misma cinemática que el rig del visor) */
export function jointPositions(bones: SkeletonBone[], rotations: Map<number, Quat>, offset: Vec3): Vec3[] {
  const pos = bones.map((b) => b.position);
  const head = (b: number) => pos[bones[b].parent ?? b];
  const world: Quat[] = [];
  const origin: Vec3[] = [];
  const out: Vec3[] = [];
  const visit = (b: number) => {
    const p = bones[b].parent;
    if (p === null) {
      world[b] = rotations.get(b) ?? IDENTITY;
      origin[b] = add(head(b), offset);
    } else {
      // El giro de la raíz va a su propio hueso; el de otra articulación, a los de sus hijos
      const q = bones[p].parent === null ? IDENTITY : (rotations.get(p) ?? IDENTITY);
      world[b] = mul(world[p], q);
      origin[b] = add(origin[p], rotate(world[p], sub(head(b), head(p))));
    }
    out[b] = add(origin[b], rotate(world[b], sub(pos[b], head(b))));
    bones.forEach((c, i) => c.parent === b && visit(i));
  };
  bones.forEach((b, i) => b.parent === null && visit(i));
  return out;
}

interface BakeOptions {
  /** Cuadros de un ciclo */
  frames: number;
  /** Cada cuántos cuadros va una key */
  step: number;
  /** Baja o sube el cuerpo para que la pata más baja quede en el suelo */
  grounded?: boolean;
  fps: number;
}

const round = (x: number) => Math.round(x * 1e5) / 1e5 || 0;

function bake(name: string, body: Body, options: BakeOptions, fill: (pose: PoseBuilder, t: number) => void): AnimationClip {
  const { frames, step, fps } = options;
  const tips = body.chains.filter((c) => c.kind === "leg").map((c) => c.joints[c.joints.length - 1]);
  const samples: { frame: number; pose: PoseBuilder }[] = [];
  for (let f = 0; ; f = Math.min(f + step, frames)) {
    const pose = new PoseBuilder();
    // t = 1 repite t = 0: el ciclo cierra en la última key
    fill(pose, f / frames);
    if (options.grounded && tips.length > 0) {
      const positions = jointPositions(body.bones, pose.rotations, pose.offset);
      const lowest = Math.min(...tips.map((j) => positions[j][1]));
      pose.move([0, body.ground - lowest, 0]);
    }
    samples.push({ frame: f, pose });
    if (f === frames) break;
  }

  const joints = new Set<number>();
  samples.forEach((s) => s.pose.rotations.forEach((_, j) => joints.add(j)));
  const moves = samples.some((s) => length(s.pose.offset) > 1e-6 * body.size);
  if (moves) joints.add(body.root);

  const tracks: BoneTrack[] = [...joints]
    .sort((a, b) => a - b)
    .map((j) => ({
      bone: body.bones[j].name,
      rotation: samples.map((s) => ({
        frame: s.frame,
        value: (s.pose.rotations.get(j) ?? IDENTITY).map(round) as Quat,
        interpolation: "linear" as const,
      })),
      translation:
        j === body.root && moves
          ? samples.map((s) => ({ frame: s.frame, value: s.pose.offset.map(round) as Vec3, interpolation: "linear" as const }))
          : [],
    }));
  const clip = createClip(name, fps);
  return { ...clip, start: 0, end: frames - 1, tracks };
}

// ─── Ejes ───────────────────────────────────────────────────────────────────

/** Eje que lleva la punta de una cadena con dirección `d` hacia adelante */
function swingAxis(body: Body, d: Vec3): Vec3 {
  return unit(cross(d, body.forward), cross(body.forward, body.up));
}

/** Eje que levanta la punta de una cadena con dirección `d` */
function liftAxis(body: Body, d: Vec3): Vec3 {
  return unit(cross(d, body.up), cross(body.forward, body.up));
}

/** Cabeceo: positivo baja la nariz de lo que mira adelante (y la de un cuello vertical) */
const nodAxis = (body: Body) => cross(body.up, body.forward);

/** Dirección del segmento que gira la articulación `i` de la cadena */
function segment(body: Body, chain: Chain, i: number): Vec3 {
  const a = body.bones[chain.joints[i]].position;
  const b = body.bones[chain.joints[i + 1]].position;
  return unit(sub(b, a), chain.dir);
}

/**
 * Onda que recorre la cadena: cada articulación con retraso `lag` (en ciclos)
 * respecto de la anterior. Con `bias` 1 el giro va de 0 a 2·amplitud (solo
 * hacia un lado: tentáculos que no deben hundirse en el suelo).
 */
function chainWave(
  pose: PoseBuilder,
  chain: Chain,
  axis: (i: number) => Vec3,
  amplitude: (i: number) => number,
  t: number,
  cycles: number,
  lag: number,
  phase = 0,
  bias = 0
): void {
  chain.rotating.forEach((j, i) => pose.turn(j, axis(i), amplitude(i) * (bias + wave(t, cycles, phase + lag * i))));
}

const chainsOf = (body: Body, ...kinds: ChainKind[]) => body.chains.filter((c) => kinds.includes(c.kind));

/** Índice en la cadena del hueso que se dobla (rodilla, codo), o el segundo */
function bendIndex(body: Body, chain: Chain, pattern: RegExp): number {
  const i = chain.rotating.findIndex((j) => pattern.test(body.bones[j].name));
  return i > 0 ? i : 1;
}

/** Apéndices que se mecen solos: cola, trompa, orejas, antenas, aletas, tentáculos */
function appendages(body: Body, pose: PoseBuilder, t: number, cycles: number, strength: number): void {
  const up = body.up;
  for (const chain of chainsOf(body, "tail")) {
    const n = chain.rotating.length;
    chainWave(pose, chain, () => up, () => (deg(18) * strength) / Math.sqrt(n), t, cycles, 0.12);
  }
  for (const chain of chainsOf(body, "trunk")) {
    const n = chain.rotating.length;
    chainWave(pose, chain, (i) => liftAxis(body, segment(body, chain, i)), () => (deg(30) * strength) / n, t, 1, 0.08);
    chainWave(pose, chain, () => up, () => (deg(20) * strength) / n, t, 1, 0.06, 0.25);
  }
  chainsOf(body, "ear").forEach((chain, k) =>
    chainWave(pose, chain, (i) => liftAxis(body, segment(body, chain, i)), () => deg(8) * strength, t, 2 * cycles, 0.1, 0.3 * k)
  );
  for (const chain of chainsOf(body, "antenna")) {
    chainWave(pose, chain, () => up, () => deg(8) * strength, t, 2 * cycles, 0.15, chain.side > 0 ? 0.5 : 0);
  }
  for (const chain of chainsOf(body, "fin")) {
    chainWave(pose, chain, (i) => liftAxis(body, segment(body, chain, i)), () => deg(15) * strength, t, 2 * cycles, 0.1);
  }
  chainsOf(body, "tentacle").forEach((chain, k, all) =>
    chainWave(pose, chain, (i) => liftAxis(body, segment(body, chain, i)), () => deg(4) * strength, t, 1, 0.1, k / all.length, 1)
  );
}

// ─── Marchas ────────────────────────────────────────────────────────────────

interface Gait {
  frames: number;
  /** Parte del ciclo con la pata apoyada */
  duty: number;
  hip: number;
  knee: number;
  /** Pies abiertos: levantar la pata al avanzarla */
  lift: number;
  arm: number;
  elbow: number;
  /** Inclinación del tronco hacia adelante */
  lean: number;
  /** Fase de cada pata (0 a 1) */
  phase: (leg: Chain, pair: number, pairs: number) => number;
}

/** Avance de la pata (+1 adelante, −1 atrás) y cuánto va levantada (0 a 1) */
function stride(u: number, duty: number): { swing: number; lift: number } {
  if (u < duty) return { swing: 1 - (2 * u) / duty, lift: 0 };
  const v = (u - duty) / (1 - duty);
  return { swing: -Math.cos(Math.PI * v), lift: Math.sin(Math.PI * v) };
}

const left = (leg: Chain) => leg.side < 0;

const GAITS: Record<"walk" | "run" | "trot" | "crawl", Gait> = {
  walk: {
    frames: 24,
    duty: 0.62,
    hip: deg(24),
    knee: deg(40),
    lift: deg(20),
    arm: deg(18),
    elbow: deg(12),
    lean: 0,
    // Bípedo: alternadas; cuadrúpedo: secuencia lateral (TI, DI, TD, DD)
    phase: (leg, pair, pairs) => (left(leg) ? 0 : 0.5) + (pairs > 1 && pair === 0 ? 0.25 : 0),
  },
  run: {
    frames: 16,
    duty: 0.4,
    hip: deg(38),
    knee: deg(95),
    lift: deg(25),
    arm: deg(35),
    elbow: deg(80),
    lean: deg(10),
    phase: (leg) => (left(leg) ? 0 : 0.5),
  },
  trot: {
    frames: 16,
    duty: 0.5,
    hip: deg(28),
    knee: deg(55),
    lift: deg(25),
    arm: deg(20),
    elbow: deg(20),
    lean: 0,
    // Diagonales juntas
    phase: (leg, pair) => (left(leg) ? 0 : 0.5) + (pair === 0 ? 0.5 : 0),
  },
  crawl: {
    frames: 20,
    duty: 0.55,
    hip: deg(18),
    knee: deg(30),
    lift: deg(22),
    arm: deg(10),
    elbow: deg(10),
    lean: 0,
    // Trípode alterno: patas vecinas en contrafase
    phase: (leg, pair) => ((pair + (left(leg) ? 0 : 1)) % 2) * 0.5,
  },
};

function gait(body: Body, g: Gait, pose: PoseBuilder, t: number): void {
  const legs = chainsOf(body, "leg");
  // Pares de adelante hacia atrás, por lado
  const sides = [legs.filter((l) => l.side < 0), legs.filter((l) => l.side > 0), legs.filter((l) => l.side === 0)];
  sides.forEach((s) => s.sort((a, b) => b.along - a.along));
  const pairs = Math.max(sides[0].length, sides[1].length, 1);
  const phases = new Map<Chain, number>();
  for (const s of sides) {
    s.forEach((leg, pair) => {
      const phase = leg.side === 0 ? pair / Math.max(s.length, 1) : g.phase(leg, pair, pairs);
      phases.set(leg, phase);
    });
  }

  for (const [leg, phase] of phases) {
    const { swing, lift } = stride((((t + phase) % 1) + 1) % 1, g.duty);
    const [hip] = leg.rotating;
    if (leg.sprawl) {
      // Pata abierta: avanza girando hacia adelante y se levanta al avanzar
      const d = flat(leg.dir);
      pose.turn(hip, swingAxis(body, d), g.hip * swing);
      pose.turn(hip, liftAxis(body, d), g.lift * lift);
      continue;
    }
    pose.turn(hip, swingAxis(body, segment(body, leg, 0)), g.hip * swing);
    if (leg.rotating.length > 1) {
      const knee = bendIndex(body, leg, /knee|elbow|tibio/i);
      // La rodilla se dobla llevando el pie hacia atrás; el tobillo lo endereza
      const bend = g.knee * (lift + 0.1 * Math.max(0, swing));
      pose.turn(leg.rotating[knee], swingAxis(body, segment(body, leg, knee)), -bend);
      if (knee + 1 < leg.rotating.length) {
        pose.turn(leg.rotating[knee + 1], swingAxis(body, segment(body, leg, knee + 1)), 0.5 * bend);
      }
    }
  }

  // Brazos en contrafase con la pata del mismo lado
  const legOfSide = (side: number) => [...phases.entries()].find(([l]) => l.side === side)?.[1] ?? 0;
  for (const arm of chainsOf(body, "arm")) {
    const phase = legOfSide(arm.side) + 0.5;
    const { swing } = stride((((t + phase) % 1) + 1) % 1, 0.5);
    pose.turn(arm.rotating[0], swingAxis(body, segment(body, arm, 0)), g.arm * swing);
    if (arm.rotating.length > 1) {
      const elbow = bendIndex(body, arm, /elbow/i);
      pose.turn(arm.rotating[elbow], swingAxis(body, segment(body, arm, elbow)), g.elbow * (1 + 0.3 * swing));
    }
  }

  // Tronco: se inclina, gira contra las caderas y la cabeza acompaña
  const nod = nodAxis(body);
  const [firstSpine] = body.spine;
  if (firstSpine !== undefined) {
    pose.turn(firstSpine, nod, g.lean);
    const twist = legs.length === 2 ? deg(6) : deg(3);
    body.spine.forEach((j) => pose.turn(j, body.up, (twist / body.spine.length) * wave(t, 1, legOfSide(-1))));
  } else if (g.lean) {
    pose.turn(body.root, nod, g.lean);
  }
  pose.turn(body.root, body.forward, deg(legs.length === 2 ? 3 : 1.5) * wave(t, 1, legOfSide(-1)));
  for (const j of headJoints(body)) pose.turn(j, nod, deg(3) * wave(t, 2) - g.lean / Math.max(headJoints(body).length, 1));
  appendages(body, pose, t, 1, 0.8);
}

function headJoints(body: Body): number[] {
  return [...body.neck, ...chainsOf(body, "head").flatMap((c) => c.rotating)];
}

// ─── Generadores ────────────────────────────────────────────────────────────

function idle(body: Body, pose: PoseBuilder, t: number): void {
  const nod = nodAxis(body);
  // Respiración: dos por ciclo
  const breath = wave(t, 2);
  body.spine.forEach((j) => pose.turn(j, nod, (-deg(2) / body.spine.length) * breath));
  if (body.spine.length === 0) pose.move([0, 0.006 * body.size * breath, 0]);
  // Mira a un lado y al otro
  const head = headJoints(body);
  head.forEach((j) => {
    pose.turn(j, body.up, (deg(14) / head.length) * wave(t));
    pose.turn(j, nod, (deg(4) / head.length) * wave(t, 2, 0.2));
  });
  for (const arm of chainsOf(body, "arm")) {
    pose.turn(arm.rotating[0], swingAxis(body, segment(body, arm, 0)), deg(3) * wave(t, 2, arm.side > 0 ? 0.1 : 0));
  }
  for (const wing of chainsOf(body, "wing")) {
    pose.turn(wing.rotating[0], liftAxis(body, segment(body, wing, 0)), deg(4) * wave(t, 2));
  }
  for (const pincer of chainsOf(body, "pincer")) {
    pose.turn(pincer.rotating[0], body.up, deg(6) * pincer.side * wave(t, 3));
  }
  for (const leg of chainsOf(body, "leg").filter((l) => l.sprawl)) {
    pose.turn(leg.rotating[0], liftAxis(body, flat(leg.dir)), deg(2) * wave(t, 2, leg.along));
  }
  for (const chain of chainsOf(body, "body")) {
    chainWave(pose, chain, () => body.up, () => deg(10) / chain.rotating.length, t, 1, 0.1);
  }
  appendages(body, pose, t, 2, 1);
}

function waveHand(body: Body, pose: PoseBuilder, t: number): void {
  const arms = chainsOf(body, "arm");
  const arm = arms.find((a) => a.side > 0) ?? arms[0];
  // Sube, saluda tres veces y baja
  const raise = smoothstep(t / 0.25) * (1 - smoothstep((t - 0.75) / 0.25));
  const d = segment(body, arm, 0);
  pose.turn(arm.rotating[0], liftAxis(body, d), deg(140) * raise);
  if (arm.rotating.length > 1) {
    const elbow = bendIndex(body, arm, /elbow/i);
    const axis = liftAxis(body, segment(body, arm, elbow));
    pose.turn(arm.rotating[elbow], axis, raise * (deg(20) + deg(25) * wave(t, 4)));
  }
  const head = headJoints(body);
  head.forEach((j) => pose.turn(j, body.up, (deg(12) * raise * -arm.side) / head.length));
  for (const other of arms.filter((a) => a !== arm)) {
    pose.turn(other.rotating[0], swingAxis(body, segment(body, other, 0)), deg(3) * wave(t, 2));
  }
  appendages(body, pose, t, 2, 0.6);
}

function fly(body: Body, pose: PoseBuilder, t: number): void {
  for (const wing of chainsOf(body, "wing")) {
    chainWave(
      pose,
      wing,
      (i) => liftAxis(body, segment(body, wing, i)),
      (i) => (i === 0 ? deg(50) : deg(18)),
      t,
      1,
      0.12,
      -0.25
    );
  }
  // Patas recogidas hacia atrás
  for (const leg of chainsOf(body, "leg")) {
    if (leg.sprawl) {
      pose.turn(leg.rotating[0], liftAxis(body, flat(leg.dir)), -deg(20));
    } else {
      pose.turn(leg.rotating[0], swingAxis(body, segment(body, leg, 0)), -deg(35));
      if (leg.rotating.length > 1) {
        const knee = bendIndex(body, leg, /knee|tibio/i);
        pose.turn(leg.rotating[knee], swingAxis(body, segment(body, leg, knee)), -deg(40));
      }
    }
  }
  // El cuerpo sube con cada aletazo hacia abajo
  pose.move([0, 0.03 * body.size * wave(t, 1, 0.5), 0]);
  const nod = nodAxis(body);
  for (const tail of chainsOf(body, "tail")) {
    chainWave(pose, tail, () => nod, () => deg(10) / tail.rotating.length, t, 1, 0.1);
  }
  for (const j of headJoints(body)) pose.turn(j, nod, deg(3) * wave(t, 1, 0.3));
  appendages(body, pose, t, 1, 0.3);
}

function swim(body: Body, pose: PoseBuilder, t: number, vertical: boolean): void {
  const tentacles = chainsOf(body, "tentacle");
  if (tentacles.length >= 3) {
    // Pulsos: los brazos se cierran y abren a la vez y el cuerpo avanza
    for (const arm of tentacles) {
      chainWave(pose, arm, (i) => liftAxis(body, segment(body, arm, i)), () => deg(22), t, 1, 0.05);
    }
    pose.move([0, 0.08 * body.size * wave(t, 1, 0.2), 0]);
    return;
  }
  const axis = vertical ? nodAxis(body) : body.up;
  // Ondulación que crece hacia la punta de la cola; la raíz compensa
  for (const tail of chainsOf(body, "tail", "body")) {
    const n = tail.rotating.length;
    chainWave(pose, tail, () => axis, (i) => deg(8 + (16 * i) / Math.max(n - 1, 1)), t, 1, 0.1);
  }
  pose.turn(body.root, axis, -deg(5) * wave(t, 1, -0.1));
  for (const fin of chainsOf(body, "fin")) {
    pose.turn(fin.rotating[0], liftAxis(body, segment(body, fin, 0)), deg(20) * wave(t, 2));
  }
}

function slither(body: Body, pose: PoseBuilder, t: number): void {
  for (const chain of chainsOf(body, "body")) {
    const n = chain.rotating.length;
    // Una onda y media a lo largo del cuerpo, que viaja hacia la cola
    chain.rotating.forEach((j, i) => {
      if (i === 0) return;
      pose.turn(j, body.up, deg(Math.min(30, 220 / n)) * wave(t, 1, (1.5 * i) / n));
    });
    pose.turn(chain.rotating[0], body.up, deg(12) * wave(t, 1, 0.1));
  }
  appendages(body, pose, t, 1, 0.5);
}

/** Crea el clip `id` para el esqueleto; `name` por defecto, el de la animación */
export function generateAnimation(
  body: Body,
  id: PresetAnimationId,
  options: PresetOptions = {},
  name = ANIMATIONS[id].name
): AnimationClip {
  const fps = options.fps ?? 24;
  const legs = chainsOf(body, "leg");
  switch (id) {
    case "idle":
      return bake(name, body, { frames: 72, step: 4, grounded: true, fps }, (p, t) => idle(body, p, t));
    case "walk": {
      const g = legs.length > 4 || legs.every((l) => l.sprawl) ? GAITS.crawl : GAITS.walk;
      const frames = legs.length >= 4 && g === GAITS.walk ? 28 : g.frames;
      return bake(name, body, { frames, step: 2, grounded: true, fps }, (p, t) => gait(body, g, p, t));
    }
    case "run":
      return bake(name, body, { frames: GAITS.run.frames, step: 1, grounded: true, fps }, (p, t) =>
        gait(body, GAITS.run, p, t)
      );
    case "trot":
      return bake(name, body, { frames: GAITS.trot.frames, step: 1, grounded: true, fps }, (p, t) =>
        gait(body, GAITS.trot, p, t)
      );
    case "wave":
      return bake(name, body, { frames: 48, step: 2, grounded: legs.length > 0, fps }, (p, t) => waveHand(body, p, t));
    case "fly":
      return bake(name, body, { frames: 16, step: 2, fps }, (p, t) => fly(body, p, t));
    case "swim":
      return bake(name, body, { frames: 36, step: 3, fps }, (p, t) => swim(body, p, t, !!options.verticalSwim));
    case "slither":
      return bake(name, body, { frames: 36, step: 3, fps }, (p, t) => slither(body, p, t));
  }
}
