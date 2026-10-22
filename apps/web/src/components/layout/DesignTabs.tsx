// Barra de pestañas de Diseñar, abajo del visor (como en Onshape): cada
// pestaña es un diseño aparte, con sus operaciones, parámetros y piezas.
// Clic cambia, doble clic renombra, arrastrar reordena y el clic derecho
// duplica o borra. Ver libs/cad/PLAN_PESTANAS.md.

import { For, Show, createSignal, onCleanup } from "solid-js";
import { clsx } from "clsx";
import type { CadStore, CadTab } from "../../lib/cad";
import type { CadUi } from "../../lib/cadUi";
import { t } from "../../lib/i18n";
import { tBackend } from "../../lib/i18nBackend";
import { dropIndex } from "../../lib/designTabs";
import { ContextMenu, type MenuEntry } from "../ui/ContextMenu";
import type { ConfirmRequest } from "./ConfirmDialog";
import * as Icons from "../icons";
import * as DesignIcons from "../icons/design";

type TabCommand = Parameters<CadStore["changeTabs"]>[0];

/**
 * Corre un comando de pestañas. Si puede cambiar la pestaña activa, antes se
 * termina el sketch abierto (queda guardado en su diseño) y se sueltan las
 * selecciones y el modo ensamble.
 */
export async function changeDesignTabs(store: CadStore, ui: CadUi, cmd: TabCommand, args: Record<string, unknown>) {
  if (cmd !== "cad_tab_rename" && cmd !== "cad_tab_move") {
    if (ui.session()) await ui.finishSketch();
    ui.clearPicks();
    ui.setAssemblyMode(false);
  }
  try {
    await store.changeTabs(cmd, args);
  } catch (e) {
    store.setError(tBackend(String(e)));
  }
}

export interface DesignTabsProps {
  store: CadStore;
  ui: CadUi;
  confirm: (request: ConfirmRequest) => Promise<boolean>;
}

export const DesignTabs = (props: DesignTabsProps) => {
  const tabs = () => props.store.tabs().tabs;
  const active = () => props.store.tabs().active;
  const run = (cmd: TabCommand, args: Record<string, unknown>) => changeDesignTabs(props.store, props.ui, cmd, args);

  const [renaming, setRenaming] = createSignal<number>();
  const [menu, setMenu] = createSignal<{ x: number; y: number; tab: CadTab }>();

  const rename = (tab: CadTab, name: string) => {
    setRenaming(undefined);
    if (name.trim() && name.trim() !== tBackend(tab.name)) void run("cad_tab_rename", { id: tab.id, name });
  };

  const remove = async (tab: CadTab) => {
    const ok = await props.confirm({
      title: t("Borrar «{name}»", { name: tBackend(tab.name) }),
      message: t("Se borra la pestaña con todas sus operaciones y no se puede deshacer. Las piezas que ya pasaron a la escena con su malla se quedan."),
      confirmLabel: t("Borrar"),
      danger: true,
    });
    if (ok) await run("cad_tab_delete", { id: tab.id });
  };

  const menuItems = (tab: CadTab): MenuEntry[] => [
    { label: t("Renombrar"), onSelect: () => setRenaming(tab.id) },
    { label: t("Duplicar"), onSelect: () => void run("cad_tab_duplicate", { id: tab.id }) },
    { separator: true },
    { label: t("Borrar…"), onSelect: () => void remove(tab), disabled: tabs().length < 2 },
  ];

  // Arrastrar para reordenar (con el puntero: el arrastre de HTML lo toma la ventana para soltar archivos)
  const refs = new Map<number, HTMLElement>();
  const [drag, setDrag] = createSignal<{ id: number; x0: number; moved: boolean; to: number }>();
  // El clic que llega al soltar un arrastre no cambia de pestaña
  let dragged = false;
  /** Posición en la barra donde caería la pestaña soltada en `x` */
  const slotAt = (x: number) => tabs().filter((tab) => {
    const r = refs.get(tab.id)?.getBoundingClientRect();
    return r && x > r.left + r.width / 2;
  }).length;
  const onMove = (e: PointerEvent) => {
    const d = drag();
    if (!d) return;
    const moved = d.moved || Math.abs(e.clientX - d.x0) > 4;
    setDrag({ ...d, moved, to: slotAt(e.clientX) });
  };
  const onUp = () => {
    const d = drag();
    setDrag(undefined);
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("pointerup", onUp);
    if (!d?.moved) return;
    dragged = true;
    setTimeout(() => (dragged = false));
    const from = tabs().findIndex((x) => x.id === d.id);
    const to = dropIndex(from, d.to);
    if (to !== from) void run("cad_tab_move", { id: d.id, to });
  };
  onCleanup(() => {
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("pointerup", onUp);
  });
  /** Dónde se dibuja la marca de soltar (px desde el borde de la barra) */
  const dropMark = () => {
    const d = drag();
    if (!d?.moved) return undefined;
    const list = tabs();
    const el = d.to < list.length ? refs.get(list[d.to].id) : refs.get(list[list.length - 1]?.id);
    const bar = el?.parentElement?.getBoundingClientRect();
    const r = el?.getBoundingClientRect();
    if (!r || !bar) return undefined;
    return (d.to < list.length ? r.left : r.right) - bar.left;
  };

  return (
    <div data-design-tabs role="tablist" aria-label={t("Pestañas del diseño")} class="relative flex h-full items-stretch gap-px overflow-x-auto bg-bg-darker px-1 text-xs">
      <For each={tabs()}>
        {(tab) => (
          <div
            ref={(el) => refs.set(tab.id, el)}
            role="tab"
            data-tab={tab.id}
            aria-selected={tab.id === active()}
            title={tBackend(tab.name)}
            class={clsx(
              "flex shrink-0 cursor-pointer select-none items-center gap-1.5 border-t-2 px-3",
              tab.id === active() ? "border-accent bg-bg-lighter text-text" : "border-transparent text-text-muted hover:bg-bg-lighter/60 hover:text-text",
              drag()?.id === tab.id && drag()?.moved && "opacity-50",
            )}
            onPointerDown={(e) => {
              if (e.button !== 0 || renaming() === tab.id) return;
              setDrag({ id: tab.id, x0: e.clientX, moved: false, to: 0 });
              window.addEventListener("pointermove", onMove);
              window.addEventListener("pointerup", onUp);
            }}
            onClick={() => !dragged && tab.id !== active() && void run("cad_tab_activate", { id: tab.id })}
            onDblClick={() => setRenaming(tab.id)}
            onContextMenu={(e) => {
              e.preventDefault();
              setMenu({ x: e.clientX, y: e.clientY, tab });
            }}
          >
            {tab.kind === "assembly" ? <DesignIcons.Assembly size={13} /> : <Icons.Cube size={13} />}
            <Show when={renaming() === tab.id} fallback={<span class="max-w-40 truncate">{tBackend(tab.name)}</span>}>
              <input
                data-tab-rename
                class="w-32 rounded border border-accent bg-bg px-1 text-xs text-text outline-none"
                value={tBackend(tab.name)}
                ref={(el) => queueMicrotask(() => (el.focus(), el.select()))}
                onKeyDown={(e) => {
                  e.stopPropagation();
                  if (e.key === "Enter") rename(tab, e.currentTarget.value);
                  else if (e.key === "Escape") setRenaming(undefined);
                }}
                onBlur={(e) => renaming() === tab.id && rename(tab, e.currentTarget.value)}
              />
            </Show>
          </div>
        )}
      </For>
      <button
        data-tab-add
        class="flex shrink-0 items-center px-2 text-text-muted hover:text-text"
        title={t("Nuevo diseño")}
        aria-label={t("Nuevo diseño")}
        onClick={() => void run("cad_tab_add", { kind: "design", name: null })}
      >
        <Icons.Plus size={14} />
      </button>
      <Show when={dropMark()}>{(x) => <div class="pointer-events-none absolute inset-y-0.5 w-0.5 bg-accent" style={{ left: `${x()}px` }} />}</Show>
      <Show when={menu()}>{(m) => <ContextMenu x={m().x} y={m().y} items={menuItems(m().tab)} onClose={() => setMenu(undefined)} />}</Show>
    </div>
  );
};
