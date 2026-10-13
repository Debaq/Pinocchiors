import { Component, For, Show, createSignal } from "solid-js";
import { Button, Checkbox, NumberInput, Select } from "../ui";
import * as Icons from "../icons";
import { fmtNumber, t } from "../../lib/i18n";
import { tBackend } from "../../lib/i18nBackend.ts";

// ═══════════════════════════════════════════════════════════════════════════
// TIPOS (lo que devuelve el backend, en milímetros con Z arriba)
// ═══════════════════════════════════════════════════════════════════════════

export interface PrinterProfile {
  name: string;
  build_volume: [number, number, number];
  nozzle: number;
  layer_height: number;
}

export interface MaterialProfile {
  name: string;
  density: number;
  max_overhang_deg: number;
}

export interface PrintPresets {
  printers: PrinterProfile[];
  materials: MaterialProfile[];
}

export interface PrintIssue {
  severity: "error" | "warning" | "info";
  code: string;
  message: string;
}

export interface Readiness {
  subject: string;
  printable: boolean;
  is_watertight: boolean;
  components: number;
  overhang_fraction: number;
  support_volume_max: number;
  bed_contact_area: number;
  min_thickness: number | null;
  thin_fraction: number;
  fit: { fits: boolean; fits_rotated: boolean; excess: [number, number, number]; pieces_needed: [number, number, number] };
  issues: PrintIssue[];
}

export interface OrientationOption {
  /** Normal de la cara que queda abajo (en la escena) */
  down: [number, number, number];
  overhang_area: number;
  contact_area: number;
  height: number;
  support_volume_max: number;
  score: number;
}

export interface JointsReport {
  joints_placed: number;
  connections: Array<{ piece_a: number; piece_b: number; contact_area: number; requested: number; placed: number; notes: string[] }>;
}

export type JointShape = "round" | "square" | "dovetail";

export interface JointSettings {
  shape: JointShape;
  size: number;
  depth: number;
  tolerance: number;
  count: number;
}

export interface MaterialEstimate {
  volume_mm3: number;
  mass_g: number;
  filament_m: number;
  hollow: { cavity_volume: number; saved_fraction: number; volume_mm3: number; mass_g: number; filament_m: number } | null;
}

export interface PlateSettings {
  format: string;
  spacing: number;
  orient: boolean;
  hollowWall: number | null;
}

export interface PlateResult {
  files: string[];
  plates: number;
  pieces: number;
  utilization: number[];
  unplaced: string[];
  too_tall: string[];
  total_bytes: number;
}

/** La forma del backend (`PlanarShape`) a partir de lo que se elige en el panel */
export function jointShapeConfig(s: JointSettings) {
  switch (s.shape) {
    case "square":
      return { kind: "square", size: s.size };
    case "dovetail":
      return { kind: "dovetail", width_base: s.size, width_top: s.size * 0.7, length: s.size * 1.2 };
    default:
      return { kind: "round", diameter: s.size };
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// SECCIONES
// ═══════════════════════════════════════════════════════════════════════════

const Title: Component<{ children: string }> = (props) => (
  <h4 class="text-xs font-semibold uppercase tracking-wider text-text-muted">{props.children}</h4>
);

const Row: Component<{ label: string; value: string; tone?: "ok" | "bad" | "warn" }> = (props) => (
  <div class="flex justify-between text-xs gap-2">
    <span class="text-text-muted">{props.label}</span>
    <span class={props.tone === "bad" ? "text-red" : props.tone === "ok" ? "text-green" : props.tone === "warn" ? "text-warning" : "text-text font-mono"}>
      {props.value}
    </span>
  </div>
);

const mm = (v: number) => fmtNumber(v, { maximumFractionDigits: 1 });
const cm3 = (mm3: number) => fmtNumber(mm3 / 1000, { maximumFractionDigits: 1 });

/** Impresora y material: de ellos salen la cama, la boquilla y los voladizos */
export const PrinterMaterial: Component<{
  presets?: PrintPresets;
  printer: PrinterProfile;
  material: MaterialProfile;
  onPrinter: (p: PrinterProfile) => void;
  onMaterial: (m: MaterialProfile) => void;
}> = (props) => {
  const printers = () => props.presets?.printers ?? [];
  const materials = () => props.presets?.materials ?? [];
  const setVolume = (axis: number, v: number) => {
    const bv = [...props.printer.build_volume] as [number, number, number];
    bv[axis] = v;
    props.onPrinter({ ...props.printer, name: t("Personalizada"), build_volume: bv });
  };
  return (
    <div class="space-y-2">
      <Title>{t("Impresora y material")}</Title>
      <Select
        label={t("Impresora")}
        options={[...printers().map((p) => ({ value: p.name, label: p.name })), { value: t("Personalizada"), label: t("Personalizada") }]}
        value={props.printer.name}
        onChange={(name) => {
          const preset = printers().find((p) => p.name === name);
          if (preset) props.onPrinter(preset);
        }}
      />
      <p class="text-[10px] text-text-dim">{t("Volumen de impresión (mm):")}</p>
      <div class="flex gap-1.5">
        <For each={["X", "Y", "Z"]}>
          {(axis, i) => <NumberInput label={axis} value={props.printer.build_volume[i()]} onChange={(v) => setVolume(i(), v)} min={10} step={10} />}
        </For>
      </div>
      <NumberInput
        label={t("Boquilla")}
        value={props.printer.nozzle}
        onChange={(v) => props.onPrinter({ ...props.printer, nozzle: v })}
        min={0.1}
        max={2}
        step={0.1}
        suffix="mm"
      />
      <Select
        label={t("Material")}
        options={materials().map((m) => ({ value: m.name, label: m.name }))}
        value={props.material.name}
        onChange={(name) => {
          const m = materials().find((x) => x.name === name);
          if (m) props.onMaterial(m);
        }}
      />
    </div>
  );
};

export const ReadinessSection: Component<{ readiness?: Readiness; busy?: boolean; disabled?: boolean; onCheck?: () => void }> = (props) => (
  <div class="space-y-2">
    <Title>{t("¿Se puede imprimir?")}</Title>
    <Button onClick={props.onCheck} disabled={props.disabled || props.busy} loading={props.busy} fullWidth size="sm" icon={<Icons.Printer size={14} />}>
      {t("Verificar imprimibilidad")}
    </Button>
    <Show when={props.readiness}>
      {(r) => (
        <div class="space-y-1.5 p-3 rounded-md bg-surface/30 border border-border">
          <Row label={t("Resultado")} value={r().printable ? t("Se puede imprimir") : t("Hay que arreglarla")} tone={r().printable ? "ok" : "bad"} />
          <Row label={t("En voladizo")} value={`${fmtNumber(r().overhang_fraction * 100, { maximumFractionDigits: 0 })} %`} tone={r().overhang_fraction > 0.05 ? "warn" : undefined} />
          <Show when={r().min_thickness !== null}>
            <Row label={t("Pared más fina")} value={`${fmtNumber(r().min_thickness!, { maximumFractionDigits: 2 })} mm`} />
          </Show>
          <Show when={r().support_volume_max > 0}>
            <Row label={t("Soporte (máx.)")} value={`${cm3(r().support_volume_max)} cm³`} />
          </Show>
          <For each={r().issues}>
            {(issue) => (
              <div class="flex gap-1.5 items-start text-[11px] leading-snug pt-1">
                <span class={issue.severity === "error" ? "text-red" : issue.severity === "warning" ? "text-warning" : "text-text-muted"} style={{ "flex-shrink": 0 }}>
                  {issue.severity === "info" ? <Icons.Info size={12} /> : <Icons.Warning size={12} />}
                </span>
                <span class="text-text">{tBackend(issue.message)}</span>
              </div>
            )}
          </For>
          <Show when={r().issues.length === 0}>
            <p class="text-[11px] text-green">{t("Sin problemas")}</p>
          </Show>
        </div>
      )}
    </Show>
  </div>
);

export const OrientationSection: Component<{
  options?: OrientationOption[];
  busy?: boolean;
  disabled?: boolean;
  onSuggest?: () => void;
  onApply?: (down: [number, number, number]) => void;
}> = (props) => (
  <div class="space-y-2">
    <Title>{t("Orientación")}</Title>
    <Button onClick={props.onSuggest} disabled={props.disabled || props.busy} fullWidth size="sm" icon={<Icons.Cube size={14} />}>
      {t("Sugerir cómo apoyarlo")}
    </Button>
    <Show when={props.options}>
      <div class="space-y-1">
        <For each={props.options}>
          {(o, i) => (
            <div class="flex items-center justify-between px-2 py-1.5 rounded bg-surface/20 border border-border gap-2">
              <div class="text-[11px] leading-tight">
                <div class="text-text">
                  {i() === 0 ? t("Mejor") : t("Opción {n}", { n: i() + 1 })} · {mm(o.height)} mm {t("de alto")}
                </div>
                <div class="text-text-dim">
                  {t("voladizo")} {fmtNumber(o.overhang_area / 100, { maximumFractionDigits: 1 })} cm² · {t("base")} {fmtNumber(o.contact_area / 100, { maximumFractionDigits: 1 })} cm²
                </div>
              </div>
              <Button size="sm" variant={i() === 0 ? "primary" : "ghost"} onClick={() => props.onApply?.(o.down)} disabled={props.busy}>
                {t("Aplicar")}
              </Button>
            </div>
          )}
        </For>
      </div>
    </Show>
  </div>
);

export const JointsSection: Component<{
  settings: JointSettings;
  onSettings: (s: JointSettings) => void;
  report?: JointsReport;
  busy?: boolean;
  hasPieces: boolean;
  onAdd?: () => void;
}> = (props) => {
  const set = (patch: Partial<JointSettings>) => props.onSettings({ ...props.settings, ...patch });
  return (
    <div class="space-y-2 pt-2 border-t border-border/50">
      <Title>{t("Uniones entre piezas")}</Title>
      <Select
        label={t("Forma")}
        options={[
          { value: "round", label: t("Clavija redonda") },
          { value: "square", label: t("Perno cuadrado") },
          { value: "dovetail", label: t("Trapecio") },
        ]}
        value={props.settings.shape}
        onChange={(v) => set({ shape: v as JointShape })}
      />
      <div class="grid grid-cols-2 gap-1.5">
        <NumberInput label={t("Tamaño")} value={props.settings.size} onChange={(v) => set({ size: v })} min={2} max={40} step={0.5} suffix="mm" />
        <NumberInput label={t("Hondo")} value={props.settings.depth} onChange={(v) => set({ depth: v })} min={3} max={60} step={1} suffix="mm" />
        <NumberInput label={t("Holgura")} value={props.settings.tolerance} onChange={(v) => set({ tolerance: v })} min={-1.5} max={0} step={0.05} suffix="mm" />
        <NumberInput label={t("Por cara")} value={props.settings.count} onChange={(v) => set({ count: Math.max(1, Math.round(v)) })} min={1} max={8} step={1} />
      </div>
      <Button onClick={props.onAdd} disabled={!props.hasPieces || props.busy} loading={props.busy} fullWidth size="sm" variant="primary">
        {t("Añadir uniones")}
      </Button>
      <Show when={!props.hasPieces}>
        <p class="text-[10px] text-text-dim">{t("Primero divide el modelo en piezas")}</p>
      </Show>
      <Show when={props.report}>
        {(r) => (
          <div class="space-y-1 p-2 rounded-md bg-surface/30 border border-border">
            <Row label={t("Uniones puestas")} value={String(r().joints_placed)} tone={r().joints_placed > 0 ? "ok" : "bad"} />
            <For each={r().connections}>
              {(c) => (
                <div class="text-[11px] leading-snug">
                  <span class="text-text">
                    {c.piece_a + 1} ↔ {c.piece_b + 1}: {c.placed}/{c.requested}
                  </span>
                  <For each={c.notes}>{(n) => <div class="text-warning">{tBackend(n)}</div>}</For>
                </div>
              )}
            </For>
          </div>
        )}
      </Show>
    </div>
  );
};

export const MaterialSection: Component<{
  infill: number;
  onInfill: (v: number) => void;
  hollow: boolean;
  onHollow: (v: boolean) => void;
  wall: number;
  onWall: (v: number) => void;
  estimate?: MaterialEstimate;
  busy?: boolean;
  disabled?: boolean;
  onEstimate?: () => void;
}> = (props) => (
  <div class="space-y-2 pt-2 border-t border-border/50">
    <Title>{t("Material")}</Title>
    <NumberInput label={t("Relleno")} value={props.infill * 100} onChange={(v) => props.onInfill(Math.min(1, Math.max(0, v / 100)))} min={0} max={100} step={5} suffix="%" />
    <Checkbox
      small
      label={t("Vaciar por dentro")}
      checked={props.hollow}
      onChange={props.onHollow}
      hint={t("Deja una cáscara y un hueco cerrado adentro: gasta menos material. Las partes más finas que el doble de la pared quedan macizas.")}
    />
    <Show when={props.hollow}>
      <NumberInput label={t("Pared")} value={props.wall} onChange={props.onWall} min={0.8} max={20} step={0.2} suffix="mm" />
    </Show>
    <Button onClick={props.onEstimate} disabled={props.disabled || props.busy} loading={props.busy} fullWidth size="sm">
      {t("Estimar material")}
    </Button>
    <Show when={props.estimate}>
      {(e) => (
        <div class="space-y-1 p-2 rounded-md bg-surface/30 border border-border">
          <Row label={t("Volumen")} value={`${cm3(e().volume_mm3)} cm³`} />
          <Row label={t("Peso")} value={`${fmtNumber(e().mass_g, { maximumFractionDigits: 1 })} g`} />
          <Row label={t("Filamento")} value={`${fmtNumber(e().filament_m, { maximumFractionDigits: 1 })} m`} />
          <Show when={e().hollow}>
            {(h) => (
              <>
                <Row label={t("Vaciado: peso")} value={`${fmtNumber(h().mass_g, { maximumFractionDigits: 1 })} g`} tone="ok" />
                <Row label={t("Ahorro")} value={`${fmtNumber(h().saved_fraction * 100, { maximumFractionDigits: 0 })} %`} tone="ok" />
              </>
            )}
          </Show>
        </div>
      )}
    </Show>
  </div>
);

export const PlatesSection: Component<{
  settings: PlateSettings;
  onSettings: (s: PlateSettings) => void;
  result?: PlateResult;
  busy?: boolean;
  disabled?: boolean;
  hasPieces: boolean;
  onExport?: () => void;
}> = (props) => {
  const set = (patch: Partial<PlateSettings>) => props.onSettings({ ...props.settings, ...patch });
  const [open, setOpen] = createSignal(true);
  return (
    <div class="space-y-2 pt-2 border-t border-border/50">
      <button class="flex w-full items-center justify-between" onClick={() => setOpen(!open())}>
        <Title>{t("Exportar para imprimir")}</Title>
      </button>
      <Show when={open()}>
        <Select
          label={t("Formato")}
          options={[
            { value: "stl", label: "STL" },
            { value: "3mf", label: "3MF" },
            { value: "obj", label: "OBJ" },
          ]}
          value={props.settings.format}
          onChange={(v) => set({ format: v })}
        />
        <NumberInput label={t("Separación")} value={props.settings.spacing} onChange={(v) => set({ spacing: Math.max(0, v) })} min={0} max={50} step={1} suffix="mm" />
        <Checkbox
          small
          label={t("Apoyar cada pieza en su mejor cara")}
          checked={props.settings.orient}
          onChange={(v) => set({ orient: v })}
          hint={t("Gira cada pieza para que tenga la menor cantidad de voladizos antes de acomodarla en la cama.")}
        />
        <Button onClick={props.onExport} disabled={props.disabled || props.busy} loading={props.busy} fullWidth size="sm" variant="primary" icon={<Icons.Download size={14} />}>
          {props.hasPieces ? t("Exportar placas") : t("Exportar para imprimir")}
        </Button>
        <Show when={props.result}>
          {(r) => (
            <div class="space-y-1 p-2 rounded-md bg-surface/30 border border-border">
              <Row label={t("Placas")} value={String(r().plates)} />
              <Row label={t("Piezas")} value={String(r().pieces)} />
              <Row label={t("Cama ocupada")} value={r().utilization.map((u) => `${fmtNumber(u * 100, { maximumFractionDigits: 0 })} %`).join(" · ")} />
              <Show when={r().unplaced.length > 0}>
                <p class="text-[11px] text-red">{t("No caben en la cama: {names}", { names: r().unplaced.join(", ") })}</p>
              </Show>
              <Show when={r().too_tall.length > 0}>
                <p class="text-[11px] text-warning">{t("Más altas que la impresora: {names}", { names: r().too_tall.join(", ") })}</p>
              </Show>
            </div>
          )}
        </Show>
      </Show>
    </div>
  );
};
