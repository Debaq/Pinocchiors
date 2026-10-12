import { Component, Show, createEffect, createMemo, createSignal, on } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import type { CadStore, FlatPattern, PartId } from "../../lib/cad";
import type { CadUi } from "../../lib/cadUi";
import { dimText } from "../../lib/drawing";
import { flatDxf, flatSvg } from "../../lib/flatPattern";
import { Button, Select } from "../ui";
import { t, tn } from "../../lib/i18n";

/**
 * Desarrollo de la chapa sobre el visor: la pieza plana con las líneas de
 * doblez, a 1:1; se exporta a DXF (para cortar), SVG o PDF. Se rehace solo
 * cuando cambia el diseño.
 */
export const FlatPatternView: Component<{ store: CadStore; ui: CadUi }> = (props) => {
  const parts = () => props.store.result()?.parts ?? [];
  // "auto": la pieza de la chapa
  const [partKey, setPartKey] = createSignal("auto");
  const part = (): PartId | undefined => (partKey() === "auto" ? undefined : parts()[Number(partKey())]?.id);
  const [flat, setFlat] = createSignal<FlatPattern>();
  const [message, setMessage] = createSignal<string>();
  const [busy, setBusy] = createSignal(false);
  let seq = 0;

  createEffect(
    on([() => props.store.result()?.version, partKey], async () => {
      const n = ++seq;
      setBusy(true);
      try {
        const fp = await props.store.flatPattern(part());
        if (n === seq) {
          setFlat(fp);
          setMessage(undefined);
        }
      } catch (e) {
        if (n === seq) {
          setFlat(undefined);
          setMessage(String(e));
        }
      } finally {
        if (n === seq) setBusy(false);
      }
    }),
  );

  const name = () => {
    const p = part();
    if (p) return parts().find((x) => x.id.feature === p.feature && x.id.index === p.index)?.name ?? t("Pieza");
    // Automático: la pieza que nace de la última chapa (como el backend)
    const sheet = [...(props.store.committed()?.features ?? [])].reverse().find((f) => f.kind.type === "sheet_metal" && !f.suppressed);
    return (parts().find((x) => x.id.feature === sheet?.id) ?? parts()[0])?.name ?? t("Pieza");
  };
  const svg = createMemo(() => (flat() ? flatSvg(flat()!, name()) : ""));
  const file = (ext: string) => `${name().replace(/[^\p{L}\p{N}_-]+/gu, "_") || "desarrollo"}_desarrollo.${ext}`;

  const write = async (ext: "dxf" | "svg" | "pdf") => {
    const fp = flat();
    if (!fp) return;
    const path = await save({ filters: [{ name: ext.toUpperCase(), extensions: [ext] }], defaultPath: file(ext) });
    if (!path) return;
    try {
      const bytes =
        ext === "pdf"
          ? await invoke<number>("cad_write_pdf", { path, svg: svg() })
          : await invoke<number>("cad_write_text", { path, content: ext === "dxf" ? flatDxf(fp) : svg() });
      setMessage(t("Exportado ({kb} KB)", { kb: Math.round(bytes / 1024) }));
    } catch (e) {
      setMessage(String(e));
    }
  };

  return (
    <div class="absolute inset-0 z-[6] flex flex-col bg-bg-darker" aria-label={t("Desarrollo")}>
      <div class="flex flex-wrap items-center gap-2 border-b border-border bg-bg-lighter px-2 py-1.5">
        <span class="text-xs font-semibold text-text">{t("Desarrollo")}</span>
        <Show when={parts().length > 1}>
          <div class="w-44">
            <Select
              options={[{ value: "auto", label: t("La pieza de la chapa") }, ...parts().map((p, i) => ({ value: String(i), label: p.name }))]}
              value={partKey()}
              onChange={setPartKey}
            />
          </div>
        </Show>
        <Show when={flat()}>
          {(fp) => (
            <span data-flat-size class="text-xs text-text-muted">
              {t("{w} × {h} mm · {bends} · espesor {th} · K {k}", {
                w: dimText(fp().max[0] - fp().min[0]),
                h: dimText(fp().max[1] - fp().min[1]),
                bends: tn(fp().bends.length, "{n} doblez", "{n} dobleces"),
                th: dimText(fp().thickness),
                k: dimText(fp().k_factor),
              })}
            </span>
          )}
        </Show>
        <div class="flex-1" />
        <Show when={busy()}>
          <span class="text-[11px] text-text-dim">{t("Desarrollando…")}</span>
        </Show>
        <Button size="sm" variant="primary" disabled={!flat()} onClick={() => void write("dxf")}>
          {t("Exportar DXF")}
        </Button>
        <Button size="sm" disabled={!flat()} onClick={() => void write("svg")}>
          {t("Exportar SVG")}
        </Button>
        <Button size="sm" disabled={!flat()} onClick={() => void write("pdf")}>
          {t("Exportar PDF")}
        </Button>
        <Button size="sm" variant="ghost" onClick={() => props.ui.setFlatOpen(false)}>
          {t("Cerrar")}
        </Button>
      </div>
      <Show when={message()}>
        <p class="px-2 py-1 text-xs text-text-muted">{message()}</p>
      </Show>
      {/* La hoja entera a la vista (a su tamaño real si entra) */}
      <div class="flex-1 min-h-0 p-4 flex items-center justify-center">
        <div data-flat class="h-full w-full flex items-center justify-center [&>svg]:block [&>svg]:max-h-full [&>svg]:max-w-full [&>svg]:shadow-lg" innerHTML={svg()} />
      </div>
    </div>
  );
};
