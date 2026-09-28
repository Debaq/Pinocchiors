import { Component, Show, createSignal, onMount, onCleanup } from "solid-js";
import { clsx } from "clsx";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { IconButton, Tooltip } from "../ui";
import * as Icons from "../icons";
import { ContextMenu, type MenuEntry } from "../ui/ContextMenu";

export interface HeaderProps {
  fps?: number;
  title?: string;
  fileName?: string;
  /** Ítems del menú Archivo (se arman al abrirlo) */
  fileMenu?: () => MenuEntry[];
  onOpenSettings?: () => void;
}

export const Header: Component<HeaderProps> = (props) => {
  const [isMaximized, setIsMaximized] = createSignal(false);
  const [fileMenu, setFileMenu] = createSignal<{ x: number; y: number; items: MenuEntry[] } | null>(null);
  let menuClosedAt = 0;

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

        <Show when={props.fileMenu}>
          <button
            class={clsx(
              "h-7 px-2.5 rounded-md text-xs text-text-muted",
              "hover:text-text hover:bg-current/40",
              fileMenu() && "text-text bg-current/40"
            )}
            onClick={(e) => {
              // El clic que acaba de cerrar el menú (clic fuera) no lo vuelve a abrir
              if (performance.now() - menuClosedAt < 300) return;
              const rect = e.currentTarget.getBoundingClientRect();
              setFileMenu({ x: rect.left, y: rect.bottom + 4, items: props.fileMenu!() });
            }}
          >
            Archivo
          </button>
        </Show>
        <Show when={fileMenu()}>
          {(m) => <ContextMenu x={m().x} y={m().y} items={m().items} onClose={() => { menuClosedAt = performance.now(); setFileMenu(null); }} />}
        </Show>
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
        <Tooltip content="Configuración">
          <IconButton variant="ghost" size="sm" aria-label="Configuracion" onClick={() => props.onOpenSettings?.()}>
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
