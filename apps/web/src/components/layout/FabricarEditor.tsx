import { Component, For } from "solid-js";
import { clsx } from "clsx";
import { IconButton } from "../ui";
import * as Icons from "../icons";
import { FAB_TABS, Print3DPanel, type FabTab, type Print3DPanelProps } from "../panels/Print3DPanel";
import { t } from "../../lib/i18n";

export interface FabricarEditorProps extends Print3DPanelProps {
  tab: FabTab;
  onTab: (tab: FabTab) => void;
  onClose: () => void;
}

/**
 * Herramientas de Fabricar a la izquierda del visor, en pestañas (revisar, tamaño, piezas,
 * uniones, relleno, placas, pieles). El panel derecho queda con los datos, la impresora y
 * las acciones básicas. Se distingue del laminador por el ícono y el acento naranja.
 */
export const FabricarEditor: Component<FabricarEditorProps> = (props) => (
  <section class="flex flex-col h-full min-w-0 bg-bg-darker">
    <header class="flex items-center gap-1 h-9 px-2 shrink-0">
      <Icons.Hammer size={14} class="mx-1 text-orange" />
      <span class="text-xs font-semibold text-text">{t("Fabricar")}</span>
      <div class="flex-1" />
      <IconButton size="sm" variant="ghost" aria-label={t("Cerrar el panel")} title={t("Cerrar el panel")} onClick={props.onClose}>
        <Icons.X size={14} />
      </IconButton>
    </header>
    {/* Las pestañas saltan de línea si el panel es angosto */}
    <nav class="flex flex-wrap gap-0.5 px-2 pb-1.5 shrink-0 border-b border-orange/40">
      <For each={FAB_TABS}>
        {(tab) => (
          <button
            class={clsx(
              "flex items-center gap-1 px-2 h-7 rounded text-xs transition-colors",
              props.tab === tab.id ? "bg-orange/20 text-orange" : "text-text-muted hover:bg-surface/50 hover:text-text"
            )}
            onClick={() => props.onTab(tab.id)}
            title={t(tab.label)}
          >
            <tab.icon size={12} />
            {t(tab.label)}
          </button>
        )}
      </For>
    </nav>
    <div class="flex-1 min-h-0 overflow-y-auto px-3 py-3">
      <div class="max-w-[520px]">
        <Print3DPanel {...props} mode="editor" tab={props.tab} onOpenEditor={props.onTab} />
      </div>
    </div>
  </section>
);
