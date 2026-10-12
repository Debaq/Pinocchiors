import { Component, For, JSX, Show, createSignal } from "solid-js";
import { clsx } from "clsx";
import { Panel, Button, Select } from "../ui";
import * as Icons from "../icons";
import type { PlacementMode } from "../../lib/Viewer3D";
import type { OriginMode, PivotChoice, PivotMode } from "../../lib/placement";
import { AXES, type Axis } from "../../lib/axes";
import { t } from "../../lib/i18n";

export interface PlacementPanelProps {
  /** Qué se está eligiendo con el clic (`undefined` = nada) */
  mode?: PlacementMode;
  onPickMode?: (mode: PlacementMode | undefined) => void;
  /** Ejes como se muestran: Z arriba, frente hacia −Y */
  onRotate?: (axis: Axis, degrees: number) => void;
  onMirror?: (axis: Axis) => void;
  onDrop?: () => void;
  /** Con "point" se elige el punto con un clic en el visor */
  onOrigin?: (mode: OriginMode) => void;
  /** Escala uniforme (queda apoyado); `description` va al historial */
  onScale?: (factor: number, description: string) => void;
  /** Traslación en unidades de la escena */
  onMove?: (offset: [number, number, number]) => void;
  /** Medidas de la caja del modelo en X, Y, Z (Z arriba), en unidades de la escena */
  size?: [number, number, number];
  /** Unidad en que se muestran las medidas (la de la grilla) */
  unit?: { label: string; perSceneUnit: number };
  /** Posición de la referencia al mover, en los ejes que se muestran y unidades de la escena */
  reference?: [number, number, number];
  /** Centro del gizmo: `spin` para girar y escalar, `move` para mover */
  pivots?: { spin: PivotChoice; move: PivotChoice };
  /** "point" no pasa por aquí: se elige con un clic (`onPickMode`) */
  onPivot?: (kind: "spin" | "move", choice: PivotChoice) => void;
  disabled?: boolean;
}

const TABS = [
  { id: "place" as const, get label() { return t("Colocar"); } },
  { id: "move" as const, get label() { return t("Mover"); } },
  { id: "rotate" as const, get label() { return t("Girar"); } },
  { id: "scale" as const, get label() { return t("Escala"); } },
];

const PIVOT_OPTIONS = [
  { value: "auto", get label() { return t("Automático"); } },
  { value: "base", get label() { return t("Centro de la base"); } },
  { value: "box", get label() { return t("Centro de la caja"); } },
  { value: "mass", get label() { return t("Centro de masa"); } },
  { value: "point", get label() { return t("Punto con clic"); } },
];

const ORIGIN_OPTIONS = [
  { value: "base", get label() { return t("Centro de la base"); } },
  { value: "box", get label() { return t("Centro de la caja"); } },
  { value: "mass", get label() { return t("Centro de masa"); } },
  { value: "point", get label() { return t("Punto con clic"); } },
];

/** Conversiones típicas de un modelo que vino en otra unidad */
const CONVERSIONS: { label: string; factor: number; title: string }[] = [
  { label: "mm → m", factor: 0.001, title: "Estaba en milímetros y se trata como metros: ×0,001" },
  { label: "m → mm", factor: 1000, title: "Estaba en metros y se trata como milímetros: ×1000" },
  { label: "cm → mm", factor: 10, title: "Estaba en centímetros y se trata como milímetros: ×10" },
  { label: "pulg → mm", factor: 25.4, title: "Estaba en pulgadas y se trata como milímetros: ×25,4" },
];

const parse = (text: string) => parseFloat(text.replace(",", "."));
const fmt = (v: number) => Number(v.toPrecision(5)).toString();

/** Campo numérico que aplica al confirmar (Enter o salir), no con cada tecla */
const CommitInput = (props: {
  value: number;
  onCommit: (value: number) => void;
  label?: string;
  suffix?: string;
  disabled?: boolean;
  title?: string;
  /** Paso de la rueda del mouse (sin él la rueda no lo cambia) */
  wheelStep?: number;
}) => (
  <label class="flex items-center gap-1 min-w-0" title={props.title}>
    {props.label && <span class="text-[11px] text-text-muted shrink-0">{props.label}</span>}
    <input
      type="text"
      inputmode="decimal"
      value={fmt(props.value)}
      disabled={props.disabled}
      data-wheel-number={props.wheelStep !== undefined ? "" : undefined}
      data-wheel-step={props.wheelStep}
      class={clsx(
        "w-full min-w-0 h-7 px-1.5 rounded text-xs font-mono text-right text-text",
        "bg-bg-lighter border border-border focus:border-accent focus:outline-none",
        "disabled:opacity-50"
      )}
      onKeyDown={(e) => {
        // Los atajos del visor no deben dispararse al escribir
        e.stopPropagation();
        if (e.key === "Enter") e.currentTarget.blur();
        if (e.key === "Escape") {
          e.currentTarget.value = fmt(props.value);
          e.currentTarget.blur();
        }
      }}
      onBlur={(e) => {
        const v = parse(e.currentTarget.value);
        if (Number.isFinite(v) && v !== props.value) props.onCommit(v);
        else e.currentTarget.value = fmt(props.value);
      }}
      onChange={(e) => {
        // La rueda avisa con "change" sin pasar por el foco
        if (props.wheelStep === undefined || document.activeElement === e.currentTarget) return;
        const v = parse(e.currentTarget.value);
        if (Number.isFinite(v) && v !== props.value) props.onCommit(v);
      }}
    />
    {props.suffix && <span class="text-[10px] text-text-muted shrink-0">{props.suffix}</span>}
  </label>
);

const Section = (props: { title: string; children: JSX.Element }) => (
  <div class="space-y-2">
    <p class="text-[11px] uppercase tracking-wide text-text-muted">{props.title}</p>
    {props.children}
  </div>
);

/** Botones X / Y / Z para elegir un eje */
const AxisPicker = (props: { value: Axis; onChange: (axis: Axis) => void }) => (
  <div class="flex rounded-md border border-border overflow-hidden shrink-0">
    <For each={AXES}>
      {(axis) => (
        <button
          class={clsx(
            "w-7 h-7 text-xs font-medium",
            props.value === axis ? "bg-accent/20 text-accent" : "text-text-muted hover:text-text hover:bg-surface/40"
          )}
          onClick={() => props.onChange(axis)}
        >
          {axis.toUpperCase()}
        </button>
      )}
    </For>
  </div>
);

export const PlacementPanel: Component<PlacementPanelProps> = (props) => {
  const [origin, setOrigin] = createSignal<OriginMode>("base");
  const [rotAxis, setRotAxis] = createSignal<Axis>("z");
  const [rotDegrees, setRotDegrees] = createSignal(45);
  const [factor, setFactor] = createSignal(2);
  const [move, setMove] = createSignal<[number, number, number]>([0, 0, 0]);
  const [tab, setTab] = createSignal<"place" | "move" | "rotate" | "scale">("place");
  const toggle = (mode: PlacementMode) => props.onPickMode?.(props.mode === mode ? undefined : mode);

  const perUnit = () => props.unit?.perSceneUnit ?? 1;
  const unitLabel = () => props.unit?.label ?? "";
  /** Medidas en la unidad de la grilla */
  const dims = () => (props.size ?? [0, 0, 0]).map((v) => v * perUnit()) as [number, number, number];

  /** Cambiar una medida escala todo en proporción */
  const setDimension = (axis: number, value: number) => {
    const current = dims()[axis];
    if (!(value > 0) || !(current > 0)) return;
    const names = [t("ancho"), t("fondo"), t("alto")];
    props.onScale?.(value / current, t("Escalar: {eje} {valor} {unidad}", { eje: names[axis], valor: fmt(value), unidad: unitLabel() }));
  };

  /** Selector del centro del gizmo; "Punto con clic" se elige en el visor */
  const PivotSelect = (p: { kind: "spin" | "move"; pick: "pivotSpin" | "pivotMove"; label: string }) => (
    <div class="space-y-1.5">
      <Select
        label={p.label}
        options={PIVOT_OPTIONS}
        value={props.pivots?.[p.kind].mode ?? "auto"}
        onChange={(value) => {
          if (value === "point") props.onPickMode?.(p.pick);
          else props.onPivot?.(p.kind, { mode: value as PivotMode });
        }}
        disabled={props.disabled}
      />
      {props.mode === p.pick && (
        <Button size="sm" variant="danger" fullWidth onClick={() => props.onPickMode?.(undefined)}>
          {t("Cancelar")}
        </Button>
      )}
    </div>
  );

  /** Mover la referencia a la coordenada escrita: el modelo se desplaza la diferencia */
  const moveReferenceTo = (axis: number, value: number) => {
    const ref = props.reference;
    if (!ref) return;
    const offset: [number, number, number] = [0, 0, 0];
    offset[axis] = value / perUnit() - ref[axis];
    props.onMove?.(offset);
  };

  return (
    <Panel title={t("Posicionar modelo")} icon={<Icons.CubeFocus size={14} />} defaultOpen>
      <div class="space-y-4">
        <div class="grid grid-cols-4 rounded-md border border-border overflow-hidden">
          <For each={TABS}>
            {(item) => (
              <button
                class={clsx(
                  "h-7 text-xs font-medium",
                  tab() === item.id ? "bg-accent/20 text-accent" : "text-text-muted hover:text-text hover:bg-surface/40"
                )}
                onClick={() => setTab(item.id)}
              >
                {item.label}
              </button>
            )}
          </For>
        </div>

        <Show when={tab() === "place"}>
          <Section title={t("Apoyar según una cara")}>
            <Button
              onClick={() => toggle("floor")}
              variant={props.mode === "floor" ? "danger" : "primary"}
              fullWidth
              disabled={props.disabled}
            >
              {props.mode === "floor" ? t("Cancelar") : t("Elegir piso")}
            </Button>
            <Button
              onClick={() => toggle("front")}
              variant={props.mode === "front" ? "danger" : "default"}
              fullWidth
              disabled={props.disabled}
            >
              {props.mode === "front" ? t("Cancelar") : t("Elegir frente")}
            </Button>
            <Button size="sm" fullWidth onClick={() => props.onDrop?.()} disabled={props.disabled}>
              {t("Apoyar en el piso")}
            </Button>
            <p class="text-[11px] text-text-muted leading-snug">
              {t("Todo esto transforma el modelo. Si ya hay esqueleto, retopología o pesos, se transforman con él para seguir calzando.")}
            </p>
          </Section>

          <Section title={t("Eje vertical del archivo")}>
            {/* STL, PLY y 3MF se leen con Z arriba; GLB y OBJ con Y arriba. Si el archivo no seguía la costumbre, queda acostado */}
            <div class="grid grid-cols-2 gap-1.5">
              <Button size="sm" onClick={() => props.onRotate?.("x", -90)} disabled={props.disabled}>
                <span title={t("Para un GLB u OBJ que quedó acostado: su programa usaba Z arriba")}>{t("Venía con Z arriba")}</span>
              </Button>
              <Button size="sm" onClick={() => props.onRotate?.("x", 90)} disabled={props.disabled}>
                <span title={t("Para un STL, PLY o 3MF que quedó acostado: su programa usaba Y arriba")}>{t("Venía con Y arriba")}</span>
              </Button>
            </div>
          </Section>

          <Section title={t("Origen del modelo")}>
            <Select
              label={t("Origen")}
              options={ORIGIN_OPTIONS}
              value={origin()}
              onChange={(value) => setOrigin(value as OriginMode)}
              disabled={props.disabled}
            />
            <Button
              size="sm"
              fullWidth
              variant={props.mode === "point" ? "danger" : "default"}
              onClick={() => (props.mode === "point" ? props.onPickMode?.(undefined) : props.onOrigin?.(origin()))}
              disabled={props.disabled}
            >
              {props.mode === "point" ? t("Cancelar") : t("Fijar origen")}
            </Button>
          </Section>
        </Show>

        <Show when={tab() === "move"}>
          <Section title={t("Referencia")}>
            <PivotSelect kind="move" pick="pivotMove" label={t("Punto que agarras")} />
            <div class="grid grid-cols-3 gap-1.5">
              <For each={AXES}>
                {(axis, i) => (
                  <CommitInput
                    label={axis.toUpperCase()}
                    value={(props.reference?.[i()] ?? 0) * perUnit()}
                    onCommit={(v) => moveReferenceTo(i(), v)}
                    wheelStep={1}
                    disabled={props.disabled || !props.reference}
                    title={t("Escribe dónde debe quedar la referencia: el modelo se mueve hasta ahí")}
                  />
                )}
              </For>
            </div>
            <p class="text-[11px] text-text-muted leading-snug">
              {t("Posición de la referencia en la grilla")}{unitLabel() ? ` (${unitLabel()})` : ""}. {t("Escribe otra y el modelo se mueve hasta ahí. No cambia el origen del modelo.")}
            </p>
          </Section>

          <Section title={`${t("Desplazar")}${unitLabel() ? ` (${unitLabel()})` : ""}`}>
            <div class="grid grid-cols-3 gap-1.5">
              <For each={AXES}>
                {(axis, i) => (
                  <CommitInput
                    label={axis.toUpperCase()}
                    value={move()[i()]}
                    onCommit={(v) => setMove((m) => m.map((x, j) => (j === i() ? v : x)) as [number, number, number])}
                    wheelStep={1}
                    disabled={props.disabled}
                  />
                )}
              </For>
            </div>
            <Button
              size="sm"
              fullWidth
              onClick={() => props.onMove?.(move().map((v) => v / perUnit()) as [number, number, number])}
              disabled={props.disabled || move().every((v) => v === 0)}
            >
              {t("Mover")}
            </Button>
          </Section>
        </Show>

        <Show when={tab() === "rotate"}>
          <Section title={t("Centro de giro")}>
            <PivotSelect kind="spin" pick="pivotSpin" label={t("Girar, espejar y escalar sobre")} />
            <p class="text-[11px] text-text-muted leading-snug">
              {t("Es también donde queda el gizmo de Rotar y Escalar (R, S). Automático: centro de la caja al girar, centro de la base al escalar.")}
            </p>
          </Section>

          <Section title={t("Girar")}>
            <div class="grid grid-cols-3 gap-1.5">
              <For each={[-90, 90]}>
                {(degrees) => (
                  <For each={AXES}>
                    {(axis) => (
                      <Button size="sm" onClick={() => props.onRotate?.(axis, degrees)} disabled={props.disabled}>
                        {axis.toUpperCase()} {degrees > 0 ? "+90" : "−90"}
                      </Button>
                    )}
                  </For>
                )}
              </For>
            </div>
            <div class="flex items-center gap-1.5">
              <AxisPicker value={rotAxis()} onChange={setRotAxis} />
              <CommitInput value={rotDegrees()} onCommit={setRotDegrees} wheelStep={1} suffix="°" disabled={props.disabled} />
              <Button size="sm" onClick={() => props.onRotate?.(rotAxis(), rotDegrees())} disabled={props.disabled}>
                {t("Girar")}
              </Button>
            </div>
          </Section>

          <Section title={t("Espejar")}>
            <div class="grid grid-cols-3 gap-1.5">
              <For each={AXES}>
                {(axis) => (
                  <Button size="sm" onClick={() => props.onMirror?.(axis)} disabled={props.disabled}>
                    {axis.toUpperCase()}
                  </Button>
                )}
              </For>
            </div>
          </Section>
        </Show>

        <Show when={tab() === "scale"}>
          <Section title={`${t("Tamaño")}${unitLabel() ? ` (${unitLabel()})` : ""}`}>
            <div class="grid grid-cols-3 gap-1.5">
              <For each={[t("Ancho"), t("Fondo"), t("Alto")]}>
                {(name, i) => (
                  <div class="space-y-0.5">
                    <span class="text-[10px] text-text-muted">
                      {name} ({AXES[i()].toUpperCase()})
                    </span>
                    <CommitInput
                      value={dims()[i()]}
                      onCommit={(v) => setDimension(i(), v)}
                      disabled={props.disabled || !props.size}
                      title={t("Escribe una medida: el modelo se escala en proporción")}
                    />
                  </div>
                )}
              </For>
            </div>
            <div class="flex items-center gap-1.5">
              <CommitInput value={factor()} onCommit={setFactor} wheelStep={0.1} label="×" disabled={props.disabled} />
              <Button
                size="sm"
                onClick={() => factor() > 0 && props.onScale?.(factor(), t("Escalar ×{f}", { f: fmt(factor()) }))}
                disabled={props.disabled || !(factor() > 0)}
              >
                {t("Escalar")}
              </Button>
            </div>
            <div class="grid grid-cols-2 gap-1.5">
              <For each={CONVERSIONS}>
                {(c) => (
                  <Button size="sm" onClick={() => props.onScale?.(c.factor, t("Convertir {c}", { c: c.label }))} disabled={props.disabled}>
                    <span title={t(c.title)}>{t(c.label)}</span>
                  </Button>
                )}
              </For>
            </div>
            <p class="text-[11px] text-text-muted leading-snug">
              {t("La escala es igual en los tres ejes. Se hace desde el centro de giro (pestaña Girar); en automático, desde el centro de la base, así el modelo sigue apoyado.")}
            </p>
          </Section>
        </Show>
      </div>
    </Panel>
  );
};
