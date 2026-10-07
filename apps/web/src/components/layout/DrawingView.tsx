import { Component, Show, createEffect, createMemo, createSignal, on } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import type { CadStore } from "../../lib/cad";
import type { CadUi } from "../../lib/cadUi";
import { SCALES, SHEETS, layout, scaleLabel, sheetSvg, viewSpec, type DrawingLine, type Projection, type ViewName } from "../../lib/drawing";
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
  const [title, setTitle] = createSignal("Diseño");
  const [author, setAuthor] = createSignal("");
  const [views, setViews] = createSignal<Partial<Record<ViewName, DrawingLine[]>>>();
  const [busy, setBusy] = createSignal(false);
  const [message, setMessage] = createSignal<string>();
  let seq = 0;

  createEffect(
    on([() => props.store.result()?.version, projection, iso], async () => {
      const names: ViewName[] = ["front", "top", "side", ...(iso() ? (["iso"] as const) : [])];
      const n = ++seq;
      setBusy(true);
      try {
        const lines = await invoke<DrawingLine[][]>("cad_drawing", { views: names.map((v) => viewSpec(v, projection())) });
        if (n === seq) {
          setViews(Object.fromEntries(names.map((v, i) => [v, lines[i]])));
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
    return layout(v, sheet(), projection(), scaleChoice() === "auto" ? undefined : Number(scaleChoice()));
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
      { hidden: hidden(), smooth: smooth() },
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
        <Button size="sm" variant="primary" disabled={!svg()} onClick={() => void exportSvg()}>
          Exportar SVG
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
        <div data-sheet class="mx-auto max-w-[1400px] shadow-lg [&>svg]:block [&>svg]:h-auto [&>svg]:w-full" innerHTML={svg()} />
      </div>
    </div>
  );
};
