import { Component, For, Show, createMemo, createSignal } from "solid-js";
import { clsx } from "clsx";
import * as Icons from "../icons";
import type { SkeletonPreset } from "../panels/SkeletonPanel";
import { CATEGORIES, byUse, matches, mostUsed, templateFavorites, toggleFavorite } from "../../lib/templateUsage";
import { thumbnail, type TemplateShape } from "../../lib/skeletonThumb";

export interface TemplateGalleryProps {
  presets: SkeletonPreset[];
  /** Huesos de cada plantilla, para dibujarlas (llegan después de la lista) */
  shapes: Map<string, TemplateShape>;
  selected?: string;
  onPick: (id: string) => void;
  /** Seguir a la pestaña Personalizar con la plantilla elegida */
  onCustomize?: () => void;
  disabled?: boolean;
}

/** Cuántas van en "Más usadas" */
const TOP = 4;
const THUMB_W = 120;
const THUMB_H = 84;

/** Estrella de favorita (rellena si está marcada) */
const Star: Component<{ on: boolean }> = (props) => (
  <svg viewBox="0 0 24 24" width="14" height="14" fill={props.on ? "currentColor" : "none"} stroke="currentColor" stroke-width="1.8" stroke-linejoin="round">
    <path d="M12 3.5l2.6 5.3 5.9.9-4.3 4.1 1 5.8L12 16.9l-5.2 2.7 1-5.8-4.3-4.1 5.9-.9z" />
  </svg>
);

/** Dibujo del esqueleto de la plantilla */
const Thumbnail: Component<{ shape?: TemplateShape }> = (props) => {
  const thumb = createMemo(() => (props.shape ? thumbnail(props.shape, THUMB_W, THUMB_H) : undefined));
  return (
    <svg viewBox={`0 0 ${THUMB_W} ${THUMB_H}`} class="w-full h-auto" aria-hidden="true">
      <Show when={thumb()} fallback={<rect x="40" y="30" width="40" height="24" rx="4" fill="currentColor" opacity="0.08" />}>
        {(t) => (
          <g stroke-linecap="round">
            <For each={t().segments}>
              {(s) => (
                <line
                  x1={s.x1}
                  y1={s.y1}
                  x2={s.x2}
                  y2={s.y2}
                  stroke="currentColor"
                  stroke-width={2.2 - 0.9 * s.depth}
                  opacity={1 - 0.6 * s.depth}
                />
              )}
            </For>
            <For each={t().tips}>{(p) => <circle cx={p.x} cy={p.y} r="1.6" fill="currentColor" opacity={1 - 0.6 * p.depth} />}</For>
          </g>
        )}
      </Show>
    </svg>
  );
};

/**
 * Galería de plantillas (pestaña Plantillas del editor de esqueleto): cada
 * una con el dibujo de su esqueleto, buscador, filtro por grupo, favoritas
 * y las más usadas arriba
 */
export const TemplateGallery: Component<TemplateGalleryProps> = (props) => {
  const [query, setQuery] = createSignal("");
  /** "all", "favorites" o el id de un grupo */
  const [filter, setFilter] = createSignal("all");

  const favorites = createMemo(() => props.presets.filter((p) => templateFavorites().includes(p.id)));
  const filtered = createMemo(() => {
    const f = filter();
    const pool =
      f === "all" ? props.presets : f === "favorites" ? favorites() : props.presets.filter((p) => (p.category ?? "other") === f);
    return byUse(pool.filter((p) => matches(query(), p)));
  });
  /** Sin búsqueda ni filtro: favoritas, más usadas y los grupos */
  const browsing = () => !query() && filter() === "all";
  const top = createMemo(() => mostUsed(props.presets, TOP));
  const groups = createMemo(() =>
    CATEGORIES.map((c) => ({ ...c, items: props.presets.filter((p) => (p.category ?? "other") === c.id) })).filter((g) => g.items.length > 0)
  );
  const selectedName = () => props.presets.find((p) => p.id === props.selected)?.name;

  const Card: Component<{ preset: SkeletonPreset }> = (p) => {
    const active = () => p.preset.id === props.selected;
    const favorite = () => templateFavorites().includes(p.preset.id);
    return (
      <div
        class={clsx(
          "relative rounded-lg border transition-colors",
          active() ? "border-accent bg-accent/10" : "border-border hover:border-border-hover hover:bg-surface/30",
          props.disabled && "opacity-50 pointer-events-none"
        )}
      >
        <button
          type="button"
          class="w-full text-left p-2 space-y-1"
          title={p.preset.description}
          onClick={() => props.onPick(p.preset.id)}
        >
          <div class={clsx("rounded bg-bg/40 px-1", active() ? "text-accent" : "text-text-muted")}>
            <Thumbnail shape={props.shapes.get(p.preset.id)} />
          </div>
          <div class={clsx("text-xs font-medium leading-tight", active() ? "text-accent" : "text-text")}>{p.preset.name}</div>
          <div class="text-[10px] leading-snug text-text-muted line-clamp-2">{p.preset.description}</div>
          <div class="text-[10px] font-mono text-text-dim">{p.preset.numBones} huesos</div>
        </button>
        <button
          type="button"
          class={clsx(
            "absolute top-1.5 right-1.5 w-6 h-6 rounded flex items-center justify-center transition-colors",
            favorite() ? "text-warning" : "text-text-dim hover:text-text"
          )}
          title={favorite() ? "Quitar de favoritas" : "Marcar como favorita"}
          aria-pressed={favorite()}
          onClick={() => toggleFavorite(p.preset.id)}
        >
          <Star on={favorite()} />
        </button>
      </div>
    );
  };

  const Grid: Component<{ items: SkeletonPreset[] }> = (g) => (
    <div class="grid gap-2 grid-cols-[repeat(auto-fill,minmax(132px,1fr))]">
      <For each={g.items}>{(p) => <Card preset={p} />}</For>
    </div>
  );

  const Heading: Component<{ label: string; count?: number }> = (h) => (
    <div class="text-[10px] font-semibold uppercase tracking-wide text-text-muted pt-1">
      {h.label}
      <Show when={h.count !== undefined}>
        <span class="font-mono font-normal ml-1.5">{h.count}</span>
      </Show>
    </div>
  );

  const FilterChip: Component<{ id: string; label: string; count: number }> = (c) => (
    <button
      type="button"
      class={clsx(
        "px-2.5 h-6 rounded-full border text-[11px] whitespace-nowrap transition-colors",
        filter() === c.id ? "border-accent bg-accent/15 text-accent" : "border-border text-text-muted hover:border-border-hover hover:text-text"
      )}
      onClick={() => setFilter(c.id)}
    >
      {c.label} <span class="font-mono opacity-70">{c.count}</span>
    </button>
  );

  return (
    <div class="space-y-3">
      {/* Buscador y elegida */}
      <div class="flex flex-wrap items-center gap-2">
        <div class="relative flex-1 min-w-[200px]">
          <Icons.MagnifyingGlass size={13} class="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-muted pointer-events-none" />
          <input
            type="text"
            value={query()}
            onInput={(e) => setQuery(e.currentTarget.value)}
            onKeyDown={(e) => {
              // Los atajos del visor no deben dispararse al escribir
              e.stopPropagation();
              if (e.key === "Escape") setQuery("");
              else if (e.key === "Enter" && filtered().length > 0) props.onPick(filtered()[0].id);
            }}
            placeholder={`Buscar entre ${props.presets.length}: gato, dinosaurio, alas, cuernos, cola…`}
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
        <Show when={selectedName() && props.onCustomize}>
          <button
            type="button"
            class="h-8 px-3 rounded-md bg-accent text-bg text-xs font-medium flex items-center gap-1 hover:opacity-90"
            title="Cambiar partes de la plantilla elegida"
            onClick={() => props.onCustomize?.()}
          >
            Personalizar {selectedName()}
            <Icons.CaretRight size={12} />
          </button>
        </Show>
      </div>

      {/* Filtros */}
      <div class="flex flex-wrap gap-1.5">
        <FilterChip id="all" label="Todas" count={props.presets.length} />
        <Show when={favorites().length > 0}>
          <FilterChip id="favorites" label="Favoritas" count={favorites().length} />
        </Show>
        <For each={groups()}>{(g) => <FilterChip id={g.id} label={g.label} count={g.items.length} />}</For>
      </div>

      <Show
        when={browsing()}
        fallback={
          <Show when={filtered().length > 0} fallback={<p class="text-xs text-text-muted py-4">Ninguna plantilla coincide con «{query()}».</p>}>
            <Grid items={filtered()} />
          </Show>
        }
      >
        <Show when={favorites().length > 0}>
          <Heading label="Favoritas" count={favorites().length} />
          <Grid items={byUse(favorites())} />
        </Show>
        <Show when={top().length > 0}>
          <Heading label="Más usadas" />
          <Grid items={top()} />
        </Show>
        <For each={groups()}>
          {(g) => (
            <>
              <Heading label={g.label} count={g.items.length} />
              <Grid items={g.items} />
            </>
          )}
        </For>
      </Show>
    </div>
  );
};
