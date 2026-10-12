import { Show } from "solid-js";
import * as Icons from "../icons";
import { Tooltip } from "./Tooltip";

export interface HintProps {
  /** Explicación que aparece al pasar el mouse */
  text: string;
  /** Texto corto visible junto al ícono (sin él, solo el ícono) */
  label?: string;
  placement?: "top" | "bottom" | "left" | "right";
}

/** Ayuda en un tooltip: reemplaza los párrafos chicos y largos de los paneles angostos */
export const Hint = (props: HintProps) => (
  <Tooltip content={props.text} placement={props.placement ?? "left"}>
    <span class="inline-flex items-center gap-1.5 text-xs text-text-muted cursor-help">
      <Show when={props.label}>{props.label}</Show>
      <Icons.Info size={12} class="text-text-dim hover:text-text shrink-0" />
    </span>
  </Tooltip>
);
