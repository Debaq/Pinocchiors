import { Component, For, Show, createSignal } from "solid-js";
import { clsx } from "clsx";
import { Button, Panel } from "../ui";
import * as Icons from "../icons";
import type { StoredPose } from "../../lib/poseTools";
import type { FactoryPose, PresetAnimation, PresetAnimationId } from "../../lib/presetAnimations";
import { LiveSlider, type PoseSource } from "./PosePanel";

export interface LibraryPanelProps {
  hasSelection: boolean;
  /** Animaciones básicas que admite el esqueleto */
  animations: PresetAnimation[];
  onAddAnimation: (id: PresetAnimationId) => void;
  /** Poses de fábrica que admite el esqueleto */
  factory: FactoryPose[];
  /** Poses guardadas en el proyecto */
  library: StoredPose[];
  onSavePose: (selectionOnly: boolean) => void;
  onDeletePose: (id: string) => void;
  onRenamePose: (id: string, name: string) => void;
  onApplyPose: (source: PoseSource, factor: number, commit: boolean) => void;
}

/**
 * Biblioteca: todo lo que se agrega ya hecho. Animaciones básicas (cada una
 * crea una animación nueva) y poses, de fábrica o guardadas (se aplican en
 * el cuadro actual con mezcla)
 */
export const LibraryPanel: Component<LibraryPanelProps> = (props) => {
  const [chosen, setChosen] = createSignal<PoseSource | null>(null);
  const [blend, setBlend] = createSignal(100);
  const [renaming, setRenaming] = createSignal<string | null>(null);
  const isChosen = (s: PoseSource) => chosen()?.kind === s.kind && chosen()?.id === s.id;

  return (
    <div class="space-y-3">
      <Panel title="Animaciones básicas" icon={<Icons.Play size={14} />} defaultOpen>
        <div class="space-y-2">
          <Show
            when={props.animations.length > 0}
            fallback={<p class="text-xs text-text-dim">No se reconoce el cuerpo del esqueleto: no hay animaciones básicas.</p>}
          >
            <div class="grid grid-cols-2 gap-1">
              <For each={props.animations}>
                {(preset) => (
                  <Button size="sm" title={preset.description} onClick={() => props.onAddAnimation(preset.id)}>
                    {preset.name}
                  </Button>
                )}
              </For>
            </div>
          </Show>
          <p class="text-xs text-text-dim leading-relaxed">
            Cada una se agrega como animación nueva, con keys que puedes ajustar en la línea de tiempo.
          </p>
        </div>
      </Panel>

      <Show when={props.factory.length > 0}>
        <Panel title="Poses de fábrica" icon={<Icons.PersonArmsSpread size={14} />} defaultOpen>
          <div class="flex flex-wrap gap-1">
            <For each={props.factory}>
              {(pose) => (
                <button
                  class={clsx(
                    "px-2 py-1 rounded border text-xs",
                    isChosen({ kind: "factory", id: pose.id })
                      ? "border-accent text-accent bg-accent/10"
                      : "border-border text-text-muted hover:text-text"
                  )}
                  title={pose.description}
                  onClick={() => setChosen({ kind: "factory", id: pose.id })}
                >
                  {pose.name}
                </button>
              )}
            </For>
          </div>
        </Panel>
      </Show>

      <Panel
        title="Mis poses"
        icon={<Icons.Person size={14} />}
        defaultOpen
        headerActions={
          <div class="flex gap-1">
            <Button size="sm" variant="ghost" onClick={() => props.onSavePose(false)} title="Guardar la pose completa">
              <Icons.Plus size={12} />
            </Button>
            <Button size="sm" variant="ghost" onClick={() => props.onSavePose(true)} disabled={!props.hasSelection} title="Guardar solo la selección">
              Selección
            </Button>
          </div>
        }
      >
        <div class="space-y-2">
          <div class="grid grid-cols-3 gap-1.5">
            <For each={props.library}>
              {(pose) => (
                <div
                  class={clsx(
                    "group relative rounded border overflow-hidden cursor-pointer",
                    isChosen({ kind: "stored", id: pose.id }) ? "border-accent" : "border-border hover:border-border-hover"
                  )}
                  title={`${pose.name} · ${Object.keys(pose.bones).length} huesos · doble clic: renombrar`}
                  onClick={() => setChosen({ kind: "stored", id: pose.id })}
                  onDblClick={() => setRenaming(pose.id)}
                >
                  <Show
                    when={pose.thumbnail}
                    fallback={<div class="aspect-square flex items-center justify-center bg-surface/40"><Icons.Person size={20} /></div>}
                  >
                    <img src={pose.thumbnail} class="aspect-square w-full object-cover" alt="" />
                  </Show>
                  <Show
                    when={renaming() === pose.id}
                    fallback={<div class="px-1 py-0.5 text-[10px] text-text-muted truncate">{pose.name}</div>}
                  >
                    <input
                      class="w-full px-1 py-0.5 text-[10px] bg-surface text-text outline-none"
                      value={pose.name}
                      autofocus
                      onClick={(e) => e.stopPropagation()}
                      onKeyDown={(e) => {
                        e.stopPropagation();
                        if (e.key === "Enter") e.currentTarget.blur();
                        if (e.key === "Escape") setRenaming(null);
                      }}
                      onBlur={(e) => {
                        const name = e.currentTarget.value.trim();
                        setRenaming(null);
                        if (name && name !== pose.name) props.onRenamePose(pose.id, name);
                      }}
                    />
                  </Show>
                  <button
                    class="absolute top-0.5 right-0.5 hidden group-hover:flex w-4 h-4 items-center justify-center rounded bg-bg-darker/80 text-text-muted hover:text-red"
                    title="Borrar pose"
                    onClick={(e) => {
                      e.stopPropagation();
                      props.onDeletePose(pose.id);
                    }}
                  >
                    <Icons.X size={10} />
                  </button>
                </div>
              )}
            </For>
          </div>
          <Show when={props.library.length === 0}>
            <p class="text-xs text-text-dim">Guarda poses con + para reusarlas en otros cuadros.</p>
          </Show>
        </div>
      </Panel>

      <Show when={chosen()}>
        {(source) => (
          <Panel title="Aplicar pose" icon={<Icons.Check size={14} />} defaultOpen>
            <div class="space-y-2">
              <LiveSlider
                label="Mezcla"
                min={0}
                max={100}
                initial={blend()}
                format={(v) => `${v} %`}
                onInput={(v) => {
                  setBlend(v);
                  props.onApplyPose(source(), v / 100, false);
                }}
                onCommit={(v) => setBlend(v)}
              />
              <Button size="sm" variant="primary" fullWidth onClick={() => props.onApplyPose(source(), blend() / 100, true)}>
                Aplicar en el cuadro actual
              </Button>
            </div>
          </Panel>
        )}
      </Show>
    </div>
  );
};
