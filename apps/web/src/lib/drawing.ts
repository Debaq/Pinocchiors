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
  name: ViewName;
  /** Traslado en la hoja (mm) del origen de la vista y escala */
  x: number;
  y: number;
  scale: number;
  lines: DrawingLine[];
  /** Triángulos de la cara cortada (vista en corte), para rayar */
  hatch?: [P2, P2, P2][];
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
  a: P2;
  b: P2;
  /** Hacia dónde se corre la línea de cota (perpendicular, en mm de hoja) */
  offset: P2;
  value: number;
}

/**
 * Cotas generales de cada vista (no la isométrica): ancho abajo y alto a la
 * izquierda, del rectángulo que ocupa la vista. Se recalculan con el modelo.
 */
export function overallDimensions(placed: Placed[]): Dimension[] {
  const dims: Dimension[] = [];
  for (const v of placed) {
    if (v.name === "iso") continue;
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

function dimensionSvg(d: Dimension): string {
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
  const arrow = (p: P2, dir: number) => {
    const [bx, by] = [p[0] + dir * ux * 2.5, p[1] + dir * uy * 2.5];
    const [px, py] = [-uy * 0.8, ux * 0.8];
    return `<polygon points="${n(p[0])},${n(p[1])} ${n(bx + px)},${n(by + py)} ${n(bx - px)},${n(by - py)}" fill="#000" stroke="none"/>`;
  };
  const mid: P2 = [(a2[0] + b2[0]) / 2, (a2[1] + b2[1]) / 2];
  const vertical = Math.abs(uy) > Math.abs(ux);
  const text = vertical
    ? `<text x="${n(mid[0] - 1)}" y="${n(mid[1])}" font-size="3" text-anchor="middle" transform="rotate(-90 ${n(mid[0] - 1)} ${n(mid[1])})" stroke="none" fill="#000">${esc(dimText(d.value))}</text>`
    : `<text x="${n(mid[0])}" y="${n(mid[1] - 1)}" font-size="3" text-anchor="middle" stroke="none" fill="#000">${esc(dimText(d.value))}</text>`;
  return (
    `<g data-dimension stroke="#000" stroke-width="0.18">` +
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
export function sheetSvg(placed: Placed[], sheet: SheetSize, info: TitleBlock, opts: { hidden: boolean; smooth: boolean; dimensions?: boolean; section?: SectionInfo }): string {
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
      out.push(`<g data-hatch="${v.name}" fill="url(#rayado)" stroke="none">`);
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
  if (opts.dimensions) for (const d of overallDimensions(placed)) out.push(dimensionSvg(d));
  // Corte: rótulo bajo el frente y la línea de corte con sus letras en la planta
  const front = placed.find((v) => v.name === "front");
  const top = placed.find((v) => v.name === "top");
  if (opts.section && front) {
    const b = bounds(front.lines);
    const cx = front.x + ((b.min[0] + b.max[0]) / 2) * front.scale;
    const below = front.y - b.min[1] * front.scale + (opts.dimensions ? 14 : 6);
    out.push(`<text data-section-label x="${n(cx)}" y="${n(below)}" font-size="4" text-anchor="middle" fill="#000">Corte ${esc(opts.section.label)}-${esc(opts.section.label)}</text>`);
  }
  if (opts.section && top) {
    const b = bounds(top.lines);
    const y = top.y - opts.section.at * top.scale;
    const [x0, x1] = [top.x + b.min[0] * top.scale - 6, top.x + b.max[0] * top.scale + 6];
    out.push(
      `<g data-section-line stroke="#000" fill="#000"><line x1="${n(x0)}" y1="${n(y)}" x2="${n(x1)}" y2="${n(y)}" stroke-width="0.35" stroke-dasharray="8 1.5 1.5 1.5"/>` +
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

// ─── DXF ────────────────────────────────────────────────────────────────────

/**
 * DXF (R12, texto) en mm, Y hacia arriba: capas VISIBLE, OCULTA (punteada) y
 * TANGENTE; cada tramo de polilínea como LINE (lo leen todos los programas de
 * corte). `view` exporta solo esa vista a escala 1:1 con sus coordenadas
 * (para láser o CNC); sin ella, la hoja entera con el recuadro.
 */
export function sheetDxf(placed: Placed[], sheet: SheetSize, opts: { hidden: boolean; smooth: boolean; view?: ViewName; dimensions?: boolean }): string {
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
    if (!opts.view && opts.dimensions) {
      // Cotas generales como líneas y texto (Y hacia arriba)
      const up = (p: P2): P2 => [p[0], sheet.height - p[1]];
      for (const d of overallDimensions(placed)) {
        const a2: P2 = [d.a[0] + d.offset[0], d.a[1] + d.offset[1]];
        const b2: P2 = [d.b[0] + d.offset[0], d.b[1] + d.offset[1]];
        line("COTAS", up(d.a), up(a2));
        line("COTAS", up(d.b), up(b2));
        line("COTAS", up(a2), up(b2));
        const mid = up([(a2[0] + b2[0]) / 2, (a2[1] + b2[1]) / 2]);
        pair(0, "TEXT");
        pair(8, "COTAS");
        pair(10, mid[0]);
        pair(20, mid[1] + 1);
        pair(30, 0);
        pair(40, 3);
        pair(1, dimText(d.value));
        if (Math.abs(d.offset[0]) > Math.abs(d.offset[1])) pair(50, 90);
      }
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
