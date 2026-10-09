import { Component, For, Show, createResource, createSignal } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { Button, Checkbox, NumberInput, Select } from "../ui";
import * as Icons from "../icons";

/** Medidas de un objeto (en mm, Z arriba) */
interface ObjectInfo {
  id: number;
  size: [number, number, number];
  triangles: number;
  volume: number;
}

export interface SelectedObjectsPanelProps {
  objects: { id: number; name: string }[];
  /** Cambia cuando cambia alguno de los objetos (para volver a medir) */
  revision: string;
  isProcessing?: boolean;
  onExport: (format: string, spacing: number | null) => void;
}

const FORMATS = [
  { value: "3mf", label: "3MF (un objeto por pieza)" },
  { value: "stl", label: "STL (todo junto)" },
  { value: "obj", label: "OBJ" },
  { value: "ply", label: "PLY" },
  { value: "glb", label: "GLB" },
];

const mm = (v: number) => v.toLocaleString("es", { maximumFractionDigits: 1 });

/**
 * Varios objetos elegidos en el Outliner (Ctrl o Mayús + clic): sus medidas y
 * exportarlos juntos, acomodados en la cama uno al lado del otro o donde están.
 */
export const SelectedObjectsPanel: Component<SelectedObjectsPanelProps> = (props) => {
  const [format, setFormat] = createSignal("3mf");
  const [arrange, setArrange] = createSignal(true);
  const [spacing, setSpacing] = createSignal(5);
  const [info] = createResource(
    () => `${props.objects.map((o) => o.id).join(",")}|${props.revision}`,
    () => invoke<ObjectInfo[]>("objects_info", { ids: props.objects.map((o) => o.id) }).catch(() => [] as ObjectInfo[]),
  );
  const infoOf = (id: number) => info()?.find((i) => i.id === id);
  // Largo de la fila acomodada: los anchos más las separaciones
  const row = () => {
    const list = info() ?? [];
    if (list.length === 0) return undefined;
    return {
      x: list.reduce((a, i) => a + i.size[0], 0) + spacing() * (list.length - 1),
      y: Math.max(...list.map((i) => i.size[1])),
      z: Math.max(...list.map((i) => i.size[2])),
    };
  };
  return (
    <div class="space-y-3" aria-label="Objetos elegidos">
      <h4 class="text-xs font-semibold uppercase tracking-wider text-text-muted">Objetos elegidos ({props.objects.length})</h4>
      <div class="space-y-1">
        <For each={props.objects}>
          {(o) => (
            <div class="flex items-center justify-between gap-2 text-xs" data-selected-object={o.id}>
              <span class="truncate text-text">{o.name}</span>
              <span class="font-mono text-[10px] text-text-dim">
                <Show when={infoOf(o.id)} fallback="…">
                  {(i) => `${mm(i().size[0])} × ${mm(i().size[1])} × ${mm(i().size[2])} mm · ${(i().volume / 1000).toLocaleString("es", { maximumFractionDigits: 2 })} cm³`}
                </Show>
              </span>
            </div>
          )}
        </For>
      </div>
      <Select options={FORMATS} value={format()} onChange={setFormat} />
      <Checkbox small label="Acomodar en la cama, en fila" checked={arrange()} onChange={setArrange} />
      <Show when={arrange()}>
        <div class="flex items-center gap-2 text-xs">
          <span class="text-text-muted">Separación</span>
          <NumberInput value={spacing()} min={0} step={1} suffix="mm" onChange={(v) => setSpacing(Math.max(0, v))} />
        </div>
        <Show when={row()}>
          {(r) => <p class="text-[11px] text-text-dim">La fila ocupa {mm(r().x)} × {mm(r().y)} mm y {mm(r().z)} mm de alto.</p>}
        </Show>
      </Show>
      <Button size="sm" fullWidth disabled={props.isProcessing} onClick={() => props.onExport(format(), arrange() ? spacing() : null)}>
        <Icons.Export size={12} /> Exportar juntos
      </Button>
      <p class="text-[11px] text-text-dim">Ctrl o Mayús + clic en el Outliner suma o quita objetos. Lo demás de Fabricar actúa sobre el activo.</p>
    </div>
  );
};
