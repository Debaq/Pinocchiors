import { Component, Match, Show, Switch, createEffect, createMemo, createSignal, on, onCleanup, onMount, untrack, type JSX } from "solid-js";
import { clsx } from "clsx";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import type { CadStore } from "../../lib/cad";
import type { CadUi } from "../../lib/cadUi";
import {
  SCALES,
  SHEETS,
  asArc,
  asCircle,
  asSegment,
  bounds,
  detailScale,
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
  type Detail,
  type DrawingLine,
  type P2,
  type Projection,
  type ViewName,
} from "../../lib/drawing";
import {
  VIEW_NAMES,
  addDrawing,
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
import { themeColor } from "../../lib/theme";
import { Button, Select } from "../ui";

type Tool = "" | "dim" | "distance" | "angle" | "detail";

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
  const upd = (change: (s: DrawingSheet) => void) => {
    const id = ui.activeDrawing();
    if (id !== undefined) void store.commit((doc) => void updateDrawing(doc, id, change));
  };
  const sel = () => {
    const s = d();
    const el = ui.drawingSel();
    return s && el && hasElement(s, el) ? el : undefined;
  };

  const [tool, setTool] = createSignal<Tool>("");
  let firstLine: { view: string; ref: P2 } | null = null;
  const [views, setViews] = createSignal<Partial<Record<ViewName, DrawingLine[]>>>();
  const [hatch, setHatch] = createSignal<Partial<Record<ViewName, [P2, P2, P2][]>>>({});
  const [sectionAt, setSectionAt] = createSignal(0);
  const [busy, setBusy] = createSignal(false);
  const [message, setMessage] = createSignal<string>();
  let seq = 0;

  const projection = (): Projection => d()?.projection ?? "first";
  // Como texto: el efecto corre solo si cambia lo que se pide
  const shownViews = createMemo(() => VIEW_NAMES.filter((v) => d() && !d()!.hiddenViews.includes(v)).join(","));
  const sectionOn = createMemo(() => !!d()?.section && !d()!.section!.hidden);
  const sectionY = createMemo(() => d()?.section?.y ?? undefined);
  createEffect(
    on([() => store.result()?.version, projection, shownViews, sectionOn, sectionY], async () => {
      const names = (shownViews() ? shownViews().split(",") : []) as ViewName[];
      const n = ++seq;
      if (!names.length) return setViews({});
      setBusy(true);
      try {
        // El corte va por el medio del sólido, paralelo al frente: se queda la mitad de atrás
        const body = store.result()?.body;
        const at = sectionY() ?? (body ? (body.bbox_min[1] + body.bbox_max[1]) / 2 : 0);
        const specs = names.map((v) => {
          const spec = viewSpec(v, projection());
          return v === "front" && sectionOn() ? { ...spec, section: { origin: [0, at, 0], normal: [0, 1, 0] } } : spec;
        });
        const res = await invoke<{ lines: DrawingLine[]; hatch: [P2, P2, P2][] }[]>("cad_drawing", { views: specs });
        if (n === seq) {
          setViews(Object.fromEntries(names.map((v, i) => [v, res[i].lines])));
          setHatch(Object.fromEntries(names.map((v, i) => [v, res[i].hatch])));
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
    const r = layout(v, sheet(), projection(), s.scale ?? undefined, hatch());
    return { ...r, placed: [...r.placed, ...placeDetails(r.placed, s.details.filter((x) => !x.hidden), sheet())] };
  });
  // Lo dibujado a mano sobre la hoja (un sketch en mm de hoja)
  const layer = createMemo(() => (d()?.sketchHidden ? [] : sheetLayerLines(d()?.sketch)));
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
      },
    );
  };
  const svg = createMemo(() => sheetOf(true));
  const fileBase = () => (d()?.title ?? "").replace(/[^\p{L}\p{N}_-]+/gu, "_") || "plano";

  /**
   * Dibujar sobre la hoja: se abre el editor del sketch con la hoja de fondo
   * (sin el papel blanco, en el color del texto del tema). Al terminar se
   * vuelve acá.
   */
  const startDrawing = async () => {
    const s = d();
    if (!s) return;
    const sh = sheet();
    try {
      const backdrop = await rasterSheet(sheetOf(false), sh.width, sh.height);
      // La de la vez anterior ya no se usa
      if (lastBackdrop) URL.revokeObjectURL(lastBackdrop);
      lastBackdrop = backdrop;
      ui.setDrawingOpen(false);
      ui.editSheet(s.sketch, { drawing: s.id, width: sh.width, height: sh.height, backdrop });
    } catch (e) {
      setMessage(String(e));
    }
  };

  const write = async (ext: string, name: string, run: (path: string) => Promise<number>) => {
    const path = await save({ filters: [{ name: ext.toUpperCase(), extensions: [ext] }], defaultPath: name });
    if (!path) return;
    try {
      const bytes = await run(path);
      setMessage(`Exportado (${Math.round(bytes / 1024)} KB)`);
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
        content: sheetDxf(r.placed, sheet(), { hidden: s.hiddenLines, smooth: s.smooth, dimensions: s.overall, userDims: s.dims, layer: layer(), view: what === "sheet" ? undefined : what }),
      }),
    );
  };

  /** Punto de la hoja (mm, Y hacia abajo) bajo el puntero */
  const sheetPoint = (e: MouseEvent): DOMPoint | undefined => {
    const svgEl = (e.currentTarget as HTMLElement).querySelector("svg");
    const m = svgEl?.getScreenCTM();
    return m ? new DOMPoint(e.clientX, e.clientY).matrixTransform(m.inverse()) : undefined;
  };

  /** Sin herramienta: lo que está bajo el clic (el elemento del SVG o la vista que lo contiene) */
  const elementAt = (e: MouseEvent): DrawingElement | undefined => {
    const t = e.target as Element;
    const dim = t.closest("[data-user-dim]") as HTMLElement | null;
    if (dim?.dataset.userDim) return { kind: "dim", index: Number(dim.dataset.userDim) };
    const det = t.closest("[data-detail], [data-detail-mark]") as HTMLElement | null;
    if (det) return { kind: "detail", label: det.dataset.detail ?? det.dataset.detailMark! };
    if (t.closest("[data-hatch], [data-section-line], [data-section-label]")) return { kind: "section" };
    if (t.closest("[data-dimension]")) return { kind: "overall" };
    if (t.closest("[data-layer]")) return { kind: "sketch" };
    if (t.closest("[data-title-block]")) return { kind: "sheet" };
    const view = (t.closest("[data-view]") as HTMLElement | null)?.dataset.view;
    if (view?.startsWith("detalle-")) return { kind: "detail", label: view.slice(8) };
    if (view) return { kind: "view", view: view as ViewName };
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
      if (p[0] >= b.min[0] - pad && p[0] <= b.max[0] + pad && p[1] >= b.min[1] - pad && p[1] <= b.max[1] + pad) return { kind: "view", view: v.name as ViewName };
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
    const t = tool();
    const s = d();
    if (!r || !s) return;
    if (!t) return ui.setDrawingSel(elementAt(e));
    const pt = sheetPoint(e);
    if (!pt) return;
    // Primero los detalles (están encima, ampliados)
    const order = [...r.placed].sort((a, b) => Number(!!b.detail) - Number(!!a.detail));
    for (const v of order) {
      if (v.name === "iso") continue;
      const p: P2 = [(pt.x - v.x) / v.scale, (v.y - pt.y) / v.scale];
      if (v.detail && Math.hypot(p[0] - v.detail.center[0], p[1] - v.detail.center[1]) > v.detail.radius) continue;
      if (t === "detail") {
        if (v.detail) return setMessage("El detalle se marca sobre una vista");
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
      if (t === "dim") {
        const kind = asCircle(line) ? "diameter" : asSegment(line) ? "length" : asArc(line) ? "radius" : undefined;
        if (!kind) return setMessage("Esa línea no es recta, ni un círculo, ni un arco");
        upd((x) => (x.dims = [...x.dims, { view: v.name, kind, refs: [p] }]));
        return;
      }
      if (!asSegment(line)) return setMessage(t === "angle" ? "Elegir rectas para el ángulo" : "Elegir rectas para la distancia");
      if (!firstLine || firstLine.view !== v.name) {
        firstLine = { view: v.name, ref: p };
        return setMessage(t === "angle" ? "Ahora la otra recta del ángulo" : "Ahora la recta paralela");
      }
      const first = firstLine;
      upd((x) => (x.dims = [...x.dims, { view: v.name, kind: t === "angle" ? "angle" : "distance", refs: [first.ref, p] }]));
      firstLine = null;
      setMessage(undefined);
      return;
    }
  };

  // Esc suelta la herramienta o lo elegido; Supr borra lo elegido (si se puede)
  const onKey = (e: KeyboardEvent) => {
    if ((e.target as HTMLElement | null)?.closest?.("input, textarea, select, [contenteditable]")) return;
    if (e.key === "Escape") {
      if (tool()) setTool("");
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
        return lines([`[data-dimension]:not([data-user-dim]) *`]) + texts([`[data-dimension]:not([data-user-dim])`]);
      case "section":
        return lines([`[data-section-line] *`]) + texts([`[data-section-line]`]) + `${S} [data-section-label] { fill: ${c} !important; }`;
      case "sketch":
        return lines([`[data-layer] polyline`]);
      case "sheet":
        return lines([`[data-title-block] rect`]);
    }
  });

  const toolButton = (id: Exclude<Tool, "">, label: string, tip: string) => (
    <button
      role="radio"
      aria-checked={tool() === id}
      class={clsx("px-1.5 py-0.5 rounded text-[11px]", tool() === id ? "bg-accent text-bg" : "text-text-muted hover:text-text hover:bg-surface")}
      title={tip}
      onClick={() => {
        firstLine = null;
        setTool(tool() === id ? "" : id);
      }}
    >
      {label}
    </button>
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
    <div class="absolute inset-0 z-[6] flex flex-col bg-bg-darker" aria-label="Plano 2D">
      <div class="flex flex-wrap items-center gap-2 border-b border-border bg-bg-lighter px-2 py-1.5">
        <span class="text-xs font-semibold text-text" data-drawing-name>
          {d()?.name ?? "Plano"}
        </span>
        <div class="flex items-center gap-0.5" role="radiogroup" aria-label="Herramienta de cotas">
          {toolButton("dim", "Cota", "Clic en una recta (largo), un círculo (diámetro) o un arco (radio)")}
          {toolButton("distance", "Distancia", "Clic en dos rectas paralelas")}
          {toolButton("angle", "Ángulo", "Clic en dos rectas que se cruzan")}
          {toolButton("detail", "Detalle", "Clic en una vista: amplía esa zona aparte")}
        </div>
        <Button
          size="sm"
          variant="ghost"
          title="El frente cortado por un plano paralelo a él, rayado, con la línea de corte en la planta"
          disabled={!d() || !!d()!.section}
          onClick={() => {
            upd((x) => (x.section = { y: null }));
            ui.setDrawingSel({ kind: "section" });
          }}
        >
          Corte A-A
        </Button>
        <Button size="sm" variant="ghost" disabled={!svg()} title="Líneas, arcos, curvas y cotas propias sobre la hoja, con las herramientas del sketch" onClick={() => void startDrawing()}>
          Dibujar
        </Button>
        <Button size="sm" variant="ghost" title="Tamaño, escala, proyección, líneas y cajetín" onClick={() => ui.setDrawingSel({ kind: "sheet" })}>
          Hoja
        </Button>
        <div class="flex-1" />
        <Show when={busy()}>
          <span class="text-[11px] text-text-dim">Proyectando…</span>
        </Show>
        <Button size="sm" variant="primary" disabled={!svg()} onClick={() => void exportPdf()}>
          Exportar PDF
        </Button>
        <Button size="sm" disabled={!svg()} onClick={() => void exportSvg()}>
          Exportar SVG
        </Button>
        <div class="w-36">
          <Select
            options={[
              { value: "sheet", label: "DXF: hoja entera" },
              { value: "front", label: "DXF: frente 1:1" },
              { value: "top", label: "DXF: planta 1:1" },
              { value: "side", label: "DXF: lateral 1:1" },
            ]}
            value={dxfWhat()}
            onChange={(v) => setDxfWhat(v as "sheet" | ViewName)}
          />
        </div>
        <Button size="sm" disabled={!svg()} onClick={() => void exportDxf()}>
          Exportar DXF
        </Button>
        <Button size="sm" variant="ghost" onClick={() => props.ui.setDrawingOpen(false)}>
          Cerrar
        </Button>
      </div>
      <Show when={message()}>
        <p class="px-2 py-1 text-xs text-text-muted">{message()}</p>
      </Show>
      <div ref={areaRef} class="relative flex-1 min-h-0 overflow-auto p-4 flex items-center justify-center">
        <style>{highlight()}</style>
        {/* La hoja entra entera en lo que hay (ancho o alto, lo que limite), con su proporción */}
        <div
          data-sheet
          class={clsx("shrink-0 shadow-lg [&>svg]:block [&>svg]:h-full [&>svg]:w-full", tool() && "cursor-crosshair")}
          style={sheetSize() ? { width: `${sheetSize()![0]}px`, height: `${sheetSize()![1]}px` } : { width: "100%" }}
          innerHTML={svg()}
          onClick={onSheetClick}
        />
        <Show when={d() && sel()}>
          <ElementWindow
            sheet={d()!}
            element={sel()!}
            scale={result()?.scale ?? 1}
            dimValue={(i) => {
              const r = result();
              const x = r && userDimensions(r.placed, d()!.dims).find((y) => y.index === i);
              if (!x) return undefined;
              if (x.kind === "angle") return `${dimText(x.value)}°`;
              if (x.kind === "radius") return `R${dimText(x.value)}`;
              return `${x.prefix ?? ""}${dimText(x.value)}`;
            }}
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
 * Ventana flotante con las propiedades de lo elegido en el plano (se arrastra
 * por el título). Los cambios son pasos del historial del diseño.
 */
const ElementWindow: Component<{
  sheet: DrawingSheet;
  element: DrawingElement;
  scale: number;
  dimValue: (index: number) => string | undefined;
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
  const scaleOptions = (auto: number) => [{ value: "auto", label: `Auto (${scaleLabel(auto)})` }, ...SCALES.map((s) => ({ value: String(s), label: scaleLabel(s) }))];

  return (
    <div
      class="absolute z-10 w-64 rounded-lg border border-border bg-bg-lighter shadow-xl"
      style={{ right: `${pos()[0]}px`, top: `${pos()[1]}px` }}
      aria-label="Propiedades del plano"
      data-drawing-props={el().kind}
      onClick={(e) => e.stopPropagation()}
    >
      <div class="flex items-center gap-2 border-b border-border px-2 py-1.5 cursor-move select-none" onPointerDown={startDrag}>
        <span class="flex-1 truncate text-xs font-semibold text-text">{elementLabel(props.sheet, el())}</span>
        <button class="text-text-muted hover:text-text text-sm leading-none" aria-label="Cerrar propiedades" onClick={() => props.close()}>
          ×
        </button>
      </div>
      <div class="space-y-2 p-2">
        <Show when={el().kind !== "sheet"}>{check("Visible en la hoja", elementVisible(props.sheet, el()), () => props.update((s) => toggleElement(s, el())))}</Show>
        <Switch>
          <Match when={el().kind === "sheet"}>
            {row(
              "Nombre",
              <input
                aria-label="Nombre del plano"
                class={input}
                value={props.sheet.name}
                onChange={(e) => {
                  const v = e.currentTarget.value.trim();
                  if (v) props.update((s) => (s.name = v));
                }}
              />,
            )}
            {row(
              "Hoja",
              <div class="w-28">
                <Select options={SHEETS.map((s) => ({ value: s.name, label: s.name }))} value={props.sheet.sheet} onChange={(v) => props.update((s) => (s.sheet = v))} />
              </div>,
            )}
            {row(
              "Proyección",
              <div class="w-28">
                <Select
                  options={[
                    { value: "first", label: "1.er diedro (ISO)" },
                    { value: "third", label: "3.er diedro (ANSI)" },
                  ]}
                  value={props.sheet.projection}
                  onChange={(v) => props.update((s) => (s.projection = v as Projection))}
                />
              </div>,
            )}
            {row(
              "Escala",
              <div class="w-28">
                <Select options={scaleOptions(props.scale)} value={props.sheet.scale ? String(props.sheet.scale) : "auto"} onChange={(v) => props.update((s) => (s.scale = v === "auto" ? null : Number(v)))} />
              </div>,
            )}
            <div class="flex flex-wrap gap-x-3">
              {check("Ocultas", props.sheet.hiddenLines, (v) => props.update((s) => (s.hiddenLines = v)))}
              {check("Tangentes", props.sheet.smooth, (v) => props.update((s) => (s.smooth = v)))}
            </div>
            {row("Título", <input aria-label="Título del plano" class={input} value={props.sheet.title} onChange={(e) => props.update((s) => (s.title = e.currentTarget.value))} />)}
            {row("Autor", <input aria-label="Autor" placeholder="Autor" class={input} value={props.sheet.author} onChange={(e) => props.update((s) => (s.author = e.currentTarget.value))} />)}
          </Match>
          <Match when={el().kind === "view"}>
            <p class="text-[11px] text-text-dim">Escala {scaleLabel(props.scale)} (la de la hoja).</p>
          </Match>
          <Match when={el().kind === "overall"}>
            <p class="text-[11px] text-text-dim">Ancho y alto de cada vista.</p>
          </Match>
          <Match when={el().kind === "section"}>
            {row(
              "En Y (mm)",
              <input
                aria-label="Posición del corte"
                type="number"
                step="1"
                placeholder="medio"
                class={input}
                value={props.sheet.section?.y ?? ""}
                onChange={(e) => {
                  const v = e.currentTarget.value.trim();
                  props.update((s) => (s.section = { ...(s.section ?? {}), y: v === "" || !isFinite(num(v)) ? null : num(v) }));
                }}
              />,
            )}
            <p class="text-[11px] text-text-dim">Paralelo al frente; vacío: por el medio del sólido.</p>
          </Match>
          <Match when={detail()}>
            {(det) => (
              <>
                {row(
                  "Radio (mm)",
                  <input
                    aria-label={`Radio del detalle ${det().label}`}
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
                  "Escala",
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
              Mide{" "}
              <span class="font-mono text-text" data-dim-value>
                {props.dimValue((el() as { index: number }).index) ?? "—"}
              </span>
            </p>
            <p class="text-[11px] text-text-dim">Sigue al modelo: si cambia el diseño, se vuelve a medir.</p>
          </Match>
          <Match when={el().kind === "sketch"}>
            <Button size="sm" fullWidth onClick={() => props.editDrawing()}>
              Editar el dibujo
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
            Quitar
          </Button>
        </Show>
      </div>
    </div>
  );
};

/** Hoja de fondo de la última vez que se dibujó (se libera al preparar otra) */
let lastBackdrop: string | undefined;

/**
 * La hoja como imagen para dibujar encima: el SVG sin el papel blanco, con
 * las líneas en el color del texto del tema (claras en el oscuro). Devuelve
 * una URL de objeto (PNG), a lo sumo 4096 px de lado.
 */
async function rasterSheet(svg: string, width: number, height: number): Promise<string> {
  const k = Math.min(12, 4096 / Math.max(width, height));
  const [w, h] = [Math.round(width * k), Math.round(height * k)];
  const clean = svg.replace(/<rect width="[\d.]+" height="[\d.]+" fill="#fff"\/>/, "").replace(/width="[\d.]+mm" height="[\d.]+mm"/, `width="${w}" height="${h}"`);
  const url = URL.createObjectURL(new Blob([clean], { type: "image/svg+xml" }));
  try {
    const img = new Image();
    img.src = url;
    await img.decode();
    const canvas = document.createElement("canvas");
    canvas.width = w;
    canvas.height = h;
    const ctx = canvas.getContext("2d")!;
    if (document.documentElement.dataset.theme !== "light") ctx.filter = "invert(1)";
    ctx.drawImage(img, 0, 0, w, h);
    const blob = await new Promise<Blob | null>((ok) => canvas.toBlob(ok, "image/png"));
    if (!blob) throw new Error("No se pudo preparar la hoja para dibujar");
    return URL.createObjectURL(blob);
  } finally {
    URL.revokeObjectURL(url);
  }
}
