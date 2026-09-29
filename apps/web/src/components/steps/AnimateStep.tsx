import { Component, For, Show, type JSX } from "solid-js";
import { clsx } from "clsx";
import { Button } from "../ui";
import * as Icons from "../icons";
import type { AnimationClip } from "../../lib/animation";
import type { PresetAnimation, PresetAnimationId } from "../../lib/presetAnimations";

export interface AnimateStepProps {
  /** Hay esqueleto con pesos sobre la malla que se ve */
  ready: boolean;
  clips: AnimationClip[];
  activeClipId?: string;
  onSelectClip?: (id: string) => void;
  onNewClip?: () => void;
  onDuplicateClip?: () => void;
  onDeleteClip?: () => void;
  onRenameClip?: (name: string) => void;
  /** Animaciones básicas que admite el esqueleto */
  presets?: PresetAnimation[];
  onAddPreset?: (id: PresetAnimationId) => void;
  selectedBoneName?: string;
  /** Propiedades del rig (hueso activo, grupos, controles, reposo) */
  rigPanel?: JSX.Element;
}

const SHORTCUTS: [string, string][] = [
  ["Clic", "Seleccionar articulación"],
  ["Shift+clic", "Sumar a la selección"],
  ["Doble clic", "Seleccionar la cadena"],
  ["R", "Girar (X/Y/Z eje; dos veces, local)"],
  ["G", "Desplazar"],
  ["Ctrl al girar", "Pasos de 5° (con Shift, 15°)"],
  ["Número al girar", "Grados exactos"],
  ["I", "Insertar key"],
  ["X / Supr", "Borrar keys seleccionadas"],
  ["Espacio", "Reproducir / pausar"],
  ["← / →", "Cuadro anterior / siguiente"],
];

export const AnimateStep: Component<AnimateStepProps> = (props) => {
  const active = () => props.clips.find((c) => c.id === props.activeClipId);

  return (
    <div class="space-y-5">
      <h3 class="text-sm font-semibold text-text">Animar</h3>
      <Show
        when={props.ready}
        fallback={
          <p class="text-xs text-text-muted leading-relaxed">
            Para animar hace falta el esqueleto con sus pesos: calcúlalos en el paso Esqueleto.
          </p>
        }
      >
        <p class="text-xs text-text-muted leading-relaxed">
          Elige una articulación, gírala con R (o desplázala con G) e inserta keys en la línea de tiempo. Con
          auto-key, cada cambio confirmado crea su key.
        </p>

        {/* Animaciones */}
        <div class="space-y-2">
          <div class="flex items-center justify-between">
            <span class="text-xs font-semibold text-text">Animaciones</span>
            <div class="flex gap-1">
              <Button size="sm" variant="ghost" onClick={props.onNewClip}>
                <span class="flex items-center gap-1"><Icons.Plus size={12} /> Nueva</span>
              </Button>
              <Button size="sm" variant="ghost" onClick={props.onDuplicateClip} disabled={!active()}>
                Duplicar
              </Button>
            </div>
          </div>
          <div class="rounded border border-border divide-y divide-border/60">
            <For each={props.clips} fallback={<p class="px-2 py-2 text-xs text-text-dim">Sin animaciones.</p>}>
              {(clip) => (
                <button
                  class={clsx(
                    "w-full flex items-center justify-between px-2 py-1.5 text-xs text-left",
                    clip.id === props.activeClipId ? "bg-accent/15 text-accent" : "text-text-muted hover:text-text"
                  )}
                  onClick={() => props.onSelectClip?.(clip.id)}
                >
                  <span class="truncate">{clip.name}</span>
                  <span class="font-mono text-text-dim">
                    {clip.end - clip.start + 1} c · {clip.tracks.length} art.
                  </span>
                </button>
              )}
            </For>
          </div>
          <Show when={active()}>
            {(clip) => (
              <div class="flex items-center gap-2">
                <input
                  class="flex-1 min-w-0 px-2 py-1 rounded bg-surface/40 border border-border text-xs text-text outline-none focus:border-accent"
                  value={clip().name}
                  onChange={(e) => props.onRenameClip?.(e.currentTarget.value.trim() || clip().name)}
                />
                <Button size="sm" variant="ghost" onClick={props.onDeleteClip}>
                  Borrar
                </Button>
              </div>
            )}
          </Show>
        </div>

        {/* Animaciones básicas generadas a partir del esqueleto */}
        <Show when={(props.presets?.length ?? 0) > 0}>
          <div class="space-y-2">
            <span class="text-xs font-semibold text-text">Animaciones básicas</span>
            <div class="grid grid-cols-2 gap-1">
              <For each={props.presets}>
                {(preset) => (
                  <Button size="sm" title={preset.description} onClick={() => props.onAddPreset?.(preset.id)}>
                    {preset.name}
                  </Button>
                )}
              </For>
            </div>
            <p class="text-xs text-text-dim leading-relaxed">
              Cada una se agrega como animación nueva, con keys que puedes ajustar en la línea de tiempo.
            </p>
          </div>
        </Show>

        <div class="text-xs">
          <span class="text-text-muted">Articulación: </span>
          <span class="text-text">{props.selectedBoneName ?? "ninguna (clic en una esfera)"}</span>
        </div>

        {props.rigPanel}


        <div class="space-y-1">
          <For each={SHORTCUTS}>
            {([keys, description]) => (
              <div class="flex justify-between text-xs">
                <span class="font-mono text-text">{keys}</span>
                <span class="text-text-muted">{description}</span>
              </div>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
};
