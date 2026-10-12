import { Component, For, Show, type JSX } from "solid-js";
import { Button, Checkbox, Hint, Panel, Select, Slider } from "../ui";
import type { PaintSettings } from "../../lib/Viewer3D";
import { SkeletonPanel, type SkeletonPreset } from "../panels/SkeletonPanel";
import { ConfigPanel, type AutorigConfig } from "../panels/ConfigPanel";
import { SkeletonTransformPanel, type SkeletonTransform } from "../panels/SkeletonTransformPanel";
import * as Icons from "../icons";
import { SKELETON_TABS, type SkeletonEditorTab } from "../layout/SkeletonEditor";
import { t, tn } from "../../lib/i18n";

/** Resultado del ajuste automático (ver `AutoFitResult` en apps/desktop) */
export interface SkeletonFitInfo {
  quality: number;
  extremities: number;
  unusedExtremities: number;
}

export interface SkeletonStepProps {
  // Skeleton
  presets: SkeletonPreset[];
  selectedPreset?: string;
  /** Hay esqueleto, aunque no venga de una plantilla (el del archivo importado) */
  skeletonLoaded?: boolean;
  /** Huesos del esqueleto actual */
  skeletonBones?: number;
  onPresetChange?: (presetId: string) => void;
  /** Editor de esqueleto al costado del visor */
  editorOpen?: boolean;
  onToggleEditor?: () => void;
  /** Ajuste fino de la articulación elegida (pestaña Ajustar) */
  tunePanel?: JSX.Element;
  // Ajuste automático
  onAutoFit?: () => void;
  /** Sin modelo se edita el esqueleto solo: no hay a qué ajustarlo ni pesos */
  hasModel?: boolean;
  fitInfo?: SkeletonFitInfo;
  // Edición manual
  editing?: boolean;
  onEdit?: () => void;
  symmetric?: boolean;
  onSymmetricChange?: (symmetric: boolean) => void;
  onCenterAll?: () => void;
  // Transform
  skeletonTransform?: SkeletonTransform;
  onTransformChange?: (transform: SkeletonTransform) => void;
  onResetTransform?: () => void;
  // Pesos
  autorigConfig: AutorigConfig;
  onAutorigConfigChange?: (config: AutorigConfig) => void;
  onAutorig?: () => void;
  canAutorig?: boolean;
  isProcessing?: boolean;
  autorigComplete?: boolean;
  numBones?: number;
  // Pincel de pesos
  painting?: boolean;
  onPaint?: () => void;
  paintConfig?: Omit<PaintSettings, "bone">;
  onPaintConfigChange?: (config: Omit<PaintSettings, "bone">) => void;
  boneNames?: string[];
  selectedBone?: number;
  onSelectBone?: (index: number) => void;
  // Pose de prueba
  posing?: boolean;
  onPose?: () => void;
  onResetPose?: () => void;
}

const PAINT_MODES = [
  { value: "add", get label() { return t("Sumar"); } },
  { value: "subtract", get label() { return t("Restar"); } },
  { value: "smooth", get label() { return t("Suavizar"); } },
];


/** Barra lateral de la sección Esqueleto: plantillas, estado y acceso al editor */
export const SkeletonStep: Component<SkeletonStepProps> = (props) => {
  /** Esqueleto propio: ya colocado sobre el modelo, sin plantilla de origen */
  const ownSkeleton = () => props.skeletonLoaded && !props.selectedPreset;
  const hasSkeleton = () => Boolean(props.skeletonLoaded && props.skeletonTransform);
  const done = (tab: SkeletonEditorTab) =>
    tab === "templates" || tab === "create"
      ? hasSkeleton()
      : tab === "fit"
        ? !!props.fitInfo
        : tab === "weights"
          ? !!props.autorigComplete
          : false;

  return (
    <div class="space-y-5">
      <div class="flex items-center justify-between">
        <h3 class="text-sm font-semibold text-text">{t("Esqueleto y pesos")}</h3>
        <Hint text={t("Crea el esqueleto, ajústalo al modelo y calcula los pesos en el editor al costado del visor (Ctrl+E).")} />
      </div>

      <div class="space-y-1">
        <Button
          size="sm"
          fullWidth
          variant={props.editorOpen ? "default" : "primary"}
          onClick={() => props.onToggleEditor?.()}
          hint={t("Crear, ajustar al modelo, pesos y controles, al costado del visor (Ctrl+E). El borde se arrastra para ensancharlo.")}
        >
          {props.editorOpen ? t("Cerrar el editor de esqueleto") : t("Abrir el editor de esqueleto")}
        </Button>
      </div>

      {/* Avance */}
      <div class="space-y-1 text-xs">
        <For each={SKELETON_TABS.filter((tab) => tab.id !== "controls" && tab.id !== "create")}>
          {(tab) => (
            <div class="flex items-center gap-2">
              <span class={done(tab.id) ? "text-green" : "text-text-dim"}>
                {done(tab.id) ? <Icons.Check size={13} /> : "·"}
              </span>
              <span class={done(tab.id) ? "text-text" : "text-text-muted"}>{t(tab.label)}</span>
            </div>
          )}
        </For>
      </div>

      <Show when={ownSkeleton()}>
        <div class="p-2 rounded-md bg-accent/10 border border-accent/20 space-y-1">
          <div class="flex items-center gap-2 text-xs text-text">
            <Icons.Bone size={14} class="text-accent" />
            {t("Esqueleto del modelo")}
            <Show when={props.skeletonBones}>
              <span class="text-text-muted ml-auto font-mono">{tn(props.skeletonBones!, "{n} hueso", "{n} huesos")}</span>
            </Show>
          </div>
          <Hint text={t("Ya está colocado sobre el modelo y se usa tal cual. Elegir una plantilla lo reemplaza.")} />
        </div>
      </Show>

      <SkeletonPanel presets={props.presets} selectedPreset={props.selectedPreset} onPresetChange={props.onPresetChange} />

      <Show when={hasSkeleton() && props.hasModel === false}>
        <p class="text-xs text-text-muted p-2 rounded-md bg-surface/30 border border-border">
          {t("Sin modelo: se exporta solo el esqueleto (GLB, glTF o BVH). Al importar un modelo se conserva.")}
        </p>
      </Show>
    </div>
  );
};

/** Pestaña Ajustar del editor: automático, a mano, ajuste fino y el esqueleto entero */
export const SkeletonFitTab: Component<SkeletonStepProps> = (props) => (
  <Show
    when={props.skeletonLoaded && props.skeletonTransform}
    fallback={<p class="text-xs text-text-muted">{t("Primero crea o elige un esqueleto (pestaña Crear).")}</p>}
  >
    <div>
      <Show when={props.hasModel !== false}>
        <Panel title={t("Ajuste automático")} id="skeleton.autofit" icon={<Icons.MagicWand size={14} />} defaultOpen>
          <div class="space-y-3 pb-3">
            <Button
              onClick={props.onAutoFit}
              disabled={props.isProcessing}
              variant="primary"
              fullWidth
              hint={t("Detecta las extremidades del modelo (patas, cabeza, cola) y su orientación, y lleva cada punta a la suya.")}
            >
              <span class="flex items-center gap-1.5">
                <Icons.MagicWand size={14} />
                {t("Ajustar automáticamente")}
              </span>
            </Button>
            <Show when={props.fitInfo}>
              {(info) => (
                <div class="space-y-1 text-xs">
                  <div class="flex justify-between">
                    <span class="text-text-muted">{t("Extremidades detectadas")}</span>
                    <span class="text-text">{info().extremities}</span>
                  </div>
                  <div class="flex justify-between">
                    <span class="text-text-muted">{t("Proporciones vs. plantilla")}</span>
                    <span class={info().quality < 0.5 ? "text-warning" : "text-text"}>
                      {Math.round(info().quality * 100)} %
                    </span>
                  </div>
                  <Show when={info().unusedExtremities > 0}>
                    <p class="text-text-muted leading-relaxed pt-1">
                      {t("{n} extremidad(es) sin hueso (trompa, orejas, colmillos…): se moverán con la parte del cuerpo más cercana.", { n: info().unusedExtremities })}
                    </p>
                  </Show>
                </div>
              )}
            </Show>
            <Button onClick={props.onCenterAll} disabled={props.isProcessing} fullWidth>
              {t("Centrar todas las articulaciones en la malla")}
            </Button>
          </div>
        </Panel>
      </Show>
      <Panel title={t("Con el mouse")} id="skeleton.manual" icon={<Icons.ArrowsOutCardinal size={14} />} defaultOpen>
        <div class="space-y-3 pb-3">
          <Hint label={t("Atajos")} text={t("Clic en una articulación la selecciona; G la mueve (X/Y/Z fija el eje; clic confirma, Esc cancela). S escala todo el esqueleto.")} />
          <Button onClick={props.onEdit} disabled={props.isProcessing} variant={props.editing ? "primary" : "default"} fullWidth>
            <span class="flex items-center gap-1.5">
              <Icons.ArrowsOutCardinal size={14} />
              {props.editing ? t("Editando articulaciones") : t("Mover articulaciones")}
            </span>
          </Button>
        </div>
      </Panel>
    </div>
    {props.tunePanel}
    <div>
      <SkeletonTransformPanel
        transform={props.skeletonTransform!}
        onChange={props.onTransformChange}
        onReset={props.onResetTransform}
        disabled={props.isProcessing}
      />
    </div>
  </Show>
);

/** Pestaña Pesos del editor: calcular, pintar y probar */
export const SkeletonWeightsTab: Component<SkeletonStepProps> = (props) => (
  <Show
    when={props.skeletonLoaded && props.hasModel !== false}
    fallback={
      <p class="text-xs text-text-muted">
        {props.hasModel === false ? t("Sin modelo no hay malla que pesar: importa uno.") : t("Primero crea o elige un esqueleto (pestaña Crear).")}
      </p>
    }
  >
    <div>
      <Panel title={t("Calcular")} id="skeleton.weights" icon={<Icons.PersonArmsSpread size={14} />} defaultOpen>
        <div class="space-y-3 pb-3">
        <Button
          onClick={props.onAutorig}
          disabled={!props.canAutorig || props.isProcessing}
          variant="primary"
          fullWidth
        >
          <Show
            when={!props.isProcessing}
            fallback={
              <span class="flex items-center gap-2">
                <span class="animate-spin">&#9203;</span>
                {t("Procesando...")}
              </span>
            }
          >
            <span class="flex items-center gap-2">
              <Icons.PersonArmsSpread size={16} />
              {t("Calcular pesos")}
            </span>
          </Show>
        </Button>

        <Show when={props.autorigComplete}>
          <div class="flex items-center gap-2 text-xs text-green p-2 rounded-md bg-green/10 border border-green/20">
            <Icons.Check size={14} />
            {t("Pesos calculados")}
            <Show when={props.numBones}>
              <span class="text-text-muted ml-auto font-mono">{tn(props.numBones!, "{n} hueso", "{n} huesos")}</span>
            </Show>
          </div>
        </Show>
        </div>
      </Panel>
      <ConfigPanel config={props.autorigConfig} onChange={props.onAutorigConfigChange} />
    </div>
    <Show when={props.autorigComplete}>
      <div>
          <Show when={props.paintConfig}>
            {(config) => {
              const update = (partial: Partial<Omit<PaintSettings, "bone">>) =>
                props.onPaintConfigChange?.({ ...config(), ...partial });
              return (
                <Panel title={t("Pintar pesos")} id="skeleton.paint" icon={<Icons.PaintBrush size={14} />} defaultOpen>
                  <div class="space-y-4 pt-1">
                    <Hint label={t("Atajos")} text={t("Pintar (B) sobre la malla: el mapa de calor muestra el hueso activo. Ctrl invierte, Shift suaviza, F cambia el radio y Shift+F la intensidad. Recalcular descarta lo pintado.")} />
                    <Select
                      label={t("Hueso")}
                      options={(props.boneNames ?? []).map((name, i) => ({ value: String(i), label: name }))}
                      value={props.selectedBone !== undefined && props.selectedBone >= 0 ? String(props.selectedBone) : undefined}
                      onChange={(v) => props.onSelectBone?.(Number(v))}
                      placeholder={t("Elige un hueso o haz clic en una articulación")}
                    />
                    <Select
                      label={t("Modo")}
                      options={PAINT_MODES}
                      value={config().mode}
                      onChange={(v) => update({ mode: v as PaintSettings["mode"] })}
                    />
                    <Slider
                      label={t("Radio")}
                      value={Math.round(config().radius * 1000) / 10}
                      onChange={(v) => update({ radius: v / 100 })}
                      min={0.5}
                      max={20}
                      step={0.5}
                      formatValue={(v) => `${v} %`}
                    />
                    <Slider
                      label={t("Intensidad")}
                      value={Math.round(config().strength * 100)}
                      onChange={(v) => update({ strength: v / 100 })}
                      min={5}
                      max={100}
                      step={5}
                      formatValue={(v) => `${v} %`}
                    />
                    <Checkbox
                      label={t("Espejo: pintar también el lado opuesto")}
                      checked={config().mirror}
                      onChange={(mirror) => update({ mirror })}
                    />
                    <Button onClick={props.onPaint} variant={props.painting ? "primary" : "default"} fullWidth>
                      <span class="flex items-center gap-1.5">
                        <Icons.PaintBrush size={14} />
                        {props.painting ? t("Pintando (Ctrl+Z deshace)") : t("Pintar")}
                      </span>
                    </Button>
                  </div>
                </Panel>
              );
            }}
          </Show>
          <Panel title={t("Probar la pose")} id="skeleton.pose" icon={<Icons.ArrowsClockwise size={14} />} defaultOpen>
            <div class="space-y-3 pt-1">
              <Hint label={t("Cómo se usa")} text={t("Selecciona una articulación y pulsa R para girarla (X/Y/Z fija el eje). Los giros se suman; Esc deshace el último. Si se deforma mal, mueve esa articulación y recalcula.")} />
              <div class="flex gap-2">
                <Button onClick={props.onPose} variant={props.posing ? "primary" : "default"} fullWidth>
                  {props.posing ? t("Girando articulaciones") : t("Girar articulaciones")}
                </Button>
                <Button onClick={props.onResetPose} fullWidth>
                  {t("Restablecer pose")}
                </Button>
              </div>
            </div>
          </Panel>
      </div>
    </Show>
  </Show>
);
