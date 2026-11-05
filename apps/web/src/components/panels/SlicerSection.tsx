import { Component, For, Show, createEffect, createResource, createSignal, on, onCleanup } from "solid-js";
import { Channel, invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { Button, Checkbox, NumberInput, Panel, Select } from "../ui";
import * as Icons from "../icons";
import { fmtNumber, t } from "../../lib/i18n";
import { tBackend } from "../../lib/i18nBackend.ts";
import { FEATURES, ORCA_ROLE_NAMES, decodePreview, gcodePreview, type GcodeMeta } from "../../lib/gcodePreview";

// ═══════════════════════════════════════════════════════════════════════════
// TIPOS (espejo de apps/desktop/src/slicer.rs)
// ═══════════════════════════════════════════════════════════════════════════

export interface SlicerPrinter {
  name: string;
  model: string;
  nozzle: number;
  default_process: string;
  default_filament: string;
  processes: string[];
  filaments: string[];
}

export interface SlicerInfo {
  available: boolean;
  engine: string | null;
  orca_version: string;
  printers: SlicerPrinter[];
}

/** Lo elegido en «Laminar»; se guarda en el proyecto con el resto de Fabricar */
export interface SlicerSettings {
  printer?: string;
  process?: string;
  filament?: string;
  /** Cambios sobre el perfil, con las claves de Orca (valores en texto) */
  overrides: Record<string, string>;
  /** Líneas `clave = valor` para cualquier otro ajuste de Orca */
  advanced?: string;
  plate?: number;
}

export const defaultSlicer: SlicerSettings = { overrides: {} };

export interface SliceResult {
  plate: number;
  plates: number;
  time_s: number;
  filament_mm: number;
  filament_cm3: number;
  filament_g: number;
  layers: number;
  warnings: string[];
  roles: { role: string; m: number; g: number }[];
  bed: [number, number];
  gcode_bytes: number;
  unplaced: string[];
  preview: GcodeMeta;
}

interface Progress {
  stage: string;
  percent: number;
  message: string;
}

/** Datos que necesita el acomodo de placas (igual que «Exportar para imprimir», sin ruta) */
export type PlateInputArgs = Record<string, unknown>;

// ═══════════════════════════════════════════════════════════════════════════
// AYUDAS
// ═══════════════════════════════════════════════════════════════════════════

/** La impresora del laminador que corresponde a la de Fabricar (por nombre) */
export function matchSlicerPrinter(printers: SlicerPrinter[], name: string | undefined): SlicerPrinter | undefined {
  if (!name) return undefined;
  const n = name.toLowerCase().replace(/\s+/g, "");
  const key = (p: SlicerPrinter) => p.model.toLowerCase().replace(/\s+/g, "");
  return printers.find((p) => key(p) === n) ?? printers.find((p) => n.includes(key(p)) || key(p).includes(n));
}

/** Nombre corto de un proceso: «0.20mm Standard @Creality K1 (0.4 nozzle)» → «0.20mm Standard» */
const shortName = (name: string) => name.split("@")[0].trim();

function parseAdvanced(text: string | undefined): Record<string, string> {
  const out: Record<string, string> = {};
  for (const raw of (text ?? "").split("\n")) {
    const line = raw.trim();
    if (!line || line.startsWith("#")) continue;
    const i = line.indexOf("=");
    if (i <= 0) continue;
    out[line.slice(0, i).trim()] = line.slice(i + 1).trim();
  }
  return out;
}

function fmtTime(seconds: number): string {
  const s = Math.round(seconds);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  return h > 0 ? `${h} h ${m} min` : `${m} min ${s % 60} s`;
}

/** Valor de una clave del perfil como texto (las listas por extrusor usan el primero) */
function first(v: unknown): string {
  if (Array.isArray(v)) return String(v[0] ?? "");
  return v == null ? "" : String(v);
}

const num = (v: string, fallback: number) => {
  const x = parseFloat(v);
  return Number.isFinite(x) ? x : fallback;
};

const INFILL_PATTERNS = [
  ["grid", "Rejilla"],
  ["gyroid", "Giroide"],
  ["cubic", "Cúbico"],
  ["adaptivecubic", "Cúbico adaptativo"],
  ["crosshatch", "Entramado"],
  ["rectilinear", "Rectilíneo"],
  ["line", "Líneas"],
  ["triangles", "Triángulos"],
  ["honeycomb", "Panal"],
  ["3dhoneycomb", "Panal 3D"],
  ["lightning", "Rayo (solo sostiene el techo)"],
  ["concentric", "Concéntrico"],
] as const;

const SEAMS = [
  ["aligned", "Alineada"],
  ["aligned_back", "Alineada atrás"],
  ["nearest", "Más cercana"],
  ["back", "Atrás"],
  ["random", "Al azar"],
] as const;

const BRIMS = [
  ["auto_brim", "Automático"],
  ["no_brim", "Sin borde"],
  ["outer_only", "Solo por fuera"],
  ["brim_ears", "Orejas en las esquinas"],
] as const;

// ═══════════════════════════════════════════════════════════════════════════
// PANEL (a la izquierda del visor, ver layout/SlicerEditor.tsx)
// ═══════════════════════════════════════════════════════════════════════════

export type SlicerTab = "settings" | "preview";

/** Tamaño de la cama del perfil de Orca (mm: ancho, fondo, alto) */
export function slicerBed(config: Record<string, unknown> | undefined): [number, number, number] | undefined {
  const area = config?.["printable_area"];
  const points = (Array.isArray(area) ? area : typeof area === "string" ? area.split(",") : [])
    .map((p) => String(p).split("x").map(Number))
    .filter((p) => p.length === 2 && p.every(Number.isFinite));
  if (points.length < 3) return undefined;
  const xs = points.map((p) => p[0]);
  const ys = points.map((p) => p[1]);
  const w = Math.max(...xs) - Math.min(...xs);
  const d = Math.max(...ys) - Math.min(...ys);
  const h = parseFloat(first(config?.["printable_height"]));
  return w > 0 && d > 0 ? [w, d, Number.isFinite(h) && h > 0 ? h : 0] : undefined;
}

export const SlicerSection: Component<{
  settings: SlicerSettings;
  onSettings: (s: SlicerSettings) => void;
  /** Nombre de la impresora elegida en Fabricar (para proponer la del laminador) */
  printerName?: string;
  /** Datos para acomodar las placas; undefined si falta impresora o material */
  plateInput: () => PlateInputArgs | undefined;
  /** Cuántas placas salen (de «Ver cómo quedan en la cama»), si se sabe */
  plateCount?: number;
  disabled?: boolean;
  tab: SlicerTab;
  onTab: (tab: SlicerTab) => void;
  /** Cama de la impresora del laminador (para dibujarla en el visor) */
  onBed?: (bed: [number, number, number] | undefined) => void;
}> = (props) => {
  const [info] = createResource(() => invoke<SlicerInfo>("slicer_presets").catch(() => undefined));
  const [busy, setBusy] = createSignal(false);
  const [progress, setProgress] = createSignal<Progress>();
  const [result, setResult] = createSignal<SliceResult>();
  const [error, setError] = createSignal<string>();
  const [saved, setSaved] = createSignal<string>();

  const set = (patch: Partial<SlicerSettings>) => props.onSettings({ ...props.settings, ...patch });
  const printers = () => info()?.printers ?? [];
  const printer = () =>
    printers().find((p) => p.name === props.settings.printer) ?? matchSlicerPrinter(printers(), props.printerName) ?? printers()[0];
  const process = () => {
    const p = printer();
    if (!p) return undefined;
    return p.processes.includes(props.settings.process ?? "") ? props.settings.process : p.default_process;
  };
  const filament = () => {
    const p = printer();
    if (!p) return undefined;
    return p.filaments.includes(props.settings.filament ?? "") ? props.settings.filament : p.default_filament;
  };

  // Valores del perfil elegido (sin cambios): de ahí salen los números que se muestran
  const [base] = createResource(
    () => {
      const p = printer();
      const pr = process();
      const f = filament();
      return p && pr && f ? { printer: p.name, process: pr, filament: f } : undefined;
    },
    (args) => invoke<Record<string, unknown>>("slicer_config", args).catch(() => ({}) as Record<string, unknown>),
  );
  const value = (key: string) => props.settings.overrides[key] ?? first(base()?.[key]);
  const setValue = (key: string, v: string) => {
    const overrides = { ...props.settings.overrides };
    if (v === first(base()?.[key])) delete overrides[key];
    else overrides[key] = v;
    set({ overrides });
  };
  const changed = () => Object.keys(props.settings.overrides).length;

  // La cama de esta impresora se dibuja en el visor y es la que se usa para acomodar las piezas
  const bed = () => slicerBed(base());
  createEffect(() => props.onBed?.(bed()));
  onCleanup(() => props.onBed?.(undefined));

  // Al cambiar de perfil, el laminado anterior ya no vale
  createEffect(
    on([() => props.settings.process, () => props.settings.filament, () => props.settings.printer], () => {
      setResult(undefined);
      gcodePreview.setVisible(false);
      props.onTab("settings");
    }, { defer: true }),
  );

  const plate = () => Math.min(props.settings.plate ?? 0, Math.max(0, (props.plateCount ?? 1) - 1));

  const slice = async () => {
    const plateInput = props.plateInput();
    const p = printer();
    const pr = process();
    const f = filament();
    if (!plateInput || !p || !pr || !f) return;
    // Las piezas se acomodan en la cama de la impresora del laminador
    const b = bed();
    const input = b ? { ...plateInput, printer: { ...(plateInput.printer as Record<string, unknown>), build_volume: b } } : plateInput;
    setBusy(true);
    setError(undefined);
    setSaved(undefined);
    setProgress({ stage: "start", percent: 0, message: t("Empezando...") });
    const channel = new Channel<Progress>();
    channel.onmessage = (m) => {
      if (m.stage !== "warning") setProgress(m);
    };
    try {
      const overrides = { ...props.settings.overrides, ...parseAdvanced(props.settings.advanced) };
      const r = await invoke<SliceResult>("slice_plate", {
        input,
        plate: plate(),
        settings: { printer: p.name, process: pr, filament: f, overrides },
        onProgress: channel,
      });
      setResult(r);
      await showPreview(r);
      props.onTab("preview");
    } catch (e) {
      const msg = typeof e === "string" ? tBackend(e) : String(e);
      if (!/cancelad/i.test(String(e))) setError(msg);
    } finally {
      setBusy(false);
      setProgress(undefined);
    }
  };
  onCleanup(() => {
    if (busy()) void invoke("slicer_cancel");
    gcodePreview.setVisible(false);
  });

  // Vista previa en el visor: se trae el binario de segmentos y se muestra la placa completa
  const showPreview = async (r: SliceResult) => {
    try {
      const buffer = await invoke<ArrayBuffer>("slicer_preview");
      gcodePreview.setPreview(decodePreview(buffer, r.preview, r.bed));
      gcodePreview.setHidden([]);
      gcodePreview.setLayer(Math.max(0, r.preview.layer_z.length - 1));
      gcodePreview.setVisible(true);
    } catch (e) {
      setError(typeof e === "string" ? tBackend(e) : String(e));
    }
  };

  const saveGcode = async () => {
    const path = await save({ title: t("Guardar gcode"), filters: [{ name: "G-code", extensions: ["gcode"] }] });
    if (!path) return;
    try {
      await invoke<number>("save_gcode", { path });
      setSaved(path);
    } catch (e) {
      setError(typeof e === "string" ? tBackend(e) : String(e));
    }
  };

  const int = (key: string, label: string, fallback: number, min: number, max: number, step = 1, suffix?: string) => (
    <NumberInput label={t(label)} value={num(value(key), fallback)} onChange={(v) => setValue(key, String(Math.round(v)))} min={min} max={max} step={step} suffix={suffix} />
  );

  const settingsPanels = () => (
    <div>
      <div>
        <Panel id="slicer.profile" dense title={t("Impresora y perfil")} icon={<Icons.Stack size={14} />} defaultOpen>
          <div class="space-y-2 pb-3">
            <Select
              label={t("Impresora")}
              options={printers().map((p) => ({ value: p.name, label: p.name }))}
              value={printer()?.name}
              onChange={(v) => set({ printer: v, process: undefined, filament: undefined, overrides: {} })}
            />
            <Select
              label={t("Calidad")}
              options={(printer()?.processes ?? []).map((p) => ({ value: p, label: shortName(p) }))}
              value={process()}
              onChange={(v) => set({ process: v, overrides: {} })}
            />
            <Select
              label={t("Filamento")}
              options={(printer()?.filaments ?? []).map((f) => ({ value: f, label: shortName(f) }))}
              value={filament()}
              onChange={(v) => set({ filament: v })}
            />
            <Show when={bed()}>
              {(b) => (
                <p class="text-[10px] text-text-dim">
                  {t("Cama de {x} × {y} mm", { x: fmtNumber(b()[0]), y: fmtNumber(b()[1]) })}
                  <Show when={b()[2] > 0}> · {t("alto {z} mm", { z: fmtNumber(b()[2]) })}</Show>
                </p>
              )}
            </Show>
            <Show when={changed() > 0}>
              <button class="text-[11px] text-text-dim hover:underline" onClick={() => set({ overrides: {} })}>
                {t("Volver al perfil ({n} cambios)", { n: changed() })}
              </button>
            </Show>
          </div>
        </Panel>
      </div>
      <Show when={base()}>
        <div>
          <Panel id="slicer.layers" dense title={t("Capas y paredes")} defaultOpen>
            <div class="space-y-2 pb-3">
              <NumberInput label={t("Altura de capa")} value={num(value("layer_height"), 0.2)} onChange={(v) => setValue("layer_height", String(v))} min={0.04} max={0.32} step={0.02} suffix="mm" />
              {int("wall_loops", "Paredes", 2, 1, 10)}
              {int("top_shell_layers", "Capas arriba", 5, 0, 20)}
              {int("bottom_shell_layers", "Capas abajo", 3, 0, 20)}
            </div>
          </Panel>
        </div>
        <div>
          <Panel id="slicer.infill" dense title={t("Relleno")} defaultOpen>
            <div class="space-y-2 pb-3">
              <NumberInput label={t("Relleno")} value={num(value("sparse_infill_density"), 15)} onChange={(v) => setValue("sparse_infill_density", `${Math.round(v)}%`)} min={0} max={100} step={5} suffix="%" />
              <Select
                label={t("Patrón de relleno")}
                options={INFILL_PATTERNS.map(([value, label]) => ({ value, label: t(label) }))}
                value={value("sparse_infill_pattern")}
                onChange={(v) => setValue("sparse_infill_pattern", v)}
              />
            </div>
          </Panel>
        </div>
        <div>
          <Panel id="slicer.support" dense title={t("Soportes")} defaultOpen>
            <div class="space-y-2 pb-3">
              <Checkbox small label={t("Soportes")} checked={value("enable_support") === "1"} onChange={(v) => setValue("enable_support", v ? "1" : "0")} hint={t("Pone soportes debajo de los voladizos más inclinados que el ángulo límite.")} />
              <Show when={value("enable_support") === "1"}>
                <Select
                  label={t("Tipo de soporte")}
                  options={[
                    { value: "normal(auto)", label: t("Normal") },
                    { value: "tree(auto)", label: t("Árbol") },
                  ]}
                  value={value("support_type").startsWith("tree") ? "tree(auto)" : "normal(auto)"}
                  onChange={(v) => setValue("support_type", v)}
                />
                {int("support_threshold_angle", "Ángulo límite", 30, 0, 90, 5, "°")}
                <Checkbox small label={t("Solo apoyados en la cama")} checked={value("support_on_build_plate_only") === "1"} onChange={(v) => setValue("support_on_build_plate_only", v ? "1" : "0")} />
              </Show>
            </div>
          </Panel>
        </div>
        <div>
          <Panel id="slicer.adhesion" dense title={t("Adhesión y costura")} defaultOpen>
            <div class="space-y-2 pb-3">
              <Select label={t("Borde (brim)")} options={BRIMS.map(([value, label]) => ({ value, label: t(label) }))} value={value("brim_type")} onChange={(v) => setValue("brim_type", v)} />
              <Select label={t("Costura")} options={SEAMS.map(([value, label]) => ({ value, label: t(label) }))} value={value("seam_position")} onChange={(v) => setValue("seam_position", v)} />
            </div>
          </Panel>
        </div>
        <div>
          <Panel id="slicer.temps" dense title={t("Temperaturas")} defaultOpen>
            <div class="space-y-2 pb-3">
              {int("nozzle_temperature", "Temperatura boquilla", 220, 150, 320, 5, "°C")}
              <NumberInput label={t("Temperatura cama")} value={num(props.settings.overrides["bed_temperature"] ?? first(base()?.["hot_plate_temp"]), 60)} onChange={(v) => setValue("bed_temperature", String(Math.round(v)))} min={0} max={120} step={5} suffix="°C" />
            </div>
          </Panel>
        </div>
        <div>
          <Panel id="slicer.speed" dense title={t("Velocidad")} defaultOpen>
            <div class="space-y-2 pb-3">
              {int("outer_wall_speed", "Velocidad pared externa", 200, 10, 600, 10, "mm/s")}
              {int("sparse_infill_speed", "Velocidad relleno", 270, 10, 600, 10, "mm/s")}
            </div>
          </Panel>
        </div>
        <div>
          <Panel id="slicer.advanced" dense title={t("Avanzado")} defaultOpen={false}>
            <div class="space-y-2 pb-3">
              <p class="text-[10px] text-text-dim">{t("Cualquier ajuste de Orca, uno por línea: clave = valor (por ejemplo ironing_type = top).")}</p>
              <textarea
                class="w-full h-28 rounded border border-border bg-surface/30 p-1.5 font-mono text-[11px] text-text"
                spellcheck={false}
                value={props.settings.advanced ?? ""}
                onChange={(e) => set({ advanced: e.currentTarget.value })}
              />
            </div>
          </Panel>
        </div>
      </Show>
    </div>
  );

  const previewPanels = () => (
    <Show when={result()} fallback={<p class="text-[11px] text-text-dim">{t("Lamina la placa para ver capa por capa cómo se imprime.")}</p>}>
      {(r) => (
        <div>
          <div>
            <Panel id="slicer.summary" dense title={t("Resumen")} defaultOpen>
              <div class="space-y-1 pb-3">
                <ResultRow label={t("Tiempo estimado")} value={fmtTime(r().time_s)} />
                <ResultRow
                  label={t("Filamento")}
                  value={`${fmtNumber(r().filament_mm / 1000, { maximumFractionDigits: 2 })} m · ${fmtNumber(r().filament_g, { maximumFractionDigits: 1 })} g`}
                />
                <ResultRow label={t("Capas")} value={String(r().layers)} />
                <Show when={r().plates > 1}>
                  <ResultRow label={t("Placa")} value={`${r().plate + 1} / ${r().plates}`} />
                </Show>
                <Show when={r().unplaced.length > 0}>
                  <p class="text-[11px] text-red">{t("No caben en la cama: {names}", { names: r().unplaced.join(", ") })}</p>
                </Show>
                <For each={r().warnings}>{(w) => <p class="text-[11px] text-warning">{w}</p>}</For>
                <div class="pt-1">
                  <Button onClick={saveGcode} fullWidth size="sm" icon={<Icons.Download size={14} />}>
                    {t("Guardar gcode")}
                  </Button>
                </div>
                <Show when={saved()}>
                  <p class="text-[10px] text-green truncate">{t("Guardado en {path}", { path: saved()! })}</p>
                </Show>
              </div>
            </Panel>
          </div>
          <div>
            <Panel id="slicer.layersView" dense title={t("Vista previa")} defaultOpen>
              <div class="pb-3">
                <PreviewControls result={r()} />
              </div>
            </Panel>
          </div>
        </div>
      )}
    </Show>
  );

  return (
    <div class="flex flex-col h-full min-h-0">
      <div class="flex-1 min-h-0 overflow-y-auto px-3 py-2">
        <Show
          when={info()?.available}
          fallback={
            <p class="text-[11px] text-warning">
              {info.loading ? t("Buscando el motor de laminado...") : t("Falta el motor de laminado (pinocchio-slicer). Se compila con scripts/build-orca-engine.sh.")}
            </p>
          }
        >
          {/* Las secciones fluyen en columnas de ~280 px sin partirse (como Orizon3D) */}
          <div class="[column-width:280px] [column-gap:1rem] [&>div>div]:break-inside-avoid [&>div>div]:mb-3">
            {props.tab === "preview" ? previewPanels() : settingsPanels()}
          </div>
        </Show>
      </div>
      <Show when={info()?.available}>
        <footer class="shrink-0 border-t border-border px-3 py-2 space-y-1.5">
          <Show when={(props.plateCount ?? 1) > 1}>
            <Select
              label={t("Placa")}
              options={Array.from({ length: props.plateCount ?? 1 }, (_, i) => ({ value: String(i), label: t("Placa {n}", { n: i + 1 }) }))}
              value={String(plate())}
              onChange={(v) => set({ plate: Number(v) })}
            />
          </Show>
          <Show
            when={busy()}
            fallback={
              <Button onClick={slice} disabled={props.disabled || !props.plateInput()} fullWidth size="sm" variant="primary" icon={<Icons.Stack size={14} />}>
                {result() ? t("Laminar de nuevo") : t("Laminar")}
              </Button>
            }
          >
            <div class="space-y-1">
              <div class="h-1.5 w-full rounded bg-surface/50 overflow-hidden">
                <div class="h-full bg-accent transition-all" style={{ width: `${progress()?.percent ?? 0}%` }} />
              </div>
              <p class="text-[10px] text-text-dim truncate">{tBackend(progress()?.message ?? "")}</p>
              <Button onClick={() => void invoke("slicer_cancel")} fullWidth size="sm" variant="ghost" icon={<Icons.X size={14} />}>
                {t("Cancelar")}
              </Button>
            </div>
          </Show>
          <Show when={error()}>
            <p class="text-[11px] text-red">{error()}</p>
          </Show>
          <p class="text-[10px] text-text-dim">{t("Motor de OrcaSlicer {v}", { v: info()?.orca_version ?? "" })}</p>
        </footer>
      </Show>
    </div>
  );
};

const ResultRow: Component<{ label: string; value: string }> = (props) => (
  <div class="flex justify-between text-xs gap-2">
    <span class="text-text-muted">{props.label}</span>
    <span class="text-text font-mono">{props.value}</span>
  </div>
);

/** Capa, viajes y tipos de línea de la vista previa del gcode */
const PreviewControls: Component<{ result: SliceResult }> = (props) => {
  const layers = () => props.result.preview.layer_z.length;
  const layer = () => Math.min(gcodePreview.layer(), Math.max(0, layers() - 1));
  const setLayer = (v: number) => gcodePreview.setLayer(Math.max(0, Math.min(layers() - 1, Math.round(v))));
  const totalLength = () => props.result.preview.length_by_feature.reduce((a, b) => a + b, 0) || 1;
  const grams = (i: number) => props.result.roles.find((r) => r.role === ORCA_ROLE_NAMES[i])?.g;
  const used = () =>
    props.result.preview.length_by_feature
      .map((len, i) => ({ i, len }))
      .filter((f) => f.len > 0)
      .sort((a, b) => b.len - a.len);
  const toggle = (i: number) => {
    const h = gcodePreview.hidden();
    gcodePreview.setHidden(h.includes(i) ? h.filter((x) => x !== i) : [...h, i]);
  };
  return (
    <div class="space-y-1.5 pt-1">
      <Button
        onClick={() => gcodePreview.setVisible(!gcodePreview.visible())}
        fullWidth
        size="sm"
        variant={gcodePreview.visible() ? "primary" : "ghost"}
        icon={<Icons.Cube size={14} />}
      >
        {gcodePreview.visible() ? t("Salir de la vista previa") : t("Ver en el visor")}
      </Button>
      <Show when={gcodePreview.visible() && layers() > 0}>
        <div class="flex items-center gap-1">
          <button class="px-1 text-text-dim hover:text-text" onClick={() => setLayer(layer() - 1)} aria-label={t("Capa anterior")}>
            ‹
          </button>
          <input
            type="range"
            class="flex-1 accent-[var(--accent)]"
            min={0}
            max={layers() - 1}
            step={1}
            value={layer()}
            onInput={(e) => setLayer(Number(e.currentTarget.value))}
          />
          <button class="px-1 text-text-dim hover:text-text" onClick={() => setLayer(layer() + 1)} aria-label={t("Capa siguiente")}>
            ›
          </button>
        </div>
        <p class="text-[10px] text-text-dim">
          {t("Capa {n} de {total} · {z} mm", { n: layer() + 1, total: layers(), z: fmtNumber(props.result.preview.layer_z[layer()] ?? 0, { maximumFractionDigits: 2 }) })}
        </p>
        <Checkbox small label={t("Mostrar viajes")} checked={gcodePreview.travels()} onChange={(v) => gcodePreview.setTravels(v)} />
        <div class="space-y-0.5">
          <For each={used()}>
            {(f) => {
              const g = grams(f.i);
              return (
                <button
                  class={`flex w-full items-center gap-1.5 text-[11px] ${gcodePreview.hidden().includes(f.i) ? "opacity-40" : ""}`}
                  onClick={() => toggle(f.i)}
                  title={t("Mostrar u ocultar")}
                >
                  <span class="inline-block w-2.5 h-2.5 rounded-sm shrink-0" style={{ background: `#${FEATURES[f.i].color.toString(16).padStart(6, "0")}` }} />
                  <span class="flex-1 text-left text-text-muted truncate">{t(FEATURES[f.i].name)}</span>
                  <span class="font-mono text-text-dim">
                    {fmtNumber((100 * f.len) / totalLength(), { maximumFractionDigits: 0 })} %
                    <Show when={g !== undefined && g > 0}> · {fmtNumber(g!, { maximumFractionDigits: 1 })} g</Show>
                  </span>
                </button>
              );
            }}
          </For>
        </div>
      </Show>
    </div>
  );
};
