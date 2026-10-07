/**
 * Animaciones básicas generadas a partir del esqueleto: reposo, caminar,
 * correr, trotar o galopar, saltar, saludar, aplaudir, golpear, patear,
 * bailar, asentir, negar, comer, sacudirse, mover la cola, aletear, planear,
 * nadar, reptar, atacar, amenazar, arrastrarse con tentáculos, paso de
 * portante, echarse, flexiones, morder, caminar de lado, picar, braquiar,
 * colgarse de la cola y salto de rana.
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
import { Fk, solveTwoBone } from "./ik";

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
  | "mouth"
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

export type PresetAnimationId =
  | "idle"
  | "walk"
  | "run"
  | "trot"
  | "gallop"
  | "jump"
  | "wave"
  | "clap"
  | "punch"
  | "kick"
  | "dance"
  | "nod"
  | "shakeHead"
  | "eat"
  | "shake"
  | "wag"
  | "fly"
  | "glide"
  | "swim"
  | "swimFast"
  | "slither"
  | "strike"
  | "pinch"
  | "threat"
  | "tentacleCrawl"
  | "pace"
  | "lieDown"
  | "pushUps"
  | "bite"
  | "sideWalk"
  | "sting"
  | "brachiate"
  | "tailHang"
  | "hop";

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
  if (/fang|mandible|chelicer/.test(n)) return "mouth";
  if (/antenna|(^|[_.])palp|eyestalk/.test(n)) return "antenna";
  if (/pincer|pedipalp|claw/.test(n)) return "pincer";
  if (/dorsal|horn|tusk|antler/.test(n)) return "other";
  if (/pectoral|fin|fluke/.test(n)) return "fin";
  if (/tail|vertebra|abdomen/.test(n)) return "tail";
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
  gallop: { name: "Galopar", description: "Las patas traseras empujan y las delanteras caen una tras otra; la espalda se flexiona" },
  jump: { name: "Saltar", description: "Se agacha, salta en el lugar y amortigua al caer" },
  clap: { name: "Aplaudir", description: "Brazos adelante, las manos se juntan cuatro veces" },
  punch: { name: "Golpear", description: "Guardia y golpes rectos alternando los brazos" },
  kick: { name: "Patada", description: "Recoge la rodilla, patea adelante y vuelve a apoyar" },
  dance: { name: "Bailar", description: "Rebota con las rodillas, mece el tronco y sube los brazos al ritmo" },
  nod: { name: "Asentir", description: "Dice que sí con la cabeza" },
  shakeHead: { name: "Negar", description: "Dice que no con la cabeza" },
  eat: { name: "Comer", description: "Baja la cabeza al suelo, mastica o picotea y la vuelve a subir" },
  shake: { name: "Sacudirse", description: "Sacude el cuerpo como un perro mojado, de la cabeza a la cola" },
  wag: { name: "Mover la cola", description: "La cola se mueve rápido de lado a lado" },
  glide: { name: "Planear", description: "Alas extendidas, inclinándose al virar" },
  swimFast: { name: "Nadar rápido", description: "Ondulación más amplia y el doble de rápida" },
  strike: { name: "Atacar", description: "Levanta la cabeza, se enrosca hacia atrás y se lanza adelante" },
  pinch: { name: "Atacar con pinzas", description: "Levanta las pinzas y las cierra de golpe; la cola pica si la tiene" },
  threat: { name: "Amenazar", description: "Se alza sobre las patas traseras y agita las delanteras" },
  tentacleCrawl: { name: "Arrastrarse", description: "Los tentáculos se enroscan y estiran por turnos" },
  pace: { name: "Paso de portante", description: "Las dos patas del mismo lado avanzan juntas y el cuerpo se mece (camélidos, jirafa)" },
  bite: { name: "Morder", description: "Los colmillos o mandíbulas se abren y se cierran; la cabeza acompaña" },
  sideWalk: { name: "Caminar de lado", description: "Paso de cangrejo: las patas empujan y tiran hacia el costado" },
  brachiate: { name: "Braquiar", description: "Avanza colgado de los brazos, una mano y después la otra, con el cuerpo como péndulo" },
  tailHang: { name: "Colgarse de la cola", description: "Cabeza abajo, colgado de la cola prensil; se mece y estira los brazos" },
  hop: { name: "Salto de rana", description: "Se agacha, estira las patas traseras de golpe, vuela y cae sobre las delanteras" },
  sting: { name: "Picar", description: "El abdomen se curva por debajo del cuerpo y pica dos veces" },
  pushUps: { name: "Flexiones", description: "Despliegue de lagartija: estira las patas delanteras y sube y baja la cabeza" },
  lieDown: {
    name: "Echarse",
    description: "Se arrodilla sobre las patas delanteras, dobla las traseras, descansa y se levanta empezando por atrás",
  },
};

const count = (body: Body, kind: ChainKind) => body.chains.filter((c) => c.kind === kind).length;

/** Largo de una cadena (suma de sus tramos) */
const chainLength = (body: Body, c: Chain) =>
  c.joints.slice(1).reduce((sum, j, i) => sum + length(sub(pos(body, j), pos(body, c.joints[i]))), 0);

/** Dos brazos al menos un 20 % más largos que las piernas: gibón, mono araña */
function longArms(body: Body): boolean {
  const arms = limbs(body, "arm");
  const legs = limbs(body, "leg").filter((l) => !l.sprawl);
  if (arms.length !== 2 || legs.length !== 2) return false;
  const arm = Math.min(...arms.map((a) => chainLength(body, a)));
  const leg = Math.max(...legs.map((l) => chainLength(body, l)));
  return arm > 1.2 * leg;
}

/** Patas traseras plegadas en Z: la rodilla adelante de la cadera y el tobillo detrás de la rodilla */
function foldedHindLegs(body: Body): Chain[] {
  const legs = chainsOf(body, "leg");
  if (legs.length < 4) return [];
  const middle = legs.reduce((sum, l) => sum + l.along, 0) / legs.length;
  const fwd = (a: number, b: number) => dot(sub(pos(body, b), pos(body, a)), body.forward);
  return legs.filter((l) => l.along < middle && l.joints.length >= 4 && fwd(l.joints[0], l.joints[1]) > 0 && fwd(l.joints[1], l.joints[2]) < 0);
}

/** Colas de verdad (no el abdomen de un artrópodo) */
const wagging = (body: Body) =>
  body.chains.filter((c) => c.kind === "tail" && !/abdomen/.test(body.bones[c.joints[0]].name));

/**
 * Se arrastra con los brazos: cinco o más, con las puntas en el suelo y el
 * cuerpo abajo (pulpo, estrella de mar; no la medusa ni la anémona)
 */
function crawlingArms(body: Body): boolean {
  const arms = chainsOf(body, "tentacle");
  if (body.chains.some((c) => c.kind === "leg") || arms.length < 5) return false;
  // Contra el tamaño, no la altura: la estrella de mar es plana
  const low = (j: number) => body.bones[j].position[1] - body.ground < 0.15 * body.size;
  const rootLow = body.bones[body.root].position[1] - body.ground < 0.4 * body.size;
  return rootLow && arms.every((c) => low(c.joints[c.joints.length - 1]));
}

/** Abdómenes que pican (los de los insectos alados) */
const stingers = (body: Body) =>
  body.chains.filter((c) => c.kind === "tail" && c.rotating.length >= 2 && /abdomen/.test(body.bones[c.joints[0]].name));

/** Cuatro patas abiertas al costado, sin pinzas: reptiles, anfibios */
function sprawlingQuadruped(body: Body): boolean {
  const legs = body.chains.filter((c) => c.kind === "leg");
  return legs.length === 4 && legs.every((l) => l.sprawl) && count(body, "pincer") === 0;
}

/** Animaciones que tienen sentido para este esqueleto */
export function availableAnimations(body: Body): PresetAnimation[] {
  const legs = body.chains.filter((c) => c.kind === "leg");
  const upright = legs.filter((c) => !c.sprawl);
  const ids: PresetAnimationId[] = ["idle"];
  if (legs.length >= 2) ids.push("walk");
  if (upright.length === 2 || sprawlingQuadruped(body)) ids.push("run");
  if (upright.length >= 4) ids.push("trot");
  if (count(body, "arm") > 0) ids.push("wave");
  if (count(body, "wing") >= 2) ids.push("fly");
  if (legs.length === 0 && (count(body, "tentacle") >= 3 || count(body, "tail") + count(body, "fin") > 0)) {
    ids.push("swim");
  }
  if (legs.length === 0 && body.chains.some((c) => c.kind === "body" && c.rotating.length >= 3)) ids.push("slither");
  // Las nuevas, por parte del cuerpo
  const arms = limbs(body, "arm").length;
  const head = headJoints(body).length > 0;
  if (upright.length >= 4) ids.push("gallop");
  // Un elefante no salta (y la trompa atravesaría el suelo al agacharse)
  if (upright.length >= 2 && count(body, "trunk") === 0) ids.push("jump");
  if (arms >= 2) ids.push("clap");
  if (arms >= 2 && upright.length === 2) ids.push("punch", "dance");
  if (upright.length === 2) ids.push("kick");
  if (head) ids.push("nod", "shakeHead");
  if (head && upright.length >= 2 && arms === 0) ids.push("eat");
  if (upright.length >= 4) ids.push("shake");
  if (legs.length >= 2 && wagging(body).length > 0 && count(body, "pincer") === 0) ids.push("wag");
  if (count(body, "wing") >= 2) ids.push("glide");
  if (ids.includes("swim")) ids.push("swimFast");
  if (legs.length === 0 && body.chains.some((c) => c.kind === "body" && c.rotating.length >= 6)) ids.push("strike");
  if (count(body, "pincer") > 0) ids.push("pinch");
  if (legs.length >= 4 && legs.every((l) => l.sprawl) && !sprawlingQuadruped(body)) ids.push("threat");
  if (crawlingArms(body)) ids.push("tentacleCrawl");
  if (upright.length >= 4) ids.push("pace");
  // Arrodillarse sobre el carpo pide patas delanteras con carpo y menudillo
  if (kneelingLegs(body)) ids.push("lieDown");
  // Las lagartijas; una tortuga (cola corta) no
  if (sprawlingQuadruped(body) && head && chainsOf(body, "tail").some((c) => c.rotating.length >= 3)) ids.push("pushUps");
  if (count(body, "mouth") >= 2) ids.push("bite");
  if (count(body, "pincer") > 0 && legs.length >= 6 && legs.every((l) => l.sprawl)) ids.push("sideWalk");
  if (count(body, "wing") >= 2 && stingers(body).length > 0) ids.push("sting");
  if (longArms(body)) ids.push("brachiate");
  if (arms >= 2 && wagging(body).some((c) => c.rotating.length >= 6)) ids.push("tailHang");
  if (foldedHindLegs(body).length >= 2) ids.push("hop");
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

const GAITS: Record<"walk" | "run" | "trot" | "gallop" | "crawl" | "pace" | "sprint", Gait> = {
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
  gallop: {
    frames: 12,
    duty: 0.32,
    hip: deg(40),
    knee: deg(80),
    lift: deg(25),
    arm: deg(30),
    elbow: deg(40),
    lean: 0,
    // Galope transverso: traseras casi juntas, delanteras media vuelta después
    phase: (leg, pair) => (pair === 0 ? 0.45 : 0) + (left(leg) ? 0 : 0.12),
  },
  pace: {
    frames: 28,
    duty: 0.6,
    hip: deg(24),
    knee: deg(40),
    lift: deg(20),
    arm: deg(18),
    elbow: deg(12),
    lean: 0,
    // Las dos del mismo lado juntas
    phase: (leg) => (left(leg) ? 0 : 0.5),
  },
  // Pique de reptil: patas en diagonal, rápido y con zancada larga
  sprint: {
    frames: 12,
    duty: 0.45,
    hip: deg(30),
    knee: deg(30),
    lift: deg(28),
    arm: deg(10),
    elbow: deg(10),
    lean: 0,
    phase: (leg, pair) => ((pair + (left(leg) ? 0 : 1)) % 2) * 0.5,
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
    // Trípode alterno (patas vecinas en contrafase); con muchas patas
    // (ciempiés), una onda que corre de atrás hacia adelante
    phase: (leg, pair, pairs) =>
      pairs > 4 ? (((1 - pair / pairs) * 1.5 + (left(leg) ? 0 : 0.5)) % 1) : ((pair + (left(leg) ? 0 : 1)) % 2) * 0.5,
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

  const withIk = ikLegs(body);
  for (const [leg, phase] of phases) {
    const { swing, lift } = stride((((t + phase) % 1) + 1) % 1, g.duty);
    const [hip] = leg.rotating;
    // Las patas con IK se resuelven al final, con el cuerpo ya puesto
    if (withIk.includes(leg)) continue;
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
    if (sprawlingQuadruped(body)) {
      // Reptil: la columna ondula de lado, caderas y hombros a contramano, y
      // la cabeza sigue mirando adelante
      const bend = (g === GAITS.sprint ? deg(14) : deg(10)) * wave(t, 1, legOfSide(-1));
      pose.turn(body.root, body.up, bend);
      body.spine.forEach((j) => pose.turn(j, body.up, (-2 * bend) / body.spine.length));
      const head = headJoints(body);
      head.forEach((j) => pose.turn(j, body.up, bend / Math.max(head.length, 1)));
    } else {
      const twist = legs.length === 2 ? deg(6) : deg(3);
      body.spine.forEach((j) => pose.turn(j, body.up, (twist / body.spine.length) * wave(t, 1, legOfSide(-1))));
    }
  } else if (g.lean) {
    pose.turn(body.root, nod, g.lean);
  }
  pose.turn(body.root, body.forward, deg(legs.length === 2 ? 3 : 1.5) * wave(t, 1, legOfSide(-1)));
  for (const j of headJoints(body)) pose.turn(j, nod, deg(3) * wave(t, 2) - g.lean / Math.max(headJoints(body).length, 1));
  appendages(body, pose, t, 1, 0.8);

  // Pies: apoyados en el suelo corren hacia atrás a velocidad pareja, en el
  // aire avanzan en arco; el IK de dos huesos pone la pierna
  if (withIk.length > 0) {
    const legs = withIk.map((leg) => {
      const { swing, lift } = stride((((t + phases.get(leg)!) % 1) + 1) % 1, g.duty);
      const k = bendIndex(body, leg, /knee|tibio|shin|calf/i);
      const [a, b, c] = [leg.joints[k - 1], leg.joints[k], leg.joints[k + 1]];
      const size = legSize(body, leg);
      const rest = body.bones[c].position;
      const target = add(add(rest, scale(body.forward, stepReach(g, size) * swing)), scale(body.up, (g.lift / deg(20)) * 0.12 * size * lift));
      return { leg, k, a, b, c, size, target, lift };
    });
    // Péndulo invertido: la cadera tan alta como dejen las patas apoyadas
    // casi rectas (baja en el doble apoyo, sube a mitad del paso)
    const drop = Math.max(
      0,
      ...legs
        .filter((l) => l.lift === 0)
        .map((l) => {
          const hip = body.bones[l.a].position;
          const d = sub(hip, l.target);
          const height = dot(d, body.up);
          const flatDist = length(sub(d, scale(body.up, height)));
          return height - Math.sqrt(Math.max(0, (0.985 * l.size) ** 2 - flatDist * flatDist));
        })
    );
    pose.move([0, -drop, 0]);
    const ikPose = { rotations: pose.rotations, translations: new Map([[body.root, pose.offset]]), controls: new Map() };
    const fk = new Fk(body.bones, ikPose);
    const lateral = cross(body.up, body.forward);
    const middle = legs.reduce((sum, l) => sum + l.leg.along, 0) / legs.length;
    for (const { leg, k, a, b, c, target, lift } of legs) {
      solveTwoBone(fk, a, b, c, target, { restNormal: legNormal(body, a, b, c) });
      // Pie plano en el suelo; en el aire, la punta un poco hacia abajo. Con
      // carpo o corvejón y menudillo, la caña se pliega atrás al avanzar (más
      // la delantera)
      const long = leg.joints.length > k + 3;
      const fold = !long ? deg(15) : leg.along > middle ? 1.5 * g.knee : 0.8 * g.knee;
      if (leg.joints.length > k + 2) fk.setWorld(c, axisAngle(lateral, fold * lift));
    }
  }
}

/** Medio paso (adelante o atrás de donde está el pie en reposo) */
const stepReach = (g: Gait, legLength: number) => 0.75 * Math.sin(g.hip) * legLength;

/** Patas que caminan con IK: bajo el cuerpo, con rodilla y algo más abajo */
function ikLegs(body: Body): Chain[] {
  return chainsOf(body, "leg").filter((l) => !l.sprawl && bendIndex(body, l, /knee|tibio|shin|calf/i) + 1 < l.joints.length);
}

/** Largo de la pata de la cadera al tobillo */
function legSize(body: Body, leg: Chain): number {
  const k = bendIndex(body, leg, /knee|tibio|shin|calf/i);
  const p = (i: number) => body.bones[leg.joints[i]].position;
  return length(sub(p(k), p(k - 1))) + length(sub(p(k + 1), p(k)));
}

/** Normal del plano de la pata en reposo (hacia adelante si está recta) */
function legNormal(body: Body, a: number, b: number, c: number): Vec3 {
  const [A, B, C] = [a, b, c].map((j) => body.bones[j].position);
  const n = cross(sub(B, A), sub(C, B));
  return length(n) > 1e-4 * length(sub(C, A)) ** 2 ? unit(n, [1, 0, 0]) : unit(cross(body.forward, sub(B, A)), [1, 0, 0]);
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

function swim(body: Body, pose: PoseBuilder, t: number, vertical: boolean, cycles = 1, strength = 1): void {
  const tentacles = chainsOf(body, "tentacle");
  if (tentacles.length >= 3) {
    // Pulsos: los brazos se cierran y abren a la vez y el cuerpo avanza
    for (const arm of tentacles) {
      chainWave(pose, arm, (i) => liftAxis(body, segment(body, arm, i)), () => deg(22) * strength, t, cycles, 0.05);
    }
    pose.move([0, 0.08 * body.size * strength * wave(t, cycles, 0.2), 0]);
    return;
  }
  const axis = vertical ? nodAxis(body) : body.up;
  // Ondulación que crece hacia la punta de la cola; la raíz compensa
  for (const tail of chainsOf(body, "tail", "body")) {
    const n = tail.rotating.length;
    chainWave(pose, tail, () => axis, (i) => deg(8 + (16 * i) / Math.max(n - 1, 1)) * strength, t, cycles, 0.1);
  }
  pose.turn(body.root, axis, -deg(5) * strength * wave(t, cycles, -0.1));
  for (const fin of chainsOf(body, "fin")) {
    pose.turn(fin.rotating[0], liftAxis(body, segment(body, fin, 0)), deg(20) * wave(t, 2 * cycles));
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

// ─── Más generadores ────────────────────────────────────────────────────────

/** Curva por puntos (t, valor) con tramos suaves; fuera de rango, el extremo */
function curve(t: number, points: [number, number][]): number {
  if (t <= points[0][0]) return points[0][1];
  for (let i = 1; i < points.length; i++) {
    const [t1, v1] = points[i];
    const [t0, v0] = points[i - 1];
    if (t <= t1) return v0 + (v1 - v0) * smoothstep((t - t0) / Math.max(t1 - t0, 1e-9));
  }
  return points[points.length - 1][1];
}

/** Giro que lleva la dirección `from` a `to` (unitarias), en la fracción `amount` */
function aim(pose: PoseBuilder, joint: number, from: Vec3, to: Vec3, amount = 1): void {
  const axis = cross(from, to);
  if (length(axis) < 1e-6) return;
  pose.turn(joint, unit(axis, [1, 0, 0]), angleBetween(from, to) * amount);
}

/** Baja o sube el cuerpo para que la más baja de `joints` toque el suelo */
function plant(body: Body, pose: PoseBuilder, joints: number[]): void {
  if (joints.length === 0) return;
  const positions = jointPositions(body.bones, pose.rotations, pose.offset);
  pose.move([0, body.ground - Math.min(...joints.map((j) => positions[j][1])), 0]);
}

const uprightLegs = (body: Body) => limbs(body, "leg").filter((l) => !l.sprawl);
const tipOf = (chain: Chain) => chain.joints[chain.joints.length - 1];
const pos = (body: Body, j: number) => body.bones[j].position;
/** Pulso de 0 a 1 entre `a` y `b`: sube en la fracción `rise` del tramo y baja en el resto */
function pulse(t: number, a: number, b: number, rise = 0.3): number {
  if (t < a || t > b) return 0;
  const u = (t - a) / (b - a);
  return u < rise ? smoothstep(u / rise) : 1 - smoothstep((u - rise) / (1 - rise));
}

/** Brazo estirado hacia `point`: el hombro apunta ahí (la mano llega si el brazo está recto) */
function reach(body: Body, pose: PoseBuilder, arm: Chain, point: Vec3, amount: number): void {
  const shoulder = pos(body, arm.joints[0]);
  aim(pose, arm.rotating[0], segment(body, arm, 0), unit(sub(point, shoulder), body.forward), amount);
}

/** Lugar al frente del pecho, a la altura `drop` (en largos de brazo) bajo los hombros, abierto `open` hacia el lado */
function frontPoint(body: Body, arm: Chain, drop: number, open: number): Vec3 {
  const shoulder = pos(body, arm.joints[0]);
  const length_ = length(sub(pos(body, tipOf(arm)), shoulder));
  const root = pos(body, body.root);
  const lateral = dot(sub(shoulder, root), body.right);
  const side = lateral - Math.sign(lateral || arm.side) * (1 - open) * Math.abs(lateral);
  const down = drop * length_;
  const ahead = Math.sqrt(Math.max(0.05 * length_ ** 2, length_ ** 2 - (lateral - side) ** 2 - down ** 2));
  return add(add(add(shoulder, scale(body.right, side - lateral)), scale(body.forward, ahead)), scale(body.up, -down));
}

/** Codo doblado `angle` (positivo lleva el antebrazo hacia adelante) */
function bendElbow(body: Body, pose: PoseBuilder, arm: Chain, angle: number): void {
  if (arm.rotating.length < 2) return;
  const elbow = bendIndex(body, arm, /elbow/i);
  pose.turn(arm.rotating[elbow], swingAxis(body, segment(body, arm, elbow)), angle);
}

function gallop(body: Body, pose: PoseBuilder, t: number): void {
  // La espalda se encoge con las traseras adelante y se estira al volar
  const flex = wave(t, 1, 0.1);
  body.spine.forEach((j) => pose.turn(j, nodAxis(body), (deg(6) / Math.max(body.spine.length, 1)) * flex));
  gait(body, GAITS.gallop, pose, t);
}

function jump(body: Body, pose: PoseBuilder, t: number): void {
  const legs = uprightLegs(body);
  // Agacharse, despegar, recoger en el aire, amortiguar, pararse
  const crouch = curve(t, [[0, 0], [0.25, 1], [0.34, 0], [0.5, 0.35], [0.66, 0], [0.74, 0.8], [1, 0]]);
  const height = 0.35 * Math.max(...legs.map((l) => legSize(body, l)), 0.2 * body.height);
  const air = t > 0.34 && t < 0.68 ? Math.sin((Math.PI * (t - 0.34)) / 0.34) : 0;
  for (const leg of legs) bendLeg(body, pose, leg, deg(50) * crouch, deg(95) * crouch);
  if (body.spine[0] !== undefined) pose.turn(body.spine[0], nodAxis(body), deg(18) * crouch);
  // Brazos atrás al agacharse y arriba al saltar
  const swing = curve(t, [[0, 0], [0.25, -deg(40)], [0.42, deg(125)], [0.7, deg(30)], [0.85, 0]]);
  for (const arm of limbs(body, "arm")) pose.turn(arm.rotating[0], swingAxis(body, segment(body, arm, 0)), swing);
  for (const wing of chainsOf(body, "wing")) pose.turn(wing.rotating[0], liftAxis(body, segment(body, wing, 0)), deg(45) * air);
  for (const j of headJoints(body)) pose.turn(j, nodAxis(body), (-deg(10) * crouch) / headJoints(body).length);
  appendages(body, pose, t, 1, 0.5);
  plant(body, pose, legs.map(tipOf));
  pose.move(scale(body.up, height * air));
}

function clap(body: Body, pose: PoseBuilder, t: number): void {
  const arms = limbs(body, "arm");
  const raise = smoothstep(t / 0.15) * (1 - smoothstep((t - 0.85) / 0.15));
  // Cuatro palmadas entre la subida y la bajada
  const open = 0.5 + 0.5 * Math.cos(TAU * 4 * Math.min(1, Math.max(0, (t - 0.15) / 0.7)));
  for (const arm of arms) {
    reach(body, pose, arm, frontPoint(body, arm, 0.25, 0.05 + 0.5 * open), raise);
    bendElbow(body, pose, arm, deg(15) * raise);
  }
  const head = headJoints(body);
  head.forEach((j) => pose.turn(j, nodAxis(body), (deg(6) * raise) / head.length));
  appendages(body, pose, t, 2, 0.6);
}

function punch(body: Body, pose: PoseBuilder, t: number): void {
  const arms = limbs(body, "arm").sort((a, b) => a.side - b.side);
  arms.forEach((arm, k) => {
    // Un golpe por brazo: el izquierdo en la primera mitad, el derecho en la segunda
    const hit = pulse(t, k * 0.5, k * 0.5 + 0.5, 0.25);
    const guard = frontPoint(body, arm, 0.75, 0.45);
    const target = frontPoint(body, arm, 0.05, 0.15);
    const point = add(scale(guard, 1 - hit), scale(target, hit));
    reach(body, pose, arm, point, 1);
    bendElbow(body, pose, arm, deg(115) * (1 - hit) + deg(5) * hit);
    // El tronco gira para acompañar el golpe
    body.spine.forEach((j) => pose.turn(j, body.up, (-Math.sign(arm.side || 1) * deg(14) * hit) / body.spine.length));
  });
  for (const leg of uprightLegs(body)) bendLeg(body, pose, leg, deg(12), deg(22));
  pose.turn(body.root, body.up, deg(3) * wave(t, 2));
}

function kick(body: Body, pose: PoseBuilder, t: number): void {
  const legs = uprightLegs(body);
  const kicker = legs.find((l) => l.side > 0) ?? legs[0];
  const hip = curve(t, [[0, 0], [0.25, deg(75)], [0.4, deg(85)], [0.6, deg(60)], [0.8, 0]]);
  const knee = curve(t, [[0, 0], [0.25, deg(100)], [0.38, deg(5)], [0.5, deg(5)], [0.65, deg(90)], [0.85, 0]]);
  for (const leg of legs) {
    if (leg === kicker) bendLeg(body, pose, leg, hip, knee);
    else bendLeg(body, pose, leg, deg(10), deg(18));
  }
  const effort = Math.min(1, hip / deg(75));
  if (body.spine[0] !== undefined) pose.turn(body.spine[0], nodAxis(body), -deg(12) * effort);
  for (const arm of limbs(body, "arm")) {
    reach(body, pose, arm, frontPoint(body, arm, 0.7, 0.5), 0.8);
    bendElbow(body, pose, arm, deg(100));
  }
  appendages(body, pose, t, 1, 0.6);
}

function dance(body: Body, pose: PoseBuilder, t: number): void {
  // Cuatro tiempos: las rodillas rebotan en cada uno, el tronco va y viene cada dos
  const bounce = 0.5 - 0.5 * Math.cos(TAU * 4 * t);
  for (const leg of uprightLegs(body)) bendLeg(body, pose, leg, deg(14) * bounce, deg(28) * bounce);
  const sway = wave(t, 2);
  body.spine.forEach((j) => {
    pose.turn(j, body.forward, (deg(8) * sway) / body.spine.length);
    pose.turn(j, body.up, (deg(12) * sway) / body.spine.length);
  });
  limbs(body, "arm").forEach((arm) => {
    const up = Math.max(0, wave(t, 2, arm.side > 0 ? 0.5 : 0));
    pose.turn(arm.rotating[0], liftAxis(body, segment(body, arm, 0)), deg(100) * up);
    pose.turn(arm.rotating[0], swingAxis(body, segment(body, arm, 0)), deg(25));
    bendElbow(body, pose, arm, deg(30) + deg(50) * up);
  });
  const head = headJoints(body);
  head.forEach((j) => {
    pose.turn(j, nodAxis(body), (deg(8) * bounce) / head.length);
    pose.turn(j, body.forward, (-deg(6) * sway) / head.length);
  });
  appendages(body, pose, t, 4, 0.8);
}

function nod(body: Body, pose: PoseBuilder, t: number): void {
  const head = headJoints(body);
  // Tres veces que sí, con la cabeza volviendo arriba entre una y otra
  const down = (0.5 - 0.5 * Math.cos(TAU * 3 * t)) * smoothstep(t / 0.1) * (1 - smoothstep((t - 0.85) / 0.15));
  head.forEach((j) => pose.turn(j, nodAxis(body), (deg(22) * down) / head.length));
  appendages(body, pose, t, 1, 0.4);
}

function shakeHead(body: Body, pose: PoseBuilder, t: number): void {
  const head = headJoints(body);
  const envelope = Math.sin(Math.PI * t);
  head.forEach((j) => pose.turn(j, body.up, (deg(28) * envelope * wave(t, 3)) / head.length));
  appendages(body, pose, t, 1, 0.4);
}

/** Hocico: lo que más adelante llega de lo que cuelga de la primera articulación de la cabeza (no una oreja ni un cuerno) */
function muzzle(body: Body): number {
  const first = headJoints(body)[0];
  const children = body.bones.map(() => [] as number[]);
  body.bones.forEach((b, i) => b.parent !== null && children[b.parent]?.push(i));
  let far = first;
  const ahead = (j: number) => dot(sub(pos(body, j), pos(body, first)), body.forward);
  const visit = (j: number) => {
    if (ahead(j) > ahead(far)) far = j;
    children[j].forEach(visit);
  };
  visit(first);
  return far;
}

/** Inclinación (respecto de la horizontal hacia adelante) de lo que mira la cabeza */
function headPitch(body: Body): number {
  const first = pos(body, headJoints(body)[0]);
  const d = sub(pos(body, muzzle(body)), first);
  return Math.atan2(dot(d, body.up), dot(d, body.forward));
}

function eat(body: Body, pose: PoseBuilder, t: number): void {
  const head = headJoints(body);
  const trunks = chainsOf(body, "trunk");
  if (trunks.length > 0) {
    // Con trompa: la cabeza apenas baja y la trompa lleva la comida a la boca tres veces
    const curl = 0.5 - 0.5 * Math.cos(TAU * 3 * t);
    head.forEach((j) => pose.turn(j, nodAxis(body), (deg(8) * (1 - curl)) / head.length));
    for (const trunk of trunks) {
      const n = trunk.rotating.length;
      trunk.rotating.forEach((j, i) => pose.turn(j, liftAxis(body, segment(body, trunk, i)), (deg(150) * curl) / n));
    }
    appendages(body, pose, t, 2, 0.6);
    return;
  }
  const down = smoothstep((t - 0.08) / 0.15) * (1 - smoothstep((t - 0.8) / 0.15));
  // Lleva la cabeza a mirar 65° hacia abajo; la espalda ayuda un poco
  const needed = Math.max(0, headPitch(body) + deg(65));
  const spine = body.spine.length > 0 ? Math.min(deg(15), 0.25 * needed) : 0;
  body.spine.forEach((j) => pose.turn(j, nodAxis(body), (spine * down) / body.spine.length));
  const chew = deg(5) * wave(t, 10) * down;
  head.forEach((j) => pose.turn(j, nodAxis(body), ((needed - spine) * down + chew) / head.length));
  appendages(body, pose, t, 2, 0.8);
  // Cuello corto: el cuerpo se inclina adelante (sobre la cadera) hasta que el
  // hocico llegue cerca del suelo y las patas se doblan para no moverse
  const legs = uprightLegs(body).filter((l) => ikLegs(body).includes(l));
  if (legs.length < 2 || down <= 0) return;
  const tip = muzzle(body);
  const positions = jointPositions(body.bones, pose.rotations, pose.offset);
  const root = pos(body, body.root);
  const gap = positions[tip][1] - (body.ground + 0.04 * body.height);
  const ahead = dot(sub(positions[tip], root), body.forward);
  if (gap > 0 && ahead > 1e-6) pose.turn(body.root, nodAxis(body), Math.min(deg(25), Math.atan2(gap, ahead)) * down);
  legsOnGround(body, pose, legs);
}

/** Las patas con rodilla vuelven a pisar donde estaban en reposo (IK de dos huesos) */
function legsOnGround(body: Body, pose: PoseBuilder, legs: Chain[]): void {
  const fk = new Fk(body.bones, { rotations: pose.rotations, translations: new Map([[body.root, pose.offset]]), controls: new Map() });
  for (const leg of legs) {
    const k = bendIndex(body, leg, /knee|tibio|shin|calf/i);
    const [a, b, c] = [leg.joints[k - 1], leg.joints[k], leg.joints[k + 1]];
    solveTwoBone(fk, a, b, c, pos(body, c), { restNormal: legNormal(body, a, b, c) });
    // El tramo bajo (caña, pie) queda como en reposo: el casco no se mueve
    if (leg.joints.length > k + 2) fk.setWorld(c, IDENTITY);
  }
}

function shakeOff(body: Body, pose: PoseBuilder, t: number): void {
  const envelope = Math.sin(Math.PI * t) ** 2;
  const turns = 5;
  // Onda de giro que nace en la cabeza y llega a la cola
  const head = headJoints(body);
  head.forEach((j) => pose.turn(j, body.up, (deg(30) * envelope * wave(t, turns)) / head.length));
  body.spine.forEach((j, i) => pose.turn(j, body.forward, (deg(6) * envelope * wave(t, turns, 0.15 * (i + 1))) / body.spine.length));
  for (const tail of chainsOf(body, "tail")) {
    chainWave(pose, tail, () => body.up, () => (deg(30) * envelope) / Math.sqrt(tail.rotating.length), t, turns, 0.1, 0.4);
  }
  chainsOf(body, "ear").forEach((ear) =>
    chainWave(pose, ear, (i) => liftAxis(body, segment(body, ear, i)), () => deg(35) * envelope, t, turns, 0.1, ear.side > 0 ? 0.5 : 0)
  );
}

function wag(body: Body, pose: PoseBuilder, t: number): void {
  for (const tail of chainsOf(body, "tail")) {
    const n = tail.rotating.length;
    // Alzada y moviéndose rápido de lado a lado
    pose.turn(tail.rotating[0], nodAxis(body), deg(25));
    chainWave(pose, tail, () => body.up, () => deg(30) / Math.sqrt(n), t, 4, 0.08);
  }
  pose.turn(body.root, body.up, -deg(3) * wave(t, 4));
  const head = headJoints(body);
  head.forEach((j) => pose.turn(j, nodAxis(body), (-deg(5) - deg(2) * wave(t, 4)) / Math.max(head.length, 1)));
  chainsOf(body, "ear").forEach((ear) => pose.turn(ear.rotating[0], liftAxis(body, segment(body, ear, 0)), deg(10)));
}

function glide(body: Body, pose: PoseBuilder, t: number): void {
  for (const wing of chainsOf(body, "wing")) {
    pose.turn(wing.rotating[0], liftAxis(body, segment(body, wing, 0)), deg(6) + deg(3) * wave(t, 2));
  }
  // Vira a un lado y al otro: el cuerpo se inclina y la cola compensa
  const bank = wave(t);
  pose.turn(body.root, body.forward, deg(14) * bank);
  pose.turn(body.root, body.up, deg(6) * bank);
  for (const leg of chainsOf(body, "leg")) {
    if (leg.sprawl) pose.turn(leg.rotating[0], liftAxis(body, flat(leg.dir)), -deg(20));
    else pose.turn(leg.rotating[0], swingAxis(body, segment(body, leg, 0)), -deg(40));
  }
  for (const tail of chainsOf(body, "tail")) tail.rotating.forEach((j) => pose.turn(j, body.up, (-deg(8) * bank) / tail.rotating.length));
  const head = headJoints(body);
  head.forEach((j) => pose.turn(j, body.forward, (-deg(10) * bank) / head.length));
  pose.move(scale(body.up, 0.02 * body.size * wave(t, 2)));
  // En el aire: nada por debajo del suelo
  const lowest = Math.min(...jointPositions(body.bones, pose.rotations, pose.offset).map((p) => p[1]));
  if (lowest < body.ground) pose.move([0, body.ground - lowest + 0.05 * body.height, 0]);
}

function strike(body: Body, pose: PoseBuilder, t: number): void {
  const chain = chainsOf(body, "body")[0];
  const n = chain.rotating.length;
  const m = Math.max(2, Math.round(n * 0.35));
  // Cabeza alzada; se echa atrás enroscándose y se lanza
  const rise = curve(t, [[0, deg(30)], [0.35, deg(55)], [0.5, deg(15)], [0.65, deg(20)], [1, deg(30)]]);
  const coil = curve(t, [[0, 0.2], [0.35, 1], [0.48, 0], [0.7, 0], [1, 0.2]]);
  const lunge = curve(t, [[0, 0], [0.35, -0.08], [0.5, 0.25], [0.65, 0.22], [1, 0]]);
  const nodA = nodAxis(body);
  pose.turn(chain.rotating[0], nodA, -rise);
  pose.turn(chain.rotating[m], nodA, rise);
  // S del cuello: curvas alternadas entre la cabeza y la parte apoyada
  for (let i = 1; i < m; i++) pose.turn(chain.rotating[i], body.up, deg(28) * coil * (i % 2 ? 1 : -1));
  const length_ = length(sub(pos(body, tipOf(chain)), pos(body, chain.joints[0])));
  pose.move(scale(body.forward, lunge * length_));
  plant(body, pose, chain.joints.slice(m));
}

function pinch(body: Body, pose: PoseBuilder, t: number): void {
  const hit = pulse(t, 0.3, 0.75, 0.25);
  const raise = smoothstep(t / 0.2) * (1 - smoothstep((t - 0.8) / 0.2));
  for (const pincer of chainsOf(body, "pincer")) {
    const d = segment(body, pincer, 0);
    pose.turn(pincer.rotating[0], liftAxis(body, d), deg(30) * raise - deg(20) * hit);
    // La pinza se abre al alzarse y se cierra de golpe al atacar
    const claw = pincer.rotating[pincer.rotating.length - 1];
    pose.turn(claw, body.up, Math.sign(pincer.side || 1) * (deg(30) * raise * (1 - hit) - deg(5) * hit));
  }
  for (const tail of chainsOf(body, "tail")) {
    tail.rotating.forEach((j) => pose.turn(j, nodAxis(body), (deg(35) * hit) / tail.rotating.length));
  }
  pose.move(scale(body.forward, 0.04 * body.size * hit));
  appendages(body, pose, t, 2, 0.5);
}

function threat(body: Body, pose: PoseBuilder, t: number): void {
  const legs = chainsOf(body, "leg");
  const front = Math.max(...legs.map((l) => l.along));
  const raised = legs.filter((l) => l.along >= front - 1e-6);
  const up = smoothstep(t / 0.2) * (1 - smoothstep((t - 0.8) / 0.2));
  // Se alza adelante y agita las patas delanteras
  pose.turn(body.root, nodAxis(body), -deg(15) * up);
  for (const leg of raised) {
    pose.turn(leg.rotating[0], liftAxis(body, flat(leg.dir)), deg(45) * up + deg(15) * up * wave(t, 4, leg.side > 0 ? 0.5 : 0));
  }
  for (const pincer of chainsOf(body, "pincer")) {
    pose.turn(pincer.rotating[0], liftAxis(body, segment(body, pincer, 0)), deg(35) * up);
  }
  appendages(body, pose, t, 2, 1);
  plant(body, pose, legs.filter((l) => !raised.includes(l)).map(tipOf));
}

function tentacleCrawl(body: Body, pose: PoseBuilder, t: number): void {
  const tentacles = chainsOf(body, "tentacle");
  tentacles.forEach((arm, k) => {
    // Mitad de los brazos se enrosca mientras la otra se estira (alternados alrededor del cuerpo)
    chainWave(pose, arm, (i) => liftAxis(body, segment(body, arm, i)), () => deg(9), t, 2, 0.08, (k % 2) * 0.5 + 0.25, 1);
  });
  pose.turn(body.root, nodAxis(body), deg(4) * wave(t, 2));
  plant(body, pose, tentacles.flatMap((a) => a.joints));
}

/** Con patas a IK los pies ya pisan el suelo; si no, se baja el cuerpo hasta la pata más baja */
const grounded = (body: Body) => ikLegs(body).length === 0;

/**
 * Flexiones de lagartija: estira las patas delanteras (el pecho sube, el
 * cuerpo gira sobre la cadera) tres veces y la cabeza cabecea al subir
 */
function pushUps(body: Body, pose: PoseBuilder, t: number): void {
  const legs = chainsOf(body, "leg");
  const middle = legs.reduce((sum, l) => sum + l.along, 0) / legs.length;
  const front = legs.filter((l) => l.along > middle);
  const raise = 0.5 - 0.5 * Math.cos(TAU * 3 * t);
  const pitch = deg(14) * raise;
  pose.turn(body.root, nodAxis(body), -pitch);
  // Las delanteras bajan lo que sube el pecho, para seguir apoyadas
  const root = pos(body, body.root);
  for (const leg of front) {
    const shoulder = pos(body, leg.joints[0]);
    const tip = pos(body, tipOf(leg));
    const span = dot(sub(shoulder, root), body.forward);
    const reach = Math.max(length(flat(sub(tip, shoulder))), 1e-6);
    pose.turn(leg.rotating[0], liftAxis(body, flat(leg.dir)), -Math.atan2(span * Math.sin(pitch), reach));
  }
  const head = headJoints(body);
  head.forEach((j) => pose.turn(j, nodAxis(body), (pitch * 0.6 + deg(8) * raise * wave(t, 6)) / Math.max(head.length, 1)));
  // La cola queda apoyada: compensa el giro del cuerpo
  for (const tail of chainsOf(body, "tail")) pose.turn(tail.rotating[0], nodAxis(body), 1.2 * pitch);
  appendages(body, pose, t, 1, 0.4);
}

/** `v` girado `angle` alrededor del eje unitario `axis` */
function rotated(v: Vec3, axis: Vec3, angle: number): Vec3 {
  return rotate(axisAngle(axis, angle), v);
}

/** Endereza la cadena desde la articulación `from`: cada tramo sigue al anterior (en la fracción `amount`) */
function straighten(body: Body, pose: PoseBuilder, chain: Chain, from: number, amount = 1): void {
  for (let i = Math.max(from, 1); i < chain.rotating.length; i++) {
    aim(pose, chain.rotating[i], segment(body, chain, i), segment(body, chain, i - 1), amount);
  }
}

/** Sube el cuerpo lo justo para que nada quede bajo el suelo (cuerpos colgados) */
function clearGround(body: Body, pose: PoseBuilder): void {
  const lowest = Math.min(...jointPositions(body.bones, pose.rotations, pose.offset).map((p) => p[1]));
  const floor = body.ground + 0.02 * body.height;
  if (lowest < floor) pose.move([0, floor - lowest, 0]);
}

/**
 * Braquiar en el lugar, como el paso de caminar: la mano que agarra corre
 * de adelante hacia atrás mientras el cuerpo pasa por debajo como péndulo,
 * y la otra da la vuelta por abajo y llega adelante justo cuando se
 * cambian. Los dos brazos van rectos, así el ciclo no salta
 */
function brachiate(body: Body, pose: PoseBuilder, t: number): void {
  const arms = limbs(body, "arm").sort((a, b) => a.side - b.side);
  const half = t < 0.5 ? 0 : 1;
  const u = (t - 0.5 * half) / 0.5;
  const grip = arms[half];
  const free = arms[1 - half];
  const right = nodAxis(body);
  const reachOf = (arm: Chain) => chainLength(body, arm);
  const length_ = reachOf(grip);
  // La mano que agarra: de adelante (+d) a atrás (−d)
  const d = 0.6 * length_;
  const x = d * (1 - 2 * u);
  const gripDir = unit(add(scale(body.forward, x), scale(body.up, Math.sqrt(length_ * length_ - x * x))), body.up);
  // La libre: de arriba-atrás, por abajo, a arriba-adelante (una vuelta)
  const beta = Math.asin(0.6);
  const gamma = -beta - (2 * Math.PI - 2 * beta) * smoothstep(u);
  const freeDir = rotated(body.up, right, gamma);
  // El cuerpo se mece: los pies adelante a mitad del vaivén
  const tilt = -deg(25) * Math.sin(Math.PI * u);
  pose.turn(body.root, right, tilt);
  const inBody = (v: Vec3) => rotated(v, right, -tilt);
  for (const [arm, dir] of [
    [grip, gripDir],
    [free, freeDir],
  ] as const) {
    straighten(body, pose, arm, 1);
    aim(pose, arm.rotating[0], segment(body, arm, 0), inBody(dir));
  }
  // Piernas colgando, un poco recogidas
  for (const leg of limbs(body, "leg")) {
    pose.turn(leg.rotating[0], right, -0.4 * tilt - deg(15) * wave(t, 2));
    const k = bendIndex(body, leg, /knee|tibio|shin|calf/i);
    if (k < leg.rotating.length) pose.turn(leg.rotating[k], right, -deg(30));
  }
  // La cabeza mira adelante; la cola se alza y se balancea
  const head = headJoints(body);
  head.forEach((j) => pose.turn(j, right, -tilt / Math.max(head.length, 1)));
  for (const tail of wagging(body)) {
    tail.rotating.forEach((j, i) => pose.turn(j, right, (deg(40) + deg(15) * wave(t, 2, 0.05 * i)) / tail.rotating.length));
  }
  // La mano que agarra queda en su lugar sobre la rama
  const target = add(pos(body, grip.joints[0]), scale(gripDir, length_));
  const at = jointPositions(body.bones, pose.rotations, pose.offset)[tipOf(grip)];
  pose.move(sub(target, at));
  clearGround(body, pose);
}

/**
 * Colgado de la cola: cabeza abajo, la cola sube recta hasta la rama y se
 * enrosca en la punta; los brazos cuelgan hacia el suelo y el cuerpo se mece
 */
function tailHang(body: Body, pose: PoseBuilder, t: number): void {
  const right = nodAxis(body);
  const sway = deg(10) * wave(t);
  const tilt = Math.PI + sway;
  pose.turn(body.root, right, tilt);
  pose.turn(body.root, body.up, deg(8) * wave(t, 1, 0.25));
  const inBody = (v: Vec3) => rotated(v, right, -tilt);
  const tail = wagging(body).sort((a, b) => b.rotating.length - a.rotating.length)[0];
  const n = tail.rotating.length;
  aim(pose, tail.rotating[0], segment(body, tail, 0), inBody(body.up));
  straighten(body, pose, tail, 1);
  // La punta se enrosca en la rama
  const curl = Math.max(1, Math.round(0.3 * n));
  for (let i = n - curl; i < n; i++) pose.turn(tail.rotating[i], right, deg(70));
  for (const arm of limbs(body, "arm")) {
    aim(pose, arm.rotating[0], segment(body, arm, 0), inBody(scale(body.up, -1)));
    straighten(body, pose, arm, 1);
    pose.turn(arm.rotating[0], body.forward, arm.side * deg(12) * wave(t, 2));
  }
  for (const leg of limbs(body, "leg")) {
    pose.turn(leg.rotating[0], right, deg(35));
    const k = bendIndex(body, leg, /knee|tibio|shin|calf/i);
    if (k < leg.rotating.length) pose.turn(leg.rotating[k], right, -deg(60));
  }
  const head = headJoints(body);
  head.forEach((j) => pose.turn(j, right, -deg(20) / Math.max(head.length, 1)));
  // La punta de la cola queda fija arriba de la cadera
  const target = add(pos(body, body.root), scale(body.up, 0.95 * chainLength(body, tail)));
  const at = jointPositions(body.bones, pose.rotations, pose.offset)[tipOf(tail)];
  pose.move(sub(target, at));
  clearGround(body, pose);
}

/**
 * Salto de rana: se agacha, estira las patas traseras de golpe hacia atrás,
 * vuela con las delanteras adelante y cae sobre ellas
 */
function hop(body: Body, pose: PoseBuilder, t: number): void {
  const hind = foldedHindLegs(body);
  const front = chainsOf(body, "leg").filter((l) => !hind.includes(l));
  const right = nodAxis(body);
  const push = curve(t, [[0, 0], [0.15, 0], [0.3, 1], [0.6, 0.6], [0.75, 0], [1, 0]]);
  const air = t > 0.28 && t < 0.72 ? Math.sin((Math.PI * (t - 0.28)) / 0.44) : 0;
  const land = pulse(t, 0.68, 0.95, 0.3);
  // Cuerpo: se levanta adelante al empujar y baja la nariz al caer
  pose.turn(body.root, right, -deg(12) * push + deg(12) * land);
  for (const leg of hind) {
    const back = unit(add(add(scale(body.forward, -0.8), scale(body.up, -0.5)), scale(body.right, 0.35 * leg.side)), [0, -1, 0]);
    aim(pose, leg.rotating[0], segment(body, leg, 0), back, push);
    straighten(body, pose, leg, 1, push);
  }
  for (const leg of front) {
    const ahead = unit(add(scale(body.forward, 0.7), scale(body.up, -0.7)), [0, -1, 0]);
    aim(pose, leg.rotating[0], segment(body, leg, 0), ahead, 0.6 * air);
  }
  for (const j of headJoints(body)) pose.turn(j, right, deg(10) * push);
  appendages(body, pose, t, 1, 0.4);
  plant(body, pose, chainsOf(body, "leg").map(tipOf));
  pose.move(scale(body.up, 0.45 * body.height * air));
}

/** Morder: colmillos o mandíbulas se cierran hacia el medio, tres veces */
function bite(body: Body, pose: PoseBuilder, t: number): void {
  const close = 0.5 - 0.5 * Math.cos(TAU * 3 * t);
  for (const fang of chainsOf(body, "mouth")) {
    const side = fang.side || 1;
    // Abiertas hacia afuera y cerradas hacia el medio
    pose.turn(fang.rotating[0], body.up, side * (deg(25) * (1 - close) - deg(10) * close));
  }
  const head = headJoints(body);
  head.forEach((j) => pose.turn(j, nodAxis(body), (deg(6) * close) / Math.max(head.length, 1)));
  appendages(body, pose, t, 3, 0.6);
}

/**
 * Paso de cangrejo: cada pata se levanta y se dobla o se estira hacia el
 * costado (las de un lado tiran mientras las del otro empujan) y el cuerpo
 * se mece de lado a lado
 */
function sideWalk(body: Body, pose: PoseBuilder, t: number): void {
  const legs = chainsOf(body, "leg");
  const sides = [legs.filter((l) => l.side < 0), legs.filter((l) => l.side > 0)];
  sides.forEach((s) => s.sort((a, b) => b.along - a.along));
  for (const s of sides) {
    s.forEach((leg, pair) => {
      const phase = ((pair + (leg.side < 0 ? 0 : 1)) % 2) * 0.5;
      const { swing, lift } = stride((((t + phase) % 1) + 1) % 1, 0.5);
      const d = flat(leg.dir);
      pose.turn(leg.rotating[0], liftAxis(body, d), deg(18) * lift);
      if (leg.rotating.length > 1) pose.turn(leg.rotating[1], liftAxis(body, segment(body, leg, 1)), deg(20) * swing * leg.side);
    });
  }
  pose.move(scale(body.right, 0.02 * body.size * wave(t, 2)));
  for (const pincer of chainsOf(body, "pincer")) pose.turn(pincer.rotating[0], liftAxis(body, segment(body, pincer, 0)), deg(8) * wave(t, 2));
  appendages(body, pose, t, 2, 0.5);
}

/** Picar: el abdomen se curva por debajo y adelante, dos veces */
function sting(body: Body, pose: PoseBuilder, t: number): void {
  const curl = pulse(t, 0.1, 0.45, 0.4) + pulse(t, 0.55, 0.9, 0.4);
  for (const abdomen of stingers(body)) {
    abdomen.rotating.forEach((j) => pose.turn(j, nodAxis(body), (-deg(70) * curl) / abdomen.rotating.length));
  }
  for (const wing of chainsOf(body, "wing")) pose.turn(wing.rotating[0], liftAxis(body, segment(body, wing, 0)), deg(25) * wave(t, 8));
  appendages(body, pose, t, 2, 0.5);
}

/** Portante: como el paso, con las dos patas de cada lado juntas y el cuerpo meciéndose hacia el lado que apoya */
function pace(body: Body, pose: PoseBuilder, t: number): void {
  pose.turn(body.root, body.forward, deg(4) * wave(t, 1, 0.5));
  gait(body, GAITS.pace, pose, t);
}

/**
 * Patas para echarse, si las delanteras tienen carpo y menudillo (hombro,
 * codo, carpo, menudillo, casco) y hay cuatro con IK
 */
function kneelingLegs(body: Body): { front: Chain[]; back: Chain[] } | null {
  const legs = ikLegs(body);
  if (legs.length !== 4) return null;
  const sorted = [...legs].sort((a, b) => b.along - a.along);
  const [front, back] = [sorted.slice(0, 2), sorted.slice(2)];
  const long = (l: Chain) => l.joints.length >= bendIndex(body, l, /knee|tibio|shin|calf/i) + 4;
  return front.every(long) && front[0].side !== front[1].side ? { front, back } : null;
}

/**
 * Echarse como un camélido o un caballo: baja de rodillas (los carpos al
 * suelo, las cañas plegadas atrás), dobla las traseras, apoya el pecho,
 * descansa y se levanta al revés: primero el pecho, luego atrás, luego adelante
 */
function lieDown(body: Body, pose: PoseBuilder, t: number): void {
  const legs = kneelingLegs(body);
  if (!legs) return;
  const ramp = (a: number, b: number) => smoothstep((t - a) / (b - a));
  const kneel = ramp(0.08, 0.28) * (1 - ramp(0.8, 0.94));
  const fold = ramp(0.28, 0.46) * (1 - ramp(0.68, 0.8));
  const settle = ramp(0.44, 0.54) * (1 - ramp(0.62, 0.68));
  const parts = (leg: Chain) => {
    const k = bendIndex(body, leg, /knee|tibio|shin|calf/i);
    const [a, b, c] = [leg.joints[k - 1], leg.joints[k], leg.joints[k + 1]];
    const upper = length(sub(pos(body, b), pos(body, a))) + length(sub(pos(body, c), pos(body, b)));
    const lower = length(sub(pos(body, tipOf(leg)), pos(body, c)));
    return { leg, k, a, b, c, upper, lower, height: pos(body, a)[1] - body.ground };
  };
  const front = legs.front.map(parts);
  const back = legs.back.map(parts);
  const avg = (xs: number[]) => xs.reduce((s, x) => s + x, 0) / xs.length;
  // Altura de hombros y caderas sobre el suelo: de rodillas el carpo toca el
  // suelo y el brazo queda casi recto; echado, todo se recoge a la mitad
  const frontRest = avg(front.map((f) => f.height));
  const backRest = avg(back.map((f) => f.height));
  const frontUpper = avg(front.map((f) => f.upper));
  const backUpper = avg(back.map((f) => f.upper));
  const frontHeight = frontRest + (0.9 * frontUpper - frontRest) * kneel + (0.5 * frontUpper - 0.9 * frontUpper) * settle;
  const backHeight = backRest + (0.5 * backUpper - backRest) * fold;
  const frontDrop = frontRest - frontHeight;
  const backDrop = backRest - backHeight;
  // El cuerpo gira sobre la raíz (cerca de las caderas) y baja
  const along = (j: number) => dot(sub(pos(body, j), pos(body, body.root)), body.forward);
  const span = avg(front.map((f) => along(f.a))) - avg(back.map((f) => along(f.a)));
  const pitch = Math.asin(Math.max(-0.9, Math.min(0.9, (frontDrop - backDrop) / Math.max(span, 1e-6))));
  pose.turn(body.root, nodAxis(body), pitch);
  // La cabeza sigue derecha: el cuello compensa el giro del cuerpo
  const head = headJoints(body);
  head.forEach((j) => pose.turn(j, nodAxis(body), (-pitch + deg(4) * wave(t, 3)) / head.length));
  appendages(body, pose, t, 2, 0.5);
  // La cola se alza para no hundirse en el suelo al bajar la grupa
  for (const tail of chainsOf(body, "tail")) pose.turn(tail.rotating[0], nodAxis(body), deg(35) * (backDrop / Math.max(backRest, 1e-6)));
  pose.move(scale(body.up, -backDrop));

  // Patas: la articulación de abajo (carpo o corvejón) baja al suelo y la
  // caña se pliega lo justo para que el casco siga apoyado
  const fk = new Fk(body.bones, { rotations: pose.rotations, translations: new Map([[body.root, pose.offset]]), controls: new Map() });
  const lateral = cross(body.up, body.forward);
  const floor = body.ground + 0.02 * body.height;
  for (const [group, amount, sign] of [
    [front, kneel, 1],
    [back, fold, -1],
  ] as const) {
    for (const f of group) {
      const rest = pos(body, f.c);
      const target: Vec3 = [rest[0], rest[1] + (floor - rest[1]) * amount, rest[2]];
      solveTwoBone(fk, f.a, f.b, f.c, target, { restNormal: legNormal(body, f.a, f.b, f.c) });
      if (f.leg.joints.length > f.k + 2) {
        // Delanteras: la caña atrás; traseras: adelante, bajo el vientre. El
        // menor pliegue que deja el casco sobre el suelo (búsqueda binaria:
        // plegar más lo sube)
        const tipY = (angle: number) => {
          fk.setWorld(f.c, axisAngle(lateral, sign * angle));
          return fk.position(tipOf(f.leg))[1];
        };
        let [lo, hi] = [0, deg(110)];
        if (tipY(lo) < body.ground) {
          for (let i = 0; i < 20; i++) {
            const mid = 0.5 * (lo + hi);
            if (tipY(mid) < body.ground) lo = mid;
            else hi = mid;
          }
          tipY(hi);
        }
      }
    }
  }
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
      return bake(name, body, { frames, step: 2, grounded: grounded(body), fps }, (p, t) => gait(body, g, p, t));
    }
    case "run": {
      const g = sprawlingQuadruped(body) ? GAITS.sprint : GAITS.run;
      return bake(name, body, { frames: g.frames, step: 1, grounded: grounded(body), fps }, (p, t) => gait(body, g, p, t));
    }
    case "bite":
      return bake(name, body, { frames: 36, step: 1, grounded: legs.length > 0, fps }, (p, t) => bite(body, p, t));
    case "sideWalk":
      return bake(name, body, { frames: 24, step: 1, grounded: true, fps }, (p, t) => sideWalk(body, p, t));
    case "sting":
      return bake(name, body, { frames: 36, step: 1, grounded: legs.length > 0, fps }, (p, t) => sting(body, p, t));
    case "brachiate":
      return bake(name, body, { frames: 48, step: 1, fps }, (p, t) => brachiate(body, p, t));
    case "tailHang":
      return bake(name, body, { frames: 72, step: 2, fps }, (p, t) => tailHang(body, p, t));
    case "hop":
      return bake(name, body, { frames: 36, step: 1, fps }, (p, t) => hop(body, p, t));
    case "pushUps":
      return bake(name, body, { frames: 48, step: 1, grounded: true, fps }, (p, t) => pushUps(body, p, t));
    case "trot":
      return bake(name, body, { frames: GAITS.trot.frames, step: 1, grounded: grounded(body), fps }, (p, t) =>
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
    case "gallop":
      return bake(name, body, { frames: GAITS.gallop.frames, step: 1, grounded: grounded(body), fps }, (p, t) => gallop(body, p, t));
    case "jump":
      return bake(name, body, { frames: 36, step: 1, fps }, (p, t) => jump(body, p, t));
    case "clap":
      return bake(name, body, { frames: 48, step: 1, grounded: legs.length > 0, fps }, (p, t) => clap(body, p, t));
    case "punch":
      return bake(name, body, { frames: 32, step: 1, grounded: true, fps }, (p, t) => punch(body, p, t));
    case "kick":
      return bake(name, body, { frames: 36, step: 1, grounded: true, fps }, (p, t) => kick(body, p, t));
    case "dance":
      return bake(name, body, { frames: 48, step: 1, grounded: true, fps }, (p, t) => dance(body, p, t));
    case "nod":
      return bake(name, body, { frames: 36, step: 2, grounded: legs.length > 0, fps }, (p, t) => nod(body, p, t));
    case "shakeHead":
      return bake(name, body, { frames: 36, step: 2, grounded: legs.length > 0, fps }, (p, t) => shakeHead(body, p, t));
    case "eat":
      return bake(name, body, { frames: 72, step: 2, grounded: true, fps }, (p, t) => eat(body, p, t));
    case "shake":
      return bake(name, body, { frames: 36, step: 1, grounded: true, fps }, (p, t) => shakeOff(body, p, t));
    case "wag":
      return bake(name, body, { frames: 24, step: 1, grounded: legs.length > 0, fps }, (p, t) => wag(body, p, t));
    case "glide":
      return bake(name, body, { frames: 72, step: 4, fps }, (p, t) => glide(body, p, t));
    case "swimFast":
      return bake(name, body, { frames: 36, step: 2, fps }, (p, t) => swim(body, p, t, !!options.verticalSwim, 2, 1.3));
    case "strike":
      return bake(name, body, { frames: 36, step: 1, fps }, (p, t) => strike(body, p, t));
    case "pinch":
      return bake(name, body, { frames: 32, step: 1, grounded: legs.length > 0, fps }, (p, t) => pinch(body, p, t));
    case "threat":
      return bake(name, body, { frames: 36, step: 2, fps }, (p, t) => threat(body, p, t));
    case "tentacleCrawl":
      return bake(name, body, { frames: 48, step: 2, fps }, (p, t) => tentacleCrawl(body, p, t));
    case "pace":
      return bake(name, body, { frames: 28, step: 2, grounded: grounded(body), fps }, (p, t) => pace(body, p, t));
    case "lieDown":
      return bake(name, body, { frames: 120, step: 2, fps }, (p, t) => lieDown(body, p, t));
  }
}

// ─── Poses de fábrica ───────────────────────────────────────────────────────

export type FactoryPoseId =
  | "crouch"
  | "sit"
  | "armsDown"
  | "armsUp"
  | "wingsFolded"
  | "wingsOpen"
  | "headDown"
  | "tailCurl"
  | "fist"
  | "handOpen";

export interface FactoryPose {
  id: FactoryPoseId;
  name: string;
  description: string;
}

const POSES: Record<FactoryPoseId, Omit<FactoryPose, "id">> = {
  crouch: { name: "Agachado", description: "Rodillas dobladas, los pies en el suelo" },
  sit: { name: "Sentado", description: "Bípedo en una silla; cuadrúpedo sobre las patas traseras" },
  armsDown: { name: "Brazos abajo", description: "Los brazos cuelgan junto al cuerpo" },
  armsUp: { name: "Brazos arriba", description: "Los brazos apuntan al cielo" },
  wingsFolded: { name: "Alas plegadas", description: "Alas recogidas contra el cuerpo" },
  wingsOpen: { name: "Alas abiertas", description: "Alas extendidas y un poco levantadas" },
  headDown: { name: "Cabeza gacha", description: "Cuello y cabeza hacia abajo" },
  tailCurl: { name: "Cola enroscada", description: "La cola se enrolla hacia arriba" },
  fist: { name: "Mano cerrada", description: "Los dedos se doblan hacia la palma" },
  handOpen: { name: "Mano abierta", description: "Los dedos rectos, como en reposo" },
};

const FINGER = /finger|thumb|index|middle|ring|pinky|digit|dedo|pulgar|meñique/i;

/** Cadenas de dedos (por nombre): el análisis del cuerpo las toma como brazos u otras */
const fingerChains = (body: Body) => body.chains.filter((c) => c.joints.some((j) => FINGER.test(body.bones[j].name)));
const limbs = (body: Body, kind: ChainKind) => chainsOf(body, kind).filter((c) => !fingerChains(body).includes(c));

/** Poses de fábrica que tienen sentido para este esqueleto */
export function availablePoses(body: Body): FactoryPose[] {
  const ids: FactoryPoseId[] = [];
  const legs = limbs(body, "leg");
  if (legs.some((l) => !l.sprawl && l.rotating.length >= 2)) ids.push("crouch");
  if (legs.filter((l) => !l.sprawl).length >= 2) ids.push("sit");
  if (limbs(body, "arm").length > 0) ids.push("armsDown", "armsUp");
  if (chainsOf(body, "wing").length > 0) ids.push("wingsFolded", "wingsOpen");
  if (headJoints(body).length > 0) ids.push("headDown");
  if (chainsOf(body, "tail").length > 0) ids.push("tailCurl");
  if (fingerChains(body).length > 0) ids.push("fist", "handOpen");
  return ids.map((id) => ({ id, ...POSES[id] }));
}

/** Ángulo entre dos direcciones unitarias */
const angleBetween = (a: Vec3, b: Vec3) => Math.acos(Math.max(-1, Math.min(1, dot(a, b))));

/** Dobla una pata: cadera adelante, rodilla atrás y tobillo parejo */
function bendLeg(body: Body, pose: PoseBuilder, leg: Chain, hip: number, knee: number): void {
  pose.turn(leg.rotating[0], swingAxis(body, segment(body, leg, 0)), hip);
  if (leg.rotating.length < 2) return;
  const k = bendIndex(body, leg, /knee|tibio|shin|calf/i);
  pose.turn(leg.rotating[k], swingAxis(body, segment(body, leg, k)), -knee);
  if (k + 1 < leg.rotating.length) pose.turn(leg.rotating[k + 1], swingAxis(body, segment(body, leg, k + 1)), knee - hip);
}

/**
 * Pose de fábrica: giros locales por articulación y, si la pose baja o sube
 * el cuerpo para dejar las patas en el suelo, el desplazamiento de la raíz
 */
export function generatePose(body: Body, id: FactoryPoseId): { rotations: Map<number, Quat>; offset: Vec3 | null } {
  const pose = new PoseBuilder();
  const legs = limbs(body, "leg").filter((l) => !l.sprawl);
  let grounded = false;
  switch (id) {
    case "crouch":
      for (const leg of legs) bendLeg(body, pose, leg, deg(55), deg(105));
      for (const leg of limbs(body, "leg").filter((l) => l.sprawl)) pose.turn(leg.rotating[0], liftAxis(body, flat(leg.dir)), deg(15));
      if (body.spine[0] !== undefined) pose.turn(body.spine[0], nodAxis(body), deg(12));
      grounded = true;
      break;
    case "sit": {
      if (legs.length === 2) {
        for (const leg of legs) bendLeg(body, pose, leg, deg(85), deg(85));
      } else {
        // Cuadrúpedo: las traseras muy dobladas, el cuerpo se levanta adelante
        const mid = legs.reduce((sum, l) => sum + l.along, 0) / Math.max(legs.length, 1);
        for (const leg of legs.filter((l) => l.along <= mid)) bendLeg(body, pose, leg, deg(70), deg(120));
        pose.turn(body.root, nodAxis(body), -deg(30));
        for (const leg of legs.filter((l) => l.along > mid)) pose.turn(leg.rotating[0], swingAxis(body, segment(body, leg, 0)), -deg(30));
      }
      grounded = true;
      break;
    }
    case "armsDown":
    case "armsUp":
      for (const arm of limbs(body, "arm")) {
        const d = segment(body, arm, 0);
        const target = id === "armsDown" ? scale(body.up, -1) : body.up;
        // Casi hasta la vertical, sin meterse en el cuerpo
        const angle = angleBetween(d, target) - deg(12);
        if (angle > 0) pose.turn(arm.rotating[0], liftAxis(body, d), id === "armsUp" ? angle : -angle);
      }
      break;
    case "wingsFolded":
      for (const wing of chainsOf(body, "wing")) {
        // Zigzag hacia atrás: brazo atrás, antebrazo adelante, mano atrás
        const turns = [-deg(70), deg(150), -deg(150)];
        wing.rotating.forEach((j, i) => pose.turn(j, swingAxis(body, segment(body, wing, i)), turns[Math.min(i, 2)]));
        pose.turn(wing.rotating[0], liftAxis(body, segment(body, wing, 0)), -deg(15));
      }
      break;
    case "wingsOpen":
      for (const wing of chainsOf(body, "wing")) pose.turn(wing.rotating[0], liftAxis(body, segment(body, wing, 0)), deg(20));
      break;
    case "headDown": {
      const head = headJoints(body);
      head.forEach((j) => pose.turn(j, nodAxis(body), deg(35) / head.length));
      break;
    }
    case "tailCurl":
      for (const tail of chainsOf(body, "tail")) {
        const n = tail.rotating.length;
        tail.rotating.forEach((j, i) => pose.turn(j, liftAxis(body, segment(body, tail, i)), deg(Math.min(60, 240 / n))));
      }
      break;
    case "fist":
      for (const finger of fingerChains(body)) {
        const thumb = finger.joints.some((j) => /thumb|pulgar/i.test(body.bones[j].name));
        finger.rotating.forEach((j, i) =>
          pose.turn(j, liftAxis(body, segment(body, finger, i)), -(thumb ? deg(30) : deg(75)))
        );
      }
      break;
    case "handOpen":
      for (const finger of fingerChains(body)) for (const j of finger.rotating) pose.rotations.set(j, IDENTITY);
      break;
  }
  if (grounded) {
    const tips = legs.map((c) => c.joints[c.joints.length - 1]);
    if (tips.length > 0) {
      const positions = jointPositions(body.bones, pose.rotations, pose.offset);
      const lowest = Math.min(...tips.map((j) => positions[j][1]));
      pose.move([0, body.ground - lowest, 0]);
    }
  }
  return { rotations: pose.rotations, offset: length(pose.offset) > 1e-9 * body.size ? pose.offset : null };
}
