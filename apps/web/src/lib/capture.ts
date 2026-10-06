/**
 * Captura de movimiento (F6): de los puntos de MediaPipe Pose (33 por
 * cuadro, en metros, con el origen entre las caderas) a un movimiento de
 * origen para `retarget.ts`.
 *
 * - Se arma un esqueleto con nombres de Mixamo (Hips, Spine, LeftArm…), así
 *   el mapeo automático del retargeting lo reconoce.
 * - Limpieza: puntos con poca visibilidad se rellenan interpolando entre
 *   cuadros buenos y todo pasa por un filtro 1€ (quita el temblor sin
 *   retardo notable).
 * - La raíz se desplaza con la posición de la cadera en la imagen, escalada
 *   a metros con la altura del actor.
 * - Contactos: cuadros en que cada pie está quieto y abajo; la app los pasa
 *   a keys de fijado del IK, que es lo que quita el patinaje.
 *
 * La detección misma (MediaPipe) está en `captureRunner.ts`.
 */

import type { Quat, Vec3 } from "./animation";
import { rotationBetweenFrames } from "./ik";
import type { SkeletonBone } from "./presetAnimations";
import type { SourceMotion } from "./retarget";

export interface CapturePoint {
  x: number;
  y: number;
  z: number;
  visibility?: number;
}

/** Un cuadro detectado: puntos en metros y en la imagen (0–1), o `null` sin persona */
export interface CaptureFrame {
  world: CapturePoint[];
  image: CapturePoint[];
}

/** Articulaciones del actor: nombre (estilo Mixamo), padre y puntos de MediaPipe que promedia */
const JOINTS: { name: string; parent: string | null; from: number[] }[] = [
  { name: "Hips", parent: null, from: [23, 24] },
  { name: "Spine", parent: "Hips", from: [23, 24, 11, 12] },
  { name: "Neck", parent: "Spine", from: [11, 12] },
  { name: "Head", parent: "Neck", from: [7, 8] },
  { name: "HeadTop_End", parent: "Head", from: [0] },
  { name: "LeftArm", parent: "Neck", from: [11] },
  { name: "LeftForeArm", parent: "LeftArm", from: [13] },
  { name: "LeftHand", parent: "LeftForeArm", from: [15] },
  { name: "LeftHand_end", parent: "LeftHand", from: [17, 19] },
  { name: "RightArm", parent: "Neck", from: [12] },
  { name: "RightForeArm", parent: "RightArm", from: [14] },
  { name: "RightHand", parent: "RightForeArm", from: [16] },
  { name: "RightHand_end", parent: "RightHand", from: [18, 20] },
  { name: "LeftUpLeg", parent: "Hips", from: [23] },
  { name: "LeftLeg", parent: "LeftUpLeg", from: [25] },
  { name: "LeftFoot", parent: "LeftLeg", from: [27] },
  { name: "LeftToeBase", parent: "LeftFoot", from: [31] },
  { name: "RightUpLeg", parent: "Hips", from: [24] },
  { name: "RightLeg", parent: "RightUpLeg", from: [26] },
  { name: "RightFoot", parent: "RightLeg", from: [28] },
  { name: "RightToeBase", parent: "RightFoot", from: [32] },
];

/** Filtro 1€ (Casiez et al. 2012): suaviza fuerte lo lento y deja pasar lo rápido */
export class OneEuro {
  private x: number | null = null;
  private dx = 0;
  constructor(
    private minCutoff: number,
    private beta: number,
    private dCutoff = 1
  ) {}
  private alpha(cutoff: number, dt: number) {
    const tau = 1 / (2 * Math.PI * cutoff);
    return 1 / (1 + tau / dt);
  }
  filter(value: number, dt: number): number {
    if (this.x === null) {
      this.x = value;
      return value;
    }
    const d = (value - this.x) / dt;
    this.dx += this.alpha(this.dCutoff, dt) * (d - this.dx);
    const cutoff = this.minCutoff + this.beta * Math.abs(this.dx);
    this.x += this.alpha(cutoff, dt) * (value - this.x);
    return this.x;
  }
}

export interface CaptureOptions {
  /** 0 (crudo) a 1 (muy suave) */
  smoothing: number;
  /** Visibilidad mínima de un punto para usarlo (0–1) */
  minVisibility?: number;
}

export interface CaptureMotion extends SourceMotion {
  /** Cuadros en que cada pie (índice de articulación de origen) está apoyado */
  contacts: Map<number, boolean[]>;
}

/**
 * Movimiento de origen a partir de los cuadros detectados. Los cuadros sin
 * persona o con puntos poco visibles se rellenan con los vecinos.
 */
export function captureMotion(frames: (CaptureFrame | null)[], fps: number, options: CaptureOptions): CaptureMotion | null {
  const minVisibility = options.minVisibility ?? 0.5;
  const n = frames.length;
  const P = 33;
  // Pasa a Y arriba (MediaPipe usa Y abajo y Z hacia la cámara con signo inverso)
  const world: (Vec3 | null)[][] = frames.map((f) =>
    Array.from({ length: P }, (_, k) => {
      const p = f?.world[k];
      if (!p || (p.visibility ?? 1) < minVisibility) return null;
      return [p.x, -p.y, -p.z] as Vec3;
    })
  );
  const image: ([number, number] | null)[] = frames.map((f) => {
    const a = f?.image[23];
    const b = f?.image[24];
    return a && b ? [(a.x + b.x) / 2, (a.y + b.y) / 2] : null;
  });
  // Rellena huecos de cada punto interpolando entre los cuadros buenos. Una
  // punta que nunca se ve (dedos, extremo de la mano) toma el punto de su
  // articulación; sin cadera, hombros o miembros no hay captura
  const used = USED;
  for (const k of used) {
    const good = world.flatMap((f, i) => (f[k] ? [i] : []));
    if (good.length > 0) {
      fillMissing(world, k, good);
      continue;
    }
    const fallback = TIP_FALLBACK[k];
    if (fallback === undefined) return null;
    const fallbackGood = world.flatMap((f, i) => (f[fallback] ? [i] : []));
    if (fallbackGood.length === 0) return null;
    fillMissing(world, fallback, fallbackGood);
    for (const f of world) f[k] = [...f[fallback]!] as Vec3;
  }
  // Filtro 1€ por coordenada: más suavizado, corte más bajo
  const { minCutoff, beta } = filterParams(options.smoothing);
  const dt = 1 / (fps > 0 ? fps : 30);
  for (const k of used) {
    const filters = [0, 1, 2].map(() => new OneEuro(minCutoff, beta));
    for (let i = 0; i < n; i++) {
      const p = world[i][k]!;
      world[i][k] = [filters[0].filter(p[0], dt), filters[1].filter(p[1], dt), filters[2].filter(p[2], dt)];
    }
  }

  const index = new Map(JOINTS.map((j, i) => [j.name, i]));
  const mean = (f: (Vec3 | null)[], from: number[]): Vec3 => {
    const sum: Vec3 = [0, 0, 0];
    for (const k of from) for (let a = 0; a < 3; a++) sum[a] += f[k]![a] / from.length;
    return sum;
  };
  // Metros por unidad de imagen: altura del actor contra su altura en la imagen
  const height = (f: (Vec3 | null)[]) => {
    const feet = mean(f, [27, 28]);
    return Math.hypot(...(mean(f, [0]).map((v, a) => v - feet[a]) as Vec3));
  };
  const firstImage = image.findIndex(Boolean);
  const imageHeight = (i: number) => {
    const f = frames[i];
    if (!f) return 0;
    const top = f.image[0];
    const feet = [f.image[27], f.image[28]];
    if (!top || !feet[0] || !feet[1]) return 0;
    return Math.abs((feet[0].y + feet[1].y) / 2 - top.y);
  };
  const metersPerImage = firstImage >= 0 && imageHeight(firstImage) > 1e-3 ? height(world[firstImage]) / imageHeight(firstImage) : 0;

  const positions: Vec3[][] = [];
  const rootRotation: Quat[] = [];
  let rest: { right: Vec3; up: Vec3 } | null = null;
  let hip0: [number, number] | null = null;
  for (let i = 0; i < n; i++) {
    const f = world[i];
    const joints = JOINTS.map((j) => mean(f, j.from));
    // Desplazamiento de la cadera en la imagen (en metros)
    const hip = image[i];
    if (hip && metersPerImage > 0) {
      hip0 ??= hip;
      const offset: Vec3 = [(hip[0] - hip0[0]) * metersPerImage, -(hip[1] - hip0[1]) * metersPerImage, 0];
      for (const p of joints) for (let a = 0; a < 3; a++) p[a] += offset[a];
    }
    positions.push(joints);
    // Giro del torso respecto del primer cuadro: de cadera a cadera y de las caderas a los hombros
    const right = sub(mean(f, [24]), mean(f, [23]));
    const up = sub(mean(f, [11, 12]), mean(f, [23, 24]));
    rest ??= { right, up };
    rootRotation.push(rotationBetweenFrames(rest.right, rest.up, right, up));
  }

  // Esqueleto en reposo: el primer cuadro sin giro del torso
  const bones = JOINTS.map((j, i) => ({ name: j.name, parent: j.parent === null ? null : index.get(j.parent)!, position: positions[0][i] }));

  // Contactos: pie quieto (menos de 0.3 m/s) y a menos de 4 cm del más bajo del clip
  const contacts = new Map<number, boolean[]>();
  for (const foot of ["LeftFoot", "RightFoot"]) {
    const j = index.get(foot)!;
    const ys = positions.map((p) => p[j][1]);
    const floor = Math.min(...ys);
    contacts.set(
      j,
      positions.map((p, i) => {
        const a = Math.max(0, i - 1);
        const b = Math.min(n - 1, i + 1);
        const [prev, next] = [positions[a][j], positions[b][j]];
        const speed = b > a ? Math.hypot(next[0] - prev[0], next[1] - prev[1], next[2] - prev[2]) / ((b - a) * dt) : 0;
        return speed < 0.3 && p[j][1] - floor < 0.04;
      })
    );
  }
  return { bones, fps, positions, rootRotation, contacts };
}

const sub = (a: Vec3, b: Vec3): Vec3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];

/** Pares izquierda/derecha de MediaPipe Pose (la nariz, 0, queda sola) */
const MIRROR = Array.from({ length: 33 }, (_, k) => (k === 0 ? 0 : k <= 3 ? k + 3 : k <= 6 ? k - 3 : k % 2 ? k + 1 : k - 1));

/** El cuadro visto en un espejo: el lado izquierdo del actor mueve el derecho del modelo */
export function mirrorFrame(frame: CaptureFrame | null): CaptureFrame | null {
  if (!frame) return null;
  const flip = (points: CapturePoint[], x: (v: number) => number) => points.map((_, k) => ({ ...points[MIRROR[k]], x: x(points[MIRROR[k]].x) }));
  return { world: flip(frame.world, (v) => -v), image: flip(frame.image, (v) => 1 - v) };
}

/** Parámetros del filtro 1€ para un suavizado de 0 (crudo) a 1 (muy suave) */
function filterParams(smoothing: number) {
  const s = Math.max(0, Math.min(1, smoothing));
  return { minCutoff: 4 * (1 - s) + 0.3 * s, beta: 0.6 * (1 - s) + 0.05 * s };
}

/** Puntos de MediaPipe que usan las articulaciones */
const USED = [...new Set(JOINTS.flatMap((j) => j.from))].sort((a, b) => a - b);
/** Punta que no se ve → el punto de su articulación */
const TIP_FALLBACK: Record<number, number> = { 0: 7, 17: 15, 19: 15, 18: 16, 20: 16, 31: 27, 32: 28 };

/**
 * Captura en vivo: el mismo esqueleto del actor que `captureMotion`, cuadro
 * a cuadro, para mover el modelo mientras la persona se mueve. El reposo es
 * el primer cuadro con el cuerpo entero a la vista (`recalibrate` lo
 * vuelve a tomar). Lo que deja de verse un momento queda donde estaba.
 */
export class LiveSolver {
  /** Esqueleto del actor en reposo (null hasta verlo entero) */
  bones: SkeletonBone[] | null = null;
  private filters = new Map<number, OneEuro[]>();
  private last = new Map<number, Vec3>();
  private rest: { right: Vec3; up: Vec3 } | null = null;
  private hip0: [number, number] | null = null;
  private metersPerImage = 0;

  constructor(
    private smoothing: number,
    private minVisibility = 0.5
  ) {}

  setSmoothing(smoothing: number): void {
    this.smoothing = smoothing;
    this.filters.clear();
  }

  recalibrate(): void {
    this.bones = null;
    this.rest = null;
    this.hip0 = null;
    this.filters.clear();
    this.last.clear();
  }

  /** Posiciones de las articulaciones del actor y giro del torso, o null si todavía no hay reposo */
  push(frame: CaptureFrame | null, dt: number): { positions: Vec3[]; rootRotation: Quat } | null {
    if (!frame) return null;
    const { minCutoff, beta } = filterParams(this.smoothing);
    const points = new Map<number, Vec3>();
    for (const k of USED) {
      const p = frame.world[k];
      if (p && (p.visibility ?? 1) >= this.minVisibility) {
        let filters = this.filters.get(k);
        if (!filters) this.filters.set(k, (filters = [0, 1, 2].map(() => new OneEuro(minCutoff, beta))));
        const v: Vec3 = [filters[0].filter(p.x, dt), filters[1].filter(-p.y, dt), filters[2].filter(-p.z, dt)];
        this.last.set(k, v);
      }
      const v = this.last.get(k);
      if (v) points.set(k, v);
    }
    for (const k of USED) {
      if (points.has(k)) continue;
      const fallback = points.get(TIP_FALLBACK[k]);
      if (!fallback) return null;
      points.set(k, fallback);
    }
    const mean = (from: number[]): Vec3 => {
      const sum: Vec3 = [0, 0, 0];
      for (const k of from) for (let a = 0; a < 3; a++) sum[a] += points.get(k)![a] / from.length;
      return sum;
    };
    const joints = JOINTS.map((j) => mean(j.from));
    const right = sub(mean([24]), mean([23]));
    const up = sub(mean([11, 12]), mean([23, 24]));
    const image = frame.image;
    if (!this.bones) {
      const index = new Map(JOINTS.map((j, i) => [j.name, i]));
      this.bones = JOINTS.map((j, i) => ({ name: j.name, parent: j.parent === null ? null : index.get(j.parent)!, position: joints[i] }));
      this.rest = { right, up };
      // Metros por unidad de imagen: altura del actor contra su altura en la imagen
      const feet = mean([27, 28]);
      const height = Math.hypot(...sub(mean([0]), feet));
      const imageHeight = image[0] && image[27] && image[28] ? Math.abs((image[27].y + image[28].y) / 2 - image[0].y) : 0;
      this.metersPerImage = imageHeight > 1e-3 ? height / imageHeight : 0;
      this.hip0 = null;
    }
    // Desplazamiento de la cadera en la imagen (en metros)
    if (image[23] && image[24] && this.metersPerImage > 0) {
      const hip: [number, number] = [(image[23].x + image[24].x) / 2, (image[23].y + image[24].y) / 2];
      this.hip0 ??= hip;
      const offset: Vec3 = [(hip[0] - this.hip0[0]) * this.metersPerImage, -(hip[1] - this.hip0[1]) * this.metersPerImage, 0];
      for (const p of joints) for (let a = 0; a < 3; a++) p[a] += offset[a];
    }
    return { positions: joints, rootRotation: rotationBetweenFrames(this.rest!.right, this.rest!.up, right, up) };
  }
}

/** Completa el punto `k` en los cuadros que no lo tienen, con los cuadros buenos `good` */
function fillMissing(world: (Vec3 | null)[][], k: number, good: number[]): void {
  const n = world.length;
  for (let i = 0; i < n; i++) {
    if (world[i][k]) continue;
    const after = good.find((g) => g > i);
    const before = [...good].reverse().find((g) => g < i);
    if (before === undefined) world[i][k] = [...world[after!][k]!] as Vec3;
    else if (after === undefined) world[i][k] = [...world[before][k]!] as Vec3;
    else {
      const t = (i - before) / (after - before);
      const a = world[before][k]!;
      const b = world[after][k]!;
      world[i][k] = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
    }
  }
}

