import { Component, Show, createSignal } from "solid-js";
import { clsx } from "clsx";
import { Button, Panel, Select } from "../ui";
import * as Icons from "../icons";
import type { TransformSpace } from "../../lib/Viewer3D";
import type { FactoryPoseId } from "../../lib/presetAnimations";

export type SelectCommand = "all" | "none" | "mirror" | "parent" | "children" | "group";
export type PoseSource = { kind: "stored"; id: string } | { kind: "factory"; id: FactoryPoseId };

export interface PosePanelProps {
  /** Hay rig en el visor */
  posing: boolean;
  hasSelection: boolean;
  space: TransformSpace;
  onSpace: (space: TransformSpace) => void;
  onSelect: (command: SelectCommand) => void;
  onReset: (what: "rotation" | "translation" | "all") => void;
  onMirror: (mode: "flip" | "leftToRight" | "rightToLeft") => void;
  onCopy: () => void;
  onPaste: (mirrored: boolean) => void;
  canPaste: boolean;
  /** Deslizadores: `commit` al soltar (queda en keys), si no es vista previa */
  onBreakdown: (t: number, commit: boolean) => void;
  onPushRelax: (factor: number, commit: boolean) => void;
}

const SPACES = [
  { value: "global", label: "Global" },
  { value: "local", label: "Local" },
  { value: "parent", label: "Del padre" },
];

/** Deslizador que avisa mientras se arrastra y al soltar */
export const LiveSlider: Component<{
  label: string;
  min: number;
  max: number;
  initial: number;
  format: (v: number) => string;
  disabled?: boolean;
  onInput: (v: number) => void;
  onCommit: (v: number) => void;
  /** Vuelve al valor inicial al soltar */
  resets?: boolean;
}> = (props) => {
  const [value, setValue] = createSignal(props.initial);
  return (
    <label class={clsx("block space-y-1", props.disabled && "opacity-50")}>
      <div class="flex justify-between text-xs">
        <span class="text-text-muted">{props.label}</span>
        <span class="font-mono text-text">{props.format(value())}</span>
      </div>
      <input
        type="range"
        class="w-full accent-[var(--color-accent)]"
        min={props.min}
        max={props.max}
        step={1}
        value={value()}
        disabled={props.disabled}
        onInput={(e) => {
          const v = Number(e.currentTarget.value);
          setValue(v);
          props.onInput(v);
        }}
        onChange={(e) => {
          const v = Number(e.currentTarget.value);
          props.onCommit(v);
          if (props.resets) setValue(props.initial);
        }}
      />
    </label>
  );
};

/** Herramientas de pose: selección, ejes, reiniciar, espejo, copiar/pegar e intermedias (las poses guardadas y de fábrica están en la Biblioteca) */
export const PosePanel: Component<PosePanelProps> = (props) => {
  return (
    <div class="space-y-3">
      <Panel title="Pose" icon={<Icons.PersonArmsSpread size={14} />} defaultOpen>
        <div class="space-y-3">
          {/* Selección */}
          <div class="space-y-1">
            <span class="text-xs text-text-muted">Seleccionar</span>
            <div class="grid grid-cols-3 gap-1">
              <Button size="sm" onClick={() => props.onSelect("all")} title="Todas las articulaciones visibles (A)">
                Todo
              </Button>
              <Button size="sm" onClick={() => props.onSelect("none")} title="Ninguna (Alt+A)">
                Nada
              </Button>
              <Button size="sm" onClick={() => props.onSelect("mirror")} disabled={!props.hasSelection} title="Las del otro lado (Ctrl+Shift+M)">
                Espejo
              </Button>
              <Button size="sm" onClick={() => props.onSelect("parent")} disabled={!props.hasSelection} title="El padre ([; Shift suma)">
                Padre
              </Button>
              <Button size="sm" onClick={() => props.onSelect("children")} disabled={!props.hasSelection} title="Los hijos (]; Shift: todos los descendientes)">
                Hijos
              </Button>
              <Button size="sm" onClick={() => props.onSelect("group")} disabled={!props.hasSelection} title="Todo su grupo (Shift+G)">
                Grupo
              </Button>
            </div>
          </div>

          <Select
            label="Ejes del gizmo"
            options={SPACES}
            value={props.space}
            onChange={(v) => props.onSpace(v as TransformSpace)}
          />

          <Show when={props.posing}>
            {/* Reiniciar */}
            <div class="space-y-1">
              <span class="text-xs text-text-muted">
                Volver al reposo {props.hasSelection ? "(la selección)" : "(todo el esqueleto)"}
              </span>
              <div class="grid grid-cols-3 gap-1">
                <Button size="sm" onClick={() => props.onReset("rotation")} title="Alt+R">
                  Giro
                </Button>
                <Button size="sm" onClick={() => props.onReset("translation")} title="Alt+G">
                  Posición
                </Button>
                <Button size="sm" onClick={() => props.onReset("all")}>
                  Todo
                </Button>
              </div>
            </div>

            {/* Espejo */}
            <div class="space-y-1">
              <span class="text-xs text-text-muted">Espejo</span>
              <div class="grid grid-cols-3 gap-1">
                <Button size="sm" onClick={() => props.onMirror("leftToRight")} title="Copia el lado izquierdo sobre el derecho">
                  Izq → der
                </Button>
                <Button size="sm" onClick={() => props.onMirror("rightToLeft")} title="Copia el lado derecho sobre el izquierdo">
                  Der → izq
                </Button>
                <Button size="sm" onClick={() => props.onMirror("flip")} title="Cada lado toma la pose del otro">
                  Voltear
                </Button>
              </div>
            </div>

            {/* Portapapeles */}
            <div class="grid grid-cols-3 gap-1">
              <Button size="sm" onClick={props.onCopy} title="Copia la pose de la selección, o de todo (Ctrl+C)">
                Copiar
              </Button>
              <Button size="sm" onClick={() => props.onPaste(false)} disabled={!props.canPaste} title="Ctrl+V">
                Pegar
              </Button>
              <Button size="sm" onClick={() => props.onPaste(true)} disabled={!props.canPaste} title="Pegar del otro lado (Ctrl+Shift+V)">
                Espejado
              </Button>
            </div>

            {/* Intermedias */}
            <LiveSlider
              label="Intermedia (key anterior → siguiente)"
              min={0}
              max={100}
              initial={50}
              format={(v) => `${v} %`}
              resets
              onInput={(v) => props.onBreakdown(v / 100, false)}
              onCommit={(v) => props.onBreakdown(v / 100, true)}
            />
            <LiveSlider
              label="Relajar ← → empujar"
              min={0}
              max={200}
              initial={100}
              format={(v) => `${v} %`}
              resets
              onInput={(v) => props.onPushRelax(v / 100, false)}
              onCommit={(v) => props.onPushRelax(v / 100, true)}
            />
          </Show>
        </div>
      </Panel>

    </div>
  );
};
