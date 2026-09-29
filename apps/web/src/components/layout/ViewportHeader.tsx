import { Component, For, JSX, Show, createMemo, createSignal, onCleanup } from "solid-js";
import { clsx } from "clsx";
import { Checkbox, Select, Slider, Tooltip, type SelectOption } from "../ui";
import type { ViewSettings } from "../panels/ViewPanel";
import type { BoneDisplay } from "../../lib/Viewer3D";
import * as Icons from "../icons";

export interface ViewportHeaderProps {
  settings: ViewSettings;
  onChange: (change: Partial<ViewSettings>) => void;
  showGrid: boolean;
  onToggleGrid: () => void;
  hasSkeleton: boolean;
  /** Hay pesos calculados (si no, se ve la influencia estimada) */
  hasWeights: boolean;
  boneNames: string[];
  /** Elegir el hueso cuyos pesos se ven (−1: todos) */
  onSelectBone: (index: number) => void;
}

type Shading = "wireframe" | "solid" | "material";

const SHADINGS: { id: Shading; label: string; icon: Component<{ size?: number }>; change: Partial<ViewSettings> }[] = [
  { id: "wireframe", label: "Alambre", icon: Icons.ShadingWireframe, change: { showMesh: false, showWireframe: true } },
  { id: "solid", label: "Sólido (sin piel)", icon: Icons.ShadingSolid, change: { showMesh: true, showTextures: false } },
  { id: "material", label: "Material (con texturas)", icon: Icons.ShadingMaterial, change: { showMesh: true, showTextures: true } },
];

const BONE_DISPLAYS: { id: BoneDisplay; label: string }[] = [
  { id: "octahedral", label: "Octaedro" },
  { id: "stick", label: "Línea" },
];

export const shadingOf = (s: ViewSettings): Shading =>
  !s.showMesh && s.showWireframe ? "wireframe" : s.showTextures === false ? "solid" : "material";

/** Botón con menú desplegable debajo; se cierra al hacer clic fuera o con Escape */
const Dropdown: Component<{ label: string; icon: JSX.Element; children: JSX.Element }> = (props) => {
  let ref: HTMLDivElement | undefined;
  const [open, setOpen] = createSignal(false);
  const onDown = (e: PointerEvent) => {
    if (!ref?.contains(e.target as Node)) {
      setOpen(false);
      close();
    }
  };
  const onKey = (e: KeyboardEvent) => {
    if (e.key !== "Escape") return;
    e.stopPropagation();
    setOpen(false);
    close();
  };
  const toggle = () => {
    const next = !open();
    setOpen(next);
    if (next) {
      window.addEventListener("pointerdown", onDown, true);
      window.addEventListener("keydown", onKey, true);
    } else close();
  };
  const close = () => {
    window.removeEventListener("pointerdown", onDown, true);
    window.removeEventListener("keydown", onKey, true);
  };
  onCleanup(close);

  return (
    <div ref={ref} class="relative">
      <Tooltip content={props.label} placement="bottom">
        <button
          class={clsx(
            "h-6 flex items-center gap-0.5 px-1.5 rounded text-text-muted hover:text-text hover:bg-surface/50",
            open() && "bg-surface/60 text-text"
          )}
          aria-label={props.label}
          aria-expanded={open()}
          onClick={toggle}
        >
          {props.icon}
          <Icons.CaretDown size={10} />
        </button>
      </Tooltip>
      <Show when={open()}>
        <div class="absolute right-0 top-full mt-1 z-30 w-60 p-3 flex flex-col gap-2 rounded-md border border-border bg-bg-lighter shadow-lg text-sm">
          {props.children}
        </div>
      </Show>
    </div>
  );
};

/** Barra del visor: sombreado, rayos X, superposiciones y cómo se ven huesos y pesos */
export const ViewportHeader: Component<ViewportHeaderProps> = (props) => {
  const set = (change: Partial<ViewSettings>) => props.onChange(change);

  const boneOptions = createMemo((): SelectOption[] => [
    { value: "-1", label: "Todos los huesos" },
    ...props.boneNames.map((name, i) => ({ value: String(i), label: name })),
  ]);

  const segmented = (active: boolean) =>
    clsx(
      "h-6 px-1.5 flex items-center justify-center gap-1 transition-colors",
      active ? "bg-accent/25 text-accent" : "text-text-muted hover:text-text hover:bg-surface/50"
    );

  return (
    <div class="h-8 shrink-0 flex items-center gap-2 px-2 bg-bg-darker border-b border-border text-xs select-none">
      {/* Esqueleto: forma de los huesos y qué pesos se ven */}
      <Show when={props.hasSkeleton}>
        <span class="text-text-muted">Huesos</span>
        <div class="flex rounded border border-border overflow-hidden">
          <For each={BONE_DISPLAYS}>
            {(d) => (
              <button
                class={segmented((props.settings.boneDisplay ?? "octahedral") === d.id)}
                onClick={() => set({ boneDisplay: d.id, showSkeleton: true })}
              >
                {d.label}
              </button>
            )}
          </For>
        </div>
        <Tooltip content="Colorear la malla según cuánto la mueve cada hueso" placement="bottom">
          <button
            class={clsx(
              "h-6 px-2 flex items-center gap-1 rounded border",
              props.settings.showWeights
                ? "border-accent/50 bg-accent/20 text-accent"
                : "border-border text-text-muted hover:text-text"
            )}
            onClick={() => set({ showWeights: !props.settings.showWeights })}
          >
            <Icons.PaintBrush size={12} />
            {props.hasWeights ? "Pesos" : "Influencia"}
          </button>
        </Tooltip>
        <Show when={props.settings.showWeights}>
          <Select
            class="w-40! h-6! px-2! text-xs! whitespace-nowrap"
            options={boneOptions()}
            value={String(props.settings.selectedBone)}
            onChange={(value) => props.onSelectBone(parseInt(value))}
          />
          <Show when={!props.hasWeights}>
            <span class="text-text-muted truncate" title="Sin pesos calculados: cada zona se asigna al hueso más cercano">
              estimada por cercanía
            </span>
          </Show>
        </Show>
      </Show>

      <div class="ml-auto flex items-center gap-1">
        <Dropdown label="Superposiciones" icon={<Icons.Overlays size={14} />}>
          <div class="text-[10px] uppercase tracking-wide text-text-muted">Superposiciones</div>
          <Checkbox label="Grilla" checked={props.showGrid} onChange={() => props.onToggleGrid()} />
          <Checkbox
            label="Alambre"
            checked={props.settings.showWireframe}
            onChange={(checked) => set({ showWireframe: checked })}
          />
          <Show when={props.hasSkeleton}>
            <Checkbox
              label="Esqueleto"
              checked={props.settings.showSkeleton}
              onChange={(checked) => set({ showSkeleton: checked })}
            />
            <Checkbox
              label={props.hasWeights ? "Pesos" : "Influencia estimada"}
              checked={props.settings.showWeights}
              onChange={(checked) => set({ showWeights: checked })}
            />
          </Show>
        </Dropdown>

        <Tooltip content="Rayos X: modelo translúcido (Alt+Z)" placement="bottom">
          <button
            class={clsx("h-6 w-7 flex items-center justify-center rounded", segmented(props.settings.xray === true))}
            aria-label="Rayos X"
            aria-pressed={props.settings.xray === true}
            onClick={() => set({ xray: !props.settings.xray })}
          >
            <Icons.XRay size={14} />
          </button>
        </Tooltip>

        <div class="flex rounded border border-border overflow-hidden">
          <For each={SHADINGS}>
            {(s) => (
              <Tooltip content={s.label} placement="bottom">
                <button
                  class={clsx(segmented(shadingOf(props.settings) === s.id), "w-7")}
                  aria-label={s.label}
                  onClick={() => set(s.change)}
                >
                  <s.icon size={14} />
                </button>
              </Tooltip>
            )}
          </For>
        </div>

        <Dropdown label="Opciones de sombreado" icon={<></>}>
          <div class="text-[10px] uppercase tracking-wide text-text-muted">Rayos X</div>
          <Checkbox label="Modelo translúcido" checked={props.settings.xray === true} onChange={(checked) => set({ xray: checked })} />
          <Slider
            label="Opacidad"
            min={0.05}
            max={1}
            step={0.05}
            showValue
            formatValue={(v) => `${Math.round(v * 100)} %`}
            value={props.settings.xrayAlpha ?? 0.35}
            onChange={(v) => set({ xrayAlpha: v, xray: true })}
          />
        </Dropdown>
      </div>
    </div>
  );
};
