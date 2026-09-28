import { Component, For, createSignal } from "solid-js";
import { Panel, Button, Select } from "../ui";
import * as Icons from "../icons";
import type { PlacementMode } from "../../lib/Viewer3D";
import type { Axis, OriginMode } from "../../lib/placement";

export interface PlacementPanelProps {
  /** Qué se está eligiendo con el clic (`undefined` = nada) */
  mode?: PlacementMode;
  onPickMode?: (mode: PlacementMode | undefined) => void;
  onRotate?: (axis: Axis, degrees: number) => void;
  onMirror?: (axis: Axis) => void;
  onDrop?: () => void;
  /** Con "point" se elige el punto con un clic en el visor */
  onOrigin?: (mode: OriginMode) => void;
  disabled?: boolean;
}

const ORIGIN_OPTIONS = [
  { value: "base", label: "Centro de la base" },
  { value: "box", label: "Centro de la caja" },
  { value: "mass", label: "Centro de masa" },
  { value: "point", label: "Punto con clic" },
];

export const PlacementPanel: Component<PlacementPanelProps> = (props) => {
  const [origin, setOrigin] = createSignal<OriginMode>("base");
  const toggle = (mode: PlacementMode) => props.onPickMode?.(props.mode === mode ? undefined : mode);

  return (
    <Panel title="Orientación" icon={<Icons.CubeFocus size={14} />} defaultOpen>
      <div class="space-y-4">
        <div class="space-y-2">
          <Button
            onClick={() => toggle("floor")}
            variant={props.mode === "floor" ? "danger" : "primary"}
            fullWidth
            disabled={props.disabled}
          >
            {props.mode === "floor" ? "Cancelar" : "Elegir piso"}
          </Button>
          <Button
            onClick={() => toggle("front")}
            variant={props.mode === "front" ? "danger" : "default"}
            fullWidth
            disabled={props.disabled}
          >
            {props.mode === "front" ? "Cancelar" : "Elegir frente"}
          </Button>
          <p class="text-[11px] text-text-muted leading-snug">
            El piso apoya el modelo en la grilla; el frente lo gira para que mire hacia +Z. Esqueleto, retopología y
            pesos se mueven con él.
          </p>
        </div>

        <div class="space-y-2">
          <p class="text-xs text-text-muted">Girar 90°</p>
          <div class="grid grid-cols-3 gap-1.5">
            <For each={[-90, 90]}>
              {(degrees) => (
                <For each={["x", "y", "z"] as Axis[]}>
                  {(axis) => (
                    <Button size="sm" onClick={() => props.onRotate?.(axis, degrees)} disabled={props.disabled}>
                      {axis.toUpperCase()} {degrees > 0 ? "+90" : "−90"}
                    </Button>
                  )}
                </For>
              )}
            </For>
          </div>
          <div class="grid grid-cols-2 gap-1.5">
            <Button size="sm" onClick={() => props.onRotate?.("x", -90)} disabled={props.disabled}>
              Z arriba → Y
            </Button>
            <Button size="sm" onClick={() => props.onMirror?.("x")} disabled={props.disabled}>
              Espejar X
            </Button>
          </div>
        </div>

        <div class="space-y-2">
          <Select
            label="Origen"
            options={ORIGIN_OPTIONS}
            value={origin()}
            onChange={(value) => setOrigin(value as OriginMode)}
            disabled={props.disabled}
          />
          <div class="grid grid-cols-2 gap-1.5">
            <Button
              size="sm"
              variant={props.mode === "point" ? "danger" : "default"}
              onClick={() => (props.mode === "point" ? props.onPickMode?.(undefined) : props.onOrigin?.(origin()))}
              disabled={props.disabled}
            >
              {props.mode === "point" ? "Cancelar" : "Fijar origen"}
            </Button>
            <Button size="sm" onClick={() => props.onDrop?.()} disabled={props.disabled}>
              Apoyar en el piso
            </Button>
          </div>
        </div>
      </div>
    </Panel>
  );
};
