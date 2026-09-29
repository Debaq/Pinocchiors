import { Component } from "solid-js";
import { clsx } from "clsx";

export interface PlaceholderStepProps {
  title: string;
  description: string;
  icon: Component<{ size?: number | string }>;
}

export const PlaceholderStep: Component<PlaceholderStepProps> = (props) => {
  return (
    <div class="flex flex-col items-center justify-center py-6 text-center gap-5">
      {/* Icon */}
      <div
        class={clsx(
          "w-14 h-14 rounded-full",
          "bg-surface/30 border border-border",
          "flex items-center justify-center",
          "text-text-muted"
        )}
      >
        <props.icon size={28} />
      </div>

      {/* Title */}
      <h3 class="text-sm font-semibold text-text">{props.title}</h3>

      {/* Badge */}
      <span
        class={clsx(
          "px-2.5 py-1 rounded-full",
          "text-[10px] font-semibold uppercase tracking-wider",
          "bg-orange/15 text-orange border border-orange/30"
        )}
      >
        Proximamente
      </span>

      {/* Description */}
      <p class="text-xs text-text-muted leading-relaxed max-w-[220px]">
        {props.description}
      </p>
    </div>
  );
};
