import { Component, For } from "solid-js";
import { clsx } from "clsx";
import { IconButton } from "../ui";
import * as Icons from "../icons";
import { SlicerSection, type SlicerTab } from "../panels/SlicerSection";
import { t } from "../../lib/i18n";

type SlicerSectionProps = Parameters<typeof SlicerSection>[0];

export interface SlicerEditorProps extends SlicerSectionProps {
  onClose: () => void;
}

const TABS: { id: SlicerTab; label: string }[] = [
  { id: "settings", label: "Ajustes" },
  { id: "preview", label: "Vista previa" },
];

/** Laminador a la izquierda del visor (como Render y Orizon3D): ajustes y vista previa del gcode */
export const SlicerEditor: Component<SlicerEditorProps> = (props) => (
  <section class="flex flex-col h-full min-w-0 bg-bg-darker">
    <header class="flex items-center gap-1 h-9 px-2 shrink-0 border-b border-border">
      <Icons.Stack size={14} class="mx-1 text-accent" />
      <span class="pr-1.5 text-xs font-semibold text-text">{t("Laminar")}</span>
      <For each={TABS}>
        {(tab) => (
          <button
            class={clsx(
              "px-2.5 h-7 rounded text-xs transition-colors",
              props.tab === tab.id ? "bg-accent/20 text-accent" : "text-text-muted hover:bg-surface/50 hover:text-text"
            )}
            onClick={() => props.onTab(tab.id)}
          >
            {t(tab.label)}
          </button>
        )}
      </For>
      <div class="flex-1" />
      <IconButton size="sm" variant="ghost" aria-label={t("Cerrar el panel")} title={t("Cerrar el panel")} onClick={props.onClose}>
        <Icons.X size={14} />
      </IconButton>
    </header>
    <div class="flex-1 min-h-0">
      <SlicerSection {...props} />
    </div>
  </section>
);
