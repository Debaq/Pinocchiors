/**
 * Movimiento secundario (F8): huesos resorte y seguimiento con retardo.
 *
 * Un hueso con resorte no gira por sus keys sino por una punta con masa
 * que persigue a la punta animada: con rigidez, amortiguación y gravedad
 * (orejas, trompa, cola, papada, antenas). Con rigidez baja y más
 * amortiguación es el "seguimiento": la cola repite el movimiento del
 * cuerpo con retardo, sin keys. Las puntas no bajan del suelo.
 *
 * La pila evalúa un cuadro suelto, pero un resorte necesita la historia:
 * el estado hasta el cuadro anterior se simula con el clip (y se guarda por
 * clip y rig) y el último paso usa la pose que llega a la pila, así los
 * resortes reaccionan también a lo que se está arrastrando. Se hornea como
 * todo lo demás.
 */

import * as THREE from "three";
import type { AnimationClip, Pose } from "./animation";
import { clonePose } from "./animation";
import { Fk, rotationBetween } from "./ik";
import type { RigContext } from "./rig";

export interface SpringSettings {
  /** 0–1: de blando (cuelga y se atrasa) a duro (casi sigue la animación) */
  stiffness: number;
  /** 0–1: cuánto frena el rebote (1 = sin rebote) */
  damping: number;
  /** 0–1: cuánto lo tira hacia abajo */
  gravity: number;
  /** No atraviesa el resto del cuerpo (por defecto sí) */
  collide?: boolean;
}

export const SPRING_PRESETS: { id: string; label: string; settings: SpringSettings }[] = [
  { id: "jiggle", label: "Resorte (rebota)", settings: { stiffness: 0.55, damping: 0.25, gravity: 0.1 } },
  { id: "follow", label: "Seguimiento (se atrasa)", settings: { stiffness: 0.3, damping: 0.7, gravity: 0 } },
  { id: "heavy", label: "Pesado (cuelga)", settings: { stiffness: 0.2, damping: 0.5, gravity: 0.6 } },
];

interface Particle {
  p: THREE.Vector3;
  v: THREE.Vector3;
}
type SpringState = Map<number, Particle>;

export interface SecondaryDeps {
  /** Pose del clip en `frame` con las etapas anteriores de la pila (sin resortes) */
  before: (clip: AnimationClip, frame: number) => Pose;
  /** Altura del suelo para la punta `j` */
  ground: (j: number) => number;
}

/** Articulaciones con resorte (y un hijo que dé la punta), de la raíz a las puntas */
function springJoints(ctx: RigContext): { j: number; tip: number; s: SpringSettings }[] {
  const out: { j: number; tip: number; s: SpringSettings; depth: number }[] = [];
  ctx.bones.forEach((b, j) => {
    const s = ctx.settings.bones[b.name]?.spring;
    const tip = ctx.children[j][0];
    if (!s || tip === undefined) return;
    let depth = 0;
    for (let p = b.parent; p !== null; p = ctx.bones[p].parent) depth++;
    out.push({ j, tip, s, depth });
  });
  return out.sort((a, b) => a.depth - b.depth);
}

/**
 * Un paso de simulación: cada punta persigue a la animada y el hueso gira
 * hacia ella. Modifica `pose` y devuelve el estado nuevo.
 */
function step(pose: Pose, ctx: RigContext, prev: SpringState | null, dt: number, deps: SecondaryDeps): SpringState {
  const joints = springJoints(ctx);
  const fk = new Fk(ctx.bones, pose);
  const radius = COLLISION_RADIUS * (ctx.body?.size ?? 1);
  const next: SpringState = new Map();
  const size = ctx.body?.size ?? 1;
  const up = new THREE.Vector3(...(ctx.body?.up ?? [0, 1, 0]));
  const SUBSTEPS = 4;
  const h = dt / SUBSTEPS;
  for (const { j, tip, s } of joints) {
    const root = new THREE.Vector3(...fk.position(j));
    const target = new THREE.Vector3(...fk.position(tip));
    const length = target.distanceTo(root);
    if (length < 1e-9) continue;
    const old = prev?.get(j);
    const particle: Particle = old ? { p: old.p.clone(), v: old.v.clone() } : { p: target.clone(), v: new THREE.Vector3() };
    // Frecuencia de 0.5 a 10 Hz; amortiguación relativa a la crítica
    const omega = 2 * Math.PI * (0.5 + 9.5 * s.stiffness);
    const k = omega * omega;
    const c = 2 * Math.max(0.02, s.damping) * omega;
    const g = up.clone().multiplyScalar(-9.8 * s.gravity * (size / 1.7));
    for (let n = 0; n < SUBSTEPS; n++) {
      const a = target.clone().sub(particle.p).multiplyScalar(k).sub(particle.v.clone().multiplyScalar(c)).add(g);
      particle.v.addScaledVector(a, h);
      particle.p.addScaledVector(particle.v, h);
      // El hueso no se estira: la punta queda a su largo de la articulación
      const dir = particle.p.clone().sub(root);
      if (dir.lengthSq() < 1e-18) dir.copy(target).sub(root);
      particle.p.copy(root).addScaledVector(dir.normalize(), length);
    }
    // El resto del cuerpo: cápsulas en los huesos que no son de la cadena
    if (s.collide !== false) {
      for (const [a, b] of colliders(ctx, j)) {
        const A = new THREE.Vector3(...fk.position(a));
        const B = new THREE.Vector3(...fk.position(b));
        const closest = closestOnSegment(particle.p, A, B);
        const away = particle.p.clone().sub(closest);
        const d = away.length();
        if (d >= radius) continue;
        particle.p.copy(closest).addScaledVector(d > 1e-9 ? away.divideScalar(d) : new THREE.Vector3(...(ctx.body?.up ?? [0, 1, 0])), radius);
        const dir = particle.p.clone().sub(root);
        particle.p.copy(root).addScaledVector(dir.normalize(), length);
      }
    }
    // Suelo
    const floor = deps.ground(tip);
    if (particle.p.y < floor) {
      particle.p.y = floor;
      particle.v.y = Math.max(0, particle.v.y);
    }
    // Sin velocidad a lo largo del hueso (la restricción de largo la quita)
    const axis = particle.p.clone().sub(root).normalize();
    particle.v.addScaledVector(axis, -particle.v.dot(axis));
    next.set(j, particle);
    const have = target.clone().sub(root);
    const want = particle.p.clone().sub(root);
    fk.rotate(j, rotationBetween([have.x, have.y, have.z], [want.x, want.y, want.z]));
  }
  return next;
}

/** Radio de las cápsulas del cuerpo, relativo al tamaño del esqueleto */
const COLLISION_RADIUS = 0.045;

const colliderCache = new WeakMap<RigContext, Map<number, [number, number][]>>();

/**
 * Segmentos del cuerpo con los que choca el resorte de `j`: todos menos los
 * de su propia cadena (sus descendientes) y los de sus dos antecesores, que
 * nacen pegados a él
 */
function colliders(ctx: RigContext, j: number): [number, number][] {
  let byJoint = colliderCache.get(ctx);
  if (!byJoint) {
    byJoint = new Map();
    colliderCache.set(ctx, byJoint);
  }
  const cached = byJoint.get(j);
  if (cached) return cached;
  const skip = new Set<number>([j]);
  const stack = [...ctx.children[j]];
  while (stack.length > 0) {
    const c = stack.pop()!;
    skip.add(c);
    stack.push(...ctx.children[c]);
  }
  for (let p = ctx.bones[j].parent, k = 0; p !== null && k < 2; p = ctx.bones[p].parent, k++) skip.add(p);
  const out: [number, number][] = [];
  ctx.bones.forEach((b, i) => {
    if (b.parent === null || skip.has(i) || skip.has(b.parent)) return;
    // Otros resortes se mueven solos: no son cuerpo
    if (ctx.settings.bones[ctx.bones[b.parent].name]?.spring) return;
    out.push([b.parent, i]);
  });
  byJoint.set(j, out);
  return out;
}

function closestOnSegment(p: THREE.Vector3, a: THREE.Vector3, b: THREE.Vector3): THREE.Vector3 {
  const ab = b.clone().sub(a);
  const len = ab.lengthSq();
  const t = len > 1e-18 ? Math.max(0, Math.min(1, p.clone().sub(a).dot(ab) / len)) : 0;
  return a.clone().addScaledVector(ab, t);
}

interface ClipCache {
  /** Estado después de cada cuadro simulado (índice: cuadro − inicio) */
  states: SpringState[];
  /** Estado inicial (en un ciclo, el del final de una vuelta previa) */
  initial: SpringState | null;
}

const caches = new WeakMap<RigContext, WeakMap<AnimationClip, ClipCache>>();

/** Estado del resorte después del cuadro `frame` del clip (simulando lo que falte) */
function stateAfter(clip: AnimationClip, frame: number, ctx: RigContext, deps: SecondaryDeps): SpringState | null {
  const start = Math.round(clip.start);
  if (frame < start) return null;
  let byClip = caches.get(ctx);
  if (!byClip) {
    byClip = new WeakMap();
    caches.set(ctx, byClip);
  }
  let cache = byClip.get(clip);
  const dt = 1 / (clip.fps > 0 ? clip.fps : 24);
  if (!cache) {
    let initial: SpringState | null = null;
    // En un ciclo, el primer cuadro sigue al último: se da una vuelta antes
    if (clip.cyclic) {
      for (let f = start; f <= Math.round(clip.end); f++) initial = step(clonePose(deps.before(clip, f)), ctx, initial, dt, deps);
    }
    cache = { states: [], initial };
    byClip.set(clip, cache);
  }
  const index = Math.min(frame, Math.round(clip.end)) - start;
  for (let i = cache.states.length; i <= index; i++) {
    const prev = i === 0 ? cache.initial : cache.states[i - 1];
    cache.states.push(step(clonePose(deps.before(clip, start + i)), ctx, prev, dt, deps));
  }
  return cache.states[index] ?? null;
}

/** Etapa de la pila: los huesos con resorte siguen a su punta simulada */
export function applySecondary(pose: Pose, ctx: RigContext, info: { clip?: AnimationClip; frame?: number }, deps: SecondaryDeps): Pose {
  if (!info.clip || springJoints(ctx).length === 0) return pose;
  const frame = Math.round(info.frame ?? info.clip.start);
  const clip = info.clip;
  const prev = frame <= Math.round(clip.start) ? (clip.cyclic ? stateAfter(clip, Math.round(clip.end), ctx, deps) : null) : stateAfter(clip, frame - 1, ctx, deps);
  const out = clonePose(pose);
  step(out, ctx, prev, 1 / (clip.fps > 0 ? clip.fps : 24), deps);
  return out;
}
