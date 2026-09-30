import { Component, For, Match, Switch, type JSX } from "solid-js";
import { clsx } from "clsx";
import { IconButton } from "../ui";
import * as Icons from "../icons";

export type RigEditorTab = "library" | "joint" | "pose" | "ik" | "rig";

const TABS: { id: RigEditorTab; label: string; title: string }[] = [
  { id: "library", label: "Biblioteca", title: "Animaciones básicas, poses de fábrica y poses guardadas" },
  { id: "joint", label: "Articulación", title: "Límites de giro, trayectoria, cadena y relaciones" },
  { id: "pose", label: "Pose", title: "Selección, espejo, reiniciar, copiar y pegar, intermedias" },
  { id: "ik", label: "IK", title: "Cadenas de IK, mezcla, fijar, rig automático" },
  { id: "rig", label: "Rig", title: "Hueso activo, grupos, controles y reposo" },
];

export interface RigEditorProps {
  tab: RigEditorTab;
  onTab: (tab: RigEditorTab) => void;
  onClose: () => void;
  panels: Record<RigEditorTab, JSX.Element>;
}

/**
 * Editor de pose y rig al costado del visor (como el de texturas): una
 * pestaña por herramienta y sus secciones repartidas en columnas según el
 * ancho que se le dé
 */
export const RigEditor: Component<RigEditorProps> = (props) => (
  <section class="flex flex-col h-full min-w-0 bg-bg-darker">
    <header class="flex items-center gap-1 h-9 px-2 shrink-0 border-b border-border">
      <For each={TABS}>
        {(t) => (
          <button
            title={t.title}
            class={clsx(
              "px-2.5 h-7 rounded text-xs transition-colors",
              props.tab === t.id ? "bg-accent/20 text-accent" : "text-text-muted hover:bg-surface/50 hover:text-text"
            )}
            onClick={() => props.onTab(t.id)}
          >
            {t.label}
          </button>
        )}
      </For>
      <div class="flex-1" />
      <IconButton size="sm" variant="ghost" aria-label="Cerrar el editor" title="Cerrar (Ctrl+E)" onClick={props.onClose}>
        <Icons.X size={14} />
      </IconButton>
    </header>
    <div class="flex-1 min-h-0 overflow-y-auto px-3 py-2">
      {/* Las tarjetas de cada panel fluyen en columnas de ~300 px sin partirse */}
      <div class="[column-width:300px] [column-gap:1rem] [&>div>div]:break-inside-avoid [&>div>div]:mb-3">
        <Switch>
          <Match when={props.tab === "library"}>{props.panels.library}</Match>
          <Match when={props.tab === "joint"}>{props.panels.joint}</Match>
          <Match when={props.tab === "pose"}>{props.panels.pose}</Match>
          <Match when={props.tab === "ik"}>{props.panels.ik}</Match>
          <Match when={props.tab === "rig"}>{props.panels.rig}</Match>
        </Switch>
      </div>
    </div>
  </section>
);
