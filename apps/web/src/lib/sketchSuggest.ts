// Restricciones y cotas que se pueden poner con lo elegido en un sketch (las
// ofrece el panel del sketch y la barra de restricciones de arriba). Cada una
// con su texto y cómo se arma; las cotas toman lo que mide la geometría ahora.

import { geometryPoints, type Sketch, type SketchConstraint } from "./cad";
import { angleBetweenEntities, hasAngleDirection, measureConstraint } from "./sketchTransform";

export interface ConstraintSuggestion {
  label: string;
  make: () => SketchConstraint | SketchConstraint[];
}

/**
 * Lo que se puede poner con `selection` (ids de puntos y entidades, en el
 * orden en que se eligieron). `free`: puntos con grados libres (el ángulo
 * toma de referencia la entidad que ya está quieta).
 */
export function constraintSuggestions(sk: Sketch, selection: number[], free?: number[]): ConstraintSuggestion[] {
  const sketch = (): Sketch => sk;
  const entity = (id: number) => sketch().entities.find((e) => e.id === id);
  const point = (id: number) => sketch().points.find((p) => p.id === id);
  const sel = () => selection;
  const selEntities = () => sel().map(entity).filter((e) => e !== undefined);
  const selPoints = () => sel().filter((id) => point(id));
  const lines = () => selEntities().filter((e) => e!.geometry.type === "line");
  const curves = () => selEntities().filter((e) => e!.geometry.type === "circle" || e!.geometry.type === "arc");
  const lineLength = (id: number) => {
    const g = entity(id)!.geometry as { start: number; end: number };
    const [a, b] = [point(g.start)!, point(g.end)!];
    return Math.hypot(b.x - a.x, b.y - a.y);
  };  const radius = (id: number) => {
    const g = entity(id)!.geometry;
    if (g.type === "circle") return g.radius;
    if (g.type === "arc") {
      const [c, a] = [point(g.center)!, point(g.start)!];
      return Math.hypot(a.x - c.x, a.y - c.y);
    }
    return 0;
  };  const measure = (c: SketchConstraint) => +(measureConstraint(sketch(), c) ?? 0).toFixed(3);
  const isArc = (id: number) => entity(id)?.geometry.type === "arc";

  const suggestions = (): { label: string; make: () => SketchConstraint | SketchConstraint[] }[] => {
    const out: { label: string; make: () => SketchConstraint | SketchConstraint[] }[] = [];
    const L = lines().map((e) => e!.id);
    const C = curves().map((e) => e!.id);
    const P = selPoints();
    if (L.length === 1 && C.length === 0 && P.length === 0) {
      out.push({ label: "Horizontal", make: () => ({ type: "horizontal", line: L[0] }) });
      out.push({ label: "Vertical", make: () => ({ type: "vertical", line: L[0] }) });
      out.push({ label: "Largo", make: () => ({ type: "length", line: L[0], value: +lineLength(L[0]).toFixed(3) }) });
    }
    if (L.length === 2) {
      out.push({ label: "Paralelas", make: () => ({ type: "parallel", a: L[0], b: L[1] }) });
      out.push({ label: "Perpendiculares", make: () => ({ type: "perpendicular", a: L[0], b: L[1] }) });
      out.push({ label: "Colineales", make: () => ({ type: "collinear", a: L[0], b: L[1] }) });
      out.push({ label: "Mismo largo", make: () => ({ type: "equal", a: L[0], b: L[1] }) });
      // Entre paralelas: un extremo de la segunda hasta la recta de la primera (y paralelas si no lo eran)
      out.push({
        label: "Distancia entre paralelas",
        make: () => {
          const g = entity(L[1])!.geometry as { start: number };
          const c: SketchConstraint = { type: "point_line_distance", point: g.start, line: L[0], value: 0 };
          c.value = measure(c);
          const already = sketch().constraints.some((k) => k.type === "parallel" && [k.a, k.b].includes(L[0]) && [k.a, k.b].includes(L[1]));
          return already ? c : [{ type: "parallel", a: L[0], b: L[1] }, c];
        },
      });
    }
    if (C.length === 1 && L.length === 0) {
      out.push({ label: "Radio", make: () => ({ type: "radius", entity: C[0], value: +radius(C[0]).toFixed(3) }) });
      out.push({ label: "Diámetro", make: () => ({ type: "diameter", entity: C[0], value: +(2 * radius(C[0])).toFixed(3) }) });
      if (isArc(C[0])) {
        const c: SketchConstraint = { type: "arc_length", arc: C[0], value: 0 };
        out.push({ label: "Largo del arco", make: () => ({ ...c, value: measure(c) }) });
      }
    }
    if (C.length === 2) {
      out.push({ label: "Mismo radio", make: () => ({ type: "equal", a: C[0], b: C[1] }) });
      out.push({ label: "Tangentes", make: () => ({ type: "tangent", a: C[0], b: C[1] }) });
      out.push({ label: "Concéntricos", make: () => ({ type: "concentric", a: C[0], b: C[1] }) });
    }
    if (L.length === 1 && C.length === 1) out.push({ label: "Tangente", make: () => ({ type: "tangent", a: L[0], b: C[0] }) });
    if (P.length === 1 && L.length === 1) {
      out.push({ label: "Punto en la línea", make: () => ({ type: "point_on_line", point: P[0], line: L[0] }) });
      out.push({ label: "Punto medio", make: () => ({ type: "midpoint", point: P[0], line: L[0] }) });
      const d: SketchConstraint = { type: "point_line_distance", point: P[0], line: L[0], value: 0 };
      out.push({ label: "Distancia a la línea", make: () => ({ ...d, value: measure(d) }) });
      // Perfiles de revolución: el diámetro que da el punto al girar alrededor de la línea
      const ax: SketchConstraint = { type: "axis_diameter", point: P[0], line: L[0], value: 0 };
      out.push({ label: "Diámetro respecto del eje", make: () => ({ ...ax, value: measure(ax) }) });
    }
    // Con una línea central en el sketch: el diámetro de cada punto elegido sin elegir el eje
    const centerline = sketch().entities.find((e) => e.axis && e.geometry.type === "line");
    if (centerline && P.length >= 1 && L.length === 0 && C.length === 0) {
      const dims = P.map((p): SketchConstraint => {
        const c: SketchConstraint = { type: "axis_diameter", point: p, line: centerline.id, value: 0 };
        return { ...c, value: measure(c) } as SketchConstraint;
      });
      out.push({ label: P.length > 1 ? "Diámetros respecto de la línea central" : "Diámetro respecto de la línea central", make: () => dims });
    }
    if (P.length === 1 && C.length === 1) out.push({ label: "Punto en el círculo", make: () => ({ type: "point_on_circle", point: P[0], circle: C[0] }) });
    if (P.length === 2 && L.length === 0) {
      const [a, b] = [point(P[0])!, point(P[1])!];
      out.push({ label: "Coincidentes", make: () => ({ type: "coincident", a: P[0], b: P[1] }) });
      out.push({ label: "Alineados horizontal", make: () => ({ type: "horizontal_points", a: P[0], b: P[1] }) });
      out.push({ label: "Alineados vertical", make: () => ({ type: "vertical_points", a: P[0], b: P[1] }) });
      out.push({ label: "Distancia", make: () => ({ type: "distance", a: P[0], b: P[1], value: +Math.hypot(b.x - a.x, b.y - a.y).toFixed(3) }) });
      out.push({ label: "Distancia horizontal", make: () => ({ type: "horizontal_distance", a: P[0], b: P[1], value: +(b.x - a.x).toFixed(3) }) });
      out.push({ label: "Distancia vertical", make: () => ({ type: "vertical_distance", a: P[0], b: P[1], value: +(b.y - a.y).toFixed(3) }) });
    }
    if (P.length === 2 && L.length === 1) out.push({ label: "Simétricos", make: () => ({ type: "symmetric", a: P[0], b: P[1], line: L[0] }) });
    if (P.length === 1 && L.length === 0 && C.length === 0) {
      const p = point(P[0])!;
      out.push({ label: "Fijar", make: () => ({ type: "fixed", point: P[0], x: p.x, y: p.y }) });
    }
    // Ángulo entre dos entidades con dirección (líneas, arcos, elipses, puntas
    // de curvas), con lo que mide ahora: de la referencia (la primera, o la
    // que ya está quieta) a la que gira
    const D = selEntities().filter((e) => hasAngleDirection(sketch(), e!.id));
    if (D.length === 2 && D.length === selEntities().length && P.length === 0) {
      let [a, b] = [D[0]!.id, D[1]!.id];
      const moves = (id: number) => !!free && geometryPoints(entity(id)!.geometry).some((p) => free.includes(p));
      if (moves(a) && !moves(b)) [a, b] = [b, a];
      const c = angleBetweenEntities(sketch(), a, b);
      if (c) {
        out.push({ label: "Ángulo", make: () => ({ ...c }) });
        const { supplementary, ...rest } = c;
        const other: SketchConstraint = supplementary ? rest : { ...rest, supplementary: true };
        out.push({ label: "Ángulo suplementario", make: () => ({ ...other, degrees: measure(other) }) });
      }
    }
    // Cadena de líneas, arcos y círculos: su largo total
    const E = selEntities().map((e) => e!);
    const chain = E.filter((e) => ["line", "arc", "circle"].includes(e.geometry.type));
    if (chain.length >= 2 && chain.length === E.length && P.length === 0) {
      const c: SketchConstraint = { type: "curve_length", entities: chain.map((e) => e.id), value: 0 };
      out.push({ label: "Largo total", make: () => ({ ...c, value: measure(c) }) });
    }
    // Distancia mínima y máxima: un círculo o arco con otro, un punto o una línea
    const pair: [number, number] | undefined =
      C.length === 2 && L.length === 0 && P.length === 0 ? [C[0], C[1]] : C.length === 1 && L.length + P.length === 1 && E.length === L.length + 1 ? [C[0], L[0] ?? P[0]] : undefined;
    if (pair) {
      for (const max of [false, true]) {
        const c: SketchConstraint = { type: "circle_distance", a: pair[0], b: pair[1], ...(max ? { max } : {}), value: 0 };
        out.push({ label: max ? "Distancia máxima" : "Distancia mínima", make: () => ({ ...c, value: measure(c) }) });
      }
    }
    if (C.length === 2 && L.length === 0) out.push({ label: "Coradiales", make: () => ({ type: "coradial", a: C[0], b: C[1] }) });
    // Dos entidades del mismo tipo y una línea (la última elegida) como eje
    if (E.length === 3 && P.length === 0) {
      const axis = [...sel()].reverse().find((id) => entity(id)?.geometry.type === "line");
      const pairE = E.filter((e) => e.id !== axis);
      if (axis !== undefined && pairE.length === 2 && pairE[0].geometry.type === pairE[1].geometry.type)
        out.push({ label: "Simétricas (eje: la última línea)", make: () => ({ type: "symmetric_entities", a: pairE[0].id, b: pairE[1].id, line: axis }) });
    }
    // Con una spline por polos: tangencia y curvatura igual (G2) en el extremo común
    if (E.length === 2 && P.length === 0 && E.some((e) => e.geometry.type === "bspline") && E.every((e) => ["line", "arc", "bspline"].includes(e.geometry.type))) {
      out.push({ label: "Tangentes en la unión", make: () => ({ type: "tangent", a: E[0].id, b: E[1].id }) });
      out.push({ label: "Curvatura igual (G2)", make: () => ({ type: "curvature", a: E[0].id, b: E[1].id }) });
    }
    const curvesAny = E.filter((e) => e.geometry.type !== "point");
    if (P.length === 1 && curvesAny.length === 1 && E.length === 1 && ["ellipse", "spline", "bspline", "ellipse_arc"].includes(curvesAny[0].geometry.type))
      out.push({ label: "Punto en la curva", make: () => ({ type: "point_on_curve", point: P[0], curve: curvesAny[0].id }) });
    if (P.length === 1 && curvesAny.length === 2 && E.length === 2)
      out.push({ label: "Punto en la intersección", make: () => ({ type: "intersection", point: P[0], a: curvesAny[0].id, b: curvesAny[1].id }) });
    if (E.length > 0) {
      const locked = new Set(sketch().constraints.flatMap((c) => (c.type === "lock" ? [c.entity] : [])));
      const fresh = E.filter((e) => !locked.has(e.id));
      if (fresh.length) out.push({ label: fresh.length > 1 ? "Bloquear las entidades" : "Bloquear la entidad", make: () => fresh.map((e) => ({ type: "lock", entity: e.id })) });
    }
    // Varios puntos: cotas de ordenadas desde el primero o en cadena
    if (P.length >= 3 && E.length === 0) {
      for (const [axis, name] of [
        [0, "horizontales"],
        [1, "verticales"],
      ] as const) {
        out.push({ label: `Ordenadas ${name}`, make: () => ordinateDims(P, axis) });
        out.push({ label: `Cadena ${name}`, make: () => chainDims(P, axis) });
      }
    }
    return out;
  };

  /** Fila donde van los textos: un poco más allá de los puntos, del lado de afuera */
  const dimRow = (ids: number[], axis: 0 | 1) => {
    const other = ids.map((id) => (axis === 0 ? point(id)!.y : point(id)!.x));
    const span = Math.max(...ids.map((id) => (axis === 0 ? point(id)!.x : point(id)!.y))) - Math.min(...ids.map((id) => (axis === 0 ? point(id)!.x : point(id)!.y)));
    return Math.max(...other) + Math.max(2, span * 0.08);
  };
  /** Distancias desde el primer punto elegido a cada uno de los otros, con los textos en fila junto a cada punto */
  const ordinateDims = (ids: number[], axis: 0 | 1): SketchConstraint[] => {
    const row = dimRow(ids, axis);
    const type = axis === 0 ? "horizontal_distance" : "vertical_distance";
    return ids.slice(1).map((b) => {
      const p = point(b)!;
      const offset: [number, number] = axis === 0 ? [0, +(row - p.y).toFixed(4)] : [+(row - p.x).toFixed(4), 0];
      const c: SketchConstraint = { type, a: ids[0], b, value: 0, opts: { ordinate: true, offset } };
      return { ...c, value: measure(c) } as SketchConstraint;
    });
  };
  /** Cada punto desde el anterior (ordenados a lo largo del eje), con los textos en fila */
  const chainDims = (ids: number[], axis: 0 | 1): SketchConstraint[] => {
    const coord = (id: number) => (axis === 0 ? point(id)!.x : point(id)!.y);
    const sorted = [...ids].sort((a, b) => coord(a) - coord(b));
    const row = dimRow(ids, axis);
    const type = axis === 0 ? "horizontal_distance" : "vertical_distance";
    return sorted.slice(1).map((b, i) => {
      const a = sorted[i];
      const [pa, pb] = [point(a)!, point(b)!];
      const offset: [number, number] = axis === 0 ? [0, +(row - (pa.y + pb.y) / 2).toFixed(4)] : [+(row - (pa.x + pb.x) / 2).toFixed(4), 0];
      const c: SketchConstraint = { type, a, b, value: 0, opts: { offset } };
      return { ...c, value: measure(c) } as SketchConstraint;
    });
  };

  return suggestions();
}

/**
 * Botones de la barra de restricciones del sketch: cada uno pone la primera
 * de sus opciones que vale para lo elegido (en ese orden). La cota es una
 * sola herramienta: largo, ángulo, diámetro de un círculo, radio de un arco,
 * distancia…
 */
export interface ConstraintTool {
  key: string;
  label: string;
  glyph: string;
  tip: string;
  options: string[];
}
export const CONSTRAINT_TOOLS: ConstraintTool[] = [
  {
    key: "dim",
    label: "Cota",
    glyph: "↔",
    tip: "Una línea: su largo; dos líneas (o arcos, elipses): el ángulo; un círculo: el diámetro; un arco: el radio; dos puntos: la distancia; un punto y una línea: la distancia a la línea",
    options: ["Largo", "Ángulo", "Diámetro", "Radio", "Distancia", "Distancia a la línea", "Largo del arco", "Distancia entre paralelas"],
  },
  {
    key: "angle",
    label: "Ángulo",
    glyph: "∠",
    tip: "Dos líneas, arcos, elipses o puntas de curvas: el ángulo que tienen ahora; la primera es la referencia y gira la otra (Mayús+clic: el suplementario)",
    options: ["Ángulo"],
  },
  { key: "coincident", label: "Coincidente", glyph: "◉", tip: "Dos puntos juntos, o un punto sobre una línea, un círculo o una curva", options: ["Coincidentes", "Punto en la línea", "Punto en el círculo", "Punto en la curva"] },
  { key: "horizontal", label: "Horizontal", glyph: "H", tip: "Una línea horizontal, o dos puntos a la misma altura", options: ["Horizontal", "Alineados horizontal"] },
  { key: "vertical", label: "Vertical", glyph: "V", tip: "Una línea vertical, o dos puntos uno sobre otro", options: ["Vertical", "Alineados vertical"] },
  { key: "parallel", label: "Paralelas", glyph: "∥", tip: "Dos líneas paralelas", options: ["Paralelas"] },
  { key: "perpendicular", label: "Perpendiculares", glyph: "⊥", tip: "Dos líneas perpendiculares", options: ["Perpendiculares"] },
  { key: "tangent", label: "Tangente", glyph: "◠", tip: "Una línea y un arco o círculo, o dos arcos o círculos, tangentes", options: ["Tangente", "Tangentes", "Tangentes en la unión"] },
  { key: "equal", label: "Igual", glyph: "=", tip: "Dos líneas del mismo largo, o dos círculos o arcos del mismo radio", options: ["Mismo largo", "Mismo radio"] },
  { key: "midpoint", label: "Punto medio", glyph: "½", tip: "Un punto en el medio de una línea", options: ["Punto medio"] },
  { key: "concentric", label: "Concéntricos", glyph: "◎", tip: "Dos círculos o arcos con el mismo centro", options: ["Concéntricos"] },
  { key: "symmetric", label: "Simetría", glyph: "⋈", tip: "Dos puntos (o dos entidades) simétricos respecto de una línea (la última elegida)", options: ["Simétricos", "Simétricas (eje: la última línea)"] },
  { key: "fix", label: "Fijo", glyph: "⊡", tip: "Un punto fijo donde está, o entidades bloqueadas", options: ["Fijar", "Bloquear la entidad", "Bloquear las entidades"] },
];

/** La opción de `tool` que vale para lo elegido (un arco solo: el radio antes que el diámetro) */
export function pickSuggestion(sk: Sketch, selection: number[], tool: ConstraintTool, free?: number[]): ConstraintSuggestion | undefined {
  const list = constraintSuggestions(sk, selection, free);
  const only = selection.length === 1 ? sk.entities.find((e) => e.id === selection[0]) : undefined;
  const order = tool.key === "dim" && only?.geometry.type === "arc" ? ["Radio", ...tool.options.filter((o) => o !== "Radio")] : tool.options;
  for (const o of order) {
    const s = list.find((x) => x.label === o);
    if (s) return s;
  }
  return undefined;
}
