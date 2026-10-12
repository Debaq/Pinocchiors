import { Component, For, Show, createEffect, onCleanup, onMount } from "solid-js";
import { Button, Checkbox, IconButton, NumberInput, ProgressBar, Select, Slider, Tooltip, Hint } from "../ui";
import * as Icons from "../icons";
import { t } from "../../lib/i18n";
import { MAX_STILL_SIDE, type StillBackground, type StillFormat } from "../../lib/renderStill";
import { RENDER_CAMERAS, sheetGrid } from "../../lib/renderCameras";
import { RENDER_LIGHTS } from "../../lib/renderLights";

export type RenderSize = "screen" | "hd" | "uhd" | "custom";
/** Una imagen por cámara, o todas juntas en una hoja */
export type RenderOutput = "files" | "sheet";

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
  /** Color de abajo del degradado (`color` es el de arriba) */
  color2: string;
  /** Iluminación (ver `RENDER_LIGHTS`): "viewer" = las luces del visor */
  lightPreset: string;
  /** Suelo invisible que recibe la sombra del modelo */
  groundShadow: boolean;
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
  /** Ids de las cámaras activas (ver `RENDER_CAMERAS`) */
  cameras: string[];
  output: RenderOutput;
  /** Columnas de la hoja: 2, 3 o 4 */
  sheetColumns: number;
  /** Nombre de la vista escrito en cada celda de la hoja */
  sheetLabels: boolean;
  /** Un mismo zoom para todas las cámaras, o uno por cámara */
  zoomShared: boolean;
  zoom: number;
  /** Zoom de cada cámara cuando no se comparte (por id; falta = 1) */
  zooms: Record<string, number>;
}

/** Zoom con que sale una cámara (1 = el modelo entero con un margen) */
export const cameraZoom = (s: RenderSettings, id: string): number => (s.zoomShared ? s.zoom : (s.zooms[id] ?? 1));

export const defaultRenderSettings: RenderSettings = {
  size: "hd",
  width: 1920,
  height: 1080,
  lockRatio: true,
  background: "viewer",
  color: "#ffffff",
  color2: "#9aa4b5",
  lightPreset: "viewer",
  groundShadow: false,
  samples: 4,
  format: "png",
  quality: 92,
  hideGrid: true,
  hideSkeleton: true,
  hideGizmos: true,
  hideSelection: true,
  shadows: true,
  ambientOcclusion: true,
  cameras: ["current"],
  output: "files",
  sheetColumns: 2,
  sheetLabels: true,
  zoomShared: true,
  zoom: 1,
  zooms: {},
};

/** Medidas en píxeles que salen de las opciones; "Pantalla" toma las de la vista */
export function renderPixels(s: RenderSettings, view: { width: number; height: number }): { width: number; height: number } {
  if (s.size === "screen") return { width: view.width, height: view.height };
  if (s.size === "hd") return { width: 1920, height: 1080 };
  if (s.size === "uhd") return { width: 3840, height: 2160 };
  return { width: s.width, height: s.height };
}

/** Cómo sale el render: cámaras activas (en orden de la lista) y medidas de cada imagen y de la hoja */
export function renderLayout(s: RenderSettings, view: { width: number; height: number }) {
  const cameras = RENDER_CAMERAS.filter((c) => s.cameras.includes(c.id));
  const total = renderPixels(s, view);
  const sheet = s.output === "sheet" && cameras.length > 1;
  if (!sheet) return { cameras, sheet, cols: 1, rows: 1, cell: total, total };
  const { cols, rows } = sheetGrid(cameras.length, s.sheetColumns);
  const cell = { width: Math.max(16, Math.floor(total.width / cols)), height: Math.max(16, Math.floor(total.height / rows)) };
  return { cameras, sheet, cols, rows, cell, total: { width: cell.width * cols, height: cell.height * rows } };
}

const clampSide = (v: number) => Math.max(16, Math.min(MAX_STILL_SIDE, Math.round(v) || 16));

export interface RenderOptionsProps {
  settings: RenderSettings;
  onSettingsChange: (settings: RenderSettings) => void;
  /** Lleva el visor a esa cámara, con el encuadre que tendrá la imagen */
  onPreviewCamera: (id: string, zoom?: number) => void;
  /** Tamaño de la vista en píxeles (lo que sale con "Pantalla") */
  viewSize: { width: number; height: number };
  /** Hay un render en curso: las opciones no se tocan */
  busy?: boolean;
}

/** Opciones del render: tarjetas que fluyen en columnas según el ancho del panel */
export const RenderOptions: Component<RenderOptionsProps> = (props) => {
  const s = () => props.settings;
  const set = (patch: Partial<RenderSettings>) => props.onSettingsChange({ ...s(), ...patch });
  const busy = () => props.busy === true;
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

  const toggleCamera = (id: string, on: boolean) => {
    const cameras = on ? [...s().cameras, id] : s().cameras.filter((c) => c !== id);
    set({ cameras: RENDER_CAMERAS.map((c) => c.id).filter((c) => cameras.includes(c)) });
  };
  /** Cámaras activas con zoom propio ("Vista actual" queda como se ve) */
  const zoomable = () => RENDER_CAMERAS.filter((c) => c.direction && s().cameras.includes(c.id));
  const setZoom = (id: string, v: number) => {
    props.onSettingsChange(s().zoomShared ? { ...s(), zoom: v } : { ...s(), zooms: { ...s().zooms, [id]: v } });
    // El visor sigue el cambio para ver el encuadre; con zoom compartido, desde la primera cámara
    props.onPreviewCamera(id, v);
  };
  const outputOptions = [
    { value: "files", get label() { return t("Una imagen por cámara"); } },
    { value: "sheet", get label() { return t("Hoja con todas las vistas"); } },
  ];
  const columnOptions = [2, 3, 4].map((n) => ({ value: String(n), get label() { return t("{n} columnas", { n }); } }));
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
    { value: "gradient", get label() { return t("Degradado"); } },
  ];
  const lightOptions = RENDER_LIGHTS.map((l) => ({ value: l.id, get label() { return l.label(); } }));
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
    <div class="[column-width:260px] [column-gap:1rem]">
      {/* Cámaras */}
      <div class="flex flex-col items-start gap-2 p-3 rounded-md bg-surface/30 border border-border break-inside-avoid mb-3">
        <span class="flex items-center gap-1.5 text-xs font-medium text-text-muted">
          {t("Cámaras")}
          <Hint text={t("Encuadran el modelo entero desde cada lado. El ojo lleva el visor a esa cámara para ver el encuadre.")} />
        </span>
        <For each={RENDER_CAMERAS}>
          {(camera) => (
            <div class="flex items-center gap-2 w-full">
              <div class="flex-1 min-w-0">
                <Checkbox label={camera.label()} checked={s().cameras.includes(camera.id)} disabled={busy()} onChange={(v) => toggleCamera(camera.id, v)} />
              </div>
              <IconButton size="sm" variant="ghost" aria-label={t("Ver desde esta cámara")} title={t("Ver desde esta cámara")} disabled={busy()} onClick={() => props.onPreviewCamera(camera.id)}>
                <Icons.Eye size={14} />
              </IconButton>
            </div>
          )}
        </For>
        <Show when={zoomable().length > 0}>
          <Show when={zoomable().length > 1}>
            <Checkbox label={t("Mismo zoom en todas")} checked={s().zoomShared} disabled={busy()} onChange={(v) => set({ zoomShared: v })} />
          </Show>
          <Show
            when={s().zoomShared || zoomable().length === 1}
            fallback={
              <For each={zoomable()}>
                {(camera) => (
                  <div class="w-full">
                    <Slider label={`${t("Zoom")} · ${camera.label()}`} min={0.5} max={2.5} step={0.05} value={s().zooms[camera.id] ?? 1} onChange={(v) => setZoom(camera.id, v)} showValue />
                  </div>
                )}
              </For>
            }
          >
            <div class="w-full">
              <Slider label={t("Zoom")} min={0.5} max={2.5} step={0.05} value={s().zoomShared ? s().zoom : (s().zooms[zoomable()[0].id] ?? 1)} onChange={(v) => setZoom(zoomable()[0].id, v)} showValue />
            </div>
          </Show>
        </Show>
        <Show when={s().cameras.length > 1}>
          <Select label={t("Salida")} options={outputOptions} value={s().output} disabled={busy()} onChange={(v) => set({ output: v as RenderOutput })} />
          <Show when={s().output === "sheet"}>
            <Select label={t("Columnas")} options={columnOptions} value={String(s().sheetColumns)} disabled={busy()} onChange={(v) => set({ sheetColumns: Number(v) })} />
            <Checkbox label={t("Nombre en cada vista")} checked={s().sheetLabels} disabled={busy()} onChange={(v) => set({ sheetLabels: v })} />
          </Show>
        </Show>
      </div>

      {/* Tamaño */}
      <div class="space-y-2 p-3 rounded-md bg-surface/30 border border-border break-inside-avoid mb-3">
        <Select label={t("Tamaño")} options={sizeOptions} value={s().size} disabled={busy()} onChange={(v) => set({ size: v as RenderSize })} />
        <Show when={s().size === "custom"}>
          <div class="flex items-center gap-2">
            <NumberInput value={s().width} min={16} max={MAX_STILL_SIDE} step={16} suffix="px" disabled={busy()} onChange={setWidth} />
            <span class="text-xs text-text-dim">×</span>
            <NumberInput value={s().height} min={16} max={MAX_STILL_SIDE} step={16} suffix="px" disabled={busy()} onChange={setHeight} />
          </div>
          <Checkbox label={t("Bloquear la relación")} checked={s().lockRatio} disabled={busy()} onChange={(v) => set({ lockRatio: v })} />
        </Show>
      </div>

      {/* Fondo */}
      <div class="space-y-2 p-3 rounded-md bg-surface/30 border border-border break-inside-avoid mb-3">
        <Select label={t("Fondo de la imagen")} options={backgroundOptions} value={s().background} disabled={busy()} onChange={(v) => set({ background: v as StillBackground })} />
        <Show when={s().background === "color" || s().background === "gradient"}>
          <input
            type="color"
            value={s().color}
            disabled={busy()}
            aria-label={s().background === "gradient" ? t("Color de arriba") : t("Color del fondo")}
            class="h-8 w-full rounded border border-border bg-surface/30 cursor-pointer"
            onInput={(e) => set({ color: e.currentTarget.value })}
          />
        </Show>
        <Show when={s().background === "gradient"}>
          <input
            type="color"
            value={s().color2}
            disabled={busy()}
            aria-label={t("Color de abajo")}
            class="h-8 w-full rounded border border-border bg-surface/30 cursor-pointer"
            onInput={(e) => set({ color2: e.currentTarget.value })}
          />
        </Show>
        <Show when={s().background === "transparent" && s().format === "jpeg"}>
          <p class="text-[10px] text-warning leading-relaxed">{t("JPG no guarda transparencia: el fondo saldrá blanco.")}</p>
        </Show>
      </div>

      {/* Calidad y formato */}
      <div class="space-y-2 p-3 rounded-md bg-surface/30 border border-border break-inside-avoid mb-3">
        <Select label={t("Calidad")} options={sampleOptions} value={String(s().samples)} disabled={busy()} onChange={(v) => set({ samples: Number(v) })} />
        <Select label={t("Formato")} options={formatOptions} value={s().format} disabled={busy()} onChange={(v) => set({ format: v as StillFormat })} />
        <Show when={s().format !== "png"}>
          <Slider label={t("Calidad de compresión")} min={50} max={100} step={1} value={s().quality} onChange={(v) => set({ quality: v })} showValue />
        </Show>
      </div>

      {/* Luz */}
      <div class="flex flex-col items-start gap-2 p-3 rounded-md bg-surface/30 border border-border break-inside-avoid mb-3">
        <span class="text-xs font-medium text-text-muted">{t("Luz")}</span>
        <div class="w-full">
          <Select label={t("Iluminación")} options={lightOptions} value={s().lightPreset} disabled={busy()} onChange={(v) => set({ lightPreset: v })} />
        </div>
        <Tooltip content={t("Salen de la luz principal y ganan penumbra con 4 o 16 cuadros.")} placement="left">
          <Checkbox label={t("Sombras suaves")} checked={s().shadows} disabled={busy()} onChange={(v) => set({ shadows: v })} />
        </Tooltip>
        <Tooltip content={t("Un suelo invisible bajo el modelo recibe su sombra. Pide sombras suaves.")} placement="left">
          <Checkbox label={t("Suelo con sombra")} checked={s().groundShadow && s().shadows} disabled={busy() || !s().shadows} onChange={(v) => set({ groundShadow: v })} />
        </Tooltip>
        <Tooltip content={t("Oscurece rincones y contactos entre superficies.")} placement="left">
          <Checkbox label={t("Oclusión ambiental")} checked={s().ambientOcclusion} disabled={busy()} onChange={(v) => set({ ambientOcclusion: v })} />
        </Tooltip>
      </div>

      {/* Qué se oculta */}
      <div class="flex flex-col items-start gap-2 p-3 rounded-md bg-surface/30 border border-border break-inside-avoid mb-3">
        <span class="text-xs font-medium text-text-muted">{t("Ocultar")}</span>
        <Checkbox label={t("Grilla")} checked={s().hideGrid} disabled={busy()} onChange={(v) => set({ hideGrid: v })} />
        <Checkbox label={t("Esqueleto")} checked={s().hideSkeleton} disabled={busy()} onChange={(v) => set({ hideSkeleton: v })} />
        <Checkbox label={t("Gizmos y guías")} checked={s().hideGizmos} disabled={busy()} onChange={(v) => set({ hideGizmos: v })} />
        <Checkbox label={t("Resaltados de selección")} checked={s().hideSelection} disabled={busy()} onChange={(v) => set({ hideSelection: v })} />
      </div>
    </div>
  );
};

export interface RenderStepProps {
  settings: RenderSettings;
  viewSize: { width: number; height: number };
  /** Avance de 0 a 1 mientras se renderiza; undefined si no hay render en curso */
  progress?: number;
  /** Hay algo que renderizar */
  canRender?: boolean;
  onRender: () => void;
  /** Render chico de la vista actual; la imagen se muestra encima del visor */
  onPreview: () => void;
  /** Detiene el render en curso */
  onStop: () => void;
  /** Panel de opciones a la izquierda del visor */
  panelOpen: boolean;
  onTogglePanel: () => void;
}

/**
 * Sección Render del panel derecho: lanza el render y muestra su avance. Las
 * opciones están en el panel a la izquierda del visor (`RenderEditor`), que
 * también muestra el marco con el área exacta de la imagen.
 */
export const RenderStep: Component<RenderStepProps> = (props) => {
  const busy = () => props.progress !== undefined;
  const layout = () => renderLayout(props.settings, props.viewSize);
  const px = () => layout().total;

  // Esc detiene el render en curso (sin llegar a los atajos del visor)
  onMount(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || !busy()) return;
      e.preventDefault();
      e.stopPropagation();
      props.onStop();
    };
    window.addEventListener("keydown", onKey, true);
    onCleanup(() => window.removeEventListener("keydown", onKey, true));
  });

  return (
    <div class="space-y-4">
      <div class="flex items-center justify-between">
        <h3 class="flex items-center gap-1.5 text-sm font-semibold text-text">
          {t("Render")}
          <Hint text={t("Con «Vista actual» sale de lo que se ve ahora: misma posición, giro y zoom. El marco del visor muestra el área exacta; mueve la vista para encuadrar.")} />
        </h3>
        <span class="text-[10px] text-text-dim">
          {px().width} × {px().height} · {((px().width * px().height) / 1e6).toFixed(1)} MP
        </span>
      </div>
      <Tooltip content={t("Hace un render de la vista actual, del tamaño del marco, con estas mismas opciones y lo muestra encima del visor.")} placement="left">
        <Button fullWidth disabled={busy() || !props.canRender} icon={<Icons.Eye size={16} />} onClick={props.onPreview}>
          {t("Vista previa")}
        </Button>
      </Tooltip>
      <Show
        when={busy()}
        fallback={
          <Button variant="primary" fullWidth disabled={!props.canRender || layout().cameras.length === 0} icon={<Icons.FrameCorners size={16} />} onClick={props.onRender}>
            {t("Renderizar")}
          </Button>
        }
      >
        <div class="space-y-2">
          <ProgressBar value={Math.round((props.progress ?? 0) * 100)} showValue label={t("Renderizando...")} />
          <Button variant="danger" fullWidth onClick={props.onStop}>
            {t("Cancelar")}
          </Button>
        </div>
      </Show>
      <Show when={!props.panelOpen}>
        <Button fullWidth onClick={props.onTogglePanel}>
          {t("Mostrar las opciones")}
        </Button>
      </Show>
    </div>
  );
};
