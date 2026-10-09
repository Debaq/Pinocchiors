// Íconos de las restricciones del sketch: qué símbolo lleva cada una y dónde
// va (en coordenadas del sketch) junto a lo que ata, como Onshape.
// Función pura: se prueba con node (e2e/sketchGlyphs.test.mjs).

import { constraintValue, type P2, type Sketch, type SketchConstraint } from "./cad.ts";
import { polylineOf } from "./sketchCheck.ts";

/** Símbolo de cada restricción geométrica (las cotas tienen su etiqueta; los patrones, ninguno) */
export const GLYPHS: Partial<Record<SketchConstraint["type"], string>> = {
  coincident: "◉",
  fixed: "⊡",
  horizontal: "H",
  vertical: "V",
  horizontal_points: "H",
  vertical_points: "V",
  parallel: "∥",
  perpendicular: "⊥",
  collinear: "≡",
  equal: "=",
  tangent: "◠",
  concentric: "◎",
  point_on_line: "∈",
  point_on_circle: "∈",
  midpoint: "½",
  symmetric: "⋈",
  symmetric_entities: "⋈",
  coradial: "⊚",
  point_on_curve: "∈",
  intersection: "✕",
  lock: "⊞",
  curvature: "G2",
  pierce: "⊙",
};

export interface Glyph {
  /** Índice de la restricción */
  index: number;
  glyph: string;
  /** Dónde va (coordenadas del sketch) */
  at: P2;
  /** Lo que la lleva (los íconos de una misma cosa se ponen en fila) */
  host: string;
}

/** Si la restricción es una cota (tiene valor) */
export const isDimension = (c: SketchConstraint) => constraintValue(c) !== undefined;

/** Íconos de las restricciones geométricas del sketch */
export function constraintGlyphs(s: Sketch): Glyph[] {
  const pt = new Map(s.points.map((p) => [p.id, [p.x, p.y] as P2]));
  const ent = new Map(s.entities.map((e) => [e.id, e]));
  /** Un lugar visible de la entidad: el medio de la línea o del arco, arriba a la izquierda del círculo */
  const onEntity = (id: number): P2 | undefined => {
    const e = ent.get(id);
    if (!e) return undefined;
    const g = e.geometry;
    if (g.type === "point") return pt.get(g.point);
    if (g.type === "circle") {
      const c = pt.get(g.center);
      return c && [c[0] - g.radius * Math.SQRT1_2, c[1] + g.radius * Math.SQRT1_2];
    }
    const pl = polylineOf(s, g);
    if (!pl.length) return undefined;
    if (pl.length === 2) return [(pl[0][0] + pl[1][0]) / 2, (pl[0][1] + pl[1][1]) / 2];
    return pl[Math.floor(pl.length / 2)];
  };
  const ends = (id: number): number[] => {
    const g = ent.get(id)?.geometry;
    if (g?.type === "bspline" && !g.closed) return [g.poles[0], g.poles[g.poles.length - 1]];
    return g?.type === "line" || g?.type === "arc" || g?.type === "ellipse_arc" ? [g.start, g.end] : [];
  };
  const out: Glyph[] = [];
  s.constraints.forEach((c, index) => {
    const glyph = GLYPHS[c.type];
    if (!glyph) return;
    const put = (at: P2 | undefined, host: string) => at && out.push({ index, glyph, at, host });
    const onPoint = (id: number) => put(pt.get(id), `p${id}`);
    const onEnt = (id: number) => put(onEntity(id), `e${id}`);
    switch (c.type) {
      case "horizontal":
      case "vertical":
        return void onEnt(c.line);
      case "horizontal_points":
      case "vertical_points": {
        const [a, b] = [pt.get(c.a), pt.get(c.b)];
        return void put(a && b && [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2], `p${Math.min(c.a, c.b)}-${Math.max(c.a, c.b)}`);
      }
      case "tangent":
      case "curvature": {
        // En el punto de contacto si comparten un extremo; si no, en las dos
        const shared = ends(c.a).find((p) => ends(c.b).includes(p));
        if (shared !== undefined) return void onPoint(shared);
        onEnt(c.a);
        return void onEnt(c.b);
      }
      case "parallel":
      case "perpendicular":
      case "equal":
      case "collinear":
        onEnt(c.a);
        return void onEnt(c.b);
      case "concentric":
      case "coradial": {
        const g = ent.get(c.a)?.geometry;
        return void (g && "center" in g ? onPoint(g.center) : onEnt(c.a));
      }
      case "symmetric_entities":
        onEnt(c.a);
        return void onEnt(c.b);
      case "point_on_curve":
      case "intersection":
        return void onPoint(c.point);
      case "lock":
        return void onEnt(c.entity);
      case "coincident":
        return void onPoint(c.a);
      case "fixed":
      case "point_on_line":
      case "point_on_circle":
      case "midpoint":
        return void onPoint(c.point);
      case "symmetric":
        onPoint(c.a);
        return void onPoint(c.b);
    }
  });
  return out;
}
