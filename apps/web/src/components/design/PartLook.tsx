// Aspecto de una pieza: opacidad (acrílico, vidrio) y tonos listos. Lo usan
// el editor del ensamble (por instancia) y la lista de piezas de Diseñar.

import { Component, For, createEffect, createSignal, on, onCleanup } from "solid-js";
import { Slider, Tooltip } from "../ui";
import { t as tl } from "../../lib/i18n";

/** Tonos listos: color (null = deja el que tiene) y opacidad (null = opaca) */
export const LOOK_PRESETS = () => [
  { label: tl("Opaca"), color: null, opacity: null },
  { label: tl("Acrílico transparente"), color: "#dceeff", opacity: 0.25 },
  { label: tl("Acrílico humo"), color: "#4b5058", opacity: 0.5 },
  { label: tl("Acrílico azul"), color: "#2f7fe0", opacity: 0.45 },
  { label: tl("Acrílico rojo"), color: "#e04040", opacity: 0.45 },
  { label: tl("Acrílico verde"), color: "#2fae5a", opacity: 0.45 },
  { label: tl("Acrílico ámbar"), color: "#f2a531", opacity: 0.45 },
] as const;

export const PartLook: Component<{
  /** Opacidad actual, 0–1 */
  opacity: number;
  /** Cambia la opacidad (null = opaca, sin valor propio) */
  onOpacity: (opacity: number | null) => void;
  /** Un tono listo: color (null = deja el que tiene) y opacidad */
  onPreset: (color: string | null, opacity: number | null) => void;
}> = (props) => {
  // Mientras se arrastra solo cambia el número; se guarda al soltar un momento (no un paso de deshacer por píxel)
  const [value, setValue] = createSignal(Math.round(props.opacity * 100));
  createEffect(on(() => props.opacity, (o) => setValue(Math.round(o * 100))));
  let timer: ReturnType<typeof setTimeout> | undefined;
  const change = (v: number) => {
    setValue(v);
    clearTimeout(timer);
    timer = setTimeout(() => props.onOpacity(v >= 100 ? null : v / 100), 300);
  };
  onCleanup(() => clearTimeout(timer));
  return (
    <div class="space-y-1" data-part-look>
      <Slider
        inline
        label={tl("Opacidad")}
        title={tl("Menos de 100 %: se ve a través, como un acrílico o un vidrio (se sigue eligiendo)")}
        min={5}
        max={100}
        step={5}
        value={value()}
        formatValue={(v) => `${v} %`}
        onChange={change}
      />
      <div class="flex flex-wrap gap-1">
        <For each={LOOK_PRESETS()}>
          {(p) => (
            <Tooltip content={p.label}>
              <button
                type="button"
                aria-label={p.label}
                class="h-4 w-4 rounded-sm border border-border hover:ring-1 hover:ring-accent"
                style={{
                  background: p.color ?? "var(--color-surface)",
                  opacity: p.opacity === null ? 1 : Math.max(0.35, p.opacity + 0.2),
                }}
                onClick={() => {
                  clearTimeout(timer);
                  props.onPreset(p.color, p.opacity);
                }}
              />
            </Tooltip>
          )}
        </For>
      </div>
    </div>
  );
};
