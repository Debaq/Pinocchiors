import { Component, For, Show, createEffect, createResource, createSignal, on, onCleanup } from "solid-js";
import { Channel, invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { Button, Checkbox, NumberInput, Panel, Select } from "../ui";
import * as Icons from "../icons";
import { fmtNumber, t } from "../../lib/i18n";
import { tBackend } from "../../lib/i18nBackend.ts";
import {
  decodeLayerMask,
  decodeResinView,
  resinPreview,
  type PrepareSettings,
  type ResinInfo,
  type ResinPrep,
  type ResinResult,
  type ResinSettings,
  type SkipReason,
  type SupportDensity,
  type SupportPoint,
  type SupportSettings,
  type TiltOption,
} from "../../lib/resinPreview";
import { PrinterPicture } from "./PrinterSection";
import type { PlateInputArgs, SlicerTab } from "./SlicerSection";

interface Progress {
  stage: string;
  percent: number;
  message: string;
}

let cached: Promise<ResinInfo | undefined> | undefined;
/** Impresoras, perfiles y soportes de fábrica de resina (se piden una vez) */
export function loadResinInfo(): Promise<ResinInfo | undefined> {
  cached ??= invoke<ResinInfo>("resin_presets").catch(() => undefined);
  return cached;
}

const DENSITIES: [SupportDensity, string][] = [
  ["light", "Livianos"],
  ["medium", "Medios"],
  ["heavy", "Pesados"],
];

const SKIP_REASONS: Record<SkipReason, string> = {
  too_close: "pieza muy cerca abajo",
  inside: "quedó dentro de la pieza",
  on_model_off: "abajo hay pieza",
  vent: "sobre un venteo",
  too_low: "casi sobre la balsa",
};

function fmtTime(seconds: number): string {
  const s = Math.round(seconds);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  return h > 0 ? `${h} h ${m} min` : `${m} min ${s % 60} s`;
}

const errText = (e: unknown) => (typeof e === "string" ? tBackend(e) : String(e));

/**
 * Laminar para resina dentro de Fabricar (pestañas Laminar / Vista previa / Enviar):
 * perfil de resina, exposición, soportes, venteos e inclinación; vista previa capa por
 * capa con la máscara de la pantalla; guardar el archivo para la impresora.
 */
export const ResinSection: Component<{
  /** id de la impresora de resina (ld-002h, halot-sky) */
  printerId: string;
  prep: ResinPrep;
  onPrep: (p: ResinPrep) => void;
  plateInput: () => PlateInputArgs | undefined;
  plateCount?: number;
  disabled?: boolean;
  tab: SlicerTab;
  onTab: (tab: SlicerTab) => void;
  onEditPrinter?: () => void;
}> = (props) => {
  const [info] = createResource(loadResinInfo);
  const [busy, setBusy] = createSignal(false);
  const [progress, setProgress] = createSignal<Progress>();
  const [result, setResult] = createSignal<ResinResult>();
  const [error, setError] = createSignal<string>();
  const [saved, setSaved] = createSignal<string>();
  const [tilts, setTilts] = createSignal<TiltOption[]>();
  const [tiltBusy, setTiltBusy] = createSignal(false);
  /** Puntos editados a mano (sirven mientras no cambie la geometría: placa, giro, altura) */
  const [edited, setEdited] = createSignal<SupportPoint[]>();
  const [editing, setEditing] = createSignal(false);
  /** Lo que define dónde quedó la pieza al laminar; si cambia, los puntos ya no calzan */
  const [slicedKey, setSlicedKey] = createSignal<string>();
  /** Lo que se ve en la vista previa es la pieza de calibración */
  const [calibrating, setCalibrating] = createSignal(false);
  const [calStep, setCalStep] = createSignal(0.5);
  const [calSaved, setCalSaved] = createSignal<{ dir: string; files: string[] }>();
  /** Exposiciones a probar: 5 alrededor de la actual */
  const calExposures = () => {
    const e = settings()?.exposure ?? 3;
    return [-2, -1, 0, 1, 2].map((k) => Math.round((e + k * calStep()) * 100) / 100).filter((x) => x > 0.2);
  };

  const set = (patch: Partial<ResinPrep>) => props.onPrep({ ...props.prep, ...patch });
  const printer = () => info()?.printers.find((p) => p.id === props.printerId);
  const profiles = () => info()?.profiles.filter((p) => p.printer === props.printerId) ?? [];
  const profile = () => profiles().find((p) => p.id === props.prep.profile) ?? profiles()[0];
  const settings = (): ResinSettings | undefined => {
    const p = profile();
    return p ? { ...p.settings, ...props.prep.settings } : undefined;
  };
  const setSetting = <K extends keyof ResinSettings>(key: K, v: ResinSettings[K]) => {
    const next = { ...props.prep.settings, [key]: v };
    if (profile()?.settings[key] === v) delete next[key];
    set({ settings: next });
  };
  const changed = () => Object.keys(props.prep.settings ?? {}).length;
  const supportBase = () => info()?.supports.find(([d]) => d === props.prep.density)?.[1];
  const supports = (): SupportSettings | undefined => {
    const b = supportBase();
    return b ? { ...b, ...props.prep.supportOverrides } : undefined;
  };
  const setSupport = <K extends keyof SupportSettings>(key: K, v: SupportSettings[K]) => {
    const next = { ...props.prep.supportOverrides, [key]: v };
    if (supportBase()?.[key] === v) delete next[key];
    set({ supportOverrides: next });
  };
  const geometryKey = () =>
    JSON.stringify({ printer: props.printerId, input: input(), plate: plate(), tilt: props.prep.tilt ?? null, supports: props.prep.supports, lift: supports()?.lift, raft: supports()?.raft_thickness });
  /** Los puntos editados todavía calzan con la pieza */
  const pointsValid = () => props.prep.supports && slicedKey() !== undefined && slicedKey() === geometryKey();
  const prepare = (): PrepareSettings => ({ supports: props.prep.supports ? (supports() ?? null) : null, vents: props.prep.vents, vent_diameter: props.prep.ventDiameter });

  const [warnings] = createResource(
    () => (settings() ? { printer: props.printerId, settings: settings()! } : undefined),
    (args) => invoke<string[]>("resin_validate", args).catch(() => [] as string[]),
  );

  // Otra impresora: lo laminado ya no vale
  createEffect(
    on(
      () => props.printerId,
      () => {
        setResult(undefined);
        setTilts(undefined);
        resinPreview.setVisible(false);
        if (props.tab === "preview") props.onTab("settings");
      },
      { defer: true },
    ),
  );
  onCleanup(() => {
    resinPreview.setVisible(false);
    resinPreview.setEditor(undefined);
  });

  // Editar puntos: los clics del visor llegan acá
  const applyEdit = (e: { add: [number, number, number] } | { remove: number }) => {
    const current = resinPreview.points();
    if ("add" in e) {
      const next = [...current, { pos: e.add, kind: "manual" as const }];
      resinPreview.setPoints(next);
      setEdited(next);
    } else {
      const i = e.remove;
      resinPreview.setPoints(current.filter((_, k) => k !== i));
      resinPreview.setSkipped(resinPreview.skipped().filter((k) => k !== i).map((k) => (k > i ? k - 1 : k)));
      setEdited(resinPreview.points());
    }
  };
  createEffect(() => {
    const on = editing() && props.tab === "preview" && !!result() && pointsValid();
    // Una función se guarda en la señal envuelta en otra (si no, Solid la toma como actualizador)
    if (on) resinPreview.setEditor(() => applyEdit);
    else resinPreview.setEditor(undefined);
    if (on) resinPreview.setVisible(true);
  });
  // La pieza cambió de lugar: los puntos editados ya no sirven
  createEffect(
    on(geometryKey, () => {
      if (edited() && !pointsValid()) {
        setEdited(undefined);
        setEditing(false);
      }
    }, { defer: true }),
  );
  const addIslandPoints = () => {
    const r = result();
    if (!r) return;
    const next = [...resinPreview.points(), ...r.islands.map((i) => ({ pos: i.pos, kind: "manual" as const }))];
    resinPreview.setPoints(next);
    setEdited(next);
  };

  const plate = () => Math.min(props.prep.plate ?? 0, Math.max(0, (props.plateCount ?? 1) - 1));
  /** La entrada de placas con la cama de esta impresora */
  const input = () => {
    const base = props.plateInput();
    const p = printer();
    if (!base || !p) return undefined;
    return { ...base, printer: { ...(base.printer as Record<string, unknown>), build_volume: p.bed } };
  };

  /** Lamina; con puntos editados válidos, los soportes salen de esos puntos */
  const slice = async (auto = false) => {
    const i = input();
    const s = settings();
    if (!i || !s) return;
    const points = !auto && pointsValid() ? (edited() ?? null) : null;
    const key = geometryKey();
    setBusy(true);
    setError(undefined);
    setSaved(undefined);
    setProgress({ stage: "start", percent: 0, message: t("Empezando...") });
    const channel = new Channel<Progress>();
    channel.onmessage = (m) => setProgress(m);
    try {
      const tilt = props.prep.tilt && props.prep.tilt.degrees !== 0 ? [props.prep.tilt.axis, props.prep.tilt.degrees] : null;
      const r = await invoke<ResinResult>("resin_slice", {
        input: i,
        plate: plate(),
        printer: props.printerId,
        settings: s,
        prepare: prepare(),
        points,
        tilt,
        onProgress: channel,
      });
      setResult(r);
      setCalibrating(false);
      setSlicedKey(key);
      if (!points) setEdited(undefined);
      resinPreview.setPoints(r.points);
      resinPreview.setSkipped(r.skipped.map(([k]) => k));
      const view = decodeResinView(await invoke<ArrayBuffer>("resin_view"));
      resinPreview.setData({ ...view, bed: r.bed, layerZ: r.layer_z, layerHeight: s.layer_height, vents: r.vents, islands: r.islands });
      resinPreview.setLayer(Math.max(0, r.layers - 1));
      resinPreview.setVisible(true);
      props.onTab("preview");
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
      setProgress(undefined);
    }
  };

  /** Lamina la pieza de calibración y la muestra en la vista previa */
  const sliceCalibration = async (): Promise<boolean> => {
    const s = settings();
    if (!s) return false;
    setBusy(true);
    setError(undefined);
    try {
      const r = await invoke<ResinResult>("resin_calibration", { printer: props.printerId, settings: s });
      setResult(r);
      setCalibrating(true);
      setSlicedKey(undefined);
      setEdited(undefined);
      resinPreview.setPoints([]);
      resinPreview.setSkipped([]);
      const view = decodeResinView(await invoke<ArrayBuffer>("resin_view"));
      resinPreview.setData({ ...view, bed: r.bed, layerZ: r.layer_z, layerHeight: s.layer_height, vents: [], islands: r.islands });
      resinPreview.setLayer(Math.max(0, r.layers - 1));
      resinPreview.setVisible(true);
      return true;
    } catch (e) {
      setError(errText(e));
      return false;
    } finally {
      setBusy(false);
    }
  };

  const saveCalibration = async () => {
    const dir = await open({ directory: true, title: t("Carpeta para las pruebas de exposición") });
    if (typeof dir !== "string") return;
    if (!(await sliceCalibration())) return;
    try {
      const files = await invoke<string[]>("save_resin_calibration", { dir, exposures: calExposures() });
      setCalSaved({ dir, files });
    } catch (e) {
      setError(errText(e));
    }
  };

  const compareTilts = async () => {
    const i = input();
    const s = settings();
    if (!i || !s) return;
    setTiltBusy(true);
    setError(undefined);
    try {
      setTilts(await invoke<TiltOption[]>("resin_tilts", { input: i, plate: plate(), printer: props.printerId, layerHeight: s.layer_height }));
    } catch (e) {
      setError(errText(e));
    } finally {
      setTiltBusy(false);
    }
  };

  const saveFile = async () => {
    const r = result();
    if (!r) return;
    const path = await save({ title: t("Guardar para la impresora"), filters: [{ name: r.format.toUpperCase(), extensions: [r.format] }] });
    if (!path) return;
    try {
      await invoke<number>("save_resin", { path });
      setSaved(path);
    } catch (e) {
      setError(errText(e));
    }
  };

  const num = (label: string, value: number, onChange: (v: number) => void, min: number, max: number, step: number, suffix?: string) => (
    <NumberInput label={t(label)} value={value} onChange={onChange} min={min} max={max} step={step} suffix={suffix} />
  );

  const settingsPanels = () => (
    <Show when={settings()}>
      {(s) => (
        <div>
          <div>
            <Panel id="resin.printer" dense title={t("Impresora y resina")} icon={<Icons.Printer size={14} />} defaultOpen>
              <div class="space-y-2 pb-3">
                <div class="flex items-center gap-2">
                  <PrinterPicture model={printer()?.name} size={48} />
                  <div class="min-w-0 flex-1">
                    <p class="text-xs font-semibold text-text truncate">{printer()?.name}</p>
                    <Show when={printer()}>
                      {(p) => (
                        <p class="text-[10px] text-text-dim">
                          {p().resolution.join(" × ")} px · {fmtNumber(p().pixel_um[0], { maximumFractionDigits: 0 })} µm · .{p().format}
                        </p>
                      )}
                    </Show>
                    <Show when={props.onEditPrinter}>
                      <button class="text-[10px] text-accent hover:underline" onClick={() => props.onEditPrinter?.()}>
                        {t("Cambiar impresora")}
                      </button>
                    </Show>
                  </div>
                </div>
                <Select label={t("Resina")} options={profiles().map((p) => ({ value: p.id, label: t(p.name) }))} value={profile()?.id} onChange={(v) => set({ profile: v, settings: {} })} />
                <p class="text-[10px] text-text-dim">{t("Los perfiles son un punto de partida: la exposición depende de la resina y de la temperatura. Calibra con una pieza de prueba.")}</p>
                <Show when={changed() > 0}>
                  <button class="text-[11px] text-text-dim hover:underline" onClick={() => set({ settings: {} })}>
                    {t("Volver al perfil ({n} cambios)", { n: changed() })}
                  </button>
                </Show>
              </div>
            </Panel>
          </div>
          <div>
            <Panel id="resin.exposure" dense title={t("Exposición")} icon={<Icons.Lightning size={14} />} defaultOpen>
              <div class="space-y-2 pb-3">
                {num("Altura de capa", s().layer_height, (v) => setSetting("layer_height", v), 0.01, 0.2, 0.01, "mm")}
                {num("Exposición", s().exposure, (v) => setSetting("exposure", v), 0.5, 30, 0.1, "s")}
                {num("Exposición de base", s().bottom_exposure, (v) => setSetting("bottom_exposure", v), 5, 120, 1, "s")}
                {num("Capas de base", s().bottom_layers, (v) => setSetting("bottom_layers", Math.round(v)), 0, 30, 1)}
                {num("Espera con luz apagada", s().light_off, (v) => setSetting("light_off", v), 0, 30, 0.5, "s")}
                <Select
                  label={t("Suavizar bordes")}
                  options={[1, 2, 4, 8].map((n) => ({ value: String(n), label: n === 1 ? t("No") : t("Nivel {n}", { n }) }))}
                  value={String(s().antialias ?? 1)}
                  onChange={(v) => setSetting("antialias", Number(v))}
                />
                <p class="text-[10px] text-text-dim">{t("Pinta los bordes en grises: curvas y paredes inclinadas sin escalones. Más nivel, archivo más grande.")}</p>
                <For each={warnings() ?? []}>{(w) => <p class="text-[11px] text-warning">{tBackend(w)}</p>}</For>
              </div>
            </Panel>
          </div>
          <div>
            <Panel id="resin.calibration" dense title={t("Calibrar la exposición")} defaultOpen={false}>
              <div class="space-y-2 pb-3">
                <p class="text-[10px] text-text-dim">
                  {t("Una pieza de prueba de {w} × {d} mm, directo sobre la placa, que se guarda una vez por cada exposición. Hileras de adelante hacia atrás, de lo más fino a lo más grueso:", { w: 46, d: 24 })}
                </p>
                <For each={info()?.calibration ?? []}>
                  {(row) => (
                    <p class="text-[10px] text-text-muted">
                      <span class="font-semibold">{t(row.name)}</span>: {row.sizes.map((x) => fmtNumber(x)).join(" · ")} mm
                    </p>
                  )}
                </For>
                {num("Paso entre pruebas", calStep(), (v) => setCalStep(Math.max(0.05, v)), 0.05, 5, 0.05, "s")}
                <p class="text-[11px] text-text-muted">{t("Exposiciones: {list} s", { list: calExposures().map((x) => fmtNumber(x)).join(" · ") })}</p>
                <div class="grid grid-cols-2 gap-1.5">
                  <Button size="sm" variant="ghost" onClick={() => void sliceCalibration().then((ok) => ok && props.onTab("preview"))} disabled={busy()} icon={<Icons.Eye size={14} />}>
                    {t("Ver la pieza")}
                  </Button>
                  <Button size="sm" onClick={() => void saveCalibration()} disabled={busy() || printer()?.can_write === false} icon={<Icons.Download size={14} />}>
                    {t("Guardar las pruebas")}
                  </Button>
                </div>
                <Show when={calSaved()}>
                  {(c) => <p class="text-[10px] text-green">{t("{n} archivos en {dir}", { n: c().files.length, dir: c().dir })}</p>}
                </Show>
                <p class="text-[10px] text-text-dim">
                  {t("Imprime cada una y elige la exposición más baja con la que salen de pie los pines y las paredes finas sin que se cierren los agujeros ni las ranuras.")}
                </p>
                <div class="flex flex-wrap gap-1">
                  <For each={calExposures()}>
                    {(e) => (
                      <button
                        class={`rounded border px-1.5 py-0.5 text-[10px] ${e === s().exposure ? "border-accent text-accent" : "border-border text-text-muted hover:border-accent/60"}`}
                        onClick={() => setSetting("exposure", e)}
                        title={t("Usar esta exposición")}
                      >
                        {t("Usar {e} s", { e: fmtNumber(e) })}
                      </button>
                    )}
                  </For>
                </div>
              </div>
            </Panel>
          </div>
          <div>
            <Panel id="resin.motion" dense title={t("Subida y bajada")} defaultOpen={false}>
              <div class="space-y-2 pb-3">
                {num("Subida", s().lift, (v) => setSetting("lift", v), 1, 20, 0.5, "mm")}
                {num("Velocidad de subida", s().lift_speed, (v) => setSetting("lift_speed", v), 10, 400, 5, "mm/min")}
                {num("Subida de base", s().bottom_lift, (v) => setSetting("bottom_lift", v), 1, 20, 0.5, "mm")}
                {num("Velocidad de subida de base", s().bottom_lift_speed, (v) => setSetting("bottom_lift_speed", v), 10, 400, 5, "mm/min")}
                {num("Velocidad de bajada", s().retract_speed, (v) => setSetting("retract_speed", v), 10, 400, 5, "mm/min")}
              </div>
            </Panel>
          </div>
          <div>
            <Panel id="resin.supports" dense title={t("Soportes")} defaultOpen>
              <div class="space-y-2 pb-3">
                <Checkbox small label={t("Soportes y balsa")} checked={props.prep.supports} onChange={(v) => set({ supports: v })} hint={t("Sin soportes la pieza va pegada a la placa: solo para piezas con la base plana.")} />
                <Show when={props.prep.supports && supports()}>
                  {(sp) => (
                    <>
                      <Select label={t("Densidad")} options={DENSITIES.map(([value, label]) => ({ value, label: t(label) }))} value={props.prep.density} onChange={(v) => set({ density: v as SupportDensity, supportOverrides: {} })} />
                      {num("Distancia entre puntos", sp().spacing, (v) => setSupport("spacing", v), 0.8, 10, 0.1, "mm")}
                      {num("Ángulo límite", sp().overhang_angle, (v) => setSupport("overhang_angle", v), 10, 80, 5, "°")}
                      {num("Punta", sp().tip_diameter, (v) => setSupport("tip_diameter", v), 0.15, 1.5, 0.05, "mm")}
                      {num("Columna", sp().pillar_diameter, (v) => setSupport("pillar_diameter", v), 0.4, 4, 0.1, "mm")}
                      {num("Altura sobre la balsa", sp().lift, (v) => setSupport("lift", v), 1, 30, 0.5, "mm")}
                      <Checkbox small label={t("Apoyar soportes sobre la pieza")} checked={sp().on_model} onChange={(v) => setSupport("on_model", v)} />
                      <Show when={Object.keys(props.prep.supportOverrides ?? {}).length > 0}>
                        <button class="text-[11px] text-text-dim hover:underline" onClick={() => set({ supportOverrides: {} })}>
                          {t("Volver a los de fábrica")}
                        </button>
                      </Show>
                    </>
                  )}
                </Show>
              </div>
            </Panel>
          </div>
          <div>
            <Panel id="resin.suction" dense title={t("Succión y huecos")} defaultOpen>
              <div class="space-y-2 pb-3">
                <Checkbox
                  small
                  label={t("Ventear huecos encerrados")}
                  checked={props.prep.vents}
                  onChange={(v) => set({ vents: v })}
                  hint={t("Un hueco encerrado hace ventosa contra el film al despegar y deja resina atrapada. Se le abre un agujero desde el piso hacia abajo.")}
                />
                <Show when={props.prep.vents}>{num("Diámetro del venteo", props.prep.ventDiameter, (v) => set({ ventDiameter: v }), 1, 6, 0.5, "mm")}</Show>
                <p class="text-[10px] text-text-dim">{t("Para vaciar la pieza (menos resina) usa la pestaña Relleno de Fabricar.")}</p>
                <div class="pt-1 space-y-1.5">
                  <p class="text-xs text-text-muted">
                    {props.prep.tilt && props.prep.tilt.degrees !== 0
                      ? t("Inclinada {deg}° en {axis}", { deg: props.prep.tilt.degrees, axis: props.prep.tilt.axis.toUpperCase() })
                      : t("Sin inclinar")}
                    <Show when={props.prep.tilt && props.prep.tilt.degrees !== 0}>
                      {" · "}
                      <button class="text-accent hover:underline" onClick={() => set({ tilt: undefined })}>
                        {t("Quitar")}
                      </button>
                    </Show>
                  </p>
                  <Button size="sm" variant="ghost" fullWidth onClick={compareTilts} disabled={tiltBusy() || busy() || props.disabled || !input()} icon={<Icons.ArrowsClockwise size={14} />}>
                    {tiltBusy() ? t("Comparando...") : t("Comparar inclinaciones")}
                  </Button>
                  <Show when={tilts()}>
                    {(list) => (
                      <div class="space-y-0.5">
                        <p class="text-[10px] text-text-dim">{t("Menos área por capa = menos fuerza al despegar. La primera es la recomendada.")}</p>
                        <For each={list().slice(0, 5)}>
                          {(o, i) => (
                            <div class="flex items-center gap-1.5 text-[11px]">
                              <span class={i() === 0 ? "text-green" : "text-text-muted"}>{o.degrees === 0 ? t("Sin girar") : `${o.degrees}° ${o.axis.toUpperCase()}`}</span>
                              <span class="flex-1 text-right font-mono text-text-dim">
                                {fmtNumber(o.max_area_mm2, { maximumFractionDigits: 0 })} mm² · {fmtNumber(o.height_mm, { maximumFractionDigits: 0 })} mm
                                <Show when={!o.fits}> · {t("no cabe")}</Show>
                              </span>
                              <button
                                class="text-accent hover:underline disabled:opacity-40"
                                disabled={!o.fits}
                                onClick={() => set({ tilt: o.degrees === 0 ? undefined : { axis: o.axis, degrees: o.degrees } })}
                              >
                                {t("Usar")}
                              </button>
                            </div>
                          )}
                        </For>
                      </div>
                    )}
                  </Show>
                </div>
              </div>
            </Panel>
          </div>
          <div>
            <Panel id="resin.cost" dense title={t("Resina y costo")} defaultOpen={false}>
              <div class="space-y-2 pb-3">
                {num("Densidad", s().density, (v) => setSetting("density", v), 0.8, 2, 0.01, "g/ml")}
                {num("Precio por ml", s().cost_per_ml, (v) => setSetting("cost_per_ml", v), 0, 10000, 1)}
              </div>
            </Panel>
          </div>
        </div>
      )}
    </Show>
  );

  const previewPanels = () => (
    <Show when={result()} fallback={<p class="text-[11px] text-text-dim">{t("Lamina la placa para ver capa por capa lo que muestra la pantalla.")}</p>}>
      {(r) => (
        <div>
          <div>
            <Panel id="resin.summary" dense title={t("Resumen")} defaultOpen>
              <div class="space-y-1 pb-3">
                <Show when={calibrating()}>
                  <p class="text-[11px] text-accent">{t("Pieza de calibración (no es tu modelo)")}</p>
                </Show>
                <Row label={t("Tiempo estimado")} value={fmtTime(r().time_s)} />
                <Row label={t("Resina")} value={`${fmtNumber(r().volume_ml, { maximumFractionDigits: 1 })} ml · ${fmtNumber(r().grams, { maximumFractionDigits: 1 })} g`} />
                <Show when={r().cost > 0}>
                  <Row label={t("Costo")} value={fmtNumber(r().cost, { maximumFractionDigits: 0 })} />
                </Show>
                <Row label={t("Capas")} value={`${r().layers} · ${fmtNumber(r().height_mm, { maximumFractionDigits: 1 })} mm`} />
                <Row label={t("Capa con más área")} value={`${fmtNumber(r().max_area_mm2, { maximumFractionDigits: 0 })} mm² (${r().max_area_layer + 1})`} />
                <Show when={r().plates > 1}>
                  <Row label={t("Placa")} value={`${r().plate + 1} / ${r().plates}`} />
                </Show>
                <Show when={r().unplaced.length > 0}>
                  <p class="text-[11px] text-red">{t("No caben en la cama: {names}", { names: r().unplaced.join(", ") })}</p>
                </Show>
              </div>
            </Panel>
          </div>
          <div>
            <Panel id="resin.check" dense title={t("Revisión")} defaultOpen>
              <div class="space-y-1 pb-3">
                <Row label={t("Soportes")} value={String(r().points.length - r().skipped.length)} />
                <Row label={t("Venteos")} value={String(r().vents.length)} />
                <Check ok={r().islands.length === 0} text={r().islands.length === 0 ? t("Sin islas sueltas") : t("{n} islas sin soporte (en rojo en el visor)", { n: r().islands.length })} />
                <Check
                  ok={r().cavities.length === 0}
                  text={r().cavities.length === 0 ? t("Sin ventosas ni resina atrapada") : t("{n} huecos con succión o resina atrapada", { n: r().cavities.length })}
                />
                <Show when={r().skipped.length > 0}>
                  <p class="text-[10px] text-text-dim">
                    {t("Puntos sin soporte: {list}", {
                      list: Object.entries(
                        r().skipped.reduce<Record<string, number>>((acc, [, why]) => ({ ...acc, [why]: (acc[why] ?? 0) + 1 }), {}),
                      )
                        .map(([why, n]) => `${n} ${t(SKIP_REASONS[why as SkipReason])}`)
                        .join(", "),
                    })}
                  </p>
                </Show>
                <For each={r().notes}>{(n) => <p class="text-[11px] text-text-muted">{tBackend(n)}</p>}</For>
                <For each={r().warnings}>{(w) => <p class="text-[11px] text-warning">{tBackend(w)}</p>}</For>
              </div>
            </Panel>
          </div>
          <Show when={props.prep.supports && !calibrating()}>
            <div>
              <Panel id="resin.points" dense title={t("Puntos de soporte")} defaultOpen>
                <div class="space-y-1.5 pb-3">
                  <Show when={pointsValid()} fallback={<p class="text-[11px] text-text-dim">{t("Cambiaste la placa, el giro o la altura: lamina de nuevo para editar los puntos.")}</p>}>
                    <Checkbox small label={t("Editar puntos en el visor")} checked={editing()} onChange={setEditing} />
                    <Show when={editing()}>
                      <p class="text-[10px] text-text-dim">{t("Clic en la pieza: agrega un punto ahí. Clic en un punto: lo quita. Azul = automático, verde = a mano, rojo = no se pudo poner.")}</p>
                    </Show>
                    <p class="text-[11px] text-text-muted">
                      {t("{n} puntos ({m} a mano)", { n: resinPreview.points().length, m: resinPreview.points().filter((p) => p.kind === "manual").length })}
                    </p>
                    <Show when={r().islands.length > 0}>
                      <Button size="sm" variant="ghost" fullWidth onClick={addIslandPoints} icon={<Icons.Plus size={14} />}>
                        {t("Un punto en cada isla")}
                      </Button>
                    </Show>
                    <Show when={edited()}>
                      <Button size="sm" variant="primary" fullWidth onClick={() => void slice()} disabled={busy()} icon={<Icons.ArrowsClockwise size={14} />}>
                        {t("Rehacer soportes con estos puntos")}
                      </Button>
                      <button class="text-[11px] text-text-dim hover:underline" onClick={() => void slice(true)} disabled={busy()}>
                        {t("Volver a los automáticos")}
                      </button>
                    </Show>
                  </Show>
                </div>
              </Panel>
            </div>
          </Show>
          <div>
            <Panel id="resin.layers" dense title={t("Capas")} defaultOpen>
              <div class="pb-3">
                <LayerControls result={r()} />
              </div>
            </Panel>
          </div>
        </div>
      )}
    </Show>
  );

  const sendPanel = () => (
    <Show when={result()} fallback={<p class="text-[11px] text-text-dim">{t("Primero lamina la placa.")}</p>}>
      {(r) => (
        <div class="space-y-2 max-w-[420px]">
          <Show
            when={r().file_bytes != null}
            fallback={<p class="text-[11px] text-warning">{t("Todavía no se puede guardar el archivo de esta impresora: falta confirmar la unidad de exposición del formato .cxdlp.")}</p>}
          >
            <Button onClick={saveFile} fullWidth size="sm" variant="primary" icon={<Icons.Download size={14} />}>
              {t("Guardar .{ext} ({size} MB)", { ext: r().format, size: fmtNumber((r().file_bytes ?? 0) / 1e6, { maximumFractionDigits: 1 }) })}
            </Button>
            <Show when={saved()}>
              <p class="text-[10px] text-green truncate">{t("Guardado en {path}", { path: saved()! })}</p>
            </Show>
            <p class="text-[11px] text-text-muted">{t("Copia el archivo a un pendrive, ponlo en la impresora y elígelo en su pantalla.")}</p>
          </Show>
          <p class="text-[10px] text-text-dim">{t("Con resina: guantes, lentes y ventilación. Lava y cura las piezas después de imprimir.")}</p>
        </div>
      )}
    </Show>
  );

  return (
    <div class="flex flex-col h-full min-h-0">
      <div class="flex-1 min-h-0 overflow-y-auto px-3 py-2">
        <Show when={info()} fallback={<p class="text-[11px] text-text-dim">{info.loading ? t("Cargando...") : t("No se pudo cargar el laminador de resina.")}</p>}>
          <Show when={props.tab !== "printer"} fallback={sendPanel()}>
            <div class="[column-width:280px] [column-gap:1rem] [&>div>div]:break-inside-avoid [&>div>div]:mb-3">{props.tab === "preview" ? previewPanels() : settingsPanels()}</div>
          </Show>
        </Show>
      </div>
      <Show when={info() && props.tab !== "printer"}>
        <footer class="shrink-0 border-t border-border px-3 py-2 space-y-1.5">
          <Show when={(props.plateCount ?? 1) > 1}>
            <Select
              label={t("Placa")}
              options={Array.from({ length: props.plateCount ?? 1 }, (_, i) => ({ value: String(i), label: t("Placa {n}", { n: i + 1 }) }))}
              value={String(plate())}
              onChange={(v) => set({ plate: Number(v) })}
            />
          </Show>
          <Show
            when={busy()}
            fallback={
              <div class="flex gap-1.5">
                <Button onClick={() => void slice()} disabled={props.disabled || !input() || !settings()} fullWidth size="sm" variant="primary" icon={<Icons.Stack size={14} />}>
                  {result() ? t("Laminar de nuevo") : t("Laminar")}
                </Button>
                <Show when={result()?.file_bytes != null && !calibrating()}>
                  <Button onClick={saveFile} fullWidth size="sm" icon={<Icons.Download size={14} />}>
                    {t("Guardar .{ext}", { ext: result()!.format })}
                  </Button>
                </Show>
              </div>
            }
          >
            <div class="space-y-1">
              <div class="h-1.5 w-full rounded bg-surface/50 overflow-hidden">
                <div class="h-full bg-accent transition-all" style={{ width: `${progress()?.percent ?? 0}%` }} />
              </div>
              <p class="text-[10px] text-text-dim truncate">{tBackend(progress()?.message ?? "")}</p>
            </div>
          </Show>
          <Show when={error()}>
            <p class="text-[11px] text-red">{error()}</p>
          </Show>
          <p class="text-[10px] text-text-dim">{t("Laminador de resina de Pinocchio")}</p>
        </footer>
      </Show>
    </div>
  );
};

const Row: Component<{ label: string; value: string }> = (props) => (
  <div class="flex justify-between text-xs gap-2">
    <span class="text-text-muted">{props.label}</span>
    <span class="text-text font-mono">{props.value}</span>
  </div>
);

const Check: Component<{ ok: boolean; text: string }> = (props) => (
  <p class={`flex gap-1.5 text-[11px] ${props.ok ? "text-text-muted" : "text-warning"}`}>
    <Show when={props.ok} fallback={<Icons.Warning size={12} class="mt-0.5 shrink-0" />}>
      <Icons.Check size={12} class="mt-0.5 shrink-0 text-green" />
    </Show>
    <span>{props.text}</span>
  </p>
);

/** Capa elegida: la máscara de la pantalla y el corte en el visor */
const LayerControls: Component<{ result: ResinResult }> = (props) => {
  let canvas: HTMLCanvasElement | undefined;
  const layers = () => props.result.layers;
  const layer = () => Math.min(resinPreview.layer(), Math.max(0, layers() - 1));
  const setLayer = (v: number) => resinPreview.setLayer(Math.max(0, Math.min(layers() - 1, Math.round(v))));

  // La máscara se pide al cambiar de capa (la última pedida gana)
  let ticket = 0;
  createEffect(() => {
    const index = layer();
    const mine = ++ticket;
    void invoke<ArrayBuffer>("resin_layer", { index, maxSize: 360 })
      .then((buffer) => {
        if (mine !== ticket || !canvas) return;
        const m = decodeLayerMask(buffer);
        canvas.width = m.width;
        canvas.height = m.height;
        const ctx = canvas.getContext("2d");
        if (!ctx) return;
        const img = ctx.createImageData(m.width, m.height);
        // Fila 0 = y más chica: abajo en el dibujo
        for (let y = 0; y < m.height; y++) {
          for (let x = 0; x < m.width; x++) {
            const g = m.data[y * m.width + x];
            const o = ((m.height - 1 - y) * m.width + x) * 4;
            img.data[o] = g;
            img.data[o + 1] = g;
            img.data[o + 2] = g;
            img.data[o + 3] = 255;
          }
        }
        ctx.putImageData(img, 0, 0);
      })
      .catch(() => {});
  });

  return (
    <div class="space-y-1.5 pt-1">
      <Button onClick={() => resinPreview.setVisible(!resinPreview.visible())} fullWidth size="sm" variant={resinPreview.visible() ? "primary" : "ghost"} icon={<Icons.Cube size={14} />}>
        {resinPreview.visible() ? t("Salir de la vista previa") : t("Ver en el visor")}
      </Button>
      <div class="flex items-center gap-1">
        <button class="px-1 text-text-dim hover:text-text" onClick={() => setLayer(layer() - 1)} aria-label={t("Capa anterior")}>
          ‹
        </button>
        <input type="range" class="flex-1 accent-[var(--accent)]" min={0} max={layers() - 1} step={1} value={layer()} onInput={(e) => setLayer(Number(e.currentTarget.value))} />
        <button class="px-1 text-text-dim hover:text-text" onClick={() => setLayer(layer() + 1)} aria-label={t("Capa siguiente")}>
          ›
        </button>
      </div>
      <p class="text-[10px] text-text-dim">
        {t("Capa {n} de {total} · {z} mm", { n: layer() + 1, total: layers(), z: fmtNumber(props.result.layer_z[layer()] ?? 0, { maximumFractionDigits: 2 }) })}
      </p>
      <canvas ref={canvas} class="w-full rounded border border-border bg-black [image-rendering:pixelated]" />
      <p class="text-[10px] text-text-dim">{t("Lo que muestra la pantalla en esta capa: blanco = luz.")}</p>
      <Checkbox small label={t("Mostrar soportes")} checked={resinPreview.showSupports()} onChange={(v) => resinPreview.setShowSupports(v)} />
    </div>
  );
};
