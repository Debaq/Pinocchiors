// Archivos del sketch: coordenadas de un CSV o TXT, importar DXF (líneas,
// arcos, círculos, polilíneas con curvatura, splines, elipses y puntos) y
// exportar a DXF y SVG. Funciones puras: se prueban con node
// (e2e/sketchFiles.test.mjs).

import { addEntity, addPoint, type Geometry, type P2, type Sketch } from "./cad.ts";
import { polylineOf } from "./sketchCheck.ts";
import { between } from "./sketchTrim.ts";
import { t as tr } from "./i18n.ts";

const sub = (a: P2, b: P2): P2 => [a[0] - b[0], a[1] - b[1]];
const len = (v: P2) => Math.hypot(v[0], v[1]);

// ─── Coordenadas ──────────────────────────────────────────────────────────

/**
 * Puntos de un archivo de coordenadas: una fila por punto con x e y (una z
 * se ignora), separados por coma, punto y coma, tabulación o espacios. Con
 * punto y coma, tabulación o espacios entre números con coma, la coma es
 * decimal. Las filas que no son
 * números (encabezados, comentarios con #) se saltean.
 */
export function parseCoordinates(text: string): P2[] | string {
  const out: P2[] = [];
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.replace(/#.*/, "").trim();
    if (!line) continue;
    // Coma decimal: con punto y coma o tabulación entre columnas, o con espacios
    // entre números que llevan coma («1,5 2,5» es (1,5; 2,5), no (1; 5))
    const spaced = line.split(/\s+/);
    const commaBySpace = spaced.length >= 2 && spaced.every((x) => /^[-+]?(\d+(,\d+)?|,\d+)(e[-+]?\d+)?$/i.test(x)) && spaced.some((x) => x.includes(","));
    const decimalComma = /[;\t]/.test(line) || commaBySpace;
    const parts = (/[;\t]/.test(line) ? line.split(/[;\t]+/) : commaBySpace ? spaced : line.split(/[,\s]+/)).map((x) => x.trim()).filter(Boolean);
    const nums = parts.map((x) => Number(decimalComma ? x.replace(",", ".") : x));
    if (nums.length < 2 || nums.some((v) => !Number.isFinite(v))) continue;
    out.push([nums[0], nums[1]]);
  }
  if (out.length < 2) return tr("El archivo no tiene al menos dos filas con x e y");
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
  /** Por polos; cerrada: periódica uniforme (los polos no se repiten) */
  | { kind: "bspline"; poles: P2[]; degree: number; knots?: number[]; weights?: number[]; closed?: boolean }
  /** Elipse (`start`/`end` = arco antihorario) */
  | { kind: "ellipse"; c: P2; major: P2; minor: P2; start?: P2; end?: P2 }
  | { kind: "point"; p: P2 };

/**
 * Agrega las formas al sketch con los extremos compartidos donde coinciden
 * (así las regiones se cierran) y devuelve las entidades nuevas.
 */
export function addShapes(s: Sketch, shapes: Shape[], construction = false): number[] {
  // Tolerancia por el tamaño del dibujo, no por lo lejos del origen: con
  // coordenadas georreferenciadas (millones de mm) eran 0,5 mm y se unían
  // extremos distintos y se borraban líneas cortas. El piso es lo que
  // distinguen los números a esa distancia.
  let [x0, y0, x1, y1, far] = [Infinity, Infinity, -Infinity, -Infinity, 0];
  for (const sh of shapes)
    for (const p of shapePoints(sh)) {
      [x0, y0, x1, y1] = [Math.min(x0, p[0]), Math.min(y0, p[1]), Math.max(x1, p[0]), Math.max(y1, p[1])];
      far = Math.max(far, Math.abs(p[0]), Math.abs(p[1]));
    }
  const extent = Number.isFinite(x0) ? Math.max(x1 - x0, y1 - y0, 1e-3) : 1;
  const tol = Math.max(extent * 1e-7, far * 1e-12);
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
        const weights = sh.weights && sh.weights.some((w) => w !== 1) ? { weights: sh.weights } : {};
        if (sh.closed) {
          add({ type: "bspline", poles: sh.poles.map((p) => addPoint(s, p)), degree: sh.degree, closed: true, ...weights });
          break;
        }
        const ids = sh.poles.map((p, i) => (i === 0 || i === n - 1 ? endPoint(p) : addPoint(s, p)));
        add({ type: "bspline", poles: ids, degree: sh.degree, ...(sh.knots ? { knots: sh.knots } : {}), ...weights });
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

/** mm por unidad según $INSUNITS (todas las de la referencia de DXF) */
const UNITS: Record<number, number> = {
  0: 1,
  1: 25.4,
  2: 304.8,
  3: 1609344,
  4: 1,
  5: 10,
  6: 1000,
  7: 1e6,
  8: 0.0000254,
  9: 0.0254,
  10: 914.4,
  11: 1e-7,
  12: 1e-6,
  13: 0.001,
  14: 100,
  15: 10000,
  16: 100000,
  17: 1e12,
  18: 1.495978707e14,
  19: 9.4607304725808e18,
  20: 3.0856775814913673e19,
  21: 304.8006096012192,
  22: 25.4000508001016,
  23: 914.4018288036576,
  24: 1609347.2186944373,
};

/** Transformación afín del plano [a, b, c, d, e, f]: (x, y) → (a·x + c·y + e, b·x + d·y + f) */
type Xf = [number, number, number, number, number, number];
const apply = (m: Xf, p: P2): P2 => [m[0] * p[0] + m[2] * p[1] + m[4], m[1] * p[0] + m[3] * p[1] + m[5]];
/** m después de n */
const compose = (m: Xf, n: Xf): Xf => [
  m[0] * n[0] + m[2] * n[1],
  m[1] * n[0] + m[3] * n[1],
  m[0] * n[2] + m[2] * n[3],
  m[1] * n[2] + m[3] * n[3],
  m[0] * n[4] + m[2] * n[5] + m[4],
  m[1] * n[4] + m[3] * n[5] + m[5],
];
const linear = (m: Xf, v: P2): P2 => [m[0] * v[0] + m[2] * v[1], m[1] * v[0] + m[3] * v[1]];
const det = (m: Xf) => m[0] * m[3] - m[1] * m[2];
/** Sin deformar (giro, escala pareja y quizás espejo): un círculo sigue siendo círculo */
const similar = (m: Xf) => {
  const s = Math.hypot(m[0], m[1]);
  return Math.abs(Math.hypot(m[2], m[3]) - s) <= 1e-9 * s && Math.abs(m[0] * m[2] + m[1] * m[3]) <= 1e-9 * s * s;
};

/** Ejes principales de la elipse c + u·cos t + v·sin t (u y v conjugados, no siempre perpendiculares) */
function principal(u: P2, v: P2): { major: P2; minor: P2 } {
  const [uu, vv, uv] = [u[0] * u[0] + u[1] * u[1], v[0] * v[0] + v[1] * v[1], u[0] * v[0] + u[1] * v[1]];
  const t = 0.5 * Math.atan2(2 * uv, uu - vv);
  const at = (a: number): P2 => [u[0] * Math.cos(a) + v[0] * Math.sin(a), u[1] * Math.cos(a) + v[1] * Math.sin(a)];
  const [p, q] = [at(t), at(t + Math.PI / 2)];
  return len(p) >= len(q) ? { major: p, minor: q } : { major: q, minor: p };
}

/** La forma llevada por `m` (un círculo deformado es elipse; un espejo invierte el sentido de los arcos) */
function transformShape(sh: Shape, m: Xf): Shape {
  const flip = det(m) < 0;
  switch (sh.kind) {
    case "line":
      return { kind: "line", a: apply(m, sh.a), b: apply(m, sh.b) };
    case "point":
      return { kind: "point", p: apply(m, sh.p) };
    case "circle": {
      const c = apply(m, sh.c);
      if (similar(m)) return { kind: "circle", c, r: sh.r * Math.sqrt(Math.abs(det(m))) };
      const { major, minor } = principal(linear(m, [sh.r, 0]), linear(m, [0, sh.r]));
      return { kind: "ellipse", c, major: [c[0] + major[0], c[1] + major[1]], minor: [c[0] + minor[0], c[1] + minor[1]] };
    }
    case "arc": {
      const [c, a, b] = [apply(m, sh.c), apply(m, sh.a), apply(m, sh.b)];
      const [from, to] = flip ? [b, a] : [a, b];
      if (similar(m)) return { kind: "arc", c, a: from, b: to };
      const r = len(sub(sh.a, sh.c));
      const { major, minor } = principal(linear(m, [r, 0]), linear(m, [0, r]));
      return { kind: "ellipse", c, major: [c[0] + major[0], c[1] + major[1]], minor: [c[0] + minor[0], c[1] + minor[1]], start: from, end: to };
    }
    case "ellipse": {
      const c = apply(m, sh.c);
      const ends = sh.start && sh.end ? (flip ? { start: apply(m, sh.end), end: apply(m, sh.start) } : { start: apply(m, sh.start), end: apply(m, sh.end) }) : {};
      // Sin deformar, los ejes siguen siendo ejes
      if (similar(m)) return { kind: "ellipse", c, major: apply(m, sh.major), minor: apply(m, sh.minor), ...ends };
      const { major, minor } = principal(linear(m, sub(sh.major, sh.c)), linear(m, sub(sh.minor, sh.c)));
      return { kind: "ellipse", c, major: [c[0] + major[0], c[1] + major[1]], minor: [c[0] + minor[0], c[1] + minor[1]], ...ends };
    }
    case "spline":
      return { ...sh, points: sh.points.map((p) => apply(m, p)) };
    case "bspline":
      return { ...sh, poles: sh.poles.map((p) => apply(m, p)) };
  }
}

/** Una forma que se puede dibujar: números finitos y tamaño (radio 0, elipse sin semieje, NaN: no) */
function usable(sh: Shape): boolean {
  if (!shapePoints(sh).every((p) => Number.isFinite(p[0]) && Number.isFinite(p[1]))) return false;
  switch (sh.kind) {
    case "circle":
      return Number.isFinite(sh.r) && sh.r > 0;
    case "arc":
      return len(sub(sh.a, sh.c)) > 0 && len(sub(sh.b, sh.c)) > 0;
    case "ellipse":
      return len(sub(sh.major, sh.c)) > 0 && len(sub(sh.minor, sh.c)) > 0;
    case "bspline":
      return sh.poles.length >= 2 && (sh.weights ?? []).every((w) => Number.isFinite(w) && w > 0);
    case "spline":
      return sh.points.length >= 2;
    default:
      return true;
  }
}

type DxfEnt = { type: string; data: [number, string][] };

/** Entidades (grupos que empiezan con código 0) entre `from` y el primer `0 <end>` */
function entitiesOf(pairs: [number, string][], from: number, end: string): { ents: DxfEnt[]; next: number } {
  const ents: DxfEnt[] = [];
  let i = from;
  for (; i < pairs.length; i++) {
    const [c, v] = pairs[i];
    if (c === 0 && v === end) break;
    if (c === 0) ents.push({ type: v, data: [] });
    else ents[ents.length - 1]?.data.push([c, v]);
  }
  return { ents, next: i };
}

/**
 * La SPLINE de un DXF: sujeta a su dominio (así empieza y termina en sus
 * polos de las puntas, que se unen con lo demás; antes los extremos iban a
 * los polos aunque la curva no pasara por ellos) y cerrada si lo es.
 */
function dxfSpline(poles: P2[], degree: number, knots: number[], weights: number[], closed: boolean): Shape | undefined {
  const n = poles.length;
  const p = Math.max(1, Math.min(degree, n - 1));
  const w = weights.length === n ? weights : undefined;
  let curve: { poles: P2[]; degree: number; knots: number[]; weights?: number[] } | undefined;
  if (knots.length === n + p + 1 && knots.every((k, i) => i === 0 || k >= knots[i - 1])) curve = { poles, degree: p, knots, ...(w ? { weights: w } : {}) };
  else if (closed && knots.length === n + 1) {
    // Periódica con un solo período de nudos: los polos dan la vuelta
    const period = knots[n] - knots[0];
    const ext = [...knots.slice(n - p, n).map((k) => k - period), ...knots, ...knots.slice(1, p + 1).map((k) => k + period)];
    const wrap = [...poles, ...poles.slice(0, p)];
    curve = { poles: wrap, degree: p, knots: ext, ...(w ? { weights: [...w, ...w.slice(0, p)] } : {}) };
  }
  if (!curve) {
    if (closed) return { kind: "bspline", poles, degree: p, closed: true, ...(w ? { weights: w } : {}) };
    return { kind: "bspline", poles, degree: p, ...(w ? { weights: w } : {}) };
  }
  const m = curve.poles.length;
  const [u0, u1] = [curve.knots[p], curve.knots[m]];
  if (!(u1 > u0)) return undefined;
  const part = between(curve, u0, u1);
  const out: Shape = { kind: "bspline", poles: part.poles, degree: part.degree, knots: part.knots, ...(part.weights ? { weights: part.weights } : {}) };
  // Cerrada pero sus puntas no se tocan (polos sin repetir y nudos de una abierta): periódica uniforme
  const [a, b] = [part.poles[0], part.poles[part.poles.length - 1]];
  if (closed && len(sub(a, b)) > 1e-9 * Math.max(1, ...part.poles.map((q) => Math.max(Math.abs(q[0]), Math.abs(q[1]))))) {
    const same = (i: number, j: number) => len(sub(poles[i], poles[j])) <= 1e-12 * Math.max(1, Math.abs(poles[i][0]), Math.abs(poles[i][1]));
    const repeated = n > 2 * p && Array.from({ length: p }, (_, i) => i).every((i) => same(i, n - p + i));
    return { kind: "bspline", poles: repeated ? poles.slice(0, n - p) : poles, degree: p, closed: true, ...(w ? { weights: repeated ? w.slice(0, n - p) : w } : {}) };
  }
  return out;
}

/**
 * Lee un DXF de texto: las entidades de dibujo de ENTITIES, con los bloques
 * insertados (INSERT, también anidados y en filas y columnas), en mm según
 * $INSUNITS. Las que están dadas en un plano dado vuelta (extrusión −Z) se
 * espejan como corresponde. Devuelve las formas y lo que no se pudo leer
 * (textos, rayados, cotas, formas sin tamaño), o un mensaje si no es un DXF.
 */
export function parseDxf(text: string): { shapes: Shape[]; skipped: Record<string, number> } | string {
  const lines = text.split(/\r?\n/);
  const pairs: [number, string][] = [];
  for (let i = 0; i + 1 < lines.length; i += 2) {
    const code = Number(lines[i].trim());
    if (!Number.isInteger(code)) return tr("No parece un DXF de texto (DWG y DXF binario no se leen: guardarlo como DXF ASCII)");
    pairs.push([code, lines[i + 1].trim()]);
  }
  if (!pairs.some(([c, v]) => c === 0 && v === "SECTION")) return tr("No parece un DXF (no tiene secciones)");
  // Unidades
  let unit = 1;
  const iu = pairs.findIndex(([c, v]) => c === 9 && v === "$INSUNITS");
  if (iu >= 0 && pairs[iu + 1]?.[0] === 70) unit = UNITS[Number(pairs[iu + 1][1])] ?? 1;
  // Bloques: nombre → punto base y entidades
  const blocks = new Map<string, { base: P2; ents: DxfEnt[] }>();
  const bs = pairs.findIndex(([c, v], i) => c === 2 && v === "BLOCKS" && pairs[i - 1]?.[1] === "SECTION");
  if (bs >= 0) {
    for (let i = bs + 1; i < pairs.length; i++) {
      const [c, v] = pairs[i];
      if (c === 0 && v === "ENDSEC") break;
      if (c !== 0 || v !== "BLOCK") continue;
      // Encabezado del bloque hasta la primera entidad
      let j = i + 1;
      let name = "";
      const base: P2 = [0, 0];
      for (; j < pairs.length && pairs[j][0] !== 0; j++) {
        const [cc, vv] = pairs[j];
        if (cc === 2) name = vv;
        else if (cc === 10) base[0] = Number(vv);
        else if (cc === 20) base[1] = Number(vv);
      }
      const { ents, next } = entitiesOf(pairs, j, "ENDBLK");
      blocks.set(name, { base, ents });
      i = next;
    }
  }
  // Entidades: grupos que empiezan con código 0 dentro de ENTITIES
  const start = pairs.findIndex(([c, v], i) => c === 2 && v === "ENTITIES" && pairs[i - 1]?.[1] === "SECTION");
  if (start < 0) return tr("El DXF no tiene entidades");
  const { ents } = entitiesOf(pairs, start + 1, "ENDSEC");
  const shapes: Shape[] = [];
  const skipped: Record<string, number> = {};
  const skip = (t: string) => (skipped[t] = (skipped[t] ?? 0) + 1);
  const read = (list: DxfEnt[], xf: Xf, depth: number) => {
    for (let k = 0; k < list.length; k++) {
      const { type, data } = list[k];
      const num = (code: number, d = 0) => {
        const f = data.find(([c]) => c === code);
        return f ? Number(f[1]) : d;
      };
      const all = (code: number) => data.filter(([c]) => c === code).map(([, v]) => Number(v));
      // Las que van en su propio plano (OCS): con la normal hacia −Z, x se espeja
      const ocs: Xf = num(230, 1) < 0 ? compose(xf, [-1, 0, 0, 1, 0, 0]) : xf;
      const push = (sh: Shape, m: Xf) => {
        const t = transformShape(sh, m);
        if (usable(t)) shapes.push(t);
        else skip(type);
      };
      switch (type) {
        case "LINE":
          push({ kind: "line", a: [num(10), num(20)], b: [num(11), num(21)] }, xf);
          break;
        case "CIRCLE":
          push({ kind: "circle", c: [num(10), num(20)], r: num(40) }, ocs);
          break;
        case "ARC": {
          const c: P2 = [num(10), num(20)];
          const r = num(40);
          const [a0, a1] = [(num(50) * Math.PI) / 180, (num(51) * Math.PI) / 180];
          push({ kind: "arc", c, a: [c[0] + r * Math.cos(a0), c[1] + r * Math.sin(a0)], b: [c[0] + r * Math.cos(a1), c[1] + r * Math.sin(a1)] }, ocs);
          break;
        }
        case "POINT":
          push({ kind: "point", p: [num(10), num(20)] }, xf);
          break;
        case "LWPOLYLINE": {
          // Vértices en orden: 10, 20 y la curvatura (42) del tramo que sale de cada uno
          const verts: { p: P2; bulge: number }[] = [];
          for (const [c, v] of data) {
            if (c === 10) verts.push({ p: [Number(v), 0], bulge: 0 });
            else if (c === 20 && verts.length) verts[verts.length - 1].p[1] = Number(v);
            else if (c === 42 && verts.length) verts[verts.length - 1].bulge = Number(v);
          }
          for (const sh of polyline(verts, (num(70) & 1) === 1)) push(sh, ocs);
          break;
        }
        case "POLYLINE": {
          const closed = (num(70) & 1) === 1;
          const verts: { p: P2; bulge: number }[] = [];
          while (list[k + 1]?.type === "VERTEX") {
            const d = list[++k].data;
            const g = (code: number) => Number(d.find(([c]) => c === code)?.[1] ?? 0);
            verts.push({ p: [g(10), g(20)], bulge: g(42) });
          }
          if (list[k + 1]?.type === "SEQEND") k++;
          for (const sh of polyline(verts, closed)) push(sh, ocs);
          break;
        }
        case "SPLINE": {
          const xs = all(10), ys = all(20);
          const poles: P2[] = xs.map((x, i) => [x, ys[i] ?? 0]);
          const fx = all(11), fy = all(21);
          const fit: P2[] = fx.map((x, i) => [x, fy[i] ?? 0]);
          const closed = (num(70) & 1) === 1;
          if (poles.length >= 2) {
            const sh = dxfSpline(poles, num(71, 3), all(40), all(41), closed);
            if (sh) push(sh, xf);
            else skip(type);
          } else if (fit.length >= 2) push({ kind: "spline", points: fit, closed }, xf);
          else skip(type);
          break;
        }
        case "ELLIPSE": {
          const c: P2 = [num(10), num(20)];
          const m: P2 = [num(11), num(21)];
          const ratio = num(40, 1);
          const [t0, t1] = [num(41, 0), num(42, 2 * Math.PI)];
          // En coordenadas del mundo; el semieje menor es normal × mayor: con la
          // normal hacia −Z va del otro lado y el parámetro gira al revés
          const down = num(230, 1) < 0;
          const minor: P2 = down ? [m[1] * ratio, -m[0] * ratio] : [-m[1] * ratio, m[0] * ratio];
          const at = (t: number): P2 => [c[0] + m[0] * Math.cos(t) + minor[0] * Math.sin(t), c[1] + m[1] * Math.cos(t) + minor[1] * Math.sin(t)];
          const full = Math.abs(t1 - t0 - 2 * Math.PI) < 1e-9 || Math.abs(t1 - t0) < 1e-12;
          // Los arcos van antihorarios: con −Z, de donde termina a donde empieza
          const ends = full ? {} : down ? { start: at(t1), end: at(t0) } : { start: at(t0), end: at(t1) };
          push({ kind: "ellipse", c, major: [c[0] + m[0], c[1] + m[1]], minor: [c[0] + minor[0], c[1] + minor[1]], ...ends }, xf);
          break;
        }
        case "INSERT": {
          // Los atributos que siguen (textos) van con el bloque
          if (num(66) === 1) {
            while (list[k + 1] && list[k + 1].type !== "SEQEND") {
              skip(list[++k].type);
            }
            if (list[k + 1]?.type === "SEQEND") k++;
          }
          const name = data.find(([c]) => c === 2)?.[1] ?? "";
          const b = blocks.get(name);
          if (!b || depth > 16) {
            skip(type);
            break;
          }
          const [sx, sy, rot] = [num(41, 1), num(42, 1), (num(50) * Math.PI) / 180];
          const [cols, rows] = [Math.max(1, num(70, 1)), Math.max(1, num(71, 1))];
          const [dc, dr] = [num(44), num(45)];
          if (!Number.isFinite(sx * sy * rot) || sx === 0 || sy === 0 || cols * rows > 10000) {
            skip(type);
            break;
          }
          const [co, si] = [Math.cos(rot), Math.sin(rot)];
          for (let i = 0; i < cols; i++)
            for (let j = 0; j < rows; j++) {
              // Lugar → giro → corrimiento de la fila/columna → escala → base del bloque al origen
              const place: Xf = [co, si, -si, co, num(10), num(20)];
              const local: Xf = [sx, 0, 0, sy, i * dc - sx * b.base[0], j * dr - sy * b.base[1]];
              read(b.ents, compose(ocs, compose(place, local)), depth + 1);
            }
          break;
        }
        case "VERTEX":
        case "SEQEND":
          break;
        default:
          skip(type);
      }
    }
  };
  read(ents, [unit, 0, 0, unit, 0, 0], 0);
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
  // R12 solo admite nombres ASCII (letras, números, _ y -): sin acentos (ñ → n) y sin repetir
  const names = new Map<number, string>();
  const taken = new Set(["0", "CONSTRUCCION"]);
  for (const l of s.layers ?? []) {
    const base = l.name.normalize("NFD").replace(/[\u0300-\u036f]/g, "").replace(/[^A-Za-z0-9_-]+/g, "_").toUpperCase().slice(0, 31) || "CAPA";
    let name = base;
    for (let k = 2; taken.has(name); k++) name = `${base}_${k}`;
    taken.add(name);
    names.set(l.id, name);
  }
  const layerName = (id?: number | null) => (id != null && names.get(id)) || "0";
  const hidden = new Set((s.layers ?? []).filter((l) => l.hidden).map((l) => names.get(l.id)!));
  const layers = [...taken];
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
  pair(70, layers.length);
  for (const l of layers) {
    pair(0, "LAYER");
    pair(2, l);
    pair(70, 0);
    // Color negativo: capa apagada (las ocultas del sketch)
    const color = l === "CONSTRUCCION" ? 8 : 7;
    pair(62, hidden.has(l) ? -color : color);
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
    } else if (g.type === "arc" && len(sub(at(g.start), at(g.end))) <= 1e-9 * Math.max(1, len(sub(at(g.start), at(g.center))))) {
      // Vuelta entera (inicio = fin): un ARC de 0° a 0° no se dibuja en ningún lado
      const [c, a] = [at(g.center), at(g.start)];
      head("CIRCLE");
      pair(10, c[0]);
      pair(20, c[1]);
      pair(40, len(sub(a, c)));
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
