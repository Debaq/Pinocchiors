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

export interface TimelineRow {
  joint: number;
  bone: string;
  /** Profundidad en el árbol (sangría) */
  depth: number;
}

export interface TimelineProps {
  clip: AnimationClip;
  frame: number;
  playing: boolean;
  rows: TimelineRow[];
  selectedJoint: number;
  /** Keys seleccionadas (ver `keyId`) */
  selection: Set<string>;
  autoKey: boolean;
  interpolation: KeyInterpolation;
  onFrame: (frame: number) => void;
  onTogglePlay: () => void;
  onRangeChange: (range: { start?: number; end?: number; fps?: number }) => void;
  onSelectJoint: (joint: number) => void;
  onSelection: (selection: Set<string>) => void;
  onMoveKeys: (delta: number) => void;
  onDeleteKeys: () => void;
  onInsertKey: () => void;
  onAutoKey: (enabled: boolean) => void;
  onInterpolation: (interpolation: KeyInterpolation) => void;
}

const INTERPOLATIONS = [
  { value: "linear", label: "Lineal" },
  { value: "step", label: "Constante" },
];

const ROW = 22;
const NAMES = 168;

/**
 * Línea de tiempo al estilo de la hoja de claves de Blender: regla para
 * moverse por los cuadros, una fila por articulación (y un resumen arriba)
 * con sus keys como rombos (cuadrados si son constantes). Clic selecciona
 * (Shift suma), arrastrar corre las seleccionadas.
 */
export const Timeline: Component<TimelineProps> = (props) => {
  const [height, setHeight] = createPersisted("timeline.height", 220);
  const [width, setWidth] = createSignal(600);
  /** Corrimiento de las keys seleccionadas mientras se arrastran */
  const [dragDelta, setDragDelta] = createSignal(0);

  let lanesRef: HTMLDivElement | undefined;
  onMount(() => {
    const observer = new ResizeObserver(() => setWidth(lanesRef?.clientWidth ?? 600));
    if (lanesRef) observer.observe(lanesRef);
    onCleanup(() => observer.disconnect());
  });

  const span = () => Math.max(1, props.clip.end - props.clip.start + 1);
  const scale = () => width() / span();
  const x = (frame: number) => (frame - props.clip.start + 0.5) * scale();
  const frameAt = (clientX: number) => {
    const rect = lanesRef!.getBoundingClientRect();
    const f = Math.round((clientX - rect.left) / scale() - 0.5 + props.clip.start);
    return Math.min(props.clip.end, Math.max(props.clip.start, f));
  };

  /** Marcas de la regla: cada 1, 2, 5, 10… cuadros, con al menos 40 px entre ellas */
  const ticks = createMemo(() => {
    const steps = [1, 2, 5, 10, 20, 50, 100, 200, 500, 1000];
    const step = steps.find((s) => s * scale() >= 40) ?? 1000;
    const first = Math.ceil(props.clip.start / step) * step;
    const list: number[] = [];
    for (let f = first; f <= props.clip.end; f += step) list.push(f);
    return list;
  });

  const tracks = createMemo(() => new Map<string, BoneTrack>(props.clip.tracks.map((t) => [t.bone, t])));
  const summaryFrames = createMemo(() => [...new Set(props.clip.tracks.flatMap(keyFrames))].sort((a, b) => a - b));

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

  /** Clic en un rombo: `ids` son las keys que representa (varias en el resumen) */
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

  const resize = (e: PointerEvent) => {
    const start = height();
    startDrag(e, "row-resize", (_, dy) => setHeight(Math.min(600, Math.max(96, start - dy))));
  };

  const Diamond: Component<{ frame: number; ids: string[]; step: boolean }> = (p) => {
    const selected = () => p.ids.length > 0 && p.ids.every((id) => props.selection.has(id));
    const offset = () => (selected() ? dragDelta() : 0);
    return (
      <Show when={p.frame + offset() >= props.clip.start && p.frame + offset() <= props.clip.end}>
        <div
          class={clsx(
            "absolute top-1/2 w-2.5 h-2.5 -mt-[5px] -ml-[5px] border cursor-pointer",
            !p.step && "rotate-45",
            selected() ? "bg-accent border-accent-hover" : "bg-text-muted border-bg-darker hover:bg-text"
          )}
          style={{ left: `${x(p.frame + offset())}px` }}
          onPointerDown={(e) => pressKey(e, p.ids)}
        />
      </Show>
    );
  };

  const Playhead = () => (
    <div class="absolute inset-y-0 w-px bg-accent pointer-events-none" style={{ left: `${x(props.frame)}px` }} />
  );

  const Lane: Component<{ children: any; onClick?: () => void; active?: boolean }> = (p) => (
    <div
      class={clsx("relative border-b border-border/40", p.active && "bg-accent/5")}
      style={{ height: `${ROW}px` }}
      onPointerDown={() => p.onClick?.()}
    >
      {p.children}
      <Playhead />
    </div>
  );

  return (
    <section class="relative flex flex-col shrink-0 border-t border-border bg-bg-darker" style={{ height: `${height()}px` }}>
      <div class="absolute inset-x-0 -top-0.5 h-1 z-10 cursor-row-resize hover:bg-accent/50" onPointerDown={resize} />

      {/* Transporte y opciones */}
      <header class="flex items-center gap-2 px-2 h-9 shrink-0 border-b border-border text-xs">
        <IconButton size="sm" variant="ghost" aria-label="Al inicio" onClick={() => props.onFrame(props.clip.start)}>
          <Icons.SkipBack size={14} />
        </IconButton>
        <Tooltip content="Reproducir / pausar (Espacio)">
          <IconButton size="sm" variant={props.playing ? "primary" : "ghost"} aria-label="Reproducir" onClick={props.onTogglePlay}>
            <Show when={props.playing} fallback={<Icons.Play size={14} />}>
              <Icons.Pause size={14} />
            </Show>
          </IconButton>
        </Tooltip>
        <IconButton size="sm" variant="ghost" aria-label="Al final" onClick={() => props.onFrame(props.clip.end)}>
          <Icons.SkipForward size={14} />
        </IconButton>
        <NumberInput class="w-20" label="▸" value={Math.round(props.frame)} step={1} onChange={(v) => props.onFrame(Math.round(v))} />
        <NumberInput class="w-24" label="De" value={props.clip.start} step={1} onChange={(v) => props.onRangeChange({ start: Math.round(v) })} />
        <NumberInput class="w-24" label="A" value={props.clip.end} step={1} onChange={(v) => props.onRangeChange({ end: Math.round(v) })} />
        <NumberInput class="w-20" suffix="fps" value={props.clip.fps} min={1} max={120} step={1} onChange={(v) => v > 0 && props.onRangeChange({ fps: v })} />
        <div class="flex-1" />
        <div class="w-28">
          <Select
            options={INTERPOLATIONS}
            value={props.interpolation}
            onChange={(v) => props.onInterpolation(v as KeyInterpolation)}
          />
        </div>
        <Tooltip content="Auto-key: girar o mover una articulación crea la key">
          <IconButton size="sm" variant={props.autoKey ? "primary" : "ghost"} aria-label="Auto-key" onClick={() => props.onAutoKey(!props.autoKey)}>
            <span class={clsx("w-2.5 h-2.5 rounded-full", props.autoKey ? "bg-red-400" : "border border-text-muted")} />
          </IconButton>
        </Tooltip>
        <Tooltip content="Insertar key (I)">
          <IconButton size="sm" variant="ghost" aria-label="Insertar key" onClick={props.onInsertKey}>
            <span class="w-2.5 h-2.5 rotate-45 bg-text-muted" />
          </IconButton>
        </Tooltip>
        <Tooltip content="Borrar keys seleccionadas (X / Supr)">
          <IconButton size="sm" variant="ghost" aria-label="Borrar keys" disabled={props.selection.size === 0} onClick={props.onDeleteKeys}>
            <Icons.X size={14} />
          </IconButton>
        </Tooltip>
      </header>

      {/* Regla */}
      <div class="flex shrink-0 h-6 border-b border-border">
        <div class="shrink-0 px-2 flex items-center text-[10px] text-text-dim border-r border-border" style={{ width: `${NAMES}px` }}>
          {props.clip.name}
        </div>
        <div ref={lanesRef} class="relative flex-1 min-w-0 cursor-ew-resize select-none overflow-hidden" onPointerDown={scrub}>
          <For each={ticks()}>
            {(f) => (
              <div class="absolute inset-y-0 border-l border-border/60 pl-1 text-[10px] text-text-dim font-mono" style={{ left: `${x(f)}px` }}>
                {f}
              </div>
            )}
          </For>
          <div
            class="absolute top-0.5 -ml-3 w-6 rounded bg-accent text-[10px] text-bg text-center font-mono pointer-events-none"
            style={{ left: `${x(props.frame)}px` }}
          >
            {Math.round(props.frame)}
          </div>
        </div>
      </div>

      {/* Filas: resumen y una por articulación */}
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
                    "flex items-center text-xs truncate cursor-pointer border-b border-border/40",
                    row.joint === props.selectedJoint ? "bg-accent/15 text-accent" : "text-text-muted hover:text-text"
                  )}
                  style={{ height: `${ROW}px`, "padding-left": `${8 + row.depth * 10}px` }}
                  onClick={() => props.onSelectJoint(row.joint)}
                >
                  {row.bone}
                </div>
              )}
            </For>
          </div>
          <div class="relative flex-1 min-w-0 overflow-hidden select-none">
            <Lane onClick={() => props.onSelection(new Set())}>
              <For each={summaryFrames()}>
                {(f) => <Diamond frame={f} ids={keysAtFrames(props.clip, new Set([f]))} step={false} />}
              </For>
            </Lane>
            <For each={props.rows}>
              {(row) => {
                const track = () => tracks().get(row.bone);
                const isStep = (f: number) => track()?.rotation.find((k) => k.frame === f)?.interpolation === "step";
                return (
                  <Lane
                    active={row.joint === props.selectedJoint}
                    onClick={() => {
                      props.onSelection(new Set());
                      props.onSelectJoint(row.joint);
                    }}
                  >
                    <For each={keyFrames(track())}>
                      {(f) => <Diamond frame={f} ids={[keyId(row.bone, f)]} step={isStep(f)} />}
                    </For>
                  </Lane>
                );
              }}
            </For>
          </div>
        </div>
      </div>
    </section>
  );
};
