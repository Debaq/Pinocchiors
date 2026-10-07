import { Component, For, Show } from "solid-js";
import { clsx } from "clsx";
import { Button, Checkbox, NumberInput, Panel, Select } from "../ui";
import * as Icons from "../icons";
import { createPersisted } from "../../lib/ui-state";
import type { Vec3 } from "../../lib/axes";

export type TuneDirection = "forward" | "back" | "up" | "down" | "left" | "right";
export type TuneRotation = "pitch" | "spread" | "turn";

export interface JointTunePanelProps {
  boneNames: string[];
  /** Articulación elegida (−1: ninguna) */
  selected: number;
  onSelect: (index: number) => void;
  /** Posición de la elegida en los ejes que se muestran (Z arriba) */
  position?: Vec3;
  /** Tiene padre: se puede alargar el hueso que llega a ella */
  hasParent: boolean;
  /** Está a un costado (tiene lado que copiar al otro) */
  sided: boolean;
  symmetric: boolean;
  onSymmetricChange: (symmetric: boolean) => void;
  onSetPosition: (position: Vec3, withChildren: boolean) => void;
  /** `fraction` del tamaño del esqueleto */
  onMove: (direction: TuneDirection, fraction: number, withChildren: boolean) => void;
  /** `degrees` con signo: + adelante, abrir, a la izquierda */
  onRotate: (rotation: TuneRotation, degrees: number) => void;
  onStretch: (fraction: number) => void;
  onCopySide: () => void;
  onCenter?: () => void;
  disabled?: boolean;
}

const STEPS = [
  { value: "0.005", label: "0,5 %" },
  { value: "0.01", label: "1 %" },
  { value: "0.02", label: "2 %" },
  { value: "0.05", label: "5 %" },
];
const ANGLES = [
  { value: "2", label: "2°" },
  { value: "5", label: "5°" },
  { value: "15", label: "15°" },
  { value: "45", label: "45°" },
];

/** Botón chico de una flecha */
const Nudge: Component<{ label: string; title: string; onClick: () => void; disabled?: boolean }> = (props) => (
  <button
    type="button"
    title={props.title}
    disabled={props.disabled}
    class={clsx(
      "h-7 px-2 rounded border border-border text-[11px] text-text",
      "hover:border-border-hover hover:bg-surface/40 disabled:opacity-40 disabled:pointer-events-none"
    )}
    onClick={props.onClick}
  >
    {props.label}
  </button>
);

/** Fila: nombre y dos botones opuestos */
const Pair: Component<{
  label: string;
  minus: [string, string];
  plus: [string, string];
  onMinus: () => void;
  onPlus: () => void;
  disabled?: boolean;
}> = (props) => (
  <div class="flex items-center gap-2">
    <span class="flex-1 text-xs text-text-muted">{props.label}</span>
    <Nudge label={props.minus[0]} title={props.minus[1]} onClick={props.onMinus} disabled={props.disabled} />
    <Nudge label={props.plus[0]} title={props.plus[1]} onClick={props.onPlus} disabled={props.disabled} />
  </div>
);

/**
 * Ajuste fino de la articulación elegida: posición exacta, empujones en las
 * direcciones del cuerpo (adelante, arriba, a un lado), giro del miembro que
 * cuelga de ella y largo del hueso. Con espejo, el otro lado se mueve igual.
 */
export const JointTunePanel: Component<JointTunePanelProps> = (props) => {
  const [step, setStep] = createPersisted("jointTune.step", "0.01");
  const [angle, setAngle] = createPersisted("jointTune.angle", "5");
  const [withChildren, setWithChildren] = createPersisted("jointTune.children", true);
  const off = () => props.disabled || props.selected < 0;
  const move = (d: TuneDirection) => props.onMove(d, Number(step()), withChildren());
  const turn = (r: TuneRotation, sign: number) => props.onRotate(r, sign * Number(angle()));

  return (
    <div>
      <Panel title="Ajuste fino" id="skeleton.tune" icon={<Icons.Bone size={14} />} defaultOpen>
        <div class="space-y-3 pb-3">
          <Select
            label="Articulación"
            options={props.boneNames.map((name, i) => ({ value: String(i), label: name }))}
            value={props.selected >= 0 ? String(props.selected) : undefined}
            onChange={(v) => props.onSelect(Number(v))}
            placeholder="Haz clic en una articulación del visor"
          />
          <Show when={props.position}>
            {(p) => (
              <div class="grid grid-cols-3 gap-1.5">
                <For each={["X", "Y", "Z"]}>
                  {(axis, i) => (
                    <NumberInput
                      label={axis}
                      value={Math.round(p()[i()] * 1e4) / 1e4}
                      step={0.001}
                      disabled={off()}
                      onChange={(v) => {
                        const next = [...p()] as Vec3;
                        next[i()] = v;
                        props.onSetPosition(next, withChildren());
                      }}
                    />
                  )}
                </For>
              </div>
            )}
          </Show>
          <Checkbox
            label="Mover también lo que cuelga de ella"
            checked={withChildren()}
            onChange={setWithChildren}
          />
          <Checkbox label="Espejo: el otro lado igual" checked={props.symmetric} onChange={props.onSymmetricChange} />
        </div>
      </Panel>

      <Panel title="Mover" id="skeleton.tune.move" icon={<Icons.ArrowsOutCardinal size={14} />} defaultOpen>
        <div class="space-y-2 pb-3">
          <Select label="Paso (del tamaño del esqueleto)" options={STEPS} value={step()} onChange={setStep} />
          <Pair
            label="Adelante / atrás"
            minus={["Atrás", "Hacia la espalda del personaje"]}
            plus={["Adelante", "Hacia donde mira el personaje"]}
            onMinus={() => move("back")}
            onPlus={() => move("forward")}
            disabled={off()}
          />
          <Pair
            label="Arriba / abajo"
            minus={["Abajo", "Hacia el suelo"]}
            plus={["Arriba", "Hacia arriba"]}
            onMinus={() => move("down")}
            onPlus={() => move("up")}
            disabled={off()}
          />
          <Pair
            label="A los lados"
            minus={["Izquierda", "Hacia el lado izquierdo del personaje (huesos _l)"]}
            plus={["Derecha", "Hacia el lado derecho del personaje (huesos _r)"]}
            onMinus={() => move("left")}
            onPlus={() => move("right")}
            disabled={off()}
          />
          <Pair
            label="Largo del hueso"
            minus={["Acortar", "Acerca la articulación a su padre; lo que cuelga la sigue"]}
            plus={["Alargar", "Aleja la articulación de su padre; lo que cuelga la sigue"]}
            onMinus={() => props.onStretch(-Number(step()))}
            onPlus={() => props.onStretch(Number(step()))}
            disabled={off() || !props.hasParent}
          />
        </div>
      </Panel>

      <Panel title="Girar el miembro" id="skeleton.tune.rotate" icon={<Icons.ArrowsClockwise size={14} />} defaultOpen>
        <div class="space-y-2 pb-3">
          <p class="text-xs text-text-muted leading-relaxed">
            Gira todo lo que cuelga de la articulación alrededor de ella, como un bloque: una pierna entera desde la
            cadera, una mano desde la muñeca.
          </p>
          <Select label="Ángulo" options={ANGLES} value={angle()} onChange={setAngle} />
          <Pair
            label="Adelante / atrás"
            minus={["Atrás", "La punta va hacia la espalda"]}
            plus={["Adelante", "La punta va hacia donde mira el personaje"]}
            onMinus={() => turn("pitch", -1)}
            onPlus={() => turn("pitch", 1)}
            disabled={off()}
          />
          <Pair
            label="Abrir / cerrar"
            minus={["Cerrar", "La punta va hacia el centro del cuerpo"]}
            plus={["Abrir", "La punta se aleja del centro del cuerpo"]}
            onMinus={() => turn("spread", -1)}
            onPlus={() => turn("spread", 1)}
            disabled={off()}
          />
          <Pair
            label="Girar sobre la vertical"
            minus={["Derecha", "Gira hacia el lado derecho del personaje"]}
            plus={["Izquierda", "Gira hacia el lado izquierdo del personaje"]}
            onMinus={() => turn("turn", -1)}
            onPlus={() => turn("turn", 1)}
            disabled={off()}
          />
        </div>
      </Panel>

      <Panel title="Simetría y centro" id="skeleton.tune.side" icon={<Icons.Bone size={14} />} defaultOpen>
        <div class="space-y-2 pb-3">
          <Button size="sm" fullWidth disabled={off() || !props.sided} onClick={props.onCopySide}>
            Copiar este lado al otro
          </Button>
          <Show when={props.onCenter}>
            <Button size="sm" fullWidth disabled={off()} onClick={() => props.onCenter?.()}>
              Centrar en el miembro
            </Button>
          </Show>
          <p class="text-xs text-text-muted leading-relaxed">
            Corrige una mitad y copia: el otro lado queda en el reflejo. Centrar lleva la articulación al medio del
            volumen de la malla.
          </p>
        </div>
      </Panel>
    </div>
  );
};
