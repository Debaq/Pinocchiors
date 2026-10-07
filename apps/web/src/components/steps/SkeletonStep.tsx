import { Component, For, Show, type JSX } from "solid-js";
import { Button, Checkbox, Panel, Select, Slider } from "../ui";
import type { PaintSettings } from "../../lib/Viewer3D";
import { SkeletonPanel, type SkeletonPreset } from "../panels/SkeletonPanel";
import { ConfigPanel, type AutorigConfig } from "../panels/ConfigPanel";
import { SkeletonTransformPanel, type SkeletonTransform } from "../panels/SkeletonTransformPanel";
import * as Icons from "../icons";
import { SKELETON_TABS, type SkeletonEditorTab } from "../layout/SkeletonEditor";

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
  { value: "add", label: "Sumar" },
  { value: "subtract", label: "Restar" },
  { value: "smooth", label: "Suavizar" },
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
      <h3 class="text-sm font-semibold text-text">Esqueleto y pesos</h3>
      <p class="text-xs text-text-muted leading-relaxed">
        Crea el esqueleto, ajústalo al modelo y calcula los pesos en el editor al costado del visor (Ctrl+E).
      </p>

      <div class="space-y-1">
        <Button
          size="sm"
          fullWidth
          variant={props.editorOpen ? "default" : "primary"}
          onClick={() => props.onToggleEditor?.()}
        >
          {props.editorOpen ? "Cerrar el editor de esqueleto" : "Abrir el editor de esqueleto"}
        </Button>
        <p class="text-xs text-text-dim leading-relaxed">
          Crear, ajustar al modelo, pesos y controles, al costado del visor (Ctrl+E). El borde se arrastra para darle
          más ancho.
        </p>
      </div>

      {/* Avance */}
      <div class="space-y-1 text-xs">
        <For each={SKELETON_TABS.filter((t) => t.id !== "controls" && t.id !== "create")}>
          {(t) => (
            <div class="flex items-center gap-2">
              <span class={done(t.id) ? "text-green" : "text-text-dim"}>
                {done(t.id) ? <Icons.Check size={13} /> : "·"}
              </span>
              <span class={done(t.id) ? "text-text" : "text-text-muted"}>{t.label}</span>
            </div>
          )}
        </For>
      </div>

      <Show when={ownSkeleton()}>
        <div class="p-2 rounded-md bg-accent/10 border border-accent/20 space-y-1">
          <div class="flex items-center gap-2 text-xs text-text">
            <Icons.Bone size={14} class="text-accent" />
            Esqueleto del modelo
            <Show when={props.skeletonBones}>
              <span class="text-text-muted ml-auto font-mono">{props.skeletonBones} huesos</span>
            </Show>
          </div>
          <p class="text-xs text-text-muted leading-relaxed">
            Ya está colocado sobre el modelo y se usa tal cual. Elegir una plantilla lo reemplaza.
          </p>
        </div>
      </Show>

      <SkeletonPanel presets={props.presets} selectedPreset={props.selectedPreset} onPresetChange={props.onPresetChange} />

      <Show when={hasSkeleton() && props.hasModel === false}>
        <p class="text-xs text-text-muted leading-relaxed p-3 rounded-md bg-surface/30 border border-border">
          Sin modelo: mueve las articulaciones o transforma el esqueleto y anímalo en Animar. Se exporta solo (GLB,
          glTF o BVH). Si después importas un modelo, el esqueleto y sus animaciones se conservan: ajústalo y calcula
          los pesos.
        </p>
      </Show>
    </div>
  );
};

/** Pestaña Ajustar del editor: automático, a mano, ajuste fino y el esqueleto entero */
export const SkeletonFitTab: Component<SkeletonStepProps> = (props) => (
  <Show
    when={props.skeletonLoaded && props.skeletonTransform}
    fallback={<p class="text-xs text-text-muted">Primero crea o elige un esqueleto (pestaña Crear).</p>}
  >
    <div>
      <Show when={props.hasModel !== false}>
        <Panel title="Ajuste automático" id="skeleton.autofit" icon={<Icons.MagicWand size={14} />} defaultOpen>
          <div class="space-y-3 pb-3">
            <p class="text-xs text-text-muted leading-relaxed">
              Detecta las extremidades del modelo (patas, cabeza, cola) y su orientación, y lleva cada punta a la suya.
            </p>
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
                    <span class={info().quality < 0.5 ? "text-warning" : "text-text"}>
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
            <Button onClick={props.onCenterAll} disabled={props.isProcessing} fullWidth>
              Centrar todas las articulaciones en la malla
            </Button>
          </div>
        </Panel>
      </Show>
      <Panel title="Con el mouse" id="skeleton.manual" icon={<Icons.ArrowsOutCardinal size={14} />} defaultOpen>
        <div class="space-y-3 pb-3">
          <p class="text-xs text-text-muted leading-relaxed">
            Clic cerca de una articulación la selecciona y <span class="text-text">G</span> la mueve con el mouse
            (X/Y/Z la limita a un eje; clic confirma, clic derecho o Esc cancela). La herramienta Mover también tiene
            gizmo; <span class="text-text">Escalar</span> (S) agranda o achica el esqueleto entero desde su centro.
          </p>
          <Button onClick={props.onEdit} disabled={props.isProcessing} variant={props.editing ? "primary" : "default"} fullWidth>
            <span class="flex items-center gap-1.5">
              <Icons.ArrowsOutCardinal size={14} />
              {props.editing ? "Editando articulaciones" : "Mover articulaciones"}
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
        {props.hasModel === false ? "Sin modelo no hay malla que pesar: importa uno." : "Primero crea o elige un esqueleto (pestaña Crear)."}
      </p>
    }
  >
    <div>
      <Panel title="Calcular" id="skeleton.weights" icon={<Icons.PersonArmsSpread size={14} />} defaultOpen>
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
                <Panel title="Pintar pesos" id="skeleton.paint" icon={<Icons.PaintBrush size={14} />} defaultOpen>
                  <div class="space-y-4 pt-1">
                    <p class="text-xs text-text-muted leading-relaxed">
                      Con <span class="text-text">Pintar (B)</span> arrastra sobre la malla: el mapa de
                      calor muestra el hueso activo (rojo = todo su peso). Ctrl invierte el modo,
                      Shift suaviza, <span class="text-text">F</span> cambia el radio y Shift+F la
                      intensidad. Recalcular los pesos descarta lo pintado.
                    </p>
                    <Select
                      label="Hueso"
                      options={(props.boneNames ?? []).map((name, i) => ({ value: String(i), label: name }))}
                      value={props.selectedBone !== undefined && props.selectedBone >= 0 ? String(props.selectedBone) : undefined}
                      onChange={(v) => props.onSelectBone?.(Number(v))}
                      placeholder="Elige un hueso o haz clic en una articulación"
                    />
                    <Select
                      label="Modo"
                      options={PAINT_MODES}
                      value={config().mode}
                      onChange={(v) => update({ mode: v as PaintSettings["mode"] })}
                    />
                    <Slider
                      label="Radio"
                      value={Math.round(config().radius * 1000) / 10}
                      onChange={(v) => update({ radius: v / 100 })}
                      min={0.5}
                      max={20}
                      step={0.5}
                      formatValue={(v) => `${v} %`}
                    />
                    <Slider
                      label="Intensidad"
                      value={Math.round(config().strength * 100)}
                      onChange={(v) => update({ strength: v / 100 })}
                      min={5}
                      max={100}
                      step={5}
                      formatValue={(v) => `${v} %`}
                    />
                    <Checkbox
                      label="Espejo: pintar también el lado opuesto"
                      checked={config().mirror}
                      onChange={(mirror) => update({ mirror })}
                    />
                    <Button onClick={props.onPaint} variant={props.painting ? "primary" : "default"} fullWidth>
                      <span class="flex items-center gap-1.5">
                        <Icons.PaintBrush size={14} />
                        {props.painting ? "Pintando (Ctrl+Z deshace)" : "Pintar"}
                      </span>
                    </Button>
                  </div>
                </Panel>
              );
            }}
          </Show>
          <Panel title="Probar la pose" id="skeleton.pose" icon={<Icons.ArrowsClockwise size={14} />} defaultOpen>
            <div class="space-y-3 pt-1">
              <p class="text-xs text-text-muted leading-relaxed">
                Selecciona una articulación y pulsa <span class="text-text">R</span>: gira con el
                mouse (X/Y/Z para un eje) y la malla se dobla con los pesos. Los giros se suman: gira el hombro y
                después el codo para ver la cadena entera. Esc deshace solo el último giro. Si se estira o arrastra partes que no
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
      </div>
    </Show>
  </Show>
);
