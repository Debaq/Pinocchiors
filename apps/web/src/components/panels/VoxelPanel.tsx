import { Component, Show } from "solid-js";
import { Checkbox, Slider } from "../ui";
import { RemeshActions, type RemeshActionsProps } from "./RemeshActions";
import { formatMm, type RemeshInfo } from "./IsotropicPanel";

/** Opciones de Vóxeles (Remallar) */
export interface VoxelConfig {
  /** Vóxeles en el lado más largo del modelo */
  resolution: number;
  /** Pasadas de suavizado después */
  smoothIterations: number;
  /** Triángulos parejos después (lado = `EVEN_EDGE` vóxeles) */
  even: boolean;
}

export const DEFAULT_VOXEL: VoxelConfig = { resolution: 200, smoothIterations: 2, even: true };

/** Lado de los triángulos parejos, en vóxeles */
const EVEN_EDGE = 1.5;

/** Lado del vóxel en mm */
export const voxelSize = (c: VoxelConfig, info?: RemeshInfo) => (info?.extent_mm ?? 100) / c.resolution;

export const voxelParams = (c: VoxelConfig, info?: RemeshInfo) => ({
  mode: "voxel" as const,
  voxel_mm: voxelSize(c, info),
  smooth_iterations: Math.round(c.smoothIterations),
  isotropic_edge_mm: c.even ? voxelSize(c, info) * EVEN_EDGE : null,
});

/** La grilla que usaría la reconstrucción (del backend) */
export interface VoxelGridInfo {
  voxel_mm: number;
  dims: [number, number, number];
  triangles: number;
  memory_mb: number;
}

/** Desde aquí se pide confirmación antes de correr */
export const isHeavyGrid = (g: VoxelGridInfo) => g.memory_mb > 1500 || g.triangles > 3_000_000;

export interface VoxelPanelProps extends RemeshActionsProps {
  config: VoxelConfig;
  onChange: (config: VoxelConfig) => void;
  info?: RemeshInfo;
  grid?: VoxelGridInfo;
}

const short = (v: number) => v.toLocaleString("es", { maximumSignificantDigits: 2 });

export const VoxelPanel: Component<VoxelPanelProps> = (props) => {
  const update = (change: Partial<VoxelConfig>) => props.onChange({ ...props.config, ...change });

  return (
    <div data-voxel class="space-y-4">
      <div class="space-y-1">
        <Slider
          label="Detalle"
          value={props.config.resolution}
          onChange={(resolution) => update({ resolution })}
          min={48}
          max={640}
          step={8}
          formatValue={(v) => `${v} en el lado largo`}
          title="Vóxeles en el lado más largo del modelo; los rasgos más chicos que un vóxel se pierden"
        />
        <Show when={props.grid}>
          {(g) => (
            <p data-voxel-grid class="text-xs text-text-muted">
              Vóxel de <span class="text-text">{formatMm(g().voxel_mm)}</span> · grilla {g().dims.join(" × ")} · ≈{" "}
              {short(g().triangles)} triángulos antes de emparejar ·{" "}
              <span class={isHeavyGrid(g()) ? "text-warning" : undefined}>
                {g().memory_mb >= 1000 ? `${short(g().memory_mb / 1000)} GB` : `${Math.ceil(g().memory_mb)} MB`} de memoria
              </span>
            </p>
          )}
        </Show>
      </div>

      <Slider
        label="Suavizar después"
        value={props.config.smoothIterations}
        onChange={(smoothIterations) => update({ smoothIterations })}
        min={0}
        max={10}
        step={1}
        formatValue={(v) => (v === 0 ? "no" : `${v} ${v === 1 ? "pasada" : "pasadas"}`)}
      />
      <div class="space-y-1">
        <Checkbox label="Triángulos parejos después" checked={props.config.even} onChange={(even) => update({ even })} />
        <p class="text-xs text-text-muted leading-relaxed">
          La grilla deja muchos triángulos de tamaños dispares; emparejarlos los reduce a una cuarta parte o menos (tarda un
          poco más).
        </p>
      </div>

      <p class="text-xs text-text-dim leading-relaxed">
        Cierra los agujeros, une las piezas que se cruzan y quita las caras internas: el resultado siempre es cerrado, listo
        para imprimir. Las aristas vivas quedan redondeadas a la escala del vóxel. Es una malla nueva: se pierden las UV, la
        textura y los pesos pintados.
      </p>

      <RemeshActions {...props} />
    </div>
  );
};
