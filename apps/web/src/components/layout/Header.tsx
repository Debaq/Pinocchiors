import { Component, Show, createSignal, onMount, onCleanup } from "solid-js";
import { clsx } from "clsx";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { IconButton, Tooltip } from "../ui";
import * as Icons from "../icons";

export interface HeaderProps {
  fps?: number;
  title?: string;
  fileName?: string;
}

export const Header: Component<HeaderProps> = (props) => {
  const [isMaximized, setIsMaximized] = createSignal(false);

  let unlisten: (() => void) | undefined;

  onMount(async () => {
    const win = getCurrentWindow();
    setIsMaximized(await win.isMaximized());
    unlisten = await win.onResized(async () => {
      setIsMaximized(await win.isMaximized());
    });
  });

  onCleanup(() => unlisten?.());

  const handleMinimize = () => getCurrentWindow().minimize();
  const handleToggleMaximize = () => getCurrentWindow().toggleMaximize();
  const handleClose = () => getCurrentWindow().close();

  return (
    <header
      class={clsx(
        "flex items-center justify-between",
        "h-12 px-6",
        "bg-bg-darker/50 border-b border-border",
        "draggable"
      )}
    >
      {/* Left - Logo & Title */}
      <div class="flex items-center gap-3 no-drag">
        {/* Logo */}
        <div
          class={clsx(
            "w-7 h-7 rounded-md",
            "bg-gradient-to-br from-purple to-pink",
            "flex items-center justify-center",
            "text-bg font-bold text-sm"
          )}
        >
          P
        </div>

        {/* Title */}
        <h1 class="text-sm font-semibold text-text">
          {props.title ?? "Pinocchio"}
        </h1>
      </div>

      {/* Center - File name */}
      <Show when={props.fileName}>
        <div class="flex items-center gap-2 no-drag">
          <Icons.Cube size={14} class="text-text-muted" />
          <span class="text-xs text-text-muted font-mono">
            {props.fileName}
          </span>
        </div>
      </Show>

      {/* Right - Status & Actions */}
      <div class="flex items-center gap-3 no-drag">
        {/* FPS Counter */}
        <Show when={props.fps !== undefined}>
          <span class="text-xs font-mono text-text-dim">
            {props.fps} FPS
          </span>
        </Show>

        {/* Settings */}
        <Tooltip content="Configuracion">
          <IconButton variant="ghost" size="sm" aria-label="Configuracion">
            <Icons.Gear size={18} />
          </IconButton>
        </Tooltip>

        {/* Window Controls */}
        <div class="w-px h-4 bg-border" />

        <Tooltip content="Minimizar">
          <IconButton
            variant="ghost"
            size="sm"
            aria-label="Minimizar"
            onClick={handleMinimize}
          >
            <Icons.Minus size={16} />
          </IconButton>
        </Tooltip>

        <Tooltip content={isMaximized() ? "Restaurar" : "Maximizar"}>
          <IconButton
            variant="ghost"
            size="sm"
            aria-label={isMaximized() ? "Restaurar" : "Maximizar"}
            onClick={handleToggleMaximize}
          >
            <Show when={isMaximized()} fallback={<Icons.Square size={14} />}>
              <Icons.CornersIn size={16} />
            </Show>
          </IconButton>
        </Tooltip>

        <Tooltip content="Cerrar">
          <IconButton
            variant="ghost"
            size="sm"
            aria-label="Cerrar"
            class="window-close-btn"
            onClick={handleClose}
          >
            <Icons.X size={16} />
          </IconButton>
        </Tooltip>
      </div>
    </header>
  );
};
