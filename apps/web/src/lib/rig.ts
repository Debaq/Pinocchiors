/**
 * Modelo del rig: lo que cada hueso sabe además de nombre, posición y padre.
 *
 * El esqueleto (posiciones y jerarquía) vive en el backend; esto es lo que
 * el animador agrega encima y se guarda en el proyecto (`ui.rig`):
 *
 * - **Orientación local** de cada articulación: el eje Y apunta a lo largo
 *   del hueso que gira, X es el eje de bisagra (perpendicular al plano
 *   padre–articulación–hija, o el costado del cuerpo en cadenas rectas) y
 *   Z completa. El `roll` del usuario la gira alrededor de Y. Se recalcula
 *   sola al mover el esqueleto: solo el roll se guarda.
 * - **Propiedades por hueso**: grupo, color, visible, bloqueado, deforma,
 *   ejes de giro bloqueados, traslación bloqueada, modo de rotación y forma.
 * - **Grupos** de huesos con color (generados desde el análisis del cuerpo).
 * - **Controles**: objetos que no deforman la malla y tienen keys propias.
 *
 * Y la **pila de evaluación** única de cada cuadro: keys FK → restricciones →
 * IK → límites → pose final. El visor, la exportación (que hornea la pila a
 * giros por cuadro) y lo que venga después usan la misma.
 *
 * Marco de los giros (ver `Viewer3D.buildRig`): en reposo todos los huesos
 * del rig tienen orientación identidad, así que el giro local `q` de una
 * articulación está en ejes del modelo; en los ejes propios de la
 * articulación `O` es `O⁻¹·q·O`.
 */

import * as THREE from "three";
import {
  clonePose,
  pinStart,
  samplePose,
  sampleScalar,
  slerp,
  type AnimationClip,
  type BoneTrack,
  type Key,
  type Pose,
  type Quat,
  type Vec3,
} from "./animation";
import { Fk, solveCcd, solveFabrik, solveLookAt, solveSpline, solveTwoBone } from "./ik";
import { applyLimits, type JointLimits } from "./jointLimits";
import { eulerCodec, type Codec, type RotationCodecs } from "./curves";
import { analyzeBody, type Body, type ChainKind, type SkeletonBone } from "./presetAnimations";
import { applyConstraints, isConstraintId, loadConstraints, type RigConstraint } from "./constraints";
import { applySecondary, type SpringSettings } from "./secondary";

// ─── Tipos ──────────────────────────────────────────────────────────────────

export type RotationMode = "quaternion" | "XYZ" | "XZY" | "YXZ" | "YZX" | "ZXY" | "ZYX";
export const ROTATION_MODES: { value: RotationMode; label: string }[] = [
  { value: "quaternion", label: "Cuaternión" },
  { value: "XYZ", label: "Euler XYZ" },
  { value: "XZY", label: "Euler XZY" },
  { value: "YXZ", label: "Euler YXZ" },
  { value: "YZX", label: "Euler YZX" },
  { value: "ZXY", label: "Euler ZXY" },
  { value: "ZYX", label: "Euler ZYX" },
];

/** Forma de dibujo de un hueso; sin forma propia, la de la barra del visor */
export type BoneShape = "octahedral" | "stick" | "sphere" | "box";
export const BONE_SHAPES: { value: BoneShape | "default"; label: string }[] = [
  { value: "default", label: "Como la vista" },
  { value: "octahedral", label: "Octaedro" },
  { value: "stick", label: "Línea" },
  { value: "sphere", label: "Esfera" },
  { value: "box", label: "Caja" },
];

export type ControlShape = "cube" | "circle" | "arrow" | "sphere";
export const CONTROL_SHAPES: { value: ControlShape; label: string }[] = [
  { value: "circle", label: "Círculo" },
  { value: "cube", label: "Cubo" },
  { value: "sphere", label: "Esfera" },
  { value: "arrow", label: "Flecha" },
];

export interface BoneProps {
  /** Id del grupo */
  group?: string;
  /** Color propio (#rrggbb); sin él, el del grupo */
  color?: string;
  hidden?: boolean;
  /** No se selecciona ni se transforma */
  locked?: boolean;
  /** Mueve la malla (por defecto sí) */
  deform?: boolean;
  /** Ejes locales (X, Y, Z) en los que no gira */
  lockRotation?: [boolean, boolean, boolean];
  lockTranslation?: boolean;
  rotationMode?: RotationMode;
  shape?: BoneShape;
  /** Giro extra de los ejes alrededor del hueso, en radianes */
  roll?: number;
  /** Rango de giro (bisagra o rótula), ver `jointLimits.ts` */
  limits?: JointLimits;
  /** Resorte (F8): el hueso sigue a una punta con masa en vez de a sus keys */
  spring?: SpringSettings;
}

export interface BoneGroup {
  id: string;
  name: string;
  color: string;
  hidden?: boolean;
  /** Generado a partir del cuerpo (se rehace con "grupos automáticos") */
  auto?: boolean;
}

export interface RigControl {
  id: string;
  name: string;
  /** Articulación a la que sigue (por nombre); `null` = suelta en el modelo */
  parent: string | null;
  shape: ControlShape;
  /** Posición de reposo, en el espacio del modelo */
  position: Vec3;
  /** Radio del dibujo */
  size: number;
  color?: string;
  /** Lo creó el rig automático (rehacerlo lo reemplaza) */
  auto?: boolean;
}

export type IkSolver = "twoBone" | "fabrik" | "ccd" | "spline" | "lookAt" | "root";
export const IK_SOLVERS: { value: IkSolver; label: string }[] = [
  { value: "twoBone", label: "Dos huesos + pole" },
  { value: "fabrik", label: "FABRIK" },
  { value: "ccd", label: "CCD" },
  { value: "spline", label: "Curva" },
  { value: "lookAt", label: "Mirar a" },
  { value: "root", label: "Centro de masa" },
];

/**
 * Cadena de IK. `joints` va de la base al efector (por nombre); en las de
 * dos huesos son cadera, rodilla y tobillo, y lo que sigue (planta, punta)
 * es el pie. En "Mirar a", las articulaciones que reparten el giro; en
 * "Centro de masa", la raíz.
 */
export interface IkChain {
  id: string;
  name: string;
  solver: IkSolver;
  joints: string[];
  /** Control objetivo (id) */
  target?: string;
  /** Control hacia el que apunta la rodilla o el codo */
  pole?: string;
  /** Controles por los que pasa la curva */
  curve?: string[];
  /** Estirar los huesos si el objetivo queda lejos (en la curva: ajustar la cadena a su largo) */
  stretch?: boolean;
  /** Suavizado de la extensión total (0–0,5 del largo) */
  softness?: number;
  /** El efector no baja del suelo */
  ground?: boolean;
  /** Pie invertido: el objetivo lleva el pie, que rueda con el balanceo */
  footRoll?: boolean;
  /** El efector gira con el objetivo (mano o pie) */
  alignEffector?: boolean;
  /** Mezcla IK/FK sin keys (0 FK, 1 IK) */
  blend?: number;
  /** Balanceo del pie sin keys, en grados */
  roll?: number;
  /** Reparto del giro de "Mirar a", de la base a la punta */
  weights?: number[];
  iterations?: number;
  disabled?: boolean;
  /** Lo creó el rig automático (rehacerlo lo reemplaza) */
  auto?: boolean;
}

export interface RigSettings {
  version: 1;
  bones: Record<string, BoneProps>;
  groups: BoneGroup[];
  controls: RigControl[];
  ikChains: IkChain[];
  /** Restricciones (F3), en orden de evaluación */
  constraints?: RigConstraint[];
  /** Los límites de giro no se aplican (se ven igual en el panel) */
  limitsOff?: boolean;
  /** Piel con cuaterniones duales en el visor (la exportación sigue lineal: glTF no la tiene) */
  dualQuaternion?: boolean;
}

export const emptyRigSettings = (): RigSettings => ({ version: 1, bones: {}, groups: [], controls: [], ikChains: [] });

/**
 * Lee lo guardado en el proyecto. Los proyectos anteriores no lo tienen:
 * quedan con todo por defecto (sin grupos, sin controles, sin roll).
 */
export function loadRigSettings(raw: unknown): RigSettings {
  const settings = emptyRigSettings();
  if (!raw || typeof raw !== "object") return settings;
  const r = raw as Partial<RigSettings>;
  if (r.version !== 1) return settings;
  if (r.bones && typeof r.bones === "object") settings.bones = { ...r.bones };
  if (Array.isArray(r.groups)) settings.groups = r.groups.filter((g) => g && typeof g.id === "string");
  if (Array.isArray(r.controls)) {
    settings.controls = r.controls.filter((c) => c && typeof c.id === "string" && Array.isArray(c.position));
  }
  if (r.limitsOff === true) settings.limitsOff = true;
  if (r.dualQuaternion === true) settings.dualQuaternion = true;
  if (Array.isArray(r.ikChains)) {
    settings.ikChains = r.ikChains.filter((c) => c && typeof c.id === "string" && Array.isArray(c.joints));
  }
  const constraints = loadConstraints(r.constraints);
  if (constraints.length > 0) settings.constraints = constraints;
  return settings;
}

/** Propiedades de un hueso con los valores por defecto */
export function boneProps(settings: RigSettings, name: string): Required<Omit<BoneProps, "group" | "color" | "shape" | "limits" | "spring">> & BoneProps {
  const p = settings.bones[name] ?? {};
  return {
    ...p,
    hidden: p.hidden ?? false,
    locked: p.locked ?? false,
    deform: p.deform ?? true,
    lockRotation: p.lockRotation ?? [false, false, false],
    lockTranslation: p.lockTranslation ?? false,
    rotationMode: p.rotationMode ?? "quaternion",
    roll: p.roll ?? 0,
  };
}

/** Cambia propiedades de varios huesos (quita las que vuelven al valor por defecto) */
export function withBoneProps(settings: RigSettings, names: string[], change: Partial<BoneProps>): RigSettings {
  const bones = { ...settings.bones };
  for (const name of names) {
    const next: BoneProps = { ...bones[name], ...change };
    for (const key of Object.keys(next) as (keyof BoneProps)[]) {
      if (next[key] === undefined) delete next[key];
    }
    if (Object.keys(next).length === 0) delete bones[name];
    else bones[name] = next;
  }
  return { ...settings, bones };
}

/** El hueso no se ve: oculto él o su grupo */
export function isBoneHidden(settings: RigSettings, name: string): boolean {
  const p = settings.bones[name];
  if (p?.hidden) return true;
  const group = p?.group ? settings.groups.find((g) => g.id === p.group) : undefined;
  return group?.hidden === true;
}

/** Color del hueso: el propio, o el de su grupo */
export function boneColor(settings: RigSettings, name: string): string | undefined {
  const p = settings.bones[name];
  if (p?.color) return p.color;
  return p?.group ? settings.groups.find((g) => g.id === p.group)?.color : undefined;
}

let nextId = 0;
export const newRigId = (prefix: string) => `${prefix}-${Date.now().toString(36)}-${++nextId}`;

// ─── Álgebra ────────────────────────────────────────────────────────────────

const sub = (a: Vec3, b: Vec3): Vec3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const dot = (a: Vec3, b: Vec3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a: Vec3, b: Vec3): Vec3 => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const scale = (a: Vec3, s: number): Vec3 => [a[0] * s, a[1] * s, a[2] * s];
const length = (a: Vec3) => Math.hypot(a[0], a[1], a[2]);
const unit = (a: Vec3, fallback: Vec3): Vec3 => {
  const l = length(a);
  return l > 1e-9 ? scale(a, 1 / l) : fallback;
};

/** Operaciones de vectores para quien use este módulo */
export const vec = { sub, dot, cross, scale, length, unit };

export const IDENTITY: Quat = [0, 0, 0, 1];

const tq = (q: Quat) => new THREE.Quaternion(q[0], q[1], q[2], q[3]);
const fq = (q: THREE.Quaternion): Quat => [q.x, q.y, q.z, q.w];

export function quatMultiply(a: Quat, b: Quat): Quat {
  return fq(tq(a).multiply(tq(b)));
}

export function quatInverse(q: Quat): Quat {
  return [-q[0], -q[1], -q[2], q[3]];
}

export function rotateVec(q: Quat, v: Vec3): Vec3 {
  const r = new THREE.Vector3(...v).applyQuaternion(tq(q));
  return [r.x, r.y, r.z];
}

/** Giro de `angle` radianes alrededor de `axis` (unitario) */
export function axisAngle(axis: Vec3, angle: number): Quat {
  const s = Math.sin(angle / 2);
  return [axis[0] * s, axis[1] * s, axis[2] * s, Math.cos(angle / 2)];
}

/** Ángulo del giro, en radianes (0 a π); estable con giros chicos y cuaterniones casi unitarios */
export function quatAngle(q: Quat): number {
  return 2 * Math.atan2(Math.hypot(q[0], q[1], q[2]), Math.abs(q[3]));
}

// ─── Orientación de las articulaciones ──────────────────────────────────────

export interface JointFrame {
  /** Ejes locales en el espacio del modelo (unitarios, ortogonales) */
  x: Vec3;
  y: Vec3;
  z: Vec3;
  /** Giro de los ejes del modelo a los locales */
  q: Quat;
  /** Eje X sin el roll del usuario (el que da el cálculo automático) */
  autoX: Vec3;
}

/** Ejes del cuerpo: los del análisis, o los del modelo si no se reconoce */
export function bodyAxes(body: Body | null): { up: Vec3; forward: Vec3; right: Vec3 } {
  return body ? { up: body.up, forward: body.forward, right: body.right } : { up: [0, 1, 0], forward: [0, 0, 1], right: [1, 0, 0] };
}

/** Dirección del hueso que gira la articulación `j` */
function boneDirection(bones: SkeletonBone[], children: number[][], j: number, up: Vec3): Vec3 {
  const p = bones[j].position;
  const kids = children[j];
  if (kids.length > 0) {
    const mean: Vec3 = [0, 0, 0];
    for (const c of kids) {
      const d = unit(sub(bones[c].position, p), [0, 0, 0]);
      mean[0] += d[0];
      mean[1] += d[1];
      mean[2] += d[2];
    }
    const d = unit(mean, [0, 0, 0]);
    if (length(d) > 0) return d;
  }
  const parent = bones[j].parent;
  if (parent !== null) return unit(sub(p, bones[parent].position), up);
  return up;
}

function basisQuat(x: Vec3, y: Vec3, z: Vec3): Quat {
  const m = new THREE.Matrix4().makeBasis(new THREE.Vector3(...x), new THREE.Vector3(...y), new THREE.Vector3(...z));
  return fq(new THREE.Quaternion().setFromRotationMatrix(m).normalize());
}

/** Ejes de cada articulación con el roll de `settings` */
export function jointFrames(bones: SkeletonBone[], settings: RigSettings, body: Body | null = analyzeBody(bones)): JointFrame[] {
  const { up, forward, right } = bodyAxes(body);
  const children = bones.map(() => [] as number[]);
  bones.forEach((b, i) => b.parent !== null && children[b.parent]?.push(i));
  return bones.map((bone, j) => {
    const y = boneDirection(bones, children, j, up);
    // Bisagra: normal del plano padre–articulación–hija, si la articulación
    // sigue la línea del padre con un doblez (rodilla, codo). Donde el padre
    // llega de costado (cadera, hombro) ese plano no dice nada del doblez
    let x: Vec3 = [0, 0, 0];
    const parent = bone.parent;
    if (parent !== null && children[j].length === 1) {
      const a = unit(sub(bone.position, bones[parent].position), y);
      const n = cross(a, y);
      if (length(n) > 0.05 && dot(a, y) > Math.cos((50 * Math.PI) / 180)) x = unit(n, [0, 0, 0]);
    }
    // Cadena recta (o sin padre): el costado del cuerpo, o adelante si el hueso va de costado
    if (length(x) === 0) {
      const ref = Math.abs(dot(right, y)) < 0.9 ? right : forward;
      x = unit(sub(ref, scale(y, dot(ref, y))), [1, 0, 0]);
    }
    // Mismo sentido en todo el cuerpo: X hacia la derecha (o adelante)
    const side = Math.abs(dot(x, right)) > 0.1 ? right : forward;
    if (dot(x, side) < 0) x = scale(x, -1);
    const autoX = x;
    const roll = settings.bones[bone.name]?.roll ?? 0;
    if (roll !== 0) x = rotateVec(axisAngle(y, roll), x);
    const z = cross(x, y);
    return { x, y, z, q: basisQuat(x, y, z), autoX };
  });
}

/** Roll que deja el eje X de la articulación lo más cerca posible de `target` */
export function rollToward(frame: JointFrame, target: Vec3): number | null {
  const t = sub(target, scale(frame.y, dot(target, frame.y)));
  if (length(t) < 1e-6) return null;
  const tn = unit(t, frame.autoX);
  const angle = Math.atan2(dot(cross(frame.autoX, tn), frame.y), dot(frame.autoX, tn));
  return Math.abs(angle) < 1e-9 ? 0 : angle;
}

/** Giro local (ejes del modelo) → ejes de la articulación */
export function toJointSpace(q: Quat, frame: JointFrame): Quat {
  return quatMultiply(quatMultiply(quatInverse(frame.q), q), frame.q);
}

/** Giro en ejes de la articulación → giro local (ejes del modelo) */
export function fromJointSpace(r: Quat, frame: JointFrame): Quat {
  return quatMultiply(quatMultiply(frame.q, r), quatInverse(frame.q));
}

/** Ángulos en grados del giro en ejes de la articulación, en el orden del modo */
export function eulerDegrees(q: Quat, frame: JointFrame, mode: RotationMode): Vec3 {
  const e = new THREE.Euler().setFromQuaternion(tq(toJointSpace(q, frame)), mode === "quaternion" ? "XYZ" : mode);
  const d = (r: number) => Math.round(THREE.MathUtils.radToDeg(r) * 100) / 100 || 0;
  return [d(e.x), d(e.y), d(e.z)];
}

/** Giro local a partir de ángulos en grados en los ejes de la articulación */
export function fromEulerDegrees(angles: Vec3, frame: JointFrame, mode: RotationMode): Quat {
  const r = THREE.MathUtils.degToRad;
  const e = new THREE.Euler(r(angles[0]), r(angles[1]), r(angles[2]), mode === "quaternion" ? "XYZ" : mode);
  return fromJointSpace(fq(new THREE.Quaternion().setFromEuler(e)), frame);
}

/**
 * Quita el giro de los ejes bloqueados. Con un solo eje libre es una bisagra
 * exacta (la parte del giro alrededor de ese eje, "twist"); si no, se
 * anulan los ángulos de Euler bloqueados.
 */
export function applyRotationLocks(q: Quat, frame: JointFrame, locks: [boolean, boolean, boolean], mode: RotationMode): Quat {
  const lockedCount = locks.filter(Boolean).length;
  if (lockedCount === 0) return q;
  if (lockedCount === 3) return IDENTITY;
  const r = toJointSpace(q, frame);
  let out: Quat;
  if (lockedCount === 2) {
    const free = locks.indexOf(false);
    // Twist: proyección de la parte vectorial sobre el eje libre
    const twist: Quat = [0, 0, 0, r[3]];
    twist[free] = r[free];
    const len = Math.hypot(twist[free], twist[3]);
    out = len > 1e-9 ? [twist[0] / len, twist[1] / len, twist[2] / len, twist[3] / len] : IDENTITY;
  } else {
    const order = mode === "quaternion" ? "XYZ" : mode;
    const e = new THREE.Euler().setFromQuaternion(tq(r), order);
    if (locks[0]) e.x = 0;
    if (locks[1]) e.y = 0;
    if (locks[2]) e.z = 0;
    out = fq(new THREE.Quaternion().setFromEuler(e));
  }
  return fromJointSpace(out, frame);
}

// ─── Contexto y pila de evaluación ──────────────────────────────────────────

export interface RigContext {
  bones: SkeletonBone[];
  boneIndex: Map<string, number>;
  settings: RigSettings;
  body: Body | null;
  frames: JointFrame[];
  children: number[][];
  /** Par izquierda ↔ derecha de cada articulación (o `null`) */
  mirror: (number | null)[];
  /** Normal del plano de simetría (el costado del cuerpo) y un punto del plano */
  symmetry: { normal: Vec3; point: Vec3 };
  /** Cómo se interpolan los giros de cada pista en los tramos Bézier */
  rotation: RotationCodecs;
}

/**
 * Giros de cada pista como ángulos de Euler en los ejes de su articulación,
 * con el orden de su modo de rotación (cuaternión: XYZ). Los controles, en
 * los ejes del modelo
 */
export function rigRotationCodecs(bones: SkeletonBone[], settings: RigSettings, frames: JointFrame[]): RotationCodecs {
  const byName = new Map(bones.map((b, i) => [b.name, i]));
  const codecs = new Map<string, Codec<Quat>>();
  const fallback = eulerCodec();
  return (track) => {
    if (track.kind) return fallback;
    const j = byName.get(track.bone);
    if (j === undefined) return fallback;
    let codec = codecs.get(track.bone);
    if (!codec) {
      const mode = boneProps(settings, track.bone).rotationMode;
      const order = mode === "quaternion" ? "XYZ" : mode;
      const frame = frames[j];
      const id = `j:${order}:${frame.q.map((x) => x.toFixed(6)).join(",")}`;
      codec = eulerCodec(order, id, { toLocal: (q) => toJointSpace(q, frame), toModel: (q) => fromJointSpace(q, frame) });
      codecs.set(track.bone, codec);
    }
    return codec;
  };
}

export function createRigContext(bones: SkeletonBone[], settings: RigSettings): RigContext {
  const body = analyzeBody(bones);
  const children = bones.map(() => [] as number[]);
  bones.forEach((b, i) => b.parent !== null && children[b.parent]?.push(i));
  const { right } = bodyAxes(body);
  const root = body?.root ?? bones.findIndex((b) => b.parent === null);
  const frames = jointFrames(bones, settings, body);
  return {
    bones,
    boneIndex: new Map(bones.map((b, i) => [b.name, i])),
    settings,
    body,
    frames,
    rotation: rigRotationCodecs(bones, settings, frames),
    children,
    mirror: mirrorPairs(bones, right, root >= 0 ? bones[root].position : [0, 0, 0]),
    symmetry: { normal: right, point: root >= 0 ? bones[root].position : [0, 0, 0] },
  };
}

/** De dónde sale la pose: el clip y el cuadro (para los canales animados y el fijado) */
export interface StageInfo {
  clip?: AnimationClip;
  frame?: number;
  /** Sin fijado (al calcular dónde quedó el efector cuando se fijó) */
  noPins?: boolean;
}

/**
 * Etapa de la pila de evaluación: recibe la pose de la etapa anterior y
 * devuelve la suya. Las restricciones (F3) se enchufan aquí.
 */
export interface PoseStage {
  name: string;
  run: (pose: Pose, ctx: RigContext, info: StageInfo) => Pose;
}

/** Ejes bloqueados y rango de giro de un hueso aplicados a un giro local */
export function constrainRotation(ctx: RigContext, j: number, q: Quat, props = boneProps(ctx.settings, ctx.bones[j].name)): Quat {
  let out = applyRotationLocks(q, ctx.frames[j], props.lockRotation, props.rotationMode);
  if (props.limits && !ctx.settings.limitsOff) out = applyLimits(out, ctx.frames[j], props.limits);
  return out;
}

/** Límites de los huesos como función para los solucionadores y el visor */
export function constrainer(ctx: RigContext): (j: number, q: Quat) => Quat {
  const props = ctx.bones.map((b) => boneProps(ctx.settings, b.name));
  return (j, q) => (props[j] ? constrainRotation(ctx, j, q, props[j]) : q);
}

/** Posición y giro en mundo de un control con la pose (sigue al hueso de su articulación) */
export function controlWorld(control: RigControl, ctx: RigContext, fk: Fk, pose: Pose): { position: Vec3; rotation: Quat; follow: THREE.Matrix4 } {
  const follow = new THREE.Matrix4();
  const joint = control.parent === null ? undefined : ctx.boneIndex.get(control.parent);
  if (joint !== undefined) {
    const bone = ctx.bones[joint].parent === null ? joint : (ctx.children[joint][0] ?? joint);
    const head = ctx.bones[ctx.bones[bone].parent ?? bone].position;
    follow
      .compose(fk.origin[bone], fk.rot[bone], new THREE.Vector3(1, 1, 1))
      .multiply(new THREE.Matrix4().makeTranslation(-head[0], -head[1], -head[2]));
  }
  const c = pose.controls.get(control.id);
  const local = new THREE.Matrix4().compose(
    new THREE.Vector3(...control.position).add(new THREE.Vector3(...(c?.translation ?? [0, 0, 0]))),
    new THREE.Quaternion(...(c?.rotation ?? IDENTITY)),
    new THREE.Vector3(1, 1, 1)
  );
  const world = follow.clone().multiply(local);
  const p = new THREE.Vector3();
  const q = new THREE.Quaternion();
  world.decompose(p, q, new THREE.Vector3());
  return { position: [p.x, p.y, p.z], rotation: [q.x, q.y, q.z, q.w], follow };
}

/** Normal del plano de la pata en reposo (cadera, rodilla, tobillo), o la del plano con el pole */
export function restLegNormal(ctx: RigContext, a: number, b: number, c: number, poleRest?: Vec3): Vec3 {
  const A = ctx.bones[a].position;
  const B = ctx.bones[b].position;
  const C = ctx.bones[c].position;
  const n = cross(sub(B, A), sub(C, B));
  if (length(n) > 1e-4 * length(sub(C, A)) ** 2) return unit(n, [1, 0, 0]);
  const toward = poleRest ? sub(poleRest, A) : bodyAxes(ctx.body).forward;
  return unit(cross(toward, sub(B, A)), ctx.frames[b]?.x ?? [1, 0, 0]);
}

/** Altura mínima del efector: la del suelo más lo que sobresale de él hacia abajo en reposo */
function groundHeight(ctx: RigContext, effector: number): number {
  const floor = Math.min(...ctx.bones.map((b) => b.position[1]));
  const below = [effector, ...descendants(ctx.children, effector)].map((j) => ctx.bones[j].position[1]);
  return floor + (ctx.bones[effector].position[1] - Math.min(...below));
}

const toV3 = (v: THREE.Vector3): Vec3 => [v.x, v.y, v.z];

/**
 * Pie invertido: dónde va el tobillo y cómo giran el pie y los dedos según
 * el objetivo y el balanceo (grados; positivo levanta el talón rodando
 * sobre la planta y después la punta, negativo rueda sobre el talón)
 */
function reverseFoot(
  ctx: RigContext,
  ankle: number,
  foot: number[],
  target: { position: Vec3; rotation: Quat; follow: THREE.Matrix4 },
  control: RigControl,
  roll: number
): { ankle: Vec3; footWorld: Quat; toeWorld?: Quat; ball?: number } {
  const rest = (j: number) => new THREE.Vector3(...ctx.bones[j].position);
  const A = rest(ankle);
  const ball = foot.length >= 2 ? foot[0] : undefined;
  const tip = foot[foot.length - 1];
  const T = rest(tip);
  const floor = Math.min(A.y, T.y, ...(ball !== undefined ? [rest(ball).y] : []));
  const heel = new THREE.Vector3(A.x, floor, A.z);
  const toward = T.clone().sub(A).setY(0);
  if (toward.lengthSq() < 1e-12) toward.set(...bodyAxes(ctx.body).forward);
  const lateral = new THREE.Vector3(0, 1, 0).cross(toward.normalize()).normalize();
  const turn = (angle: number) => new THREE.Quaternion().setFromAxisAngle(lateral, THREE.MathUtils.degToRad(angle));
  const about = (q: THREE.Quaternion, pivot: THREE.Vector3, p: THREE.Vector3) => p.clone().sub(pivot).applyQuaternion(q).add(pivot);
  // Balanceo en el espacio de reposo del pie
  let footQ = new THREE.Quaternion();
  let toeQ = new THREE.Quaternion();
  let a = A.clone();
  if (roll < 0) {
    footQ = turn(roll);
    toeQ = footQ.clone();
    a = about(footQ, heel, A);
  } else if (roll > 0) {
    const BREAK = 35;
    const onBall = ball !== undefined ? Math.min(roll, BREAK) : 0;
    const onTip = roll - onBall;
    const qTip = turn(onTip);
    const qBall = turn(onBall);
    a = about(qTip, T, ball !== undefined ? about(qBall, rest(ball), A) : A);
    footQ = qTip.clone().multiply(qBall);
    toeQ = qTip;
  }
  // Y todo lo lleva el control: su reposo es su posición
  const P = new THREE.Vector3(...control.position);
  const move = new THREE.Matrix4().compose(new THREE.Vector3(...target.position), new THREE.Quaternion(...target.rotation), new THREE.Vector3(1, 1, 1));
  move.multiply(new THREE.Matrix4().makeTranslation(-P.x, -P.y, -P.z));
  const R = new THREE.Quaternion(...target.rotation);
  const out = {
    ankle: toV3(a.applyMatrix4(move)),
    footWorld: [...R.clone().multiply(footQ).toArray()] as Quat,
    toeWorld: ball !== undefined ? ([...R.clone().multiply(toeQ).toArray()] as Quat) : undefined,
    ball,
  };
  return out;
}

/** Cuánto se mueve cada articulación (giros y desplazamientos) al pasar de `before` a `after` */
function blendInto(out: Pose, before: Pose, after: Pose, joints: number[], t: number): void {
  for (const j of joints) {
    const b = before.rotations.get(j) ?? IDENTITY;
    const a = after.rotations.get(j) ?? IDENTITY;
    out.rotations.set(j, t >= 1 ? a : slerp(b, a, t));
    const tb = before.translations.get(j);
    const ta = after.translations.get(j);
    if (tb || ta) {
      const x = tb ?? [0, 0, 0];
      const y = ta ?? [0, 0, 0];
      out.translations.set(j, [x[0] + (y[0] - x[0]) * t, x[1] + (y[1] - x[1]) * t, x[2] + (y[2] - x[2]) * t]);
    }
  }
}

/** Controles que mueven la cadena: objetivo, pole y los de la curva */
export function chainControls(chain: IkChain): string[] {
  return [chain.target, chain.pole, ...(chain.curve ?? [])].filter((id): id is string => !!id);
}

/**
 * Mezcla IK/FK de la cadena en `frame`, con `pose` la pose de las keys
 * (antes del IK). Manda la key de mezcla; si no hay, el valor fijo de la
 * cadena; y si tampoco, la animación decide: si anima las articulaciones
 * de la cadena y no sus controles, sigue las keys FK (así las animaciones
 * básicas, las poses y lo animado antes de crear el IK no quedan pisados
 * por controles quietos en reposo); si no, IK.
 */
export function chainBlend(chain: IkChain, clip: AnimationClip | undefined, frame: number, pose: Pose, joints: number[]): number {
  const clamp = (x: number) => Math.max(0, Math.min(1, x));
  const keyed = sampleScalar(clip, chain.id, "blend", frame);
  if (keyed !== undefined) return clamp(keyed);
  if (chain.blend !== undefined) return clamp(chain.blend);
  if (chainControls(chain).some((id) => pose.controls.has(id))) return 1;
  return joints.some((j) => pose.rotations.has(j) || pose.translations.has(j)) ? 0 : 1;
}

/**
 * Ordena las pistas de los clips después de cambiar el rig: quita las de
 * cadenas y controles que ya no existen, y la mezcla 0 que se ponía antes
 * al principio de las animaciones previas al IK (hoy la regla de
 * `chainBlend` da lo mismo sin esa key)
 */
export function tidyClips(clips: AnimationClip[], settings: RigSettings): AnimationClip[] {
  const chains = new Map((settings.ikChains ?? []).map((c) => [c.id, c]));
  const controls = new Set(settings.controls.map((c) => c.id));
  const constraints = new Set((settings.constraints ?? []).map((c) => c.id));
  return clips.map((clip) => {
    const animated = (t: BoneTrack) => t.rotation.length > 0 || t.translation.length > 0;
    let changed = false;
    const tracks = clip.tracks.flatMap((t): BoneTrack[] => {
      if (t.kind === "control" && !controls.has(t.bone)) {
        changed = true;
        return [];
      }
      if (t.kind !== "ik") return [t];
      // Influencia y valor de las restricciones van en pistas "ik" con su id
      if (isConstraintId(t.bone)) {
        if (constraints.has(t.bone)) return [t];
        changed = true;
        return [];
      }
      const chain = chains.get(t.bone);
      if (!chain) {
        changed = true;
        return [];
      }
      const blend = t.blend ?? [];
      const fk =
        blend.length === 1 &&
        blend[0].value === 0 &&
        chain.blend === undefined &&
        !clip.tracks.some((o) => o.kind === "control" && chainControls(chain).includes(o.bone) && animated(o)) &&
        clip.tracks.some((o) => !o.kind && chain.joints.includes(o.bone) && animated(o));
      if (!fk) return [t];
      changed = true;
      const rest: BoneTrack = { ...t, blend: undefined };
      return rest.pin?.length || rest.roll?.length ? [rest] : [];
    });
    return changed ? { ...clip, tracks } : clip;
  });
}

/**
 * IK: cada cadena, en orden, resuelve sus articulaciones hacia su objetivo y
 * se mezcla con la pose de las keys (FK) según su mezcla. Fijada, el
 * efector queda donde estaba cuando empezó el fijado.
 */
export const ikStage: PoseStage = {
  name: "IK",
  run: (pose, ctx, info) => {
    // De la raíz hacia afuera: el centro de masa antes que las patas que se apoyan
    const depth = (name: string) => {
      let d = 0;
      for (let j = ctx.boneIndex.get(name) ?? null; j !== null && j !== undefined; j = ctx.bones[j].parent) d++;
      return d;
    };
    const chains = (ctx.settings.ikChains ?? [])
      .filter((c) => !c.disabled && c.joints.length > 0)
      .map((c) => ({ c, d: depth(c.joints[0]) }))
      .sort((a, b) => a.d - b.d)
      .map(({ c }) => c);
    if (chains.length === 0) return pose;
    const out = clonePose(pose);
    const fk = new Fk(ctx.bones, out);
    const constrain = constrainer(ctx);
    const controls = new Map(ctx.settings.controls.map((c) => [c.id, c]));
    const world = (id?: string) => {
      const c = id ? controls.get(id) : undefined;
      return c ? controlWorld(c, ctx, fk, out) : undefined;
    };
    for (const chain of chains) {
      const joints = chain.joints.map((n) => ctx.boneIndex.get(n));
      if (joints.length === 0 || joints.some((j) => j === undefined)) continue;
      const idx = joints as number[];
      const frame = info.frame ?? 0;
      const start = info.noPins ? undefined : pinStart(info.clip, chain.id, frame);
      const blend = start !== undefined ? 1 : chainBlend(chain, info.clip, frame, pose, idx);
      if (blend <= 0) continue;
      const before = clonePose(out);
      const involved = [...idx];

      if (chain.solver === "root") {
        const target = world(chain.target);
        const control = chain.target ? controls.get(chain.target) : undefined;
        if (!target || !control) continue;
        // La raíz se corre lo mismo que el control desde su reposo, y gira con él
        out.translations.set(idx[0], sub(target.position, control.position));
        out.rotations.set(idx[0], target.rotation);
        fk.update();
      } else if (chain.solver === "twoBone" && idx.length >= 3) {
        const [a, b, c] = idx;
        const foot = idx.slice(3);
        const control = chain.target ? controls.get(chain.target) : undefined;
        const target = world(chain.target);
        const pole = world(chain.pole)?.position;
        let goal = target?.position;
        let footWorld: Quat | undefined;
        let toeWorld: Quat | undefined;
        let ball: number | undefined;
        if (target && control && chain.footRoll && foot.length > 0) {
          const roll = sampleScalar(info.clip, chain.id, "roll", frame) ?? chain.roll ?? 0;
          const r = reverseFoot(ctx, c, foot, target, control, roll);
          goal = r.ankle;
          footWorld = r.footWorld;
          toeWorld = r.toeWorld;
          ball = r.ball;
        } else if (target && chain.alignEffector) {
          footWorld = target.rotation;
        }
        // Fijado: el tobillo y el pie donde estaban al empezar
        if (start !== undefined) {
          const pinned = evaluatePose(info.clip, start, ctx, { noPins: true });
          const pinnedFk = new Fk(ctx.bones, pinned);
          goal = pinnedFk.position(c);
          footWorld = [...pinnedFk.world(c).toArray()] as Quat;
          toeWorld = foot.length >= 2 ? ([...pinnedFk.world(foot[0]).toArray()] as Quat) : undefined;
          ball = foot.length >= 2 ? foot[0] : undefined;
        }
        if (!goal) continue;
        if (chain.ground) {
          const h = groundHeight(ctx, c);
          if (goal[1] < h) goal = [goal[0], h, goal[2]];
        }
        const poleRest = chain.pole ? controls.get(chain.pole)?.position : undefined;
        solveTwoBone(fk, a, b, c, goal, {
          pole,
          restNormal: restLegNormal(ctx, a, b, c, poleRest),
          stretch: chain.stretch,
          softness: chain.softness,
          constrain,
        });
        if (footWorld) fk.setWorld(c, footWorld, constrain);
        if (toeWorld && ball !== undefined) fk.setWorld(ball, toeWorld, constrain);
        involved.push(...foot);
      } else if (chain.solver === "spline") {
        const points = (chain.curve ?? []).flatMap((id) => {
          const w = world(id);
          return w ? [w.position] : [];
        });
        if (points.length === 0) continue;
        solveSpline(fk, idx, points, { fit: chain.stretch, constrain });
      } else if (chain.solver === "lookAt") {
        const target = world(chain.target);
        if (!target) continue;
        solveLookAt(fk, idx, target.position, bodyAxes(ctx.body).forward, { weights: chain.weights, constrain });
      } else if (chain.solver === "fabrik" || chain.solver === "ccd") {
        const target = world(chain.target);
        if (!target || idx.length < 2) continue;
        let goal = target.position;
        const effector = idx[idx.length - 1];
        if (start !== undefined) {
          goal = new Fk(ctx.bones, evaluatePose(info.clip, start, ctx, { noPins: true })).position(effector);
        }
        if (chain.ground) {
          const h = groundHeight(ctx, effector);
          if (goal[1] < h) goal = [goal[0], h, goal[2]];
        }
        const solve = chain.solver === "fabrik" ? solveFabrik : solveCcd;
        solve(fk, idx, goal, { iterations: chain.iterations, constrain });
        if (chain.alignEffector) fk.setWorld(effector, target.rotation, constrain);
      }
      if (blend < 1) {
        const after = clonePose(out);
        blendInto(out, before, after, [...new Set(involved)], blend);
        fk.update();
      }
    }
    return out;
  },
};

/** Límites: ejes de giro bloqueados, rango de giro y traslación bloqueada de cada hueso */
export const limitsStage: PoseStage = {
  name: "límites",
  run: (pose, ctx) => {
    const out = clonePose(pose);
    for (const [j, q] of pose.rotations) {
      if (!ctx.bones[j]) continue;
      out.rotations.set(j, constrainRotation(ctx, j, q));
    }
    for (const j of pose.translations.keys()) {
      const bone = ctx.bones[j];
      if (bone && boneProps(ctx.settings, bone.name).lockTranslation) out.translations.delete(j);
    }
    return out;
  },
};

/** Restricciones (F3): copiar, hijo de, seguir, estirar, mapeo, drivers y reparto */
export const constraintStage: PoseStage = {
  name: "restricciones",
  run: (pose, ctx, info) => applyConstraints(pose, ctx, info, { controlWorld }),
};

/** Resortes y seguimiento (F8): simulados con el clip hasta el cuadro anterior */
export const secondaryStage: PoseStage = {
  name: "resortes",
  run: (pose, ctx, info) =>
    applySecondary(pose, ctx, info, {
      before: (clip, frame) =>
        [constraintStage, ikStage].reduce(
          (p, stage) => stage.run(p, ctx, { clip, frame }),
          samplePose(clip, frame, ctx.boneIndex, ctx.rotation)
        ),
      ground: (j) => groundHeight(ctx, j),
    }),
};

/** Etapas después de las keys, en orden: restricciones → IK → resortes → límites */
export const POSE_STACK: PoseStage[] = [constraintStage, ikStage, secondaryStage, limitsStage];

/** Pasa una pose (de keys o editada a mano) por la pila */
export function runPoseStack(pose: Pose, ctx: RigContext, info: StageInfo = {}): Pose {
  const stages = info.clip?.baked ? [limitsStage] : POSE_STACK;
  return stages.reduce((p, stage) => stage.run(p, ctx, info), pose);
}

/** Pose final del clip en `frame` */
export function evaluatePose(clip: AnimationClip | undefined, frame: number, ctx: RigContext, options: { noPins?: boolean } = {}): Pose {
  return runPoseStack(samplePose(clip, frame, ctx.boneIndex, ctx.rotation), ctx, { clip, frame, noPins: options.noPins });

}

// ─── Horneado para exportar ─────────────────────────────────────────────────

const round = (x: number) => Math.round(x * 1e6) / 1e6 || 0;

/** Quita keys que la interpolación lineal entre sus vecinas ya reproduce */
export function reduceKeys<T extends number[]>(keys: Key<T>[], close: (a: T, b: T) => boolean, mix: (a: T, b: T, t: number) => T): Key<T>[] {
  if (keys.length <= 2) return keys;
  const out: Key<T>[] = [keys[0]];
  let last = 0;
  for (let i = 1; i < keys.length - 1; i++) {
    const a = keys[last];
    const b = keys[i + 1];
    // Se puede saltar si todas las keys entre `a` y `b` quedan sobre la
    // interpolación (con un tope de tramo, para no volverse cuadrático)
    let redundant = i - last < 120;
    for (let k = last + 1; k <= i && redundant; k++) {
      const t = (keys[k].frame - a.frame) / (b.frame - a.frame);
      redundant = close(mix(a.value, b.value, t), keys[k].value);
    }
    if (!redundant) {
      out.push(keys[i]);
      last = i;
    }
  }
  out.push(keys[keys.length - 1]);
  return out;
}

/**
 * Hornea la pila de evaluación: una key por cuadro (lineal) en cada
 * articulación que se mueve, y luego se quitan las que sobran. El resultado
 * es FK puro, lo único que entienden glTF, USD y BVH. Las pistas de los
 * controles no se exportan.
 */
export function bakeClip(clip: AnimationClip, ctx: RigContext): AnimationClip {
  const start = Math.round(clip.start);
  const end = Math.max(start, Math.round(clip.end));
  const rotations = new Map<number, Key<Quat>[]>();
  const translations = new Map<number, Key<Vec3>[]>();
  for (let f = start; f <= end; f++) {
    const pose = evaluatePose(clip, f, ctx);
    for (const [j, q] of pose.rotations) {
      if (!rotations.has(j)) rotations.set(j, []);
      rotations.get(j)!.push({ frame: f, value: q.map(round) as Quat, interpolation: "linear" });
    }
    for (const [j, t] of pose.translations) {
      if (!translations.has(j)) translations.set(j, []);
      translations.get(j)!.push({ frame: f, value: t.map(round) as Vec3, interpolation: "linear" });
    }
  }
  const angleTol = 1e-4;
  const distTol = 1e-5 * (ctx.body?.size ?? 1);
  const joints = [...new Set([...rotations.keys(), ...translations.keys()])].sort((a, b) => a - b);
  const tracks: BoneTrack[] = joints.map((j) => ({
    bone: ctx.bones[j].name,
    rotation: reduceKeys(
      rotations.get(j) ?? [],
      (a, b) => quatAngle(quatMultiply(quatInverse(a), b)) < angleTol,
      slerp
    ),
    translation: reduceKeys(
      translations.get(j) ?? [],
      (a, b) => length(sub(a, b)) < distTol,
      (a, b, t) => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
    ),
  }));
  return { ...clip, tracks };
}

// ─── Simetría ───────────────────────────────────────────────────────────────

/** Nombre del par del otro lado: hip_l ↔ hip_r, hand.L ↔ hand.R, LeftArm ↔ RightArm, paw_fl ↔ paw_fr */
export function mirrorName(name: string): string | undefined {
  const suffix = /^(.*[._-][a-zA-Z]*?)([lLrR])$/.exec(name);
  if (suffix) {
    const flip: Record<string, string> = { l: "r", r: "l", L: "R", R: "L" };
    return suffix[1] + flip[suffix[2]];
  }
  const word = /(left|right)/i.exec(name);
  if (word) {
    const w = word[1];
    const other = w.toLowerCase() === "left" ? "right" : "left";
    const cased = w === w.toUpperCase() ? other.toUpperCase() : w[0] === w[0].toUpperCase() ? other[0].toUpperCase() + other.slice(1) : other;
    return name.slice(0, word.index) + cased + name.slice(word.index + w.length);
  }
  return undefined;
}

/** Refleja un punto en el plano de simetría */
export function reflectPoint(p: Vec3, normal: Vec3, point: Vec3): Vec3 {
  const d = dot(sub(p, point), normal);
  return sub(p, scale(normal, 2 * d));
}

/** Refleja una dirección (o desplazamiento) */
export function reflectVector(v: Vec3, normal: Vec3): Vec3 {
  return sub(v, scale(normal, 2 * dot(v, normal)));
}

/** El giro visto en el espejo: eje reflejado y ángulo al revés */
export function reflectQuat(q: Quat, normal: Vec3): Quat {
  const v = reflectVector([q[0], q[1], q[2]], normal);
  return [-v[0], -v[1], -v[2], q[3]];
}

/**
 * Par de cada articulación: por nombre y, si el nombre no lo dice, por la
 * posición reflejada (las que caen sobre el plano no tienen par).
 */
export function mirrorPairs(bones: SkeletonBone[], normal: Vec3, point: Vec3): (number | null)[] {
  const byName = new Map(bones.map((b, i) => [b.name, i]));
  const pairs: (number | null)[] = bones.map((b) => {
    const twin = mirrorName(b.name);
    const j = twin === undefined ? undefined : byName.get(twin);
    return j === undefined ? null : j;
  });
  let size = 0;
  for (const a of bones) for (const b of bones) size = Math.max(size, length(sub(a.position, b.position)));
  const tol = 0.02 * (size || 1);
  bones.forEach((b, i) => {
    if (pairs[i] !== null) return;
    if (Math.abs(dot(sub(b.position, point), normal)) < tol) return;
    const target = reflectPoint(b.position, normal, point);
    let best = -1;
    let bestD = tol;
    bones.forEach((c, k) => {
      if (k === i || pairs[k] !== null) return;
      const d = length(sub(c.position, target));
      if (d < bestD) {
        bestD = d;
        best = k;
      }
    });
    if (best >= 0) {
      pairs[i] = best;
      pairs[best] = i;
    }
  });
  return pairs;
}

// ─── Grupos automáticos ─────────────────────────────────────────────────────

const KIND_NAMES: Record<ChainKind, string> = {
  leg: "Pata",
  arm: "Brazo",
  wing: "Ala",
  tail: "Cola",
  head: "Cabeza",
  trunk: "Trompa",
  ear: "Oreja",
  antenna: "Antena",
  pincer: "Pinza",
  fin: "Aleta",
  tentacle: "Tentáculo",
  mouth: "Boca",
  body: "Cuerpo",
  other: "Otros",
};

/** Tono (0–360) por tipo de cadena; la izquierda más clara que la derecha */
const KIND_HUES: Record<ChainKind, number> = {
  leg: 210,
  arm: 140,
  wing: 190,
  tail: 30,
  head: 50,
  trunk: 280,
  ear: 320,
  antenna: 260,
  pincer: 0,
  fin: 170,
  tentacle: 300,
  mouth: 15,
  body: 45,
  other: 0,
};

function hsl(h: number, s: number, l: number): string {
  return `#${new THREE.Color().setHSL(h / 360, s, l).getHexString()}`;
}

/**
 * Grupos por cadena del cuerpo: patas, brazos, alas, cola, cabeza… con lado
 * y, entre varias patas, delanteras y traseras. La columna (raíz, tronco y
 * cuello) va aparte. Devuelve los grupos y el grupo de cada hueso.
 */
export function autoGroups(bones: SkeletonBone[], body: Body | null = analyzeBody(bones)): { groups: BoneGroup[]; assignment: Map<string, string> } {
  const groups: BoneGroup[] = [];
  const assignment = new Map<string, string>();
  if (!body) return { groups, assignment };

  const spine: BoneGroup = { id: "auto-spine", name: "Columna", color: hsl(45, 0.7, 0.55), auto: true };
  const spineJoints = [body.root, ...body.spine, ...body.neck];
  groups.push(spine);
  spineJoints.forEach((j) => assignment.set(bones[j].name, spine.id));

  // Nombre base de cada cadena: tipo, adelante/atrás entre varias patas, lado
  const legs = body.chains.filter((c) => c.kind === "leg");
  const alongs = legs.map((c) => c.along).sort((a, b) => a - b);
  const mid = alongs.length >= 4 ? (alongs[0] + alongs[alongs.length - 1]) / 2 : null;
  const names = body.chains.map((c) => {
    // Dos patas con brazos: bípedo, son piernas
    const biped = legs.length === 2 && body.chains.some((x) => x.kind === "arm");
    let name = c.kind === "leg" && biped ? "Pierna" : KIND_NAMES[c.kind];
    if (c.kind === "leg" && mid !== null && legs.length === 4) name += c.along > mid ? " delantera" : " trasera";
    if (c.side < 0) name += " izq.";
    else if (c.side > 0) name += " der.";
    return name;
  });
  // Nombres repetidos (arañas, tentáculos): numerados
  const total = new Map<string, number>();
  names.forEach((n) => total.set(n, (total.get(n) ?? 0) + 1));
  const seen = new Map<string, number>();
  body.chains.forEach((chain, k) => {
    let name = names[k];
    if ((total.get(name) ?? 0) > 1) {
      const i = (seen.get(name) ?? 0) + 1;
      seen.set(name, i);
      name = `${name} ${i}`;
    }
    const lightness = chain.side < 0 ? 0.62 : chain.side > 0 ? 0.48 : 0.55;
    const group: BoneGroup = { id: `auto-${k}`, name, color: hsl(KIND_HUES[chain.kind], 0.7, lightness), auto: true };
    groups.push(group);
    for (const j of chain.joints) if (!assignment.has(bones[j].name)) assignment.set(bones[j].name, group.id);
  });
  return { groups, assignment };
}

/** Reemplaza los grupos automáticos (conserva los que hizo el usuario y sus huesos) */
export function withAutoGroups(settings: RigSettings, bones: SkeletonBone[]): RigSettings {
  const { groups, assignment } = autoGroups(bones);
  const manual = settings.groups.filter((g) => !g.auto);
  const manualIds = new Set(manual.map((g) => g.id));
  const hiddenByName = new Map(settings.groups.filter((g) => g.auto).map((g) => [g.name, g.hidden]));
  const next: RigSettings = {
    ...settings,
    groups: [...groups.map((g) => (hiddenByName.get(g.name) ? { ...g, hidden: true } : g)), ...manual],
    bones: { ...settings.bones },
  };
  for (const bone of bones) {
    const current = settings.bones[bone.name]?.group;
    if (current && manualIds.has(current)) continue;
    const group = assignment.get(bone.name);
    const props: BoneProps = { ...next.bones[bone.name] };
    if (group) props.group = group;
    else delete props.group;
    if (Object.keys(props).length === 0) delete next.bones[bone.name];
    else next.bones[bone.name] = props;
  }
  return next;
}

// ─── Selección ──────────────────────────────────────────────────────────────

/** Cadena sin ramificar que contiene `j`: sube y baja mientras no haya bifurcación */
export function chainAround(bones: SkeletonBone[], children: number[][], j: number): number[] {
  const out = [j];
  let up = j;
  while (true) {
    const p = bones[up].parent;
    if (p === null || children[p].length !== 1) break;
    out.unshift(p);
    up = p;
  }
  let down = j;
  while (children[down].length === 1) {
    down = children[down][0];
    out.push(down);
  }
  return out;
}

/** Todas las articulaciones que cuelgan de `j` (sin ella) */
export function descendants(children: number[][], j: number): number[] {
  const out: number[] = [];
  const stack = [...children[j]];
  while (stack.length > 0) {
    const c = stack.pop()!;
    out.push(c);
    stack.push(...children[c]);
  }
  return out;
}
