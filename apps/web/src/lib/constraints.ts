/**
 * Restricciones del rig (F3): etapa de la pila entre las keys y el IK.
 *
 * Cada restricción actúa sobre una articulación ("dueña") y se aplica en el
 * orden de la lista, con una influencia de 0 a 1 que se puede animar (canal
 * `blend` de la pista `kind: "ik"` con el id de la restricción; el driver
 * anima su valor con el canal `roll`). Como todo el rig, se evalúa en la app
 * y se hornea a FK al exportar.
 *
 * Solo importa tipos de `rig.ts`: lo que necesita de allí llega como
 * parámetro (evita el ciclo de módulos).
 */

import * as THREE from "three";
import { clonePose, sampleScalar, slerp, type AnimationClip, type Pose, type Quat, type Vec3 } from "./animation";
import { Fk, rotationBetween } from "./ik";
import type { JointFrame, RigContext, RigControl } from "./rig";

export type ConstraintType = "copyRotation" | "copyLocation" | "childOf" | "track" | "stretch" | "map" | "driver" | "distribute";

export const CONSTRAINT_TYPES: { value: ConstraintType; label: string; hint: string }[] = [
  { value: "copyRotation", label: "Copiar giro", hint: "Gira como otro hueso o control (por eje, del mundo o en sus propios ejes)" },
  { value: "copyLocation", label: "Copiar posición", hint: "La articulación va a donde está otro hueso o control" },
  { value: "childOf", label: "Hijo de", hint: "Sigue a otro hueso o control como si colgara de él (agarrar, apoyar)" },
  { value: "track", label: "Seguir", hint: "El hueso apunta siempre hacia el objetivo" },
  { value: "stretch", label: "Estirar hacia", hint: "Apunta al objetivo y se alarga hasta tocarlo (tendones, músculos)" },
  { value: "map", label: "Mapeo de giro", hint: "El giro de otro hueso en un eje da el giro de este en otro eje" },
  { value: "driver", label: "Driver", hint: "Un valor de 0 a 1 dobla varias articulaciones a la vez (cerrar la mano)" },
  { value: "distribute", label: "Reparto de giro", hint: "El giro de la articulación se reparte entre ella y las siguientes (columna)" },
];

export interface RigConstraint {
  id: string;
  name: string;
  type: ConstraintType;
  /** Articulación que se restringe (la primera, en driver y reparto) */
  owner: string;
  /** Hueso (su nombre) o control (`control:<id>`) del que se toma el valor */
  target?: string;
  /** 0–1 (1 por defecto); una key de `blend` en la animación manda */
  influence?: number;
  /** Ejes que se copian o giran (por defecto los tres) */
  axes?: [boolean, boolean, boolean];
  /** Copiar giro: "world" (orientación en el mundo) o "local" (giro de la articulación, en sus ejes) */
  space?: "world" | "local";
  /** Copiar giro local: al revés (lo que el objetivo gira a un lado, este al otro) */
  invert?: boolean;
  /** Copiar giro: se suma al propio en vez de reemplazarlo */
  offset?: boolean;
  /** Mapeo: eje y rango (grados) del objetivo → eje y rango de la dueña */
  fromAxis?: 0 | 1 | 2;
  fromRange?: [number, number];
  toAxis?: 0 | 1 | 2;
  toRange?: [number, number];
  /** Driver y reparto: articulaciones además de la dueña */
  joints?: string[];
  /** Driver: eje y ángulo (grados) de cada articulación con el valor en 1 */
  axis?: 0 | 1 | 2;
  angle?: number;
  /** Driver: valor fijo (0–1) si la animación no lo tiene */
  value?: number;
  disabled?: boolean;
}

export const CONSTRAINT_PREFIX = "constraint-";
export const isConstraintId = (id: string) => id.startsWith(CONSTRAINT_PREFIX);
export const controlTarget = (id: string) => `control:${id}`;

/** Lee las restricciones guardadas (descarta las mal formadas) */
export function loadConstraints(raw: unknown): RigConstraint[] {
  if (!Array.isArray(raw)) return [];
  const types = new Set(CONSTRAINT_TYPES.map((t) => t.value));
  return raw.filter(
    (c): c is RigConstraint =>
      !!c && typeof c.id === "string" && typeof c.owner === "string" && typeof c.name === "string" && types.has(c.type)
  );
}

/** Qué necesita la etapa del rig (se pasa desde `rig.ts`) */
export interface ConstraintHelpers {
  controlWorld: (control: RigControl, ctx: RigContext, fk: Fk, pose: Pose) => { position: Vec3; rotation: Quat };
}

// ─── Álgebra local ──────────────────────────────────────────────────────────

const tq = (q: Quat) => new THREE.Quaternion(q[0], q[1], q[2], q[3]);
const fq = (q: THREE.Quaternion): Quat => [q.x, q.y, q.z, q.w];
const I: Quat = [0, 0, 0, 1];
const toJoint = (q: Quat, f: JointFrame) => tq(f.q).invert().multiply(tq(q)).multiply(tq(f.q));
const fromJoint = (r: THREE.Quaternion, f: JointFrame): Quat => fq(tq(f.q).multiply(r).multiply(tq(f.q).invert()));
const euler = (r: THREE.Quaternion) => new THREE.Euler().setFromQuaternion(r, "XYZ");
const fromEuler = (e: THREE.Euler) => new THREE.Quaternion().setFromEuler(e);
const component = (e: THREE.Euler, axis: number) => (axis === 0 ? e.x : axis === 1 ? e.y : e.z);
const setComponent = (e: THREE.Euler, axis: number, v: number) => (axis === 0 ? (e.x = v) : axis === 1 ? (e.y = v) : (e.z = v));

/** Reemplaza en `own` los ejes elegidos por los de `from` (Euler XYZ en los ejes de la articulación) */
function mixAxes(own: THREE.Quaternion, from: THREE.Quaternion, axes: [boolean, boolean, boolean]): THREE.Quaternion {
  if (axes.every(Boolean)) return from.clone();
  const a = euler(own);
  const b = euler(from);
  axes.forEach((on, i) => on && setComponent(a, i, component(b, i)));
  return fromEuler(a);
}

// ─── Objetivos ──────────────────────────────────────────────────────────────

interface TargetState {
  position: Vec3;
  rotation: Quat;
  restPosition: Vec3;
  /** Articulación del objetivo (si es un hueso) */
  joint?: number;
}

function targetState(target: string | undefined, ctx: RigContext, fk: Fk, pose: Pose, helpers: ConstraintHelpers): TargetState | null {
  if (!target) return null;
  if (target.startsWith("control:")) {
    const control = ctx.settings.controls.find((c) => c.id === target.slice(8));
    if (!control) return null;
    const w = helpers.controlWorld(control, ctx, fk, pose);
    return { position: w.position, rotation: w.rotation, restPosition: control.position };
  }
  const j = ctx.boneIndex.get(target);
  if (j === undefined) return null;
  return { position: fk.position(j), rotation: fq(fk.world(j)), restPosition: ctx.bones[j].position, joint: j };
}

// ─── Etapa ──────────────────────────────────────────────────────────────────

/** Influencia en el cuadro: la key de la animación o el valor fijo */
export function constraintInfluence(c: RigConstraint, clip: AnimationClip | undefined, frame: number): number {
  const keyed = sampleScalar(clip, c.id, "blend", frame);
  return Math.max(0, Math.min(1, keyed ?? c.influence ?? 1));
}

/** Valor del driver en el cuadro */
export function driverValue(c: RigConstraint, clip: AnimationClip | undefined, frame: number): number {
  return Math.max(0, Math.min(1, sampleScalar(clip, c.id, "roll", frame) ?? c.value ?? 0));
}

/** Aplica las restricciones del rig a la pose, en orden */
export function applyConstraints(
  pose: Pose,
  ctx: RigContext,
  info: { clip?: AnimationClip; frame?: number },
  helpers: ConstraintHelpers
): Pose {
  const list = (ctx.settings.constraints ?? []).filter((c) => !c.disabled);
  if (list.length === 0) return pose;
  const out = clonePose(pose);
  const fk = new Fk(ctx.bones, out);
  const frame = info.frame ?? 0;
  for (const c of list) {
    const owner = ctx.boneIndex.get(c.owner);
    if (owner === undefined) continue;
    const influence = constraintInfluence(c, info.clip, frame);
    if (influence <= 0) continue;
    const before = clonePose(out);
    const touched = applyOne(c, owner, ctx, fk, out, info.clip, frame, helpers);
    if (touched.length === 0 || influence >= 1) continue;
    for (const j of touched) {
      const a = before.rotations.get(j) ?? I;
      const b = out.rotations.get(j) ?? I;
      out.rotations.set(j, slerp(a, b, influence));
      const ta = before.translations.get(j);
      const tb = out.translations.get(j);
      if (ta || tb) {
        const x = ta ?? [0, 0, 0];
        const y = tb ?? [0, 0, 0];
        out.translations.set(j, [x[0] + (y[0] - x[0]) * influence, x[1] + (y[1] - x[1]) * influence, x[2] + (y[2] - x[2]) * influence]);
      }
    }
    fk.update();
  }
  return out;
}

/** Aplica una restricción con influencia completa; devuelve las articulaciones que tocó */
function applyOne(
  c: RigConstraint,
  owner: number,
  ctx: RigContext,
  fk: Fk,
  pose: Pose,
  clip: AnimationClip | undefined,
  frame: number,
  helpers: ConstraintHelpers
): number[] {
  const axes = c.axes ?? [true, true, true];
  const frames = ctx.frames;
  const firstChild = ctx.children[owner][0];
  switch (c.type) {
    case "copyRotation": {
      const t = targetState(c.target, ctx, fk, pose, helpers);
      if (!t) return [];
      const ownLocal = toJoint(pose.rotations.get(owner) ?? I, frames[owner]);
      if (c.space === "local") {
        // El giro propio del objetivo, en sus ejes, pasado a los de la dueña
        let r: THREE.Quaternion;
        if (t.joint !== undefined) r = toJoint(pose.rotations.get(t.joint) ?? I, frames[t.joint]);
        else r = tq(pose.controls.get(c.target!.slice(8))?.rotation ?? I);
        if (c.invert) r.invert();
        const mixed = mixAxes(c.offset ? new THREE.Quaternion() : ownLocal, r, axes);
        const next = c.offset ? ownLocal.clone().multiply(mixed) : mixed;
        pose.rotations.set(owner, fromJoint(next.normalize(), frames[owner]));
        fk.update();
        return [owner];
      }
      // Orientación en el mundo, filtrada por eje en los ejes de la articulación
      const current = fk.world(owner);
      const inJoint = (w: THREE.Quaternion) => toJoint(fq(w), frames[owner]);
      const goal = mixAxes(inJoint(current), inJoint(tq(t.rotation)), axes);
      let world = tq(fromJoint(goal, frames[owner]));
      if (c.offset) world = tq(t.rotation).multiply(current);
      fk.setWorld(owner, fq(world.normalize()));
      return [owner];
    }
    case "copyLocation": {
      const t = targetState(c.target, ctx, fk, pose, helpers);
      if (!t) return [];
      const now = fk.position(owner);
      fk.moveTo(owner, [0, 1, 2].map((i) => (axes[i] ? t.position[i] : now[i])) as Vec3);
      return [owner];
    }
    case "childOf": {
      const t = targetState(c.target, ctx, fk, pose, helpers);
      if (!t) return [];
      // Como si colgara del objetivo desde el reposo: se mueve y gira con él
      const R = tq(t.rotation);
      const rest = ctx.bones[owner].position;
      const offset = new THREE.Vector3(rest[0] - t.restPosition[0], rest[1] - t.restPosition[1], rest[2] - t.restPosition[2]).applyQuaternion(R);
      fk.moveTo(owner, [t.position[0] + offset.x, t.position[1] + offset.y, t.position[2] + offset.z]);
      fk.setWorld(owner, t.rotation);
      return [owner];
    }
    case "track":
    case "stretch": {
      const t = targetState(c.target, ctx, fk, pose, helpers);
      if (!t || firstChild === undefined) return [];
      const from = fk.position(owner);
      const tip = fk.position(firstChild);
      const dir: Vec3 = [tip[0] - from[0], tip[1] - from[1], tip[2] - from[2]];
      const want: Vec3 = [t.position[0] - from[0], t.position[1] - from[1], t.position[2] - from[2]];
      if (Math.hypot(...want) < 1e-9 || Math.hypot(...dir) < 1e-9) return [];
      fk.rotate(owner, rotationBetween(dir, want));
      if (c.type === "track") return [owner];
      // Estirar: la punta del hueso llega al objetivo
      fk.moveTo(firstChild, t.position);
      return [owner, firstChild];
    }
    case "map": {
      const source = c.target !== undefined ? ctx.boneIndex.get(c.target) : undefined;
      if (source === undefined) return [];
      const [f0, f1] = c.fromRange ?? [0, 90];
      const [t0, t1] = c.toRange ?? [0, 45];
      const a = THREE.MathUtils.radToDeg(component(euler(toJoint(pose.rotations.get(source) ?? I, frames[source])), c.fromAxis ?? 0));
      const s = f1 === f0 ? 0 : Math.max(0, Math.min(1, (a - f0) / (f1 - f0)));
      const own = euler(toJoint(pose.rotations.get(owner) ?? I, frames[owner]));
      setComponent(own, c.toAxis ?? 0, THREE.MathUtils.degToRad(t0 + s * (t1 - t0)));
      pose.rotations.set(owner, fromJoint(fromEuler(own), frames[owner]));
      fk.update();
      return [owner];
    }
    case "driver": {
      const v = driverValue(c, clip, frame);
      const joints = [owner, ...(c.joints ?? []).flatMap((n) => ctx.boneIndex.get(n) ?? [])];
      const axis = new THREE.Vector3(c.axis === 1 ? 0 : c.axis === 2 ? 0 : 1, c.axis === 1 ? 1 : 0, c.axis === 2 ? 1 : 0);
      const turn = new THREE.Quaternion().setFromAxisAngle(axis, THREE.MathUtils.degToRad((c.angle ?? 90) * v));
      for (const j of joints) {
        const r = toJoint(pose.rotations.get(j) ?? I, frames[j]).multiply(turn);
        pose.rotations.set(j, fromJoint(r.normalize(), frames[j]));
      }
      fk.update();
      return joints;
    }
    case "distribute": {
      const rest = (c.joints ?? []).flatMap((n) => ctx.boneIndex.get(n) ?? []);
      if (rest.length === 0) return [];
      const joints = [owner, ...rest];
      // Cada una se queda con una parte igual del giro de la dueña
      const whole = toJoint(pose.rotations.get(owner) ?? I, frames[owner]);
      const part = new THREE.Quaternion().slerp(whole, 1 / joints.length);
      pose.rotations.set(owner, fromJoint(part, frames[owner]));
      for (const j of rest) {
        const r = toJoint(pose.rotations.get(j) ?? I, frames[j]).multiply(part);
        pose.rotations.set(j, fromJoint(r.normalize(), frames[j]));
      }
      fk.update();
      return joints;
    }
  }
}

/** Articulaciones que una restricción puede mover (para el grafo de relaciones) */
export function constraintEdges(c: RigConstraint): { from: string; to: string }[] {
  const out: { from: string; to: string }[] = [];
  if (c.target && !c.target.startsWith("control:")) out.push({ from: c.target, to: c.owner });
  for (const j of c.joints ?? []) out.push({ from: c.owner, to: j });
  return out;
}
