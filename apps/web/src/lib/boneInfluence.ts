/**
 * Influencia estimada de los huesos antes de calcular los pesos: cada vértice
 * se reparte entre los huesos más cercanos (distancia al segmento padre →
 * hueso), como una vista previa de qué mueve cada uno. No reemplaza a los
 * pesos calculados, que miran el grosor de la malla.
 */

export interface InfluenceBone {
  position: [number, number, number];
  parent: number | null;
}

export interface Influence {
  numVertices: number;
  numBones: number;
  /** Por vértice, `maxInfluences` pares (hueso, peso), como los pesos calculados */
  weights: Float32Array;
  maxInfluences: number;
}

/**
 * Peso de un hueso: (dmin / d)^6, con dmin la distancia al más cercano. Se
 * calcula con distancias al cuadrado, (dmin² / d²)³, y se descartan los que
 * quedan a más de 2,5 × dmin (pesarían menos de 0,4 %)
 */
const CUTOFF2 = 2.5 * 2.5;

export function estimateInfluence(positions: Float32Array, bones: InfluenceBone[], maxInfluences = 4): Influence {
  const numVertices = positions.length / 3;
  const k = maxInfluences;
  const weights = new Float32Array(numVertices * k * 2);

  // Segmentos padre → hueso (el peso del hueso b es el de su segmento):
  // origen, dirección y 1 / largo², de a 8 números por segmento
  const segBone: number[] = [];
  const seg: number[] = [];
  let extent = 0;
  bones.forEach((bone, b) => {
    if (bone.parent === null) return;
    const [ax, ay, az] = bones[bone.parent].position;
    const [bx, by, bz] = bone.position;
    const dx = bx - ax;
    const dy = by - ay;
    const dz = bz - az;
    const len2 = dx * dx + dy * dy + dz * dz;
    extent = Math.max(extent, Math.sqrt(len2));
    segBone.push(b);
    seg.push(ax, ay, az, dx, dy, dz, len2 > 0 ? 1 / len2 : 0, 0);
  });
  const numSegments = segBone.length;
  if (numSegments === 0) return { numVertices, numBones: bones.length, weights, maxInfluences: k };
  const S = Float64Array.from(seg);

  // Tamaño del esqueleto: evita dividir por cero sobre el hueso mismo
  const eps2 = Math.max(extent * 1e-3, 1e-9) ** 2;

  const dist = new Float64Array(numSegments);
  const best = new Int32Array(k);
  const bestW = new Float64Array(k);
  for (let v = 0; v < numVertices; v++) {
    const px = positions[v * 3];
    const py = positions[v * 3 + 1];
    const pz = positions[v * 3 + 2];
    let dmin = Infinity;
    for (let i = 0; i < numSegments; i++) {
      const o = i * 8;
      const dx = S[o + 3];
      const dy = S[o + 4];
      const dz = S[o + 5];
      const wx = px - S[o];
      const wy = py - S[o + 1];
      const wz = pz - S[o + 2];
      let t = (wx * dx + wy * dy + wz * dz) * S[o + 6];
      t = t < 0 ? 0 : t > 1 ? 1 : t;
      const ex = wx - t * dx;
      const ey = wy - t * dy;
      const ez = wz - t * dz;
      const d2 = ex * ex + ey * ey + ez * ez + eps2;
      dist[i] = d2;
      if (d2 < dmin) dmin = d2;
    }

    // Los k de más peso
    best.fill(-1);
    bestW.fill(0);
    const far = dmin * CUTOFF2;
    for (let i = 0; i < numSegments; i++) {
      if (dist[i] > far) continue;
      const r = dmin / dist[i];
      const w = r * r * r;
      if (w <= bestW[k - 1]) continue;
      let slot = k - 1;
      while (slot > 0 && bestW[slot - 1] < w) {
        bestW[slot] = bestW[slot - 1];
        best[slot] = best[slot - 1];
        slot--;
      }
      bestW[slot] = w;
      best[slot] = i;
    }
    let sum = 0;
    for (let slot = 0; slot < k; slot++) sum += bestW[slot];
    for (let slot = 0; slot < k; slot++) {
      if (best[slot] < 0) continue;
      weights[(v * k + slot) * 2] = segBone[best[slot]];
      weights[(v * k + slot) * 2 + 1] = bestW[slot] / sum;
    }
  }
  return { numVertices, numBones: bones.length, weights, maxInfluences: k };
}

/** Tono (0–1) propio de cada hueso: ángulo áureo, vecinos bien distintos */
export function boneHue(bone: number): number {
  return (bone * 0.618033988749895) % 1;
}
