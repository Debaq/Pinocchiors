/**
 * Ragdoll (F8): el cuerpo cae como un muñeco desde la pose actual y el
 * resultado se hornea a un clip FK normal.
 *
 * Partículas en las articulaciones (Verlet) con los largos de los huesos
 * fijos, refuerzos en las ramificaciones (hombros y caderas no se cierran
 * sobre la columna), gravedad y suelo con fricción. Arranca con la
 * velocidad que traía la animación. Las posiciones se pasan a giros con el
 * retargeting sobre el mismo esqueleto; al reproducir, los límites de giro
 * de cada articulación recortan lo imposible.
 */

import type { AnimationClip, Pose, Vec3 } from "./animation";
import { Fk, rotationBetweenFrames } from "./ik";
import type { RigContext } from "./rig";
import { retargetClip, type SourceMotion } from "./retarget";

export interface RagdollOptions {
  /** Segundos de caída */
  seconds: number;
  fps: number;
  /** 0–1: cuánto se frena al rozar el suelo */
  friction?: number;
  name?: string;
}

type V = [number, number, number];
const sub = (a: V, b: V): V => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const len = (a: V) => Math.hypot(a[0], a[1], a[2]);

/**
 * Simula la caída desde `pose` (con `previous`, la pose un cuadro antes,
 * para la velocidad inicial) y devuelve el clip horneado
 */
export function ragdollClip(ctx: RigContext, pose: Pose, previous: Pose | null, options: RagdollOptions): AnimationClip {
  const bones = ctx.bones;
  const n = bones.length;
  const size = ctx.body?.size ?? 1;
  const up = (ctx.body?.up ?? [0, 1, 0]) as V;
  const fps = options.fps > 0 ? options.fps : 24;
  const dt = 1 / fps;
  const SUBSTEPS = 4;
  const h = dt / SUBSTEPS;
  const friction = Math.max(0, Math.min(1, options.friction ?? 0.6));

  const positions = (p: Pose) => {
    const fk = new Fk(bones, p);
    return bones.map((_, j) => fk.position(j) as V);
  };
  let x = positions(pose);
  // Velocidad inicial: la que traía la animación (por subpaso)
  let old = previous ? positions(previous).map((p, j) => {
    const v = sub(x[j], p);
    return [x[j][0] - v[0] / SUBSTEPS, x[j][1] - v[1] / SUBSTEPS, x[j][2] - v[2] / SUBSTEPS] as V;
  }) : x.map((p) => [...p] as V);

  // Restricciones de distancia: huesos y refuerzos en cada ramificación
  const links: [number, number, number][] = [];
  const link = (a: number, b: number) => {
    if (a === b || links.some(([p, q]) => (p === a && q === b) || (p === b && q === a))) return;
    links.push([a, b, len(sub(x[a], x[b]))]);
  };
  bones.forEach((b, j) => b.parent !== null && link(j, b.parent));
  bones.forEach((b, j) => {
    const kids = ctx.children[j];
    if (kids.length < 2) return;
    for (let a = 0; a < kids.length; a++) {
      for (let c = a + 1; c < kids.length; c++) link(kids[a], kids[c]);
      if (b.parent !== null) link(kids[a], b.parent);
    }
  });

  // Suelo: el punto más bajo del esqueleto en reposo
  const floor = Math.min(...bones.map((b) => b.position[0] * up[0] + b.position[1] * up[1] + b.position[2] * up[2]));
  const height = (p: V) => p[0] * up[0] + p[1] * up[1] + p[2] * up[2];
  const g = 9.8 * (size / 1.7);

  const frames = Math.max(1, Math.round(options.seconds * fps));
  const track: V[][] = [x.map((p) => [...p] as V)];
  for (let f = 1; f <= frames; f++) {
    for (let s = 0; s < SUBSTEPS; s++) {
      const next = x.map((p, j) => {
        const v = sub(p, old[j]);
        const damp = 0.995;
        return [p[0] + v[0] * damp - up[0] * g * h * h, p[1] + v[1] * damp - up[1] * g * h * h, p[2] + v[2] * damp - up[2] * g * h * h] as V;
      });
      old = x;
      x = next;
      for (let it = 0; it < 12; it++) {
        for (const [a, b, rest] of links) {
          const d = sub(x[b], x[a]);
          const l = len(d) || 1e-9;
          const k = (0.5 * (l - rest)) / l;
          x[a] = [x[a][0] + d[0] * k, x[a][1] + d[1] * k, x[a][2] + d[2] * k];
          x[b] = [x[b][0] - d[0] * k, x[b][1] - d[1] * k, x[b][2] - d[2] * k];
        }
        // Suelo con fricción: lo que toca no se desliza del todo
        for (let j = 0; j < n; j++) {
          const below = floor - height(x[j]);
          if (below <= 0) continue;
          x[j] = [x[j][0] + up[0] * below, x[j][1] + up[1] * below, x[j][2] + up[2] * below];
          const slide = sub(x[j], old[j]);
          const along = slide[0] * up[0] + slide[1] * up[1] + slide[2] * up[2];
          const tangent: V = [slide[0] - up[0] * along, slide[1] - up[1] * along, slide[2] - up[2] * along];
          old[j] = [old[j][0] + tangent[0] * friction, old[j][1] + tangent[1] * friction, old[j][2] + tangent[2] * friction];
        }
      }
    }
    track.push(x.map((p) => [...p] as V));
  }

  // Giro de la raíz: el marco que forman dos de sus ramas
  const root = ctx.body?.root ?? bones.findIndex((b) => b.parent === null);
  const kids = ctx.children[root];
  const axes = (p: V[]): [Vec3, Vec3] => {
    const a = kids[0] !== undefined ? sub(p[kids[0]], p[root]) : ([0, 1, 0] as V);
    const b = kids[1] !== undefined ? sub(p[kids[1]], p[root]) : ([1, 0, 0] as V);
    return [a, b];
  };
  const restAxes = axes(bones.map((b) => b.position as V));
  const motion: SourceMotion = {
    bones,
    fps,
    positions: track,
    rootRotation: track.map((p) => {
      const [a, b] = axes(p);
      return rotationBetweenFrames(restAxes[0], restAxes[1], a, b);
    }),
  };
  const identity = new Map(bones.map((_, j) => [j, j] as [number, number]));
  return retargetClip(bones, motion, identity, { rootMotion: true, name: options.name ?? "Ragdoll" });
}
