// Archivos del sketch: coordenadas de un CSV o TXT, importar DXF (líneas,
// arcos, círculos, polilíneas con curvatura, splines, elipses y puntos) y
// exportar a DXF y SVG. Funciones puras: se prueban con node
// (e2e/sketchFiles.test.mjs).

import { addEntity, addPoint, type Geometry, type P2, type Sketch } from "./cad.ts";
import { polylineOf } from "./sketchCheck.ts";

const sub = (a: P2, b: P2): P2 => [a[0] - b[0], a[1] - b[1]];
const len = (v: P2) => Math.hypot(v[0], v[1]);

// ─── Coordenadas ──────────────────────────────────────────────────────────

/**
 * Puntos de un archivo de coordenadas: una fila por punto con x e y (una z
 * se ignora), separados por coma, punto y coma, tabulación o espacios. Con
 * punto y coma o tabulación la coma puede ser decimal. Las filas que no son
 * números (encabezados, comentarios con #) se saltean.
 */
export function parseCoordinates(text: string): P2[] | string {
  const out: P2[] = [];
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.replace(/#.*/, "").trim();
    if (!line) continue;
    const decimalComma = /[;\t]/.test(line);
    const parts = (decimalComma ? line.split(/[;\t]+/) : line.split(/[,\s]+/)).map((x) => x.trim()).filter(Boolean);
    const nums = parts.map((x) => Number(decimalComma ? x.replace(",", ".") : x));
    if (nums.length < 2 || nums.some((v) => !Number.isFinite(v))) continue;
    out.push([nums[0], nums[1]]);
  }
  if (out.length < 2) return "El archivo no tiene al menos dos filas con x e y";
  return out;
}

// ─── Lo que se agrega al sketch ───────────────────────────────────────────

/** Forma leída de un archivo (en mm, coordenadas del sketch) */
export type Shape =
  | { kind: "line"; a: P2; b: P2 }
  | { kind: "circle"; c: P2; r: number }
  /** Arco antihorario de `a` a `b` */
  | { kind: "arc"; c: P2; a: P2; b: P2 }
  /** Spline por puntos de paso */
  | { kind: "spline"; points: P2[]; closed: boolean }
  | { kind: "bspline"; poles: P2[]; degree: number; knots?: number[]; weights?: number[] }
  /** Elipse (`start`/`end` = arco antihorario) */
  | { kind: "ellipse"; c: P2; major: P2; minor: P2; start?: P2; end?: P2 }
  | { kind: "point"; p: P2 };

/**
 * Agrega las formas al sketch con los extremos compartidos donde coinciden
 * (así las regiones se cierran) y devuelve las entidades nuevas.
 */
export function addShapes(s: Sketch, shapes: Shape[], construction = false): number[] {
  let size = 1;
  for (const sh of shapes) for (const p of shapePoints(sh)) size = Math.max(size, Math.abs(p[0]), Math.abs(p[1]));
  const tol = size * 1e-7;
  // Solo se comparten los extremos; los puntos sueltos, centros y polos no
  const ends: { p: P2; id: number }[] = [];
  const endPoint = (p: P2) => {
    const hit = ends.find((e) => len(sub(e.p, p)) <= tol);
    if (hit) return hit.id;
    const id = addPoint(s, p);
    ends.push({ p, id });
    return id;
  };
  const made: number[] = [];
  const add = (g: Geometry) => {
    const id = addEntity(s, g);
    if (construction) s.entities.find((e) => e.id === id)!.construction = true;
    made.push(id);
  };
  for (const sh of shapes) {
    switch (sh.kind) {
      case "line":
        if (len(sub(sh.a, sh.b)) > tol) add({ type: "line", start: endPoint(sh.a), end: endPoint(sh.b) });
        break;
      case "circle":
        add({ type: "circle", center: addPoint(s, sh.c), radius: sh.r });
        break;
      case "arc":
        add({ type: "arc", center: addPoint(s, sh.c), start: endPoint(sh.a), end: endPoint(sh.b) });
        break;
      case "spline": {
        const pts = sh.closed && len(sub(sh.points[0], sh.points[sh.points.length - 1])) <= tol ? sh.points.slice(0, -1) : sh.points;
        const ids = pts.map((p, i) => (!sh.closed && (i === 0 || i === pts.length - 1) ? endPoint(p) : addPoint(s, p)));
        add({ type: "spline", points: ids, closed: sh.closed });
        break;
      }
      case "bspline": {
        const n = sh.poles.length;
        const ids = sh.poles.map((p, i) => (i === 0 || i === n - 1 ? endPoint(p) : addPoint(s, p)));
        add({ type: "bspline", poles: ids, degree: sh.degree, ...(sh.knots ? { knots: sh.knots } : {}), ...(sh.weights && sh.weights.some((w) => w !== 1) ? { weights: sh.weights } : {}) });
        break;
      }
      case "ellipse": {
        const [c, a, b] = [addPoint(s, sh.c), addPoint(s, sh.major), addPoint(s, sh.minor)];
        if (sh.start && sh.end) add({ type: "ellipse_arc", center: c, major: a, minor: b, start: endPoint(sh.start), end: endPoint(sh.end) });
        else add({ type: "ellipse", center: c, major: a, minor: b });
        break;
      }
      case "point":
        add({ type: "point", point: addPoint(s, sh.p) });
        break;
    }
  }
  return made;
}

function shapePoints(sh: Shape): P2[] {
  switch (sh.kind) {
    case "line":
      return [sh.a, sh.b];
    case "circle":
      return [sh.c];
    case "arc":
      return [sh.c, sh.a, sh.b];
    case "spline":
      return sh.points;
    case "bspline":
      return sh.poles;
    case "ellipse":
      return [sh.c, sh.major, sh.minor];
    case "point":
      return [sh.p];
  }
}

// ─── DXF ──────────────────────────────────────────────────────────────────

/** mm por unidad según $INSUNITS */
const UNITS: Record<number, number> = { 0: 1, 1: 25.4, 2: 304.8, 4: 1, 5: 10, 6: 1000, 8: 0.0000254, 9: 0.0254, 10: 914.4, 13: 0.001, 14: 100 };

/**
 * Lee un DXF de texto: las entidades de dibujo de ENTITIES (las de bloques no:
 * se cuentan como salteadas), en mm según $INSUNITS. Devuelve las formas y lo
 * que no se pudo leer, o un mensaje si no es un DXF.
 */
export function parseDxf(text: string): { shapes: Shape[]; skipped: Record<string, number> } | string {
  const lines = text.split(/\r?\n/);
  const pairs: [number, string][] = [];
  for (let i = 0; i + 1 < lines.length; i += 2) {
    const code = Number(lines[i].trim());
    if (!Number.isInteger(code)) return "No parece un DXF de texto (DWG y DXF binario no se leen: guardarlo como DXF ASCII)";
    pairs.push([code, lines[i + 1].trim()]);
  }
  if (!pairs.some(([c, v]) => c === 0 && v === "SECTION")) return "No parece un DXF (no tiene secciones)";
  // Unidades
  let unit = 1;
  const iu = pairs.findIndex(([c, v]) => c === 9 && v === "$INSUNITS");
  if (iu >= 0 && pairs[iu + 1]?.[0] === 70) unit = UNITS[Number(pairs[iu + 1][1])] ?? 1;
  // Entidades: grupos que empiezan con código 0 dentro de ENTITIES
  const start = pairs.findIndex(([c, v], i) => c === 2 && v === "ENTITIES" && pairs[i - 1]?.[1] === "SECTION");
  if (start < 0) return "El DXF no tiene entidades";
  const ents: { type: string; data: [number, string][] }[] = [];
  for (let i = start + 1; i < pairs.length; i++) {
    const [c, v] = pairs[i];
    if (c === 0 && v === "ENDSEC") break;
    if (c === 0) ents.push({ type: v, data: [] });
    else ents[ents.length - 1]?.data.push([c, v]);
  }
  const shapes: Shape[] = [];
  const skipped: Record<string, number> = {};
  const skip = (t: string) => (skipped[t] = (skipped[t] ?? 0) + 1);
  const P = (x: number, y: number): P2 => [x * unit, y * unit];
  for (let k = 0; k < ents.length; k++) {
    const { type, data } = ents[k];
    const num = (code: number, d = 0) => {
      const f = data.find(([c]) => c === code);
      return f ? Number(f[1]) : d;
    };
    const all = (code: number) => data.filter(([c]) => c === code).map(([, v]) => Number(v));
    switch (type) {
      case "LINE":
        shapes.push({ kind: "line", a: P(num(10), num(20)), b: P(num(11), num(21)) });
        break;
      case "CIRCLE":
        shapes.push({ kind: "circle", c: P(num(10), num(20)), r: num(40) * unit });
        break;
      case "ARC": {
        const c = P(num(10), num(20));
        const r = num(40) * unit;
        const [a0, a1] = [(num(50) * Math.PI) / 180, (num(51) * Math.PI) / 180];
        shapes.push({ kind: "arc", c, a: [c[0] + r * Math.cos(a0), c[1] + r * Math.sin(a0)], b: [c[0] + r * Math.cos(a1), c[1] + r * Math.sin(a1)] });
        break;
      }
      case "POINT":
        shapes.push({ kind: "point", p: P(num(10), num(20)) });
        break;
      case "LWPOLYLINE": {
        // Vértices en orden: 10, 20 y la curvatura (42) del tramo que sale de cada uno
        const verts: { p: P2; bulge: number }[] = [];
        for (const [c, v] of data) {
          if (c === 10) verts.push({ p: [Number(v) * unit, 0], bulge: 0 });
          else if (c === 20 && verts.length) verts[verts.length - 1].p[1] = Number(v) * unit;
          else if (c === 42 && verts.length) verts[verts.length - 1].bulge = Number(v);
        }
        shapes.push(...polyline(verts, (num(70) & 1) === 1));
        break;
      }
      case "POLYLINE": {
        const closed = (num(70) & 1) === 1;
        const verts: { p: P2; bulge: number }[] = [];
        while (ents[k + 1]?.type === "VERTEX") {
          const d = ents[++k].data;
          const g = (code: number) => Number(d.find(([c]) => c === code)?.[1] ?? 0);
          verts.push({ p: P(g(10), g(20)), bulge: g(42) });
        }
        if (ents[k + 1]?.type === "SEQEND") k++;
        shapes.push(...polyline(verts, closed));
        break;
      }
      case "SPLINE": {
        const xs = all(10), ys = all(20);
        const poles = xs.map((x, i) => P(x, ys[i] ?? 0));
        const fx = all(11), fy = all(21);
        const fit = fx.map((x, i) => P(x, fy[i] ?? 0));
        const closed = (num(70) & 1) === 1;
        if (poles.length >= 2) {
          const knots = all(40);
          const weights = all(41);
          const degree = num(71, 3);
          shapes.push({ kind: "bspline", poles, degree, ...(knots.length === poles.length + degree + 1 ? { knots } : {}), ...(weights.length === poles.length ? { weights } : {}) });
        } else if (fit.length >= 2) shapes.push({ kind: "spline", points: fit, closed });
        else skip(type);
        break;
      }
      case "ELLIPSE": {
        const c = P(num(10), num(20));
        const m: P2 = [num(11) * unit, num(21) * unit];
        const ratio = num(40, 1);
        const [t0, t1] = [num(41, 0), num(42, 2 * Math.PI)];
        const minor: P2 = [-m[1] * ratio, m[0] * ratio];
        const at = (t: number): P2 => [c[0] + m[0] * Math.cos(t) + minor[0] * Math.sin(t), c[1] + m[1] * Math.cos(t) + minor[1] * Math.sin(t)];
        const full = Math.abs(t1 - t0 - 2 * Math.PI) < 1e-9 || Math.abs(t1 - t0) < 1e-12;
        shapes.push({ kind: "ellipse", c, major: [c[0] + m[0], c[1] + m[1]], minor: [c[0] + minor[0], c[1] + minor[1]], ...(full ? {} : { start: at(t0), end: at(t1) }) });
        break;
      }
      case "VERTEX":
      case "SEQEND":
        break;
      default:
        skip(type);
    }
  }
  return { shapes, skipped };
}

/** Tramos de una polilínea: rectos o arcos según la curvatura (bulge = tan(ángulo/4)) */
function polyline(verts: { p: P2; bulge: number }[], closed: boolean): Shape[] {
  const out: Shape[] = [];
  const n = verts.length;
  for (let i = 0; i < (closed ? n : n - 1); i++) {
    const { p: a, bulge } = verts[i];
    const b = verts[(i + 1) % n].p;
    const c = len(sub(b, a));
    if (c < 1e-12) continue;
    if (Math.abs(bulge) < 1e-12) {
      out.push({ kind: "line", a, b });
      continue;
    }
    const mid: P2 = [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2];
    const nrm: P2 = [-(b[1] - a[1]) / c, (b[0] - a[0]) / c];
    const k = ((1 - bulge * bulge) / (4 * bulge)) * c;
    const center: P2 = [mid[0] + nrm[0] * k, mid[1] + nrm[1] * k];
    out.push(bulge > 0 ? { kind: "arc", c: center, a, b } : { kind: "arc", c: center, a: b, b: a });
  }
  return out;
}

// ─── Exportar ─────────────────────────────────────────────────────────────

const num = (v: number) => (Math.abs(v) < 1e-12 ? "0" : String(+v.toFixed(6)));

/** DXF (R12, mm) del sketch: líneas, círculos y arcos como tales; lo demás, polilíneas. Capas: las del sketch y CONSTRUCCION */
export function sketchToDxf(s: Sketch): string {
  const out: string[] = [];
  const pair = (code: number, value: string | number) => out.push(String(code), typeof value === "number" ? num(value) : value);
  const at = (id: number): P2 => {
    const p = s.points.find((q) => q.id === id)!;
    return [p.x, p.y];
  };
  const layerName = (id?: number | null) => (s.layers ?? []).find((l) => l.id === id)?.name.replace(/[^\p{L}\p{N}_-]+/gu, "_") || "0";
  const layers = new Set(["0", "CONSTRUCCION", ...(s.layers ?? []).map((l) => layerName(l.id))]);
  pair(0, "SECTION");
  pair(2, "HEADER");
  pair(9, "$ACADVER");
  pair(1, "AC1009");
  pair(9, "$INSUNITS");
  pair(70, 4);
  pair(0, "ENDSEC");
  pair(0, "SECTION");
  pair(2, "TABLES");
  pair(0, "TABLE");
  pair(2, "LAYER");
  pair(70, layers.size);
  for (const l of layers) {
    pair(0, "LAYER");
    pair(2, l);
    pair(70, 0);
    pair(62, l === "CONSTRUCCION" ? 8 : 7);
    pair(6, "CONTINUOUS");
  }
  pair(0, "ENDTAB");
  pair(0, "ENDSEC");
  pair(0, "SECTION");
  pair(2, "ENTITIES");
  for (const e of s.entities) {
    const g = e.geometry;
    const layer = e.construction ? "CONSTRUCCION" : layerName(e.layer);
    const head = (type: string) => {
      pair(0, type);
      pair(8, layer);
    };
    if (g.type === "line") {
      const [a, b] = [at(g.start), at(g.end)];
      head("LINE");
      pair(10, a[0]);
      pair(20, a[1]);
      pair(11, b[0]);
      pair(21, b[1]);
    } else if (g.type === "circle") {
      const c = at(g.center);
      head("CIRCLE");
      pair(10, c[0]);
      pair(20, c[1]);
      pair(40, g.radius);
    } else if (g.type === "arc") {
      const [c, a, b] = [at(g.center), at(g.start), at(g.end)];
      head("ARC");
      pair(10, c[0]);
      pair(20, c[1]);
      pair(40, len(sub(a, c)));
      pair(50, (Math.atan2(a[1] - c[1], a[0] - c[0]) * 180) / Math.PI);
      pair(51, (Math.atan2(b[1] - c[1], b[0] - c[0]) * 180) / Math.PI);
    } else if (g.type === "point") {
      const p = at(g.point);
      head("POINT");
      pair(10, p[0]);
      pair(20, p[1]);
    } else {
      const pts = polylineOf(s, g);
      if (pts.length < 2) continue;
      head("POLYLINE");
      pair(66, 1);
      pair(10, 0);
      pair(20, 0);
      pair(70, 0);
      for (const p of pts) {
        pair(0, "VERTEX");
        pair(8, layer);
        pair(10, p[0]);
        pair(20, p[1]);
      }
      pair(0, "SEQEND");
      pair(8, layer);
    }
  }
  pair(0, "ENDSEC");
  pair(0, "EOF");
  return out.join("\n") + "\n";
}

/** SVG del sketch en mm (y hacia arriba), la construcción punteada */
export function sketchToSvg(s: Sketch): string {
  const polys = s.entities.filter((e) => e.geometry.type !== "point").map((e) => ({ e, pts: polylineOf(s, e.geometry) })).filter((x) => x.pts.length >= 2);
  const all = polys.flatMap((x) => x.pts);
  if (!all.length) all.push([0, 0]);
  const xs = all.map((p) => p[0]);
  const ys = all.map((p) => p[1]);
  const [x0, x1, y0, y1] = [Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)];
  const m = Math.max(x1 - x0, y1 - y0, 1) * 0.05;
  const [w, h] = [x1 - x0 + 2 * m, y1 - y0 + 2 * m];
  const P = (p: P2) => `${num(p[0] - x0 + m)},${num(y1 + m - p[1])}`;
  const body = polys
    .map(({ e, pts }) => `<polyline points="${pts.map(P).join(" ")}" fill="none" stroke="${e.construction ? "#888" : "#000"}" stroke-width="0.25"${e.construction ? ' stroke-dasharray="2 1.5"' : ""}/>`)
    .join("\n  ");
  return `<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="${num(w)}mm" height="${num(h)}mm" viewBox="0 0 ${num(w)} ${num(h)}">
  ${body}
</svg>
`;
}
