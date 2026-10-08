// Desarrollo de chapa (lo calcula el backend, `cad_flat_pattern`): la hoja en
// SVG con el contorno, las líneas de doblez con su ángulo y las cotas
// generales, y el DXF 1:1 para cortar (capas CONTORNO y DOBLEZ).

import { dimText, dimensionSvg, type Dimension } from "./drawing.ts";

type P2 = [number, number];

export interface FlatPatternData {
  outline: P2[][];
  bends: { line: [P2, P2]; angle: number; radius: number; up: boolean }[];
  thickness: number;
  k_factor: number;
  min: P2;
  max: P2;
}

const n = (v: number) => (Math.round(v * 1000) / 1000).toString();
const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

/** Texto de un doblez: "↑ 90° R2" (hacia quien mira) o "↓" (hacia atrás) */
export function bendLabel(b: FlatPatternData["bends"][number]): string {
  return `${b.up ? "↑" : "↓"} ${dimText(b.angle)}° R${dimText(b.radius)}`;
}

/** Margen alrededor de la pieza (mm), con lugar para las cotas abajo y a la izquierda */
const MARGIN = 16;

/** La pieza plana a escala 1:1 en SVG (mm) */
export function flatSvg(fp: FlatPatternData, title = ""): string {
  const [w, h] = [fp.max[0] - fp.min[0], fp.max[1] - fp.min[1]];
  const W = w + 2 * MARGIN;
  const H = h + 2 * MARGIN + (title ? 8 : 0);
  // Y hacia abajo en la hoja
  const at = (p: P2): P2 => [MARGIN + p[0] - fp.min[0], MARGIN + (fp.max[1] - p[1])];
  const out: string[] = [];
  out.push(`<svg xmlns="http://www.w3.org/2000/svg" width="${n(W)}mm" height="${n(H)}mm" viewBox="0 0 ${n(W)} ${n(H)}" font-family="sans-serif">`);
  out.push(`<rect width="${n(W)}" height="${n(H)}" fill="#fff"/>`);
  out.push(`<g data-outline="" fill="none" stroke="#000" stroke-width="0.35" stroke-linecap="round" stroke-linejoin="round">`);
  for (const l of fp.outline) out.push(`<polyline points="${l.map((p) => at(p).map(n).join(",")).join(" ")}"/>`);
  out.push(`</g>`);
  for (const b of fp.bends) {
    const [p, q] = [at(b.line[0]), at(b.line[1])];
    const mid: P2 = [(p[0] + q[0]) / 2, (p[1] + q[1]) / 2];
    // El texto a lo largo de la línea, legible (sin quedar cabeza abajo)
    let ang = (Math.atan2(q[1] - p[1], q[0] - p[0]) * 180) / Math.PI;
    if (ang > 90) ang -= 180;
    if (ang < -90) ang += 180;
    out.push(
      `<g data-bend="" stroke="#c00" fill="#c00"><line x1="${n(p[0])}" y1="${n(p[1])}" x2="${n(q[0])}" y2="${n(q[1])}" stroke-width="0.25" stroke-dasharray="4 1.5 1 1.5"/>` +
        `<text x="${n(mid[0])}" y="${n(mid[1] - 1)}" font-size="3" text-anchor="middle" stroke="none" transform="rotate(${n(ang)} ${n(mid[0])} ${n(mid[1])})">${esc(bendLabel(b))}</text></g>`,
    );
  }
  // Cotas generales: ancho abajo y alto a la izquierda
  const dims: Dimension[] = [
    { a: at([fp.min[0], fp.min[1]]), b: at([fp.max[0], fp.min[1]]), offset: [0, 8], value: w },
    { a: at([fp.min[0], fp.min[1]]), b: at([fp.min[0], fp.max[1]]), offset: [-8, 0], value: h },
  ];
  for (const d of dims) out.push(dimensionSvg(d));
  const info = `${title ? title + " · " : ""}Desarrollo 1:1 · espesor ${dimText(fp.thickness)} mm · K ${dimText(fp.k_factor)}`;
  out.push(`<text x="${n(MARGIN)}" y="${n(H - 4)}" font-size="3.5" fill="#000">${esc(info)}</text>`);
  out.push(`</svg>`);
  return out.join("\n");
}

/** DXF R12 en mm a 1:1: el contorno (CONTORNO) y los dobleces (DOBLEZ, punteados, con su texto) */
export function flatDxf(fp: FlatPatternData): string {
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
    for (const [name, desc, dashes] of [
      ["CONTINUOUS", "Continua", []],
      ["DASHDOT", "Doblez _ . _", [4, -1.5, 1, -1.5]],
    ] as const) {
      pair(0, "LTYPE");
      pair(2, name);
      pair(70, 0);
      pair(3, desc);
      pair(72, 65);
      pair(73, dashes.length);
      pair(40, dashes.reduce((a: number, d: number) => a + Math.abs(d), 0));
      for (const d of dashes) pair(49, d);
    }
    pair(0, "ENDTAB");
    pair(0, "TABLE");
    pair(2, "LAYER");
    pair(70, 2);
    for (const [name, color, ltype] of [
      ["CONTORNO", 7, "CONTINUOUS"],
      ["DOBLEZ", 1, "DASHDOT"],
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
    for (const l of fp.outline) for (let i = 0; i + 1 < l.length; i++) line("CONTORNO", l[i], l[i + 1]);
    for (const b of fp.bends) {
      line("DOBLEZ", b.line[0], b.line[1]);
      const mid: P2 = [(b.line[0][0] + b.line[1][0]) / 2, (b.line[0][1] + b.line[1][1]) / 2];
      pair(0, "TEXT");
      pair(8, "DOBLEZ");
      pair(10, mid[0]);
      pair(20, mid[1] + 1);
      pair(30, 0);
      pair(40, 2.5);
      // Sin flechas (no todas las fuentes de DXF las tienen)
      pair(1, `${b.up ? "ARRIBA" : "ABAJO"} ${dimText(b.angle)} R${dimText(b.radius)}`);
      pair(50, (Math.atan2(b.line[1][1] - b.line[0][1], b.line[1][0] - b.line[0][0]) * 180) / Math.PI);
    }
  });
  pair(0, "EOF");
  return out.join("\n") + "\n";
}
