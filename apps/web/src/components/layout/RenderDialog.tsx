import { Component, Show, createEffect, onCleanup, onMount } from "solid-js";
import { Button, Checkbox, NumberInput, ProgressBar, Select, Slider } from "../ui";
import * as Icons from "../icons";
import { t } from "../../lib/i18n";
import { MAX_STILL_SIDE, type StillBackground, type StillFormat } from "../../lib/renderStill";

export type RenderSize = "screen" | "hd" | "uhd" | "custom";

/** Lo que recuerda el diálogo de Render entre sesiones */
export interface RenderSettings {
  size: RenderSize;
  /** Medidas del tamaño personalizado */
  width: number;
  height: number;
  /** Al cambiar un lado del tamaño personalizado, el otro sigue la relación */
  lockRatio: boolean;
  background: StillBackground;
  color: string;
  /** Cuadros promediados por píxel: 1, 4 o 16 */
  samples: number;
  format: StillFormat;
  /** Calidad de JPEG y WebP (0-100) */
  quality: number;
  hideGrid: boolean;
  hideSkeleton: boolean;
  hideGizmos: boolean;
  hideSelection: boolean;
  shadows: boolean;
  ambientOcclusion: boolean;
}

export const defaultRenderSettings: RenderSettings = {
  size: "hd",
  width: 1920,
  height: 1080,
  lockRatio: true,
  background: "viewer",
  color: "#ffffff",
  samples: 4,
  format: "png",
  quality: 92,
  hideGrid: true,
  hideSkeleton: true,
  hideGizmos: true,
  hideSelection: true,
  shadows: true,
  ambientOcclusion: true,
};

/** Medidas en píxeles que salen de las opciones; "Pantalla" toma las de la vista */
export function renderPixels(s: RenderSettings, view: { width: number; height: number }): { width: number; height: number } {
  if (s.size === "screen") return { width: view.width, height: view.height };
  if (s.size === "hd") return { width: 1920, height: 1080 };
  if (s.size === "uhd") return { width: 3840, height: 2160 };
  return { width: s.width, height: s.height };
}

const clampSide = (v: number) => Math.max(16, Math.min(MAX_STILL_SIDE, Math.round(v) || 16));

export interface RenderDialogProps {
  settings: RenderSettings;
  onSettingsChange: (settings: RenderSettings) => void;
  /** Tamaño de la vista en píxeles (lo que sale con "Pantalla") */
  viewSize: { width: number; height: number };
  /** Avance de 0 a 1 mientras se renderiza; undefined si no hay render en curso */
  progress?: number;
  onRender: () => void;
  /** Detiene el render en curso */
  onStop: () => void;
  onClose: () => void;
}

/**
 * Opciones del render de una imagen. Es un panel flotante sin velo: el visor se
 * sigue viendo (con el marco del área que saldrá) y se puede mover la vista.
 */
export const RenderDialog: Component<RenderDialogProps> = (props) => {
  const s = () => props.settings;
  const set = (patch: Partial<RenderSettings>) => props.onSettingsChange({ ...s(), ...patch });
  const busy = () => props.progress !== undefined;
  const px = () => renderPixels(s(), props.viewSize);
  const megapixels = () => (px().width * px().height) / 1e6;

  onMount(() => {
    // En captura: Esc cierra (o detiene) y no llega a los atajos del visor
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      if (busy()) props.onStop();
      else props.onClose();
    };
    window.addEventListener("keydown", onKey, true);
    onCleanup(() => window.removeEventListener("keydown", onKey, true));
  });

  // Al elegir "Personalizado" arranca con las medidas que había
  let previous: RenderSize = s().size;
  createEffect(() => {
    const size = s().size;
    if (size === "custom" && previous !== "custom") {
      const p = renderPixels({ ...s(), size: previous }, props.viewSize);
      props.onSettingsChange({ ...s(), width: clampSide(p.width), height: clampSide(p.height) });
    }
    previous = size;
  });

  const setWidth = (v: number) => {
    const width = clampSide(v);
    set(s().lockRatio ? { width, height: clampSide((width * s().height) / s().width) } : { width });
  };
  const setHeight = (v: number) => {
    const height = clampSide(v);
    set(s().lockRatio ? { height, width: clampSide((height * s().width) / s().height) } : { height });
  };

  const sizeOptions = [
    { value: "screen", get label() { return t("Pantalla"); } },
    { value: "hd", label: "1080p (1920 × 1080)" },
    { value: "uhd", label: "4K (3840 × 2160)" },
    { value: "custom", get label() { return t("Personalizado"); } },
  ];
  const backgroundOptions = [
    { value: "viewer", get label() { return t("El del visor"); } },
    { value: "transparent", get label() { return t("Transparente"); } },
    { value: "color", get label() { return t("Color"); } },
  ];
  const sampleOptions = [
    { value: "1", get label() { return t("Borrador (1 cuadro)"); } },
    { value: "4", get label() { return t("Normal (4 cuadros)"); } },
    { value: "16", get label() { return t("Alta (16 cuadros)"); } },
  ];
  const formatOptions = [
    { value: "png", label: "PNG" },
    { value: "jpeg", label: "JPG" },
    { value: "webp", label: "WebP" },
  ];

  return (
    <div
      role="dialog"
      aria-label={t("Render")}
      class="fixed right-4 top-16 z-50 w-80 max-h-[calc(100vh-6rem)] flex flex-col rounded-lg border border-border bg-bg shadow-xl"
    >
      <div class="flex items-center justify-between px-4 h-11 border-b border-border shrink-0">
        <h2 class="flex items-center gap-2 text-sm font-semibold text-text">
          <Icons.FrameCorners size={16} class="text-accent" />
          {t("Render")}
        </h2>
        <button
          class="w-7 h-7 flex items-center justify-center rounded text-text-muted hover:text-text hover:bg-surface/40 disabled:opacity-40"
          aria-label={t("Cerrar")}
          disabled={busy()}
          onClick={props.onClose}
        >
          <Icons.X size={14} />
        </button>
      </div>

      <div class="p-4 space-y-4 overflow-y-auto">
        <p class="text-[10px] text-text-dim leading-relaxed">
          {t("Sale de lo que se ve ahora: misma posición, giro y zoom. El marco del visor muestra el área exacta; mueve la vista para encuadrar.")}
        </p>

        {/* Tamaño */}
        <div class="space-y-2">
          <Select label={t("Tamaño")} options={sizeOptions} value={s().size} disabled={busy()} onChange={(v) => set({ size: v as RenderSize })} />
          <Show when={s().size === "custom"}>
            <div class="flex items-center gap-2">
              <NumberInput value={s().width} min={16} max={MAX_STILL_SIDE} step={16} suffix="px" disabled={busy()} onChange={setWidth} />
              <span class="text-xs text-text-dim">×</span>
              <NumberInput value={s().height} min={16} max={MAX_STILL_SIDE} step={16} suffix="px" disabled={busy()} onChange={setHeight} />
            </div>
            <Checkbox label={t("Bloquear la relación")} checked={s().lockRatio} disabled={busy()} onChange={(v) => set({ lockRatio: v })} />
          </Show>
          <p class="text-[10px] text-text-dim">
            {px().width} × {px().height} px · {megapixels().toFixed(1)} MP
          </p>
        </div>

        {/* Fondo */}
        <div class="space-y-2">
          <Select label={t("Fondo de la imagen")} options={backgroundOptions} value={s().background} disabled={busy()} onChange={(v) => set({ background: v as StillBackground })} />
          <Show when={s().background === "color"}>
            <input
              type="color"
              value={s().color}
              disabled={busy()}
              aria-label={t("Color del fondo")}
              class="h-8 w-full rounded border border-border bg-surface/30 cursor-pointer"
              onInput={(e) => set({ color: e.currentTarget.value })}
            />
          </Show>
          <Show when={s().background === "transparent" && s().format === "jpeg"}>
            <p class="text-[10px] text-warning leading-relaxed">{t("JPG no guarda transparencia: el fondo saldrá blanco.")}</p>
          </Show>
        </div>

        {/* Calidad y formato */}
        <div class="space-y-2">
          <Select label={t("Calidad")} options={sampleOptions} value={String(s().samples)} disabled={busy()} onChange={(v) => set({ samples: Number(v) })} />
          <Select label={t("Formato")} options={formatOptions} value={s().format} disabled={busy()} onChange={(v) => set({ format: v as StillFormat })} />
          <Show when={s().format !== "png"}>
            <Slider label={t("Calidad de compresión")} min={50} max={100} step={1} value={s().quality} onChange={(v) => set({ quality: v })} showValue />
          </Show>
        </div>

        {/* Luz */}
        <div class="flex flex-col items-start gap-2">
          <span class="text-xs font-medium text-text-muted">{t("Luz")}</span>
          <Checkbox label={t("Sombras suaves")} checked={s().shadows} disabled={busy()} onChange={(v) => set({ shadows: v })} />
          <Checkbox label={t("Oclusión ambiental")} checked={s().ambientOcclusion} disabled={busy()} onChange={(v) => set({ ambientOcclusion: v })} />
          <p class="text-[10px] text-text-dim leading-relaxed">
            {t("Las sombras salen de la luz principal y ganan penumbra con 4 o 16 cuadros. La oclusión oscurece rincones y contactos.")}
          </p>
        </div>

        {/* Qué se oculta */}
        <div class="flex flex-col items-start gap-2">
          <span class="text-xs font-medium text-text-muted">{t("Ocultar")}</span>
          <Checkbox label={t("Grilla")} checked={s().hideGrid} disabled={busy()} onChange={(v) => set({ hideGrid: v })} />
          <Checkbox label={t("Esqueleto")} checked={s().hideSkeleton} disabled={busy()} onChange={(v) => set({ hideSkeleton: v })} />
          <Checkbox label={t("Gizmos y guías")} checked={s().hideGizmos} disabled={busy()} onChange={(v) => set({ hideGizmos: v })} />
          <Checkbox label={t("Resaltados de selección")} checked={s().hideSelection} disabled={busy()} onChange={(v) => set({ hideSelection: v })} />
        </div>

        <Show when={busy()}>
          <ProgressBar value={Math.round((props.progress ?? 0) * 100)} showValue label={t("Renderizando...")} />
        </Show>
      </div>

      <div class="flex justify-end gap-2 px-4 pb-4 pt-2 border-t border-border shrink-0">
        <Show
          when={busy()}
          fallback={
            <>
              <Button size="sm" variant="ghost" onClick={props.onClose}>
                {t("Cerrar")}
              </Button>
              <Button size="sm" variant="primary" icon={<Icons.FrameCorners size={14} />} onClick={props.onRender}>
                {t("Renderizar")}
              </Button>
            </>
          }
        >
          <Button size="sm" variant="danger" onClick={props.onStop}>
            {t("Cancelar")}
          </Button>
        </Show>
      </div>
    </div>
  );
};
