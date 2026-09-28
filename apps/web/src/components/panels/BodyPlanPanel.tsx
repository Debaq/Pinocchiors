import { Component, For, Show } from "solid-js";
import { Panel, Slider, Checkbox } from "../ui";
import * as Icons from "../icons";

/** Forma de cuerpo + apéndices (ver `BodyPlanDto` en apps/desktop) */
export interface BodyPlan {
  shape: "biped" | "digitigrade" | "quadruped" | "radial" | "fish" | "arthropod" | "serpent";
  neck: number;
  tail: number;
  trunk: number;
  ears: number;
  wings: number;
  limbs: number;
  limb_segments: number;
  fins: boolean;
  pincers: boolean;
  antennae: number;
}

type NumericField = "neck" | "tail" | "trunk" | "ears" | "wings" | "limbs" | "limb_segments" | "antennae";

interface Control {
  field: NumericField;
  label: string;
  min: number;
  max: number;
}

const HEADED = ["biped", "digitigrade", "quadruped"];

/** Controles que aplican a cada forma */
function controlsFor(shape: BodyPlan["shape"]): Control[] {
  const controls: Control[] = [];
  if (HEADED.includes(shape)) {
    controls.push({ field: "neck", label: "Cuello (segmentos)", min: 1, max: 10 });
    controls.push({ field: "trunk", label: "Trompa", min: 0, max: 12 });
    controls.push({ field: "ears", label: "Orejas móviles", min: 0, max: 3 });
  }
  if (shape === "serpent") {
    controls.push({ field: "tail", label: "Segmentos del cuerpo", min: 4, max: 32 });
  } else if (shape !== "radial") {
    controls.push({ field: "tail", label: "Cola", min: 0, max: 16 });
  }
  if (HEADED.includes(shape) || shape === "arthropod") {
    controls.push({ field: "wings", label: "Alas (segmentos)", min: 0, max: 5 });
  }
  if (shape === "radial") {
    controls.push({ field: "limbs", label: "Brazos", min: 3, max: 12 });
    controls.push({ field: "limb_segments", label: "Segmentos por brazo", min: 2, max: 10 });
  }
  if (shape === "arthropod") {
    controls.push({ field: "limbs", label: "Pares de patas", min: 1, max: 6 });
    controls.push({ field: "limb_segments", label: "Segmentos por pata", min: 2, max: 4 });
    controls.push({ field: "antennae", label: "Antenas", min: 0, max: 4 });
  }
  return controls;
}

export interface BodyPlanPanelProps {
  plan: BodyPlan;
  onChange?: (plan: BodyPlan) => void;
  disabled?: boolean;
}

/** Apéndices de una plantilla generada: cada cambio rehace el esqueleto */
export const BodyPlanPanel: Component<BodyPlanPanelProps> = (props) => {
  const update = (partial: Partial<BodyPlan>) => props.onChange?.({ ...props.plan, ...partial });
  return (
    <Panel title="Apéndices" icon={<Icons.Bone size={14} />} defaultOpen>
      <div class="space-y-4 pt-1">
        <p class="text-xs text-text-muted leading-relaxed">
          Más segmentos doblan más suave (una trompa o un tentáculo necesitan varios). 0 quita el
          apéndice.
        </p>
        <For each={controlsFor(props.plan.shape)}>
          {(control) => (
            <Slider
              label={control.label}
              value={props.plan[control.field]}
              onChange={(v) => update({ [control.field]: Math.round(v) } as Partial<BodyPlan>)}
              min={control.min}
              max={control.max}
              step={1}
              disabled={props.disabled}
            />
          )}
        </For>
        <Show when={props.plan.shape === "fish"}>
          <Checkbox label="Aletas pectorales y dorsal" checked={props.plan.fins} onChange={(fins) => update({ fins })} />
        </Show>
        <Show when={props.plan.shape === "arthropod"}>
          <Checkbox label="Pinzas" checked={props.plan.pincers} onChange={(pincers) => update({ pincers })} />
        </Show>
      </div>
    </Panel>
  );
};
