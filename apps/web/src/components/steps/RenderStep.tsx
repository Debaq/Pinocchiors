import { Component, For, Show, createEffect, onCleanup, onMount } from "solid-js";
import { open } from "@tauri-apps/plugin-dialog";
import { Button, Checkbox, IconButton, NumberInput, ProgressBar, Select, Slider, Tooltip, Hint } from "../ui";
import * as Icons from "../icons";
import { t } from "../../lib/i18n";
import { MAX_STILL_SIDE, type StillBackground, type StillFormat } from "../../lib/renderStill";
import { RENDER_CAMERAS, sheetGrid } from "../../lib/renderCameras";
import { RENDER_LIGHTS } from "../../lib/renderLights";
import { BUILTIN_HDRIS, HDRI_FILE, HDRI_NONE, hdriSelectValue } from "../../lib/rayHdri";

export type RenderSize = "screen" | "hd" | "uhd" | "custom";
/** Una imagen por cámara, o todas juntas en una hoja */
export type RenderOutput = "files" | "sheet";

/** Lo que recuerda el diálogo de Render entre sesiones */
export type RenderEngine = "viewer" | "geppettoray";

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
  /** Motor: el del visor (rápido) o GeppettoRay (path tracing, luz global) */
  engine: RenderEngine;
  /** Muestras por píxel de GeppettoRay */
  raySamples: number;
  /** GeppettoRay: quita el ruido que queda al final */
  rayDenoise: boolean;
  /** GeppettoRay: entorno HDR: "builtin:id" (los incluidos), la ruta de un .hdr, o vacío = cielo uniforme del visor */
  rayHdri: string;
  rayHdriIntensity: number;
  /** Giro del entorno en grados */
  rayHdriRotation: number;
  /** Animación: clip que se renderiza ("" = el primero) */
  animClip: string;
  /** Desenfoque de movimiento (obturador de 180° con varios instantes por cuadro) */
  animBlur: boolean;
  /** Saltar los cuadros que ya están en la carpeta */
  animResume: boolean;
  /** Armar un video con ffmpeg al terminar */
  animVideo: boolean;
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
  engine: "viewer",
  raySamples: 128,
  rayDenoise: true,
  rayHdri: "builtin:studio",
  rayHdriIntensity: 1,
  rayHdriRotation: 0,
  animClip: "",
  animBlur: true,
  animResume: false,
  animVideo: true,
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
  /** Animaciones que se pueden renderizar (con el rig listo y claves) */
  animation?: { clips: { id: string; name: string }[]; ready: boolean };
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
  const engineOptions = [
    { value: "viewer", get label() { return t("Rápido (visor)"); } },
    { value: "geppettoray", label: "GeppettoRay" },
  ];
  const raySampleOptions = [
    { value: "32", get label() { return t("Borrador (32 muestras)"); } },
    { value: "128", get label() { return t("Normal (128 muestras)"); } },
    { value: "512", get label() { return t("Alta (512 muestras)"); } },
    { value: "2048", get label() { return t("Máxima (2048 muestras)"); } },
  ];
  const ray = () => s().engine === "geppettoray";
  /** Elegir el entorno: uno incluido, ninguno (cielo del visor) o un .hdr del disco */
  const chooseHdri = async (value: string) => {
    if (value === HDRI_FILE) {
      const path = await open({ multiple: false, filters: [{ name: "HDR", extensions: ["hdr"] }] });
      if (typeof path === "string") set({ rayHdri: path });
    } else {
      set({ rayHdri: value === HDRI_NONE ? "" : value });
    }
  };
  const hdriOptions = () => [
    { value: HDRI_NONE, label: t("Cielo uniforme del visor") },
    ...BUILTIN_HDRIS.map((h) => ({ value: h.path, label: h.label() })),
    { value: HDRI_FILE, label: s().rayHdri && hdriSelectValue(s().rayHdri) === HDRI_FILE ? hdriName() : t("Cargar un .hdr…") },
  ];
  const hdriName = () => s().rayHdri.split(/[\\/]/).pop() ?? "";
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
        <Select label={t("Motor")} options={engineOptions} value={s().engine} disabled={busy()} onChange={(v) => set({ engine: v as RenderEngine })} />
        <Show
          when={ray()}
          fallback={<Select label={t("Calidad")} options={sampleOptions} value={String(s().samples)} disabled={busy()} onChange={(v) => set({ samples: Number(v) })} />}
        >
          <Select label={t("Calidad")} options={raySampleOptions} value={String(s().raySamples)} disabled={busy()} onChange={(v) => set({ raySamples: Number(v) })} />
          <Tooltip content={t("Quita el ruido al terminar, sin borrar el detalle de las texturas ni los bordes.")} placement="left">
            <Checkbox label={t("Quitar ruido")} checked={s().rayDenoise} disabled={busy()} onChange={(v) => set({ rayDenoise: v })} />
          </Tooltip>
          <Hint text={t("Simula la luz de verdad (rebotes, vidrio, reflejos): tarda más, pero la imagen se ve mientras se afina. Las sombras suaves y la oclusión ambiental del visor no se usan: la luz ya las produce.")} />
        </Show>
        <Select label={t("Formato")} options={formatOptions} value={s().format} disabled={busy()} onChange={(v) => set({ format: v as StillFormat })} />
        <Show when={s().format !== "png"}>
          <Slider label={t("Calidad de compresión")} min={50} max={100} step={1} value={s().quality} onChange={(v) => set({ quality: v })} showValue />
        </Show>
      </div>

      {/* Animación */}
      <Show when={ray()}>
        <div class="space-y-2 p-3 rounded-md bg-surface/30 border border-border break-inside-avoid mb-3">
          <span class="text-xs font-medium text-text-muted">{t("Animación")}</span>
          <Show
            when={props.animation?.ready}
            fallback={<Hint text={t("Para renderizar una animación hace falta el rig (esqueleto y pesos) y un clip con claves.")} />}
          >
            <Select
              label={t("Clip")}
              options={(props.animation?.clips ?? []).map((c) => ({ value: c.id, label: c.name }))}
              value={props.animation?.clips.some((c) => c.id === s().animClip) ? s().animClip : (props.animation?.clips[0]?.id ?? "")}
              disabled={busy()}
              onChange={(v) => set({ animClip: v })}
            />
            <Tooltip content={t("Promedia varios instantes de cada cuadro: lo que se mueve queda borroso, como en una cámara. Tarda más.")} placement="left">
              <Checkbox label={t("Desenfoque de movimiento")} checked={s().animBlur} disabled={busy()} onChange={(v) => set({ animBlur: v })} />
            </Tooltip>
            <Tooltip content={t("Salta los cuadros que ya están en la carpeta: sirve para seguir un render interrumpido.")} placement="left">
              <Checkbox label={t("Reanudar")} checked={s().animResume} disabled={busy()} onChange={(v) => set({ animResume: v })} />
            </Tooltip>
            <Tooltip content={t("Arma un MP4 con los cuadros si ffmpeg está instalado. No guarda transparencia: usa fondo de color o degradado.")} placement="left">
              <Checkbox label={t("Video (MP4)")} checked={s().animVideo} disabled={busy()} onChange={(v) => set({ animVideo: v })} />
            </Tooltip>
            <Hint text={t("Los cuadros salen como PNG en la carpeta que elijas, con la primera cámara activa, el tamaño y las luces de arriba.")} />
          </Show>
        </div>
      </Show>

      {/* Luz */}
      <div class="flex flex-col items-start gap-2 p-3 rounded-md bg-surface/30 border border-border break-inside-avoid mb-3">
        <span class="text-xs font-medium text-text-muted">{t("Luz")}</span>
        <div class="w-full">
          <Select label={t("Iluminación")} options={lightOptions} value={s().lightPreset} disabled={busy()} onChange={(v) => set({ lightPreset: v })} />
        </div>
        <Tooltip content={t("Salen de la luz principal y ganan penumbra con 4 o 16 cuadros.")} placement="left">
          <Checkbox label={t("Sombras suaves")} checked={s().shadows} disabled={busy() || ray()} onChange={(v) => set({ shadows: v })} />
        </Tooltip>
        <Tooltip content={t("Un suelo invisible bajo el modelo recibe su sombra. En el visor pide sombras suaves.")} placement="left">
          <Checkbox
            label={t("Suelo con sombra")}
            checked={s().groundShadow && (ray() || s().shadows)}
            disabled={busy() || (!ray() && !s().shadows)}
            onChange={(v) => set({ groundShadow: v })}
          />
        </Tooltip>
        <Tooltip content={t("Oscurece rincones y contactos entre superficies.")} placement="left">
          <Checkbox label={t("Oclusión ambiental")} checked={s().ambientOcclusion} disabled={busy() || ray()} onChange={(v) => set({ ambientOcclusion: v })} />
        </Tooltip>
        <Show when={ray()}>
          <div class="w-full space-y-2">
            <Tooltip content={t("Una imagen HDR ilumina la escena y se ve de fondo (con el fondo del visor); reemplaza al cielo uniforme. Elige uno incluido o carga un .hdr propio.")} placement="left">
              <div class="w-full">
                <Select label={t("Entorno HDR")} options={hdriOptions()} value={hdriSelectValue(s().rayHdri)} disabled={busy()} onChange={(v) => void chooseHdri(v)} />
              </div>
            </Tooltip>
            <Show when={s().rayHdri}>
              <Slider label={t("Intensidad del entorno")} min={0} max={5} step={0.05} value={s().rayHdriIntensity} onChange={(v) => set({ rayHdriIntensity: v })} showValue />
              <Slider label={t("Giro del entorno")} min={0} max={360} step={1} value={s().rayHdriRotation} onChange={(v) => set({ rayHdriRotation: v })} showValue />
            </Show>
          </div>
        </Show>
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
  /** Texto bajo la barra: pasada, muestras y tiempo que falta (GeppettoRay) */
  progressDetail?: string;
  /** Hay algo que renderizar */
  canRender?: boolean;
  onRender: () => void;
  /** Hay rig y un clip con claves: se puede renderizar la animación */
  canAnimate?: boolean;
  onRenderAnimation?: () => void;
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
          <Show when={props.progressDetail}>
            <p class="text-[10px] text-text-dim text-center">{props.progressDetail}</p>
          </Show>
          <Button variant="danger" fullWidth onClick={props.onStop}>
            {t("Cancelar")}
          </Button>
        </div>
      </Show>
      <Show when={!busy() && props.settings.engine === "geppettoray" && props.canAnimate}>
        <Tooltip content={t("Renderiza cada cuadro de la animación con GeppettoRay y los guarda como imágenes en una carpeta.")} placement="left">
          <Button fullWidth disabled={!props.canRender} icon={<Icons.FrameCorners size={16} />} onClick={() => props.onRenderAnimation?.()}>
            {t("Renderizar animación")}
          </Button>
        </Tooltip>
      </Show>
      <Show when={!props.panelOpen}>
        <Button fullWidth onClick={props.onTogglePanel}>
          {t("Mostrar las opciones")}
        </Button>
      </Show>
    </div>
  );
};
