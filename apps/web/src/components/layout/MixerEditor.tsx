import { Component, For, Show } from "solid-js";
import { clsx } from "clsx";
import * as Icons from "../icons";
import type { AnimationClip } from "../../lib/animation";
import { layerLength, layerTrim, type AnimationLayer, type Mixer } from "../../lib/layers";

export interface MixerEditorProps {
  mixer: Mixer;
  clips: AnimationClip[];
  frame: number;
  /** Posición en la línea de tiempo de un cuadro y al revés (misma escala que la regla) */
  xOf: (frame: number) => number;
  scale: number;
  selectedLayer?: string;
  onSelectLayer: (id: string) => void;
  onChange: (description: string, mixer: Mixer, commit: boolean) => void;
  namesWidth: number;
}

const LANE = 34;

/**
 * Mezclador: una pista por capa (la de más arriba se aplica última). La
 * tira se arrastra para moverla en el tiempo; su borde derecho, para
 * repetirla; los triángulos de las puntas son las transiciones.
 */
export const MixerEditor: Component<MixerEditorProps> = (props) => {
  // De arriba (se aplica última) hacia abajo
  const ordered = () => [...props.mixer.layers].reverse();
  const clipOf = (l: AnimationLayer) => props.clips.find((c) => c.id === l.clipId);

  const setLayer = (id: string, change: Partial<AnimationLayer>, description: string, commit = true) =>
    props.onChange(description, { ...props.mixer, layers: props.mixer.layers.map((l) => (l.id === id ? { ...l, ...change } : l)) }, commit);

  /** Arrastre en cuadros enteros desde el valor de partida */
  const drag = (e: PointerEvent, apply: (df: number) => Partial<AnimationLayer>, description: string, id: string) => {
    e.preventDefault();
    e.stopPropagation();
    props.onSelectLayer(id);
    const x0 = e.clientX;
    let last: Partial<AnimationLayer> | null = null;
    const move = (ev: PointerEvent) => {
      const df = Math.round((ev.clientX - x0) / props.scale);
      last = apply(df);
      setLayer(id, last, description, false);
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      if (last) setLayer(id, last, description, true);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  const move = (i: number, dir: -1 | 1) => {
    const layers = [...props.mixer.layers];
    const j = i + dir;
    if (j < 0 || j >= layers.length) return;
    [layers[i], layers[j]] = [layers[j], layers[i]];
    props.onChange(dir > 0 ? "Subir capa" : "Bajar capa", { ...props.mixer, layers }, true);
  };

  return (
    <div class="flex flex-1 min-h-0 overflow-y-auto overflow-x-hidden">
      <div class="shrink-0 border-r border-border" style={{ width: `${props.namesWidth}px` }}>
        <For each={ordered()} fallback={<p class="px-2 py-2 text-xs text-text-dim">Sin capas: agrega animaciones para mezclarlas.</p>}>
          {(layer) => {
            const index = () => props.mixer.layers.indexOf(layer);
            return (
              <div
                class={clsx(
                  "flex items-center gap-1 px-1.5 text-xs border-b border-border/40 cursor-pointer",
                  layer.id === props.selectedLayer ? "bg-accent/15 text-accent" : "text-text-muted hover:text-text"
                )}
                style={{ height: `${LANE}px` }}
                onClick={() => props.onSelectLayer(layer.id)}
              >
                <button
                  class="shrink-0 text-text-muted hover:text-text"
                  title={layer.muted ? "Activar" : "Silenciar"}
                  onClick={(e) => {
                    e.stopPropagation();
                    setLayer(layer.id, { muted: !layer.muted || undefined }, layer.muted ? "Activar capa" : "Silenciar capa");
                  }}
                >
                  <Show when={layer.muted} fallback={<Icons.Eye size={12} />}>
                    <Icons.EyeSlash size={12} />
                  </Show>
                </button>
                <span class={clsx("flex-1 truncate", layer.muted && "line-through opacity-60")}>{clipOf(layer)?.name ?? "?"}</span>
                <button
                  class="shrink-0 px-1 rounded border border-border text-[9px] font-mono"
                  title={layer.mode === "add" ? "Se suma a lo de abajo (clic: reemplazar)" : "Reemplaza lo de abajo (clic: sumar)"}
                  onClick={(e) => {
                    e.stopPropagation();
                    setLayer(layer.id, { mode: layer.mode === "add" ? "replace" : "add" }, "Modo de la capa");
                  }}
                >
                  {layer.mode === "add" ? "+" : "="}
                </button>
                <div class="flex flex-col shrink-0">
                  <button class="leading-none text-[9px] hover:text-text" title="Subir" onClick={(e) => (e.stopPropagation(), move(index(), 1))}>▲</button>
                  <button class="leading-none text-[9px] hover:text-text" title="Bajar" onClick={(e) => (e.stopPropagation(), move(index(), -1))}>▼</button>
                </div>
                <button
                  class="shrink-0 text-text-muted hover:text-red"
                  title="Quitar la capa"
                  onClick={(e) => {
                    e.stopPropagation();
                    props.onChange("Quitar capa", { ...props.mixer, layers: props.mixer.layers.filter((l) => l.id !== layer.id) }, true);
                  }}
                >
                  <Icons.X size={12} />
                </button>
              </div>
            );
          }}
        </For>
      </div>
      <div class="relative flex-1 min-w-0 overflow-hidden select-none">
        <For each={ordered()}>
          {(layer) => {
            const clip = () => clipOf(layer);
            return (
              <div class="relative border-b border-border/40" style={{ height: `${LANE}px` }}>
                <Show when={clip()}>
                  {(c) => {
                    const length = () => layerLength(layer, c());
                    const span = () => {
                      const [a, b] = layerTrim(layer, c());
                      return b - a + 1;
                    };
                    const left = () => props.xOf(layer.start) - props.scale / 2;
                    const w = () => length() * props.scale;
                    return (
                      <div
                        class={clsx(
                          "absolute top-1.5 bottom-1.5 rounded border overflow-hidden cursor-grab",
                          layer.id === props.selectedLayer ? "border-accent bg-accent/25" : "border-border-hover bg-surface/70",
                          layer.muted && "opacity-40"
                        )}
                        style={{ left: `${left()}px`, width: `${Math.max(4, w())}px` }}
                        title={`${c().name} · desde el cuadro ${layer.start} · ${length()} cuadros · peso ${Math.round(layer.weight * 100)} %`}
                        onPointerDown={(e) => {
                          const start = layer.start;
                          drag(e, (df) => ({ start: start + df }), "Mover capa", layer.id);
                        }}
                      >
                        {/* Repeticiones */}
                        <For each={Array.from({ length: Math.max(0, layer.repeat - 1) }, (_, i) => i + 1)}>
                          {(i) => <div class="absolute inset-y-0 border-l border-dashed border-text-dim/60" style={{ left: `${i * span() * props.scale}px` }} />}
                        </For>
                        {/* Transiciones */}
                        <Show when={layer.blendIn > 0}>
                          <svg class="absolute inset-y-0 left-0 h-full pointer-events-none" style={{ width: `${layer.blendIn * props.scale}px` }} viewBox="0 0 10 10" preserveAspectRatio="none">
                            <polygon points="0,10 10,0 0,0" fill="var(--color-bg-darker)" fill-opacity="0.6" />
                          </svg>
                        </Show>
                        <Show when={layer.blendOut > 0}>
                          <svg class="absolute inset-y-0 right-0 h-full pointer-events-none" style={{ width: `${layer.blendOut * props.scale}px` }} viewBox="0 0 10 10" preserveAspectRatio="none">
                            <polygon points="10,10 0,0 10,0" fill="var(--color-bg-darker)" fill-opacity="0.6" />
                          </svg>
                        </Show>
                        <span class="absolute left-1.5 top-1/2 -translate-y-1/2 text-[10px] text-text whitespace-nowrap pointer-events-none">
                          {c().name}
                          {layer.repeat > 1 ? ` ×${layer.repeat}` : ""}
                        </span>
                        {/* Borde derecho: repetir */}
                        <div
                          class="absolute inset-y-0 right-0 w-1.5 cursor-ew-resize bg-accent/40 hover:bg-accent"
                          title="Arrastrar para repetir"
                          onPointerDown={(e) => {
                            const repeat = layer.repeat;
                            drag(e, (df) => ({ repeat: Math.max(1, Math.round((repeat * span() + df) / span())) }), "Repetir capa", layer.id);
                          }}
                        />
                      </div>
                    );
                  }}
                </Show>
                <div class="absolute inset-y-0 w-px bg-accent pointer-events-none" style={{ left: `${props.xOf(props.frame)}px` }} />
              </div>
            );
          }}
        </For>
      </div>
    </div>
  );
};
