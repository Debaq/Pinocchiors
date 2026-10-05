// Texto del sketch: los contornos de las letras (de opentype.js) pasan a
// líneas y splines. Sin dependencias de ejecución para probarlo con node
// (e2e/sketchText.test.mjs); quien lo usa carga la fuente.

import type { PathCommand } from "opentype.js";
import type { P2 } from "./cad";

/** Tramo de un contorno: recta, o spline que pasa por esos puntos */
export type OutlinePiece = { kind: "line"; a: P2; b: P2 } | { kind: "spline"; points: P2[] };

/** Contorno de una letra: tramos encadenados, o una spline cerrada si es todo curva suave */
export type Contour = { pieces: OutlinePiece[] } | { closed: P2[] };

/** Tramo leído de la fuente; las curvas con los puntos por donde pasan y su dirección de salida y llegada */
type Segment = { kind: "line"; a: P2; b: P2 } | { kind: "curve"; points: P2[]; t0: P2; t1: P2 };

const sub = (a: P2, b: P2): P2 => [a[0] - b[0], a[1] - b[1]];
const len = (v: P2) => Math.hypot(v[0], v[1]);

/** Empalme suave entre dos curvas: las direcciones difieren menos de 15° */
function smoothJoint(a: Segment, b: Segment): boolean {
  if (a.kind !== "curve" || b.kind !== "curve") return false;
  const [u, v] = [a.t1, b.t0];
  const cos = (u[0] * v[0] + u[1] * v[1]) / (len(u) * len(v) || 1);
  return cos > Math.cos((15 * Math.PI) / 180);
}

/** Dirección no nula: la primera de la lista que tenga largo */
function firstDir(...dirs: P2[]): P2 {
  return dirs.find((d) => len(d) > 1e-12) ?? [1, 0];
}

/** Arma los contornos de un contorno leído: corridas de curvas suaves → una spline */
function contourOf(segs: Segment[]): Contour | undefined {
  if (!segs.length) return undefined;
  const n = segs.length;
  const joint = (i: number) => smoothJoint(segs[i], segs[(i + 1) % n]);
  // Todo curva y todo suave (una "o"): spline cerrada
  if (segs.every((s) => s.kind === "curve") && segs.every((_, i) => joint(i))) {
    const pts = segs.flatMap((s) => (s.kind === "curve" ? s.points.slice(0, -1) : []));
    return { closed: pts };
  }
  // Empezar después de un quiebre, así ninguna corrida cruza el inicio
  const brk = segs.findIndex((_, i) => !joint(i));
  const order = segs.map((_, i) => segs[(brk + 1 + i) % n]);
  const pieces: OutlinePiece[] = [];
  let run: P2[] | undefined;
  order.forEach((s, i) => {
    if (s.kind === "line") {
      pieces.push({ kind: "line", a: s.a, b: s.b });
      return;
    }
    run = run ? [...run, ...s.points.slice(1)] : [...s.points];
    const next = order[i + 1];
    if (!next || !smoothJoint(s, next)) {
      pieces.push({ kind: "spline", points: run });
      run = undefined;
    }
  });
  return { pieces };
}

/**
 * Contornos del camino de una fuente (opentype: y hacia abajo), con la línea
 * base empezando en `at` (coordenadas del sketch, y hacia arriba).
 */
export function outlineContours(commands: PathCommand[], at: P2): Contour[] {
  const map = (x = 0, y = 0): P2 => [at[0] + x, at[1] - y];
  const out: Contour[] = [];
  let segs: Segment[] = [];
  let start: P2 = at;
  let cur: P2 = at;
  const eps = 1e-9;
  const flush = () => {
    const c = contourOf(segs);
    if (c) out.push(c);
    segs = [];
  };
  for (const c of commands) {
    if (c.type === "M") {
      flush();
      start = cur = map(c.x, c.y);
    } else if (c.type === "L") {
      const p = map(c.x, c.y);
      if (len(sub(p, cur)) > eps) segs.push({ kind: "line", a: cur, b: p });
      cur = p;
    } else if (c.type === "Q") {
      const [q, p] = [map(c.x1, c.y1), map(c.x, c.y)];
      const mid: P2 = [0.25 * cur[0] + 0.5 * q[0] + 0.25 * p[0], 0.25 * cur[1] + 0.5 * q[1] + 0.25 * p[1]];
      segs.push({ kind: "curve", points: [cur, mid, p], t0: firstDir(sub(q, cur), sub(p, cur)), t1: firstDir(sub(p, q), sub(p, cur)) });
      cur = p;
    } else if (c.type === "C") {
      const [c1, c2, p] = [map(c.x1, c.y1), map(c.x2, c.y2), map(c.x, c.y)];
      const at3 = (t: number): P2 => {
        const u = 1 - t;
        const [k0, k1, k2, k3] = [u * u * u, 3 * u * u * t, 3 * u * t * t, t * t * t];
        return [k0 * cur[0] + k1 * c1[0] + k2 * c2[0] + k3 * p[0], k0 * cur[1] + k1 * c1[1] + k2 * c2[1] + k3 * p[1]];
      };
      segs.push({
        kind: "curve",
        points: [cur, at3(1 / 3), at3(2 / 3), p],
        t0: firstDir(sub(c1, cur), sub(c2, cur), sub(p, cur)),
        t1: firstDir(sub(p, c2), sub(p, c1), sub(p, cur)),
      });
      cur = p;
    } else {
      // Z: cerrar con una recta si hace falta
      if (len(sub(start, cur)) > eps) segs.push({ kind: "line", a: cur, b: start });
      cur = start;
      flush();
    }
  }
  flush();
  return out;
}
