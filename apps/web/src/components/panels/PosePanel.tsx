import { Component, For, Show, createSignal } from "solid-js";
import { clsx } from "clsx";
import { Button, Panel, Select } from "../ui";
import * as Icons from "../icons";
import type { TransformSpace } from "../../lib/Viewer3D";
import type { FactoryPoseId } from "../../lib/presetAnimations";
import { t } from "../../lib/i18n";

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
  /** Caída como muñeco desde la pose actual, horneada a un clip nuevo */
  onRagdoll: (seconds: number) => void;
}

const SPACES = [
  { value: "global", get label() { return t("Global"); } },
  { value: "local", get label() { return t("Local"); } },
  { value: "parent", get label() { return t("Del padre"); } },
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
        <span class="text-text-muted">{t(props.label)}</span>
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
      <Panel title={t("Pose")} icon={<Icons.PersonArmsSpread size={14} />} defaultOpen>
        <div class="space-y-3">
          {/* Selección */}
          <div class="space-y-1">
            <span class="text-xs text-text-muted">{t("Seleccionar")}</span>
            <div class="grid grid-cols-3 gap-1">
              <Button size="sm" onClick={() => props.onSelect("all")} title={t("Todas las articulaciones visibles (A)")}>
                {t("Todo")}
              </Button>
              <Button size="sm" onClick={() => props.onSelect("none")} title={t("Ninguna (Alt+A)")}>
                {t("Nada")}
              </Button>
              <Button size="sm" onClick={() => props.onSelect("mirror")} disabled={!props.hasSelection} title={t("Las del otro lado (Ctrl+Shift+M)")}>
                {t("Espejo")}
              </Button>
              <Button size="sm" onClick={() => props.onSelect("parent")} disabled={!props.hasSelection} title={t("El padre ([; Shift suma)")}>
                {t("Padre")}
              </Button>
              <Button size="sm" onClick={() => props.onSelect("children")} disabled={!props.hasSelection} title={t("Los hijos (]; Shift: todos los descendientes)")}>
                {t("Hijos")}
              </Button>
              <Button size="sm" onClick={() => props.onSelect("group")} disabled={!props.hasSelection} title={t("Todo su grupo (Shift+G)")}>
                {t("Grupo")}
              </Button>
            </div>
          </div>

          <Select
            label={t("Ejes del gizmo")}
            options={SPACES}
            value={props.space}
            onChange={(v) => props.onSpace(v as TransformSpace)}
          />

          <Show when={props.posing}>
            {/* Reiniciar */}
            <div class="space-y-1">
              <span class="text-xs text-text-muted">
                {t("Volver al reposo")} {props.hasSelection ? t("(la selección)") : t("(todo el esqueleto)")}
              </span>
              <div class="grid grid-cols-3 gap-1">
                <Button size="sm" onClick={() => props.onReset("rotation")} title="Alt+R">
                  {t("Giro")}
                </Button>
                <Button size="sm" onClick={() => props.onReset("translation")} title="Alt+G">
                  {t("Posición")}
                </Button>
                <Button size="sm" onClick={() => props.onReset("all")}>
                  {t("Todo")}
                </Button>
              </div>
            </div>

            {/* Espejo */}
            <div class="space-y-1">
              <span class="text-xs text-text-muted">{t("Espejo")}</span>
              <div class="grid grid-cols-3 gap-1">
                <Button size="sm" onClick={() => props.onMirror("leftToRight")} title={t("Copia el lado izquierdo sobre el derecho")}>
                  {t("Izq → der")}
                </Button>
                <Button size="sm" onClick={() => props.onMirror("rightToLeft")} title={t("Copia el lado derecho sobre el izquierdo")}>
                  {t("Der → izq")}
                </Button>
                <Button size="sm" onClick={() => props.onMirror("flip")} title={t("Cada lado toma la pose del otro")}>
                  {t("Voltear")}
                </Button>
              </div>
            </div>

            {/* Portapapeles */}
            <div class="grid grid-cols-3 gap-1">
              <Button size="sm" onClick={props.onCopy} title={t("Copia la pose de la selección, o de todo (Ctrl+C)")}>
                {t("Copiar")}
              </Button>
              <Button size="sm" onClick={() => props.onPaste(false)} disabled={!props.canPaste} title="Ctrl+V">
                {t("Pegar")}
              </Button>
              <Button size="sm" onClick={() => props.onPaste(true)} disabled={!props.canPaste} title={t("Pegar del otro lado (Ctrl+Shift+V)")}>
                {t("Espejado")}
              </Button>
            </div>

            {/* Intermedias */}
            <LiveSlider
              label={t("Intermedia (key anterior → siguiente)")}
              min={0}
              max={100}
              initial={50}
              format={(v) => `${v} %`}
              resets
              onInput={(v) => props.onBreakdown(v / 100, false)}
              onCommit={(v) => props.onBreakdown(v / 100, true)}
            />
            <LiveSlider
              label={t("Relajar ← → empujar")}
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

      <Panel title="Ragdoll" icon={<Icons.Person size={14} />}>
        <div class="space-y-2">
          <p class="text-xs text-text-muted leading-relaxed">
            {t("El cuerpo cae como un muñeco desde la pose del cuadro actual, con el impulso que traía la animación. Queda en una animación nueva; los límites de giro recortan lo imposible.")}
          </p>
          <div class="grid grid-cols-3 gap-1">
            <For each={[1, 2, 3]}>
              {(seconds) => (
                <Button size="sm" disabled={!props.posing} onClick={() => props.onRagdoll(seconds)}>
                  {seconds} s
                </Button>
              )}
            </For>
          </div>
        </div>
      </Panel>

    </div>
  );
};
