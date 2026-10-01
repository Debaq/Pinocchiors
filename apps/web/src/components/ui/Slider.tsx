import { Slider as KobalteSlider } from "@kobalte/core/slider";
import { Show, createSignal } from "solid-js";
import { clsx } from "clsx";

export interface SliderProps {
  value?: number;
  onChange?: (value: number) => void;
  min?: number;
  max?: number;
  step?: number;
  disabled?: boolean;
  label?: string;
  showValue?: boolean;
  formatValue?: (value: number) => string;
  /** Etiqueta, barra y valor en una sola fila (paneles densos) */
  inline?: boolean;
  /** Texto de ayuda al pasar sobre la etiqueta */
  title?: string;
  class?: string;
}

export const Slider = (props: SliderProps) => {
  const min = () => props.min ?? 0;
  const max = () => props.max ?? 100;
  const step = () => props.step ?? 1;
  const value = () => props.value ?? min();
  const formatValue = () => props.formatValue ?? ((v: number) => v.toString());

  // Doble clic en el valor: se escribe con el teclado (Enter confirma, Escape cancela)
  const [editing, setEditing] = createSignal(false);
  const decimals = () => (step().toString().split(".")[1] ?? "").length;
  const commit = (text: string) => {
    if (!editing()) return;
    setEditing(false);
    const parsed = parseFloat(text.replace(",", "."));
    if (!Number.isFinite(parsed)) return;
    const clamped = Math.min(max(), Math.max(min(), parsed));
    const rounded = Number(clamped.toFixed(decimals()));
    if (rounded !== value()) props.onChange?.(rounded);
  };

  const valueLabel = () => (
    <Show when={props.showValue !== false}>
      <Show
        when={editing()}
        fallback={
          <KobalteSlider.ValueLabel
            class={clsx(
              "text-xs font-mono text-text-muted rounded-sm px-1 -mx-1",
              !props.disabled && "cursor-text hover:bg-surface/30"
            )}
            title={props.disabled ? undefined : "Doble clic para escribir el valor"}
            onDblClick={() => !props.disabled && setEditing(true)}
          />
        }
      >
        <input
          ref={(el) => requestAnimationFrame(() => { el.focus(); el.select(); })}
          type="text"
          inputmode="decimal"
          value={Number(value().toFixed(decimals()))}
          class="w-20 text-xs font-mono text-right text-text bg-bg-darker border border-accent rounded-sm px-1 outline-none"
          onKeyDown={(e) => {
            // Las flechas y atajos del visor no deben tocar el slider ni la escena
            e.stopPropagation();
            if (e.key === "Enter") commit(e.currentTarget.value);
            else if (e.key === "Escape") setEditing(false);
          }}
          onBlur={(e) => commit(e.currentTarget.value)}
        />
      </Show>
    </Show>
  );

  const track = () => (
    <KobalteSlider.Track
      class={clsx(
        "relative h-1.5 w-full rounded-full",
        "bg-surface",
        "data-[disabled]:opacity-50"
      )}
    >
      <KobalteSlider.Fill
        class="absolute h-full rounded-full bg-accent"
      />
      <KobalteSlider.Thumb
        class={clsx(
          "block w-4 h-4 -top-[5px]",
          "bg-accent rounded-full",
          "border-2 border-bg",
          "shadow-md",
          "transition-transform duration-100",
          "hover:scale-110",
          "focus:outline-none focus:ring-2 focus:ring-accent/50",
          "data-[disabled]:pointer-events-none"
        )}
      >
        <KobalteSlider.Input />
      </KobalteSlider.Thumb>
    </KobalteSlider.Track>
  );

  return (
    <KobalteSlider
      value={[value()]}
      onChange={(values) => props.onChange?.(values[0])}
      minValue={min()}
      maxValue={max()}
      step={step()}
      disabled={props.disabled}
      // Kobalte pone este texto en ValueLabel e ignora lo que tenga adentro
      getValueLabel={(params) => formatValue()(params.values[0])}
      class={clsx("w-full", props.class)}
    >
      <Show
        when={props.inline}
        fallback={
          <>
            <div class="flex items-center justify-between mb-1.5">
              <Show when={props.label}>
                <KobalteSlider.Label class="text-xs text-text-muted" title={props.title}>
                  {props.label}
                </KobalteSlider.Label>
              </Show>
              {valueLabel()}
            </div>
            {track()}
          </>
        }
      >
        <div class="flex items-center gap-2 min-h-6">
          <Show when={props.label}>
            <KobalteSlider.Label class="w-[38%] shrink-0 truncate text-xs text-text-muted" title={props.title ?? props.label}>
              {props.label}
            </KobalteSlider.Label>
          </Show>
          <div class="flex-1 min-w-0">{track()}</div>
          <div class="w-[4.5rem] shrink-0 flex justify-end">{valueLabel()}</div>
        </div>
      </Show>
    </KobalteSlider>
  );
};
