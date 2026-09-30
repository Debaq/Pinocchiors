import { Component, For, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { Button, Checkbox } from "../ui";
import * as Icons from "../icons";
import type { SkeletonBone } from "../../lib/presetAnimations";
import type { RetargetMap } from "../../lib/retarget";

export interface RetargetRequest {
  /** De dónde viene el movimiento (nombre del archivo o del clip) */
  sourceName: string;
  source: SkeletonBone[];
  target: SkeletonBone[];
  /** Mapeo inicial: el automático o la plantilla guardada */
  map: RetargetMap;
  /** Cuadros del origen */
  frames: number;
  fps: number;
  /** Se usó una plantilla guardada para este origen */
  fromTemplate: boolean;
}

export interface RetargetDialogProps extends RetargetRequest {
  onConfirm: (result: { map: RetargetMap; rootMotion: boolean; name: string; saveTemplate: boolean }) => void;
  onCancel: () => void;
}

/**
 * Mapeo del retargeting: qué articulación del origen mueve cada una del
 * modelo (tabla editable), desplazamiento de la raíz y nombre del clip
 */
export const RetargetDialog: Component<RetargetDialogProps> = (props) => {
  const [map, setMap] = createSignal(new Map(props.map));
  const [rootMotion, setRootMotion] = createSignal(true);
  const [saveTemplate, setSaveTemplate] = createSignal(!props.fromTemplate);
  const [name, setName] = createSignal(props.sourceName.replace(/\.[^.]+$/, ""));
  // Solo las articulaciones que giran algo (las puntas siguen a su padre)
  const rows = createMemo(() =>
    props.target.flatMap((b, j) => (b.parent === null || props.target.some((c) => c.parent === j) ? [j] : []))
  );
  const mapped = () => rows().filter((j) => map().has(j)).length;

  const confirm = () => props.onConfirm({ map: map(), rootMotion: rootMotion(), name: name().trim() || "Animación importada", saveTemplate: saveTemplate() });

  onMount(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        props.onCancel();
      } else if (e.key !== "Tab") {
        e.stopPropagation();
      }
    };
    window.addEventListener("keydown", onKey, true);
    onCleanup(() => window.removeEventListener("keydown", onKey, true));
  });

  return (
    <div
      class="fixed inset-0 z-50 flex items-center justify-center bg-bg-darker/70 backdrop-blur-sm p-4"
      onPointerDown={(e) => e.target === e.currentTarget && props.onCancel()}
    >
      <div role="dialog" aria-label="Importar animación" class="w-full max-w-lg max-h-full flex flex-col rounded-lg border border-border bg-bg shadow-xl">
        <div class="flex items-center justify-between px-4 h-11 border-b border-border shrink-0">
          <h2 class="flex items-center gap-2 text-sm font-semibold text-text">
            <Icons.Person size={16} class="text-accent" />
            Importar animación
          </h2>
          <button class="w-7 h-7 flex items-center justify-center rounded text-text-muted hover:text-text hover:bg-surface/40" aria-label="Cancelar" onClick={props.onCancel}>
            <Icons.X size={14} />
          </button>
        </div>
        <div class="p-4 space-y-3 overflow-y-auto min-h-0">
          <p class="text-xs text-text-muted leading-relaxed">
            {props.sourceName}: {props.source.length} articulaciones, {props.frames} cuadros a {props.fps} fps. Cada articulación del modelo apunta
            como la del origen que tenga asignada; las proporciones y la pose de reposo pueden ser distintas.
          </p>
          <label class="block space-y-1">
            <span class="text-xs text-text-muted">Nombre del clip</span>
            <input
              class="w-full px-2 py-1 rounded bg-surface/40 border border-border text-xs text-text outline-none focus:border-accent"
              value={name()}
              onInput={(e) => setName(e.currentTarget.value)}
            />
          </label>
          <div class="flex items-center justify-between text-xs">
            <span class="text-text-muted">
              Mapeo: {mapped()} de {rows().length} articulaciones {props.fromTemplate ? "(plantilla guardada)" : "(automático)"}
            </span>
            <button class="text-accent hover:underline" onClick={() => setMap(new Map())}>
              Vaciar
            </button>
          </div>
          <div class="rounded border border-border divide-y divide-border/60 max-h-72 overflow-y-auto">
            <For each={rows()}>
              {(j) => (
                <div class="flex items-center gap-2 px-2 py-1 text-xs">
                  <span class="w-1/2 truncate font-mono text-text">{props.target[j].name}</span>
                  <select
                    class="w-1/2 h-6 px-1 rounded bg-surface/40 border border-border text-xs text-text"
                    value={map().get(j) ?? ""}
                    onChange={(e) => {
                      const next = new Map(map());
                      const v = e.currentTarget.value;
                      if (v === "") next.delete(j);
                      else next.set(j, Number(v));
                      setMap(next);
                    }}
                  >
                    <option value="">(sin mover)</option>
                    <For each={props.source}>{(b, k) => <option value={k()}>{b.name}</option>}</For>
                  </select>
                </div>
              )}
            </For>
          </div>
          <Checkbox label="Desplazar la raíz (si no, la animación queda en el lugar)" checked={rootMotion()} onChange={setRootMotion} />
          <Checkbox label="Guardar este mapeo para los archivos con el mismo esqueleto" checked={saveTemplate()} onChange={setSaveTemplate} />
        </div>
        <div class="flex justify-end gap-2 px-4 py-3 border-t border-border shrink-0">
          <Button size="sm" variant="ghost" onClick={props.onCancel}>
            Cancelar
          </Button>
          <Button size="sm" variant="primary" onClick={confirm} disabled={mapped() === 0}>
            Crear clip
          </Button>
        </div>
      </div>
    </div>
  );
};
