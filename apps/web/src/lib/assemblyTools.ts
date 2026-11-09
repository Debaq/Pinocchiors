// Ensamble (PLAN_ENSAMBLE): poses rígidas, conectores en el mundo, explosión por pasos y piezas
// movidas dentro de la malla armada (animación y explosión sin volver a teselar). Todo puro.
import { INLINE_BASE, type Assembly, type AssemblyRange, type CadMesh, type Connector, type ExplodeStep, type P3 } from "./cad.ts";
import { rotationMatrix } from "./assemblyDrag.ts";

/** Pose rígida: giro (por filas) y traslación, en mm con Z arriba */
export interface Pose3 {
  r: [P3, P3, P3];
  t: P3;
}

const dot = (a: P3, b: P3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a: P3, b: P3): P3 => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const unit = (a: P3): P3 => {
  const l = Math.hypot(...a) || 1;
  return [a[0] / l, a[1] / l, a[2] / l];
};

export const IDENTITY: Pose3 = { r: [[1, 0, 0], [0, 1, 0], [0, 0, 1]], t: [0, 0, 0] };

export function poseOf(position: P3, rotation: P3): Pose3 {
  return { r: rotationMatrix(rotation), t: [...position] as P3 };
}

export function turn(p: Pose3, v: P3): P3 {
  return [dot(p.r[0], v), dot(p.r[1], v), dot(p.r[2], v)];
}

export function applyPose(p: Pose3, v: P3): P3 {
  const w = turn(p, v);
  return [w[0] + p.t[0], w[1] + p.t[1], w[2] + p.t[2]];
}

export function compose(a: Pose3, b: Pose3): Pose3 {
  const col = (j: number): P3 => [b.r[0][j], b.r[1][j], b.r[2][j]];
  const r = [0, 1, 2].map((i) => [0, 1, 2].map((j) => dot(a.r[i], col(j))) as P3) as [P3, P3, P3];
  return { r, t: applyPose(a, b.t) };
}

export function inverse(p: Pose3): Pose3 {
  const r: [P3, P3, P3] = [
    [p.r[0][0], p.r[1][0], p.r[2][0]],
    [p.r[0][1], p.r[1][1], p.r[2][1]],
    [p.r[0][2], p.r[1][2], p.r[2][2]],
  ];
  const t = turn({ r, t: [0, 0, 0] }, p.t);
  return { r, t: [-t[0], -t[1], -t[2]] };
}

/** Conector en el mundo con la pose de su instancia: origen, Z, X (unitarios) */
export function connectorWorld(c: Connector, pose: Pose3): { origin: P3; z: P3; x: P3 } {
  return { origin: applyPose(pose, c.origin), z: unit(turn(pose, c.z)), x: unit(turn(pose, c.x)) };
}

/** Pose de cada instancia en la solución */
export function solutionPoses(poses: [number, P3, P3][]): Map<number, Pose3> {
  return new Map(poses.map(([id, p, r]) => [id, poseOf(p, r)]));
}

/** Corrimiento de cada instancia con los pasos de la explosión hasta `upto` (como `explode_offsets`) */
export function explodeOffsets(steps: ExplodeStep[], upto: number): Map<number, P3> {
  const out = new Map<number, P3>();
  for (let k = 0; k < steps.length; k++) {
    const t = Math.min(1, Math.max(0, upto - k));
    if (t <= 0) break;
    const d = unit(steps[k].dir);
    const v: P3 = [d[0] * steps[k].distance * t, d[1] * steps[k].distance * t, d[2] * steps[k].distance * t];
    for (const id of steps[k].instances) {
      const o = out.get(id) ?? [0, 0, 0];
      out.set(id, [o[0] + v[0], o[1] + v[1], o[2] + v[2]]);
    }
  }
  return out;
}

/**
 * Mueve cada instancia de la malla armada con su pose (en mm, Z arriba): `moves` por id de
 * instancia. La malla está en unidades del visor (Y arriba, `mmPerUnit` mm por unidad).
 */
export function moveAssemblyMesh(mesh: CadMesh, ranges: AssemblyRange[], moves: Map<number, Pose3>, mmPerUnit: number): CadMesh {
  if (moves.size === 0) return mesh;
  const s = mmPerUnit;
  // Visor → CAD: (x, y, z) → (x, −z, y)·s; CAD → visor: (x, y, z) → (x, z, −y)/s
  const toCad = (x: number, y: number, z: number): P3 => [x * s, -z * s, y * s];
  const range = ranges.map((r) => moves.get(r[0]));
  const faceRange = (face: number) => ranges.findIndex((r) => face >= r[1] && face < r[2]);
  const n = mesh.positions.length / 3;
  const vertexRange = new Int32Array(n).fill(-1);
  let lastFace = -1;
  let last = -1;
  for (let t = 0; t < mesh.triangleFace.length; t++) {
    const face = mesh.triangleFace[t];
    if (face !== lastFace) {
      lastFace = face;
      last = faceRange(face);
    }
    for (let k = 0; k < 3; k++) vertexRange[mesh.indices[t * 3 + k]] = last;
  }
  const positions = Float32Array.from(mesh.positions);
  const normals = Float32Array.from(mesh.normals);
  for (let v = 0; v < n; v++) {
    const i = vertexRange[v];
    const m = i >= 0 ? range[i] : undefined;
    if (!m) continue;
    const p = applyPose(m, toCad(positions[v * 3], positions[v * 3 + 1], positions[v * 3 + 2]));
    positions[v * 3] = p[0] / s;
    positions[v * 3 + 1] = p[2] / s;
    positions[v * 3 + 2] = -p[1] / s;
    const nn = turn(m, [normals[v * 3], -normals[v * 3 + 2], normals[v * 3 + 1]]);
    normals[v * 3] = nn[0];
    normals[v * 3 + 1] = nn[2];
    normals[v * 3 + 2] = -nn[1];
  }
  const edgePoints = Float32Array.from(mesh.edgePoints);
  ranges.forEach(([, , , e0, e1], i) => {
    const m = range[i];
    if (!m) return;
    const from = e0 > 0 ? mesh.edgeEnds[e0 - 1] : 0;
    const to = e1 > 0 ? mesh.edgeEnds[Math.min(e1, mesh.edgeEnds.length) - 1] : 0;
    for (let p = from; p < to; p++) {
      const q = applyPose(m, toCad(edgePoints[p * 3], edgePoints[p * 3 + 1], edgePoints[p * 3 + 2]));
      edgePoints[p * 3] = q[0] / s;
      edgePoints[p * 3 + 1] = q[2] / s;
      edgePoints[p * 3 + 2] = -q[1] / s;
    }
  });
  return { ...mesh, positions, normals, edgePoints };
}

/** Cuánto movió cada instancia de `from` a `to` (poses de dos soluciones): to ∘ from⁻¹ */
export function poseDeltas(from: [number, P3, P3][], to: [number, P3, P3][]): Map<number, Pose3> {
  const a = solutionPoses(from);
  const out = new Map<number, Pose3>();
  for (const [id, p, r] of to) {
    const f = a.get(id);
    if (f) out.set(id, compose(poseOf(p, r), inverse(f)));
  }
  return out;
}

/** Pose → vector de giro (eje × ángulo en rad), la inversa de `rotationMatrix` */
export function rotationVector(r: [P3, P3, P3]): P3 {
  const tr = r[0][0] + r[1][1] + r[2][2];
  const c = Math.min(1, Math.max(-1, (tr - 1) / 2));
  const th = Math.acos(c);
  const w: P3 = [r[2][1] - r[1][2], r[0][2] - r[2][0], r[1][0] - r[0][1]];
  if (th < 1e-9) return [w[0] / 2, w[1] / 2, w[2] / 2];
  if (Math.PI - th < 1e-6) {
    const cols: P3[] = [0, 1, 2].map((j) => [r[0][j] + (j === 0 ? 1 : 0), r[1][j] + (j === 1 ? 1 : 0), r[2][j] + (j === 2 ? 1 : 0)] as P3);
    const best = cols.reduce((a, b) => (Math.hypot(...b) > Math.hypot(...a) ? b : a));
    const u = unit(best);
    return [u[0] * th, u[1] * th, u[2] * th];
  }
  const k = th / (2 * Math.sin(th));
  return [w[0] * k, w[1] * k, w[2] * k];
}

/** Gira la instancia `deg` grados alrededor de un eje del mundo por su propio origen */
export function rotateInstance(position: P3, rotation: P3, axis: P3, deg: number): P3 {
  const a = unit(axis);
  const th = (deg * Math.PI) / 180;
  const q = poseOf([0, 0, 0], [a[0] * th, a[1] * th, a[2] * th]);
  const p = compose(q, poseOf(position, rotation));
  return rotationVector(p.r);
}

/** Copias de un patrón: los ids nuevos y su nombre */
export function patternCopies(asm: Assembly, source: number, count: number): { id: number; name: string }[] {
  const src = asm.instances.find((i) => i.id === source);
  let next = freshId(asm);
  const out: { id: number; name: string }[] = [];
  for (let k = 0; k < count; k++) out.push({ id: next++, name: `${src?.name ?? "#" + source} (${k + 2})` });
  return out;
}

/** Un id libre en el ensamble (instancias, uniones, grupos y patrones comparten numeración) */
export function freshId(asm: Assembly): number {
  // Los de los subensambles flexibles abiertos (≥ INLINE_BASE) no cuentan: no se guardan
  const own = (id: number) => (id < INLINE_BASE ? id + 1 : 1);
  return Math.max(
    Math.min(asm.next_id || 1, INLINE_BASE - 1),
    ...asm.instances.map((i) => own(i.id)),
    ...asm.mates.map((m) => own(m.id)),
    ...(asm.groups ?? []).map((g) => own(g.id)),
    ...(asm.patterns ?? []).map((p) => own(p.id)),
  );
}

/** Vector perpendicular a `z` (para un X por defecto) */
export function perpendicular(z: P3): P3 {
  const h: P3 = Math.abs(z[0]) < 0.9 ? [1, 0, 0] : [0, 1, 0];
  return unit(cross(cross(z, h), z));
}

/**
 * Cambia corrimiento, giro o volteo de un conector. Los ligados a su cara guardan el cambio (el
 * backend lo aplica al rehacerlo); los de antes (sin `at`) lo llevan puesto en origen, Z y X.
 */
export function adjustConnector(c: Connector, change: { offset?: number; spin?: number; reverse?: boolean }): Connector {
  if (c.at) return { ...c, ...change };
  let { origin, z, x } = c;
  z = unit(z);
  if (change.offset !== undefined) {
    const d = change.offset - (c.offset ?? 0);
    origin = [origin[0] + z[0] * d, origin[1] + z[1] * d, origin[2] + z[2] * d];
  }
  if (change.spin !== undefined) {
    const a = ((change.spin - (c.spin ?? 0)) * Math.PI) / 180;
    const y = cross(z, x);
    x = unit([x[0] * Math.cos(a) + y[0] * Math.sin(a), x[1] * Math.cos(a) + y[1] * Math.sin(a), x[2] * Math.cos(a) + y[2] * Math.sin(a)]);
  }
  if (change.reverse !== undefined && change.reverse !== !!c.reverse) z = [-z[0], -z[1], -z[2]];
  return { ...c, ...change, origin, z, x };
}
