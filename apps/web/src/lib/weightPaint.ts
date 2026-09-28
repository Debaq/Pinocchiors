// Matemática del pincel de pesos sobre la tabla de influencias del visor:
// por vértice, `k` pares (hueso, peso) que suman 1.

export type PaintMode = "add" | "subtract" | "smooth";

/**
 * Cambia el peso de `bone` en la fila que empieza en `base` y reparte el
 * resto entre las otras influencias, conservando la suma 1. Si el hueso no
 * está entre las influencias, reemplaza a la más débil (si el peso nuevo la
 * supera). Devuelve `true` si la fila cambió.
 *
 * - `add`: acerca el peso a 1 en `amount` (fracción de lo que falta).
 * - `subtract`: lo acerca a 0 en `amount` (fracción de lo que tiene).
 * - `smooth`: lo acerca a `smoothTarget` (el promedio de los vecinos).
 *
 * Si `bone` es la única influencia, lo que pierde va a `fallback` (p. ej. el
 * hueso vecino en la cadena); sin respaldo la fila no cambia.
 */
export function paintRow(
  row: Float32Array,
  base: number,
  k: number,
  bone: number,
  mode: PaintMode,
  amount: number,
  smoothTarget?: number,
  fallback?: number
): boolean {
  let slot = -1;
  let weakest = 0;
  for (let i = 0; i < k; i++) {
    if (row[base + i * 2] === bone && (row[base + i * 2 + 1] > 0 || slot < 0)) slot = i;
    if (row[base + i * 2 + 1] < row[base + weakest * 2 + 1]) weakest = i;
  }
  const current = slot >= 0 ? row[base + slot * 2 + 1] : 0;
  let target = current;
  if (mode === "add") target = current + amount * (1 - current);
  else if (mode === "subtract") target = current - amount * current;
  else if (smoothTarget !== undefined) target = current + amount * (smoothTarget - current);
  target = Math.min(1, Math.max(0, target));
  if (Math.abs(target - current) < 1e-6) return false;

  if (slot < 0) {
    if (target <= row[base + weakest * 2 + 1]) return false;
    slot = weakest;
    row[base + slot * 2] = bone;
    row[base + slot * 2 + 1] = 0;
  }
  let others = 0;
  for (let i = 0; i < k; i++) if (i !== slot) others += row[base + i * 2 + 1];
  if (others <= 1e-9 && target < 1) {
    if (fallback === undefined || fallback === bone || k < 2) return false;
    // Única influencia: lo que pierde va al respaldo, en la ranura libre
    const free = slot === 0 ? 1 : 0;
    row[base + free * 2] = fallback;
    row[base + free * 2 + 1] = 1 - target;
    row[base + slot * 2 + 1] = target;
    return true;
  }
  const scale = others > 1e-9 ? (1 - target) / others : 0;
  for (let i = 0; i < k; i++) {
    if (i === slot) row[base + i * 2 + 1] = target;
    else row[base + i * 2 + 1] *= scale;
  }
  return true;
}

/** Peso de `bone` en la fila que empieza en `base` */
export function boneWeight(row: Float32Array, base: number, k: number, bone: number): number {
  for (let i = 0; i < k; i++) {
    if (row[base + i * 2] === bone) return row[base + i * 2 + 1];
  }
  return 0;
}
