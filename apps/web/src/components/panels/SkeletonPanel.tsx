import { Component, For, Show, createMemo, createSignal } from "solid-js";
import { clsx } from "clsx";
import { Panel } from "../ui";
import * as Icons from "../icons";
import { byUse, matches } from "../../lib/templateUsage";

export interface SkeletonPreset {
  id: string;
  name: string;
  description: string;
  numBones: number;
  /** Grupo de la lista (ver `CATEGORIES` en lib/templateUsage) */
  category?: string;
  /** Forma base del plan, para su dibujo */
  shape?: string;
}

export interface SkeletonPanelProps {
  presets: SkeletonPreset[];
  selectedPreset?: string;
  onPresetChange?: (presetId: string) => void;
}


export const SkeletonPanel: Component<SkeletonPanelProps> = (props) => {
  const [query, setQuery] = createSignal("");

  // Las más usadas primero; cada palabra buscada debe aparecer en el nombre o la descripción
  const filtered = createMemo(() => byUse(props.presets.filter((p) => matches(query(), p))));

  const selectedPreset = () =>
    props.presets.find((p) => p.id === props.selectedPreset);

  return (
    <Panel title="Esqueleto" icon={<Icons.Bone size={14} />} defaultOpen>
      <div class="space-y-2">
        {/* Buscador */}
        <div class="relative">
          <Icons.MagnifyingGlass size={13} class="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-muted pointer-events-none" />
          <input
            type="text"
            value={query()}
            onInput={(e) => setQuery(e.currentTarget.value)}
            onKeyDown={(e) => {
              // Los atajos del visor no deben dispararse al escribir
              e.stopPropagation();
              if (e.key === "Escape") setQuery("");
              else if (e.key === "Enter" && filtered().length > 0) props.onPresetChange?.(filtered()[0].id);
            }}
            placeholder={`Buscar entre ${props.presets.length} esqueletos…`}
            class={clsx(
              "w-full h-8 pl-8 pr-7 rounded-md text-sm text-text",
              "bg-bg-lighter border border-border placeholder:text-text-muted/60",
              "hover:border-border-hover focus:border-accent focus:outline-none"
            )}
          />
          <Show when={query()}>
            <button
              class="absolute right-1.5 top-1/2 -translate-y-1/2 w-5 h-5 flex items-center justify-center text-text-muted hover:text-text"
              title="Limpiar búsqueda"
              onClick={() => setQuery("")}
            >
              <Icons.X size={11} />
            </button>
          </Show>
        </div>

        {/* Lista */}
        <div class="max-h-56 overflow-y-auto rounded-md border border-border divide-y divide-border">
          <For
            each={filtered()}
            fallback={<p class="px-3 py-3 text-xs text-text-muted text-center">Ningún esqueleto coincide con «{query()}»</p>}
          >
            {(preset) => (
              <button
                class={clsx(
                  "w-full text-left px-3 py-1.5 flex items-center justify-between gap-2",
                  "transition-colors duration-100",
                  preset.id === props.selectedPreset
                    ? "bg-accent/15 text-accent"
                    : "text-text hover:bg-surface/40"
                )}
                title={preset.description}
                onClick={() => props.onPresetChange?.(preset.id)}
              >
                <span class="text-sm truncate">{preset.name}</span>
                <span class="text-[10px] font-mono text-text-muted shrink-0">{preset.numBones} huesos</span>
              </button>
            )}
          </For>
        </div>

        {/* Selected Info */}
        <Show when={selectedPreset()}>
          <div class="p-2 rounded bg-surface/30 space-y-1">
            <p class="text-xs text-text-muted">
              {selectedPreset()!.description}
            </p>
            <p class="text-xs">
              <span class="text-text-muted">Huesos:</span>{" "}
              <span class="text-accent font-mono">
                {selectedPreset()!.numBones}
              </span>
            </p>
          </div>
        </Show>
      </div>
    </Panel>
  );
};
