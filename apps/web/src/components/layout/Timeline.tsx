import { Component, For, Show, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { clsx } from "clsx";
import { IconButton, NumberInput, Select, Tooltip } from "../ui";
import * as Icons from "../icons";
import { createPersisted, startDrag } from "../../lib/ui-state";
import {
  keyFrames,
  keyId,
  keysAtFrames,
  type AnimationClip,
  type BoneTrack,
  type KeyInterpolation,
} from "../../lib/animation";
import { HANDLE_MODES, type HandleMode, type Marker, type RotationCodecs } from "../../lib/curves";
import type { Mixer } from "../../lib/layers";
import { CurveEditor, type CurveChannelRow } from "./CurveEditor";
import { MixerEditor } from "./MixerEditor";

export interface TimelineRow {
  /** Articulación (−1 en los controles) */
  joint: number;
  /** Nombre del hueso, o id del control: la pista que muestra */
  bone: string;
  /** Texto de la fila (por defecto, `bone`) */
  label?: string;
  /** Profundidad en el árbol (sangría) */
  depth: number;
  control?: boolean;
  /** Canales de una cadena de IK (mezcla, fijado, balanceo) */
  ik?: boolean;
  /** Fila resumen de un grupo de huesos: sus miembros y si está plegado */
  group?: { id: string; color: string; collapsed: boolean; members: string[] };
}

export type TimelineMode = "keys" | "curves" | "mixer";

/** Herramientas de la barra de cada modo */
export type TimelineTool =
  | { op: "interpolation"; value: KeyInterpolation }
  | { op: "handles"; value: HandleMode }
  | { op: "ease"; value: "in" | "out" | "both" | "none" }
  | { op: "scale"; factor: number }
  | { op: "retime"; factor: number }
  | { op: "closeCycle" }
  | { op: "eulerFilter" }
  | { op: "smooth" }
  | { op: "noise"; amount: number }
  | { op: "reduce"; tolerance: number }
  | { op: "marker" };

export interface OnionSettings {
  enabled: boolean;
  before: number;
  after: number;
  step: number;
  mesh: boolean;
}

export interface TimelineProps {
  clip: AnimationClip;
  frame: number;
  playing: boolean;
  rows: TimelineRow[];
  selectedJoint: number;
  selectedControl?: string;
  /** Keys seleccionadas (ver `keyId`) */
  selection: Set<string>;
  autoKey: boolean;
  /** Cuadros fuera del límite de giro, por hueso (marcas rojas) */
  violations?: Map<string, number[]>;
  interpolation: KeyInterpolation;
  onFrame: (frame: number) => void;
  onTogglePlay: () => void;
  onRangeChange: (range: { start?: number; end?: number; fps?: number }) => void;
  onSelectJoint: (joint: number) => void;
  onSelectControl?: (id: string) => void;
  onSelection: (selection: Set<string>) => void;
  onMoveKeys: (delta: number) => void;
  onDeleteKeys: () => void;
  onInsertKey: () => void;
  onAutoKey: (enabled: boolean) => void;
  onInterpolation: (interpolation: KeyInterpolation) => void;

  mode: TimelineMode;
  onMode: (mode: TimelineMode) => void;
  onTool: (tool: TimelineTool) => void;
  onToggleGroup?: (id: string) => void;
  onlySelection: boolean;
  onOnlySelection: (v: boolean) => void;
  onMarker: (change: { rename?: { frame: number; name: string }; move?: { from: number; to: number }; remove?: number }) => void;
  onion: OnionSettings;
  onOnion: (v: OnionSettings) => void;
  paths: boolean;
  onPaths: (v: boolean) => void;

  /** Editor de curvas */
  curves: {
    rows: CurveChannelRow[];
    rotation: RotationCodecs;
    selection: Set<string>;
    onSelection: (s: Set<string>) => void;
    onEdit: (clip: AnimationClip, commit: boolean, description: string) => void;
  };
  /** Mezclador */
  mixer: {
    mixer: Mixer;
    clips: AnimationClip[];
    view: [number, number];
    selectedLayer?: string;
    onSelectLayer: (id: string) => void;
    onChange: (description: string, mixer: Mixer, commit: boolean) => void;
    onAddLayer: (clipId: string) => void;
    onBake: () => void;
    onEnabled: (enabled: boolean) => void;
  };
}

const INTERPOLATIONS = [
  { value: "bezier", label: "Bézier" },
  { value: "linear", label: "Lineal" },
  { value: "step", label: "Constante" },
];

const MODES: { value: TimelineMode; label: string; title: string }[] = [
  { value: "keys", label: "Claves", title: "Hoja de claves" },
  { value: "curves", label: "Curvas", title: "Editor de curvas" },
  { value: "mixer", label: "Mezclador", title: "Capas de animaciones (NLA)" },
];

const ROW = 22;
const NAMES = 168;

const ToolButton: Component<{ title: string; onClick: () => void; active?: boolean; disabled?: boolean; children: any }> = (p) => (
  <button
    title={p.title}
    disabled={p.disabled}
    class={clsx(
      "px-2 h-6 rounded text-[11px] border transition-colors disabled:opacity-40",
      p.active ? "bg-accent/20 border-accent text-accent" : "border-border text-text-muted hover:text-text hover:border-border-hover"
    )}
    onClick={p.onClick}
  >
    {p.children}
  </button>
);

/** Lista que ejecuta una acción al elegir y vuelve a mostrar su texto (no guarda valor) */
const ActionSelect: Component<{ placeholder: string; options: { value: string; label: string }[]; onPick: (value: string) => void }> = (p) => (
  <select
    class="h-6 shrink-0 px-1.5 rounded border border-border bg-bg-lighter text-[11px] text-text-muted hover:border-border-hover focus:outline-none"
    value=""
    onChange={(e) => {
      const v = e.currentTarget.value;
      e.currentTarget.value = "";
      if (v) p.onPick(v);
    }}
  >
    <option value="" disabled selected hidden>
      {p.placeholder}
    </option>
    <For each={p.options}>{(o) => <option value={o.value}>{o.label}</option>}</For>
  </select>
);

/**
 * Línea de tiempo con tres vistas: la hoja de claves (una fila por
 * articulación, resúmenes por grupo, marcadores), el editor de curvas y el
 * mezclador de capas. La regla es la misma para las tres.
 */
export const Timeline: Component<TimelineProps> = (props) => {
  const [height, setHeight] = createPersisted("timeline.height", 220);
  const [width, setWidth] = createSignal(600);
  const [dragDelta, setDragDelta] = createSignal(0);
  const [scaleFactor, setScaleFactor] = createSignal(2);
  const [retimeFactor, setRetimeFactor] = createSignal(0.5);
  const [noise, setNoise] = createSignal(2);
  const [tolerance, setTolerance] = createSignal(0.5);
  const [normalize, setNormalize] = createPersisted("curves.normalize", false);
  const [fitRequest, setFitRequest] = createSignal(0);
  const [renaming, setRenaming] = createSignal<number | null>(null);

  let lanesRef: HTMLDivElement | undefined;
  onMount(() => {
    const observer = new ResizeObserver(() => setWidth(lanesRef?.clientWidth ?? 600));
    if (lanesRef) observer.observe(lanesRef);
    onCleanup(() => observer.disconnect());
  });

  /** Cuadros que muestra la regla: el clip, o toda la mezcla */
  const view = (): [number, number] => (props.mode === "mixer" ? props.mixer.view : [props.clip.start, props.clip.end]);
  const span = () => Math.max(1, view()[1] - view()[0] + 1);
  const scale = () => width() / span();
  const x = (frame: number) => (frame - view()[0] + 0.5) * scale();
  const frameAt = (clientX: number) => {
    const rect = lanesRef!.getBoundingClientRect();
    const f = Math.round((clientX - rect.left) / scale() - 0.5 + view()[0]);
    return Math.min(view()[1], Math.max(view()[0], f));
  };

  const ticks = createMemo(() => {
    const steps = [1, 2, 5, 10, 20, 50, 100, 200, 500, 1000];
    const step = steps.find((s) => s * scale() >= 40) ?? 1000;
    const first = Math.ceil(view()[0] / step) * step;
    const list: number[] = [];
    for (let f = first; f <= view()[1]; f += step) list.push(f);
    return list;
  });

  const tracks = createMemo(() => new Map<string, BoneTrack>(props.clip.tracks.map((t) => [`${t.kind ?? "b"}|${t.bone}`, t])));
  const trackOf = (row: TimelineRow) => tracks().get(`${row.control ? "control" : row.ik ? "ik" : "b"}|${row.bone}`);
  const isActive = (row: TimelineRow) =>
    row.control || row.ik ? row.bone === props.selectedControl : row.joint === props.selectedJoint;
  const selectRow = (row: TimelineRow) => {
    if (row.group) return props.onToggleGroup?.(row.group.id);
    return row.control || row.ik ? props.onSelectControl?.(row.bone) : props.onSelectJoint(row.joint);
  };

  const summaryFrames = createMemo(() => [...new Set(props.clip.tracks.flatMap(keyFrames))].sort((a, b) => a - b));
  /** Cuadros con keys de los miembros de un grupo, y sus ids */
  const groupKeys = (members: string[], frame: number) =>
    members.flatMap((m) => {
      const t = tracks().get(`b|${m}`);
      return t && keyFrames(t).includes(frame) ? [keyId(m, frame)] : [];
    });
  const groupFrames = (members: string[]) =>
    [...new Set(members.flatMap((m) => keyFrames(tracks().get(`b|${m}`))))].sort((a, b) => a - b);

  const scrub = (e: PointerEvent) => {
    e.preventDefault();
    props.onFrame(frameAt(e.clientX));
    const move = (ev: PointerEvent) => props.onFrame(frameAt(ev.clientX));
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  const pressKey = (e: PointerEvent, ids: string[]) => {
    e.stopPropagation();
    const selected = ids.every((id) => props.selection.has(id));
    let selection = props.selection;
    if (e.shiftKey) {
      selection = new Set(props.selection);
      ids.forEach((id) => (selected ? selection.delete(id) : selection.add(id)));
      props.onSelection(selection);
      if (selected) return;
    } else if (!selected) {
      selection = new Set(ids);
      props.onSelection(selection);
    }
    startDrag(e, "ew-resize", (dx) => setDragDelta(Math.round(dx / scale())));
    const up = () => {
      window.removeEventListener("pointerup", up);
      const delta = dragDelta();
      setDragDelta(0);
      if (delta !== 0) props.onMoveKeys(delta);
    };
    window.addEventListener("pointerup", up);
  };

  /** Marcador: clic va al cuadro, arrastrar lo mueve, doble clic lo renombra, clic derecho lo borra */
  const pressMarker = (e: PointerEvent, m: Marker) => {
    e.stopPropagation();
    if (e.button !== 0) return;
    props.onFrame(m.frame);
    const x0 = e.clientX;
    const move = (ev: PointerEvent) => void ev;
    const up = (ev: PointerEvent) => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      const to = frameAt(ev.clientX);
      if (Math.abs(ev.clientX - x0) > 3 && to !== m.frame) props.onMarker({ move: { from: m.frame, to } });
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  const resize = (e: PointerEvent) => {
    const start = height();
    startDrag(e, "row-resize", (_, dy) => setHeight(Math.min(700, Math.max(96, start - dy))));
  };

  const Diamond: Component<{ frame: number; ids: string[]; step: boolean; bezier?: boolean; tint?: string }> = (p) => {
    const selected = () => p.ids.length > 0 && p.ids.every((id) => props.selection.has(id));
    const offset = () => (selected() ? dragDelta() : 0);
    return (
      <Show when={p.frame + offset() >= view()[0] && p.frame + offset() <= view()[1]}>
        <div
          class={clsx(
            "absolute top-1/2 w-2.5 h-2.5 -mt-[5px] -ml-[5px] border cursor-pointer",
            !p.step && "rotate-45",
            p.bezier && "rounded-full rotate-0",
            selected() ? "bg-accent border-accent-hover" : "bg-text-muted border-bg-darker hover:bg-text"
          )}
          style={{ left: `${x(p.frame + offset())}px`, ...(p.tint && !selected() ? { background: p.tint } : {}) }}
          onPointerDown={(e) => pressKey(e, p.ids)}
        />
      </Show>
    );
  };

  const Playhead = () => <div class="absolute inset-y-0 w-px bg-accent pointer-events-none" style={{ left: `${x(props.frame)}px` }} />;

  const Lane: Component<{ children: any; onClick?: () => void; active?: boolean; tint?: string }> = (p) => (
    <div
      class={clsx("relative border-b border-border/40", p.active && "bg-accent/5")}
      style={{ height: `${ROW}px`, ...(p.tint ? { background: `color-mix(in srgb, ${p.tint} 8%, transparent)` } : {}) }}
      onPointerDown={() => p.onClick?.()}
    >
      {p.children}
      <Playhead />
    </div>
  );

  const clipsForLayer = () => props.mixer.clips.map((c) => ({ value: c.id, label: c.name }));
  const layer = () => props.mixer.mixer.layers.find((l) => l.id === props.mixer.selectedLayer);
  const setLayer = (change: Record<string, unknown>, description: string) => {
    const l = layer();
    if (!l) return;
    props.mixer.onChange(description, { ...props.mixer.mixer, layers: props.mixer.mixer.layers.map((x) => (x.id === l.id ? { ...x, ...change } : x)) }, true);
  };

  return (
    <section class="relative flex flex-col shrink-0 border-t border-border bg-bg-darker" style={{ height: `${height()}px` }}>
      <div class="absolute inset-x-0 -top-0.5 h-1 z-10 cursor-row-resize hover:bg-accent/50" onPointerDown={resize} />

      {/* Transporte, rango y vista */}
      <header class="flex items-center gap-2 px-2 h-9 shrink-0 border-b border-border text-xs">
        <IconButton size="sm" variant="ghost" aria-label="Al inicio" onClick={() => props.onFrame(view()[0])}>
          <Icons.SkipBack size={14} />
        </IconButton>
        <Tooltip content="Reproducir / pausar (Espacio)">
          <IconButton size="sm" variant={props.playing ? "primary" : "ghost"} aria-label="Reproducir" onClick={props.onTogglePlay}>
            <Show when={props.playing} fallback={<Icons.Play size={14} />}>
              <Icons.Pause size={14} />
            </Show>
          </IconButton>
        </Tooltip>
        <IconButton size="sm" variant="ghost" aria-label="Al final" onClick={() => props.onFrame(view()[1])}>
          <Icons.SkipForward size={14} />
        </IconButton>
        <NumberInput class="w-20" label="▸" value={Math.round(props.frame)} step={1} onChange={(v) => props.onFrame(Math.round(v))} />
        <Show when={props.mode !== "mixer"}>
          <NumberInput class="w-24" label="De" value={props.clip.start} step={1} onChange={(v) => props.onRangeChange({ start: Math.round(v) })} />
          <NumberInput class="w-24" label="A" value={props.clip.end} step={1} onChange={(v) => props.onRangeChange({ end: Math.round(v) })} />
        </Show>
        <NumberInput class="w-20" suffix="fps" value={props.clip.fps} min={1} max={120} step={1} onChange={(v) => v > 0 && props.onRangeChange({ fps: v })} />
        <div class="flex rounded-md border border-border overflow-hidden ml-2">
          <For each={MODES}>
            {(m) => (
              <button
                title={m.title}
                class={clsx("px-2.5 h-7 text-xs", props.mode === m.value ? "bg-accent/20 text-accent" : "text-text-muted hover:bg-surface/50 hover:text-text")}
                onClick={() => props.onMode(m.value)}
              >
                {m.label}
              </button>
            )}
          </For>
        </div>
        <div class="flex-1" />
        <Tooltip content="Auto-key: girar o mover una articulación crea la key">
          <IconButton size="sm" variant={props.autoKey ? "primary" : "ghost"} aria-label="Auto-key" onClick={() => props.onAutoKey(!props.autoKey)}>
            <span class={clsx("w-2.5 h-2.5 rounded-full", props.autoKey ? "bg-red" : "border border-text-muted")} />
          </IconButton>
        </Tooltip>
        <Tooltip content="Insertar key (I)">
          <IconButton size="sm" variant="ghost" aria-label="Insertar key" onClick={props.onInsertKey}>
            <span class="w-2.5 h-2.5 rotate-45 bg-text-muted" />
          </IconButton>
        </Tooltip>
        <Tooltip content="Borrar keys seleccionadas (X / Supr)">
          <IconButton size="sm" variant="ghost" aria-label="Borrar keys" disabled={props.selection.size === 0 && props.curves.selection.size === 0} onClick={props.onDeleteKeys}>
            <Icons.X size={14} />
          </IconButton>
        </Tooltip>
      </header>

      {/* Herramientas de la vista */}
      <div class="flex items-center gap-1.5 px-2 h-8 shrink-0 border-b border-border text-[11px] overflow-x-auto whitespace-nowrap">
        <Show when={props.mode !== "mixer"}>
          <div class="w-28 shrink-0">
            <Select options={INTERPOLATIONS} value={props.interpolation} onChange={(v) => props.onInterpolation(v as KeyInterpolation)} />
          </div>
          <ToolButton title="Arranca suave: la key sale plana (Bézier)" onClick={() => props.onTool({ op: "ease", value: "in" })}>Acelerar</ToolButton>
          <ToolButton title="Llega suave: la key entra plana (Bézier)" onClick={() => props.onTool({ op: "ease", value: "out" })}>Frenar</ToolButton>
          <ToolButton title="Suave al salir y al llegar" onClick={() => props.onTool({ op: "ease", value: "both" })}>Ambos</ToolButton>
          <ToolButton title="Sin aceleración ni freno" onClick={() => props.onTool({ op: "ease", value: "none" })}>Sin</ToolButton>
          <span class="w-px h-4 bg-border mx-1" />
        </Show>
        <Show when={props.mode === "keys"}>
          <NumberInput class="w-20 shrink-0" label="×" step={0.1} min={0.01} value={scaleFactor()} onChange={setScaleFactor} />
          <ToolButton title="Escala las keys elegidas alrededor del cursor" onClick={() => props.onTool({ op: "scale", factor: scaleFactor() })}>Escalar</ToolButton>
          <NumberInput class="w-20 shrink-0" label="×" step={0.1} min={0.01} value={retimeFactor()} onChange={setRetimeFactor} />
          <ToolButton title="El tramo de las keys elegidas dura ese factor; lo de después se corre" onClick={() => props.onTool({ op: "retime", factor: retimeFactor() })}>Retiempo</ToolButton>
          <ToolButton title="El último cuadro igual al primero, con las tangentes continuas" onClick={() => props.onTool({ op: "closeCycle" })}>Cerrar ciclo</ToolButton>
          <span class="w-px h-4 bg-border mx-1" />
          <ToolButton title="Marcador con nombre en el cuadro actual" onClick={() => props.onTool({ op: "marker" })}>+ Marcador</ToolButton>
          <ToolButton title="Mostrar solo las articulaciones elegidas" active={props.onlySelection} onClick={() => props.onOnlySelection(!props.onlySelection)}>Solo elegidas</ToolButton>
        </Show>
        <Show when={props.mode === "curves"}>
          <ActionSelect placeholder="Manijas…" options={HANDLE_MODES} onPick={(v) => props.onTool({ op: "handles", value: v as HandleMode })} />
          <ToolButton title="Ver cada canal entre −1 y 1" active={normalize()} onClick={() => setNormalize(!normalize())}>Normalizar</ToolButton>
          <ToolButton title="Encuadrar lo elegido (o todo)" onClick={() => setFitRequest(fitRequest() + 1)}>Encuadrar</ToolButton>
          <span class="w-px h-4 bg-border mx-1" />
          <ToolButton title="Quita saltos de ±180° (giros continuos)" onClick={() => props.onTool({ op: "eulerFilter" })}>Filtro Euler</ToolButton>
          <ToolButton title="Acerca las keys elegidas al promedio con sus vecinas" onClick={() => props.onTool({ op: "smooth" })}>Suavizar</ToolButton>
          <NumberInput class="w-20 shrink-0" label="±" step={0.5} value={noise()} onChange={setNoise} />
          <ToolButton title="Ruido en las keys elegidas (grados o unidades)" onClick={() => props.onTool({ op: "noise", amount: noise() })}>Ruido</ToolButton>
          <NumberInput class="w-20 shrink-0" label="tol" step={0.1} value={tolerance()} onChange={setTolerance} />
          <ToolButton title="Quita keys que no cambian la curva más que la tolerancia" onClick={() => props.onTool({ op: "reduce", tolerance: tolerance() })}>Reducir</ToolButton>
          <ToolButton title="El último cuadro igual al primero, con las tangentes continuas" onClick={() => props.onTool({ op: "closeCycle" })}>Cerrar ciclo</ToolButton>
        </Show>
        <Show when={props.mode === "mixer"}>
          <ActionSelect placeholder="Agregar capa…" options={clipsForLayer()} onPick={(v) => props.mixer.onAddLayer(v)} />
          <ToolButton title="El visor muestra la mezcla de las capas" active={props.mixer.mixer.enabled} onClick={() => props.mixer.onEnabled(!props.mixer.mixer.enabled)}>Ver la mezcla</ToolButton>
          <ToolButton title="Hornea la mezcla a una animación nueva (para exportar o seguir editando)" disabled={props.mixer.mixer.layers.length === 0} onClick={props.mixer.onBake}>Hornear</ToolButton>
          <Show when={layer()}>
            {(l) => (
              <>
                <span class="w-px h-4 bg-border mx-1" />
                <NumberInput class="w-24 shrink-0" label="peso" suffix="%" min={0} max={100} step={5} value={Math.round(l().weight * 100)} onChange={(v) => setLayer({ weight: Math.max(0, Math.min(1, v / 100)) }, "Peso de la capa")} />
                <NumberInput class="w-24 shrink-0" label="desde" step={1} value={l().trimStart ?? props.mixer.clips.find((c) => c.id === l().clipId)?.start ?? 0} onChange={(v) => setLayer({ trimStart: Math.round(v) }, "Recortar capa")} />
                <NumberInput class="w-24 shrink-0" label="hasta" step={1} value={l().trimEnd ?? props.mixer.clips.find((c) => c.id === l().clipId)?.end ?? 0} onChange={(v) => setLayer({ trimEnd: Math.round(v) }, "Recortar capa")} />
                <NumberInput class="w-24 shrink-0" label="entra" step={1} min={0} value={l().blendIn} onChange={(v) => setLayer({ blendIn: Math.max(0, Math.round(v)) }, "Transición de entrada")} />
                <NumberInput class="w-24 shrink-0" label="sale" step={1} min={0} value={l().blendOut} onChange={(v) => setLayer({ blendOut: Math.max(0, Math.round(v)) }, "Transición de salida")} />
              </>
            )}
          </Show>
        </Show>
        <span class="w-px h-4 bg-border mx-1" />
        <ToolButton title="Papel cebolla: la pose de los cuadros vecinos" active={props.onion.enabled} onClick={() => props.onOnion({ ...props.onion, enabled: !props.onion.enabled })}>Cebolla</ToolButton>
        <Show when={props.onion.enabled}>
          <NumberInput class="w-16 shrink-0" label="−" min={0} max={5} step={1} value={props.onion.before} onChange={(v) => props.onOnion({ ...props.onion, before: Math.round(v) })} />
          <NumberInput class="w-16 shrink-0" label="+" min={0} max={5} step={1} value={props.onion.after} onChange={(v) => props.onOnion({ ...props.onion, after: Math.round(v) })} />
          <NumberInput class="w-20 shrink-0" label="cada" min={1} max={24} step={1} value={props.onion.step} onChange={(v) => props.onOnion({ ...props.onion, step: Math.max(1, Math.round(v)) })} />
          <ToolButton title="Fantasmas de la malla (si no, solo de los huesos)" active={props.onion.mesh} onClick={() => props.onOnion({ ...props.onion, mesh: !props.onion.mesh })}>Malla</ToolButton>
        </Show>
        <ToolButton title="Trayectorias de las articulaciones elegidas; sus keys se arrastran en el visor" active={props.paths} onClick={() => props.onPaths(!props.paths)}>Trayectorias</ToolButton>
      </div>

      {/* Regla con marcadores */}
      <div class="flex shrink-0 h-6 border-b border-border">
        <div class="shrink-0 px-2 flex items-center text-[10px] text-text-dim border-r border-border truncate" style={{ width: `${NAMES}px` }}>
          {props.mode === "mixer" ? "Mezcla" : props.clip.name}
        </div>
        <div ref={lanesRef} class="relative flex-1 min-w-0 cursor-ew-resize select-none overflow-hidden" onPointerDown={scrub}>
          <For each={ticks()}>
            {(f) => (
              <div class="absolute inset-y-0 border-l border-border/60 pl-1 text-[10px] text-text-dim font-mono" style={{ left: `${x(f)}px` }}>
                {f}
              </div>
            )}
          </For>
          <Show when={props.mode !== "mixer"}>
            <For each={props.clip.markers ?? []}>
              {(m) => (
                <div
                  class="absolute bottom-0 flex items-end gap-0.5 cursor-pointer"
                  style={{ left: `${x(m.frame) - 4}px` }}
                  title={`${m.name} · doble clic: renombrar · clic derecho: borrar`}
                  onPointerDown={(e) => pressMarker(e, m)}
                  onDblClick={(e) => {
                    e.stopPropagation();
                    setRenaming(m.frame);
                  }}
                  onContextMenu={(e) => {
                    e.preventDefault();
                    e.stopPropagation();
                    props.onMarker({ remove: m.frame });
                  }}
                >
                  <span class="w-0 h-0 border-x-4 border-x-transparent border-b-[7px] border-b-yellow" />
                  <Show
                    when={renaming() === m.frame}
                    fallback={<span class="text-[9px] leading-none text-yellow mb-px">{m.name}</span>}
                  >
                    <input
                      class="w-20 px-1 text-[10px] bg-surface text-text outline-none"
                      value={m.name}
                      autofocus
                      onPointerDown={(e) => e.stopPropagation()}
                      onKeyDown={(e) => {
                        e.stopPropagation();
                        if (e.key === "Enter") e.currentTarget.blur();
                        if (e.key === "Escape") setRenaming(null);
                      }}
                      onBlur={(e) => {
                        setRenaming(null);
                        const name = e.currentTarget.value.trim();
                        if (name && name !== m.name) props.onMarker({ rename: { frame: m.frame, name } });
                      }}
                    />
                  </Show>
                </div>
              )}
            </For>
          </Show>
          <div class="absolute top-0.5 -ml-3 w-6 rounded bg-accent text-[10px] text-bg text-center font-mono pointer-events-none" style={{ left: `${x(props.frame)}px` }}>
            {Math.round(props.frame)}
          </div>
        </div>
      </div>

      {/* Cuerpo según la vista */}
      <Show when={props.mode === "curves"}>
        <CurveEditor
          clip={props.clip}
          frame={props.frame}
          rows={props.curves.rows}
          rotation={props.curves.rotation}
          selection={props.curves.selection}
          onSelection={props.curves.onSelection}
          onEdit={props.curves.onEdit}
          normalize={normalize()}
          fitRequest={fitRequest()}
          namesWidth={NAMES}
        />
      </Show>
      <Show when={props.mode === "mixer"}>
        <MixerEditor
          mixer={props.mixer.mixer}
          clips={props.mixer.clips}
          frame={props.frame}
          xOf={x}
          scale={scale()}
          selectedLayer={props.mixer.selectedLayer}
          onSelectLayer={props.mixer.onSelectLayer}
          onChange={props.mixer.onChange}
          namesWidth={NAMES}
        />
      </Show>
      <Show when={props.mode === "keys"}>
        <div class="flex-1 min-h-0 overflow-y-auto overflow-x-hidden">
          <div class="flex">
            <div class="shrink-0 border-r border-border" style={{ width: `${NAMES}px` }}>
              <div class="px-2 flex items-center text-xs font-semibold text-text border-b border-border/40" style={{ height: `${ROW}px` }}>
                Resumen
              </div>
              <For each={props.rows}>
                {(row) => (
                  <div
                    class={clsx(
                      "flex items-center gap-1 text-xs truncate cursor-pointer border-b border-border/40",
                      row.group ? "font-semibold text-text" : isActive(row) ? "bg-accent/15 text-accent" : "text-text-muted hover:text-text"
                    )}
                    style={{ height: `${ROW}px`, "padding-left": `${8 + row.depth * 10}px` }}
                    onClick={() => selectRow(row)}
                  >
                    <Show when={row.group}>
                      {(g) => (
                        <>
                          <span class="text-[9px] w-2">{g().collapsed ? "▸" : "▾"}</span>
                          <span class="w-2 h-2 rounded-sm shrink-0" style={{ background: g().color }} />
                        </>
                      )}
                    </Show>
                    <Show when={row.control}>
                      <span class="w-2 h-2 rounded-full border border-current shrink-0" />
                    </Show>
                    <Show when={row.ik}>
                      <span class="text-[9px] font-mono shrink-0">IK</span>
                    </Show>
                    {row.label ?? row.bone}
                  </div>
                )}
              </For>
            </div>
            <div class="relative flex-1 min-w-0 overflow-hidden select-none">
              <Lane onClick={() => props.onSelection(new Set())}>
                <For each={summaryFrames()}>{(f) => <Diamond frame={f} ids={keysAtFrames(props.clip, new Set([f]))} step={false} />}</For>
              </Lane>
              <For each={props.rows}>
                {(row) => {
                  if (row.group) {
                    const g = row.group;
                    return (
                      <Lane tint={g.color} onClick={() => props.onSelection(new Set())}>
                        <For each={groupFrames(g.members)}>{(f) => <Diamond frame={f} ids={groupKeys(g.members, f)} step={false} tint={g.color} />}</For>
                      </Lane>
                    );
                  }
                  const track = () => trackOf(row);
                  const interpolationAt = (f: number) =>
                    ([...(track()?.rotation ?? []), ...(track()?.translation ?? [])] as { frame: number; interpolation: KeyInterpolation }[]).find((k) => k.frame === f)?.interpolation;
                  return (
                    <Lane
                      active={isActive(row)}
                      onClick={() => {
                        props.onSelection(new Set());
                        selectRow(row);
                      }}
                    >
                      <For each={row.control || row.ik ? [] : (props.violations?.get(row.bone) ?? [])}>
                        {(f) => (
                          <div
                            class="absolute bottom-0 h-1 bg-red/80 pointer-events-none"
                            style={{ left: `${x(f) - scale() / 2}px`, width: `${Math.max(1, scale())}px` }}
                            title="Fuera del límite de giro"
                          />
                        )}
                      </For>
                      <For each={keyFrames(track())}>
                        {(f) => <Diamond frame={f} ids={[keyId(row.bone, f)]} step={interpolationAt(f) === "step"} bezier={interpolationAt(f) === "bezier"} />}
                      </For>
                    </Lane>
                  );
                }}
              </For>
            </div>
          </div>
        </div>
      </Show>
    </section>
  );
};
