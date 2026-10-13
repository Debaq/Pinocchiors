import { Component, Show, For, createEffect, createSignal } from "solid-js";
import { Button, Checkbox, Select, NumberInput } from "../ui";
import * as Icons from "../icons";
import { fmtNumber, t, tx } from "../../lib/i18n";
import { CalibrationWizard, type CalibrationApply } from "../layout/CalibrationWizard";
import { SkinPanel, type SkinPanelProps } from "./SkinPanel";
import type { CutPlane } from "../../lib/printCuts.ts";
import { saveCustomMaterial, saveCustomPrinter } from "../../lib/settings.ts";
import {
  JointsSection,
  defaultPrep,
  type PrepSettings,
  MaterialSection,
  OrientationSection,
  PlatesSection,
  PrepareAllSection,
  PrinterMaterial,
  ReadinessSection,
  type FabInfo,
  type JointSettings,
  type JointsReport,
  type MaterialEstimate,
  type MaterialProfile,
  type OrientationOption,
  type PlateResult,
  type PlatePreview,
  type PlateSettings,
  type PrepareReport,
  type PrintPresets,
  type PrinterProfile,
  type Readiness,
} from "./PrintPrepPanel";

export type {
  FabInfo,
  PlatePreview,
  PrepSettings,
  JointSettings,
  JointsReport,
  MaterialEstimate,
  MaterialProfile,
  OrientationOption,
  PlateResult,
  PlateSettings,
  PrepareReport,
  PrepareStep,
  PrintPresets,
  PrinterProfile,
  Readiness,
} from "./PrintPrepPanel";

// ═══════════════════════════════════════════════════════════════════════════
// TYPES
// ═══════════════════════════════════════════════════════════════════════════

export interface MeshAnalysis {
  volume: number;
  surface_area: number;
  center_of_mass: [number, number, number];
  dimensions: [number, number, number];
  is_closed: boolean;
  vertex_count: number;
  triangle_count: number;
  estimated_weight: number | null;
}

export interface SubdivideResult {
  piece_count: number;
  pieces: PieceInfo[];
}

export interface PieceInfo {
  index: number;
  label: string;
  vertex_count: number;
  face_count: number;
  dimensions: [number, number, number];
}

export interface Print3DPanelProps {
  onAnalyze?: () => void;
  onScale?: (params: ScaleParams) => void;
  onUndoScale?: () => void;
  canUndoScale?: boolean;
  onSubdivide?: (config: SubdivideConfig) => void;
  onExportPiece?: (index: number) => void;
  analysis?: MeshAnalysis;
  subdivideResult?: SubdivideResult;
  canAnalyze?: boolean;
  isProcessing?: boolean;
  /** Impresoras y materiales de fábrica */
  presets?: PrintPresets;
  /** Holguras derivadas de la impresora y el material */
  fab?: FabInfo;
  onFabNeeded?: (printer: PrinterProfile, material: MaterialProfile) => void;
  /** Lo elegido (impresora, material, uniones...): si falta, el panel lo guarda él */
  /** Pieles y texturas imprimibles (zona e imagen) */
  skin?: SkinPanelProps;
  /** Ver las piezas separadas y de colores en el visor */
  piecesView?: { on: boolean; explode: number; busy?: boolean; onToggle: () => void; onExplode: (v: number) => void };
  prep?: PrepSettings;
  onPrep?: (s: PrepSettings) => void;
  readiness?: Readiness;
  onCheck?: (printer: PrinterProfile, material: MaterialProfile) => void;
  orientations?: OrientationOption[];
  onSuggestOrientation?: (printer: PrinterProfile, material: MaterialProfile) => void;
  onApplyOrientation?: (down: [number, number, number]) => void;
  jointsReport?: JointsReport;
  onAddJoints?: (settings: JointSettings, printer: PrinterProfile, material: MaterialProfile) => void;
  materialEstimate?: MaterialEstimate;
  onEstimate?: (material: MaterialProfile, infill: number, hollowWall: number | null) => void;
  plateResult?: PlateResult;
  platePreview?: PlatePreview;
  /** Placa vista en 3D en el visor */
  plateView?: { plate: number | null; busy?: boolean; onView: (plate: number | null) => void };
  onPreviewPlates?: (printer: PrinterProfile, material: MaterialProfile, settings: PlateSettings) => void;
  onExportPlates?: (printer: PrinterProfile, material: MaterialProfile, settings: PlateSettings) => void;
  /** Todo de una vez: verificar, dividir, uniones, vaciar y exportar */
  prepareReport?: PrepareReport;
  onPrepareAll?: (args: {
    printer: PrinterProfile;
    material: MaterialProfile;
    subdivide: SubdivideConfig;
    joint: JointSettings | null;
    plate: PlateSettings;
  }) => void;
}

export interface ScaleParams {
  mode: string;
  factor?: number;
  /** Con el modo `axes`: factor por eje (X, Y, Z con Z arriba) */
  factors?: [number, number, number];
  target_size?: [number, number, number];
  target_volume?: number;
}

export interface SubdivideConfig {
  build_volume: [number, number, number];
  strategy?: string;
  margin?: number;
  /** Con la estrategia manual: los planos elegidos */
  planes?: CutPlane[];
}

// ═══════════════════════════════════════════════════════════════════════════
// HELPERS
// ═══════════════════════════════════════════════════════════════════════════

function formatMm(value: number): string {
  return value.toFixed(1);
}

function formatCm3(value: number): string {
  // mm³ -> cm³
  return (value / 1000).toFixed(2);
}

function formatCm2(value: number): string {
  // mm² -> cm²
  return (value / 100).toFixed(2);
}

// ═══════════════════════════════════════════════════════════════════════════
// COMPONENT
// ═══════════════════════════════════════════════════════════════════════════

export const Print3DPanel: Component<Print3DPanelProps> = (props) => {
  // Scale state
  const [scaleFactor, setScaleFactor] = createSignal(1.0);
  const [targetSize, setTargetSize] = createSignal<[number, number, number]>([100, 100, 100]);
  // Candado de proporción: con él, cambiar una medida cambia las tres con el mismo factor
  const [lockRatio, setLockRatio] = createSignal(true);
  const currentDims = () => props.analysis?.dimensions;
  // La medida final parte de la actual y se rehace cuando el modelo cambia (se escala, se deshace...)
  createEffect(() => {
    const d = currentDims();
    if (d) setTargetSize([d[0], d[1], d[2]]);
  });
  const setAxis = (axis: number, value: number) => {
    const d = currentDims();
    if (!d || !(value > 0)) return;
    if (lockRatio()) {
      const f = value / d[axis];
      setTargetSize([d[0] * f, d[1] * f, d[2] * f]);
    } else {
      const next = [...targetSize()] as [number, number, number];
      next[axis] = value;
      setTargetSize(next);
    }
  };
  /** Cuánto hay que escalar cada eje para llegar a la medida final */
  const targetFactors = (): [number, number, number] | undefined => {
    const d = currentDims();
    if (!d || d.some((x) => !(x > 1e-9))) return undefined;
    const t3 = targetSize();
    return [t3[0] / d[0], t3[1] / d[1], t3[2] / d[2]];
  };

  // Impresora y material, y lo demás de la preparación (viven en el proyecto)
  const [localPrep, setLocalPrep] = createSignal<PrepSettings>(defaultPrep);
  const prep = () => props.prep ?? localPrep();
  const patch = (p: Partial<PrepSettings>) => {
    const next = { ...prep(), ...p };
    setLocalPrep(next);
    props.onPrep?.(next);
  };
  const printer = (): PrinterProfile =>
    prep().printer ?? props.presets?.printers[0] ?? { name: "Creality Ender 3", build_volume: [220, 220, 250], nozzle: 0.4, layer_height: 0.2 };
  const material = (): MaterialProfile =>
    prep().material ?? props.presets?.materials[0] ?? { name: "PLA", density: 1.24, max_overhang_deg: 50 };
  const setPrinterChoice = (printer: PrinterProfile) => patch({ printer });
  const setMaterialChoice = (material: MaterialProfile) => patch({ material });
  createEffect(() => props.onFabNeeded?.(printer(), material()));
  const joint = () => prep().joint;
  const setJoint = (joint: JointSettings) => patch({ joint });
  const infill = () => prep().infill;
  const setInfill = (infill: number) => patch({ infill });
  const hollow = () => prep().hollow;
  const setHollow = (hollow: boolean) => patch({ hollow });
  const wall = () => prep().wall;
  const setWall = (wall: number) => patch({ wall });
  const drain = () => prep().drain;
  const setDrain = (drain: boolean) => patch({ drain });
  const vent = () => prep().vent ?? false;
  const setVent = (vent: boolean) => patch({ vent });
  const drainDiameter = () => prep().drainDiameter;
  const setDrainDiameter = (drainDiameter: number) => patch({ drainDiameter });
  const plate = () => prep().plate;
  const setPlate = (plate: PlateSettings) => patch({ plate });

  // Asistente de calibración: lo que calcula se aplica a la impresora y al material elegidos
  const [calibrating, setCalibrating] = createSignal(false);
  const applyCalibration = (r: CalibrationApply) => {
    if (r.savePrinterAs) saveCustomPrinter(r.printer);
    if (r.saveMaterialAs) saveCustomMaterial(r.material);
    patch({ printer: r.printer, material: r.material });
  };

  // Subdivide state
  const strategy = () => prep().cutStrategy ?? "grid";
  const setStrategy = (cutStrategy: string) => patch({ cutStrategy });
  const cuts = () => prep().cuts ?? [];
  const setCuts = (next: CutPlane[]) => patch({ cuts: next });
  const modelSize = () => props.analysis?.dimensions ?? [100, 100, 100];
  const [margin, setMargin] = createSignal(2.0);

  return (
    <div class="space-y-4">
      <Show when={calibrating()}>
        <CalibrationWizard printer={printer()} material={material()} onApply={applyCalibration} onClose={() => setCalibrating(false)} />
      </Show>
      {/* Análisis */}
      <div class="space-y-3">
        <h4 class="text-xs font-semibold uppercase tracking-wider text-text-muted">{t("Análisis")}</h4>

        <Button
          onClick={props.onAnalyze}
          disabled={!props.canAnalyze || props.isProcessing}
          loading={props.isProcessing}
          variant="default"
          fullWidth
          size="sm"
          icon={<Icons.Ruler size={14} />}
        >
          {t("Analizar para impresión")}
        </Button>
      </div>

      {/* Resultados del análisis */}
      <Show when={props.analysis}>
        <div class="space-y-2 p-3 rounded-md bg-surface/30 border border-border">
          <div class="space-y-1.5">
            <div class="flex justify-between text-xs">
              <span class="text-text-muted">{t("Dimensiones")}</span>
              <span class="text-text font-mono">
                {formatMm(props.analysis!.dimensions[0])} x {formatMm(props.analysis!.dimensions[1])} x {formatMm(props.analysis!.dimensions[2])} mm
              </span>
            </div>
            <div class="flex justify-between text-xs">
              <span class="text-text-muted">{t("Volumen")}</span>
              <span class="text-text font-mono">{formatCm3(props.analysis!.volume)} cm³</span>
            </div>
            <div class="flex justify-between text-xs">
              <span class="text-text-muted">{t("Área")}</span>
              <span class="text-text font-mono">{formatCm2(props.analysis!.surface_area)} cm²</span>
            </div>
            <div class="flex justify-between text-xs">
              <span class="text-text-muted">{t("Cerrada")}</span>
              <span class={props.analysis!.is_closed ? "text-green" : "text-red"}>
                {props.analysis!.is_closed ? t("Sí") : t("No")}
              </span>
            </div>
            <div class="flex justify-between text-xs">
              <span class="text-text-muted">{t("Triángulos")}</span>
              <span class="text-text font-mono">{fmtNumber(props.analysis!.triangle_count)}</span>
            </div>
          </div>
        </div>

        {/* Impresora, verificación y orientación */}
        <div class="space-y-3 pt-2 border-t border-border/50">
          <PrinterMaterial presets={props.presets} printer={printer()} material={material()} onPrinter={setPrinterChoice} onMaterial={setMaterialChoice} onCalibrate={() => setCalibrating(true)} />
          <Checkbox
            small
            label={t("Compensar los huecos del diseño")}
            checked={!!prep().compensateAll}
            onChange={(compensateAll) => patch({ compensateAll })}
            hint={t("En Diseñar, los agujeros, las roscas interiores, las tuercas, las arandelas y los ejes y chaveteros de los engranes salen ya más anchos lo que esta impresora los achica. Cambia la medida de lo que ves.")}
          />
          <ReadinessSection
            readiness={props.readiness}
            busy={props.isProcessing}
            disabled={!props.canAnalyze}
            onCheck={() => props.onCheck?.(printer(), material())}
          />
          <OrientationSection
            options={props.orientations}
            busy={props.isProcessing}
            disabled={!props.canAnalyze}
            onSuggest={() => props.onSuggestOrientation?.(printer(), material())}
            onApply={props.onApplyOrientation}
          />
        </div>

        {/* Escalar */}
        <div class="space-y-3 pt-2 border-t border-border/50">
          <h4 class="text-xs font-semibold uppercase tracking-wider text-text-muted">{t("Escalar")}</h4>

          <div class="space-y-2">
            <div class="flex gap-2 items-end">
              <NumberInput
                label="×"
                value={scaleFactor()}
                onChange={setScaleFactor}
                min={0.01}
                max={100}
                step={0.1}
              />
              <Button
                onClick={() => props.onScale?.({ mode: "uniform", factor: scaleFactor() })}
                disabled={props.isProcessing}
                variant="default"
                size="sm"
              >
                {t("Escalar")}
              </Button>
            </div>

            <div class="flex items-center justify-between">
              <p class="text-[10px] text-text-dim">{t("Medida final (mm):")}</p>
              <button
                class={`flex items-center gap-1 px-1.5 h-5 rounded text-[10px] border ${lockRatio() ? "border-accent text-accent bg-accent/10" : "border-border text-text-muted hover:text-text"}`}
                onClick={() => setLockRatio(!lockRatio())}
                title={lockRatio() ? t("Las tres medidas cambian juntas, con la misma proporción") : t("Cada medida cambia sola: el modelo se estira")}
                aria-pressed={lockRatio()}
              >
                <Icons.Lock size={10} />
                {lockRatio() ? t("Proporción fija") : t("Proporción libre")}
              </button>
            </div>
            <div class="flex gap-1.5">
              <For each={["X", "Y", "Z"]}>
                {(axis, i) => (
                  <NumberInput label={axis} value={Number(targetSize()[i()].toFixed(2))} onChange={(v) => setAxis(i(), v)} min={0.1} step={1} />
                )}
              </For>
            </div>
            <Show when={targetFactors()}>
              {(f) => (
                <p class="text-[10px] text-text-dim">
                  {lockRatio() ? t("Escala ×{f}", { f: fmtNumber(f()[0], { maximumFractionDigits: 3 }) }) : t("Escala ×{x} · ×{y} · ×{z}", { x: fmtNumber(f()[0], { maximumFractionDigits: 3 }), y: fmtNumber(f()[1], { maximumFractionDigits: 3 }), z: fmtNumber(f()[2], { maximumFractionDigits: 3 }) })}
                </p>
              )}
            </Show>
            <Button
              onClick={() => {
                const f = targetFactors();
                if (!f) return;
                props.onScale?.(lockRatio() ? { mode: "uniform", factor: f[0] } : { mode: "axes", factors: f });
              }}
              disabled={props.isProcessing || !targetFactors() || targetFactors()!.every((x) => Math.abs(x - 1) < 1e-6)}
              variant="default"
              size="sm"
              fullWidth
            >
              {t("Escalar a esa medida")}
            </Button>
            <Button
              onClick={() => {
                const bv = printer().build_volume;
                const m = margin();
                props.onScale?.({ mode: "fit", target_size: [bv[0] - 2 * m, bv[1] - 2 * m, bv[2] - 2 * m] });
              }}
              disabled={props.isProcessing}
              variant="ghost"
              size="sm"
              fullWidth
            >
              {t("Reducir para que quepa en la cama")}
            </Button>
            <Button
              onClick={() => props.onUndoScale?.()}
              disabled={props.isProcessing || !props.canUndoScale}
              variant="ghost"
              size="sm"
              fullWidth
            >
              {t("Deshacer escala")}
            </Button>
          </div>
        </div>

        {/* Subdividir */}
        <div class="space-y-3 pt-2 border-t border-border/50">
          <h4 class="text-xs font-semibold uppercase tracking-wider text-text-muted">{t("Subdividir")}</h4>

          <Select
            label={t("Estrategia")}
            options={[
              { value: "grid", label: t("Grid (regular)") },
              { value: "optimal", label: t("Optimal (menos cortes)") },
              { value: "zlayers", label: t("Capas Z") },
              { value: "manual", label: t("A mano (tus planos)") },
            ]}
            value={strategy()}
            onChange={setStrategy}
          />
          <Show when={strategy() === "optimal"}>
            <p class="text-[10px] text-text-dim leading-snug">{t("Reparte el modelo con el menor número de piezas (pueden girarse para caber) y corta por donde es más angosto.")}</p>
          </Show>
          <Show when={strategy() === "manual"}>
            <div class="space-y-2">
              <p class="text-[10px] text-text-dim leading-snug">{t("Cada plano corta perpendicular a un eje, a la distancia que pongas desde el borde del modelo. Se ven en el visor.")}</p>
              <For each={cuts()}>
                {(plane, i) => (
                  <div class="grid grid-cols-[3.2rem_1fr_1fr_auto] gap-1 items-end p-1.5 rounded bg-surface/20 border border-border">
                    <Select
                      options={[{ value: "0", label: "X" }, { value: "1", label: "Y" }, { value: "2", label: "Z" }]}
                      value={String(plane.axis)}
                      onChange={(v) => {
                        const axis = Number(v) as 0 | 1 | 2;
                        setCuts(cuts().map((c, k) => (k === i() ? { ...c, axis, position: Math.min(c.position, modelSize()[axis] * 0.95) } : c)));
                      }}
                    />
                    <NumberInput
                      label={tx("corte", "A")}
                      value={Number(plane.position.toFixed(2))}
                      onChange={(v) => setCuts(cuts().map((c, k) => (k === i() ? { ...c, position: Math.max(0.1, Math.min(modelSize()[c.axis] - 0.1, v)) } : c)))}
                      min={0.1}
                      max={modelSize()[plane.axis]}
                      step={1}
                      suffix="mm"
                    />
                    <NumberInput
                      label={t("Giro")}
                      value={plane.tilt_deg}
                      onChange={(v) => setCuts(cuts().map((c, k) => (k === i() ? { ...c, tilt_deg: Math.max(-80, Math.min(80, v)) } : c)))}
                      min={-80}
                      max={80}
                      step={5}
                      suffix="°"
                    />
                    <button
                      class="h-7 w-7 flex items-center justify-center rounded text-text-muted hover:text-red hover:bg-surface/40"
                      aria-label={t("Quitar plano")}
                      onClick={() => setCuts(cuts().filter((_, k) => k !== i()))}
                    >
                      <Icons.X size={12} />
                    </button>
                  </div>
                )}
              </For>
              <Button
                size="sm"
                variant="ghost"
                fullWidth
                onClick={() => setCuts([...cuts(), { axis: 2, position: Number((modelSize()[2] / 2).toFixed(1)), tilt_deg: 0 }])}
              >
                {t("Agregar plano")}
              </Button>
            </div>
          </Show>

          <NumberInput
            label={t("Margen")}
            value={margin()}
            onChange={setMargin}
            min={0}
            max={20}
            step={0.5}
            suffix="mm"
          />

          <Button
            onClick={() =>
              props.onSubdivide?.({
                build_volume: printer().build_volume,
                strategy: strategy(),
                margin: margin(),
                planes: strategy() === "manual" ? cuts() : undefined,
              })
            }
            disabled={!props.canAnalyze || props.isProcessing || (strategy() === "manual" && cuts().length === 0)}
            loading={props.isProcessing}
            variant="primary"
            fullWidth
            size="sm"
            icon={<Icons.Scissors size={14} />}
          >
            {t("Subdividir")}
          </Button>
        </div>
      </Show>

      {/* Resultado de subdivisión */}
      <Show when={props.subdivideResult}>
        <div class="space-y-2 pt-2 border-t border-border/50">
          <h4 class="text-xs font-semibold uppercase tracking-wider text-text-muted">
            {t("Piezas")} ({props.subdivideResult!.piece_count})
          </h4>

          <Show when={props.piecesView}>
            {(view) => (
              <div class="space-y-1.5">
                <Button size="sm" fullWidth variant={view().on ? "primary" : "default"} loading={view().busy} onClick={view().onToggle} icon={<Icons.Cube size={14} />}>
                  {view().on ? t("Volver al modelo entero") : t("Ver las piezas separadas")}
                </Button>
                <Show when={view().on}>
                  <NumberInput label={t("Separación")} value={view().explode} onChange={(v) => view().onExplode(Math.max(0, Math.min(2, v)))} min={0} max={2} step={0.1} suffix="×" />
                </Show>
              </div>
            )}
          </Show>
          <div class="space-y-1.5 max-h-48 overflow-y-auto">
            <For each={props.subdivideResult!.pieces}>
              {(piece) => (
                <div class="flex items-center justify-between px-2 py-1.5 rounded bg-surface/20 border border-border">
                  <div>
                    <span class="text-xs font-mono font-semibold text-text">{piece.label}</span>
                    <span class="text-[10px] text-text-dim ml-2">
                      {piece.vertex_count}v / {piece.face_count}f
                    </span>
                  </div>
                  <Button
                    onClick={() => props.onExportPiece?.(piece.index)}
                    variant="ghost"
                    size="sm"
                    icon={<Icons.Download size={12} />}
                  >
                    STL
                  </Button>
                </div>
              )}
            </For>
          </div>
        </div>
      </Show>

      <Show when={props.analysis}>
        <Show when={props.subdivideResult}>
          <JointsSection
            fab={props.fab}
            settings={joint()}
            onSettings={setJoint}
            report={props.jointsReport}
            busy={props.isProcessing}
            hasPieces={!!props.subdivideResult}
            onAdd={() => props.onAddJoints?.(joint(), printer(), material())}
          />
        </Show>
        <Show when={props.skin}>{(skin) => <SkinPanel {...skin()} busy={props.isProcessing} />}</Show>
        <MaterialSection
          infill={infill()}
          onInfill={setInfill}
          hollow={hollow()}
          onHollow={setHollow}
          wall={wall()}
          onWall={setWall}
          drain={drain()}
          onDrain={setDrain}
          drainDiameter={drainDiameter()}
          onDrainDiameter={setDrainDiameter}
          vent={vent()}
          onVent={setVent}
          estimate={props.materialEstimate}
          busy={props.isProcessing}
          disabled={!props.canAnalyze}
          onEstimate={() => props.onEstimate?.(material(), infill(), hollow() ? wall() : null)}
        />
        <PlatesSection
          settings={plate()}
          onSettings={setPlate}
          result={props.plateResult}
          preview={props.platePreview}
          viewing={props.plateView?.plate}
          viewBusy={props.plateView?.busy}
          onView={props.plateView?.onView}
          onPreview={() => props.onPreviewPlates?.(printer(), material(), plate())}
          busy={props.isProcessing}
          disabled={!props.canAnalyze}
          hasPieces={!!props.subdivideResult}
          onExport={() => props.onExportPlates?.(printer(), material(), { ...plate(), hollowWall: hollow() ? wall() : null, drain: hollow() && drain() ? drainDiameter() : null, vent: hollow() && drain() && vent() })}
        />
        <PrepareAllSection
          withJoints={prep().prepareJoints ?? true}
          onWithJoints={(prepareJoints) => patch({ prepareJoints })}
          report={props.prepareReport}
          busy={props.isProcessing}
          disabled={!props.canAnalyze}
          onPrepare={() =>
            props.onPrepareAll?.({
              printer: printer(),
              material: material(),
              subdivide: { build_volume: printer().build_volume, strategy: strategy(), margin: margin(), planes: strategy() === "manual" ? cuts() : undefined },
              joint: (prep().prepareJoints ?? true) ? joint() : null,
              plate: { ...plate(), hollowWall: hollow() ? wall() : null, drain: hollow() && drain() ? drainDiameter() : null, vent: hollow() && drain() && vent() },
            })
          }
        />
      </Show>
    </div>
  );
};
