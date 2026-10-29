import { Component, For, JSX, Show, createSignal, onMount } from "solid-js";
import { WORKSPACES, type WorkspaceId } from "../../lib/pipeline";
import { clsx } from "clsx";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { IconButton, Tooltip } from "../ui";
import * as Icons from "../icons";
import { ContextMenu, type MenuEntry } from "../ui/ContextMenu";
import { t } from "../../lib/i18n";
import { IS_ANDROID } from "../../lib/platform";
import { watchWindow, windowMaximized } from "../../lib/windowState";
import appIcon from "../../../../desktop/icons/icon-small.svg";

export interface HeaderProps {
  fps?: number;
  title?: string;
  fileName?: string;
  /** Hay cambios sin guardar en el proyecto */
  unsaved?: boolean;
  /** Ítems del menú Archivo (se arman al abrirlo) */
  fileMenu?: () => MenuEntry[];
  /** Más menús junto a Archivo (Editar, Vista, Malla…); sus ítems se arman al abrirlos */
  menus?: { label: string; items: () => MenuEntry[] }[];
  onOpenSettings?: () => void;
  /** Abre o cierra el chat con la IA */
  onToggleChat?: () => void;
  chatOpen?: boolean;
  /** Estado de la IA en vivo (va junto al chat, al lado de Exportar) */
  aiStatus?: JSX.Element;
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
  const isMaximized = windowMaximized;
  const [fileMenu, setFileMenu] = createSignal<{ x: number; y: number; items: MenuEntry[]; index: number } | null>(null);
  let menuClosedAt = 0;

  onMount(watchWindow);

  const handleMinimize = () => getCurrentWindow().minimize();
  const handleToggleMaximize = () => getCurrentWindow().toggleMaximize();
  const handleClose = () => getCurrentWindow().close();

  // `-webkit-app-region` es de Electron: en Tauri la ventana se arrastra
  // pidiéndolo. Arrastra la barra salvo sus botones; doble clic maximiza
  const handleBarMouseDown = (e: MouseEvent) => {
    if (e.button !== 0) return;
    if ((e.target as HTMLElement).closest("button, a, input, select, textarea, nav, [role='menu']")) return;
    e.preventDefault();
    if (e.detail === 2) void handleToggleMaximize();
    else void getCurrentWindow().startDragging();
  };

  return (
    <header
      class={clsx(
        "flex items-center justify-between",
        "h-12 px-6",
        "bg-bg-darker/50 border-b border-border",
        "draggable"
      )}
      onMouseDown={handleBarMouseDown}
    >
      {/* Left - Logo & Title */}
      <div class="flex items-center gap-3 no-drag">
        {/* Logo: el ícono de la app (versión plana para tamaños chicos) */}
        <img src={appIcon} alt="" class="w-7 h-7 rounded-md select-none" draggable={false} />

        {/* Title */}
        <h1 class="text-sm font-semibold text-text">
          {props.title ?? "Pinocchio"}
        </h1>

        <For each={[...(props.fileMenu ? [{ label: t("Archivo"), items: props.fileMenu }] : []), ...(props.menus ?? [])]}>
          {(menu, index) => (
            <button
              class={clsx(
                "h-7 px-2.5 rounded-md text-xs text-text-muted",
                "hover:text-text hover:bg-surface/40",
                fileMenu()?.index === index() && "text-text bg-surface/40"
              )}
              onClick={(e) => {
                // El clic que acaba de cerrar el menú (clic fuera) no lo vuelve a abrir
                if (performance.now() - menuClosedAt < 300) return;
                const rect = e.currentTarget.getBoundingClientRect();
                setFileMenu({ x: rect.left, y: rect.bottom + 4, items: menu.items(), index: index() });
              }}
              onMouseEnter={(e) => {
                // Con un menú abierto, pasar a otro de la barra lo abre (como en un menú de escritorio)
                if (!fileMenu() || fileMenu()!.index === index()) return;
                const rect = e.currentTarget.getBoundingClientRect();
                setFileMenu({ x: rect.left, y: rect.bottom + 4, items: menu.items(), index: index() });
              }}
            >
              {menu.label}
            </button>
          )}
        </For>
        <Show when={fileMenu()} keyed>
          {(m) => <ContextMenu x={m.x} y={m.y} items={m.items} onClose={() => { menuClosedAt = performance.now(); setFileMenu(null); }} />}
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
                {t(ws.label)}
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
            <Show when={props.unsaved}>
              <Tooltip content={t("Cambios sin guardar")}>
                <span class="w-1.5 h-1.5 rounded-full bg-warning shrink-0" aria-label={t("Cambios sin guardar")} />
              </Tooltip>
            </Show>
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
            {t("Exportar")}
          </button>
        </Show>

        {/* IA: estado en vivo y chat, juntos */}
        <Show when={props.onToggleChat}>
          <div class="w-px h-4 bg-border" />
          <div class="flex items-center gap-1.5">
            {props.aiStatus}
            <Tooltip content={t("Chat con IA")}>
              <IconButton variant={props.chatOpen ? "primary" : "ghost"} size="sm" aria-label={t("Chat con IA")} active={props.chatOpen} onClick={() => props.onToggleChat?.()}>
                <Icons.MagicWand size={17} />
              </IconButton>
            </Tooltip>
          </div>
          <div class="w-px h-4 bg-border" />
        </Show>

        {/* FPS Counter */}
        <Show when={props.fps !== undefined}>
          <span class="text-xs font-mono text-text-dim">
            {props.fps} FPS
          </span>
        </Show>

        {/* Settings */}
        <Tooltip content={t("Configuración")}>
          <IconButton variant="ghost" size="sm" aria-label={t("Configuración")} onClick={() => props.onOpenSettings?.()}>
            <Icons.Gear size={18} />
          </IconButton>
        </Tooltip>

        {/* Window Controls (en Android la ventana es la pantalla entera) */}
        <Show when={!IS_ANDROID}>
        <div class="w-px h-4 bg-border" />

        <Tooltip content={t("Minimizar")}>
          <IconButton
            variant="ghost"
            size="sm"
            aria-label={t("Minimizar")}
            onClick={handleMinimize}
          >
            <Icons.Minus size={16} />
          </IconButton>
        </Tooltip>

        <Tooltip content={isMaximized() ? t("Restaurar") : t("Maximizar")}>
          <IconButton
            variant="ghost"
            size="sm"
            aria-label={isMaximized() ? t("Restaurar") : t("Maximizar")}
            onClick={handleToggleMaximize}
          >
            <Show when={isMaximized()} fallback={<Icons.Square size={14} />}>
              <Icons.CornersIn size={16} />
            </Show>
          </IconButton>
        </Tooltip>

        <Tooltip content={t("Cerrar")}>
          <IconButton
            variant="ghost"
            size="sm"
            aria-label={t("Cerrar")}
            class="window-close-btn"
            onClick={handleClose}
          >
            <Icons.X size={16} />
          </IconButton>
        </Tooltip>
        </Show>
      </div>
    </header>
  );
};
