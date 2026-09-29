import { ParentComponent, Show, createSignal } from "solid-js";
import { clsx } from "clsx";
import { Tooltip } from "../ui";
import * as Icons from "../icons";

export type SidebarPosition = "left" | "right";

export interface SidebarProps {
  position: SidebarPosition;
  title?: string;
  collapsible?: boolean;
  defaultCollapsed?: boolean;
  minWidth?: number;
  maxWidth?: number;
}

export const Sidebar: ParentComponent<SidebarProps> = (props) => {
  const [collapsed, setCollapsed] = createSignal(props.defaultCollapsed ?? false);
  const [width, setWidth] = createSignal(props.minWidth ?? 260);
  const [isResizing, setIsResizing] = createSignal(false);

  const minWidth = () => props.minWidth ?? 200;
  const maxWidth = () => props.maxWidth ?? 400;

  const handleResizeStart = (e: MouseEvent) => {
    e.preventDefault();
    setIsResizing(true);

    const startX = e.clientX;
    const startWidth = width();

    const handleMouseMove = (moveEvent: MouseEvent) => {
      const delta = props.position === "left"
        ? moveEvent.clientX - startX
        : startX - moveEvent.clientX;

      const newWidth = Math.min(maxWidth(), Math.max(minWidth(), startWidth + delta));
      setWidth(newWidth);
    };

    const handleMouseUp = () => {
      setIsResizing(false);
      document.removeEventListener("mousemove", handleMouseMove);
      document.removeEventListener("mouseup", handleMouseUp);
    };

    document.addEventListener("mousemove", handleMouseMove);
    document.addEventListener("mouseup", handleMouseUp);
  };

  return (
    <aside
      class={clsx(
        "relative flex flex-col",
        "bg-bg-darker",
        "border-border",
        props.position === "left" ? "border-r" : "border-l",
        "transition-[width] duration-200",
        collapsed() && "!w-0 !border-0 overflow-hidden"
      )}
      style={{ width: collapsed() ? "0" : `${width()}px` }}
    >
      {/* Content */}
      <div class="flex-1 overflow-y-auto overflow-x-hidden">
        {props.children}
      </div>

      {/* Resize Handle */}
      <Show when={!collapsed() && props.collapsible !== false}>
        <div
          class={clsx(
            "absolute top-0 bottom-0 w-1",
            "cursor-col-resize",
            "hover:bg-accent/50",
            "transition-colors duration-100",
            props.position === "left" ? "right-0" : "left-0",
            isResizing() && "bg-accent"
          )}
          onMouseDown={handleResizeStart}
        />
      </Show>

      {/* Collapse Button */}
      <Show when={props.collapsible !== false}>
        <div
          class={clsx(
            "absolute top-1/2 -translate-y-1/2",
            "z-10",
            props.position === "left" ? "-right-3" : "-left-3"
          )}
        >
          <Tooltip
            content={collapsed() ? "Expandir" : "Colapsar"}
            placement={props.position === "left" ? "right" : "left"}
          >
            <button
              class={clsx(
                "w-6 h-12 rounded-full",
                "bg-bg-lighter border border-border",
                "flex items-center justify-center",
                "text-text-muted hover:text-text",
                "hover:bg-surface/50",
                "transition-all duration-100"
              )}
              onClick={() => setCollapsed(!collapsed())}
              aria-label={collapsed() ? "Expandir panel" : "Colapsar panel"}
            >
              <Icons.CaretRight
                size={14}
                class={clsx(
                  "transition-transform duration-200",
                  props.position === "left" && !collapsed() && "rotate-180",
                  props.position === "right" && collapsed() && "rotate-180"
                )}
              />
            </button>
          </Tooltip>
        </div>
      </Show>
    </aside>
  );
};
