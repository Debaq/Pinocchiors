import { Component, Show, For, createSignal } from "solid-js";
import { Button, Select, NumberInput } from "../ui";
import * as Icons from "../icons";

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
  onSubdivide?: (config: SubdivideConfig) => void;
  onExportPiece?: (index: number) => void;
  analysis?: MeshAnalysis;
  subdivideResult?: SubdivideResult;
  canAnalyze?: boolean;
  isProcessing?: boolean;
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

  // Subdivide state
  const [buildVolume, setBuildVolume] = createSignal<[number, number, number]>([220, 220, 250]);
  const [strategy, setStrategy] = createSignal("grid");
  const [margin, setMargin] = createSignal(2.0);

  return (
    <div class="space-y-4">
      {/* Análisis */}
      <div class="space-y-3">
        <h4 class="text-xs font-semibold uppercase tracking-wider text-text-muted">Análisis</h4>

        <Button
          onClick={props.onAnalyze}
          disabled={!props.canAnalyze || props.isProcessing}
          loading={props.isProcessing}
          variant="default"
          fullWidth
          size="sm"
          icon={<Icons.Ruler size={14} />}
        >
          Analizar para impresión
        </Button>
      </div>

      {/* Resultados del análisis */}
      <Show when={props.analysis}>
        <div class="space-y-2 p-3 rounded-md bg-current/30 border border-border">
          <div class="space-y-1.5">
            <div class="flex justify-between text-xs">
              <span class="text-text-muted">Dimensiones</span>
              <span class="text-text font-mono">
                {formatMm(props.analysis!.dimensions[0])} x {formatMm(props.analysis!.dimensions[1])} x {formatMm(props.analysis!.dimensions[2])} mm
              </span>
            </div>
            <div class="flex justify-between text-xs">
              <span class="text-text-muted">Volumen</span>
              <span class="text-text font-mono">{formatCm3(props.analysis!.volume)} cm³</span>
            </div>
            <div class="flex justify-between text-xs">
              <span class="text-text-muted">Área</span>
              <span class="text-text font-mono">{formatCm2(props.analysis!.surface_area)} cm²</span>
            </div>
            <div class="flex justify-between text-xs">
              <span class="text-text-muted">Cerrada</span>
              <span class={props.analysis!.is_closed ? "text-green" : "text-red"}>
                {props.analysis!.is_closed ? "Sí" : "No"}
              </span>
            </div>
            <div class="flex justify-between text-xs">
              <span class="text-text-muted">Triángulos</span>
              <span class="text-text font-mono">{props.analysis!.triangle_count.toLocaleString()}</span>
            </div>
          </div>
        </div>

        {/* Escalar */}
        <div class="space-y-3 pt-2 border-t border-border/50">
          <h4 class="text-xs font-semibold uppercase tracking-wider text-text-muted">Escalar</h4>

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
                Escalar
              </Button>
            </div>

            <p class="text-[10px] text-text-dim">Dimensiones objetivo (mm):</p>
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
              Ajustar a medida
            </Button>
          </div>
        </div>

        {/* Subdividir */}
        <div class="space-y-3 pt-2 border-t border-border/50">
          <h4 class="text-xs font-semibold uppercase tracking-wider text-text-muted">Subdividir</h4>

          <p class="text-[10px] text-text-dim">Volumen de impresión (mm):</p>
          <div class="flex gap-1.5">
            <NumberInput
              label="X"
              value={buildVolume()[0]}
              onChange={(v) => setBuildVolume([v, buildVolume()[1], buildVolume()[2]])}
              min={10}
              step={10}
            />
            <NumberInput
              label="Y"
              value={buildVolume()[1]}
              onChange={(v) => setBuildVolume([buildVolume()[0], v, buildVolume()[2]])}
              min={10}
              step={10}
            />
            <NumberInput
              label="Z"
              value={buildVolume()[2]}
              onChange={(v) => setBuildVolume([buildVolume()[0], buildVolume()[1], v])}
              min={10}
              step={10}
            />
          </div>

          <Select
            label="Estrategia"
            options={[
              { value: "grid", label: "Grid (regular)" },
              { value: "optimal", label: "Optimal (menos cortes)" },
              { value: "zlayers", label: "Capas Z" },
            ]}
            value={strategy()}
            onChange={setStrategy}
          />

          <NumberInput
            label="Margen"
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
                build_volume: buildVolume(),
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
            Subdividir
          </Button>
        </div>
      </Show>

      {/* Resultado de subdivisión */}
      <Show when={props.subdivideResult}>
        <div class="space-y-2 pt-2 border-t border-border/50">
          <h4 class="text-xs font-semibold uppercase tracking-wider text-text-muted">
            Piezas ({props.subdivideResult!.piece_count})
          </h4>

          <div class="space-y-1.5 max-h-48 overflow-y-auto">
            <For each={props.subdivideResult!.pieces}>
              {(piece) => (
                <div class="flex items-center justify-between px-2 py-1.5 rounded bg-current/20 border border-border">
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
    </div>
  );
};
