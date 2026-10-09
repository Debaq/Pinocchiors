import { Component } from "solid-js";
import { Checkbox, Slider } from "../ui";
import { RemeshActions, type RemeshActionsProps } from "./RemeshActions";

/** Opciones de Triángulos a quads (Remallar) */
export interface QuadsConfig {
  /** Ángulo máximo entre los dos triángulos, en grados */
  faceAngle: number;
  /** Desvío máximo de las esquinas respecto de 90°, en grados */
  shapeAngle: number;
  /** Subdividir un paso para que todo sean quads */
  allQuads: boolean;
}

export const DEFAULT_QUADS: QuadsConfig = { faceAngle: 40, shapeAngle: 40, allQuads: false };

export const quadsParams = (c: QuadsConfig) => ({
  mode: "quads" as const,
  face_angle: c.faceAngle,
  shape_angle: c.shapeAngle,
  all_quads: c.allQuads,
});

export interface QuadsPanelProps extends RemeshActionsProps {
  config: QuadsConfig;
  onChange: (config: QuadsConfig) => void;
}

export const QuadsPanel: Component<QuadsPanelProps> = (props) => {
  const update = (change: Partial<QuadsConfig>) => props.onChange({ ...props.config, ...change });

  return (
    <div data-quads class="space-y-4">
      <Slider
        label="Ángulo entre caras"
        value={props.config.faceAngle}
        onChange={(faceAngle) => update({ faceAngle })}
        min={0}
        max={90}
        step={5}
        formatValue={(v) => `hasta ${v}°`}
        title="Dos triángulos que se doblan más que esto no se juntan (arista viva)"
      />
      <Slider
        label="Forma"
        value={props.config.shapeAngle}
        onChange={(shapeAngle) => update({ shapeAngle })}
        min={0}
        max={90}
        step={5}
        formatValue={(v) => `esquinas a ±${v}° de 90°`}
        title="Cuánto puede alejarse el quad de un rectángulo"
      />
      <p class="text-xs text-text-muted leading-relaxed">
        Se juntan primero los pares que forman los quads más rectos y planos. No se cruzan las costuras de UV ni las aristas
        con normales partidas; los triángulos que quedan sin pareja siguen siendo triángulos.
      </p>

      <div class="space-y-1">
        <Checkbox label="Todo quads" checked={props.config.allQuads} onChange={(allQuads) => update({ allQuads })} />
        <p class="text-xs text-text-muted leading-relaxed">
          Además subdivide un paso sin cambiar la forma: cada quad da cuatro y cada triángulo suelto tres. Cuatro veces más
          caras, pero ninguna de tres lados.
        </p>
      </div>

      <p class="text-xs text-text-dim leading-relaxed">
        Los vértices no se mueven: UV, textura y pesos se conservan. Los quads se exportan como tales en OBJ y USD; GLB, STL y
        los demás formatos los triangulan.
      </p>

      <RemeshActions {...props} />
    </div>
  );
};
