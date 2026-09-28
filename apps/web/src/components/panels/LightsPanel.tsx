import { Component } from "solid-js";
import { Panel, Slider, Checkbox, Button } from "../ui";
import * as Icons from "../icons";
import { defaultLights, type LightSettings } from "../../lib/Viewer3D";

export interface LightsPanelProps {
  lights: LightSettings;
  onChange?: (lights: LightSettings) => void;
}

/** Luces del visor: la principal se mueve con los controles o con Alt + arrastrar */
export const LightsPanel: Component<LightsPanelProps> = (props) => {
  const update = (partial: Partial<LightSettings>) => props.onChange?.({ ...props.lights, ...partial });
  return (
    <Panel title="Luces" icon={<Icons.Lightning size={14} />} defaultOpen={false}>
      <div class="space-y-4 pt-1">
        <p class="text-xs text-text-muted leading-relaxed">
          <span class="text-text">Alt + arrastrar</span> en el visor gira la luz principal alrededor
          del modelo.
        </p>
        <Slider
          label="Dirección"
          value={Math.round(props.lights.azimuth)}
          onChange={(v) => update({ azimuth: v })}
          min={-180}
          max={180}
          step={5}
          formatValue={(v) => `${v}°`}
        />
        <Slider
          label="Altura"
          value={Math.round(props.lights.elevation)}
          onChange={(v) => update({ elevation: v })}
          min={-10}
          max={89}
          step={1}
          formatValue={(v) => `${v}°`}
        />
        <Slider
          label="Intensidad"
          value={props.lights.intensity}
          onChange={(v) => update({ intensity: v })}
          min={0}
          max={6}
          step={0.1}
          formatValue={(v) => v.toFixed(1)}
        />
        <label class="flex items-center justify-between text-xs text-text-muted">
          Color
          <input
            type="color"
            value={props.lights.color}
            onInput={(e) => update({ color: e.currentTarget.value })}
            class="w-10 h-6 rounded border border-border bg-transparent cursor-pointer"
          />
        </label>
        <Slider
          label="Relleno"
          value={props.lights.fill}
          onChange={(v) => update({ fill: v })}
          min={0}
          max={3}
          step={0.1}
          formatValue={(v) => v.toFixed(1)}
        />
        <Slider
          label="Ambiente"
          value={props.lights.ambient}
          onChange={(v) => update({ ambient: v })}
          min={0}
          max={3}
          step={0.1}
          formatValue={(v) => v.toFixed(1)}
        />
        <Slider
          label="Reflejos del entorno"
          value={props.lights.environment}
          onChange={(v) => update({ environment: v })}
          min={0}
          max={2}
          step={0.1}
          formatValue={(v) => v.toFixed(1)}
        />
        <Checkbox
          label="Luz desde la cámara"
          checked={props.lights.headlight}
          onChange={(headlight) => update({ headlight })}
        />
        <Button variant="ghost" fullWidth onClick={() => props.onChange?.({ ...defaultLights })}>
          Restablecer luces
        </Button>
      </div>
    </Panel>
  );
};
