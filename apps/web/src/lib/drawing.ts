// Planos 2D: vistas proyectadas del diseño sobre una hoja con cajetín, en SVG.
// La proyección (líneas ocultas exactas) la hace el backend (`cad_drawing`);
// acá se acomodan las vistas, se elige la escala y se arma el SVG (en mm).

export type P2 = [number, number];
export type P3 = [number, number, number];

export type LineKind = "visible" | "outline" | "hidden" | "hidden_outline" | "smooth";

export interface DrawingLine {
  kind: LineKind;
  points: P2[];
}

export type ViewName = "front" | "top" | "side" | "iso";

/** Desde dónde se mira cada vista (hacia quien mira) y la derecha de la hoja; Z arriba */
export function viewSpec(name: ViewName, projection: Projection): { eye: P3; xdir: P3 } {
  const s = Math.SQRT1_2;
  const t = 1 / Math.sqrt(3);
  switch (name) {
    case "front":
      return { eye: [0, -1, 0], xdir: [1, 0, 0] };
    case "top":
      return { eye: [0, 0, 1], xdir: [1, 0, 0] };
    // Primer diedro: la vista desde la izquierda va a la derecha del frente;
    // tercer diedro: la de la derecha, a la derecha
    case "side":
      return projection === "first" ? { eye: [-1, 0, 0], xdir: [0, -1, 0] } : { eye: [1, 0, 0], xdir: [0, 1, 0] };
    case "iso":
      return { eye: [t, -t, t], xdir: [s, s, 0] };
  }
}

/** Primer diedro (ISO, Europa y Latinoamérica) o tercer diedro (ANSI) */
export type Projection = "first" | "third";

export interface SheetSize {
  name: string;
  /** Apaisada, en mm */
  width: number;
  height: number;
}

export const SHEETS: SheetSize[] = [
  { name: "A4", width: 297, height: 210 },
  { name: "A3", width: 420, height: 297 },
  { name: "A2", width: 594, height: 420 },
  { name: "Carta", width: 279.4, height: 215.9 },
];

/** Escalas normalizadas (ISO 5455), de mayor a menor */
export const SCALES: number[] = [10, 5, 2, 1, 1 / 2, 1 / 5, 1 / 10, 1 / 20, 1 / 50, 1 / 100, 1 / 200];

export function scaleLabel(s: number): string {
  return s >= 1 ? `${s}:1` : `1:${Math.round(1 / s)}`;
}

export interface Bounds {
  min: P2;
  max: P2;
}

export function bounds(lines: DrawingLine[]): Bounds {
  const min: P2 = [Infinity, Infinity];
  const max: P2 = [-Infinity, -Infinity];
  for (const l of lines)
    for (const p of l.points) {
      min[0] = Math.min(min[0], p[0]);
      min[1] = Math.min(min[1], p[1]);
      max[0] = Math.max(max[0], p[0]);
      max[1] = Math.max(max[1], p[1]);
    }
  if (!isFinite(min[0])) return { min: [0, 0], max: [0, 0] };
  return { min, max };
}

const size = (b: Bounds): P2 => [b.max[0] - b.min[0], b.max[1] - b.min[1]];

/** Margen del recuadro y alto del cajetín (mm) */
export const MARGIN = 10;
export const TITLE_H = 28;
export const TITLE_W = 130;
/** Separación entre vistas (mm de hoja) */
const GAP = 18;

export interface Placed {
  /** La vista ("front"…) o un detalle ("detalle-B") */
  name: string;
  /** Traslado en la hoja (mm) del origen de la vista y escala */
  x: number;
  y: number;
  scale: number;
  lines: DrawingLine[];
  /** Triángulos de la cara cortada (vista en corte), para rayar */
  hatch?: [P2, P2, P2][];
  /** Vista de detalle: de qué vista sale y qué círculo amplía */
  detail?: Detail;
}

/**
 * Vista de detalle: un círculo de una vista (centro en coordenadas de la
 * vista, radio en mm de la pieza) ampliado aparte. `scale` es la escala del
 * detalle en la hoja (sin ella, el doble de la de la hoja, normalizada).
 */
export interface Detail {
  label: string;
  view: ViewName;
  center: P2;
  radius: number;
  scale?: number;
}

/** Corte A-A: el frente cortado por un plano paralelo a él; `at` es la Y del plano (mm del modelo) */
export interface SectionInfo {
  label: string;
  at: number;
}

/**
 * Acomoda frente, planta y lateral en cruz (planta debajo del frente en primer
 * diedro, arriba en tercero; lateral a la derecha) y la isométrica arriba a la
 * derecha. Elige la escala normalizada más grande con la que todo entra.
 * Coordenadas de hoja: x a la derecha, y hacia abajo (como SVG).
 */
export function layout(
  views: Partial<Record<ViewName, DrawingLine[]>>,
  sheet: SheetSize,
  projection: Projection,
  fixedScale?: number,
  hatch?: Partial<Record<ViewName, [P2, P2, P2][]>>,
): { placed: Placed[]; scale: number } {
  const b = Object.fromEntries(Object.entries(views).map(([k, v]) => [k, bounds(v!)])) as Record<ViewName, Bounds>;
  const sz = (n: ViewName): P2 => (views[n] ? size(b[n]) : [0, 0]);
  const [fw, fh] = sz("front");
  const [tw, th] = sz("top");
  const [sw, sh] = sz("side");
  const [iw, ih] = sz("iso");
  const areaW = sheet.width - 2 * MARGIN;
  const areaH = sheet.height - 2 * MARGIN - TITLE_H;
  const fits = (s: number) => {
    const w = Math.max(fw, tw) * s + (views.side ? GAP + sw * s : 0) + (views.iso ? GAP + iw * s : 0) + 2 * GAP;
    const h = Math.max(Math.max(fh, sh) * s + (views.top ? GAP + th * s : 0), views.iso ? ih * s : 0) + 2 * GAP;
    return w <= areaW && h <= areaH;
  };
  const scale = fixedScale ?? SCALES.find(fits) ?? SCALES[SCALES.length - 1];
  // Columna de frente y planta; la lateral al lado del frente; la isométrica al final
  const left = MARGIN + GAP;
  const colW = Math.max(fw, tw) * scale;
  const top = MARGIN + GAP;
  const frontY = projection === "first" || !views.top ? top : top + th * scale + GAP;
  const topY = projection === "first" ? top + fh * scale + GAP : top;
  const placed: Placed[] = [];
  // En la hoja y crece hacia abajo: el punto (u, v) de la vista va a (x + u·s, y − v·s)
  const put = (name: ViewName, x0: number, y0: number, w: number) => {
    const bb = b[name];
    placed.push({ name, scale, lines: views[name]!, hatch: hatch?.[name], x: x0 + (w - (bb.max[0] - bb.min[0]) * scale) / 2 - bb.min[0] * scale, y: y0 + bb.max[1] * scale });
  };
  if (views.front) put("front", left, frontY, colW);
  if (views.top) put("top", left, topY, colW);
  if (views.side) put("side", left + colW + GAP, frontY, sw * scale);
  if (views.iso) put("iso", left + colW + (views.side ? GAP + sw * scale : 0) + GAP, top, iw * scale);
  return { placed, scale };
}

export interface TitleBlock {
  title: string;
  author?: string;
  material?: string;
  date: string;
  scale: string;
  projection: Projection;
  sheet: string;
}

const STYLE: Record<LineKind, string> = {
  visible: 'stroke="#000" stroke-width="0.5"',
  outline: 'stroke="#000" stroke-width="0.5"',
  hidden: 'stroke="#000" stroke-width="0.25" stroke-dasharray="3 1.5"',
  hidden_outline: 'stroke="#000" stroke-width="0.25" stroke-dasharray="3 1.5"',
  smooth: 'stroke="#888" stroke-width="0.18"',
};

const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
const n = (v: number) => (Math.round(v * 1000) / 1000).toString();

/** Cota en la hoja: de `a` a `b` (mm de hoja), corrida `offset` hacia afuera, con su valor en mm de la pieza */
export interface Dimension {
  kind?: "linear";
  a: P2;
  b: P2;
  /** Hacia dónde se corre la línea de cota (perpendicular, en mm de hoja) */
  offset: P2;
  value: number;
  /** "Ø" en diámetros */
  prefix?: string;
}

/** Cota de ángulo: arco de radio `r` con centro `c` entre los ángulos `a0` y `a1` (radianes, coordenadas de hoja) */
export interface AngleDimension {
  kind: "angle";
  c: P2;
  r: number;
  a0: number;
  a1: number;
  /** Grados */
  value: number;
  /** Extremos de las rectas acotadas (para las líneas de referencia hasta el arco) */
  ends: [P2, P2];
}

/** Cota de radio: flecha del centro `c` al arco en `p` */
export interface RadiusDimension {
  kind: "radius";
  c: P2;
  p: P2;
  value: number;
}

export type AnyDimension = Dimension | AngleDimension | RadiusDimension;

/**
 * Cota puesta por el usuario sobre líneas de una vista. Las líneas se
 * recuerdan por un punto sobre ellas (coordenadas de la vista, mm de la
 * pieza): al cambiar el modelo se vuelve a buscar la línea más cercana y el
 * valor se actualiza.
 */
export interface UserDim {
  /** La vista o el detalle ("detalle-B") */
  view: string;
  /**
   * "length": largo de una recta; "diameter": de un círculo; "radius": de un
   * arco; "distance": entre dos rectas paralelas; "angle": entre dos rectas
   */
  kind: "length" | "diameter" | "distance" | "radius" | "angle";
  refs: P2[];
}

const SOLID_KINDS: LineKind[] = ["visible", "outline"];

function distToPolyline(p: P2, pts: P2[]): number {
  let best = Infinity;
  for (let i = 0; i + 1 < pts.length; i++) {
    const [a, b] = [pts[i], pts[i + 1]];
    const [dx, dy] = [b[0] - a[0], b[1] - a[1]];
    const l2 = dx * dx + dy * dy;
    const t = l2 < 1e-18 ? 0 : Math.max(0, Math.min(1, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2));
    best = Math.min(best, Math.hypot(p[0] - a[0] - t * dx, p[1] - a[1] - t * dy));
  }
  return best;
}

/** Línea visible de la vista más cercana a `p` (coordenadas de la vista), si está a menos de `tol` */
export function lineNear(v: Placed, p: P2, tol: number, accept: (l: DrawingLine) => boolean = () => true): DrawingLine | undefined {
  let best: { d: number; l: DrawingLine } | undefined;
  for (const l of v.lines) {
    if (!SOLID_KINDS.includes(l.kind) || !accept(l)) continue;
    const d = distToPolyline(p, l.points);
    if (d <= tol && (!best || d < best.d)) best = { d, l };
  }
  return best?.l;
}

/** ¿Es una circunferencia cerrada? Centro y radio (ajuste por promedio) */
export function asCircle(l: DrawingLine): { c: P2; r: number } | null {
  const pts = l.points;
  if (pts.length < 8) return null;
  const [f, z] = [pts[0], pts[pts.length - 1]];
  const span = Math.max(...pts.map((p) => Math.hypot(p[0] - f[0], p[1] - f[1])));
  if (Math.hypot(f[0] - z[0], f[1] - z[1]) > span * 0.02) return null;
  // Sin el último punto (repite el primero) para no sesgar el centro
  const ring = pts.slice(0, -1);
  const c: P2 = [ring.reduce((a, p) => a + p[0], 0) / ring.length, ring.reduce((a, p) => a + p[1], 0) / ring.length];
  const rs = ring.map((p) => Math.hypot(p[0] - c[0], p[1] - c[1]));
  const r = rs.reduce((a, x) => a + x, 0) / rs.length;
  return rs.every((x) => Math.abs(x - r) < r * 0.03) ? { c, r } : null;
}

/** ¿Es un arco (abierto)? Centro, radio, ángulos de los extremos y punto medio */
export function asArc(l: DrawingLine): { c: P2; r: number; mid: P2 } | null {
  const pts = l.points;
  if (pts.length < 4 || asCircle(l) || asSegment(l)) return null;
  const [a, m, b] = [pts[0], pts[Math.floor(pts.length / 2)], pts[pts.length - 1]];
  // Circunferencia por tres puntos
  const d = 2 * (a[0] * (m[1] - b[1]) + m[0] * (b[1] - a[1]) + b[0] * (a[1] - m[1]));
  if (Math.abs(d) < 1e-12) return null;
  const sq = (p: P2) => p[0] * p[0] + p[1] * p[1];
  const c: P2 = [
    (sq(a) * (m[1] - b[1]) + sq(m) * (b[1] - a[1]) + sq(b) * (a[1] - m[1])) / d,
    (sq(a) * (b[0] - m[0]) + sq(m) * (a[0] - b[0]) + sq(b) * (m[0] - a[0])) / d,
  ];
  const r = Math.hypot(a[0] - c[0], a[1] - c[1]);
  if (!pts.every((p) => Math.abs(Math.hypot(p[0] - c[0], p[1] - c[1]) - r) < r * 0.02)) return null;
  return { c, r, mid: m };
}

/** ¿Es una recta? Sus extremos */
export function asSegment(l: DrawingLine): [P2, P2] | null {
  const pts = l.points;
  const [a, b] = [pts[0], pts[pts.length - 1]];
  const len = Math.hypot(b[0] - a[0], b[1] - a[1]);
  if (len < 1e-9) return null;
  return distToPolyline(a, [a, b]) <= len * 1e-3 && pts.every((p) => distToPolyline(p, [a, b]) <= len * 1e-3) ? [a, b] : null;
}

/** Cotas del usuario resueltas contra las vistas actuales (las que ya no encuentran su línea se omiten) */
export function userDimensions(placed: Placed[], dims: UserDim[]): AnyDimension[] {
  const out: AnyDimension[] = [];
  for (const d of dims) {
    const v = placed.find((x) => x.name === d.view);
    if (!v) continue;
    const tol = 3 / v.scale;
    const sheet = (p: P2): P2 => [v.x + p[0] * v.scale, v.y - p[1] * v.scale];
    const b = bounds(v.lines);
    const center: P2 = [(b.min[0] + b.max[0]) / 2, (b.min[1] + b.max[1]) / 2];
    // Primero donde se hizo clic; si el modelo cambió, la más cercana a una
    // distancia razonable (la arista se movió)
    const far = Math.hypot(b.max[0] - b.min[0], b.max[1] - b.min[1]) * 0.15;
    const shape =
      d.kind === "diameter" ? (l: DrawingLine) => !!asCircle(l) : d.kind === "radius" ? (l: DrawingLine) => !!asArc(l) : (l: DrawingLine) => !!asSegment(l);
    const lines = d.refs.map((r) => lineNear(v, r, tol, shape) ?? lineNear(v, r, far, shape));
    if (lines.some((l) => !l)) continue;
    if (d.kind === "radius") {
      const arc = asArc(lines[0]!);
      if (!arc) continue;
      // La flecha a 45° (arriba a la derecha o a la izquierda…) si el arco
      // pasa por ahí: por el medio del arco solía pisar rótulos
      const pts = lines[0]!.points;
      const toward = (deg: number) => {
        const t = (deg * Math.PI) / 180;
        const best = pts.reduce((a, p) => {
          const q = Math.atan2(p[1] - arc.c[1], p[0] - arc.c[0]);
          const diff = Math.abs(Math.atan2(Math.sin(q - t), Math.cos(q - t)));
          return diff < a.diff ? { p, diff } : a;
        }, { p: arc.mid, diff: Infinity });
        return best.diff < 0.2 ? best.p : undefined;
      };
      const at = toward(45) ?? toward(135) ?? toward(-45) ?? toward(-135) ?? arc.mid;
      out.push({ kind: "radius", c: sheet(arc.c), p: sheet([arc.c[0] + ((at[0] - arc.c[0]) * arc.r) / Math.hypot(at[0] - arc.c[0], at[1] - arc.c[1]), arc.c[1] + ((at[1] - arc.c[1]) * arc.r) / Math.hypot(at[0] - arc.c[0], at[1] - arc.c[1])]), value: arc.r });
    } else if (d.kind === "angle") {
      const [s1, s2] = [asSegment(lines[0]!), asSegment(lines[1]!)];
      if (!s1 || !s2) continue;
      const a = angleBetween(s1, s2, d.refs[0], d.refs[1]);
      if (!a) continue;
      // En la hoja Y va hacia abajo: los ángulos cambian de signo
      const c = sheet(a.x);
      const r = Math.max(8, a.reach * v.scale);
      out.push({ kind: "angle", c, r, a0: -a.t0, a1: -a.t1, value: a.degrees, ends: [sheet(a.far0), sheet(a.far1)] });
    } else if (d.kind === "diameter") {
      const circ = asCircle(lines[0]!);
      if (!circ) continue;
      out.push({ a: sheet([circ.c[0] - circ.r, circ.c[1]]), b: sheet([circ.c[0] + circ.r, circ.c[1]]), offset: [0, 0], value: 2 * circ.r, prefix: "Ø" });
    } else if (d.kind === "length") {
      const seg = asSegment(lines[0]!);
      if (!seg) continue;
      const [p, q] = seg;
      // Hacia afuera de la vista: del lado opuesto al centro
      const len = Math.hypot(q[0] - p[0], q[1] - p[1]);
      let n: P2 = [-(q[1] - p[1]) / len, (q[0] - p[0]) / len];
      const mid: P2 = [(p[0] + q[0]) / 2, (p[1] + q[1]) / 2];
      if ((mid[0] - center[0]) * n[0] + (mid[1] - center[1]) * n[1] < 0) n = [-n[0], -n[1]];
      // En la hoja Y va hacia abajo
      out.push({ a: sheet(p), b: sheet(q), offset: [n[0] * 7, -n[1] * 7], value: len });
    } else {
      const [s1, s2] = [asSegment(lines[0]!), asSegment(lines[1]!)];
      if (!s1 || !s2) continue;
      const [p, q] = s1;
      const len = Math.hypot(q[0] - p[0], q[1] - p[1]);
      const u: P2 = [(q[0] - p[0]) / len, (q[1] - p[1]) / len];
      const n: P2 = [-u[1], u[0]];
      // Paralelas: la distancia es la del medio de la segunda a la recta de la primera
      const m2: P2 = [(s2[0][0] + s2[1][0]) / 2, (s2[0][1] + s2[1][1]) / 2];
      const off = (m2[0] - p[0]) * n[0] + (m2[1] - p[1]) * n[1];
      const along = (m2[0] - p[0]) * u[0] + (m2[1] - p[1]) * u[1];
      const foot: P2 = [p[0] + u[0] * along, p[1] + u[1] * along];
      const v2 = s2[1];
      const cross = Math.abs((v2[0] - s2[0][0]) * u[1] - (v2[1] - s2[0][1]) * u[0]) / Math.hypot(v2[0] - s2[0][0], v2[1] - s2[0][1]);
      if (cross > 0.02) continue;
      // La línea de cota corrida hacia afuera a lo largo de las rectas
      const sideSign = (foot[0] - center[0]) * u[0] + (foot[1] - center[1]) * u[1] >= 0 ? 1 : -1;
      out.push({ a: sheet(foot), b: sheet(m2), offset: [u[0] * 7 * sideSign, -u[1] * 7 * sideSign], value: Math.abs(off) });
    }
  }
  return out;
}

/**
 * Ángulo entre dos rectas que se cortan: el vértice, el ángulo de cada rayo
 * (del vértice hacia el lado de la recta donde se hizo clic), los grados, el
 * radio del arco (la distancia media a los clics) y el extremo más lejano de
 * cada recta. `null` si son paralelas.
 */
export function angleBetween(s1: [P2, P2], s2: [P2, P2], ref1: P2, ref2: P2) {
  const d1: P2 = [s1[1][0] - s1[0][0], s1[1][1] - s1[0][1]];
  const d2: P2 = [s2[1][0] - s2[0][0], s2[1][1] - s2[0][1]];
  const den = d1[0] * d2[1] - d1[1] * d2[0];
  if (Math.abs(den) < 1e-9 * Math.hypot(...d1) * Math.hypot(...d2)) return null;
  const t = ((s2[0][0] - s1[0][0]) * d2[1] - (s2[0][1] - s1[0][1]) * d2[0]) / den;
  const x: P2 = [s1[0][0] + t * d1[0], s1[0][1] + t * d1[1]];
  // La dirección de la recta, hacia el lado del clic (el clic no cae justo
  // sobre ella; si cae en el vértice, hacia el medio de la recta)
  const ray = (seg: [P2, P2], ref: P2): P2 => {
    const l = Math.hypot(seg[1][0] - seg[0][0], seg[1][1] - seg[0][1]);
    const d: P2 = [(seg[1][0] - seg[0][0]) / l, (seg[1][1] - seg[0][1]) / l];
    let side = (ref[0] - x[0]) * d[0] + (ref[1] - x[1]) * d[1];
    if (Math.abs(side) < 1e-9) side = ((seg[0][0] + seg[1][0]) / 2 - x[0]) * d[0] + ((seg[0][1] + seg[1][1]) / 2 - x[1]) * d[1];
    return side >= 0 ? d : [-d[0], -d[1]];
  };
  const [u1, u2] = [ray(s1, ref1), ray(s2, ref2)];
  const far = (seg: [P2, P2]) => (Math.hypot(seg[0][0] - x[0], seg[0][1] - x[1]) > Math.hypot(seg[1][0] - x[0], seg[1][1] - x[1]) ? seg[0] : seg[1]);
  const degrees = (Math.acos(Math.max(-1, Math.min(1, u1[0] * u2[0] + u1[1] * u2[1]))) * 180) / Math.PI;
  const reach = (Math.hypot(ref1[0] - x[0], ref1[1] - x[1]) + Math.hypot(ref2[0] - x[0], ref2[1] - x[1])) / 2;
  return { x, t0: Math.atan2(u1[1], u1[0]), t1: Math.atan2(u2[1], u2[0]), degrees, reach, far0: far(s1), far1: far(s2) };
}

// ─── Detalles ───────────────────────────────────────────────────────────────

/** Partes de una polilínea dentro del círculo (`c`, `r`) */
export function clipToCircle(pts: P2[], c: P2, r: number): P2[][] {
  const inside = (p: P2) => Math.hypot(p[0] - c[0], p[1] - c[1]) <= r;
  const out: P2[][] = [];
  let cur: P2[] = [];
  const flush = () => {
    if (cur.length > 1) out.push(cur);
    cur = [];
  };
  for (let i = 0; i + 1 < pts.length; i++) {
    const [a, b] = [pts[i], pts[i + 1]];
    const d: P2 = [b[0] - a[0], b[1] - a[1]];
    const f: P2 = [a[0] - c[0], a[1] - c[1]];
    // |a + t·d − c|² = r²
    const A = d[0] * d[0] + d[1] * d[1];
    const B = 2 * (f[0] * d[0] + f[1] * d[1]);
    const C = f[0] * f[0] + f[1] * f[1] - r * r;
    const disc = B * B - 4 * A * C;
    let t0 = 0;
    let t1 = 1;
    if (A < 1e-18) {
      if (!inside(a)) continue;
    } else if (disc <= 0) {
      flush();
      continue;
    } else {
      const sq = Math.sqrt(disc);
      t0 = Math.max(0, (-B - sq) / (2 * A));
      t1 = Math.min(1, (-B + sq) / (2 * A));
      if (t0 >= t1) {
        flush();
        continue;
      }
    }
    const at = (t: number): P2 => [a[0] + t * d[0], a[1] + t * d[1]];
    if (t0 > 0) flush();
    if (!cur.length) cur.push(at(t0));
    cur.push(at(t1));
    if (t1 < 1) flush();
  }
  flush();
  return out;
}

/** Escala de un detalle: la normalizada que sea al menos el doble de la de la hoja */
export function detailScale(sheetScale: number): number {
  const bigger = SCALES.filter((s) => s >= 2 * sheetScale - 1e-9);
  return bigger.length ? bigger[bigger.length - 1] : 2 * sheetScale;
}

/**
 * Los detalles como vistas más: las líneas de la vista recortadas al círculo,
 * a su escala, en el primer hueco libre de la hoja (de arriba a la derecha
 * hacia abajo, sin pisar vistas, cotas ni el cajetín).
 */
export function placeDetails(placed: Placed[], details: Detail[], sheet: SheetSize): Placed[] {
  type Rect = [number, number, number, number];
  const busy: Rect[] = placed.map((v) => {
    const b = bounds(v.lines);
    // Lugar para las cotas alrededor
    return [v.x + b.min[0] * v.scale - 12, v.y - b.max[1] * v.scale - 8, v.x + b.max[0] * v.scale + 8, v.y - b.min[1] * v.scale + 14];
  });
  busy.push([sheet.width - MARGIN - TITLE_W - 2, sheet.height - MARGIN - TITLE_H - 2, sheet.width, sheet.height]);
  const overlaps = (r: Rect) => busy.some((q) => r[0] < q[2] && r[2] > q[0] && r[1] < q[3] && r[3] > q[1]);
  const out: Placed[] = [];
  for (const d of details) {
    const parent = placed.find((v) => v.name === d.view);
    if (!parent) continue;
    const scale = d.scale ?? detailScale(parent.scale);
    const lines: DrawingLine[] = parent.lines.flatMap((l) => clipToCircle(l.points, d.center, d.radius).map((points) => ({ kind: l.kind, points })));
    // El rayado entero: en el SVG se recorta al círculo
    const D = 2 * d.radius * scale;
    // Círculo + rótulo debajo
    const [w, h] = [D + 4, D + 12];
    let spot: [number, number] | undefined;
    for (let y = MARGIN + 4; !spot && y + h <= sheet.height - MARGIN; y += 2)
      for (let x = sheet.width - MARGIN - 4 - w; x >= MARGIN + 4; x -= 2) {
        if (!overlaps([x, y, x + w, y + h])) {
          spot = [x, y];
          break;
        }
      }
    // Sin lugar: abajo a la izquierda (encima de lo que haya)
    const [x0, y0] = spot ?? [MARGIN + 4, sheet.height - MARGIN - h];
    busy.push([x0, y0, x0 + w, y0 + h]);
    const [cx, cy] = [x0 + w / 2, y0 + 2 + D / 2];
    out.push({ name: `detalle-${d.label}`, scale, lines, hatch: parent.hatch, x: cx - d.center[0] * scale, y: cy + d.center[1] * scale, detail: { ...d, scale } });
  }
  return out;
}

/**
 * Cotas generales de cada vista (no la isométrica): ancho abajo y alto a la
 * izquierda, del rectángulo que ocupa la vista. Se recalculan con el modelo.
 */
export function overallDimensions(placed: Placed[]): Dimension[] {
  const dims: Dimension[] = [];
  for (const v of placed) {
    if (v.name === "iso" || v.detail) continue;
    const b = bounds(v.lines);
    const [x0, x1] = [v.x + b.min[0] * v.scale, v.x + b.max[0] * v.scale];
    const [yTop, yBot] = [v.y - b.max[1] * v.scale, v.y - b.min[1] * v.scale];
    if (x1 - x0 > 1e-6) dims.push({ a: [x0, yBot], b: [x1, yBot], offset: [0, 7], value: b.max[0] - b.min[0] });
    if (yBot - yTop > 1e-6) dims.push({ a: [x0, yBot], b: [x0, yTop], offset: [-7, 0], value: b.max[1] - b.min[1] });
  }
  return dims;
}

/** Texto de una cota: hasta 2 decimales, coma decimal */
export const dimText = (v: number) => v.toLocaleString("es", { maximumFractionDigits: 2 });

const arrowSvg = (p: P2, ux: number, uy: number) => {
  // Punta en `p`, apuntando hacia −u
  const [bx, by] = [p[0] + ux * 2.5, p[1] + uy * 2.5];
  const [px, py] = [-uy * 0.8, ux * 0.8];
  return `<polygon points="${n(p[0])},${n(p[1])} ${n(bx + px)},${n(by + py)} ${n(bx - px)},${n(by - py)}" fill="#000" stroke="none"/>`;
};

/** Ángulos de un arco de cota llevados al tramo corto (≤ 180°) de a0 a a1 */
function shortSweep(a0: number, a1: number): [number, number] {
  let d = a1 - a0;
  while (d > Math.PI) d -= 2 * Math.PI;
  while (d < -Math.PI) d += 2 * Math.PI;
  return [a0, a0 + d];
}

function angleSvg(d: AngleDimension): string {
  const [a0, a1] = shortSweep(d.a0, d.a1);
  const at = (a: number, r = d.r): P2 => [d.c[0] + r * Math.cos(a), d.c[1] + r * Math.sin(a)];
  const [p0, p1] = [at(a0), at(a1)];
  const sweep = a1 > a0 ? 1 : 0;
  const mid = (a0 + a1) / 2;
  const t = at(mid, d.r + 3);
  // Flechas tangentes al arco, hacia adentro
  const s = sweep ? 1 : -1;
  const tan = (a: number, k: number): [number, number] => [-Math.sin(a) * k * s, Math.cos(a) * k * s];
  // Líneas de referencia: de la punta de cada recta hasta el arco, si no llega
  const ext = (end: P2, p: P2) =>
    Math.hypot(end[0] - d.c[0], end[1] - d.c[1]) < d.r - 0.5 ? `<line x1="${n(end[0])}" y1="${n(end[1])}" x2="${n(p[0])}" y2="${n(p[1])}"/>` : "";
  return (
    `<g data-dimension="" data-angle="" stroke="#000" stroke-width="0.18" fill="none">` +
    ext(d.ends[0], p0) +
    ext(d.ends[1], p1) +
    `<path d="M ${n(p0[0])} ${n(p0[1])} A ${n(d.r)} ${n(d.r)} 0 0 ${sweep} ${n(p1[0])} ${n(p1[1])}"/>` +
    arrowSvg(p0, ...tan(a0, 1)) +
    arrowSvg(p1, ...tan(a1, -1)) +
    `<text x="${n(t[0])}" y="${n(t[1] + 1)}" font-size="3" text-anchor="middle" stroke="none" fill="#000">${esc(dimText(d.value) + "°")}</text>` +
    `</g>`
  );
}

function radiusSvg(d: RadiusDimension): string {
  const len = Math.hypot(d.p[0] - d.c[0], d.p[1] - d.c[1]) || 1;
  const [ux, uy] = [(d.p[0] - d.c[0]) / len, (d.p[1] - d.c[1]) / len];
  // El texto sale por afuera del arco, en la dirección de la flecha
  const tip: P2 = [d.p[0] + ux * 6, d.p[1] + uy * 6];
  const right = ux >= 0;
  return (
    `<g data-dimension="" data-radius="" stroke="#000" stroke-width="0.18">` +
    `<line x1="${n(d.c[0])}" y1="${n(d.c[1])}" x2="${n(tip[0])}" y2="${n(tip[1])}"/>` +
    arrowSvg(d.p, -ux, -uy) +
    `<text x="${n(tip[0] + (right ? 1 : -1))}" y="${n(tip[1] + 1)}" font-size="3" text-anchor="${right ? "start" : "end"}" stroke="none" fill="#000">${esc("R" + dimText(d.value))}</text>` +
    `</g>`
  );
}

function dimensionSvg(d: AnyDimension): string {
  if (d.kind === "angle") return angleSvg(d);
  if (d.kind === "radius") return radiusSvg(d);
  const [ox, oy] = d.offset;
  const a2: P2 = [d.a[0] + ox, d.a[1] + oy];
  const b2: P2 = [d.b[0] + ox, d.b[1] + oy];
  const len = Math.hypot(b2[0] - a2[0], b2[1] - a2[1]) || 1;
  const [ux, uy] = [(b2[0] - a2[0]) / len, (b2[1] - a2[1]) / len];
  // Líneas de referencia (con 1 mm de luz y 1,5 de sobrante) y flechas de 2,5 mm
  const ext = (p: P2, q: P2) => {
    const ol = Math.hypot(ox, oy) || 1;
    const [ex, ey] = [ox / ol, oy / ol];
    return `<line x1="${n(p[0] + ex)}" y1="${n(p[1] + ey)}" x2="${n(q[0] + ex * 1.5)}" y2="${n(q[1] + ey * 1.5)}"/>`;
  };
  const arrow = (p: P2, dir: number) => arrowSvg(p, dir * ux, dir * uy);
  const mid: P2 = [(a2[0] + b2[0]) / 2, (a2[1] + b2[1]) / 2];
  const vertical = Math.abs(uy) > Math.abs(ux);
  const text = vertical
    ? `<text x="${n(mid[0] - 1)}" y="${n(mid[1])}" font-size="3" text-anchor="middle" transform="rotate(-90 ${n(mid[0] - 1)} ${n(mid[1])})" stroke="none" fill="#000">${esc((d.prefix ?? "") + dimText(d.value))}</text>`
    : `<text x="${n(mid[0])}" y="${n(mid[1] - 1)}" font-size="3" text-anchor="middle" stroke="none" fill="#000">${esc((d.prefix ?? "") + dimText(d.value))}</text>`;
  return (
    `<g data-dimension="" stroke="#000" stroke-width="0.18">` +
    ext(d.a, a2) +
    ext(d.b, b2) +
    `<line x1="${n(a2[0])}" y1="${n(a2[1])}" x2="${n(b2[0])}" y2="${n(b2[1])}"/>` +
    arrow(a2, 1) +
    arrow(b2, -1) +
    text +
    `</g>`
  );
}

/** La hoja completa en SVG (unidades en mm) */
export function sheetSvg(
  placed: Placed[],
  sheet: SheetSize,
  info: TitleBlock,
  opts: { hidden: boolean; smooth: boolean; dimensions?: boolean; section?: SectionInfo; userDims?: UserDim[] },
): string {
  const out: string[] = [];
  out.push(
    `<svg xmlns="http://www.w3.org/2000/svg" width="${sheet.width}mm" height="${sheet.height}mm" viewBox="0 0 ${sheet.width} ${sheet.height}" font-family="sans-serif">`,
  );
  out.push(`<rect width="${sheet.width}" height="${sheet.height}" fill="#fff"/>`);
  // Rayado a 45° para las caras cortadas
  out.push(
    `<defs><pattern id="rayado" patternUnits="userSpaceOnUse" width="2" height="2" patternTransform="rotate(45)"><line x1="0" y1="0" x2="0" y2="2" stroke="#000" stroke-width="0.25"/></pattern></defs>`,
  );
  out.push(`<rect x="${MARGIN}" y="${MARGIN}" width="${n(sheet.width - 2 * MARGIN)}" height="${n(sheet.height - 2 * MARGIN)}" fill="none" stroke="#000" stroke-width="0.7"/>`);
  for (const v of placed) {
    if (v.hatch?.length) {
      // En un detalle, solo lo que cae en su círculo
      const clip = v.detail ? `clip-${v.name}` : "";
      if (v.detail)
        out.push(
          `<clipPath id="${clip}"><circle cx="${n(v.x + v.detail.center[0] * v.scale)}" cy="${n(v.y - v.detail.center[1] * v.scale)}" r="${n(v.detail.radius * v.scale)}"/></clipPath>`,
        );
      out.push(`<g data-hatch="${v.name}" fill="url(#rayado)" stroke="none"${clip ? ` clip-path="url(#${clip})"` : ""}>`);
      for (const t of v.hatch) out.push(`<polygon points="${t.map((p) => `${n(v.x + p[0] * v.scale)},${n(v.y - p[1] * v.scale)}`).join(" ")}"/>`);
      out.push("</g>");
    }
    out.push(`<g data-view="${v.name}" fill="none" stroke-linecap="round" stroke-linejoin="round">`);
    for (const l of v.lines) {
      // La isométrica va sin ocultas (como se acostumbra)
      if ((!opts.hidden || v.name === "iso") && (l.kind === "hidden" || l.kind === "hidden_outline")) continue;
      if (!opts.smooth && l.kind === "smooth") continue;
      const pts = l.points.map((p) => `${n(v.x + p[0] * v.scale)},${n(v.y - p[1] * v.scale)}`).join(" ");
      out.push(`<polyline data-kind="${l.kind}" points="${pts}" ${STYLE[l.kind]}/>`);
    }
    out.push("</g>");
  }
  // Detalles: el círculo con su letra en la vista y el rótulo bajo la ampliación
  for (const v of placed) {
    const d = v.detail;
    if (!d) continue;
    const parent = placed.find((p) => p.name === d.view);
    if (parent) {
      const [cx, cy, r] = [parent.x + d.center[0] * parent.scale, parent.y - d.center[1] * parent.scale, d.radius * parent.scale];
      out.push(
        `<g data-detail-mark="${esc(d.label)}"><circle cx="${n(cx)}" cy="${n(cy)}" r="${n(r)}" fill="none" stroke="#000" stroke-width="0.35"/>` +
          `<text x="${n(cx + r * 0.75 + 1)}" y="${n(cy - r * 0.75 - 1)}" font-size="4" fill="#000">${esc(d.label)}</text></g>`,
      );
    }
    const [cx, cy, r] = [v.x + d.center[0] * v.scale, v.y - d.center[1] * v.scale, d.radius * v.scale];
    out.push(
      `<g data-detail="${esc(d.label)}"><circle cx="${n(cx)}" cy="${n(cy)}" r="${n(r)}" fill="none" stroke="#000" stroke-width="0.25"/>` +
        `<text x="${n(cx)}" y="${n(cy + r + 6)}" font-size="3.5" text-anchor="middle" fill="#000">Detalle ${esc(d.label)} (${scaleLabel(v.scale)})</text></g>`,
    );
  }
  if (opts.dimensions) for (const d of overallDimensions(placed)) out.push(dimensionSvg(d));
  for (const d of userDimensions(placed, opts.userDims ?? [])) out.push(dimensionSvg(d).replace("data-dimension=\"\"", 'data-dimension="" data-user-dim=""'));
  // Corte: rótulo bajo el frente y la línea de corte con sus letras en la planta
  const front = placed.find((v) => v.name === "front");
  const top = placed.find((v) => v.name === "top");
  if (opts.section && front) {
    const b = bounds(front.lines);
    const cx = front.x + ((b.min[0] + b.max[0]) / 2) * front.scale;
    const below = front.y - b.min[1] * front.scale + (opts.dimensions ? 14 : 6);
    out.push(`<text data-section-label="" x="${n(cx)}" y="${n(below)}" font-size="4" text-anchor="middle" fill="#000">Corte ${esc(opts.section.label)}-${esc(opts.section.label)}</text>`);
  }
  if (opts.section && top) {
    const b = bounds(top.lines);
    const y = top.y - opts.section.at * top.scale;
    const [x0, x1] = [top.x + b.min[0] * top.scale - 6, top.x + b.max[0] * top.scale + 6];
    out.push(
      `<g data-section-line="" stroke="#000" fill="#000"><line x1="${n(x0)}" y1="${n(y)}" x2="${n(x1)}" y2="${n(y)}" stroke-width="0.35" stroke-dasharray="8 1.5 1.5 1.5"/>` +
        // Flechas hacia donde se mira (hacia arriba en la planta: desde el frente)
        [x0 + 2, x1 - 2].map((x) => `<polygon points="${n(x)},${n(y - 4)} ${n(x - 1)},${n(y - 1.5)} ${n(x + 1)},${n(y - 1.5)}" stroke="none"/><line x1="${n(x)}" y1="${n(y)}" x2="${n(x)}" y2="${n(y - 2)}" stroke-width="0.35"/>`).join("") +
        // Las letras sobre las flechas (al costado pisarían las cotas)
        [x0 + 2, x1 - 2].map((x) => `<text x="${n(x)}" y="${n(y - 5)}" font-size="4" text-anchor="middle" stroke="none">${esc(opts.section!.label)}</text>`).join("") +
        `</g>`,
    );
  }
  // Cajetín abajo a la derecha
  const x0 = sheet.width - MARGIN - TITLE_W;
  const y0 = sheet.height - MARGIN - TITLE_H;
  const cell = (x: number, y: number, w: number, h: number, label: string, value: string, size = 3.5) =>
    `<rect x="${n(x)}" y="${n(y)}" width="${n(w)}" height="${n(h)}" fill="none" stroke="#000" stroke-width="0.35"/>` +
    `<text x="${n(x + 1.5)}" y="${n(y + 3)}" font-size="2" fill="#555">${esc(label)}</text>` +
    `<text x="${n(x + 1.5)}" y="${n(y + h - 2)}" font-size="${size}" fill="#000">${esc(value)}</text>`;
  out.push(`<g data-title-block="">`);
  out.push(cell(x0, y0, TITLE_W, 12, "Título", info.title, 5));
  out.push(cell(x0, y0 + 12, 50, 8, "Autor", info.author ?? ""));
  out.push(cell(x0 + 50, y0 + 12, 50, 8, "Material", info.material ?? ""));
  out.push(cell(x0 + 100, y0 + 12, 30, 8, "Hoja", info.sheet));
  out.push(cell(x0, y0 + 20, 50, 8, "Fecha", info.date));
  out.push(cell(x0 + 50, y0 + 20, 30, 8, "Escala", info.scale));
  out.push(cell(x0 + 80, y0 + 20, 50, 8, "Proyección", info.projection === "first" ? "Primer diedro (ISO E)" : "Tercer diedro (ISO A)", 2.8));
  out.push("</g>");
  out.push("</svg>");
  return out.join("\n");
}

// ─── DXF ────────────────────────────────────────────────────────────────────

/**
 * DXF (R12, texto) en mm, Y hacia arriba: capas VISIBLE, OCULTA (punteada) y
 * TANGENTE; cada tramo de polilínea como LINE (lo leen todos los programas de
 * corte). `view` exporta solo esa vista a escala 1:1 con sus coordenadas
 * (para láser o CNC); sin ella, la hoja entera con el recuadro.
 */
export function sheetDxf(placed: Placed[], sheet: SheetSize, opts: { hidden: boolean; smooth: boolean; view?: string; dimensions?: boolean; userDims?: UserDim[] }): string {
  const out: string[] = [];
  const pair = (code: number, value: string | number) => out.push(String(code), typeof value === "number" ? n(value) : value);
  const section = (name: string, body: () => void) => {
    pair(0, "SECTION");
    pair(2, name);
    body();
    pair(0, "ENDSEC");
  };
  section("HEADER", () => {
    pair(9, "$ACADVER");
    pair(1, "AC1009");
    pair(9, "$INSUNITS");
    pair(70, 4);
  });
  section("TABLES", () => {
    pair(0, "TABLE");
    pair(2, "LTYPE");
    pair(70, 2);
    pair(0, "LTYPE");
    pair(2, "CONTINUOUS");
    pair(70, 0);
    pair(3, "Continua");
    pair(72, 65);
    pair(73, 0);
    pair(40, 0);
    pair(0, "LTYPE");
    pair(2, "DASHED");
    pair(70, 0);
    pair(3, "Punteada __ __ __");
    pair(72, 65);
    pair(73, 2);
    pair(40, 4.5);
    pair(49, 3);
    pair(49, -1.5);
    pair(0, "ENDTAB");
    pair(0, "TABLE");
    pair(2, "LAYER");
    pair(70, 4);
    for (const [name, color, ltype] of [
      ["VISIBLE", 7, "CONTINUOUS"],
      ["OCULTA", 8, "DASHED"],
      ["TANGENTE", 9, "CONTINUOUS"],
      ["COTAS", 3, "CONTINUOUS"],
    ] as const) {
      pair(0, "LAYER");
      pair(2, name);
      pair(70, 0);
      pair(62, color);
      pair(6, ltype);
    }
    pair(0, "ENDTAB");
  });
  const line = (layer: string, a: P2, b: P2) => {
    pair(0, "LINE");
    pair(8, layer);
    pair(10, a[0]);
    pair(20, a[1]);
    pair(30, 0);
    pair(11, b[0]);
    pair(21, b[1]);
    pair(31, 0);
  };
  section("ENTITIES", () => {
    const only = opts.view ? placed.filter((v) => v.name === opts.view) : placed;
    for (const v of only) {
      // 1:1 con las coordenadas de la vista, o en la hoja (Y hacia arriba)
      const at = (p: P2): P2 => (opts.view ? p : [v.x + p[0] * v.scale, sheet.height - (v.y - p[1] * v.scale)]);
      for (const l of v.lines) {
        const hidden = l.kind === "hidden" || l.kind === "hidden_outline";
        if (hidden && (!opts.hidden || v.name === "iso")) continue;
        if (l.kind === "smooth" && !opts.smooth) continue;
        const layer = hidden ? "OCULTA" : l.kind === "smooth" ? "TANGENTE" : "VISIBLE";
        for (let i = 0; i + 1 < l.points.length; i++) line(layer, at(l.points[i]), at(l.points[i + 1]));
      }
    }
    if (!opts.view && (opts.dimensions || (opts.userDims ?? []).length)) {
      // Cotas generales como líneas y texto (Y hacia arriba)
      const up = (p: P2): P2 => [p[0], sheet.height - p[1]];
      const text = (at: P2, value: string, rotate = false) => {
        pair(0, "TEXT");
        pair(8, "COTAS");
        pair(10, at[0]);
        pair(20, at[1]);
        pair(30, 0);
        pair(40, 3);
        pair(1, value);
        if (rotate) pair(50, 90);
      };
      for (const d of [...(opts.dimensions ? overallDimensions(placed) : []), ...userDimensions(placed, opts.userDims ?? [])]) {
        if (d.kind === "angle") {
          // El arco en tramos de ~3°
          const [a0, a1] = shortSweep(d.a0, d.a1);
          const k = Math.max(4, Math.ceil(Math.abs(a1 - a0) / 0.05));
          const at = (a: number, r = d.r): P2 => [d.c[0] + r * Math.cos(a), d.c[1] + r * Math.sin(a)];
          for (let i = 0; i < k; i++) line("COTAS", up(at(a0 + ((a1 - a0) * i) / k)), up(at(a0 + ((a1 - a0) * (i + 1)) / k)));
          text(up(at((a0 + a1) / 2, d.r + 3)), dimText(d.value) + "°");
          continue;
        }
        if (d.kind === "radius") {
          const len = Math.hypot(d.p[0] - d.c[0], d.p[1] - d.c[1]) || 1;
          const tip: P2 = [d.p[0] + ((d.p[0] - d.c[0]) / len) * 6, d.p[1] + ((d.p[1] - d.c[1]) / len) * 6];
          line("COTAS", up(d.c), up(tip));
          text(up(tip), "R" + dimText(d.value));
          continue;
        }
        const a2: P2 = [d.a[0] + d.offset[0], d.a[1] + d.offset[1]];
        const b2: P2 = [d.b[0] + d.offset[0], d.b[1] + d.offset[1]];
        line("COTAS", up(d.a), up(a2));
        line("COTAS", up(d.b), up(b2));
        line("COTAS", up(a2), up(b2));
        const mid = up([(a2[0] + b2[0]) / 2, (a2[1] + b2[1]) / 2]);
        text([mid[0], mid[1] + 1], (d.prefix ?? "") + dimText(d.value), Math.abs(d.offset[0]) > Math.abs(d.offset[1]));
      }
    }
    // Círculos de los detalles (en la vista y en la ampliación) y su rótulo
    if (!opts.view)
      for (const v of placed) {
        const d = v.detail;
        if (!d) continue;
        const ring = (cx: number, cy: number, r: number) => {
          for (let i = 0; i < 72; i++) {
            const [t0, t1] = [(i / 72) * 2 * Math.PI, ((i + 1) / 72) * 2 * Math.PI];
            line("VISIBLE", [cx + r * Math.cos(t0), cy + r * Math.sin(t0)], [cx + r * Math.cos(t1), cy + r * Math.sin(t1)]);
          }
        };
        const parent = placed.find((p) => p.name === d.view);
        if (parent) ring(parent.x + d.center[0] * parent.scale, sheet.height - (parent.y - d.center[1] * parent.scale), d.radius * parent.scale);
        const [cx, cy, r] = [v.x + d.center[0] * v.scale, sheet.height - (v.y - d.center[1] * v.scale), d.radius * v.scale];
        ring(cx, cy, r);
        pair(0, "TEXT");
        pair(8, "COTAS");
        pair(10, cx - r);
        pair(20, cy - r - 6);
        pair(30, 0);
        pair(40, 3.5);
        pair(1, `Detalle ${d.label} (${scaleLabel(v.scale)})`);
      }
    if (!opts.view) {
      const [x0, y0, x1, y1] = [MARGIN, MARGIN, sheet.width - MARGIN, sheet.height - MARGIN];
      line("VISIBLE", [x0, y0], [x1, y0]);
      line("VISIBLE", [x1, y0], [x1, y1]);
      line("VISIBLE", [x1, y1], [x0, y1]);
      line("VISIBLE", [x0, y1], [x0, y0]);
    }
  });
  pair(0, "EOF");
  return out.join("\n") + "\n";
}
