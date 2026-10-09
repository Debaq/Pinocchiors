// Revisión del contorno de un sketch: lo que hace que una región no se cierre
// (extremos sueltos, curvas que se cruzan o se tocan sin un punto común,
// entidades encimadas), y selección por cadena y por tipo.
// Funciones puras: se prueban con node (e2e/sketchCheck.test.mjs).
//
// Las regiones (model/src/regions.rs) unen extremos por posición, descartan
// las ramas sueltas y suponen que las curvas no se cruzan: esto avisa de
// cada caso que las rompe.

import { ellipsePolyline, geometryPoints, splineOf, type Geometry, type P2, type Sketch, type SketchEntity } from "./cad.ts";

export interface SketchProblems {
  /** Extremos de una sola curva: la rama no cierra (no forma región) */
  looseEnds: { p: P2; entity: number }[];
  /** Curvas que se cruzan o se tocan sin compartir un punto */
  crossings: { p: P2; a: number; b: number }[];
  /** Entidades encimadas (la misma recta o el mismo círculo) */
  overlaps: [number, number][];
}

const dist = (a: P2, b: P2) => Math.hypot(a[0] - b[0], a[1] - b[1]);

/** Curvas que cuentan para las regiones */
const isCurve = (e: SketchEntity) => !e.construction && e.geometry.type !== "point";
/** Abiertas: tienen extremos que tienen que unirse a algo */
const isOpen = (g: Geometry) => g.type === "line" || g.type === "arc" || (g.type === "spline" && !g.closed);

/** Polilínea de una entidad (círculos y arcos muestreados por dentro) */
export function polylineOf(s: Sketch, g: Geometry): P2[] {
  const at = (id: number): P2 | undefined => {
    const p = s.points.find((q) => q.id === id);
    return p && [p.x, p.y];
  };
  switch (g.type) {
    case "line": {
      const [a, b] = [at(g.start), at(g.end)];
      return a && b ? [a, b] : [];
    }
    case "circle": {
      const c = at(g.center);
      return c ? Array.from({ length: 65 }, (_, i) => [c[0] + g.radius * Math.cos((i / 64) * 2 * Math.PI), c[1] + g.radius * Math.sin((i / 64) * 2 * Math.PI)] as P2) : [];
    }
    case "arc": {
      const [c, a, b] = [at(g.center), at(g.start), at(g.end)];
      if (!c || !a || !b) return [];
      const r = dist(a, c);
      const a0 = Math.atan2(a[1] - c[1], a[0] - c[0]);
      let sweep = Math.atan2(b[1] - c[1], b[0] - c[0]) - a0;
      while (sweep <= 1e-12) sweep += 2 * Math.PI;
      // Los extremos exactos (el muestreo los repetiría con error de redondeo)
      return [a, ...Array.from({ length: 31 }, (_, i) => [c[0] + r * Math.cos(a0 + (sweep * (i + 1)) / 32), c[1] + r * Math.sin(a0 + (sweep * (i + 1)) / 32)] as P2), b];
    }
    case "spline":
      return splineOf(g, at) ?? [];
    case "ellipse": {
      const [c, a, b] = [at(g.center), at(g.major), at(g.minor)];
      return c && a && b ? ellipsePolyline(c, a, b) : [];
    }
    default:
      return [];
  }
}

/** Cruce de dos segmentos (incluidos los extremos) */
function segCross(a: P2, b: P2, c: P2, d: P2, eps: number): P2 | undefined {
  const r: P2 = [b[0] - a[0], b[1] - a[1]];
  const q: P2 = [d[0] - c[0], d[1] - c[1]];
  const den = r[0] * q[1] - r[1] * q[0];
  if (Math.abs(den) < 1e-15) return undefined; // paralelos: los encimados se ven aparte
  const t = ((c[0] - a[0]) * q[1] - (c[1] - a[1]) * q[0]) / den;
  const u = ((c[0] - a[0]) * r[1] - (c[1] - a[1]) * r[0]) / den;
  const tl = eps / (Math.hypot(r[0], r[1]) || 1);
  const ul = eps / (Math.hypot(q[0], q[1]) || 1);
  if (t < -tl || t > 1 + tl || u < -ul || u > 1 + ul) return undefined;
  return [a[0] + t * r[0], a[1] + t * r[1]];
}

function bbox(pts: P2[]): [number, number, number, number] {
  let [x0, y0, x1, y1] = [Infinity, Infinity, -Infinity, -Infinity];
  for (const [x, y] of pts) {
    x0 = Math.min(x0, x);
    y0 = Math.min(y0, y);
    x1 = Math.max(x1, x);
    y1 = Math.max(y1, y);
  }
  return [x0, y0, x1, y1];
}

/** Si dos entidades están encimadas: misma recta con tramos que se pisan, o mismo círculo con arcos que se pisan */
function overlapping(s: Sketch, a: Geometry, b: Geometry, tol: number): boolean {
  const at = (id: number): P2 => {
    const p = s.points.find((q) => q.id === id)!;
    return [p.x, p.y];
  };
  if (a.type === "line" && b.type === "line") {
    const [p, q, r, t] = [at(a.start), at(a.end), at(b.start), at(b.end)];
    const d: P2 = [q[0] - p[0], q[1] - p[1]];
    const l = Math.hypot(d[0], d[1]);
    if (l < tol) return false;
    const off = (x: P2) => Math.abs(d[0] * (x[1] - p[1]) - d[1] * (x[0] - p[0])) / l;
    if (off(r) > tol || off(t) > tol) return false;
    const proj = (x: P2) => ((x[0] - p[0]) * d[0] + (x[1] - p[1]) * d[1]) / l;
    const [u0, u1] = [Math.min(proj(r), proj(t)), Math.max(proj(r), proj(t))];
    return Math.min(l, u1) - Math.max(0, u0) > tol;
  }
  const round = (g: Geometry) => (g.type === "circle" ? { c: at(g.center), r: g.radius, g } : g.type === "arc" ? { c: at(g.center), r: dist(at(g.start), at(g.center)), g } : undefined);
  const [ra, rb] = [round(a), round(b)];
  if (!ra || !rb || dist(ra.c, rb.c) > tol || Math.abs(ra.r - rb.r) > tol) return false;
  if (a.type === "circle" || b.type === "circle") return true;
  // Dos arcos del mismo círculo: se pisan si el medio de uno cae en el otro
  const ang = (p: P2, c: P2) => Math.atan2(p[1] - c[1], p[0] - c[0]);
  const span = (g: Extract<Geometry, { type: "arc" }>) => {
    const c = at(g.center);
    const a0 = ang(at(g.start), c);
    let sw = ang(at(g.end), c) - a0;
    while (sw <= 0) sw += 2 * Math.PI;
    return { a0, sw };
  };
  const inside = (x: number, sp: { a0: number; sw: number }) => {
    let t = x - sp.a0;
    while (t < 0) t += 2 * Math.PI;
    while (t >= 2 * Math.PI) t -= 2 * Math.PI;
    return t > 1e-9 && t < sp.sw - 1e-9;
  };
  const [sa, sb] = [span(a as Extract<Geometry, { type: "arc" }>), span(b as Extract<Geometry, { type: "arc" }>)];
  return inside(sb.a0 + sb.sw / 2, sa) || inside(sa.a0 + sa.sw / 2, sb);
}

/** Lo que impide que el sketch forme regiones */
export function checkSketch(s: Sketch): SketchProblems {
  const span = s.points.reduce((m, p) => Math.max(m, Math.abs(p.x), Math.abs(p.y)), 1);
  // La misma tolerancia que las regiones para unir extremos; un poco más para cruces
  const tolNode = span * 1e-9;
  const tol = span * 1e-7;
  const curves = s.entities.filter(isCurve);

  // Extremos: nodos por posición, como las regiones
  const nodes: { p: P2; ents: number[] }[] = [];
  const nodeOf = (p: P2, e: number) => {
    const n = nodes.find((x) => dist(x.p, p) <= tolNode);
    if (n) n.ents.push(e);
    else nodes.push({ p, ents: [e] });
  };
  const poly = new Map<number, P2[]>();
  for (const e of curves) {
    const pl = polylineOf(s, e.geometry);
    poly.set(e.id, pl);
    if (isOpen(e.geometry) && pl.length >= 2) {
      nodeOf(pl[0], e.id);
      nodeOf(pl[pl.length - 1], e.id);
    }
  }
  const looseEnds = nodes.filter((n) => n.ents.length === 1).map((n) => ({ p: n.p, entity: n.ents[0] }));

  // Encimadas
  const overlaps: [number, number][] = [];
  for (let i = 0; i < curves.length; i++)
    for (let j = i + 1; j < curves.length; j++) if (overlapping(s, curves[i].geometry, curves[j].geometry, tol)) overlaps.push([curves[i].id, curves[j].id]);
  const encimadas = new Set(overlaps.map(([a, b]) => `${a},${b}`));

  // Cruces sin punto común: no cuentan los extremos que comparten las dos
  const ends = new Map(
    curves.map((e) => {
      const pl = poly.get(e.id)!;
      return [e.id, isOpen(e.geometry) && pl.length ? [pl[0], pl[pl.length - 1]] : []] as const;
    }),
  );
  const boxes = new Map(curves.map((e) => [e.id, bbox(poly.get(e.id)!)] as const));
  const crossings: SketchProblems["crossings"] = [];
  for (let i = 0; i < curves.length; i++)
    for (let j = i + 1; j < curves.length; j++) {
      const [a, b] = [curves[i].id, curves[j].id];
      if (encimadas.has(`${a},${b}`)) continue;
      const [ba, bb] = [boxes.get(a)!, boxes.get(b)!];
      if (ba[0] > bb[2] + tol || bb[0] > ba[2] + tol || ba[1] > bb[3] + tol || bb[1] > ba[3] + tol) continue;
      const [pa, pb] = [poly.get(a)!, poly.get(b)!];
      const shared = ends.get(a)!.filter((p) => ends.get(b)!.some((q) => dist(p, q) <= tolNode));
      for (let k = 0; k + 1 < pa.length; k++)
        for (let m = 0; m + 1 < pb.length; m++) {
          const x = segCross(pa[k], pa[k + 1], pb[m], pb[m + 1], tol);
          if (!x || shared.some((p) => dist(p, x) <= tol * 10)) continue;
          if (!crossings.some((c) => dist(c.p, x) <= tol * 10)) crossings.push({ p: x, a, b });
        }
    }
  return { looseEnds, crossings, overlaps };
}

/** Resumen corto para la barra ("2 extremos sueltos · 1 cruce"), o nada si está bien */
export function problemsText(p: SketchProblems): string | undefined {
  const parts: string[] = [];
  const n = (k: number, one: string, many: string) => k && parts.push(`${k} ${k === 1 ? one : many}`);
  n(p.looseEnds.length, "extremo suelto", "extremos sueltos");
  n(p.crossings.length, "cruce sin unir", "cruces sin unir");
  n(p.overlaps.length, "entidad encimada", "entidades encimadas");
  return parts.length ? parts.join(" · ") : undefined;
}

/** Por qué un sketch no tiene regiones (para el aviso al extruir) */
export function whyNoRegions(s: Sketch): string {
  const p = checkSketch(s);
  if (!s.entities.some(isCurve)) return "El sketch no tiene curvas (la construcción no cuenta)";
  const t = problemsText(p);
  return t ? `El sketch no tiene regiones cerradas: ${t} (se marcan en rojo al editarlo)` : "El sketch no tiene regiones cerradas";
}

/** Cadena: las curvas unidas a `id` por sus extremos (con la misma marca de construcción) */
export function connectedChain(s: Sketch, id: number): number[] {
  const start = s.entities.find((e) => e.id === id);
  if (!start || start.geometry.type === "point") return start ? [id] : [];
  const span = s.points.reduce((m, p) => Math.max(m, Math.abs(p.x), Math.abs(p.y)), 1);
  const tol = span * 1e-9;
  const same = s.entities.filter((e) => e.geometry.type !== "point" && !!e.construction === !!start.construction);
  const endsOf = (e: SketchEntity): P2[] => {
    if (!isOpen(e.geometry)) return [];
    const pl = polylineOf(s, e.geometry);
    return pl.length >= 2 ? [pl[0], pl[pl.length - 1]] : [];
  };
  const ends = new Map(same.map((e) => [e.id, endsOf(e)] as const));
  const out = new Set([id]);
  const queue = [id];
  while (queue.length) {
    const cur = queue.pop()!;
    for (const p of ends.get(cur) ?? [])
      for (const e of same)
        if (!out.has(e.id) && (ends.get(e.id) ?? []).some((q) => dist(p, q) <= tol)) {
          out.add(e.id);
          queue.push(e.id);
        }
  }
  return [...out];
}

/** Tipos para elegir todo de una vez */
export type SelectKind = "lines" | "round" | "construction" | "points" | "free" | "all";

/** Todo lo de un tipo (`free`: lo que le falta definir, según el último cálculo) */
export function selectByKind(s: Sketch, kind: SelectKind, freeEntities: number[] = []): number[] {
  const pick = (f: (e: SketchEntity) => boolean) => s.entities.filter(f).map((e) => (e.geometry.type === "point" ? e.geometry.point : e.id));
  switch (kind) {
    case "lines":
      return pick((e) => e.geometry.type === "line" && !e.construction);
    case "round":
      return pick((e) => (e.geometry.type === "circle" || e.geometry.type === "arc") && !e.construction);
    case "construction":
      return pick((e) => !!e.construction);
    case "points":
      return pick((e) => e.geometry.type === "point");
    case "free":
      return freeEntities.filter((id) => s.entities.some((e) => e.id === id));
    case "all":
      return pick(() => true);
  }
}

/** Puntos que usa una entidad (para las propiedades) */
export const entityPoints = (e: SketchEntity) => geometryPoints(e.geometry);
