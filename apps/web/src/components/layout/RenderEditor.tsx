import { Component } from "solid-js";
import { IconButton } from "../ui";
import * as Icons from "../icons";
import { RenderOptions, type RenderOptionsProps } from "../steps/RenderStep";
import { t } from "../../lib/i18n";

export interface RenderEditorProps extends RenderOptionsProps {
  onClose: () => void;
}

/** Opciones del render a la izquierda del visor (como los editores de texturas, esqueleto y Orizon3D) */
export const RenderEditor: Component<RenderEditorProps> = (props) => (
  <section class="flex flex-col h-full min-w-0 bg-bg-darker">
    <header class="flex items-center gap-2 h-9 px-3 shrink-0 border-b border-border">
      <Icons.FrameCorners size={14} class="text-accent" />
      <span class="text-xs font-semibold text-text">{t("Render")}</span>
      <div class="flex-1" />
      <IconButton size="sm" variant="ghost" aria-label={t("Cerrar el editor")} title={t("Cerrar")} onClick={props.onClose}>
        <Icons.X size={14} />
      </IconButton>
    </header>
    <div class="flex-1 min-h-0 overflow-y-auto px-3 py-3">
      <RenderOptions {...props} />
    </div>
  </section>
);
