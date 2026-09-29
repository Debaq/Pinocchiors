import { Component, For, JSX, createSignal } from "solid-js";
import { clsx } from "clsx";
import { Panel, Button, Select } from "../ui";
import * as Icons from "../icons";
import type { PlacementMode } from "../../lib/Viewer3D";
import type { Axis, OriginMode } from "../../lib/placement";

export interface PlacementPanelProps {
  /** Qué se está eligiendo con el clic (`undefined` = nada) */
  mode?: PlacementMode;
  onPickMode?: (mode: PlacementMode | undefined) => void;
  onRotate?: (axis: Axis, degrees: number) => void;
  onMirror?: (axis: Axis) => void;
  onDrop?: () => void;
  /** Con "point" se elige el punto con un clic en el visor */
  onOrigin?: (mode: OriginMode) => void;
  /** Escala uniforme (queda apoyado); `description` va al historial */
  onScale?: (factor: number, description: string) => void;
  /** Traslación en unidades de la escena */
  onMove?: (offset: [number, number, number]) => void;
  /** Medidas de la caja del modelo, en unidades de la escena */
  size?: [number, number, number];
  /** Unidad en que se muestran las medidas (la de la grilla) */
  unit?: { label: string; perSceneUnit: number };
  disabled?: boolean;
}

const ORIGIN_OPTIONS = [
  { value: "base", label: "Centro de la base" },
  { value: "box", label: "Centro de la caja" },
  { value: "mass", label: "Centro de masa" },
  { value: "point", label: "Punto con clic" },
];

const AXES: Axis[] = ["x", "y", "z"];

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
}) => (
  <label class="flex items-center gap-1 min-w-0" title={props.title}>
    {props.label && <span class="text-[11px] text-text-muted shrink-0">{props.label}</span>}
    <input
      type="text"
      inputmode="decimal"
      value={fmt(props.value)}
      disabled={props.disabled}
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
            props.value === axis ? "bg-accent/20 text-accent" : "text-text-muted hover:text-text hover:bg-current/40"
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
  const [rotAxis, setRotAxis] = createSignal<Axis>("y");
  const [rotDegrees, setRotDegrees] = createSignal(45);
  const [factor, setFactor] = createSignal(2);
  const [move, setMove] = createSignal<[number, number, number]>([0, 0, 0]);
  const toggle = (mode: PlacementMode) => props.onPickMode?.(props.mode === mode ? undefined : mode);

  const perUnit = () => props.unit?.perSceneUnit ?? 1;
  const unitLabel = () => props.unit?.label ?? "";
  /** Medidas en la unidad de la grilla */
  const dims = () => (props.size ?? [0, 0, 0]).map((v) => v * perUnit()) as [number, number, number];

  /** Cambiar una medida escala todo en proporción */
  const setDimension = (axis: number, value: number) => {
    const current = dims()[axis];
    if (!(value > 0) || !(current > 0)) return;
    const names = ["ancho", "alto", "fondo"];
    props.onScale?.(value / current, `Escalar: ${names[axis]} ${fmt(value)} ${unitLabel()}`);
  };

  return (
    <Panel title="Orientación" icon={<Icons.CubeFocus size={14} />} defaultOpen>
      <div class="space-y-4">
        <div class="space-y-2">
          <Button
            onClick={() => toggle("floor")}
            variant={props.mode === "floor" ? "danger" : "primary"}
            fullWidth
            disabled={props.disabled}
          >
            {props.mode === "floor" ? "Cancelar" : "Elegir piso"}
          </Button>
          <Button
            onClick={() => toggle("front")}
            variant={props.mode === "front" ? "danger" : "default"}
            fullWidth
            disabled={props.disabled}
          >
            {props.mode === "front" ? "Cancelar" : "Elegir frente"}
          </Button>
          <p class="text-[11px] text-text-muted leading-snug">
            Todo esto transforma el modelo. Si ya hay esqueleto, retopología o pesos, se transforman con él para seguir
            calzando.
          </p>
        </div>

        <Section title="Girar">
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
            <CommitInput value={rotDegrees()} onCommit={setRotDegrees} suffix="°" disabled={props.disabled} />
            <Button size="sm" onClick={() => props.onRotate?.(rotAxis(), rotDegrees())} disabled={props.disabled}>
              Girar
            </Button>
          </div>
          <Button size="sm" fullWidth onClick={() => props.onRotate?.("x", -90)} disabled={props.disabled}>
            Z arriba → Y arriba
          </Button>
        </Section>

        <Section title="Espejar">
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

        <Section title={`Tamaño${unitLabel() ? ` (${unitLabel()})` : ""}`}>
          <div class="grid grid-cols-3 gap-1.5">
            <For each={["Ancho", "Alto", "Fondo"]}>
              {(name, i) => (
                <div class="space-y-0.5">
                  <span class="text-[10px] text-text-muted">
                    {name} ({AXES[i()].toUpperCase()})
                  </span>
                  <CommitInput
                    value={dims()[i()]}
                    onCommit={(v) => setDimension(i(), v)}
                    disabled={props.disabled || !props.size}
                    title="Escribe una medida: el modelo se escala en proporción"
                  />
                </div>
              )}
            </For>
          </div>
          <div class="flex items-center gap-1.5">
            <CommitInput value={factor()} onCommit={setFactor} label="×" disabled={props.disabled} />
            <Button
              size="sm"
              onClick={() => factor() > 0 && props.onScale?.(factor(), `Escalar ×${fmt(factor())}`)}
              disabled={props.disabled || !(factor() > 0)}
            >
              Escalar
            </Button>
          </div>
          <div class="grid grid-cols-2 gap-1.5">
            <For each={CONVERSIONS}>
              {(c) => (
                <Button size="sm" onClick={() => props.onScale?.(c.factor, `Convertir ${c.label}`)} disabled={props.disabled}>
                  <span title={c.title}>{c.label}</span>
                </Button>
              )}
            </For>
          </div>
          <p class="text-[11px] text-text-muted leading-snug">
            La escala es igual en los tres ejes y se hace desde el centro de la base, así el modelo sigue apoyado.
          </p>
        </Section>

        <Section title={`Mover${unitLabel() ? ` (${unitLabel()})` : ""}`}>
          <div class="grid grid-cols-3 gap-1.5">
            <For each={AXES}>
              {(axis, i) => (
                <CommitInput
                  label={axis.toUpperCase()}
                  value={move()[i()]}
                  onCommit={(v) => setMove((m) => m.map((x, j) => (j === i() ? v : x)) as [number, number, number])}
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
            Mover
          </Button>
          <Select
            label="Origen"
            options={ORIGIN_OPTIONS}
            value={origin()}
            onChange={(value) => setOrigin(value as OriginMode)}
            disabled={props.disabled}
          />
          <div class="grid grid-cols-2 gap-1.5">
            <Button
              size="sm"
              variant={props.mode === "point" ? "danger" : "default"}
              onClick={() => (props.mode === "point" ? props.onPickMode?.(undefined) : props.onOrigin?.(origin()))}
              disabled={props.disabled}
            >
              {props.mode === "point" ? "Cancelar" : "Fijar origen"}
            </Button>
            <Button size="sm" onClick={() => props.onDrop?.()} disabled={props.disabled}>
              Apoyar en el piso
            </Button>
          </div>
        </Section>
      </div>
    </Panel>
  );
};
