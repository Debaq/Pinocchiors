import { Component, Show, onCleanup, onMount } from "solid-js";
import { Checkbox, Select, Slider } from "../ui";
import * as Icons from "../icons";
import { THEME_OPTIONS, type ThemeSetting } from "../../lib/theme";
import { LOCALES, localeSetting, setLocale, t, tn, type LocaleSetting } from "../../lib/i18n";

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
const unitName = (u: LengthUnit): string =>
  ({ mm: t("Milímetros"), cm: t("Centímetros"), m: t("Metros"), in: t("Pulgadas") })[u];
const UNITS: LengthUnit[] = ["mm", "cm", "m", "in"];
// Con getters para que el nombre siga al idioma
const unitOptions = (auto: () => string) => [
  { value: "auto", get label() { return auto(); } },
  ...UNITS.map((u) => ({ value: u, get label() { return `${unitName(u)} (${u})`; } })),
];
const GRID_UNIT_OPTIONS = unitOptions(() => t("Automática (la del modelo)"));
const MODEL_UNIT_OPTIONS = unitOptions(() => t("Según el archivo"));
const LOCALE_OPTIONS = [
  { value: "system", get label() { return t("Según el sistema"); } },
  ...LOCALES.map((l) => ({ value: l.value, label: l.label })),
];

export interface SettingsDialogProps {
  autosave: AutosaveSettings;
  onAutosaveChange: (settings: AutosaveSettings) => void;
  grid: GridSettings;
  onGridChange: (settings: GridSettings) => void;
  theme: ThemeSetting;
  onThemeChange: (theme: ThemeSetting) => void;
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
        return;
      }
      // Los atajos de la app no corren detrás del diálogo; los campos de
      // texto reciben sus teclas (los atajos ya los ignoran) y Tab navega
      const target = e.target as HTMLElement | null;
      if (e.key !== "Tab" && !target?.closest("input, textarea, select, [contenteditable='true']")) e.stopPropagation();
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
      <div role="dialog" aria-label={t("Configuración")} class="w-full max-w-md rounded-lg border border-border bg-bg shadow-xl">
        <div class="flex items-center justify-between px-4 h-11 border-b border-border">
          <h2 class="flex items-center gap-2 text-sm font-semibold text-text">
            <Icons.Gear size={16} class="text-text-muted" />
            {t("Configuración")}
          </h2>
          <button
            class="w-7 h-7 flex items-center justify-center rounded text-text-muted hover:text-text hover:bg-surface/40"
            aria-label={t("Cerrar")}
            onClick={props.onClose}
          >
            <Icons.X size={14} />
          </button>
        </div>

        <div class="p-4 space-y-4">
          <section class="space-y-3">
            <h3 class="text-[11px] uppercase tracking-wide text-text-muted">{t("Apariencia")}</h3>
            <Select
              label={t("Tema")}
              options={THEME_OPTIONS}
              value={props.theme}
              onChange={(theme) => props.onThemeChange(theme as ThemeSetting)}
            />
            <Select
              label={t("Idioma")}
              options={LOCALE_OPTIONS}
              value={localeSetting()}
              onChange={(l) => setLocale(l as LocaleSetting)}
            />
          </section>

          <section class="space-y-3 pt-3 border-t border-border">
            <h3 class="text-[11px] uppercase tracking-wide text-text-muted">{t("Proyecto")}</h3>
            <Checkbox
              label={t("Guardado automático")}
              checked={props.autosave.enabled}
              onChange={(enabled) => update({ enabled })}
            />
            <Show when={props.autosave.enabled}>
              <Slider
                label={t("Cada")}
                min={1}
                max={30}
                step={1}
                value={props.autosave.minutes}
                onChange={(minutes) => update({ minutes })}
                formatValue={(v) => tn(v, "{n} minuto", "{n} minutos")}
              />
              <p class="text-xs text-text-muted leading-relaxed">
                <Show
                  when={props.projectPath}
                  fallback={
                    <>
                      {t("El proyecto todavía no tiene archivo: se guarda en un archivo de recuperación, que se ofrece al abrir la app. Con")}{" "}
                      <span class="text-text">{t("Guardar")}</span> {t("(Ctrl+S) pasa a guardarse en el tuyo.")}
                    </>
                  }
                >
                  {t("Se guarda en")} <span class="text-text font-mono break-all">{props.projectPath}</span>.
                </Show>{" "}
                {t("Si nada cambió, no se reescribe.")}
              </p>
            </Show>
          </section>

          <section class="space-y-3 pt-3 border-t border-border">
            <h3 class="text-[11px] uppercase tracking-wide text-text-muted">{t("Grilla y unidades")}</h3>
            <Select
              label={t("Medir la grilla en")}
              options={GRID_UNIT_OPTIONS}
              value={props.grid.unit}
              onChange={(unit) => props.onGridChange({ ...props.grid, unit: unit as GridSettings["unit"] })}
            />
            <Select
              label={t("El modelo está en")}
              options={MODEL_UNIT_OPTIONS}
              value={props.grid.modelUnit}
              onChange={(modelUnit) =>
                props.onGridChange({ ...props.grid, modelUnit: modelUnit as GridSettings["modelUnit"] })
              }
            />
            <p class="text-xs text-text-muted leading-relaxed">
              <Show when={props.fileMetersPerUnit !== undefined}>
                {t("El archivo dice 1 unidad = {m} m.", { m: props.fileMetersPerUnit ?? "" })}{" "}
              </Show>
              {t("Los STL y OBJ no guardan unidades: si la grilla no calza con el tamaño real, elige acá en qué está el modelo. Solo cambia cómo se mide, no el modelo.")}
            </p>
          </section>
        </div>
      </div>
    </div>
  );
};
