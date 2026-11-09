// Panel izquierdo de la biblioteca maker: pestañas por categoría, buscador y
// la ficha de la pieza elegida con su medida. En un diseño la pone (sobre la
// cara plana elegida); en un ensamble la agrega como pieza comprada.

import { Component, For, Show, createMemo, createResource, createSignal } from "solid-js";
import { clsx } from "clsx";
import type { CadStore, CatalogItem, LibraryCategory, LibraryPart } from "../../lib/cad";
import type { CadUi } from "../../lib/cadUi";
import type { DesignActions } from "../../lib/designActions";
import { LIBRARY_CATEGORIES, itemLabel, libraryCatalog } from "../../lib/library";
import { Button, IconButton, NumberInput, Select } from "../ui";
import * as Icons from "../icons";
import { t } from "../../lib/i18n";
import { tBackend } from "../../lib/i18nBackend";

export const LibraryPanel: Component<{ store: CadStore; ui: CadUi; actions: DesignActions; onClose: () => void }> = (props) => {
  const [catalog] = createResource(libraryCatalog);
  const [tab, setTab] = createSignal<LibraryCategory>("motors");
  const [query, setQuery] = createSignal("");
  const [picked, setPicked] = createSignal<string>();
  const [variant, setVariant] = createSignal("");
  const [length, setLength] = createSignal<number>();
  const [busy, setBusy] = createSignal(false);
  const inAssembly = () => props.store.activeTab()?.kind === "assembly" || props.ui.assemblyMode();

  const norm = (s: string) => s.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();
  /** Lo de la pestaña o, buscando, lo de todas */
  const shown = createMemo(() => {
    const all = catalog() ?? [];
    const q = norm(query().trim());
    if (!q) return all.filter((i) => i.category === tab());
    return all.filter((i) => norm(`${itemLabel(i)} ${tBackend(i.description)} ${i.variants.map((v) => v.label).join(" ")} ${i.name}`).includes(q));
  });
  const item = createMemo(() => (catalog() ?? []).find((i) => i.id === picked()));
  const choose = (it: CatalogItem) => {
    setPicked(it.id);
    setVariant(it.variants[0]?.id ?? "");
    setLength(it.length?.default);
  };
  const part = (): LibraryPart | undefined => {
    const it = item();
    if (!it) return undefined;
    return { item: it.id, variant: variant() || undefined, length: it.length ? length() ?? it.length.default : undefined };
  };
  const place = async () => {
    const p = part();
    if (!p) return;
    setBusy(true);
    try {
      if (inAssembly()) await props.store.addPurchased(p);
      else await props.actions.addLibrary(p);
    } catch (e) {
      props.ui.setMessage(tBackend(String(e)));
    } finally {
      setBusy(false);
    }
  };
  const count = (c: LibraryCategory) => (catalog() ?? []).filter((i) => i.category === c).length;

  return (
    <section class="flex flex-col h-full min-w-0 bg-bg-darker" data-library-panel>
      <header class="flex items-center gap-1 h-9 px-2 shrink-0 border-b border-border">
        <Icons.Cube size={14} />
        <span class="text-xs font-medium text-text">{t("Biblioteca")}</span>
        <input
          type="search"
          placeholder={t("Buscar (NEMA 17, 2020, Uno, imán 10…)")}
          aria-label={t("Buscar en la biblioteca")}
          value={query()}
          onInput={(e) => setQuery(e.currentTarget.value)}
          class="ml-2 flex-1 min-w-0 h-6 px-2 rounded bg-surface/40 border border-border text-xs text-text outline-none focus:border-accent"
        />
        <IconButton size="sm" aria-label={t("Cerrar")} onClick={() => props.onClose()}>
          <Icons.X size={14} />
        </IconButton>
      </header>
      <Show when={!query().trim()}>
        <div role="tablist" aria-label={t("Categorías")} class="flex flex-wrap gap-0.5 px-2 py-1.5 shrink-0 border-b border-border">
          <For each={LIBRARY_CATEGORIES}>
            {(c) => (
              <button
                role="tab"
                aria-selected={tab() === c.id}
                class={clsx("px-2 h-6 rounded text-[11px] whitespace-nowrap", tab() === c.id ? "bg-accent/20 text-accent" : "text-text-muted hover:bg-surface/50 hover:text-text")}
                onClick={() => setTab(c.id)}
              >
                {c.label()}
                <span class="ml-1 text-text-dim">{count(c.id)}</span>
              </button>
            )}
          </For>
        </div>
      </Show>
      <div class="flex-1 min-h-0 overflow-y-auto p-2 space-y-1">
        <Show when={catalog.error}>
          <p class="text-xs text-error">{tBackend(String(catalog.error))}</p>
        </Show>
        <For each={shown()} fallback={<p class="text-xs text-text-dim px-1">{catalog.loading ? t("Cargando…") : t("Nada con ese nombre")}</p>}>
          {(it) => (
            <button
              data-library-item={it.id}
              class={clsx(
                "w-full flex items-start gap-2 rounded border p-1.5 text-left transition-colors",
                picked() === it.id ? "border-accent bg-accent/10" : "border-border hover:border-accent/60",
              )}
              onClick={() => choose(it)}
            >
              <span class="mt-0.5 w-3 h-3 shrink-0 rounded-sm border border-border" style={{ background: it.color }} />
              <span class="min-w-0">
                <span class="block text-xs text-text truncate">{itemLabel(it)}</span>
                <span class="block text-[10px] text-text-dim leading-snug line-clamp-2">{tBackend(it.description)}</span>
              </span>
            </button>
          )}
        </For>
      </div>
      <Show when={item()}>
        {(it) => (
          <div class="shrink-0 border-t border-border p-2 space-y-2" data-library-detail>
            <p class="text-xs font-medium text-text">{itemLabel(it())}</p>
            <Show when={it().variants.length > 1}>
              <Select options={it().variants.map((v) => ({ value: v.id, label: tBackend(v.label) }))} value={variant()} onChange={setVariant} />
            </Show>
            <Show when={it().length}>
              {(l) => <NumberInput label={t("Largo")} suffix="mm" min={l().min} max={l().max} step={1} value={length() ?? l().default} onChange={setLength} />}
            </Show>
            <Button size="sm" fullWidth loading={busy()} disabled={busy()} icon={<Icons.Plus size={12} />} onClick={() => void place()}>
              {inAssembly() ? t("Agregar al ensamble") : t("Poner en el diseño")}
            </Button>
            <p class="text-[10px] text-text-dim leading-snug">
              {inAssembly()
                ? t("Queda como pieza comprada (pestaña «Piezas compradas»): se une con las demás y sale en la lista de materiales, sin diseñarla.")
                : t("Se apoya en la cara plana elegida (si no, en el origen). Es de referencia: no se manda a imprimir. En su ventana: «Hacer agujeros» para alojarla.")}
            </p>
          </div>
        )}
      </Show>
    </section>
  );
};
