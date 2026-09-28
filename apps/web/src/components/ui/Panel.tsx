import { JSX, ParentComponent, createSignal, splitProps, Show } from "solid-js";
import { clsx } from "clsx";
import { createPersisted } from "../../lib/ui-state";

export interface PanelProps {
  title: string;
  /** Si se da, el panel recuerda si quedó abierto o cerrado */
  id?: string;
  icon?: JSX.Element;
  defaultOpen?: boolean;
  collapsible?: boolean;
  class?: string;
  headerActions?: JSX.Element;
}

export const Panel: ParentComponent<PanelProps> = (props) => {
  const [local, _rest] = splitProps(props, [
    "title",
    "id",
    "icon",
    "defaultOpen",
    "collapsible",
    "class",
    "headerActions",
    "children",
  ]);

  const [isOpen, setIsOpen] = local.id
    ? createPersisted(`panel.${local.id}`, local.defaultOpen ?? true)
    : createSignal(local.defaultOpen ?? true);

  const toggleOpen = () => {
    if (local.collapsible !== false) {
      setIsOpen(!isOpen());
    }
  };

  return (
    <div
      class={clsx(
        "border-b border-border/50",
        local.class
      )}
    >
      {/* Header */}
      <button
        type="button"
        class={clsx(
          "w-full flex items-center gap-2",
          "h-8 px-1",
          "rounded",
          "text-xs font-semibold uppercase tracking-wider text-text-muted",
          "transition-colors duration-100",
          local.collapsible !== false && "hover:bg-current/30 cursor-pointer",
          local.collapsible === false && "cursor-default"
        )}
        onClick={toggleOpen}
      >
        {/* Collapse indicator */}
        <Show when={local.collapsible !== false}>
          <svg
            class={clsx(
              "w-3 h-3 text-text-dim transition-transform duration-100",
              isOpen() && "rotate-90"
            )}
            viewBox="0 0 24 24"
            fill="currentColor"
          >
            <path d="M8.59 16.59L13.17 12 8.59 7.41 10 6l6 6-6 6-1.41-1.41z" />
          </svg>
        </Show>

        {/* Icon */}
        <Show when={local.icon}>
          <span class="text-text-muted">{local.icon}</span>
        </Show>

        {/* Title */}
        <span class="flex-1 text-left">{local.title}</span>

        {/* Actions */}
        <Show when={local.headerActions}>
          <div class="flex items-center gap-1" onClick={(e) => e.stopPropagation()}>
            {local.headerActions}
          </div>
        </Show>
      </button>

      {/* Content */}
      <Show when={isOpen()}>
        <div class="pt-3 pb-4 space-y-4 animate-fade-in">
          {local.children}
        </div>
      </Show>
    </div>
  );
};
