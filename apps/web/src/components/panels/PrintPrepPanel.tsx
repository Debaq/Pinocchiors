import { Component, For, Show, createSignal } from "solid-js";
import { Button, Checkbox, NumberInput, Select } from "../ui";
import * as Icons from "../icons";
import { fmtNumber, t, tx } from "../../lib/i18n";
import { tBackend } from "../../lib/i18nBackend.ts";
import { customPrinters, removeCustomPrinter, saveCustomPrinter } from "../../lib/settings.ts";

// ═══════════════════════════════════════════════════════════════════════════
// TIPOS (lo que devuelve el backend, en milímetros con Z arriba)
// ═══════════════════════════════════════════════════════════════════════════

export interface PrinterProfile {
  name: string;
  build_volume: [number, number, number];
  nozzle: number;
  layer_height: number;
  /** Holgura de base de un ajuste (mm); sigue a la boquilla si falta */
  fit_clearance?: number;
  /** Cuánto más chico sale un agujero, por lado (mm) */
  hole_comp?: number;
}

export interface MaterialProfile {
  name: string;
  density: number;
  max_overhang_deg: number;
  /** Contracción lineal por eje (fracción: 0,004 = 0,4 %) */
  shrink?: [number, number, number];
  /** Multiplica la holgura de base */
  clearance_factor?: number;
}

export type FitKind = "tight" | "normal" | "loose";

/** Holguras que salen de la impresora y el material (backend) */
export interface FabInfo {
  clearance: number;
  hole_comp: number;
  shrink: [number, number, number];
  min_wall: number;
  tight: number;
  normal: number;
  loose: number;
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
  support_volume: number;
  bed_contact_area: number;
  min_thickness: number | null;
  thin_fraction: number;
  /** Puntos de pared fina, en la malla (se marcan en el visor) */
  thin_points: [number, number, number][];
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
  support_volume: number;
  score: number;
}

export interface JointsReport {
  joints_placed: number;
  connections: Array<{ piece_a: number; piece_b: number; contact_area: number; requested: number; placed: number; notes: string[] }>;
}

export type JointShape = "round" | "square" | "dovetail" | "bayonet" | "rail";

export interface JointSettings {
  shape: JointShape;
  size: number;
  depth: number;
  tolerance: number;
  count: number;
  /** Ajuste: la holgura sale de la impresora y el material (si no, vale `tolerance`) */
  fit?: FitKind | "manual";
  /** Giro de la unión dentro de la cara (grados): hacia dónde corre el riel */
  angle: number;
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
  /** Agrandar por eje lo que se encoge el material */
  compensate: boolean;
  /** Acomodar por el contorno de cada pieza y no por su caja */
  contour: boolean;
  hollowWall: number | null;
  /** Diámetro del agujero de drenaje (mm) al vaciar; null: sin agujero */
  drain: number | null;
}

export interface PlateResult {
  files: string[];
  plates: number;
  pieces: number;
  utilization: number[];
  unplaced: string[];
  too_tall: string[];
  total_bytes: number;
  /** Factor por eje aplicado por la contracción (1 = sin compensar) */
  shrink_scale?: [number, number, number];
}

/** Todo lo que se elige en la preparación (va al proyecto) */
export interface PrepSettings {
  printer?: PrinterProfile;
  material?: MaterialProfile;
  joint: JointSettings;
  infill: number;
  hollow: boolean;
  wall: number;
  /** Agujero de drenaje al vaciar (resina) y su diámetro */
  drain: boolean;
  drainDiameter: number;
  /** El diseño (Diseñar) sale con todo lo hueco compensado para la impresora */
  compensateAll?: boolean;
  plate: PlateSettings;
}

export const defaultPrep: PrepSettings = {
  joint: { shape: "round", size: 6, depth: 10, tolerance: -0.4, count: 2, angle: 0, fit: "normal" },
  infill: 1,
  hollow: false,
  wall: 2,
  drain: false,
  drainDiameter: 4,
  plate: { format: "stl", spacing: 5, orient: true, compensate: true, contour: true, hollowWall: null, drain: null },
};

export interface PlatePreview {
  bed: [number, number];
  plates: Array<Array<{ label: string; x: number; y: number; w: number; h: number; height: number }>>;
  utilization: number[];
  unplaced: string[];
  too_tall: string[];
}

/** La forma del backend (`PlanarShape`) a partir de lo que se elige en el panel */
export function jointShapeConfig(s: JointSettings) {
  switch (s.shape) {
    case "square":
      return { kind: "square", size: s.size };
    case "rail":
      return { kind: "rail", width: s.size };
    case "bayonet":
      return { kind: "bayonet", length: s.size * 2.3, width: s.size };
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
  const printers = () => [...(props.presets?.printers ?? []), ...customPrinters()];
  const isSaved = () => customPrinters().some((p) => p.name === props.printer.name);
  const [newName, setNewName] = createSignal("");
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
      <div class="flex gap-1.5 items-center">
        <input
          class="min-w-0 flex-1 px-2 py-1 rounded bg-surface/40 border border-border text-xs text-text outline-none focus:border-accent"
          placeholder={t("Nombre para guardarla")}
          value={newName()}
          onInput={(e) => setNewName(e.currentTarget.value)}
        />
        <Button
          size="sm"
          variant="ghost"
          disabled={!newName().trim()}
          onClick={() => {
            const name = newName().trim();
            saveCustomPrinter({ ...props.printer, name });
            props.onPrinter({ ...props.printer, name });
            setNewName("");
          }}
        >
          {t("Guardar")}
        </Button>
        <Show when={isSaved()}>
          <Button
            size="sm"
            variant="ghost"
            onClick={() => {
              removeCustomPrinter(props.printer.name);
              props.onPrinter(props.presets?.printers[0] ?? props.printer);
            }}
          >
            {t("Quitar")}
          </Button>
        </Show>
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
      <div class="grid grid-cols-2 gap-1.5">
        <NumberInput
          label={t("Holgura")}
          value={props.printer.fit_clearance ?? props.printer.nozzle}
          onChange={(v) => props.onPrinter({ ...props.printer, fit_clearance: Math.max(0, v) })}
          min={0}
          max={2}
          step={0.05}
          suffix="mm"
        />
        <NumberInput
          label={t("Agujeros")}
          value={props.printer.hole_comp ?? props.printer.nozzle * 0.25}
          onChange={(v) => props.onPrinter({ ...props.printer, hole_comp: Math.max(0, v) })}
          min={0}
          max={1}
          step={0.05}
          suffix="mm"
        />
      </div>
      <Select
        label={t("Material")}
        options={materials().map((m) => ({ value: m.name, label: m.name }))}
        value={props.material.name}
        onChange={(name) => {
          const m = materials().find((x) => x.name === name);
          if (m) props.onMaterial(m);
        }}
      />
      <NumberInput
        label={t("Contracción")}
        value={Number(((props.material.shrink?.[0] ?? 0) * 100).toFixed(2))}
        onChange={(v) => props.onMaterial({ ...props.material, shrink: [v / 100, v / 100, v / 100] })}
        min={0}
        max={5}
        step={0.1}
        suffix="%"
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
          <Show when={r().support_volume > 0}>
            <Row label={t("Soporte (estimado)")} value={`${cm3(r().support_volume)} cm³`} />
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
                  {t("soporte")} {cm3(o.support_volume)} cm³ · {t("voladizo")} {fmtNumber(o.overhang_area / 100, { maximumFractionDigits: 1 })} cm² · {t("base")} {fmtNumber(o.contact_area / 100, { maximumFractionDigits: 1 })} cm²
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
  fab?: FabInfo;
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
          { value: "bayonet", label: t("Cierre de cuarto de vuelta") },
          { value: "rail", label: t("Cola de milano deslizante") },
        ]}
        value={props.settings.shape}
        onChange={(v) => set({ shape: v as JointShape })}
      />
      <div class="grid grid-cols-2 gap-1.5">
        <NumberInput label={t("Tamaño")} value={props.settings.size} onChange={(v) => set({ size: v })} min={2} max={40} step={0.5} suffix="mm" />
        <NumberInput label={t("Hondo")} value={props.settings.depth} onChange={(v) => set({ depth: v })} min={3} max={60} step={1} suffix="mm" />
        <Select
          label={t("Ajuste")}
          options={[
            { value: "tight", label: t("Justo") },
            { value: "normal", label: t("Normal") },
            { value: "loose", label: tx("ajuste", "Libre") },
            { value: "manual", label: t("A mano") },
          ]}
          value={props.settings.fit ?? "normal"}
          onChange={(v) => set({ fit: v as FitKind | "manual" })}
        />
        <Show when={(props.settings.fit ?? "normal") === "manual"}>
          <NumberInput label={t("Holgura")} value={props.settings.tolerance} onChange={(v) => set({ tolerance: v })} min={-1.5} max={0} step={0.05} suffix="mm" />
        </Show>
        <Show when={(props.settings.fit ?? "normal") !== "manual" && props.fab}>
          {(f) => (
            <p class="col-span-2 text-[10px] text-text-dim">
              {t("Holgura de {v} mm según la impresora y el material", { v: fmtNumber(f()[(props.settings.fit ?? "normal") as FitKind], { maximumFractionDigits: 2 }) })}
            </p>
          )}
        </Show>
        <NumberInput label={t("Por cara")} value={props.settings.count} onChange={(v) => set({ count: Math.max(1, Math.round(v)) })} min={1} max={8} step={1} />
        <NumberInput label={t("Giro")} value={props.settings.angle ?? 0} onChange={(v) => set({ angle: v })} min={-180} max={180} step={5} suffix="°" />
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
  drain: boolean;
  onDrain: (v: boolean) => void;
  drainDiameter: number;
  onDrainDiameter: (v: number) => void;
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
      <Checkbox
        small
        label={t("Agujero de drenaje")}
        checked={props.drain}
        onChange={props.onDrain}
        hint={t("Abre un agujero en la cara plana de abajo para que salga la resina del hueco. Pide una cara plana apoyada en la cama.")}
      />
      <Show when={props.drain}>
        <NumberInput label={t("Diámetro")} value={props.drainDiameter} onChange={props.onDrainDiameter} min={2} max={15} step={0.5} suffix="mm" />
      </Show>
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
  preview?: PlatePreview;
  busy?: boolean;
  disabled?: boolean;
  hasPieces: boolean;
  onExport?: () => void;
  onPreview?: () => void;
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
        <Checkbox
          small
          label={t("Compensar la contracción del material")}
          checked={props.settings.compensate !== false}
          onChange={(v) => set({ compensate: v })}
          hint={t("Agranda cada pieza lo que se va a encoger al enfriar, para que salga a la medida diseñada. El diseño no cambia: solo el archivo que exportas.")}
        />
        <Checkbox
          small
          label={t("Acomodar por el contorno")}
          checked={props.settings.contour !== false}
          onChange={(v) => set({ contour: v })}
          hint={t("Usa la forma de cada pieza vista desde arriba y no su caja: caben más piezas por cama, pero tarda un poco más.")}
        />
        <Button onClick={props.onPreview} disabled={props.disabled || props.busy} fullWidth size="sm" icon={<Icons.Cube size={14} />}>
          {t("Ver cómo quedan en la cama")}
        </Button>
        <Show when={props.preview}>{(p) => <PlatePreviewView preview={p()} />}</Show>
        <Button onClick={props.onExport} disabled={props.disabled || props.busy} loading={props.busy} fullWidth size="sm" variant="primary" icon={<Icons.Download size={14} />}>
          {props.hasPieces ? t("Exportar placas") : t("Exportar para imprimir")}
        </Button>
        <Show when={props.result}>
          {(r) => (
            <div class="space-y-1 p-2 rounded-md bg-surface/30 border border-border">
              <Row label={t("Placas")} value={String(r().plates)} />
              <Show when={r().shrink_scale && r().shrink_scale!.some((x) => Math.abs(x - 1) > 1e-9)}>
                <Row label={t("Compensación")} value={r().shrink_scale!.map((x) => `×${fmtNumber(x, { maximumFractionDigits: 4 })}`).join(" ")} />
              </Show>
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

/** Las camas vistas desde arriba, con cada pieza como un rectángulo */
export const PlatePreviewView: Component<{ preview: PlatePreview }> = (props) => {
  const [w, h] = props.preview.bed;
  return (
    <div class="space-y-2">
      <For each={props.preview.plates}>
        {(pieces, i) => (
          <div class="space-y-0.5">
            <p class="text-[10px] text-text-dim">
              {t("Placa {n}", { n: i() + 1 })} · {fmtNumber((props.preview.utilization[i()] ?? 0) * 100, { maximumFractionDigits: 0 })} %
            </p>
            <svg viewBox={`0 0 ${w} ${h}`} class="w-full rounded border border-border bg-surface/30" style={{ "aspect-ratio": `${w} / ${h}` }}>
              <For each={pieces}>
                {(p) => (
                  <g>
                    <rect
                      x={p.x}
                      y={h - p.y - p.h}
                      width={p.w}
                      height={p.h}
                      fill="var(--accent, #6cf)"
                      fill-opacity="0.35"
                      stroke="var(--accent, #6cf)"
                      stroke-width={Math.max(w, h) / 300}
                    />
                    <text x={p.x + p.w / 2} y={h - p.y - p.h / 2} font-size={String(Math.max(Math.min(p.w, p.h) / 3, w / 40))} text-anchor="middle" dominant-baseline="middle" fill="currentColor">
                      {p.label}
                    </text>
                  </g>
                )}
              </For>
            </svg>
          </div>
        )}
      </For>
      <Show when={props.preview.unplaced.length > 0}>
        <p class="text-[11px] text-red">{t("No caben en la cama: {names}", { names: props.preview.unplaced.join(", ") })}</p>
      </Show>
      <Show when={props.preview.too_tall.length > 0}>
        <p class="text-[11px] text-warning">{t("Más altas que la impresora: {names}", { names: props.preview.too_tall.join(", ") })}</p>
      </Show>
    </div>
  );
};
