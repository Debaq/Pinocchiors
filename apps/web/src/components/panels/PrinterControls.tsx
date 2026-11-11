import { Component, For, JSX, ParentComponent, Show, createEffect, createSignal, on, onCleanup } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { clsx } from "clsx";
import { Button, Checkbox, NumberInput } from "../ui";
import * as Icons from "../icons";
import { fmtNumber, t } from "../../lib/i18n";
import { tBackend } from "../../lib/i18nBackend.ts";
import type { ConfirmRequest } from "../layout/ConfirmDialog";
import { PrinterCamera } from "./PrinterCamera";

/** Sección que se pliega (cerrada de entrada); abierta o cerrada la puede mandar quien la usa */
export const Fold: ParentComponent<{
  title: string;
  icon?: JSX.Element;
  defaultOpen?: boolean;
  open?: boolean;
  onToggle?: (open: boolean) => void;
}> = (props) => {
  const [own, setOwn] = createSignal(props.defaultOpen ?? false);
  const isOpen = () => props.open ?? own();
  const toggle = () => {
    setOwn(!isOpen());
    props.onToggle?.(!isOpen());
  };
  return (
    <div class="border-b border-border/50">
      <button
        type="button"
        class="w-full flex items-center gap-2 h-8 px-1 rounded text-xs font-semibold uppercase tracking-wider text-text-muted hover:bg-surface/30"
        aria-expanded={isOpen()}
        onClick={toggle}
      >
        <Icons.CaretRight size={12} class={clsx("shrink-0 text-text-dim transition-transform duration-100", isOpen() && "rotate-90")} />
        <Show when={props.icon}>
          <span class="shrink-0">{props.icon}</span>
        </Show>
        <span class="truncate">{props.title}</span>
      </button>
      <Show when={isOpen()}>
        <div class="space-y-2 pt-1 pb-3 px-1">{props.children}</div>
      </Show>
    </div>
  );
};


// ═══════════════════════════════════════════════════════════════════════════
// TIPOS (espejo de apps/desktop/src/printer_control.rs)
// ═══════════════════════════════════════════════════════════════════════════

export type FanKind = "model" | "aux" | "case";

export type Control =
  | { kind: "z_offset"; delta: number }
  | { kind: "nozzle_temp"; celsius: number }
  | { kind: "bed_temp"; celsius: number }
  | { kind: "fan"; fan: FanKind; percent: number }
  | { kind: "light"; on: boolean }
  | { kind: "home"; axes: string }
  | { kind: "jog"; axis: "X" | "Y" | "Z"; mm: number }
  | { kind: "motors_off" }
  | { kind: "heaters_off" }
  | { kind: "auto_level" }
  | { kind: "pid_nozzle"; celsius: number }
  | { kind: "pid_bed"; celsius: number }
  | { kind: "input_shaper" }
  | { kind: "feed_rate"; percent: number }
  | { kind: "flow_rate"; percent: number }
  | { kind: "exclude_object"; name: string }
  | { kind: "gcode"; script: string }
  | { kind: "reprint"; name: string }
  | { kind: "delete_file"; name: string }
  | { kind: "timelapse"; on: boolean }
  | { kind: "restart_klipper" }
  | { kind: "restart_firmware" }
  | { kind: "clear_error" };

interface HistoryEntry {
  name: string;
  started: number;
  duration_s: number;
  filament_mm: number;
  finished: boolean;
  running: boolean;
}

interface FileEntry {
  name: string;
  size: number;
  modified: number;
  print_time_s: number | null;
  filament_g: number | null;
}

interface BedMesh {
  rows: number[][];
  min: [number, number];
  max: [number, number];
}

interface PrinterDetails {
  z_offset: number | null;
  fans: [FanKind, number][];
  light: boolean | null;
  feed_rate: number | null;
  flow_rate: number | null;
  timelapse: boolean | null;
  total_jobs: number | null;
  total_time_s: number | null;
  history: HistoryEntry[];
  files: FileEntry[];
  mesh: BedMesh | null;
  objects: string[];
  excluded: string[];
  current_object: string | null;
  supports: string[];
}

const errText = (e: unknown) => (typeof e === "string" ? tBackend(e) : String(e));

function fmtDuration(seconds: number): string {
  const s = Math.max(0, Math.round(seconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  return h > 0 ? `${h} h ${m} min` : `${m} min`;
}


const FANS: [FanKind, string][] = [
  ["model", "Pieza"],
  ["aux", "Lateral"],
  ["case", "Gabinete"],
];

/** Color de una altura de la malla: azul abajo, verde al medio, rojo arriba */
function meshColor(z: number, lo: number, hi: number): string {
  const k = hi > lo ? (z - lo) / (hi - lo) : 0.5;
  const hue = 240 - 240 * Math.min(1, Math.max(0, k));
  return `hsl(${hue} 70% 45%)`;
}


const Title: ParentComponent = (props) => <h4 class="text-xs font-semibold uppercase tracking-wider text-text-muted">{props.children}</h4>;

/**
 * Manejar la impresora más allá de imprimir: ajustes de durante la impresión (altura, velocidad,
 * flujo, excluir objetos), luz y ventiladores, mover ejes y temperaturas, malla de la cama,
 * cámara, historial, archivos y consola. Lo que entiende cada conexión lo dice `supports`.
 * Lo que se pasa como hijos (la puesta a punto) va entre lo de durante la impresión y lo plegado.
 */
export const PrinterControls: Component<{
  conn: () => Record<string, unknown> | undefined;
  /** Nombre de la impresora (para los mensajes de confirmación) */
  name: string;
  /** Estado de la impresión (printing, paused, standby…) */
  state: string;
  /** "moonraker" | "creality" | "flashforge" */
  printerKind: string;
  confirm: (request: ConfirmRequest) => Promise<boolean>;
  children?: JSX.Element;
}> = (props) => {
  const [details, setDetails] = createSignal<PrinterDetails>();
  const [error, setError] = createSignal<string>();
  const [busy, setBusy] = createSignal(false);
  const [done, setDone] = createSignal<string>();
  const [nozzle, setNozzle] = createSignal(220);
  const [bed, setBed] = createSignal(60);
  const [step, setStep] = createSignal(10);
  const [console_, setConsole] = createSignal("");
  const [sent, setSent] = createSignal<string[]>([]);
  const [showFiles, setShowFiles] = createSignal(10);

  const printing = () => props.state === "printing" || props.state === "paused";
  const can = (kind: string) => details()?.supports.includes(kind) ?? false;

  const refresh = async () => {
    const c = props.conn();
    if (!c) return;
    try {
      setDetails(await invoke<PrinterDetails>("printer_details", { conn: c }));
      setError(undefined);
    } catch (e) {
      setError(errText(e));
    }
  };
  // Detalles al abrir, al cambiar de impresora y cada 15 s
  let timer: ReturnType<typeof setTimeout> | undefined;
  let alive = true;
  const loop = async () => {
    clearTimeout(timer);
    await refresh();
    if (alive) timer = setTimeout(loop, 15000);
  };
  createEffect(on(() => JSON.stringify(props.conn() ?? {}), () => void loop()));
  onCleanup(() => {
    alive = false;
    clearTimeout(timer);
  });

  /** Manda una orden; con `ask`, primero pide confirmar */
  const send = async (control: Control, label: string, ask?: ConfirmRequest) => {
    const c = props.conn();
    if (!c) return;
    if (ask && !(await props.confirm(ask))) return;
    setBusy(true);
    setError(undefined);
    setDone(undefined);
    try {
      await invoke("printer_control", { conn: c, control });
      setDone(label);
      setTimeout(() => void refresh(), 1500);
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  const runConsole = async () => {
    const script = console_().trim();
    if (!script) return;
    await send({ kind: "gcode", script }, t("Gcode enviado"));
    setSent([script, ...sent()].slice(0, 8));
    setConsole("");
  };

  const hasHistory = () => (details()?.history.length ?? 0) > 0 || (details()?.files.length ?? 0) > 0;

  return (
    <div class="space-y-3">
      {/* ── Durante la impresión ── */}
      <Show when={printing() && (can("z_offset") || can("feed_rate") || (details()?.objects.length ?? 0) > 1)}>
        <section class="space-y-2">
          <Title>{t("Durante la impresión")}</Title>
          <Show when={can("z_offset")}>
            <div class="space-y-1">
              <div class="flex items-center justify-between gap-2 text-xs">
                <span class="text-text-muted">{t("Altura de la boquilla (Z offset)")}</span>
                <span class="font-mono text-text shrink-0">{details()?.z_offset != null ? `${fmtNumber(details()!.z_offset!, { maximumFractionDigits: 3 })} mm` : "—"}</span>
              </div>
              <div class="grid gap-1 [grid-template-columns:repeat(auto-fit,minmax(44px,1fr))]">
                <For each={[-0.05, -0.01, 0.01, 0.05]}>
                  {(d) => (
                    <Button size="sm" disabled={busy()} onClick={() => send({ kind: "z_offset", delta: d }, t("Altura {d} mm", { d: `${d > 0 ? "+" : ""}${d}` }))}>
                      {d > 0 ? "+" : "−"}
                      {Math.abs(d)}
                    </Button>
                  )}
                </For>
              </div>
              <p class="text-[10px] text-text-dim">{t("Durante la primera capa: si el plástico no se pega, baja (−); si rasca o se transparenta, sube (+).")}</p>
            </div>
          </Show>
          <Show when={can("feed_rate")}>
            <div class="grid gap-1.5 [grid-template-columns:repeat(auto-fit,minmax(90px,1fr))]">
              <NumberInput label={t("Velocidad")} value={details()?.feed_rate ?? 100} min={10} max={300} step={5} suffix="%" onChange={(v) => send({ kind: "feed_rate", percent: v }, t("Velocidad {v} %", { v }))} />
              <NumberInput label={t("Flujo")} value={details()?.flow_rate ?? 100} min={10} max={300} step={1} suffix="%" onChange={(v) => send({ kind: "flow_rate", percent: v }, t("Flujo {v} %", { v }))} />
            </div>
          </Show>
          <Show when={(details()?.objects.length ?? 0) > 1}>
            <div class="space-y-1 pt-1">
              <p class="text-xs text-text-muted">{t("Objetos de esta impresión")}</p>
              <For each={details()!.objects}>
                {(o) => (
                  <div class="flex items-center gap-1.5 text-[11px]">
                    <span class={`flex-1 truncate ${details()!.excluded.includes(o) ? "line-through text-text-dim" : "text-text"}`}>
                      {o}
                      <Show when={o === details()!.current_object}> ●</Show>
                    </span>
                    <Show when={!details()!.excluded.includes(o)}>
                      <button
                        class="text-red hover:underline"
                        disabled={busy()}
                        onClick={() =>
                          send({ kind: "exclude_object", name: o }, t("{o} ya no se imprime", { o }), {
                            title: t("Excluir objeto"),
                            message: t("{printer} deja de imprimir {o}; el resto sigue. No se puede deshacer.", { printer: props.name, o }),
                            confirmLabel: t("Excluir"),
                            danger: true,
                          })
                        }
                      >
                        {t("Excluir")}
                      </button>
                    </Show>
                  </div>
                )}
              </For>
            </div>
          </Show>
        </section>
      </Show>

      {/* ── Luz y ventiladores ── */}
      <Show when={can("fan") || (can("light") && details()?.light != null)}>
        <Fold title={t("Luz y ventiladores")} icon={<Icons.Lightning size={14} />}>
          <Show when={can("fan")}>
            <div class="grid gap-1.5 [grid-template-columns:repeat(auto-fit,minmax(80px,1fr))]">
              <For each={FANS}>
                {([fan, label]) => (
                  <NumberInput
                    label={t(label)}
                    value={details()?.fans.find(([f]) => f === fan)?.[1] ?? 0}
                    min={0}
                    max={100}
                    step={10}
                    suffix="%"
                    onChange={(v) => send({ kind: "fan", fan, percent: v }, t("Ventilador {v} %", { v }))}
                  />
                )}
              </For>
            </div>
          </Show>
          <Show when={can("light") && details()?.light != null}>
            <Checkbox small label={t("Luz")} checked={!!details()?.light} onChange={(on) => send({ kind: "light", on }, on ? t("Luz encendida") : t("Luz apagada"))} />
          </Show>
        </Fold>
      </Show>

      <Show when={done()}>
        <p class="text-[11px] text-green">{done()}</p>
      </Show>
      <Show when={error()}>
        <p class="text-[11px] text-red">{error()}</p>
      </Show>

      {props.children}

      {/* ── Cámara (solo se pide al abrirla) ── */}
      <Fold title={t("Cámara")} icon={<Icons.Eye size={14} />}>
        <PrinterCamera conn={props.conn} printerKind={props.printerKind} />
      </Fold>

      {/* ── Mover y calentar ── */}
      <Fold title={t("Mover y calentar")} icon={<Icons.ArrowsOutCardinal size={14} />}>
        <div class="grid grid-cols-[1fr_auto] gap-1.5 items-end">
          <NumberInput label={t("Boquilla")} value={nozzle()} min={0} max={300} step={5} suffix="°C" onChange={setNozzle} />
          <Button size="sm" disabled={busy()} onClick={() => send({ kind: "nozzle_temp", celsius: nozzle() }, t("Boquilla a {c} °C", { c: nozzle() }))}>
            {t("Calentar")}
          </Button>
          <NumberInput label={t("Cama")} value={bed()} min={0} max={110} step={5} suffix="°C" onChange={setBed} />
          <Button size="sm" disabled={busy()} onClick={() => send({ kind: "bed_temp", celsius: bed() }, t("Cama a {c} °C", { c: bed() }))}>
            {t("Calentar")}
          </Button>
        </div>
        <Button size="sm" variant="ghost" fullWidth disabled={busy()} onClick={() => send({ kind: "heaters_off" }, t("Calentadores apagados"))}>
          {t("Apagar los calentadores")}
        </Button>
        <Show when={!printing()} fallback={<p class="text-[10px] text-text-dim">{t("Mover los ejes no se puede mientras imprime.")}</p>}>
          <div class="flex flex-wrap items-center gap-1 pt-1">
            <span class="text-[11px] text-text-muted">{t("Paso")}</span>
            <For each={[0.1, 1, 10, 50]}>
              {(s) => (
                <button class={`rounded border px-1.5 text-[11px] ${step() === s ? "border-accent text-accent" : "border-border text-text-muted"}`} onClick={() => setStep(s)}>
                  {s}
                </button>
              )}
            </For>
            <span class="text-[11px] text-text-dim">mm</span>
          </div>
          <div class="grid gap-1 [grid-template-columns:repeat(auto-fit,minmax(84px,1fr))]">
            <For each={["X", "Y", "Z"] as const}>
              {(axis) => (
                <div class="flex gap-0.5">
                  <Button size="sm" variant="ghost" class="flex-1" disabled={busy()} onClick={() => send({ kind: "jog", axis, mm: -step() }, `${axis} −${step()} mm`)}>
                    {axis}−
                  </Button>
                  <Button size="sm" variant="ghost" class="flex-1" disabled={busy()} onClick={() => send({ kind: "jog", axis, mm: step() }, `${axis} +${step()} mm`)}>
                    {axis}+
                  </Button>
                </div>
              )}
            </For>
          </div>
          <div class="grid gap-1 [grid-template-columns:repeat(auto-fit,minmax(60px,1fr))]">
            <For each={[["X Y Z", "Todo"], ["X", "X"], ["Y", "Y"], ["Z", "Z"]] as const}>
              {([axes, label]) => (
                <Button size="sm" variant="ghost" icon={<Icons.House size={12} />} disabled={busy()} onClick={() => send({ kind: "home", axes }, t("Al origen: {a}", { a: axes }))}>
                  {t(label)}
                </Button>
              )}
            </For>
          </div>
          <Button size="sm" variant="ghost" fullWidth disabled={busy()} onClick={() => send({ kind: "motors_off" }, t("Motores sueltos"))}>
            {t("Soltar los motores")}
          </Button>
        </Show>
      </Fold>

      {/* ── Malla de la cama (la deja «Nivelar la cama»); en Klipper la dibuja la puesta a punto ── */}
      <Show when={props.printerKind !== "moonraker" && details()?.mesh}>
        {(m) => {
          const flat = () => m().rows.flat();
          const lo = () => Math.min(...flat());
          const hi = () => Math.max(...flat());
          return (
            <Fold title={t("Malla de la cama")} icon={<Icons.GridFour size={14} />}>
              <div class="flex justify-between gap-2 text-xs">
                <span class="text-text-muted">{t("Desnivel")}</span>
                <span class="font-mono text-text">{fmtNumber(hi() - lo(), { maximumFractionDigits: 2 })} mm</span>
              </div>
              {/* Fila 0 = y mínima: abajo, como se mira la cama de frente */}
              <div class="grid gap-0.5" style={{ "grid-template-columns": `repeat(${m().rows[0]?.length ?? 1}, minmax(0, 1fr))` }}>
                <For each={[...m().rows].reverse()}>
                  {(row) => (
                    <For each={row}>
                      {(z) => (
                        <div class="rounded-sm py-1 text-center font-mono text-[9px] text-white" style={{ background: meshColor(z, lo(), hi()) }}>
                          {fmtNumber(z, { maximumFractionDigits: 2 })}
                        </div>
                      )}
                    </For>
                  )}
                </For>
              </div>
              <p class="text-[10px] text-text-dim">{t("Vista desde el frente. Rojo = más alto, azul = más bajo. Más de 0,5 mm de desnivel conviene revisar la cama.")}</p>
            </Fold>
          );
        }}
      </Show>

      {/* ── Historial y archivos ── */}
      <Show when={hasHistory()}>
        <Fold title={t("Historial y archivos")} icon={<Icons.Clock size={14} />}>
          <Show when={(details()?.history.length ?? 0) > 0}>
            <p class="text-xs text-text-muted">{t("Historial")}</p>
            <Show when={details()?.total_jobs != null}>
              <p class="text-[11px] text-text-muted">
                {t("{n} trabajos", { n: details()!.total_jobs! })}
                <Show when={details()?.total_time_s}> · {t("{h} h de uso", { h: fmtNumber(details()!.total_time_s! / 3600, { maximumFractionDigits: 0 }) })}</Show>
              </p>
            </Show>
            <For each={details()!.history.slice(0, 15)}>
              {(h) => (
                <div class="flex items-center gap-1.5 text-[11px]">
                  <Show
                    when={h.running}
                    fallback={
                      <Show when={h.finished} fallback={<Icons.X size={11} class="shrink-0 text-red" />}>
                        <Icons.Check size={11} class="shrink-0 text-green" />
                      </Show>
                    }
                  >
                    <Icons.Play size={11} class="shrink-0 text-accent" />
                  </Show>
                  <span class="flex-1 truncate text-text" title={h.name}>
                    {h.name}
                  </span>
                  <span class="shrink-0 font-mono text-text-dim">{fmtDuration(h.duration_s)}</span>
                </div>
              )}
            </For>
          </Show>
          <Show when={(details()?.files.length ?? 0) > 0}>
            <p class="text-xs text-text-muted pt-1">{t("Archivos en la impresora")}</p>
            <For each={details()!.files.slice(0, showFiles())}>
              {(f) => (
                <div class="flex items-center gap-1.5 text-[11px]">
                  <span class="flex-1 truncate text-text" title={f.name}>
                    {f.name}
                  </span>
                  <Show when={f.print_time_s}>
                    <span class="shrink-0 font-mono text-text-dim">{fmtDuration(f.print_time_s!)}</span>
                  </Show>
                  <button
                    class="shrink-0 text-accent hover:underline disabled:opacity-40"
                    disabled={busy() || printing()}
                    title={t("Imprimir")}
                    onClick={() =>
                      send({ kind: "reprint", name: f.name }, t("Imprimiendo {f}", { f: f.name }), {
                        title: t("Imprimir"),
                        message: t("{printer} va a imprimir {f}. ¿La cama está despejada?", { printer: props.name, f: f.name }),
                        confirmLabel: t("Imprimir"),
                      })
                    }
                  >
                    <Icons.Play size={11} />
                  </button>
                  <button
                    class="shrink-0 text-red hover:underline disabled:opacity-40"
                    disabled={busy()}
                    title={t("Borrar")}
                    onClick={() =>
                      send({ kind: "delete_file", name: f.name }, t("{f} borrado", { f: f.name }), {
                        title: t("Borrar archivo"),
                        message: t("Se borra {f} de {printer}. No se puede deshacer.", { printer: props.name, f: f.name }),
                        confirmLabel: t("Borrar"),
                        danger: true,
                      })
                    }
                  >
                    <Icons.Trash size={11} />
                  </button>
                </div>
              )}
            </For>
            <Show when={details()!.files.length > showFiles()}>
              <button class="text-[11px] text-text-dim hover:underline" onClick={() => setShowFiles(showFiles() + 20)}>
                {t("Ver más ({n})", { n: details()!.files.length - showFiles() })}
              </button>
            </Show>
          </Show>
        </Fold>
      </Show>

      {/* ── Consola y avanzado ── */}
      <Fold title={t("Consola y avanzado")} icon={<Icons.Gear size={14} />}>
        <Show when={can("gcode")}>
          <p class="text-xs text-text-muted">{t("Consola de gcode")}</p>
          <div class="flex gap-1">
            <input
              class="flex-1 min-w-0 rounded border border-border bg-surface/30 px-1.5 py-1 font-mono text-[11px] text-text"
              placeholder="G28 X Y"
              value={console_()}
              onInput={(e) => setConsole(e.currentTarget.value)}
              onKeyDown={(e) => {
                e.stopPropagation();
                if (e.key === "Enter") void runConsole();
              }}
            />
            <Button size="sm" disabled={busy() || !console_().trim()} onClick={() => void runConsole()}>
              {t("Enviar")}
            </Button>
          </div>
          <For each={sent()}>{(g) => <p class="font-mono text-[10px] text-text-dim truncate">› {g}</p>}</For>
          <p class="text-[10px] text-text-dim">{t("Va directo a Klipper. En la K1 de fábrica no vuelve la respuesta: el efecto se ve en el estado.")}</p>
        </Show>
        <p class="text-xs text-text-muted pt-1">{t("Sistema")}</p>
        <Show when={can("timelapse") && details()?.timelapse != null}>
          <Checkbox small label={t("Timelapse")} checked={!!details()?.timelapse} onChange={(on) => send({ kind: "timelapse", on }, on ? t("Timelapse activado") : t("Timelapse desactivado"))} />
        </Show>
        <Show when={can("restart_klipper")}>
          <div class="space-y-0.5">
            <Button
              size="sm"
              fullWidth
              disabled={busy() || printing()}
              onClick={() => send({ kind: "restart_klipper" }, t("Klipper reiniciado"), { title: t("Reiniciar Klipper"), message: t("{printer} recarga su configuración. Tarda unos segundos.", { printer: props.name }), confirmLabel: t("Reiniciar") })}
            >
              {t("Reiniciar Klipper")}
            </Button>
            <p class="text-[10px] text-text-dim">{t("Recarga la configuración; sirve después de cambiar un ajuste.")}</p>
          </div>
        </Show>
        <Show when={can("restart_firmware")}>
          <div class="space-y-0.5">
            <Button
              size="sm"
              fullWidth
              disabled={busy() || printing()}
              onClick={() => send({ kind: "restart_firmware" }, t("Firmware reiniciado"), { title: t("Reiniciar el firmware"), message: t("{printer} reinicia sus placas. Se pierde lo que esté haciendo.", { printer: props.name }), confirmLabel: t("Reiniciar"), danger: true })}
            >
              {t("Reiniciar firmware")}
            </Button>
            <p class="text-[10px] text-text-dim">{t("Reinicia las placas de la impresora; sirve si quedó trabada.")}</p>
          </div>
        </Show>
        <Show when={can("clear_error")}>
          <div class="space-y-0.5">
            <Button size="sm" fullWidth disabled={busy()} onClick={() => send({ kind: "clear_error" }, t("Error borrado"))}>
              {t("Borrar error")}
            </Button>
            <p class="text-[10px] text-text-dim">{t("Quita el aviso de error para poder seguir usándola.")}</p>
          </div>
        </Show>
      </Fold>
    </div>
  );
};
