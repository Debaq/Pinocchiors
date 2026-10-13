import { Component, Show, For, createEffect, createSignal } from "solid-js";
import { Button, Checkbox, Select, NumberInput } from "../ui";
import * as Icons from "../icons";
import { fmtNumber, t } from "../../lib/i18n";
import { CalibrationWizard, type CalibrationApply } from "../layout/CalibrationWizard";
import { saveCustomMaterial, saveCustomPrinter } from "../../lib/settings.ts";
import {
  JointsSection,
  defaultPrep,
  type PrepSettings,
  MaterialSection,
  OrientationSection,
  PlatesSection,
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
  onPreviewPlates?: (printer: PrinterProfile, material: MaterialProfile, settings: PlateSettings) => void;
  onExportPlates?: (printer: PrinterProfile, material: MaterialProfile, settings: PlateSettings) => void;
}

export interface ScaleParams {
  mode: string;
  factor?: number;
  target_size?: [number, number, number];
  target_volume?: number;
}

export interface SubdivideConfig {
  build_volume: [number, number, number];
  strategy?: string;
  margin?: number;
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
  const [strategy, setStrategy] = createSignal("grid");
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

            <p class="text-[10px] text-text-dim">{t("Dimensiones objetivo (mm):")}</p>
            <div class="flex gap-1.5">
              <NumberInput
                label="X"
                value={targetSize()[0]}
                onChange={(v) => setTargetSize([v, targetSize()[1], targetSize()[2]])}
                min={1}
                step={1}
              />
              <NumberInput
                label="Y"
                value={targetSize()[1]}
                onChange={(v) => setTargetSize([targetSize()[0], v, targetSize()[2]])}
                min={1}
                step={1}
              />
              <NumberInput
                label="Z"
                value={targetSize()[2]}
                onChange={(v) => setTargetSize([targetSize()[0], targetSize()[1], v])}
                min={1}
                step={1}
              />
            </div>
            <Button
              onClick={() => props.onScale?.({ mode: "fit", target_size: targetSize() })}
              disabled={props.isProcessing}
              variant="default"
              size="sm"
              fullWidth
            >
              {t("Ajustar a medida")}
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
            ]}
            value={strategy()}
            onChange={setStrategy}
          />

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
              })
            }
            disabled={!props.canAnalyze || props.isProcessing}
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
          onPreview={() => props.onPreviewPlates?.(printer(), material(), plate())}
          busy={props.isProcessing}
          disabled={!props.canAnalyze}
          hasPieces={!!props.subdivideResult}
          onExport={() => props.onExportPlates?.(printer(), material(), { ...plate(), hollowWall: hollow() ? wall() : null, drain: hollow() && drain() ? drainDiameter() : null })}
        />
      </Show>
    </div>
  );
};
