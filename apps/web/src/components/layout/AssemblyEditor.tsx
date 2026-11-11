import { Component, For } from "solid-js";
import { clsx } from "clsx";
import { IconButton } from "../ui";
import * as Icons from "../icons";
import { AssemblyPanel, type AssemblyTab } from "../steps/DesignStep";
import type { CadStore } from "../../lib/cad";
import type { CadUi } from "../../lib/cadUi";
import { t } from "../../lib/i18n";

const TABS: { id: AssemblyTab; label: () => string; title: () => string }[] = [
  { id: "parts", label: () => t("Piezas"), title: () => t("Instancias del ensamble: insertar, mover, girar, repetir y lista de materiales") },
  { id: "relations", label: () => t("Relaciones"), title: () => t("Uniones entre piezas, acoplamientos y revisión de choques") },
  { id: "motion", label: () => t("Movimiento"), title: () => t("Grupos, animación y vista explosionada") },
  { id: "fasten", label: () => t("Tornillos"), title: () => t("Tornillería del ensamble") },
];

/** Editor del ensamble a la izquierda del visor (como la biblioteca y los editores de otras áreas) */
export const AssemblyEditor: Component<{ store: CadStore; ui: CadUi; onClose: () => void }> = (props) => (
  <section class="flex flex-col h-full min-w-0 bg-bg-darker" data-assembly-editor>
    <header class="flex items-center gap-1 h-9 px-2 shrink-0 border-b border-border">
      <For each={TABS}>
        {(tab) => (
          <button
            title={tab.title()}
            class={clsx(
              "px-2.5 h-7 rounded text-xs transition-colors whitespace-nowrap",
              props.ui.asmEditorTab() === tab.id ? "bg-accent/20 text-accent" : "text-text-muted hover:bg-surface/50 hover:text-text",
            )}
            onClick={() => props.ui.setAsmEditorTab(tab.id)}
          >
            {tab.label()}
          </button>
        )}
      </For>
      <div class="flex-1" />
      <IconButton size="sm" variant="ghost" aria-label={t("Cerrar el panel")} title={t("Cerrar")} onClick={props.onClose}>
        <Icons.X size={14} />
      </IconButton>
    </header>
    <div class="flex-1 min-h-0 overflow-y-auto p-3">
      <AssemblyPanel store={props.store} ui={props.ui} tab={props.ui.asmEditorTab()} />
    </div>
  </section>
);
