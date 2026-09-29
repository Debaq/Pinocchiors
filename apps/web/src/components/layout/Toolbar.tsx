import { Component, For } from "solid-js";
import { clsx } from "clsx";
import { IconButton, Tooltip } from "../ui";
import type { ToolDef, ToolId } from "../../lib/tools";
import * as Icons from "../icons";

export interface ToolbarProps {
  /** Herramientas del contexto actual (cambian con la sección abierta) */
  tools: ToolDef[];
  activeTool: ToolId;
  /** Motivo por el que una herramienta no se puede usar todavía */
  disabledReason?: (tool: ToolId) => string | undefined;
  onToolChange?: (tool: ToolId) => void;
  canUndo?: boolean;
  canRedo?: boolean;
  onUndo?: () => void;
  onRedo?: () => void;
  onResetView?: () => void;
}

const toolIcons: Record<ToolId, Component<{ size?: number | string; class?: string }>> = {
  select: Icons.Cursor,
  move: Icons.ArrowsOutCardinal,
  rotate: Icons.ArrowsClockwise,
  scale: Icons.ArrowsOut,
  paint: Icons.PaintBrush,
  measure: Icons.Ruler,
};

export const Toolbar: Component<ToolbarProps> = (props) => {
  return (
    <div
      class={clsx(
        "flex flex-col items-center",
        "w-10 shrink-0",
        "bg-bg-darker border-r border-border",
        "py-2 gap-1"
      )}
    >
      {/* Tools */}
      <For each={props.tools}>
        {(tool) => {
          const ToolIcon = toolIcons[tool.id];
          const reason = () => props.disabledReason?.(tool.id);
          const tip = () => {
            const name = tool.shortcut ? `${tool.label} (${tool.shortcut})` : tool.label;
            return reason() ? `${name}: ${reason()}` : name;
          };
          return (
            <Tooltip content={tip()} placement="right">
              <IconButton
                aria-label={tool.label}
                variant="ghost"
                size="md"
                active={props.activeTool === tool.id}
                disabled={!!reason()}
                onClick={() => props.onToolChange?.(tool.id)}
              >
                <ToolIcon size={16} />
              </IconButton>
            </Tooltip>
          );
        }}
      </For>

      {/* Separator */}
      <div class="w-6 h-px bg-border my-1" />

      {/* Reset View */}
      <Tooltip content="Ver todo (Inicio)" placement="right">
        <IconButton
          aria-label="Ver todo"
          variant="ghost"
          size="md"
          onClick={props.onResetView}
        >
          <Icons.House size={16} />
        </IconButton>
      </Tooltip>

      {/* Spacer */}
      <div class="flex-1" />

      {/* Undo / Redo */}
      <Tooltip content="Deshacer (Ctrl+Z)" placement="right">
        <IconButton
          aria-label="Deshacer"
          variant="ghost"
          size="md"
          disabled={!props.canUndo}
          onClick={props.onUndo}
        >
          <Icons.ArrowCounterClockwise size={16} />
        </IconButton>
      </Tooltip>

      <Tooltip content="Rehacer (Ctrl+Shift+Z)" placement="right">
        <IconButton
          aria-label="Rehacer"
          variant="ghost"
          size="md"
          disabled={!props.canRedo}
          onClick={props.onRedo}
        >
          <Icons.ArrowClockwise size={16} />
        </IconButton>
      </Tooltip>
    </div>
  );
};
