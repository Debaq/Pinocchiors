// Planos 2D del documento: cada plano con su hoja, escala, vistas, corte,
// detalles, cotas a mano, dibujo y cajetín (en `doc.drawing.sheets`), los ids
// de sus elementos en el Outliner y el árbol que cuelga del grupo «Planos 2D».
// Funciones puras: se prueban con node (e2e/drawingDoc.test.mjs).
//
// Antes había un solo plano con `doc.drawing = { dims, details, sketch }` y el
// resto en la interfaz: se lee como «Plano 1» y se reescribe al primer cambio.

import type { CadDocument, Sketch } from "./cad.ts";
import type { Detail, Projection, UserDim, ViewName } from "./drawing.ts";
import type { SceneNode } from "./scene-tree.ts";

export interface DrawingSheet {
  id: number;
  name: string;
  /** Tamaño de hoja ("A4"…) */
  sheet: string;
  projection: Projection;
  /** Escala fija; sin ella, la normalizada más grande con la que todo entra */
  scale?: number | null;
  /** Líneas ocultas y tangentes */
  hiddenLines: boolean;
  smooth: boolean;
  /** Cotas generales (ancho y alto de cada vista) */
  overall: boolean;
  /** Vistas que no se muestran */
  hiddenViews: ViewName[];
  /** Ya con las seis vistas (antes había una sola lateral, "side") */
  sixViews?: boolean;
  /** Corte A-A paralelo al frente: `y` del plano (mm del modelo; sin ella, por el medio) */
  section?: { y?: number | null; hidden?: boolean } | null;
  title: string;
  author: string;
  dims: UserDim[];
  details: Detail[];
  /** Lo dibujado a mano sobre la hoja (mm de hoja, Y hacia arriba) */
  sketch?: Sketch | null;
  sketchHidden?: boolean;
}

/** Lo que se guarda en `doc.drawing` (la forma vieja también se acepta) */
export interface DrawingData {
  sheets?: DrawingSheet[];
  next_id?: number;
  /** Forma vieja (un solo plano) */
  dims?: UserDim[];
  details?: Detail[];
  sketch?: Sketch | null;
}

export const VIEW_NAMES: ViewName[] = ["front", "top", "bottom", "left", "right", "back", "iso"];
export const VIEW_LABELS: Record<ViewName, string> = {
  front: "Frente",
  top: "Superior (planta)",
  bottom: "Inferior",
  left: "Lateral izquierda",
  right: "Lateral derecha",
  back: "Posterior",
  iso: "Isométrica",
};

/** Las que no se ven al empezar: frente, superior, una lateral (la que va a la derecha del frente) e isométrica a la vista */
export const defaultHiddenViews = (projection: Projection): ViewName[] => ["bottom", projection === "first" ? "right" : "left", "back"];
const DIM_LABELS: Record<UserDim["kind"], string> = { length: "Largo", diameter: "Diámetro", radius: "Radio", distance: "Distancia", angle: "Ángulo" };

export function newSheet(id: number, name = `Plano ${id}`): DrawingSheet {
  return { id, name, sheet: "A4", projection: "first", scale: null, hiddenLines: true, smooth: false, overall: true, hiddenViews: defaultHiddenViews("first"), sixViews: true, section: null, title: "Diseño", author: "", dims: [], details: [], sketch: null };
}

/**
 * Planos de antes de las seis vistas: la lateral ("side") era la que va a la
 * derecha del frente (la izquierda en primer diedro, la derecha en tercero);
 * las vistas nuevas empiezan ocultas.
 */
function sixViews(s: DrawingSheet): DrawingSheet {
  if (s.sixViews) return s;
  const side: ViewName = s.projection === "third" ? "right" : "left";
  const map = (v: string) => (v === "side" ? side : v);
  const hidden = new Set<ViewName>([...defaultHiddenViews(s.projection), ...(s.hiddenViews as string[]).map(map)] as ViewName[]);
  if (!(s.hiddenViews as string[]).includes("side")) hidden.delete(side);
  return {
    ...s,
    sixViews: true,
    hiddenViews: VIEW_NAMES.filter((v) => hidden.has(v)),
    dims: s.dims.map((d) => ({ ...d, view: map(d.view) })),
    details: s.details.map((d) => ({ ...d, view: map(d.view) as ViewName })),
  };
}

/** Los planos del documento (el viejo, como «Plano 1»); no cambia el documento */
export function drawingSheets(doc: Pick<CadDocument, "drawing"> | null | undefined): DrawingSheet[] {
  const d = doc?.drawing as DrawingData | null | undefined;
  if (!d) return [];
  if (d.sheets) return d.sheets.map((s) => sixViews({ ...newSheet(s.id), sixViews: false, ...s }));
  if (d.dims?.length || d.details?.length || d.sketch) return [sixViews({ ...newSheet(1), sixViews: false, hiddenViews: [], dims: d.dims ?? [], details: d.details ?? [], sketch: d.sketch ?? null })];
  return [];
}

/** Escribe los planos en el documento (con la forma nueva) */
function store(doc: Pick<CadDocument, "drawing">, sheets: DrawingSheet[]) {
  const d = (doc.drawing ?? {}) as DrawingData;
  const next = Math.max(d.next_id ?? 1, ...sheets.map((s) => s.id + 1));
  doc.drawing = { sheets, next_id: next } as CadDocument["drawing"];
}

/** Agrega un plano nuevo y devuelve su id */
export function addDrawing(doc: Pick<CadDocument, "drawing">): number {
  const sheets = drawingSheets(doc);
  const d = (doc.drawing ?? {}) as DrawingData;
  const id = Math.max(d.sheets ? (d.next_id ?? 1) : 1, ...sheets.map((s) => s.id + 1));
  store(doc, [...sheets, newSheet(id)]);
  return id;
}

/** Cambia el plano `id` (si existe) */
export function updateDrawing(doc: Pick<CadDocument, "drawing">, id: number, change: (s: DrawingSheet) => void): boolean {
  const sheets = drawingSheets(doc);
  const s = sheets.find((x) => x.id === id);
  if (!s) return false;
  change(s);
  store(doc, sheets);
  return true;
}

export function removeDrawing(doc: Pick<CadDocument, "drawing">, id: number) {
  store(
    doc,
    drawingSheets(doc).filter((s) => s.id !== id),
  );
}

// ─── Elementos ──────────────────────────────────────────────────────────────

/** Un elemento de un plano: lo que se elige en el Outliner o en la hoja */
export type DrawingElement =
  | { kind: "sheet" }
  | { kind: "view"; view: ViewName }
  | { kind: "overall" }
  | { kind: "section" }
  | { kind: "detail"; label: string }
  | { kind: "dim"; index: number }
  | { kind: "sketch" };

export const sameElement = (a: DrawingElement | undefined, b: DrawingElement | undefined) => !!a && !!b && JSON.stringify(a) === JSON.stringify(b);

/** Id en el Outliner: `drawing-<id>` el plano, `drawing-<id>:<elemento>` lo suyo */
export function elementNodeId(drawing: number, el?: DrawingElement): string {
  if (!el) return `drawing-${drawing}`;
  const key = el.kind === "view" ? `view:${el.view}` : el.kind === "detail" ? `detail:${el.label}` : el.kind === "dim" ? `dim:${el.index}` : el.kind;
  return `drawing-${drawing}:${key}`;
}

export function parseElementNode(id: string): { drawing: number; element?: DrawingElement } | undefined {
  const m = /^drawing-(\d+)(?::(\w+)(?::(.+))?)?$/.exec(id);
  if (!m) return undefined;
  const drawing = Number(m[1]);
  const [kind, arg] = [m[2], m[3]];
  if (!kind) return { drawing };
  if (kind === "view" && VIEW_NAMES.includes(arg as ViewName)) return { drawing, element: { kind, view: arg as ViewName } };
  if (kind === "detail" && arg) return { drawing, element: { kind, label: arg } };
  if (kind === "dim" && arg !== undefined && /^\d+$/.test(arg)) return { drawing, element: { kind, index: Number(arg) } };
  if (kind === "sheet" || kind === "overall" || kind === "section" || kind === "sketch") return { drawing, element: { kind } };
  return undefined;
}

/** Si el elemento existe en el plano */
export function hasElement(s: DrawingSheet, el: DrawingElement): boolean {
  switch (el.kind) {
    case "section":
      return !!s.section;
    case "detail":
      return s.details.some((d) => d.label === el.label);
    case "dim":
      return el.index >= 0 && el.index < s.dims.length;
    case "sketch":
      return !!s.sketch;
    default:
      return true;
  }
}

/** Se ve en la hoja (la hoja misma, siempre) */
export function elementVisible(s: DrawingSheet, el: DrawingElement): boolean {
  switch (el.kind) {
    case "view":
      return !s.hiddenViews.includes(el.view);
    case "overall":
      return s.overall;
    case "section":
      return !!s.section && !s.section.hidden;
    case "detail":
      return !s.details.find((d) => d.label === el.label)?.hidden;
    case "dim":
      return !s.dims[el.index]?.hidden;
    case "sketch":
      return !!s.sketch && !s.sketchHidden;
    default:
      return true;
  }
}

/** Muestra u oculta (la hoja no se oculta) */
export function toggleElement(s: DrawingSheet, el: DrawingElement) {
  const on = !elementVisible(s, el);
  switch (el.kind) {
    case "view":
      s.hiddenViews = on ? s.hiddenViews.filter((v) => v !== el.view) : [...s.hiddenViews, el.view];
      break;
    case "overall":
      s.overall = on;
      break;
    case "section":
      if (s.section) s.section = { ...s.section, hidden: !on };
      break;
    case "detail":
      s.details = s.details.map((d) => (d.label === el.label ? { ...d, hidden: !on } : d));
      break;
    case "dim":
      s.dims = s.dims.map((d, i) => (i === el.index ? { ...d, hidden: !on } : d));
      break;
    case "sketch":
      s.sketchHidden = !on;
      break;
  }
}

/** Se puede borrar (las vistas, las cotas generales y la hoja solo se ocultan) */
export const elementDeletable = (el: DrawingElement) => el.kind === "section" || el.kind === "detail" || el.kind === "dim" || el.kind === "sketch";

export function removeElement(s: DrawingSheet, el: DrawingElement) {
  if (el.kind === "section") s.section = null;
  else if (el.kind === "detail") s.details = s.details.filter((d) => d.label !== el.label);
  else if (el.kind === "dim") s.dims = s.dims.filter((_, i) => i !== el.index);
  else if (el.kind === "sketch") {
    s.sketch = null;
    s.sketchHidden = false;
  }
}

export function elementLabel(s: DrawingSheet, el: DrawingElement): string {
  switch (el.kind) {
    case "sheet":
      return "Hoja y cajetín";
    case "view":
      return VIEW_LABELS[el.view];
    case "overall":
      return "Cotas generales";
    case "section":
      return "Corte A-A";
    case "detail":
      return `Detalle ${el.label}`;
    case "dim": {
      const d = s.dims[el.index];
      if (!d) return "Cota";
      const view = (VIEW_LABELS as Record<string, string>)[d.view] ?? d.view.replace(/^detalle-/, "Detalle ");
      return `${DIM_LABELS[d.kind]} · ${view}`;
    }
    case "sketch": {
      const n = s.sketch?.entities.length ?? 0;
      return `Dibujo (${n} ${n === 1 ? "curva" : "curvas"})`;
    }
  }
}

/** Los elementos de un plano, en el orden del Outliner */
export function sheetElements(s: DrawingSheet): DrawingElement[] {
  return [
    { kind: "sheet" },
    ...VIEW_NAMES.map((view): DrawingElement => ({ kind: "view", view })),
    { kind: "overall" },
    ...(s.section ? [{ kind: "section" } as const] : []),
    ...s.details.map((d): DrawingElement => ({ kind: "detail", label: d.label })),
    ...s.dims.map((_, index): DrawingElement => ({ kind: "dim", index })),
    ...(s.sketch ? [{ kind: "sketch" } as const] : []),
  ];
}

const ELEMENT_TYPE: Record<DrawingElement["kind"], SceneNode["type"]> = {
  sheet: "drawing_sheet",
  view: "drawing_view",
  overall: "drawing_dim",
  section: "drawing_section",
  detail: "drawing_detail",
  dim: "drawing_dim",
  sketch: "drawing_sketch",
};

/**
 * Filas del grupo «Planos 2D»: cada plano con sus elementos. `open` es el
 * plano abierto y `selected` lo elegido en él (se marcan).
 */
export function drawingTree(sheets: DrawingSheet[], open: number | undefined, selected: DrawingElement | undefined): SceneNode[] {
  return sheets.map((s) => ({
    id: elementNodeId(s.id),
    type: "drawing",
    label: s.name,
    visible: true,
    readonly: true,
    deletable: true,
    expanded: s.id === open,
    selected: s.id === open && !selected,
    hint: `${s.sheet} · ${s.title || "sin título"}`,
    children: sheetElements(s).map((el) => ({
      id: elementNodeId(s.id, el),
      type: ELEMENT_TYPE[el.kind],
      label: elementLabel(s, el),
      visible: elementVisible(s, el),
      readonly: el.kind === "sheet",
      deletable: elementDeletable(el),
      expanded: false,
      selected: s.id === open && sameElement(selected, el),
      children: [],
    })),
  }));
}
