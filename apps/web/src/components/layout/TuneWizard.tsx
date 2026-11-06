import { Component, For, Match, Show, Switch, createEffect, createMemo, createResource, createSignal, on, onCleanup, onMount } from "solid-js";
import { Channel, invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { clsx } from "clsx";
import { Button, NumberInput, Select } from "../ui";
import * as Icons from "../icons";
import { fmtNumber, t } from "../../lib/i18n";
import { tBackend } from "../../lib/i18nBackend.ts";
import { loadSlicerInfo } from "../../lib/machines";
import { netPrinters, selectNetPrinter, type NetPrinter } from "../../lib/settings";
import {
  KEY_FORMAT,
  TUNE_STEPS,
  calTarget,
  fmtCal,
  fmtCalDate,
  getCalibration,
  stepDate,
  stepDone,
  stepStale,
  type CalibrationDefaults,
  type CalibrationView,
  type Reading,
  type TuneStep,
} from "../../lib/calibration";
import { PrinterPicture, SendToPrinter } from "../panels/PrinterSection";
import type { ConfirmRequest } from "./ConfirmDialog";

/** Resultado de laminar una prueba (espejo de `CalibrationSlice` en slicer.rs) */
interface CalibrationSlice {
  result: { time_s: number; filament_g: number; layers: number; warnings: string[] };
  reading: Reading;
  unit: string;
  start: number;
  end: number;
  step: number;
}

interface Progress {
  stage: string;
  percent: number;
  message: string;
}

/** Imagen para el chat de IA */
export interface ChatPicture {
  mime: string;
  data: string;
}

/** Las piezas de medidas (las mismas que arma el backend por defecto) */
const CLEARANCES = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6];
const BLOCK = { height: 20, depth: 30, holes: [4, 6, 8, 10] };

const shortName = (name: string) => name.split("@")[0].trim();
const errText = (e: unknown) => (typeof e === "string" ? tBackend(e) : String(e));
const conn = (p: NetPrinter) => ({ kind: p.kind, address: p.address, apiKey: p.apiKey.trim() || null, checkCode: p.checkCode.trim() || null, serial: p.serial.trim() || null });

/**
 * Asistente «Poner a punto»: calibra una impresora física con un filamento, paso a paso y en el
 * orden recomendado. Cada paso lamina su prueba (con lo ya calibrado debajo), la manda a la
 * impresora o la guarda, y pregunta cuál salió mejor con un dibujo de la prueba. Lo elegido se
 * guarda por impresora y filamento (calibrations.rs) y el laminador lo usa solo.
 */
export const TuneWizard: Component<{
  /** Perfil de máquina y filamento del laminador con que abrir */
  machine?: string;
  filament?: string;
  confirm: (request: ConfirmRequest) => Promise<boolean>;
  /** Abre el chat de IA con este mensaje (y fotos) listo para mandar */
  onAskAi?: (text: string, images: ChatPicture[]) => void;
  /** Holguras, agujeros y contracción medidos: también van a los perfiles de Fabricar */
  onApplyProfiles?: (p: { fit_clearance?: number; hole_comp?: number; shrink?: [number, number, number] }) => void;
  onClose: () => void;
}> = (props) => {
  const [info] = createResource(loadSlicerInfo);
  const slicerPrinters = () => info()?.printers ?? [];
  /** Las impresoras de red que se pueden laminar (con perfil del laminador) */
  const printers = () => netPrinters().filter((n) => slicerPrinters().some((p) => p.model === n.model));
  const machineOf = (n?: NetPrinter) =>
    n ? (slicerPrinters().find((p) => p.name === props.machine && p.model === n.model) ?? slicerPrinters().find((p) => p.model === n.model)) : undefined;

  const [printerId, setPrinterId] = createSignal<string>();
  const printer = () => printers().find((p) => p.id === printerId()) ?? printers().find((p) => machineOf(p)?.name === props.machine) ?? printers()[0];
  const machine = () => machineOf(printer());
  const [filamentName, setFilamentName] = createSignal<string>();
  const filament = () => {
    const m = machine();
    if (!m) return undefined;
    const want = filamentName() ?? props.filament;
    return want && m.filaments.includes(want) ? want : m.default_filament;
  };
  const target = () => {
    const p = printer();
    const m = machine();
    const f = filament();
    return p && m && f ? calTarget(p, m.name, f) : undefined;
  };
  const settings = () => {
    const m = machine();
    return m && filament() ? { printer: m.name, process: m.default_process, filament: filament()!, overrides: {} } : undefined;
  };
  const klipper = () => printer()?.kind === "moonraker" || printer()?.kind === "creality";

  const [view, { refetch }] = createResource(target, (tgt) => getCalibration(tgt).catch(() => undefined));
  const values = () => view()?.values ?? {};

  // ── Paso abierto ──
  const [step, setStep] = createSignal<TuneStep>();
  const [defaults, setDefaults] = createSignal<CalibrationDefaults>();
  const [range, setRange] = createSignal<{ start: number; end: number; step: number }>();
  const [sliced, setSliced] = createSignal<CalibrationSlice>();
  const [busy, setBusy] = createSignal(false);
  const [progress, setProgress] = createSignal<Progress>();
  const [error, setError] = createSignal<string>();
  const [message, setMessage] = createSignal<string>();
  const [showAdvanced, setShowAdvanced] = createSignal(false);

  // Lo que se elige al final
  const [pickValue, setPickValue] = createSignal<number>();
  const [height, setHeight] = createSignal<number>();
  const [zTotal, setZTotal] = createSignal(0);
  const [clearancePick, setClearancePick] = createSignal<number>();
  const [blockSize, setBlockSize] = createSignal<[number | null, number | null, number | null]>([null, null, null]);
  const [blockHoles, setBlockHoles] = createSignal<(number | null)[]>(BLOCK.holes.map(() => null));

  onMount(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        if (step()) closeStep();
        else props.onClose();
      } else if (e.key !== "Tab") {
        e.stopPropagation();
      }
    };
    window.addEventListener("keydown", onKey, true);
    onCleanup(() => window.removeEventListener("keydown", onKey, true));
  });
  onCleanup(() => {
    if (busy()) void invoke("slicer_cancel");
  });

  // La impresora del asistente es la del envío
  createEffect(() => {
    const p = printer();
    if (p) selectNetPrinter(p.id);
  });

  const closeStep = () => {
    if (busy()) void invoke("slicer_cancel");
    setStep(undefined);
    setSliced(undefined);
    setDefaults(undefined);
    setRange(undefined);
    setError(undefined);
    setMessage(undefined);
    setPickValue(undefined);
    setHeight(undefined);
  };

  const openStep = async (s: TuneStep) => {
    closeStep();
    setStep(s);
    setZTotal(values()["z_offset"]?.value ?? 0);
    const st = settings();
    if (!st) return;
    try {
      const d = await invoke<CalibrationDefaults>("calibration_defaults", { settings: st, calibration: target(), test: s.test, range: null });
      setDefaults(d);
      setRange({ start: d.start, end: d.end, step: d.step });
    } catch (e) {
      setError(errText(e));
    }
  };

  // Cambiar de impresora o filamento cierra el paso abierto
  createEffect(on(() => JSON.stringify(target() ?? {}), () => closeStep(), { defer: true }));

  /** Los pasos que tocan a esta impresora */
  const steps = () => TUNE_STEPS.filter((s) => !s.klipper || klipper());
  const basic = () => steps().filter((s) => !s.advanced);
  const advanced = () => steps().filter((s) => s.advanced);
  const next = () => basic().find((s) => !stepDone(s, values()) || stepStale(s, values())) ?? advanced().find((s) => !stepDone(s, values()));

  const slice = async () => {
    const s = step();
    const st = settings();
    const r = range();
    if (!s || !st) return;
    setBusy(true);
    setError(undefined);
    setMessage(undefined);
    setSliced(undefined);
    const channel = new Channel<Progress>();
    channel.onmessage = (m) => {
      if (m.stage !== "warning") setProgress(m);
    };
    try {
      const res = await invoke<CalibrationSlice>("slice_calibration", {
        settings: st,
        calibration: target(),
        test: s.test,
        range: s.range && r ? { start: r.start, end: r.end, step: r.step } : null,
        onProgress: channel,
      });
      setSliced(res);
    } catch (e) {
      if (!/cancelad/i.test(String(e))) setError(errText(e));
    } finally {
      setBusy(false);
      setProgress(undefined);
    }
  };

  const saveGcode = async () => {
    const s = step();
    const path = await save({ title: t("Guardar gcode"), defaultPath: `calibracion_${s?.test ?? "prueba"}.gcode`, filters: [{ name: "G-code", extensions: ["gcode"] }] });
    if (!path) return;
    try {
      await invoke<number>("save_gcode", { path });
      setMessage(t("Guardado: {file}", { file: path.split("/").pop() ?? path }));
    } catch (e) {
      setError(errText(e));
    }
  };

  // ── Elegir ──
  const reading = () => sliced()?.reading ?? defaults()?.reading;
  /** Valor de la prueba a una altura (bloques o de a poco) */
  const valueAtHeight = (h: number): number | undefined => {
    const r = reading();
    if (!r) return undefined;
    if (r.kind === "blocks") {
      const last = r.blocks[r.blocks.length - 1];
      return r.blocks.find((b) => h >= b.z0 && h < b.z1)?.value ?? (last && h >= last.z1 ? last.value : r.blocks[0]?.value);
    }
    if (r.kind === "height") return r.start + (Math.max(0, Math.min(r.height_mm, h)) / r.height_mm) * (r.end - r.start);
    return undefined;
  };
  createEffect(() => {
    const h = height();
    if (h !== undefined) setPickValue(valueAtHeight(h));
  });

  /** Lo que se guarda: clave → valor */
  const toSave = createMemo((): Record<string, number> | undefined => {
    const s = step();
    if (!s) return undefined;
    switch (s.test) {
      case "first_layer":
        return { z_offset: Math.round(zTotal() * 1000) / 1000 };
      case "clearance": {
        const i = clearancePick();
        return i === undefined ? undefined : { fit_clearance: CLEARANCES[i] };
      }
      case "block":
        return undefined;
      case "volumetric_speed": {
        const v = pickValue();
        // Un 10 % menos que donde empezó a fallar, para ir seguro
        return v === undefined ? undefined : { max_volumetric_speed: Math.round(v * 0.9 * 10) / 10 };
      }
      case "shaper_frequency": {
        const v = pickValue();
        return v === undefined ? undefined : { shaper_freq_x: Math.round(v * 10) / 10, shaper_freq_y: Math.round(v * 10) / 10 };
      }
      default: {
        const v = pickValue();
        return v === undefined ? undefined : { [s.keys[0]]: v };
      }
    }
  });

  const store = async (entries: Record<string, number>, test: string) => {
    const tgt = target();
    if (!tgt) return;
    for (const [key, value] of Object.entries(entries)) {
      await invoke<CalibrationView>("calibration_save", { target: tgt, key, value, test, note: null });
    }
    await refetch();
  };

  const saveResult = async () => {
    const s = step();
    if (!s) return;
    setError(undefined);
    try {
      if (s.test === "block") {
        const [x, y, z] = blockSize();
        const holes = blockHoles();
        const firstMissing = holes.findIndex((h) => h === null);
        const measured = (firstMissing === -1 ? holes : holes.slice(0, firstMissing)) as number[];
        const r = await invoke<{ shrink: [number, number, number] | null; hole_comp: number | null }>("solve_print_calibration", {
          block: BLOCK,
          measures: { fit_clearance: null, block_size: x !== null && y !== null && z !== null ? [x, y, z] : null, block_holes: measured },
        });
        const entries: Record<string, number> = {};
        if (r.shrink) [entries.shrink_x, entries.shrink_y, entries.shrink_z] = r.shrink.map((v) => Math.max(0, Math.round(v * 100000) / 100000));
        if (r.hole_comp !== null) entries.hole_comp = Math.max(0, Math.round(r.hole_comp * 1000) / 1000);
        if (Object.keys(entries).length === 0) return setError(t("Mide al menos el bloque o un agujero"));
        await store(entries, "block");
        props.onApplyProfiles?.({ hole_comp: entries.hole_comp, shrink: r.shrink ? [entries.shrink_x, entries.shrink_y, entries.shrink_z] : undefined });
      } else {
        const entries = toSave();
        if (!entries) return;
        // Las K1 de fábrica y la AD5X guardan su propio Z: no se repite en el gcode
        const test = s.test === "first_layer" && printer()?.kind !== "moonraker" ? "printer" : s.test;
        await store(entries, test);
        if (s.test === "clearance") props.onApplyProfiles?.({ fit_clearance: entries.fit_clearance });
      }
      closeStep();
    } catch (e) {
      setError(errText(e));
    }
  };

  // ── Primera capa en vivo ──
  const canAdjustZ = () => printer()?.kind === "moonraker" || printer()?.kind === "creality";
  const adjustZ = async (delta: number) => {
    const p = printer();
    if (!p) return;
    try {
      await invoke("printer_control", { conn: conn(p), control: { kind: "z_offset", delta } });
      setZTotal((z) => Math.round((z + delta) * 1000) / 1000);
    } catch (e) {
      setError(errText(e));
    }
  };
  const autoLevel = async () => {
    const p = printer();
    if (!p) return;
    const ok = await props.confirm({
      title: t("Medir la cama"),
      message: t("{printer} va a calentar, ir al origen y medir toda la cama (unos minutos). Asegúrate de que la cama esté limpia y vacía.", { printer: p.name }),
      confirmLabel: t("Medir la cama"),
    });
    if (!ok) return;
    try {
      await invoke("printer_control", { conn: conn(p), control: { kind: "auto_level" } });
      setMessage(t("Midiendo la cama en {printer}...", { printer: p.name }));
    } catch (e) {
      setError(errText(e));
    }
  };

  /** Input shaping con el acelerómetro de la impresora (K1): mide y guarda en la impresora */
  const autoShaper = async () => {
    const p = printer();
    if (!p) return;
    const ok = await props.confirm({
      title: t("Medir vibraciones"),
      message: t("{printer} va a sacudir el cabezal y la cama para medir sus vibraciones con el acelerómetro y guardar el resultado en su configuración (unos minutos, hace ruido). Solo sirve si tiene acelerómetro (las K1 sí).", { printer: p.name }),
      confirmLabel: t("Medir vibraciones"),
    });
    if (!ok) return;
    try {
      await invoke("printer_control", { conn: conn(p), control: { kind: "input_shaper" } });
      setMessage(t("{printer} está midiendo sus vibraciones. Lo guarda ella misma: si sale bien, no hace falta imprimir la torre.", { printer: p.name }));
    } catch (e) {
      setError(errText(e));
    }
  };

  // ── IA ──
  const askAi = async () => {
    const s = step();
    const p = printer();
    if (!s || !p || !props.onAskAi) return;
    const r = reading();
    const lines = [
      t("Estoy calibrando {printer} ({machine}) con el filamento «{filament}» en Pinocchio.", { printer: p.name, machine: machine()?.name ?? "", filament: filament() ?? "" }),
      t("Paso: {title}. {fixes}", { title: t(s.title), fixes: t(s.fixes) }),
      t("Qué hay que mirar: {look}", { look: t(s.look) }),
    ];
    if (r?.kind === "blocks") lines.push(t("Bloques de abajo hacia arriba: {list}.", { list: r.blocks.map((b) => `${fmtNumber(b.z0)}–${fmtNumber(b.z1)} mm = ${fmtNumber(b.value, { maximumFractionDigits: 4 })}`).join("; ") }));
    if (r?.kind === "pieces") lines.push(t("Piezas (marca → valor): {list}.", { list: r.pieces.map((x) => `${x.mark} → ${fmtNumber(x.value, { maximumFractionDigits: 3 })}`).join("; ") }));
    if (r?.kind === "height") lines.push(t("El valor va de {a} (abajo) a {b} (a {h} mm de alto), de a poco.", { a: fmtNumber(r.start), b: fmtNumber(r.end), h: fmtNumber(r.height_mm) }));
    lines.push(t("Te adjunto una foto de cómo salió. Dime cuál es el mejor valor y por qué; si te parece bien, guárdalo con calibration_save para {printer}.", { printer: p.name }));
    // Si la impresora tiene cámara, va una foto
    const images: ChatPicture[] = [];
    try {
      const cams = await invoke<{ snapshot_url: string }[]>("printer_cameras", { conn: conn(p) });
      if (cams[0]) {
        const buf = await invoke<ArrayBuffer>("printer_snapshot", { conn: conn(p), url: cams[0].snapshot_url });
        const bytes = new Uint8Array(buf);
        let bin = "";
        for (let i = 0; i < bytes.length; i += 0x8000) bin += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
        images.push({ mime: "image/jpeg", data: btoa(bin) });
      }
    } catch {
      // Sin cámara: el usuario adjunta su foto
    }
    props.onAskAi(lines.join("\n"), images);
  };

  // ═══════════════════════════════════════════════════════════════════════
  // VISTA
  // ═══════════════════════════════════════════════════════════════════════

  const StepRow = (p: { s: TuneStep }) => {
    const done = () => stepDone(p.s, values());
    const stale = () => stepStale(p.s, values());
    const isNext = () => next()?.test === p.s.test;
    return (
      <button
        type="button"
        onClick={() => void openStep(p.s)}
        class={clsx(
          "w-full flex items-start gap-2 rounded-md border px-2.5 py-2 text-left transition-colors",
          isNext() ? "border-orange bg-orange/10" : "border-border hover:border-orange/50",
        )}
      >
        <span
          class={clsx(
            "mt-0.5 w-4 h-4 shrink-0 rounded-full flex items-center justify-center text-[9px]",
            done() && !stale() ? "bg-green/80 text-bg" : stale() ? "bg-warning/80 text-bg" : "border border-border text-text-dim",
          )}
        >
          <Show when={done()} fallback={TUNE_STEPS.indexOf(p.s) + 1}>
            <Icons.Check size={10} />
          </Show>
        </span>
        <span class="min-w-0 flex-1">
          <span class="flex items-center gap-2">
            <span class="text-xs font-semibold text-text">{t(p.s.title)}</span>
            <Show when={isNext()}>
              <span class="text-[9px] uppercase tracking-wider text-orange">{t("Sigue")}</span>
            </Show>
          </span>
          <span class="block text-[10px] text-text-dim leading-snug">{t(p.s.fixes)}</span>
          <Show when={done()}>
            <span class={clsx("block text-[10px] leading-snug", stale() ? "text-warning" : "text-text-muted")}>
              {p.s.keys
                .filter((k) => values()[k])
                .map((k) => `${t(KEY_FORMAT[k]?.label ?? k)} ${fmtCal(k, values()[k].value)}`)
                .join(" · ")}{" "}
              · {fmtCalDate(stepDate(p.s, values()) ?? "")}
              {stale() ? ` · ${t("conviene repetirlo")}` : ""}
            </span>
          </Show>
        </span>
        <Icons.CaretRight size={12} class="mt-1 text-text-dim shrink-0" />
      </button>
    );
  };

  /** Torre de bloques: se hace clic en el mejor */
  const BlocksPicker = (p: { blocks: { value: number; z0: number; z1: number }[]; unit: string }) => {
    const top = () => p.blocks[p.blocks.length - 1]?.z1 ?? 1;
    const many = () => p.blocks.length > 16;
    return (
      <Show
        when={!many()}
        fallback={
          <div class="space-y-1.5">
            <NumberInput label={t("Altura desde la cama")} value={height()} onChange={(v) => setHeight(Math.max(0, Math.min(top(), v)))} min={0} max={top()} step={0.5} suffix="mm" />
            <input type="range" class="w-full accent-orange" min={0} max={top()} step={0.1} value={height() ?? 0} onInput={(e) => setHeight(Number(e.currentTarget.value))} />
          </div>
        }
      >
        <div class="flex flex-col-reverse gap-0.5 w-48">
          <For each={p.blocks}>
            {(b) => (
              <button
                type="button"
                onClick={() => setPickValue(b.value)}
                class={clsx(
                  "flex items-center justify-between px-2 py-1 rounded border text-[11px] font-mono",
                  pickValue() === b.value ? "border-orange bg-orange/20 text-text" : "border-border bg-surface/30 text-text-muted hover:border-orange/50",
                )}
              >
                <span>{fmtNumber(b.value, { maximumFractionDigits: 4 })} {p.unit}</span>
                <span class="text-[9px] text-text-dim">{fmtNumber(b.z0)}–{fmtNumber(b.z1)} mm</span>
              </button>
            )}
          </For>
        </div>
      </Show>
    );
  };

  /** Piezas en la cama, dibujadas donde quedaron: se hace clic en la mejor */
  const PiecesPicker = (p: { pieces: { value: number; mark: string; x: number; y: number }[] }) => {
    const xs = () => p.pieces.map((x) => x.x);
    const ys = () => p.pieces.map((x) => x.y);
    const box = () => ({ x0: Math.min(...xs()) - 15, x1: Math.max(...xs()) + 15, y0: Math.min(...ys()) - 15, y1: Math.max(...ys()) + 15 });
    return (
      <svg viewBox={`${box().x0} ${-box().y1} ${box().x1 - box().x0} ${box().y1 - box().y0}`} class="w-full max-w-md h-56 rounded border border-border bg-surface/20">
        <For each={p.pieces}>
          {(piece) => (
            <g class="cursor-pointer" onClick={() => setPickValue(piece.value)}>
              <rect
                x={piece.x - 11}
                y={-piece.y - 11}
                width={22}
                height={22}
                rx={2}
                class={pickValue() === piece.value ? "fill-orange/40 stroke-orange" : "fill-surface/50 stroke-border hover:stroke-orange"}
                stroke-width={0.8}
              />
              <text x={piece.x} y={-piece.y + 3} text-anchor="middle" class="fill-text" style={{ "font-size": "8px" }}>
                {piece.mark}
              </text>
            </g>
          )}
        </For>
      </svg>
    );
  };

  const unit = () => sliced()?.unit ?? defaults()?.unit ?? "";

  return (
    <div class="fixed inset-0 z-50 flex items-center justify-center bg-bg-darker/70 backdrop-blur-sm p-4" onPointerDown={(e) => e.target === e.currentTarget && props.onClose()}>
      <div role="dialog" aria-label={t("Poner a punto")} class="w-full max-w-2xl max-h-full flex flex-col rounded-lg border border-border bg-bg shadow-xl">
        <div class="flex items-center justify-between px-4 h-11 border-b border-border shrink-0">
          <h2 class="flex items-center gap-2 text-sm font-semibold text-text min-w-0">
            <Icons.Ruler size={16} class="text-accent shrink-0" />
            <span class="truncate">
              {t("Poner a punto")}
              <Show when={step()}> · {t(step()!.title)}</Show>
            </span>
          </h2>
          <button class="w-7 h-7 flex items-center justify-center rounded text-text-muted hover:text-text hover:bg-surface/40" aria-label={t("Cerrar")} onClick={props.onClose}>
            <Icons.X size={14} />
          </button>
        </div>

        <div class="p-4 space-y-3 overflow-y-auto min-h-0 text-xs">
          <Show when={printers().length > 0} fallback={<p class="text-text-muted leading-relaxed">{t("Primero agrega tu impresora en Imprimir → Impresora: la calibración se guarda para cada impresora.")}</p>}>
            {/* ── Impresora y filamento ── */}
            <Show when={!step()}>
              <div class="grid grid-cols-2 gap-1.5">
                <For each={printers()}>
                  {(n) => (
                    <button
                      type="button"
                      onClick={() => setPrinterId(n.id)}
                      class={clsx("flex items-center gap-2 rounded border p-1.5 text-left", printer()?.id === n.id ? "border-orange bg-orange/10" : "border-border hover:border-orange/50")}
                    >
                      <PrinterPicture model={n.model} size={40} />
                      <span class="min-w-0">
                        <span class="block text-xs font-semibold text-text truncate">{n.name}</span>
                        <span class="block text-[10px] text-text-dim truncate">{n.model}</span>
                      </span>
                    </button>
                  )}
                </For>
              </div>
              <Select
                label={t("Filamento")}
                options={(machine()?.filaments ?? []).map((f) => ({ value: f, label: shortName(f) }))}
                value={filament()}
                onChange={setFilamentName}
              />
              <p class="text-[10px] text-text-dim leading-relaxed">
                {t("Sigue los pasos en orden: cada uno imprime una pieza de prueba chica y te pregunta cuál salió mejor. Lo que elijas se guarda para {printer} con este filamento y el laminador lo usa solo.", { printer: printer()?.name ?? "" })}
              </p>
              <div class="space-y-1.5">
                <For each={basic()}>{(s) => <StepRow s={s} />}</For>
              </div>
              <Show when={advanced().length > 0}>
                <button class="flex items-center gap-1 text-[11px] text-text-muted hover:text-text" onClick={() => setShowAdvanced(!showAdvanced())}>
                  <Show when={showAdvanced()} fallback={<Icons.CaretRight size={12} />}>
                    <Icons.CaretDown size={12} />
                  </Show>
                  {t("Avanzado")}
                </button>
                <Show when={showAdvanced()}>
                  <div class="space-y-1.5">
                    <For each={advanced()}>{(s) => <StepRow s={s} />}</For>
                  </div>
                </Show>
              </Show>
            </Show>

            {/* ── Un paso ── */}
            <Show when={step()}>
              {(s) => (
                <div class="space-y-3">
                  <p class="text-text leading-relaxed">{t(s().fixes)}</p>

                  {/* 1. Prueba */}
                  <div class="space-y-2 p-3 rounded-md bg-surface/30 border border-border">
                    <p class="text-[11px] font-medium text-text">{t("1. Imprime la prueba")}</p>
                    <Show when={s().test === "first_layer"}>
                      <p class="text-[10px] text-text-dim leading-relaxed">{t("Limpia la cama (alcohol isopropílico, sin tocarla con los dedos). Si la impresora mide la cama sola, hazlo antes de imprimir.")}</p>
                      <Show when={canAdjustZ()}>
                        <Button size="sm" variant="ghost" onClick={() => void autoLevel()} icon={<Icons.GridFour size={14} />}>
                          {t("Medir la cama ahora")}
                        </Button>
                      </Show>
                    </Show>
                    <Show when={s().test === "shaper_frequency" && canAdjustZ()}>
                      <p class="text-[10px] text-text-dim leading-relaxed">{t("Si la impresora tiene acelerómetro, puede medirlo sola, sin imprimir nada.")}</p>
                      <Button size="sm" variant="ghost" onClick={() => void autoShaper()} icon={<Icons.Lightning size={14} />}>
                        {t("Medir vibraciones (automático)")}
                      </Button>
                    </Show>
                    <Show when={s().range && range()}>
                      {(r) => (
                        <div class="grid grid-cols-3 gap-1.5">
                          <NumberInput label={t("Desde")} value={r().start} onChange={(v) => setRange({ ...r(), start: v })} step={r().step || 1} suffix={unit()} />
                          <NumberInput label={t("Hasta")} value={r().end} onChange={(v) => setRange({ ...r(), end: v })} step={r().step || 1} suffix={unit()} />
                          <Show when={s().test !== "temperature"}>
                            <NumberInput label={t("Paso")} value={r().step} onChange={(v) => setRange({ ...r(), step: Math.max(1e-4, v) })} step={r().step / 2 || 0.1} suffix={unit()} />
                          </Show>
                        </div>
                      )}
                    </Show>
                    <Show when={defaults()?.filament_type}>
                      <p class="text-[10px] text-text-dim">{t("Rango propuesto para {kind}.", { kind: defaults()!.filament_type })}</p>
                    </Show>
                    <div class="flex flex-wrap gap-1.5 items-center">
                      <Button size="sm" variant={sliced() ? "ghost" : "primary"} onClick={() => void slice()} loading={busy()} disabled={busy() || !settings()} icon={<Icons.Stack size={14} />}>
                        {sliced() ? t("Laminar de nuevo") : t("Laminar la prueba")}
                      </Button>
                      <Show when={busy()}>
                        <Button size="sm" variant="ghost" onClick={() => void invoke("slicer_cancel")}>
                          {t("Cancelar")}
                        </Button>
                      </Show>
                    </div>
                    <Show when={progress()}>{(pr) => <p class="text-[10px] text-text-dim">{pr().percent}% · {tBackend(pr().message)}</p>}</Show>
                    <Show when={sliced()}>
                      {(r) => (
                        <div class="space-y-2">
                          <p class="text-text">
                            {t("Lista: {min} min · {g} g · {layers} capas", {
                              min: fmtNumber(Math.round(r().result.time_s / 60)),
                              g: fmtNumber(r().result.filament_g, { maximumFractionDigits: 1 }),
                              layers: fmtNumber(r().result.layers),
                            })}
                          </p>
                          <For each={r().result.warnings}>{(w) => <p class="text-[10px] text-warning">{tBackend(w)}</p>}</For>
                          <SendToPrinter slicedModel={printer()?.model} onSent={() => setMessage(t("Enviada. Cuando termine (y se enfríe), sigue con el punto 2."))} />
                          <Button size="sm" variant="ghost" onClick={() => void saveGcode()} icon={<Icons.FloppyDisk size={14} />}>
                            {t("Guardar gcode (tarjeta o USB)")}
                          </Button>
                        </div>
                      )}
                    </Show>
                  </div>

                  {/* 2. Elegir */}
                  <div class="space-y-2 p-3 rounded-md bg-surface/30 border border-border">
                    <p class="text-[11px] font-medium text-text">{t("2. ¿Cómo salió?")}</p>
                    <p class="text-[10px] text-text-muted leading-relaxed">{t(s().look)}</p>
                    <Switch>
                      <Match when={s().test === "first_layer"}>
                        <Show
                          when={canAdjustZ()}
                          fallback={
                            <NumberInput label={t("Z offset que quedó en la impresora")} value={zTotal()} onChange={(v) => setZTotal(v)} min={-2} max={2} step={0.01} suffix="mm" />
                          }
                        >
                          <p class="text-[10px] text-text-dim">{t("Mientras imprime los parches, acerca o aleja la boquilla:")}</p>
                          <div class="flex flex-wrap gap-1.5">
                            <Button size="sm" variant="ghost" onClick={() => void adjustZ(-0.05)}>{t("Más cerca −0,05")}</Button>
                            <Button size="sm" variant="ghost" onClick={() => void adjustZ(-0.01)}>{t("−0,01")}</Button>
                            <Button size="sm" variant="ghost" onClick={() => void adjustZ(0.01)}>{t("+0,01")}</Button>
                            <Button size="sm" variant="ghost" onClick={() => void adjustZ(0.05)}>{t("Más lejos +0,05")}</Button>
                          </div>
                          <p class="text-text font-mono">{t("Ajuste: {z} mm", { z: fmtNumber(zTotal(), { maximumFractionDigits: 3, signDisplay: "always" }) })}</p>
                        </Show>
                      </Match>
                      <Match when={s().test === "clearance"}>
                        <Select
                          label={t("Entra justa en")}
                          options={[{ value: "", label: "—" }, ...CLEARANCES.map((c, i) => ({ value: String(i), label: t("Agujero {n}: {c} mm de holgura", { n: i + 1, c: fmtNumber(c, { maximumFractionDigits: 3 }) }) }))]}
                          value={clearancePick() === undefined ? "" : String(clearancePick())}
                          onChange={(v) => setClearancePick(v === "" ? undefined : Number(v))}
                        />
                      </Match>
                      <Match when={s().test === "block"}>
                        <div class="grid grid-cols-3 gap-1.5">
                          <For each={["X", "Y", "Z"]}>
                            {(axis, i) => (
                              <NumberInput
                                label={axis}
                                value={blockSize()[i()] ?? undefined}
                                onChange={(v) => setBlockSize((b) => b.map((x, k) => (k === i() ? v : x)) as [number | null, number | null, number | null])}
                                min={1}
                                max={200}
                                step={0.01}
                                suffix="mm"
                              />
                            )}
                          </For>
                        </div>
                        <div class="grid grid-cols-4 gap-1.5">
                          <For each={BLOCK.holes}>
                            {(nominal, i) => (
                              <NumberInput label={`Ø${nominal}`} value={blockHoles()[i()] ?? undefined} onChange={(v) => setBlockHoles((h) => h.map((x, k) => (k === i() ? v : x)))} min={0.5} max={30} step={0.01} suffix="mm" />
                            )}
                          </For>
                        </div>
                        <p class="text-[10px] text-text-dim">{t("Medidas externas (modelo: {x} × {y} × {z} mm)", { x: 5 + 4 + 5 + 6 + 5 + 8 + 5 + 10 + 5, y: BLOCK.depth, z: BLOCK.height })}</p>
                      </Match>
                      <Match when={reading()?.kind === "blocks" && reading()}>
                        {(r) => <BlocksPicker blocks={(r() as Extract<Reading, { kind: "blocks" }>).blocks} unit={unit()} />}
                      </Match>
                      <Match when={reading()?.kind === "pieces" && reading()}>
                        {(r) => <PiecesPicker pieces={(r() as Extract<Reading, { kind: "pieces" }>).pieces} />}
                      </Match>
                      <Match when={reading()?.kind === "height" && reading()}>
                        {(r) => {
                          const h = () => (r() as Extract<Reading, { kind: "height" }>).height_mm;
                          return (
                            <div class="space-y-1.5">
                              <NumberInput label={t("Altura medida desde la cama")} value={height()} onChange={(v) => setHeight(Math.max(0, Math.min(h(), v)))} min={0} max={h()} step={0.5} suffix="mm" />
                              <input type="range" class="w-full accent-orange" min={0} max={h()} step={0.1} value={height() ?? 0} onInput={(e) => setHeight(Number(e.currentTarget.value))} />
                            </div>
                          );
                        }}
                      </Match>
                    </Switch>
                    <Show when={toSave()}>
                      {(entries) => (
                        <p class="text-text">
                          {t("Se guarda")}:{" "}
                          <span class="font-mono">
                            {Object.entries(entries())
                              .map(([k, v]) => `${t(KEY_FORMAT[k]?.label ?? k)} ${fmtCal(k, v)}`)
                              .join(" · ")}
                          </span>
                        </p>
                      )}
                    </Show>
                    <Show when={props.onAskAi}>
                      <button class="text-[11px] text-accent hover:underline" onClick={() => void askAi()}>
                        {t("¿No sabes cuál elegir? Pregúntale a la IA (con una foto)")}
                      </button>
                    </Show>
                  </div>

                  <Show when={message()}>
                    <p class="text-text-muted leading-relaxed">{message()}</p>
                  </Show>
                  <Show when={error()}>
                    <p class="text-warning leading-relaxed">{error()}</p>
                  </Show>
                </div>
              )}
            </Show>
          </Show>
        </div>

        <div class="flex items-center justify-between gap-2 px-4 py-3 border-t border-border shrink-0">
          <Button size="sm" variant="ghost" onClick={() => (step() ? closeStep() : props.onClose())}>
            {step() ? t("Volver a los pasos") : t("Cerrar")}
          </Button>
          <Show when={step()} fallback={<Show when={next()}>{(n) => <Button size="sm" variant="primary" onClick={() => void openStep(n())}>{t("Seguir: {step}", { step: t(n().title) })}</Button>}</Show>}>
            <Button size="sm" variant="primary" disabled={step()!.test !== "block" && !toSave()} onClick={() => void saveResult()} icon={<Icons.Check size={14} />}>
              {t("Guardar resultado")}
            </Button>
          </Show>
        </div>
      </div>
    </div>
  );
};
