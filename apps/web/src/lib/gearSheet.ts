// Engranes en el plano 2D: el círculo primitivo (trazo y punto) en las vistas
// que miran a lo largo de su eje y la tabla de datos del engrane (módulo,
// dientes, ángulos, diámetros, cota sobre dientes y sobre bolas).
// Funciones puras: se prueban con node (e2e/gearSheet.test.mjs).

import type { GearAxis, GearInfo } from "./cad.ts";
import { t as tr } from "./i18n.ts";
import type { P2, P3, ViewDir } from "./drawing.ts";

export interface GearMark {
  view: string;
  /** Centro en coordenadas de la vista y radio primitivo (mm de la pieza) */
  center: P2;
  radius: number;
  /** Los otros círculos del engrane, rotulados (cabeza, pie, base): lo que se sabe de `gearInfo` */
  circles?: { label: string; radius: number }[];
}

export interface GearTable {
  title: string;
  rows: [string, string][];
}

const dot = (a: P3, b: P3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const num = (v: number, digits = 3) => (+v.toFixed(digits)).toLocaleString("es", { maximumFractionDigits: digits });

/**
 * Círculos primitivos de los engranes `gears` en las vistas `views` (nombre y
 * hacia dónde mira): solo donde el eje del engrane apunta a quien mira.
 */
export function gearMarks(gears: GearAxis[], views: { name: string; dir: ViewDir }[], infos: (GearInfo | undefined)[] = []): GearMark[] {
  const out: GearMark[] = [];
  for (const [index, g] of gears.entries()) {
    if (g.part.spec.kind === "rack" || !(g.pitch_radius > 0)) continue;
    const z = norm(g.z);
    // Centro del engrane a media altura de su ancho
    const mid: P3 = [0, 1, 2].map((i) => g.origin[i] + z[i] * (g.part.width / 2)) as P3;
    for (const v of views) {
      if (Math.abs(dot(z, v.dir.eye)) < 0.9999) continue;
      // Arriba de la hoja: eye × xdir (como `viewUp`)
      const e = v.dir.eye;
      const x = v.dir.xdir;
      const up = norm([e[1] * x[2] - e[2] * x[1], e[2] * x[0] - e[0] * x[2], e[0] * x[1] - e[1] * x[0]]);
      const info = infos[index];
      const circles = info && [
        { label: "da", radius: info.tip_diameter / 2 },
        { label: "df", radius: info.root_diameter / 2 },
        ...(g.part.spec.kind === "external" || g.part.spec.kind === "internal" ? [{ label: "db", radius: info.base_diameter / 2 }] : []),
      ];
      out.push({ view: v.name, center: [dot(mid, v.dir.xdir), dot(mid, up)], radius: g.pitch_radius, circles });
    }
  }
  return out;
}

function norm(a: P3): P3 {
  const l = Math.hypot(...a) || 1;
  return [a[0] / l, a[1] / l, a[2] / l];
}

/** Posición del rótulo de un círculo de radio `r` (a 45° arriba a la derecha del centro) */
export const circleLabelAt = (center: P2, r: number): P2 => [center[0] + r * Math.SQRT1_2, center[1] - r * Math.SQRT1_2];

/** La tabla de datos de un engrane (los datos ya vienen calculados de `gearInfo`) */
export function gearTable(gear: GearAxis, info: GearInfo, name: string): GearTable {
  const { spec } = gear.part;
  const rows: [string, string][] = [];
  const add = (label: string, value: string) => rows.push([tr(label), value]);
  const lead = (spec.teeth * Math.PI * spec.module).toFixed(3);
  if (spec.kind === "worm") {
    add("Módulo axial (m)", num(spec.module));
    add("Entradas (z₁)", String(spec.teeth));
    add("Ángulo de presión (α)", `${num(spec.pressure_angle, 2)}°`);
    add("Ø primitivo", num(info.pitch_diameter));
    add("Ø exterior", num(info.tip_diameter));
    add("Ø de pie", num(info.root_diameter));
    add("Avance", num(+lead));
    add("Ángulo de avance (γ)", `${num(Math.atan((spec.teeth * spec.module) / info.pitch_diameter) * (180 / Math.PI), 2)}°`);
    add("Mano", spec.left_hand ? tr("Izquierda") : tr("Derecha"));
  } else if (spec.kind === "rack") {
    add("Módulo (m)", num(spec.module));
    add("Dientes (z)", String(spec.teeth));
    add("Ángulo de presión (α)", `${num(spec.pressure_angle, 2)}°`);
    add("Paso", num(Math.PI * spec.module));
    if (spec.shift) add("Corrección (x)", num(spec.shift, 3));
  } else {
    add("Módulo (m)", num(spec.module));
    add("Dientes (z)", String(spec.teeth));
    add("Ángulo de presión (α)", `${num(spec.pressure_angle, 2)}°`);
    if (spec.helix_angle) add("Ángulo de hélice (β)", `${num(spec.helix_angle, 2)}°`);
    if (spec.kind === "bevel") {
      add("Ángulo del cono (δ)", `${num(spec.cone_angle, 2)}°`);
      add("Ángulo entre ejes (Σ)", `${num(spec.shaft_angle, 2)}°`);
      if (spec.spiral_angle) add("Ángulo de espiral (β)", `${num(spec.spiral_angle, 2)}°`);
    }
    if (spec.shift) add("Corrección (x)", num(spec.shift, 3));
    add("Ø primitivo", num(info.pitch_diameter));
    add("Ø exterior", num(info.tip_diameter));
    add("Ø de pie", num(info.root_diameter));
    if (spec.kind !== "bevel") add("Ø base", num(info.base_diameter));
    if (info.span > 0) add("Cota sobre {k} dientes", num(info.span));
    if (info.over_pins > 0) add(spec.kind === "internal" ? "Cota entre bolas Ø{d}" : "Cota sobre bolas Ø{d}", num(info.over_pins));
    if (spec.backlash) add("Juego", num(spec.backlash));
  }
  add("Ancho", num(gear.part.width));
  // Las etiquetas con datos se arman al final (el idioma las traduce enteras)
  return {
    title: name,
    rows: rows.map(([l, v]) => [l.replace("{k}", String(info.span_teeth)).replace("{d}", num(info.pin_diameter, 2)), v]),
  };
}

/** Tamaño de las tablas en la hoja (mm) */
export const GEAR_TABLE = { width: 64, labelWidth: 40, row: 4.2, title: 5, gap: 2 };

/** Dónde va cada tabla: se apilan hacia arriba desde el cajetín, pegadas a la derecha */
export function gearTableBoxes(tables: GearTable[], sheetWidth: number, sheetHeight: number, margin: number, titleHeight: number): { table: GearTable; x: number; y: number; w: number; h: number }[] {
  const out = [];
  let bottom = sheetHeight - margin - titleHeight - GEAR_TABLE.gap;
  for (const table of tables) {
    const h = GEAR_TABLE.title + table.rows.length * GEAR_TABLE.row;
    const y = bottom - h;
    out.push({ table, x: sheetWidth - margin - GEAR_TABLE.width, y, w: GEAR_TABLE.width, h });
    bottom = y - GEAR_TABLE.gap;
  }
  return out;
}
