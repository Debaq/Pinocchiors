import { Component, For, Show, createEffect, createMemo, createSignal, on } from "solid-js";
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
  layout,
  lineNear,
  placeDetails,
  scaleLabel,
  sheetDxf,
  sheetSvg,
  viewSpec,
  type Detail,
  type DrawingLine,
  type P2,
  type Projection,
  type UserDim,
  type ViewName,
} from "../../lib/drawing";
import { Button, Select } from "../ui";

/**
 * Plano 2D del diseño: frente, planta, lateral e isométrica con líneas ocultas
 * exactas, escala normalizada y cajetín; se exporta a SVG. Se rehace solo
 * cuando cambia el diseño.
 */
export const DrawingView: Component<{ store: CadStore; ui: CadUi }> = (props) => {
  const [sheetName, setSheetName] = createSignal("A4");
  const [projection, setProjection] = createSignal<Projection>("first");
  const [scaleChoice, setScaleChoice] = createSignal("auto");
  const [hidden, setHidden] = createSignal(true);
  const [smooth, setSmooth] = createSignal(false);
  const [iso, setIso] = createSignal(true);
  const [dimensions, setDimensions] = createSignal(true);
  // Cotas del usuario (guardadas en el documento) y herramienta activa
  const userDims = () => props.store.doc()?.drawing?.dims ?? [];
  const setUserDims = (dims: UserDim[]) => void props.store.commit((d) => (d.drawing = { ...(d.drawing ?? {}), dims }));
  // Vistas de detalle (también en el documento)
  const details = () => props.store.doc()?.drawing?.details ?? [];
  const setDetails = (list: Detail[]) => void props.store.commit((d) => (d.drawing = { ...(d.drawing ?? {}), details: list }));
  const [tool, setTool] = createSignal<"" | "dim" | "distance" | "angle" | "detail">("");
  let firstLine: { view: string; ref: P2 } | null = null;
  // Frente en corte por el plano medio (A-A), marcado en la planta
  const [section, setSection] = createSignal(false);
  // Dónde corta (Y del modelo, mm); vacío: por el medio
  const [sectionY, setSectionY] = createSignal<number>();
  const [title, setTitle] = createSignal("Diseño");
  const [author, setAuthor] = createSignal("");
  const [views, setViews] = createSignal<Partial<Record<ViewName, DrawingLine[]>>>();
  const [hatch, setHatch] = createSignal<Partial<Record<ViewName, [P2, P2, P2][]>>>({});
  const [sectionAt, setSectionAt] = createSignal(0);
  const [busy, setBusy] = createSignal(false);
  const [message, setMessage] = createSignal<string>();
  let seq = 0;

  createEffect(
    on([() => props.store.result()?.version, projection, iso, section, sectionY], async () => {
      const names: ViewName[] = ["front", "top", "side", ...(iso() ? (["iso"] as const) : [])];
      const n = ++seq;
      setBusy(true);
      try {
        // El corte va por el medio del sólido, paralelo al frente: se queda la mitad de atrás
        const body = props.store.result()?.body;
        const at = sectionY() ?? (body ? (body.bbox_min[1] + body.bbox_max[1]) / 2 : 0);
        const specs = names.map((v) => {
          const spec = viewSpec(v, projection());
          return v === "front" && section() ? { ...spec, section: { origin: [0, at, 0], normal: [0, 1, 0] } } : spec;
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

  const sheet = () => SHEETS.find((s) => s.name === sheetName()) ?? SHEETS[0];
  const result = createMemo(() => {
    const v = views();
    if (!v) return null;
    const r = layout(v, sheet(), projection(), scaleChoice() === "auto" ? undefined : Number(scaleChoice()), hatch());
    return { ...r, placed: [...r.placed, ...placeDetails(r.placed, details(), sheet())] };
  });
  const svg = createMemo(() => {
    const r = result();
    if (!r) return "";
    const today = new Date().toISOString().slice(0, 10);
    return sheetSvg(
      r.placed,
      sheet(),
      {
        title: title(),
        author: author(),
        material: props.store.doc()?.material?.name ?? "",
        date: today,
        scale: scaleLabel(r.scale),
        projection: projection(),
        sheet: sheet().name,
      },
      { hidden: hidden(), smooth: smooth(), dimensions: dimensions(), section: section() ? { label: "A", at: sectionAt() } : undefined, userDims: userDims() },
    );
  });

  const exportSvg = async () => {
    const path = await save({ filters: [{ name: "SVG", extensions: ["svg"] }], defaultPath: `${title().replace(/[^\p{L}\p{N}_-]+/gu, "_") || "plano"}.svg` });
    if (!path) return;
    try {
      const bytes = await invoke<number>("cad_write_text", { path, content: svg() });
      setMessage(`Exportado (${Math.round(bytes / 1024)} KB)`);
    } catch (e) {
      setMessage(String(e));
    }
  };

  const exportPdf = async () => {
    const path = await save({ filters: [{ name: "PDF", extensions: ["pdf"] }], defaultPath: `${title().replace(/[^\p{L}\p{N}_-]+/gu, "_") || "plano"}.pdf` });
    if (!path) return;
    try {
      const bytes = await invoke<number>("cad_write_pdf", { path, svg: svg() });
      setMessage(`Exportado (${Math.round(bytes / 1024)} KB)`);
    } catch (e) {
      setMessage(String(e));
    }
  };

  // DXF: la hoja entera o una vista sola a 1:1 (para corte láser o CNC)
  const [dxfWhat, setDxfWhat] = createSignal<"sheet" | ViewName>("sheet");
  const exportDxf = async () => {
    const r = result();
    if (!r) return;
    const what = dxfWhat();
    const name = `${title().replace(/[^\p{L}\p{N}_-]+/gu, "_") || "plano"}${what === "sheet" ? "" : `_${what}_1a1`}.dxf`;
    const path = await save({ filters: [{ name: "DXF", extensions: ["dxf"] }], defaultPath: name });
    if (!path) return;
    try {
      const content = sheetDxf(r.placed, sheet(), { hidden: hidden(), smooth: smooth(), dimensions: dimensions(), userDims: userDims(), view: what === "sheet" ? undefined : what });
      const bytes = await invoke<number>("cad_write_text", { path, content });
      setMessage(`Exportado (${Math.round(bytes / 1024)} KB)`);
    } catch (e) {
      setMessage(String(e));
    }
  };

  /**
   * Clic en la hoja con una herramienta de cota: busca la línea de la vista bajo
   * el puntero. "Cota": largo de una recta o diámetro de un círculo;
   * "Distancia": dos rectas paralelas, una después de la otra.
   */
  const onSheetClick = (e: MouseEvent) => {
    const r = result();
    const t = tool();
    if (!r || !t) return;
    const svgEl = (e.currentTarget as HTMLElement).querySelector("svg");
    const m = svgEl?.getScreenCTM();
    if (!svgEl || !m) return;
    const pt = new DOMPoint(e.clientX, e.clientY).matrixTransform(m.inverse());
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
        const radius = Math.round(Math.max(b.max[0] - b.min[0], b.max[1] - b.min[1]) / 8 * 10) / 10 || 5;
        const used = new Set(details().map((d) => d.label));
        const label = "BCDEFGHJKLMNPQRSTUVWXYZ".split("").find((l) => !used.has(l)) ?? `${details().length + 1}`;
        setDetails([...details(), { label, view: v.name as ViewName, center: p, radius }]);
        setMessage(undefined);
        return;
      }
      const line = lineNear(v, p, 3 / v.scale);
      if (!line) continue;
      if (t === "dim") {
        if (asCircle(line)) setUserDims([...userDims(), { view: v.name, kind: "diameter", refs: [p] }]);
        else if (asSegment(line)) setUserDims([...userDims(), { view: v.name, kind: "length", refs: [p] }]);
        else if (asArc(line)) setUserDims([...userDims(), { view: v.name, kind: "radius", refs: [p] }]);
        else setMessage("Esa línea no es recta, ni un círculo, ni un arco");
        return;
      }
      if (!asSegment(line)) return setMessage(t === "angle" ? "Elegir rectas para el ángulo" : "Elegir rectas para la distancia");
      if (!firstLine || firstLine.view !== v.name) {
        firstLine = { view: v.name, ref: p };
        return setMessage(t === "angle" ? "Ahora la otra recta del ángulo" : "Ahora la recta paralela");
      }
      setUserDims([...userDims(), { view: v.name, kind: t === "angle" ? "angle" : "distance", refs: [firstLine.ref, p] }]);
      firstLine = null;
      setMessage(undefined);
      return;
    }
  };

  const check = (label: string, value: () => boolean, set: (v: boolean) => void) => (
    <label class="flex items-center gap-1 text-xs text-text-muted cursor-pointer">
      <input type="checkbox" checked={value()} onChange={(e) => set(e.currentTarget.checked)} />
      {label}
    </label>
  );

  return (
    <div class="absolute inset-0 z-[6] flex flex-col bg-bg-darker" aria-label="Plano 2D">
      <div class="flex flex-wrap items-center gap-2 border-b border-border bg-bg-lighter px-2 py-1.5">
        <span class="text-xs font-semibold text-text">Plano</span>
        <div class="w-24">
          <Select options={SHEETS.map((s) => ({ value: s.name, label: s.name }))} value={sheetName()} onChange={setSheetName} />
        </div>
        <div class="w-44">
          <Select
            options={[
              { value: "first", label: "Primer diedro (ISO)" },
              { value: "third", label: "Tercer diedro (ANSI)" },
            ]}
            value={projection()}
            onChange={(v) => setProjection(v as Projection)}
          />
        </div>
        <div class="w-32">
          <Select
            options={[{ value: "auto", label: result() ? `Auto (${scaleLabel(result()!.scale)})` : "Auto" }, ...SCALES.map((s) => ({ value: String(s), label: scaleLabel(s) }))]}
            value={scaleChoice()}
            onChange={setScaleChoice}
          />
        </div>
        {check("Ocultas", hidden, setHidden)}
        {check("Tangentes", smooth, setSmooth)}
        {check("Isométrica", iso, setIso)}
        {check("Cotas", dimensions, setDimensions)}
        {check("Corte A-A", section, setSection)}
        <Show when={section()}>
          <label class="flex items-center gap-1 text-xs text-text-muted" title="Dónde corta, paralelo al frente (vacío: por el medio)">
            en Y
            <input
              aria-label="Posición del corte"
              type="number"
              step="1"
              placeholder="medio"
              class="w-16 px-1 py-0.5 rounded bg-surface/40 border border-border text-xs text-text outline-none focus:border-accent"
              value={sectionY() ?? ""}
              onChange={(e) => {
                const v = e.currentTarget.value.trim().replace(",", ".");
                setSectionY(v === "" || !isFinite(Number(v)) ? undefined : Number(v));
              }}
            />
          </label>
        </Show>
        <div class="flex items-center gap-0.5" role="radiogroup" aria-label="Herramienta de cotas">
          <For each={[["dim", "Cota"], ["distance", "Distancia"], ["angle", "Ángulo"], ["detail", "Detalle"]] as const}>
            {([id, label]) => (
              <button
                role="radio"
                aria-checked={tool() === id}
                class={clsx("px-1.5 py-0.5 rounded text-[11px]", tool() === id ? "bg-accent text-bg" : "text-text-muted hover:text-text hover:bg-surface")}
                title={
                  {
                    dim: "Clic en una recta (largo), un círculo (diámetro) o un arco (radio)",
                    distance: "Clic en dos rectas paralelas",
                    angle: "Clic en dos rectas que se cruzan",
                    detail: "Clic en una vista: amplía esa zona aparte",
                  }[id]
                }
                onClick={() => {
                  firstLine = null;
                  setTool(tool() === id ? "" : id);
                }}
              >
                {label}
              </button>
            )}
          </For>
          <Show when={userDims().length > 0}>
            <button class="px-1.5 py-0.5 rounded text-[11px] text-text-muted hover:text-text" onClick={() => setUserDims(userDims().slice(0, -1))} title="Quitar la última cota">
              Quitar última
            </button>
          </Show>
        </div>
        {/* Detalles: radio y escala de cada uno */}
        <For each={details()}>
          {(d, i) => {
            const edit = (change: (x: Detail) => void) =>
              setDetails(
                details().map((x, j) => {
                  if (j !== i()) return x;
                  const y = { ...x };
                  change(y);
                  return y;
                }),
              );
            const auto = () => detailScale(result()?.scale ?? 1);
            return (
              <div data-detail-chip={d.label} class="flex items-center gap-1 rounded border border-border px-1 py-0.5 text-[11px] text-text-muted">
                <span class="text-text">Detalle {d.label}</span>
                <span>R</span>
                <input
                  aria-label={`Radio del detalle ${d.label}`}
                  type="number"
                  min="0.5"
                  step="0.5"
                  class="w-12 px-1 rounded bg-surface/40 border border-border text-text outline-none focus:border-accent"
                  value={d.radius}
                  onChange={(e) => {
                    const v = Number(e.currentTarget.value.replace(",", "."));
                    if (v > 0) edit((x) => (x.radius = v));
                  }}
                />
                <div class="w-20">
                  <Select
                    options={[{ value: "auto", label: `Auto (${scaleLabel(auto())})` }, ...SCALES.map((s) => ({ value: String(s), label: scaleLabel(s) }))]}
                    value={d.scale ? String(d.scale) : "auto"}
                    onChange={(v) => edit((x) => (x.scale = v === "auto" ? undefined : Number(v)))}
                  />
                </div>
                <button aria-label={`Quitar detalle ${d.label}`} class="hover:text-text" onClick={() => setDetails(details().filter((_, j) => j !== i()))}>
                  ×
                </button>
              </div>
            );
          }}
        </For>
        <input
          aria-label="Título del plano"
          class="w-36 px-1.5 py-0.5 rounded bg-surface/40 border border-border text-xs text-text outline-none focus:border-accent"
          value={title()}
          onInput={(e) => setTitle(e.currentTarget.value)}
        />
        <input
          aria-label="Autor"
          placeholder="Autor"
          class="w-28 px-1.5 py-0.5 rounded bg-surface/40 border border-border text-xs text-text outline-none focus:border-accent"
          value={author()}
          onInput={(e) => setAuthor(e.currentTarget.value)}
        />
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
      <div class="flex-1 overflow-auto p-4">
        {/* La hoja ocupa el ancho disponible, con su proporción */}
        <div
          data-sheet
          class={clsx("mx-auto max-w-[1400px] shadow-lg [&>svg]:block [&>svg]:h-auto [&>svg]:w-full", tool() && "cursor-crosshair")}
          innerHTML={svg()}
          onClick={onSheetClick}
        />
      </div>
    </div>
  );
};
