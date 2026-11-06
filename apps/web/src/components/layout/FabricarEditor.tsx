import { Component, For, Show, createEffect } from "solid-js";
import { clsx } from "clsx";
import { IconButton } from "../ui";
import * as Icons from "../icons";
import { FAB_TABS, PRINT_TABS, Print3DPanel, isPrintTab, type FabTab, type Print3DPanelProps } from "../panels/Print3DPanel";
import { SlicerSection, type PlateInputArgs, type SlicerSettings, type SlicerTab } from "../panels/SlicerSection";
import type { ConfirmRequest } from "./ConfirmDialog";
import { gcodePreview } from "../../lib/gcodePreview";
import { resinPreview, type ResinPrep } from "../../lib/resinPreview";
import { ResinSection } from "../panels/ResinSection";
import { t } from "../../lib/i18n";

/** Lo que necesita el laminador dentro de Fabricar */
export interface FabricarSlicerProps {
  settings: SlicerSettings;
  onSettings: (s: SlicerSettings) => void;
  plateInput: () => PlateInputArgs | undefined;
  plateCount?: number;
  disabled?: boolean;
  confirm: (request: ConfirmRequest) => Promise<boolean>;
  orientAuto: boolean;
  onOrientAuto: (auto: boolean) => void;
}

/** Laminar para resina: si la impresora elegida es de resina, en lugar del de filamento */
export interface FabricarResinProps {
  /** id de la impresora de resina elegida; undefined si es de filamento */
  printerId?: string;
  prep: ResinPrep;
  onPrep: (p: ResinPrep) => void;
}

export interface FabricarEditorProps extends Print3DPanelProps {
  tab: FabTab;
  onTab: (tab: FabTab) => void;
  onClose: () => void;
  slicer: FabricarSlicerProps;
  resin: FabricarResinProps;
}

const toSlicerTab = (tab: FabTab): SlicerTab => (tab === "preview" ? "preview" : tab === "send" ? "printer" : "settings");
const fromSlicerTab = (tab: SlicerTab): FabTab => (tab === "preview" ? "preview" : tab === "printer" ? "send" : "slice");

/**
 * Fabricar a la izquierda del visor: preparar el modelo (impresora, revisar, tamaño, piezas,
 * uniones, relleno, placas, pieles) e imprimirlo (laminar, vista previa, enviar), en un solo
 * panel con la impresora elegida una vez. El panel derecho queda con los datos y los accesos.
 */
export const FabricarEditor: Component<FabricarEditorProps> = (props) => {
  // La vista previa del gcode solo se ve en las pestañas del laminador
  createEffect(() => {
    if (!isPrintTab(props.tab)) {
      gcodePreview.setVisible(false);
      resinPreview.setVisible(false);
    }
  });

  const TabButton = (p: { id: FabTab; label: string; icon: Component<{ size?: number }> }) => (
    <button
      class={clsx(
        "flex items-center gap-1 px-2 h-7 rounded text-xs transition-colors",
        props.tab === p.id ? "bg-orange/20 text-orange" : "text-text-muted hover:bg-surface/50 hover:text-text"
      )}
      onClick={() => props.onTab(p.id)}
      title={t(p.label)}
    >
      <p.icon size={12} />
      {t(p.label)}
    </button>
  );

  return (
    <section class="flex flex-col h-full min-w-0 bg-bg-darker">
      <header class="flex items-center gap-1 h-9 px-2 shrink-0">
        <Icons.Hammer size={14} class="mx-1 text-orange" />
        <span class="text-xs font-semibold text-text">{t("Fabricar")}</span>
        <div class="flex-1" />
        <IconButton size="sm" variant="ghost" aria-label={t("Cerrar el panel")} title={t("Cerrar el panel")} onClick={props.onClose}>
          <Icons.X size={14} />
        </IconButton>
      </header>
      {/* Dos grupos de pestañas; saltan de línea si el panel es angosto */}
      <nav class="px-2 pb-1.5 shrink-0 space-y-1 border-b border-orange/40">
        <div class="flex flex-wrap items-center gap-0.5">
          <span class="w-14 shrink-0 text-[10px] uppercase tracking-wider text-text-dim">{t("Preparar")}</span>
          <For each={FAB_TABS}>{(tab) => <TabButton {...tab} />}</For>
        </div>
        <div class="flex flex-wrap items-center gap-0.5">
          <span class="w-14 shrink-0 text-[10px] uppercase tracking-wider text-text-dim">{t("Imprimir")}</span>
          <For each={PRINT_TABS}>{(tab) => <TabButton {...tab} />}</For>
        </div>
      </nav>
      <div class="relative flex-1 min-h-0">
        <Show when={!isPrintTab(props.tab)}>
          <div class="absolute inset-0 overflow-y-auto px-3 py-3">
            <div class="max-w-[520px]">
              <Print3DPanel {...props} mode="editor" tab={props.tab} onOpenEditor={props.onTab} />
            </div>
          </div>
        </Show>
        {/* El laminador queda montado: cambiar de pestaña no pierde lo laminado ni lo corta a medias */}
        <div class={clsx("absolute inset-0", (!isPrintTab(props.tab) || !props.resin.printerId) && "hidden")}>
          <Show when={props.resin.printerId}>
            {(id) => (
              <ResinSection
                printerId={id()}
                prep={props.resin.prep}
                onPrep={props.resin.onPrep}
                plateInput={props.slicer.plateInput}
                plateCount={props.slicer.plateCount}
                disabled={props.slicer.disabled}
                tab={toSlicerTab(props.tab)}
                onTab={(t) => props.onTab(fromSlicerTab(t))}
                onEditPrinter={() => props.onTab("printer")}
              />
            )}
          </Show>
        </div>
        <div class={clsx("absolute inset-0", (!isPrintTab(props.tab) || props.resin.printerId) && "hidden")}>
          <SlicerSection
            settings={props.slicer.settings}
            onSettings={props.slicer.onSettings}
            plateInput={props.slicer.plateInput}
            plateCount={props.slicer.plateCount}
            disabled={props.slicer.disabled}
            confirm={props.slicer.confirm}
            tab={toSlicerTab(props.tab)}
            onTab={(t) => props.onTab(fromSlicerTab(t))}
            onEditPrinter={() => props.onTab("printer")}
            orientAuto={props.slicer.orientAuto}
            onOrientAuto={props.slicer.onOrientAuto}
            onTune={props.onTune}
          />
        </div>
      </div>
    </section>
  );
};
