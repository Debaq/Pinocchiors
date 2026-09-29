import { Component, Show } from "solid-js";
import { Panel, Slider, Checkbox, Button, Select } from "../ui";
import * as Icons from "../icons";

export type RebuildMode = "auto" | "always" | "never";
export type SymmetryAxis = "none" | "x" | "y" | "z";

export interface RetopologyConfig {
  targetQuads: number;
  preserveSharp: boolean;
  sharpAngle: number;
  smoothIterations: number;
  /** Reconstrucción de mallas rotas (no-manifold, cáscaras superpuestas) */
  rebuild: RebuildMode;
  /** Alineación a las direcciones de curvatura, 0–1 */
  curvatureAlignment: number;
  /** Quads más chicos donde la pieza es más delgada que un quad */
  adaptiveDensity: boolean;
  /** Simetría espejo por el centro de la caja envolvente */
  symmetry: SymmetryAxis;
  /** Los quads siguen las costuras de UV: la textura original se traslada limpia */
  followSeams: boolean;
}

/** Calidad de la malla de quads resultante */
export interface QuadQuality {
  irregular_percent: number;
  folded_quads: number;
  poor_quads: number;
  stretched_quads: number;
  mean_angle_deviation: number;
  max_distance_percent: number | null;
}

const SYMMETRY_OPTIONS = [
  { value: "none", label: "Sin simetría" },
  { value: "x", label: "Espejo en X" },
  { value: "y", label: "Espejo en Y" },
  { value: "z", label: "Espejo en Z" },
];

const REBUILD_OPTIONS = [
  { value: "auto", label: "Automática (si la malla está rota)" },
  { value: "always", label: "Siempre" },
  { value: "never", label: "Nunca" },
];

export interface RetopologyPanelProps {
  config: RetopologyConfig;
  onChange?: (config: RetopologyConfig) => void;
  onExecute?: () => void;
  canExecute?: boolean;
  isProcessing?: boolean;
  hasResult?: boolean;
  showQuadMesh?: boolean;
  onShowQuadMeshChange?: (show: boolean) => void;
  quality?: QuadQuality;
}

/** Fila del informe de calidad; `warn` la resalta. */
const QualityRow: Component<{ label: string; value: string; warn?: boolean }> = (props) => (
  <div class="flex justify-between text-xs">
    <span class="text-text-muted">{props.label}</span>
    <span class={props.warn ? "text-warning" : "text-text"}>{props.value}</span>
  </div>
);

export const RetopologyPanel: Component<RetopologyPanelProps> = (props) => {
  const updateConfig = (partial: Partial<RetopologyConfig>) => {
    props.onChange?.({ ...props.config, ...partial });
  };

  return (
    <Panel title="Retopología" icon={<Icons.GridFour size={14} />} defaultOpen>
      <div class="space-y-5">
        {/* Target Quads */}
        <Slider
          label="Quads objetivo"
          value={props.config.targetQuads}
          onChange={(value) => updateConfig({ targetQuads: Math.round(value) })}
          min={100}
          max={50000}
          step={100}
          formatValue={(v) => v.toLocaleString()}
        />

        {/* Preserve Sharp Edges */}
        <Checkbox
          label="Preservar bordes agudos"
          checked={props.config.preserveSharp}
          onChange={(checked) => updateConfig({ preserveSharp: checked })}
        />

        {/* Sharp Angle (only visible when preserveSharp is true) */}
        <Show when={props.config.preserveSharp}>
          <Slider
            label="Ángulo de borde"
            value={props.config.sharpAngle}
            onChange={(value) => updateConfig({ sharpAngle: value })}
            min={15}
            max={90}
            step={5}
            formatValue={(v) => `${v}°`}
          />
        </Show>

        {/* UV seams */}
        <div class="space-y-1">
          <Checkbox
            label="Seguir las costuras de UV"
            checked={props.config.followSeams}
            onChange={(checked) => updateConfig({ followSeams: checked })}
          />
          <p class="text-xs text-text-muted leading-relaxed">
            Cada quad queda dentro de una isla del mapa original y la textura se traslada sin
            estirarse en las costuras. Desmárcalo si vas a desplegar y hornear en UV / Piel: los
            quads salen más parejos.
          </p>
        </div>

        {/* Mirror symmetry */}
        <Select
          label="Simetría"
          options={SYMMETRY_OPTIONS}
          value={props.config.symmetry}
          onChange={(value) => updateConfig({ symmetry: value as SymmetryAxis })}
        />

        {/* Advanced options */}
        <Panel title="Avanzado" defaultOpen={false}>
          <div class="space-y-5 pt-2">
            <Slider
              label="Seguir la curvatura"
              value={Math.round(props.config.curvatureAlignment * 100)}
              onChange={(value) => updateConfig({ curvatureAlignment: value / 100 })}
              min={0}
              max={100}
              step={5}
              formatValue={(v) => `${v}%`}
            />

            <Checkbox
              label="Densidad adaptativa (rasgos delgados)"
              checked={props.config.adaptiveDensity}
              onChange={(checked) => updateConfig({ adaptiveDensity: checked })}
            />
            <Show when={props.config.adaptiveDensity}>
              <p class="text-xs text-text-muted leading-relaxed">
                Conserva puntas, dedos y paredes finas con quads más chicos, a cambio de algunos
                vértices irregulares más.
              </p>
            </Show>

            <Select
              label="Reparar malla rota"
              options={REBUILD_OPTIONS}
              value={props.config.rebuild}
              onChange={(value) => updateConfig({ rebuild: value as RebuildMode })}
            />

            <Slider
              label="Iteraciones de suavizado"
              value={props.config.smoothIterations}
              onChange={(value) => updateConfig({ smoothIterations: Math.round(value) })}
              min={1}
              max={50}
              step={1}
              formatValue={(v) => `${v}`}
            />
          </div>
        </Panel>

        {/* Execute Button */}
        <Button
          onClick={props.onExecute}
          disabled={!props.canExecute || props.isProcessing}
          variant="primary"
          class="w-full"
        >
          <Show when={!props.isProcessing} fallback={
            <span class="flex items-center gap-2">
              <span class="animate-spin">⏳</span>
              Procesando...
            </span>
          }>
            <span class="flex items-center gap-2">
              <Icons.Lightning size={16} />
              Retopologizar
            </span>
          </Show>
        </Button>

        {/* Result indicator and toggle */}
        <Show when={props.hasResult}>
          <div class="space-y-2 pt-2 border-t border-border">
            <div class="flex items-center gap-2 text-xs text-success">
              <svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <polyline points="20 6 9 17 4 12" />
              </svg>
              Retopología completada
            </div>

            <Show when={props.quality}>
              {(q) => (
                <div class="space-y-1 rounded bg-surface/20 p-2">
                  <QualityRow
                    label="Vértices irregulares"
                    value={`${q().irregular_percent.toFixed(1)}%`}
                    warn={q().irregular_percent > 8}
                  />
                  <QualityRow
                    label="Quads deformes"
                    value={q().poor_quads.toLocaleString()}
                    warn={q().folded_quads > 0}
                  />
                  <QualityRow
                    label="Quads plegados"
                    value={q().folded_quads.toLocaleString()}
                    warn={q().folded_quads > 0}
                  />
                  <QualityRow label="Quads estirados" value={q().stretched_quads.toLocaleString()} />
                  <QualityRow label="Desvío de ángulos" value={`${q().mean_angle_deviation.toFixed(1)}°`} />
                  <Show when={q().max_distance_percent !== null}>
                    <QualityRow
                      label="Distancia máx. a la original"
                      value={`${q().max_distance_percent!.toFixed(2)}%`}
                      warn={q().max_distance_percent! > 2}
                    />
                  </Show>
                </div>
              )}
            </Show>

            <Checkbox
              label="Mostrar malla de quads"
              checked={props.showQuadMesh}
              onChange={(checked) => props.onShowQuadMeshChange?.(checked)}
            />
          </div>
        </Show>
      </div>
    </Panel>
  );
};
