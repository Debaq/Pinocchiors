import { Component, For, Show, createEffect, createSignal, on, onCleanup } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { Button, Checkbox, NumberInput } from "../ui";
import * as Icons from "../icons";
import { fmtNumber, t, tx } from "../../lib/i18n";

// ═══════════════════════════════════════════════════════════════════════════
// TIPOS
// ═══════════════════════════════════════════════════════════════════════════

/** Zona de caras elegida (las caras se numeran como las dibuja el visor) */
export interface SkinZone {
  faces: number[];
  total: number;
  area_mm2: number;
  fraction: number;
}

/** Patrón de fábrica o imagen importada */
export interface SkinPattern {
  id: string;
  name: string;
  builtin: boolean;
  width: number;
  height: number;
}

/** Cómo se retoca la imagen antes de usarla */
export interface SkinImageSettings {
  /** Id de la imagen elegida (patrón de fábrica o importada) */
  image: string;
  invert: boolean;
  contrast: number;
  blur: number;
}

export const defaultSkinImage: SkinImageSettings = { image: "leather", invert: false, contrast: 1, blur: 0 };

export interface SkinPanelProps {
  zone?: SkinZone;
  /** Se están eligiendo caras en el visor */
  picking: boolean;
  onTogglePicking: () => void;
  onGrow: (angle: number) => void;
  onInvert: () => void;
  onClear: () => void;
  library: SkinPattern[];
  settings: SkinImageSettings;
  onSettings: (s: SkinImageSettings) => void;
  onImport: () => void;
  onRemove: (id: string) => void;
  busy?: boolean;
}

// ═══════════════════════════════════════════════════════════════════════════
// MINIATURA
// ═══════════════════════════════════════════════════════════════════════════

/** Vista previa en grises de un patrón o imagen, con los retoques aplicados */
const Thumb: Component<{ id: string; settings: SkinImageSettings; size: number; applyOps?: boolean }> = (props) => {
  const [url, setUrl] = createSignal<string | undefined>();
  let request = 0;
  createEffect(
    on(
      () => [props.id, props.applyOps ? props.settings.invert : false, props.applyOps ? props.settings.contrast : 1, props.applyOps ? props.settings.blur : 0, props.size] as const,
      async ([id, invert, contrast, blur, size]) => {
        const mine = ++request;
        try {
          const bytes = await invoke<ArrayBuffer>("skin_preview", { id, ops: { invert, contrast, blur }, size });
          if (mine !== request) return;
          const next = URL.createObjectURL(new Blob([bytes], { type: "image/png" }));
          const old = url();
          setUrl(next);
          if (old) URL.revokeObjectURL(old);
        } catch {
          if (mine === request) setUrl(undefined);
        }
      },
    ),
  );
  onCleanup(() => {
    const old = url();
    if (old) URL.revokeObjectURL(old);
  });
  return (
    <Show when={url()} fallback={<div class="bg-surface/40" style={{ width: `${props.size}px`, height: `${props.size}px` }} />}>
      <img src={url()} width={props.size} height={props.size} class="block" style={{ "image-rendering": "auto" }} alt="" />
    </Show>
  );
};

// ═══════════════════════════════════════════════════════════════════════════
// PANEL
// ═══════════════════════════════════════════════════════════════════════════

const Title: Component<{ children: string }> = (props) => (
  <h4 class="text-xs font-semibold uppercase tracking-wider text-text-muted">{props.children}</h4>
);

export const SkinPanel: Component<SkinPanelProps> = (props) => {
  const [angle, setAngle] = createSignal(15);
  const set = (patch: Partial<SkinImageSettings>) => props.onSettings({ ...props.settings, ...patch });
  const count = () => props.zone?.faces.length ?? 0;
  const selected = () => props.library.find((p) => p.id === props.settings.image);

  return (
    <div class="space-y-3 pt-2 border-t border-border/50">
      <Title>{t("Texturas imprimibles")}</Title>
      <p class="text-[10px] text-text-dim leading-snug">
        {t("Un patrón en blanco y negro que se vuelve relieve de verdad en la zona que elijas (blanco = alto). Si el modelo no tiene UV, despliégalo primero en UV / Piel.")}
      </p>

      <div class="space-y-2">
        <p class="text-[11px] font-medium text-text">{t("1. Zona")}</p>
        <Button size="sm" fullWidth variant={props.picking ? "primary" : "default"} onClick={props.onTogglePicking} icon={<Icons.Cube size={14} />}>
          {props.picking ? t("Terminar de elegir") : t("Elegir zona en el modelo")}
        </Button>
        <Show when={props.picking}>
          <p class="text-[10px] text-text-dim leading-snug">{t("Clic en una cara o arrastra un recuadro. Mayús suma, Ctrl resta.")}</p>
        </Show>
        <div class="flex items-end gap-1.5">
          <NumberInput label={t("Ángulo")} value={angle()} onChange={(v) => setAngle(Math.max(0, Math.min(90, v)))} min={0} max={90} step={5} suffix="°" />
          <Button size="sm" onClick={() => props.onGrow(angle())} disabled={props.busy || count() === 0}>
            {t("Crecer")}
          </Button>
        </div>
        <div class="flex gap-1.5">
          <Button size="sm" variant="ghost" onClick={props.onInvert} disabled={props.busy || !props.zone}>
            {t("Invertir")}
          </Button>
          <Button size="sm" variant="ghost" onClick={props.onClear} disabled={props.busy || count() === 0}>
            {t("Quitar zona")}
          </Button>
        </div>
        <p class={`text-[11px] ${count() > 0 ? "text-text" : "text-text-dim"}`}>
          {count() > 0 && props.zone
            ? t("{n} caras · {area} cm² ({pct} % del modelo)", {
                n: fmtNumber(count()),
                area: fmtNumber(props.zone.area_mm2 / 100, { maximumFractionDigits: 1 }),
                pct: fmtNumber(props.zone.fraction * 100, { maximumFractionDigits: 0 }),
              })
            : t("Sin zona elegida")}
        </p>
      </div>

      <div class="space-y-2">
        <p class="text-[11px] font-medium text-text">{t("2. Imagen")}</p>
        <div class="grid grid-cols-3 gap-1.5">
          <For each={props.library}>
            {(p) => (
              <div class="relative">
                <button
                  class={`w-full rounded border overflow-hidden text-left ${props.settings.image === p.id ? "border-accent ring-1 ring-accent" : "border-border hover:border-border-hover"}`}
                  title={p.name}
                  onClick={() => set({ image: p.id })}
                >
                  <div class="flex justify-center bg-surface/30">
                    <Thumb id={p.id} settings={props.settings} size={64} />
                  </div>
                  <div class="px-1 py-0.5 text-[10px] text-text truncate">{tx("textura", p.name)}</div>
                </button>
                <Show when={!p.builtin}>
                  <button
                    class="absolute top-0.5 right-0.5 w-4 h-4 flex items-center justify-center rounded bg-bg/80 text-text-muted hover:text-red"
                    aria-label={t("Quitar imagen")}
                    onClick={() => props.onRemove(p.id)}
                  >
                    <Icons.X size={10} />
                  </button>
                </Show>
              </div>
            )}
          </For>
        </div>
        <Button size="sm" fullWidth onClick={props.onImport} disabled={props.busy} icon={<Icons.Download size={14} />}>
          {t("Importar imagen...")}
        </Button>

        <Show when={selected()}>
          {(sel) => (
            <div class="flex gap-2 items-start p-2 rounded-md bg-surface/30 border border-border">
              <div class="shrink-0 rounded overflow-hidden border border-border">
                <Thumb id={sel().id} settings={props.settings} size={96} applyOps />
              </div>
              <div class="min-w-0 flex-1 space-y-1.5">
                <p class="text-[11px] text-text truncate">{tx("textura", sel().name)}</p>
                <Checkbox small label={t("Invertir (blanco ↔ negro)")} checked={props.settings.invert} onChange={(v) => set({ invert: v })} />
                <NumberInput label={t("Contraste")} value={props.settings.contrast} onChange={(v) => set({ contrast: Math.max(0, Math.min(8, v)) })} min={0} max={8} step={0.1} suffix="×" />
                <NumberInput label={t("Suavizar")} value={props.settings.blur} onChange={(v) => set({ blur: Math.max(0, Math.min(32, Math.round(v))) })} min={0} max={32} step={1} suffix="px" />
              </div>
            </div>
          )}
        </Show>
      </div>

      <p class="text-[10px] text-text-dim leading-snug">{t("Todavía no genera el relieve: aquí se prepara la zona y la imagen que se usarán.")}</p>
    </div>
  );
};
