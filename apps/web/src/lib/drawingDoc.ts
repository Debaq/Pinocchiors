// Planos 2D del documento: cada plano con su hoja, escala, vistas, corte,
// detalles, cotas a mano, dibujo y cajetín (en `doc.drawing.sheets`), los ids
// de sus elementos en el Outliner y el árbol que cuelga del grupo «Planos 2D».
// Funciones puras: se prueban con node (e2e/drawingDoc.test.mjs).
//
// Antes había un solo plano con `doc.drawing = { dims, details, sketch }` y el
// resto en la interfaz: se lee como «Plano 1» y se reescribe al primer cambio.

import type { CadDocument, Sketch } from "./cad.ts";
import { t as tr } from "./i18n.ts";
import { VIEW_LABELS, scaleLabel, viewSpec, type ViewDir, type Detail, type Projection, type UserDim, type ViewName } from "./drawing.ts";
import type { SceneNode } from "./scene-tree.ts";
import type { SheetLink, SheetOwner } from "./sheetRefs.ts";
import { sectionViewName, type BreakDef, type HatchStyle, type SectionDef } from "./drawingCuts.ts";

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
  /** Vistas puestas a mano: dónde va su origen en la hoja (mm, Y hacia abajo como el SVG) */
  viewAt?: Partial<Record<string, [number, number]>>;
  /** Escala propia de una vista (sin ella, la de la hoja); se rotula debajo */
  viewScales?: Partial<Record<string, number | null>>;
  /** Dirección de la isométrica girada y de las vistas nuevas (por nombre de vista) */
  viewDirs?: Partial<Record<string, ViewDir>>;
  /** Vistas nuevas (`vista-<id>`): miran desde donde se las gire, con su nombre debajo */
  customViews?: { id: number; name: string; hidden?: boolean }[];
  /** Cortes por una línea dibujada en una vista (cada uno con su vista `corte-<letra>`) */
  sections?: SectionDef[];
  /** Medio corte de una vista (qué mitad se ve cortada) */
  halves?: Partial<Record<string, "right" | "left">>;
  /** Roturas (cortes locales) de cada vista */
  breaks?: Partial<Record<string, BreakDef[]>>;
  /** Vistas 3D cortadas: por un corte (su letra) o un cuarto («quarter») */
  cutBy?: Partial<Record<string, string>>;
  /** Vistas sombreadas (como imagen), con las líneas encima o sin ellas */
  shaded?: Partial<Record<string, { lines: boolean }>>;
  /** Rayado de la hoja (los cortes pueden tener el suyo); sin `hatchByMaterial` false, según el material */
  hatch?: HatchStyle;
  hatchByMaterial?: boolean;
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
  /** Entidades del dibujo ligadas a líneas de las vistas (siguen al modelo) */
  links?: SheetLink[];
  /** Lo dibujado encima de una vista (se mueve y se escala con ella) */
  owners?: SheetOwner[];
  sketchHidden?: boolean;
  /** Engranes (ids de sus operaciones) con círculo primitivo y tabla de datos */
  gears?: number[];
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
export { VIEW_LABELS };

/** Las que no se ven al empezar: frente, superior, una lateral (la que va a la derecha del frente) e isométrica a la vista */
export const defaultHiddenViews = (projection: Projection): ViewName[] => ["bottom", projection === "first" ? "right" : "left", "back"];
const DIM_LABELS: Record<UserDim["kind"], string> = {
  get length() { return tr("Largo"); },
  get diameter() { return tr("Diámetro"); },
  get radius() { return tr("Radio"); },
  get distance() { return tr("Distancia"); },
  get angle() { return tr("Ángulo"); },
};

export function newSheet(id: number, name = tr("Plano {id}", { id })): DrawingSheet {
  return { id, name, sheet: "A4", projection: "first", scale: null, hiddenLines: true, smooth: false, overall: true, hiddenViews: defaultHiddenViews("first"), sixViews: true, section: null, title: tr("Diseño"), author: "", dims: [], details: [], sketch: null };
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
  | { kind: "custom"; id: number }
  | { kind: "cut"; label: string }
  | { kind: "break"; view: string; index: number }
  | { kind: "overall" }
  | { kind: "section" }
  | { kind: "detail"; label: string }
  | { kind: "dim"; index: number }
  | { kind: "sketch" };

export const sameElement = (a: DrawingElement | undefined, b: DrawingElement | undefined) => !!a && !!b && JSON.stringify(a) === JSON.stringify(b);

/** Id en el Outliner: `drawing-<id>` el plano, `drawing-<id>:<elemento>` lo suyo */
export function elementNodeId(drawing: number, el?: DrawingElement): string {
  if (!el) return `drawing-${drawing}`;
  const key =
    el.kind === "view"
      ? `view:${el.view}`
      : el.kind === "detail"
        ? `detail:${el.label}`
        : el.kind === "dim"
          ? `dim:${el.index}`
          : el.kind === "custom"
            ? `custom:${el.id}`
            : el.kind === "cut"
              ? `cut:${el.label}`
              : el.kind === "break"
                ? `break:${el.view}/${el.index}`
                : el.kind;
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
  if (kind === "custom" && arg !== undefined && /^\d+$/.test(arg)) return { drawing, element: { kind, id: Number(arg) } };
  if (kind === "cut" && arg) return { drawing, element: { kind, label: arg } };
  const br = kind === "break" && arg ? /^(.+)\/(\d+)$/.exec(arg) : null;
  if (br) return { drawing, element: { kind: "break", view: br[1], index: Number(br[2]) } };
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
    case "custom":
      return !!s.customViews?.some((v) => v.id === el.id);
    case "cut":
      return !!s.sections?.some((c) => c.label === el.label);
    case "break":
      return !!s.breaks?.[el.view]?.[el.index];
    default:
      return true;
  }
}

/** Nombre de vista (en la hoja, `Placed.name`) de una vista nueva */
export const customViewName = (id: number) => `vista-${id}`;

/** Agrega una vista nueva mirando como la isométrica; devuelve su id */
export function addCustomView(s: DrawingSheet): number {
  const id = Math.max(0, ...(s.customViews ?? []).map((v) => v.id)) + 1;
  s.customViews = [...(s.customViews ?? []), { id, name: tr("Vista {id}", { id }) }];
  s.viewDirs = { ...(s.viewDirs ?? {}), [customViewName(id)]: viewSpec("iso") };
  return id;
}

/** Desde dónde mira una vista del plano (la guardada, o la de siempre) */
export function viewDirOf(s: DrawingSheet, name: string): ViewDir | undefined {
  const saved = s.viewDirs?.[name];
  if (saved) return saved;
  return (VIEW_NAMES as string[]).includes(name) ? viewSpec(name as ViewName) : undefined;
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
    case "custom":
      return !s.customViews?.find((v) => v.id === el.id)?.hidden;
    case "cut":
      return !s.sections?.find((c) => c.label === el.label)?.hidden;
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
    case "custom":
      s.customViews = (s.customViews ?? []).map((v) => (v.id === el.id ? { ...v, hidden: !on } : v));
      break;
    case "cut":
      s.sections = (s.sections ?? []).map((c) => (c.label === el.label ? { ...c, hidden: !on } : c));
      break;
  }
}

/** Se puede borrar (las vistas, las cotas generales y la hoja solo se ocultan) */
export const elementDeletable = (el: DrawingElement) =>
  el.kind === "section" || el.kind === "detail" || el.kind === "dim" || el.kind === "sketch" || el.kind === "custom" || el.kind === "cut" || el.kind === "break";

/** Saca lo que cuelga de una vista que se va (cotas, detalles, lugar, escala, cortes de ella) */
function dropView(s: DrawingSheet, name: string) {
  s.dims = s.dims.filter((d) => d.view !== name);
  s.details = s.details.filter((d) => d.view !== name);
  for (const k of ["viewDirs", "viewAt", "viewScales", "halves", "breaks", "cutBy", "shaded"] as const) {
    const m = { ...((s[k] as Record<string, unknown> | undefined) ?? {}) };
    delete m[name];
    (s as unknown as Record<string, unknown>)[k] = m;
  }
}

/** Letra libre para un corte o un detalle (la A es del corte A-A) */
export function nextLabel(s: DrawingSheet): string {
  const used = new Set([...s.details.map((d) => d.label), ...(s.sections ?? []).map((c) => c.label)]);
  return "BCDEFGHJKLMNPQRSTUVWXYZ".split("").find((l) => !used.has(l)) ?? `${used.size + 1}`;
}

export function removeElement(s: DrawingSheet, el: DrawingElement) {
  if (el.kind === "section") s.section = null;
  else if (el.kind === "detail") s.details = s.details.filter((d) => d.label !== el.label);
  else if (el.kind === "dim") s.dims = s.dims.filter((_, i) => i !== el.index);
  else if (el.kind === "sketch") {
    s.sketch = null;
    s.sketchHidden = false;
  } else if (el.kind === "custom") {
    s.customViews = (s.customViews ?? []).filter((v) => v.id !== el.id);
    dropView(s, customViewName(el.id));
  } else if (el.kind === "cut") {
    s.sections = (s.sections ?? []).filter((c) => c.label !== el.label);
    dropView(s, sectionViewName(el.label));
    // Las vistas 3D que se cortaban con él vuelven a enteras
    s.cutBy = Object.fromEntries(Object.entries(s.cutBy ?? {}).filter(([, v]) => v !== el.label));
  } else if (el.kind === "break") {
    const list = [...(s.breaks?.[el.view] ?? [])];
    list.splice(el.index, 1);
    s.breaks = { ...(s.breaks ?? {}), [el.view]: list };
  }
}

export function elementLabel(s: DrawingSheet, el: DrawingElement): string {
  switch (el.kind) {
    case "sheet":
      return tr("Hoja y cajetín");
    case "view": {
      const k = s.viewScales?.[el.view];
      return k ? `${VIEW_LABELS[el.view]} (${scaleLabel(k)})` : VIEW_LABELS[el.view];
    }
    case "overall":
      return tr("Cotas generales");
    case "section":
      return tr("Corte A-A");
    case "detail":
      return tr("Detalle {label}", { label: el.label });
    case "dim": {
      const d = s.dims[el.index];
      if (!d) return tr("Cota");
      const view = (VIEW_LABELS as Record<string, string>)[d.view] ?? d.view.replace(/^detalle-/, tr("Detalle") + " ");
      return `${DIM_LABELS[d.kind]} · ${view}`;
    }
    case "sketch": {
      const n = s.sketch?.entities.length ?? 0;
      return tr(n === 1 ? "Dibujo ({n} curva)" : "Dibujo ({n} curvas)", { n });
    }
    case "custom": {
      const v = s.customViews?.find((x) => x.id === el.id);
      const k = s.viewScales?.[customViewName(el.id)];
      return `${v?.name ?? tr("Vista")}${k ? ` (${scaleLabel(k)})` : ""}`;
    }
    case "cut": {
      const k = s.viewScales?.[sectionViewName(el.label)];
      const c = s.sections?.find((x) => x.label === el.label);
      return tr("Corte {label}-{label}", { label: el.label }) + (c && c.points.length > 2 ? tr(" (escalonado)") : "") + (k ? ` (${scaleLabel(k)})` : "");
    }
    case "break":
      return tr("Rotura {n} · {view}", { n: el.index + 1, view: (VIEW_LABELS as Record<string, string>)[el.view] ?? el.view });
  }
}

/** Los elementos de un plano, en el orden del Outliner */
export function sheetElements(s: DrawingSheet): DrawingElement[] {
  return [
    { kind: "sheet" },
    ...VIEW_NAMES.map((view): DrawingElement => ({ kind: "view", view })),
    ...(s.customViews ?? []).map((v): DrawingElement => ({ kind: "custom", id: v.id })),
    ...(s.sections ?? []).map((c): DrawingElement => ({ kind: "cut", label: c.label })),
    ...Object.entries(s.breaks ?? {}).flatMap(([view, list]) => (list ?? []).map((_, index): DrawingElement => ({ kind: "break", view, index }))),
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
  custom: "drawing_view",
  cut: "drawing_section",
  break: "drawing_section",
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
    hint: `${s.sheet} · ${s.title || tr("sin título")}`,
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
