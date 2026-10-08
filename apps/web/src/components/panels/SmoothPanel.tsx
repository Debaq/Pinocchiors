import { Component, Show } from "solid-js";
import { Checkbox, Slider } from "../ui";
import { RemeshActions, type RemeshActionsProps } from "./RemeshActions";

/** Opciones de Suavizar (Remallar) */
export interface SmoothConfig {
  /** Pasadas del filtro */
  iterations: number;
  /** Cuánto se acerca cada vértice al promedio de sus vecinos por pasada, en % */
  strength: number;
  /** Respetar las aristas vivas */
  keepSharp: boolean;
  /** Ángulo entre caras desde el que una arista es viva, en grados */
  sharpAngle: number;
  /** No mover los bordes abiertos */
  fixBorders: boolean;
  /** Mover los vértices solo hacia afuera o adentro (la textura no se corre) */
  normalOnly: boolean;
}

export const DEFAULT_SMOOTH: SmoothConfig = {
  iterations: 10,
  strength: 50,
  keepSharp: true,
  sharpAngle: 60,
  fixBorders: false,
  normalOnly: true,
};

/** Los parámetros del comando `remesh_preview` / `remesh_apply` */
export const smoothParams = (c: SmoothConfig) => ({
  mode: "smooth" as const,
  iterations: Math.round(c.iterations),
  strength: c.strength / 100,
  sharp_angle: c.keepSharp ? c.sharpAngle : null,
  fix_borders: c.fixBorders,
  normal_only: c.normalOnly,
});

export interface SmoothPanelProps extends RemeshActionsProps {
  config: SmoothConfig;
  onChange: (config: SmoothConfig) => void;
}

export const SmoothPanel: Component<SmoothPanelProps> = (props) => {
  const update = (change: Partial<SmoothConfig>) => props.onChange({ ...props.config, ...change });

  return (
    <div data-smooth class="space-y-4">
      <Slider
        label="Pasadas"
        value={props.config.iterations}
        onChange={(iterations) => update({ iterations })}
        min={1}
        max={100}
        step={1}
        title="Más pasadas quitan ruido más grueso"
      />
      <Slider
        label="Intensidad"
        value={props.config.strength}
        onChange={(strength) => update({ strength })}
        min={5}
        max={100}
        step={5}
        formatValue={(v) => `${v} %`}
        title="Cuánto se acerca cada vértice al promedio de sus vecinos en cada pasada"
      />

      <div class="space-y-2">
        <Checkbox label="Respetar las aristas vivas" checked={props.config.keepSharp} onChange={(keepSharp) => update({ keepSharp })} />
        <Show when={props.config.keepSharp}>
          <Slider
            label="Desde"
            value={props.config.sharpAngle}
            onChange={(sharpAngle) => update({ sharpAngle })}
            min={10}
            max={120}
            step={5}
            formatValue={(v) => `${v}° entre caras`}
          />
          <p class="text-xs text-text-muted leading-relaxed">
            Las aristas vivas se suavizan solo a lo largo y sus esquinas quedan quietas. En un escaneo con mucho ruido, un
            ángulo bajo marca el ruido como arista.
          </p>
        </Show>
      </div>

      <Checkbox label="No mover los bordes abiertos" checked={props.config.fixBorders} onChange={(fixBorders) => update({ fixBorders })} />
      <div class="space-y-1">
        <Checkbox
          label="Sin deslizar por la superficie"
          checked={props.config.normalOnly}
          onChange={(normalOnly) => update({ normalOnly })}
        />
        <p class="text-xs text-text-muted leading-relaxed">
          Los vértices solo se mueven hacia afuera o hacia adentro: quita el ruido sin que la textura se corra. Apagado,
          además empareja el tamaño de los triángulos.
        </p>
      </div>

      <p class="text-xs text-text-dim leading-relaxed">
        No encoge el modelo. Los triángulos, las UV y los pesos no cambian; las normales se rehacen.
      </p>

      <RemeshActions {...props} />
    </div>
  );
};
