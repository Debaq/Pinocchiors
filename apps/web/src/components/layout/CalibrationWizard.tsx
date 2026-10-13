import { Component, For, Show, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { Button, Checkbox, NumberInput, Select } from "../ui";
import * as Icons from "../icons";
import { fmtNumber, t, tx } from "../../lib/i18n";
import { tBackend } from "../../lib/i18nBackend.ts";
import type { MaterialProfile, PrinterProfile } from "../panels/PrintPrepPanel";

/** El bloque de calibración que se descarga (los mismos números que el modelo) */
const BLOCK = { height: 20, depth: 30, holes: [4, 6, 8, 10] };
/** Medidas nominales del bloque (X, Y, Z): 5 mm de pared entre agujeros y a los lados */
const BLOCK_SIZE: [number, number, number] = [5 + 4 + 5 + 6 + 5 + 8 + 5 + 10 + 5, BLOCK.depth, BLOCK.height];

interface Result {
  fit_clearance: number | null;
  shrink: [number, number, number] | null;
  hole_comp: number | null;
}

export interface CalibrationApply {
  printer: PrinterProfile;
  material: MaterialProfile;
  /** Nombres con que guardar los perfiles calibrados (sin nombre: solo se aplican) */
  savePrinterAs?: string;
  saveMaterialAs?: string;
}

/**
 * Asistente de calibración: se descargan dos piezas de prueba (placa de holguras y bloque medible),
 * se imprimen sin compensar, y de lo que se mide salen la holgura de los ajustes, la contracción
 * del material por eje y cuánto más chicos salen los agujeros.
 */
export const CalibrationWizard: Component<{
  printer: PrinterProfile;
  material: MaterialProfile;
  onApply: (r: CalibrationApply) => void;
  onClose: () => void;
}> = (props) => {
  const [step, setStep] = createSignal(0);
  const STEPS = [t("Cómo funciona"), t("Holgura de los ajustes"), t("Contracción y agujeros"), t("Resultado")];

  // Paso 1: prueba de holgura
  const [diameter, setDiameter] = createSignal(8);
  const [start, setStart] = createSignal(0.1);
  const [increment, setIncrement] = createSignal(0.1);
  const [count, setCount] = createSignal(6);
  const clearances = createMemo(() => Array.from({ length: Math.max(1, Math.min(9, count())) }, (_, i) => Math.round((start() + increment() * i) * 1000) / 1000));
  const [pick, setPick] = createSignal("none");

  // Paso 2: bloque
  const [size, setSize] = createSignal<[number | null, number | null, number | null]>([null, null, null]);
  const [holes, setHoles] = createSignal<(number | null)[]>(BLOCK.holes.map(() => null));

  const [format, setFormat] = createSignal("stl");
  const [message, setMessage] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [result, setResult] = createSignal<Result | undefined>();
  const [saveProfiles, setSaveProfiles] = createSignal(true);
  const [printerName, setPrinterName] = createSignal(`${props.printer.name} (${t("calibrada")})`);
  const [materialName, setMaterialName] = createSignal(`${props.material.name} (${t("calibrado")})`);

  onMount(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        props.onClose();
      } else if (e.key !== "Tab") {
        e.stopPropagation();
      }
    };
    window.addEventListener("keydown", onKey, true);
    onCleanup(() => window.removeEventListener("keydown", onKey, true));
  });

  const download = async (name: string, model: object) => {
    try {
      const path = await save({ title: t("Guardar pieza de prueba"), defaultPath: `${name}.${format()}`, filters: [{ name: format().toUpperCase(), extensions: [format()] }] });
      if (!path) return;
      setBusy(true);
      const bytes = await invoke<number>("export_calibration_model", { model, format: format(), path });
      setMessage(t("Guardado: {file} ({kb} KB)", { file: path.split("/").pop() ?? path, kb: fmtNumber(bytes / 1024, { maximumFractionDigits: 0 }) }));
    } catch (e) {
      setMessage(tBackend(String(e)));
    } finally {
      setBusy(false);
    }
  };

  const measures = () => {
    const [x, y, z] = size();
    const done = holes().filter((h): h is number => h !== null);
    return {
      fit_clearance: pick() === "none" ? null : clearances()[Number(pick())],
      block_size: x !== null && y !== null && z !== null ? [x, y, z] : null,
      // Solo los primeros agujeros seguidos que se midieron
      block_holes: holes().findIndex((h) => h === null) === -1 ? done : done.slice(0, holes().findIndex((h) => h === null)),
    };
  };

  const solve = async () => {
    try {
      setBusy(true);
      setMessage("");
      const r = await invoke<Result>("solve_print_calibration", { block: BLOCK, measures: measures() });
      setResult(r);
      setStep(3);
    } catch (e) {
      setMessage(tBackend(String(e)));
    } finally {
      setBusy(false);
    }
  };

  const nothingMeasured = () => {
    const m = measures();
    return m.fit_clearance === null && m.block_size === null && m.block_holes.length === 0;
  };

  const apply = () => {
    const r = result();
    if (!r) return;
    const factor = props.material.clearance_factor ?? 1;
    const printer: PrinterProfile = { ...props.printer };
    if (r.fit_clearance !== null) printer.fit_clearance = Math.round((r.fit_clearance / factor) * 1000) / 1000;
    if (r.hole_comp !== null) printer.hole_comp = Math.max(0, Math.round(r.hole_comp * 1000) / 1000);
    const material: MaterialProfile = { ...props.material };
    if (r.shrink !== null) material.shrink = r.shrink.map((s) => Math.max(0, Math.round(s * 100000) / 100000)) as [number, number, number];
    const changedPrinter = r.fit_clearance !== null || r.hole_comp !== null;
    const changedMaterial = r.shrink !== null;
    props.onApply({
      printer: saveProfiles() && changedPrinter && printerName().trim() ? { ...printer, name: printerName().trim() } : printer,
      material: saveProfiles() && changedMaterial && materialName().trim() ? { ...material, name: materialName().trim() } : material,
      savePrinterAs: saveProfiles() && changedPrinter ? printerName().trim() || undefined : undefined,
      saveMaterialAs: saveProfiles() && changedMaterial ? materialName().trim() || undefined : undefined,
    });
    props.onClose();
  };

  const pct = (v: number) => `${fmtNumber(v * 100, { maximumFractionDigits: 2 })} %`;
  const mm = (v: number, digits = 2) => `${fmtNumber(v, { maximumFractionDigits: digits })} mm`;

  return (
    <div class="fixed inset-0 z-50 flex items-center justify-center bg-bg-darker/70 backdrop-blur-sm p-4" onPointerDown={(e) => e.target === e.currentTarget && props.onClose()}>
      <div role="dialog" aria-label={t("Calibrar la impresora")} class="w-full max-w-xl max-h-full flex flex-col rounded-lg border border-border bg-bg shadow-xl">
        <div class="flex items-center justify-between px-4 h-11 border-b border-border shrink-0">
          <h2 class="flex items-center gap-2 text-sm font-semibold text-text">
            <Icons.Ruler size={16} class="text-accent" />
            {t("Calibrar la impresora")} · {props.printer.name} · {props.material.name}
          </h2>
          <button class="w-7 h-7 flex items-center justify-center rounded text-text-muted hover:text-text hover:bg-surface/40" aria-label={t("Cerrar")} onClick={props.onClose}>
            <Icons.X size={14} />
          </button>
        </div>
        <div class="flex gap-1 px-4 pt-3">
          <For each={STEPS}>
            {(name, i) => (
              <div class={`flex-1 text-[10px] pb-1 border-b-2 ${i() === step() ? "border-accent text-text" : i() < step() ? "border-accent/40 text-text-muted" : "border-border text-text-dim"}`}>
                {i() + 1}. {name}
              </div>
            )}
          </For>
        </div>

        <div class="p-4 space-y-3 overflow-y-auto min-h-0 text-xs">
          <Show when={step() === 0}>
            <p class="text-text-muted leading-relaxed">
              {t("Las holguras de los ajustes y la contracción del material dependen de tu impresora, tu filamento y tu perfil de corte. Imprime dos piezas de prueba, mide lo que salió y el asistente saca los números.")}
            </p>
            <ol class="list-decimal pl-5 space-y-1 text-text">
              <li>{t("Placa de holgura: una clavija y agujeros que van de justos a flojos. Dices en cuál entra bien.")}</li>
              <li>{t("Bloque de calibración: lo mides con un calibre (largo, fondo, alto y el diámetro de sus agujeros).")}</li>
            </ol>
            <p class="text-text-muted leading-relaxed">
              {t("Imprime con el mismo material, boquilla y perfil con que vas a fabricar, sin soportes, y sin ninguna compensación. Puedes hacer solo una de las dos pruebas.")}
            </p>
          </Show>

          <Show when={step() === 1}>
            <p class="text-text-muted leading-relaxed">{t("La placa lleva agujeros del diámetro de la clavija más una holgura creciente, numerados en relieve.")}</p>
            <div class="grid grid-cols-2 gap-2">
              <NumberInput label={t("Clavija")} value={diameter()} onChange={(v) => setDiameter(Math.max(3, Math.min(30, v)))} min={3} max={30} step={1} suffix="mm" />
              <NumberInput label={t("Agujeros")} value={count()} onChange={(v) => setCount(Math.max(2, Math.min(9, Math.round(v))))} min={2} max={9} step={1} />
              <NumberInput label={t("Primera holgura")} value={start()} onChange={(v) => setStart(Math.max(0, Math.min(1, v)))} min={0} max={1} step={0.05} suffix="mm" />
              <NumberInput label={tx("calibración", "Paso")} value={increment()} onChange={(v) => setIncrement(Math.max(0.01, Math.min(0.5, v)))} min={0.01} max={0.5} step={0.05} suffix="mm" />
            </div>
            <div class="flex gap-2 items-end">
              <div class="w-24">
                <Select label={t("Formato")} options={[{ value: "stl", label: "STL" }, { value: "3mf", label: "3MF" }]} value={format()} onChange={setFormat} />
              </div>
              <Button size="sm" variant="primary" disabled={busy()} onClick={() => void download("prueba_holgura", { kind: "clearance", diameter: diameter(), clearances: clearances() })} icon={<Icons.Download size={14} />}>
                {t("Descargar placa de holgura")}
              </Button>
            </div>
            <p class="text-text-muted leading-relaxed">
              {t("Ya impresa, prueba la clavija en cada agujero, del 1 en adelante. ¿En cuál entra justa: con la mano, sin forzar y sin que baile?")}
            </p>
            <Select
              label={t("Entra justa en")}
              options={[{ value: "none", label: t("No hice esta prueba") }, ...clearances().map((c, i) => ({ value: String(i), label: t("Agujero {n}: {c} mm de holgura", { n: i + 1, c: fmtNumber(c, { maximumFractionDigits: 3 }) }) }))]}
              value={pick()}
              onChange={setPick}
            />
          </Show>

          <Show when={step() === 2}>
            <p class="text-text-muted leading-relaxed">
              {t("El bloque mide {x} × {y} × {z} mm: largo en X, fondo en Y y alto en Z. Lleva cuatro agujeros pasantes, de menor a mayor a lo largo de X.", { x: BLOCK_SIZE[0], y: BLOCK_SIZE[1], z: BLOCK_SIZE[2] })}
            </p>
            <div class="flex gap-2 items-end">
              <div class="w-24">
                <Select label={t("Formato")} options={[{ value: "stl", label: "STL" }, { value: "3mf", label: "3MF" }]} value={format()} onChange={setFormat} />
              </div>
              <Button size="sm" variant="primary" disabled={busy()} onClick={() => void download("bloque_calibracion", { kind: "block", ...BLOCK })} icon={<Icons.Download size={14} />}>
                {t("Descargar bloque")}
              </Button>
            </div>
            <p class="text-text-muted leading-relaxed">{t("Ya impreso y frío, mídelo con el calibre (en mm, con dos decimales). Lo que no midas se deja vacío.")}</p>
            <div class="grid grid-cols-3 gap-2">
              <For each={["X", "Y", "Z"]}>
                {(axis, i) => (
                  <NumberInput
                    label={axis}
                    value={size()[i()] ?? undefined}
                    onChange={(v) => setSize((s) => s.map((x, k) => (k === i() ? v : x)) as [number | null, number | null, number | null])}
                    min={1}
                    max={200}
                    step={0.01}
                    suffix="mm"
                  />
                )}
              </For>
            </div>
            <p class="text-[10px] text-text-dim">{t("Medidas externas (modelo: {x} × {y} × {z} mm)", { x: BLOCK_SIZE[0], y: BLOCK_SIZE[1], z: BLOCK_SIZE[2] })}</p>
            <div class="grid grid-cols-2 gap-2">
              <For each={BLOCK.holes}>
                {(nominal, i) => (
                  <NumberInput
                    label={`Ø${nominal}`}
                    value={holes()[i()] ?? undefined}
                    onChange={(v) => setHoles((h) => h.map((x, k) => (k === i() ? v : x)))}
                    min={0.5}
                    max={30}
                    step={0.01}
                    suffix="mm"
                  />
                )}
              </For>
            </div>
            <p class="text-[10px] text-text-dim">{t("Diámetro medido de cada agujero, en orden de menor a mayor; si mides solo algunos, los primeros.")}</p>
          </Show>

          <Show when={step() === 3 && result()}>
            {(r) => (
              <div class="space-y-3">
                <div class="space-y-1 p-3 rounded-md bg-surface/30 border border-border">
                  <Show when={r().fit_clearance !== null}>
                    <div class="flex justify-between"><span class="text-text-muted">{t("Holgura de un ajuste normal")}</span><span class="font-mono text-text">{mm(r().fit_clearance!, 3)}</span></div>
                  </Show>
                  <Show when={r().shrink}>
                    {(s) => (
                      <div class="flex justify-between"><span class="text-text-muted">{t("Contracción X · Y · Z")}</span><span class="font-mono text-text">{s().map(pct).join(" · ")}</span></div>
                    )}
                  </Show>
                  <Show when={r().hole_comp !== null}>
                    <div class="flex justify-between"><span class="text-text-muted">{t("Los agujeros salen más chicos, por lado")}</span><span class="font-mono text-text">{mm(r().hole_comp!, 3)}</span></div>
                  </Show>
                </div>
                <p class="text-text-muted leading-relaxed">
                  {t("Se aplican a la impresora (holgura y agujeros) y al material (contracción). La holgura se reparte con el factor del material para que un ajuste normal de este material sea justo el que elegiste.")}
                </p>
                <Checkbox small label={t("Guardar como perfiles propios")} checked={saveProfiles()} onChange={setSaveProfiles} />
                <Show when={saveProfiles()}>
                  <div class="grid grid-cols-2 gap-2">
                    <Show when={r().fit_clearance !== null || r().hole_comp !== null}>
                      <label class="block space-y-1">
                        <span class="text-text-muted">{t("Impresora")}</span>
                        <input class="w-full px-2 py-1 rounded bg-surface/40 border border-border text-text outline-none focus:border-accent" value={printerName()} onInput={(e) => setPrinterName(e.currentTarget.value)} />
                      </label>
                    </Show>
                    <Show when={r().shrink !== null}>
                      <label class="block space-y-1">
                        <span class="text-text-muted">{t("Material")}</span>
                        <input class="w-full px-2 py-1 rounded bg-surface/40 border border-border text-text outline-none focus:border-accent" value={materialName()} onInput={(e) => setMaterialName(e.currentTarget.value)} />
                      </label>
                    </Show>
                  </div>
                </Show>
              </div>
            )}
          </Show>

          <Show when={message()}>
            <p class="text-warning leading-relaxed">{message()}</p>
          </Show>
        </div>

        <div class="flex items-center justify-between gap-2 px-4 py-3 border-t border-border shrink-0">
          <Button size="sm" variant="ghost" onClick={() => (step() === 0 ? props.onClose() : setStep(step() - 1))}>
            {step() === 0 ? t("Cancelar") : t("Atrás")}
          </Button>
          <Show when={step() < 2}>
            <Button size="sm" variant="primary" onClick={() => setStep(step() + 1)}>
              {t("Siguiente")}
            </Button>
          </Show>
          <Show when={step() === 2}>
            <Button size="sm" variant="primary" disabled={busy() || nothingMeasured()} loading={busy()} onClick={() => void solve()}>
              {t("Calcular")}
            </Button>
          </Show>
          <Show when={step() === 3}>
            <Button size="sm" variant="primary" onClick={apply}>
              {t("Aplicar")}
            </Button>
          </Show>
        </div>
      </div>
    </div>
  );
};
