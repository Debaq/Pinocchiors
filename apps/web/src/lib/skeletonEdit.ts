/**
 * Edición de la estructura del esqueleto: agregar y borrar huesos,
 * renombrar, y el formato JSON de `pinocchio_skeleton::load_skeleton_json`
 * (huesos con el nombre del padre), para guardar esqueletos propios y
 * llevarlos a otros modelos o a la biblioteca de Rust.
 *
 * Las funciones son puras: devuelven la lista nueva, que la app manda al
 * backend (`set_skeleton_bones`).
 */

import type { Vec3 } from "./animation";

export interface EditBone {
  name: string;
  position: Vec3;
  parent: number | null;
}

/** Nombre libre: `base`, `base_2`, `base_3`… */
export function uniqueName(bones: EditBone[], base: string): string {
  const used = new Set(bones.map((b) => b.name));
  if (!used.has(base)) return base;
  for (let k = 2; ; k++) if (!used.has(`${base}_${k}`)) return `${base}_${k}`;
}

/**
 * Hueso nuevo que cuelga de `parent`: sigue la dirección del hueso que llega
 * al padre (con la mitad de su largo) o, en la raíz, hacia arriba
 */
export function addChildBone(bones: EditBone[], parent: number, up: Vec3 = [0, 1, 0]): { bones: EditBone[]; index: number } {
  const p = bones[parent];
  const grand = p.parent !== null ? bones[p.parent] : null;
  let size = 0;
  for (const a of bones) for (const b of bones) size = Math.max(size, Math.hypot(a.position[0] - b.position[0], a.position[1] - b.position[1], a.position[2] - b.position[2]));
  let dir: Vec3 = up;
  let length = 0.1 * (size || 1);
  if (grand) {
    const d: Vec3 = [p.position[0] - grand.position[0], p.position[1] - grand.position[1], p.position[2] - grand.position[2]];
    const l = Math.hypot(...d);
    if (l > 1e-9) {
      dir = [d[0] / l, d[1] / l, d[2] / l];
      length = 0.5 * l;
    }
  }
  const bone: EditBone = {
    name: uniqueName(bones, `${p.name}_hijo`),
    position: [p.position[0] + dir[0] * length, p.position[1] + dir[1] * length, p.position[2] + dir[2] * length],
    parent,
  };
  return { bones: [...bones, bone], index: bones.length };
}

/**
 * Borra el hueso `index`: sus hijos pasan a colgar de su padre. Si era la
 * raíz, el primer hijo pasa a ser la raíz y los demás cuelgan de él.
 * Devuelve `null` si quedaría vacío.
 */
export function removeBone(bones: EditBone[], index: number): EditBone[] | null {
  if (bones.length <= 1) return null;
  const removed = bones[index];
  const children = bones.flatMap((b, i) => (b.parent === index ? [i] : []));
  const newRoot = removed.parent === null ? children[0] : undefined;
  const reparented = bones.map((b, i) => {
    if (b.parent !== index) return b;
    if (removed.parent !== null) return { ...b, parent: removed.parent };
    return i === newRoot ? { ...b, parent: null } : { ...b, parent: newRoot ?? null };
  });
  // Índices corridos después del borrado
  return reparented
    .filter((_, i) => i !== index)
    .map((b) => ({ ...b, parent: b.parent === null ? null : b.parent > index ? b.parent - 1 : b.parent }));
}

// ─── JSON (formato de `pinocchio_skeleton::json_loader`) ────────────────────

interface BoneJson {
  name: string;
  position: Vec3;
  parent?: string;
  is_leaf?: boolean;
}

interface SkeletonJson {
  name: string;
  bones: BoneJson[];
  metadata?: { description?: string; symmetric_pairs?: [string, string][] };
}

export function skeletonToJson(name: string, bones: EditBone[]): string {
  const json: SkeletonJson = {
    name,
    bones: bones.map((b, i) => ({
      name: b.name,
      position: b.position.map((x) => Math.round(x * 1e6) / 1e6) as Vec3,
      ...(b.parent !== null ? { parent: bones[b.parent].name } : {}),
      is_leaf: !bones.some((c) => c.parent === i),
    })),
    metadata: { description: "Esqueleto de Pinocchio" },
  };
  return JSON.stringify(json, null, 2);
}

/** Lee un esqueleto JSON; lanza un error legible si está mal formado */
export function skeletonFromJson(text: string): { name: string; bones: EditBone[] } {
  const raw = JSON.parse(text) as Partial<SkeletonJson>;
  if (!raw || !Array.isArray(raw.bones) || raw.bones.length === 0) throw new Error("no tiene huesos");
  const index = new Map(raw.bones.map((b, i) => [b?.name, i]));
  const bones = raw.bones.map((b): EditBone => {
    if (!b || typeof b.name !== "string" || !Array.isArray(b.position) || b.position.length !== 3 || !b.position.every(Number.isFinite)) {
      throw new Error(`hueso mal formado: ${JSON.stringify(b)?.slice(0, 60)}`);
    }
    let parent: number | null = null;
    if (b.parent !== undefined && b.parent !== null) {
      const p = index.get(b.parent);
      if (p === undefined) throw new Error(`el padre «${b.parent}» de «${b.name}» no existe`);
      parent = p;
    }
    return { name: b.name, position: [b.position[0], b.position[1], b.position[2]], parent };
  });
  return { name: typeof raw.name === "string" ? raw.name : "Esqueleto", bones };
}

// ─── Ajuste fino de articulaciones ──────────────────────────────────────────
//
// Mueven articulaciones sin cambiar la estructura: reciben y devuelven las
// posiciones de todas. Con espejo, el par del otro lado queda en el reflejo
// de cada articulación tocada.

export interface JointEditContext {
  parents: (number | null)[];
  /** Par izquierda ↔ derecha de cada articulación */
  mirror: (number | null)[];
  /** Plano de simetría: normal (hacia la derecha) y un punto */
  symmetry: { normal: Vec3; point: Vec3 };
}

const add = (a: Vec3, b: Vec3): Vec3 => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
const sub = (a: Vec3, b: Vec3): Vec3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const scale = (a: Vec3, s: number): Vec3 => [a[0] * s, a[1] * s, a[2] * s];
const dot = (a: Vec3, b: Vec3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const len = (a: Vec3) => Math.hypot(a[0], a[1], a[2]);

/** La articulación y todas las que cuelgan de ella */
export function subtree(parents: (number | null)[], j: number): number[] {
  const out = [j];
  for (let k = 0; k < out.length; k++) parents.forEach((p, i) => p === out[k] && out.push(i));
  return out;
}

/** −1 a la izquierda del plano, 1 a la derecha, 0 sobre él (en fracción de `size`) */
export function sideOf(p: Vec3, ctx: JointEditContext, size: number): number {
  const d = dot(sub(p, ctx.symmetry.point), ctx.symmetry.normal);
  return Math.abs(d) < 1e-3 * size ? 0 : Math.sign(d);
}

/**
 * Lleva al otro lado las articulaciones `touched`: cada par queda en el
 * reflejo. Las del medio no tienen par y no cambian.
 */
function mirrored(positions: Vec3[], ctx: JointEditContext, touched: number[]): Vec3[] {
  const out = positions.map((p) => [...p] as Vec3);
  const set = new Set(touched);
  const { normal, point } = ctx.symmetry;
  for (const j of touched) {
    const m = ctx.mirror[j];
    // Si los dos lados se tocaron (giro de algo del medio), no se pisan
    if (m === null || m === undefined || set.has(m)) continue;
    const d = dot(sub(out[j], point), normal);
    out[m] = sub(out[j], scale(normal, 2 * d));
  }
  return out;
}

/** Desplaza la articulación `j` (y lo que cuelga de ella si `withChildren`) */
export function moveJoints(positions: Vec3[], ctx: JointEditContext, j: number, delta: Vec3, withChildren: boolean, mirror: boolean): Vec3[] {
  const touched = withChildren ? subtree(ctx.parents, j) : [j];
  const out = positions.map((p, i) => (touched.includes(i) ? add(p, delta) : ([...p] as Vec3)));
  return mirror ? mirrored(out, ctx, touched) : out;
}

/** Giro de `v` alrededor del eje unitario `axis` (Rodrigues) */
function rotate(v: Vec3, axis: Vec3, angle: number): Vec3 {
  const c = Math.cos(angle);
  const s = Math.sin(angle);
  const cross: Vec3 = [axis[1] * v[2] - axis[2] * v[1], axis[2] * v[0] - axis[0] * v[2], axis[0] * v[1] - axis[1] * v[0]];
  return add(add(scale(v, c), scale(cross, s)), scale(axis, dot(axis, v) * (1 - c)));
}

/** Gira lo que cuelga de `j` alrededor de `j` (el miembro entero se mueve como un bloque) */
export function rotateLimb(positions: Vec3[], ctx: JointEditContext, j: number, axis: Vec3, angle: number, mirror: boolean): Vec3[] {
  const pivot = positions[j];
  const moved = subtree(ctx.parents, j).slice(1);
  const out = positions.map((p, i) => (moved.includes(i) ? add(pivot, rotate(sub(p, pivot), axis, angle)) : ([...p] as Vec3)));
  return mirror ? mirrored(out, ctx, moved) : out;
}

/**
 * Alarga (o acorta, con `amount` negativo) el hueso que llega a `j`: la
 * articulación se aleja de su padre en la misma dirección y lo que cuelga
 * de ella la sigue. Nunca deja el hueso más corto que un 5 % de su largo
 */
export function stretchBone(positions: Vec3[], ctx: JointEditContext, j: number, amount: number, mirror: boolean): Vec3[] {
  const parent = ctx.parents[j];
  if (parent === null || parent === undefined) return positions.map((p) => [...p] as Vec3);
  const bone = sub(positions[j], positions[parent]);
  const length = len(bone);
  if (length < 1e-9) return positions.map((p) => [...p] as Vec3);
  const next = Math.max(0.05 * length, length + amount);
  return moveJoints(positions, ctx, j, scale(bone, next / length - 1), true, mirror);
}

/**
 * Copia el lado de `j` al otro: cada articulación de ese lado con par deja
 * a su par en el reflejo (para corregir una mitad y emparejar la otra)
 */
export function copySide(positions: Vec3[], ctx: JointEditContext, j: number, size: number): Vec3[] {
  const side = sideOf(positions[j], ctx, size);
  if (side === 0) return positions.map((p) => [...p] as Vec3);
  const touched = positions.flatMap((p, i) => (ctx.mirror[i] != null && sideOf(p, ctx, size) === side ? [i] : []));
  return mirrored(positions, ctx, touched);
}
