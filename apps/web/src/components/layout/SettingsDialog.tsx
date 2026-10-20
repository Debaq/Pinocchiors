import { Component, For, JSX, Show, createSignal, onCleanup, onMount } from "solid-js";
import { Checkbox, Select, Slider } from "../ui";
import * as Icons from "../icons";
import { THEME_OPTIONS, type ThemeSetting } from "../../lib/theme";
import { LOCALES, localeSetting, setLocale, t, tn, type LocaleSetting } from "../../lib/i18n";
import {
  UI_SCALES,
  defaultExportFormat,
  meshBuild,
  adaptiveQuality,
  animationLod,
  clearRecent,
  peekBeforeImport,
  previewFirst,
  recentFiles,
  setPeekBeforeImportModel,
  resetLayout,
  setAdaptiveQuality,
  setAnimationLodEnabled,
  setDefaultExportFormat,
  setMeshBuild,
  setPreviewFirstModel,
  setShowFps,
  setUiScale,
  showFps,
  uiScale,
  type MeshBuildMode,
} from "../../lib/settings";
import { MOUSE_HINTS, type ShortcutHint } from "./ContextPanel";
import {
  AREAS,
  conflictFor,
  hasShortcutChanges,
  rebind,
  resetShortcuts,
  shortcutRows,
  type Binding,
} from "../../lib/shortcuts";

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

const MESH_BUILD_OPTIONS = () => [
  { value: "background", label: t("En segundo plano (recomendado)") },
  { value: "lazy", label: t("Al usar una herramienta") },
  { value: "now", label: t("Al importar (espera a que termine)") },
];

const EXPORT_FORMAT_OPTIONS = [
  { value: "glb", label: "GLB" },
  { value: "gltf", label: "glTF" },
  { value: "usdz", label: "USDZ" },
  { value: "usda", label: "USDA" },
  { value: "obj", label: "OBJ" },
  { value: "3mf", label: "3MF" },
  { value: "stl", label: "STL" },
  { value: "ply", label: "PLY" },
];
const SCALE_OPTIONS = UI_SCALES.map((s) => ({ value: String(s), label: `${Math.round(s * 100)} %` }));

type TabId = "general" | "viewer" | "shortcuts" | "files" | "about";
const TABS: { id: TabId; label: () => string; icon: Component<{ size?: number }> }[] = [
  { id: "general", label: () => t("General"), icon: Icons.Gear },
  { id: "viewer", label: () => t("Visor y navegación"), icon: Icons.Eye },
  { id: "shortcuts", label: () => t("Atajos de teclado"), icon: Icons.Lightning },
  { id: "files", label: () => t("Archivos y exportación"), icon: Icons.Folder },
  { id: "about", label: () => t("Acerca de"), icon: Icons.Info },
];

const Section = (props: { title?: string; children: JSX.Element }) => (
  <section class="space-y-3 pb-4 mb-4 border-b border-border last:border-0 last:mb-0 last:pb-0">
    <Show when={props.title}>
      <h3 class="text-[11px] uppercase tracking-wide text-text-muted">{props.title}</h3>
    </Show>
    {props.children}
  </section>
);

const Hint = (props: { children: JSX.Element }) => <p class="text-xs text-text-muted leading-relaxed">{props.children}</p>;

export interface SettingsDialogProps {
  autosave: AutosaveSettings;
  onAutosaveChange: (settings: AutosaveSettings) => void;
  grid: GridSettings;
  onGridChange: (settings: GridSettings) => void;
  theme: ThemeSetting;
  onThemeChange: (theme: ThemeSetting) => void;
  /** Navegación del visor pensada para trackpad (dos dedos = orbitar) */
  trackpad: boolean;
  onTrackpadChange: (on: boolean) => void;
  /** Metros por unidad que declara el archivo abierto */
  fileMetersPerUnit?: number;
  /** Archivo del proyecto abierto (sin él, el automático va al de recuperación) */
  projectPath?: string;
  onClose: () => void;
}

/** Configuración de la app (se recuerda entre sesiones) */
export const SettingsDialog: Component<SettingsDialogProps> = (props) => {
  const [tab, setTab] = createSignal<TabId>("general");
  const [confirmReset, setConfirmReset] = createSignal(false);
  /** Atajo que espera su nueva combinación, y el aviso si chocó con otro */
  const [query, setQuery] = createSignal("");
  /** Sin tildes ni mayúsculas, para que "extrusion" encuentre "Extrusión" */
  const plain = (text: string) => text.normalize("NFD").replace(/[\u0300-\u036f]/g, "").toLowerCase();
  const rowsOf = (area: string) => {
    const q = plain(query().trim());
    const areaName = plain(AREAS.find((a) => a.id === area)?.label() ?? "");
    return shortcutRows().filter(
      (r) => r.area === area && (!q || plain(t(r.description)).includes(q) || plain(r.keys).includes(q) || areaName.includes(q)),
    );
  };
  const anyRows = () => AREAS.some((a) => rowsOf(a.id).length > 0);
  const [capturing, setCapturing] = createSignal<string>();
  const [clash, setClash] = createSignal<{ id: string; with: string }>();

  onMount(() => {
    const onKey = (e: KeyboardEvent) => {
      const id = capturing();
      if (id !== undefined) {
        // Esperando la nueva combinación: todo lo que se pulsa es para ella
        e.preventDefault();
        e.stopPropagation();
        if (["Control", "Shift", "Alt", "Meta", "Dead"].includes(e.key)) return;
        if (e.key === "Escape") {
          setCapturing(undefined);
          return;
        }
        // Supr solo (sin modificadores) deja el atajo sin tecla
        if (e.key === "Delete" && !e.ctrlKey && !e.metaKey && !e.shiftKey && !e.altKey) {
          rebind(id, null);
          setClash(undefined);
          setCapturing(undefined);
          return;
        }
        const binding: Binding = {
          key: e.key.length === 1 ? e.key.toLowerCase() : e.key,
          ctrl: e.ctrlKey || e.metaKey,
          shift: e.shiftKey,
          alt: e.altKey,
        };
        if (shortcutRows().find((r) => r.id === id)?.plainKeyOnly && (binding.ctrl || binding.shift || binding.alt)) {
          setClash({ id, with: "" });
          return;
        }
        const other = conflictFor(id, binding);
        if (other) {
          setClash({ id, with: other });
          return;
        }
        rebind(id, binding);
        setClash(undefined);
        setCapturing(undefined);
        return;
      }
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
      <div
        role="dialog"
        aria-label={t("Configuración")}
        class="w-full max-w-3xl h-[min(34rem,90vh)] flex flex-col rounded-lg border border-border bg-bg shadow-xl"
      >
        <div class="flex items-center justify-between px-4 h-11 shrink-0 border-b border-border">
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

        <div class="flex flex-1 min-h-0">
          <nav class="w-48 shrink-0 p-2 space-y-0.5 border-r border-border overflow-y-auto" role="tablist" aria-orientation="vertical">
            <For each={TABS}>
              {(item) => (
                <button
                  role="tab"
                  aria-selected={tab() === item.id}
                  class={
                    "w-full flex items-center gap-2 px-2.5 h-8 rounded-md text-xs text-left transition-colors " +
                    (tab() === item.id
                      ? "bg-accent/20 text-accent font-medium"
                      : "text-text-muted hover:text-text hover:bg-surface/40")
                  }
                  onClick={() => setTab(item.id)}
                >
                  <item.icon size={14} />
                  <span class="truncate">{item.label()}</span>
                </button>
              )}
            </For>
          </nav>

          <div class="flex-1 min-w-0 overflow-y-auto p-4" role="tabpanel">
            <Show when={tab() === "general"}>
              <Section title={t("Apariencia")}>
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
                <Select
                  label={t("Tamaño de la interfaz")}
                  options={SCALE_OPTIONS}
                  value={String(uiScale())}
                  onChange={(v) => setUiScale(Number(v))}
                />
                <Hint>{t("Agranda o achica todo: texto, botones y paneles.")}</Hint>
              </Section>

              <Section title={t("Proyecto")}>
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
                  <Hint>
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
                  </Hint>
                </Show>
              </Section>

              <Section title={t("Disposición")}>
                <Hint>
                  {t("Olvida los anchos de paneles, las pestañas abiertas y las alturas recordadas, y recarga la interfaz. No toca tus preferencias ni tus proyectos.")}
                </Hint>
                <Show
                  when={confirmReset()}
                  fallback={
                    <button
                      class="h-8 px-3 rounded-md text-xs border border-border text-text hover:border-accent hover:text-accent"
                      onClick={() => setConfirmReset(true)}
                    >
                      {t("Restablecer la disposición")}
                    </button>
                  }
                >
                  <div class="flex items-center gap-2">
                    <button
                      class="h-8 px-3 rounded-md text-xs bg-accent text-bg font-medium"
                      onClick={() => {
                        resetLayout();
                        location.reload();
                      }}
                    >
                      {t("Sí, restablecer")}
                    </button>
                    <button
                      class="h-8 px-3 rounded-md text-xs text-text-muted hover:text-text"
                      onClick={() => setConfirmReset(false)}
                    >
                      {t("Cancelar")}
                    </button>
                  </div>
                </Show>
              </Section>
            </Show>

            <Show when={tab() === "viewer"}>
              <Section title={t("Navegación")}>
                <Checkbox
                  label={t("Navegación de trackpad")}
                  checked={props.trackpad}
                  onChange={props.onTrackpadChange}
                />
                <Hint>
                  {t("Dos dedos (o la rueda) giran la vista y con Mayús la desplazan; Ctrl o el pellizco hacen zoom. Sin esto, la rueda hace zoom.")}
                </Hint>
              </Section>

              <Section title={t("Pantalla")}>
                <Checkbox label={t("Mostrar los FPS en la cabecera")} checked={showFps()} onChange={setShowFps} />
                <Checkbox label={t("Calidad adaptativa al mover")} checked={adaptiveQuality()} onChange={setAdaptiveQuality} />
                <Hint>{t("Mientras animas, arrastras o giras la vista, el visor baja un poco la resolución para ir más fluido y la restaura al quedar quieto. Las capturas y los renders siempre salen a calidad completa.")}</Hint>
              </Section>

              <Section title={t("Grilla y unidades")}>
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
                <Hint>
                  <Show when={props.fileMetersPerUnit !== undefined}>
                    {t("El archivo dice 1 unidad = {m} m.", { m: props.fileMetersPerUnit ?? "" })}{" "}
                  </Show>
                  {t("Los STL y OBJ no guardan unidades: si la grilla no calza con el tamaño real, elige acá en qué está el modelo. Solo cambia cómo se mide, no el modelo.")}
                </Hint>
              </Section>
            </Show>

            <Show when={tab() === "shortcuts"}>
              <Section title={t("Teclado")}>
                <input
                  type="search"
                  class="w-full h-8 px-2.5 rounded-md border border-border bg-bg-lighter text-xs text-text placeholder:text-text-dim focus:outline-none focus:border-accent"
                  placeholder={t("Buscar un atajo, una tecla o un área…")}
                  aria-label={t("Buscar atajos")}
                  value={query()}
                  onInput={(e) => setQuery(e.currentTarget.value)}
                />
                <Hint>{t("Pulsa una combinación para cambiarla. Esc cancela; Supr deja el atajo sin tecla.")}</Hint>
                <Show when={!anyRows()}>
                  <p class="text-xs text-text-muted">{t("Ningún atajo coincide con «{q}».", { q: query() })}</p>
                </Show>
                <For each={AREAS.filter((a) => rowsOf(a.id).length > 0)}>
                  {(area) => (
                    <div class="space-y-1.5">
                      <h4 class="text-[11px] font-medium text-text pt-1">{area.label()}</h4>
                      <ul class="grid grid-cols-[1fr_auto] items-center gap-x-4 gap-y-1 text-xs">
                  <For each={rowsOf(area.id)}>
                    {(row) => (
                      <>
                        <li class="contents">
                          <span class="text-text-muted">{t(row.description)}</span>
                          <span class="flex items-center gap-1.5 justify-self-end">
                            <Show when={row.changed && capturing() !== row.id}>
                              <button
                                class="text-[11px] text-text-dim hover:text-accent"
                                title={t("Volver al atajo de fábrica")}
                                onClick={() => rebind(row.id, undefined)}
                              >
                                {t("Restablecer")}
                              </button>
                            </Show>
                            <button
                              class={
                                "min-w-24 px-2 py-0.5 rounded border font-sans text-[11px] whitespace-nowrap " +
                                (capturing() === row.id
                                  ? "border-accent text-accent bg-accent/10"
                                  : row.changed
                                    ? "border-accent/60 text-text bg-surface/40 hover:border-accent"
                                    : "border-border text-text bg-surface/40 hover:border-accent")
                              }
                              onClick={() => {
                                setClash(undefined);
                                setCapturing(capturing() === row.id ? undefined : row.id);
                              }}
                            >
                              {capturing() === row.id ? t("Pulsa las teclas…") : row.keys}
                            </button>
                          </span>
                          <Show when={clash()?.id === row.id}>
                            <span class="col-span-2 text-[11px] text-warning">
                              {clash()!.with
                                ? t("Ya la usa «{name}». Cámbiala o desactívala primero.", { name: t(clash()!.with) })
                                : t("Acá va una sola tecla, sin Ctrl, Shift ni Alt.")}
                            </span>
                          </Show>
                        </li>
                      </>
                    )}
                  </For>
                </ul>
                    </div>
                  )}
                </For>
                <Show when={hasShortcutChanges()}>
                  <button
                    class="h-8 px-3 rounded-md text-xs border border-border text-text hover:border-accent hover:text-accent"
                    onClick={() => {
                      resetShortcuts();
                      setCapturing(undefined);
                      setClash(undefined);
                    }}
                  >
                    {t("Restablecer todos los atajos")}
                  </button>
                </Show>
              </Section>
              <Section title={t("Mouse en el visor")}>
                <ShortcutTable rows={MOUSE_HINTS} />
                <Hint>{t("Los gestos del mouse y las teclas propias de cada herramienta (Esc, Enter, Supr, Q y A dentro del Sketch; las del pincel) no se pueden cambiar.")}</Hint>
              </Section>
            </Show>

            <Show when={tab() === "files"}>
              <Section title={t("Modelos pesados")}>
                <Checkbox
                  label={t("Mostrar primero una vista previa")}
                  checked={previewFirst()}
                  onChange={setPreviewFirstModel}
                />
                <Hint>{t("En modelos de más de 100 000 caras, el visor muestra una versión liviana al instante y la cambia por el detalle completo cuando llega.")}</Hint>
                <Checkbox
                  label={t("LOD al animar")}
                  checked={animationLod()}
                  onChange={setAnimationLodEnabled}
                />
                <Hint>{t("En modelos de más de 100 000 caras con esqueleto, el visor dibuja una versión simplificada mientras animas y vuelve al detalle completo al parar. Exportar, guardar y renderizar siempre usan la malla completa.")}</Hint>
                <Select
                  label={t("Armar la malla de edición")}
                  options={MESH_BUILD_OPTIONS()}
                  value={meshBuild()}
                  onChange={(mode) => setMeshBuild(mode as MeshBuildMode)}
                />
                <Hint>{t("La malla de edición es la que usan reparar, esqueleto, remallar y demás herramientas; verla en el visor no la necesita. En modelos grandes tarda segundos y ocupa mucha memoria.")}</Hint>
              </Section>
              <Section title={t("Importar")}>
                <Checkbox
                  label={t("Mostrar la información del archivo antes de importar")}
                  checked={peekBeforeImport()}
                  onChange={setPeekBeforeImportModel}
                />
                <Hint>{t("Antes de importar un modelo se muestra su peso y, si el formato lo permite, cuántas caras y materiales trae.")}</Hint>
                <button
                  class="text-xs text-text-muted hover:text-text underline disabled:opacity-40 disabled:no-underline"
                  disabled={recentFiles().length === 0}
                  onClick={clearRecent}
                >
                  {t("Borrar la lista de archivos recientes")}
                </button>
              </Section>
              <Section title={t("Exportar")}>
                <Select
                  label={t("Formato inicial")}
                  options={EXPORT_FORMAT_OPTIONS}
                  value={defaultExportFormat()}
                  onChange={setDefaultExportFormat}
                />
                <Hint>{t("Con el que arranca Exportar en un proyecto nuevo. Cada proyecto recuerda el suyo.")}</Hint>
              </Section>
            </Show>

            <Show when={tab() === "about"}>
              <Section>
                <div class="flex items-center gap-3">
                  <div>
                    <div class="text-sm font-semibold text-text">Pinocchio</div>
                    <div class="text-xs text-text-muted font-mono">{import.meta.env.VITE_APP_VERSION}</div>
                  </div>
                </div>
                <Hint>{t("Escanear, reparar, animar y fabricar modelos 3D")}</Hint>
              </Section>
            </Show>
          </div>
        </div>
      </div>
    </div>
  );
};

const ShortcutTable: Component<{ rows: ShortcutHint[] }> = (props) => (
  <ul class="grid grid-cols-[minmax(7rem,auto)_1fr] gap-x-4 gap-y-1.5 text-xs">
    <For each={props.rows}>
      {(row) => (
        <>
          <li class="contents">
            <kbd class="justify-self-start px-1.5 py-0.5 rounded border border-border bg-surface/40 font-sans text-[11px] text-text whitespace-nowrap">
              {row.keys}
            </kbd>
            <span class="text-text-muted">{t(row.description)}</span>
          </li>
        </>
      )}
    </For>
  </ul>
);
