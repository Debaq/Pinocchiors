/**
 * Alinear el escaneo por sus rasgos: una cara al piso o al frente, un eje
 * vertical, un punto al origen. Cada función da la transformación rígida en
 * coordenadas del CAD (mm, Z arriba) como matriz 4×4 por columnas;
 * `toSceneMatrix` la pasa a las del modelo (Y arriba, sus unidades), que es
 * lo que recibe `apply_placement`.
 */
import type { P3, Plane } from "./cad";

/** 4×4 por columnas */
export type Mat4 = number[];

const dot = (a: P3, b: P3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a: P3, b: P3): P3 => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const norm = (a: P3) => Math.hypot(a[0], a[1], a[2]);
const unit = (a: P3): P3 => {
  const l = norm(a) || 1;
  return [a[0] / l, a[1] / l, a[2] / l];
};

/** Rotación 3×3 (por filas) mínima que lleva `from` a `to` (unitarios) */
function rotationBetween(from: P3, to: P3): number[][] {
  const c = dot(from, to);
  if (c > 1 - 1e-12) return [[1, 0, 0], [0, 1, 0], [0, 0, 1]];
  if (c < -1 + 1e-12) {
    // Media vuelta alrededor de cualquier eje perpendicular
    const helper: P3 = Math.abs(from[0]) < 0.9 ? [1, 0, 0] : [0, 1, 0];
    const k = unit(cross(from, helper));
    return [0, 1, 2].map((i) => [0, 1, 2].map((j) => 2 * k[i] * k[j] - (i === j ? 1 : 0)));
  }
  // Rodrigues: R = I + [v]× + [v]×² / (1 + c)
  const v = cross(from, to);
  const vx = [
    [0, -v[2], v[1]],
    [v[2], 0, -v[0]],
    [-v[1], v[0], 0],
  ];
  const vx2 = [0, 1, 2].map((i) => [0, 1, 2].map((j) => vx[i][0] * vx[0][j] + vx[i][1] * vx[1][j] + vx[i][2] * vx[2][j]));
  return [0, 1, 2].map((i) => [0, 1, 2].map((j) => (i === j ? 1 : 0) + vx[i][j] + vx2[i][j] / (1 + c)));
}

/** Matriz por columnas desde una rotación (filas) y una traslación */
function compose(r: number[][], t: P3): Mat4 {
  return [r[0][0], r[1][0], r[2][0], 0, r[0][1], r[1][1], r[2][1], 0, r[0][2], r[1][2], r[2][2], 0, t[0], t[1], t[2], 1];
}

export function applyMat(m: Mat4, p: P3): P3 {
  return [0, 1, 2].map((i) => m[i] * p[0] + m[4 + i] * p[1] + m[8 + i] * p[2] + m[12 + i]) as P3;
}

export function multiply(a: Mat4, b: Mat4): Mat4 {
  const out = new Array(16).fill(0);
  for (let c = 0; c < 4; c++) for (let r = 0; r < 4; r++) for (let k = 0; k < 4; k++) out[c * 4 + r] += a[k * 4 + r] * b[c * 4 + k];
  return out;
}

/** Gira alrededor de `pivot` con la rotación `r` y después corre `shift` */
function about(r: number[][], pivot: P3, shift: P3 = [0, 0, 0]): Mat4 {
  const rp = [0, 1, 2].map((i) => r[i][0] * pivot[0] + r[i][1] * pivot[1] + r[i][2] * pivot[2]);
  return compose(r, [pivot[0] - rp[0] + shift[0], pivot[1] - rp[1] + shift[1], pivot[2] - rp[2] + shift[2]]);
}

/** La cara (normal hacia afuera) apoyada en el piso: normal hacia −Z y la cara en z = 0 */
export function faceToFloor(plane: Plane): Mat4 {
  const r = rotationBetween(unit(plane.normal), [0, 0, -1]);
  // Gira alrededor del punto de la cara y la baja (o sube) hasta z = 0
  return about(r, plane.origin, [0, 0, -plane.origin[2]]);
}

/** Gira alrededor del eje vertical para que la cara mire al frente (−Y) */
export function faceToFront(plane: Plane): Mat4 | null {
  const h: P3 = [plane.normal[0], plane.normal[1], 0];
  // Una cara horizontal no tiene frente
  if (norm(h) < 0.2) return null;
  return about(rotationBetween(unit(h), [0, -1, 0]), plane.origin);
}

/** El eje (de un cilindro) vertical y pasando por el origen en planta */
export function axisToVertical(origin: P3, direction: P3): Mat4 {
  const d = unit(direction);
  const r = rotationBetween(d[2] < 0 ? [-d[0], -d[1], -d[2]] : d, [0, 0, 1]);
  const placed = applyMat(about(r, [0, 0, 0]), origin);
  return compose(r, [-placed[0], -placed[1], 0]);
}

/** Corre el escaneo para que el punto quede en el origen */
export function pointToOrigin(p: P3): Mat4 {
  return compose([[1, 0, 0], [0, 1, 0], [0, 0, 1]], [-p[0], -p[1], -p[2]]);
}

/**
 * De coordenadas del CAD a las del modelo: el CAD es el modelo en mm con Z
 * arriba, `cad = (x, −z, y) · mmPerUnit`. Queda `C⁻¹ · m · C` (rígida: la
 * escala se cancela).
 */
export function toSceneMatrix(m: Mat4, mmPerUnit: number): Mat4 {
  const s = mmPerUnit;
  // C por columnas: x → x, y → z, z → −y, por s
  const c: Mat4 = [s, 0, 0, 0, 0, 0, s, 0, 0, -s, 0, 0, 0, 0, 0, 1];
  const ci: Mat4 = [1 / s, 0, 0, 0, 0, 0, -1 / s, 0, 0, 1 / s, 0, 0, 0, 0, 0, 1];
  return multiply(ci, multiply(m, c));
}
