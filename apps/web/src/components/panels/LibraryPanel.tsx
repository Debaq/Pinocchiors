import { Component, For, Show, createSignal } from "solid-js";
import { clsx } from "clsx";
import { Button, Hint, Panel } from "../ui";
import * as Icons from "../icons";
import type { StoredPose } from "../../lib/poseTools";
import { parseTags, poseMatches } from "../../lib/poseLibrary";
import type { FactoryPose, PresetAnimation, PresetAnimationId } from "../../lib/presetAnimations";
import { LiveSlider, type PoseSource } from "./PosePanel";
import { t } from "../../lib/i18n";

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
  /** Categoría y etiquetas de una pose (vacías las quitan) */
  onPoseMeta: (id: string, meta: { category?: string; tags: string[] }) => void;
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
  const [query, setQuery] = createSignal("");
  const [category, setCategory] = createSignal<string | null>(null);
  const categories = () => [...new Set(props.library.map((p) => p.category ?? "").filter(Boolean))].sort((a, b) => a.localeCompare(b));
  // Si la categoría elegida ya no existe (se borró la última pose), se vuelve a ver todo
  const activeCategory = () => (category() === null || category() === "" || categories().includes(category()!) ? category() : null);
  const shown = () => props.library.filter((p) => poseMatches(p, query(), activeCategory()));
  const chosenPose = () => {
    const c = chosen();
    return c?.kind === "stored" ? props.library.find((p) => p.id === c.id) : undefined;
  };
  const isChosen = (s: PoseSource) => chosen()?.kind === s.kind && chosen()?.id === s.id;

  return (
    <div class="space-y-3">
      <Panel title={t("Animaciones básicas")} icon={<Icons.Play size={14} />} defaultOpen>
        <div class="space-y-2">
          <Show
            when={props.animations.length > 0}
            fallback={<p class="text-xs text-text-dim">{t("No se reconoce el cuerpo del esqueleto: no hay animaciones básicas.")}</p>}
          >
            <div class="grid grid-cols-2 gap-1">
              <For each={props.animations}>
                {(preset) => (
                  <Button size="sm" title={t(preset.description)} onClick={() => props.onAddAnimation(preset.id)}>
                    {t(preset.name)}
                  </Button>
                )}
              </For>
            </div>
          </Show>
          <div class="flex justify-end">
            <Hint text={t("Cada una se agrega como animación nueva, con keys que puedes ajustar en la línea de tiempo.")} />
          </div>
        </div>
      </Panel>

      <Show when={props.factory.length > 0}>
        <Panel title={t("Poses de fábrica")} icon={<Icons.PersonArmsSpread size={14} />} defaultOpen>
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
                  title={t(pose.description)}
                  onClick={() => setChosen({ kind: "factory", id: pose.id })}
                >
                  {t(pose.name)}
                </button>
              )}
            </For>
          </div>
        </Panel>
      </Show>

      <Panel
        title={t("Mis poses")}
        icon={<Icons.Person size={14} />}
        defaultOpen
        headerActions={
          <div class="flex gap-1">
            <Button size="sm" variant="ghost" onClick={() => props.onSavePose(false)} title={t("Guardar la pose completa")}>
              <Icons.Plus size={12} />
            </Button>
            <Button size="sm" variant="ghost" onClick={() => props.onSavePose(true)} disabled={!props.hasSelection} title={t("Guardar solo la selección")}>
              {t("Selección")}
            </Button>
          </div>
        }
      >
        <div class="space-y-2">
          <Show when={props.library.length > 1}>
            <div class="flex flex-wrap items-center gap-1.5">
              <input
                type="search"
                value={query()}
                placeholder={t("Buscar pose…")}
                aria-label={t("Buscar pose")}
                class="flex-1 min-w-[6rem] px-2 py-1 rounded bg-surface/40 border border-border text-xs text-text outline-none focus:border-accent"
                onInput={(e) => setQuery(e.currentTarget.value)}
              />
              <Show when={categories().length > 0}>
                <select
                  aria-label={t("Categoría")}
                  class="px-1.5 py-1 rounded bg-surface/40 border border-border text-xs text-text outline-none focus:border-accent"
                  value={activeCategory() === null ? "__all" : activeCategory() === "" ? "__none" : activeCategory()!}
                  onChange={(e) => setCategory(e.currentTarget.value === "__all" ? null : e.currentTarget.value === "__none" ? "" : e.currentTarget.value)}
                >
                  <option value="__all">{t("Todas")}</option>
                  <option value="__none">{t("Sin categoría")}</option>
                  <For each={categories()}>{(c) => <option value={c}>{c}</option>}</For>
                </select>
              </Show>
            </div>
          </Show>
          <div class="grid grid-cols-3 gap-1.5">
            <For each={shown()}>
              {(pose) => (
                <div
                  class={clsx(
                    "group relative rounded border overflow-hidden cursor-pointer",
                    isChosen({ kind: "stored", id: pose.id }) ? "border-accent" : "border-border hover:border-border-hover"
                  )}
                  title={t("{name} · {n} huesos · doble clic: renombrar", { name: pose.name, n: Object.keys(pose.bones).length })}
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
                    title={t("Borrar pose")}
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
            <p class="text-xs text-text-dim">{t("Guarda poses con + para reusarlas en otros cuadros.")}</p>
          </Show>
          <Show when={props.library.length > 0 && shown().length === 0}>
            <p class="text-xs text-text-dim">{t("Ninguna pose coincide con la búsqueda.")}</p>
          </Show>
          <Show when={chosenPose()} keyed>
            {(pose) => (
              <div class="space-y-1.5 pt-1 border-t border-border">
                <label class="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs">
                  <span class="text-text-muted">{t("Categoría")}</span>
                  <input
                    list="pose-categories"
                    value={pose.category ?? ""}
                    class="flex-1 min-w-[6rem] px-2 py-1 rounded bg-surface/40 border border-border text-xs text-text outline-none focus:border-accent"
                    onKeyDown={(e) => {
                      e.stopPropagation();
                      if (e.key === "Enter") e.currentTarget.blur();
                    }}
                    onChange={(e) => props.onPoseMeta(pose.id, { category: e.currentTarget.value.trim() || undefined, tags: pose.tags ?? [] })}
                  />
                  <datalist id="pose-categories">
                    <For each={categories()}>{(c) => <option value={c} />}</For>
                  </datalist>
                </label>
                <label class="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs">
                  <span class="text-text-muted">{t("Etiquetas")}</span>
                  <input
                    value={(pose.tags ?? []).join(", ")}
                    placeholder={t("separadas por comas")}
                    class="flex-1 min-w-[6rem] px-2 py-1 rounded bg-surface/40 border border-border text-xs text-text outline-none focus:border-accent"
                    onKeyDown={(e) => {
                      e.stopPropagation();
                      if (e.key === "Enter") e.currentTarget.blur();
                    }}
                    onChange={(e) => props.onPoseMeta(pose.id, { category: pose.category, tags: parseTags(e.currentTarget.value) })}
                  />
                </label>
              </div>
            )}
          </Show>
        </div>
      </Panel>

      <Show when={chosen()}>
        {(source) => (
          <Panel title={t("Aplicar pose")} icon={<Icons.Check size={14} />} defaultOpen>
            <div class="space-y-2">
              <LiveSlider
                label={t("Mezcla")}
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
                {t("Aplicar en el cuadro actual")}
              </Button>
            </div>
          </Panel>
        )}
      </Show>
    </div>
  );
};
