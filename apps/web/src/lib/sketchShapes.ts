// Formas del sketch que arman varias entidades con sus restricciones:
// rectángulo por 3 puntos, polígono circunscrito, ranura en arco y por el
// centro, y la geometría de los círculos por 2 y 3 puntos y tangente a 3.
// Funciones puras sobre `Sketch`: se prueban con node (e2e/sketchShapes.test.mjs).

import { addEntity, addPoint, type P2, type Sketch } from "./cad.ts";

const sub = (a: P2, b: P2): P2 => [a[0] - b[0], a[1] - b[1]];
const dist = (a: P2, b: P2) => Math.hypot(a[0] - b[0], a[1] - b[1]);
const pos = (s: Sketch, id: number): P2 => {
  const p = s.points.find((q) => q.id === id)!;
  return [p.x, p.y];
};

/** Círculo que pasa por tres puntos (no alineados) */
export function circumcircle(a: P2, b: P2, c: P2): { c: P2; r: number } | undefined {
  const d = 2 * (a[0] * (b[1] - c[1]) + b[0] * (c[1] - a[1]) + c[0] * (a[1] - b[1]));
  const scale = Math.max(dist(a, b), dist(b, c), dist(c, a));
  if (Math.abs(d) < 1e-12 * scale * scale) return undefined;
  const [a2, b2, c2] = [a[0] ** 2 + a[1] ** 2, b[0] ** 2 + b[1] ** 2, c[0] ** 2 + c[1] ** 2];
  const center: P2 = [(a2 * (b[1] - c[1]) + b2 * (c[1] - a[1]) + c2 * (a[1] - b[1])) / d, (a2 * (c[0] - b[0]) + b2 * (a[0] - c[0]) + c2 * (b[0] - a[0])) / d];
  return { c: center, r: dist(center, a) };
}

/** Punto de una entidad (línea, círculo o arco) más cercano a `p` */
export function nearestOnEntity(s: Sketch, id: number, p: P2): P2 | undefined {
  const g = s.entities.find((e) => e.id === id)?.geometry;
  if (g?.type === "line") {
    const [a, b] = [pos(s, g.start), pos(s, g.end)];
    const d = sub(b, a);
    const t = ((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / (d[0] * d[0] + d[1] * d[1] || 1);
    return [a[0] + t * d[0], a[1] + t * d[1]];
  }
  if (g?.type === "circle" || g?.type === "arc") {
    const c = pos(s, g.center);
    const r = g.type === "circle" ? g.radius : dist(c, pos(s, g.start));
    const v = sub(p, c);
    const l = Math.hypot(v[0], v[1]) || 1;
    return [c[0] + (v[0] / l) * r, c[1] + (v[1] / l) * r];
  }
  return undefined;
}

/**
 * Rectángulo por 3 puntos: `a`–`b` es un lado y `c` da el ancho (su
 * distancia a la recta ab, del lado donde está). Devuelve las 4 esquinas en
 * orden o nada si es plano.
 */
export function rectFrom3(a: P2, b: P2, c: P2): [P2, P2, P2, P2] | undefined {
  const d = sub(b, a);
  const l = Math.hypot(d[0], d[1]);
  if (l < 1e-12) return undefined;
  const n: P2 = [-d[1] / l, d[0] / l];
  const w = (c[0] - a[0]) * n[0] + (c[1] - a[1]) * n[1];
  if (Math.abs(w) < 1e-12 * Math.max(1, l)) return undefined;
  return [a, b, [b[0] + n[0] * w, b[1] + n[1] * w], [a[0] + n[0] * w, a[1] + n[1] * w]];
}

/**
 * Rectángulo inclinado sobre las esquinas `pa`, `pb` (puntos ya puestos) y
 * dos nuevas en `c`, `d`: lados perpendicular el primero al segundo y
 * paralelos los opuestos. Devuelve las 4 líneas (ab, bc, cd, da).
 */
export function addRect3(s: Sketch, pa: number, pb: number, c: P2, d: P2): number[] {
  const pc = addPoint(s, c);
  const pd = addPoint(s, d);
  const l = [
    [pa, pb],
    [pb, pc],
    [pc, pd],
    [pd, pa],
  ].map(([start, end]) => addEntity(s, { type: "line", start, end }));
  s.constraints.push(
    { type: "perpendicular", a: l[0], b: l[1] },
    { type: "parallel", a: l[0], b: l[2] },
    { type: "parallel", a: l[1], b: l[3] },
  );
  return l;
}

/**
 * Vértices de un polígono regular circunscrito a un círculo de centro `c`,
 * con `m` el punto medio del primer lado (el radio es la apotema)
 */
export function circumscribedVertices(c: P2, m: P2, n: number): P2[] {
  const apothem = dist(c, m);
  const r = apothem / Math.cos(Math.PI / n);
  const a0 = Math.atan2(m[1] - c[1], m[0] - c[0]) - Math.PI / n;
  return Array.from({ length: n }, (_, i) => [c[0] + r * Math.cos(a0 + (2 * Math.PI * i) / n), c[1] + r * Math.sin(a0 + (2 * Math.PI * i) / n)] as P2);
}

/**
 * Polígono regular circunscrito: círculo de construcción de centro `center`
 * (punto ya puesto) y radio `apothem`, lados tangentes a él e iguales.
 * Devuelve el círculo y las líneas.
 */
export function addCircumscribedPolygon(s: Sketch, center: number, m: P2, n: number): { circle: number; lines: number[] } {
  const c = pos(s, center);
  const circle = addEntity(s, { type: "circle", center, radius: dist(c, m) });
  s.entities.find((e) => e.id === circle)!.construction = true;
  const pts = circumscribedVertices(c, m, n).map((p) => addPoint(s, p));
  const lines = pts.map((p, i) => addEntity(s, { type: "line", start: p, end: pts[(i + 1) % n] }));
  for (const l of lines) s.constraints.push({ type: "tangent", a: l, b: circle });
  for (let i = 1; i < n; i++) s.constraints.push({ type: "equal", a: lines[0], b: lines[i] });
  return { circle, lines };
}

/** Barrido antihorario (0, 2π] de `a` a `b` alrededor de `c` */
function ccwSweep(c: P2, a: P2, b: P2): number {
  let t = Math.atan2(b[1] - c[1], b[0] - c[0]) - Math.atan2(a[1] - c[1], a[0] - c[0]);
  while (t <= 0) t += 2 * Math.PI;
  while (t > 2 * Math.PI) t -= 2 * Math.PI;
  return t;
}

/**
 * Ranura en arco: el eje es un arco de construcción de centro `center` de
 * `start` a `end` (puntos ya puestos, antihorario) y `w` el medio ancho.
 * Arcos interior y exterior concéntricos, tapas de radio `w` en los
 * extremos, tangentes, y tapas iguales. Devuelve las entidades.
 */
export function addArcSlot(s: Sketch, center: number, start: number, end: number, w: number): { axis: number; outer: number; inner: number; capStart: number; capEnd: number } | string {
  const [c, a, b] = [pos(s, center), pos(s, start), pos(s, end)];
  const R = dist(c, a);
  if (w <= 0 || w >= R) return "El ancho tiene que ser menor que el radio del arco";
  const u = (p: P2): P2 => {
    const v = sub(p, c);
    const l = Math.hypot(v[0], v[1]) || 1;
    return [v[0] / l, v[1] / l];
  };
  const [ua, ub] = [u(a), u(b)];
  const at = (p: P2, d: P2, k: number): P2 => [p[0] + d[0] * k, p[1] + d[1] * k];
  const oS = addPoint(s, at(a, ua, w));
  const iS = addPoint(s, at(a, ua, -w));
  // El fin queda en el radio del comienzo (la ranura sigue al arco)
  const bR = at(c, ub, R);
  const oE = addPoint(s, at(bR, ub, w));
  const iE = addPoint(s, at(bR, ub, -w));
  const axis = addEntity(s, { type: "arc", center, start, end });
  s.entities.find((e) => e.id === axis)!.construction = true;
  const outer = addEntity(s, { type: "arc", center, start: oS, end: oE });
  const inner = addEntity(s, { type: "arc", center, start: iS, end: iE });
  // Tapas antihorarias hacia afuera de la ranura: en el fin de afuera hacia adentro, en el comienzo al revés
  const capEnd = addEntity(s, { type: "arc", center: end, start: oE, end: iE });
  const capStart = addEntity(s, { type: "arc", center: start, start: iS, end: oS });
  s.constraints.push(
    { type: "tangent", a: capEnd, b: outer },
    { type: "tangent", a: capEnd, b: inner },
    { type: "tangent", a: capStart, b: outer },
    { type: "tangent", a: capStart, b: inner },
    { type: "equal", a: capStart, b: capEnd },
  );
  return { axis, outer, inner, capStart, capEnd };
}

/** Si el arco de `a` a `b` alrededor de `c` que pasa cerca de `via` va antihorario */
export function ccwThrough(c: P2, a: P2, b: P2, via: P2): boolean {
  return ccwSweep(c, a, via) < ccwSweep(c, a, b);
}

/** Contorno de una ranura en arco para la vista previa */
export function arcSlotOutline(c: P2, a: P2, sweep: number, w: number): P2[] {
  const R = dist(c, a);
  const a0 = Math.atan2(a[1] - c[1], a[0] - c[0]);
  const out: P2[] = [];
  const n = 32;
  const P = (r: number, t: number): P2 => [c[0] + r * Math.cos(t), c[1] + r * Math.sin(t)];
  for (let i = 0; i <= n; i++) out.push(P(R + w, a0 + (sweep * i) / n));
  const e = P(R, a0 + sweep);
  const te = a0 + sweep;
  for (let i = 0; i <= 16; i++) {
    const t = te + (Math.PI * i) / 16;
    out.push([e[0] + w * Math.cos(t), e[1] + w * Math.sin(t)]);
  }
  for (let i = n; i >= 0; i--) out.push(P(R - w, a0 + (sweep * i) / n));
  for (let i = 0; i <= 16; i++) {
    const t = a0 + Math.PI + (Math.PI * i) / 16;
    out.push([a[0] + w * Math.cos(t), a[1] + w * Math.sin(t)]);
  }
  return out;
}

/**
 * Ranura recta entre los centros `ca` y `cb` (puntos ya puestos) con medio
 * ancho `r`: dos líneas y dos arcos tangentes, arcos iguales.
 */
export function addSlot(s: Sketch, ca: number, cb: number, r: number): { bottom: number; arcB: number; top: number; arcA: number } {
  const [a, b] = [pos(s, ca), pos(s, cb)];
  const len = dist(a, b) || 1;
  const n: P2 = [-(b[1] - a[1]) / len, (b[0] - a[0]) / len];
  const off = (p: P2, k: number): P2 => [p[0] + n[0] * r * k, p[1] + n[1] * r * k];
  const a1 = addPoint(s, off(a, 1));
  const a2 = addPoint(s, off(a, -1));
  const b1 = addPoint(s, off(b, 1));
  const b2 = addPoint(s, off(b, -1));
  const bottom = addEntity(s, { type: "line", start: a2, end: b2 });
  const arcB = addEntity(s, { type: "arc", center: cb, start: b2, end: b1 });
  const top = addEntity(s, { type: "line", start: b1, end: a1 });
  const arcA = addEntity(s, { type: "arc", center: ca, start: a1, end: a2 });
  s.constraints.push(
    { type: "tangent", a: bottom, b: arcB },
    { type: "tangent", a: top, b: arcB },
    { type: "tangent", a: top, b: arcA },
    { type: "tangent", a: bottom, b: arcA },
    { type: "equal", a: arcA, b: arcB },
  );
  return { bottom, arcB, top, arcA };
}
