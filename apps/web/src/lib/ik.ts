/**
 * Cinemática del rig y solucionadores de IK.
 *
 * Misma cinemática que `Viewer3D.buildRig` y la exportación: un hueso por
 * articulación `b` (el segmento padre(b) → b), con la cabeza en la posición
 * del padre y orientación identidad en reposo. El giro de la articulación `J`
 * va a los huesos de sus hijos (la raíz, a su propio hueso) y su
 * desplazamiento también: corre el punto `J` con todo lo que cuelga.
 *
 * Los solucionadores cambian giros locales de la pose (`Pose.rotations`) y,
 * al estirar, desplazamientos. Trabajan en el espacio del modelo.
 */

import * as THREE from "three";
import type { Pose, Quat, Vec3 } from "./animation";

export interface IkBone {
  name: string;
  position: Vec3;
  parent: number | null;
}

// ─── Vectores y cuaterniones ────────────────────────────────────────────────

const I: Quat = [0, 0, 0, 1];
const v3 = (v: Vec3) => new THREE.Vector3(v[0], v[1], v[2]);
const q4 = (q: Quat) => new THREE.Quaternion(q[0], q[1], q[2], q[3]);
const toV = (v: THREE.Vector3): Vec3 => [v.x, v.y, v.z];
const toQ = (q: THREE.Quaternion): Quat => [q.x, q.y, q.z, q.w];

/** Giro mínimo que lleva la dirección `from` a `to` */
export function rotationBetween(from: Vec3, to: Vec3): Quat {
  const a = v3(from).normalize();
  const b = v3(to).normalize();
  if (a.lengthSq() < 1e-20 || b.lengthSq() < 1e-20) return I;
  return toQ(new THREE.Quaternion().setFromUnitVectors(a, b));
}

/** Giro que lleva el par (dirección, normal) a otro par: alinea también el giro alrededor de la dirección */
export function rotationBetweenFrames(x1: Vec3, n1: Vec3, x2: Vec3, n2: Vec3): Quat {
  const basis = (x: Vec3, n: Vec3) => {
    const bx = v3(x).normalize();
    const bn = v3(n).sub(bx.clone().multiplyScalar(v3(n).dot(bx)));
    if (bn.lengthSq() < 1e-16) return null;
    bn.normalize();
    const bz = bx.clone().cross(bn);
    return new THREE.Quaternion().setFromRotationMatrix(new THREE.Matrix4().makeBasis(bx, bn, bz));
  };
  const a = basis(x1, n1);
  const b = basis(x2, n2);
  if (!a || !b) return rotationBetween(x1, x2);
  return toQ(b.multiply(a.invert()).normalize());
}

// ─── Cinemática directa ─────────────────────────────────────────────────────

/**
 * Estado de la pose en el espacio del modelo: giro y cabeza de cada hueso, y
 * la posición de cada articulación. `update()` lo rehace después de cambiar
 * la pose.
 */
export class Fk {
  readonly bones: IkBone[];
  readonly children: number[][];
  /** Orden de la raíz hacia las puntas */
  readonly order: number[];
  /** Traslación de reposo de cada hueso respecto del padre */
  private readonly rest: THREE.Vector3[];
  /** De la cabeza del hueso a su articulación (reposo) */
  private readonly tip: THREE.Vector3[];
  rot: THREE.Quaternion[] = [];
  origin: THREE.Vector3[] = [];
  joint: THREE.Vector3[] = [];

  constructor(bones: IkBone[], public pose: Pose) {
    this.bones = bones;
    this.children = bones.map(() => [] as number[]);
    bones.forEach((b, i) => b.parent !== null && this.children[b.parent]?.push(i));
    this.order = [];
    const visit = (b: number) => {
      this.order.push(b);
      this.children[b].forEach(visit);
    };
    bones.forEach((b, i) => b.parent === null && visit(i));
    const head = (b: number) => v3(bones[bones[b].parent ?? b].position);
    this.rest = bones.map((b, i) => (b.parent === null ? head(i) : head(i).sub(head(b.parent))));
    this.tip = bones.map((b, i) => v3(b.position).sub(head(i)));
    this.update();
  }

  /** Giro y desplazamiento locales que le tocan al hueso `b` */
  private local(b: number): { q: THREE.Quaternion; t: THREE.Vector3 } {
    const p = this.bones[b].parent;
    const driver = p === null ? b : this.bones[p].parent === null ? null : p;
    const q = driver === null ? new THREE.Quaternion() : q4(this.pose.rotations.get(driver) ?? I);
    const t = this.rest[b].clone();
    if (driver !== null) {
      const d = this.pose.translations.get(driver);
      if (d) t.add(v3(d));
    }
    return { q, t };
  }

  update(): void {
    for (const b of this.order) {
      const { q, t } = this.local(b);
      const p = this.bones[b].parent;
      if (p === null) {
        this.rot[b] = q;
        this.origin[b] = t;
      } else {
        this.rot[b] = this.rot[p].clone().multiply(q);
        this.origin[b] = this.origin[p].clone().add(t.applyQuaternion(this.rot[p]));
      }
    }
    for (const b of this.order) {
      const kids = this.children[b];
      this.joint[b] = kids.length > 0 ? this.origin[kids[0]].clone() : this.origin[b].clone().add(this.tip[b].clone().applyQuaternion(this.rot[b]));
    }
  }

  /** Giro del marco en el que vive el giro local de la articulación (el hueso que llega a ella) */
  frame(j: number): THREE.Quaternion {
    return this.bones[j].parent === null ? new THREE.Quaternion() : this.rot[j].clone();
  }

  /** Giro en mundo de lo que gira la articulación (sus hijos, o su hueso en la raíz) */
  world(j: number): THREE.Quaternion {
    return this.frame(j).multiply(q4(this.pose.rotations.get(j) ?? I));
  }

  position(j: number): Vec3 {
    return toV(this.joint[j]);
  }

  /** Suma el giro `delta` (en mundo) al giro local de `j`, pasándolo por `constrain` */
  rotate(j: number, delta: Quat, constrain?: (j: number, q: Quat) => Quat): void {
    const f = this.frame(j);
    const local = f.clone().invert().multiply(q4(delta)).multiply(f).multiply(q4(this.pose.rotations.get(j) ?? I)).normalize();
    let q = toQ(local);
    if (constrain) q = constrain(j, q);
    this.pose.rotations.set(j, q);
    this.update();
  }

  /** Pone el giro de `j` para que lo que gira quede con el giro `world` en mundo */
  setWorld(j: number, world: Quat, constrain?: (j: number, q: Quat) => Quat): void {
    let q = toQ(this.frame(j).invert().multiply(q4(world)).normalize());
    if (constrain) q = constrain(j, q);
    this.pose.rotations.set(j, q);
    this.update();
  }

  /** Desplaza la articulación `j` hasta `target` (en mundo), en el marco de su padre */
  moveTo(j: number, target: Vec3): void {
    const frame = this.frame(j);
    const current = v3(this.pose.translations.get(j) ?? [0, 0, 0]);
    const delta = v3(target).sub(this.joint[j]).applyQuaternion(frame.invert());
    this.pose.translations.set(j, toV(current.add(delta)));
    this.update();
  }
}

// ─── IK de dos huesos ───────────────────────────────────────────────────────

export interface TwoBoneOptions {
  /** Hacia dónde apunta la rodilla (en mundo) */
  pole?: Vec3;
  /**
   * Normal del plano de la pata en reposo, en el marco del hueso A→B: la
   * lleva el muslo, así la rodilla sigue doblando hacia el mismo lado
   * aunque la pata esté recta
   */
  restNormal: Vec3;
  /** Alarga los huesos si el objetivo queda lejos */
  stretch?: boolean;
  /** Fracción del largo en la que la extensión se suaviza (0 = sin suavizar) */
  softness?: number;
  constrain?: (j: number, q: Quat) => Quat;
}

/**
 * IK analítico: las articulaciones `a` (cadera) y `b` (rodilla) giran para
 * que `c` (tobillo) llegue a `target`. La rodilla va hacia `pole` si se da;
 * si no, conserva el plano que tenía.
 */
export function solveTwoBone(fk: Fk, a: number, b: number, c: number, target: Vec3, opts: TwoBoneOptions): void {
  const A = fk.joint[a].clone();
  const B = fk.joint[b].clone();
  const C = fk.joint[c].clone();
  const T = v3(target);
  const la = B.distanceTo(A);
  const lb = C.distanceTo(B);
  const full = la + lb;
  if (la < 1e-9 || lb < 1e-9) return;
  const toT = T.clone().sub(A);
  let d = toT.length();
  const dir = d > 1e-9 ? toT.clone().divideScalar(d) : C.clone().sub(A).normalize();

  // Suavizado de la extensión (soft IK) y estiramiento
  let scale = 1;
  const soft = Math.max(0, Math.min(0.5, opts.softness ?? 0)) * full;
  const start = full - soft;
  if (soft > 0 && d > start) {
    const reach = start + soft * (1 - Math.exp(-(d - start) / soft));
    if (opts.stretch) scale = d / reach;
    d = reach;
  } else if (d > full && opts.stretch) {
    scale = d / full;
  }
  d = Math.max(Math.abs(la - lb) + 1e-6, Math.min(full - 1e-6, d));
  const [sa, sb] = [la * scale, lb * scale];
  const dd = d * scale;

  // Dirección de la rodilla: el pole o el plano que ya tiene la pata
  const plane = v3(opts.restNormal).applyQuaternion(fk.rot[b]).normalize();
  let side: THREE.Vector3;
  if (opts.pole) {
    side = v3(opts.pole).sub(A);
    side.sub(dir.clone().multiplyScalar(side.dot(dir)));
  } else {
    side = new THREE.Vector3().crossVectors(dir, plane);
  }
  if (side.lengthSq() < 1e-16) side = new THREE.Vector3().crossVectors(dir, plane);
  side.normalize();
  const cosA = Math.max(-1, Math.min(1, (sa * sa + dd * dd - sb * sb) / (2 * sa * dd)));
  const B2 = A.clone().add(dir.clone().multiplyScalar(sa * cosA)).add(side.clone().multiplyScalar(sa * Math.sqrt(1 - cosA * cosA)));
  const C2 = A.clone().add(dir.clone().multiplyScalar(dd));
  const normal2 = new THREE.Vector3().crossVectors(B2.clone().sub(A), C2.clone().sub(B2));

  // Cadera: el muslo a su lugar y el plano de la pata al del objetivo
  const delta = rotationBetweenFrames(toV(B.clone().sub(A)), toV(plane), toV(B2.clone().sub(A)), toV(normal2));
  fk.rotate(a, delta, opts.constrain);
  // Estirar: la rodilla y el tobillo se corren a lo largo de sus huesos
  if (scale !== 1) {
    const Bn = fk.joint[b].clone();
    fk.moveTo(b, toV(A.clone().add(Bn.sub(A).multiplyScalar(scale))));
  }
  // Rodilla: la pierna hacia el tobillo
  const B3 = fk.joint[b].clone();
  fk.rotate(b, rotationBetween(toV(fk.joint[c].clone().sub(B3)), toV(C2.clone().sub(B3))), opts.constrain);
  if (scale !== 1 && fk.children[c].length > 0) {
    const Cn = fk.joint[c].clone();
    fk.moveTo(c, toV(B3.clone().add(Cn.sub(B3).multiplyScalar(scale))));
  }
}

// ─── Solucionadores iterativos ──────────────────────────────────────────────

export interface ChainOptions {
  iterations?: number;
  /** Distancia a la que se da por llegado */
  tolerance?: number;
  constrain?: (j: number, q: Quat) => Quat;
}

/**
 * CCD: de la punta hacia la base, cada articulación gira lo mínimo para
 * acercar el efector al objetivo. `joints` va de la base al efector; el
 * efector no gira. Respeta los límites en cada paso.
 */
export function solveCcd(fk: Fk, joints: number[], target: Vec3, opts: ChainOptions = {}): void {
  const effector = joints[joints.length - 1];
  const T = v3(target);
  const tol = opts.tolerance ?? 1e-4;
  for (let it = 0; it < (opts.iterations ?? 12); it++) {
    for (let k = joints.length - 2; k >= 0; k--) {
      const P = fk.joint[joints[k]];
      const E = fk.joint[effector];
      fk.rotate(joints[k], rotationBetween(toV(E.clone().sub(P)), toV(T.clone().sub(P))), opts.constrain);
    }
    if (fk.joint[effector].distanceTo(T) < tol) break;
  }
}

/** Gira cada articulación de la cadena para que su hueso apunte a la posición deseada de la siguiente */
function alignChain(fk: Fk, joints: number[], positions: THREE.Vector3[], constrain?: (j: number, q: Quat) => Quat): void {
  for (let k = 0; k < joints.length - 1; k++) {
    const P = fk.joint[joints[k]];
    const next = fk.joint[joints[k + 1]];
    fk.rotate(joints[k], rotationBetween(toV(next.clone().sub(P)), toV(positions[k + 1].clone().sub(P))), constrain);
  }
}

/**
 * FABRIK: ajusta las posiciones de la cadena hacia adelante y hacia atrás
 * conservando los largos, y después gira cada articulación hacia la suya.
 * Con límites se repite desde lo que quedó, así el resultado los respeta.
 */
export function solveFabrik(fk: Fk, joints: number[], target: Vec3, opts: ChainOptions = {}): void {
  const T = v3(target);
  const tol = opts.tolerance ?? 1e-4;
  const rounds = opts.constrain ? 4 : 1;
  for (let round = 0; round < rounds; round++) {
    const p = joints.map((j) => fk.joint[j].clone());
    const lengths = p.slice(1).map((q, i) => q.distanceTo(p[i]));
    const total = lengths.reduce((s, l) => s + l, 0);
    const base = p[0].clone();
    if (T.distanceTo(base) >= total) {
      const dir = T.clone().sub(base).normalize();
      for (let i = 1; i < p.length; i++) p[i] = p[i - 1].clone().add(dir.clone().multiplyScalar(lengths[i - 1]));
    } else {
      for (let it = 0; it < (opts.iterations ?? 12); it++) {
        p[p.length - 1] = T.clone();
        for (let i = p.length - 2; i >= 0; i--) {
          p[i] = p[i + 1].clone().add(p[i].clone().sub(p[i + 1]).normalize().multiplyScalar(lengths[i]));
        }
        p[0] = base.clone();
        for (let i = 1; i < p.length; i++) {
          p[i] = p[i - 1].clone().add(p[i].clone().sub(p[i - 1]).normalize().multiplyScalar(lengths[i - 1]));
        }
        if (p[p.length - 1].distanceTo(T) < tol) break;
      }
    }
    alignChain(fk, joints, p, opts.constrain);
    if (fk.joint[joints[joints.length - 1]].distanceTo(T) < tol) break;
  }
}

// ─── IK por curva ───────────────────────────────────────────────────────────

/** Puntos de una curva Catmull-Rom que pasa por `points`, cada `step` de largo */
function curvePolyline(points: THREE.Vector3[], samples = 16): THREE.Vector3[] {
  if (points.length < 2) return points.map((p) => p.clone());
  const curve = new THREE.CatmullRomCurve3(points, false, "centripetal");
  return curve.getPoints(samples * (points.length - 1));
}

/** Punto a distancia `s` a lo largo de la polilínea (sigue recta después del final) */
function pointAt(line: THREE.Vector3[], s: number): THREE.Vector3 {
  let acc = 0;
  for (let i = 0; i < line.length - 1; i++) {
    const len = line[i].distanceTo(line[i + 1]);
    if (acc + len >= s || i === line.length - 2) {
      const t = len > 0 ? (s - acc) / len : 0;
      return line[i].clone().lerp(line[i + 1], t);
    }
    acc += len;
  }
  return line[line.length - 1].clone();
}

/**
 * La cadena sigue la curva que pasa por su base y por `points` (controles):
 * cada articulación queda a su largo de la anterior sobre la curva. Con
 * `fit`, la cadena se estira o encoge a lo largo de la curva.
 */
export function solveSpline(fk: Fk, joints: number[], points: Vec3[], opts: { fit?: boolean; constrain?: (j: number, q: Quat) => Quat } = {}): void {
  const base = fk.joint[joints[0]].clone();
  const line = curvePolyline([base, ...points.map(v3)]);
  const lengths = joints.slice(1).map((j, i) => fk.joint[j].distanceTo(fk.joint[joints[i]]));
  const total = lengths.reduce((s, l) => s + l, 0);
  let curveLength = 0;
  for (let i = 0; i < line.length - 1; i++) curveLength += line[i].distanceTo(line[i + 1]);
  const scale = opts.fit && total > 0 ? curveLength / total : 1;
  const targets = [base];
  let s = 0;
  for (const l of lengths) {
    s += l * scale;
    targets.push(pointAt(line, s));
  }
  // Sin estirar los huesos: cada uno apunta hacia su punto de la curva
  alignChain(fk, joints, targets, opts.constrain);
}

// ─── Mirar a ────────────────────────────────────────────────────────────────

/**
 * Mirar a: `joints` (cuello y cabeza, de la base a la punta) reparten el
 * giro que lleva la dirección `forward` (la que mira la cabeza en reposo, en
 * el marco de lo que gira la última articulación) hacia `target`, según
 * `weights` (por defecto parejo). La última termina mirando exacto.
 */
export function solveLookAt(
  fk: Fk,
  joints: number[],
  target: Vec3,
  forward: Vec3,
  opts: { weights?: number[]; constrain?: (j: number, q: Quat) => Quat } = {}
): void {
  const n = joints.length;
  if (n === 0) return;
  const weights = opts.weights?.length === n ? opts.weights : joints.map(() => 1);
  const last = joints[n - 1];
  for (let k = 0; k < n; k++) {
    const remaining = weights.slice(k).reduce((s, w) => s + w, 0);
    const fraction = remaining > 0 ? weights[k] / remaining : 1;
    const facing = v3(forward).applyQuaternion(fk.world(last));
    const want = v3(target).sub(fk.joint[last]);
    if (want.lengthSq() < 1e-16) return;
    const full = q4(rotationBetween(toV(facing), toV(want)));
    const part = new THREE.Quaternion().slerp(full, fraction);
    fk.rotate(joints[k], toQ(part), opts.constrain);
  }
}
