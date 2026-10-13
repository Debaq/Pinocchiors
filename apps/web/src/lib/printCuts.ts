/**
 * Planos de corte de Fabricar: dónde caen en el modelo (para dibujarlos en el visor) a partir de lo que se
 * pide al backend. El backend y la interfaz hablan en ejes con Z arriba y milímetros; el visor, en la
 * escena (Y arriba, unidades de la escena).
 */

export type V3 = [number, number, number];

/** Un corte: plano perpendicular a un eje, a `position` mm del borde mínimo del modelo, con inclinación */
export interface CutPlane {
  /** 0 = X, 1 = Y, 2 = Z (como se muestran, Z arriba) */
  axis: 0 | 1 | 2;
  position: number;
  tilt_deg: number;
}

/** Escena (Y arriba) → ejes de la interfaz (Z arriba) */
export const toUi = (p: V3): V3 => [p[0], -p[2], p[1]];
/** Ejes de la interfaz → escena */
export const fromUi = (p: V3): V3 => [p[0], p[2], -p[1]];

/** Caja del modelo en los ejes de la interfaz y en unidades de la escena */
export function uiBounds(positions: ArrayLike<number>): { min: V3; max: V3 } {
  const min: V3 = [Infinity, Infinity, Infinity];
  const max: V3 = [-Infinity, -Infinity, -Infinity];
  for (let i = 0; i + 2 < positions.length; i += 3) {
    const p = toUi([positions[i], positions[i + 1], positions[i + 2]]);
    for (let k = 0; k < 3; k++) {
      if (p[k] < min[k]) min[k] = p[k];
      if (p[k] > max[k]) max[k] = p[k];
    }
  }
  return { min, max };
}

export interface PlaneQuad {
  /** Centro del recuadro (escena) */
  origin: V3;
  /** Normal (escena, unitaria) */
  normal: V3;
  /** Las cuatro esquinas del recuadro (escena) */
  corners: [V3, V3, V3, V3];
}

const cross = (a: V3, b: V3): V3 => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const norm = (a: V3): V3 => {
  const l = Math.hypot(a[0], a[1], a[2]) || 1;
  return [a[0] / l, a[1] / l, a[2] / l];
};

/**
 * El recuadro de un plano de corte sobre el modelo. `mmPerUnit` convierte la posición (mm) a unidades de
 * la escena. Mismas cuentas que el backend: el plano pasa por la posición sobre su eje y por el centro del
 * modelo en los otros dos; la inclinación gira la normal hacia el eje siguiente (X→Y, Y→Z, Z→X)
 */
export function planeQuad(bounds: { min: V3; max: V3 }, plane: CutPlane, mmPerUnit: number): PlaneQuad {
  const center: V3 = [0, 1, 2].map((k) => (bounds.min[k] + bounds.max[k]) / 2) as V3;
  const origin: V3 = [...center];
  origin[plane.axis] = bounds.min[plane.axis] + plane.position / mmPerUnit;
  const t = (plane.tilt_deg * Math.PI) / 180;
  const n: V3 = [0, 0, 0];
  n[plane.axis] = Math.cos(t);
  n[(plane.axis + 1) % 3] = Math.sin(t);
  const diag = Math.hypot(bounds.max[0] - bounds.min[0], bounds.max[1] - bounds.min[1], bounds.max[2] - bounds.min[2]);
  const half = diag * 0.55;
  // Base del plano (en ejes de la interfaz): un eje cualquiera que no sea paralelo a la normal
  const helper: V3 = Math.abs(n[0]) < 0.9 ? [1, 0, 0] : [0, 1, 0];
  const u = norm(cross(helper, n));
  const v = cross(n, u);
  const corner = (a: number, b: number): V3 => [0, 1, 2].map((k) => origin[k] + half * (a * u[k] + b * v[k])) as V3;
  return {
    origin: fromUi(origin),
    normal: fromUi(n),
    corners: [fromUi(corner(-1, -1)), fromUi(corner(1, -1)), fromUi(corner(1, 1)), fromUi(corner(-1, 1))],
  };
}

/** Cuánto mide el modelo (mm) en cada eje de la interfaz, para acotar las posiciones */
export function modelSizeMm(bounds: { min: V3; max: V3 }, mmPerUnit: number): V3 {
  return [0, 1, 2].map((k) => (bounds.max[k] - bounds.min[k]) * mmPerUnit) as V3;
}
