import { Component, Show } from "solid-js";
import { Dynamic } from "solid-js/web";
import { clsx } from "clsx";
import type { StepStatus } from "../../lib/pipeline";
import * as Icons from "../icons";

export interface PipelineNodeProps {
  icon: Component<{ size?: number | string }>;
  label: string;
  status: StepStatus;
  onClick?: () => void;
}

export const PipelineNode: Component<PipelineNodeProps> = (props) => {
  return (
    <button
      class={clsx(
        "flex flex-col items-center gap-1.5 relative",
        "min-w-[3.5rem] px-1",
        "transition-all duration-200",
        props.status === "locked" && "opacity-40 cursor-not-allowed",
        props.status === "available" && "cursor-pointer",
        props.status === "active" && "cursor-pointer",
        props.status === "completed" && "cursor-pointer"
      )}
      onClick={() => props.status !== "locked" && props.onClick?.()}
      disabled={props.status === "locked"}
    >
      {/* Circle */}
      <div
        class={clsx(
          "w-9 h-9 rounded-full flex items-center justify-center",
          "border-2 transition-all duration-200 relative",
          props.status === "locked" && "border-border bg-bg-darker text-text-dim",
          props.status === "available" && "border-border bg-bg-darker text-text-muted hover:border-border-hover hover:text-text",
          props.status === "active" && "border-accent bg-accent/20 text-accent pipeline-glow",
          props.status === "completed" && "border-green bg-green/20 text-green"
        )}
      >
        {/* Lock overlay for locked state */}
        <Show when={props.status === "locked"}>
          <Icons.Lock size={14} />
        </Show>

        {/* Check overlay for completed state */}
        <Show when={props.status === "completed"}>
          <Icons.Check size={16} />
        </Show>

        {/* Icon for available/active states */}
        <Show when={props.status === "available" || props.status === "active"}>
          <Dynamic component={props.icon} size={16} />
        </Show>
      </div>

      {/* Label */}
      <span
        class={clsx(
          "text-[10px] leading-tight font-medium whitespace-nowrap",
          props.status === "locked" && "text-text-dim",
          props.status === "available" && "text-text-muted",
          props.status === "active" && "text-accent",
          props.status === "completed" && "text-green"
        )}
      >
        {props.label}
      </span>
    </button>
  );
};
