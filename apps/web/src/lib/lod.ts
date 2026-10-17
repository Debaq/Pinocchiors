// LOD al animar: malla decimada con pesos de skinning, solo para el visor.
// El backend (`animation_lod`, apps/desktop/src/lod.rs) la calcula en un hilo
// aparte; el visor la guarda y la usa mientras dura la animación. Exportar,
// guardar, renderizar y capturar siempre usan la malla completa.

import { invoke } from "@tauri-apps/api/core";
import { decodeMesh, decodeWeights } from "./buffers.ts";
import type { MeshData, WeightsData } from "./Viewer3D";

/** Desde cuántas caras se usa LOD (igual que `LOD_FROM_FACES` del backend) */
export const LOD_FROM_FACES = 100_000;

export interface AnimationLodData {
  /** Vértices de la malla completa de la que sale (para comprobar que sigue siendo la del visor) */
  sourceVertices: number;
  mesh: MeshData;
  weights: WeightsData;
}

/** Binario de `Lod::to_bytes`; vacío = no hay LOD */
export function decodeLod(buffer: ArrayBuffer): AnimationLodData | null {
  if (buffer.byteLength < 8) return null;
  const [sourceVertices, meshLength] = new Uint32Array(buffer, 0, 2);
  return {
    sourceVertices,
    mesh: decodeMesh(buffer.slice(8, 8 + meshLength)),
    weights: decodeWeights(buffer.slice(8 + meshLength)),
  };
}

/** Pide al backend la malla decimada del modelo actual (`null`: no hace falta o no se puede) */
export async function fetchAnimationLod(): Promise<AnimationLodData | null> {
  return decodeLod(await invoke<ArrayBuffer>("animation_lod"));
}

/**
 * Hasta 4 influencias por vértice (como el GLB exportado), renormalizadas.
 * `target` lleva cada hueso al que recibe su peso (los que no deforman pasan
 * el suyo al primer ancestro que sí). Sin pesos, el vértice sigue a la raíz.
 */
export function skinAttributes(
  weights: WeightsData,
  numVertices: number,
  numBones: number,
  target: number[]
): { skinIndex: Uint16Array; skinWeight: Float32Array } {
  const k = weights.maxInfluences;
  const skinIndex = new Uint16Array(numVertices * 4);
  const skinWeight = new Float32Array(numVertices * 4);
  const pairs: [number, number][] = [];
  for (let v = 0; v < numVertices; v++) {
    pairs.length = 0;
    for (let i = 0; i < k; i++) {
      const w = weights.weights[(v * k + i) * 2 + 1];
      if (w <= 0) continue;
      const bone = target[weights.weights[(v * k + i) * 2]] ?? 0;
      const same = pairs.find((p) => p[0] === bone);
      if (same) same[1] += w;
      else pairs.push([bone, w]);
    }
    pairs.sort((a, b) => b[1] - a[1]);
    const top = pairs.slice(0, 4);
    const sum = top.reduce((acc, [, w]) => acc + w, 0);
    top.forEach(([bone, w], slot) => {
      skinIndex[v * 4 + slot] = Math.min(bone, numBones - 1);
      skinWeight[v * 4 + slot] = w / sum;
    });
    if (top.length === 0) skinWeight[v * 4] = 1;
  }
  return { skinIndex, skinWeight };
}
