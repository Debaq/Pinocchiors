import { Component, Show, createSignal, onCleanup, onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { Button, Panel, ProgressBar } from "../ui";
import * as Icons from "../icons";
import { fmtNumber, t } from "../../lib/i18n";
import { tBackend } from "../../lib/i18nBackend.ts";
import { printerConnection, setPrinterConnection } from "../../lib/settings";
import type { ConfirmRequest } from "../layout/ConfirmDialog";

// ═══════════════════════════════════════════════════════════════════════════
// TIPOS (espejo de apps/desktop/src/moonraker.rs)
// ═══════════════════════════════════════════════════════════════════════════

interface PrinterInfo {
  url: string;
  hostname: string;
  klippy_state: string;
  message: string;
  moonraker_version: string;
  klipper_version: string;
}

interface PrinterStatus {
  klippy_state: string;
  state: string;
  message: string;
  filename: string;
  progress: number;
  print_duration: number;
  total_duration: number;
  remaining: number | null;
  layer: number | null;
  layers: number | null;
  filament_mm: number;
  extruder: [number, number];
  bed: [number, number];
}

interface SendResult {
  filename: string;
  started: boolean;
}

const KLIPPY: Record<string, string> = {
  ready: "Klipper listo",
  startup: "Klipper arrancando",
  shutdown: "Klipper detenido",
  error: "Klipper con error",
  disconnected: "Klipper desconectado",
};

const STATES: Record<string, string> = {
  standby: "En espera",
  printing: "Imprimiendo",
  paused: "En pausa",
  complete: "Terminada",
  cancelled: "Cancelada",
  error: "Con error",
};

const errText = (e: unknown) => (typeof e === "string" ? tBackend(e) : String(e));

/** Argumentos de conexión para los comandos de Moonraker */
const conn = () => {
  const c = printerConnection();
  return { address: c.address, apiKey: c.apiKey.trim() || null };
};

function fmtDuration(seconds: number): string {
  const s = Math.max(0, Math.round(seconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  return h > 0 ? `${h} h ${m} min` : `${m} min`;
}

/** Nombre propuesto para el gcode en la impresora: pinocchio-AAAAMMDD-HHMM */
function defaultName(): string {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  return `pinocchio-${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}-${p(d.getHours())}${p(d.getMinutes())}`;
}

// Lo último que se mandó (lo muestran las dos pestañas)
const [lastSent, setLastSent] = createSignal<string>();

// ═══════════════════════════════════════════════════════════════════════════
// ENVIAR (en el resumen del laminado)
// ═══════════════════════════════════════════════════════════════════════════

/** «Enviar e imprimir» y «Solo enviar» del último laminado */
export const SendToPrinter: Component<{ onSent?: () => void; onConfigure?: () => void }> = (props) => {
  const [name, setName] = createSignal(defaultName());
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string>();

  const send = async (start: boolean) => {
    setBusy(true);
    setError(undefined);
    try {
      const r = await invoke<SendResult>("moonraker_send", { ...conn(), filename: name(), start });
      setLastSent(
        !start
          ? t("Enviado a la impresora: {file}", { file: r.filename })
          : r.started
            ? t("Imprimiendo {file}", { file: r.filename })
            : t("Se subió {file}, pero no empezó: la impresora está ocupada", { file: r.filename }),
      );
      props.onSent?.();
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Show
      when={printerConnection().address.trim()}
      fallback={
        <button class="text-[11px] text-text-dim hover:underline text-left" onClick={() => props.onConfigure?.()}>
          {t("Para mandarlo a la impresora, configúrala en la pestaña Impresora")}
        </button>
      }
    >
      <div class="space-y-1.5">
        <input
          class="w-full px-2 py-1 rounded bg-surface/40 border border-border text-xs text-text outline-none focus:border-accent"
          title={t("Nombre del archivo en la impresora")}
          value={name()}
          onInput={(e) => setName(e.currentTarget.value)}
        />
        <div class="flex gap-1.5">
          <Button onClick={() => send(true)} disabled={busy() || !name().trim()} size="sm" variant="primary" icon={<Icons.Printer size={14} />} class="flex-1">
            {busy() ? t("Enviando...") : t("Enviar e imprimir")}
          </Button>
          <Button onClick={() => send(false)} disabled={busy() || !name().trim()} size="sm" variant="ghost" title={t("Subir el gcode sin empezar a imprimir")}>
            {t("Solo enviar")}
          </Button>
        </div>
        <Show when={error()}>
          <p class="text-[11px] text-red">{error()}</p>
        </Show>
      </div>
    </Show>
  );
};

// ═══════════════════════════════════════════════════════════════════════════
// PESTAÑA «IMPRESORA»: conexión, estado y controles
// ═══════════════════════════════════════════════════════════════════════════

export const PrinterTab: Component<{ confirm: (request: ConfirmRequest) => Promise<boolean> }> = (props) => {
  const [address, setAddress] = createSignal(printerConnection().address);
  const [apiKey, setApiKey] = createSignal(printerConnection().apiKey);
  const [testing, setTesting] = createSignal(false);
  const [info, setInfo] = createSignal<PrinterInfo>();
  const [status, setStatus] = createSignal<PrinterStatus>();
  const [error, setError] = createSignal<string>();
  const [acting, setActing] = createSignal(false);

  const save = () => setPrinterConnection({ address: address().trim(), apiKey: apiKey().trim() });

  const test = async () => {
    save();
    if (!address().trim()) return;
    setTesting(true);
    setError(undefined);
    try {
      setInfo(await invoke<PrinterInfo>("moonraker_info", conn()));
      await poll();
    } catch (e) {
      setInfo(undefined);
      setStatus(undefined);
      setError(errText(e));
    } finally {
      setTesting(false);
    }
  };

  // Estado cada 3 s mientras la pestaña está abierta (cada 10 s si no responde)
  let timer: ReturnType<typeof setTimeout> | undefined;
  let alive = true;
  const poll = async () => {
    if (!printerConnection().address) return;
    try {
      setStatus(await invoke<PrinterStatus>("moonraker_status", conn()));
      setError(undefined);
    } catch (e) {
      setStatus(undefined);
      setError(errText(e));
    }
  };
  const loop = async () => {
    await poll();
    if (alive) timer = setTimeout(loop, error() ? 10000 : 3000);
  };
  onMount(() => {
    if (printerConnection().address) void loop();
  });
  onCleanup(() => {
    alive = false;
    clearTimeout(timer);
  });

  const act = async (action: "pause" | "resume" | "cancel") => {
    if (action === "cancel") {
      const ok = await props.confirm({
        title: t("Cancelar la impresión"),
        message: t("La impresora deja de imprimir {file} y no se puede retomar.", { file: status()?.filename ?? "" }),
        confirmLabel: t("Cancelar la impresión"),
        cancelLabel: t("Seguir imprimiendo"),
        danger: true,
      });
      if (!ok) return;
    }
    setActing(true);
    try {
      await invoke("moonraker_action", { ...conn(), action });
      await poll();
    } catch (e) {
      setError(errText(e));
    } finally {
      setActing(false);
    }
  };

  const printing = () => ["printing", "paused"].includes(status()?.state ?? "");
  const temp = (v: [number, number]) =>
    v[1] > 0 ? `${fmtNumber(v[0], { maximumFractionDigits: 0 })} / ${fmtNumber(v[1], { maximumFractionDigits: 0 })} °C` : `${fmtNumber(v[0], { maximumFractionDigits: 0 })} °C`;

  return (
    <div class="[column-width:280px] [column-gap:1rem] [&>div>div]:break-inside-avoid [&>div>div]:mb-3">
      <div>
        <Panel id="printer.connection" dense title={t("Conexión")} icon={<Icons.Printer size={14} />} defaultOpen>
          <div class="space-y-2 pb-3">
            <p class="text-[10px] text-text-dim">{t("Impresoras con Klipper y Moonraker (por ejemplo una K1 rooteada), en la misma red.")}</p>
            <label class="block space-y-0.5">
              <span class="text-[11px] text-text-muted">{t("Dirección de la impresora")}</span>
              <input
                class="w-full px-2 py-1 rounded bg-surface/40 border border-border text-xs text-text outline-none focus:border-accent"
                placeholder={t("192.168.1.50 o k1.local")}
                value={address()}
                onInput={(e) => setAddress(e.currentTarget.value)}
                onChange={save}
                onKeyDown={(e) => {
                  e.stopPropagation();
                  if (e.key === "Enter") void test();
                }}
              />
            </label>
            <label class="block space-y-0.5">
              <span class="text-[11px] text-text-muted">{t("Clave de API (si la pide)")}</span>
              <input
                type="password"
                class="w-full px-2 py-1 rounded bg-surface/40 border border-border text-xs text-text outline-none focus:border-accent"
                value={apiKey()}
                onInput={(e) => setApiKey(e.currentTarget.value)}
                onChange={save}
                onKeyDown={(e) => e.stopPropagation()}
              />
            </label>
            <Button onClick={test} disabled={testing() || !address().trim()} fullWidth size="sm" icon={<Icons.ArrowsClockwise size={14} />}>
              {testing() ? t("Conectando...") : t("Probar conexión")}
            </Button>
            <Show when={info()}>
              {(i) => (
                <p class={`text-[11px] ${i().klippy_state === "ready" ? "text-green" : "text-warning"}`}>
                  {t("Conectada: {name}", { name: i().hostname || i().url })} · {t(KLIPPY[i().klippy_state] ?? i().klippy_state)}
                  <Show when={i().klippy_state !== "ready" && i().message}>
                    <span class="block text-text-dim">{i().message}</span>
                  </Show>
                </p>
              )}
            </Show>
            <Show when={error()}>
              <p class="text-[11px] text-red">{error()}</p>
            </Show>
          </div>
        </Panel>
      </div>
      <Show when={status()}>
        {(s) => (
          <div>
            <Panel id="printer.status" dense title={t("Estado")} defaultOpen>
              <div class="space-y-1.5 pb-3">
                <Show
                  when={s().klippy_state === "ready"}
                  fallback={
                    <p class="text-[11px] text-warning">
                      {t(KLIPPY[s().klippy_state] ?? s().klippy_state)}
                      <Show when={s().message}>
                        <span class="block text-text-dim">{s().message}</span>
                      </Show>
                    </p>
                  }
                >
                  <Row label={t("Impresora")} value={t(STATES[s().state] ?? s().state)} />
                  <Show when={s().filename}>
                    <Row label={t("Archivo")} value={s().filename} />
                  </Show>
                  <Show when={printing() || s().state === "complete"}>
                    <ProgressBar value={100 * s().progress} size="sm" variant={s().state === "paused" ? "warning" : "default"} />
                    <Row label={t("Progreso")} value={`${fmtNumber(100 * s().progress, { maximumFractionDigits: 0 })} %`} />
                    <Show when={s().layer != null && s().layers}>
                      <Row label={t("Capa")} value={`${s().layer} / ${s().layers}`} />
                    </Show>
                    <Row label={t("Imprimiendo hace")} value={fmtDuration(s().print_duration)} />
                    <Show when={printing() && s().remaining != null}>
                      <Row label={t("Falta")} value={`~${fmtDuration(s().remaining!)}`} />
                    </Show>
                  </Show>
                  <Row label={t("Boquilla")} value={temp(s().extruder)} />
                  <Row label={t("Cama")} value={temp(s().bed)} />
                  <Show when={s().message}>
                    <p class="text-[11px] text-warning">{s().message}</p>
                  </Show>
                  <Show when={printing()}>
                    <div class="flex gap-1.5 pt-1">
                      <Show
                        when={s().state === "paused"}
                        fallback={
                          <Button onClick={() => act("pause")} disabled={acting()} size="sm" icon={<Icons.Pause size={14} />} class="flex-1">
                            {t("Pausar")}
                          </Button>
                        }
                      >
                        <Button onClick={() => act("resume")} disabled={acting()} size="sm" icon={<Icons.Play size={14} />} class="flex-1">
                          {t("Reanudar")}
                        </Button>
                      </Show>
                      <Button onClick={() => act("cancel")} disabled={acting()} size="sm" variant="danger" icon={<Icons.Stop size={14} />} class="flex-1">
                        {t("Cancelar")}
                      </Button>
                    </div>
                  </Show>
                </Show>
                <Show when={lastSent()}>
                  <p class="text-[10px] text-green">{lastSent()}</p>
                </Show>
              </div>
            </Panel>
          </div>
        )}
      </Show>
    </div>
  );
};

const Row: Component<{ label: string; value: string }> = (props) => (
  <div class="flex justify-between text-xs gap-2">
    <span class="text-text-muted shrink-0">{props.label}</span>
    <span class="text-text font-mono truncate">{props.value}</span>
  </div>
);
