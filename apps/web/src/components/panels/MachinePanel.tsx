import { Component, For, Show, createMemo, createResource, createSignal } from "solid-js";
import { clsx } from "clsx";
import { Button, Checkbox, NumberInput, Select } from "../ui";
import * as Icons from "../icons";
import { fmtNumber, t } from "../../lib/i18n";
import { CREATOR_1, filamentOf, loadSlicerInfo, machineCards, machineOf, machinesFrom, nozzleLabel, nozzlesOf, withFilament, withMachine, type Machine } from "../../lib/machines";
import { PrinterPicture } from "./PrinterSection";
import { netPrinters } from "../../lib/settings";
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
  /** Lleva a la pestaña «Mis impresoras» (las de la red) */
  onOpenPrinters?: () => void;
}> = (props) => {
  const [info] = createResource(loadSlicerInfo);
  // Memo: las máquinas se arman de nuevo solo si cambian los perfiles. Sin esto, cada lectura daba
  // objetos nuevos, <For> rehacía las tarjetas de filamento a cada cuadro y el clic se perdía
  const machines = createMemo(() => machinesFrom(info()?.printers));
  const current = (): Machine | undefined => machineOf(props.prep, machines());
  const slicerPrinter = () => info()?.printers.find((p) => p.name === current()?.id);
  const materials = () => props.presets?.materials ?? [];
  const patchPrep = (next: PrepSettings) => props.onPatch({ printer: next.printer, material: next.material, slicer: next.slicer });
  const choose = (m: Machine) => patchPrep(withMachine(props.prep, m));
  const nozzles = () => {
    const m = current();
    return m && m.kind === "fdm" && m.sliceable ? nozzlesOf(m, machines()) : [];
  };
  const filament = () => filamentOf(props.prep, slicerPrinter());
  const setFilament = (f: string) => patchPrep(withFilament(props.prep, f, materials()));
  const materialFor = (name: string): MaterialProfile => materials().find((m) => m.name === name) ?? props.material;

  const [details, setDetails] = createSignal(false);
  /** Las impresoras de la red de este modelo */
  const mine = () => netPrinters().filter((n) => n.model === current()?.model);
  const shrink = (): [number, number, number] => props.material.shrink ?? [0, 0, 0];
  const setShrink = (axis: number, percent: number) => {
    const next = [...shrink()] as [number, number, number];
    next[axis] = Math.max(0, percent) / 100;
    props.onPatch({ material: { ...props.material, shrink: next } });
  };
  const setPrinter = (p: Partial<PrinterProfile>) => props.onPatch({ printer: { ...props.printer, ...p } });

  return (
    <div class="space-y-3">
      <div class="grid gap-1.5 [grid-template-columns:repeat(auto-fill,minmax(min(100%,170px),1fr))]">
        <For each={machineCards(machines(), current())}>
          {(m) => (
            <button
              type="button"
              onClick={() => choose(m)}
              class={clsx(
                "flex items-center gap-2 rounded border p-1.5 text-left transition-colors",
                current()?.model === m.model ? "border-orange bg-orange/10" : "border-border hover:border-orange/50"
              )}
            >
              <PrinterPicture model={m.model} size={52} />
              <span class="min-w-0">
                <span class="block text-xs font-semibold text-text break-words">{m.name}</span>
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

      <Show when={mine().length > 0}>
        <p class="text-[11px] text-text-muted">
          {t("Tus impresoras: {names}", { names: mine().map((n) => n.name).join(", ") })}
          <Show when={props.onOpenPrinters}>
            {" "}
            <button class="text-accent hover:underline" onClick={() => props.onOpenPrinters?.()}>
              {t("Ver en Mis impresoras")}
            </button>
          </Show>
        </p>
      </Show>

      <Show when={nozzles().length > 1}>
        <Select
          label={t("Boquilla")}
          options={nozzles().map((m) => ({ value: m.id, label: nozzleLabel(m.nozzle) }))}
          value={current()?.id}
          onChange={(id) => {
            const m = nozzles().find((x) => x.id === id);
            if (m) choose(m);
          }}
        />
      </Show>

      <Show when={current()?.kind === "resin"}>
        <p class="text-[10px] text-text-dim">{t("Impresora de resina: se lamina con el laminador propio de Pinocchio (soportes, venteos y capas) en Imprimir → Laminar.")}</p>
      </Show>

      <Show when={current()?.id === CREATOR_1.id}>
        <p class="text-[10px] text-text-dim">
          {t("La Creator Pro lee archivos .x3g: se puede preparar y exportar, pero para laminarla hay que cambiarle el firmware.")}
        </p>
      </Show>

      <Show
        when={slicerPrinter()}
        fallback={
          <div class="space-y-0.5">
            <Select
              label={t("Material")}
              options={(props.presets?.materials ?? []).map((m) => ({ value: m.name, label: m.name }))}
              value={props.material.name}
              onChange={(name) => props.onPatch({ material: materialFor(name) })}
            />
            <p class="text-[10px] text-text-dim">{t("Con el material se calculan el peso, los voladizos y las holguras.")}</p>
          </div>
        }
      >
        {(p) => (
          <div class="space-y-0.5">
            <Select
              label={t("Filamento")}
              options={p().filaments.map((f) => ({ value: f, label: shortName(f) }))}
              value={filament()}
              onChange={setFilament}
            />
            <p class="text-[10px] text-text-dim">{t("Es el perfil del laminador; también fija el material para preparar.")}</p>
          </div>
        )}
      </Show>

      {/* Poner a punto: lo principal; las holguras a mano quedan plegadas */}
      <div class="space-y-1.5 rounded-md border border-border p-2">
        <p class="text-xs font-semibold text-text">{t("Holguras y calibración")}</p>
        <p class="text-[10px] text-text-dim">{t("Se miden con piezas de prueba y quedan guardadas para esta impresora y este filamento.")}</p>
        <Button onClick={() => (props.onTune ?? props.onCalibrate)()} size="sm" variant="primary" fullWidth icon={<Icons.Ruler size={14} />}>
          {t("Poner a punto")}
        </Button>
      </div>

      <button class="flex items-center gap-1 text-[11px] text-text-muted hover:text-text" onClick={() => setDetails(!details())}>
        <Show when={details()} fallback={<Icons.CaretRight size={12} />}>
          <Icons.CaretDown size={12} />
        </Show>
        {t("Ajustes a mano")}
      </button>
      <Show when={details()}>
        <div class="space-y-2 pl-3 border-l border-border">
          <div class="space-y-0.5">
            <NumberInput
              label={t("Holgura de encaje")}
              value={props.printer.fit_clearance ?? props.printer.nozzle}
              onChange={(v) => setPrinter({ fit_clearance: Math.max(0, v) })}
              min={0}
              max={2}
              step={0.05}
              suffix="mm"
            />
            <p class="text-[10px] text-text-dim">{t("Espacio que se deja entre dos piezas que encajan.")}</p>
          </div>
          <div class="space-y-0.5">
            <NumberInput
              label={t("Agujeros más anchos")}
              value={props.printer.hole_comp ?? props.printer.nozzle * 0.25}
              onChange={(v) => setPrinter({ hole_comp: Math.max(0, v) })}
              min={0}
              max={1}
              step={0.05}
              suffix="mm"
            />
            <p class="text-[10px] text-text-dim">{t("Lo que se agrandan los agujeros, porque la impresora los deja más chicos.")}</p>
          </div>
          <div class="grid grid-cols-3 gap-1.5">
            <For each={["X", "Y", "Z"]}>
              {(axis, i) => <NumberInput label={axis} value={Number((shrink()[i()] * 100).toFixed(3))} onChange={(v) => setShrink(i(), v)} min={0} max={5} step={0.05} />}
            </For>
          </div>
          <p class="text-[10px] text-text-dim">{t("Contracción del material por eje (%)")}</p>
          <div class="space-y-0.5">
            <NumberInput
              label={t("Holgura según el material")}
              value={props.material.clearance_factor ?? 1}
              onChange={(v) => props.onPatch({ material: { ...props.material, clearance_factor: Math.max(0.1, v) } })}
              min={0.1}
              max={4}
              step={0.05}
              suffix="×"
            />
            <p class="text-[10px] text-text-dim">{t("Multiplica las holguras: más de 1 si el material se aprieta o se contrae mucho.")}</p>
          </div>
          <Show when={props.onTune}>
            <Button onClick={props.onCalibrate} size="sm" fullWidth variant="ghost" icon={<Icons.Ruler size={14} />}>
              {t("Solo holguras y medidas (sin impresora conectada)")}
            </Button>
          </Show>
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
