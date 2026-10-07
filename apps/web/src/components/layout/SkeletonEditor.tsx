import { Component, For, Show, untrack, type JSX } from "solid-js";
import { clsx } from "clsx";
import { IconButton } from "../ui";
import * as Icons from "../icons";

export type SkeletonEditorTab = "create" | "fit" | "weights" | "controls";

export const SKELETON_TABS: { id: SkeletonEditorTab; label: string; title: string; intro: string }[] = [
  {
    id: "create",
    label: "Crear",
    title: "Forma del cuerpo, plantillas y estructura de huesos",
    intro:
      "Arma el esqueleto: elige una forma base o parte de una plantilla y prende o apaga partes. Pasa el mouse sobre cada dibujo para ver ejemplos. Estructura agrega, borra o renombra huesos sueltos.",
  },
  {
    id: "fit",
    label: "Ajustar",
    title: "Ajuste automático al modelo y ajuste fino de cada articulación",
    intro:
      "Ajusta el esqueleto al modelo y corrige cada articulación: elígela con un clic en el visor y muévela, gira el miembro que cuelga de ella o cambia el largo del hueso. Mira desde dos vistas.",
  },
  {
    id: "weights",
    label: "Pesos",
    title: "Qué parte de la malla mueve cada hueso",
    intro:
      "Calcula qué parte de la malla sigue a cada hueso, corrige con el pincel y prueba girando articulaciones.",
  },
  {
    id: "controls",
    label: "Controles",
    title: "Ayudantes del rig: IK, controles, grupos, orientación de huesos y restricciones",
    intro:
      "Ayudantes para animar: cadenas de IK (mueves el pie y la pierna se acomoda), controles que se agarran en el visor, grupos de huesos, la orientación (roll) de cada hueso y restricciones entre ellos. Se usan en Animar.",
  },
];

export interface SkeletonEditorProps {
  tab: SkeletonEditorTab;
  onTab: (tab: SkeletonEditorTab) => void;
  onClose: () => void;
  /** Contenido de cada pestaña: se arma al abrirla */
  panels: Record<SkeletonEditorTab, () => JSX.Element>;
  /** Huesos del esqueleto actual */
  bones?: number;
}

/**
 * Editor de esqueleto al costado del visor (como los de texturas y de pose):
 * crear → ajustar → pesos, más los ayudantes del rig. Las secciones de cada
 * pestaña se reparten en columnas según el ancho que se le dé.
 */
export const SkeletonEditor: Component<SkeletonEditorProps> = (props) => {
  const current = () => SKELETON_TABS.find((t) => t.id === props.tab) ?? SKELETON_TABS[0];
  return (
    <section class="flex flex-col h-full min-w-0 bg-bg-darker">
      <header class="flex items-center gap-1 h-9 px-2 shrink-0 border-b border-border">
        <For each={SKELETON_TABS}>
          {(t) => (
            <button
              title={t.title}
              class={clsx(
                "px-2.5 h-7 rounded text-xs transition-colors whitespace-nowrap",
                props.tab === t.id ? "bg-accent/20 text-accent" : "text-text-muted hover:bg-surface/50 hover:text-text"
              )}
              onClick={() => props.onTab(t.id)}
            >
              {t.label}
            </button>
          )}
        </For>
        <div class="flex-1" />
        <Show when={props.bones}>
          <span class="text-[11px] font-mono text-text-muted whitespace-nowrap px-1">{props.bones} huesos</span>
        </Show>
        <IconButton size="sm" variant="ghost" aria-label="Cerrar el editor" title="Cerrar (Ctrl+E)" onClick={props.onClose}>
          <Icons.X size={14} />
        </IconButton>
      </header>
      <div class="flex-1 min-h-0 overflow-y-auto px-3 py-2">
        <p class="text-xs text-text-muted leading-relaxed mb-2 max-w-3xl">{current().intro}</p>
        {/* Las tarjetas de cada pestaña fluyen en columnas de ~320 px sin partirse */}
        <div class="[column-width:320px] [column-gap:1.25rem] [&>div>div]:break-inside-avoid [&>div>div]:mb-1">
          {(() => {
            const tab = props.tab;
            return untrack(() => props.panels[tab]());
          })()}
        </div>
      </div>
    </section>
  );
};
