import { Component, For, Show, createMemo, createSignal } from "solid-js";
import { clsx } from "clsx";
import * as Icons from "../icons";
import type { SkeletonPreset } from "../panels/SkeletonPanel";
import { CATEGORIES, byUse, hasUsage, matches } from "../../lib/templateUsage";
import { SHAPE_ICONS } from "./bodyIcons";

export interface TemplatePickerProps {
  /** Plantillas con plan (`plan:<id>`) */
  presets: SkeletonPreset[];
  selected?: string;
  onPick?: (id: string) => void;
  disabled?: boolean;
}

/** Cuántas van en "Más usadas" */
const TOP = 10;

/**
 * Plantillas de partida: buscador, las más usadas arriba y el resto por
 * grupo (mamíferos, aves…). Cada una con el dibujo de su forma base
 */
export const TemplatePicker: Component<TemplatePickerProps> = (props) => {
  const [query, setQuery] = createSignal("");
  const results = createMemo(() => byUse(props.presets.filter((p) => matches(query(), p))));
  const top = createMemo(() => byUse(props.presets).slice(0, TOP));
  const groups = createMemo(() =>
    CATEGORIES.map((c) => ({ ...c, items: props.presets.filter((p) => (p.category ?? "other") === c.id) })).filter((g) => g.items.length > 0)
  );

  const Chip: Component<{ preset: SkeletonPreset }> = (p) => {
    const Icon = () => {
      const I = SHAPE_ICONS[p.preset.shape ?? ""];
      return I ? <I /> : null;
    };
    return (
      <button
        type="button"
        title={`${p.preset.description} · ${p.preset.numBones} huesos`}
        disabled={props.disabled}
        class={clsx(
          "flex items-center gap-1 pl-1 pr-2 h-7 rounded-full border text-[11px] transition-colors",
          "[&_svg]:w-5 [&_svg]:h-5",
          p.preset.id === props.selected
            ? "border-accent bg-accent/15 text-accent"
            : "border-border text-text-muted hover:border-border-hover hover:text-text"
        )}
        onClick={() => props.onPick?.(p.preset.id)}
      >
        <Icon />
        {p.preset.name}
      </button>
    );
  };

  return (
    <div class="space-y-2">
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
            else if (e.key === "Enter" && results().length > 0) props.onPick?.(results()[0].id);
          }}
          placeholder={`Buscar entre ${props.presets.length}: gato, dinosaurio, alas, cola…`}
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

      <div class="max-h-96 overflow-y-auto pr-1 space-y-3">
        <Show
          when={!query()}
          fallback={
            <div class="flex flex-wrap gap-1.5">
              <For each={results()} fallback={<p class="text-xs text-text-muted">Ninguna plantilla coincide con «{query()}»</p>}>
                {(p) => <Chip preset={p} />}
              </For>
            </div>
          }
        >
          <div class="space-y-1.5">
            <div class="text-[10px] font-semibold uppercase tracking-wide text-text-muted">
              {hasUsage() ? "Más usadas" : "Más comunes"}
            </div>
            <div class="flex flex-wrap gap-1.5">
              <For each={top()}>{(p) => <Chip preset={p} />}</For>
            </div>
          </div>
          <For each={groups()}>
            {(g) => (
              <div class="space-y-1.5">
                <div class="text-[10px] font-semibold uppercase tracking-wide text-text-muted">
                  {g.label} <span class="font-mono font-normal">{g.items.length}</span>
                </div>
                <div class="flex flex-wrap gap-1.5">
                  <For each={g.items}>{(p) => <Chip preset={p} />}</For>
                </div>
              </div>
            )}
          </For>
        </Show>
      </div>
    </div>
  );
};
