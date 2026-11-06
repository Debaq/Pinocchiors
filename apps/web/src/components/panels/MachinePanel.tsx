import { Component, For, Show, createResource, createSignal } from "solid-js";
import { clsx } from "clsx";
import { Button, Checkbox, NumberInput, Select } from "../ui";
import * as Icons from "../icons";
import { fmtNumber, t } from "../../lib/i18n";
import { CREATOR_1, filamentOf, loadSlicerInfo, machineOf, machinesFrom, withFilament, withMachine, type Machine } from "../../lib/machines";
import { PrinterPicture } from "./PrinterSection";
import type { MaterialProfile, PrepSettings, PrintPresets, PrinterProfile } from "./PrintPrepPanel";

/** Nombre corto de un perfil: «Flashforge PLA Basic @FF AD5X» → «Flashforge PLA Basic» */
const shortName = (name: string) => name.split("@")[0].trim();

/**
 * La impresora de Fabricar, una sola vez para todo: de ella salen la cama y la boquilla
 * para preparar, y el perfil del laminador. El filamento elige el material (densidad,
 * voladizos, holguras) y el perfil de filamento del laminador.
 */
export const MachinePanel: Component<{
  prep: PrepSettings;
  onPatch: (p: Partial<PrepSettings>) => void;
  presets?: PrintPresets;
  printer: PrinterProfile;
  material: MaterialProfile;
  onCalibrate: () => void;
  /** Asistente «Poner a punto» (con la impresora conectada) */
  onTune?: () => void;
}> = (props) => {
  const [info] = createResource(loadSlicerInfo);
  const machines = () => machinesFrom(info()?.printers);
  const current = (): Machine | undefined => machineOf(props.prep, machines());
  const slicerPrinter = () => info()?.printers.find((p) => p.name === current()?.id);
  const materials = () => props.presets?.materials ?? [];
  const patchPrep = (next: PrepSettings) => props.onPatch({ printer: next.printer, material: next.material, slicer: next.slicer });
  const choose = (m: Machine) => patchPrep(withMachine(props.prep, m));
  const filament = () => filamentOf(props.prep, slicerPrinter());
  const setFilament = (f: string) => patchPrep(withFilament(props.prep, f, materials()));
  const materialFor = (name: string): MaterialProfile => materials().find((m) => m.name === name) ?? props.material;

  const [details, setDetails] = createSignal(false);
  const shrink = (): [number, number, number] => props.material.shrink ?? [0, 0, 0];
  const setShrink = (axis: number, percent: number) => {
    const next = [...shrink()] as [number, number, number];
    next[axis] = Math.max(0, percent) / 100;
    props.onPatch({ material: { ...props.material, shrink: next } });
  };
  const setPrinter = (p: Partial<PrinterProfile>) => props.onPatch({ printer: { ...props.printer, ...p } });

  return (
    <div class="space-y-3">
      <div class="grid grid-cols-2 gap-1.5">
        <For each={machines()}>
          {(m) => (
            <button
              type="button"
              onClick={() => choose(m)}
              class={clsx(
                "flex items-center gap-2 rounded border p-1.5 text-left transition-colors",
                current()?.id === m.id ? "border-orange bg-orange/10" : "border-border hover:border-orange/50"
              )}
            >
              <PrinterPicture model={m.model} size={52} />
              <span class="min-w-0">
                <span class="block text-xs font-semibold text-text truncate">{m.name}</span>
                <span class="block text-[10px] text-text-dim">{m.bed.map((v) => fmtNumber(v)).join(" × ")} mm</span>
                <Show when={m.tools > 1}>
                  <span class="block text-[10px] text-text-dim">{t("{n} carretes", { n: m.tools })}</span>
                </Show>
                <Show when={!m.sliceable}>
                  <span class="block text-[10px] text-warning">{t("Sin laminar todavía")}</span>
                </Show>
              </span>
            </button>
          )}
        </For>
      </div>

      <Show when={current()?.kind === "resin"}>
        <p class="text-[10px] text-text-dim">{t("Impresora de resina: se lamina con el laminador propio de Pinocchio (soportes, venteos y capas) en Imprimir → Laminar.")}</p>
      </Show>

      <Show when={current()?.id === CREATOR_1.id}>
        <p class="text-[10px] text-text-dim">
          {t("La Creator original lee archivos .x3g: se puede preparar y exportar, pero para laminarla hay que cambiarle el firmware.")}
        </p>
      </Show>

      <Show
        when={slicerPrinter()}
        fallback={
          <Select
            label={t("Material")}
            options={(props.presets?.materials ?? []).map((m) => ({ value: m.name, label: m.name }))}
            value={props.material.name}
            onChange={(name) => props.onPatch({ material: materialFor(name) })}
          />
        }
      >
        {(p) => (
          <Select
            label={t("Filamento")}
            options={p().filaments.map((f) => ({ value: f, label: shortName(f) }))}
            value={filament()}
            onChange={setFilament}
          />
        )}
      </Show>

      <button class="flex items-center gap-1 text-[11px] text-text-muted hover:text-text" onClick={() => setDetails(!details())}>
        <Show when={details()} fallback={<Icons.CaretRight size={12} />}>
          <Icons.CaretDown size={12} />
        </Show>
        {t("Holguras y calibración")}
      </button>
      <Show when={details()}>
        <div class="space-y-2 pl-3 border-l border-border">
          <div class="grid grid-cols-2 gap-1.5">
            <NumberInput
              label={t("Holgura")}
              value={props.printer.fit_clearance ?? props.printer.nozzle}
              onChange={(v) => setPrinter({ fit_clearance: Math.max(0, v) })}
              min={0}
              max={2}
              step={0.05}
              suffix="mm"
            />
            <NumberInput
              label={t("Agujeros")}
              value={props.printer.hole_comp ?? props.printer.nozzle * 0.25}
              onChange={(v) => setPrinter({ hole_comp: Math.max(0, v) })}
              min={0}
              max={1}
              step={0.05}
              suffix="mm"
            />
          </div>
          <div class="grid grid-cols-3 gap-1.5">
            <For each={["X", "Y", "Z"]}>
              {(axis, i) => <NumberInput label={axis} value={Number((shrink()[i()] * 100).toFixed(3))} onChange={(v) => setShrink(i(), v)} min={0} max={5} step={0.05} />}
            </For>
          </div>
          <p class="text-[10px] text-text-dim">{t("Contracción del material por eje (%)")}</p>
          <NumberInput
            label={t("Factor de holgura")}
            value={props.material.clearance_factor ?? 1}
            onChange={(v) => props.onPatch({ material: { ...props.material, clearance_factor: Math.max(0.1, v) } })}
            min={0.1}
            max={4}
            step={0.05}
            suffix="×"
          />
          <Show when={props.onTune}>
            <Button onClick={() => props.onTune?.()} size="sm" fullWidth icon={<Icons.Ruler size={14} />}>
              {t("Poner a punto: calibrar impresora y filamento")}
            </Button>
          </Show>
          <Button onClick={props.onCalibrate} size="sm" fullWidth variant="ghost" icon={<Icons.Ruler size={14} />}>
            {t("Solo holguras y medidas (sin impresora conectada)")}
          </Button>
          <Checkbox
            small
            label={t("Compensar los huecos del diseño")}
            checked={!!props.prep.compensateAll}
            onChange={(compensateAll) => props.onPatch({ compensateAll })}
            hint={t("En Diseñar, los agujeros, las roscas interiores, las tuercas, las arandelas y los ejes y chaveteros de los engranes salen ya más anchos lo que esta impresora los achica. Cambia la medida de lo que ves.")}
          />
        </div>
      </Show>
    </div>
  );
};
