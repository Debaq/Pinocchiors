import { Component, Show } from "solid-js";
import { Button, Checkbox, Slider } from "../ui";
import * as Icons from "../icons";

/** Opciones de Simplificar (Remallar) */
export interface SimplifyConfig {
  /** Porcentaje de los triángulos que quedan */
  percent: number;
  /** Parar antes del objetivo si la forma se aleja más que `maxErrorPercent` */
  limitError: boolean;
  /** Desviación máxima, en % del tamaño del modelo */
  maxErrorPercent: number;
  /** No mover los bordes abiertos */
  lockBorders: boolean;
  /** No colapsar a través de las costuras de UV, normales y materiales */
  keepSeams: boolean;
  /** Llegar al objetivo aunque cambie la topología */
  aggressive: boolean;
}

export const DEFAULT_SIMPLIFY: SimplifyConfig = {
  percent: 25,
  limitError: false,
  maxErrorPercent: 0.5,
  lockBorders: false,
  keepSeams: true,
  aggressive: false,
};

/** Los parámetros del comando `remesh_preview` / `remesh_apply` */
export const simplifyParams = (c: SimplifyConfig) => ({
  mode: "simplify" as const,
  ratio: c.percent / 100,
  max_error_percent: c.limitError ? c.maxErrorPercent : null,
  lock_borders: c.lockBorders,
  keep_seams: c.keepSeams,
  aggressive: c.aggressive,
});

export interface SimplifyPanelProps {
  config: SimplifyConfig;
  onChange: (config: SimplifyConfig) => void;
  /** Triángulos del modelo ahora */
  triangles: number;
  hasPreview: boolean;
  onPreview: () => void;
  onApply: () => void;
  onDiscard: () => void;
  canExecute: boolean;
  isProcessing: boolean;
}

export const SimplifyPanel: Component<SimplifyPanelProps> = (props) => {
  const update = (change: Partial<SimplifyConfig>) => props.onChange({ ...props.config, ...change });
  const target = () => Math.max(1, Math.round((props.triangles * props.config.percent) / 100));

  return (
    <div data-simplify class="space-y-4">
      <div class="space-y-1">
        <Slider
          label="Quedan"
          value={props.config.percent}
          onChange={(percent) => update({ percent })}
          min={0.5}
          max={100}
          step={0.5}
          formatValue={(v) => `${v.toLocaleString("es")} %`}
          title="Doble clic en el valor para escribirlo"
        />
        <p class="text-xs text-text-muted">
          ≈ <span data-simplify-target class="text-text">{target().toLocaleString("es")}</span> de{" "}
          {props.triangles.toLocaleString("es")} triángulos
          <Show when={props.config.limitError}>
            <span class="text-text-dim"> (o menos si se aleja demasiado)</span>
          </Show>
        </p>
      </div>

      <div class="space-y-2">
        <Checkbox
          label="Limitar cuánto se aleja de la forma"
          checked={props.config.limitError}
          onChange={(limitError) => update({ limitError })}
        />
        <Show when={props.config.limitError}>
          <Slider
            label="Desviación máxima"
            value={props.config.maxErrorPercent}
            onChange={(maxErrorPercent) => update({ maxErrorPercent })}
            min={0.05}
            max={5}
            step={0.05}
            formatValue={(v) => `${v.toLocaleString("es")} % del tamaño`}
          />
        </Show>
      </div>

      <Checkbox
        label="Conservar los bordes abiertos"
        checked={props.config.lockBorders}
        onChange={(lockBorders) => update({ lockBorders })}
        disabled={props.config.aggressive}
      />
      <Checkbox
        label="Conservar las costuras de UV y materiales"
        checked={props.config.keepSeams}
        onChange={(keepSeams) => update({ keepSeams })}
        disabled={props.config.aggressive}
      />
      <div class="space-y-1">
        <Checkbox
          label="Agresivo"
          checked={props.config.aggressive}
          onChange={(aggressive) => update({ aggressive })}
        />
        <p class="text-xs text-text-muted leading-relaxed">
          Llega a la cantidad pedida aunque junte partes cercanas o cierre agujeros chicos. Para
          reducir muchísimo cuando la forma exacta no importa (vistas lejanas, colisiones).
        </p>
      </div>

      <p class="text-xs text-text-dim leading-relaxed">
        Las UV, la textura y los pesos se conservan: cada vértice que queda es uno del original.
      </p>

      <div class="flex gap-2">
        <Button
          data-remesh-preview
          class="flex-1"
          onClick={() => props.onPreview()}
          disabled={!props.canExecute || props.isProcessing}
          icon={<Icons.Eye size={14} />}
        >
          Vista previa
        </Button>
        <Button
          data-remesh-apply
          variant="primary"
          class="flex-1"
          onClick={() => props.onApply()}
          disabled={!props.canExecute || props.isProcessing}
          loading={props.isProcessing}
        >
          Aplicar
        </Button>
      </div>
      <Show when={props.hasPreview}>
        <Button data-remesh-discard variant="ghost" size="sm" fullWidth onClick={() => props.onDiscard()} disabled={props.isProcessing}>
          Descartar la vista previa
        </Button>
      </Show>
    </div>
  );
};
