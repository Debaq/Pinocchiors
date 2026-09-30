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
