// Arrastrar una pieza del ensamble en el visor: gira la unión (revoluta o cilíndrica) que la
// sostiene. Todo es geometría pura: el visor solo da dos puntos del puntero sobre el plano
// perpendicular al eje de la unión.
import type { Assembly, Mate, P3 } from "./cad";

type Pose = [number, P3, P3];

const dot = (a: P3, b: P3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a: P3, b: P3): P3 => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const norm = (a: P3): P3 => {
  const l = Math.hypot(...a) || 1;
  return [a[0] / l, a[1] / l, a[2] / l];
};

/** Matriz de giro de un vector de giro (eje × ángulo en rad), por Rodrigues, por filas */
export function rotationMatrix(r: P3): [P3, P3, P3] {
  const t = Math.hypot(...r);
  if (t < 1e-12) return [[1, 0, 0], [0, 1, 0], [0, 0, 1]];
  const [x, y, z] = [r[0] / t, r[1] / t, r[2] / t];
  const [c, s] = [Math.cos(t), Math.sin(t)];
  const k = 1 - c;
  return [
    [c + x * x * k, x * y * k - z * s, x * z * k + y * s],
    [y * x * k + z * s, c + y * y * k, y * z * k - x * s],
    [z * x * k - y * s, z * y * k + x * s, c + z * z * k],
  ];
}

const apply = (m: [P3, P3, P3], v: P3): P3 => [dot(m[0], v), dot(m[1], v), dot(m[2], v)];

/** Eje de la unión en el mundo: un punto, la dirección y una X perpendicular (el conector de `a`) */
export function mateAxis(mate: Mate, poses: Pose[]): { origin: P3; z: P3; x: P3 } | undefined {
  const pose = poses.find((p) => p[0] === mate.a.instance);
  if (!pose) return undefined;
  const r = rotationMatrix(pose[2]);
  const o = apply(r, mate.a.origin);
  return {
    origin: [o[0] + pose[1][0], o[1] + pose[1][1], o[2] + pose[1][2]],
    z: norm(apply(r, norm(mate.a.z))),
    x: norm(apply(r, norm(mate.a.x))),
  };
}

/** Unión que mueve a la pieza agarrada, y hacia dónde cuenta el ángulo (+1: la pieza es `b`) */
export function jointOf(asm: Assembly, instance: number): { mate: Mate; sign: 1 | -1 } | undefined {
  const turns = (m: Mate) => m.kind === "revolute" || m.kind === "cylindrical";
  const own = asm.mates.filter((m) => turns(m) && (m.a.instance === instance || m.b.instance === instance));
  if (own.length === 0) return undefined;
  // Si hay varias, la que sostiene a la pieza contra una fija; si no, la primera
  const fixed = (id: number) => asm.instances.find((i) => i.id === id)?.fixed;
  const m = own.find((x) => fixed(x.a.instance === instance ? x.b.instance : x.a.instance)) ?? own[0];
  return { mate: m, sign: m.b.instance === instance ? 1 : -1 };
}

/** Una forma de mover la pieza agarrada: girar una unión (revoluta o cilíndrica) o deslizar por otra (deslizante o cilíndrica) */
export interface Joint {
  mate: Mate;
  /** +1 si la pieza es `b` de la unión */
  sign: 1 | -1;
  mode: "turn" | "slide";
}

/** Clave estable de una forma de mover (para guardar la que eligió la persona) */
export const jointKey = (j: Joint) => `${j.mate.id}:${j.mode}`;

/**
 * Todas las formas de mover una pieza arrastrándola, la de siempre primero (giros, la que la sostiene
 * contra una fija antes) y después los deslizamientos.
 */
export function jointsOf(asm: Assembly, instance: number): Joint[] {
  const out: Joint[] = [];
  const first = jointOf(asm, instance);
  const sign = (m: Mate): 1 | -1 => (m.b.instance === instance ? 1 : -1);
  if (first) out.push({ mate: first.mate, sign: first.sign, mode: "turn" });
  for (const m of asm.mates) {
    if ((m.a.instance !== instance && m.b.instance !== instance) || m === first?.mate) continue;
    if (m.kind === "revolute" || m.kind === "cylindrical") out.push({ mate: m, sign: sign(m), mode: "turn" });
  }
  for (const m of asm.mates) {
    if ((m.a.instance === instance || m.b.instance === instance) && (m.kind === "slider" || m.kind === "cylindrical")) out.push({ mate: m, sign: sign(m), mode: "slide" });
  }
  return out;
}

/** La forma de mover que se usa al agarrar la pieza: la elegida (`prefer`, una clave) o la primera */
export function dragJointOf(asm: Assembly, instance: number, prefer?: string): Joint | undefined {
  const all = jointsOf(asm, instance);
  return all.find((j) => jointKey(j) === prefer) ?? all[0];
}

/** Distancia (mm) con que está ahora la unión: de `b` respecto de `a` a lo largo del Z de `a` */
export function measuredDistance(mate: Mate, poses: Pose[]): number | undefined {
  const a = mateAxis(mate, poses);
  const pb = poses.find((p) => p[0] === mate.b.instance);
  if (!a || !pb) return undefined;
  const ob = apply(rotationMatrix(pb[2]), mate.b.origin);
  const w: P3 = [ob[0] + pb[1][0] - a.origin[0], ob[1] + pb[1][1] - a.origin[1], ob[2] + pb[1][2] - a.origin[2]];
  return dot(w, a.z);
}

/** Distancia nueva de la unión tras correr el puntero `delta` mm a lo largo del eje */
export const nextDistance = (start: number, delta: number, sign: 1 | -1) => start + sign * delta;

/** Plano perpendicular al eje, para leer el puntero (formato de `Plane` del visor) */
export const dragPlane = (axis: { origin: P3; z: P3; x: P3 }) => ({ origin: axis.origin, normal: axis.z, x_dir: axis.x });

/** Ángulo (°) del punto del plano `(x, y)` alrededor del origen del plano */
export const angleOf = (p: [number, number]) => (Math.atan2(p[1], p[0]) * 180) / Math.PI;

/** Cuánto giró el puntero entre `from` y `to` (°, entre −180 y 180), antihorario alrededor de Z */
export function turned(from: [number, number], to: [number, number]): number {
  let d = angleOf(to) - angleOf(from);
  while (d > 180) d -= 360;
  while (d < -180) d += 360;
  return d;
}

/** Ángulo nuevo de la unión tras girar el puntero `delta` grados desde que se agarró con `start` */
export const nextAngle = (start: number, delta: number, sign: 1 | -1) => start + sign * delta;

/** Ángulo (°) con que está ahora la unión: el giro de `b` respecto de `a` alrededor del Z de `a` */
export function measuredAngle(mate: Mate, poses: Pose[]): number | undefined {
  const a = mateAxis(mate, poses);
  const pb = poses.find((p) => p[0] === mate.b.instance);
  if (!a || !pb) return undefined;
  const xb = norm(apply(rotationMatrix(pb[2]), norm(mate.b.x)));
  const ya = cross(a.z, a.x);
  return (Math.atan2(dot(xb, ya), dot(xb, a.x)) * 180) / Math.PI;
}
