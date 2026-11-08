// Deformar la malla con la pose de prueba (girar articulaciones sobre el
// modelo con pesos). Se queda en CPU a propósito: elegir caras, pintar pesos y
// medir leen las posiciones posadas. Antes cada vértice creaba objetos y
// giraba cuaterniones de three.js por cada influencia; ahora cada hueso trae
// su matriz armada una vez y el bucle es solo aritmética (PLAN_RENDIMIENTO R9).

/** Por hueso, 12 números: rotación 3 × 3 por filas y traslación (x → R·x + t) */
export const BONE_STRIDE = 12;

/**
 * Posiciones y normales posadas: `rest + Σ wᵢ (Rᵢ·rest + tᵢ − rest)` y la
 * normal girada igual (normalizada). Los huesos con `moved[b] = 0` no se
 * mueven y no suman nada.
 */
export function skinPose(
  rest: Float32Array,
  restNormals: Float32Array,
  weights: Float32Array,
  k: number,
  numVertices: number,
  bones: Float64Array,
  moved: Uint8Array,
  outPositions: Float32Array,
  outNormals: Float32Array,
): void {
  for (let v = 0; v < numVertices; v++) {
    const o = 3 * v;
    const rx = rest[o];
    const ry = rest[o + 1];
    const rz = rest[o + 2];
    const sx = restNormals[o];
    const sy = restNormals[o + 1];
    const sz = restNormals[o + 2];
    let px = rx;
    let py = ry;
    let pz = rz;
    let nx = sx;
    let ny = sy;
    let nz = sz;
    for (let i = 0; i < k; i++) {
      const w = weights[(v * k + i) * 2 + 1];
      if (w <= 0) continue;
      const b = weights[(v * k + i) * 2];
      if (!moved[b]) continue;
      const m = b * BONE_STRIDE;
      px += w * (bones[m] * rx + bones[m + 1] * ry + bones[m + 2] * rz + bones[m + 9] - rx);
      py += w * (bones[m + 3] * rx + bones[m + 4] * ry + bones[m + 5] * rz + bones[m + 10] - ry);
      pz += w * (bones[m + 6] * rx + bones[m + 7] * ry + bones[m + 8] * rz + bones[m + 11] - rz);
      nx += w * (bones[m] * sx + bones[m + 1] * sy + bones[m + 2] * sz - sx);
      ny += w * (bones[m + 3] * sx + bones[m + 4] * sy + bones[m + 5] * sz - sy);
      nz += w * (bones[m + 6] * sx + bones[m + 7] * sy + bones[m + 8] * sz - sz);
    }
    const len = Math.sqrt(nx * nx + ny * ny + nz * nz);
    const inv = len > 0 ? 1 / len : 0;
    outPositions[o] = px;
    outPositions[o + 1] = py;
    outPositions[o + 2] = pz;
    outNormals[o] = nx * inv;
    outNormals[o + 1] = ny * inv;
    outNormals[o + 2] = nz * inv;
  }
}

/** Matriz de giro (por filas) de un cuaternión unitario x, y, z, w */
export function rotationRows(x: number, y: number, z: number, w: number, out: Float64Array, at: number): void {
  const x2 = x + x;
  const y2 = y + y;
  const z2 = z + z;
  const xx = x * x2;
  const xy = x * y2;
  const xz = x * z2;
  const yy = y * y2;
  const yz = y * z2;
  const zz = z * z2;
  const wx = w * x2;
  const wy = w * y2;
  const wz = w * z2;
  out[at] = 1 - (yy + zz);
  out[at + 1] = xy - wz;
  out[at + 2] = xz + wy;
  out[at + 3] = xy + wz;
  out[at + 4] = 1 - (xx + zz);
  out[at + 5] = yz - wx;
  out[at + 6] = xz - wy;
  out[at + 7] = yz + wx;
  out[at + 8] = 1 - (xx + yy);
}
