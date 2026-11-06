import { Component, For, Show, createEffect, createSignal, on, onCleanup } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { clsx } from "clsx";
import { Button, Panel, ProgressBar, Select } from "../ui";
import * as Icons from "../icons";
import { fmtNumber, t } from "../../lib/i18n";
import { tBackend } from "../../lib/i18nBackend.ts";
import { netPrinters, removeNetPrinter, saveNetPrinter, selectNetPrinter, selectedNetPrinter, type NetPrinter, type NetPrinterKind } from "../../lib/settings";
import { PRINTER_MODELS, printerImage } from "../../lib/printerModels";
import type { ConfirmRequest } from "../layout/ConfirmDialog";
import { PrinterControls } from "./PrinterControls";

// ═══════════════════════════════════════════════════════════════════════════
// TIPOS (espejo de apps/desktop/src/printers.rs, moonraker.rs y flashforge.rs)
// ═══════════════════════════════════════════════════════════════════════════

interface Slot {
  id: number;
  has_filament: boolean;
  material: string;
  color: string;
}

interface NetPrinterInfo {
  kind: NetPrinterKind;
  name: string;
  ready: boolean;
  state: string;
  message: string;
  firmware: string;
  serial: string | null;
  slots: Slot[];
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
  slots: Slot[];
  current_slot: number | null;
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

/** Cómo llegar a la impresora (argumento `conn` de los comandos) */
const conn = (p: NetPrinter) => ({
  kind: p.kind,
  address: p.address,
  apiKey: p.apiKey.trim() || null,
  checkCode: p.checkCode.trim() || null,
  serial: p.serial.trim() || null,
});

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

const inputClass = "w-full px-2 py-1 rounded bg-surface/40 border border-border text-xs text-text outline-none focus:border-accent";

// Lo último que se mandó (lo muestran las dos pestañas)
const [lastSent, setLastSent] = createSignal<string>();

// ═══════════════════════════════════════════════════════════════════════════
// PIEZAS
// ═══════════════════════════════════════════════════════════════════════════

/** Foto del modelo (o un ícono si no hay) */
export const PrinterPicture: Component<{ model?: string; size?: number; class?: string }> = (props) => {
  const size = () => props.size ?? 40;
  return (
    <Show
      when={printerImage(props.model)}
      fallback={
        <div class={clsx("flex items-center justify-center rounded bg-surface/40 text-text-dim shrink-0", props.class)} style={{ width: `${size()}px`, height: `${size()}px` }}>
          <Icons.Printer size={Math.round(size() / 2)} />
        </div>
      }
    >
      {(src) => <img src={src()} alt={props.model ?? ""} class={clsx("object-contain shrink-0", props.class)} style={{ width: `${size()}px`, height: `${size()}px` }} draggable={false} />}
    </Show>
  );
};

/** Carretes del cambiador (AD5X): con filamento, material y el que está en uso */
const SlotChips: Component<{ slots: Slot[]; current?: number | null; selected?: number; onSelect?: (id: number) => void }> = (props) => (
  <div class="grid grid-cols-4 gap-1">
    <For each={props.slots}>
      {(s) => (
        <button
          type="button"
          disabled={!props.onSelect || !s.has_filament}
          onClick={() => props.onSelect?.(s.id)}
          class={clsx(
            "flex flex-col items-center gap-0.5 rounded border px-1 py-1 text-[10px] transition-colors",
            props.selected === s.id ? "border-accent bg-accent/15 text-accent" : "border-border text-text-muted",
            !s.has_filament && "opacity-40",
            props.onSelect && s.has_filament && "hover:border-accent/60"
          )}
          title={s.has_filament ? t("Carrete {n}", { n: s.id }) : t("Carrete {n} vacío", { n: s.id })}
        >
          <span class="inline-block w-3 h-3 rounded-full border border-border" style={{ background: s.has_filament && s.color ? s.color : "transparent" }} />
          <span class="font-mono">{s.id}</span>
          <span class="truncate max-w-full">{s.has_filament ? s.material || "?" : t("vacío")}</span>
          <Show when={props.current === s.id}>
            <span class="text-[9px] text-green">{t("en uso")}</span>
          </Show>
        </button>
      )}
    </For>
  </div>
);

// ═══════════════════════════════════════════════════════════════════════════
// ENVIAR (en el resumen del laminado)
// ═══════════════════════════════════════════════════════════════════════════

/** «Enviar e imprimir» y «Solo enviar» del último laminado */
export const SendToPrinter: Component<{
  /** Modelo para el que se laminó (`printer_model`): si no es el de la impresora, no se manda */
  slicedModel?: string;
  onSent?: () => void;
  onConfigure?: () => void;
}> = (props) => {
  const [name, setName] = createSignal(defaultName());
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string>();
  const [slots, setSlots] = createSignal<Slot[]>([]);
  const [slot, setSlot] = createSignal<number>();

  const target = () => selectedNetPrinter();
  const mismatch = () => {
    const p = target();
    return !!p && !!props.slicedModel && p.model !== props.slicedModel;
  };

  // Carretes de la AD5X: se preguntan al elegirla; se propone el que está en uso o el primero con filamento
  createEffect(
    on(
      () => target()?.id,
      () => {
        setSlots([]);
        setSlot(undefined);
        setError(undefined);
        const p = target();
        if (p?.kind !== "flashforge") return;
        invoke<PrinterStatus>("printer_status", { conn: conn(p) })
          .then((st) => {
            setSlots(st.slots);
            const loaded = st.slots.filter((s) => s.has_filament);
            setSlot(loaded.find((s) => s.id === st.current_slot)?.id ?? loaded[0]?.id);
          })
          .catch((e) => setError(errText(e)));
      }
    )
  );

  const needsSlot = () => target()?.kind === "flashforge" && slots().length > 0;

  const send = async (start: boolean) => {
    const p = target();
    if (!p) return;
    setBusy(true);
    setError(undefined);
    try {
      const r = await invoke<SendResult>("printer_send", { conn: conn(p), filename: name(), start, slot: needsSlot() ? (slot() ?? null) : null });
      setLastSent(
        !start
          ? t("Enviado a {printer}: {file}", { printer: p.name, file: r.filename })
          : r.started
            ? t("Imprimiendo en {printer}: {file}", { printer: p.name, file: r.filename })
            : t("Se subió {file}, pero no empezó: la impresora está ocupada", { file: r.filename })
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
      when={target()}
      fallback={
        <button class="text-[11px] text-text-dim hover:underline text-left" onClick={() => props.onConfigure?.()}>
          {t("Para mandarlo a una impresora, agrégala en la pestaña Impresora")}
        </button>
      }
    >
      {(p) => (
        <div class="space-y-1.5">
          <div class="flex items-center gap-2 rounded border border-border p-1.5">
            <PrinterPicture model={p().model} size={48} />
            <div class="min-w-0 flex-1">
              <Show when={netPrinters().length > 1} fallback={<p class="text-xs font-semibold text-text truncate">{p().name}</p>}>
                <Select options={netPrinters().map((x) => ({ value: x.id, label: x.name }))} value={p().id} onChange={(v) => selectNetPrinter(v)} />
              </Show>
              <p class="text-[10px] text-text-dim truncate">
                {p().model} · {p().address}
              </p>
            </div>
          </div>
          <Show when={mismatch()}>
            <p class="text-[11px] text-red">
              {t("Este gcode se laminó para {sliced} y la impresora es {model}: elige esa impresora al laminar.", { sliced: props.slicedModel!, model: p().model })}
            </p>
          </Show>
          <Show when={needsSlot()}>
            <p class="text-[10px] text-text-dim">{t("Carrete")}</p>
            <SlotChips slots={slots()} selected={slot()} onSelect={setSlot} />
          </Show>
          <input class={inputClass} title={t("Nombre del archivo en la impresora")} value={name()} onInput={(e) => setName(e.currentTarget.value)} onKeyDown={(e) => e.stopPropagation()} />
          <div class="flex gap-1.5">
            <Button
              onClick={() => send(true)}
              disabled={busy() || !name().trim() || mismatch() || (needsSlot() && !slot())}
              size="sm"
              variant="primary"
              icon={<Icons.Printer size={14} />}
              class="flex-1"
            >
              {busy() ? t("Enviando...") : t("Enviar e imprimir")}
            </Button>
            <Button onClick={() => send(false)} disabled={busy() || !name().trim() || mismatch()} size="sm" variant="ghost" title={t("Subir el gcode sin empezar a imprimir")}>
              {t("Solo enviar")}
            </Button>
          </div>
          <Show when={error()}>
            <p class="text-[11px] text-red">{error()}</p>
          </Show>
        </div>
      )}
    </Show>
  );
};

// ═══════════════════════════════════════════════════════════════════════════
// PESTAÑA «IMPRESORA»: impresoras guardadas, estado y controles
// ═══════════════════════════════════════════════════════════════════════════

const KIND_DEFAULT_MODEL: Record<NetPrinterKind, string> = { moonraker: "Creality K1", creality: "Creality K1", flashforge: "Flashforge AD5X" };

/** Modelos que se pueden elegir para un tipo de conexión (las K1 van por Klipper o de fábrica) */
const modelsFor = (kind: NetPrinterKind) => PRINTER_MODELS.filter((m) => m.kind === kind || (kind === "creality" && m.model.startsWith("Creality K1")));

function newPrinter(kind: NetPrinterKind): NetPrinter {
  const n = netPrinters().filter((p) => p.kind === kind).length + 1;
  return {
    id: `${kind}-${Date.now().toString(36)}`,
    name: kind === "flashforge" ? `AD5X ${n}` : kind === "creality" ? `K1 fábrica ${n}` : `K1 ${n}`,
    kind,
    model: KIND_DEFAULT_MODEL[kind],
    address: "",
    apiKey: "",
    checkCode: "",
    serial: "",
  };
}

export const PrinterTab: Component<{ confirm: (request: ConfirmRequest) => Promise<boolean> }> = (props) => {
  const [testing, setTesting] = createSignal(false);
  const [info, setInfo] = createSignal<NetPrinterInfo>();
  const [status, setStatus] = createSignal<PrinterStatus>();
  const [error, setError] = createSignal<string>();
  const [acting, setActing] = createSignal(false);
  const [notice, setNotice] = createSignal<string>();

  const current = () => selectedNetPrinter();
  const update = (patch: Partial<NetPrinter>) => {
    const p = current();
    if (p) saveNetPrinter({ ...p, ...patch });
  };

  const add = (kind: NetPrinterKind) => {
    const p = newPrinter(kind);
    saveNetPrinter(p);
    selectNetPrinter(p.id);
  };

  const remove = async () => {
    const p = current();
    if (!p) return;
    const ok = await props.confirm({
      title: t("Quitar impresora"),
      message: t("Se olvida {name} ({address}) en este equipo. La impresora no cambia.", { name: p.name, address: p.address || "—" }),
      confirmLabel: t("Quitar"),
      danger: true,
    });
    if (ok) {
      removeNetPrinter(p.id);
      selectNetPrinter(netPrinters()[0]?.id ?? "");
    }
  };

  const test = async () => {
    const p = current();
    if (!p?.address.trim()) return;
    setTesting(true);
    setError(undefined);
    setNotice(undefined);
    try {
      let i: NetPrinterInfo;
      try {
        i = await invoke<NetPrinterInfo>("printer_info", { conn: conn(p) });
      } catch (e) {
        // Una K1 agregada como Klipper que en realidad tiene el firmware de fábrica: se reconoce y se cambia sola
        if (p.kind !== "moonraker") throw e;
        const asCreality = { ...p, kind: "creality" as const };
        i = await invoke<NetPrinterInfo>("printer_info", { conn: conn(asCreality) }).catch(() => Promise.reject(e));
        update({ kind: "creality", model: p.model.startsWith("Creality K1") ? p.model : "Creality K1" });
        setNotice(t("Esta K1 tiene el firmware de fábrica (sin Klipper): se cambió a «Creality de fábrica»."));
      }
      setInfo(i);
      // Se guarda el número de serie y, en Flashforge, el modelo que dice la impresora
      const patch: Partial<NetPrinter> = {};
      if (i.serial && i.serial !== p.serial) patch.serial = i.serial;
      const known = modelsFor(p.kind).find((m) => i.name.toLowerCase().includes(m.model.split(" ").pop()!.toLowerCase()));
      if (p.kind === "flashforge" && known && known.model !== p.model) patch.model = known.model;
      if (Object.keys(patch).length) update(patch);
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
    const p = current();
    if (!p?.address.trim()) return;
    try {
      setStatus(await invoke<PrinterStatus>("printer_status", { conn: conn(p) }));
      setError(undefined);
    } catch (e) {
      setStatus(undefined);
      setError(errText(e));
    }
  };
  const loop = async () => {
    clearTimeout(timer);
    await poll();
    if (alive) timer = setTimeout(loop, error() ? 10000 : 3000);
  };
  // Al cambiar de impresora se empieza de cero
  createEffect(
    on(
      () => current()?.id,
      () => {
        setInfo(undefined);
        setStatus(undefined);
        setError(undefined);
        void loop();
      }
    )
  );
  onCleanup(() => {
    alive = false;
    clearTimeout(timer);
  });

  const act = async (action: "pause" | "resume" | "cancel") => {
    const p = current();
    if (!p) return;
    if (action === "cancel") {
      const ok = await props.confirm({
        title: t("Cancelar la impresión"),
        message: t("{printer} deja de imprimir {file} y no se puede retomar.", { printer: p.name, file: status()?.filename ?? "" }),
        confirmLabel: t("Cancelar la impresión"),
        cancelLabel: t("Seguir imprimiendo"),
        danger: true,
      });
      if (!ok) return;
    }
    setActing(true);
    try {
      await invoke("printer_action", { conn: conn(p), action });
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
        <Panel id="printer.list" dense title={t("Impresoras")} icon={<Icons.Printer size={14} />} defaultOpen>
          <div class="space-y-1.5 pb-3">
            <For each={netPrinters()} fallback={<p class="text-[11px] text-text-dim">{t("Todavía no hay impresoras. Agrega una:")}</p>}>
              {(p) => (
                <button
                  type="button"
                  onClick={() => selectNetPrinter(p.id)}
                  class={clsx(
                    "flex w-full items-center gap-2 rounded border p-1.5 text-left transition-colors",
                    current()?.id === p.id ? "border-accent bg-accent/10" : "border-border hover:border-accent/50"
                  )}
                >
                  <PrinterPicture model={p.model} size={44} />
                  <span class="min-w-0 flex-1">
                    <span class="block text-xs font-semibold text-text truncate">{p.name}</span>
                    <span class="block text-[10px] text-text-dim truncate">{p.model}</span>
                    <span class="block text-[10px] text-text-dim font-mono truncate">{p.address || t("sin dirección")}</span>
                  </span>
                </button>
              )}
            </For>
            <div class="flex gap-1.5">
              <Button size="sm" variant="ghost" icon={<Icons.Plus size={12} />} onClick={() => add("moonraker")} class="flex-1" title={t("Impresoras con Klipper y Moonraker, como una K1 rooteada")}>
                {t("Klipper (K1)")}
              </Button>
              <Button size="sm" variant="ghost" icon={<Icons.Plus size={12} />} onClick={() => add("creality")} class="flex-1" title={t("Creality K1, K1C o K1 Max con su firmware de fábrica (sin root)")}>
                {t("K1 de fábrica")}
              </Button>
              <Button size="sm" variant="ghost" icon={<Icons.Plus size={12} />} onClick={() => add("flashforge")} class="flex-1" title={t("Flashforge con su firmware de fábrica, como la AD5X")}>
                {t("Flashforge (AD5X)")}
              </Button>
            </div>
          </div>
        </Panel>
      </div>
      <Show when={current()}>
        {(p) => (
          <div>
            <Panel id="printer.connection" dense title={t("Conexión")} defaultOpen>
              <div class="space-y-2 pb-3">
                <div class="flex items-center gap-2">
                  <PrinterPicture model={p().model} size={72} />
                  <div class="min-w-0 flex-1 space-y-1">
                    <input class={inputClass} title={t("Nombre")} value={p().name} onChange={(e) => update({ name: e.currentTarget.value.trim() || p().name })} onKeyDown={(e) => e.stopPropagation()} />
                    <Select
                      options={modelsFor(p().kind).map((m) => ({ value: m.model, label: m.model }))}
                      value={p().model}
                      onChange={(v) => update({ model: v })}
                    />
                  </div>
                </div>
                <label class="block space-y-0.5">
                  <span class="text-[11px] text-text-muted">{t("Dirección de la impresora")}</span>
                  <input
                    class={inputClass}
                    placeholder={p().kind === "flashforge" ? "192.168.1.60" : t("192.168.1.50 o k1.local")}
                    value={p().address}
                    onChange={(e) => update({ address: e.currentTarget.value.trim() })}
                    onKeyDown={(e) => {
                      e.stopPropagation();
                      if (e.key === "Enter") {
                        update({ address: e.currentTarget.value.trim() });
                        void test();
                      }
                    }}
                  />
                </label>
                <Show when={p().kind === "creality"}>
                  <p class="text-[10px] text-text-dim">{t("Firmware de Creality sin root: se conecta como Creality Print (puertos 80 y 9999). La dirección la muestra la pantalla de la impresora.")}</p>
                </Show>
                <Show
                  when={p().kind === "flashforge"}
                  fallback={
                    <Show when={p().kind === "moonraker"}>
                      <label class="block space-y-0.5">
                        <span class="text-[11px] text-text-muted">{t("Clave de API (si la pide)")}</span>
                        <input type="password" class={inputClass} value={p().apiKey} onChange={(e) => update({ apiKey: e.currentTarget.value.trim() })} onKeyDown={(e) => e.stopPropagation()} />
                      </label>
                    </Show>
                  }
                >
                  <label class="block space-y-0.5">
                    <span class="text-[11px] text-text-muted">{t("Código LAN")}</span>
                    <input class={inputClass} value={p().checkCode} onChange={(e) => update({ checkCode: e.currentTarget.value.trim() })} onKeyDown={(e) => e.stopPropagation()} />
                    <span class="block text-[10px] text-text-dim">{t("Lo muestra la pantalla de la impresora en la configuración de red (modo LAN).")}</span>
                  </label>
                  <Show when={p().serial}>
                    <p class="text-[10px] text-text-dim font-mono">{t("N.º de serie: {sn}", { sn: p().serial })}</p>
                  </Show>
                </Show>
                <div class="flex gap-1.5">
                  <Button onClick={test} disabled={testing() || !p().address.trim()} size="sm" icon={<Icons.ArrowsClockwise size={14} />} class="flex-1">
                    {testing() ? t("Conectando...") : t("Probar conexión")}
                  </Button>
                  <Button onClick={remove} size="sm" variant="ghost" icon={<Icons.Trash size={14} />} title={t("Quitar impresora")} />
                </div>
                <Show when={info()}>
                  {(i) => (
                    <p class={`text-[11px] ${i().ready ? "text-green" : "text-warning"}`}>
                      {t("Conectada: {name}", { name: i().name })}
                      <Show when={i().kind === "moonraker"}> · {t(KLIPPY[i().state] ?? i().state)}</Show>
                      <Show when={i().firmware}>
                        <span class="block text-text-dim">{t("Firmware {v}", { v: i().firmware })}</span>
                      </Show>
                      <Show when={!i().ready && i().message}>
                        <span class="block text-text-dim">{i().message}</span>
                      </Show>
                    </p>
                  )}
                </Show>
                <Show when={notice()}>
                  <p class="text-[11px] text-accent">{notice()}</p>
                </Show>
                <Show when={error()}>
                  <p class="text-[11px] text-red">{error()}</p>
                </Show>
              </div>
            </Panel>
          </div>
        )}
      </Show>
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
                  <Show when={s().slots.length > 0}>
                    <p class="text-[10px] text-text-dim pt-1">{t("Carretes")}</p>
                    <SlotChips slots={s().slots} current={s().current_slot} />
                  </Show>
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
      {/* Ajustes en vivo, mando manual, calibraciones, consola, historial y archivos (Klipper y K1 de fábrica) */}
      <Show when={current()?.kind !== "flashforge" && status() && current()}>
        {(p) => <PrinterControls conn={() => conn(p())} name={p().name} state={status()?.state ?? ""} confirm={props.confirm} />}
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
