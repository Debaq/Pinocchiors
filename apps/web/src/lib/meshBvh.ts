// Aceleración del picking (raycast) con BVH: se construye una vez por geometría y se reutiliza.
// Solo para mallas estáticas de reposo (currentMesh / weightsMesh); las SkinnedMesh no se tocan.
import * as THREE from "three";
import { MeshBVH, acceleratedRaycast } from "three-mesh-bvh";

/** Por debajo de esto el raycast de three ya es instantáneo y no vale la pena el BVH */
export const BVH_MIN_TRIANGLES = 20000;

const stale = new WeakSet<THREE.BufferGeometry>();
const wired = new WeakSet<THREE.Mesh>();

function triangleCount(geometry: THREE.BufferGeometry): number {
  return geometry.index ? geometry.index.count / 3 : (geometry.getAttribute("position")?.count ?? 0) / 3;
}

/**
 * Deja la malla lista para picking rápido: construye el BVH la primera vez y lo ajusta
 * (refit) si los vértices cambiaron. Devuelve true si la malla usa BVH.
 * `indirect` deja intacto el índice de la geometría (se comparte con MeshData y los grupos).
 */
export function ensureBvh(mesh: THREE.Mesh): boolean {
  const geometry = mesh.geometry;
  if (!geometry.getAttribute("position") || triangleCount(geometry) < BVH_MIN_TRIANGLES) return false;
  const tree = geometry.boundsTree as MeshBVH | undefined;
  if (!tree) {
    geometry.boundsTree = new MeshBVH(geometry, { indirect: true });
    stale.delete(geometry);
  } else if (stale.has(geometry)) {
    tree.refit();
    stale.delete(geometry);
  }
  if (!wired.has(mesh)) {
    mesh.raycast = acceleratedRaycast;
    wired.add(mesh);
  }
  return true;
}

/** Los vértices se movieron en sitio: el BVH se ajusta en el próximo picking */
export function markBvhStale(mesh: THREE.Mesh): void {
  if (mesh.geometry.boundsTree) stale.add(mesh.geometry);
}

/** La geometría cambió de índice o se libera: se descarta el BVH (se reconstruye al próximo uso) */
export function disposeBvh(mesh: THREE.Mesh | null | undefined): void {
  if (!mesh) return;
  const geometry = mesh.geometry;
  if (geometry.boundsTree) {
    geometry.boundsTree = undefined;
    stale.delete(geometry);
  }
  if (wired.has(mesh)) {
    mesh.raycast = THREE.Mesh.prototype.raycast;
    wired.delete(mesh);
  }
}

/** Primer impacto de un rayo contra una malla (con BVH si corresponde) */
export function pickMesh(raycaster: THREE.Raycaster, mesh: THREE.Mesh): THREE.Intersection | null {
  ensureBvh(mesh);
  return raycaster.intersectObject(mesh)[0] ?? null;
}
