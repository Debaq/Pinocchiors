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
const GAP = 15;

export interface Placed {
  name: ViewName;
  /** Traslado en la hoja (mm) del origen de la vista y escala */
  x: number;
  y: number;
  scale: number;
  lines: DrawingLine[];
}

/**
 * Acomoda frente, planta y lateral en cruz (planta debajo del frente en primer
 * diedro, arriba en tercero; lateral a la derecha) y la isométrica arriba a la
 * derecha. Elige la escala normalizada más grande con la que todo entra.
 * Coordenadas de hoja: x a la derecha, y hacia abajo (como SVG).
 */
export function layout(views: Partial<Record<ViewName, DrawingLine[]>>, sheet: SheetSize, projection: Projection, fixedScale?: number): { placed: Placed[]; scale: number } {
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
    placed.push({ name, scale, lines: views[name]!, x: x0 + (w - (bb.max[0] - bb.min[0]) * scale) / 2 - bb.min[0] * scale, y: y0 + bb.max[1] * scale });
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

/** La hoja completa en SVG (unidades en mm) */
export function sheetSvg(placed: Placed[], sheet: SheetSize, info: TitleBlock, opts: { hidden: boolean; smooth: boolean }): string {
  const out: string[] = [];
  out.push(
    `<svg xmlns="http://www.w3.org/2000/svg" width="${sheet.width}mm" height="${sheet.height}mm" viewBox="0 0 ${sheet.width} ${sheet.height}" font-family="sans-serif">`,
  );
  out.push(`<rect width="${sheet.width}" height="${sheet.height}" fill="#fff"/>`);
  out.push(`<rect x="${MARGIN}" y="${MARGIN}" width="${n(sheet.width - 2 * MARGIN)}" height="${n(sheet.height - 2 * MARGIN)}" fill="none" stroke="#000" stroke-width="0.7"/>`);
  for (const v of placed) {
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
  // Cajetín abajo a la derecha
  const x0 = sheet.width - MARGIN - TITLE_W;
  const y0 = sheet.height - MARGIN - TITLE_H;
  const cell = (x: number, y: number, w: number, h: number, label: string, value: string, size = 3.5) =>
    `<rect x="${n(x)}" y="${n(y)}" width="${n(w)}" height="${n(h)}" fill="none" stroke="#000" stroke-width="0.35"/>` +
    `<text x="${n(x + 1.5)}" y="${n(y + 3)}" font-size="2" fill="#555">${esc(label)}</text>` +
    `<text x="${n(x + 1.5)}" y="${n(y + h - 2)}" font-size="${size}" fill="#000">${esc(value)}</text>`;
  out.push(`<g data-title-block>`);
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
