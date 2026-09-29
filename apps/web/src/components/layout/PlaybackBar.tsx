import { Component, For, Show } from "solid-js";
import { clsx } from "clsx";
import { IconButton } from "../ui";
import * as Icons from "../icons";
import { startDrag } from "../../lib/ui-state";

/** Un clip dentro de la secuencia, en segundos desde el inicio */
export interface PlaybackSegment {
  name: string;
  start: number;
  duration: number;
}

export interface PlaybackBarProps {
  segments: PlaybackSegment[];
  /** Posición en segundos (`null` = en reposo) */
  time: number | null;
  playing: boolean;
  loop: boolean;
  onTogglePlay: () => void;
  onStop: () => void;
  onLoop: (loop: boolean) => void;
  onSeek: (time: number) => void;
}

const formatTime = (t: number) => `${t.toFixed(1)} s`;

/**
 * Reproducción simple de las animaciones del modelo fuera del paso Animar:
 * los clips van uno tras otro en una sola línea, separados por rayitas.
 */
export const PlaybackBar: Component<PlaybackBarProps> = (props) => {
  let track: HTMLDivElement | undefined;
  const total = () => props.segments.reduce((sum, s) => sum + s.duration, 0);
  const fraction = (t: number) => (total() > 0 ? t / total() : 0);

  /** Posición bajo el puntero y arrastre para recorrer */
  const seek = (e: PointerEvent) => {
    if (!track) return;
    const rect = track.getBoundingClientRect();
    const at = (x: number) => Math.min(1, Math.max(0, (x - rect.left) / rect.width)) * total();
    props.onSeek(at(e.clientX));
    const startX = e.clientX;
    startDrag(e, "ew-resize", (dx) => props.onSeek(at(startX + dx)));
  };

  return (
    <div class="flex items-center gap-2 h-10 px-2 border-t border-border bg-bg-darker shrink-0">
      <IconButton
        variant="ghost"
        size="sm"
        aria-label={props.playing ? "Pausa" : "Reproducir"}
        title={props.playing ? "Pausa (Espacio)" : "Reproducir (Espacio)"}
        onClick={() => props.onTogglePlay()}
      >
        <Show when={props.playing} fallback={<Icons.Play size={14} />}>
          <Icons.Pause size={14} />
        </Show>
      </IconButton>
      <IconButton
        variant="ghost"
        size="sm"
        aria-label="Detener"
        title="Detener y volver a la pose de reposo"
        disabled={props.time === null}
        onClick={() => props.onStop()}
      >
        <Icons.Stop size={14} />
      </IconButton>
      <IconButton
        variant="ghost"
        size="sm"
        active={props.loop}
        aria-label="Repetir"
        title={props.loop ? "Repetir: al terminar vuelve a empezar" : "Repetir: apagado"}
        onClick={() => props.onLoop(!props.loop)}
      >
        <Icons.ArrowsClockwise size={14} />
      </IconButton>
      <span class="w-24 text-center text-xs font-mono text-text-muted shrink-0">
        {formatTime(props.time ?? 0)} / {formatTime(total())}
      </span>

      {/* Línea de tiempo: un tramo por clip */}
      <div ref={track} class="relative flex-1 h-7 rounded bg-bg-lighter cursor-pointer select-none" onPointerDown={seek}>
        <For each={props.segments}>
          {(s, i) => (
            <div
              class={clsx("absolute inset-y-0 overflow-hidden", i() > 0 && "border-l-2 border-text-muted/70")}
              style={{ left: `${100 * fraction(s.start)}%`, width: `${100 * fraction(s.duration)}%` }}
              title={`${s.name} · ${formatTime(s.duration)}`}
            >
              <span class="absolute left-1.5 top-1/2 -translate-y-1/2 text-[10px] text-text-muted whitespace-nowrap">
                {s.name}
              </span>
            </div>
          )}
        </For>
        <Show when={props.time !== null}>
          <div
            class="absolute inset-y-0 w-0.5 -ml-px bg-accent pointer-events-none"
            style={{ left: `${100 * fraction(props.time!)}%` }}
          />
        </Show>
      </div>
    </div>
  );
};
