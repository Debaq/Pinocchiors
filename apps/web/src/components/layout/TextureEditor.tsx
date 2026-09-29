import { Component, For, Show, createEffect, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { clsx } from "clsx";
import { Button, IconButton, Select } from "../ui";
import { ContextMenu, type MenuEntry } from "../ui/ContextMenu";
import * as Icons from "../icons";
import type { MeshData, SceneMaterial } from "../../lib/Viewer3D";
import { UvView, uvEdges, type ImageChannel, type UvEdgeMode } from "../../lib/UvView";
import { createPersisted } from "../../lib/ui-state";

/** Mapas de un material PBR (claves de `SceneMaterial.maps`) */
export type TextureSlot = keyof SceneMaterial["maps"];

export const TEXTURE_SLOTS: { slot: TextureSlot; label: string }[] = [
  { slot: "base", label: "Color" },
  { slot: "metallicRoughness", label: "Metal / rugosidad" },
  { slot: "normal", label: "Normal" },
  { slot: "occlusion", label: "Oclusión" },
  { slot: "emissive", label: "Emisión" },
];

/** Textura abierta en el editor: un mapa de un material */
export interface TextureSelection {
  material: number;
  slot: TextureSlot;
}

const CHANNELS: { value: ImageChannel; label: string; title: string }[] = [
  { value: "rgba", label: "RGB", title: "Color con transparencia" },
  { value: "r", label: "R", title: "Canal rojo" },
  { value: "g", label: "G", title: "Canal verde (rugosidad en metal / rugosidad)" },
  { value: "b", label: "B", title: "Canal azul (metal en metal / rugosidad)" },
  { value: "a", label: "A", title: "Transparencia" },
];

const WIRE_MODES = [
  { value: "all", label: "Malla", title: "Todas las aristas de la malla" },
  { value: "borders", label: "Bordes", title: "Solo el contorno de cada isla" },
  { value: "none", label: "Nada", title: "Solo la imagen" },
];

/** Botones de opción pegados (uno activo) */
const Segmented: Component<{
  options: { value: string; label: string; title: string }[];
  value: string;
  onChange: (value: string) => void;
  mono?: boolean;
}> = (props) => (
  <div class="flex rounded-md border border-border overflow-hidden">
    <For each={props.options}>
      {(o) => (
        <button
          title={o.title}
          class={clsx(
            "px-2 h-7 text-xs transition-colors",
            props.mono && "font-mono",
            props.value === o.value ? "bg-accent/20 text-accent" : "text-text-muted hover:bg-surface/50 hover:text-text"
          )}
          onClick={() => props.onChange(o.value)}
        >
          {o.label}
        </button>
      )}
    </For>
  </div>
);

export interface TextureEditorProps {
  /** Materiales de la malla que se muestra, con sus imágenes */
  materials: SceneMaterial[];
  /** Malla con UV (grupos por material; con quads, sus índices) */
  mesh?: MeshData;
  selection: TextureSelection;
  onSelect: (selection: TextureSelection) => void;
  /** Malla que se muestra: original o retopología (`undefined` = no se puede elegir) */
  target?: "original" | "quad";
  onTargetChange?: (target: "original" | "quad") => void;
  onClose: () => void;
  /** Exportar la textura: con capas (xcf/psd), la imagen sola o la malla sola */
  onExport?: (kind: TextureExportKind) => void;
  /** Reemplazar la textura por un archivo (con capas o imagen) */
  onImport?: () => void;
  /** Abrir en GIMP y recargar al guardar */
  onEditExternally?: () => void;
  /** Se está editando afuera: hora de la última recarga (o `null` si todavía no) */
  external?: { lastUpdate: string | null };
  onStopExternal?: () => void;
}

export type TextureExportKind = "xcf" | "psd" | "png" | "layout";

/** Caras de un material: índices de quad (con `quadIndices`) o de triángulo */
function materialFaces(mesh: MeshData, material: number, perFace: 3 | 4): number[] {
  const indicesPerFace = perFace === 4 ? 6 : 3;
  const total = mesh.indices.length / indicesPerFace;
  const groups = mesh.groups;
  if (!groups || groups.length < 3) return Array.from({ length: total }, (_, i) => i);
  const faces: number[] = [];
  for (let g = 0; g + 2 < groups.length; g += 3) {
    if (groups[g + 2] !== material) continue;
    const start = groups[g] / indicesPerFace;
    const end = start + groups[g + 1] / indicesPerFace;
    for (let f = start; f < end; f++) faces.push(f);
  }
  return faces;
}

export const TextureEditor: Component<TextureEditorProps> = (props) => {
  let canvas: HTMLCanvasElement | undefined;
  let view: UvView | undefined;
  const [channel, setChannel] = createSignal<ImageChannel>("rgba");
  const [wireMode, setWireMode] = createPersisted<UvEdgeMode | "none">("textureEditor.wire", "all");
  const [wireOpacity, setWireOpacity] = createPersisted("textureEditor.wireOpacity", 0.8);
  const [cursor, setCursor] = createSignal<[number, number] | null>(null);
  const [exportMenu, setExportMenu] = createSignal<{ x: number; y: number } | null>(null);
  const exportItems = (): MenuEntry[] => [
    { header: "Con capas (textura + malla UV)" },
    { label: "GIMP (.xcf)", onSelect: () => props.onExport?.("xcf") },
    { label: "Photoshop, Krita, Photopea (.psd)", onSelect: () => props.onExport?.("psd") },
    { separator: true },
    { label: "Solo la imagen (.png)", onSelect: () => props.onExport?.("png"), disabled: !image() },
    { label: "Solo la malla UV (.png transparente)", onSelect: () => props.onExport?.("layout") },
  ];

  const material = () => props.materials[props.selection.material];
  const image = () => material()?.maps[props.selection.slot];

  const materialOptions = createMemo(() =>
    props.materials.map((m, i) => ({ value: String(i), label: m.name || `Material ${i + 1}` }))
  );
  /** Mapas que tiene el material elegido */
  const slotOptions = createMemo(() => {
    const maps = material()?.maps ?? {};
    return TEXTURE_SLOTS.filter((s) => maps[s.slot]).map((s) => ({ value: s.slot, label: s.label }));
  });

  // Aristas UV de las caras del material elegido
  const edges = createMemo(() => {
    const mesh = props.mesh;
    if (!mesh?.uvs) return undefined;
    const mode = wireMode();
    if (mode === "none") return undefined;
    const quads = mesh.quadIndices && mesh.quadIndices.length * 6 === mesh.indices.length * 4;
    const faces = materialFaces(mesh, props.selection.material, quads ? 4 : 3);
    return quads
      ? uvEdges(mesh.uvs, mesh.quadIndices!, 4, faces, mode)
      : uvEdges(mesh.uvs, mesh.indices, 3, faces, mode);
  });

  onMount(() => {
    view = new UvView(canvas!);
    view.onCursor = setCursor;
  });
  onCleanup(() => view?.dispose());

  createEffect(() => view?.setImage(image()));
  createEffect(() => view?.setLayout(props.mesh?.uvs, edges()));
  createEffect(() => view?.setChannel(channel()));
  createEffect(() => view?.setWireOpacity(wireOpacity()));

  // Otra malla con menos materiales: el primero
  createEffect(() => {
    if (props.materials.length > 0 && !material()) props.onSelect({ material: 0, slot: props.selection.slot });
  });

  // Material sin el mapa elegido: el primero que tenga
  createEffect(() => {
    const options = slotOptions();
    if (options.length > 0 && !options.some((o) => o.value === props.selection.slot)) {
      props.onSelect({ material: props.selection.material, slot: options[0].value });
    }
  });

  const readout = () => {
    const uv = cursor();
    if (!uv) return "";
    const text = `U ${uv[0].toFixed(3)}  V ${uv[1].toFixed(3)}`;
    const img = image();
    if (!img) return text;
    return `${text}  ·  px ${Math.floor(uv[0] * img.width)}, ${Math.floor(uv[1] * img.height)}`;
  };

  return (
    <section class="relative flex flex-col w-full h-full min-w-0 bg-bg">
      {/* Barra: qué textura y cómo verla */}
      <div class="flex flex-wrap items-center gap-2 px-2 py-1.5 border-b border-border bg-bg-darker">
        <Show when={props.target && props.onTargetChange}>
          <div class="w-32">
            <Select
              options={[
                { value: "original", label: "Original" },
                { value: "quad", label: "Retopología" },
              ]}
              value={props.target}
              onChange={(v) => props.onTargetChange?.(v as "original" | "quad")}
            />
          </div>
        </Show>
        <Show when={props.materials.length > 1}>
          <div class="w-40">
            <Select
              options={materialOptions()}
              value={String(props.selection.material)}
              onChange={(v) => props.onSelect({ material: Number(v), slot: props.selection.slot })}
            />
          </div>
        </Show>
        <Show when={slotOptions().length > 0}>
          <div class="w-40">
            <Select
              options={slotOptions()}
              value={props.selection.slot}
              onChange={(v) => props.onSelect({ material: props.selection.material, slot: v as TextureSlot })}
            />
          </div>
        </Show>
        <Segmented options={CHANNELS} value={channel()} onChange={(v) => setChannel(v as ImageChannel)} mono />
        <Segmented
          options={WIRE_MODES}
          value={wireMode()}
          onChange={(v) => setWireMode(v as UvEdgeMode | "none")}
        />
        <Show when={wireMode() !== "none"}>
          <input
            type="range"
            min="0.1"
            max="1"
            step="0.05"
            value={wireOpacity()}
            onInput={(e) => setWireOpacity(Number(e.currentTarget.value))}
            title="Opacidad de la malla"
            aria-label="Opacidad de la malla"
            class="w-20 accent-accent"
          />
        </Show>
        <div class="flex-1" />
        <Show when={props.onEditExternally}>
          <Button
            size="sm"
            variant="primary"
            title="Abre la textura con la malla en capas; al guardar en GIMP se recarga sola"
            onClick={() => props.onEditExternally?.()}
          >
            Editar en GIMP
          </Button>
        </Show>
        <Show when={props.onExport}>
          <Button
            size="sm"
            onClick={(e: MouseEvent) => {
              const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
              setExportMenu({ x: rect.left, y: rect.bottom + 4 });
            }}
          >
            Exportar
          </Button>
        </Show>
        <Show when={props.onImport}>
          <Button size="sm" title="Reemplaza la textura por un archivo XCF, PSD o imagen" onClick={() => props.onImport?.()}>
            Importar
          </Button>
        </Show>
        <IconButton variant="ghost" size="sm" title="Ver todo (doble clic)" aria-label="Ver todo" onClick={() => view?.fit()}>
          <Icons.FrameCorners size={14} />
        </IconButton>
        <IconButton variant="ghost" size="sm" title="Cerrar el editor de texturas" aria-label="Cerrar" onClick={() => props.onClose()}>
          <Icons.X size={14} />
        </IconButton>
      </div>

      <div class="relative flex-1 min-h-0">
        <canvas ref={canvas} class="w-full h-full block" />
        <div class="absolute inset-0 pointer-events-none">
          <Show when={!props.mesh?.uvs}>
            <div class="absolute top-3 left-1/2 -translate-x-1/2 px-3 py-1.5 rounded-md bg-bg-darker/90 text-warning text-xs">
              La malla no tiene UV
            </div>
          </Show>
          <Show when={props.mesh?.uvs && !image()}>
            <div class="absolute top-3 left-1/2 -translate-x-1/2 px-3 py-1.5 rounded-md bg-bg-darker/90 text-text-muted text-xs">
              {slotOptions().length > 0 ? "Sin imagen en este mapa" : "Este material no tiene texturas"}
            </div>
          </Show>
          <Show when={props.external}>
            {(ext) => (
              <div class="absolute bottom-3 right-3 flex items-center gap-2 px-2.5 py-1.5 rounded-md bg-bg-darker/90 border border-accent/60 text-xs text-text pointer-events-auto">
                <span class="w-2 h-2 rounded-full bg-success animate-pulse" />
                <span>
                  Editando afuera: se recarga al guardar
                  {ext().lastUpdate ? ` · última ${ext().lastUpdate}` : ""}
                </span>
                <IconButton variant="ghost" size="sm" aria-label="Dejar de seguir" title="Dejar de seguir el archivo" onClick={() => props.onStopExternal?.()}>
                  <Icons.X size={12} />
                </IconButton>
              </div>
            )}
          </Show>
          <div class="absolute bottom-3 left-3 flex gap-3 px-2.5 py-1.5 rounded-md bg-bg-darker/90 border border-border/50 text-xs text-text-muted font-mono">
            <Show when={image()} fallback={<span>sin imagen</span>}>
              <span class="text-text">
                {image()!.width} × {image()!.height}
              </span>
            </Show>
            <Show when={readout()}>
              <span>{readout()}</span>
            </Show>
          </div>
        </div>
      </div>
      <Show when={exportMenu()}>
        {(m) => <ContextMenu x={m().x} y={m().y} items={exportItems()} onClose={() => setExportMenu(null)} />}
      </Show>
    </section>
  );
};
