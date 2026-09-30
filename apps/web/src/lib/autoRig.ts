/**
 * Rig automático de controles a partir del análisis del cuerpo:
 * - patas, brazos y la mitad superior de las alas → IK de dos huesos con
 *   pole (las patas con pie invertido y suelo);
 * - cola, trompa y tentáculos → IK por curva;
 * - cuello y cabeza → mirar a;
 * - raíz → control de centro de masa.
 *
 * Los controles y cadenas generados llevan `auto`: rehacer el rig los
 * reemplaza sin tocar los que hizo el usuario.
 */

import type { Vec3 } from "./animation";
import type { Chain } from "./presetAnimations";
import { bodyAxes, newRigId, vec, type IkChain, type RigContext, type RigControl, type RigSettings } from "./rig";

const KNEE = /knee|shin|tibia|calf|elbow|forearm|lower|radius|tibio/i;

/** Índice en la cadena de la rodilla o el codo: por nombre, o la segunda articulación */
function bendIndex(ctx: RigContext, chain: Chain): number {
  const i = chain.joints.findIndex((j, n) => n > 0 && n < chain.joints.length - 1 && KNEE.test(ctx.bones[j].name));
  return i > 0 ? i : 1;
}

/**
 * Dónde va el pole: hacia donde ya dobla la rodilla en reposo (o, si la pata
 * es recta, `fallback`), a una distancia parecida al largo de la pata
 */
function polePosition(ctx: RigContext, a: number, b: number, c: number, fallback: Vec3): Vec3 {
  const A = ctx.bones[a].position;
  const B = ctx.bones[b].position;
  const C = ctx.bones[c].position;
  const mid: Vec3 = [(A[0] + C[0]) / 2, (A[1] + C[1]) / 2, (A[2] + C[2]) / 2];
  const axis = vec.unit(vec.sub(C, A), [0, -1, 0]);
  let out = vec.sub(B, mid);
  out = vec.sub(out, vec.scale(axis, vec.dot(out, axis)));
  const length = vec.length(vec.sub(B, A)) + vec.length(vec.sub(C, B));
  if (vec.length(out) < 0.02 * length) {
    out = vec.sub(fallback, vec.scale(axis, vec.dot(fallback, axis)));
  }
  const dir = vec.unit(out, fallback);
  return [B[0] + dir[0] * length * 0.6, B[1] + dir[1] * length * 0.6, B[2] + dir[2] * length * 0.6];
}

const name = (ctx: RigContext, j: number) => ctx.bones[j].name;

/** Rehace los controles y cadenas automáticos del rig */
export function autoRig(ctx: RigContext): RigSettings {
  const body = ctx.body;
  const settings = ctx.settings;
  const controls: RigControl[] = settings.controls.filter((c) => !c.auto);
  const chains: IkChain[] = (settings.ikChains ?? []).filter((c) => !c.auto);
  if (!body) return { ...settings, controls, ikChains: chains };
  // Rehacer conserva los ids de lo que se vuelve a generar igual (por nombre),
  // así las keys de controles y cadenas siguen valiendo
  const oldControls = new Map(settings.controls.filter((c) => c.auto).map((c) => [c.name, c.id]));
  const oldChains = new Map((settings.ikChains ?? []).filter((c) => c.auto).map((c) => [`${c.solver}:${c.name}`, c.id]));
  const reuse = (ids: Map<string, string>, key: string) => {
    const id = ids.get(key);
    ids.delete(key);
    return id;
  };
  const chainId = (solver: IkChain["solver"], label: string) => reuse(oldChains, `${solver}:${label}`) ?? newRigId("ik");
  const { forward, up } = bodyAxes(body);
  const size = body.size;
  const control = (label: string, position: Vec3, shape: RigControl["shape"], scale: number, parent: string | null = null): RigControl => {
    const c: RigControl = { id: reuse(oldControls, label) ?? newRigId("ctl"), name: label, parent, shape, position: [...position], size: size * scale, auto: true };
    controls.push(c);
    return c;
  };

  // Patas, brazos y alas: dos huesos + pole
  for (const chain of body.chains) {
    const limb = chain.kind === "leg" || chain.kind === "arm" || chain.kind === "wing";
    if (!limb || chain.joints.length < 3) continue;
    const k = bendIndex(ctx, chain);
    if (k + 1 >= chain.joints.length) continue;
    const [a, b, c] = [chain.joints[k - 1], chain.joints[k], chain.joints[k + 1]];
    const foot = chain.joints.slice(k + 2);
    const leg = chain.kind === "leg";
    // Las rodillas doblan hacia adelante (o arriba si la pata es abierta), los codos hacia atrás, las alas hacia atrás
    const fallback: Vec3 = leg ? (chain.sprawl ? up : forward) : vec.scale(forward, -1);
    const label = name(ctx, c);
    const target = control(`${label}_ik`, ctx.bones[c].position, "cube", 0.035);
    const pole = control(`${name(ctx, b)}_pole`, polePosition(ctx, a, b, c, fallback), "sphere", 0.015);
    chains.push({
      id: chainId("twoBone", `${label} IK`),
      name: `${label} IK`,
      solver: "twoBone",
      joints: [a, b, c, ...(leg ? foot : [])].map((j) => name(ctx, j)),
      target: target.id,
      pole: pole.id,
      footRoll: leg && foot.length > 0,
      ground: leg,
      alignEffector: !leg,
      auto: true,
    });
  }

  // Cola, trompa, tentáculos: curva con controles que siguen al cuerpo
  for (const chain of body.chains) {
    if (!["tail", "trunk", "tentacle"].includes(chain.kind) || chain.joints.length < 3) continue;
    const base = ctx.bones[chain.joints[0]].parent;
    const parent = base === null ? null : name(ctx, base);
    const count = Math.min(4, chain.joints.length - 1);
    const curve = Array.from({ length: count }, (_, i) => {
      const j = chain.joints[Math.round(((i + 1) * (chain.joints.length - 1)) / count)];
      return control(`${name(ctx, j)}_curva`, ctx.bones[j].position, "sphere", 0.02, parent);
    });
    chains.push({
      id: chainId("spline", `${name(ctx, chain.joints[chain.joints.length - 1])} curva`),
      name: `${name(ctx, chain.joints[chain.joints.length - 1])} curva`,
      solver: "spline",
      joints: chain.joints.map((j) => name(ctx, j)),
      curve: curve.map((c) => c.id),
      auto: true,
    });
  }

  // Cabeza: mirar a, con el giro repartido del cuello a la cabeza (más en la cabeza)
  const head = [...body.neck, ...body.chains.filter((c) => c.kind === "head").flatMap((c) => c.rotating)].filter(
    (j, i, all) => all.indexOf(j) === i && ctx.children[j].length > 0
  );
  if (head.length > 0) {
    const last = head[head.length - 1];
    const p = ctx.bones[last].position;
    const target = control("mirar", [p[0] + forward[0] * size * 0.5, p[1] + forward[1] * size * 0.5, p[2] + forward[2] * size * 0.5], "circle", 0.03);
    chains.push({
      id: chainId("lookAt", "Mirar a"),
      name: "Mirar a",
      solver: "lookAt",
      joints: head.map((j) => name(ctx, j)),
      target: target.id,
      weights: head.map((_, i) => i + 2),
      auto: true,
    });
  }

  // Raíz: control de centro de masa
  const root = ctx.bones[body.root];
  const com = control("centro de masa", root.position, "circle", 0.12);
  chains.push({ id: chainId("root", "Centro de masa"), name: "Centro de masa", solver: "root", joints: [root.name], target: com.id, auto: true });

  return { ...settings, controls, ikChains: chains };
}
