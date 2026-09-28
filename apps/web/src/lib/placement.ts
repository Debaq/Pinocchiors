/**
 * Orientación del modelo: arma la transformación rígida que el backend aplica
 * a malla, quads, esqueleto y rig (`apply_placement`).
 *
 * Convención: Y arriba, frente hacia +Z. Las rotaciones se hacen en torno al
 * centro de la caja y el modelo vuelve a apoyarse en la grilla, así un giro
 * rápido no lo deja flotando ni hundido.
 */
import * as THREE from "three";

export interface FloorCandidate {
  /** Normal hacia afuera: al apoyar el modelo apunta hacia abajo */
  normal: [number, number, number];
  polygon: [number, number, number][];
  area: number;
  /** El centro de masa cae sobre el apoyo */
  stable: boolean;
}

export interface PlacementInfo {
  candidates: FloorCandidate[];
  center_of_mass: [number, number, number];
}

export type OriginMode = "base" | "box" | "mass" | "point";
export type Axis = "x" | "y" | "z";

const AXES: Record<Axis, THREE.Vector3> = {
  x: new THREE.Vector3(1, 0, 0),
  y: new THREE.Vector3(0, 1, 0),
  z: new THREE.Vector3(0, 0, 1),
};

/** Caja de los vértices tras aplicar `m` (sin `m`, tal como están) */
export function boundsAfter(positions: Float32Array, m?: THREE.Matrix4): THREE.Box3 {
  const box = new THREE.Box3();
  const p = new THREE.Vector3();
  for (let i = 0; i < positions.length; i += 3) {
    p.set(positions[i], positions[i + 1], positions[i + 2]);
    if (m) p.applyMatrix4(m);
    box.expandByPoint(p);
  }
  return box;
}

/** Tras `m`, baja el modelo hasta tocar la grilla y (si `center`) lo centra en X y Z */
function seated(positions: Float32Array, m: THREE.Matrix4, center: boolean): THREE.Matrix4 {
  const box = boundsAfter(positions, m);
  const c = box.getCenter(new THREE.Vector3());
  const offset = new THREE.Vector3(center ? -c.x : 0, -box.min.y, center ? -c.z : 0);
  return new THREE.Matrix4().makeTranslation(offset.x, offset.y, offset.z).multiply(m);
}

/** `r` aplicada en torno al centro de la caja actual */
function aboutCenter(positions: Float32Array, r: THREE.Matrix4): THREE.Matrix4 {
  const c = boundsAfter(positions).getCenter(new THREE.Vector3());
  return new THREE.Matrix4()
    .makeTranslation(c.x, c.y, c.z)
    .multiply(r)
    .multiply(new THREE.Matrix4().makeTranslation(-c.x, -c.y, -c.z));
}

/** El plano de normal `normal` pasa a ser el piso; el modelo queda apoyado y centrado */
export function floorMatrix(positions: Float32Array, normal: THREE.Vector3): THREE.Matrix4 {
  const q = new THREE.Quaternion().setFromUnitVectors(normal.clone().normalize(), new THREE.Vector3(0, -1, 0));
  return seated(positions, aboutCenter(positions, new THREE.Matrix4().makeRotationFromQuaternion(q)), true);
}

/**
 * Gira en torno al eje vertical para que `normal` mire hacia +Z (el frente).
 * `null` si la normal es casi vertical: no indica ninguna dirección horizontal.
 */
export function frontMatrix(positions: Float32Array, normal: THREE.Vector3): THREE.Matrix4 | null {
  const horizontal = new THREE.Vector2(normal.x, normal.z);
  if (horizontal.length() < 0.2 * normal.length()) return null;
  const angle = -Math.atan2(normal.x, normal.z);
  return aboutCenter(positions, new THREE.Matrix4().makeRotationY(angle));
}

/** Giro de `degrees` en torno a `axis` (regla de la mano derecha); queda apoyado */
export function rotationMatrix(positions: Float32Array, axis: Axis, degrees: number): THREE.Matrix4 {
  const r = new THREE.Matrix4().makeRotationAxis(AXES[axis], THREE.MathUtils.degToRad(degrees));
  return seated(positions, aboutCenter(positions, r), false);
}

/** Espejo que invierte `axis`, en torno al centro de la caja */
export function mirrorMatrix(positions: Float32Array, axis: Axis): THREE.Matrix4 {
  const s = new THREE.Vector3(1, 1, 1).setComponent(["x", "y", "z"].indexOf(axis), -1);
  return aboutCenter(positions, new THREE.Matrix4().makeScale(s.x, s.y, s.z));
}

/** Solo baja (o sube) el modelo hasta tocar la grilla */
export function dropMatrix(positions: Float32Array): THREE.Matrix4 {
  return seated(positions, new THREE.Matrix4(), false);
}

/** Lleva el punto de referencia elegido al origen */
export function originMatrix(
  positions: Float32Array,
  mode: OriginMode,
  centerOfMass?: THREE.Vector3,
  point?: THREE.Vector3
): THREE.Matrix4 | null {
  const box = boundsAfter(positions);
  const c = box.getCenter(new THREE.Vector3());
  const target =
    mode === "base" ? new THREE.Vector3(c.x, box.min.y, c.z) : mode === "box" ? c : mode === "mass" ? centerOfMass : point;
  if (!target) return null;
  return new THREE.Matrix4().makeTranslation(-target.x, -target.y, -target.z);
}

/**
 * Normal de la zona alrededor de `point`: promedio por área de los triángulos
 * cercanos. Una sola cara es ruidosa en mallas orgánicas o escaneadas.
 */
export function zoneNormal(
  positions: Float32Array,
  indices: Uint32Array,
  point: THREE.Vector3,
  radius: number
): THREE.Vector3 | null {
  const sum = new THREE.Vector3();
  const [a, b, c, ab, ac, n, centroid] = Array.from({ length: 7 }, () => new THREE.Vector3());
  const r2 = radius * radius;
  for (let t = 0; t < indices.length; t += 3) {
    a.fromArray(positions, indices[t] * 3);
    b.fromArray(positions, indices[t + 1] * 3);
    c.fromArray(positions, indices[t + 2] * 3);
    centroid.copy(a).add(b).add(c).divideScalar(3);
    if (centroid.distanceToSquared(point) > r2) continue;
    // Producto cruz sin normalizar: pesa por área
    n.crossVectors(ab.subVectors(b, a), ac.subVectors(c, a));
    sum.add(n);
  }
  return sum.lengthSq() > 0 ? sum.normalize() : null;
}
