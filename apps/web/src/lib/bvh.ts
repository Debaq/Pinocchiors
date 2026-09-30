/**
 * Lectura de BVH (captura de movimiento clásica): jerarquía de articulaciones
 * con su desplazamiento respecto del padre, canales por articulación y un
 * cuadro por línea. Se convierte a un movimiento de origen (`SourceMotion`)
 * que `retarget.ts` pasa al esqueleto del modelo.
 *
 * Los `End Site` se agregan como articulaciones punta (`<padre>_end`): dan la
 * dirección del último hueso (cabeza, manos, pies).
 */

import * as THREE from "three";
import type { Quat, Vec3 } from "./animation";
import type { SourceMotion } from "./retarget";

interface BvhJoint {
  name: string;
  parent: number | null;
  offset: Vec3;
  channels: string[];
  /** Posición del primer canal de la articulación en cada línea de MOTION */
  firstChannel: number;
}

export interface BvhFile {
  joints: BvhJoint[];
  fps: number;
  /** Valores de cada cuadro (todos los canales, en orden) */
  frames: number[][];
}

/** Lee el texto de un BVH; lanza un error con la línea si no se entiende */
export function parseBvh(text: string): BvhFile {
  const tokens = text.split(/\s+/).filter(Boolean);
  let i = 0;
  const next = () => {
    if (i >= tokens.length) throw new Error("BVH incompleto");
    return tokens[i++];
  };
  const expect = (word: string) => {
    const t = next();
    if (t.toUpperCase() !== word) throw new Error(`BVH: se esperaba ${word} y hay ${t}`);
  };
  const number = () => {
    const t = next();
    const v = Number(t);
    if (!Number.isFinite(v)) throw new Error(`BVH: número inválido "${t}"`);
    return v;
  };

  expect("HIERARCHY");
  const joints: BvhJoint[] = [];
  let channelCount = 0;
  const readJoint = (parent: number | null, endSite: boolean) => {
    const name = endSite ? `${joints[parent!].name}_end` : next();
    const index = joints.length;
    joints.push({ name, parent, offset: [0, 0, 0], channels: [], firstChannel: channelCount });
    expect("{");
    for (;;) {
      const t = next();
      const upper = t.toUpperCase();
      if (upper === "}") break;
      if (upper === "OFFSET") joints[index].offset = [number(), number(), number()];
      else if (upper === "CHANNELS") {
        const n = number();
        for (let k = 0; k < n; k++) joints[index].channels.push(next().toLowerCase());
        channelCount += n;
      } else if (upper === "JOINT") readJoint(index, false);
      else if (upper === "END") {
        expect("SITE");
        readJoint(index, true);
      } else throw new Error(`BVH: palabra inesperada "${t}"`);
    }
  };
  expect("ROOT");
  readJoint(null, false);
  // Otras raíces (raro): se leen igual, porque sus canales también están en cada línea
  while (tokens[i]?.toUpperCase() === "ROOT") {
    next();
    readJoint(null, false);
  }

  expect("MOTION");
  expect("FRAMES:");
  const count = Math.round(number());
  expect("FRAME");
  expect("TIME:");
  const frameTime = number();
  const frames: number[][] = [];
  // Un archivo cortado se queda con los cuadros completos
  for (let f = 0; f < count && tokens.length - i >= channelCount; f++) {
    const values: number[] = [];
    for (let k = 0; k < channelCount; k++) values.push(number());
    frames.push(values);
  }
  if (frames.length === 0) throw new Error("El BVH no tiene cuadros");
  return { joints, fps: frameTime > 0 ? Math.round(1 / frameTime) : 30, frames };
}

const AXIS: Record<string, THREE.Vector3> = {
  xrotation: new THREE.Vector3(1, 0, 0),
  yrotation: new THREE.Vector3(0, 1, 0),
  zrotation: new THREE.Vector3(0, 0, 1),
};

/** Movimiento de origen: posiciones de las articulaciones por cuadro y giro de la raíz */
export function bvhMotion(file: BvhFile): SourceMotion {
  const { joints } = file;
  const pose = (values: number[] | null) => {
    const rot: THREE.Quaternion[] = [];
    const pos: THREE.Vector3[] = [];
    joints.forEach((j, k) => {
      const local = new THREE.Vector3(...j.offset);
      const q = new THREE.Quaternion();
      if (values) {
        j.channels.forEach((c, n) => {
          const v = values[j.firstChannel + n];
          if (c === "xposition") local.x = v;
          else if (c === "yposition") local.y = v;
          else if (c === "zposition") local.z = v;
          else if (AXIS[c]) q.multiply(new THREE.Quaternion().setFromAxisAngle(AXIS[c], THREE.MathUtils.degToRad(v)));
        });
        // Los canales de posición reemplazan el offset (como el BVHLoader de three.js)
      }
      if (j.parent === null) {
        rot[k] = q;
        pos[k] = local;
      } else {
        rot[k] = rot[j.parent].clone().multiply(q);
        pos[k] = pos[j.parent].clone().add(local.applyQuaternion(rot[j.parent]));
      }
    });
    return { rot, pos };
  };
  const rest = pose(null);
  const toV = (v: THREE.Vector3): Vec3 => [v.x, v.y, v.z];
  const root = joints.findIndex((j) => j.parent === null);
  const positions: Vec3[][] = [];
  const rootRotation: Quat[] = [];
  for (const values of file.frames) {
    const p = pose(values);
    positions.push(p.pos.map(toV));
    const q = p.rot[root];
    rootRotation.push([q.x, q.y, q.z, q.w]);
  }
  return {
    bones: joints.map((j, k) => ({ name: j.name, parent: j.parent, position: toV(rest.pos[k]) })),
    fps: file.fps,
    positions,
    rootRotation,
  };
}
