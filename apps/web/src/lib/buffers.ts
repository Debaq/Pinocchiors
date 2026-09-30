// Decodificación de los binarios que envía el backend (ver `pack_mesh` y
// `WeightsData::to_bytes` en apps/desktop/src/commands.rs). Todo va en
// palabras de 4 bytes little-endian, así que se leen como vistas de typed
// arrays sin copiar ni parsear JSON.

import type { MeshData, WeightsData } from "./Viewer3D";

/** Malla: cabecera u32 × 4 (vértices, índices, hay UVs, índices de quads) */
export function decodeMesh(buffer: ArrayBuffer): MeshData {
  const [vertices, indexCount, hasUvs, quadCount] = new Uint32Array(buffer, 0, 4);
  let offset = 16;
  const floats = (count: number) => {
    const view = new Float32Array(buffer, offset, count);
    offset += count * 4;
    return view;
  };
  const uints = (count: number) => {
    const view = new Uint32Array(buffer, offset, count);
    offset += count * 4;
    return view;
  };
  const positions = floats(vertices * 3);
  const normals = floats(vertices * 3);
  const uvs = hasUvs ? floats(vertices * 2) : undefined;
  const indices = uints(indexCount);
  const quadIndices = quadCount > 0 ? uints(quadCount) : undefined;
  // Opcional al final: grupos por material, cantidad y [inicio, cantidad, material]
  let groups: Uint32Array | undefined;
  let groupNodes: Uint32Array | undefined;
  if (offset + 4 <= buffer.byteLength) {
    const [count] = new Uint32Array(buffer, offset, 1);
    offset += 4;
    groups = uints(count * 3);
    // Opcional después: el nodo del archivo de cada grupo
    if (offset + count * 4 <= buffer.byteLength) groupNodes = uints(count);
  }
  return { positions, normals, indices, uvs, quadIndices, groups, groupNodes };
}

/** Pesos: cabecera u32 × 4 (vértices, huesos, influencias, bytes de nombres) */
export function decodeWeights(buffer: ArrayBuffer): WeightsData {
  const [numVertices, numBones, maxInfluences, namesLength] = new Uint32Array(buffer, 0, 4);
  const names = new TextDecoder().decode(new Uint8Array(buffer, 16, namesLength));
  const offset = 16 + Math.ceil(namesLength / 4) * 4;
  return {
    numVertices,
    numBones,
    boneNames: names.length > 0 ? names.split("\n") : [],
    weights: new Float32Array(buffer, offset, numVertices * maxInfluences * 2),
    maxInfluences,
  };
}
