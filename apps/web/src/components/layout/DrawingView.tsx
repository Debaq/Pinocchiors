import { Component, For, Match, Show, Switch, createEffect, createMemo, createResource, createSignal, on, onCleanup, onMount, untrack, type JSX } from "solid-js";
import { clsx } from "clsx";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { partMaterial, type CadStore, type GearPart } from "../../lib/cad";
import { gearMarks, gearTable } from "../../lib/gearSheet";
import { bomRows, bomTable, placeBalloons } from "../../lib/drawingBom";
import type { CadUi, DrawingTool } from "../../lib/cadUi";
import * as D from "../icons/design";
import * as SketchIcons from "../icons/sketch";
import { SideButton, SideSep } from "../design/DesignToolbar";
import {
  BALLOON_R,
  SCALES,
  SHEETS,
  asArc,
  asCircle,
  asSegment,
  bounds,
  detailScale,
  dragAxis,
  isPictorial,
  placementProblems,
  sheetBox,
  placeExtraViews,
  projectPolylines,
  rotateViewDir,
  viewDirFromCamera,
  viewUp,
  type ViewDir,
  dimText,
  layout,
  lineNear,
  placeDetails,
  scaleLabel,
  sheetDxf,
  sheetLayerLines,
  sheetSvg,
  userDimensions,
  viewSpec,
  type CutMarks,
  type Detail,
  type DrawingLine,
  type P2,
  type P3,
  type Placed,
  type Projection,
  type ViewName,
  sheetTextBoxes,
} from "../../lib/drawing";
import {
  VIEW_LABELS,
  VIEW_NAMES,
  addCustomView,
  addDrawing,
  customViewName,
  nextLabel,
  viewDirOf,
  drawingSheets,
  elementDeletable,
  elementLabel,
  elementVisible,
  hasElement,
  removeElement,
  toggleElement,
  updateDrawing,
  type DrawingElement,
  type DrawingSheet,
} from "../../lib/drawingDoc";
import { applySheetLinks, applySheetOwners, sheetSketchDims } from "../../lib/sheetRefs";
import {
  DEFAULT_HATCH,
  breakPrism,
  materialHatch,
  partHatch,
  halfPrism,
  quarterPrism,
  reachOf,
  sectionArrow,
  sectionPrism,
  sectionViewDir,
  sectionViewName,
  viewCenter,
  type Box3,
  type CutPrism,
  type HatchStyle,
  type SectionDef,
} from "../../lib/drawingCuts";
import { dataUrlBytes, shadeView, type DrawingMesh } from "../../lib/drawingShade";
import { themeColor } from "../../lib/theme";
import { Button, Select } from "../ui";
import { t } from "../../lib/i18n";

type Tool = DrawingTool;

/**
 * Plano 2D del diseño: frente, planta, lateral e isométrica con líneas ocultas
 * exactas, escala normalizada y cajetín; se exporta a SVG, PDF y DXF. Se
 * rehace solo cuando cambia el diseño. Hay varios planos (en el Outliner, grupo
 * «Planos 2D»); lo elegido en la hoja o en el Outliner abre su ventana de
 * propiedades.
 */
export const DrawingView: Component<{ store: CadStore; ui: CadUi }> = (props) => {
  const ui = props.ui;
  const store = props.store;
  const sheets = () => drawingSheets(store.doc());
  // El abierto: el elegido o el primero; sin ninguno se crea «Plano 1»
  let creating = false;
  createEffect(() => {
    const list = sheets();
    if (!store.doc() || list.some((s) => s.id === ui.activeDrawing())) return;
    if (list.length) return ui.setActiveDrawing(list[0].id);
    if (creating) return;
    creating = true;
    let id: number | undefined;
    untrack(() =>
      void Promise.resolve(store.commit((d) => void (id = addDrawing(d)))).finally(() => {
        creating = false;
        if (id !== undefined) ui.setActiveDrawing(id);
      }),
    );
  });
  const d = createMemo(() => sheets().find((s) => s.id === ui.activeDrawing()));
  /** Cambia el plano abierto (un paso del historial del diseño) */
  const upd = (change: (s: DrawingSheet) => void) => void upd2(change);
  /** Igual, devolviendo la promesa del envío */
  const upd2 = (change: (s: DrawingSheet) => void) => {
    const id = ui.activeDrawing();
    return id !== undefined ? store.commit((doc) => void updateDrawing(doc, id, change)) : undefined;
  };
  const sel = () => {
    const s = d();
    const el = ui.drawingSel();
    return s && el && hasElement(s, el) ? el : undefined;
  };

  const tool = ui.drawingTool;
  const setTool = (t: Tool) => ui.setDrawingTool(t);
  // Al abrir, sin herramienta; al cerrar se suelta
  setTool("");
  onCleanup(() => {
    setTool("");
    ui.setDrawingActions(undefined);
  });
  // Vista que se está arrastrando: dónde va su origen (se guarda al soltar)
  const [dragAt, setDragAt] = createSignal<{ view: string; at: P2 }>();
  let drag: { view: string; x0: number; y0: number; at: P2; k: number; axis: "x" | "y" | "free" } | undefined;
  // Girando una vista 3D con Ctrl+arrastrar: se ve una vista previa con las aristas del sólido; al soltar se pide la vista exacta
  let orbit: { name: string; x0: number; y0: number; dir: ViewDir } | undefined;
  const [orbitDir, setOrbitDir] = createSignal<{ name: string; dir: ViewDir }>();
  let dragged = false;
  let firstLine: { view: string; ref: P2 } | null = null;
  // Corte o rotura a medias: la vista y los puntos (coordenadas de la vista)
  const [cutPts, setCutPts] = createSignal<{ view: string; pts: P2[] }>();
  /** ¿El punto de la hoja (mm, Y abajo) cae en la caja de la vista (con margen en mm de hoja)? */
  const insideBox = (v: Placed, pt: { x: number; y: number }, pad: number) => {
    const b = sheetBox(v);
    return pt.x >= b[0] - pad && pt.x <= b[2] + pad && pt.y >= b[1] - pad && pt.y <= b[3] + pad;
  };
  const [views, setViews] = createSignal<Partial<Record<string, DrawingLine[]>>>();
  const [hatch, setHatch] = createSignal<Partial<Record<string, [P2, P2, P2][]>>>({});
  // Con varias piezas: de qué pieza es cada triángulo del rayado
  const [hatchParts, setHatchParts] = createSignal<Partial<Record<string, number[]>>>({});
  const [sectionAt, setSectionAt] = createSignal(0);
  const [busy, setBusy] = createSignal(false);
  const [message, setMessage] = createSignal<string>();
  let seq = 0;

  const projection = (): Projection => d()?.projection ?? "first";
  // Lo que se pide al backend (nombre y desde dónde se mira), como texto: el
  // efecto corre solo si cambia. La que se está girando va con su giro del momento
  // Caja de la pieza (para que los prismas de corte la atraviesen entera)
  // En un ensamble, la caja de las instancias en su lugar y el centro de cada una (para los globos)
  const inAssembly = () => store.activeTab()?.kind === "assembly";
  const [info, setInfo] = createSignal<{ min: P3; max: P3; items: { instance: number; center: P3 }[] }>();
  let infoSeq = 0;
  createEffect(
    on([() => store.result()?.version, inAssembly], ([, asm]) => {
      const n = ++infoSeq;
      if (!asm) return setInfo(undefined);
      invoke<{ min: P3; max: P3; items: { instance: number; center: P3 }[] }>("cad_drawing_info").then(
        (i) => n === infoSeq && setInfo(i),
        () => n === infoSeq && setInfo(undefined),
      );
    }),
  );
  const box = createMemo((): Box3 | undefined => {
    if (inAssembly()) {
      const i = info();
      return i ? { min: i.min, max: i.max } : undefined;
    }
    const b = store.result()?.body;
    return b ? { min: b.bbox_min as P3, max: b.bbox_max as P3 } : undefined;
  });
  const requests = createMemo(() => {
    const s = d();
    if (!s) return "[]";
    const bx = box();
    const reach = bx ? reachOf(bx) : 1000;
    const out: { name: string; eye: P3; xdir: P3; cut: CutPrism[]; shaded: boolean }[] = [];
    const sectionCut = (label: string) => {
      const def = s.sections?.find((c) => c.label === label);
      const parent = def && viewDirOf(s, def.view);
      return def && parent && def.points.length >= 2 ? sectionPrism(def, parent, reach) : undefined;
    };
    const names = [...VIEW_NAMES.filter((v) => !s.hiddenViews.includes(v)), ...(s.customViews ?? []).filter((v) => !v.hidden).map((v) => customViewName(v.id))];
    for (const name of names) {
      const dir = viewDirOf(s, name)!;
      const cut: CutPrism[] = [];
      const half = s.halves?.[name];
      if (half && bx && !isPictorial(name)) cut.push(halfPrism(dir, bx, half));
      if (bx) for (const br of s.breaks?.[name] ?? []) if (br.points.length >= 3) cut.push(breakPrism(dir, br, bx));
      const by = s.cutBy?.[name];
      if (by && bx && isPictorial(name)) {
        const p = by === "quarter" ? quarterPrism(bx) : sectionCut(by);
        if (p) cut.push(p);
      }
      out.push({ name, ...dir, cut, shaded: !!s.shaded?.[name] });
    }
    // La vista de cada corte: mira en el sentido de las flechas, sin lo de atrás de la línea
    for (const def of s.sections ?? []) {
      const parent = viewDirOf(s, def.view);
      const p = sectionCut(def.label);
      if (def.hidden || !parent || !p) continue;
      const name = sectionViewName(def.label);
      out.push({ name, ...sectionViewDir(def, parent), cut: [p], shaded: !!s.shaded?.[name] });
    }
    return JSON.stringify(out);
  });
  // Mallas de las vistas sombreadas (con su dirección), y sus imágenes
  const [images, setImages] = createSignal<Record<string, { href: string; min: P2; max: P2 }>>({});
  const sectionOn = createMemo(() => !!d()?.section && !d()!.section!.hidden);
  const sectionY = createMemo(() => d()?.section?.y ?? undefined);
  createEffect(
    on([() => store.result()?.version, requests, sectionOn, sectionY], async () => {
      const asked = JSON.parse(requests()) as { name: string; eye: P3; xdir: P3; cut: CutPrism[]; shaded: boolean }[];
      const names = asked.map((x) => x.name);
      const n = ++seq;
      if (!names.length) return setViews({});
      setBusy(true);
      try {
        // El corte va por el medio del sólido, paralelo al frente: se queda la mitad de atrás
        const bx = box();
        const at = sectionY() ?? (bx ? (bx.min[1] + bx.max[1]) / 2 : 0);
        const specs = asked.map(({ name, eye, xdir, cut, shaded }) =>
          name === "front" && sectionOn() ? { eye, xdir, cut, shaded, section: { origin: [0, at, 0], normal: [0, 1, 0] } } : { eye, xdir, cut, shaded },
        );
        const res = await invoke<{ lines: DrawingLine[]; hatch: [P2, P2, P2][]; hatch_part?: number[]; mesh?: DrawingMesh }[]>("cad_drawing", { views: specs });
        if (n === seq) {
          setViews(Object.fromEntries(names.map((v, i) => [v, res[i].lines])));
          setHatch(Object.fromEntries(names.map((v, i) => [v, res[i].hatch])));
          setHatchParts(Object.fromEntries(names.flatMap((v, i) => (res[i].hatch_part?.length ? [[v, res[i].hatch_part!]] : []))));
          // Las sombreadas: la malla dibujada mirando desde la vista, cubriendo lo que cubren sus líneas
          const imgs: Record<string, { href: string; min: P2; max: P2 }> = {};
          asked.forEach((a, i) => {
            const mesh = res[i].mesh;
            if (!a.shaded || !mesh || !res[i].lines.length) return;
            const b = bounds(res[i].lines);
            const href = shadeView(mesh, { eye: a.eye, xdir: a.xdir }, b.min, b.max, 1600 / Math.max(b.max[0] - b.min[0], b.max[1] - b.min[1], 1e-9));
            if (href) imgs[a.name] = { href, min: b.min, max: b.max };
          });
          setImages(imgs);
          setSectionAt(at);
          setMessage(undefined);
        }
      } catch (e) {
        if (n === seq) setMessage(String(e));
      } finally {
        if (n === seq) setBusy(false);
      }
    }),
  );

  const sheet = () => SHEETS.find((s) => s.name === d()?.sheet) ?? SHEETS[0];
  const result = createMemo(() => {
    const v = views();
    const s = d();
    if (!v || !s) return null;
    // Las puestas a mano (y la que se está arrastrando, donde va el puntero)
    const at: Partial<Record<string, P2>> = { ...(s.viewAt ?? {}) };
    const g = dragAt();
    if (g) at[g.view] = g.at;
    const std = Object.fromEntries(VIEW_NAMES.filter((n) => v[n]).map((n) => [n, v[n]!])) as Partial<Record<ViewName, DrawingLine[]>>;
    const extraViews = [
      ...(s.customViews ?? []).filter((c) => !c.hidden && v[customViewName(c.id)]).map((c) => ({ name: customViewName(c.id), title: c.name, lines: v[customViewName(c.id)]!, scale: s.viewScales?.[customViewName(c.id)] })),
      ...(s.sections ?? [])
        .filter((c) => !c.hidden && v[sectionViewName(c.label)])
        .map((c) => ({ name: sectionViewName(c.label), title: t("Corte {l}-{l}", { l: c.label }), lines: v[sectionViewName(c.label)]!, scale: s.viewScales?.[sectionViewName(c.label)], align: sectionAlign(s, c) })),
    ];
    // Las vistas nuevas y las de los cortes, en el lugar libre (o donde se dejaron). Con la
    // escala automática, la mayor con la que también ellas entran en la hoja
    const arrange = (fixed?: number) => {
      const r = layout(std, sheet(), projection(), fixed, hatch() as Partial<Record<ViewName, [P2, P2, P2][]>>, s.viewScales, at);
      return { r, extra: placeExtraViews(r.placed, extraViews, r.scale, sheet(), at) };
    };
    let { r, extra } = arrange(s.scale ?? undefined);
    if (!s.scale && !extra.fits)
      for (const k of SCALES.filter((x) => x < r.scale)) {
        const next = arrange(k);
        ({ r, extra } = next);
        if (extra.fits) break;
      }
    const extras = extra.placed;
    // Cada vista con su rayado y su imagen sombreada. Rayado: el elegido para ese corte; si no,
    // según el material (cada pieza el suyo; las vecinas con el ángulo al revés) o el de la hoja
    const imgs = images();
    const doc = store.doc();
    const parts = store.result()?.parts ?? [];
    const byMaterial = s.hatchByMaterial !== false;
    const own = (name: string) => (name.startsWith("corte-") ? s.sections?.find((c) => sectionViewName(c.label) === name)?.hatch : undefined);
    const style = (name: string): HatchStyle => own(name) ?? (byMaterial ? materialHatch(doc?.material?.name) : (s.hatch ?? DEFAULT_HATCH));
    const partStyles = Object.fromEntries(parts.map((p, i) => [i, partHatch(partMaterial(doc, p)?.name, i)]));
    const dress = (p: Placed): Placed => {
      const hp = hatchParts()[p.name];
      return {
        ...p,
        hatch: p.hatch ?? (hatch()[p.name] as [P2, P2, P2][] | undefined),
        hatchStyle: style(p.name),
        hatchPart: hp,
        hatchStyles: hp && byMaterial && !own(p.name) ? partStyles : undefined,
        image: imgs[p.name] ? { ...imgs[p.name], lines: s.shaded?.[p.name]?.lines !== false } : undefined,
      };
    };
    const all = [...r.placed, ...extras].map(dress);
    return { ...r, placed: [...all, ...placeDetails(all, s.details.filter((x) => !x.hidden), sheet())] };
  });
  // Engranes que pidió el plano (con lo que se mide de cada uno)
  const pickedGears = createMemo(() => (d()?.gears ?? []).flatMap((id) => store.result()?.gears?.filter((g) => g.feature === id) ?? []));
  const [gearInfos] = createResource(
    () => JSON.stringify(pickedGears().map((g) => g.part)),
    (key) => Promise.all((JSON.parse(key) as GearPart[]).map((p) => store.gearInfo(p).catch(() => undefined))),
  );
  // Marcas de los cortes en las vistas: líneas de corte, la central del medio corte, las roturas
  const marks = createMemo((): CutMarks => {
    const s = d();
    const r = result();
    const bx = box();
    if (!s || !r) return {};
    const lines = (s.sections ?? []).filter((c) => !c.hidden && c.points.length >= 2).map((c) => ({ view: c.view, points: c.points, arrow: sectionArrow(c), label: c.label }));
    const centers = Object.entries(s.halves ?? {}).flatMap(([name, side]) => {
      const v = r.placed.find((p) => p.name === name && !p.detail);
      const dir = viewDirOf(s, name);
      if (!side || !v || !dir || !bx) return [];
      const c = viewCenter(dir, bx);
      const b = bounds(v.lines);
      return [{ view: name, a: [c[0], b.min[1] - 3 / v.scale] as P2, b: [c[0], b.max[1] + 3 / v.scale] as P2 }];
    });
    const breaks = Object.entries(s.breaks ?? {}).flatMap(([view, list]) => (list ?? []).map((br, index) => ({ view, index, points: br.points })));
    // Engranes elegidos: círculo primitivo donde el eje apunta a quien mira, y su tabla
    const picked = pickedGears();
    const views = r.placed.filter((p) => !p.detail && !isPictorial(p.name)).flatMap((p) => {
      const dir = viewDirOf(s, p.name);
      return dir ? [{ name: p.name, dir }] : [];
    });
    const infos = gearInfos() ?? [];
    const gears = gearMarks(picked, views, infos);
    const tables = picked.flatMap((g, i) => (infos[i] ? [gearTable(g, infos[i]!, store.doc()?.features.find((f) => f.id === g.feature)?.name ?? `#${g.feature}`)] : []));
    // Ensamble: la lista de materiales y sus globos sobre una vista
    const bom = inAssembly() ? bomRows(store.doc()?.assembly?.instances ?? [], store.result()?.parts ?? []) : [];
    if (bom.length && s.bom !== false) tables.unshift(bomTable(bom, t("Lista de materiales")));
    const balloons = (() => {
      if (!bom.length || s.balloons === null) return [];
      const name = s.balloons ?? (r.placed.some((p) => p.name === "iso" && !p.detail) ? "iso" : r.placed.find((p) => !p.detail)?.name);
      const v = name ? r.placed.find((p) => p.name === name && !p.detail) : undefined;
      const dir = name ? viewDirOf(s, name) : undefined;
      if (!v || !dir) return [];
      const up = viewUp(dir);
      const dot = (a: P3, b: P3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
      const items = info()?.items ?? [];
      const anchors = bom.flatMap((row) => {
        const it = items.find((x) => x.instance === row.instances[0]);
        return it ? [{ label: String(row.item), to: [dot(it.center, dir.xdir), dot(it.center, up)] as P2 }] : [];
      });
      return placeBalloons(anchors, bounds(v.lines), 10 / v.scale, (2 * BALLOON_R + 1.5) / v.scale).map((b) => ({ view: v.name, ...b }));
    })();
    return { lines, centers, breaks, gears, tables, balloons };
  });
  // Lo dibujado a mano sobre la hoja (un sketch en mm de hoja), con lo ligado
  // a las vistas donde están hoy sus líneas
  const linked = createMemo(() => {
    const s = d();
    const r = result();
    if (!s?.sketch) return undefined;
    if (!r) return { sketch: s.sketch, links: s.links ?? [], lost: 0 };
    // Primero lo dibujado encima de cada vista (se corre y se escala con ella), después lo ligado
    const owned = s.owners?.length ? applySheetOwners(s.sketch, s.owners, r.placed, sheet().height).sketch : s.sketch;
    if (!s.links?.length) return { sketch: owned, links: [], lost: 0 };
    return applySheetLinks(owned, s.links, r.placed, sheet().height);
  });
  const layer = createMemo(() => (d()?.sketchHidden ? [] : sheetLayerLines(linked()?.sketch)));
  const layerDims = createMemo(() => (d()?.sketchHidden ? [] : sheetSketchDims(linked()?.sketch, sheet().height)));
  // Los textos dibujados: las cotas no se les ponen encima
  const textBoxes = createMemo(() => (d()?.sketchHidden ? [] : sheetTextBoxes(linked()?.sketch, sheet().height)));
  const sheetOf = (withLayer: boolean) => {
    const r = result();
    const s = d();
    if (!r || !s) return "";
    const today = new Date().toISOString().slice(0, 10);
    return sheetSvg(
      r.placed,
      sheet(),
      {
        title: s.title,
        author: s.author,
        material: store.doc()?.material?.name ?? "",
        date: today,
        scale: scaleLabel(r.scale),
        projection: projection(),
        sheet: sheet().name,
      },
      {
        hidden: s.hiddenLines,
        smooth: s.smooth,
        dimensions: s.overall,
        section: sectionOn() ? { label: "A", at: sectionAt() } : undefined,
        userDims: s.dims,
        layer: withLayer ? layer() : undefined,
        layerDims: withLayer ? layerDims() : undefined,
        textBoxes: withLayer ? textBoxes() : undefined,
        marks: marks(),
      },
    );
  };
  const svg = createMemo(() => sheetOf(true));
  const fileBase = () => (d()?.title ?? "").replace(/[^\p{L}\p{N}_-]+/gu, "_") || "plano";

  /**
   * Dibujar sobre la hoja: se abre el editor del sketch con la hoja de fondo
   * (blanca, igual que acá). Al terminar se vuelve acá.
   */
  const startDrawing = async () => {
    const s = d();
    const r = result();
    if (!s || !r) return;
    const sh = sheet();
    try {
      const backdrop = await rasterSheet(sheetOf(false), sh.width, sh.height);
      // La de la vez anterior ya no se usa
      if (lastBackdrop) URL.revokeObjectURL(lastBackdrop);
      lastBackdrop = backdrop;
      const l = linked();
      ui.setDrawingOpen(false);
      ui.editSheet(l?.sketch ?? s.sketch, { drawing: s.id, width: sh.width, height: sh.height, backdrop, views: r.placed, hiddenLines: s.hiddenLines, links: [...(l?.links ?? [])] });
      if (l?.lost) ui.setMessage(l.lost === 1 ? t("Una curva ligada perdió su línea de la vista: quedó donde estaba") : t("{n} curvas ligadas perdieron su línea de la vista: quedó donde estaba", { n: l.lost }));
    } catch (e) {
      setMessage(String(e));
    }
  };

  const write = async (ext: string, name: string, run: (path: string) => Promise<number>) => {
    const path = await save({ filters: [{ name: ext.toUpperCase(), extensions: [ext] }], defaultPath: name });
    if (!path) return;
    try {
      const bytes = await run(path);
      setMessage(t("Exportado ({kb} KB)", { kb: Math.round(bytes / 1024) }));
    } catch (e) {
      setMessage(String(e));
    }
  };
  const exportSvg = () => write("svg", `${fileBase()}.svg`, (path) => invoke<number>("cad_write_text", { path, content: svg() }));
  const exportPdf = () => write("pdf", `${fileBase()}.pdf`, (path) => invoke<number>("cad_write_pdf", { path, svg: svg() }));
  // DXF: la hoja entera o una vista sola a 1:1 (para corte láser o CNC)
  const [dxfWhat, setDxfWhat] = createSignal<"sheet" | ViewName>("sheet");
  const exportDxf = () => {
    const r = result();
    const s = d();
    if (!r || !s) return;
    const what = dxfWhat();
    return write("dxf", `${fileBase()}${what === "sheet" ? "" : `_${what}_1a1`}.dxf`, (path) =>
      invoke<number>("cad_write_text", {
        path,
        content: sheetDxf(r.placed, sheet(), { hidden: s.hiddenLines, smooth: s.smooth, dimensions: s.overall, userDims: s.dims, layer: layer(), layerDims: layerDims(), textBoxes: textBoxes(), marks: marks(), view: what === "sheet" ? undefined : what }),
      }),
    );
  };

  /** Punto de la hoja (mm, Y hacia abajo) bajo el puntero */
  const sheetPoint = (e: MouseEvent): DOMPoint | undefined => {
    const svgEl = (e.currentTarget as HTMLElement).querySelector("svg");
    const m = svgEl?.getScreenCTM();
    return m ? new DOMPoint(e.clientX, e.clientY).matrixTransform(m.inverse()) : undefined;
  };

  /** Una vista de la hoja como elemento: de las seis (o la isométrica) o una nueva */
  const asElement = (name: string): DrawingElement =>
    name.startsWith("vista-")
      ? { kind: "custom", id: Number(name.slice(6)) }
      : name.startsWith("corte-")
        ? { kind: "cut", label: name.slice(6) }
        : { kind: "view", view: name as ViewName };
  /** El nombre en la hoja de una vista elegida */
  const viewNameOf = (el: DrawingElement | undefined) =>
    el?.kind === "view" ? el.view : el?.kind === "custom" ? customViewName(el.id) : el?.kind === "cut" ? sectionViewName(el.label) : undefined;

  /** Sin herramienta: lo que está bajo el clic (el elemento del SVG o la vista que lo contiene) */
  const elementAt = (e: MouseEvent): DrawingElement | undefined => {
    const tg = e.target as Element;
    const dim = tg.closest("[data-user-dim]") as HTMLElement | null;
    if (dim?.dataset.userDim) return { kind: "dim", index: Number(dim.dataset.userDim) };
    const det = tg.closest("[data-detail], [data-detail-mark]") as HTMLElement | null;
    if (det) return { kind: "detail", label: det.dataset.detail ?? det.dataset.detailMark! };
    const cutLine = tg.closest("[data-cut-line]") as HTMLElement | null;
    if (cutLine?.dataset.cutLine) return { kind: "cut", label: cutLine.dataset.cutLine };
    const br = tg.closest("[data-break-line]") as HTMLElement | null;
    if (br?.dataset.breakLine) return { kind: "break", view: br.dataset.breakLine, index: Number(br.dataset.breakIndex ?? 0) };
    const hatched = (tg.closest("[data-hatch]") as HTMLElement | null)?.dataset.hatch;
    if (hatched?.startsWith("corte-")) return { kind: "cut", label: hatched.slice(6) };
    if (tg.closest("[data-hatch], [data-section-line], [data-section-label]")) return { kind: "section" };
    if (tg.closest("[data-layer]")) return { kind: "sketch" };
    if (tg.closest("[data-overall-dim]")) return { kind: "overall" };
    if (tg.closest("[data-title-block]")) return { kind: "sheet" };
    const view = (tg.closest("[data-view]") as HTMLElement | null)?.dataset.view;
    if (view?.startsWith("detalle-")) return { kind: "detail", label: view.slice(8) };
    if (view) return asElement(view);
    // Las líneas son finas: un clic dentro de la caja de una vista la elige
    const r = result();
    const pt = sheetPoint(e);
    if (!r || !pt) return undefined;
    for (const v of [...r.placed].sort((a, b) => Number(!!b.detail) - Number(!!a.detail))) {
      const p: P2 = [(pt.x - v.x) / v.scale, (v.y - pt.y) / v.scale];
      if (v.detail) {
        if (Math.hypot(p[0] - v.detail.center[0], p[1] - v.detail.center[1]) <= v.detail.radius) return { kind: "detail", label: v.detail.label };
        continue;
      }
      const b = bounds(v.lines);
      const pad = 2 / v.scale;
      if (p[0] >= b.min[0] - pad && p[0] <= b.max[0] + pad && p[1] >= b.min[1] - pad && p[1] <= b.max[1] + pad) return asElement(v.name);
    }
    return undefined;
  };

  /**
   * Clic en la hoja. Sin herramienta elige lo que hay (y abre sus
   * propiedades). "Cota": largo de una recta o diámetro de un círculo;
   * "Distancia": dos rectas paralelas, una después de la otra.
   */
  const onSheetClick = (e: MouseEvent) => {
    const r = result();
    const tk = tool();
    const s = d();
    if (!r || !s) return;
    // Fue el final de un arrastre
    if (dragged) {
      dragged = false;
      return;
    }
    if (!tk) return ui.setDrawingSel(elementAt(e));
    const pt = sheetPoint(e);
    if (!pt) return;
    // Corte o rotura: puntos sobre una vista (la del primer clic); doble clic o Enter termina
    if (tk === "section" || tk === "break") {
      const cur = cutPts();
      const v = cur ? r.placed.find((p) => p.name === cur.view && !p.detail) : r.placed.find((p) => !p.detail && !isPictorial(p.name) && !p.name.startsWith("corte-") && insideBox(p, pt, 6));
      if (!v) return setMessage(tk === "section" ? t("Clic sobre una vista (o al lado): ahí empieza la línea de corte") : t("Clic sobre una vista: el contorno de la rotura"));
      const p: P2 = [(pt.x - v.x) / v.scale, (v.y - pt.y) / v.scale];
      const pts = cur?.pts ?? [];
      const last = pts[pts.length - 1];
      if (last && Math.hypot(p[0] - last[0], p[1] - last[1]) * v.scale < 0.8) return;
      setCutPts({ view: v.name, pts: [...pts, p] });
      setMessage(tk === "section" ? t("Más clics para escalonarla; doble clic o Enter termina") : t("Más puntos del contorno; doble clic o Enter lo cierra"));
      return;
    }
    // Primero los detalles (están encima, ampliados)
    const order = [...r.placed].sort((a, b) => Number(!!b.detail) - Number(!!a.detail));
    for (const v of order) {
      if (isPictorial(v.name)) continue;
      const p: P2 = [(pt.x - v.x) / v.scale, (v.y - pt.y) / v.scale];
      if (v.detail && Math.hypot(p[0] - v.detail.center[0], p[1] - v.detail.center[1]) > v.detail.radius) continue;
      if (tk === "detail") {
        if (v.detail) return setMessage(t("El detalle se marca sobre una vista"));
        const b = bounds(v.lines);
        if (p[0] < b.min[0] || p[0] > b.max[0] || p[1] < b.min[1] || p[1] > b.max[1]) continue;
        // Radio inicial: un octavo de la vista; letras desde la B (la A es del corte)
        const radius = Math.round((Math.max(b.max[0] - b.min[0], b.max[1] - b.min[1]) / 8) * 10) / 10 || 5;
        const used = new Set(s.details.map((x) => x.label));
        const label = "BCDEFGHJKLMNPQRSTUVWXYZ".split("").find((l) => !used.has(l)) ?? `${s.details.length + 1}`;
        upd((x) => (x.details = [...x.details, { label, view: v.name as ViewName, center: p, radius }]));
        setTool("");
        ui.setDrawingSel({ kind: "detail", label });
        setMessage(undefined);
        return;
      }
      const line = lineNear(v, p, 3 / v.scale);
      if (!line) continue;
      if (tk === "dim") {
        const kind = asCircle(line) ? "diameter" : asSegment(line) ? "length" : asArc(line) ? "radius" : undefined;
        if (!kind) return setMessage(t("Esa línea no es recta, ni un círculo, ni un arco"));
        upd((x) => (x.dims = [...x.dims, { view: v.name, kind, refs: [p] }]));
        return;
      }
      if (!asSegment(line)) return setMessage(tk === "angle" ? t("Elegir rectas para el ángulo") : t("Elegir rectas para la distancia"));
      if (!firstLine || firstLine.view !== v.name) {
        firstLine = { view: v.name, ref: p };
        return setMessage(tk === "angle" ? t("Ahora la otra recta del ángulo") : t("Ahora la recta paralela"));
      }
      const first = firstLine;
      upd((x) => (x.dims = [...x.dims, { view: v.name, kind: tk === "angle" ? "angle" : "distance", refs: [first.ref, p] }]));
      firstLine = null;
      setMessage(undefined);
      return;
    }
  };

  /**
   * Arrastrar una vista la corre (alineada con el frente: las de su fila en
   * horizontal, las de su columna en vertical; con Mayús o Alt, libre). Queda
   * puesta a mano: deja de seguir a la disposición automática.
   */
  const onSheetDown = (e: PointerEvent) => {
    if (tool() || e.button !== 0) return;
    const el = elementAt(e);
    const name = viewNameOf(el);
    if (!name) return;
    const v = result()?.placed.find((p) => p.name === name && !p.detail);
    const m = (e.currentTarget as HTMLElement).querySelector("svg")?.getScreenCTM();
    if (!v || !m) return;
    // Ctrl+arrastrar una vista 3D (la isométrica o una nueva) la gira
    if ((e.ctrlKey || e.metaKey) && (name === "iso" || el!.kind === "custom")) {
      const dir = viewDirOf(d()!, name);
      if (!dir) return;
      orbit = { name, x0: e.clientX, y0: e.clientY, dir };
      dragged = false;
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
      return;
    }
    drag = { view: name, x0: e.clientX, y0: e.clientY, at: [v.x, v.y], k: 1 / m.a, axis: dragAxis(name, projection()) };
    dragged = false;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  };
  const onSheetMove = (e: PointerEvent) => {
    if (orbit) {
      if (!dragged && Math.hypot(e.clientX - orbit.x0, e.clientY - orbit.y0) < 4) return;
      dragged = true;
      // Medio grado por píxel: a la derecha gira la pieza a la derecha, hacia abajo la inclina hacia quien mira
      setOrbitDir({ name: orbit.name, dir: rotateViewDir(orbit.dir, (e.clientX - orbit.x0) * 0.5, (e.clientY - orbit.y0) * 0.5) });
      return;
    }
    if (!drag) return;
    const [dx, dy] = [(e.clientX - drag.x0) * drag.k, (e.clientY - drag.y0) * drag.k];
    if (!dragged && Math.hypot(e.clientX - drag.x0, e.clientY - drag.y0) < 4) return;
    dragged = true;
    const free = e.shiftKey || e.altKey || drag.axis === "free";
    setDragAt({ view: drag.view, at: [drag.at[0] + (free || drag.axis === "x" ? dx : 0), drag.at[1] + (free || drag.axis === "y" ? dy : 0)] });
  };
  const onSheetUp = () => {
    if (orbit) {
      const o = orbitDir();
      const name = orbit.name;
      orbit = undefined;
      if (!dragged || !o) return setOrbitDir(undefined);
      ui.setDrawingSel(asElement(name));
      void Promise.resolve(upd2((x) => (x.viewDirs = { ...(x.viewDirs ?? {}), [name]: o.dir }))).finally(() => setOrbitDir(undefined));
      return;
    }
    const g = dragAt();
    const was = drag;
    drag = undefined;
    if (!was || !dragged || !g) return;
    const id = ui.activeDrawing();
    ui.setDrawingSel({ kind: "view", view: g.view as ViewName });
    if (id === undefined) return setDragAt(undefined);
    // La vista queda donde se soltó hasta que llega el documento nuevo
    void Promise.resolve(store.commit((doc) => void updateDrawing(doc, id, (x) => (x.viewAt = { ...(x.viewAt ?? {}), [g.view]: g.at })))).finally(() => setDragAt(undefined));
  };
  /**
   * Dónde va la vista de un corte respecto de la vista de donde sale: alineada
   * (comparte su x o su y en la hoja) del lado que pide el diedro: en el
   * primero, hacia donde miran las flechas; en el tercero, del otro lado
   */
  const sectionAlign = (s: DrawingSheet, c: SectionDef): { parent: string; axis: "x" | "y"; dir: 1 | -1 } | undefined => {
    const parent = viewDirOf(s, c.view);
    if (!parent || isPictorial(c.view)) return undefined;
    const sec = sectionViewDir(c, parent);
    const dot = (a: P3, b: P3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    const a = sectionArrow(c);
    const toward = (projection() === "first" ? 1 : -1) as 1 | -1;
    // Misma derecha que la madre: va arriba o abajo (las flechas en vertical)
    if (dot(sec.xdir, parent.xdir) > 0.999 && Math.abs(a[1]) > 0.999) return { parent: c.view, axis: "x", dir: (a[1] > 0 ? -toward : toward) as 1 | -1 };
    // Mismo arriba: a un costado (las flechas en horizontal)
    if (dot(viewUp(sec), viewUp(parent)) > 0.999 && Math.abs(a[0]) > 0.999) return { parent: c.view, axis: "y", dir: (a[0] > 0 ? toward : -toward) as 1 | -1 };
    return undefined;
  };

  /** Los puntos del corte o de la rotura a medias, en la hoja */
  const cutPreview = createMemo(() => {
    const cur = cutPts();
    const v = cur && result()?.placed.find((p) => p.name === cur.view && !p.detail);
    if (!cur || !v) return undefined;
    const pts = cur.pts.map((p): P2 => [v.x + p[0] * v.scale, v.y - p[1] * v.scale]);
    const ring = tool() === "break" && pts.length > 2 ? [...pts, pts[0]] : pts;
    return { points: ring.map((p) => `${p[0].toFixed(2)},${p[1].toFixed(2)}`).join(" "), dots: pts };
  });

  /** Guarda la imagen sombreada de una vista como PNG */
  const savePng = async (name: string) => {
    const im = images()[name];
    if (!im) return setMessage(t("La vista no está sombreada"));
    const path = await save({ filters: [{ name: "PNG", extensions: ["png"] }], defaultPath: `${fileBase()}_${name}.png` });
    if (!path) return;
    try {
      const bytes = await invoke<number>("cad_write_bytes", { path, bytes: dataUrlBytes(im.href) });
      setMessage(t("Imagen guardada ({kb} KB)", { kb: Math.round(bytes / 1024) }));
    } catch (e) {
      setMessage(String(e));
    }
  };

  /** Avisos de las vistas puestas a mano (se salen de la hoja, pisan otra) */
  const problems = createMemo(() => {
    const r = result();
    const names: Record<string, string> = { ...VIEW_LABELS, ...Object.fromEntries((d()?.customViews ?? []).map((c) => [customViewName(c.id), c.name])) };
    return r ? placementProblems(r.placed, sheet(), names) : [];
  });

  /**
   * Vista previa del giro: las aristas del sólido (del visor, sin ocultar)
   * vistas desde la dirección del momento, donde está la vista en la hoja
   */
  const orbitPreview = createMemo(() => {
    const o = orbitDir();
    const v = o && result()?.placed.find((p) => p.name === o.name && !p.detail);
    const m = store.mesh();
    const info = ui.viewer();
    if (!o || !v || !m || !info) return "";
    const k = info.mmPerUnit();
    const lines: P3[][] = [];
    let start = 0;
    for (const end of m.edgeEnds ?? []) {
      const poly: P3[] = [];
      for (let i = start; i < end; i++) {
        const [x, y, z] = m.edgePoints.subarray(i * 3, i * 3 + 3);
        poly.push([x * k, -z * k, y * k]);
      }
      lines.push(poly);
      start = end;
    }
    // Centrada donde estaba la vista (su caja de antes)
    const pre = projectPolylines(lines, o.dir);
    const b0 = bounds(v.lines);
    const b1 = bounds(pre.map((points) => ({ kind: "visible" as const, points })));
    const [cx0, cy0] = [(b0.min[0] + b0.max[0]) / 2, (b0.min[1] + b0.max[1]) / 2];
    const [cx1, cy1] = [(b1.min[0] + b1.max[0]) / 2, (b1.min[1] + b1.max[1]) / 2];
    return pre.map((l) => l.map((p) => `${(v.x + (p[0] - cx1 + cx0) * v.scale).toFixed(2)},${(v.y - (p[1] - cy1 + cy0) * v.scale).toFixed(2)}`).join(" ")).join("|");
  });

  /**
   * Termina el corte (dos puntos o más: recto o escalonado) o la rotura (tres o
   * más: un contorno) y los elige (se abre su ventana)
   */
  const finishCut = () => {
    const cur = cutPts();
    const tk = tool();
    setCutPts(undefined);
    setMessage(undefined);
    if (!cur) return;
    if (tk === "section") {
      if (cur.pts.length < 2) return setMessage(t("El corte necesita dos puntos o más"));
      let label = "";
      void Promise.resolve(
        upd2((x) => {
          label = nextLabel(x);
          x.sections = [...(x.sections ?? []), { label, view: cur.view, points: cur.pts }];
        }),
      ).then(() => label && ui.setDrawingSel({ kind: "cut", label }));
      setTool("");
    } else if (tk === "break") {
      if (cur.pts.length < 3) return setMessage(t("La rotura necesita un contorno de tres puntos o más"));
      const s = d();
      const dir = s && viewDirOf(s, cur.view);
      const bx = box();
      // Hasta la mitad de lo hondo de la pieza en esa vista
      const depth = dir && bx ? Math.round(Math.abs((bx.max[0] - bx.min[0]) * dir.eye[0] + (bx.max[1] - bx.min[1]) * dir.eye[1] + (bx.max[2] - bx.min[2]) * dir.eye[2]) * 5) / 10 || 5 : 5;
      let index = 0;
      void Promise.resolve(
        upd2((x) => {
          const list = [...(x.breaks?.[cur.view] ?? [])];
          index = list.length;
          list.push({ points: cur.pts, depth });
          x.breaks = { ...(x.breaks ?? {}), [cur.view]: list };
        }),
      ).then(() => ui.setDrawingSel({ kind: "break", view: cur.view, index }));
      setTool("");
    }
  };

  // Esc suelta la herramienta o lo elegido; Supr borra lo elegido (si se puede); Enter termina un corte o una rotura
  const onKey = (e: KeyboardEvent) => {
    if ((e.target as HTMLElement | null)?.closest?.("input, textarea, select, [contenteditable]")) return;
    if (e.key === "Enter" && cutPts()) {
      finishCut();
    } else if (e.key === "Escape") {
      if (cutPts()) setCutPts(undefined);
      else if (tool()) setTool("");
      else ui.setDrawingSel(undefined);
      firstLine = null;
    } else if ((e.key === "Delete" || e.key === "Backspace") && sel() && elementDeletable(sel()!)) {
      const el = sel()!;
      upd((x) => removeElement(x, el));
      ui.setDrawingSel(undefined);
    } else return;
    e.preventDefault();
  };
  onMount(() => window.addEventListener("keydown", onKey));
  onCleanup(() => window.removeEventListener("keydown", onKey));

  // Lo elegido, resaltado en la hoja
  const highlight = createMemo(() => {
    const el = sel();
    if (!el) return "";
    const c = themeColor("accent");
    const S = "[data-sheet]";
    const lines = (sels: string[]) => `${sels.map((x) => `${S} ${x}`).join(", ")} { stroke: ${c} !important; }`;
    const texts = (sels: string[]) => `${sels.map((x) => `${S} ${x} text`).join(", ")} { fill: ${c} !important; }`;
    switch (el.kind) {
      case "view":
        return lines([`[data-view="${el.view}"] polyline`]);
      case "detail": {
        const g = [`[data-detail="${el.label}"] circle`, `[data-detail-mark="${el.label}"] circle`, `[data-view="detalle-${el.label}"] polyline`];
        return lines(g) + texts([`[data-detail="${el.label}"]`, `[data-detail-mark="${el.label}"]`]);
      }
      case "dim":
        return lines([`[data-user-dim="${el.index}"] *`]) + texts([`[data-user-dim="${el.index}"]`]) + `${S} [data-user-dim="${el.index}"] polygon { fill: ${c} !important; }`;
      case "overall":
        return lines([`[data-overall-dim] *`]) + texts([`[data-overall-dim]`]);
      case "section":
        return lines([`[data-section-line] *`]) + texts([`[data-section-line]`]) + `${S} [data-section-label] { fill: ${c} !important; }`;
      case "sketch":
        return lines([`[data-layer] *`]) + texts([`[data-layer]`]);
      case "sheet":
        return lines([`[data-title-block] rect`]);
      case "cut":
        return lines([`[data-cut-line="${el.label}"] *`, `[data-view="corte-${el.label}"] polyline`]) + texts([`[data-cut-line="${el.label}"]`]) + `${S} [data-cut-line="${el.label}"] polygon { fill: ${c} !important; }`;
      case "break":
        return lines([`[data-break-line="${el.view}"][data-break-index="${el.index}"]`]);
      case "custom":
        return lines([`[data-view="vista-${el.id}"] polyline`]);
    }
  });

  // Lo que la barra izquierda del plano ofrece (Corte A-A, Dibujar, Vista nueva, Hoja, Acomodar)
  ui.setDrawingActions({
    sectionAA: () => {
      upd((x) => (x.section = { y: null }));
      ui.setDrawingSel({ kind: "section" });
    },
    canSectionAA: () => !!d() && !d()!.section,
    draw: () => void startDrawing(),
    newView: () => {
      let id = 0;
      void Promise.resolve(upd2((x) => void (id = addCustomView(x)))).then(() => id && ui.setDrawingSel({ kind: "custom", id }));
    },
    sheet: () => ui.setDrawingSel({ kind: "sheet" }),
    arrange: () => upd((x) => (x.viewAt = {})),
    canArrange: () => !!Object.keys(d()?.viewAt ?? {}).length,
  });
  // Cambiar de herramienta suelta la primera recta elegida y los puntos de un corte a medias
  createEffect(
    on(
      tool,
      () => {
        firstLine = null;
        setCutPts(undefined);
      },
      { defer: true },
    ),
  );

  // Lugar disponible para la hoja (se sigue con cada cambio de tamaño)
  let areaRef: HTMLDivElement | undefined;
  const [area, setArea] = createSignal<[number, number]>([0, 0]);
  onMount(() => {
    if (!areaRef) return;
    const measure = () => setArea([areaRef!.clientWidth - 32, areaRef!.clientHeight - 32]);
    const ro = new ResizeObserver(measure);
    ro.observe(areaRef);
    measure();
    onCleanup(() => ro.disconnect());
  });
  /** Ancho y alto en pantalla para que la hoja se vea completa */
  const sheetSize = createMemo((): [number, number] | undefined => {
    const m = /viewBox="([\d.\s-]+)"/.exec(svg() ?? "");
    const vb = m?.[1].trim().split(/\s+/).map(Number);
    const [aw, ah] = area();
    if (!vb || vb.length !== 4 || !(vb[2] > 0 && vb[3] > 0) || aw <= 0 || ah <= 0) return undefined;
    const k = Math.min(aw / vb[2], ah / vb[3]);
    return [Math.floor(vb[2] * k), Math.floor(vb[3] * k)];
  });

  return (
    <div class="absolute inset-0 z-[6] flex flex-col bg-bg-darker" aria-label={t("Plano 2D")}>
      <div class="flex flex-wrap items-center gap-2 border-b border-border bg-bg-lighter px-2 py-1.5">
        <span class="text-xs font-semibold text-text" data-drawing-name>
          {d()?.name ?? t("Plano 2D")}
        </span>
        <button
          class="px-1.5 py-0.5 rounded text-[11px] text-text hover:bg-surface border border-border"
          data-sheet-scale
          title={d()?.scale ? t("Escala fija de la hoja (se cambia en Hoja)") : t("Automática: la mayor normalizada con la que todo entra; con más vistas baja. Para que no cambie, fijarla en Hoja")}
          onClick={() => ui.setDrawingSel({ kind: "sheet" })}
        >
          {d()?.scale ? t("Escala {s}", { s: result() ? scaleLabel(result()!.scale) : "…" }) : t("Escala {s} (auto)", { s: result() ? scaleLabel(result()!.scale) : "…" })}
        </button>
        <div class="flex-1" />
        <Show when={busy()}>
          <span class="text-[11px] text-text-dim">{t("Proyectando…")}</span>
        </Show>
        <Button size="sm" variant="primary" disabled={!svg()} onClick={() => void exportPdf()}>
          {t("Exportar PDF")}
        </Button>
        <Button size="sm" disabled={!svg()} onClick={() => void exportSvg()}>
          {t("Exportar SVG")}
        </Button>
        <div class="w-36">
          <Select
            options={[
              { value: "sheet", label: t("DXF: hoja entera") },
              ...VIEW_NAMES.filter((v) => v !== "iso").map((v) => ({ value: v, label: t("DXF: {v} 1:1", { v: VIEW_LABELS[v].toLowerCase() }) })),
            ]}
            value={dxfWhat()}
            onChange={(v) => setDxfWhat(v as "sheet" | ViewName)}
          />
        </div>
        <Button size="sm" disabled={!svg()} onClick={() => void exportDxf()}>
          {t("Exportar DXF")}
        </Button>
        <Button size="sm" variant="ghost" onClick={() => props.ui.setDrawingOpen(false)}>
          Cerrar
        </Button>
      </div>
      <div ref={areaRef} class="relative flex-1 min-h-0 overflow-auto p-4 flex items-center justify-center">
        {/* Mensajes y avisos flotando arriba: no empujan la hoja (si no, cambia de tamaño en pantalla) */}
        <div class="pointer-events-none absolute left-2 top-1 z-10 space-y-0.5">
          <Show when={message()}>
            <p class="w-fit rounded bg-bg-lighter/90 px-2 py-0.5 text-xs text-text-muted shadow">{message()}</p>
          </Show>
          <Show when={problems().length}>
            <p class="w-fit rounded bg-bg-lighter/90 px-2 py-0.5 text-xs text-warning shadow" data-placement-problems>
              {problems().join(" · ")}
            </p>
          </Show>
        </div>
        <style>{highlight()}</style>
        {/* La hoja entra entera en lo que hay (ancho o alto, lo que limite), con su proporción */}
        <div
          data-sheet
          class={clsx("shrink-0 shadow-lg [&>svg]:block [&>svg]:h-full [&>svg]:w-full", tool() && "cursor-crosshair")}
          style={sheetSize() ? { width: `${sheetSize()![0]}px`, height: `${sheetSize()![1]}px` } : { width: "100%" }}
          innerHTML={svg()}
          onClick={onSheetClick}
          onDblClick={() => cutPts() && finishCut()}
          onPointerDown={onSheetDown}
          onPointerMove={onSheetMove}
          onPointerUp={onSheetUp}
          onPointerCancel={() => {
            drag = undefined;
            orbit = undefined;
            setDragAt(undefined);
            setOrbitDir(undefined);
          }}
        />
        {/* Corte o rotura a medias: sus puntos sobre la vista */}
        <Show when={cutPreview() && sheetSize()}>
          <svg
            data-cut-preview
            class="pointer-events-none absolute"
            style={{ width: `${sheetSize()![0]}px`, height: `${sheetSize()![1]}px` }}
            viewBox={`0 0 ${sheet().width} ${sheet().height}`}
          >
            <polyline points={cutPreview()!.points} fill="none" stroke={themeColor("accent")} stroke-width="0.4" stroke-dasharray={tool() === "section" ? "6 1.5 1.5 1.5" : undefined} />
            <For each={cutPreview()!.dots}>{(p) => <circle cx={p[0]} cy={p[1]} r="0.9" fill={themeColor("accent")} />}</For>
          </svg>
        </Show>
        <Show when={orbitPreview() && sheetSize()}>
          <svg
            data-orbit-preview
            class="pointer-events-none absolute"
            style={{ width: `${sheetSize()![0]}px`, height: `${sheetSize()![1]}px` }}
            viewBox={`0 0 ${sheet().width} ${sheet().height}`}
          >
            <For each={orbitPreview().split("|")}>{(pts) => <polyline points={pts} fill="none" stroke={themeColor("accent")} stroke-width="0.35" />}</For>
          </svg>
        </Show>
        <Show when={d() && sel()}>
          <ElementWindow
            sheet={d()!}
            element={sel()!}
            scale={result()?.scale ?? 1}
            placed={(() => {
              const name = viewNameOf(sel());
              return name ? result()?.placed.find((p) => p.name === name && !p.detail) : undefined;
            })()}
            cameraDir={() => {
              const pose = ui.viewer()?.cameraPose();
              return pose ? viewDirFromCamera(pose) : undefined;
            }}
            hasImage={(name) => !!images()[name]}
            savePng={(name) => void savePng(name)}
            height={sheet().height}
            dimValue={(i) => {
              const r = result();
              const x = r && userDimensions(r.placed, d()!.dims).find((y) => y.index === i);
              if (!x) return undefined;
              if (x.kind === "angle") return `${dimText(x.value)}°`;
              if (x.kind === "radius") return `R${dimText(x.value)}`;
              return `${x.prefix ?? ""}${dimText(x.value)}`;
            }}
            gearList={(inAssembly() ? [] : (store.result()?.gears ?? [])).map((g) => ({ id: g.feature, name: store.doc()?.features.find((f) => f.id === g.feature)?.name ?? `#${g.feature}` }))}
            assembly={inAssembly()}
            update={upd}
            close={() => ui.setDrawingSel(undefined)}
            editDrawing={() => void startDrawing()}
          />
        </Show>
      </div>
    </div>
  );
};

/**
 * Barra izquierda con el plano abierto: sus herramientas (las de Diseñar no
 * sirven acá). Las cotas y el detalle quedan activos hasta Esc o hasta
 * apretarlos otra vez.
 */
export const DrawingSideTools: Component<{ ui: CadUi }> = (props) => {
  const ui = props.ui;
  const act = () => ui.drawingActions();
  const tool = (id: Exclude<Tool, "">, label: string, icon: (p: { size?: number }) => JSX.Element, tip: string) => (
    <SideButton label={label} icon={icon} tip={tip} active={ui.drawingTool() === id} onClick={() => ui.setDrawingTool(ui.drawingTool() === id ? "" : id)} />
  );
  return (
    <>
      <SideButton label="Elegir" icon={SketchIcons.Select} tip="Clic elige una vista, una cota o un detalle (y abre sus propiedades); arrastrar mueve la vista" active={ui.drawingTool() === ""} onClick={() => ui.setDrawingTool("")} />
      <SideSep />
      {tool("dim", "Cota", D.DimLength, "Clic en una recta (largo), un círculo (diámetro) o un arco (radio)")}
      {tool("distance", "Distancia", D.DimDistance, "Clic en dos rectas paralelas")}
      {tool("angle", "Ángulo", D.DimAngle, "Clic en dos rectas que se cruzan")}
      <SideSep />
      {tool("detail", "Detalle", D.DrawingDetail, "Clic en una vista: amplía esa zona aparte")}
      {tool("section", "Corte", D.DrawingSection, "Clic en dos puntos sobre una vista (más clics: escalonado); doble clic o Enter termina. La vista del corte sale aparte")}
      {tool("break", "Rotura", D.DrawingDetail, "Clic en los puntos de un contorno sobre una vista; doble clic o Enter lo cierra: corte local hasta una profundidad")}
      <SideButton label="Corte A-A" icon={D.DrawingSection} tip="El frente cortado por un plano paralelo a él, rayado, con la línea de corte en la planta" disabled={!act()?.canSectionAA()} onClick={() => act()?.sectionAA()} />
      <SideButton label="Vista nueva" icon={D.DrawingNewView} tip="Una vista más que se gira a gusto (Ctrl+arrastrar en la hoja, o con los botones de su ventana)" onClick={() => act()?.newView()} />
      <SideSep />
      <SideButton label="Dibujar" icon={SketchIcons.Line} tip="Líneas, arcos, curvas y cotas propias sobre la hoja, con las herramientas del sketch" onClick={() => act()?.draw()} />
      <SideSep />
      <SideButton label="Hoja" icon={D.DrawingSheet} tip="Tamaño, escala, proyección, líneas y cajetín" onClick={() => act()?.sheet()} />
      <SideButton label="Acomodar" icon={D.DrawingArrange} tip="Las vistas movidas a mano vuelven a la disposición automática" disabled={!act()?.canArrange()} onClick={() => act()?.arrange()} />
    </>
  );
};

/**
 * Ventana flotante con las propiedades de lo elegido en el plano (se arrastra
 * por el título). Los cambios son pasos del historial del diseño.
 */
const ElementWindow: Component<{
  sheet: DrawingSheet;
  element: DrawingElement;
  scale: number;
  /** La vista elegida como está en la hoja (para su posición) */
  placed?: Placed;
  height: number;
  /** Desde dónde mira la cámara del visor de Diseñar */
  cameraDir: () => ViewDir | undefined;
  hasImage: (name: string) => boolean;
  savePng: (name: string) => void;
  dimValue: (index: number) => string | undefined;
  /** Engranes del diseño, para ponerles círculo primitivo y tabla */
  gearList: { id: number; name: string }[];
  /** Plano de un ensamble: lista de materiales y globos */
  assembly: boolean;
  update: (change: (s: DrawingSheet) => void) => void;
  close: () => void;
  editDrawing: () => void;
}> = (props) => {
  // Distancia al borde derecho y al de arriba del área de la hoja
  const [pos, setPos] = createSignal<[number, number]>([16, 16]);
  const startDrag = (e: PointerEvent) => {
    if ((e.target as HTMLElement).closest("button")) return;
    const [x0, y0] = pos();
    const [cx, cy] = [e.clientX, e.clientY];
    const move = (m: PointerEvent) => setPos([Math.max(0, x0 - (m.clientX - cx)), Math.max(0, y0 + (m.clientY - cy))]);
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };
  const el = () => props.element;
  /** Engranes que el plano pide y el diseño ya no tiene (se borraron): su tabla no se dibuja */
  const missingGears = () => (props.sheet.gears ?? []).filter((id) => !props.gearList.some((g) => g.id === id));
  const detail = (): Detail | undefined => {
    const x = el();
    return x.kind === "detail" ? props.sheet.details.find((y) => y.label === x.label) : undefined;
  };
  const editDetail = (change: (x: Detail) => void) => {
    const label = detail()?.label;
    props.update((s) => {
      s.details = s.details.map((x) => {
        if (x.label !== label) return x;
        const y = { ...x };
        change(y);
        return y;
      });
    });
  };
  const row = (label: string, control: JSX.Element) => (
    <label class="flex items-center justify-between gap-2 text-[11px] text-text-muted">
      <span>{label}</span>
      {control}
    </label>
  );
  const input = "w-28 px-1.5 py-0.5 rounded bg-surface/40 border border-border text-xs text-text outline-none focus:border-accent";
  const check = (label: string, value: boolean, set: (v: boolean) => void) => (
    <label class="flex items-center gap-1.5 text-[11px] text-text-muted cursor-pointer">
      <input type="checkbox" checked={value} onChange={(e) => set(e.currentTarget.checked)} />
      {label}
    </label>
  );
  const num = (v: string) => Number(v.trim().replace(",", "."));
  /** Rayado: ángulo, separación y cruzado */
  const hatchControls = (h: HatchStyle, set: (h: HatchStyle) => void) => (
    <div class="flex flex-wrap items-center gap-x-2 gap-y-1 text-[11px] text-text-muted" aria-label={t("Rayado")}>
      <span>{t("Rayado")}</span>
      <input
        aria-label={t("Ángulo del rayado")}
        type="number"
        step="15"
        class="w-12 px-1 py-0.5 rounded bg-surface/40 border border-border text-xs text-text outline-none focus:border-accent"
        value={h.angle}
        onChange={(e) => isFinite(num(e.currentTarget.value)) && set({ ...h, angle: num(e.currentTarget.value) })}
      />
      <span>{t("° cada")}</span>
      <input
        aria-label={t("Separación del rayado")}
        type="number"
        min="0.5"
        step="0.5"
        class="w-12 px-1 py-0.5 rounded bg-surface/40 border border-border text-xs text-text outline-none focus:border-accent"
        value={h.spacing}
        onChange={(e) => num(e.currentTarget.value) > 0 && set({ ...h, spacing: num(e.currentTarget.value) })}
      />
      <span>mm</span>
      {check(t("Cruzado"), !!h.cross, (on) => set({ ...h, cross: on }))}
    </div>
  );
  const scaleOptions = (auto: number) => [{ value: "auto", label: t("Auto ({s})", { s: scaleLabel(auto) }) }, ...SCALES.map((s) => ({ value: String(s), label: scaleLabel(s) }))];

  return (
    <div
      class="absolute z-10 w-64 rounded-lg border border-border bg-bg-lighter shadow-xl"
      style={{ right: `${pos()[0]}px`, top: `${pos()[1]}px` }}
      aria-label={t("Propiedades del plano")}
      data-drawing-props={el().kind}
      onClick={(e) => e.stopPropagation()}
    >
      <div class="flex items-center gap-2 border-b border-border px-2 py-1.5 cursor-move select-none" onPointerDown={startDrag}>
        <span class="flex-1 truncate text-xs font-semibold text-text">{elementLabel(props.sheet, el())}</span>
        <button class="text-text-muted hover:text-text text-sm leading-none" aria-label={t("Cerrar propiedades")} onClick={() => props.close()}>
          ×
        </button>
      </div>
      <div class="space-y-2 p-2">
        <Show when={el().kind !== "sheet"}>{check(t("Visible en la hoja"), elementVisible(props.sheet, el()), () => props.update((s) => toggleElement(s, el())))}</Show>
        <Switch>
          <Match when={el().kind === "sheet"}>
            {row(
              t("Nombre"),
              <input
                aria-label={t("Nombre del plano")}
                class={input}
                value={props.sheet.name}
                onChange={(e) => {
                  const v = e.currentTarget.value.trim();
                  if (v) props.update((s) => (s.name = v));
                }}
              />,
            )}
            {row(
              t("Hoja"),
              <div class="w-28">
                <Select options={SHEETS.map((s) => ({ value: s.name, label: s.name }))} value={props.sheet.sheet} onChange={(v) => props.update((s) => (s.sheet = v))} />
              </div>,
            )}
            {row(
              t("Proyección"),
              <div class="w-28">
                <Select
                  options={[
                    { value: "first", label: t("1.er diedro (ISO)") },
                    { value: "third", label: t("3.er diedro (ANSI)") },
                  ]}
                  value={props.sheet.projection}
                  onChange={(v) => props.update((s) => (s.projection = v as Projection))}
                />
              </div>,
            )}
            {row(
              t("Escala"),
              <div class="w-28">
                <Select options={scaleOptions(props.scale)} value={props.sheet.scale ? String(props.sheet.scale) : "auto"} onChange={(v) => props.update((s) => (s.scale = v === "auto" ? null : Number(v)))} />
              </div>,
            )}
            <div class="flex flex-wrap gap-x-3">
              {check(t("Ocultas"), props.sheet.hiddenLines, (v) => props.update((s) => (s.hiddenLines = v)))}
              {check(t("Tangentes"), props.sheet.smooth, (v) => props.update((s) => (s.smooth = v)))}
            </div>
            <Show when={props.gearList.length > 0 || missingGears().length > 0}>
              <div class="space-y-0.5" aria-label={t("Engranes en el plano")}>
                <p class="text-[11px] text-text-dim">{t("Engranes con círculo primitivo y tabla de datos")}</p>
                <For each={props.gearList}>
                  {(g) =>
                    check(g.name, !!props.sheet.gears?.includes(g.id), (on) =>
                      props.update((s) => {
                        const ids = new Set(s.gears ?? []);
                        if (on) ids.add(g.id);
                        else ids.delete(g.id);
                        s.gears = [...ids];
                      }),
                    )
                  }
                </For>
                <Show when={missingGears().length > 0}>
                  <p class="text-[11px] text-warning" data-missing-gears>
                    {t("{n} engrane(s) del plano ya no existen en el diseño: su tabla no se dibuja", { n: missingGears().length })}{" "}
                    <button class="underline" onClick={() => props.update((s) => (s.gears = (s.gears ?? []).filter((id) => props.gearList.some((g) => g.id === id))))}>
                      {t("Quitarlos")}
                    </button>
                  </p>
                </Show>
              </div>
            </Show>
            <Show when={props.assembly}>
              <div class="space-y-1" aria-label={t("Lista de materiales")}>
                {check(t("Lista de materiales"), props.sheet.bom !== false, (on) => props.update((s) => (s.bom = on)))}
                {row(
                  t("Globos"),
                  <div class="w-28">
                    <Select
                      options={[
                        { value: "auto", label: t("Automático") },
                        { value: "none", label: t("Sin globos") },
                        ...VIEW_NAMES.filter((v) => !props.sheet.hiddenViews.includes(v)).map((v) => ({ value: v, label: VIEW_LABELS[v] })),
                        ...(props.sheet.customViews ?? []).filter((c) => !c.hidden).map((c) => ({ value: customViewName(c.id), label: c.name })),
                      ]}
                      value={props.sheet.balloons === null ? "none" : (props.sheet.balloons ?? "auto")}
                      onChange={(v) => props.update((s) => (s.balloons = v === "auto" ? undefined : v === "none" ? null : v))}
                    />
                  </div>,
                )}
              </div>
            </Show>
            {check(t("Rayado según el material"), props.sheet.hatchByMaterial !== false, (on) => props.update((s) => (s.hatchByMaterial = on)))}
            <Show when={props.sheet.hatchByMaterial === false}>{hatchControls(props.sheet.hatch ?? DEFAULT_HATCH, (h) => props.update((s) => (s.hatch = h)))}</Show>
            {row(t("Título"), <input aria-label={t("Título del plano")} class={input} value={props.sheet.title} onChange={(e) => props.update((s) => (s.title = e.currentTarget.value))} />)}
            {row(t("Autor"), <input aria-label={t("Autor")} placeholder={t("Autor")} class={input} value={props.sheet.author} onChange={(e) => props.update((s) => (s.author = e.currentTarget.value))} />)}
          </Match>
          <Match
            when={(() => {
              const x = el();
              return x.kind === "view" ? x.view : x.kind === "custom" ? customViewName(x.id) : x.kind === "cut" ? sectionViewName(x.label) : undefined;
            })()}
          >
            {(view) => (
              <>
                <Show when={el().kind === "cut" && (el() as { label: string }).label}>
                  {(label) => {
                    const def = () => props.sheet.sections?.find((c) => c.label === label());
                    const edit = (change: (c: SectionDef) => void) =>
                      props.update((s) => {
                        s.sections = (s.sections ?? []).map((c) => {
                          if (c.label !== label()) return c;
                          const x = { ...c };
                          change(x);
                          return x;
                        });
                      });
                    return (
                      <>
                        <p class="text-[11px] text-text-dim">
                          {t("{k}, sobre {v}. Se mira en el sentido de las flechas.", { k: (def()?.points.length ?? 0) > 2 ? t("Escalonado") : t("Recto"), v: (VIEW_LABELS as Record<string, string>)[def()?.view ?? ""] ?? def()?.view ?? "" })}
                        </p>
                        <Button size="sm" variant="ghost" fullWidth onClick={() => edit((c) => (c.flip = !c.flip))}>
                          {t("Invertir el sentido")}
                        </Button>
                        {hatchControls(def()?.hatch ?? props.sheet.hatch ?? DEFAULT_HATCH, (h) => edit((c) => (c.hatch = h)))}
                      </>
                    );
                  }}
                </Show>
                <Show when={el().kind === "view" && !isPictorial(view())}>
                  <div class="flex items-center justify-between text-[11px] text-text-muted" aria-label={t("Medio corte")}>
                    <span>{t("Medio corte")}</span>
                    <span class="flex gap-0.5">
                      {(["", "right", "left"] as const).map((side) => (
                        <button
                          class={clsx("px-1.5 py-0.5 rounded border border-border", (props.sheet.halves?.[view()] ?? "") === side ? "bg-accent text-bg" : "hover:bg-surface")}
                          onClick={() => props.update((s) => (s.halves = { ...(s.halves ?? {}), [view()]: side || undefined }))}
                        >
                          {side === "" ? t("No") : side === "right" ? t("Derecha") : t("Izquierda")}
                        </button>
                      ))}
                    </span>
                  </div>
                </Show>
                <Show when={isPictorial(view())}>
                  <div class="space-y-1 text-[11px] text-text-muted" aria-label={t("Cortar la vista 3D")}>
                    <span>{t("Cortar")}</span>
                    <div class="flex flex-wrap gap-0.5">
                      {[["", t("Entera")], ["quarter", t("Un cuarto")], ...(props.sheet.sections ?? []).map((c) => [c.label, t("Corte {l}-{l}", { l: c.label })])].map(([v, label]) => (
                        <button
                          class={clsx("px-1.5 py-0.5 rounded border border-border", (props.sheet.cutBy?.[view()] ?? "") === v ? "bg-accent text-bg" : "hover:bg-surface")}
                          onClick={() => props.update((s) => (s.cutBy = { ...(s.cutBy ?? {}), [view()]: v || undefined }))}
                        >
                          {label}
                        </button>
                      ))}
                    </div>
                  </div>
                </Show>
                <div class="flex flex-wrap items-center gap-x-3 gap-y-1">
                  {check(t("Sombreada"), !!props.sheet.shaded?.[view()], (on) => props.update((s) => (s.shaded = { ...(s.shaded ?? {}), [view()]: on ? { lines: true } : undefined })))}
                  <Show when={props.sheet.shaded?.[view()]}>
                    {check(t("Con líneas"), props.sheet.shaded?.[view()]?.lines !== false, (on) => props.update((s) => (s.shaded = { ...(s.shaded ?? {}), [view()]: { lines: on } })))}
                  </Show>
                </div>
                <Show when={props.hasImage(view())}>
                  <Button size="sm" variant="ghost" fullWidth onClick={() => props.savePng(view())}>
                    {t("Guardar imagen PNG")}
                  </Button>
                </Show>
                <Show when={el().kind === "custom" && (el() as { id: number }).id}>
                  {(id) =>
                    row(
                      t("Nombre"),
                      <input
                        aria-label={t("Nombre de la vista")}
                        class={input}
                        value={props.sheet.customViews?.find((c) => c.id === id())?.name ?? ""}
                        onChange={(e) => {
                          const v = e.currentTarget.value.trim();
                          if (v) props.update((s) => (s.customViews = (s.customViews ?? []).map((c) => (c.id === id() ? { ...c, name: v } : c))));
                        }}
                      />,
                    )
                  }
                </Show>
                <Show when={view() === "iso" || el().kind === "custom"}>
                  {(() => {
                    const dir = () => viewDirOf(props.sheet, view())!;
                    const setDir = (d: ViewDir) => props.update((s) => (s.viewDirs = { ...(s.viewDirs ?? {}), [view()]: d }));
                    const turn = (label: string, tip: string, h: number, v: number, r: number) => (
                      <button class="px-1.5 py-0.5 rounded text-[11px] text-text-muted hover:text-text hover:bg-surface border border-border" title={tip} onClick={() => setDir(rotateViewDir(dir(), h, v, r))}>
                        {label}
                      </button>
                    );
                    return (
                      <div class="space-y-1" aria-label={t("Girar la vista")}>
                        <div class="flex items-center justify-between text-[11px] text-text-muted">
                          <span>{t("Girar")}</span>
                          <span class="flex gap-0.5">
                            {turn("←", t("La pieza gira 90° a la izquierda"), -90, 0, 0)}
                            {turn("→", t("La pieza gira 90° a la derecha"), 90, 0, 0)}
                            {turn("↑", t("Se inclina 90° hacia atrás"), 0, -90, 0)}
                            {turn("↓", t("Se inclina 90° hacia quien mira"), 0, 90, 0)}
                            {turn("⟲", t("Gira 90° en el plano de la hoja"), 0, 0, 90)}
                          </span>
                        </div>
                        <div class="flex gap-1">
                          <Button size="sm" variant="ghost" title={t("Como la mira ahora la cámara del visor de Diseñar")} onClick={() => { const c = props.cameraDir(); if (c) setDir(c); }}>
                            {t("Tomar la del visor")}
                          </Button>
                          <Button size="sm" variant="ghost" title={t("Isométrica de siempre")} onClick={() => setDir(viewSpec("iso"))}>
                            {t("Isométrica")}
                          </Button>
                        </div>
                        <p class="text-[11px] text-text-dim">{t("En la hoja: Ctrl+arrastrar la gira.")}</p>
                      </div>
                    );
                  })()}
                </Show>
                {row(
                  t("Escala"),
                  <div class="w-28">
                    <Select
                      options={[{ value: "sheet", label: t("Hoja ({s})", { s: scaleLabel(props.scale) }) }, ...SCALES.map((s) => ({ value: String(s), label: scaleLabel(s) }))]}
                      value={props.sheet.viewScales?.[view()] ? String(props.sheet.viewScales[view()]) : "sheet"}
                      onChange={(v) => props.update((s) => (s.viewScales = { ...(s.viewScales ?? {}), [view()]: v === "sheet" ? null : Number(v) }))}
                    />
                  </div>,
                )}
                <p class="text-[11px] text-text-dim">{t("Con otra escala que la de la hoja, la vista lleva su rótulo debajo.")}</p>
                <Show when={props.placed}>
                  {(v) => {
                    // Centro de la vista en mm de hoja desde la esquina de abajo a la izquierda
                    const center = () => {
                      const b = sheetBox(v());
                      return [(b[0] + b[2]) / 2, props.height - (b[1] + b[3]) / 2] as P2;
                    };
                    const moveTo = (axis: 0 | 1, value: number) => {
                      const c = center();
                      const at: [number, number] = axis === 0 ? [v().x + value - c[0], v().y] : [v().x, v().y - (value - c[1])];
                      props.update((s) => (s.viewAt = { ...(s.viewAt ?? {}), [view()]: at }));
                    };
                    const coord = (axis: 0 | 1) => (
                      <input
                        aria-label={t("Posición {a} de la vista", { a: axis === 0 ? "X" : "Y" })}
                        type="number"
                        step="1"
                        class="w-16 px-1 py-0.5 rounded bg-surface/40 border border-border text-xs text-text outline-none focus:border-accent"
                        value={Math.round(center()[axis] * 10) / 10}
                        onChange={(e) => {
                          const n = num(e.currentTarget.value);
                          if (isFinite(n)) moveTo(axis, n);
                        }}
                      />
                    );
                    return (
                      <>
                        <label class="flex items-center justify-between gap-2 text-[11px] text-text-muted">
                          <span>{t("Centro (mm)")}</span>
                          <span class="flex gap-1">
                            {coord(0)}
                            {coord(1)}
                          </span>
                        </label>
                        <p class="text-[11px] text-text-dim">{t("Se arrastra en la hoja (alineada con el frente; Mayús o Alt, libre).")}</p>
                        <Show when={props.sheet.viewAt?.[view()]}>
                          <Button
                            size="sm"
                            variant="ghost"
                            fullWidth
                            onClick={() =>
                              props.update((s) => {
                                const next = { ...(s.viewAt ?? {}) };
                                delete next[view()];
                                s.viewAt = next;
                              })
                            }
                          >
                            {t("Volver a su lugar")}
                          </Button>
                        </Show>
                      </>
                    );
                  }}
                </Show>
              </>
            )}
          </Match>
          <Match when={el().kind === "overall"}>
            <p class="text-[11px] text-text-dim">{t("Ancho y alto de cada vista.")}</p>
          </Match>
          <Match when={el().kind === "section"}>
            {row(
              t("En Y (mm)"),
              <input
                aria-label={t("Posición del corte")}
                type="number"
                step="1"
                placeholder={t("medio")}
                class={input}
                value={props.sheet.section?.y ?? ""}
                onChange={(e) => {
                  const v = e.currentTarget.value.trim();
                  props.update((s) => (s.section = { ...(s.section ?? {}), y: v === "" || !isFinite(num(v)) ? null : num(v) }));
                }}
              />,
            )}
            <p class="text-[11px] text-text-dim">{t("Paralelo al frente; vacío: por el medio del sólido.")}</p>
          </Match>
          <Match when={detail()}>
            {(det) => (
              <>
                {row(
                  t("Radio (mm)"),
                  <input
                    aria-label={t("Radio del detalle {l}", { l: det().label })}
                    type="number"
                    min="0.5"
                    step="0.5"
                    class={input}
                    value={det().radius}
                    onChange={(e) => {
                      const v = num(e.currentTarget.value);
                      if (v > 0) editDetail((x) => (x.radius = v));
                    }}
                  />,
                )}
                {row(
                  t("Escala"),
                  <div class="w-28">
                    <Select
                      options={scaleOptions(detailScale(props.scale))}
                      value={det().scale ? String(det().scale) : "auto"}
                      onChange={(v) => editDetail((x) => (x.scale = v === "auto" ? undefined : Number(v)))}
                    />
                  </div>,
                )}
              </>
            )}
          </Match>
          <Match when={el().kind === "dim"}>
            <p class="text-[11px] text-text-muted">
              {t("Mide")}{" "}
              <span class="font-mono text-text" data-dim-value>
                {props.dimValue((el() as { index: number }).index) ?? "—"}
              </span>
            </p>
            <p class="text-[11px] text-text-dim">{t("Sigue al modelo: si cambia el diseño, se vuelve a medir.")}</p>
          </Match>
          <Match when={el().kind === "break" && (el() as { view: string; index: number })}>
            {(br) => (
              <>
                {row(
                  t("Hondo (mm)"),
                  <input
                    aria-label={t("Profundidad de la rotura")}
                    type="number"
                    min="0.1"
                    step="0.5"
                    class={input}
                    value={props.sheet.breaks?.[br().view]?.[br().index]?.depth ?? ""}
                    onChange={(e) => {
                      const v = num(e.currentTarget.value);
                      if (v > 0)
                        props.update((s) => {
                          const list = [...(s.breaks?.[br().view] ?? [])];
                          if (list[br().index]) list[br().index] = { ...list[br().index], depth: v };
                          s.breaks = { ...(s.breaks ?? {}), [br().view]: list };
                        });
                    }}
                  />,
                )}
                <p class="text-[11px] text-text-dim">{t("Cuánto entra el corte local desde lo más cercano a quien mira.")}</p>
              </>
            )}
          </Match>
          <Match when={el().kind === "sketch"}>
            <Button size="sm" fullWidth onClick={() => props.editDrawing()}>
              {t("Editar el dibujo")}
            </Button>
          </Match>
        </Switch>
        <Show when={elementDeletable(el())}>
          <Button
            size="sm"
            variant="ghost"
            fullWidth
            onClick={() => {
              const x = el();
              props.update((s) => removeElement(s, x));
              props.close();
            }}
          >
            {t("Quitar")}
          </Button>
        </Show>
      </div>
    </div>
  );
};

/** Hoja de fondo de la última vez que se dibujó (se libera al preparar otra) */
let lastBackdrop: string | undefined;

/**
 * La hoja como imagen para dibujar encima: el SVG tal cual, papel blanco
 * incluido (en los dos temas, como se ve en el plano). Devuelve una URL de
 * objeto (PNG), a lo sumo 4096 px de lado.
 */
async function rasterSheet(svg: string, width: number, height: number): Promise<string> {
  const k = Math.min(12, 4096 / Math.max(width, height));
  const [w, h] = [Math.round(width * k), Math.round(height * k)];
  const clean = svg.replace(/width="[\d.]+mm" height="[\d.]+mm"/, `width="${w}" height="${h}"`);
  const url = URL.createObjectURL(new Blob([clean], { type: "image/svg+xml" }));
  try {
    const img = new Image();
    img.src = url;
    await img.decode();
    const canvas = document.createElement("canvas");
    canvas.width = w;
    canvas.height = h;
    const ctx = canvas.getContext("2d")!;
    ctx.drawImage(img, 0, 0, w, h);
    const blob = await new Promise<Blob | null>((ok) => canvas.toBlob(ok, "image/png"));
    if (!blob) throw new Error(t("No se pudo preparar la hoja para dibujar"));
    return URL.createObjectURL(blob);
  } finally {
    URL.revokeObjectURL(url);
  }
}
