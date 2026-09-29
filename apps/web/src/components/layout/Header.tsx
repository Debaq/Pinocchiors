import { Component, For, Show, createSignal, onMount, onCleanup } from "solid-js";
import { WORKSPACES, type WorkspaceId } from "../../lib/pipeline";
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
  /** Espacio de trabajo activo (`undefined` mientras se exporta) */
  workspace?: WorkspaceId;
  onWorkspace?: (id: WorkspaceId) => void;
  exporting?: boolean;
  onExport?: () => void;
  /** Sin modelo no hay espacios ni exportar */
  hasModel?: boolean;
  /** Hay algo que exportar (un modelo o un esqueleto solo) */
  canExport?: boolean;
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
              "hover:text-text hover:bg-surface/40",
              fileMenu() && "text-text bg-surface/40"
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

      {/* Espacios de trabajo */}
      <Show when={props.onWorkspace}>
        <nav class="flex items-center gap-1 no-drag">
          <For each={WORKSPACES}>
            {(ws) => (
              <button
                class={clsx(
                  "h-7 px-3 rounded-md text-xs transition-colors",
                  "disabled:opacity-40 disabled:pointer-events-none",
                  props.workspace === ws.id
                    ? "bg-accent/20 text-accent font-medium"
                    : "text-text-muted hover:text-text hover:bg-surface/40"
                )}
                disabled={!props.hasModel && !ws.withoutModel}
                onClick={() => props.onWorkspace?.(ws.id)}
              >
                {ws.label}
              </button>
            )}
          </For>
        </nav>
      </Show>

      {/* Right - Status & Actions */}
      <div class="flex items-center gap-3 no-drag">
        <Show when={props.fileName}>
          <div class="flex items-center gap-1.5 max-w-56">
            <Icons.Cube size={13} class="text-text-muted shrink-0" />
            <span class="text-xs text-text-muted font-mono truncate">{props.fileName}</span>
          </div>
        </Show>

        <Show when={props.onExport}>
          <button
            class={clsx(
              "h-7 px-3 rounded-md text-xs flex items-center gap-1.5 transition-colors",
              "disabled:opacity-40 disabled:pointer-events-none",
              props.exporting
                ? "bg-accent text-bg font-medium"
                : "border border-border text-text hover:border-accent hover:text-accent"
            )}
            disabled={!(props.canExport ?? props.hasModel)}
            onClick={() => props.onExport?.()}
          >
            <Icons.Export size={13} />
            Exportar
          </button>
        </Show>

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
