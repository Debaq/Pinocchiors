import { Component, For, Show } from "solid-js";
import { clsx } from "clsx";
import { RetopologyStep, type RetopologyStepProps } from "./RetopologyStep";

/** Formas de ordenar la malla (ver libs/quadriflow/PLAN_REMALLAR.md) */
export type RemeshMode = "retopology" | "simplify" | "isotropic" | "voxel" | "quads" | "smooth";

export const REMESH_MODES: { id: RemeshMode; label: string; what: string; use: string; ready: boolean }[] = [
  {
    id: "retopology",
    label: "Retopología",
    what: "Malla nueva de quads que siguen la forma.",
    use: "Para animar, deformar o subdividir.",
    ready: true,
  },
  {
    id: "simplify",
    label: "Simplificar",
    what: "Quita triángulos manteniendo la forma; conserva UV y pesos.",
    use: "Escaneos pesados para web, juegos o el laminador.",
    ready: false,
  },
  {
    id: "isotropic",
    label: "Isótropo",
    what: "Triángulos parejos, todos del mismo tamaño.",
    use: "Escaneos con triángulos alargados; una base limpia.",
    ready: false,
  },
  {
    id: "voxel",
    label: "Vóxeles",
    what: "Rehace la superficie desde el volumen: cierra agujeros y une lo que se cruza.",
    use: "Mallas rotas o de muchas piezas, antes de imprimir.",
    ready: false,
  },
  {
    id: "quads",
    label: "A quads",
    what: "Triángulos a quads: junta pares de triángulos vecinos sin mover vértices.",
    use: "Mallas bien hechas que vienen trianguladas de otro programa.",
    ready: false,
  },
  {
    id: "smooth",
    label: "Suavizar",
    what: "Empareja los vértices sin cambiar la conectividad ni encoger.",
    use: "Quitar el ruido de un escaneo.",
    ready: false,
  },
];

/** Cantidades de una malla para el "antes → después" */
export interface MeshCount {
  vertices: number;
  faces: number;
  /** "triángulos", "quads"… */
  faceLabel: string;
}

/** Antes → después de un remallado, con la desviación al original */
export const RemeshSummary: Component<{ before: MeshCount; after: MeshCount; deviationPercent?: number | null }> = (props) => {
  const num = (v: number) => v.toLocaleString("es");
  const pct = (a: number, b: number) => (a > 0 ? ` (${Math.round((b / a) * 100)} %)` : "");
  return (
    <div data-remesh-summary class="rounded-md border border-border bg-surface/30 p-2 text-xs space-y-1">
      <div class="grid grid-cols-[auto_1fr_auto_1fr] gap-x-2 gap-y-0.5 items-baseline">
        <span class="text-text-dim">Antes</span>
        <span class="text-text">
          {num(props.before.faces)} {props.before.faceLabel}
        </span>
        <span class="text-text-dim">→</span>
        <span class="text-text">
          {num(props.after.faces)} {props.after.faceLabel}
          <span class="text-text-dim">{pct(props.before.faces, props.after.faces)}</span>
        </span>
        <span />
        <span class="text-text-muted">{num(props.before.vertices)} vértices</span>
        <span />
        <span class="text-text-muted">{num(props.after.vertices)} vértices</span>
      </div>
      <Show when={props.deviationPercent != null}>
        <p class="text-text-muted">
          Se aleja del original hasta un <span class="text-text">{props.deviationPercent!.toLocaleString("es", { maximumFractionDigits: 2 })} %</span> del
          tamaño
        </p>
      </Show>
    </div>
  );
};

export interface RemeshStepProps {
  mode: RemeshMode;
  onModeChange: (mode: RemeshMode) => void;
  /** La malla de ahora (triángulos) */
  mesh: { vertices: number; faces: number };
  retopology: RetopologyStepProps & { result?: { vertices: number; quads: number } };
}

/**
 * Remallar: las formas de ordenar la malla en un solo lugar. Arriba se elige
 * el modo; abajo, sus opciones y el antes → después.
 */
export const RemeshStep: Component<RemeshStepProps> = (props) => {
  const current = () => REMESH_MODES.find((m) => m.id === props.mode) ?? REMESH_MODES[0];
  return (
    <div class="space-y-4">
      <div>
        <h3 class="text-sm font-semibold text-text">Remallar</h3>
        <p class="text-xs text-text-muted leading-relaxed">Ordenar la malla según para qué se va a usar.</p>
      </div>
      <div role="radiogroup" aria-label="Modo de remallado" class="grid grid-cols-2 gap-1">
        <For each={REMESH_MODES}>
          {(m) => (
            <button
              role="radio"
              aria-checked={props.mode === m.id}
              data-mode={m.id}
              title={m.use}
              class={clsx(
                "flex items-center justify-between gap-1 rounded border px-2 py-1 text-left text-xs transition-colors",
                props.mode === m.id ? "border-accent bg-accent/10 text-text" : "border-border text-text-muted hover:text-text hover:bg-surface/50",
              )}
              onClick={() => props.onModeChange(m.id)}
            >
              <span class="truncate">{m.label}</span>
              <Show when={!m.ready}>
                <span class="shrink-0 text-[9px] rounded bg-surface px-1 text-text-dim">pronto</span>
              </Show>
            </button>
          )}
        </For>
      </div>
      <div class="border-t border-border pt-3 space-y-3">
        <p class="text-xs text-text-muted leading-relaxed">
          {current().what} <span class="text-text-dim">{current().use}</span>
        </p>
        <Show
          when={current().id === "retopology"}
          fallback={
            <p data-remesh-pending class="text-xs text-text-dim leading-relaxed">
              Este modo todavía no está: llega en las próximas etapas. Mientras, la retopología y la reparación siguen disponibles.
            </p>
          }
        >
          <RetopologyStep {...props.retopology} />
          <Show when={props.retopology.hasResult && props.retopology.result}>
            {(r) => (
              <RemeshSummary
                before={{ vertices: props.mesh.vertices, faces: props.mesh.faces, faceLabel: "triángulos" }}
                after={{ vertices: r().vertices, faces: r().quads, faceLabel: "quads" }}
                deviationPercent={props.retopology.quality?.max_distance_percent}
              />
            )}
          </Show>
        </Show>
      </div>
    </div>
  );
};
