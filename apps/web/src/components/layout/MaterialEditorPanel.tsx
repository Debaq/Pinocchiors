import { Component, For, Show } from "solid-js";
import { clsx } from "clsx";
import { IconButton, Select } from "../ui";
import * as Icons from "../icons";
import { MaterialEditor, NodeMaterials, mapsOf, type MaterialActions, type MaterialPatch, type NodePrimitive } from "../panels/MaterialPanel";
import { cssColor, type MaterialInfo } from "../steps/StructureStep";
import { t } from "../../lib/i18n";

export type MaterialEditorTab = "object" | "materials";

const TABS: { id: MaterialEditorTab; label: () => string; title: () => string }[] = [
  { id: "object", label: () => t("Objeto"), title: () => t("Material de la parte del modelo elegida con doble clic") },
  { id: "materials", label: () => t("Materiales"), title: () => t("Todos los materiales del modelo") },
];

export interface MaterialEditorPanelProps {
  tab: MaterialEditorTab;
  onTab: (tab: MaterialEditorTab) => void;
  onClose: () => void;
  /** Nombre del objeto elegido (undefined si no hay) */
  nodeName?: string;
  nodePrimitives: NodePrimitive[];
  /** Con partes elegidas, la lista muestra solo los materiales que usan */
  onlySelected?: boolean;
  actions: MaterialActions;
  /** Materiales de la piel de la retopología y su edición */
  skinMaterials: MaterialInfo[];
  onEditSkin: (index: number, patch: MaterialPatch) => void;
  /** Qué malla se edita; sin retopología no hay donde elegir */
  target: "original" | "quad";
  canChooseTarget: boolean;
  onTarget: (target: "original" | "quad") => void;
}

/**
 * Editor de materiales al costado del visor (como los de texturas y de
 * esqueleto): el material de la parte elegida y la lista de todos. Con la
 * retopología a la vista se editan los materiales de su piel.
 */
export const MaterialEditorPanel: Component<MaterialEditorPanelProps> = (props) => {
  const onSkin = () => props.target === "quad";
  /** Materiales de la lista con su índice: los de la selección, o todos si no hay selección */
  const listed = () => {
    const all = onSkin() ? props.skinMaterials : props.actions.materials;
    const entries = all.map((material, index) => ({ material, index }));
    if (onSkin() || !props.onlySelected) return entries;
    const used = new Set(props.nodePrimitives.flatMap((p) => (p.material === null ? [] : [p.material])));
    return entries.filter((e) => used.has(e.index));
  };
  return (
    <section class="flex flex-col h-full min-w-0 bg-bg-darker">
      <header class="flex items-center gap-1 h-9 px-2 shrink-0 border-b border-border">
        <For each={TABS}>
          {(tab) => (
            <button
              title={tab.title()}
              class={clsx(
                "px-2.5 h-7 rounded text-xs transition-colors whitespace-nowrap",
                props.tab === tab.id ? "bg-accent/20 text-accent" : "text-text-muted hover:bg-surface/50 hover:text-text"
              )}
              onClick={() => props.onTab(tab.id)}
            >
              {tab.label()}
            </button>
          )}
        </For>
        <div class="flex-1" />
        <Show when={props.canChooseTarget}>
          <div class="w-40">
            <Select
              options={[
                { value: "original", label: t("Malla original") },
                { value: "quad", label: t("Retopología") },
              ]}
              value={props.target}
              onChange={(v) => props.onTarget(v as "original" | "quad")}
            />
          </div>
        </Show>
        <IconButton size="sm" variant="ghost" aria-label={t("Cerrar el panel")} title={t("Cerrar")} onClick={props.onClose}>
          <Icons.X size={14} />
        </IconButton>
      </header>
      <div class="flex-1 min-h-0 overflow-y-auto px-3 py-2">
        <Show
          when={props.tab === "materials" || onSkin()}
          fallback={
            <Show
              when={props.nodePrimitives.length > 0}
              fallback={
                <p class="text-xs text-text-muted leading-relaxed">
                  {t("Doble clic sobre una parte del modelo para elegirla y cambiar su material.")}
                </p>
              }
            >
              <div class="space-y-2 max-w-sm">
                <p class="text-xs text-text-muted">{props.nodeName}</p>
                <NodeMaterials primitives={props.nodePrimitives} {...props.actions} />
              </div>
            </Show>
          }
        >
          <div class="[column-width:320px] [column-gap:1.25rem] space-y-0">
            <Show when={props.onlySelected && !onSkin() && listed().length === 0}>
              <p class="text-xs text-text-muted">{t("Las partes elegidas no tienen material: usa la pestaña Objeto para darles uno.")}</p>
            </Show>
            <For each={listed()}>
              {(entry) => (
                <div class="break-inside-avoid mb-4 space-y-2">
                  <div class="flex items-center gap-2">
                    <span class="w-4 h-4 rounded border border-border shrink-0" style={{ background: cssColor(entry.material.base_color) }} />
                    <span class="text-xs text-text font-medium truncate">{entry.material.name || t("material {i}", { i: entry.index })}</span>
                  </div>
                  <MaterialEditor
                    material={entry.material}
                    onChange={(patch) => (onSkin() ? props.onEditSkin(entry.index, patch) : props.actions.onEdit(entry.index, patch))}
                    maps={onSkin() ? undefined : mapsOf(props.actions, entry.index)}
                  />
                  <Show when={!onSkin()}>
                    <button class="text-xs text-text-muted hover:text-text" onClick={() => props.actions.onRemove(entry.index)}>
                      {t("Quitar material")}
                    </button>
                  </Show>
                </div>
              )}
            </For>
            <Show when={!onSkin()}>
              <button class="text-xs text-accent hover:underline" onClick={() => props.actions.onAdd()}>
                {t("Material nuevo")}
              </button>
            </Show>
            <Show when={onSkin() && props.skinMaterials.length === 0}>
              <p class="text-xs text-text-muted">{t("La retopología todavía no tiene piel: despliega y hornea en UV / Piel.")}</p>
            </Show>
          </div>
        </Show>
      </div>
    </section>
  );
};
