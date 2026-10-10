// Asistencia al dibujo del sketch: coordenadas escritas para el próximo punto
// y anclaje a la rejilla. Funciones puras: se prueban con node
// (e2e/sketchInput.test.mjs).

import type { P2 } from "./cad.ts";

/** Un número con coma o punto decimal */
function num(text: string): number | undefined {
  const t = text.trim().replace(",", ".");
  if (!/^[-+]?(\d+\.?\d*|\.\d+)(e[-+]?\d+)?$/i.test(t)) return undefined;
  return Number(t);
}

/** Dos números: con punto y coma la coma es decimal ("2,5; 3"); si no, separa ("2.5, 3" o "2.5 3") */
function pair(text: string): [number, number] | undefined {
  const parts = text.includes(";") ? text.split(";") : text.includes(",") ? text.split(",") : text.trim().split(/\s+/);
  if (parts.length !== 2) return undefined;
  const [a, b] = parts.map(num);
  return a === undefined || b === undefined ? undefined : [a, b];
}

/**
 * Lee el próximo punto escrito: `x, y` (absoluto), `@dx, dy` (desde el último
 * punto) o `@d < ángulo` (polar desde el último punto, ángulo en grados
 * antihorario desde X). Sin `@`, `d < ángulo` es polar desde el origen.
 * Devuelve el punto o un mensaje.
 */
export function parseCoords(text: string, last: P2 | undefined): P2 | string {
  let t = text.trim();
  if (!t) return "Escribir x, y · @dx, dy · @d<ángulo";
  const relative = t.startsWith("@");
  if (relative) t = t.slice(1);
  const base: P2 = relative ? (last ?? [0, 0]) : [0, 0];
  if (t.includes("<")) {
    const [d, a] = t.split("<").map(num);
    if (d === undefined || a === undefined) return "Polar: @distancia<ángulo (por ejemplo @20<45)";
    const r = (a * Math.PI) / 180;
    return [base[0] + d * Math.cos(r), base[1] + d * Math.sin(r)];
  }
  const p = pair(t);
  if (!p) return "Coordenadas: x, y (o x; y con coma decimal)";
  return [base[0] + p[0], base[1] + p[1]];
}

/** El nudo de la rejilla más cercano */
export function snapToGrid(p: P2, spacing: number): P2 {
  if (!(spacing > 0)) return p;
  const r = (v: number) => {
    const k = Math.round(v / spacing) * spacing;
    // Sin el −0 ni los 0,30000000000000004
    return +k.toFixed(10) + 0;
  };
  return [r(p[0]), r(p[1])];
}

/**
 * Líneas de la rejilla que cubren la caja `[x0, y0, x1, y1]` con un margen:
 * segmentos y si son principales (cada 5). Si saldrían más de `max` por eje,
 * el paso se multiplica por 5 hasta que entren.
 */
export function gridSegments(box: [number, number, number, number], spacing: number, max = 200): { a: P2; b: P2; major: boolean }[] {
  if (!(spacing > 0)) return [];
  let step = spacing;
  const [x0, y0, x1, y1] = box;
  const span = Math.max(x1 - x0, y1 - y0, step);
  while (span / step > max) step *= 5;
  const lo = (v: number) => Math.floor(v / step) * step;
  const hi = (v: number) => Math.ceil(v / step) * step;
  const [ax, bx, ay, by] = [lo(x0), hi(x1), lo(y0), hi(y1)];
  const out: { a: P2; b: P2; major: boolean }[] = [];
  const major = (v: number) => Math.abs(Math.round(v / (step * 5)) * step * 5 - v) < step * 1e-6;
  for (let x = ax; x <= bx + step * 1e-6; x += step) out.push({ a: [x, ay], b: [x, by], major: major(x) });
  for (let y = ay; y <= by + step * 1e-6; y += step) out.push({ a: [ax, y], b: [bx, y], major: major(y) });
  return out;
}
