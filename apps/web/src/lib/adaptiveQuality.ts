/** Calidad adaptativa del visor: lógica pura (el visor solo aplica el resultado) */

/** Fracción del pixel ratio completo mientras hay movimiento */
export const MOTION_SCALE = 0.7;
/** Un movimiento marcado hace menos de esto sigue contando como movimiento (ms) */
export const MOTION_GAP_MS = 150;
/** Sin cuadros durante esto, se restaura la calidad completa (ms) */
export const MOTION_REST_MS = 250;

/** ¿Hubo un movimiento marcado explícitamente hace poco? `lastMark` es el instante de la última marca */
export function isMotionFrame(now: number, lastMark: number | null): boolean {
  return lastMark !== null && now - lastMark < MOTION_GAP_MS;
}

/** Escala del pixel ratio: 1 en reposo, desactivado o si hay algo que no admite cambiar la resolución */
export function qualityScale(enabled: boolean, moving: boolean, blocked = false): number {
  return enabled && moving && !blocked ? MOTION_SCALE : 1;
}
