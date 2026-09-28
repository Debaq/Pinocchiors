import { Component, Show } from "solid-js";
import { Button, Checkbox, Panel } from "../ui";
import { SkeletonPanel, type SkeletonPreset } from "../panels/SkeletonPanel";
import { ConfigPanel, type AutorigConfig } from "../panels/ConfigPanel";
import { SkeletonTransformPanel, type SkeletonTransform } from "../panels/SkeletonTransformPanel";
import * as Icons from "../icons";

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
  onPresetChange?: (presetId: string) => void;
  // Ajuste automático
  onAutoFit?: () => void;
  fitInfo?: SkeletonFitInfo;
  // Edición manual
  editing?: boolean;
  onEdit?: () => void;
  symmetric?: boolean;
  onSymmetricChange?: (symmetric: boolean) => void;
  selectedBoneName?: string;
  onCenterSelected?: () => void;
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
  // Pose de prueba
  posing?: boolean;
  onPose?: () => void;
  onResetPose?: () => void;
}

/** Título numerado de cada etapa del flujo */
const Stage: Component<{ n: number; title: string }> = (props) => (
  <div class="flex items-center gap-2 text-xs font-semibold text-text">
    <span class="w-5 h-5 rounded-full bg-accent/20 text-accent flex items-center justify-center text-[10px]">{props.n}</span>
    {props.title}
  </div>
);

export const SkeletonStep: Component<SkeletonStepProps> = (props) => {
  const hasSkeleton = () => Boolean(props.selectedPreset && props.skeletonTransform);

  return (
    <div class="space-y-5">
      <h3 class="text-sm font-semibold text-text">Esqueleto y pesos</h3>
      <p class="text-xs text-text-muted leading-relaxed">
        Elige una plantilla y ajústala al modelo: se detectan las extremidades (patas, cabeza,
        cola) y la orientación. Corrige lo que haga falta a mano y calcula los pesos.
      </p>

      {/* 1. Plantilla */}
      <div class="space-y-3">
        <Stage n={1} title="Plantilla" />
        <SkeletonPanel
          presets={props.presets}
          selectedPreset={props.selectedPreset}
          onPresetChange={props.onPresetChange}
        />
      </div>

      <Show when={hasSkeleton()}>
        {/* 2. Ajuste automático */}
        <div class="space-y-3">
          <Stage n={2} title="Ajustar al modelo" />
          <Button onClick={props.onAutoFit} disabled={props.isProcessing} variant="primary" fullWidth>
            <span class="flex items-center gap-1.5">
              <Icons.MagicWand size={14} />
              Ajustar automáticamente
            </span>
          </Button>
          <Show when={props.fitInfo}>
            {(info) => (
              <div class="space-y-1 text-xs">
                <div class="flex justify-between">
                  <span class="text-text-muted">Extremidades detectadas</span>
                  <span class="text-text">{info().extremities}</span>
                </div>
                <div class="flex justify-between">
                  <span class="text-text-muted">Proporciones vs. plantilla</span>
                  <span class={info().quality < 0.5 ? "text-amber-400" : "text-text"}>
                    {Math.round(info().quality * 100)} %
                  </span>
                </div>
                <Show when={info().unusedExtremities > 0}>
                  <p class="text-text-muted leading-relaxed pt-1">
                    {info().unusedExtremities} extremidad(es) sin hueso (trompa, orejas, colmillos…): se
                    moverán con la parte del cuerpo más cercana.
                  </p>
                </Show>
              </div>
            )}
          </Show>
        </div>

        {/* 3. Corrección manual */}
        <div class="space-y-3">
          <Stage n={3} title="Corregir a mano" />
          <p class="text-xs text-text-muted leading-relaxed">
            Con <span class="text-text">Mover (G)</span> haz clic en una articulación y arrástrala.
            Mira desde dos vistas: una articulación puede verse bien de frente y estar corrida de
            costado. <span class="text-text">Centrar</span> la devuelve al medio del miembro.
          </p>
          <Button onClick={props.onEdit} disabled={props.isProcessing} variant={props.editing ? "primary" : "default"} fullWidth>
            <span class="flex items-center gap-1.5">
              <Icons.ArrowsOutCardinal size={14} />
              {props.editing ? "Editando articulaciones" : "Mover articulaciones"}
            </span>
          </Button>
          <Checkbox
            label="Espejo: mover también el lado opuesto"
            checked={props.symmetric}
            onChange={props.onSymmetricChange}
          />
          <div class="flex gap-2">
            <Button
              onClick={props.onCenterSelected}
              disabled={props.isProcessing || !props.selectedBoneName}
              fullWidth
            >
              Centrar {props.selectedBoneName ?? "selección"}
            </Button>
            <Button onClick={props.onCenterAll} disabled={props.isProcessing} fullWidth>
              Centrar todas
            </Button>
          </div>
          <SkeletonTransformPanel
            transform={props.skeletonTransform!}
            onChange={props.onTransformChange}
            onReset={props.onResetTransform}
            disabled={props.isProcessing}
          />
        </div>

        {/* 4. Pesos */}
        <div class="space-y-3">
          <Stage n={4} title="Pesos" />
          <ConfigPanel config={props.autorigConfig} onChange={props.onAutorigConfigChange} />
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
                  Procesando...
                </span>
              }
            >
              <span class="flex items-center gap-2">
                <Icons.PersonArmsSpread size={16} />
                Calcular pesos
              </span>
            </Show>
          </Button>

          <Show when={props.autorigComplete}>
            <div class="flex items-center gap-2 text-xs text-green p-2 rounded-md bg-green/10 border border-green/20">
              <Icons.Check size={14} />
              Pesos calculados
              <Show when={props.numBones}>
                <span class="text-text-muted ml-auto font-mono">{props.numBones} huesos</span>
              </Show>
            </div>
            <Panel title="Probar la pose" icon={<Icons.ArrowsClockwise size={14} />} defaultOpen>
              <div class="space-y-3 pt-1">
                <p class="text-xs text-text-muted leading-relaxed">
                  Con <span class="text-text">Rotar (R)</span>, haz clic en una articulación y gírala:
                  la malla se dobla con los pesos. Si se estira o arrastra partes que no
                  corresponden, mueve esa articulación y vuelve a calcular.
                </p>
                <div class="flex gap-2">
                  <Button onClick={props.onPose} variant={props.posing ? "primary" : "default"} fullWidth>
                    {props.posing ? "Girando articulaciones" : "Girar articulaciones"}
                  </Button>
                  <Button onClick={props.onResetPose} fullWidth>
                    Restablecer pose
                  </Button>
                </div>
              </div>
            </Panel>
          </Show>
        </div>
      </Show>
    </div>
  );
};
