import { Component, Show, createEffect, onCleanup } from "solid-js";
import { Panel, Slider, Button, Select } from "../ui";
import * as Icons from "../icons";

/** Opciones del desplegado (ver `UvUnwrapConfig` en apps/desktop) */
export interface UvConfig {
  textureSize: number;
  padding: number;
  maxAngle: number;
}

export const defaultUvConfig: UvConfig = { textureSize: 2048, padding: 4, maxAngle: 55 };

/** Estado de la piel (ver `UvInfo` en apps/desktop) */
export interface UvInfo {
  mode: "transferred" | "unwrapped";
  seam_faces: number | null;
  num_charts: number | null;
  stretch: number | null;
  coverage: number | null;
  texture_size: number;
  has_base_color: boolean;
  has_normal: boolean;
  can_restore: boolean;
}

export type UvPreview = "texture" | "checker" | "none";

export interface UvStepProps {
  config: UvConfig;
  onChange?: (config: UvConfig) => void;
  hasRetopology: boolean;
  info?: UvInfo;
  isProcessing?: boolean;
  onUnwrap?: () => void;
  onRestore?: () => void;
  preview: UvPreview;
  onPreviewChange?: (preview: UvPreview) => void;
  /** UV de las esquinas de cada quad (8 floats por quad) */
  layout?: Float32Array;
  /** Imagen de fondo del atlas (color base) */
  atlasImage?: ImageBitmap;
}

const SIZE_OPTIONS = [512, 1024, 2048, 4096].map((s) => ({ value: String(s), label: `${s} × ${s} px` }));

const PREVIEW_OPTIONS = [
  { value: "texture", label: "Textura" },
  { value: "checker", label: "Tablero (distorsión)" },
  { value: "none", label: "Sin textura" },
];

const Row: Component<{ label: string; value: string; warn?: boolean }> = (props) => (
  <div class="flex justify-between text-xs">
    <span class="text-text-muted">{props.label}</span>
    <span class={props.warn ? "text-amber-400" : "text-text"}>{props.value}</span>
  </div>
);

/** Atlas UV: el color base de fondo y las aristas de los quads encima */
const Atlas: Component<{ layout: Float32Array; image?: ImageBitmap }> = (props) => {
  let canvas: HTMLCanvasElement | undefined;
  const size = 520;

  createEffect(() => {
    const ctx = canvas?.getContext("2d");
    if (!ctx) return;
    ctx.fillStyle = "#1e1f29";
    ctx.fillRect(0, 0, size, size);
    if (props.image) {
      ctx.drawImage(props.image, 0, 0, size, size);
    }
    const uv = props.layout;
    ctx.strokeStyle = "rgba(80, 250, 123, 0.7)";
    ctx.lineWidth = 1;
    ctx.beginPath();
    for (let q = 0; q + 8 <= uv.length; q += 8) {
      ctx.moveTo(uv[q] * size, uv[q + 1] * size);
      for (let k = 1; k <= 4; k++) {
        const i = q + (k % 4) * 2;
        ctx.lineTo(uv[i] * size, uv[i + 1] * size);
      }
    }
    ctx.stroke();
  });
  onCleanup(() => canvas?.getContext("2d")?.clearRect(0, 0, size, size));

  return (
    <canvas
      ref={canvas}
      width={size}
      height={size}
      class="w-full aspect-square rounded border border-border bg-bg-dark"
    />
  );
};

export const UvStep: Component<UvStepProps> = (props) => {
  const update = (partial: Partial<UvConfig>) => props.onChange?.({ ...props.config, ...partial });
  const percent = (x: number | null) => (x === null ? "--" : `${(100 * x).toFixed(1)} %`);

  return (
    <div class="space-y-5">
      <h3 class="text-sm font-semibold text-text">UV / Piel</h3>
      <p class="text-xs text-text-muted leading-relaxed">
        El mapa UV extiende la superficie en un plano para pintarla. Si el modelo traía UV,
        la retopología ya las trasladó; desplegar corta islas nuevas por las aristas de los
        quads y hornea encima las texturas originales (color, rugosidad y el relieve de la
        malla original como normal map), sin costuras.
      </p>

      <Show
        when={props.hasRetopology}
        fallback={
          <p class="text-xs text-amber-400 leading-relaxed">
            Primero ejecuta la retopología: el mapa se hace sobre la malla de quads.
          </p>
        }
      >
        <Show when={props.info}>
          {(info) => (
            <Panel title="Mapa actual" icon={<Icons.Info size={14} />} defaultOpen>
              <div class="space-y-1.5 pt-1">
                <Row label="Origen" value={info().mode === "transferred" ? "UV trasladadas" : "Desplegado nuevo"} />
                <Show when={info().mode === "transferred"}>
                  <Row
                    label="Caras que cruzan costuras"
                    value={(info().seam_faces ?? 0).toLocaleString()}
                    warn={(info().seam_faces ?? 0) > 0}
                  />
                  <Show when={(info().seam_faces ?? 0) > 0}>
                    <p class="text-xs text-text-muted leading-relaxed pt-1">
                      En esas caras la textura puede verse estirada. Desplegar y hornear lo corrige.
                    </p>
                  </Show>
                </Show>
                <Show when={info().mode === "unwrapped"}>
                  <Row label="Islas" value={(info().num_charts ?? 0).toLocaleString()} />
                  <Row
                    label="Estiramiento"
                    value={info().stretch?.toFixed(3) ?? "--"}
                    warn={(info().stretch ?? 1) > 1.15}
                  />
                  <Row label="Uso del atlas" value={percent(info().coverage)} />
                  <Row
                    label="Texturas horneadas"
                    value={info().texture_size > 0 ? `${info().texture_size} px` : "ninguna (sin UV de origen)"}
                  />
                </Show>
              </div>
            </Panel>
          )}
        </Show>

        <Panel title="Desplegar y hornear" icon={<Icons.Checkerboard size={14} />} defaultOpen>
          <div class="space-y-5 pt-1">
            <Select
              label="Tamaño de textura"
              options={SIZE_OPTIONS}
              value={String(props.config.textureSize)}
              onChange={(v) => update({ textureSize: Number(v) })}
            />
            <Slider
              label="Margen entre islas"
              value={props.config.padding}
              onChange={(v) => update({ padding: Math.round(v) })}
              min={1}
              max={16}
              step={1}
              formatValue={(v) => `${v} px`}
            />
            <Slider
              label="Curvatura por isla"
              value={props.config.maxAngle}
              onChange={(v) => update({ maxAngle: v })}
              min={25}
              max={80}
              step={5}
              formatValue={(v) => `${v}°`}
            />
            <p class="text-xs text-text-muted leading-relaxed">
              Menos grados: más islas con menos distorsión. Más grados: menos costuras.
            </p>
            <Button
              variant="primary"
              class="w-full"
              onClick={() => props.onUnwrap?.()}
              disabled={props.isProcessing}
            >
              {props.isProcessing ? "Procesando..." : "Desplegar y hornear"}
            </Button>
            <Show when={props.info?.can_restore && props.info?.mode === "unwrapped"}>
              <Button class="w-full" onClick={() => props.onRestore?.()} disabled={props.isProcessing}>
                Volver a las UV trasladadas
              </Button>
            </Show>
          </div>
        </Panel>

        <Show when={props.info}>
          <Panel title="Vista" icon={<Icons.Eye size={14} />} defaultOpen>
            <div class="space-y-4 pt-1">
              <Select
                label="En el visor"
                options={PREVIEW_OPTIONS}
                value={props.preview}
                onChange={(v) => props.onPreviewChange?.(v as UvPreview)}
              />
              <Show when={props.layout}>
                <Atlas layout={props.layout!} image={props.atlasImage} />
              </Show>
            </div>
          </Panel>
        </Show>
      </Show>
    </div>
  );
};
