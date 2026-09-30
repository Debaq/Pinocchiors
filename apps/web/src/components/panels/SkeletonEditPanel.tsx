import { Component, For, Show, createEffect, createSignal } from "solid-js";
import { Button, Panel } from "../ui";
import * as Icons from "../icons";

export interface CustomSkeletonInfo {
  id: string;
  name: string;
  bones: number;
}

export interface SkeletonEditPanelProps {
  /** Articulación elegida (la que recibe el hijo, se borra o se renombra) */
  selectedName?: string;
  disabled?: boolean;
  onAddChild: () => void;
  onDelete: () => void;
  onRename: (name: string) => void;
  onExportJson: () => void;
  onImportJson: () => void;
  custom: CustomSkeletonInfo[];
  onSaveCustom: (name: string) => void;
  onApplyCustom: (id: string) => void;
  onDeleteCustom: (id: string) => void;
}

/** Estructura del esqueleto: huesos nuevos o borrados, nombres, JSON y esqueletos propios */
export const SkeletonEditPanel: Component<SkeletonEditPanelProps> = (props) => {
  const [name, setName] = createSignal("");
  const [saveName, setSaveName] = createSignal("");
  createEffect(() => setName(props.selectedName ?? ""));

  return (
    <Panel title="Estructura" icon={<Icons.TreeStructure size={14} />}>
      <div class="space-y-3">
        <p class="text-xs text-text-muted leading-relaxed">
          Agrega o borra huesos donde la plantilla no alcanza (una cola más larga, un cuerno). Cambiar la estructura
          descarta los pesos; las animaciones siguen por nombre.
        </p>
        <Show when={props.selectedName} fallback={<p class="text-xs text-text-dim">Elige una articulación en el visor.</p>}>
          <div class="flex gap-1">
            <input
              class="flex-1 min-w-0 px-2 py-1 rounded bg-surface/40 border border-border text-xs font-mono text-text outline-none focus:border-accent"
              value={name()}
              onInput={(e) => setName(e.currentTarget.value)}
              onKeyDown={(e) => {
                e.stopPropagation();
                if (e.key === "Enter" && name().trim() && name().trim() !== props.selectedName) props.onRename(name().trim());
              }}
            />
            <Button
              size="sm"
              disabled={props.disabled || !name().trim() || name().trim() === props.selectedName}
              onClick={() => props.onRename(name().trim())}
            >
              Renombrar
            </Button>
          </div>
          <div class="grid grid-cols-2 gap-1">
            <Button size="sm" disabled={props.disabled} onClick={props.onAddChild} title="Hueso nuevo que cuelga de la articulación elegida">
              <Icons.Plus size={12} /> Hueso hijo
            </Button>
            <Button size="sm" variant="danger" disabled={props.disabled} onClick={props.onDelete} title="Sus hijos pasan a colgar de su padre">
              <Icons.Trash size={12} /> Borrar
            </Button>
          </div>
        </Show>

        <div class="grid grid-cols-2 gap-1">
          <Button size="sm" disabled={props.disabled} onClick={props.onExportJson}>
            <Icons.Export size={12} /> Exportar JSON
          </Button>
          <Button size="sm" disabled={props.disabled} onClick={props.onImportJson}>
            <Icons.Download size={12} /> Importar JSON
          </Button>
        </div>

        <div class="space-y-1 pt-1 border-t border-border/60">
          <span class="text-xs text-text-muted">Mis esqueletos</span>
          <div class="flex gap-1">
            <input
              class="flex-1 min-w-0 px-2 py-1 rounded bg-surface/40 border border-border text-xs text-text outline-none focus:border-accent"
              placeholder="Nombre"
              value={saveName()}
              onInput={(e) => setSaveName(e.currentTarget.value)}
              onKeyDown={(e) => e.stopPropagation()}
            />
            <Button
              size="sm"
              disabled={props.disabled || !saveName().trim()}
              onClick={() => {
                props.onSaveCustom(saveName().trim());
                setSaveName("");
              }}
              title="Guarda este esqueleto como plantilla propia (para otros modelos)"
            >
              <Icons.FloppyDisk size={12} /> Guardar
            </Button>
          </div>
          <div class="rounded border border-border divide-y divide-border/60 max-h-40 overflow-y-auto">
            <For each={props.custom} fallback={<p class="px-2 py-2 text-xs text-text-dim">Sin esqueletos guardados.</p>}>
              {(s) => (
                <div class="flex items-center gap-2 px-2 py-1 text-xs">
                  <button class="flex-1 min-w-0 text-left truncate text-text-muted hover:text-text" disabled={props.disabled} onClick={() => props.onApplyCustom(s.id)}>
                    {s.name}
                  </button>
                  <span class="text-text-dim font-mono">{s.bones}</span>
                  <button class="text-text-dim hover:text-red" aria-label={`Borrar ${s.name}`} onClick={() => props.onDeleteCustom(s.id)}>
                    <Icons.X size={12} />
                  </button>
                </div>
              )}
            </For>
          </div>
        </div>
      </div>
    </Panel>
  );
};
