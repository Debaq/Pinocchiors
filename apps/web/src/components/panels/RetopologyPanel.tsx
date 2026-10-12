import { Component, For, Show } from "solid-js";
import { Panel, Slider, Checkbox, Button, Select, Tooltip } from "../ui";
import * as Icons from "../icons";
import { fmtNumber, t, tx } from "../../lib/i18n";

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
  /** Cada objeto (material) se retopologiza aparte; proyectos viejos no lo traen (sí) */
  perObject?: boolean;
}

/** Resultado de un objeto retopologizado por separado */
export interface RetopologyObject {
  name: string;
  faces_in: number;
  target: number;
  quads: number;
  /** Por qué el objeto no quedó en la malla nueva */
  error: string | null;
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
  { value: "none", get label() { return t("Sin simetría"); } },
  // Ejes como se muestran (Z arriba): el valor es el eje interno (Y arriba)
  { value: "x", get label() { return t("Espejo en X (izquierda-derecha)"); } },
  { value: "z", get label() { return t("Espejo en Y (frente-atrás)"); } },
  { value: "y", get label() { return t("Espejo en Z (arriba-abajo)"); } },
];

const REBUILD_OPTIONS = [
  { value: "auto", get label() { return t("Automática (si la malla está rota)"); } },
  { value: "always", get label() { return t("Siempre"); } },
  { value: "never", get label() { return t("Nunca"); } },
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
  objects?: RetopologyObject[];
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
    <Panel title={t("Retopología")} icon={<Icons.GridFour size={14} />} defaultOpen>
      <div class="space-y-5">
        {/* Target Quads */}
        <Slider
          label={t("Quads objetivo")}
          value={props.config.targetQuads}
          onChange={(value) => updateConfig({ targetQuads: Math.round(value) })}
          min={100}
          max={50000}
          step={100}
          formatValue={(v) => fmtNumber(v)}
        />

        {/* Preserve Sharp Edges */}
        <Checkbox
          label={t("Preservar bordes agudos")}
          checked={props.config.preserveSharp}
          onChange={(checked) => updateConfig({ preserveSharp: checked })}
        />

        {/* Sharp Angle (only visible when preserveSharp is true) */}
        <Show when={props.config.preserveSharp}>
          <Slider
            label={t("Ángulo de borde")}
            value={props.config.sharpAngle}
            onChange={(value) => updateConfig({ sharpAngle: value })}
            min={15}
            max={90}
            step={5}
            formatValue={(v) => `${v}°`}
          />
        </Show>

        <Checkbox
          label={t("Seguir las costuras de UV")}
          hint={t("Los quads respetan las islas del mapa original. Desmárcalo si vas a desplegar de nuevo en UV / Piel: salen más parejos.")}
          checked={props.config.followSeams}
          onChange={(checked) => updateConfig({ followSeams: checked })}
        />

        <Checkbox
          label={t("Por objeto")}
          hint={t("Cada material se retopologiza aparte con su parte de los quads (según su área): no se funden ni se pierden piezas. Ignora la simetría.")}
          checked={props.config.perObject !== false}
          onChange={(checked) => updateConfig({ perObject: checked })}
        />

        {/* Mirror symmetry */}
        <Tooltip
          content={t("La simetría deja un loop de aristas en la línea media, útil para el rig.")}
          placement="left"
          class="block w-full"
        >
          <Select
            label={tx("simetría", "Simetría")}
            options={SYMMETRY_OPTIONS}
            value={props.config.symmetry}
            onChange={(value) => updateConfig({ symmetry: value as SymmetryAxis })}
          />
        </Tooltip>

        {/* Advanced options */}
        <Panel title={t("Avanzado")} defaultOpen={false}>
          <div class="space-y-5 pt-2">
            <Slider
              label={t("Seguir la curvatura")}
              value={Math.round(props.config.curvatureAlignment * 100)}
              onChange={(value) => updateConfig({ curvatureAlignment: value / 100 })}
              min={0}
              max={100}
              step={5}
              formatValue={(v) => `${v}%`}
            />

            <Checkbox
              label={t("Densidad adaptativa (rasgos delgados)")}
              hint={t("Conserva puntas, dedos y paredes finas con quads más chicos, a cambio de algunos vértices irregulares más.")}
              checked={props.config.adaptiveDensity}
              onChange={(checked) => updateConfig({ adaptiveDensity: checked })}
            />

            <Select
              label={t("Reparar malla rota")}
              options={REBUILD_OPTIONS}
              value={props.config.rebuild}
              onChange={(value) => updateConfig({ rebuild: value as RebuildMode })}
            />

            <Slider
              label={t("Iteraciones de suavizado")}
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
          hint={t("Convierte la malla en quads con estos ajustes")}
        >
          <Show when={!props.isProcessing} fallback={
            <span class="flex items-center gap-2">
              <span class="animate-spin">⏳</span>
              {t("Procesando...")}
            </span>
          }>
            <span class="flex items-center gap-2">
              <Icons.Lightning size={16} />
              {t("Retopologizar")}
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
              {t("Retopología completada")}
            </div>

            <Show when={props.quality}>
              {(q) => (
                <div class="space-y-1 rounded bg-surface/20 p-2">
                  <QualityRow
                    label={t("Vértices irregulares")}
                    value={`${q().irregular_percent.toFixed(1)}%`}
                    warn={q().irregular_percent > 8}
                  />
                  <QualityRow
                    label={t("Quads deformes")}
                    value={fmtNumber(q().poor_quads)}
                    warn={q().folded_quads > 0}
                  />
                  <QualityRow
                    label={t("Quads plegados")}
                    value={fmtNumber(q().folded_quads)}
                    warn={q().folded_quads > 0}
                  />
                  <QualityRow label={t("Quads estirados")} value={fmtNumber(q().stretched_quads)} />
                  <QualityRow label={t("Desvío de ángulos")} value={`${q().mean_angle_deviation.toFixed(1)}°`} />
                  <Show when={q().max_distance_percent !== null}>
                    <QualityRow
                      label={t("Distancia máx. a la original")}
                      value={`${q().max_distance_percent!.toFixed(2)}%`}
                      warn={q().max_distance_percent! > 2}
                    />
                  </Show>
                </div>
              )}
            </Show>

            <Show when={(props.objects?.length ?? 0) > 1}>
              <div class="space-y-1 rounded bg-surface/20 p-2">
                <For each={props.objects}>
                  {(o) => (
                    <div class="flex justify-between gap-2 text-xs" title={o.error ?? undefined}>
                      <span class="text-text-muted truncate">{o.name}</span>
                      <span class={o.error ? "text-warning" : "text-text"}>
                        {o.error ? t("falló") : fmtNumber(o.quads)}
                      </span>
                    </div>
                  )}
                </For>
              </div>
            </Show>

            <Checkbox
              label={t("Mostrar malla de quads")}
              checked={props.showQuadMesh}
              onChange={(checked) => props.onShowQuadMeshChange?.(checked)}
            />
          </div>
        </Show>
      </div>
    </Panel>
  );
};
