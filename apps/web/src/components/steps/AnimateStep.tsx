import { Component, For, Show } from "solid-js";
import { clsx } from "clsx";
import { Button } from "../ui";
import * as Icons from "../icons";
import type { AnimationClip } from "../../lib/animation";

export interface AnimateStepProps {
  /** Hay esqueleto con pesos sobre la malla que se ve */
  ready: boolean;
  /** Qué falta para animar (se muestra en vez de las herramientas) */
  blocker?: string;
  /** Hay pesos calculados sobre la malla (se pueden pintar) */
  hasWeights?: boolean;
  onGoSkeleton?: () => void;
  /** Calcular los pesos desde acá (hay malla y esqueleto) */
  onComputeWeights?: () => void;
  computing?: boolean;
  /** Ir a pintar los pesos (sección Esqueleto, pincel) */
  onPaintWeights?: () => void;
  clips: AnimationClip[];
  activeClipId?: string;
  onSelectClip?: (id: string) => void;
  onNewClip?: () => void;
  onDuplicateClip?: () => void;
  onDeleteClip?: () => void;
  onRenameClip?: (name: string) => void;
  /** Abre el editor en la pestaña Biblioteca (animaciones básicas y poses) */
  onOpenLibrary?: () => void;
  selectedBoneName?: string;
  /** El editor de pose y rig (al costado del visor) está abierto */
  editorOpen?: boolean;
  onToggleEditor?: () => void;
  /** La cámara en vivo mueve el esqueleto */
  puppeteering?: boolean;
  onTogglePuppet?: () => void;
  /** Captura de movimiento desde un video grabado */
  onCaptureVideo?: () => void;
  onImportBvh?: () => void;
}

const SHORTCUTS: [string, string][] = [
  ["Clic", "Seleccionar articulación"],
  ["Shift+clic", "Sumar a la selección"],
  ["Doble clic", "Seleccionar la cadena"],
  ["R", "Girar (X/Y/Z eje; otra vez: global/local)"],
  ["G", "Desplazar"],
  ["Ctrl al girar", "Pasos de 5° (con Shift, 15°)"],
  ["Número al girar", "Grados exactos"],
  ["A / Alt+A", "Seleccionar todo / nada"],
  ["[ / ]", "Seleccionar padre / hijos"],
  ["Alt+R / Alt+G", "Giro / posición de reposo"],
  ["Ctrl+C / Ctrl+V", "Copiar / pegar pose (Shift: espejada)"],
  ["I", "Insertar key"],
  ["X / Supr", "Borrar keys seleccionadas"],
  ["Espacio", "Reproducir / pausar"],
  ["← / →", "Cuadro anterior / siguiente"],
];

export const AnimateStep: Component<AnimateStepProps> = (props) => {
  const active = () => props.clips.find((c) => c.id === props.activeClipId);

  return (
    <div class="space-y-5">
      <h3 class="text-sm font-semibold text-text">Animar</h3>
      <Show
        when={props.ready}
        fallback={
          <div class="space-y-2">
            <p class="text-xs text-text-muted leading-relaxed p-2 rounded-md bg-orange/10 border border-orange/30">
              {props.blocker ?? "Para animar hace falta el esqueleto con sus pesos: calcúlalos en el paso Esqueleto."}
            </p>
            <Show when={props.onComputeWeights}>
              <Button size="sm" fullWidth variant="primary" onClick={props.onComputeWeights} disabled={props.computing}>
                {props.computing ? "Calculando pesos…" : "Calcular pesos ahora"}
              </Button>
            </Show>
            <Button size="sm" fullWidth variant="ghost" onClick={props.onGoSkeleton}>
              Ir a Esqueleto
            </Button>
          </div>
        }
      >
        <p class="text-xs text-text-muted leading-relaxed">
          Elige una articulación, gírala con R (o desplázala con G) e inserta keys en la línea de tiempo. Con
          auto-key, cada cambio confirmado crea su key.
        </p>

        {/* Animaciones */}
        <div class="space-y-2">
          <div class="flex items-center justify-between">
            <span class="text-xs font-semibold text-text">Animaciones</span>
            <div class="flex gap-1">
              <Button size="sm" variant="ghost" onClick={props.onNewClip}>
                <span class="flex items-center gap-1"><Icons.Plus size={12} /> Nueva</span>
              </Button>
              <Button size="sm" variant="ghost" onClick={props.onDuplicateClip} disabled={!active()}>
                Duplicar
              </Button>
            </div>
          </div>
          <div class="rounded border border-border divide-y divide-border/60">
            <For each={props.clips} fallback={<p class="px-2 py-2 text-xs text-text-dim">Sin animaciones.</p>}>
              {(clip) => (
                <button
                  class={clsx(
                    "w-full flex items-center justify-between px-2 py-1.5 text-xs text-left",
                    clip.id === props.activeClipId ? "bg-accent/15 text-accent" : "text-text-muted hover:text-text"
                  )}
                  onClick={() => props.onSelectClip?.(clip.id)}
                >
                  <span class="truncate">{clip.name}</span>
                  <span class="font-mono text-text-dim">
                    {clip.end - clip.start + 1} c · {clip.tracks.length} art.
                  </span>
                </button>
              )}
            </For>
          </div>
          <Show when={active()}>
            {(clip) => (
              <div class="flex items-center gap-2">
                <input
                  class="flex-1 min-w-0 px-2 py-1 rounded bg-surface/40 border border-border text-xs text-text outline-none focus:border-accent"
                  value={clip().name}
                  onChange={(e) => props.onRenameClip?.(e.currentTarget.value.trim() || clip().name)}
                />
                <Button size="sm" variant="ghost" onClick={props.onDeleteClip}>
                  Borrar
                </Button>
              </div>
            )}
          </Show>
        </div>

        {/* Captura de movimiento */}
        <div class="space-y-1.5">
          <span class="text-xs font-semibold text-text">Captura de movimiento</span>
          <Button size="sm" fullWidth variant={props.puppeteering ? "default" : "primary"} onClick={props.onTogglePuppet}>
            <span class="flex items-center gap-1">
              <Icons.PersonArmsSpread size={12} /> {props.puppeteering ? "Apagar la cámara" : "Mover con la cámara en vivo"}
            </span>
          </Button>
          <p class="text-xs text-text-dim leading-relaxed">
            El esqueleto copia tus movimientos frente a la cámara; graba para guardarlos como animación.
          </p>
          <div class="grid grid-cols-2 gap-1">
            <Button size="sm" variant="ghost" onClick={props.onCaptureVideo} title="Detecta los movimientos de una persona en un video grabado">
              Desde un video…
            </Button>
            <Button size="sm" variant="ghost" onClick={props.onImportBvh} title="Animación de captura de movimiento en formato BVH">
              Importar BVH…
            </Button>
          </div>
        </div>

        <Button size="sm" fullWidth variant="ghost" onClick={props.onOpenLibrary} title="Animaciones básicas, poses de fábrica y poses guardadas">
          <span class="flex items-center gap-1"><Icons.Play size={12} /> Biblioteca de animaciones y poses</span>
        </Button>

        <div class="text-xs">
          <span class="text-text-muted">Articulación: </span>
          <span class="text-text">{props.selectedBoneName ?? "ninguna (clic en una esfera)"}</span>
        </div>

        <Show when={props.hasWeights}>
          <Button size="sm" fullWidth variant="ghost" onClick={props.onPaintWeights} title="Vuelve a Esqueleto con el pincel de pesos (B)">
            <span class="flex items-center gap-1"><Icons.PaintBrush size={12} /> Pintar pesos</span>
          </Button>
        </Show>

        <div class="space-y-1">
          <Button size="sm" fullWidth variant={props.editorOpen ? "default" : "primary"} onClick={props.onToggleEditor}>
            {props.editorOpen ? "Cerrar el editor de pose y rig" : "Abrir el editor de pose y rig"}
          </Button>
          <p class="text-xs text-text-dim leading-relaxed">
            Articulación y límites, herramientas de pose, IK y rig, al costado del visor (Ctrl+E). El borde se arrastra
            para darle más ancho.
          </p>
        </div>


        <div class="space-y-1">
          <For each={SHORTCUTS}>
            {([keys, description]) => (
              <div class="flex justify-between text-xs">
                <span class="font-mono text-text">{keys}</span>
                <span class="text-text-muted">{description}</span>
              </div>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
};
