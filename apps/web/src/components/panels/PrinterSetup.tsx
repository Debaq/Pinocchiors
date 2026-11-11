import { Component, For, JSX, Show, createEffect, createSignal, on } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { Button, NumberInput } from "../ui";
import { Fold } from "./PrinterControls";
import * as Icons from "../icons";
import { fmtNumber, t } from "../../lib/i18n";
import { tBackend } from "../../lib/i18nBackend.ts";
import type { NetPrinterKind } from "../../lib/settings";
import type { ConfirmRequest } from "../layout/ConfirmDialog";

// ═══════════════════════════════════════════════════════════════════════════
// TIPOS (espejo de «puesta a punto» en apps/desktop/src/printer_control.rs)
// ═══════════════════════════════════════════════════════════════════════════

interface SetupHeater {
  name: string;
  temp: number;
  target: number;
}
interface SetupFan {
  name: string;
  percent: number;
  manual: boolean;
}
interface SetupScrew {
  name: string;
  x: number;
  y: number;
  base: boolean;
  z: number | null;
  dir: string;
  turn: string;
}
interface SetupMesh {
  rows: number[][];
  min: [number, number];
  max: [number, number];
  profile: string;
}
interface SetupInfo {
  supported: boolean;
  reason: string;
  heaters: SetupHeater[];
  steppers: string[];
  fans: SetupFan[];
  has_endstops: boolean;
  probe: { kind: string; z_offset: number | null } | null;
  has_screws_tilt: boolean;
  has_bed_screws: boolean;
  screws: SetupScrew[];
  rotation_distance: number | null;
  mesh: SetupMesh | null;
  homed: string;
}
interface SetupResult {
  ok: boolean;
  lines: string[];
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  data: any;
}

type SetupAction =
  | { kind: "query_endstops" }
  | { kind: "stepper_buzz"; stepper: string }
  | { kind: "set_fan"; name: string; percent: number }
  | { kind: "probe_accuracy" }
  | { kind: "probe_calibrate" }
  | { kind: "test_z"; delta: number }
  | { kind: "accept" }
  | { kind: "abort" }
  | { kind: "adjusted" }
  | { kind: "save_config" }
  | { kind: "screws_tilt" }
  | { kind: "bed_screws_adjust" }
  | { kind: "extrude_test"; celsius: number }
  | { kind: "save_rotation_distance"; value: number };

const errText = (e: unknown) => (typeof e === "string" ? tBackend(e) : String(e));

/** «01:15» (reloj de la cama de Klipper) → «1 vuelta y ¼» */
export function turnText(dir: string, turn: string): string {
  const [h, m] = turn.split(":").map((n) => parseInt(n, 10) || 0);
  const quarter: Record<number, string> = { 0: "", 15: "¼", 30: "½", 45: "¾" };
  const frac = quarter[m] ?? t("{m} min del reloj", { m });
  const whole = h > 0 ? (h === 1 ? t("1 vuelta") : t("{n} vueltas", { n: h })) : "";
  const amount = whole && frac ? t("{whole} y {frac}", { whole, frac }) : whole || (frac ? t("{frac} de vuelta", { frac }) : "");
  if (!amount) return t("está bien, no tocar");
  return `${amount} ${dir === "CCW" ? t("a la izquierda") : t("a la derecha")} (${dir} ${turn})`;
}

/** Color de una altura de la malla con los colores del tema: acento (bajo), verde (medio), rojo (alto) */
function meshFill(z: number, lo: number, hi: number): string {
  const k = hi > lo ? Math.min(1, Math.max(0, (z - lo) / (hi - lo))) : 0.5;
  return k < 0.5
    ? `color-mix(in srgb, var(--color-green) ${Math.round(k * 200)}%, var(--color-accent))`
    : `color-mix(in srgb, var(--color-red) ${Math.round((k - 0.5) * 200)}%, var(--color-green))`;
}

/** Pequeño ✓ / ✗ que marca el usuario */
const Mark: Component<{ value: boolean | undefined; onChange: (v: boolean | undefined) => void }> = (props) => (
  <span class="inline-flex shrink-0 gap-0.5">
    <button
      type="button"
      class={`h-5 w-5 rounded-sm border text-[11px] leading-none ${props.value === true ? "border-green bg-green/20 text-green" : "border-border text-text-dim hover:border-border-hover"}`}
      title={t("Está bien")}
      onClick={() => props.onChange(props.value === true ? undefined : true)}
    >
      ✓
    </button>
    <button
      type="button"
      class={`h-5 w-5 rounded-sm border text-[11px] leading-none ${props.value === false ? "border-red bg-red/20 text-red" : "border-border text-text-dim hover:border-border-hover"}`}
      title={t("Algo está mal")}
      onClick={() => props.onChange(props.value === false ? undefined : false)}
    >
      ✗
    </button>
  </span>
);

const Lines: Component<{ r: SetupResult | undefined }> = (props) => (
  <Show when={props.r?.lines.length}>
    <div class={`rounded-sm border px-2 py-1 font-mono text-[10px] leading-snug ${props.r!.ok ? "border-border text-text-muted" : "border-red text-red"}`}>
      <For each={props.r!.lines}>{(l) => <div class="break-words">{tBackend(l)}</div>}</For>
    </div>
  </Show>
);

const Note: Component<{ children: JSX.Element }> = (props) => <p class="text-[10px] text-text-dim">{props.children}</p>;

/**
 * Puesta a punto de la máquina: nivelar, PID, resonancias, revisión de lo recién armado,
 * extrusor, sonda, tornillos y malla de la cama. Pensado para una impresora recién pasada a
 * Klipper; en las de fábrica muestra solo lo que se puede.
 */
export const PrinterSetup: Component<{
  conn: () => Record<string, unknown> | undefined;
  name: string;
  printerKind: NetPrinterKind;
  state: string;
  confirm: (request: ConfirmRequest) => Promise<boolean>;
}> = (props) => {
  const [info, setInfo] = createSignal<SetupInfo>();
  const [error, setError] = createSignal<string>();
  const [busy, setBusy] = createSignal(false);
  const [results, setResults] = createSignal<Record<string, SetupResult>>({});
  const [pidNozzle, setPidNozzle] = createSignal(220);
  const [pidBed, setPidBed] = createSignal(60);
  const [marks, setMarks] = createSignal<Record<string, boolean | undefined>>({});
  const [testTemp, setTestTemp] = createSignal(210);
  const [remaining, setRemaining] = createSignal(20);

  const printing = () => props.state === "printing" || props.state === "paused";
  const klipper = () => !!info()?.supported;
  const off = () => busy() || printing();

  const refresh = async () => {
    const c = props.conn();
    if (!c) return;
    try {
      setInfo(await invoke<SetupInfo>("printer_setup_info", { conn: c }));
      setError(undefined);
    } catch (e) {
      setError(errText(e));
    }
  };
  createEffect(on(() => JSON.stringify(props.conn() ?? {}), () => void refresh()));

  const mark = (key: string, v: boolean | undefined) => setMarks({ ...marks(), [key]: v });

  /** Una orden de las de siempre (nivelar, PID, resonancias), con confirmación */
  const control = async (control: Record<string, unknown>, title: string, detail: string) => {
    const c = props.conn();
    if (!c) return;
    const ok = await props.confirm({
      title,
      message: t("{printer} va a {detail} Puede tardar varios minutos; no la toques mientras tanto.", { printer: props.name, detail }),
      confirmLabel: t("Empezar"),
    });
    if (!ok) return;
    setBusy(true);
    setError(undefined);
    try {
      await invoke("printer_control", { conn: c, control });
      setResults({ ...results(), calibrate: { ok: true, lines: [t("{what}: empezó en la impresora", { what: title })], data: null } });
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  /** Una acción de la puesta a punto; con `ask`, primero pide confirmar */
  const run = async (key: string, action: SetupAction, ask?: ConfirmRequest): Promise<SetupResult | undefined> => {
    const c = props.conn();
    if (!c) return;
    if (ask && !(await props.confirm(ask))) return;
    setBusy(true);
    setError(undefined);
    try {
      const r = await invoke<SetupResult>("printer_setup_action", { conn: c, action });
      setResults({ ...results(), [key]: r });
      return r;
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  const heaterLabel = (name: string) => (name === "extruder" ? t("Boquilla") : name === "heater_bed" ? t("Cama") : name);
  /** Un calentador apagado y frío debería marcar la temperatura de la pieza */
  const atAmbient = (h: SetupHeater) => h.temp >= 5 && h.temp <= 45;

  const mesh = () => info()?.mesh ?? null;
  const meshRange = () => {
    const flat = mesh()?.rows.flat() ?? [];
    return flat.length ? Math.max(...flat) - Math.min(...flat) : 0;
  };

  // ── Extrusor ──
  const extruded = () => 120 - remaining();
  const newRotation = () => {
    const cur = info()?.rotation_distance;
    return cur ? cur * (extruded() / 100) : undefined;
  };

  const askOf = (title: string, message: string, confirmLabel?: string, danger?: boolean): ConfirmRequest => ({ title, message, confirmLabel: confirmLabel ?? t("Empezar"), danger });

  return (
    <>
      <Show when={printing()}>
        <p class="text-[11px] text-text-dim">{t("Se está imprimiendo: la puesta a punto se hace con la impresora libre.")}</p>
      </Show>
      <Show when={error()}>
        <p class="text-[11px] text-red">{error()}</p>
      </Show>
      <Show when={info() && !klipper()}>
        <p class="text-[11px] text-text-dim">{tBackend(info()!.reason)}</p>
      </Show>

      {/* ── Nivelar, PID y resonancias ── */}
      <div>
        <Fold title={t("Calibrar")} icon={<Icons.Ruler size={14} />}>
          <div class="space-y-2 pb-3">
            <Note>{t("Con la boquilla limpia y la cama despejada. Cada una guarda el resultado en la impresora.")}</Note>
            <Note>{t("PID = que la temperatura no suba y baje: la impresora aprende cuánto empujar al calentador. Resonancias = que no vibre al ir rápido.")}</Note>
            <Button size="sm" class="w-full" disabled={off()} onClick={() => control({ kind: "auto_level" }, t("Nivelar la cama"), t("medir toda la cama y guardar la malla."))}>
              {t("Nivelar la cama")}
            </Button>
            <div class="grid grid-cols-[1fr_auto] items-end gap-1.5">
              <NumberInput label={t("Boquilla")} value={pidNozzle()} min={150} max={300} step={5} suffix="°C" onChange={setPidNozzle} />
              <Button size="sm" disabled={off()} onClick={() => control({ kind: "pid_nozzle", celsius: pidNozzle() }, t("PID de la boquilla"), t("calentar la boquilla a {c} °C varias veces para ajustar su control de temperatura.", { c: pidNozzle() }))}>
                {t("PID boquilla")}
              </Button>
            </div>
            <div class="grid grid-cols-[1fr_auto] items-end gap-1.5">
              <NumberInput label={t("Cama")} value={pidBed()} min={40} max={110} step={5} suffix="°C" onChange={setPidBed} />
              <Button size="sm" disabled={off()} onClick={() => control({ kind: "pid_bed", celsius: pidBed() }, t("PID de la cama"), t("calentar la cama a {c} °C varias veces para ajustar su control de temperatura.", { c: pidBed() }))}>
                {t("PID cama")}
              </Button>
            </div>
            <Button size="sm" class="w-full" disabled={off()} onClick={() => control({ kind: "input_shaper" }, t("Resonancias"), t("ir al origen y sacudir los ejes para medir sus resonancias (input shaping)."))}>
              {t("Resonancias")}
            </Button>
            <Lines r={results().calibrate} />
          </div>
        </Fold>
      </div>

      <Show when={klipper()}>
        {/* ── Revisión de la máquina recién armada ── */}
        <div>
          <Fold title={t("Revisar la máquina")} icon={<Icons.Check size={14} />}>
            <div class="space-y-2 pb-3">
              <Note>{t("Lista para una impresora recién armada. Marca ✓ lo que esté bien y ✗ lo que no.")}</Note>
              <div class="flex justify-end">
                <Button size="sm" variant="ghost" disabled={busy()} onClick={() => void refresh()}>
                  {t("Actualizar datos")}
                </Button>
              </div>

              {/* Temperaturas */}
              <div class="space-y-1">
                <div class="text-xs text-text-muted">{t("Temperaturas (en frío deben marcar la del cuarto)")}</div>
                <For each={info()!.heaters}>
                  {(h) => (
                    <div class="flex items-center justify-between gap-2 text-xs">
                      <span class="text-text-muted">{heaterLabel(h.name)}</span>
                      <span class="font-mono text-text">{fmtNumber(h.temp, { maximumFractionDigits: 1 })} °C</span>
                      <span class={atAmbient(h) || h.target > 0 ? "text-green" : "text-red"}>{atAmbient(h) || h.target > 0 ? "✓" : "✗"}</span>
                    </div>
                  )}
                </For>
                <Note>{t("Si marca 0, un número muy alto o −10 °C, el sensor (termistor) está mal puesto o desconectado: no calientes nada.")}</Note>
              </div>

              {/* Finales de carrera */}
              <div class="space-y-1">
                <div class="flex items-center justify-between gap-2">
                  <span class="text-xs text-text-muted">{t("Finales de carrera")}</span>
                  <Mark value={marks().endstops} onChange={(v) => mark("endstops", v)} />
                </div>
                <Show when={info()!.has_endstops} fallback={<Note>{t("Klipper no tiene finales de carrera configurados.")}</Note>}>
                  <Button size="sm" disabled={off()} onClick={() => void run("endstops", { kind: "query_endstops" })}>
                    {t("Consultar")}
                  </Button>
                  <Show when={results().endstops?.data}>
                    <div class="grid grid-cols-3 gap-1 font-mono text-[10px]">
                      <For each={Object.entries(results().endstops.data as Record<string, string | boolean>)}>
                        {([axis, v]) => {
                          const hit = v === true || String(v).toLowerCase() === "triggered";
                          return (
                            <div class={`rounded-sm border px-1 py-0.5 text-center ${hit ? "border-orange text-orange" : "border-border text-text-muted"}`}>
                              {axis.toUpperCase()}: {hit ? t("activado") : t("abierto")}
                            </div>
                          );
                        }}
                      </For>
                    </div>
                  </Show>
                  <Note>{t("Sin tocarlos deben decir «abierto». Aprieta cada uno con el dedo y consulta otra vez: debe decir «activado». Si uno está al revés, hay que invertir su pin en printer.cfg con «!».")}</Note>
                </Show>
              </div>

              {/* Motores */}
              <div class="space-y-1">
                <div class="flex items-center justify-between gap-2">
                  <span class="text-xs text-text-muted">{t("Sentido de los motores")}</span>
                  <Mark value={marks().motors} onChange={(v) => mark("motors", v)} />
                </div>
                <div class="flex flex-wrap gap-1">
                  <For each={info()!.steppers}>
                    {(s) => (
                      <Button
                        size="sm"
                        disabled={off()}
                        onClick={() =>
                          void run(
                            `buzz-${s}`,
                            { kind: "stepper_buzz", stepper: s },
                            askOf(
                              t("Probar el motor {s}", { s }),
                              t("{printer} va a mover un poquito el motor {s}, de ida y vuelta, 10 veces. Mira hacia dónde se va primero y que no choque con nada.", { printer: props.name, s }),
                            ),
                          )
                        }
                      >
                        {s}
                      </Button>
                    )}
                  </For>
                </div>
                <Note>{t("Cada motor se mueve 1 mm de ida y vuelta. Lo primero que hace es ir hacia «más»: X a la derecha, Y hacia el fondo (o la cama hacia ti, según la máquina), Z hacia arriba y el extrusor empujando el filamento. Si va al revés, se invierte con «!» en dir_pin.")}</Note>
              </div>

              {/* Ventiladores */}
              <div class="space-y-1">
                <div class="flex items-center justify-between gap-2">
                  <span class="text-xs text-text-muted">{t("Ventiladores")}</span>
                  <Mark value={marks().fans} onChange={(v) => mark("fans", v)} />
                </div>
                <For each={info()!.fans}>
                  {(f) => (
                    <div class="flex items-center justify-between gap-2 text-xs">
                      <span class="min-w-0 truncate text-text-muted">{f.name}</span>
                      <span class="font-mono text-text">{fmtNumber(f.percent, { maximumFractionDigits: 0 })} %</span>
                      <Show when={f.manual} fallback={<span class="text-[10px] text-text-dim">{t("automático")}</span>}>
                        <span class="inline-flex gap-1">
                          <Button size="sm" disabled={off()} onClick={() => void run(`fan-${f.name}`, { kind: "set_fan", name: f.name, percent: 100 }, askOf(t("Encender ventilador"), t("Se enciende el ventilador {f} a toda velocidad.", { f: f.name }), t("Encender"))).then(() => refresh())}>
                            {t("Encender")}
                          </Button>
                          <Button size="sm" disabled={off()} onClick={() => void run(`fan-${f.name}`, { kind: "set_fan", name: f.name, percent: 0 }).then(() => refresh())}>
                            {t("Apagar")}
                          </Button>
                        </span>
                      </Show>
                    </div>
                  )}
                </For>
                <Note>{t("Comprueba que el de la pieza sople hacia la boquilla y que el de la boquilla (se enciende solo al calentar) gire.")}</Note>
              </div>
            </div>
          </Fold>
        </div>

        {/* ── Extrusor ── */}
        <div>
          <Fold title={t("Extrusor (pasos)")} icon={<Icons.Wrench size={14} />}>
            <div class="space-y-2 pb-3">
              <Note>{t("Sirve para que, cuando le pides 100 mm de filamento, entren 100 mm de verdad.")}</Note>
              <ol class="list-decimal space-y-0.5 pl-4 text-[11px] text-text-muted">
                <li>{t("Con el filamento puesto, marca el filamento a 120 mm de la entrada del extrusor.")}</li>
                <li>{t("Pulsa «Extruir 100 mm»: calienta la boquilla y empuja 100 mm a 5 mm/s.")}</li>
                <li>{t("Mide cuántos mm quedan entre la entrada y la marca y escríbelos abajo.")}</li>
              </ol>
              <div class="grid grid-cols-[1fr_auto] items-end gap-1.5">
                <NumberInput label={t("Boquilla")} value={testTemp()} min={170} max={300} step={5} suffix="°C" onChange={setTestTemp} />
                <Button
                  size="sm"
                  disabled={off()}
                  onClick={() =>
                    void run(
                      "extrude",
                      { kind: "extrude_test", celsius: testTemp() },
                      askOf(t("Extruir 100 mm"), t("{printer} va a calentar la boquilla a {c} °C, empujar 100 mm de filamento y apagar la boquilla. Hace falta filamento puesto y la boquilla libre.", { printer: props.name, c: testTemp() })),
                    )
                  }
                >
                  {t("Extruir 100 mm")}
                </Button>
              </div>
              <Lines r={results().extrude} />
              <NumberInput label={t("Lo que queda hasta la marca")} value={remaining()} min={0} max={120} step={0.5} suffix="mm" onChange={setRemaining} />
              <div class="space-y-0.5 text-xs">
                <div class="flex justify-between">
                  <span class="text-text-muted">{t("Entró")}</span>
                  <span class="font-mono text-text">{fmtNumber(extruded(), { maximumFractionDigits: 1 })} mm</span>
                </div>
                <div class="flex justify-between">
                  <span class="text-text-muted">{t("rotation_distance actual")}</span>
                  <span class="font-mono text-text">{info()!.rotation_distance != null ? fmtNumber(info()!.rotation_distance!, { maximumFractionDigits: 4 }) : "—"}</span>
                </div>
                <div class="flex justify-between">
                  <span class="text-text-muted">{t("rotation_distance nuevo")}</span>
                  <span class="font-mono text-text">{newRotation() != null ? fmtNumber(newRotation()!, { maximumFractionDigits: 4 }) : "—"}</span>
                </div>
              </div>
              <Note>{t("Nuevo = actual × (lo que entró ÷ 100). Si entró menos de 100 mm, el número baja.")}</Note>
              <Show when={newRotation() != null && Math.abs(extruded() - 100) > 0.05}>
                <Button
                  size="sm"
                  class="w-full"
                  disabled={off() || (newRotation() ?? 0) < 1}
                  onClick={() =>
                    void run(
                      "rotation",
                      { kind: "save_rotation_distance", value: Math.round(newRotation()! * 10000) / 10000 },
                      askOf(
                        t("Guardar rotation_distance"),
                        t("Se hace una copia de printer.cfg (printer.cfg.pinocchio-FECHA.bak), se cambia solo rotation_distance de [extruder] a {v} y se reinicia Klipper. Si el valor no está en ese archivo, no se toca nada y te digo qué poner a mano.", { v: fmtNumber(newRotation()!, { maximumFractionDigits: 4 }) }),
                        t("Guardar y reiniciar"),
                      ),
                    ).then(() => setTimeout(() => void refresh(), 4000))
                  }
                >
                  {t("Guardar en printer.cfg")}
                </Button>
              </Show>
              <Lines r={results().rotation} />
            </div>
          </Fold>
        </div>

        {/* ── Sonda ── */}
        <div>
          <Fold title={t("Sonda (altura y precisión)")} icon={<Icons.Ruler size={14} />}>
            <div class="space-y-2 pb-3">
              <Show when={info()!.probe} fallback={<Note>{t("Esta impresora no tiene sonda (probe o BLTouch) en printer.cfg.")}</Note>}>
                {(p) => (
                  <>
                    <div class="flex justify-between text-xs">
                      <span class="text-text-muted">{t("Sonda")} ({p().kind})</span>
                      <span class="font-mono text-text">z_offset {p().z_offset != null ? fmtNumber(p().z_offset!, { maximumFractionDigits: 3 }) : "—"}</span>
                    </div>
                    <Note>{t("Prueba del papel: la boquilla baja hasta rozar una hoja de papel. Así Klipper sabe la distancia entre la sonda y la boquilla.")}</Note>
                    <Button
                      size="sm"
                      class="w-full"
                      disabled={off()}
                      onClick={() =>
                        void run("probecal", { kind: "probe_calibrate" }, askOf(t("Altura de la sonda"), t("{printer} va a ir al origen y a bajar la boquilla al centro de la cama. Pon una hoja de papel debajo.", { printer: props.name })))
                      }
                    >
                      {t("Empezar la prueba del papel")}
                    </Button>
                    <div class="grid grid-cols-4 gap-1">
                      <For each={[-0.1, -0.025, 0.025, 0.1]}>
                        {(d) => (
                          <Button size="sm" disabled={off()} onClick={() => void run("probecal", { kind: "test_z", delta: d })}>
                            {d > 0 ? "+" : "−"}
                            {fmtNumber(Math.abs(d), { maximumFractionDigits: 3 })}
                          </Button>
                        )}
                      </For>
                    </div>
                    <Note>{t("Baja (−) hasta que el papel roce al moverlo; sube (+) si queda apretado. Cuando roce un poco, pulsa «Aceptar».")}</Note>
                    <div class="grid grid-cols-3 gap-1">
                      <Button size="sm" disabled={off()} onClick={() => void run("probecal", { kind: "accept" })}>
                        {t("Aceptar")}
                      </Button>
                      <Button size="sm" disabled={off()} onClick={() => void run("probecal", { kind: "abort" })}>
                        {t("Cancelar")}
                      </Button>
                      <Button
                        size="sm"
                        disabled={off()}
                        onClick={() =>
                          void run("probecal", { kind: "save_config" }, askOf(t("Guardar en la impresora"), t("Se guarda el valor nuevo en printer.cfg y se reinicia Klipper (SAVE_CONFIG)."), t("Guardar y reiniciar"))).then(() => setTimeout(() => void refresh(), 4000))
                        }
                      >
                        {t("Guardar")}
                      </Button>
                    </div>
                    <Lines r={results().probecal} />

                    <div class="border-t border-border pt-2">
                      <Button
                        size="sm"
                        class="w-full"
                        disabled={off()}
                        onClick={() =>
                          void run("accuracy", { kind: "probe_accuracy" }, askOf(t("Precisión de la sonda"), t("{printer} va a ir al origen y tocar 10 veces el mismo punto para ver cuánto se desvía la sonda.", { printer: props.name })))
                        }
                      >
                        {t("Probar la precisión")}
                      </Button>
                      <Show when={results().accuracy?.data?.range != null}>
                        {(() => {
                          const d = () => results().accuracy.data as { range: number; stddev?: number; average?: number };
                          return (
                            <div class="mt-1 space-y-0.5 text-xs">
                              <div class="flex justify-between">
                                <span class="text-text-muted">{t("Diferencia entre el mejor y el peor toque")}</span>
                                <span class="font-mono text-text">{fmtNumber(d().range, { maximumFractionDigits: 4 })} mm</span>
                              </div>
                              <Show when={d().stddev != null}>
                                <div class="flex justify-between">
                                  <span class="text-text-muted">{t("Desvío")}</span>
                                  <span class="font-mono text-text">{fmtNumber(d().stddev!, { maximumFractionDigits: 4 })} mm</span>
                                </div>
                              </Show>
                              <p class={`text-[11px] ${d().range <= 0.025 ? "text-green" : "text-orange"}`}>
                                {d().range <= 0.025 ? t("Muy buena: la sonda repite casi igual.") : t("La sonda se mueve más de 0,025 mm entre toques: revisa que esté firme y sin cables tirantes.")}
                              </p>
                            </div>
                          );
                        })()}
                      </Show>
                      <Lines r={results().accuracy} />
                    </div>
                  </>
                )}
              </Show>
            </div>
          </Fold>
        </div>

        {/* ── Tornillos de la cama ── */}
        <div>
          <Fold title={t("Tornillos de la cama")} icon={<Icons.Wrench size={14} />}>
            <div class="space-y-2 pb-3">
              <Show
                when={info()!.has_screws_tilt}
                fallback={
                  <>
                    <Note>{t("printer.cfg no tiene la sección [screws_tilt_adjust], así que Klipper no puede calcular cuánto girar cada tornillo.")}</Note>
                    <Show when={info()!.has_bed_screws} fallback={<Note>{t("Tampoco tiene [bed_screws]. Agrega una de las dos secciones con la posición de cada tornillo.")}</Note>}>
                      <Note>{t("Sí tiene [bed_screws]: Klipper te lleva de a un tornillo; lo ajustas con el papel y pulsas «Ajustado» para pasar al siguiente.")}</Note>
                      <Button size="sm" class="w-full" disabled={off()} onClick={() => void run("screws", { kind: "bed_screws_adjust" }, askOf(t("Tornillos de la cama"), t("{printer} va a ir al origen y parar sobre cada tornillo de la cama, uno por uno.", { printer: props.name })))}>
                        {t("Empezar paso a paso")}
                      </Button>
                      <div class="grid grid-cols-3 gap-1">
                        <Button size="sm" disabled={off()} onClick={() => void run("screws", { kind: "adjusted" })}>
                          {t("Ajustado")}
                        </Button>
                        <Button size="sm" disabled={off()} onClick={() => void run("screws", { kind: "accept" })}>
                          {t("Aceptar")}
                        </Button>
                        <Button size="sm" disabled={off()} onClick={() => void run("screws", { kind: "abort" })}>
                          {t("Cancelar")}
                        </Button>
                      </div>
                      <Lines r={results().screws} />
                    </Show>
                  </>
                }
              >
                <Note>{t("La sonda mide la altura sobre cada tornillo y dice cuánto girarlo. Con la cama fría y limpia.")}</Note>
                <Button size="sm" class="w-full" disabled={off()} onClick={() => void run("screws", { kind: "screws_tilt" }, askOf(t("Medir los tornillos"), t("{printer} va a ir al origen y a tocar la cama sobre cada tornillo.", { printer: props.name }))).then(() => refresh())}>
                  {t("Medir los tornillos")}
                </Button>
                {(() => {
                  const screws = () => (results().screws?.data as SetupScrew[] | undefined) ?? info()!.screws;
                  return (
                    <Show when={screws().length}>
                      <ScrewMap screws={screws()} />
                    </Show>
                  );
                })()}
                <Lines r={results().screws} />
              </Show>
            </div>
          </Fold>
        </div>

        {/* ── Malla de la cama ── */}
        <div>
          <Fold title={t("Malla de la cama")} icon={<Icons.Ruler size={14} />}>
            <div class="space-y-2 pb-3">
              <Show when={mesh()} fallback={<Note>{t("Todavía no hay malla guardada. Usa «Nivelar la cama» en Calibrar.")}</Note>}>
                {(m) => <MeshMap mesh={m()} range={meshRange()} />}
              </Show>
            </div>
          </Fold>
        </div>
      </Show>
    </>
  );
};

/** La cama vista de frente con cada tornillo y lo que hay que girarle */
const ScrewMap: Component<{ screws: SetupScrew[] }> = (props) => {
  const box = () => {
    const xs = props.screws.map((s) => s.x);
    const ys = props.screws.map((s) => s.y);
    const pad = 30;
    const [x0, x1, y0, y1] = [Math.min(...xs) - pad, Math.max(...xs) + pad, Math.min(...ys) - pad, Math.max(...ys) + pad];
    return { x0, y0, w: Math.max(1, x1 - x0), h: Math.max(1, y1 - y0) };
  };
  return (
    <div class="space-y-1">
      <svg viewBox={`0 0 ${box().w} ${box().h}`} class="w-full rounded-sm border border-border" style={{ background: "var(--color-bg-darker)" }}>
        <rect x="6" y="6" width={box().w - 12} height={box().h - 12} rx="6" fill="none" style={{ stroke: "var(--color-border-hover)", "stroke-width": 1.5 }} />
        <For each={props.screws}>
          {(s) => {
            const cx = s.x - box().x0;
            const cy = box().h - (s.y - box().y0);
            const fine = !s.base && (!s.turn || s.turn === "00:00");
            const color = s.base ? "var(--color-accent)" : fine ? "var(--color-green)" : "var(--color-orange)";
            return (
              <g>
                <circle cx={cx} cy={cy} r="11" style={{ fill: "var(--color-bg)", stroke: color, "stroke-width": 2.5 }} />
                <text x={cx} y={cy + 4} text-anchor="middle" font-size="12" style={{ fill: color }}>
                  {s.base ? "●" : fine ? "✓" : s.dir === "CCW" ? "↺" : "↻"}
                </text>
              </g>
            );
          }}
        </For>
      </svg>
      <For each={props.screws}>
        {(s) => (
          <div class="flex justify-between gap-2 text-xs">
            <span class="text-text-muted">{s.name}</span>
            <span class="text-right text-text">{s.base ? t("referencia (no se toca)") : turnText(s.dir, s.turn)}</span>
          </div>
        )}
      </For>
      <Note>{t("Vista desde el frente. Derecha (CW) = en el sentido del reloj, mirando el tornillo desde arriba. Gira un poco, vuelve a medir y repite hasta que todos digan «está bien».")}</Note>
    </div>
  );
};

/** Mapa de colores de la malla con sus valores y un veredicto en palabras simples */
const MeshMap: Component<{ mesh: SetupMesh; range: number }> = (props) => {
  const flat = () => props.mesh.rows.flat();
  const lo = () => Math.min(...flat());
  const hi = () => Math.max(...flat());
  const cols = () => props.mesh.rows[0]?.length ?? 1;
  const rows = () => props.mesh.rows.length;
  const cell = 34;
  // Fila 0 = y mínima: abajo, como se mira la cama de frente
  const shown = () => [...props.mesh.rows].reverse();
  const verdict = () =>
    props.range <= 0.2
      ? { text: t("Está bien: la cama está bastante plana."), cls: "text-green" }
      : props.range <= 0.4
        ? { text: t("Se puede imprimir, pero conviene nivelar los tornillos para mejorar la primera capa."), cls: "text-orange" }
        : { text: t("Conviene nivelar los tornillos antes de seguir: hay mucho desnivel."), cls: "text-red" };
  return (
    <div class="space-y-1">
      <div class="flex justify-between text-xs">
        <span class="text-text-muted">{props.mesh.profile || t("Malla de la cama")}</span>
        <span class="font-mono text-text">{t("desnivel {d} mm", { d: fmtNumber(props.range, { maximumFractionDigits: 3 }) })}</span>
      </div>
      <svg viewBox={`0 0 ${cols() * cell} ${rows() * cell}`} class="w-full rounded-sm">
        <For each={shown()}>
          {(row, ri) => (
            <For each={row}>
              {(z, ci) => (
                <g>
                  <rect x={ci() * cell} y={ri() * cell} width={cell - 1} height={cell - 1} rx="2" style={{ fill: meshFill(z, lo(), hi()) }} />
                  <text x={ci() * cell + cell / 2} y={ri() * cell + cell / 2 + 3} text-anchor="middle" font-size="9" font-family="monospace" style={{ fill: "var(--color-bg)" }}>
                    {fmtNumber(z, { maximumFractionDigits: 2 })}
                  </text>
                </g>
              )}
            </For>
          )}
        </For>
      </svg>
      <div class="flex justify-between font-mono text-[10px] text-text-dim">
        <span>{t("más bajo {v} mm", { v: fmtNumber(lo(), { maximumFractionDigits: 3 }) })}</span>
        <span>{t("más alto {v} mm", { v: fmtNumber(hi(), { maximumFractionDigits: 3 }) })}</span>
      </div>
      <p class={`text-[11px] ${verdict().cls}`}>{verdict().text}</p>
      <Note>{t("Vista desde el frente. Rojo = más alto, azul/violeta = más bajo. Si la malla es muy irregular, primero se nivelan los tornillos y después se vuelve a medir.")}</Note>
    </div>
  );
};
