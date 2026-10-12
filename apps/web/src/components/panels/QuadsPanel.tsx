import { Component } from "solid-js";
import { Checkbox, Slider, Hint } from "../ui";
import { RemeshActions, type RemeshActionsProps } from "./RemeshActions";
import { t } from "../../lib/i18n";

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
        label={t("Ángulo entre caras")}
        value={props.config.faceAngle}
        onChange={(faceAngle) => update({ faceAngle })}
        min={0}
        max={90}
        step={5}
        formatValue={(v) => t("hasta {v}°", { v })}
        title={t("Dos triángulos que se doblan más que esto no se juntan (arista viva)")}
      />
      <Slider
        label={t("Forma")}
        value={props.config.shapeAngle}
        onChange={(shapeAngle) => update({ shapeAngle })}
        min={0}
        max={90}
        step={5}
        formatValue={(v) => t("esquinas a ±{v}° de 90°", { v })}
        title={t("Cuánto puede alejarse el quad de un rectángulo")}
      />
      <Hint label={t("Cómo funciona")} text={t("Junta primero los pares que dan los quads más rectos y planos, sin cruzar costuras de UV ni normales partidas. Los triángulos sin pareja quedan.")} />

      <div class="space-y-1">
        <Checkbox label={t("Todo quads")} checked={props.config.allQuads} onChange={(allQuads) => update({ allQuads })}
          hint={t("Subdivide un paso sin cambiar la forma: cuatro veces más caras, ninguna de tres lados.")}
        />
      </div>

      <Hint label={t("Cómo funciona")} text={t("Los vértices no se mueven. Los quads se exportan como tales en OBJ y USD; los demás formatos los triangulan.")} />

      <RemeshActions {...props} />
    </div>
  );
};
