import { Tooltip as KobalteTooltip } from "@kobalte/core/tooltip";
import { ParentComponent } from "solid-js";
import { clsx } from "clsx";

export interface TooltipProps {
  content: string;
  placement?: "top" | "bottom" | "left" | "right";
  delay?: number;
}

export const Tooltip: ParentComponent<TooltipProps> = (props) => {
  return (
    <KobalteTooltip
      placement={props.placement ?? "top"}
      openDelay={props.delay ?? 400}
      closeDelay={100}
    >
      <KobalteTooltip.Trigger as="span" class="inline-flex">
        {props.children}
      </KobalteTooltip.Trigger>

      <KobalteTooltip.Portal>
        <KobalteTooltip.Content
          class={clsx(
            "px-2 py-1 rounded",
            "bg-fg text-bg",
            "text-xs font-medium",
            "shadow-md",
            "animate-fade-in",
            "z-50"
          )}
        >
          <KobalteTooltip.Arrow />
          {props.content}
        </KobalteTooltip.Content>
      </KobalteTooltip.Portal>
    </KobalteTooltip>
  );
};
