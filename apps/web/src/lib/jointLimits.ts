/**
 * Límites de las articulaciones (swing-twist).
 *
 * El giro local de una articulación, en sus ejes (Y a lo largo del hueso,
 * X la bisagra, ver `rig.ts`), se parte en **twist** (cuánto gira el hueso
 * sobre su propio eje Y) y **swing** (hacia dónde apunta). El swing se
 * dibuja en un disco con proyección azimutal equidistante: el centro es la
 * dirección de reposo y la distancia al centro es el ángulo de inclinación,
 * en grados. El eje x del disco es inclinarse hacia +X; el y, hacia +Z.
 *
 * - Bisagra (rodilla, codo, dedo): solo gira alrededor de X, entre `min` y `max`.
 * - Rótula (hombro, cadera, cuello): el swing queda dentro de una elipse o un
 *   polígono (debe contener el reposo) y el twist entre sus dos valores.
 *   Con rigidez, cerca del borde el giro se frena en vez de chocar.
 */

import * as THREE from "three";
import type { AnimationClip, Quat, Vec3 } from "./animation";
import { samplePose } from "./animation";
import type { Chain } from "./presetAnimations";
import {
  IDENTITY,
  bodyAxes,
  fromJointSpace,
  toJointSpace,
  vec,
  type JointFrame,
  type RigContext,
} from "./rig";

export type SwingShape =
  | { type: "ellipse"; rx: number; ry: number; cx?: number; cy?: number }
  | { type: "polygon"; points: [number, number][] };

export interface JointLimits {
  kind: "hinge" | "ball";
  /** Bisagra: rango del giro alrededor de X, en grados */
  min?: number;
  max?: number;
  /** Rótula: dónde puede apuntar el hueso (grados en el disco) */
  swing?: SwingShape;
  /** Rótula: giro sobre el propio eje, en grados */
  twist?: [number, number];
  /** Fracción del borde en la que el giro se frena (0 = pared) */
  stiffness?: number;
}

export type DiskPoint = [number, number];

const deg = THREE.MathUtils.radToDeg;
const rad = THREE.MathUtils.degToRad;
const tq = (q: Quat) => new THREE.Quaternion(q[0], q[1], q[2], q[3]);
const fq = (q: THREE.Quaternion): Quat => [q.x, q.y, q.z, q.w];
const wrap = (a: number) => ((((a + 180) % 360) + 360) % 360) - 180;

// ─── Swing y twist ──────────────────────────────────────────────────────────

/** Giro en ejes de la articulación → punto del disco (swing) y twist, en grados */
export function decompose(r: Quat): { swing: DiskPoint; twist: number } {
  const twist = wrap(deg(2 * Math.atan2(r[1], r[3])));
  const t = new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(0, 1, 0), rad(twist));
  const swing = tq(r).multiply(t.invert());
  const d = new THREE.Vector3(0, 1, 0).applyQuaternion(swing);
  const theta = deg(Math.acos(Math.max(-1, Math.min(1, d.y))));
  const h = Math.hypot(d.x, d.z);
  return { swing: h < 1e-9 ? [0, 0] : [(theta * d.x) / h, (theta * d.z) / h], twist };
}

/** Punto del disco y twist → giro en ejes de la articulación */
export function compose(swing: DiskPoint, twist: number): Quat {
  const theta = Math.hypot(swing[0], swing[1]);
  const t = new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(0, 1, 0), rad(twist));
  if (theta < 1e-9) return fq(t);
  const s = Math.sin(rad(theta));
  const d = new THREE.Vector3((s * swing[0]) / theta, Math.cos(rad(theta)), (s * swing[1]) / theta);
  const q = new THREE.Quaternion().setFromUnitVectors(new THREE.Vector3(0, 1, 0), d);
  return fq(q.multiply(t).normalize());
}

/** Ángulo de la bisagra (alrededor de X), en grados */
export function hingeAngle(r: Quat): number {
  return wrap(deg(2 * Math.atan2(r[0], r[3])));
}

export const hingeRotation = (angle: number): Quat => fq(new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(1, 0, 0), rad(angle)));

/** Dirección del hueso para un punto del disco (en ejes de la articulación) */
export function diskDirection(p: DiskPoint): Vec3 {
  const theta = Math.hypot(p[0], p[1]);
  if (theta < 1e-9) return [0, 1, 0];
  const s = Math.sin(rad(theta));
  return [(s * p[0]) / theta, Math.cos(rad(theta)), (s * p[1]) / theta];
}

// ─── Forma del límite ───────────────────────────────────────────────────────

/** Distancia del centro del disco al borde en la dirección (ux, uy) unitaria */
export function boundaryRadius(shape: SwingShape, ux: number, uy: number): number {
  if (shape.type === "ellipse") {
    const { rx, ry } = shape;
    const cx = shape.cx ?? 0;
    const cy = shape.cy ?? 0;
    const a = (ux * ux) / (rx * rx) + (uy * uy) / (ry * ry);
    const b = -2 * ((ux * cx) / (rx * rx) + (uy * cy) / (ry * ry));
    const c = (cx * cx) / (rx * rx) + (cy * cy) / (ry * ry) - 1;
    const disc = b * b - 4 * a * c;
    if (disc < 0 || a === 0) return 0;
    return Math.max(0, (-b + Math.sqrt(disc)) / (2 * a));
  }
  // Polígono: el corte más cercano del rayo con sus lados
  let best = Infinity;
  const pts = shape.points;
  for (let i = 0; i < pts.length; i++) {
    const [ax, ay] = pts[i];
    const [bx, by] = pts[(i + 1) % pts.length];
    const ex = bx - ax;
    const ey = by - ay;
    const den = ux * ey - uy * ex;
    if (Math.abs(den) < 1e-12) continue;
    const t = (ax * ey - ay * ex) / den;
    const s = (ax * uy - ay * ux) / den;
    if (t >= 0 && s >= -1e-9 && s <= 1 + 1e-9) best = Math.min(best, t);
  }
  return Number.isFinite(best) ? best : 0;
}

/** Puntos del borde (para dibujarlo o recorrerlo), `n` direcciones */
export function boundaryPoints(shape: SwingShape, n = 48): DiskPoint[] {
  if (shape.type === "polygon") return shape.points.map((p) => [p[0], p[1]]);
  return Array.from({ length: n }, (_, i) => {
    const a = (i / n) * Math.PI * 2;
    const r = boundaryRadius(shape, Math.cos(a), Math.sin(a));
    return [r * Math.cos(a), r * Math.sin(a)] as DiskPoint;
  });
}

/** Recorte suave: hasta `(1 − s)·R` libre, después se acerca al borde sin pasarlo */
function softClamp(value: number, limit: number, stiffness: number): number {
  if (value <= 0 || limit <= 0) return Math.max(0, Math.min(value, limit));
  const start = limit * (1 - Math.max(0, Math.min(0.9, stiffness)));
  if (value <= start) return value;
  const band = limit - start;
  if (band <= 1e-9) return limit;
  return start + band * (1 - Math.exp(-(value - start) / band));
}

/** El punto del disco adentro del límite (hacia el centro, por su misma dirección) */
export function clampSwing(p: DiskPoint, shape: SwingShape, stiffness = 0): DiskPoint {
  const r = Math.hypot(p[0], p[1]);
  if (r < 1e-9) return p;
  const R = boundaryRadius(shape, p[0] / r, p[1] / r);
  const out = softClamp(r, R, stiffness);
  return out === r ? p : [(p[0] * out) / r, (p[1] * out) / r];
}

const clampRange = (v: number, [a, b]: [number, number]) => Math.min(Math.max(a, b), Math.max(Math.min(a, b), v));

/** Aplica los límites a un giro local (ejes del modelo, como las keys) */
export function applyLimits(q: Quat, frame: JointFrame, limits: JointLimits): Quat {
  const r = toJointSpace(q, frame);
  if (limits.kind === "hinge") {
    const a = clampRange(hingeAngle(r), [limits.min ?? -180, limits.max ?? 180]);
    return fromJointSpace(hingeRotation(a), frame);
  }
  const { swing, twist } = decompose(r);
  const s = limits.swing ? clampSwing(swing, limits.swing, limits.stiffness) : swing;
  const t = limits.twist ? clampRange(twist, limits.twist) : twist;
  if (s === swing && t === twist) return q;
  return fromJointSpace(compose(s, t), frame);
}

/** Cuánto se pasa el giro del límite, en grados (0 = adentro) */
export function limitExcess(q: Quat, frame: JointFrame, limits: JointLimits): number {
  const r = toJointSpace(q, frame);
  if (limits.kind === "hinge") {
    const a = hingeAngle(r);
    const lo = limits.min ?? -180;
    const hi = limits.max ?? 180;
    // Lo que gira fuera de la bisagra también cuenta
    const off = deg(2 * Math.atan2(Math.hypot(r[1], r[2]), Math.abs(r[3])));
    return Math.max(0, lo - a, a - hi, off - 0.5);
  }
  const { swing, twist } = decompose(r);
  let excess = 0;
  if (limits.swing) {
    const len = Math.hypot(swing[0], swing[1]);
    if (len > 1e-9) excess = Math.max(excess, len - boundaryRadius(limits.swing, swing[0] / len, swing[1] / len));
  }
  if (limits.twist) excess = Math.max(excess, limits.twist[0] - twist, twist - limits.twist[1]);
  return Math.max(0, excess);
}

// ─── Espejo y escala ────────────────────────────────────────────────────────

/**
 * Límites del otro lado. Los ejes X de ambos lados miran a la derecha, así
 * que el espejo invierte el x del disco y el twist; la bisagra queda igual.
 */
export function mirrorLimits(l: JointLimits): JointLimits {
  const out: JointLimits = { ...l };
  if (l.swing) {
    out.swing =
      l.swing.type === "ellipse"
        ? { ...l.swing, cx: -(l.swing.cx ?? 0) }
        : { type: "polygon", points: l.swing.points.map(([x, y]) => [-x, y] as [number, number]).reverse() };
  }
  if (l.twist) out.twist = [-l.twist[1], -l.twist[0]];
  return out;
}

/** Rangos multiplicados por `factor` (alrededor del reposo) */
export function scaleLimits(l: JointLimits, factor: number): JointLimits {
  const out: JointLimits = { ...l };
  if (l.min !== undefined) out.min = l.min * factor;
  if (l.max !== undefined) out.max = l.max * factor;
  if (l.twist) out.twist = [l.twist[0] * factor, l.twist[1] * factor];
  if (l.swing) {
    out.swing =
      l.swing.type === "ellipse"
        ? { ...l.swing, rx: l.swing.rx * factor, ry: l.swing.ry * factor, cx: (l.swing.cx ?? 0) * factor, cy: (l.swing.cy ?? 0) * factor }
        : { type: "polygon", points: l.swing.points.map(([x, y]) => [x * factor, y * factor] as [number, number]) };
  }
  return out;
}

// ─── Límites automáticos ────────────────────────────────────────────────────

const KNEE = /knee|shin|tibia|calf|elbow|forearm|lower|radius|tibio/i;

function bendIndex(ctx: RigContext, chain: Chain): number {
  const i = chain.joints.findIndex((j, n) => n > 0 && n < chain.joints.length - 1 && KNEE.test(ctx.bones[j].name));
  return i > 0 ? i : 1;
}

const ball = (rx: number, ry: number, twist: number, cy = 0): JointLimits => ({
  kind: "ball",
  swing: { type: "ellipse", rx, ry, cy },
  twist: [-twist, twist],
});

/**
 * Bisagra que dobla hacia `toward`: el giro positivo alrededor de X lleva la
 * punta hacia Z; si Z mira para el otro lado, el rango va en negativo
 */
function hingeToward(frame: JointFrame, toward: Vec3, flex: number): JointLimits {
  const sign = vec.dot(frame.z, toward) >= 0 ? 1 : -1;
  return sign > 0 ? { kind: "hinge", min: -5, max: flex } : { kind: "hinge", min: -flex, max: 5 };
}

/**
 * Límites anatómicos según el tipo de cadena: caderas y hombros rótulas
 * amplias, rodillas y codos bisagras que doblan hacia su lado (la rodilla
 * lleva el pie atrás, el codo la mano adelante), cuello, cola y apéndices
 * rótulas repartidas entre sus articulaciones
 */
export function anatomicalLimits(ctx: RigContext): Map<number, JointLimits> {
  const out = new Map<number, JointLimits>();
  const body = ctx.body;
  if (!body) return out;
  const { forward } = bodyAxes(body);
  const back = vec.scale(forward, -1);
  for (const chain of body.chains) {
    const rot = chain.rotating;
    if (rot.length === 0) continue;
    const n = rot.length;
    switch (chain.kind) {
      case "leg":
      case "arm": {
        const k = bendIndex(ctx, chain);
        const leg = chain.kind === "leg";
        rot.forEach((j, i) => {
          if (i === k) out.set(j, hingeToward(ctx.frames[j], leg ? back : forward, leg ? 150 : 145));
          else if (i === k - 1 && leg) {
            // Cadera: adelante mucho (≈100°), atrás poco (≈30°), al costado 45°
            const s = Math.sign(vec.dot(forward, ctx.frames[j].z)) || 1;
            out.set(j, { kind: "ball", swing: { type: "ellipse", rx: 45, ry: 65, cy: s * 35 }, twist: [-40, 40] });
          } else if (i === k - 1) out.set(j, ball(95, 100, 90));
          else if (i === k + 1) out.set(j, leg ? ball(30, 45, 20) : ball(40, 75, 80));
          else out.set(j, ball(20, 25, 15));
        });
        // Patas abiertas (arañas): cada articulación gira parejo
        if (leg && chain.sprawl) rot.forEach((j) => out.set(j, ball(45, 45, 15)));
        break;
      }
      case "wing":
        rot.forEach((j, i) => out.set(j, i === 0 ? ball(80, 70, 30) : ball(60, 100, 20)));
        break;
      case "head":
        rot.forEach((j) => out.set(j, ball(45 / Math.sqrt(n), 50 / Math.sqrt(n), 70 / n)));
        break;
      case "tail":
      case "body":
        rot.forEach((j) => out.set(j, ball(35 / Math.sqrt(n / 2), 35 / Math.sqrt(n / 2), 10)));
        break;
      case "trunk":
      case "tentacle":
        rot.forEach((j) => out.set(j, ball(45, 45, 20)));
        break;
      default:
        rot.forEach((j) => out.set(j, ball(35, 35, 15)));
    }
  }
  // Columna y cuello: poco cada vértebra
  const spine = [...body.spine, ...body.neck].filter((j) => ctx.children[j].length > 0);
  for (const j of spine) if (!out.has(j)) out.set(j, ball(20, 25, 20));
  return out;
}

/** Envolvente convexa de puntos 2D (cadena monótona) */
function convexHull(points: DiskPoint[]): DiskPoint[] {
  const pts = [...points].sort((a, b) => a[0] - b[0] || a[1] - b[1]);
  if (pts.length < 3) return pts;
  const cross = (o: DiskPoint, a: DiskPoint, b: DiskPoint) => (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
  const lower: DiskPoint[] = [];
  for (const p of pts) {
    while (lower.length >= 2 && cross(lower[lower.length - 2], lower[lower.length - 1], p) <= 0) lower.pop();
    lower.push(p);
  }
  const upper: DiskPoint[] = [];
  for (const p of [...pts].reverse()) {
    while (upper.length >= 2 && cross(upper[upper.length - 2], upper[upper.length - 1], p) <= 0) upper.pop();
    upper.push(p);
  }
  return [...lower.slice(0, -1), ...upper.slice(0, -1)];
}

/**
 * Límites de lo que usan las animaciones: el rango observado con `margin`
 * grados de más. La rótula toma la envolvente de lo observado (con el
 * reposo adentro)
 */
export function observedLimits(clips: AnimationClip[], ctx: RigContext, joints: number[], kinds: Map<number, "hinge" | "ball">, margin = 5): Map<number, JointLimits> {
  const out = new Map<number, JointLimits>();
  const samples = new Map<number, Quat[]>();
  for (const clip of clips) {
    if (!clip.tracks.some((t) => !t.kind)) continue;
    for (let f = Math.round(clip.start); f <= Math.round(clip.end); f++) {
      const pose = samplePose(clip, f, ctx.boneIndex);
      for (const j of joints) {
        if (!samples.has(j)) samples.set(j, []);
        samples.get(j)!.push(pose.rotations.get(j) ?? IDENTITY);
      }
    }
  }
  for (const [j, qs] of samples) {
    const frame = ctx.frames[j];
    if (kinds.get(j) === "hinge") {
      const angles = qs.map((q) => hingeAngle(toJointSpace(q, frame)));
      out.set(j, { kind: "hinge", min: Math.min(0, ...angles) - margin, max: Math.max(0, ...angles) + margin });
      continue;
    }
    const parts = qs.map((q) => decompose(toJointSpace(q, frame)));
    // El reposo tiene que quedar adentro (no en el borde): un círculo de margen a su alrededor
    const aroundRest = Array.from({ length: 8 }, (_, i) => [margin * Math.cos((i * Math.PI) / 4), margin * Math.sin((i * Math.PI) / 4)] as DiskPoint);
    const hull = convexHull([...aroundRest, ...parts.map((p) => p.swing)]);
    // Margen: cada punto del borde se aleja del centro
    const grown = hull.map(([x, y]) => {
      const r = Math.hypot(x, y);
      return (r < 1e-9 ? [0, 0] : [x + (x / r) * margin, y + (y / r) * margin]) as DiskPoint;
    });
    const swing: SwingShape = grown.length >= 3 ? { type: "polygon", points: grown } : { type: "ellipse", rx: margin, ry: margin };
    const twists = parts.map((p) => p.twist);
    out.set(j, { kind: "ball", swing, twist: [Math.min(0, ...twists) - margin, Math.max(0, ...twists) + margin] });
  }
  return out;
}

// ─── Límites por la malla ───────────────────────────────────────────────────

export interface SkinSample {
  /** Posiciones de reposo (x, y, z por vértice) */
  positions: Float32Array;
  /** Por vértice, `maxInfluences` pares (hueso, peso) */
  weights: Float32Array;
  maxInfluences: number;
}

/**
 * Límites por la malla: gira la articulación de a 5° (en `directions`
 * direcciones del disco, o en los dos sentidos de la bisagra) hasta que la
 * carne de lo que cuelga de ella se mete en la del hueso de arriba, y deja
 * el límite 5° antes. Aproximación: la parte que se mueve gira rígida con
 * su peso, y "meterse" es quedar más cerca del eje del hueso de arriba que
 * el radio típico de su carne.
 */
export function meshLimits(ctx: RigContext, skin: SkinSample, joint: number, kind: "hinge" | "ball", directions = 12): JointLimits | null {
  const parent = ctx.bones[joint].parent;
  if (parent === null || ctx.children[joint].length === 0) return null;
  const moving = new Set<number>();
  const stack = [...ctx.children[joint]];
  while (stack.length > 0) {
    const c = stack.pop()!;
    moving.add(c);
    stack.push(...ctx.children[c]);
  }
  const k = skin.maxInfluences;
  const nv = skin.positions.length / 3;
  const A = new THREE.Vector3(...ctx.bones[parent].position);
  const P = new THREE.Vector3(...ctx.bones[joint].position);
  const axis = P.clone().sub(A);
  const len = axis.length();
  if (len < 1e-9) return null;
  axis.divideScalar(len);
  // Vértices que se mueven (con su peso) y radio de la carne del hueso de arriba
  const movers: { p: THREE.Vector3; w: number }[] = [];
  const radii: number[] = [];
  const v = new THREE.Vector3();
  for (let i = 0; i < nv; i++) {
    let wMove = 0;
    let wStatic = 0;
    for (let s = 0; s < k; s++) {
      const b = skin.weights[(i * k + s) * 2];
      const w = skin.weights[(i * k + s) * 2 + 1];
      if (moving.has(b)) wMove += w;
      else if (b === joint) wStatic += w;
    }
    v.set(skin.positions[3 * i], skin.positions[3 * i + 1], skin.positions[3 * i + 2]);
    if (wMove >= 0.5) movers.push({ p: v.clone(), w: wMove });
    else if (wStatic >= 0.5) {
      const t = v.clone().sub(A).dot(axis);
      if (t > 0.1 * len && t < 0.9 * len) radii.push(v.clone().sub(A).sub(axis.clone().multiplyScalar(t)).length());
    }
  }
  if (movers.length < 8 || radii.length < 8) return null;
  radii.sort((a, b) => a - b);
  const radius = radii[Math.floor(radii.length / 2)] * 0.8;
  const stride = Math.max(1, Math.floor(movers.length / 3000));
  const sample = movers.filter((_, i) => i % stride === 0);
  const inside = (q: THREE.Quaternion) => {
    let count = 0;
    for (const { p, w } of sample) {
      const moved = p.clone().sub(P).applyQuaternion(q).add(P);
      const x = p.clone().lerp(moved, w);
      const t = x.clone().sub(A).dot(axis);
      // Cerca de la articulación la carne siempre se junta: solo cuenta lo que llega al medio del hueso de arriba
      if (t > 0.15 * len && t < 0.75 * len && x.clone().sub(A).sub(axis.clone().multiplyScalar(t)).length() < radius) count++;
    }
    return count;
  };
  const frame = ctx.frames[joint];
  const base = inside(new THREE.Quaternion());
  const tolerance = base + Math.max(2, 0.02 * sample.length);
  /** Hasta dónde se llega girando con `rotation(a)` (grados), de a 5° */
  const sweep = (rotation: (a: number) => Quat) => {
    let last = 0;
    for (let a = 5; a <= 175; a += 5) {
      const q = tq(fromJointSpace(rotation(a), frame));
      if (inside(q) > tolerance) break;
      last = a;
    }
    return Math.max(0, last - 5);
  };
  if (kind === "hinge") {
    return { kind: "hinge", min: -sweep((a) => hingeRotation(-a)), max: sweep((a) => hingeRotation(a)) };
  }
  const points: DiskPoint[] = [];
  for (let i = 0; i < directions; i++) {
    const phi = (i / directions) * Math.PI * 2;
    const r = Math.max(5, sweep((a) => compose([a * Math.cos(phi), a * Math.sin(phi)], 0)));
    points.push([r * Math.cos(phi), r * Math.sin(phi)]);
  }
  return { kind: "ball", swing: { type: "polygon", points }, twist: [-30, 30] };
}

// ─── Recorridos ─────────────────────────────────────────────────────────────

/** Giros (en ejes de la articulación) que recorren todo el límite, para "probar rango" */
export function rangePath(limits: JointLimits, steps = 60): Quat[] {
  if (limits.kind === "hinge") {
    const lo = limits.min ?? -90;
    const hi = limits.max ?? 90;
    const seq = [0, hi, lo, 0];
    return pathThrough(seq, steps).map(hingeRotation);
  }
  const out: Quat[] = [];
  const border = limits.swing ? boundaryPoints(limits.swing, 32) : [];
  const toBorder = border.length > 0 ? border[0] : ([0, 0] as DiskPoint);
  for (let i = 0; i <= 8; i++) out.push(compose([(toBorder[0] * i) / 8, (toBorder[1] * i) / 8], 0));
  for (const p of [...border, border[0]].filter(Boolean)) out.push(compose(p, 0));
  for (let i = 8; i >= 0; i--) out.push(compose([(toBorder[0] * i) / 8, (toBorder[1] * i) / 8], 0));
  if (limits.twist) for (const t of pathThrough([0, limits.twist[1], limits.twist[0], 0], 30)) out.push(compose([0, 0], t));
  return out;
}

/** Valores que pasan por `seq` en `steps` pasos parejos */
function pathThrough(seq: number[], steps: number): number[] {
  const total = seq.slice(1).reduce((s, v, i) => s + Math.abs(v - seq[i]), 0) || 1;
  const out: number[] = [];
  for (let i = 0; i < seq.length - 1; i++) {
    const n = Math.max(1, Math.round((steps * Math.abs(seq[i + 1] - seq[i])) / total));
    for (let s = 0; s < n; s++) out.push(seq[i] + ((seq[i + 1] - seq[i]) * s) / n);
  }
  out.push(seq[seq.length - 1]);
  return out;
}

/** Límites por defecto al elegir el tipo a mano */
export function defaultLimits(kind: "hinge" | "ball"): JointLimits {
  return kind === "hinge" ? { kind: "hinge", min: -5, max: 130 } : ball(45, 45, 30);
}

