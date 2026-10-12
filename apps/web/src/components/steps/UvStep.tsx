import { Component, Show } from "solid-js";
import { Panel, Slider, Button, Select, Checkbox, Hint, Tooltip } from "../ui";
import * as Icons from "../icons";
import { t } from "../../lib/i18n";

/** Opciones del desplegado (ver `UvUnwrapConfig` en apps/desktop) */
export interface UvConfig {
  textureSize: number;
  padding: number;
  maxAngle: number;
  /** Legible para pintar a mano o lo más apretado posible */
  layout: "paintable" | "compact";
  /** Para pintar: una isla por parte del cuerpo según el rig */
  byParts?: boolean;
}

export const defaultUvConfig: UvConfig = { textureSize: 2048, padding: 4, maxAngle: 55, layout: "paintable" };

const LAYOUT_OPTIONS = [
  { value: "paintable", get label() { return t("Para pintar"); } },
  { value: "compact", get label() { return t("Compacta"); } },
];

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
  /** Abre la piel en el editor de texturas */
  onOpenEditor?: () => void;
  /** Hay un modelo cargado (sin retopología se puede desplegar el original) */
  hasModel?: boolean;
  /** El modelo cargado ya trae UV */
  modelHasUvs?: boolean;
  /** Despliega la malla original, sin retopología */
  onUnwrapOriginal?: () => void;
  /** Se puede volver a la malla de antes del desplegado del original */
  canUndoOriginal?: boolean;
  onUndoOriginal?: () => void;
  /** Hay pesos del rig: se puede cortar por partes del cuerpo */
  hasRig?: boolean;
}

const SIZE_OPTIONS = [512, 1024, 2048, 4096].map((s) => ({ value: String(s), label: `${s} × ${s} px` }));

const PREVIEW_OPTIONS = [
  { value: "texture", get label() { return t("Textura"); } },
  { value: "checker", get label() { return t("Tablero (distorsión)"); } },
  { value: "none", get label() { return t("Sin textura"); } },
];

const Row: Component<{ label: string; value: string; warn?: boolean; hint?: string }> = (props) => {
  const row = (
    <div class="flex justify-between text-xs">
      <span class="text-text-muted">{props.label}</span>
      <span class={props.warn ? "text-warning" : "text-text"}>{props.value}</span>
    </div>
  );
  return props.hint ? (
    <Tooltip content={props.hint} placement="left" class="block w-full">
      {row}
    </Tooltip>
  ) : (
    row
  );
};

/** Tamaño de textura, margen y curvatura: comunes a los dos desplegados */
const UnwrapOptions: Component<{ config: UvConfig; onChange: (partial: Partial<UvConfig>) => void; hasRig?: boolean }> = (
  props
) => (
  <>
    <Tooltip
      content={
        (props.config.layout ?? "paintable") === "paintable"
          ? t("Islas grandes y derechas, ubicadas como en el modelo (una hoja de vistas).")
          : t("Islas apretadas para aprovechar la textura. Difícil de pintar a mano.")
      }
      placement="left"
      class="block w-full"
    >
      <Select
        label={t("Distribución")}
        options={LAYOUT_OPTIONS}
        value={props.config.layout ?? "paintable"}
        onChange={(v) => props.onChange({ layout: v as UvConfig["layout"] })}
      />
    </Tooltip>
    <Show when={(props.config.layout ?? "paintable") === "paintable"}>
      <Checkbox
        label={t("Una isla por parte del cuerpo")}
        hint={
          props.hasRig
            ? t("Cabeza, torso, patas y cola según el rig. No aplica con varios objetos.")
            : t("Calcula los pesos del rig para cortar por partes del cuerpo.")
        }
        checked={props.config.byParts === true && props.hasRig === true}
        disabled={!props.hasRig}
        onChange={(v) => props.onChange({ byParts: v })}
      />
    </Show>
    <Select
      label={t("Tamaño de textura")}
      options={SIZE_OPTIONS}
      value={String(props.config.textureSize)}
      onChange={(v) => props.onChange({ textureSize: Number(v) })}
    />
    <Slider
      label={t("Margen entre islas")}
      value={props.config.padding}
      onChange={(v) => props.onChange({ padding: Math.round(v) })}
      min={1}
      max={16}
      step={1}
      formatValue={(v) => `${v} px`}
    />
    <Slider
      label={t("Curvatura por isla")}
      title={t("Menos grados: más islas con menos distorsión. Más grados: menos costuras.")}
      value={props.config.maxAngle}
      onChange={(v) => props.onChange({ maxAngle: v })}
      min={25}
      max={80}
      step={5}
      formatValue={(v) => `${v}°`}
    />
  </>
);

export const UvStep: Component<UvStepProps> = (props) => {
  const update = (partial: Partial<UvConfig>) => props.onChange?.({ ...props.config, ...partial });
  const percent = (x: number | null) => (x === null ? "--" : `${(100 * x).toFixed(1)} %`);

  return (
    <div class="space-y-5">
      <div class="flex items-center justify-between">
        <h3 class="text-sm font-semibold text-text">{t("UV / Piel")}</h3>
        <Hint
          text={t("Extiende la superficie en un plano para pintarla. Desplegar corta islas nuevas y hornea encima las texturas y el relieve originales. Con varios materiales, cada objeto tiene su propio mapa.")}
        />
      </div>

      <Show
        when={props.hasRetopology}
        fallback={
          <Show
            when={props.hasModel}
            fallback={<p class="text-xs text-text-muted leading-relaxed">{t("Importa un modelo para darle UV.")}</p>}
          >
            <Panel title={t("Desplegar la malla original")} icon={<Icons.Checkerboard size={14} />} defaultOpen>
              <div class="space-y-5 pt-1">
                <UnwrapOptions config={props.config} onChange={update} hasRig={props.hasRig} />
                <Button
                  variant="primary"
                  class="w-full"
                  onClick={() => props.onUnwrapOriginal?.()}
                  disabled={props.isProcessing}
                  hint={
                    t("Despliega los triángulos del modelo tal cual: la forma no cambia y el rig sigue.") +
                    " " +
                    (props.modelHasUvs ? t("Reemplaza sus UV y hornea sus texturas.") : t("Queda listo para pintar."))
                  }
                >
                  {props.isProcessing ? t("Procesando...") : t("Desplegar la malla original")}
                </Button>
                <Show when={props.canUndoOriginal}>
                  <Button class="w-full" onClick={() => props.onUndoOriginal?.()} disabled={props.isProcessing} hint={t("Deshace el desplegado de la malla original")}>
                    {t("Volver a la malla anterior")}
                  </Button>
                </Show>
              </div>
            </Panel>
          </Show>
        }
      >
        <Show when={props.info}>
          {(info) => (
            <Panel title={t("Mapa actual")} icon={<Icons.Info size={14} />} defaultOpen>
              <div class="space-y-1.5 pt-1">
                <Row label={t("Origen")} value={info().mode === "transferred" ? t("UV trasladadas") : t("Desplegado nuevo")} />
                <Show when={info().mode === "transferred"}>
                  <Row
                    label={t("Caras que cruzan costuras")}
                    value={(info().seam_faces ?? 0).toLocaleString()}
                    warn={(info().seam_faces ?? 0) > 0}
                    hint={(info().seam_faces ?? 0) > 0 ? t("La textura puede verse estirada ahí. Desplegar y hornear lo corrige.") : undefined}
                  />
                </Show>
                <Show when={info().mode === "unwrapped"}>
                  <Row label={t("Islas")} value={(info().num_charts ?? 0).toLocaleString()} />
                  <Row
                    label={t("Estiramiento")}
                    value={info().stretch?.toFixed(3) ?? "--"}
                    warn={(info().stretch ?? 1) > 1.15}
                  />
                  <Row label={t("Uso del atlas")} value={percent(info().coverage)} />
                  <Row
                    label={t("Texturas horneadas")}
                    value={info().texture_size > 0 ? `${info().texture_size} px` : t("ninguna")}
                  />
                </Show>
              </div>
            </Panel>
          )}
        </Show>

        <Panel title={t("Desplegar y hornear")} icon={<Icons.Checkerboard size={14} />} defaultOpen>
          <div class="space-y-5 pt-1">
            <UnwrapOptions config={props.config} onChange={update} hasRig={props.hasRig} />
            <Button
              variant="primary"
              class="w-full"
              onClick={() => props.onUnwrap?.()}
              disabled={props.isProcessing}
              hint={t("Corta islas, despliega y hornea las texturas originales sobre el mapa nuevo")}
            >
              {props.isProcessing ? t("Procesando...") : t("Desplegar y hornear")}
            </Button>
            <Show when={props.info?.can_restore && props.info?.mode === "unwrapped"}>
              <Button class="w-full" onClick={() => props.onRestore?.()} disabled={props.isProcessing} hint={t("Vuelve al mapa del modelo original")}>
                {t("Volver a las UV trasladadas")}
              </Button>
            </Show>
          </div>
        </Panel>

        <Show when={props.info}>
          <Panel title={t("Vista")} icon={<Icons.Eye size={14} />} defaultOpen>
            <div class="space-y-4 pt-1">
              <Select
                label={t("En el visor")}
                options={PREVIEW_OPTIONS}
                value={props.preview}
                onChange={(v) => props.onPreviewChange?.(v as UvPreview)}
              />
              <Button class="w-full" onClick={() => props.onOpenEditor?.()}>
                {t("Abrir en el editor de texturas")}
              </Button>
            </div>
          </Panel>
        </Show>
      </Show>
    </div>
  );
};
