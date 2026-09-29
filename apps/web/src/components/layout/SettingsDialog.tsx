import { Component, Show, onCleanup, onMount } from "solid-js";
import { Checkbox, Select, Slider } from "../ui";
import * as Icons from "../icons";

export interface AutosaveSettings {
  enabled: boolean;
  /** Cada cuántos minutos */
  minutes: number;
}

export type LengthUnit = "mm" | "cm" | "m" | "in";

export interface GridSettings {
  /** Unidad en que se mide la grilla ("auto" = la del archivo) */
  unit: LengthUnit | "auto";
  /** En qué está el modelo ("auto" = lo que dice el archivo; los STL no lo dicen) */
  modelUnit: LengthUnit | "auto";
}

export const UNIT_METERS: Record<LengthUnit, number> = { mm: 0.001, cm: 0.01, m: 1, in: 0.0254 };
const UNIT_NAMES: Record<LengthUnit, string> = { mm: "Milímetros", cm: "Centímetros", m: "Metros", in: "Pulgadas" };
const unitOptions = (auto: string) => [
  { value: "auto", label: auto },
  ...(Object.keys(UNIT_NAMES) as LengthUnit[]).map((u) => ({ value: u, label: `${UNIT_NAMES[u]} (${u})` })),
];
const GRID_UNIT_OPTIONS = unitOptions("Automática (la del modelo)");
const MODEL_UNIT_OPTIONS = unitOptions("Según el archivo");

export interface SettingsDialogProps {
  autosave: AutosaveSettings;
  onAutosaveChange: (settings: AutosaveSettings) => void;
  grid: GridSettings;
  onGridChange: (settings: GridSettings) => void;
  /** Metros por unidad que declara el archivo abierto */
  fileMetersPerUnit?: number;
  /** Archivo del proyecto abierto (sin él, el automático va al de recuperación) */
  projectPath?: string;
  onClose: () => void;
}

/** Configuración de la app (se recuerda entre sesiones) */
export const SettingsDialog: Component<SettingsDialogProps> = (props) => {
  onMount(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        props.onClose();
      }
    };
    window.addEventListener("keydown", onKey, true);
    onCleanup(() => window.removeEventListener("keydown", onKey, true));
  });

  const update = (change: Partial<AutosaveSettings>) => props.onAutosaveChange({ ...props.autosave, ...change });

  return (
    <div
      class="fixed inset-0 z-40 flex items-center justify-center bg-bg-darker/70 backdrop-blur-sm p-4"
      onPointerDown={(e) => e.target === e.currentTarget && props.onClose()}
    >
      <div role="dialog" aria-label="Configuración" class="w-full max-w-md rounded-lg border border-border bg-bg shadow-xl">
        <div class="flex items-center justify-between px-4 h-11 border-b border-border">
          <h2 class="flex items-center gap-2 text-sm font-semibold text-text">
            <Icons.Gear size={16} class="text-text-muted" />
            Configuración
          </h2>
          <button
            class="w-7 h-7 flex items-center justify-center rounded text-text-muted hover:text-text hover:bg-current/40"
            aria-label="Cerrar"
            onClick={props.onClose}
          >
            <Icons.X size={14} />
          </button>
        </div>

        <div class="p-4 space-y-4">
          <section class="space-y-3">
            <h3 class="text-[11px] uppercase tracking-wide text-text-muted">Proyecto</h3>
            <Checkbox
              label="Guardado automático"
              checked={props.autosave.enabled}
              onChange={(enabled) => update({ enabled })}
            />
            <Show when={props.autosave.enabled}>
              <Slider
                label="Cada"
                min={1}
                max={30}
                step={1}
                value={props.autosave.minutes}
                onChange={(minutes) => update({ minutes })}
                formatValue={(v) => (v === 1 ? "1 minuto" : `${v} minutos`)}
              />
              <p class="text-xs text-text-muted leading-relaxed">
                <Show
                  when={props.projectPath}
                  fallback={
                    <>
                      El proyecto todavía no tiene archivo: se guarda en un archivo de recuperación, que se ofrece
                      al abrir la app. Con <span class="text-text">Guardar</span> (Ctrl+S) pasa a guardarse en el tuyo.
                    </>
                  }
                >
                  Se guarda en <span class="text-text font-mono break-all">{props.projectPath}</span>.
                </Show>{" "}
                Si nada cambió, no se reescribe.
              </p>
            </Show>
          </section>

          <section class="space-y-3 pt-3 border-t border-border">
            <h3 class="text-[11px] uppercase tracking-wide text-text-muted">Grilla y unidades</h3>
            <Select
              label="Medir la grilla en"
              options={GRID_UNIT_OPTIONS}
              value={props.grid.unit}
              onChange={(unit) => props.onGridChange({ ...props.grid, unit: unit as GridSettings["unit"] })}
            />
            <Select
              label="El modelo está en"
              options={MODEL_UNIT_OPTIONS}
              value={props.grid.modelUnit}
              onChange={(modelUnit) =>
                props.onGridChange({ ...props.grid, modelUnit: modelUnit as GridSettings["modelUnit"] })
              }
            />
            <p class="text-xs text-text-muted leading-relaxed">
              <Show when={props.fileMetersPerUnit !== undefined}>
                El archivo dice 1 unidad = {props.fileMetersPerUnit} m.{" "}
              </Show>
              Los STL y OBJ no guardan unidades: si la grilla no calza con el tamaño real, elige acá en qué
              está el modelo. Solo cambia cómo se mide, no el modelo.
            </p>
          </section>
        </div>
      </div>
    </div>
  );
};
